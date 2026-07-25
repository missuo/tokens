import { Suspense } from "react";
import { cookies } from "next/headers";
import { LeaderboardSkeleton } from "@/components/leaderboard/LeaderboardSkeleton";
import { getLeaderboardData, getUserRank } from "@/lib/leaderboard/getLeaderboard";
import type { LeaderboardData, Period, SortBy } from "@/lib/leaderboard/types";
import { getSession } from "@/lib/auth/session";
import {
  SORT_BY_COOKIE_NAME,
  resolveSortByParam,
} from "@/lib/leaderboard/constants";
import { parseCustomDateRange } from "@/lib/leaderboard/dateRange";
import LeaderboardClient from "@/components/leaderboard/Leaderboard";

function isMissingDatabaseUrl(error: unknown): boolean {
  return error instanceof Error && error.message === "DATABASE_URL environment variable is not set";
}

const VALID_PERIODS: Period[] = ["all", "month", "last-month", "week", "today", "custom"];

function createEmptyLeaderboardData(period: Period, sortBy: SortBy): LeaderboardData {
  return {
    users: [],
    pagination: {
      page: 1,
      limit: 50,
      totalUsers: 0,
      totalPages: 0,
      hasNext: false,
      hasPrev: false,
    },
    stats: {
      totalTokens: 0,
      totalCost: 0,
      uniqueUsers: 0,
    },
    period,
    sortBy,
  };
}

interface PageProps {
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
}

export default function LeaderboardPage({ searchParams }: PageProps) {
  return (
    // The board component owns the page container; wrapping it in
    // .main-container again stacked a second set of paddings and made this
    // route sit lower than the others.
    <main id="main-content">
      <Suspense fallback={<LeaderboardSkeleton />}>
        <LeaderboardWithPreferences searchParams={searchParams} />
      </Suspense>
    </main>
  );
}

async function LeaderboardWithPreferences({
  searchParams: searchParamsPromise,
}: {
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
}) {
  const [cookieStore, searchParams] = await Promise.all([cookies(), searchParamsPromise]);
  const sortByCookie = cookieStore.get(SORT_BY_COOKIE_NAME)?.value;
  const periodParam = typeof searchParams.period === "string" ? searchParams.period : null;
  const pageParam =
    typeof searchParams.page === "string" ? Math.max(1, Number(searchParams.page) || 1) : 1;
  const sortByParam = typeof searchParams.sortBy === "string" ? searchParams.sortBy : null;
  const fromParam = typeof searchParams.from === "string" ? searchParams.from : null;
  const toParam = typeof searchParams.to === "string" ? searchParams.to : null;
  const searchParam =
    typeof searchParams.search === "string" ? searchParams.search.trim() : "";

  const sortBy: SortBy =
    resolveSortByParam(sortByParam) ?? resolveSortByParam(sortByCookie) ?? "tokens";

  let period: Period =
    periodParam && VALID_PERIODS.includes(periodParam as Period)
      ? (periodParam as Period)
      : "today";

  const customDateRange =
    period === "custom" ? parseCustomDateRange(fromParam, toParam) : null;

  if (period === "custom" && !customDateRange) {
    period = "all";
  }

  const customFrom = customDateRange?.from;
  const customTo = customDateRange?.to;

  const [initialData, session] = await Promise.all([
    getLeaderboardData(period, pageParam, 50, sortBy, searchParam, customFrom, customTo).catch((error) => {
      if (isMissingDatabaseUrl(error)) {
        return createEmptyLeaderboardData(period, sortBy);
      }
      throw error;
    }),
    getSession().catch((error) => {
      if (isMissingDatabaseUrl(error)) {
        return null;
      }
      throw error;
    }),
  ]);

  const initialUserRank = session
    ? await getUserRank(session.username, period, sortBy, customFrom, customTo).catch((error) => {
        if (isMissingDatabaseUrl(error)) {
          return null;
        }
        throw error;
      })
    : null;

  return (
    <LeaderboardClient
      initialData={initialData}
      currentUser={session}
      initialSortBy={sortBy}
      initialUserRank={initialUserRank}
    />
  );
}
