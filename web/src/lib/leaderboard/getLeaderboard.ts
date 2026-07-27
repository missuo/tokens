import { unstable_cache } from "next/cache";
import { db, users, submissions, dailyBreakdown } from "@/lib/db";
import {
  USERNAME_LOOKUP_LIMIT,
  getSingleUsernameMatch,
  normalizeUsernameCacheKey,
  usernameEqualsIgnoreCase,
} from "@/lib/db/usernameLookup";
import { eq, desc, sql, and, or, gte, lte, isNull } from "drizzle-orm";
import type { LeaderboardData, LeaderboardUser, Period, SortBy } from "@/lib/leaderboard/types";
import {
  escapeLikePattern,
  hasDirectives,
  parseSearchDirectives,
} from "@/lib/leaderboard/searchDirectives";
import { SOCIAL_VERIFIED_THRESHOLD } from "@/lib/socialVerification";

export type { LeaderboardData, LeaderboardUser, Period, SortBy } from "@/lib/leaderboard/types";

// A user with >= SOCIAL_VERIFIED_THRESHOLD linked socials is "verified". The
// snapshot on users.social_links is refreshed by lib/githubSocials.ts.
function verifiedExpr() {
  return sql<boolean>`COALESCE(jsonb_array_length(${users.socialLinks}) >= ${SOCIAL_VERIFIED_THRESHOLD}, false)`;
}

interface LeaderboardPeriodRow {
  userId: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  verified: boolean;
  tokens: number;
  cost: number;
  sourceBreakdown: Record<string, { models: Record<string, unknown> }> | null;
}

interface PeriodDateRange {
  start: string;
  end: string;
}

interface PeriodLeaderboardDbRow {
  userId: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  verified: boolean | null;
  tokens: number | string | null;
  cost: number | string | null;
  /** Absent when the query skipped the column — see fetchPeriodLeaderboardRows. */
  sourceBreakdown?: Record<string, { models: Record<string, unknown> }> | null;
}

interface AllTimeLeaderboardDbRow {
  userId: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  verified: boolean | null;
  totalTokens: number | string | null;
  totalCost: number | string | null;
}

interface RankedLeaderboardDbRow extends AllTimeLeaderboardDbRow {
  rank: number | string | null;
}

function toUtcDateString(date: Date): string {
  return date.toISOString().slice(0, 10);
}

function getPeriodDateRange(
  period: Period,
  now: Date = new Date(),
  customFrom?: string,
  customTo?: string
): PeriodDateRange | null {
  if (period === "all") {
    return null;
  }

  if (period === "custom") {
    if (!customFrom || !customTo) {
      return null;
    }
    return { start: customFrom, end: customTo };
  }

  const end = new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
  );

  if (period === "today") {
    // Daily rows are bucketed by the submitter's local date, so "today" is just
    // a calendar-date match. The viewer's client passes its own local date via
    // customFrom; we fall back to the server's UTC date when it's absent (SSR /
    // direct API hits) and let the client correct it after hydration.
    const todayDate = customFrom || toUtcDateString(end);
    return {
      start: todayDate,
      end: todayDate,
    };
  }

  if (period === "week") {
    const start = new Date(end);
    start.setUTCDate(start.getUTCDate() - 6);
    return {
      start: toUtcDateString(start),
      end: toUtcDateString(end),
    };
  }

  if (period === "last-month") {
    const lastMonthEnd = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 0));
    const lastMonthStart = new Date(Date.UTC(lastMonthEnd.getUTCFullYear(), lastMonthEnd.getUTCMonth(), 1));
    return {
      start: toUtcDateString(lastMonthStart),
      end: toUtcDateString(lastMonthEnd),
    };
  }

  const start = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1));
  return {
    start: toUtcDateString(start),
    end: toUtcDateString(end),
  };
}

function compareLeaderboardUsers(
  left: Omit<LeaderboardUser, "rank">,
  right: Omit<LeaderboardUser, "rank">,
  sortBy: SortBy
): number {
  const primary = sortBy === "cost"
    ? right.totalCost - left.totalCost
    : right.totalTokens - left.totalTokens;

  if (primary !== 0) {
    return primary;
  }

  const secondary = sortBy === "cost"
    ? right.totalTokens - left.totalTokens
    : right.totalCost - left.totalCost;

  if (secondary !== 0) {
    return secondary;
  }

  return left.username.localeCompare(right.username);
}

function aggregatePeriodRows(
  rows: LeaderboardPeriodRow[],
  sortBy: SortBy
): Array<Omit<LeaderboardUser, "rank">> {
  const usersById = new Map<string, Omit<LeaderboardUser, "rank">>();

  for (const row of rows) {
    const existing = usersById.get(row.userId);

    if (existing) {
      existing.totalTokens += row.tokens;
      existing.totalCost += row.cost;
      continue;
    }

    usersById.set(row.userId, {
      userId: row.userId,
      username: row.username,
      displayName: row.displayName,
      avatarUrl: row.avatarUrl,
      verified: row.verified,
      totalTokens: row.tokens,
      totalCost: row.cost,
    });
  }

  return Array.from(usersById.values()).sort((left, right) =>
    compareLeaderboardUsers(left, right, sortBy)
  );
}

function matchesLeaderboardSearch(
  user: Pick<LeaderboardUser, "username" | "displayName">,
  textSearch: string
): boolean {
  if (!textSearch) {
    return true;
  }

  const lowerSearch = textSearch.toLowerCase();
  if (user.username.toLowerCase().includes(lowerSearch)) {
    return true;
  }
  if (user.displayName && user.displayName.toLowerCase().includes(lowerSearch)) {
    return true;
  }
  return false;
}

function buildPeriodLeaderboardData(
  rows: LeaderboardPeriodRow[],
  page: number,
  limit: number,
  period: Period,
  sortBy: SortBy = "tokens",
  search: string = ""
): LeaderboardData {
  const parsed = parseSearchDirectives(search);

  let filteredRows = rows;
  if (hasDirectives(parsed)) {
    filteredRows = rows.filter((row) => {
      if (!row.sourceBreakdown) return false;

      const clientKeys = Object.keys(row.sourceBreakdown).map((k) => k.toLowerCase());
      const modelKeys = Object.values(row.sourceBreakdown).flatMap((client) =>
        client.models ? Object.keys(client.models).map((m) => m.toLowerCase()) : []
      );

      if (parsed.clients.length > 0) {
        const hasMatchingClient = parsed.clients.some((c) =>
          clientKeys.some((k) => k.includes(c))
        );
        if (!hasMatchingClient) return false;
      }

      if (parsed.models.length > 0) {
        const hasMatchingModel = parsed.models.some((m) =>
          modelKeys.some((k) => k.includes(m))
        );
        if (!hasMatchingModel) return false;
      }

      return true;
    });
  }

  return pageRanking(
    aggregatePeriodRows(filteredRows, sortBy),
    page,
    limit,
    period,
    sortBy,
    parsed.text
  );
}

/**
 * Rank, text-filter and slice an already-aggregated ranking.
 *
 * Pure, so it can run per request against a shared cached ranking rather than
 * being baked into a per-page cache entry of its own. Ranks are assigned before
 * the text filter so a search shows each user's real position on the board.
 */
function pageRanking(
  aggregatedUsers: Array<Omit<LeaderboardUser, "rank">>,
  page: number,
  limit: number,
  period: Period,
  sortBy: SortBy,
  searchText: string
): LeaderboardData {
  const offset = (page - 1) * limit;
  const rankedUsers = aggregatedUsers.map((user, index) => ({
    ...user,
    rank: index + 1,
  }));
  const textFilteredUsers = rankedUsers.filter((user) =>
    matchesLeaderboardSearch(user, searchText)
  );
  const pagedUsers = textFilteredUsers.slice(offset, offset + limit);

  return {
    users: pagedUsers,
    pagination: {
      page,
      limit,
      totalUsers: textFilteredUsers.length,
      totalPages: Math.ceil(textFilteredUsers.length / limit),
      hasNext: offset + limit < textFilteredUsers.length,
      hasPrev: page > 1,
    },
    stats: {
      totalTokens: aggregatedUsers.reduce((sum, user) => sum + user.totalTokens, 0),
      totalCost: aggregatedUsers.reduce((sum, user) => sum + user.totalCost, 0),
      uniqueUsers: aggregatedUsers.length,
    },
    period,
    sortBy,
  };
}

/**
 * Locate one user in an already-ranked list, carrying their position with them.
 *
 * Pure and shared, so the number beside "Your position" is by construction the
 * same number the table shows for that row.
 */
function findInRanking(
  aggregatedUsers: Array<Omit<LeaderboardUser, "rank">>,
  username: string
): LeaderboardUser | null {
  const usernameCacheKey = normalizeUsernameCacheKey(username);
  const matchingUsers = aggregatedUsers.filter(
    (user) => normalizeUsernameCacheKey(user.username) === usernameCacheKey
  );
  const user = getSingleUsernameMatch(matchingUsers, username);

  if (!user) {
    return null;
  }

  return {
    ...user,
    rank: aggregatedUsers.indexOf(user) + 1,
  };
}

/**
 * Every daily row in the period, folded per user by the caller.
 *
 * `withBreakdown` decides whether source_breakdown comes along. It is the
 * heaviest column by an order of magnitude — a month of it is 4.2MB against
 * 296kB for everything else — and the only thing that reads it is a
 * `client:`/`model:` search directive. Fetching it for the ordinary case meant
 * hauling four megabytes of JSON across the wire and parsing it in the Worker
 * to answer a query that never looked at it.
 */
async function fetchPeriodLeaderboardRows(
  period: Exclude<Period, "all">,
  customFrom?: string,
  customTo?: string,
  withBreakdown = true
): Promise<LeaderboardPeriodRow[]> {
  const dateRange = getPeriodDateRange(period, new Date(), customFrom, customTo);

  if (!dateRange) {
    return [];
  }

  const rows: PeriodLeaderboardDbRow[] = await db
    .select({
      userId: users.id,
      username: users.username,
      displayName: users.displayName,
      avatarUrl: users.avatarUrl,
      verified: verifiedExpr().as("verified"),
      tokens: dailyBreakdown.tokens,
      cost: dailyBreakdown.cost,
      ...(withBreakdown
        ? { sourceBreakdown: dailyBreakdown.sourceBreakdown }
        : {}),
    })
    .from(dailyBreakdown)
    .innerJoin(submissions, eq(dailyBreakdown.submissionId, submissions.id))
    .innerJoin(users, eq(submissions.userId, users.id))
    .where(
      and(
        gte(dailyBreakdown.date, dateRange.start),
        lte(dailyBreakdown.date, dateRange.end),
        isNull(users.bannedAt)
      )
    );

  return rows.map((row: PeriodLeaderboardDbRow) => ({
    userId: row.userId,
    username: row.username,
    displayName: row.displayName,
    avatarUrl: row.avatarUrl,
    verified: Boolean(row.verified),
    tokens: Number(row.tokens) || 0,
    cost: Number(row.cost) || 0,
    sourceBreakdown: row.sourceBreakdown ?? null,
  }));
}

async function fetchLeaderboardData(
  period: Period,
  page: number,
  limit: number,
  sortBy: SortBy = "tokens",
  search: string = "",
  customFrom?: string,
  customTo?: string
): Promise<LeaderboardData> {
  if (period !== "all") {
    // Only a client:/model: directive reads the breakdown; a plain listing or
    // a username search never touches it.
    const rows = await fetchPeriodLeaderboardRows(
      period,
      customFrom,
      customTo,
      hasDirectives(parseSearchDirectives(search))
    );
    return buildPeriodLeaderboardData(rows, page, limit, period, sortBy, search);
  }

  const offset = (page - 1) * limit;
  const parsed = parseSearchDirectives(search);

  const orderByColumn = sortBy === "cost"
    ? sql`SUM(CAST(${submissions.totalCost} AS DECIMAL(18,4)))`
    : sql`SUM(${submissions.totalTokens})`;
  const secondaryOrderByColumn = sortBy === "cost"
    ? sql`SUM(${submissions.totalTokens})`
    : sql`SUM(CAST(${submissions.totalCost} AS DECIMAL(18,4)))`;

  const clientConditions = parsed.clients.map((client) =>
    sql`EXISTS (SELECT 1 FROM unnest(${submissions.sourcesUsed}) AS s WHERE LOWER(s) LIKE ${`%${escapeLikePattern(client)}%`})`
  );
  const modelConditions = parsed.models.map((model) =>
    sql`EXISTS (SELECT 1 FROM unnest(${submissions.modelsUsed}) AS m WHERE LOWER(m) LIKE ${`%${escapeLikePattern(model)}%`})`
  );
  const directiveConditions = [
    clientConditions.length > 0 ? or(...clientConditions) : undefined,
    modelConditions.length > 0 ? or(...modelConditions) : undefined,
  ].filter((condition): condition is ReturnType<typeof sql> => condition !== undefined);

  const hasTextSearch = parsed.text.length > 0;
  const hasDirectiveFilters = directiveConditions.length > 0;

  if (hasTextSearch || hasDirectiveFilters) {
    const rankedSubquery = db
      .select({
        rank: sql<number>`RANK() OVER (ORDER BY ${orderByColumn} DESC)`.as("rank"),
        userId: users.id,
        username: users.username,
        displayName: users.displayName,
        avatarUrl: users.avatarUrl,
        verified: verifiedExpr().as("verified"),
        totalTokens: sql<number>`SUM(${submissions.totalTokens})`.as("total_tokens"),
        totalCost: sql<number>`SUM(CAST(${submissions.totalCost} AS DECIMAL(18,4)))`.as("total_cost"),
      })
      .from(submissions)
      .innerJoin(users, eq(submissions.userId, users.id))
      .where(and(isNull(users.bannedAt), ...directiveConditions))
      .groupBy(users.id, users.username, users.displayName, users.avatarUrl)
      .as("ranked");
    const rankedSecondaryOrderByColumn = sortBy === "cost"
      ? rankedSubquery.totalTokens
      : rankedSubquery.totalCost;

    let textFilter: ReturnType<typeof sql> | undefined;
    if (hasTextSearch) {
      const escapedSearch = escapeLikePattern(parsed.text.toLowerCase());
      const searchPattern = `%${escapedSearch}%`;
      textFilter = sql`(LOWER(${rankedSubquery.username}) LIKE ${searchPattern} OR LOWER(COALESCE(${rankedSubquery.displayName}, '')) LIKE ${searchPattern})`;
    }

    const results = await db
      .select()
      .from(rankedSubquery)
      .where(textFilter)
      .orderBy(
        sql`${rankedSubquery.rank} ASC`,
        sql`${rankedSecondaryOrderByColumn} DESC`,
        sql`LOWER(${rankedSubquery.username}) ASC`
      )
      .limit(limit)
      .offset(offset);

    const countResult = await db
      .select({ count: sql<number>`COUNT(*)`.as("count") })
      .from(rankedSubquery)
      .where(textFilter);

    const totalUsers = Number(countResult[0]?.count) || 0;
    const totalPages = Math.ceil(totalUsers / limit);

    const globalStats = await db
      .select({
        totalTokens: sql<number>`SUM(${submissions.totalTokens})`,
        totalCost: sql<number>`SUM(CAST(${submissions.totalCost} AS DECIMAL(18,4)))`,
        uniqueUsers: sql<number>`COUNT(DISTINCT ${submissions.userId})`,
      })
      .from(submissions)
      .innerJoin(users, eq(submissions.userId, users.id))
      .where(isNull(users.bannedAt));

    return {
      users: (results as RankedLeaderboardDbRow[]).map((row) => ({
        rank: Number(row.rank),
        userId: row.userId,
        username: row.username,
        displayName: row.displayName,
        avatarUrl: row.avatarUrl,
        verified: Boolean(row.verified),
        totalTokens: Number(row.totalTokens) || 0,
        totalCost: Number(row.totalCost) || 0,
      })),
      pagination: {
        page,
        limit,
        totalUsers,
        totalPages,
        hasNext: page < totalPages,
        hasPrev: page > 1,
      },
      stats: {
        totalTokens: Number(globalStats[0]?.totalTokens) || 0,
        totalCost: Number(globalStats[0]?.totalCost) || 0,
        uniqueUsers: Number(globalStats[0]?.uniqueUsers) || 0,
      },
      period,
      sortBy,
    };
  }

  // Non-search path: competition rank with deterministic row ordering for ties.
  const leaderboardQuery = db
    .select({
      rank: sql<number>`RANK() OVER (ORDER BY ${orderByColumn} DESC)`.as("rank"),
      userId: users.id,
      username: users.username,
      displayName: users.displayName,
      avatarUrl: users.avatarUrl,
      verified: verifiedExpr().as("verified"),
      totalTokens: sql<number>`SUM(${submissions.totalTokens})`.as("total_tokens"),
      totalCost: sql<number>`SUM(CAST(${submissions.totalCost} AS DECIMAL(18,4)))`.as("total_cost"),
    })
    .from(submissions)
    .innerJoin(users, eq(submissions.userId, users.id))
    .where(isNull(users.bannedAt))
    .groupBy(users.id, users.username, users.displayName, users.avatarUrl)
    .orderBy(
      desc(orderByColumn),
      desc(secondaryOrderByColumn),
      sql`LOWER(${users.username}) ASC`
    )
    .limit(limit)
    .offset(offset);

  const [results, globalStats] = await Promise.all([
    leaderboardQuery,
    db
      .select({
        totalTokens: sql<number>`SUM(${submissions.totalTokens})`,
        totalCost: sql<number>`SUM(CAST(${submissions.totalCost} AS DECIMAL(18,4)))`,
        uniqueUsers: sql<number>`COUNT(DISTINCT ${submissions.userId})`,
      })
      .from(submissions)
      .innerJoin(users, eq(submissions.userId, users.id))
      .where(isNull(users.bannedAt)),
  ]);

  const totalUsers = Number(globalStats[0]?.uniqueUsers) || 0;
  const totalPages = Math.ceil(totalUsers / limit);

  return {
    users: (results as RankedLeaderboardDbRow[]).map((row) => ({
      rank: Number(row.rank),
      userId: row.userId,
      username: row.username,
      displayName: row.displayName,
      avatarUrl: row.avatarUrl,
      verified: Boolean(row.verified),
      totalTokens: Number(row.totalTokens) || 0,
      totalCost: Number(row.totalCost) || 0,
    })),
    pagination: {
      page,
      limit,
      totalUsers,
      totalPages,
      hasNext: page < totalPages,
      hasPrev: page > 1,
    },
    stats: {
      totalTokens: Number(globalStats[0]?.totalTokens) || 0,
      totalCost: Number(globalStats[0]?.totalCost) || 0,
      uniqueUsers: Number(globalStats[0]?.uniqueUsers) || 0,
    },
    period,
    sortBy,
  };
}

// `page` and `search` reach here straight from a public URL and go into the
// cache key below, so an uncapped value buys an unbounded number of cache
// entries for the cost of one request; `page` additionally becomes the OFFSET.
// Clamped here rather than at each caller because both the API route and the
// /leaderboard page pass their query string through unmodified.
// A username is at most 39 chars and the `client:`/`model:` directives add a
// short prefix, so 120 is well past any real query; 500 pages of 100 is well
// past the end of the board.
const MAX_SEARCH_LENGTH = 120;
const MAX_PAGE = 500;

function periodCacheKey(
  period: Exclude<Period, "all">,
  customFrom?: string,
  customTo?: string
): string {
  return period === "custom"
    ? `custom:${customFrom}:${customTo}`
    : period === "today"
    ? `today:${customFrom ?? ""}`
    : period;
}

/**
 * The period's whole ranking, aggregated once and shared by every reader.
 *
 * The viewer's own row used to be a second query under its own cache key,
 * rendered beside a table built from a different entry. Nothing made the two
 * agree: after an invalidation each is refilled whenever it happens to be
 * requested next, and `today` climbs all day as submissions land — so any gap
 * between those moments showed the same person two different totals on one
 * screen. Reading both from a single entry makes disagreement impossible
 * rather than merely unlikely, and costs one query instead of two.
 *
 * Deliberately independent of page, viewer and search: paging and text
 * filtering are pure functions applied per request. `withBreakdown: false`
 * because aggregation never reads that column — a `client:`/`model:` directive
 * does, and that path keeps its own fetch since it aggregates a filtered
 * subset and cannot share this result.
 */
function getPeriodRanking(
  period: Exclude<Period, "all">,
  sortBy: SortBy,
  customFrom?: string,
  customTo?: string
): Promise<Array<Omit<LeaderboardUser, "rank">>> {
  return unstable_cache(
    async () =>
      aggregatePeriodRows(
        await fetchPeriodLeaderboardRows(period, customFrom, customTo, false),
        sortBy
      ),
    [`period-ranking:${periodCacheKey(period, customFrom, customTo)}:${sortBy}`],
    {
      tags: ["leaderboard", `leaderboard:${period}`, "user-rank"],
      revalidate: 60,
    }
  )();
}

export function getLeaderboardData(
  period: Period = "all",
  requestedPage: number = 1,
  limit: number = 50,
  sortBy: SortBy = "tokens",
  requestedSearch: string = "",
  customFrom?: string,
  customTo?: string
): Promise<LeaderboardData> {
  const page = Number.isFinite(requestedPage)
    ? Math.min(MAX_PAGE, Math.max(1, Math.floor(requestedPage)))
    : 1;
  const search = requestedSearch.slice(0, MAX_SEARCH_LENGTH);

  // A plain listing over a bounded period is a slice of the same ranking the
  // viewer's row comes from, so it reads that shared entry instead of caching
  // its own copy. Caching the slice was what let the two drift apart.
  const parsedSearch = parseSearchDirectives(search);
  if (period !== "all" && !hasDirectives(parsedSearch)) {
    return getPeriodRanking(period, sortBy, customFrom, customTo).then((ranking) =>
      pageRanking(ranking, page, limit, period, sortBy, parsedSearch.text)
    );
  }

  const cacheKey = period === "custom"
    ? `leaderboard:custom:${customFrom}:${customTo}:${page}:${limit}:${sortBy}:${search}`
    : period === "today"
    ? `leaderboard:today:${customFrom ?? ""}:${page}:${limit}:${sortBy}:${search}`
    : `leaderboard:${period}:${page}:${limit}:${sortBy}:${search}`;

  return unstable_cache(
    () => fetchLeaderboardData(period, page, limit, sortBy, search, customFrom, customTo),
    [cacheKey],
    {
      tags: ["leaderboard", `leaderboard:${period}`],
      revalidate: 60,
    }
  )();
}

// ============================================================================
// USER RANK
// ============================================================================

/**
 * All-time standing only. Bounded periods are served from the shared ranking
 * in `getPeriodRanking`, which is what keeps them identical to the table.
 */
async function fetchAllTimeUserRank(
  username: string,
  sortBy: SortBy
): Promise<LeaderboardUser | null> {
  const userResult = await db
    .select({ id: users.id, username: users.username, displayName: users.displayName, avatarUrl: users.avatarUrl, verified: verifiedExpr() })
    .from(users)
    .where(and(usernameEqualsIgnoreCase(username), isNull(users.bannedAt)))
    .limit(USERNAME_LOOKUP_LIMIT);

  const user = getSingleUsernameMatch(userResult, username);

  if (!user) {
    return null;
  }

  const userStatsResult = await db
    .select({
      totalTokens: sql<number>`SUM(${submissions.totalTokens})`.as("total_tokens"),
      totalCost: sql<number>`SUM(CAST(${submissions.totalCost} AS DECIMAL(18,4)))`.as("total_cost"),
    })
    .from(submissions)
    .where(eq(submissions.userId, user.id));

  if (!userStatsResult[0] || userStatsResult[0].totalTokens == null) {
    return null;
  }

  const userStats = userStatsResult[0];
  const userTotalTokens = Number(userStats.totalTokens);
  const userTotalCost = userStats.totalCost != null ? Number(userStats.totalCost) : 0;

  const userCompareValue = sortBy === "cost"
    ? userTotalCost
    : userTotalTokens;
  const compareColumn = sortBy === "cost"
    ? sql`SUM(CAST(${submissions.totalCost} AS DECIMAL(18,4)))`
    : sql`SUM(${submissions.totalTokens})`;

  const higherRankedResult = await db
    .select({
      count: sql<number>`COUNT(*)`.as("count"),
    })
    .from(
      db
        .select({
          userId: submissions.userId,
          total: compareColumn.as("total"),
        })
        .from(submissions)
        .innerJoin(users, eq(submissions.userId, users.id))
        .where(isNull(users.bannedAt))
        .groupBy(submissions.userId)
        .having(sql`${compareColumn} > ${userCompareValue}`)
        .as("higher_ranked")
    );

  const rank = Number(higherRankedResult[0]?.count || 0) + 1;

  return {
    rank,
    userId: user.id,
    username: user.username,
    displayName: user.displayName,
    avatarUrl: user.avatarUrl,
    verified: Boolean(user.verified),
    totalTokens: userTotalTokens,
    totalCost: userTotalCost,
  };
}

export function getUserRank(
  username: string,
  period: Period = "all",
  sortBy: SortBy = "tokens",
  customFrom?: string,
  customTo?: string
): Promise<LeaderboardUser | null> {
  // No cache entry of its own for a bounded period: the shared ranking is
  // already cached, and wrapping it again would snapshot that result under a
  // second key — which is precisely the drift this change removes. Looking the
  // viewer up in the array is pure and cheap.
  if (period !== "all") {
    return getPeriodRanking(period, sortBy, customFrom, customTo).then((ranking) =>
      findInRanking(ranking, username)
    );
  }

  // All-time is a different shape: the table is a paginated SQL query the
  // viewer may not appear in, so their standing genuinely needs its own
  // lookup. Both sides sum the same submissions column, and an all-time total
  // moves slowly enough that a minute of skew is invisible.
  const usernameCacheKey = normalizeUsernameCacheKey(username);

  return unstable_cache(
    () => fetchAllTimeUserRank(username, sortBy),
    [`user-rank:${usernameCacheKey}:all:${sortBy}`],
    {
      tags: ["leaderboard", "user-rank", `user-rank:${usernameCacheKey}`],
      revalidate: 60,
    }
  )();
}
