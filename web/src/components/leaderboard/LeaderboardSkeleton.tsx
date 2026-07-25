import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { CONTAINER } from "@/components/layout/Container";

/**
 * The Suspense fallback for /leaderboard.
 *
 * It mirrors Leaderboard.tsx structurally rather than approximating it: same
 * container, same grid, same table primitives, same cell classes. The previous
 * version was a free-standing block with no container, so while data loaded the
 * board spanned the full viewport and then snapped back to 1200px once it
 * arrived — the wider the monitor, the more violent the jump.
 *
 * Bars sit inside a span sized to the line box the real text occupies, so a row
 * is the same height whether it holds a shimmer or a developer. Sizing the bar
 * itself would make every row a few pixels short and drift the whole table up.
 */

// Enough to cover a tall viewport. The real count depends on the period — Today
// is often short of a full page — so no fixed number matches every load; what
// matters is that nothing shifts within the fold.
const ROWS = 12;

function SkeletonRow() {
  return (
    <TableRow className="hover:bg-transparent">
      <TableCell className="w-10 py-3 pl-4 sm:w-12 sm:py-2 sm:pl-6">
        <span className="flex h-5 items-center">
          <Skeleton className="h-3.5 w-4" />
        </span>
      </TableCell>

      <TableCell className="min-w-0 py-3 sm:py-2">
        <div className="flex min-w-0 items-center gap-3">
          <Skeleton className="size-7 shrink-0 rounded-full" />
          <div className="flex min-w-0 flex-col">
            <span className="flex h-[17.5px] items-center">
              <Skeleton className="h-3.5 w-28" />
            </span>
            <span className="flex h-[15px] items-center">
              <Skeleton className="h-3 w-20" />
            </span>
          </div>
        </div>
      </TableCell>

      <TableCell className="py-3 pr-4 sm:hidden">
        <span className="flex h-5 items-center justify-end">
          <Skeleton className="h-3.5 w-16" />
        </span>
        <span className="flex h-4 items-center justify-end">
          <Skeleton className="h-3 w-12" />
        </span>
      </TableCell>

      <TableCell className="hidden w-44 sm:table-cell">
        <div className="flex flex-col items-end gap-1.5">
          <span className="flex h-5 items-center">
            <Skeleton className="h-3.5 w-16" />
          </span>
          <div className="h-0.5 w-full max-w-32 rounded-full bg-border" />
        </div>
      </TableCell>

      <TableCell className="hidden w-32 pr-6 sm:table-cell">
        <span className="flex h-5 items-center justify-end">
          <Skeleton className="h-3.5 w-14" />
        </span>
      </TableCell>
    </TableRow>
  );
}

export function LeaderboardSkeleton() {
  return (
    <div className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")}>
      {/* PageHeader. The title and description are known before the data is,
          but rendering the real strings here and again in the loaded page made
          the heading flicker on every period change, so both are bars. */}
      <header className="flex flex-col gap-1.5">
        <span className="flex h-8 items-center sm:h-9">
          <Skeleton className="h-6 w-44 sm:h-7" />
        </span>
        <span className="flex h-[22.75px] items-center">
          <Skeleton className="h-3.5 w-full max-w-[26rem]" />
        </span>
      </header>
      <Separator className="my-7" />

      <section className="grid grid-cols-2 gap-6 sm:grid-cols-4" aria-hidden="true">
        {Array.from({ length: 4 }, (_, i) => (
          <div key={i} className="flex flex-col gap-1.5">
            <span className="flex h-4 items-center">
              <Skeleton className="h-3 w-16" />
            </span>
            <span className="flex h-5 items-center sm:h-6">
              <Skeleton className="h-5 w-20 sm:h-6" />
            </span>
          </div>
        ))}
      </section>

      <div className="mt-8 flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center sm:gap-2">
        <div className="-mx-4 px-4 pb-1 sm:mx-0 sm:px-0 sm:pb-0">
          <Skeleton className="h-10 w-full max-w-[23rem] rounded-md sm:h-8 sm:w-[23rem]" />
        </div>

        <div className="flex items-center gap-2">
          <Skeleton className="h-10 w-32 rounded-md sm:h-8" />
          <Skeleton className="h-10 flex-1 rounded-md sm:ml-auto sm:h-8 sm:w-56 sm:flex-none" />
        </div>
      </div>

      <div className="mt-4 overflow-hidden rounded-lg border">
        <Table>
          <TableHeader>
            {/* Column headings are static, so they render as themselves — a
                shimmer where a permanent label goes reads as loading data that
                never was. */}
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-12 pl-4 sm:pl-6">#</TableHead>
              <TableHead>Developer</TableHead>
              <TableHead className="pr-4 text-right sm:hidden">Usage</TableHead>
              <TableHead className="hidden w-44 px-2 py-2 text-right sm:table-cell">
                Tokens
              </TableHead>
              <TableHead className="hidden w-32 pr-6 text-right sm:table-cell">
                Cost
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {Array.from({ length: ROWS }, (_, i) => (
              <SkeletonRow key={i} />
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
