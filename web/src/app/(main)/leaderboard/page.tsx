import { getLeaderboardData } from "@/lib/leaderboard/getLeaderboard";
import type { LeaderboardData, Period, SortBy } from "@/lib/leaderboard/types";
import { hasDirectives, parseSearchDirectives } from "@/lib/leaderboard/searchDirectives";
import { isValidDateString, parseCustomDateRange } from "@/lib/leaderboard/dateRange";
import LeaderboardClient from "@/components/leaderboard/Leaderboard";

function isMissingDatabaseUrl(error: unknown): boolean {
  return error instanceof Error && error.message === "DATABASE_URL environment variable is not set";
}

const VALID_PERIODS: Period[] = ["all", "month", "last-month", "week", "today", "custom"];

/**
 * The board is sent whole rather than a page at a time.
 *
 * 208 people have ever submitted usage, and the ceiling for a leaderboard of
 * AI coding tools is thousands, not millions — measured, one row costs ~69
 * bytes gzipped, so the entire ranking is ~14 KB today and would be ~69 KB at
 * a thousand. That is smaller than the page that carries it.
 *
 * Paying that once buys three things. Paging, sorting and text search stop
 * being server round trips and become array operations. The viewer's own row
 * is guaranteed to be present, so their standing is read from the same bytes
 * as the table instead of from a second query — which is the only way to make
 * the two agree rather than merely usually agree. And nothing on this page
 * depends on who is asking any more, so it is one cached document for every
 * reader, signed in or not.
 *
 * The bound is here rather than absent so a runaway dataset degrades into a
 * truncated board instead of an unbounded response.
 */
const FULL_BOARD_LIMIT = 5000;

function createEmptyLeaderboardData(period: Period, sortBy: SortBy): LeaderboardData {
  return {
    users: [],
    pagination: {
      page: 1,
      limit: FULL_BOARD_LIMIT,
      totalUsers: 0,
      totalPages: 0,
      hasNext: false,
      hasPrev: false,
    },
    stats: { totalTokens: 0, totalCost: 0, uniqueUsers: 0 },
    period,
    sortBy,
  };
}

interface PageProps {
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
}

/**
 * Deliberately not streamed.
 *
 * This used to render a skeleton inside a Suspense boundary while the board
 * resolved. That is free on a healthy request and wrong on a failing one:
 * streaming commits the status line as soon as the shell is flushed, so an
 * async child that throws afterwards cannot turn the response into a 5xx. With
 * the database unreachable this page answered `200` with an empty board and the
 * shareable cache-control middleware had already attached — an empty
 * leaderboard, stored by the edge, served to everyone in the colo for a minute
 * and a half after the database came back.
 *
 * Awaiting here puts the failure before the first byte, where it can still be a
 * 500 that Caddy strips the cache-control from. The cost is the shell no longer
 * appears first, and that cost is small: this render is ~40ms, and almost every
 * reader is answered by the edge without reaching it at all.
 */
export default async function LeaderboardPage({ searchParams }: PageProps) {
  return (
    // The board component owns the page container; wrapping it in
    // .main-container again stacked a second set of paddings and made this
    // route sit lower than the others.
    <main id="main-content">
      <Board searchParams={searchParams} />
    </main>
  );
}

async function Board({
  searchParams: searchParamsPromise,
}: {
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
}) {
  const searchParams = await searchParamsPromise;
  const periodParam = typeof searchParams.period === "string" ? searchParams.period : null;
  const fromParam = typeof searchParams.from === "string" ? searchParams.from : null;
  const toParam = typeof searchParams.to === "string" ? searchParams.to : null;
  const searchParam =
    typeof searchParams.search === "string" ? searchParams.search.trim() : "";

  let period: Period =
    periodParam && VALID_PERIODS.includes(periodParam as Period)
      ? (periodParam as Period)
      : "today";

  const customDateRange =
    period === "custom" ? parseCustomDateRange(fromParam, toParam) : null;

  if (period === "custom" && !customDateRange) {
    period = "all";
  }

  // Daily rows are bucketed by the submitter's *local* date, so resolving
  // "today" against the server's UTC date silently excludes everyone whose
  // local calendar has already rolled over — eight hours out of every day for
  // UTC+8. The client sends its own date up as `from` once it has hydrated;
  // this is the only place that reads it back. Without this the whole "today"
  // board is computed for the wrong day.
  const localToday =
    period === "today" && isValidDateString(fromParam) ? fromParam : undefined;

  const customFrom = customDateRange?.from ?? localToday;
  const customTo = customDateRange?.to;

  // A `client:`/`model:` directive re-runs the aggregation over a filtered
  // subset, so it has to reach the database. Plain text is a filter over rows
  // the client already holds, and forwarding it would split the cache into one
  // entry per search box keystroke for a result the client can produce itself.
  const directiveSearch = hasDirectives(parseSearchDirectives(searchParam))
    ? searchParam
    : "";

  // Always the default sort. The viewer's preference lives in a cookie the
  // client writes and can read back itself, and re-ordering rows the client
  // already holds costs nothing — reading that cookie here would give every
  // preference its own copy of an otherwise identical document.
  const data = await getLeaderboardData(
    period,
    1,
    FULL_BOARD_LIMIT,
    "tokens",
    directiveSearch,
    customFrom,
    customTo,
  ).catch((error) => {
    if (isMissingDatabaseUrl(error)) {
      return createEmptyLeaderboardData(period, "tokens");
    }
    throw error;
  });

  return <LeaderboardClient initialData={data} directiveSearch={directiveSearch} />;
}
