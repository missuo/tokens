"use client";

import type { TokenContributionData, GraphColorPalette } from "@/lib/types";
import { formatCurrency, formatTokenCount, calculateCurrentStreak, calculateLongestStreak, findBestDay } from "@/lib/utils";
import { formatContributionDate } from "@/lib/date-utils";
import { formatDuration } from "@/lib/format";

interface StatsPanelProps {
  data: TokenContributionData;
  palette: GraphColorPalette;
  totalActiveTimeMs?: number | null;
  sessionCount?: number | null;
  mcpServers?: string[];
}

export function StatsPanel({ data, palette, totalActiveTimeMs, sessionCount, mcpServers }: StatsPanelProps) {
  const { summary, contributions } = data;
  const currentStreak = calculateCurrentStreak(contributions);
  const longestStreak = calculateLongestStreak(contributions);
  const bestDay = findBestDay(contributions);

  return (
    <div className="rounded-2xl border border-line bg-surface p-4 sm:p-6">
      <h3 className="mb-4 text-xs font-semibold tracking-wider text-muted uppercase">Statistics</h3>

      <div className="grid grid-cols-2 gap-x-4 gap-y-5 sm:grid-cols-3 sm:gap-6 md:grid-cols-4">
        <StatItem label="Total Cost" value={formatCurrency(summary.totalCost)} highlight />
        <StatItem label="Total Tokens" value={formatTokenCount(summary.totalTokens)} />
        <StatItem label="Active Days" value={`${summary.activeDays} / ${summary.totalDays}`} />
        <StatItem label="Avg / Day" value={formatCurrency(summary.averagePerDay)} />
        <StatItem label="Current Streak" value={`${currentStreak} day${currentStreak !== 1 ? "s" : ""}`} />
        <StatItem label="Longest Streak" value={`${longestStreak} day${longestStreak !== 1 ? "s" : ""}`} />
        {bestDay && bestDay.totals.cost > 0 && (
          <StatItem label="Best Day" value={formatContributionDate(bestDay)} subValue={formatCurrency(bestDay.totals.cost)} />
        )}
        <StatItem label="Models" value={summary.models.length.toString()} />
        {totalActiveTimeMs != null && totalActiveTimeMs > 0 && <StatItem label="Active Time" value={formatDuration(totalActiveTimeMs)} />}
        {sessionCount != null && sessionCount > 0 && <StatItem label="Sessions" value={sessionCount.toString()} />}
      </div>

      <div className="mt-6 flex flex-wrap items-center gap-2 border-t border-line pt-6">
        <span className="mr-3 text-xs font-semibold tracking-wider text-muted uppercase max-[480px]:mr-0 max-[480px]:w-full">Clients:</span>
        {summary.clients.map((client) => (
          <span
            key={client}
            className="max-w-full min-w-0 truncate rounded-full px-3 py-1.5 text-xs font-medium text-foreground transition hover:scale-105"
            style={{ backgroundColor: `${palette.grade3}20` }}
          >
            {client}
          </span>
        ))}
      </div>
      {mcpServers && mcpServers.length > 0 && (
        <div className="mt-6 flex flex-wrap items-center gap-2 border-t border-line pt-6">
          <span className="mr-3 text-xs font-semibold tracking-wider text-muted uppercase max-[480px]:mr-0 max-[480px]:w-full">MCPs:</span>
          {mcpServers.map((server) => (
            <span
              key={server}
              className="max-w-full min-w-0 truncate rounded-full px-3 py-1.5 text-xs font-medium text-foreground transition hover:scale-105"
              style={{ backgroundColor: `${palette.grade3}20` }}
            >
              {server}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

interface StatItemProps {
  label: string;
  value: string;
  subValue?: string;
  highlightColor?: string;
  highlight?: boolean;
}

function StatItem({ label, value, subValue, highlight }: StatItemProps) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <div className="text-[11px] font-semibold tracking-wider text-muted uppercase [overflow-wrap:anywhere]">{label}</div>
      <div
        className={`min-w-0 font-mono font-semibold tracking-tight tabular-nums [overflow-wrap:anywhere] ${
          highlight ? "text-xl text-accent max-[400px]:text-lg" : "text-lg text-foreground max-[400px]:text-base"
        }`}
      >
        {value}
      </div>
      {subValue && <div className="font-mono text-xs font-medium text-muted tabular-nums">{subValue}</div>}
    </div>
  );
}
