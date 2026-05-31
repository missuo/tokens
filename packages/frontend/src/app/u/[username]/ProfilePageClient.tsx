"use client";

import { useState, useMemo } from "react";
import {
  ProfileHeader,
  ProfileTabBar,
  TokenBreakdown,
  ProfileModels,
  ProfileActivity,
  ProfileEmptyActivity,
  ProfileStats,
  ProfileHabits,
  type ProfileUser,
  type ProfileStatsData,
  type ProfileTab,
  type ModelUsage,
} from "@/components/profile";
import type { TokenContributionData, DailyContribution, ClientType } from "@/lib/types";

interface ProfileData {
  user: {
    id: string;
    username: string;
    displayName: string | null;
    avatarUrl: string | null;
    createdAt: string;
    rank: number | null;
  };
  stats: {
    totalTokens: number;
    totalCost: number;
    inputTokens: number;
    outputTokens: number;
    cacheReadTokens: number;
    cacheWriteTokens: number;
    submissionCount: number;
    activeDays: number;
    totalActiveTimeMs: number;
    sessionCount: number;
  };
  dateRange: {
    start: string | null;
    end: string | null;
  };
  updatedAt: string | null;
  clients: string[];
  models: string[];
  modelUsage?: ModelUsage[];
  contributions: DailyContribution[];
}

interface ProfilePageClientProps {
  initialData: ProfileData;
  username: string;
}

export default function ProfilePageClient({ initialData }: ProfilePageClientProps) {
  const [activeTab, setActiveTab] = useState<ProfileTab>("activity");
  const data = initialData;

  const graphData: TokenContributionData | null = useMemo(() => {
    if (!data || data.contributions.length === 0) return null;

    const contributions = data.contributions;
    const totalCost = data.stats.totalCost;
    const totalTokens = data.stats.totalTokens;
    const maxCost = Math.max(...contributions.map((c) => c.totals.cost), 0);

    const yearMap = new Map<string, { totalTokens: number; totalCost: number; start: string; end: string }>();
    for (const day of contributions) {
      const year = day.date.split("-")[0];
      const existing = yearMap.get(year);
      if (existing) {
        existing.totalTokens += day.totals.tokens;
        existing.totalCost += day.totals.cost;
        if (day.date < existing.start) existing.start = day.date;
        if (day.date > existing.end) existing.end = day.date;
      } else {
        yearMap.set(year, {
          totalTokens: day.totals.tokens,
          totalCost: day.totals.cost,
          start: day.date,
          end: day.date,
        });
      }
    }

    const years = Array.from(yearMap.entries())
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([year, stats]) => ({
        year,
        totalTokens: stats.totalTokens,
        totalCost: stats.totalCost,
        range: { start: stats.start, end: stats.end },
      }));

    return {
      meta: {
        generatedAt: new Date().toISOString(),
        version: "1.0.0",
        dateRange: {
          start: data.dateRange.start || contributions[0]?.date || "",
          end: data.dateRange.end || contributions[contributions.length - 1]?.date || "",
        },
      },
      summary: {
        totalTokens,
        totalCost,
        totalDays: contributions.length,
        activeDays: data.stats.activeDays,
        averagePerDay: data.stats.activeDays > 0 ? totalCost / data.stats.activeDays : 0,
        maxCostInSingleDay: maxCost,
        clients: data.clients as ClientType[],
        models: data.models,
      },
      years,
      contributions: contributions as DailyContribution[],
    };
  }, [data]);

  const user: ProfileUser = useMemo(() => ({
    username: data.user.username,
    displayName: data.user.displayName,
    avatarUrl: data.user.avatarUrl,
    rank: data.user.rank,
  }), [data]);

  const stats: ProfileStatsData = useMemo(() => ({
    totalTokens: data.stats.totalTokens,
    totalCost: data.stats.totalCost,
    inputTokens: data.stats.inputTokens,
    outputTokens: data.stats.outputTokens,
    cacheReadTokens: data.stats.cacheReadTokens,
    cacheWriteTokens: data.stats.cacheWriteTokens,
    activeDays: data.stats.activeDays,
    submissionCount: data.stats.submissionCount,
    totalActiveTimeMs: data.stats.totalActiveTimeMs,
    sessionCount: data.stats.sessionCount,
  }), [data]);

const EARLY_ADOPTERS = ["code-yeongyu", "gtg7784", "qodot"];
  const showResubmitBanner = EARLY_ADOPTERS.includes(data.user.username) && data.stats.submissionCount === 1;

  return (
    <div className="flex min-h-screen flex-col bg-background">

      {showResubmitBanner && (
        <div className="border-b border-[rgba(245,158,11,0.2)] bg-[rgba(245,158,11,0.1)]">
          <div className="mx-auto max-w-[800px] px-4 py-3 sm:px-6">
            <p className="text-sm text-[#fde68a]">
              <span className="font-semibold">Update available:</span> If you&apos;re <span className="font-semibold">@{data.user.username}</span>, please re-submit your data with{" "}
              <code className="rounded bg-[rgba(245,158,11,0.2)] px-1.5 py-0.5 font-mono text-xs">tokens submit</code> to see detailed model breakdowns per day.
            </p>
          </div>
        </div>
      )}

      <main className="mx-auto w-full max-w-[800px] flex-1 px-4 py-6 sm:px-6 sm:py-10">
        <div className="flex flex-col gap-8">
          <ProfileHeader user={user} stats={stats} lastUpdated={data.updatedAt || undefined} />

          <ProfileTabBar activeTab={activeTab} onTabChange={setActiveTab} />

          {activeTab === "activity" && (
            <div role="tabpanel" id="tabpanel-activity" aria-labelledby="tab-activity">
              {graphData ? (
                <div className="flex flex-col gap-6">
                  <ProfileActivity data={graphData} totalActiveTimeMs={data.stats.totalActiveTimeMs} sessionCount={data.stats.sessionCount} />
                  <ProfileStats
                    stats={stats}
                    favoriteModel={data.modelUsage?.reduce((max, current) => (current.cost > max.cost ? current : max), data.modelUsage[0])?.model}
                  />
                  <ProfileHabits contributions={data.contributions} />
                </div>
              ) : (
                <ProfileEmptyActivity />
              )}
            </div>
          )}
          {activeTab === "breakdown" && (
            <div role="tabpanel" id="tabpanel-breakdown" aria-labelledby="tab-breakdown">
              <TokenBreakdown stats={stats} />
            </div>
          )}
          {activeTab === "models" && (
            <div role="tabpanel" id="tabpanel-models" aria-labelledby="tab-models">
              <ProfileModels models={data.models} modelUsage={data.modelUsage} />
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
