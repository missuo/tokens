"use client";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { CONTAINER } from "@/components/layout/Container";
import { ProfileView } from "@/components/profile/ProfileView";
import { ProfileToday } from "@/components/profile/ProfileToday";
import { ProfileEmbedDialog } from "@/components/profile/ProfileEmbedDialog";
import { cn } from "@/lib/utils";

import { useEffect, useId, useMemo, useState } from "react";
import { useRouter } from "nextjs-toploader/app";
import {
  createContributionRangeOptions,
  getContributionDayForDate,
  getDefaultContributionDate,
  ProfileContributionBreakdown,
  ProfileContributionGraph,
  ProfileDevices,
  ProfileHabits,
  ProfileModels,
  ProfileUsageChart,
  reconcileContributionSelectionRange,
  resolveContributionRange,
  resolveContributionSelectedDate,
  TokenBreakdown,
  type ModelUsage,
  type ProfileDevice,
  type ProfileStatsData,
  type ProfileContributionView,
  type ProfileSocialLink,
  type ProfileUser,
} from "@/components/profile";
import type { DailyContribution } from "@/lib/types";
import { isVerifiedBySocialLinks } from "@/lib/socialVerification";
import { toLocalDateString } from "@/lib/leaderboard/dateRange";
import { useSettings } from "@/lib/useSettings";

type ProfilePeriod = "all" | "week" | "month";

export interface ProfileData {
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
    reasoningTokens?: number;
    submissionCount: number;
    activeDays: number;
    sessionCount: number;
  };
  dateRange: {
    start: string | null;
    end: string | null;
  };
  chartRange?: {
    start: string | null;
    end: string | null;
  };
  updatedAt: string | null;
  clients: string[];
  models: string[];
  mcpServers?: string[];
  /** True once any accepted submission carried a backfill provenance tag. */
  hasBackfill?: boolean;
  modelUsage?: ModelUsage[];
  contributions: DailyContribution[];
  period?: ProfilePeriod;
}

function getProfileChartRange(
  period: ProfilePeriod,
  apiRange: ProfileData["dateRange"],
  chartRange: ProfileData["chartRange"],
): { start: string | null; end: string | null } {
  return period === "all" ? (chartRange ?? apiRange) : apiRange;
}

interface ProfilePageClientProps {
  initialData: ProfileData;
  initialDevices?: ProfileDevice[];
  socialLinks?: ProfileSocialLink[];
  username: string;
}

const EARLY_ADOPTERS = ["code-yeongyu", "gtg7784", "qodot"];

export default function ProfilePageClient({
  initialData,
  initialDevices,
  socialLinks,
  username,
}: ProfilePageClientProps) {
  const router = useRouter();
  const [isEmbedOpen, setIsEmbedOpen] = useState(false);

  const [contributionView, setContributionView] =
    useState<ProfileContributionView>("2d");
  const {
    paletteName: contributionPalette,
    setPalette: setContributionPalette,
  } = useSettings();
  const contributionBreakdownId = useId();
  const data = initialData;

  // The redesigned shell renders models as a table rather than a chart.
  const modelRows = useMemo(
    () =>
      (data.modelUsage ?? []).map((m) => ({
        name: m.model,
        tokens: Number(m.tokens ?? 0),
        cost: Number(m.cost ?? 0),
      })),
    [data.modelUsage]
  );
  const period = data.period ?? "all";
  const rollingChartRange = useMemo(
    () => getProfileChartRange(period, data.dateRange, data.chartRange),
    [period, data.chartRange, data.dateRange],
  );
  const contributionRangeOptions = useMemo(
    () =>
      period === "all"
        ? createContributionRangeOptions(
            data.contributions,
            rollingChartRange.start,
            rollingChartRange.end,
          )
        : [],
    [data.contributions, period, rollingChartRange],
  );
  const [contributionRangeValue, setContributionRangeValue] =
    useState("recent");
  const selectedContributionRange = useMemo(
    () =>
      resolveContributionRange(
        contributionRangeOptions,
        contributionRangeValue,
      ),
    [contributionRangeOptions, contributionRangeValue],
  );
  const chartRange = useMemo(
    () =>
      selectedContributionRange
        ? {
            start: selectedContributionRange.startDate,
            end: selectedContributionRange.endDate,
          }
        : rollingChartRange,
    [rollingChartRange, selectedContributionRange],
  );
  const contributionSelectionEnd = useMemo(() => {
    if (!chartRange.end || !rollingChartRange.end) return chartRange.end;
    return chartRange.end > rollingChartRange.end
      ? rollingChartRange.end
      : chartRange.end;
  }, [chartRange.end, rollingChartRange.end]);
  const contributionRangeIdentity = `${chartRange.start ?? ""}:${chartRange.end ?? ""}`;
  const [contributionSelection, setContributionSelection] = useState<{
    date: string | null;
    rangeIdentity: string;
  }>(() => ({ date: null, rangeIdentity: contributionRangeIdentity }));
  const reconciledContributionSelection = reconcileContributionSelectionRange(
    contributionSelection,
    contributionRangeIdentity,
  );
  if (reconciledContributionSelection !== contributionSelection) {
    setContributionSelection(reconciledContributionSelection);
  }
  const defaultContributionDate = useMemo(
    () =>
      getDefaultContributionDate(
        data.contributions,
        chartRange.start,
        contributionSelectionEnd,
      ),
    [data.contributions, chartRange.start, contributionSelectionEnd],
  );
  const selectedContributionDate = resolveContributionSelectedDate(
    reconciledContributionSelection,
    contributionRangeIdentity,
    defaultContributionDate,
  );
  const selectedContributionDay = useMemo(
    () =>
      getContributionDayForDate(data.contributions, selectedContributionDate),
    [data.contributions, selectedContributionDate],
  );

  // The viewer's own calendar date, not UTC. `toISOString()` would answer in
  // UTC, so anywhere west of it the current local day stops being called Today
  // as soon as UTC rolls over — 8pm in New York, with four hours of the local
  // day still to go. Contribution days are calendar dates in each submitter's
  // pinned bucketing timezone, so the comparison that means anything here is
  // against another calendar date, not against an instant.
  //
  // Read after mount rather than during render: the clock gives the server and
  // the browser different answers across a timezone boundary, which would be a
  // hydration mismatch. Until it lands the day card just does not claim to be
  // today, which is the safe direction to be wrong in.
  const [viewerLocalDate, setViewerLocalDate] = useState<string | null>(null);
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setViewerLocalDate(toLocalDateString(new Date()));
  }, []);

  const user: ProfileUser = useMemo(
    () => ({
      username: data.user.username,
      displayName: data.user.displayName,
      avatarUrl: data.user.avatarUrl,
      createdAt: data.user.createdAt,
      rank: data.user.rank,
    }),
    [data.user],
  );

  const stats: ProfileStatsData = useMemo(
    () => ({
      totalTokens: data.stats.totalTokens,
      totalCost: data.stats.totalCost,
      inputTokens: data.stats.inputTokens,
      outputTokens: data.stats.outputTokens,
      cacheReadTokens: data.stats.cacheReadTokens,
      cacheWriteTokens: data.stats.cacheWriteTokens,
      reasoningTokens: data.stats.reasoningTokens ?? 0,
      activeDays: data.stats.activeDays,
      submissionCount: data.stats.submissionCount,
      sessionCount: data.stats.sessionCount,
    }),
    [data.stats],
  );

  const showResubmitBanner =
    EARLY_ADOPTERS.includes(data.user.username) &&
    data.stats.submissionCount === 1;

  return (
    <>
      {showResubmitBanner && (
        <div className={cn(CONTAINER, "pt-6")}>
          <Alert>
            <AlertTitle>Fresh detail is available</AlertTitle>
            <AlertDescription>
              Re-submit with <code className="font-mono">tokens submit</code> to add
              daily model breakdowns.
            </AlertDescription>
          </Alert>
        </div>
      )}

      <ProfileView
        user={{ ...user, createdAt: user.createdAt ?? new Date().toISOString() }}
        stats={{
          ...stats,
          submissionCount: stats.submissionCount ?? 0,
          sessionCount: stats.sessionCount ?? 0,
        }}
        updatedAt={data.updatedAt ?? null}
        clients={data.clients}
        models={modelRows}
        mcpServers={data.mcpServers}
        socialLinks={socialLinks}
        verified={isVerifiedBySocialLinks(socialLinks)}
        hasBackfill={data.hasBackfill}
        period={period}
        onPeriodChange={(next) => {
          router.push(next === "all" ? `/u/${username}` : `/u/${username}?period=${next}`);
        }}
        onEmbedClick={() => setIsEmbedOpen(true)}
        activity={
          data.contributions.length > 0 ? (
            // The graph drives a selected date; the breakdown beside it renders
            // that day. It resolves to today by default, which is the figure
            // most people open their own profile to check.
            <div className="flex flex-col gap-4">
              <ProfileContributionGraph
                breakdownId={contributionBreakdownId}
                contributions={data.contributions}
                onPaletteChange={setContributionPalette}
                onRangeChange={setContributionRangeValue}
                onSelectedDateChange={(date) => {
                  if (date) {
                    setContributionSelection({
                      date,
                      rangeIdentity: contributionRangeIdentity,
                    });
                  }
                }}
                onViewChange={setContributionView}
                paletteName={contributionPalette}
                persistentSelection
                rangeStart={chartRange.start}
                rangeEnd={chartRange.end}
                rangeOptions={contributionRangeOptions}
                rangeValue={selectedContributionRange?.value}
                selectableRangeEnd={contributionSelectionEnd}
                selectedDate={selectedContributionDate}
                showBreakdown={false}
                view={contributionView}
              />
              {/* Its own card rather than the graph's footer, and it carries
                  the id the selected cell points at with aria-controls. */}
              {selectedContributionDay && (
                <ProfileContributionBreakdown
                  day={selectedContributionDay}
                  id={contributionBreakdownId}
                  paletteName={contributionPalette}
                />
              )}
            </div>
          ) : (
            <p className="py-10 text-center text-sm text-muted-foreground">
              No activity recorded for this period.
            </p>
          )
        }
        today={
          <ProfileToday
            day={selectedContributionDay}
            // Compared against the viewer's actual current date, not against
            // the default selection. "Nothing was picked" is not the same as
            // "today": inside a historical range the default is that range's
            // last day, and heading a 2024-12-31 card "Today" is simply wrong.
            isToday={selectedContributionDate === viewerLocalDate}
          />
        }
        usageChart={
          data.contributions.length > 0 ? (
            <ProfileUsageChart
              contributions={data.contributions}
              rangeStart={chartRange.start}
              rangeEnd={chartRange.end}
            />
          ) : undefined
        }
        habits={
          data.contributions.length > 0 ? (
            <ProfileHabits contributions={data.contributions} />
          ) : undefined
        }
        breakdown={<TokenBreakdown stats={stats} />}
        modelsSection={
          <ProfileModels models={data.models} modelUsage={data.modelUsage} />
        }
        // undefined rather than an always-truthy element: ProfileDevices
        // renders null when the list is empty, and ProfileView cannot tell the
        // difference from the outside.
        devices={
          initialDevices && initialDevices.length > 0 ? (
            <ProfileDevices devices={initialDevices} />
          ) : undefined
        }
      />

      <ProfileEmbedDialog
        username={username}
        displayName={user.displayName}
        open={isEmbedOpen}
        onClose={() => setIsEmbedOpen(false)}
      />
    </>
  );
}

