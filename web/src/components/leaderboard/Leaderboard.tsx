"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Link from "next/link";
import { useRouter } from "nextjs-toploader/app";
import { usePathname, useSearchParams } from "next/navigation";
import { ArrowLeftRightIcon, SearchIcon } from "lucide-react";
import { useSettings } from "@/lib/useSettings";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { VerifiedBadge } from "@/components/ui/VerifiedBadge";
import { cn } from "@/lib/utils";
import { CONTAINER } from "@/components/layout/Container";
import { PageHeader } from "@/components/layout/PageHeader";
import { formatCurrency, formatNumber } from "@/lib/format";
import { resolveSortByParam } from "@/lib/leaderboard/constants";
import type {
  LeaderboardSortBy,
  LeaderboardTokenFormat,
} from "@/lib/leaderboard/constants";
import {
  matchesLeaderboardSearch,
  parseSearchDirectives,
} from "@/lib/leaderboard/searchDirectives";
import type { LeaderboardData, LeaderboardUser, Period } from "@/lib/leaderboard/types";
import { toLocalDateString } from "@/lib/leaderboard/dateRange";

interface SessionUser {
  id: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
}

interface LeaderboardProps {
  /** The whole board for this period, not a page of it. */
  initialData: LeaderboardData;
  /** A `client:`/`model:` query, which re-runs the aggregation server-side and
   *  therefore already narrowed what arrived here. Plain text is filtered
   *  locally and never reaches the server. */
  directiveSearch: string;
}

// All time leads because it is the standing everyone compares against; Today
// stays the landing selection, which the server resolves.
const PERIODS: ReadonlyArray<{ value: Period; label: string }> = [
  { value: "all", label: "All time" },
  { value: "today", label: "Today" },
  { value: "week", label: "Week" },
  { value: "month", label: "Month" },
  { value: "last-month", label: "Last month" },
];

function avatarFor(user: { username: string; avatarUrl: string | null }) {
  return user.avatarUrl || `https://github.com/${user.username}.png`;
}

function Stat({
  label,
  value,
  accent,
  className,
}: {
  label: string;
  value: string;
  accent?: boolean;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
        {label}
      </span>
      <span
        className={cn(
          "tabular text-xl leading-none sm:text-2xl",
          accent && "text-primary"
        )}
      >
        {value}
      </span>
    </div>
  );
}

function DeveloperRow({
  user,
  isSelf,
  max,
  sortBy,
  tokenFormat,
}: {
  user: LeaderboardUser;
  isSelf: boolean;
  max: number;
  sortBy: LeaderboardSortBy;
  tokenFormat: LeaderboardTokenFormat;
}) {
  const compact = tokenFormat === "compact";
  const primary = sortBy === "cost" ? user.totalCost : user.totalTokens;
  const share = max > 0 ? Math.max(primary / max, 0.006) : 0;

  return (
    <TableRow
      className={cn(
        "group relative",
        isSelf && "bg-primary/[0.06] hover:bg-primary/[0.09]"
      )}
    >
      <TableCell className="w-10 py-3 pl-4 sm:w-12 sm:py-2 sm:pl-6">
        <span
          className={cn(
            "tabular text-sm",
            user.rank === 1 && "font-semibold text-primary",
            user.rank > 1 && user.rank <= 3 && "font-semibold text-foreground",
            user.rank > 3 && "text-muted-foreground"
          )}
        >
          {user.rank}
        </span>
      </TableCell>

      <TableCell className="min-w-0 py-3 sm:py-2">
        {/* No viewport prefetch. App Router prefetches every link that scrolls
            into view, and a page of 50 rows therefore renders 50 profiles
            server-side — each one several database round trips — to serve a
            visitor who will open at most one. Hover still prefetches, which is
            where the intent actually is. */}
        <Link
          href={`/u/${user.username}`}
          prefetch={false}
          className="flex min-w-0 items-center gap-3"
        >
          <Avatar className="size-7 shrink-0">
            <AvatarImage src={avatarFor(user)} alt="" loading="lazy" />
            <AvatarFallback className="text-[10px]">
              {user.username.slice(0, 2).toUpperCase()}
            </AvatarFallback>
          </Avatar>
          <span className="flex min-w-0 flex-col leading-tight">
            <span className="flex items-center gap-1.5">
              <span className="truncate text-sm font-medium group-hover:underline">
                {user.displayName || user.username}
              </span>
              {user.verified && <VerifiedBadge size={13} />}
            </span>
            <span className="truncate text-xs text-muted-foreground">
              @{user.username}
            </span>
          </span>
        </Link>
      </TableCell>

      {/* Phones get one stacked cell; the split columns need the width.
          Always abbreviated here, regardless of the stored preference: the
          control that toggles it lives in the desktop-only header, so on a
          phone an exact 21,534,711,514 is both unreadable at this width and
          impossible to switch away from.

          Colour follows the same rule as the desktop columns: the metric the
          table is ranked by takes the accent, the other one recedes. Two
          weights of grey read as one flat block of numbers — which is the
          state this replaces. */}
      <TableCell className="py-3 pr-4 text-right sm:hidden">
        <span
          className={cn(
            "tabular block text-sm",
            sortBy === "tokens"
              ? "font-medium text-primary"
              : "text-muted-foreground"
          )}
        >
          {formatNumber(user.totalTokens, true)}
        </span>
        <span
          className={cn(
            "tabular block text-xs",
            sortBy === "cost"
              ? "font-medium text-primary"
              : "text-muted-foreground"
          )}
        >
          {formatCurrency(user.totalCost, true)}
        </span>
      </TableCell>

      <TableCell className="hidden w-44 sm:table-cell">
        <div className="flex flex-col items-end gap-1.5">
          <span
            className={cn(
              "tabular text-sm",
              sortBy === "tokens"
                ? "font-medium text-primary"
                : "text-muted-foreground"
            )}
          >
            {formatNumber(user.totalTokens, compact)}
          </span>
          {/* Share of the top row on this page — a shape you can scan without
              reading every number. */}
          <div className="h-0.5 w-full max-w-32 overflow-hidden rounded-full bg-border">
            <div
              className="h-full rounded-full bg-primary/50"
              style={{ width: `${share * 100}%` }}
            />
          </div>
        </div>
      </TableCell>

      <TableCell className="hidden w-32 pr-4 text-right sm:table-cell">
        <span
          className={cn(
            "tabular text-sm",
            sortBy === "cost"
              ? "font-medium text-primary"
              : "text-muted-foreground"
          )}
        >
          {formatCurrency(user.totalCost, compact)}
        </span>
      </TableCell>
    </TableRow>
  );
}

/**
 * A numeric column header that also switches abbreviated/exact formatting.
 *
 * The affordance is the point: the swap icon stays visible at rest (not only
 * on hover) because a hover-only hint is invisible on touch and easy to miss
 * on a pointer. The title spells out what the click will do, in the direction
 * it will do it.
 */
function FormatToggle({
  label,
  compact,
  onToggle,
}: {
  label: string;
  compact: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-label={
        compact ? `${label}: show exact numbers` : `${label}: abbreviate numbers`
      }
      title={compact ? "Show exact numbers" : "Abbreviate numbers"}
      className="flex h-full w-full items-center justify-end gap-1.5 px-2 py-2 transition-colors hover:text-foreground"
    >
      {label}
      <ArrowLeftRightIcon
        aria-hidden
        className="size-3 shrink-0 opacity-45 transition-opacity group-hover/head:opacity-100"
      />
    </button>
  );
}

/** Rows per page. The whole board is already here; this is a display choice. */
const PAGE_SIZE = 50;

/**
 * Order the board the way the SQL did, including the tie-break.
 *
 * The secondary column matters: without it two people on identical totals swap
 * places between renders, and the rank shown in the "Your position" card would
 * disagree with the rank on the same person's row in the table.
 */
function sortBoard(
  users: ReadonlyArray<LeaderboardUser>,
  sortBy: LeaderboardSortBy
): LeaderboardUser[] {
  const primary = (u: LeaderboardUser) =>
    sortBy === "cost" ? u.totalCost : u.totalTokens;
  const secondary = (u: LeaderboardUser) =>
    sortBy === "cost" ? u.totalTokens : u.totalCost;

  return [...users]
    .sort((a, b) => primary(b) - primary(a) || secondary(b) - secondary(a))
    .map((user, index) => ({ ...user, rank: index + 1 }));
}

export default function Leaderboard({
  initialData,
  directiveSearch,
}: LeaderboardProps) {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();

  const {
    leaderboardSortBy,
    setLeaderboardSort,
    leaderboardTokenFormat: tokenFormat,
    setLeaderboardTokenFormat,
    mounted,
  } = useSettings();

  // A `sortBy` in the URL wins, so a shared link opens on the ordering it was
  // shared with. Otherwise the stored preference — but only once mounted: the
  // server rendered the token ordering, and reaching for localStorage during
  // the first render would reorder the table underneath the reader.
  const sortByParam = resolveSortByParam(searchParams.get("sortBy"));
  const sortBy: LeaderboardSortBy =
    sortByParam ?? (mounted ? leaderboardSortBy : "tokens");

  const [search, setSearch] = useState(() => searchParams.get("search") ?? "");
  const [page, setPage] = useState(() =>
    Math.max(1, Number(searchParams.get("page")) || 1)
  );
  const [pendingPeriod, setPendingPeriod] = useState<Period | null>(null);

  /**
   * Who is reading, and nothing else.
   *
   * The page itself is one cached document for every reader, so identity
   * cannot come from the render — but it does not need to. The only thing
   * fetched here is *which* row is mine; every number in the rank card is then
   * read out of the same sorted array the table is drawn from. That is the
   * whole point: the card and the table cannot disagree, because there is only
   * one set of figures. Fetching the rank itself would reintroduce exactly the
   * drift this removes, and worse — the table would be an edge copy up to five
   * minutes old while the rank came back freshly computed.
   *
   * Same endpoint the header already uses, so a signed-in reader pays for it
   * once.
   */
  const [me, setMe] = useState<SessionUser | null>(null);
  useEffect(() => {
    let cancelled = false;
    fetch("/api/auth/session")
      .then((res) => (res.ok ? res.json() : null))
      .then((data) => {
        if (!cancelled) setMe(data?.user ?? null);
      })
      .catch(() => {
        // A failed session lookup is not a sign-out; it just means no card.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // The server resolves the period, so it is read from props rather than
  // mirrored into state. `pendingPeriod` only holds the optimistic selection
  // between the click and the new data arriving, and is cleared during render
  // once they agree — no effect, so no cascading render.
  const period = pendingPeriod ?? initialData.period;
  const pending = pendingPeriod != null && pendingPeriod !== initialData.period;
  if (pendingPeriod != null && pendingPeriod === initialData.period) {
    setPendingPeriod(null);
  }
  const searchRef = useRef<HTMLInputElement>(null);

  /** Navigate. Only the period and the date window change what the server has
   *  to compute, so only they come through here. */
  const pushQuery = useCallback(
    (next: Record<string, string | null>) => {
      const params = new URLSearchParams(searchParams.toString());
      for (const [key, value] of Object.entries(next)) {
        if (value === null || value === "") params.delete(key);
        else params.set(key, value);
      }
      const qs = params.toString();
      router.push(qs ? `${pathname}?${qs}` : pathname);
    },
    [pathname, router, searchParams]
  );

  /**
   * Record a view choice in the URL without asking the server for anything.
   *
   * Sorting, searching and paging are now array operations over rows already in
   * memory, so a navigation would fetch a document identical to the one on
   * screen. They stay in the address bar because these links get shared, and
   * `replaceState` is how you keep that without the round trip — it also keeps
   * the back button meaning "the previous page", not "the previous keystroke".
   */
  const replaceQuery = useCallback(
    (next: Record<string, string | null>) => {
      const params = new URLSearchParams(window.location.search);
      for (const [key, value] of Object.entries(next)) {
        if (value === null || value === "") params.delete(key);
        else params.set(key, value);
      }
      const qs = params.toString();
      window.history.replaceState(null, "", qs ? `${pathname}?${qs}` : pathname);
    },
    [pathname]
  );

  // The server can only resolve "today" in UTC, but daily rows are bucketed by
  // the submitter's local date, so it lands on the wrong day for anyone not on
  // UTC — and a signed-in viewer whose local day has rolled over gets no rank
  // at all, because the rank is a position within the period. Send our own date
  // up as soon as we have one. `replace` rather than `push` so the correction
  // does not sit in the back stack, and the equality check makes it idempotent
  // while still re-firing when the day rolls over under a long-lived tab.
  useEffect(() => {
    if (period !== "today") return;
    const now = new Date();
    const localDate = toLocalDateString(now);
    const fromParam = searchParams.get("from");
    if (fromParam === localDate) return;
    // No `from` means the server fell back to its own UTC date. When that
    // already agrees with ours there is nothing to correct, and navigating
    // purely to write the parameter down would cost a round trip for nothing —
    // which is most of the day even well away from UTC.
    if (fromParam === null && localDate === now.toISOString().slice(0, 10)) return;
    const params = new URLSearchParams(searchParams.toString());
    params.set("from", localDate);
    router.replace(`${pathname}?${params.toString()}`);
  }, [period, searchParams, pathname, router]);

  // "/" focuses search — the shortcut this audience reaches for by reflex.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey) return;
      const tag = (event.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      event.preventDefault();
      searchRef.current?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // One ordering, computed once. Everything below is a view of this array —
  // the table, the page count, and the viewer's own row — which is what makes
  // it impossible for the rank in the card to disagree with the rank on the
  // same person's row further down.
  const ranked = useMemo(
    () => sortBoard(initialData.users, sortBy),
    [initialData.users, sortBy]
  );

  // Only the plain-text part. A `client:`/`model:` directive was applied by the
  // server before these rows were sent, so re-applying it here would filter the
  // result of a filter.
  const appliedSearch = useMemo(
    () => parseSearchDirectives(search).text.trim(),
    [search]
  );

  const filtered = useMemo(
    () => ranked.filter((user) => matchesLeaderboardSearch(user, appliedSearch)),
    [ranked, appliedSearch]
  );

  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  // Typing a query that shortens the board past the current page would
  // otherwise leave the reader looking at an empty table with no way back.
  const currentPage = Math.min(page, totalPages);
  const users = useMemo(
    () => filtered.slice((currentPage - 1) * PAGE_SIZE, currentPage * PAGE_SIZE),
    [filtered, currentPage]
  );

  // The bar lengths compare against the whole board, not against whichever
  // fifty rows are on screen — otherwise page 2 redraws itself full-width and
  // reads as everyone suddenly being equal.
  const max = useMemo(() => {
    if (ranked.length === 0) return 0;
    return sortBy === "cost" ? ranked[0].totalCost : ranked[0].totalTokens;
  }, [ranked, sortBy]);

  const goToPage = useCallback(
    (next: number) => {
      setPage(next);
      replaceQuery({ page: next > 1 ? String(next) : null });
      window.scrollTo({ top: 0, behavior: "smooth" });
    },
    [replaceQuery]
  );

  const myRow = useMemo(
    () => (me ? (ranked.find((user) => user.userId === me.id) ?? null) : null),
    [ranked, me]
  );

  const { stats } = initialData;

  return (
    <div className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")}>
      <PageHeader
        title="Leaderboard"
        description="AI coding token usage, reported by the Tokens CLI."
      />

      {/* The rank card exists only when there is a rank to put in it. A signed
          out visitor has no standing to report, and a bare em dash reads as a
          number that failed to load rather than as "not applicable" — so the
          card goes, and the row re-flows to fill the width it left behind.
          Column count follows card count; on phones the odd card spans both
          columns so the grid never ends on a hole. */}
      <section
        className={cn(
          "grid grid-cols-2 gap-6",
          myRow ? "sm:grid-cols-4" : "sm:grid-cols-3"
        )}
        aria-label="Totals"
      >
        <Stat label="Tokens" value={formatNumber(stats.totalTokens, true)} />
        <Stat label="Cost" value={formatCurrency(stats.totalCost, true)} />
        <Stat
          label="Developers"
          value={formatNumber(stats.uniqueUsers, false)}
          className={myRow ? undefined : "col-span-2 sm:col-span-1"}
        />
        {/* The only figure here that is about the viewer, so it takes the
            accent — same signal as the highlighted self row below. */}
        {myRow && <Stat label="Your rank" value={`#${myRow.rank}`} accent />}
      </section>

      {/* Controls stack on phones and sit on one line from sm up. Touch
          targets stay at 40px on mobile; the desktop row tightens to 32px
          where the pointer is precise. */}
      <div className="mt-8 flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center sm:gap-2">
        {/* The five periods do not fit on a narrow screen, so the row scrolls
            horizontally rather than wrapping into a ragged block. */}
        <div className="-mx-4 overflow-x-auto px-4 pb-1 sm:mx-0 sm:overflow-visible sm:px-0 sm:pb-0 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          {/* Base UI's ToggleGroup is array-valued even in single-select mode. */}
          <ToggleGroup
            value={[period]}
            onValueChange={(value) => {
              const next = value[0] as Period | undefined;
              if (!next) return;
              setPendingPeriod(next);
              // `from` only ever means "the viewer's local today". Carry it
              // straight into a switch to Today — going via null would make
              // the effect above correct it in a second navigation — and drop
              // it everywhere else so it cannot linger into a period that
              // reads it differently.
              pushQuery({
                period: next,
                page: null,
                from: next === "today" ? toLocalDateString(new Date()) : null,
              });
            }}
            variant="outline"
            aria-label="Period"
            className="[&>*]:h-10 [&>*]:px-3.5 sm:[&>*]:h-8 sm:[&>*]:px-3"
          >
            {PERIODS.map((p) => (
              <ToggleGroupItem key={p.value} value={p.value}>
                {p.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>

        <div className="flex items-center gap-2">
          <ToggleGroup
            value={[sortBy]}
            onValueChange={(value) => {
              const next = value[0] as LeaderboardSortBy | undefined;
              if (!next) return;
              // Re-ordering rows already in memory. The preference is stored
              // for the next visit and written to the URL so the link carries
              // it, but nothing is fetched.
              setLeaderboardSort(next);
              setPage(1);
              replaceQuery({ sortBy: next, page: null });
            }}
            variant="outline"
            aria-label="Sort by"
            className="[&>*]:h-10 [&>*]:px-3.5 sm:[&>*]:h-8 sm:[&>*]:px-3"
          >
            <ToggleGroupItem value="tokens">Tokens</ToggleGroupItem>
            <ToggleGroupItem value="cost">Cost</ToggleGroupItem>
          </ToggleGroup>

          {/* Filters the rows already here, so results follow the keystrokes
              instead of a round trip. Submitting is kept as a no-op so Enter
              does not reload the page out from under the filter. */}
          <form
            className="relative flex-1 sm:ml-auto sm:flex-none"
            onSubmit={(event) => event.preventDefault()}
          >
            <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              ref={searchRef}
              value={search}
              onChange={(event) => {
                setSearch(event.target.value);
                setPage(1);
                replaceQuery({ search: event.target.value.trim() || null, page: null });
              }}
              placeholder="Search…"
              aria-label="Search developers"
              className="h-10 w-full pl-8 text-sm sm:h-8 sm:w-56"
            />
          </form>
        </div>
      </div>

      {/* Own standing, anchored directly under the period controls so it is
          visible without paginating to wherever the rank happens to fall. It
          follows the period/sort query like the table does, because the server
          recomputes it per period.

          Shown unconditionally, including when the same row is also visible in
          the table below. A block that appeared and disappeared as you paged
          would read as a glitch; a fixed anchor that is always in the same
          place is worth more than avoiding one duplicated row. */}
      {myRow && (
        <div
          className={cn("mt-4 transition-opacity", pending && "opacity-50")}
          aria-busy={pending}
        >
          <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Your position
          </span>
          <div className="mt-1.5 overflow-hidden rounded-lg border">
            <Table>
              <TableBody>
                <DeveloperRow
                  user={myRow}
                  isSelf
                  max={max || myRow.totalTokens}
                  sortBy={sortBy}
                  tokenFormat={tokenFormat}
                />
              </TableBody>
            </Table>
          </div>
        </div>
      )}

      <div
        className={cn(
          "mt-4 overflow-hidden rounded-lg border transition-opacity",
          pending && "opacity-50"
        )}
        aria-busy={pending}
      >
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-12 pl-4 sm:pl-6">#</TableHead>
              <TableHead>Developer</TableHead>
              <TableHead className="pr-4 text-right sm:hidden">Usage</TableHead>
              {/* Both numeric headers toggle abbreviated figures (1.2B) for
                  exact ones — a toggle contributed upstream by Fai Chou that
                  people rely on when comparing close totals. It was invisible:
                  a bare header that happened to be clickable, which nobody who
                  had not read the code would ever try. Each now carries a
                  permanently visible swap icon, and both drive the one
                  preference so the two columns cannot disagree. */}
              <TableHead className="hidden w-44 p-0 text-right sm:table-cell">
                <FormatToggle
                  label="Tokens"
                  compact={tokenFormat === "compact"}
                  onToggle={() =>
                    setLeaderboardTokenFormat(
                      tokenFormat === "compact" ? "full" : "compact"
                    )
                  }
                />
              </TableHead>
              <TableHead className="hidden w-32 p-0 pr-4 text-right sm:table-cell">
                <FormatToggle
                  label="Cost"
                  compact={tokenFormat === "compact"}
                  onToggle={() =>
                    setLeaderboardTokenFormat(
                      tokenFormat === "compact" ? "full" : "compact"
                    )
                  }
                />
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {users.map((user) => (
              <DeveloperRow
                key={user.userId}
                user={user}
                isSelf={me?.id === user.userId}
                max={max}
                sortBy={sortBy}
                tokenFormat={tokenFormat}
              />
            ))}
          </TableBody>
        </Table>

        {users.length === 0 && (
          <Empty className="border-0">
            <EmptyHeader>
              {/* A search that matched nothing is a different situation from a
                  period nobody submitted in, and telling a searcher "no usage
                  was submitted" reads as the leaderboard being broken. Keyed on
                  the applied query rather than the input, which can have been
                  typed past what these results answer. */}
              <EmptyTitle>
                {appliedSearch || directiveSearch
                  ? "No developers found"
                  : "Nothing recorded"}
              </EmptyTitle>
              {/* Three different situations, and saying "no usage was
                  submitted" for any of the other two reads as the leaderboard
                  being broken. The directive is named separately because the
                  server narrowed the board before it arrived, so a reader who
                  clears only the text box would still see nothing. */}
              <EmptyDescription>
                {appliedSearch
                  ? `No developer matches "${appliedSearch}" for this period.`
                  : directiveSearch
                    ? `No developer matches "${directiveSearch}" for this period.`
                    : "No usage was submitted for this period."}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </div>

      {/* Paging is a slice of an array that is already here, so it costs a
          render and nothing else. The scroll goes back to the top because a
          page that changes under a reader halfway down reads as content
          shifting rather than as having moved. */}
      {totalPages > 1 && (
        <nav className="mt-6 flex items-center justify-between" aria-label="Pagination">
          <Button
            variant="outline"
            disabled={currentPage <= 1}
            className="h-10 sm:h-8"
            onClick={() => goToPage(currentPage - 1)}
          >
            Previous
          </Button>
          <span className="tabular text-xs text-muted-foreground">
            {currentPage} of {totalPages}
          </span>
          <Button
            variant="outline"
            disabled={currentPage >= totalPages}
            className="h-10 sm:h-8"
            onClick={() => goToPage(currentPage + 1)}
          >
            Next
          </Button>
        </nav>
      )}
    </div>
  );
}
