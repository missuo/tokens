"use client";

import { cn } from "@/lib/utils";
import { tw } from "@/lib/tw";

/**
 * The profile's list tables.
 *
 * One layout in two shapes: a real table from 640px up, and a card grid below
 * it, where each cell prints its own column heading from `data-label` through
 * a ::before. That is why the header row is hidden rather than removed on
 * small screens — the labels still have to come from somewhere, and repeating
 * them in the markup would put them in the accessibility tree twice.
 */

export const ListCard = tw(
  "div",
  "overflow-hidden border-y border-border bg-transparent text-foreground"
);

export const ListTable = tw(
  "table",
  "w-full table-fixed border-collapse text-[0.8125rem] [font-variant-numeric:tabular-nums] max-[639px]:block"
);

// Visually hidden, still announced. Written out at each breakpoint rather
// than composed from a shared string: Tailwind scans source text, so a class
// name that only exists after a join() is a class name it never emits.
export const ListCaption = tw(
  "caption",
  "absolute m-[-1px] h-px w-px overflow-hidden whitespace-nowrap border-0 p-0 [clip:rect(0,0,0,0)]"
);

export const ListHead = tw(
  "thead",
  "border-b border-border bg-transparent max-[639px]:absolute max-[639px]:m-[-1px] max-[639px]:h-px max-[639px]:w-px max-[639px]:overflow-hidden max-[639px]:whitespace-nowrap max-[639px]:border-0 max-[639px]:p-0 max-[639px]:[clip:rect(0,0,0,0)]"
);

export const ListBody = tw("tbody", "max-[639px]:block");

export const ListRow = tw(
  "tr",
  "border-t border-border first:border-t-0 max-[639px]:grid max-[639px]:grid-cols-2 max-[639px]:gap-x-4 max-[639px]:gap-y-2.5 max-[639px]:p-3 min-[390px]:max-[639px]:grid-cols-3"
);

interface CellProps {
  $align?: "left" | "right";
  $width?: string;
}

const CELL_BASE = "px-3 py-2.5 align-middle md:px-4";

export function ListHeaderCell({
  $align,
  $width,
  className,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"th"> & CellProps) {
  return (
    <th
      {...props}
      style={{ width: $width ?? "auto", ...style }}
      className={cn(
        CELL_BASE,
        "whitespace-nowrap text-xs font-medium text-muted-foreground",
        $align === "right" ? "text-right" : "text-left",
        className
      )}
    />
  );
}

export const ListPrimaryCell = tw(
  "th",
  "px-3 py-2.5 text-left align-middle font-medium text-foreground md:px-4 max-[639px]:col-span-full max-[639px]:block max-[639px]:min-w-0 max-[639px]:px-0 max-[639px]:pb-0.5 max-[639px]:pt-0"
);

export function ListCell({
  $align,
  $width,
  className,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"td"> & CellProps) {
  return (
    <td
      {...props}
      style={{ width: $width ?? "auto", ...style }}
      className={cn(
        CELL_BASE,
        "text-foreground",
        $align === "right" ? "text-right" : "text-left",
        // Below 640px the cell becomes its own labelled block; the label is
        // the column heading, carried on data-label.
        "max-[639px]:flex max-[639px]:min-w-0 max-[639px]:flex-col max-[639px]:gap-0.5 max-[639px]:p-0 max-[639px]:text-left",
        "max-[639px]:before:text-[0.6875rem] max-[639px]:before:font-medium max-[639px]:before:leading-tight max-[639px]:before:text-muted-foreground max-[639px]:before:content-[attr(data-label)]",
        className
      )}
    />
  );
}

export function NumericValue({
  $accent,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & { $accent?: boolean }) {
  return (
    <span
      {...props}
      className={cn(
        "text-foreground [font-variant-numeric:tabular-nums]",
        $accent && "font-semibold",
        className
      )}
    />
  );
}
