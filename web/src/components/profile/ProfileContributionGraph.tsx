"use client";

import {
  Fragment,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent,
} from "react";
import { SourceLogo } from "@/components/SourceLogo";
import { SOURCE_COLORS, SOURCE_DISPLAY_NAMES, SOURCE_LOGOS } from "@/lib/constants";
import { getContributionIntensity } from "@/lib/embed/embedShared";
import type {
  ClientContribution,
  ClientType,
  DailyContribution,
  TokenBreakdown,
} from "@/lib/types";
import {
  colorPalettes,
  DEFAULT_PALETTE,
  getDarkGradeColors,
  getPalette,
  getPaletteNames,
  type ColorPaletteName,
  type GraphColorPalette,
} from "@/lib/themes";
import { formatCurrency, formatTokenCount } from "@/lib/utils";
import { tw } from "@/lib/tw";
import { cn } from "@/lib/utils";

export interface ProfileContributionGraphProps {
  breakdownId?: string;
  className?: string;
  contributions: DailyContribution[];
  description?: string;
  onPaletteChange?: (palette: ColorPaletteName) => void;
  onRangeChange?: (range: string) => void;
  onSelectedDateChange?: (date: string | null) => void;
  onViewChange?: (view: ProfileContributionView) => void;
  paletteName?: ColorPaletteName;
  persistentSelection?: boolean;
  rangeEnd?: string | null;
  rangeOptions?: readonly ContributionRangeOption[];
  rangeStart?: string | null;
  rangeValue?: string;
  selectableRangeEnd?: string | null;
  selectedDate?: string | null;
  showBreakdown?: boolean;
  view?: ProfileContributionView;
}

export type ProfileContributionView = "2d" | "3d";

export interface ContributionRangeOption {
  endDate: string;
  label: string;
  startDate: string;
  value: string;
}

export interface ContributionSelectionState {
  date: string | null;
  rangeIdentity: string;
}

export function resolveContributionSelectedDate(
  requested: ContributionSelectionState | null,
  rangeIdentity: string,
  defaultDate: string | null,
): string | null {
  return requested?.rangeIdentity === rangeIdentity && requested.date
    ? requested.date
    : defaultDate;
}

export function reconcileContributionSelectionRange(
  selection: ContributionSelectionState,
  rangeIdentity: string,
): ContributionSelectionState {
  return selection.rangeIdentity === rangeIdentity
    ? selection
    : { date: null, rangeIdentity };
}

export interface ContributionCell {
  date: string;
  intensity: 0 | 1 | 2 | 3 | 4;
  inRange: boolean;
  selectable: boolean;
  tokens: number;
}

interface MonthMarker {
  compactVisible: boolean;
  label: string;
  weekIndex: number;
}

interface ContributionTooltipState {
  cell: ContributionCell;
  day: DailyContribution;
  left: number;
  top: number;
}

export interface ContributionModelDetail {
  cost: number;
  messages: number;
  modelId: string;
  providerId: string | null;
  tokens: TokenBreakdown;
  totalTokens: number;
}

export interface ContributionClientDetail {
  client: ClientType;
  cost: number;
  messages: number;
  models: ContributionModelDetail[];
  tokens: TokenBreakdown;
  totalTokens: number;
}

export interface ProfileContributionBreakdownProps {
  className?: string;
  day: DailyContribution;
  id: string;
  onClose?: () => void;
  paletteName?: ColorPaletteName;
}

export interface ContributionCalendar {
  activeDays: number;
  cells: ContributionCell[];
  endDate: string | null;
  freeTokenDays: number;
  highestDay: ContributionCell | null;
  monthMarkers: MonthMarker[];
  selectableEndDate: string | null;
  startDate: string | null;
  weekCount: number;
}

export interface ContributionIsometricCell {
  cell: ContributionCell;
  centerX: number;
  centerY: number;
  dayIndex: number;
  height: number;
  weekIndex: number;
}

export interface ContributionIsometricGeometry {
  cells: ContributionIsometricCell[];
  viewBox: { height: number; width: number };
}

export interface ContributionHitTarget {
  bottom: number;
  date: string;
  left: number;
  right: number;
  top: number;
}

type ContributionNavigationKey =
  "ArrowDown" | "ArrowLeft" | "ArrowRight" | "ArrowUp" | "End" | "Home";

const DAY_MS = 24 * 60 * 60 * 1000;
const DATE_PATTERN = /^(\d{4})-(\d{2})-(\d{2})$/;
const LEGACY_COST_FLOAT_EPSILON = 1e-6;
export const PROFILE_CONTRIBUTION_CELL_GAP = 2;
export const PROFILE_CONTRIBUTION_CELL_RADIUS = 1.6;
export const PROFILE_CONTRIBUTION_CELL_SIZE = 8;

export function getContributionScrollOffset(
  currentScrollLeft: number,
  containerLeft: number,
  containerRight: number,
  targetLeft: number,
  targetRight: number,
): number {
  if (targetRight > containerRight) {
    return currentScrollLeft + targetRight - containerRight;
  }
  if (targetLeft < containerLeft) {
    return Math.max(0, currentScrollLeft - (containerLeft - targetLeft));
  }
  return currentScrollLeft;
}

export function isContributionDateHit(target: Element | null): boolean {
  return Boolean(target?.closest("[data-contribution-date]"));
}

const dayFormatter = new Intl.DateTimeFormat("en-US", {
  day: "numeric",
  month: "short",
  timeZone: "UTC",
  year: "numeric",
});

const fullDayFormatter = new Intl.DateTimeFormat("en-US", {
  day: "numeric",
  month: "long",
  timeZone: "UTC",
  weekday: "long",
  year: "numeric",
});

const monthFormatter = new Intl.DateTimeFormat("en-US", {
  month: "short",
  timeZone: "UTC",
});

const tokenFormatter = new Intl.NumberFormat("en-US", {
  maximumFractionDigits: 0,
});

const EMPTY_TOKEN_BREAKDOWN: TokenBreakdown = {
  cacheRead: 0,
  cacheWrite: 0,
  input: 0,
  output: 0,
  reasoning: 0,
};

const TOKEN_CATEGORIES = [
  ["Input", "input"],
  ["Output", "output"],
  ["Cache read", "cacheRead"],
  ["Cache write", "cacheWrite"],
  ["Reasoning", "reasoning"],
] as const;

function parseUtcDate(date: string): number | null {
  const match = DATE_PATTERN.exec(date);
  if (!match) return null;

  const year = Number(match[1]);
  const month = Number(match[2]) - 1;
  const day = Number(match[3]);
  const timestamp = Date.UTC(year, month, day);
  const parsed = new Date(timestamp);

  if (
    parsed.getUTCFullYear() !== year ||
    parsed.getUTCMonth() !== month ||
    parsed.getUTCDate() !== day
  ) {
    return null;
  }

  return timestamp;
}

function toDateKey(timestamp: number): string {
  return new Date(timestamp).toISOString().slice(0, 10);
}

export function createContributionRangeOptions(
  contributions: readonly DailyContribution[],
  recentStart: string | null | undefined,
  recentEnd: string | null | undefined,
): ContributionRangeOption[] {
  const startTimestamp = recentStart ? parseUtcDate(recentStart) : null;
  const endTimestamp = recentEnd ? parseUtcDate(recentEnd) : null;
  if (
    startTimestamp === null ||
    endTimestamp === null ||
    endTimestamp < startTimestamp
  ) {
    return [];
  }

  const latestYear = new Date(endTimestamp).getUTCFullYear();
  const years = new Set<number>([latestYear]);
  for (const contribution of contributions) {
    const timestamp = parseUtcDate(contribution.date);
    if (timestamp === null) continue;

    const year = new Date(timestamp).getUTCFullYear();
    if (year <= latestYear) years.add(year);
  }

  return [
    {
      endDate: recentEnd!,
      label: "Recent year",
      startDate: recentStart!,
      value: "recent",
    },
    ...[...years]
      .sort((left, right) => right - left)
      .map((year) => ({
        endDate: `${year}-12-31`,
        label: String(year),
        startDate: `${year}-01-01`,
        value: String(year),
      })),
  ];
}

export function resolveContributionRange(
  options: readonly ContributionRangeOption[],
  requestedValue: string,
): ContributionRangeOption | null {
  return (
    options.find(({ value }) => value === requestedValue) ??
    options.find(({ value }) => value === "recent") ??
    options[0] ??
    null
  );
}

function safeTokens(value: number): number {
  return Number.isFinite(value) ? Math.max(0, value) : 0;
}

function safeCost(value: number): number {
  return Number.isFinite(value) ? Math.max(0, value) : 0;
}

function safeMessages(value: number): number {
  return Number.isFinite(value) ? Math.max(0, Math.trunc(value)) : 0;
}

function sanitizeTokenBreakdown(tokens: TokenBreakdown): TokenBreakdown {
  return {
    cacheRead: safeTokens(tokens.cacheRead),
    cacheWrite: safeTokens(tokens.cacheWrite),
    input: safeTokens(tokens.input),
    output: safeTokens(tokens.output),
    reasoning: safeTokens(tokens.reasoning),
  };
}

function addTokenBreakdowns(
  left: TokenBreakdown,
  right: TokenBreakdown,
): TokenBreakdown {
  return {
    cacheRead: left.cacheRead + right.cacheRead,
    cacheWrite: left.cacheWrite + right.cacheWrite,
    input: left.input + right.input,
    output: left.output + right.output,
    reasoning: left.reasoning + right.reasoning,
  };
}

function totalBreakdownTokens(tokens: TokenBreakdown): number {
  return (
    tokens.input +
    tokens.output +
    tokens.cacheRead +
    tokens.cacheWrite +
    tokens.reasoning
  );
}

export function mergeDailyContributions(
  contributions: readonly DailyContribution[],
): Map<string, DailyContribution> {
  const days = new Map<string, DailyContribution>();

  for (const contribution of contributions) {
    if (parseUtcDate(contribution.date) === null) continue;

    const existing = days.get(contribution.date);
    const tokens = sanitizeTokenBreakdown(contribution.tokenBreakdown);
    if (!existing) {
      days.set(contribution.date, {
        ...contribution,
        clients: [...contribution.clients],
        intensity: contribution.intensity,
        tokenBreakdown: tokens,
        totals: {
          cost: safeCost(contribution.totals.cost),
          messages: safeMessages(contribution.totals.messages),
          tokens: safeTokens(contribution.totals.tokens),
        },
      });
      continue;
    }

    days.set(contribution.date, {
      ...existing,
      clients: [...existing.clients, ...contribution.clients],
      intensity: Math.max(existing.intensity, contribution.intensity) as
        0 | 1 | 2 | 3 | 4,
      timestampMs: existing.timestampMs ?? contribution.timestampMs,
      tokenBreakdown: addTokenBreakdowns(existing.tokenBreakdown, tokens),
      totals: {
        cost: existing.totals.cost + safeCost(contribution.totals.cost),
        messages:
          existing.totals.messages + safeMessages(contribution.totals.messages),
        tokens: existing.totals.tokens + safeTokens(contribution.totals.tokens),
      },
    });
  }

  return days;
}

function createEmptyContribution(cell: ContributionCell): DailyContribution {
  return {
    clients: [],
    date: cell.date,
    intensity: cell.intensity,
    tokenBreakdown: { ...EMPTY_TOKEN_BREAKDOWN },
    totals: { cost: 0, messages: 0, tokens: cell.tokens },
  };
}

export function getContributionDayForDate(
  contributions: readonly DailyContribution[],
  date: string | null,
): DailyContribution | null {
  if (!date || parseUtcDate(date) === null) return null;

  return (
    mergeDailyContributions(contributions).get(date) ??
    createEmptyContribution({
      date,
      inRange: true,
      intensity: 0,
      selectable: true,
      tokens: 0,
    })
  );
}

function addModelDetail(
  models: Map<string, ContributionModelDetail>,
  detail: ContributionModelDetail,
) {
  const key = `${detail.providerId ?? ""}\u0000${detail.modelId}`;
  const existing = models.get(key);
  if (!existing) {
    models.set(key, detail);
    return;
  }

  const tokens = addTokenBreakdowns(existing.tokens, detail.tokens);
  models.set(key, {
    ...existing,
    cost: existing.cost + detail.cost,
    messages: existing.messages + detail.messages,
    tokens,
    totalTokens: existing.totalTokens + detail.totalTokens,
  });
}

function modelsForClient(
  contribution: ClientContribution,
): ContributionModelDetail[] {
  const nestedModels = Object.entries(contribution.models ?? {});
  if (nestedModels.length > 0) {
    return nestedModels.map(([modelId, model]) => {
      const tokens = sanitizeTokenBreakdown({
        cacheRead: model.cacheRead,
        cacheWrite: model.cacheWrite,
        input: model.input,
        output: model.output,
        reasoning: model.reasoning,
      });
      return {
        cost: safeCost(model.cost),
        messages: safeMessages(model.messages),
        modelId,
        providerId: contribution.providerId?.trim() || null,
        tokens,
        totalTokens: Math.max(
          safeTokens(model.tokens),
          totalBreakdownTokens(tokens),
        ),
      };
    });
  }

  const modelId = contribution.modelId.trim();
  if (!modelId) return [];

  const tokens = sanitizeTokenBreakdown(contribution.tokens);
  return [
    {
      cost: safeCost(contribution.cost),
      messages: safeMessages(contribution.messages),
      modelId,
      providerId: contribution.providerId?.trim() || null,
      tokens,
      totalTokens: totalBreakdownTokens(tokens),
    },
  ];
}

export function createContributionClientDetails(
  day: DailyContribution,
): ContributionClientDetail[] {
  const clients = new Map<
    ClientType,
    Omit<ContributionClientDetail, "models"> & {
      models: Map<string, ContributionModelDetail>;
    }
  >();

  for (const contribution of day.clients) {
    const contributionTokens = sanitizeTokenBreakdown(contribution.tokens);
    const existing = clients.get(contribution.client) ?? {
      client: contribution.client,
      cost: 0,
      messages: 0,
      models: new Map<string, ContributionModelDetail>(),
      tokens: { ...EMPTY_TOKEN_BREAKDOWN },
      totalTokens: 0,
    };
    existing.cost += safeCost(contribution.cost);
    existing.messages += safeMessages(contribution.messages);
    existing.tokens = addTokenBreakdowns(existing.tokens, contributionTokens);
    existing.totalTokens = totalBreakdownTokens(existing.tokens);

    for (const model of modelsForClient(contribution)) {
      addModelDetail(existing.models, model);
    }
    clients.set(contribution.client, existing);
  }

  return [...clients.values()]
    .map((client) => {
      const models = [...client.models.values()].sort(
        (left, right) =>
          right.cost - left.cost ||
          right.totalTokens - left.totalTokens ||
          left.modelId.localeCompare(right.modelId),
      );
      const modelTokens = models.reduce(
        (total, model) => total + model.totalTokens,
        0,
      );
      const modelMessages = models.reduce(
        (total, model) => total + model.messages,
        0,
      );
      return {
        ...client,
        messages: client.messages || modelMessages,
        models,
        totalTokens: client.totalTokens || modelTokens,
      };
    })
    .sort(
      (left, right) =>
        right.cost - left.cost ||
        right.totalTokens - left.totalTokens ||
        String(left.client).localeCompare(String(right.client)),
    );
}

export function getContributionDayMessageCount(
  day: DailyContribution,
  clients: readonly ContributionClientDetail[] = createContributionClientDetails(
    day,
  ),
): number {
  const recordedTotal = safeMessages(day.totals.messages);
  return (
    recordedTotal ||
    clients.reduce((total, client) => total + client.messages, 0)
  );
}

export function createContributionCalendar(
  contributions: readonly DailyContribution[],
  rangeStart?: string | null,
  rangeEnd?: string | null,
  selectableRangeEnd?: string | null,
): ContributionCalendar {
  const contributionsByDate = new Map<
    string,
    { cost: number; timestamp: number; tokens: number }
  >();

  for (const contribution of contributions) {
    const timestamp = parseUtcDate(contribution.date);
    if (timestamp === null) continue;

    const tokens = safeTokens(contribution.totals.tokens);
    const existing = contributionsByDate.get(contribution.date);
    contributionsByDate.set(contribution.date, {
      cost:
        (existing?.cost ?? 0) +
        (Number.isFinite(contribution.totals.cost)
          ? Math.max(0, contribution.totals.cost)
          : 0),
      timestamp,
      tokens: (existing?.tokens ?? 0) + tokens,
    });
  }

  const sorted = [...contributionsByDate.values()].sort(
    (left, right) => left.timestamp - right.timestamp,
  );
  const requestedStart = rangeStart ? parseUtcDate(rangeStart) : null;
  const requestedEnd = rangeEnd ? parseUtcDate(rangeEnd) : null;
  const hasRequestedRange =
    requestedStart !== null &&
    requestedEnd !== null &&
    requestedEnd >= requestedStart;

  if (sorted.length === 0 && !hasRequestedRange) {
    return {
      activeDays: 0,
      cells: [],
      endDate: null,
      freeTokenDays: 0,
      highestDay: null,
      monthMarkers: [],
      selectableEndDate: null,
      startDate: null,
      weekCount: 0,
    };
  }

  const firstTimestamp = hasRequestedRange
    ? requestedStart
    : sorted[0].timestamp;
  const lastTimestamp = hasRequestedRange
    ? requestedEnd
    : sorted[sorted.length - 1].timestamp;
  const requestedSelectableEnd = selectableRangeEnd
    ? parseUtcDate(selectableRangeEnd)
    : null;
  const selectableEndTimestamp =
    requestedSelectableEnd === null
      ? lastTimestamp
      : Math.min(lastTimestamp, requestedSelectableEnd);
  const selectableContributions = sorted.filter(
    ({ timestamp }) =>
      timestamp >= firstTimestamp && timestamp <= selectableEndTimestamp,
  );
  const maxTokens = Math.max(
    0,
    ...selectableContributions.map(({ tokens }) => tokens),
  );
  const calendarStart =
    firstTimestamp - new Date(firstTimestamp).getUTCDay() * DAY_MS;
  const calendarEnd =
    lastTimestamp + (6 - new Date(lastTimestamp).getUTCDay()) * DAY_MS;
  const dayCount = Math.round((calendarEnd - calendarStart) / DAY_MS) + 1;
  const weekCount = dayCount / 7;
  const cells: ContributionCell[] = [];

  for (let offset = 0; offset < dayCount; offset += 1) {
    const timestamp = calendarStart + offset * DAY_MS;
    const date = toDateKey(timestamp);
    const contribution = contributionsByDate.get(date);
    const inRange = timestamp >= firstTimestamp && timestamp <= lastTimestamp;
    const selectable = inRange && timestamp <= selectableEndTimestamp;

    cells.push({
      date,
      inRange,
      intensity: selectable
        ? getContributionIntensity(contribution?.tokens ?? 0, maxTokens)
        : 0,
      selectable,
      tokens: selectable ? (contribution?.tokens ?? 0) : 0,
    });
  }

  const monthMarkers: MonthMarker[] = [];
  const markerWeeks = new Set<number>();
  let cursor = firstTimestamp;

  while (cursor <= lastTimestamp) {
    const date = new Date(cursor);
    const weekIndex = Math.floor((cursor - calendarStart) / (DAY_MS * 7));

    if (!markerWeeks.has(weekIndex)) {
      const month = date.getUTCMonth();
      const marker = {
        compactVisible: monthMarkers.length === 0 || month % 3 === 0,
        label: monthFormatter.format(date),
        weekIndex,
      };
      const previous = monthMarkers.at(-1);
      if (previous && weekIndex - previous.weekIndex < 3) {
        // A short partial first month can land beside the next label. Prefer
        // the first full month rather than allowing labels to collide.
        if (previous.weekIndex === 0)
          monthMarkers[monthMarkers.length - 1] = marker;
      } else {
        monthMarkers.push(marker);
      }
      markerWeeks.add(weekIndex);
    }

    cursor = Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 1);
  }

  return {
    activeDays: selectableContributions.filter(({ tokens }) => tokens > 0)
      .length,
    cells,
    endDate: toDateKey(lastTimestamp),
    freeTokenDays: selectableContributions.filter(
      ({ cost, tokens }) =>
        tokens > 0 &&
        Number.isFinite(cost) &&
        Math.abs(cost) <= LEGACY_COST_FLOAT_EPSILON,
    ).length,
    highestDay:
      [...cells]
        .filter(({ inRange, tokens }) => inRange && tokens > 0)
        .sort(
          (left, right) =>
            right.tokens - left.tokens || left.date.localeCompare(right.date),
        )[0] ?? null,
    monthMarkers,
    selectableEndDate:
      selectableEndTimestamp >= firstTimestamp
        ? toDateKey(selectableEndTimestamp)
        : null,
    startDate: toDateKey(firstTimestamp),
    weekCount,
  };
}

export function getDefaultContributionDate(
  contributions: readonly DailyContribution[],
  rangeStart?: string | null,
  rangeEnd?: string | null,
  selectableRangeEnd?: string | null,
): string | null {
  const calendar = createContributionCalendar(
    contributions,
    rangeStart,
    rangeEnd,
    selectableRangeEnd,
  );
  return calendar.selectableEndDate ?? calendar.endDate;
}

const ISOMETRIC_CELL_WIDTH = 7.5;
const ISOMETRIC_CELL_DEPTH = 3.75;
const ISOMETRIC_MARGIN = 12;
const ISOMETRIC_MIN_HEIGHT = 1.5;
const ISOMETRIC_ACTIVE_MIN_HEIGHT = 4;
const ISOMETRIC_MAX_HEIGHT = 100;

export function createContributionIsometricGeometry(
  calendar: ContributionCalendar,
): ContributionIsometricGeometry {
  const maxTokens = Math.max(
    0,
    ...calendar.cells
      .filter(({ inRange }) => inRange)
      .map(({ tokens }) => tokens),
  );
  const finalWeek = Math.max(0, calendar.weekCount - 1);
  const originX = ISOMETRIC_MARGIN + 6 * ISOMETRIC_CELL_WIDTH;
  const originY = ISOMETRIC_MARGIN + ISOMETRIC_MAX_HEIGHT;
  const cells = calendar.cells.flatMap((cell, index) => {
    if (!cell.inRange) return [];

    const weekIndex = Math.floor(index / 7);
    const dayIndex = index % 7;
    const ratio = maxTokens > 0 ? cell.tokens / maxTokens : 0;
    const height =
      cell.tokens > 0
        ? ISOMETRIC_ACTIVE_MIN_HEIGHT +
          ratio * (ISOMETRIC_MAX_HEIGHT - ISOMETRIC_ACTIVE_MIN_HEIGHT)
        : ISOMETRIC_MIN_HEIGHT;

    return [
      {
        cell,
        centerX: originX + (weekIndex - dayIndex) * ISOMETRIC_CELL_WIDTH,
        centerY: originY + (weekIndex + dayIndex) * ISOMETRIC_CELL_DEPTH,
        dayIndex,
        height,
        weekIndex,
      },
    ];
  });

  return {
    cells,
    viewBox: {
      height:
        originY +
        (finalWeek + 6) * ISOMETRIC_CELL_DEPTH +
        ISOMETRIC_CELL_DEPTH * 2 +
        ISOMETRIC_MARGIN,
      width:
        originX +
        finalWeek * ISOMETRIC_CELL_WIDTH +
        ISOMETRIC_CELL_WIDTH +
        ISOMETRIC_MARGIN,
    },
  };
}

function contributionCubeFaces({
  centerX,
  centerY,
  height,
}: ContributionIsometricCell): {
  left: string;
  right: string;
  top: string;
} {
  const topY = centerY - height;
  const leftX = centerX - ISOMETRIC_CELL_WIDTH;
  const rightX = centerX + ISOMETRIC_CELL_WIDTH;
  const middleY = topY + ISOMETRIC_CELL_DEPTH;
  const bottomTopY = topY + ISOMETRIC_CELL_DEPTH * 2;
  const middleBottomY = centerY + ISOMETRIC_CELL_DEPTH;
  const bottomY = centerY + ISOMETRIC_CELL_DEPTH * 2;

  return {
    left: `${leftX},${middleY} ${centerX},${bottomTopY} ${centerX},${bottomY} ${leftX},${middleBottomY}`,
    right: `${rightX},${middleY} ${centerX},${bottomTopY} ${centerX},${bottomY} ${rightX},${middleBottomY}`,
    top: `${centerX},${topY} ${rightX},${middleY} ${centerX},${bottomTopY} ${leftX},${middleY}`,
  };
}

function shadeContributionColor(color: string, percentage: number): string {
  return `color-mix(in srgb, ${color} ${percentage}%, #000)`;
}

export function getContributionFocusDate(
  cells: readonly ContributionCell[],
  currentDate: string | null,
  key: ContributionNavigationKey,
): string | null {
  const dates = cells
    .filter(({ selectable }) => selectable)
    .map(({ date }) => date);
  if (dates.length === 0) return null;

  const currentIndex = currentDate ? dates.indexOf(currentDate) : -1;
  const safeIndex = currentIndex >= 0 ? currentIndex : dates.length - 1;
  let nextIndex = safeIndex;

  switch (key) {
    case "ArrowLeft":
      nextIndex -= 1;
      break;
    case "ArrowRight":
      nextIndex += 1;
      break;
    case "ArrowUp":
      nextIndex -= 7;
      break;
    case "ArrowDown":
      nextIndex += 7;
      break;
    case "Home":
      nextIndex = 0;
      break;
    case "End":
      nextIndex = dates.length - 1;
      break;
  }

  return dates[Math.max(0, Math.min(dates.length - 1, nextIndex))] ?? null;
}

export function getNearestContributionDate(
  targets: readonly ContributionHitTarget[],
  clientX: number,
  clientY: number,
  maximumDistance = 24,
): string | null {
  let nearestDate: string | null = null;
  let nearestDistanceSquared = Number.POSITIVE_INFINITY;

  for (const target of targets) {
    const distanceX =
      clientX < target.left
        ? target.left - clientX
        : clientX > target.right
          ? clientX - target.right
          : 0;
    const distanceY =
      clientY < target.top
        ? target.top - clientY
        : clientY > target.bottom
          ? clientY - target.bottom
          : 0;
    const distanceSquared = distanceX ** 2 + distanceY ** 2;

    if (distanceSquared < nearestDistanceSquared) {
      nearestDate = target.date;
      nearestDistanceSquared = distanceSquared;
    }
  }

  return nearestDistanceSquared <= maximumDistance ** 2 ? nearestDate : null;
}

function formatRange(startDate: string | null, endDate: string | null): string {
  if (!startDate || !endDate) return "No activity yet";

  const start = parseUtcDate(startDate);
  const end = parseUtcDate(endDate);
  if (start === null || end === null) return "No activity yet";
  if (start === end) return dayFormatter.format(start);
  return `${dayFormatter.format(start)} – ${dayFormatter.format(end)}`;
}

function cellTitle(cell: ContributionCell): string {
  const timestamp = parseUtcDate(cell.date);
  const date = timestamp === null ? cell.date : dayFormatter.format(timestamp);
  const tokenLabel = cell.tokens === 1 ? "token" : "tokens";
  return `${date}: ${tokenFormatter.format(cell.tokens)} ${tokenLabel}`;
}

export function getContributionColor(
  palette: GraphColorPalette,
  level: ContributionCell["intensity"],
): string {
  if (level === 0) return "var(--service-surface-muted)";

  // The shared palettes are light-canvas ramps. Reverse them for dark mode,
  // then lift only colors that need contrast or a clear step from the
  // preceding intensity instead of whitening every grade.
  return getDarkGradeColors(palette)[level - 1] ?? palette.grade1;
}

function getLightContributionColor(
  palette: GraphColorPalette,
  level: ContributionCell["intensity"],
): string {
  if (level === 0) return "var(--service-surface-muted)";
  return [palette.grade1, palette.grade2, palette.grade3, palette.grade4][
    level - 1
  ] ?? palette.grade1;
}

function getContributionColors(
  palette: GraphColorPalette,
  level: ContributionCell["intensity"],
): { light: string; dark: string } {
  return {
    light: getLightContributionColor(palette, level),
    dark: getContributionColor(palette, level),
  };
}

function clientHasLogo(client: ClientType): boolean {
  return Object.prototype.hasOwnProperty.call(
    SOURCE_LOGOS,
    String(client).toLowerCase(),
  );
}

// ============================================================================
// Calendar chrome
// ============================================================================
//
// The figure is the container, so the calendar reflows on its own width — it
// is embedded at several widths and the viewport tells it nothing useful.
//
// Grade colours arrive from the palette at runtime and differ per theme, so
// they ride in as --lc/--dc custom properties with a static class pair that
// Tailwind can see. Light uses the light grade, dark the dark one.

const Figure = tw(
  "figure",
  "@container m-0 flex w-full min-w-0 max-w-full flex-col overflow-hidden rounded-xl border bg-card text-foreground"
);

const Header = tw(
  "figcaption",
  "flex items-start justify-between gap-4 border-b px-4 py-3.5 @max-[28rem]:flex-col @max-[28rem]:gap-2.5"
);

const HeadingGroup = tw("div", "min-w-0");
const HeadingRow = tw("div", "flex min-w-0 items-center gap-2");

const Heading = tw(
  "h2",
  "m-0 text-[0.9375rem] font-semibold tracking-tight text-foreground"
);

// The chevron is drawn into a second grid column so the select can size to its
// own text without the arrow overlapping it.
const RangeSelectWrapper = tw(
  "span",
  "relative inline-grid min-w-0 grid-cols-[minmax(0,1fr)_0.75rem] items-center after:pointer-events-none after:col-start-2 after:row-start-1 after:size-[0.3125rem] after:-translate-y-0.5 after:rotate-45 after:border-b after:border-r after:border-muted-foreground after:content-['']"
);

const RangeSelect = tw(
  "select",
  "col-span-full row-start-1 min-w-0 cursor-pointer appearance-none overflow-hidden text-ellipsis rounded-none border-0 border-b border-dotted border-b-muted-foreground bg-transparent pb-0.5 pl-0 pr-4 pt-0 text-xs font-medium leading-5 text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring pointer-coarse:min-h-11 pointer-coarse:py-2.5 [&_option]:bg-card [&_option]:text-foreground"
);

const Description = tw(
  "p",
  "m-0 mt-1 max-w-[46ch] text-[0.8125rem] leading-snug text-muted-foreground"
);

const HeaderAside = tw(
  "div",
  "flex flex-none items-start gap-3 @max-[28rem]:w-full @max-[28rem]:justify-between"
);

const ViewToggle = tw(
  "div",
  "inline-flex rounded-lg border bg-muted p-0.5"
);

// The inner span carries the active pill so the hit area stays 44px on touch
// while the pill itself keeps its 2rem width.
const ViewButton = ({
  $active,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"button"> & { $active: boolean }) => (
  <button
    {...props}
    className={cn(
      "relative inline-flex h-6 min-w-8 cursor-pointer items-center justify-center rounded-[0.35rem] border-0 bg-transparent p-0 text-[0.625rem] font-semibold focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring",
      "[&>span]:inline-flex [&>span]:h-full [&>span]:w-full [&>span]:items-center [&>span]:justify-center [&>span]:rounded-[inherit]",
      "pointer-coarse:min-w-11 pointer-coarse:after:absolute pointer-coarse:after:left-1/2 pointer-coarse:after:top-1/2 pointer-coarse:after:h-11 pointer-coarse:after:w-full pointer-coarse:after:-translate-x-1/2 pointer-coarse:after:-translate-y-1/2 pointer-coarse:after:content-[''] pointer-coarse:[&>span]:w-8",
      $active
        ? "text-foreground [&>span]:bg-card"
        : "text-muted-foreground [&>span]:bg-transparent",
      className
    )}
  />
);

const Summary = tw(
  "div",
  "flex-none text-right [font-variant-numeric:tabular-nums] @max-[28rem]:ml-auto @max-[28rem]:block @max-[28rem]:w-auto"
);

const ActiveDays = tw(
  "div",
  "text-[0.8125rem] font-semibold text-foreground"
);

const Range = tw(
  "div",
  "mt-0.5 whitespace-nowrap text-[0.6875rem] text-muted-foreground"
);

const CalendarBody = tw(
  "div",
  "relative flex min-w-0 flex-col justify-center overflow-x-auto px-4 pb-3 pt-3.5 [-webkit-overflow-scrolling:touch] [overscroll-behavior-inline:contain] [scrollbar-width:thin] @max-[24rem]:px-3"
);

const IsometricBody = tw(
  "div",
  "relative grid min-h-48 min-w-0 place-items-center overflow-hidden px-4 py-3 @max-[24rem]:min-h-40 @max-[24rem]:px-3"
);

const IsometricSvg = tw("svg", "block max-h-80 w-full overflow-visible");

// aria-hidden cells are spacers: they keep pointer events so the grid does not
// gap, but they must not look interactive.
//
// $active and $selected are accepted and dropped. The stylesheet declared them
// and never read them — the highlight is drawn by IsometricTop's stroke — and
// styled-components filtered $-prefixed props out on the way to the DOM, so
// forwarding them now would only produce unknown-attribute warnings.
const IsometricCell = ({
  $active: _active,
  $selected: _selected,
  className,
  ...props
}: React.ComponentPropsWithRef<"g"> & {
  $active: boolean;
  $selected: boolean;
}) => (
  <g
    {...props}
    className={cn(
      "cursor-pointer outline-none [&[aria-hidden='true']]:cursor-default [&[aria-hidden='true']]:pointer-events-auto [&_polygon]:transition-[stroke,filter] [&_polygon]:duration-100 motion-reduce:[&_polygon]:transition-none [&:not([aria-hidden='true']):hover_polygon]:brightness-110 [&:not([aria-hidden='true']):focus-visible_polygon]:brightness-110",
      className
    )}
  />
);

const IsometricTop = ({
  $active,
  $darkColor,
  $lightColor,
  $selected,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"polygon"> & {
  $active: boolean;
  $darkColor: string;
  $lightColor: string;
  $selected: boolean;
}) => (
  <polygon
    {...props}
    style={
      {
        "--lc": $lightColor,
        "--dc": $darkColor,
        stroke: $selected
          ? "var(--ring)"
          : $active
            ? "var(--foreground)"
            : "color-mix(in srgb, var(--foreground) 8%, transparent)",
        strokeWidth: $active || $selected ? 1.4 : 0.55,
        ...style,
      } as React.CSSProperties
    }
    className="fill-[var(--lc)] [vector-effect:non-scaling-stroke] dark:fill-[var(--dc)]"
  />
);

const IsometricSide = ({
  $darkColor,
  $lightColor,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"polygon"> & {
  $darkColor: string;
  $lightColor: string;
}) => (
  <polygon
    {...props}
    style={{ "--lc": $lightColor, "--dc": $darkColor, ...style } as React.CSSProperties}
    className="fill-[var(--lc)] dark:fill-[var(--dc)]"
  />
);

// Column width is the cell size unless the range is short enough that wider
// cells read better. Track counts are runtime values, so the template is
// inline.
const cellTrack = (weeks: number) =>
  weeks <= 5 ? "1.25rem" : `${PROFILE_CONTRIBUTION_CELL_SIZE}px`;

const MonthRow = ({
  $weeks,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & { $weeks: number }) => (
  <div
    {...props}
    style={{
      gridTemplateColumns: `repeat(${$weeks}, ${cellTrack($weeks)})`,
      columnGap: `${PROFILE_CONTRIBUTION_CELL_GAP}px`,
      ...style,
    }}
    className="box-border grid h-[1.125rem] w-max min-w-0 pl-7 text-muted-foreground @max-[22rem]:pl-0"
  />
);

// The ::after is the tick mark under each month label.
const Month = ({
  $compactVisible,
  $week,
  className,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & {
  $compactVisible: boolean;
  $week: number;
}) => (
  <span
    {...props}
    style={{ gridColumn: $week + 1, ...style }}
    className={cn(
      "relative min-w-0 whitespace-nowrap text-[0.625rem] leading-none [font-variant-numeric:tabular-nums]",
      "after:absolute after:left-0 after:top-3 after:h-1 after:w-px after:bg-muted-foreground/40 after:content-['']",
      !$compactVisible && "@max-[32rem]:hidden",
      className
    )}
  />
);

const CalendarRow = tw(
  "div",
  "grid w-max min-w-0 grid-cols-[1.25rem_max-content] items-stretch gap-2 @max-[22rem]:grid-cols-[max-content]"
);

const DayLabels = ({ style, ...props }: React.ComponentPropsWithoutRef<"div">) => (
  <div
    {...props}
    style={{ gap: `${PROFILE_CONTRIBUTION_CELL_GAP}px`, ...style }}
    className="grid grid-rows-[repeat(7,minmax(0,1fr))] text-[0.625rem] leading-none text-muted-foreground @max-[22rem]:hidden"
  />
);

const DayLabel = ({
  $row,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & { $row: number }) => (
  <span {...props} style={{ gridRow: $row, ...style }} className="self-center" />
);

const Grid = ({
  $weeks,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & { $weeks: number }) => (
  <div
    {...props}
    style={{
      gridTemplateColumns: `repeat(${$weeks}, ${cellTrack($weeks)})`,
      gridTemplateRows: `repeat(7, ${cellTrack($weeks)})`,
      gap: `${PROFILE_CONTRIBUTION_CELL_GAP}px`,
      ...style,
    }}
    className="grid w-max min-w-0 grid-flow-col"
  />
);

// Out-of-range cells stay in the layout but hidden, so the grid keeps its
// shape at the start and end of a range. The inset shadow gives an empty cell
// an edge; the outer one is selection or hover.
const Cell = ({
  $active,
  $darkColor,
  $inRange,
  $lightColor,
  $selected,
  style,
  ...props
}: React.ComponentPropsWithRef<"button"> & {
  $active: boolean;
  $darkColor: string;
  $inRange: boolean;
  $lightColor: string;
  $selected: boolean;
}) => (
  <button
    {...props}
    style={
      {
        "--lc": $lightColor,
        "--dc": $darkColor,
        visibility: $inRange ? "visible" : "hidden",
        borderRadius: `${PROFILE_CONTRIBUTION_CELL_RADIUS}px`,
        boxShadow: `inset 0 0 0 1px color-mix(in srgb, var(--foreground) 4%, transparent), ${
          $selected
            ? "0 0 0 1px var(--ring)"
            : $active
              ? "0 0 0 1px var(--foreground)"
              : "0 0 0 0 transparent"
        }`,
        ...style,
      } as React.CSSProperties
    }
    className="block aspect-square min-w-0 cursor-pointer border-0 bg-[var(--lc)] p-0 disabled:cursor-default focus-visible:relative focus-visible:z-[2] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring dark:bg-[var(--dc)]"
  />
);

const Footer = tw(
  "div",
  "flex items-center justify-between gap-3 px-4 pb-3.5 @max-[24rem]:px-3"
);

const PaletteControl = tw(
  "label",
  "relative inline-flex min-w-0 items-center gap-1.5 text-[0.6875rem] text-muted-foreground after:pointer-events-none after:absolute after:right-2.5 after:size-[0.3125rem] after:-translate-y-0.5 after:rotate-45 after:border-b after:border-r after:border-muted-foreground after:content-[''] pointer-coarse:min-h-11"
);

const PaletteSelect = tw(
  "select",
  "h-7 max-w-30 cursor-pointer appearance-none rounded-[0.4375rem] border bg-muted pl-2 pr-6 text-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
);

const PalettePreview = tw(
  "span",
  "inline-grid grid-cols-[repeat(4,0.625rem)] gap-[0.1875rem] overflow-hidden rounded-sm"
);

const PalettePreviewSwatch = ({
  $darkColor,
  $lightColor,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & {
  $darkColor: string;
  $lightColor: string;
}) => (
  <span
    {...props}
    style={{ "--lc": $lightColor, "--dc": $darkColor, ...style } as React.CSSProperties}
    className="size-2.5 bg-[var(--lc)] dark:bg-[var(--dc)]"
  />
);

const Legend = tw(
  "div",
  "inline-flex items-center gap-[0.3125rem] text-[0.6875rem] text-muted-foreground"
);

const LegendSwatches = tw(
  "span",
  "inline-grid grid-cols-[repeat(5,0.625rem)] gap-[0.1875rem]"
);

const LegendSwatch = ({
  $darkColor,
  $lightColor,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & {
  $darkColor: string;
  $lightColor: string;
}) => (
  <span
    {...props}
    style={{ "--lc": $lightColor, "--dc": $darkColor, ...style } as React.CSSProperties}
    className="size-2.5 rounded-sm bg-[var(--lc)] shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--foreground)_5%,transparent)] dark:bg-[var(--dc)]"
  />
);

// Positioned against the viewport, not the figure, so it is never clipped by
// the calendar's own horizontal scroll.
const CellTooltip = ({
  $left,
  $top,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & { $left: number; $top: number }) => (
  <div
    {...props}
    style={{ top: `${$top}px`, left: `${$left}px`, ...style }}
    className="pointer-events-none fixed z-[80] grid max-h-[calc(100dvh-1.5rem)] w-[min(17.5rem,calc(100vw-1.5rem))] gap-2 overflow-hidden rounded-[0.625rem] border border-muted-foreground/30 bg-muted p-3 text-foreground shadow-[0_12px_32px_rgb(0_0_0/0.34)] [font-variant-numeric:tabular-nums]"
  />
);

const CellTooltipDate = tw("span", "text-[0.6875rem] text-muted-foreground");
const TooltipTotal = tw("div", "flex items-baseline justify-between gap-3");
const TooltipTotalLabel = tw("span", "text-[0.6875rem] text-muted-foreground");

const CellTooltipValue = tw(
  "strong",
  "text-base font-semibold tracking-tight text-foreground"
);

const TooltipDivider = tw("span", "block h-px bg-border");

const TooltipMetricGrid = tw(
  "div",
  "grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1 text-[0.6875rem]"
);

const TooltipMetricLabel = tw(
  "span",
  "min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-muted-foreground"
);

const TooltipMetricValue = tw(
  "span",
  "text-right font-mono font-semibold text-foreground"
);

const TooltipSectionLabel = tw(
  "span",
  "text-[0.625rem] font-semibold uppercase tracking-[0.06em] text-muted-foreground"
);

// Standalone it is its own card; inline it is the section below the calendar
// and only needs the rule joining them.
const DetailPanel = ({
  $standalone,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"section"> & { $standalone: boolean }) => (
  <section
    {...props}
    className={cn(
      "@container overflow-hidden border-border bg-[color-mix(in_srgb,var(--muted)_42%,var(--card))]",
      $standalone ? "rounded-xl border" : "rounded-none border-x-0 border-b-0 border-t",
      className
    )}
  />
);

const DetailHeader = tw(
  "header",
  "flex items-start justify-between gap-3 border-b px-4 py-3.5"
);

const DetailEyebrow = tw(
  "div",
  "mb-0.5 text-[0.625rem] font-semibold uppercase tracking-[0.07em] text-muted-foreground"
);

const DetailTitle = tw(
  "h3",
  "m-0 text-sm font-semibold tracking-tight text-foreground"
);

const DetailClose = tw(
  "button",
  "inline-grid size-7 flex-none cursor-pointer place-items-center rounded-[0.4375rem] border border-transparent bg-transparent p-0 text-muted-foreground hover:border-border hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
);

const DetailBody = tw("div", "grid gap-4 p-4");

const DetailSummary = tw(
  "div",
  "grid grid-cols-3 overflow-hidden rounded-lg border [&>div]:rounded-none [&>div]:border-0 [&>div]:bg-transparent [&>div]:p-2.5 [&>div+div]:border-l [&>div+div]:border-border"
);

const DetailMetric = tw("div", "min-w-0 px-3 py-2.5");

const DetailMetricLabel = tw(
  "div",
  "mb-0.5 text-[0.625rem] text-muted-foreground"
);

const DetailMetricValue = tw(
  "div",
  "overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[0.8125rem] font-semibold text-foreground"
);

// Five across, then three, then two. Each step re-picks which cells start a
// row so the vertical rules never sit on a row edge.
const TokenDetailGrid = tw(
  "div",
  cn(
    "grid grid-cols-5 overflow-hidden rounded-lg border [&>div+div]:border-l [&>div+div]:border-border",
    "@max-[36rem]:grid-cols-3 @max-[36rem]:[&>div:nth-child(4)]:border-l-0 @max-[36rem]:[&>div:nth-child(n+4)]:border-t @max-[36rem]:[&>div:nth-child(n+4)]:border-border",
    "@max-[24rem]:grid-cols-2 @max-[24rem]:[&>div:nth-child(odd)]:border-l-0 @max-[24rem]:[&>div:nth-child(4)]:border-l @max-[24rem]:[&>div:nth-child(n+3)]:border-t @max-[24rem]:[&>div:nth-child(n+3)]:border-border"
  )
);

const DetailSection = tw("section", "grid gap-2");

const DetailSectionTitle = tw(
  "h4",
  "m-0 text-[0.6875rem] font-semibold text-muted-foreground"
);

// Scrolls inside the inline panel; on its own page, and on a narrow one, it
// runs to full height instead.
const ClientList = ({
  $standalone,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & { $standalone: boolean }) => (
  <div
    {...props}
    className={cn(
      "grid rounded-[0.625rem] border focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
      $standalone ? "max-h-none overflow-visible" : "max-h-100 overflow-auto",
      "@max-[28rem]:max-h-none @max-[28rem]:overflow-visible",
      className
    )}
  />
);

const ClientSection = tw(
  "section",
  "min-w-0 p-3 [&+&]:border-t [&+&]:border-border"
);

const ClientHeader = tw("div", "flex items-center justify-between gap-3");
const ClientIdentity = tw("div", "flex min-w-0 items-center gap-2");

const ClientDot = ({
  $color,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & { $color: string }) => (
  <span
    {...props}
    style={{ background: $color, ...style }}
    className="size-2 flex-none rounded-full border border-[color-mix(in_srgb,var(--foreground)_28%,transparent)]"
  />
);

const ClientName = tw(
  "strong",
  "overflow-hidden text-ellipsis whitespace-nowrap text-xs font-semibold text-foreground"
);

const ClientTotal = tw(
  "span",
  "flex-none font-mono text-[0.6875rem] text-muted-foreground [font-variant-numeric:tabular-nums]"
);

const ModelList = tw("div", "mt-2.5 grid gap-1.5 pl-4");

const ModelRow = tw(
  "div",
  "grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-0.5 border-l border-muted-foreground/30 pl-2.5"
);

const ModelName = tw(
  "span",
  "overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[0.6875rem] text-foreground"
);

const ModelValue = tw(
  "span",
  "font-mono text-[0.6875rem] text-foreground [font-variant-numeric:tabular-nums]"
);

const ModelMeta = tw(
  "span",
  "col-span-full text-[0.6875rem] leading-snug text-muted-foreground"
);

const NoDayActivity = tw("p", "m-0 text-xs text-muted-foreground");

function formatFullDay(date: string): string {
  const timestamp = parseUtcDate(date);
  return timestamp === null ? date : fullDayFormatter.format(timestamp);
}

function getClientName(client: ClientType): string {
  return SOURCE_DISPLAY_NAMES[client] ?? client;
}

function getClientColor(
  client: ClientType,
  palette: GraphColorPalette,
): string {
  return SOURCE_COLORS[client] ?? palette.grade2;
}

function modelMeta(model: ContributionModelDetail): string {
  const metrics = TOKEN_CATEGORIES.flatMap(([label, key]) =>
    model.tokens[key] > 0
      ? [`${label} ${formatTokenCount(model.tokens[key])}`]
      : [],
  );
  if (model.messages > 0) {
    metrics.push(
      `${model.messages.toLocaleString("en-US")} ${
        model.messages === 1 ? "message" : "messages"
      }`,
    );
  }
  return metrics.join(" · ");
}

function ContributionDayTooltip({ day }: { day: DailyContribution }) {
  const clients = createContributionClientDetails(day);
  const messageCount = getContributionDayMessageCount(day, clients);
  const visibleCategories = TOKEN_CATEGORIES.filter(
    ([, key]) => day.tokenBreakdown[key] > 0,
  );

  return (
    <>
      <CellTooltipDate>{formatFullDay(day.date)}</CellTooltipDate>
      <TooltipTotal>
        <TooltipTotalLabel>Total tokens</TooltipTotalLabel>
        <CellTooltipValue>
          {formatTokenCount(day.totals.tokens)}
        </CellTooltipValue>
      </TooltipTotal>
      <TooltipDivider />
      {visibleCategories.length > 0 && (
        <TooltipMetricGrid>
          {visibleCategories.map(([label, key]) => (
            <Fragment key={key}>
              <TooltipMetricLabel>{label}</TooltipMetricLabel>
              <TooltipMetricValue>
                {formatTokenCount(day.tokenBreakdown[key])}
              </TooltipMetricValue>
            </Fragment>
          ))}
        </TooltipMetricGrid>
      )}
      <TooltipMetricGrid>
        <TooltipMetricLabel>Cost</TooltipMetricLabel>
        <TooltipMetricValue>
          {formatCurrency(day.totals.cost)}
        </TooltipMetricValue>
        <TooltipMetricLabel>Messages</TooltipMetricLabel>
        <TooltipMetricValue>
          {messageCount.toLocaleString("en-US")}
        </TooltipMetricValue>
      </TooltipMetricGrid>
      {clients.length > 0 && (
        <>
          <TooltipDivider />
          <TooltipSectionLabel>Clients</TooltipSectionLabel>
          <TooltipMetricGrid>
            {clients.slice(0, 3).map((client) => (
              <Fragment key={client.client}>
                <TooltipMetricLabel>
                  {getClientName(client.client)}
                </TooltipMetricLabel>
                <TooltipMetricValue>
                  {formatTokenCount(client.totalTokens)}
                </TooltipMetricValue>
              </Fragment>
            ))}
            {clients.length > 3 && (
              <>
                <TooltipMetricLabel>
                  +{clients.length - 3} more
                </TooltipMetricLabel>
                <TooltipMetricValue>Click for detail</TooltipMetricValue>
              </>
            )}
          </TooltipMetricGrid>
        </>
      )}
    </>
  );
}

function ContributionDayBreakdown({
  className,
  day,
  id,
  onClose,
  palette,
  standalone = false,
}: {
  className?: string;
  day: DailyContribution;
  id: string;
  onClose?: () => void;
  palette: GraphColorPalette;
  standalone?: boolean;
}) {
  const headingId = `${id}-heading`;
  const clients = createContributionClientDetails(day);
  const messageCount = getContributionDayMessageCount(day, clients);

  return (
    <DetailPanel
      id={id}
      aria-labelledby={headingId}
      className={className}
      $standalone={standalone}
    >
      <DetailHeader>
        <div>
          <DetailEyebrow>Day breakdown</DetailEyebrow>
          <DetailTitle id={headingId}>{formatFullDay(day.date)}</DetailTitle>
        </div>
        {onClose && (
          <DetailClose
            type="button"
            onClick={onClose}
            aria-label="Close day breakdown"
          >
            <svg
              aria-hidden="true"
              fill="none"
              height="14"
              viewBox="0 0 14 14"
              width="14"
            >
              <path
                d="M3 3l8 8M11 3l-8 8"
                stroke="currentColor"
                strokeLinecap="round"
                strokeWidth="1.5"
              />
            </svg>
          </DetailClose>
        )}
      </DetailHeader>
      <DetailBody>
        <DetailSummary>
          <DetailMetric>
            <DetailMetricLabel>Total tokens</DetailMetricLabel>
            <DetailMetricValue>
              {formatTokenCount(day.totals.tokens)}
            </DetailMetricValue>
          </DetailMetric>
          <DetailMetric>
            <DetailMetricLabel>Cost</DetailMetricLabel>
            <DetailMetricValue>
              {formatCurrency(day.totals.cost)}
            </DetailMetricValue>
          </DetailMetric>
          <DetailMetric>
            <DetailMetricLabel>Messages</DetailMetricLabel>
            <DetailMetricValue>
              {messageCount.toLocaleString("en-US")}
            </DetailMetricValue>
          </DetailMetric>
        </DetailSummary>

        <DetailSection>
          <DetailSectionTitle>Token categories</DetailSectionTitle>
          <TokenDetailGrid>
            {TOKEN_CATEGORIES.map(([label, key]) => (
              <DetailMetric key={key}>
                <DetailMetricLabel>{label}</DetailMetricLabel>
                <DetailMetricValue>
                  {formatTokenCount(day.tokenBreakdown[key])}
                </DetailMetricValue>
              </DetailMetric>
            ))}
          </TokenDetailGrid>
        </DetailSection>

        <DetailSection>
          <DetailSectionTitle>Clients and models</DetailSectionTitle>
          {clients.length > 0 ? (
            <ClientList
              $standalone={standalone}
              tabIndex={standalone ? undefined : 0}
              aria-label="Client and model details"
            >
              {clients.map((client) => (
                <ClientSection key={client.client}>
                  <ClientHeader>
                    <ClientIdentity>
                      {clientHasLogo(client.client) ? (
                        <SourceLogo
                          sourceId={client.client}
                          height={14}
                          decorative
                        />
                      ) : (
                        <ClientDot
                          $color={getClientColor(client.client, palette)}
                          aria-hidden="true"
                        />
                      )}
                      <ClientName>{getClientName(client.client)}</ClientName>
                    </ClientIdentity>
                    <ClientTotal>
                      {formatTokenCount(client.totalTokens)} ·{" "}
                      {formatCurrency(client.cost)}
                    </ClientTotal>
                  </ClientHeader>
                  {client.models.length > 0 && (
                    <ModelList>
                      {client.models.map((model) => (
                        <ModelRow
                          key={`${model.providerId ?? ""}-${model.modelId}`}
                        >
                          <ModelName title={model.modelId}>
                            {model.modelId}
                          </ModelName>
                          <ModelValue>{formatCurrency(model.cost)}</ModelValue>
                          <ModelMeta>
                            {model.providerId && `${model.providerId} · `}
                            {formatTokenCount(model.totalTokens)} tokens
                            {modelMeta(model) && ` · ${modelMeta(model)}`}
                          </ModelMeta>
                        </ModelRow>
                      ))}
                    </ModelList>
                  )}
                </ClientSection>
              ))}
            </ClientList>
          ) : (
            <NoDayActivity>
              No client or model detail was recorded for this day.
            </NoDayActivity>
          )}
        </DetailSection>
      </DetailBody>
    </DetailPanel>
  );
}

export function ProfileContributionBreakdown({
  className,
  day,
  id,
  onClose,
  paletteName = DEFAULT_PALETTE,
}: ProfileContributionBreakdownProps) {
  return (
    <ContributionDayBreakdown
      className={className}
      day={day}
      id={id}
      onClose={onClose}
      palette={getPalette(paletteName)}
      standalone
    />
  );
}

const EmptyState = tw(
  "div",
  "px-4 py-6 text-center text-[0.8125rem] text-muted-foreground"
);

const VisuallyHidden = tw(
  "span",
  "absolute m-[-1px] h-px w-px overflow-hidden whitespace-nowrap border-0 p-0 [clip:rect(0,0,0,0)]"
);

export function ProfileContributionGraph({
  breakdownId: providedBreakdownId,
  className,
  contributions,
  description = "Daily token activity across the available history.",
  onPaletteChange,
  onRangeChange,
  onSelectedDateChange,
  onViewChange,
  paletteName: providedPaletteName,
  persistentSelection = false,
  rangeEnd,
  rangeOptions = [],
  rangeStart,
  rangeValue,
  selectableRangeEnd,
  selectedDate: providedSelectedDate,
  showBreakdown = true,
  view: providedView,
}: ProfileContributionGraphProps) {
  const titleId = useId();
  const descriptionId = useId();
  const tooltipId = useId();
  const generatedBreakdownId = useId();
  const breakdownId = providedBreakdownId ?? generatedBreakdownId;
  const calendarId = useId();
  const calendarInstructionsId = useId();
  const calendarScrollRef = useRef<HTMLDivElement>(null);
  const cellRefs = useRef(new Map<string, Element & { focus: () => void }>());
  const [tooltip, setTooltip] = useState<ContributionTooltipState | null>(null);
  const [keyboardDate, setKeyboardDate] = useState<string | null>(null);
  const [internalSelectedDate, setInternalSelectedDate] = useState<
    string | null
  >(null);
  const [internalPaletteName, setInternalPaletteName] =
    useState<ColorPaletteName>(DEFAULT_PALETTE);
  const [internalView, setInternalView] =
    useState<ProfileContributionView>("2d");
  const selectedDate =
    providedSelectedDate === undefined
      ? internalSelectedDate
      : providedSelectedDate;
  const paletteName = providedPaletteName ?? internalPaletteName;
  const view = providedView ?? internalView;
  const palette = useMemo(() => getPalette(paletteName), [paletteName]);
  const contributionDays = useMemo(
    () => mergeDailyContributions(contributions),
    [contributions],
  );
  const calendar = useMemo(
    () =>
      createContributionCalendar(
        contributions,
        rangeStart,
        rangeEnd,
        selectableRangeEnd,
      ),
    [contributions, rangeStart, rangeEnd, selectableRangeEnd],
  );
  const activeDayLabel = `${calendar.activeDays.toLocaleString("en-US")} active ${
    calendar.activeDays === 1 ? "day" : "days"
  }`;
  const accessibleDetail = calendar.highestDay
    ? `Highest activity: ${cellTitle(calendar.highestDay)}. ${calendar.freeTokenDays.toLocaleString(
        "en-US",
      )} active days used tokens with no recorded cost.`
    : "No active contribution days are available.";
  const inRangeDates = useMemo(
    () =>
      calendar.cells
        .filter(({ selectable }) => selectable)
        .map(({ date }) => date),
    [calendar.cells],
  );
  const tabbableDate =
    keyboardDate && inRangeDates.includes(keyboardDate)
      ? keyboardDate
      : (calendar.selectableEndDate ?? inRangeDates.at(-1) ?? null);
  const selectedCell = selectedDate
    ? (calendar.cells.find(
        ({ date, selectable }) => selectable && date === selectedDate,
      ) ?? null)
    : null;
  const selectedDay = selectedCell
    ? (contributionDays.get(selectedCell.date) ??
      createEmptyContribution(selectedCell))
    : null;
  const isometricGeometry = useMemo(
    () => createContributionIsometricGeometry(calendar),
    [calendar],
  );

  useEffect(() => {
    const scrollContainer = calendarScrollRef.current;
    const target = tabbableDate ? cellRefs.current.get(tabbableDate) : null;
    if (!scrollContainer || !target) return;

    const containerBounds = scrollContainer.getBoundingClientRect();
    const targetBounds = target.getBoundingClientRect();
    const nextScrollLeft = getContributionScrollOffset(
      scrollContainer.scrollLeft,
      containerBounds.left,
      containerBounds.right,
      targetBounds.left,
      targetBounds.right,
    );
    if (nextScrollLeft !== scrollContainer.scrollLeft) {
      scrollContainer.scrollLeft = nextScrollLeft;
    }
  }, [rangeEnd, rangeStart, tabbableDate, view]);

  const commitSelectedDate = (date: string | null) => {
    if (providedSelectedDate === undefined) setInternalSelectedDate(date);
    onSelectedDateChange?.(date);
  };

  const commitPalette = (name: ColorPaletteName) => {
    if (providedPaletteName === undefined) setInternalPaletteName(name);
    onPaletteChange?.(name);
  };

  const commitRange = (value: string) => {
    setTooltip(null);
    setKeyboardDate(null);
    onRangeChange?.(value);
  };

  const commitView = (nextView: ProfileContributionView) => {
    if (providedView === undefined) setInternalView(nextView);
    onViewChange?.(nextView);
  };

  const positionCellTooltip = (cell: ContributionCell, target: Element) => {
    // In-range-but-unselectable cells (e.g. future days in the current year)
    // must stay fully inert: no tooltip on hover or focus. `selectable` implies
    // `inRange`, and today remains selectable, so its tooltip is unaffected.
    if (!cell.selectable || typeof window === "undefined") {
      setTooltip(null);
      return;
    }

    const cellBounds = target.getBoundingClientRect();
    const gutter = 12;
    const tooltipWidth = Math.min(280, window.innerWidth - gutter * 2);
    const estimatedHeight = Math.min(360, window.innerHeight - gutter * 2);
    const cellCenter = cellBounds.left + cellBounds.width / 2;
    const left = Math.max(
      gutter,
      Math.min(
        window.innerWidth - tooltipWidth - gutter,
        cellCenter - tooltipWidth / 2,
      ),
    );
    const preferredTop = cellBounds.top - estimatedHeight - 8;
    const fallbackTop = cellBounds.bottom + 8;
    const top = Math.max(
      gutter,
      Math.min(
        window.innerHeight - estimatedHeight - gutter,
        preferredTop >= gutter ? preferredTop : fallbackTop,
      ),
    );

    setTooltip({
      cell,
      day: contributionDays.get(cell.date) ?? createEmptyContribution(cell),
      left,
      top,
    });
  };

  const handleCellPointerEnter = (
    cell: ContributionCell,
    event: PointerEvent<Element>,
  ) => positionCellTooltip(cell, event.currentTarget);

  const handleCellFocus = (
    cell: ContributionCell,
    event: FocusEvent<Element>,
  ) => {
    setKeyboardDate(cell.date);
    positionCellTooltip(cell, event.currentTarget);
  };

  const handleCellPointerLeave = (event: PointerEvent<Element>) => {
    const active = document.activeElement;
    // Still focused within the cell the pointer is leaving: keep its tooltip.
    if (active === event.currentTarget) return;

    // The pointer left, but keyboard focus may rest on a different cell whose
    // aria-describedby points at the tooltip. Re-anchor the tooltip to that
    // focused cell instead of stranding its description on a cleared tooltip.
    // Only while a tooltip is open: if Escape already dismissed it, leaving
    // the hovered cell must not resurrect it.
    if (active && tooltip) {
      for (const [date, node] of cellRefs.current) {
        if (node !== active) continue;
        const focusedCell = calendar.cells.find(
          (candidate) => candidate.date === date,
        );
        if (focusedCell) {
          positionCellTooltip(focusedCell, node);
          return;
        }
      }
    }

    setTooltip(null);
  };

  const handleCellKeyDown = (
    cell: ContributionCell,
    event: KeyboardEvent<Element>,
    orderedCells: readonly ContributionCell[] = calendar.cells,
  ) => {
    if (event.key === "Escape") {
      event.preventDefault();
      setTooltip(null);
      if (!persistentSelection) commitSelectedDate(null);
      return;
    }

    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      setTooltip(null);
      commitSelectedDate(
        selectedDate === cell.date && !persistentSelection ? null : cell.date,
      );
      return;
    }

    if (
      ![
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "End",
        "Home",
      ].includes(event.key)
    ) {
      return;
    }

    event.preventDefault();
    const nextDate = getContributionFocusDate(
      orderedCells,
      cell.date,
      event.key as ContributionNavigationKey,
    );
    if (!nextDate) return;

    setKeyboardDate(nextDate);
    cellRefs.current.get(nextDate)?.focus();
  };

  const closeSelectedDay = () => {
    const date = selectedDate;
    commitSelectedDate(null);
    if (date) requestAnimationFrame(() => cellRefs.current.get(date)?.focus());
  };

  const selectCell = (cell: ContributionCell) => {
    if (!cell.selectable) return;
    setTooltip(null);
    setKeyboardDate(cell.date);
    commitSelectedDate(
      selectedDate === cell.date && !persistentSelection ? null : cell.date,
    );
  };

  const selectNearestCell = (event: ReactMouseEvent<Element>) => {
    const target = event.target instanceof Element ? event.target : null;
    if (isContributionDateHit(target)) {
      return;
    }

    const targets = calendar.cells.flatMap((cell) => {
      if (!cell.selectable) return [];
      const node = cellRefs.current.get(cell.date);
      if (!node) return [];
      const bounds = node.getBoundingClientRect();
      return [
        {
          bottom: bounds.bottom,
          date: cell.date,
          left: bounds.left,
          right: bounds.right,
          top: bounds.top,
        },
      ];
    });
    const date = getNearestContributionDate(
      targets,
      event.clientX,
      event.clientY,
    );
    if (!date) return;

    const cell = calendar.cells.find(
      (candidate) => candidate.selectable && candidate.date === date,
    );
    if (cell) selectCell(cell);
  };

  return (
    <Figure
      aria-describedby={descriptionId}
      aria-labelledby={titleId}
      className={className}
    >
      <Header>
        <HeadingGroup>
          <HeadingRow>
            <Heading id={titleId}>Contributions</Heading>
            {rangeOptions.length > 1 && rangeValue && onRangeChange && (
              <RangeSelectWrapper>
                <RangeSelect
                  name="profile-contribution-range"
                  aria-label="Contribution date range"
                  aria-controls={calendarId}
                  value={rangeValue}
                  onChange={(event) => commitRange(event.currentTarget.value)}
                >
                  {rangeOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </RangeSelect>
              </RangeSelectWrapper>
            )}
          </HeadingRow>
          <Description id={descriptionId}>{description}</Description>
        </HeadingGroup>
        <HeaderAside>
          <ViewToggle role="group" aria-label="Contribution graph view">
            {(["2d", "3d"] as const).map((option) => (
              <ViewButton
                key={option}
                type="button"
                $active={view === option}
                aria-controls={calendarId}
                aria-pressed={view === option}
                onClick={() => commitView(option)}
              >
                <span>{option.toUpperCase()}</span>
              </ViewButton>
            ))}
          </ViewToggle>
          <Summary aria-live="polite">
            <ActiveDays>{activeDayLabel}</ActiveDays>
            <Range>{formatRange(calendar.startDate, calendar.endDate)}</Range>
          </Summary>
        </HeaderAside>
      </Header>
      <VisuallyHidden>{accessibleDetail}</VisuallyHidden>

      {calendar.weekCount > 0 ? (
        <>
          {view === "2d" ? (
            <CalendarBody id={calendarId} ref={calendarScrollRef}>
              <MonthRow $weeks={calendar.weekCount} aria-hidden="true">
                {calendar.monthMarkers.map((marker) => (
                  <Month
                    key={`${marker.weekIndex}-${marker.label}`}
                    $compactVisible={marker.compactVisible}
                    $week={marker.weekIndex}
                  >
                    {marker.label}
                  </Month>
                ))}
              </MonthRow>
              <CalendarRow>
                <DayLabels aria-hidden="true">
                  <DayLabel $row={2}>Mon</DayLabel>
                  <DayLabel $row={4}>Wed</DayLabel>
                  <DayLabel $row={6}>Fri</DayLabel>
                </DayLabels>
                <Grid
                  $weeks={calendar.weekCount}
                  role="group"
                  aria-label="Daily token contributions"
                  aria-describedby={calendarInstructionsId}
                  data-contribution-hit-surface="2d"
                  onClick={selectNearestCell}
                >
                  {calendar.cells.map((cell) => {
                    const colors = getContributionColors(
                      palette,
                      cell.intensity,
                    );
                    return (
                      <Cell
                        key={cell.date}
                        type="button"
                        ref={(node) => {
                          if (node && cell.selectable)
                            cellRefs.current.set(cell.date, node);
                          else cellRefs.current.delete(cell.date);
                        }}
                        disabled={!cell.inRange || !cell.selectable}
                        tabIndex={
                          cell.selectable && cell.date === tabbableDate ? 0 : -1
                        }
                        aria-hidden={cell.inRange ? undefined : true}
                        aria-label={cell.inRange ? cellTitle(cell) : undefined}
                        aria-current={
                          cell.selectable && cell.date === selectedDate
                            ? "date"
                            : undefined
                        }
                        aria-pressed={
                          cell.selectable
                            ? cell.date === selectedDate
                            : undefined
                        }
                        aria-controls={
                          cell.selectable && cell.date === selectedDate
                            ? breakdownId
                            : undefined
                        }
                        aria-describedby={
                          tooltip?.cell.date === cell.date
                            ? tooltipId
                            : undefined
                        }
                        data-contribution-date={
                          cell.inRange ? cell.date : undefined
                        }
                        $active={tooltip?.cell.date === cell.date}
                        $darkColor={colors.dark}
                        $inRange={cell.inRange}
                        $lightColor={colors.light}
                        $selected={cell.date === selectedDate}
                        onClick={() => selectCell(cell)}
                        onPointerEnter={(event) =>
                          handleCellPointerEnter(cell, event)
                        }
                        onPointerLeave={handleCellPointerLeave}
                        onFocus={(event) => handleCellFocus(cell, event)}
                        onBlur={() => setTooltip(null)}
                        onKeyDown={(event) => handleCellKeyDown(cell, event)}
                      />
                    );
                  })}
                </Grid>
              </CalendarRow>
            </CalendarBody>
          ) : (
            <IsometricBody id={calendarId}>
              <IsometricSvg
                viewBox={`0 0 ${isometricGeometry.viewBox.width} ${isometricGeometry.viewBox.height}`}
                role="group"
                aria-label="Isometric daily token contributions"
                aria-describedby={calendarInstructionsId}
                data-contribution-hit-surface="3d"
                onClick={selectNearestCell}
                preserveAspectRatio="xMidYMid meet"
              >
                {isometricGeometry.cells.map((geometry) => {
                  const { cell } = geometry;
                  const faces = contributionCubeFaces(geometry);
                  const colors = getContributionColors(
                    palette,
                    cell.intensity,
                  );
                  const active = tooltip?.cell.date === cell.date;
                  const selected = selectedDate === cell.date;
                  const interactive = cell.selectable;

                  return (
                    <IsometricCell
                      key={cell.date}
                      ref={(node) => {
                        if (node && interactive)
                          cellRefs.current.set(cell.date, node);
                        else cellRefs.current.delete(cell.date);
                      }}
                      role={interactive ? "button" : undefined}
                      tabIndex={
                        interactive && cell.date === tabbableDate ? 0 : -1
                      }
                      aria-hidden={interactive ? undefined : true}
                      aria-label={interactive ? cellTitle(cell) : undefined}
                      aria-current={
                        interactive && selected ? "date" : undefined
                      }
                      aria-pressed={interactive ? selected : undefined}
                      aria-controls={
                        interactive && selected ? breakdownId : undefined
                      }
                      aria-describedby={
                        interactive && active ? tooltipId : undefined
                      }
                      data-contribution-date={
                        cell.inRange ? cell.date : undefined
                      }
                      data-contribution-view={interactive ? "3d" : undefined}
                      $active={active}
                      $selected={selected}
                      onClick={interactive ? () => selectCell(cell) : undefined}
                      onPointerEnter={(event) =>
                        interactive && handleCellPointerEnter(cell, event)
                      }
                      onPointerLeave={handleCellPointerLeave}
                      onFocus={(event) =>
                        interactive && handleCellFocus(cell, event)
                      }
                      onBlur={() => setTooltip(null)}
                      onKeyDown={(event) =>
                        interactive && handleCellKeyDown(cell, event)
                      }
                    >
                      <IsometricSide
                        points={faces.left}
                        $darkColor={shadeContributionColor(colors.dark, 58)}
                        $lightColor={shadeContributionColor(colors.light, 58)}
                      />
                      <IsometricSide
                        points={faces.right}
                        $darkColor={shadeContributionColor(colors.dark, 72)}
                        $lightColor={shadeContributionColor(colors.light, 72)}
                      />
                      <IsometricTop
                        points={faces.top}
                        $active={active}
                        $darkColor={colors.dark}
                        $lightColor={colors.light}
                        $selected={selected}
                      />
                    </IsometricCell>
                  );
                })}
              </IsometricSvg>
            </IsometricBody>
          )}
          {tooltip && (
            <CellTooltip
              id={tooltipId}
              role="tooltip"
              data-contribution-tooltip
              $left={tooltip.left}
              $top={tooltip.top}
            >
              <ContributionDayTooltip day={tooltip.day} />
            </CellTooltip>
          )}
          <Footer>
            <PaletteControl>
              <span>Color</span>
              <PalettePreview aria-hidden="true">
                {([1, 2, 3, 4] as const).map((level) => {
                  const colors = getContributionColors(palette, level);
                  return (
                    <PalettePreviewSwatch
                      key={level}
                      $darkColor={colors.dark}
                      $lightColor={colors.light}
                    />
                  );
                })}
              </PalettePreview>
              <PaletteSelect
                name="profile-contribution-palette"
                aria-label="Contribution graph color"
                value={paletteName}
                onChange={(event) =>
                  commitPalette(event.currentTarget.value as ColorPaletteName)
                }
              >
                {getPaletteNames().map((name) => (
                  <option key={name} value={name}>
                    {colorPalettes[name].name}
                  </option>
                ))}
              </PaletteSelect>
            </PaletteControl>
            <Legend aria-label="Contribution intensity, low to high">
              <span>Low</span>
              <LegendSwatches>
                {[0, 1, 2, 3, 4].map((level) => {
                  const colors = getContributionColors(
                    palette,
                    level as ContributionCell["intensity"],
                  );
                  return (
                    <LegendSwatch
                      key={level}
                      $darkColor={colors.dark}
                      $lightColor={colors.light}
                    />
                  );
                })}
              </LegendSwatches>
              <span>High</span>
            </Legend>
          </Footer>
          {showBreakdown && selectedDay && (
            <ContributionDayBreakdown
              day={selectedDay}
              id={breakdownId}
              onClose={closeSelectedDay}
              palette={palette}
            />
          )}
        </>
      ) : (
        <EmptyState>No contribution data is available.</EmptyState>
      )}
      <VisuallyHidden id={calendarInstructionsId}>
        Use arrow keys to inspect adjacent days, Home and End to jump to the
        range boundaries, Enter or Space to select the detailed day breakdown,
        and Escape to close the floating tooltip.
      </VisuallyHidden>
    </Figure>
  );
}
