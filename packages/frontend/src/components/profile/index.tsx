"use client";

import React, { useState } from "react";
import { Button } from "@heroui/react";
import { toast } from "react-toastify";
import { GraphContainer } from "@/components/GraphContainer";
import type { TokenContributionData } from "@/lib/types";
import { formatNumber, formatCurrency, formatDuration, formatDateFull } from "@/lib/utils";
import type { DailyContribution } from "@/lib/types";
import { ProfileEmbedDialog } from "./ProfileEmbedDialog";

export interface ProfileUser {
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  rank: number | null;
}

export interface ProfileStatsData {
  totalTokens: number;
  totalCost: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  activeDays: number;
  submissionCount?: number;
  totalActiveTimeMs?: number;
  sessionCount?: number;
}

export interface ProfileHeaderProps {
  user: ProfileUser;
  stats: ProfileStatsData;
  lastUpdated?: string;
}

const EmbedIcon: React.FC<React.SVGProps<SVGSVGElement>> = (props) => (
  <svg aria-hidden="true" width="20" height="20" viewBox="0 0 24 24" fill="none" {...props}>
    <path d="M8 8L4 12L8 16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M16 8L20 12L16 16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M13.5 5L10.5 19" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

// Inline SVGs (currentColor) so they inherit the button's text color and stay
// visible in both light and dark themes — the old /icons/*.svg files had a
// hardcoded white stroke that vanished on light backgrounds.
const ShareIcon: React.FC<React.SVGProps<SVGSVGElement>> = (props) => (
  <svg aria-hidden="true" width="20" height="20" viewBox="0 0 20 20" fill="none" {...props}>
    <path
      d="M7.15833 11.2583L12.85 14.575M12.8417 5.42499L7.15833 8.74166M17.5 4.16666C17.5 5.54737 16.3807 6.66666 15 6.66666C13.6193 6.66666 12.5 5.54737 12.5 4.16666C12.5 2.78594 13.6193 1.66666 15 1.66666C16.3807 1.66666 17.5 2.78594 17.5 4.16666ZM7.5 9.99999C7.5 11.3807 6.38071 12.5 5 12.5C3.61929 12.5 2.5 11.3807 2.5 9.99999C2.5 8.61928 3.61929 7.49999 5 7.49999C6.38071 7.49999 7.5 8.61928 7.5 9.99999ZM17.5 15.8333C17.5 17.214 16.3807 18.3333 15 18.3333C13.6193 18.3333 12.5 17.214 12.5 15.8333C12.5 14.4526 13.6193 13.3333 15 13.3333C16.3807 13.3333 17.5 14.4526 17.5 15.8333Z"
      stroke="currentColor"
      strokeWidth="1.667"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const GitHubIcon: React.FC<React.SVGProps<SVGSVGElement>> = (props) => (
  <svg aria-hidden="true" width="20" height="20" viewBox="0 0 20 20" fill="none" {...props}>
    <path
      d="M12.5 18.3333V15C12.6159 13.9561 12.3166 12.9084 11.6666 12.0833C14.1666 12.0833 16.6666 10.4167 16.6666 7.49999C16.7333 6.45832 16.4416 5.43332 15.8333 4.58332C16.0666 3.62499 16.0666 2.62499 15.8333 1.66666C15.8333 1.66666 15 1.66666 13.3333 2.91666C11.1333 2.49999 8.86663 2.49999 6.66663 2.91666C4.99996 1.66666 4.16663 1.66666 4.16663 1.66666C3.91663 2.62499 3.91663 3.62499 4.16663 4.58332C3.55985 5.42989 3.26535 6.46065 3.33329 7.49999C3.33329 10.4167 5.83329 12.0833 8.33329 12.0833C8.00829 12.4917 7.76663 12.9583 7.62496 13.4583C7.48329 13.9583 7.44163 14.4833 7.49996 15M7.49996 15V18.3333M7.49996 15C3.74163 16.6667 3.33329 13.3333 1.66663 13.3333"
      stroke="currentColor"
      strokeWidth="1.667"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

export function ProfileHeader({ user, stats, lastUpdated }: ProfileHeaderProps) {
  const [isEmbedDialogOpen, setIsEmbedDialogOpen] = useState(false);
  const avatarUrl = user.avatarUrl || `https://github.com/${user.username}.png`;

  const handleShareClick = async () => {
    try {
      await navigator.clipboard.writeText(window.location.href);
      toast.success("Link copied to clipboard!");
    } catch {
      toast.error("Failed to copy link");
    }
  };

  const ghostBtn =
    "inline-flex h-9 items-center gap-1.5 rounded-lg border border-line bg-surface px-3 text-sm font-medium text-foreground transition hover:border-foreground/20 hover:bg-surface-secondary";

  return (
    <div className="rounded-2xl border border-line bg-surface p-4 sm:p-5">
      <div className="flex flex-col gap-5 sm:flex-row sm:items-center sm:justify-between">
        {/* Identity */}
        <div className="flex items-center gap-4">
          <div className="relative h-[68px] w-[68px] shrink-0 overflow-hidden rounded-xl ring-1 ring-line sm:h-20 sm:w-20">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src={avatarUrl} alt={user.username} className="h-full w-full object-cover" />
          </div>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="truncate text-xl leading-tight font-bold text-foreground sm:text-2xl">{user.displayName || user.username}</h1>
              {user.rank != null && (
                <span className="inline-flex items-center rounded-md border border-accent/30 bg-accent/10 px-2 py-0.5 font-mono text-xs font-semibold text-accent tabular-nums">
                  #{user.rank}
                </span>
              )}
            </div>
            <p className="mt-1 font-mono text-sm text-muted">@{user.username}</p>
            {lastUpdated && <p className="mt-1.5 text-xs text-muted">Updated {new Date(lastUpdated).toLocaleString()}</p>}
          </div>
        </div>

        {/* Metrics */}
        <div className="flex items-center gap-8 sm:gap-10">
          <div className="flex flex-col gap-1">
            <span className="text-[11px] font-semibold tracking-wider text-muted uppercase">Total Tokens</span>
            <span className="font-mono text-2xl font-semibold text-accent tabular-nums" title={stats.totalTokens.toLocaleString()}>
              {formatNumber(stats.totalTokens)}
            </span>
          </div>
          <div className="flex flex-col gap-1">
            <span className="text-[11px] font-semibold tracking-wider text-muted uppercase">Total Cost</span>
            <span className="font-mono text-2xl font-semibold text-foreground tabular-nums" title={stats.totalCost.toLocaleString("en-US", { style: "currency", currency: "USD" })}>
              {formatCurrency(stats.totalCost)}
            </span>
          </div>
        </div>
      </div>

      <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-line pt-4">
        <Button
          onPress={() => setIsEmbedDialogOpen(true)}
          aria-label={`Open GitHub README embed options for ${user.displayName || user.username}`}
          className="h-9 rounded-lg bg-accent px-3 text-sm font-semibold text-accent-foreground"
        >
          <EmbedIcon width={18} height={18} />
          <span className="leading-none">Embed</span>
        </Button>

        <button type="button" onClick={handleShareClick} aria-label={`Share ${user.displayName || user.username}'s profile`} className={ghostBtn}>
          <ShareIcon width={18} height={18} />
          <span className="leading-none">Share</span>
        </button>

        <a
          href={`https://github.com/${user.username}`}
          target="_blank"
          rel="noopener noreferrer"
          aria-label={`View ${user.username}'s GitHub profile (opens in new tab)`}
          className={ghostBtn}
        >
          <GitHubIcon width={18} height={18} />
          <span className="leading-none">GitHub</span>
        </a>
      </div>

      <ProfileEmbedDialog open={isEmbedDialogOpen} username={user.username} displayName={user.displayName} onClose={() => setIsEmbedDialogOpen(false)} />
    </div>
  );
}

export type ProfileTab = "activity" | "breakdown" | "models";

export interface ProfileTabBarProps {
  activeTab: ProfileTab;
  onTabChange: (tab: ProfileTab) => void;
}

export function ProfileTabBar({ activeTab, onTabChange }: ProfileTabBarProps) {
  const tabs: { id: ProfileTab; label: string }[] = [
    { id: "activity", label: "Activity" },
    { id: "breakdown", label: "Token Breakdown" },
    { id: "models", label: "Models Used" },
  ];

  const handleKeyDown = (e: React.KeyboardEvent, currentIndex: number) => {
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      onTabChange(tabs[(currentIndex + 1) % tabs.length].id);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      onTabChange(tabs[(currentIndex - 1 + tabs.length) % tabs.length].id);
    } else if (e.key === "Home") {
      e.preventDefault();
      onTabChange(tabs[0].id);
    } else if (e.key === "End") {
      e.preventDefault();
      onTabChange(tabs[tabs.length - 1].id);
    }
  };

  return (
    <div role="tablist" aria-label="Profile tabs" className="no-scrollbar flex w-full items-center gap-1 overflow-x-auto rounded-lg border border-line bg-surface-secondary p-1 sm:w-fit">
      {tabs.map((tab, index) => {
        const isActive = activeTab === tab.id;
        return (
          <button
            key={tab.id}
            id={`tab-${tab.id}`}
            role="tab"
            aria-selected={isActive}
            aria-controls={`tabpanel-${tab.id}`}
            tabIndex={isActive ? 0 : -1}
            onClick={() => onTabChange(tab.id)}
            onKeyDown={(e) => handleKeyDown(e, index)}
            className={`flex flex-1 shrink-0 items-center justify-center rounded-md px-3.5 py-1.5 text-sm font-medium whitespace-nowrap transition-colors outline-none focus-visible:ring-2 focus-visible:ring-accent sm:flex-none ${
              isActive ? "bg-surface text-foreground shadow-sm ring-1 ring-line" : "text-muted hover:text-foreground"
            }`}
          >
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}

export interface TokenBreakdownProps {
  stats: ProfileStatsData;
}

export function TokenBreakdown({ stats }: TokenBreakdownProps) {
  const { totalTokens, inputTokens, outputTokens, cacheReadTokens, cacheWriteTokens } = stats;

  const tokenTypes = [
    { label: "Input", value: inputTokens, color: "#006edb", percentage: totalTokens > 0 ? (inputTokens / totalTokens) * 100 : 0 },
    { label: "Output", value: outputTokens, color: "#894ceb", percentage: totalTokens > 0 ? (outputTokens / totalTokens) * 100 : 0 },
    { label: "Cache Read", value: cacheReadTokens, color: "#30a147", percentage: totalTokens > 0 ? (cacheReadTokens / totalTokens) * 100 : 0 },
    { label: "Cache Write", value: cacheWriteTokens, color: "#eb670f", percentage: totalTokens > 0 ? (cacheWriteTokens / totalTokens) * 100 : 0 },
  ];

  return (
    <div className="rounded-2xl border border-line bg-surface p-4 sm:p-6">
      {totalTokens > 0 && (
        <div className="mb-6">
          <div className="flex h-2.5 overflow-hidden rounded-full bg-surface-secondary">
            {tokenTypes.map((type) => (
              <div key={type.label} style={{ width: `${type.percentage}%`, backgroundColor: type.color }} title={`${type.label}: ${formatNumber(type.value)}`} />
            ))}
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        {tokenTypes.map((type) => (
          <div key={type.label} className="flex items-start gap-2.5">
            <div className="mt-1.5 h-2.5 w-2.5 shrink-0 rounded-full" style={{ backgroundColor: type.color }} />
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <p className="text-xs text-muted">{type.label}</p>
                {type.percentage > 0 && <span className="font-mono text-xs text-muted/70 tabular-nums">{type.percentage.toFixed(1)}%</span>}
              </div>
              <p className="font-mono text-base font-semibold text-foreground tabular-nums sm:text-lg">{formatNumber(type.value)}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export interface ProfileStatsProps {
  stats: ProfileStatsData;
  favoriteModel?: string;
}

export function ProfileStats({ stats, favoriteModel }: ProfileStatsProps) {
  const statItems = [
    { label: "Submits", value: (stats.submissionCount ?? 0).toString() },
    { label: "Favorite Model", value: favoriteModel ?? "N/A" },
    ...(stats.totalActiveTimeMs && stats.totalActiveTimeMs > 0 ? [{ label: "Active Time", value: formatDuration(stats.totalActiveTimeMs) }] : []),
    ...(stats.sessionCount && stats.sessionCount > 0 ? [{ label: "Sessions", value: stats.sessionCount.toString() }] : []),
  ];

  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
      {statItems.map((item) => (
        <div key={item.label} className="flex flex-col rounded-xl border border-line bg-surface px-4 py-3.5">
          <p className="text-[11px] font-semibold tracking-wider text-muted uppercase">{item.label}</p>
          <p className="mt-1 truncate font-mono text-lg font-semibold text-foreground tabular-nums" title={item.value}>{item.value}</p>
        </div>
      ))}
    </div>
  );
}

const WEEKDAY_NAMES = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
// Display order is Monday-first; each entry is the JS getDay() index + a compact axis label.
const WEEKDAY_ORDER: { idx: number; short: string }[] = [
  { idx: 1, short: "Mo" },
  { idx: 2, short: "Tu" },
  { idx: 3, short: "We" },
  { idx: 4, short: "Th" },
  { idx: 5, short: "Fr" },
  { idx: 6, short: "Sa" },
  { idx: 0, short: "Su" },
];

// Weekday of a "YYYY-MM-DD" calendar label, parsed at local midnight so the
// result matches the labelled date regardless of the viewer's timezone.
function weekdayIndexOf(date: string): number {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).getDay();
}

export interface ProfileHabitsProps {
  contributions: DailyContribution[];
}

// "Coding Patterns" — fun, accurate stats derived purely from the per-day
// contribution data: which weekday you ship the most on, and your single
// biggest day. (Time-of-day isn't shown because the pipeline only stores
// per-UTC-day totals, not hourly buckets.)
export function ProfileHabits({ contributions }: ProfileHabitsProps) {
  const weekdayTokens = [0, 0, 0, 0, 0, 0, 0];
  let totalTokens = 0;
  let biggestDay: DailyContribution | null = null;

  for (const day of contributions) {
    const tokens = day.totals.tokens;
    if (tokens <= 0) continue;
    weekdayTokens[weekdayIndexOf(day.date)] += tokens;
    totalTokens += tokens;
    if (!biggestDay || tokens > biggestDay.totals.tokens) biggestDay = day;
  }

  // No real activity yet — nothing meaningful to show.
  if (totalTokens <= 0 || !biggestDay) return null;

  const topWeekdayIdx = weekdayTokens.indexOf(Math.max(...weekdayTokens));
  const topWeekdayTokens = weekdayTokens[topWeekdayIdx];
  const topWeekdayShare = totalTokens > 0 ? (topWeekdayTokens / totalTokens) * 100 : 0;
  const maxWeekdayTokens = topWeekdayTokens;

  return (
    <div className="rounded-2xl border border-line bg-surface p-4 sm:p-6">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold text-foreground">Coding Patterns</h3>
        <span className="text-[11px] font-semibold tracking-wider text-muted uppercase">Last 12 months</span>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
        {/* #2 — most-active weekday */}
        <div className="rounded-xl border border-line bg-surface-secondary p-4">
          <p className="text-[11px] font-semibold tracking-wider text-muted uppercase">Most productive day</p>
          <p className="mt-1 text-lg font-semibold text-foreground">{WEEKDAY_NAMES[topWeekdayIdx]}</p>
          <p className="mt-0.5 font-mono text-xs text-muted tabular-nums" title={topWeekdayTokens.toLocaleString()}>
            {formatNumber(topWeekdayTokens)} tokens · {topWeekdayShare.toFixed(0)}% of total
          </p>
        </div>

        {/* #3 — biggest single day */}
        <div className="rounded-xl border border-line bg-surface-secondary p-4">
          <p className="text-[11px] font-semibold tracking-wider text-muted uppercase">Biggest day</p>
          <p className="mt-1 font-mono text-lg font-semibold text-accent tabular-nums" title={biggestDay.totals.tokens.toLocaleString()}>
            {formatNumber(biggestDay.totals.tokens)} tokens
          </p>
          <p className="mt-0.5 font-mono text-xs text-muted tabular-nums">
            {formatDateFull(biggestDay.date)} · {formatCurrency(biggestDay.totals.cost)}
          </p>
        </div>
      </div>

      {/* Weekday distribution */}
      <div className="mt-4">
        <p className="mb-2 text-[11px] font-semibold tracking-wider text-muted uppercase">Tokens by weekday</p>
        <div className="flex items-end gap-1.5 sm:gap-2">
          {WEEKDAY_ORDER.map(({ idx, short }) => {
            const tokens = weekdayTokens[idx];
            const isTop = idx === topWeekdayIdx;
            const heightPct = maxWeekdayTokens > 0 ? Math.max((tokens / maxWeekdayTokens) * 100, tokens > 0 ? 6 : 2) : 2;
            return (
              <div key={idx} className="flex flex-1 flex-col items-center gap-1.5">
                <div className="flex h-16 w-full items-end">
                  <div
                    className={`w-full rounded-md transition-all ${isTop ? "bg-accent" : "bg-foreground/15"}`}
                    style={{ height: `${heightPct}%` }}
                    title={`${WEEKDAY_NAMES[idx]}: ${formatNumber(tokens)} tokens`}
                  />
                </div>
                <span className={`text-[10px] font-medium tabular-nums ${isTop ? "text-foreground" : "text-muted"}`}>{short}</span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

const MODEL_COLORS: Record<string, string> = {
  claude: "#D97706",
  sonnet: "#D97706",
  opus: "#DC2626",
  haiku: "#059669",
  gpt: "#10B981",
  o1: "#6366F1",
  o3: "#8B5CF6",
  gemini: "#3B82F6",
  deepseek: "#06B6D4",
  codex: "#F59E0B",
  kimi: "#A855F7",
  qwen: "#1A73E8",
};

function getModelColor(modelName: string): string {
  const lowerName = modelName.toLowerCase();
  for (const [key, color] of Object.entries(MODEL_COLORS)) {
    if (lowerName.includes(key)) return color;
  }
  return "#6B7280";
}

export interface ModelUsage {
  model: string;
  tokens: number;
  cost: number;
  percentage: number;
}

export interface ProfileModelsProps {
  models: string[];
  modelUsage?: ModelUsage[];
}

export function ProfileModels({ models, modelUsage }: ProfileModelsProps) {
  const filteredModels = models.filter((m) => m !== "<synthetic>");
  if (filteredModels.length === 0) return null;

  if (modelUsage && modelUsage.length > 0) {
    const sortedUsage = [...modelUsage].sort((a, b) => b.cost - a.cost);

    return (
      <div className="overflow-hidden rounded-2xl border border-line bg-surface">
        <div className="grid grid-cols-[1fr_auto_auto] gap-3 border-b border-line bg-surface-secondary px-3 py-3 text-xs font-medium tracking-wider text-muted uppercase sm:px-6 min-[480px]:grid-cols-[1fr_auto_auto_auto] min-[480px]:gap-4 min-[480px]:px-4">
          <div>Model</div>
          <div className="w-20 text-right sm:w-24">Tokens</div>
          <div className="w-16 text-right sm:w-20">Cost</div>
          <div className="hidden w-12 text-right sm:w-16 min-[480px]:block">%</div>
        </div>

        <div>
          {sortedUsage.map((usage, index) => (
            <div
              key={usage.model}
              className="grid grid-cols-[1fr_auto_auto] items-center gap-3 px-3 py-3 sm:px-6 min-[480px]:grid-cols-[1fr_auto_auto_auto] min-[480px]:gap-4 min-[480px]:px-4"
              style={{ backgroundColor: index % 2 === 1 ? "var(--surface-secondary)" : "transparent", borderTop: index > 0 ? "1px solid var(--border)" : undefined }}
            >
              <div className="flex min-w-0 items-center gap-2">
                <div className="h-2 w-2 shrink-0 rounded-full" style={{ backgroundColor: getModelColor(usage.model) }} />
                <span className="truncate text-[13px] font-medium text-foreground sm:text-sm">{usage.model}</span>
              </div>
              <div className="w-20 text-right sm:w-24">
                <span className="font-mono text-[13px] text-foreground tabular-nums sm:text-sm">{formatNumber(usage.tokens)}</span>
              </div>
              <div className="w-16 text-right sm:w-20">
                <span className="font-mono text-[13px] font-medium text-accent tabular-nums sm:text-sm">{formatCurrency(usage.cost)}</span>
              </div>
              <div className="hidden w-12 text-right sm:w-16 min-[480px]:block">
                <span className="font-mono text-[13px] text-muted tabular-nums sm:text-sm">{usage.percentage.toFixed(1)}%</span>
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-2xl border border-line bg-surface p-4 sm:p-6">
      <div className="flex flex-wrap gap-2">
        {filteredModels.map((model) => (
          <span key={model} className="flex items-center gap-2 rounded-full bg-surface-secondary px-3 py-1.5 text-sm font-medium text-foreground">
            <div className="h-2 w-2 shrink-0 rounded-full" style={{ backgroundColor: getModelColor(model) }} />
            {model}
          </span>
        ))}
      </div>
    </div>
  );
}

export interface ProfileActivityProps {
  data: TokenContributionData;
  totalActiveTimeMs?: number | null;
  sessionCount?: number | null;
}

export function ProfileActivity({ data, totalActiveTimeMs, sessionCount }: ProfileActivityProps) {
  // GraphContainer renders the graph card *plus* the breakdown and statistics
  // panels. The contribution canvas scrolls horizontally on its own (see
  // TokenGraph2D), so we must NOT wrap the whole container in a min-width box —
  // doing so forced the breakdown/stats panels off-screen on mobile.
  return <GraphContainer data={data} totalActiveTimeMs={totalActiveTimeMs} sessionCount={sessionCount} />;
}

export function ProfileEmptyActivity() {
  return (
    <div className="rounded-2xl border border-line bg-surface p-6 text-center sm:p-8">
      <p className="text-sm text-muted sm:text-base">No contribution data available yet.</p>
    </div>
  );
}
