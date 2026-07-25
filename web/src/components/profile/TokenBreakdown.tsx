"use client";

import { useId, type CSSProperties } from "react";
import { formatNumber } from "@/lib/utils";
import type { ProfileStatsData } from "./types";
import { tw } from "@/lib/tw";

export interface TokenBreakdownProps {
  stats: ProfileStatsData;
  className?: string;
}

const TOKEN_MIX_COLORS = {
  input: "var(--token-input)",
  output: "var(--token-output)",
  cacheRead: "var(--token-cache-read)",
  cacheWrite: "var(--token-cache-write)",
  reasoning: "var(--token-reasoning)",
} as const;

const BreakdownPanel = tw(
  "section",
  "overflow-hidden rounded-xl border bg-card text-foreground"
);

const BreakdownHeader = tw("header", "border-b px-4 py-3.5");

const BreakdownHeading = tw(
  "h2",
  "m-0 text-[0.9375rem] font-semibold tracking-tight text-foreground"
);

const BreakdownDescription = tw(
  "p",
  "m-0 mt-1 max-w-[46ch] text-[0.8125rem] leading-snug text-muted-foreground"
);

const BreakdownBody = tw("div", "px-4 pb-4 pt-3.5");

const SegmentedBar = tw(
  "div",
  "flex h-2.5 w-full overflow-hidden rounded border bg-muted"
);

// flex-grow and the fill are per-token runtime values, so they stay inline.
// A zero-weight segment collapses entirely; anything above it keeps 3px so a
// token that was used at all is still visible in the bar.
const Segment = ({
  $color,
  $weight,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & {
  $color: string;
  $weight: number;
}) => (
  <span
    {...props}
    style={{
      minWidth: $weight > 0 ? "3px" : 0,
      flex: `${$weight} 1 0`,
      background: $color,
      ...style,
    }}
  />
);

const BreakdownList = tw(
  "dl",
  "m-0 mt-3.5 grid grid-cols-[repeat(auto-fit,minmax(7.5rem,1fr))] gap-3"
);

// --segment-color is set per item by the caller.
const BreakdownItem = tw(
  "div",
  "min-w-0 border-l-2 border-l-[var(--segment-color)] pl-2"
);

const LabelRow = tw("div", "flex min-w-0 items-center gap-1.5");

const Marker = ({
  $color,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & { $color: string }) => (
  <span
    {...props}
    style={{ background: $color, ...style }}
    className="size-2 flex-none rounded-sm shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--foreground)_10%,transparent)]"
  />
);

const Label = tw(
  "dt",
  "overflow-hidden text-ellipsis whitespace-nowrap text-xs leading-tight text-muted-foreground"
);

const ValueRow = tw(
  "dd",
  "m-0 mt-1 flex flex-wrap items-baseline gap-1 text-[0.9375rem] font-semibold leading-tight text-foreground [font-variant-numeric:tabular-nums]"
);

const Percentage = tw(
  "span",
  "text-[0.6875rem] font-medium text-muted-foreground"
);

function finiteNonnegative(value: number | undefined): number {
  return Number.isFinite(value) ? Math.max(0, value ?? 0) : 0;
}

export function TokenBreakdown({ stats, className }: TokenBreakdownProps) {
  const headingId = useId();
  const descriptionId = useId();
  const tokenTypes = [
    {
      label: "Input",
      value: finiteNonnegative(stats.inputTokens),
      color: TOKEN_MIX_COLORS.input,
    },
    {
      label: "Output",
      value: finiteNonnegative(stats.outputTokens),
      color: TOKEN_MIX_COLORS.output,
    },
    {
      label: "Cache read",
      value: finiteNonnegative(stats.cacheReadTokens),
      color: TOKEN_MIX_COLORS.cacheRead,
    },
    {
      label: "Cache write",
      value: finiteNonnegative(stats.cacheWriteTokens),
      color: TOKEN_MIX_COLORS.cacheWrite,
    },
    ...(finiteNonnegative(stats.reasoningTokens) > 0
      ? [
          {
            label: "Reasoning",
            value: finiteNonnegative(stats.reasoningTokens),
            color: TOKEN_MIX_COLORS.reasoning,
          },
        ]
      : []),
  ];
  const breakdownTotal = tokenTypes.reduce(
    (sum, type) => Math.min(Number.MAX_VALUE, sum + type.value),
    0,
  );
  const describedBreakdown = tokenTypes
    .map((type) => `${type.label} ${formatNumber(type.value)}`)
    .join(", ");

  return (
    <BreakdownPanel
      className={className}
      aria-labelledby={headingId}
      aria-describedby={descriptionId}
    >
      <BreakdownHeader>
        <BreakdownHeading id={headingId}>Token mix</BreakdownHeading>
        <BreakdownDescription id={descriptionId}>
          Distribution across input, output, cache, and reasoning tokens.
        </BreakdownDescription>
      </BreakdownHeader>

      <BreakdownBody>
        <SegmentedBar
          role="img"
          aria-label={`Token distribution: ${describedBreakdown}`}
        >
          {tokenTypes
            .filter((type) => type.value > 0)
            .map((type) => (
              <Segment
                key={type.label}
                $color={type.color}
                $weight={type.value}
                aria-hidden="true"
                title={`${type.label}: ${formatNumber(type.value)}`}
              />
            ))}
        </SegmentedBar>

        <BreakdownList>
          {tokenTypes.map((type) => {
            const percentage =
              Number.isFinite(type.value) &&
              Number.isFinite(breakdownTotal) &&
              breakdownTotal > 0
                ? (type.value / breakdownTotal) * 100
                : 0;

            return (
              <BreakdownItem
                key={type.label}
                style={{ "--segment-color": type.color } as CSSProperties}
              >
                <LabelRow>
                  <Marker $color={type.color} aria-hidden="true" />
                  <Label>{type.label}</Label>
                </LabelRow>
                <ValueRow>
                  {formatNumber(type.value)}
                  <Percentage>{percentage.toFixed(1)}%</Percentage>
                </ValueRow>
              </BreakdownItem>
            );
          })}
        </BreakdownList>
      </BreakdownBody>
    </BreakdownPanel>
  );
}
