"use client";

import { useState, useEffect, useRef, useMemo, memo, useCallback } from "react";
import { useRouter } from "nextjs-toploader/app";
import { useSearchParams, usePathname } from "next/navigation";
import { Button } from "@heroui/react";
import { SearchIcon, XIcon } from "@/components/ui/Icons";
import { TabBar } from "@/components/TabBar";
import { StatGrid, StatTile, Panel } from "@/components/ui/primitives";
import { LeaderboardSkeleton } from "@/components/Skeleton";
import { CommandSnippet } from "@/components/ui/CommandSnippet";
import { formatCurrency, formatNumber, formatDuration } from "@/lib/utils";
import { useSettings } from "@/lib/useSettings";
import { isValidSortBy, type LeaderboardSortBy } from "@/lib/leaderboard/constants";
import { parseCustomDateRange } from "@/lib/leaderboard/dateRange";

export type Period = "all" | "month" | "last-month" | "week" | "custom";

export interface LeaderboardUser {
  rank: number;
  userId: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  totalTokens: number;
  totalCost: number;
  totalActiveTimeMs: number | null;
  submissionCount: number | null;
  lastSubmission: string;
}

export interface LeaderboardData {
  users: LeaderboardUser[];
  pagination: {
    page: number;
    limit: number;
    totalUsers: number;
    totalPages: number;
    hasNext: boolean;
    hasPrev: boolean;
  };
  stats: {
    totalTokens: number;
    totalCost: number;
    totalActiveTimeMs: number | null;
    totalSubmissions: number | null;
    uniqueUsers: number;
  };
  period: Period;
  sortBy?: "tokens" | "cost" | "time";
}

interface LeaderboardClientProps {
  initialData: LeaderboardData;
  currentUser: { id: string; username: string; displayName: string | null; avatarUrl: string | null } | null;
  initialSortBy: "tokens" | "cost" | "time";
  initialUserRank: LeaderboardUser | null;
}

function isValidLeaderboardData(data: unknown): data is LeaderboardData {
  return (
    typeof data === "object" &&
    data !== null &&
    "users" in data &&
    "pagination" in data &&
    "stats" in data &&
    Array.isArray((data as LeaderboardData).users)
  );
}

const rankColor: Record<number, string> = {
  1: "text-[#EAB308]",
  2: "text-[#9CA3AF]",
  3: "text-[#D97706]",
};

interface LeaderboardRowProps {
  user: LeaderboardUser;
  isCurrentUser: boolean;
  showSubmissionCount: boolean;
  showTime: boolean;
  onRowClick: (username: string) => void;
}

const LeaderboardRow = memo(function LeaderboardRow({
  user,
  isCurrentUser,
  showSubmissionCount,
  showTime,
  onRowClick,
}: LeaderboardRowProps) {
  const formattedTokens = useMemo(() => user.totalTokens.toLocaleString("en-US"), [user.totalTokens]);
  const formattedCost = useMemo(
    () => user.totalCost.toLocaleString("en-US", { style: "currency", currency: "USD", minimumFractionDigits: 2 }),
    [user.totalCost],
  );

  return (
    <tr
      onClick={() => onRowClick(user.username)}
      className={`group cursor-pointer border-b border-line transition-colors last:border-b-0 ${
        isCurrentUser ? "bg-accent/[0.07] shadow-[inset_4px_0_0_var(--accent)]" : "hover:bg-foreground/[0.03]"
      }`}
    >
      <td className="w-px py-2.5 pr-3 pl-3 whitespace-nowrap sm:pl-6">
        <span className={`font-mono text-sm font-bold tabular-nums sm:text-base ${rankColor[user.rank] ?? "text-muted"}`}>#{user.rank}</span>
      </td>
      <td className="w-px py-2.5 pr-3 pl-1 whitespace-nowrap">
        <div className="flex items-center gap-2 sm:gap-3">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={user.avatarUrl || `https://github.com/${user.username}.png`}
            alt={user.username}
            width={40}
            height={40}
            className="h-9 w-9 shrink-0 rounded-full object-cover ring-1 ring-line sm:h-10 sm:w-10"
          />
          <div className="min-w-0 max-w-[160px] sm:max-w-[240px]">
            <p className="truncate text-sm font-medium text-foreground sm:text-base">{user.displayName || user.username}</p>
            <p className="truncate font-mono text-xs text-muted sm:text-sm">@{user.username}</p>
          </div>
        </div>
      </td>
      <td aria-hidden="true" className="w-full" />
      <td className="w-px px-4 py-2.5 text-right whitespace-nowrap max-[560px]:hidden">
        <span className="font-mono text-sm font-medium text-foreground tabular-nums sm:text-base" title={formattedCost}>
          {formatCurrency(user.totalCost)}
        </span>
      </td>
      <td className="w-px px-4 py-2.5 text-right whitespace-nowrap">
        <div className="flex flex-col items-end gap-0.5 min-[561px]:block">
          <span className="font-mono text-sm font-semibold text-accent tabular-nums transition-colors sm:text-base" title={formattedTokens}>
            <span className="hidden md:inline">{formattedTokens}</span>
            <span className="md:hidden">{formatNumber(user.totalTokens)}</span>
          </span>
          <span className="font-mono text-xs font-normal text-muted tabular-nums min-[561px]:hidden" title={formattedCost}>
            {formatCurrency(user.totalCost)}
          </span>
        </div>
      </td>
      {showTime && (
        <td className="w-px px-4 py-2.5 text-right whitespace-nowrap max-md:hidden">
          <span className="font-mono text-sm font-medium text-foreground tabular-nums sm:text-base">{formatDuration(user.totalActiveTimeMs)}</span>
        </td>
      )}
      {showSubmissionCount && (
        <td className="w-px px-4 py-2.5 text-right whitespace-nowrap max-md:hidden sm:pr-6">
          <span className="font-mono text-sm text-muted tabular-nums">{user.submissionCount ?? "—"}</span>
        </td>
      )}
    </tr>
  );
});

const VALID_PERIODS: Period[] = ["all", "month", "last-month", "week", "custom"];

function parsePeriodParam(value: string | null): Period | null {
  if (!value) return null;
  return VALID_PERIODS.includes(value as Period) ? (value as Period) : null;
}

const dateInputClass =
  "min-w-[140px] rounded-lg border border-line bg-surface-secondary px-3 py-2 text-sm text-foreground outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/30 [&::-webkit-calendar-picker-indicator]:cursor-pointer [&::-webkit-calendar-picker-indicator]:invert-[0.7]";

export default function LeaderboardClient({ initialData, currentUser, initialSortBy, initialUserRank }: LeaderboardClientProps) {
  const router = useRouter();
  const searchParams = useSearchParams();
  const pathname = usePathname();

  const urlPeriod = parsePeriodParam(searchParams.get("period"));
  const urlPage = searchParams.get("page") ? Math.max(1, Number(searchParams.get("page")) || 1) : null;
  const sortByParam = searchParams.get("sortBy");
  const urlSortBy = isValidSortBy(sortByParam) ? sortByParam : null;
  const urlFrom = searchParams.get("from") || "";
  const urlTo = searchParams.get("to") || "";
  const urlSearch = searchParams.get("search")?.trim() || "";
  const initialCustomDateRange = parseCustomDateRange(urlPeriod === "custom" ? urlFrom : null, urlPeriod === "custom" ? urlTo : null);

  const [data, setData] = useState<LeaderboardData>(initialData);
  const [error, setError] = useState<string | null>(null);
  const [period, setPeriod] = useState<Period>(initialData.period);
  const [page, setPage] = useState(urlPage || initialData.pagination.page);
  const [currentUserRank, setCurrentUserRank] = useState<LeaderboardUser | null>(initialUserRank);
  const [currentUserRankError, setCurrentUserRankError] = useState(false);
  const [searchQuery, setSearchQuery] = useState(urlSearch);
  const [debouncedSearch, setDebouncedSearch] = useState(urlSearch);
  const [retryToken, setRetryToken] = useState(0);
  const [customFrom, setCustomFrom] = useState(initialCustomDateRange?.from || "");
  const [customTo, setCustomTo] = useState(initialCustomDateRange?.to || "");
  const [appliedFrom, setAppliedFrom] = useState(initialCustomDateRange?.from || "");
  const [appliedTo, setAppliedTo] = useState(initialCustomDateRange?.to || "");
  const [resolvedRequest, setResolvedRequest] = useState({
    period: initialData.period,
    page: initialData.pagination.page,
    sortBy: initialSortBy,
    search: urlSearch,
    retryToken: 0,
    customFrom: initialCustomDateRange?.from || "",
    customTo: initialCustomDateRange?.to || "",
  });

  const { leaderboardSortBy, setLeaderboardSort, mounted } = useSettings();

  // URL `?sortBy=` wins on first paint; once the user clicks, their persisted
  // choice takes over. Clearing `urlSortOverride` on click is required.
  const [urlSortOverride, setUrlSortOverride] = useState<LeaderboardSortBy | null>(urlSortBy);
  const effectiveSortBy = urlSortOverride ? urlSortOverride : mounted ? leaderboardSortBy : initialSortBy;
  const requestedPage = data.pagination.totalPages > 0 ? Math.min(page, data.pagination.totalPages) : page;
  const isCustomWithoutDates = period === "custom" && (!appliedFrom || !appliedTo);
  const isLoading =
    !isCustomWithoutDates &&
    (period !== resolvedRequest.period ||
      requestedPage !== resolvedRequest.page ||
      effectiveSortBy !== resolvedRequest.sortBy ||
      debouncedSearch !== resolvedRequest.search ||
      retryToken !== resolvedRequest.retryToken ||
      (period === "custom" && (appliedFrom !== resolvedRequest.customFrom || appliedTo !== resolvedRequest.customTo)));

  const isFirstRankFetch = useRef(true);
  const isFirstUrlSync = useRef(true);

  useEffect(() => {
    if (isFirstUrlSync.current) {
      isFirstUrlSync.current = false;
      return;
    }
    const params = new URLSearchParams();
    const currentView = searchParams.get("view");
    if (currentView) params.set("view", currentView);
    if (period !== "all") params.set("period", period);
    if (requestedPage > 1) params.set("page", String(requestedPage));
    if (effectiveSortBy !== "tokens") params.set("sortBy", effectiveSortBy);
    if (period === "custom" && appliedFrom) params.set("from", appliedFrom);
    if (period === "custom" && appliedTo) params.set("to", appliedTo);
    if (debouncedSearch) params.set("search", debouncedSearch);
    const qs = params.toString();
    window.history.replaceState(null, "", qs ? `${pathname}?${qs}` : pathname);
  }, [period, requestedPage, effectiveSortBy, appliedFrom, appliedTo, pathname, debouncedSearch, searchParams]);

  const isSearchMounted = useRef(false);
  useEffect(() => {
    if (!isSearchMounted.current) {
      isSearchMounted.current = true;
      return;
    }
    const timer = setTimeout(() => {
      setDebouncedSearch(searchQuery);
      setPage(1);
    }, 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  useEffect(() => {
    if (!currentUser) return;
    if (isFirstRankFetch.current) {
      isFirstRankFetch.current = false;
      return;
    }
    const abortController = new AbortController();
    const customParams = period === "custom" ? `&from=${appliedFrom}&to=${appliedTo}` : "";
    fetch(`/api/leaderboard/user/${currentUser.username}?period=${period}&sortBy=${effectiveSortBy}${customParams}`, {
      signal: abortController.signal,
    })
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      })
      .then((userData) => {
        setCurrentUserRank(userData);
        setCurrentUserRankError(false);
      })
      .catch((err) => {
        if (err.name !== "AbortError") {
          setCurrentUserRank(null);
          setCurrentUserRankError(true);
        }
      });
    return () => abortController.abort();
  }, [currentUser, period, effectiveSortBy, appliedFrom, appliedTo]);

  const fetchData = useCallback(
    (
      targetPeriod: Period,
      targetPage: number,
      targetSortBy: LeaderboardSortBy,
      targetSearch: string,
      targetRetryToken: number,
      signal?: AbortSignal,
      targetCustomFrom?: string,
      targetCustomTo?: string,
    ) => {
      const searchParam = targetSearch ? `&search=${encodeURIComponent(targetSearch)}` : "";
      const customParams =
        targetPeriod === "custom" && targetCustomFrom && targetCustomTo ? `&from=${targetCustomFrom}&to=${targetCustomTo}` : "";
      fetch(`/api/leaderboard?period=${targetPeriod}&page=${targetPage}&limit=50&sortBy=${targetSortBy}${searchParam}${customParams}`, { signal })
        .then((res) => {
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          return res.json();
        })
        .then((result) => {
          if (!isValidLeaderboardData(result)) throw new Error("Invalid response format");
          setData(result);
          setError(null);
          setResolvedRequest({
            period: targetPeriod,
            page: result.pagination.page,
            sortBy: targetSortBy,
            search: targetSearch,
            retryToken: targetRetryToken,
            customFrom: targetCustomFrom || "",
            customTo: targetCustomTo || "",
          });
        })
        .catch((err) => {
          if (err.name !== "AbortError") {
            setError(err.message || "Failed to load");
            setResolvedRequest({
              period: targetPeriod,
              page: targetPage,
              sortBy: targetSortBy,
              search: targetSearch,
              retryToken: targetRetryToken,
              customFrom: targetCustomFrom || "",
              customTo: targetCustomTo || "",
            });
          }
        });
    },
    [],
  );

  useEffect(() => {
    if (!isLoading) return;
    if (period === "custom" && (!appliedFrom || !appliedTo)) return;
    const abortController = new AbortController();
    fetchData(period, requestedPage, effectiveSortBy, debouncedSearch, retryToken, abortController.signal, appliedFrom, appliedTo);
    return () => abortController.abort();
  }, [appliedFrom, appliedTo, debouncedSearch, effectiveSortBy, fetchData, isLoading, period, requestedPage, retryToken]);

  const sortedUsers = data.users || [];
  const showSubmissionCount = period === "all";
  const showTime = true;

  const handleRowClick = useCallback((username: string) => router.push(`/u/${username}`), [router]);

  const sortOptions: { id: LeaderboardSortBy; label: string }[] = [
    { id: "tokens", label: "Tokens" },
    { id: "cost", label: "Cost" },
    { id: "time", label: "Time" },
  ];

  const totalTokensFull = data.stats.totalTokens.toLocaleString("en-US");
  const totalCostFull = data.stats.totalCost.toLocaleString("en-US", { style: "currency", currency: "USD", minimumFractionDigits: 2 });

  return (
    <>
      <section className="mt-6 mb-8">
        <StatGrid cols={3}>
          <StatTile label="Users" value={data.stats.uniqueUsers} />
          <StatTile label="Total Tokens" value={formatNumber(data.stats.totalTokens)} title={totalTokensFull} accent />
          <StatTile label="Total Cost" value={formatCurrency(data.stats.totalCost)} title={totalCostFull} />
        </StatGrid>
      </section>

      {currentUser && currentUserRankError && (
        <div className="mb-6 flex items-center gap-2 rounded-lg border border-danger/40 bg-danger/10 px-4 py-3 text-sm text-danger">
          <span>⚠️</span>
          <span>Unable to load your ranking. Please refresh the page.</span>
        </div>
      )}

      {currentUser && currentUserRank && (
        <div className="mb-6 flex items-center justify-between gap-4 rounded-xl border border-accent/40 bg-accent/[0.06] p-4 ring-1 ring-accent/10 max-[640px]:flex-col max-[640px]:items-stretch">
          <div className="flex min-w-0 flex-1 items-center gap-3">
            <span className="font-mono text-sm font-semibold text-accent tabular-nums">#{currentUserRank.rank}</span>
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src={currentUser.avatarUrl || `https://github.com/${currentUser.username}.png`}
              alt={currentUser.username}
              width={44}
              height={44}
              className="h-11 w-11 shrink-0 rounded-full object-cover ring-1 ring-line"
            />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-semibold text-foreground">{currentUser.displayName || currentUser.username}</p>
              <p className="truncate font-mono text-xs text-muted">@{currentUser.username}</p>
            </div>
          </div>
          <div className="flex items-center gap-6 max-[640px]:justify-between">
            <div className="text-right max-[640px]:text-left">
              <p className="mb-0.5 text-[11px] font-semibold tracking-wider text-muted uppercase">Tokens</p>
              <p className="font-mono text-base font-semibold text-accent tabular-nums" title={currentUserRank.totalTokens.toLocaleString("en-US")}>
                {formatNumber(currentUserRank.totalTokens)}
              </p>
            </div>
            <div className="text-right max-[640px]:text-left">
              <p className="mb-0.5 text-[11px] font-semibold tracking-wider text-muted uppercase">Cost</p>
              <p className="font-mono text-base font-semibold text-foreground tabular-nums">{formatCurrency(currentUserRank.totalCost)}</p>
            </div>
          </div>
        </div>
      )}

      <div className="mb-6">
        <TabBar
          tabs={[
            { id: "all" as Period, label: "All Time" },
            { id: "last-month" as Period, label: "Last Month" },
            { id: "month" as Period, label: "This Month" },
            { id: "week" as Period, label: "This Week" },
            { id: "custom" as Period, label: "Custom" },
          ]}
          activeTab={period}
          onTabChange={(tab) => {
            setPeriod(tab);
            setPage(1);
            if (tab !== "custom") {
              setAppliedFrom("");
              setAppliedTo("");
              setCustomFrom("");
              setCustomTo("");
            }
          }}
        />
      </div>

      {period === "custom" && (
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <input type="date" value={customFrom} onChange={(e) => setCustomFrom(e.target.value)} max={customTo || undefined} className={dateInputClass} />
          <span className="text-sm text-muted">~</span>
          <input type="date" value={customTo} onChange={(e) => setCustomTo(e.target.value)} min={customFrom || undefined} className={dateInputClass} />
          <Button
            isDisabled={!parseCustomDateRange(customFrom, customTo)}
            onPress={() => {
              const parsed = parseCustomDateRange(customFrom, customTo);
              if (!parsed) return;
              setAppliedFrom(parsed.from);
              setAppliedTo(parsed.to);
              setPage(1);
            }}
            className="bg-accent text-accent-foreground"
          >
            Apply
          </Button>
        </div>
      )}

      <div className="mb-4 flex items-center justify-between gap-3 max-[560px]:flex-col max-[560px]:items-stretch">
        <div className="relative max-w-80 flex-1 max-[560px]:max-w-none">
          <span className="pointer-events-none absolute top-1/2 left-3 flex -translate-y-1/2 items-center text-muted">
            <SearchIcon size={16} />
          </span>
          <input
            type="text"
            placeholder="Search users..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full rounded-lg border border-line bg-surface px-9 py-2 text-sm text-foreground outline-none transition placeholder:text-muted focus:border-accent focus:ring-2 focus:ring-accent/25"
          />
          {searchQuery && (
            <button onClick={() => setSearchQuery("")} aria-label="Clear search" className="absolute top-1/2 right-2 flex -translate-y-1/2 items-center rounded p-1 text-muted hover:text-foreground">
              <XIcon size={16} />
            </button>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2 max-[560px]:justify-between">
          <span className="text-xs font-medium text-muted">Sort by</span>
          <TabBar<LeaderboardSortBy>
            aria-label="Sort leaderboard"
            size="sm"
            tabs={sortOptions}
            activeTab={effectiveSortBy}
            onTabChange={(id) => {
              setUrlSortOverride(null);
              setLeaderboardSort(id);
            }}
          />
        </div>
      </div>

      {isLoading ? (
        <LeaderboardSkeleton />
      ) : error ? (
        <Panel className="p-8 text-center">
          <p className="mb-4 text-muted">Failed to load leaderboard</p>
          <p className="text-sm text-muted">{error}</p>
          <Button onPress={() => setRetryToken((p) => p + 1)} className="mt-4 bg-accent text-accent-foreground">
            Retry
          </Button>
        </Panel>
      ) : (
        <Panel className="overflow-hidden">
          {data.users.length === 0 ? (
            <div className="p-8 text-center">
              {debouncedSearch ? (
                <>
                  <p className="mb-4 text-muted">No users found for &ldquo;{debouncedSearch}&rdquo;</p>
                  <p className="text-sm text-muted">Try a different search term</p>
                </>
              ) : (
                <>
                  <p className="mb-4 text-muted">No submissions yet. Be the first!</p>
                  <p className="text-sm text-muted">
                    Run <code className="rounded bg-surface-secondary px-2 py-1 font-mono">tokens login &amp;&amp; tokens submit</code>
                  </p>
                </>
              )}
            </div>
          ) : (
            <>
              <div className="overflow-x-auto">
                <table className="w-full min-w-[500px] max-[560px]:min-w-0">
                  <thead className="border-b border-line bg-surface-secondary">
                    <tr>
                      <th className="w-px py-3 pr-3 pl-3 text-left text-xs font-medium tracking-wider whitespace-nowrap text-muted uppercase sm:pl-6">Rank</th>
                      <th className="w-px py-3 pr-3 pl-1 text-left text-xs font-medium tracking-wider whitespace-nowrap text-muted uppercase">User</th>
                      {/* Flexible spacer: absorbs slack so the User column hugs its content and the numeric columns group tightly on the right. */}
                      <th aria-hidden="true" className="w-full" />
                      <th className="w-px px-4 py-3 text-right text-xs font-medium tracking-wider whitespace-nowrap text-muted uppercase max-[560px]:hidden">Cost</th>
                      <th className="w-px px-4 py-3 text-right text-xs font-medium tracking-wider whitespace-nowrap text-muted uppercase">Tokens</th>
                      {showTime && <th className="w-px px-4 py-3 text-right text-xs font-medium tracking-wider whitespace-nowrap text-muted uppercase max-md:hidden">Time</th>}
                      {showSubmissionCount && <th className="w-px px-4 py-3 text-right text-xs font-medium tracking-wider whitespace-nowrap text-muted uppercase max-md:hidden sm:pr-6">Submits</th>}
                    </tr>
                  </thead>
                  <tbody>
                    {sortedUsers.map((user) => (
                      <LeaderboardRow
                        key={user.userId}
                        user={user}
                        isCurrentUser={!!(currentUser && user.username === currentUser.username)}
                        showSubmissionCount={showSubmissionCount}
                        showTime={showTime}
                        onRowClick={handleRowClick}
                      />
                    ))}
                  </tbody>
                </table>
              </div>

              {data.pagination.totalPages > 1 && (
                <div className="flex flex-col items-center justify-between gap-3 border-t border-line px-3 py-3 sm:flex-row sm:px-6 sm:py-4">
                  <p className="text-center text-xs text-muted sm:text-left sm:text-sm">
                    Showing {(data.pagination.page - 1) * data.pagination.limit + 1}-
                    {Math.min(data.pagination.page * data.pagination.limit, data.pagination.totalUsers)} of {data.pagination.totalUsers}
                  </p>
                  <nav className="flex items-center gap-1">
                    <PageButton disabled={data.pagination.page <= 1} onClick={() => setPage(data.pagination.page - 1)} aria-label="Previous page">
                      ←
                    </PageButton>
                    <div className="hidden gap-1 md:flex">
                      {buildPageList(data.pagination.totalPages, data.pagination.page).map((p, idx) =>
                        p === "…" ? (
                          <span key={`e${idx}`} className="flex h-8 w-8 items-center justify-center text-sm text-muted">…</span>
                        ) : (
                          <PageButton key={p} active={p === data.pagination.page} onClick={() => setPage(p)}>
                            {p}
                          </PageButton>
                        ),
                      )}
                    </div>
                    <PageButton disabled={data.pagination.page >= data.pagination.totalPages} onClick={() => setPage(data.pagination.page + 1)} aria-label="Next page">
                      →
                    </PageButton>
                  </nav>
                </div>
              )}
            </>
          )}
        </Panel>
      )}

      <Panel className="mt-8 p-6">
        <h2 className="text-base font-semibold text-foreground">Join the Leaderboard</h2>
        <p className="mt-1 mb-4 text-sm text-muted">Install the CLI and submit your usage data:</p>
        <div className="flex flex-col gap-2">
          <CommandSnippet command="tokens login" />
          <CommandSnippet command="tokens submit" />
        </div>
      </Panel>
    </>
  );
}

function buildPageList(total: number, current: number): (number | "…")[] {
  const delta = 2;
  const visible = new Set<number>([1, total]);
  for (let i = Math.max(2, current - delta); i <= Math.min(total - 1, current + delta); i++) visible.add(i);
  const sorted = Array.from(visible).sort((a, b) => a - b);
  const out: (number | "…")[] = [];
  let last = 0;
  for (const p of sorted) {
    if (last && p - last > 1) out.push("…");
    out.push(p);
    last = p;
  }
  return out;
}

function PageButton({
  children,
  active,
  disabled,
  onClick,
  ...rest
}: {
  children: React.ReactNode;
  active?: boolean;
  disabled?: boolean;
  onClick?: () => void;
} & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      disabled={disabled}
      onClick={onClick}
      className={`flex h-8 min-w-8 items-center justify-center rounded-md border px-2 text-[13px] transition disabled:cursor-default disabled:opacity-40 ${
        active ? "border-accent bg-accent text-accent-foreground" : "border-line text-muted hover:border-accent hover:text-foreground"
      }`}
      {...rest}
    >
      {children}
    </button>
  );
}
