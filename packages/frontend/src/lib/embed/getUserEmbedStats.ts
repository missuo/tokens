import { unstable_cache } from "next/cache";
import { db, users, submissions, dailyBreakdown } from "@/lib/db";
import {
  USERNAME_LOOKUP_LIMIT,
  getSingleUsernameMatch,
  normalizeUsernameCacheKey,
  usernameEqualsIgnoreCase,
} from "@/lib/db/usernameLookup";
import { eq, sql, and, gte } from "drizzle-orm";

export type EmbedSortBy = "tokens" | "cost";

export interface EmbedContributionDay {
  date: string;
  totalTokens: number;
  totalCost: number;
  intensity: 0 | 1 | 2 | 3 | 4;
}

export interface EmbedTodayModel {
  modelId: string;
  tokens: number;
  cost: number;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  reasoning: number;
  messages: number;
}

export interface EmbedTodayClient {
  /** ClientType key, e.g. "claude". */
  source: string;
  tokens: number;
  cost: number;
  messages: number;
  /** Models used by this client today, sorted by cost descending. */
  models: EmbedTodayModel[];
}

export interface EmbedTodayUsage {
  /** UTC calendar date (YYYY-MM-DD) this usage is for. */
  date: string;
  tokens: number;
  cost: number;
  /** Clients active today, sorted by cost descending. */
  clients: EmbedTodayClient[];
}

export interface UserEmbedStats {
  user: {
    id: string;
    username: string;
    displayName: string | null;
    avatarUrl: string | null;
  };
  stats: {
    totalTokens: number;
    totalCost: number;
    submissionCount: number;
    rank: number | null;
    /** Total number of ranked users, for rendering "rank N of total". */
    rankTotal?: number | null;
    updatedAt: string | null;
  };
}

async function fetchUserEmbedStats(username: string, sortBy: EmbedSortBy): Promise<UserEmbedStats | null> {
  const matchingUsers = await db
    .select({
      id: users.id,
      username: users.username,
      displayName: users.displayName,
      avatarUrl: users.avatarUrl,
      totalTokens: sql<number>`COALESCE(${submissions.totalTokens}, 0)`,
      totalCost: sql<number>`COALESCE(CAST(${submissions.totalCost} AS DECIMAL(12,4)), 0)`,
      submissionCount: sql<number>`COALESCE(${submissions.submitCount}, 0)`,
      updatedAt: submissions.updatedAt,
    })
    .from(users)
    .leftJoin(submissions, eq(submissions.userId, users.id))
    .where(usernameEqualsIgnoreCase(username))
    .limit(USERNAME_LOOKUP_LIMIT);
  const result = getSingleUsernameMatch(matchingUsers, username);

  if (!result) {
    return null;
  }

  let rank: number | null = null;
  let rankTotal: number | null = null;

  const rankingValue = sortBy === "cost" ? Number(result.totalCost) || 0 : Number(result.totalTokens) || 0;

  if (rankingValue > 0) {
    const rankResult = await db.execute<{ rank: number; total: number }>(sql`
      WITH ranked AS (
        SELECT
          user_id,
          RANK() OVER (
            ORDER BY
              ${sortBy === "cost"
                ? sql`CAST(total_cost AS DECIMAL(12,4)) DESC`
                : sql`total_tokens DESC`}
          ) AS rank
        FROM submissions
      )
      SELECT rank, (SELECT COUNT(*)::int FROM submissions) AS total
      FROM ranked WHERE user_id = ${result.id}
    `);

    const rankRow = (rankResult as unknown as { rank: number; total: number }[])[0];
    rank = rankRow?.rank || null;
    rankTotal = rankRow?.total || null;
  }

  return {
    user: {
      id: result.id,
      username: result.username,
      displayName: result.displayName,
      avatarUrl: result.avatarUrl,
    },
    stats: {
      totalTokens: Number(result.totalTokens) || 0,
      totalCost: Number(result.totalCost) || 0,
      submissionCount: Number(result.submissionCount) || 0,
      rank,
      rankTotal,
      updatedAt: result.updatedAt?.toISOString() || null,
    },
  };
}

export function getUserEmbedStats(username: string, sortBy: EmbedSortBy = "tokens"): Promise<UserEmbedStats | null> {
  const usernameCacheKey = normalizeUsernameCacheKey(username);

  return unstable_cache(
    () => fetchUserEmbedStats(username, sortBy),
    [`embed-user:${usernameCacheKey}:${sortBy}`],
    {
      tags: [
        `user:${usernameCacheKey}`,
        `embed-user:${usernameCacheKey}`,
        `embed-user:${usernameCacheKey}:${sortBy}`,
      ],
      revalidate: 60,
    }
  )();
}

async function fetchUserEmbedContributions(username: string): Promise<EmbedContributionDay[] | null> {
  const matchingUsers = await db
    .select({ id: users.id })
    .from(users)
    .where(usernameEqualsIgnoreCase(username))
    .limit(USERNAME_LOOKUP_LIMIT);
  const user = getSingleUsernameMatch(matchingUsers, username);

  if (!user) return null;

  // Use UTC-based date and include a 7-day buffer before "one year ago"
  // so that all dates visible in the first week of the contribution grid are included.
  const today = new Date();
  const cutoffDate = new Date(Date.UTC(today.getUTCFullYear() - 1, today.getUTCMonth(), today.getUTCDate()));
  cutoffDate.setUTCDate(cutoffDate.getUTCDate() - 7);
  const cutoff = cutoffDate.toISOString().split("T")[0];

  const rows = await db
    .select({
      date: dailyBreakdown.date,
      tokens: sql<number>`sum(${dailyBreakdown.tokens})`.as("tokens"),
      cost: sql<number>`sum(${dailyBreakdown.cost})`.as("cost"),
    })
    .from(dailyBreakdown)
    .innerJoin(submissions, eq(dailyBreakdown.submissionId, submissions.id))
    .where(and(eq(submissions.userId, user.id), gte(dailyBreakdown.date, cutoff)))
    .groupBy(dailyBreakdown.date)
    .orderBy(dailyBreakdown.date);

  if (rows.length === 0) return [];

  const costs = rows.map((row) => Number(row.cost) || 0).filter((c) => c > 0);
  const maxCost = Math.max(...costs, 0);

  return rows.map((row) => {
    const totalTokens = Number(row.tokens) || 0;
    const cost = Number(row.cost) || 0;
    return {
      date: row.date,
      totalTokens,
      totalCost: cost,
      intensity: (
        maxCost === 0 ? 0 : cost === 0 ? 0 : cost <= maxCost * 0.25 ? 1 : cost <= maxCost * 0.5 ? 2 : cost <= maxCost * 0.75 ? 3 : 4
      ) as 0 | 1 | 2 | 3 | 4,
    };
  });
}

export function getUserEmbedContributions(username: string): Promise<EmbedContributionDay[] | null> {
  const usernameCacheKey = normalizeUsernameCacheKey(username);

  return unstable_cache(
    () => fetchUserEmbedContributions(username),
    [`embed-contrib:${usernameCacheKey}`],
    {
      tags: [`user:${usernameCacheKey}`, `embed-contrib:${usernameCacheKey}`],
      revalidate: 60,
    }
  )();
}

/** Accumulator mirroring EmbedTodayModel, summed across device rows. */
type ModelAccumulator = Omit<EmbedTodayModel, "modelId">;

function emptyModelAcc(): ModelAccumulator {
  return { tokens: 0, cost: 0, input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0, messages: 0 };
}

async function fetchUserEmbedToday(username: string): Promise<EmbedTodayUsage | null> {
  const matchingUsers = await db
    .select({ id: users.id })
    .from(users)
    .where(usernameEqualsIgnoreCase(username))
    .limit(USERNAME_LOOKUP_LIMIT);
  const user = getSingleUsernameMatch(matchingUsers, username);

  if (!user) return null;

  // "Today" is the current UTC calendar date: embeds are server-rendered and
  // CDN-cached static SVGs, so there is no viewer timezone to localize to.
  const today = new Date().toISOString().split("T")[0];

  const rows = await db
    .select({
      tokens: dailyBreakdown.tokens,
      cost: dailyBreakdown.cost,
      sourceBreakdown: dailyBreakdown.sourceBreakdown,
    })
    .from(dailyBreakdown)
    .innerJoin(submissions, eq(dailyBreakdown.submissionId, submissions.id))
    .where(and(eq(submissions.userId, user.id), eq(dailyBreakdown.date, today)));

  let totalTokens = 0;
  let totalCost = 0;
  // source key -> { tokens, cost, messages, models: modelId -> ModelAccumulator }
  const clients = new Map<
    string,
    { tokens: number; cost: number; messages: number; models: Map<string, ModelAccumulator> }
  >();

  for (const row of rows) {
    totalTokens += Number(row.tokens) || 0;
    totalCost += Number(row.cost) || 0;

    const breakdown = row.sourceBreakdown;
    if (!breakdown) continue;

    for (const [source, sourceData] of Object.entries(breakdown)) {
      if (!sourceData) continue;
      let client = clients.get(source);
      if (!client) {
        client = { tokens: 0, cost: 0, messages: 0, models: new Map() };
        clients.set(source, client);
      }
      client.tokens += Number(sourceData.tokens) || 0;
      client.cost += Number(sourceData.cost) || 0;
      client.messages += Number(sourceData.messages) || 0;

      const models = sourceData.models ?? {};
      for (const [modelId, modelData] of Object.entries(models)) {
        if (!modelData) continue;
        let acc = client.models.get(modelId);
        if (!acc) {
          acc = emptyModelAcc();
          client.models.set(modelId, acc);
        }
        acc.tokens += Number(modelData.tokens) || 0;
        acc.cost += Number(modelData.cost) || 0;
        acc.input += Number(modelData.input) || 0;
        acc.output += Number(modelData.output) || 0;
        acc.cacheRead += Number(modelData.cacheRead) || 0;
        acc.cacheWrite += Number(modelData.cacheWrite) || 0;
        acc.reasoning += Number(modelData.reasoning) || 0;
        acc.messages += Number(modelData.messages) || 0;
      }
    }
  }

  const clientList: EmbedTodayClient[] = Array.from(clients.entries())
    .map(([source, client]) => ({
      source,
      tokens: client.tokens,
      cost: client.cost,
      messages: client.messages,
      models: Array.from(client.models.entries())
        .map(([modelId, acc]) => ({ modelId, ...acc }))
        .sort((a, b) => b.cost - a.cost),
    }))
    .sort((a, b) => b.cost - a.cost);

  return { date: today, tokens: totalTokens, cost: totalCost, clients: clientList };
}

export function getUserEmbedToday(username: string): Promise<EmbedTodayUsage | null> {
  const usernameCacheKey = normalizeUsernameCacheKey(username);

  return unstable_cache(
    () => fetchUserEmbedToday(username),
    [`embed-today:${usernameCacheKey}`],
    {
      tags: [`user:${usernameCacheKey}`, `embed-today:${usernameCacheKey}`],
      revalidate: 60,
    }
  )();
}
