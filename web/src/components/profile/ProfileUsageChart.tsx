"use client";

import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type KeyboardEvent,
  type PointerEvent,
} from "react";
import { SOURCE_DISPLAY_NAMES } from "@/lib/constants";
import type { DailyContribution } from "@/lib/types";
import { formatCurrency, formatDate, formatNumber } from "@/lib/utils";
import {
  ALL_USAGE_PROVIDERS,
  MAX_LEGEND_MODELS,
  aggregateDailyUsage,
  buildUsageChartData,
  getActiveTooltipRows,
  getUsageProviderTotals,
  providerColor,
  reverseUsageChartData,
  selectLegendModels,
  toTrailingAverage,
  type UsageChartSeries,
  type UsageMetric,
  type UsageProviderFilter,
  type UsageProviderId,
  type UsageTooltipRow,
  type UsageView,
} from "./usageChartData";
import {
  createNonCrossingStackGeometry,
  pointToChartPercent,
  type CubicValueBoundary,
} from "./usageChartGeometry";
import { tw } from "@/lib/tw";
import { cn } from "@/lib/utils";

export interface ProfileUsageChartProps {
  contributions: DailyContribution[];
  initialMetric?: UsageMetric;
  description?: string;
  averageWindowDays?: number;
  rangeStart?: string | null;
  rangeEnd?: string | null;
}

const VIEWBOX_WIDTH = 848;
const VIEWBOX_HEIGHT = 256;
const PLOT_LEFT = 0;
const PLOT_RIGHT = 0;
const PLOT_TOP = 8;
const PLOT_BOTTOM = 30;
const PLOT_WIDTH = VIEWBOX_WIDTH - PLOT_LEFT - PLOT_RIGHT;
const PLOT_HEIGHT = VIEWBOX_HEIGHT - PLOT_TOP - PLOT_BOTTOM;
const GRID_STEPS = 4;
const MAX_TOOLTIP_MODELS = 8;
const MAX_TOOLTIP_PROVIDERS = 3;
const TOOLTIP_WIDTH = 320;
const TOOLTIP_GAP = 12;
const TOOLTIP_EDGE = 8;
const TOOLTIP_VIEWPORT_EDGE = 16;
const TOOLTIP_MAX_HEIGHT = 416;
const NEWEST_FIRST_STORAGE_KEY = "tokens:usage-newest-first";

const subscribeUsageChartMounted = () => () => {};

type InteractionMode = "idle" | "hover" | "committed";

interface ChartLayer {
  series: UsageChartSeries;
  areaPath: string;
  linePath: string;
  upperValues: number[];
}

interface ChartStack {
  layers: ChartLayer[];
  maximum: number;
}

interface ProviderCostRow {
  provider: UsageProviderId;
  label: string;
  color: string;
  value: number;
}

function xForIndex(index: number, pointCount: number): number {
  if (pointCount <= 1) return PLOT_LEFT + PLOT_WIDTH / 2;
  return PLOT_LEFT + (index / (pointCount - 1)) * PLOT_WIDTH;
}

function yForValue(value: number, maximum: number): number {
  const safeMaximum = maximum > 0 ? maximum : 1;
  const finiteValue = Number.isFinite(value) ? Math.max(0, value) : 0;
  return PLOT_TOP + PLOT_HEIGHT - (finiteValue / safeMaximum) * PLOT_HEIGHT;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, value));
}

function curvePath(boundary: CubicValueBoundary, maximum: number): string {
  const pointCount = boundary.values.length;
  if (pointCount === 0) return "";
  if (pointCount === 1) {
    const x = xForIndex(0, 1);
    const y = yForValue(boundary.values[0] ?? 0, maximum);
    return `M ${x - 4} ${y} L ${x + 4} ${y}`;
  }
  return [
    `M ${xForIndex(0, pointCount)} ${yForValue(boundary.values[0] ?? 0, maximum)}`,
    ...boundary.segments.map((segment) => {
      const fromX = xForIndex(segment.index, pointCount);
      const toX = xForIndex(segment.index + 1, pointCount);
      const third = (toX - fromX) / 3;
      return `C ${fromX + third} ${yForValue(segment.control1, maximum)} ${toX - third} ${yForValue(segment.control2, maximum)} ${toX} ${yForValue(segment.to, maximum)}`;
    }),
  ].join(" ");
}

function stackedAreaPath(
  lower: CubicValueBoundary,
  upper: CubicValueBoundary,
  maximum: number,
): string {
  const pointCount = upper.values.length;
  if (pointCount === 0) return "";
  if (pointCount === 1) {
    const x = xForIndex(0, 1);
    return [
      `M ${x - 4} ${yForValue(lower.values[0] ?? 0, maximum)}`,
      `L ${x - 4} ${yForValue(upper.values[0] ?? 0, maximum)}`,
      `L ${x + 4} ${yForValue(upper.values[0] ?? 0, maximum)}`,
      `L ${x + 4} ${yForValue(lower.values[0] ?? 0, maximum)}`,
      "Z",
    ].join(" ");
  }

  return [
    `M ${xForIndex(0, pointCount)} ${yForValue(upper.values[0] ?? 0, maximum)}`,
    ...upper.segments.map((segment) => {
      const fromX = xForIndex(segment.index, pointCount);
      const toX = xForIndex(segment.index + 1, pointCount);
      const third = (toX - fromX) / 3;
      return `C ${fromX + third} ${yForValue(segment.control1, maximum)} ${toX - third} ${yForValue(segment.control2, maximum)} ${toX} ${yForValue(segment.to, maximum)}`;
    }),
    `L ${xForIndex(pointCount - 1, pointCount)} ${yForValue(lower.values.at(-1) ?? 0, maximum)}`,
    ...[...lower.segments].reverse().map((segment) => {
      const fromX = xForIndex(segment.index, pointCount);
      const toX = xForIndex(segment.index + 1, pointCount);
      const third = (toX - fromX) / 3;
      return `C ${toX - third} ${yForValue(segment.control2, maximum)} ${fromX + third} ${yForValue(segment.control1, maximum)} ${fromX} ${yForValue(segment.from, maximum)}`;
    }),
    "Z",
  ].join(" ");
}

function createChartStack(
  series: readonly UsageChartSeries[],
  pointCount: number,
  baselineMaximum: number,
): ChartStack {
  const geometry = createNonCrossingStackGeometry(
    series.map(({ values }) => values),
    pointCount,
  );
  const maximum = Math.max(baselineMaximum, geometry.maximum);
  const layers = series.map((item, index) => {
    const layerGeometry = geometry.layers[index];
    if (!layerGeometry) {
      return {
        series: item,
        areaPath: "",
        linePath: "",
        upperValues: [],
      };
    }
    return {
      series: item,
      areaPath: stackedAreaPath(
        layerGeometry.lower,
        layerGeometry.upper,
        maximum,
      ),
      linePath: curvePath(layerGeometry.upper, maximum),
      upperValues: layerGeometry.upper.values,
    };
  });

  return { layers, maximum };
}

function providerName(provider: UsageProviderId): string {
  if (provider === "unattributed") return "Unattributed";
  return SOURCE_DISPLAY_NAMES[provider] ?? provider;
}

function formatMetric(value: number, metric: UsageMetric): string {
  return metric === "tokens" ? formatNumber(value) : formatCurrency(value);
}

function metricLabel(metric: UsageMetric): string {
  return metric === "tokens" ? "Tokens" : "Cost";
}

function viewLabel(view: UsageView, averageWindowDays: number): string {
  return view === "average" ? `${averageWindowDays}d average` : "Daily";
}

function tooltipLeft(activeOffset: number, plotWidth: number): number {
  if (!(plotWidth > 0)) return TOOLTIP_EDGE;

  const width = Math.min(
    TOOLTIP_WIDTH,
    Math.max(0, plotWidth - TOOLTIP_EDGE * 2),
  );
  const activePixel = (activeOffset / 100) * plotWidth;
  const right = activePixel + TOOLTIP_GAP;
  const left = activePixel - TOOLTIP_GAP - width;
  const rightFits = right + width <= plotWidth - TOOLTIP_EDGE;
  const leftFits = left >= TOOLTIP_EDGE;
  const preferred = rightFits
    ? right
    : leftFits
      ? left
      : plotWidth - activePixel >= activePixel
        ? right
        : left;

  return clamp(
    preferred,
    TOOLTIP_EDGE,
    Math.max(TOOLTIP_EDGE, plotWidth - TOOLTIP_EDGE - width),
  );
}

function tooltipLabels(rows: readonly UsageTooltipRow[]): Map<string, string> {
  const counts = new Map<string, number>();
  for (const { series } of rows) {
    counts.set(series.label, (counts.get(series.label) ?? 0) + 1);
  }

  return new Map(
    rows.map(({ series }) => [
      series.id,
      (counts.get(series.label) ?? 0) > 1
        ? `${series.label} · ${series.providerLabel}`
        : series.label,
    ]),
  );
}

function getProviderCostRows(
  days: ReturnType<typeof aggregateDailyUsage>,
  providerTotals: ReturnType<typeof getUsageProviderTotals>,
  providerFilter: UsageProviderFilter,
  activeIndex: number,
  view: UsageView,
  averageWindowDays: number,
): ProviderCostRow[] {
  return providerTotals
    .filter(
      ({ provider }) =>
        providerFilter === ALL_USAGE_PROVIDERS || provider === providerFilter,
    )
    .map(({ provider }) => {
      const rawValues = days.map(
        (day) =>
          day.providers.find((item) => item.provider === provider)?.cost ?? 0,
      );
      const values =
        view === "average"
          ? toTrailingAverage(rawValues, averageWindowDays)
          : rawValues;
      return {
        provider,
        label: providerName(provider),
        color: providerColor(provider),
        value: values[activeIndex] ?? 0,
      };
    })
    .filter(({ value }) => value >= 0.005)
    .sort(
      (left, right) =>
        right.value - left.value || left.label.localeCompare(right.label),
    );
}

// ============================================================================
// Chart chrome
// ============================================================================
//
// The panel is the container, so everything reflows on the panel's own width
// (@[34rem]) rather than the viewport's — this chart is embedded at several
// widths and the viewport tells it nothing useful.
//
// Series colours arrive at runtime, so they ride in as a --c custom property.
// Light mode darkens them (color-mix with #000) because the palette's colours
// are tuned for a dark canvas and wash out on a light one; dark mode uses them
// as given. That is a class pair Tailwind can see, unlike an interpolated one.

const Section = tw(
  "section",
  "@container min-w-0 overflow-visible rounded-xl border bg-card text-foreground"
);

const Header = tw(
  "div",
  "flex items-start justify-between gap-4 border-b px-4 pb-3.5 pt-4 @[34rem]:flex-row @max-[34rem]:flex-col @max-[34rem]:items-stretch @max-[34rem]:gap-3"
);

const HeadingGroup = tw("div", "min-w-0");

const Heading = tw(
  "h2",
  "m-0 text-[0.9375rem] font-semibold tracking-tight text-foreground"
);

const Description = tw(
  "p",
  "m-0 mt-1 max-w-[46ch] text-[0.8125rem] leading-normal text-muted-foreground"
);

const Total = tw(
  "div",
  "flex-none text-right [font-variant-numeric:tabular-nums] @max-[34rem]:flex @max-[34rem]:items-baseline @max-[34rem]:justify-between @max-[34rem]:text-left"
);

const TotalLabel = tw("div", "text-[0.6875rem] text-muted-foreground");
const TotalValue = tw("div", "mt-0.5 text-base font-semibold text-foreground");

const Controls = tw(
  "div",
  "flex flex-wrap items-center justify-between gap-2 px-4 py-2.5"
);

const ControlCluster = tw("div", "flex flex-wrap items-center gap-2");

const MetricControl = tw(
  "div",
  "inline-flex items-center rounded-lg border bg-muted p-0.5"
);

const MetricButton = ({
  $active,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"button"> & { $active: boolean }) => (
  <button
    {...props}
    className={cn(
      "relative min-h-7 cursor-pointer rounded-md border-0 px-2.5 py-1 text-xs font-medium focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring pointer-coarse:min-h-11",
      $active
        ? "bg-primary/10 text-primary"
        : "bg-transparent text-muted-foreground",
      className
    )}
  />
);

// The ::after is the dropdown chevron — two borders rotated 45deg.
const SelectControl = tw(
  "label",
  "relative inline-flex min-h-8 items-center gap-1.5 rounded-lg border bg-muted pl-2.5 text-xs text-muted-foreground hover:border-muted-foreground/30 focus-within:outline-2 focus-within:outline-offset-1 focus-within:outline-ring pointer-coarse:min-h-11 " +
    "after:pointer-events-none after:absolute after:right-2.5 after:size-1.5 after:-translate-y-0.5 after:rotate-45 after:border-b after:border-r after:border-muted-foreground after:content-['']"
);

const SelectCaption = tw("span", "flex-none");

const NewestFirstControl = tw(
  "label",
  "inline-flex min-w-0 cursor-pointer items-center gap-1.5 whitespace-nowrap text-xs text-muted-foreground focus-within:text-foreground pointer-coarse:min-h-11 [&_input]:m-0 [&_input]:size-[0.8125rem] [&_input]:cursor-pointer [&_input]:accent-[var(--ring)]"
);

const CompactSelect = tw(
  "select",
  "h-full min-w-0 max-w-44 cursor-pointer appearance-none border-0 bg-transparent py-1 pl-0 pr-7 font-medium text-foreground outline-0"
);

const PlotRegion = tw(
  "div",
  "min-w-0 px-4 pt-2 @max-[34rem]:px-3"
);

const InteractivePlot = tw(
  "div",
  "relative h-64 min-w-0 cursor-crosshair overflow-visible [touch-action:pan-y] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring @max-[34rem]:h-56"
);

const ChartSvg = tw(
  "svg",
  "absolute inset-0 block h-full w-full min-w-0 overflow-visible"
);

const GridLine = tw(
  "line",
  "[stroke:color-mix(in_srgb,var(--foreground)_8%,transparent)] [stroke-width:1] [vector-effect:non-scaling-stroke]"
);

const LayerArea = ({
  $color,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"path"> & { $color: string }) => (
  <path
    {...props}
    style={{ "--c": $color, ...style } as React.CSSProperties}
    className="fill-[color-mix(in_srgb,var(--c)_55%,#000)] [fill-opacity:0.4] [stroke:none] dark:fill-[var(--c)]"
  />
);

const LayerLine = ({
  $color,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"path"> & { $color: string }) => (
  <path
    {...props}
    style={{ "--c": $color, ...style } as React.CSSProperties}
    className="fill-none stroke-[color-mix(in_srgb,var(--c)_55%,#000)] [stroke-linecap:round] [stroke-linejoin:round] [stroke-opacity:1] [stroke-width:1] [vector-effect:non-scaling-stroke] dark:stroke-[var(--c)]"
  />
);

const ActiveRule = tw(
  "line",
  "stroke-muted-foreground/30 [stroke-width:1] [vector-effect:non-scaling-stroke]"
);

const ActivePoint = ({
  $color,
  $left,
  $top,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & {
  $color: string;
  $left: number;
  $top: number;
}) => (
  <span
    {...props}
    style={
      {
        "--c": $color,
        top: `${$top}%`,
        left: `${$left}%`,
        ...style,
      } as React.CSSProperties
    }
    className="pointer-events-none absolute z-[2] size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-[color-mix(in_srgb,var(--c)_55%,#000)] bg-card dark:border-[var(--c)]"
  />
);

// Below 34rem only the end date is kept — two dates do not fit, and the later
// one is the one being read.
const DateRange = tw(
  "div",
  "pointer-events-none absolute inset-x-0 bottom-1 flex justify-between gap-4 text-[0.6875rem] text-muted-foreground [font-variant-numeric:tabular-nums] @max-[34rem]:justify-end @max-[34rem]:[&_span:first-child:not(:last-child)]:hidden"
);

const EmptyState = tw(
  "div",
  "grid min-h-64 place-items-center text-[0.8125rem] text-muted-foreground @max-[34rem]:min-h-56"
);

const Legend = tw(
  "ul",
  "m-0 flex list-none flex-wrap items-center gap-x-4 gap-y-1.5 px-4 pb-3.5 pt-2.5 text-muted-foreground"
);

const LegendItem = tw(
  "li",
  "inline-flex min-w-0 items-center gap-1.5 text-xs"
);

const Swatch = ({
  $color,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & { $color: string }) => (
  <span
    {...props}
    style={{ "--c": $color, ...style } as React.CSSProperties}
    className="size-2 flex-none rounded-full bg-[color-mix(in_srgb,var(--c)_55%,#000)] dark:bg-[var(--c)]"
  />
);

// Four background layers: two solid-colour wedges pinned to the scroll box
// (background-attachment: local) and two radial shadows pinned to the viewport
// of it (scroll). Together they fade in at whichever end still has content —
// the classic scroll-shadow trick, which has no Tailwind spelling.
const SCROLL_SHADOW: React.CSSProperties = {
  background: [
    "linear-gradient(var(--muted) 30%, transparent) center top",
    "linear-gradient(transparent, var(--muted) 70%) center bottom",
    "radial-gradient(farthest-side at 50% 0, rgb(0 0 0 / 0.24), transparent) center top",
    "radial-gradient(farthest-side at 50% 100%, rgb(0 0 0 / 0.3), transparent) center bottom",
    "var(--muted)",
  ].join(", "),
  backgroundAttachment: "local, local, scroll, scroll, scroll",
  backgroundRepeat: "no-repeat",
  backgroundSize: "100% 1rem, 100% 1rem, 100% 0.5rem, 100% 0.5rem, 100% 100%",
};

const TooltipSurface = ({
  $left,
  $maxHeight,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & {
  $left: number;
  $maxHeight: number;
}) => (
  <div
    {...props}
    style={{
      ...SCROLL_SHADOW,
      left: `${$left}px`,
      maxHeight: `${$maxHeight}px`,
      ...style,
    }}
    className={cn(
      "pointer-events-auto absolute top-2 z-[5] box-border w-[min(20rem,calc(100%-1rem))] overflow-y-auto overflow-x-hidden rounded-[0.625rem] border border-muted-foreground/30 p-2.5 text-foreground shadow-[0_18px_48px_rgb(0_0_0/0.34)] [overscroll-behavior:contain]",
      "[scrollbar-color:color-mix(in_srgb,var(--primary)_65%,transparent)_transparent] [scrollbar-width:thin] [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-[color-mix(in_srgb,var(--primary)_65%,transparent)] [&::-webkit-scrollbar]:w-1.5",
      // A hover tooltip is useless on touch and there is no room for it on a
      // narrow panel; PinnedBreakdown takes over in both cases.
      "@max-[34rem]:hidden pointer-coarse:hidden"
    )}
  />
);

const BreakdownHeader = tw(
  "div",
  "mb-1.5 flex items-baseline justify-between gap-4 text-[0.8125rem]"
);

const BreakdownDate = tw(
  "span",
  "font-semibold text-foreground [font-variant-numeric:tabular-nums]"
);

const BreakdownMode = tw("span", "text-muted-foreground");
const BreakdownList = tw("ul", "m-0 grid list-none gap-1 p-0");

const BreakdownRow = tw(
  "li",
  "grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-1.5 text-[0.8125rem] leading-[1.125rem] text-muted-foreground"
);

const BreakdownName = tw(
  "span",
  "overflow-hidden text-ellipsis whitespace-nowrap"
);

const BreakdownValue = tw(
  "span",
  "text-foreground [font-variant-numeric:tabular-nums]"
);

const MoreRow = tw(
  "div",
  "mt-1 flex justify-between gap-4 text-[0.8125rem] text-muted-foreground [font-variant-numeric:tabular-nums]"
);

const CostSection = tw(
  "div",
  "mt-2 grid gap-1 border-t border-muted-foreground/30 pt-2"
);

const CostHeading = tw("div", "text-[0.6875rem] text-muted-foreground");

const BreakdownTotal = ({
  $sticky,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & { $sticky: boolean }) => (
  <div
    {...props}
    className={cn(
      "mt-2 flex justify-between gap-4 border-t border-muted-foreground/30 pt-2 text-[0.8125rem] font-semibold text-foreground [font-variant-numeric:tabular-nums]",
      // When the list scrolls, the total pins to the bottom of the box and
      // bleeds to its edges so nothing shows through beneath it.
      $sticky &&
        "sticky bottom-[-0.625rem] z-[1] -mx-2.5 -mb-2.5 bg-muted p-2.5 shadow-[0_-8px_16px_rgb(0_0_0/0.18)]",
      className
    )}
  />
);

const PinnedBreakdown = tw(
  "div",
  "hidden border-t px-4 pb-3.5 pt-3 @max-[34rem]:block pointer-coarse:block"
);

const VisuallyHidden = tw(
  "span",
  "absolute m-[-1px] h-px w-px overflow-hidden whitespace-nowrap border-0 p-0 [clip:rect(0,0,0,0)]"
);

interface BreakdownProps {
  date: string;
  mode: string;
  metric: UsageMetric;
  rows: UsageTooltipRow[];
  providerCosts: ProviderCostRow[];
  stickyTotal?: boolean;
  total: number;
}

function BreakdownContent({
  date,
  mode,
  metric,
  rows,
  providerCosts,
  stickyTotal = false,
  total,
}: BreakdownProps) {
  const labels = tooltipLabels(rows);
  const visibleRows = rows.slice(0, MAX_TOOLTIP_MODELS);
  const hiddenRows = rows.slice(MAX_TOOLTIP_MODELS);
  const hiddenValue = hiddenRows.reduce((sum, row) => sum + row.value, 0);
  const visibleCosts = providerCosts.slice(0, MAX_TOOLTIP_PROVIDERS);
  const hiddenCosts = providerCosts.slice(MAX_TOOLTIP_PROVIDERS);

  return (
    <>
      <BreakdownHeader>
        <BreakdownDate>{date}</BreakdownDate>
        <BreakdownMode>{mode}</BreakdownMode>
      </BreakdownHeader>
      <BreakdownList>
        {visibleRows.map(({ series, value }) => {
          const name = labels.get(series.id) ?? series.label;
          return (
            <BreakdownRow key={series.id}>
              <Swatch $color={series.color} aria-hidden="true" />
              <BreakdownName title={name}>{name}</BreakdownName>
              <BreakdownValue>{formatMetric(value, metric)}</BreakdownValue>
            </BreakdownRow>
          );
        })}
      </BreakdownList>
      {hiddenRows.length > 0 && (
        <MoreRow>
          <span>+{hiddenRows.length} more models</span>
          <span>{formatMetric(hiddenValue, metric)}</span>
        </MoreRow>
      )}
      {metric === "tokens" && providerCosts.length > 0 && (
        <CostSection>
          <CostHeading>Cost by provider</CostHeading>
          <BreakdownList>
            {visibleCosts.map((row) => (
              <BreakdownRow key={row.provider}>
                <Swatch $color={row.color} aria-hidden="true" />
                <BreakdownName>{row.label}</BreakdownName>
                <BreakdownValue>{formatCurrency(row.value)}</BreakdownValue>
              </BreakdownRow>
            ))}
          </BreakdownList>
          {hiddenCosts.length > 0 && (
            <MoreRow>
              <span>+{hiddenCosts.length} more providers</span>
              <span>
                {formatCurrency(
                  hiddenCosts.reduce((sum, row) => sum + row.value, 0),
                )}
              </span>
            </MoreRow>
          )}
        </CostSection>
      )}
      <BreakdownTotal $sticky={stickyTotal}>
        <span>Total {metricLabel(metric).toLowerCase()}</span>
        <span>{formatMetric(total, metric)}</span>
      </BreakdownTotal>
    </>
  );
}

export function ProfileUsageChart({
  contributions,
  initialMetric = "tokens",
  description = "Model activity, grouped by coding provider.",
  averageWindowDays = 30,
  rangeStart = null,
  rangeEnd = null,
}: ProfileUsageChartProps) {
  const headingId = useId();
  const chartTitleId = useId();
  const chartDescriptionId = useId();
  const keyboardInstructionsId = useId();
  const [metric, setMetric] = useState<UsageMetric>(initialMetric);
  const [view, setView] = useState<UsageView>("average");
  const [providerFilter, setProviderFilter] =
    useState<UsageProviderFilter>(ALL_USAGE_PROVIDERS);
  // Reversal is resolved after mount so the server render and the first client
  // paint stay chronological (no hydration mismatch). A persisted explicit
  // choice is the only way to start newest-first.
  const isMounted = useSyncExternalStore(
    subscribeUsageChartMounted,
    () => true,
    () => false,
  );
  // Lazy initializer: reads once on the client; the server (and the hydration
  // render, via the isMounted gate below) always resolves chronological.
  const [storedNewestFirst, setStoredNewestFirst] = useState<boolean | null>(
    () => {
      if (typeof window === "undefined") return null;
      try {
        const stored = window.localStorage.getItem(NEWEST_FIRST_STORAGE_KEY);
        return stored === "1" ? true : stored === "0" ? false : null;
      } catch {
        // localStorage may be unavailable (private mode / disabled).
        return null;
      }
    },
  );
  const newestFirst = isMounted && storedNewestFirst === true;
  const commitNewestFirst = (next: boolean) => {
    setStoredNewestFirst(next);
    try {
      window.localStorage.setItem(NEWEST_FIRST_STORAGE_KEY, next ? "1" : "0");
    } catch {
      // Ignore persistence failures; the in-memory choice still applies.
    }
  };
  const [activeDate, setActiveDate] = useState<string | null>(null);
  const [announcedDate, setAnnouncedDate] = useState<string | null>(null);
  const [interactionMode, setInteractionMode] =
    useState<InteractionMode>("idle");
  const plotRef = useRef<HTMLDivElement>(null);
  const [plotWidth, setPlotWidth] = useState(0);
  const [tooltipMaxHeight, setTooltipMaxHeight] = useState(TOOLTIP_MAX_HEIGHT);

  const days = useMemo(
    () =>
      aggregateDailyUsage(
        contributions,
        rangeStart ?? undefined,
        rangeEnd ?? undefined,
      ),
    [contributions, rangeStart, rangeEnd],
  );
  const providerTotals = useMemo(
    () => getUsageProviderTotals(days, metric),
    [days, metric],
  );
  const costProviderTotals = useMemo(
    () => getUsageProviderTotals(days, "cost"),
    [days],
  );
  const selectedProvider =
    providerFilter === ALL_USAGE_PROVIDERS ||
    providerTotals.some(({ provider }) => provider === providerFilter)
      ? providerFilter
      : ALL_USAGE_PROVIDERS;
  const chronologicalChartData = useMemo(
    () =>
      buildUsageChartData(
        days,
        metric,
        selectedProvider,
        view,
        averageWindowDays,
      ),
    [days, metric, selectedProvider, view, averageWindowDays],
  );
  // Everything below renders in visual order: with "Newest first" on, the
  // whole per-day pipeline (dates, series, totals) is mirrored once here so
  // pointer, keyboard, and tooltip index math needs no special cases.
  const chartData = useMemo(
    () =>
      newestFirst
        ? reverseUsageChartData(chronologicalChartData)
        : chronologicalChartData,
    [chronologicalChartData, newestFirst],
  );
  const chartStack = useMemo(
    () =>
      createChartStack(
        chartData.series,
        chartData.dates.length,
        chartData.maxDailyTotal,
      ),
    [chartData],
  );
  const { layers } = chartStack;
  const chartMaximum = chartStack.maximum;

  useEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;

    const measure = () => {
      const bounds = plot.getBoundingClientRect();
      const nextWidth = bounds.width;
      const availableHeight =
        window.innerHeight -
        Math.max(
          TOOLTIP_EDGE,
          bounds.top + TOOLTIP_EDGE + TOOLTIP_VIEWPORT_EDGE,
        );
      setPlotWidth((currentWidth) =>
        currentWidth === nextWidth ? currentWidth : nextWidth,
      );
      setTooltipMaxHeight((currentHeight) => {
        const nextHeight = clamp(availableHeight, 0, TOOLTIP_MAX_HEIGHT);
        return currentHeight === nextHeight ? currentHeight : nextHeight;
      });
    };
    measure();

    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);

    if (typeof ResizeObserver === "undefined") {
      return () => {
        window.removeEventListener("resize", measure);
        window.removeEventListener("scroll", measure, true);
      };
    }

    const observer = new ResizeObserver(measure);
    observer.observe(plot);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  }, [chartData.dates.length]);

  const requestedActiveIndex = activeDate
    ? chartData.dates.indexOf(activeDate)
    : -1;
  // The idle inspection target is always the newest day, whichever edge it
  // renders on.
  const activeIndex =
    requestedActiveIndex >= 0
      ? requestedActiveIndex
      : newestFirst
        ? 0
        : chartData.dates.length - 1;
  const currentDate = chartData.dates[activeIndex] ?? null;
  const currentTotal = chartData.dailyTotals[activeIndex] ?? 0;
  const activeRows = useMemo(
    () => getActiveTooltipRows(chartData.series, activeIndex),
    [chartData.series, activeIndex],
  );
  // `days` stays chronological, so the visual index is mapped back before
  // indexing per-provider cost values.
  const chronologicalActiveIndex = newestFirst
    ? chartData.dates.length - 1 - activeIndex
    : activeIndex;
  const providerCostRows = useMemo(
    () =>
      getProviderCostRows(
        days,
        costProviderTotals,
        selectedProvider,
        chronologicalActiveIndex,
        view,
        chartData.averageWindowDays,
      ),
    [
      days,
      costProviderTotals,
      selectedProvider,
      chronologicalActiveIndex,
      view,
      chartData.averageWindowDays,
    ],
  );

  const modelLegend = useMemo(
    () => selectLegendModels(chartData.series, MAX_LEGEND_MODELS),
    [chartData.series],
  );

  const setActiveIndex = (index: number, announce = false) => {
    if (chartData.dates.length === 0) return;
    const nextIndex = Math.max(0, Math.min(chartData.dates.length - 1, index));
    const date = chartData.dates[nextIndex];
    setActiveDate(date);
    if (announce) setAnnouncedDate(date);
  };

  const handlePointer = (
    event: PointerEvent<HTMLDivElement>,
    announce = false,
  ) => {
    if (chartData.dates.length === 0) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const viewBoxX =
      ((event.clientX - bounds.left) / bounds.width) * VIEWBOX_WIDTH;
    const progress = Math.max(
      0,
      Math.min(1, (viewBoxX - PLOT_LEFT) / PLOT_WIDTH),
    );
    setActiveIndex(
      Math.round(progress * (chartData.dates.length - 1)),
      announce,
    );
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (chartData.dates.length === 0) return;

    switch (event.key) {
      case "ArrowLeft":
        event.preventDefault();
        setInteractionMode("committed");
        setActiveIndex(activeIndex - 1, true);
        break;
      case "ArrowRight":
        event.preventDefault();
        setInteractionMode("committed");
        setActiveIndex(activeIndex + 1, true);
        break;
      case "Home":
        event.preventDefault();
        setInteractionMode("committed");
        setActiveIndex(0, true);
        break;
      case "End":
        event.preventDefault();
        setInteractionMode("committed");
        setActiveIndex(chartData.dates.length - 1, true);
        break;
      case "Escape":
        event.preventDefault();
        setInteractionMode("idle");
        setAnnouncedDate(null);
        break;
    }
  };

  const activeX =
    activeIndex >= 0
      ? xForIndex(activeIndex, chartData.dates.length)
      : PLOT_LEFT;
  const activeOffset = (activeX / VIEWBOX_WIDTH) * 100;
  const activeTooltipLeft = tooltipLeft(activeOffset, plotWidth);
  const modeLabel = viewLabel(view, chartData.averageWindowDays);
  const chartTitle = `${modeLabel} ${metricLabel(metric).toLowerCase()} usage by model and provider`;
  // Screen readers should hear the true chronological span, so build from/to
  // from `chronologicalChartData` (unmirrored source) rather than the possibly
  // reversed display order. When "Newest first" mirrors the visible axis, note
  // it so AT users know the plotted direction is flipped.
  const descriptionDates = chronologicalChartData.dates;
  const chartDescription = `${chartTitle} from ${
    descriptionDates[0] ? formatDate(descriptionDates[0]) : "no start date"
  } to ${
    descriptionDates.at(-1)
      ? formatDate(descriptionDates.at(-1) as string)
      : "no end date"
  }${newestFirst ? ", displayed newest first" : ""}. Raw range total: ${formatMetric(
    chartData.total,
    metric,
  )}.`;
  const announcedIndex = announcedDate
    ? chartData.dates.indexOf(announcedDate)
    : -1;
  const announcedRows =
    announcedIndex >= 0
      ? getActiveTooltipRows(chartData.series, announcedIndex)
      : [];
  const announcedLabels = tooltipLabels(announcedRows);
  const announcement =
    announcedIndex >= 0
      ? `${formatDate(chartData.dates[announcedIndex])}, ${modeLabel}: ${formatMetric(
          chartData.dailyTotals[announcedIndex] ?? 0,
          metric,
        )}. ${announcedRows
          .slice(0, 3)
          .map(
            ({ series, value }) =>
              `${announcedLabels.get(series.id) ?? series.label} ${formatMetric(value, metric)}`,
          )
          .join(", ")}${
          announcedRows.length > 3
            ? `, plus ${announcedRows.length - 3} more models`
            : ""
        }`
      : "";
  const isInspecting = interactionMode !== "idle" && currentDate !== null;

  return (
    <Section aria-labelledby={headingId}>
      <Header>
        <HeadingGroup>
          <Heading id={headingId}>Usage over time</Heading>
          <Description>{description}</Description>
        </HeadingGroup>
        <Total>
          <TotalLabel>
            Range total {metricLabel(metric).toLowerCase()}
          </TotalLabel>
          <TotalValue title={chartData.total.toLocaleString("en-US")}>
            {formatMetric(chartData.total, metric)}
          </TotalValue>
        </Total>
      </Header>

      <Controls>
        <ControlCluster>
          <MetricControl aria-label="Usage metric">
            {(["tokens", "cost"] as const).map((option) => (
              <MetricButton
                key={option}
                type="button"
                $active={metric === option}
                aria-pressed={metric === option}
                onClick={() => setMetric(option)}
              >
                {metricLabel(option)}
              </MetricButton>
            ))}
          </MetricControl>

          <SelectControl>
            <SelectCaption>Display</SelectCaption>
            <CompactSelect
              aria-label="Usage display"
              value={view}
              onChange={(event) =>
                setView(event.currentTarget.value as UsageView)
              }
            >
              <option value="average">
                {chartData.averageWindowDays}d average
              </option>
              <option value="daily">Daily</option>
            </CompactSelect>
          </SelectControl>
        </ControlCluster>

        <ControlCluster>
          <NewestFirstControl title="Show newest activity on the left">
            <input
              type="checkbox"
              name="profile-usage-newest-first"
              aria-label="Show newest activity on the left"
              checked={newestFirst}
              onChange={(event) =>
                commitNewestFirst(event.currentTarget.checked)
              }
            />
            <span>Newest first</span>
          </NewestFirstControl>
          <SelectControl>
            <SelectCaption>Provider</SelectCaption>
            <CompactSelect
              name="profile-usage-provider"
              aria-label="Usage provider"
              value={selectedProvider}
              onChange={(event) =>
                setProviderFilter(
                  event.currentTarget.value as UsageProviderFilter,
                )
              }
            >
              <option value={ALL_USAGE_PROVIDERS}>All</option>
              {providerTotals.map(({ provider }) => (
                <option key={provider} value={provider}>
                  {providerName(provider)}
                </option>
              ))}
            </CompactSelect>
          </SelectControl>
        </ControlCluster>
      </Controls>

      <PlotRegion>
        {chartData.dates.length > 0 ? (
          <InteractivePlot
            ref={plotRef}
            tabIndex={0}
            role="group"
            aria-describedby={keyboardInstructionsId}
            aria-label={`Interactive ${metricLabel(metric).toLowerCase()} chart`}
            onKeyDown={handleKeyDown}
            onPointerMove={(event) => {
              if (event.pointerType === "mouse") {
                setInteractionMode("hover");
                handlePointer(event);
              }
            }}
            onPointerLeave={() =>
              setInteractionMode((mode) => (mode === "hover" ? "idle" : mode))
            }
            onPointerDown={(event) => {
              setInteractionMode("committed");
              handlePointer(event, true);
            }}
          >
            <ChartSvg
              viewBox={`0 0 ${VIEWBOX_WIDTH} ${VIEWBOX_HEIGHT}`}
              preserveAspectRatio="none"
              role="img"
              aria-labelledby={`${chartTitleId} ${chartDescriptionId}`}
            >
              <title id={chartTitleId}>{chartTitle}</title>
              <desc id={chartDescriptionId}>{chartDescription}</desc>

              {Array.from({ length: GRID_STEPS + 1 }, (_, index) => {
                const value = (chartMaximum * index) / GRID_STEPS;
                const y = yForValue(value, chartMaximum);
                return (
                  <GridLine
                    key={index}
                    x1={PLOT_LEFT}
                    x2={PLOT_LEFT + PLOT_WIDTH}
                    y1={y}
                    y2={y}
                  />
                );
              })}

              {layers.map((layer) => (
                <g key={layer.series.id}>
                  <LayerArea d={layer.areaPath} $color={layer.series.color} />
                  <LayerLine d={layer.linePath} $color={layer.series.color} />
                </g>
              ))}

              {isInspecting && (
                <ActiveRule
                  x1={activeX}
                  x2={activeX}
                  y1={PLOT_TOP}
                  y2={PLOT_TOP + PLOT_HEIGHT}
                />
              )}
            </ChartSvg>

            {isInspecting &&
              layers.map((layer) => {
                if ((layer.series.values[activeIndex] ?? 0) <= 0) return null;
                const position = pointToChartPercent(
                  activeX,
                  yForValue(layer.upperValues[activeIndex] ?? 0, chartMaximum),
                  VIEWBOX_WIDTH,
                  VIEWBOX_HEIGHT,
                );
                return (
                  <ActivePoint
                    key={layer.series.id}
                    aria-hidden="true"
                    data-profile-usage-point
                    $color={layer.series.color}
                    $left={position.left}
                    $top={position.top}
                  />
                );
              })}

            <DateRange aria-label="Chart date range">
              <span>{formatDate(chartData.dates[0])}</span>
              {chartData.dates.length > 1 && (
                <span>{formatDate(chartData.dates.at(-1) as string)}</span>
              )}
            </DateRange>

            {isInspecting && currentDate && (
              <TooltipSurface
                role="tooltip"
                data-profile-usage-tooltip
                tabIndex={interactionMode === "committed" ? 0 : undefined}
                $left={activeTooltipLeft}
                $maxHeight={tooltipMaxHeight}
                onPointerMove={(event) => event.stopPropagation()}
                onPointerDown={(event) => event.stopPropagation()}
                onKeyDown={(event) => event.stopPropagation()}
              >
                <BreakdownContent
                  date={currentDate}
                  mode={modeLabel}
                  metric={metric}
                  rows={activeRows}
                  providerCosts={providerCostRows}
                  stickyTotal
                  total={currentTotal}
                />
              </TooltipSurface>
            )}
          </InteractivePlot>
        ) : (
          <EmptyState>No usage data yet.</EmptyState>
        )}
        <VisuallyHidden id={keyboardInstructionsId}>
          Use Left Arrow and Right Arrow to inspect adjacent days. Use Home and
          End to jump to the first or last day. Press Escape to close the
          inspection.
        </VisuallyHidden>
      </PlotRegion>

      {(modelLegend.visible.length > 0 || modelLegend.hiddenCount > 0) && (
        <Legend role="list" aria-label="Usage models">
          {modelLegend.visible.map((entry) => (
            <LegendItem key={entry.id}>
              <Swatch $color={entry.color} aria-hidden="true" />
              <span>{entry.label}</span>
            </LegendItem>
          ))}
          {modelLegend.hiddenCount > 0 && (
            <LegendItem>
              <span>+{modelLegend.hiddenCount} more</span>
            </LegendItem>
          )}
        </Legend>
      )}

      {interactionMode === "committed" && currentDate && (
        <PinnedBreakdown aria-label={`Usage on ${currentDate}`}>
          <BreakdownContent
            date={currentDate}
            mode={modeLabel}
            metric={metric}
            rows={activeRows}
            providerCosts={providerCostRows}
            total={currentTotal}
          />
        </PinnedBreakdown>
      )}
      <VisuallyHidden role="status" aria-live="polite" aria-atomic="true">
        {announcement}
      </VisuallyHidden>
    </Section>
  );
}
