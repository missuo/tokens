"use client";

import { useMemo, useState } from "react";
import { SourceLogo } from "@/components/SourceLogo";
import { useLocale, useT } from "@/components/layout/LocaleProvider";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { ModelIcon } from "./ModelIcon";
import { SpeedTierLabel } from "./SpeedTier";
import { cn } from "@/lib/utils";
import {
  formatCurrency,
  formatNumber,
  formatPercent,
  formatWholePercents,
} from "@/lib/format";
import { getSpeedTierRows } from "@/lib/speedUsage";
import {
  createContributionClientDetails,
  getContributionDayMessageCount,
} from "./ProfileContributionGraph";
import type { DailyContribution, TokenBreakdown } from "@/lib/types";

export interface ProfileTodayProps {
  /** Today for the viewer, or null when they have submitted nothing today. */
  day: DailyContribution | null | undefined;
  /** The day before, or null when nothing was submitted for it. */
  yesterday?: DailyContribution | null | undefined;
  className?: string;
}

type DayChoice = "today" | "yesterday";

// `labelKey` is a catalogue key, not the text.
const DAY_CHOICES: ReadonlyArray<{ value: DayChoice; labelKey: string }> = [
  { value: "today", labelKey: "profile.today.heading" },
  { value: "yesterday", labelKey: "profile.today.yesterdayHeading" },
];

/** `labelKey` is a catalogue path segment: `tokens.input` and so on. */
const COMPOSITION: ReadonlyArray<{
  key: keyof TokenBreakdown;
  labelKey: string;
  className: string;
}> = [
  { key: "input", labelKey: "profile.tokens.input", className: "bg-chart-1" },
  { key: "output", labelKey: "profile.tokens.output", className: "bg-chart-2" },
  { key: "cacheRead", labelKey: "profile.tokens.cacheRead", className: "bg-chart-3" },
  { key: "cacheWrite", labelKey: "profile.tokens.cacheWrite", className: "bg-chart-4" },
  { key: "reasoning", labelKey: "profile.tokens.reasoning", className: "bg-chart-5" },
];

function Figure({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="font-mono text-lg leading-none tabular-nums sm:text-xl">
        {value}
      </span>
    </div>
  );
}

/**
 * The day's figures, in one block.
 *
 * This is what most people open their own profile to check, so it sits
 * directly under the headline totals and carries the whole picture: the three
 * headline numbers, how the tokens split across input, output, cache and
 * reasoning, and which clients and models produced them. Nothing inside
 * scrolls — a summary you have to scroll is not a summary.
 */
export function ProfileToday({
  day: todayDay,
  yesterday: yesterdayDay,
  className,
}: ProfileTodayProps) {
  const t = useT();
  const locale = useLocale();
  // Today stays the default; Yesterday is the one finished day, which is what
  // you check once today has only just started.
  const [choice, setChoice] = useState<DayChoice>("today");
  const day = choice === "today" ? todayDay : yesterdayDay;

  const clients = useMemo(
    () => (day ? createContributionClientDetails(day) : []),
    [day]
  );
  const messages = useMemo(
    () => (day ? getContributionDayMessageCount(day, clients) : 0),
    [day, clients]
  );

  // Always today. This card used to render whichever day the contribution
  // graph had selected, which put the same tokens, cost and message count on
  // the page twice — once here and once in the graph's own breakdown, for the
  // same day. The graph keeps the day you picked; this keeps the one you came
  // to check.
  const heading = t(
    choice === "today" ? "profile.today.heading" : "profile.today.yesterdayHeading",
  );
  const tokens = day?.totals.tokens ?? 0;
  const cost = day?.totals.cost ?? 0;

  const composition = useMemo(() => {
    const totals: TokenBreakdown = {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      reasoning: 0,
    };
    for (const client of clients) {
      for (const part of COMPOSITION) {
        totals[part.key] += client.tokens[part.key] || 0;
      }
    }
    const sum = COMPOSITION.reduce((acc, part) => acc + totals[part.key], 0);
    return {
      totals,
      sum,
      parts: COMPOSITION.filter((p) => totals[p.key] > 0).map((p) => ({
        ...p,
        share: (totals[p.key] / sum) * 100,
      })),
    };
  }, [clients]);

  // Busiest client first: the one that did the work should read first.
  const ranked = useMemo(
    () => [...clients].sort((a, b) => b.totalTokens - a.totalTokens),
    [clients]
  );

  return (
    <section
      className={cn("overflow-hidden rounded-lg border bg-card", className)}
      aria-label={t("profile.today.usageAria", { heading })}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2 border-b px-4 py-3 sm:px-5">
        <div className="flex items-center gap-3">
          {/* The switch is the heading: the selected item names the day the
              figures belong to, so a separate title would only repeat it. */}
          <h2 className="sr-only">{heading}</h2>
          <ToggleGroup
            value={[choice]}
            onValueChange={(value) => {
              const next = value[0] as DayChoice | undefined;
              if (next) setChoice(next);
            }}
            variant="outline"
            aria-label={t("profile.today.dayAria")}
            className="[&>*]:h-7 [&>*]:px-2.5 [&>*]:text-xs"
          >
            {DAY_CHOICES.map((c) => (
              <ToggleGroupItem key={c.value} value={c.value}>
                {t(c.labelKey)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          {day?.date && (
            <span className="font-mono text-xs text-muted-foreground">{day.date}</span>
          )}
        </div>
        <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
          <Figure
            label={t("profile.stats.tokens")}
            value={formatNumber(tokens, true, locale)}
          />
          <Figure
            label={t("profile.stats.cost")}
            value={formatCurrency(cost, true, locale)}
          />
          <Figure
            label={t("profile.today.messages")}
            value={formatNumber(messages, true, locale)}
          />
        </div>
      </div>

      {ranked.length === 0 ? (
        <p className="px-4 py-5 text-sm text-muted-foreground sm:px-5">
          {t(choice === "today" ? "profile.today.empty" : "profile.today.emptyYesterday")}
        </p>
      ) : (
        <>
          {composition.sum > 0 && (
            <div className="border-b px-4 py-3.5 sm:px-5">
              <div className="flex h-1.5 w-full overflow-hidden rounded-full">
                {composition.parts.map((part) => (
                  <div
                    key={part.key}
                    className={part.className}
                    style={{ width: `${part.share}%` }}
                    title={t("profile.composition.segmentTitle", {
                      label: t(part.labelKey),
                      value: formatNumber(composition.totals[part.key], true, locale),
                    })}
                  />
                ))}
              </div>
              <dl className="mt-2.5 flex flex-wrap gap-x-5 gap-y-1.5">
                {composition.parts.map((part) => (
                  <div key={part.key} className="flex items-center gap-1.5">
                    <span
                      className={cn("size-2 rounded-full", part.className)}
                      aria-hidden="true"
                    />
                    <dt className="text-xs text-muted-foreground">
                      {t(part.labelKey)}
                    </dt>
                    <dd className="whitespace-nowrap font-mono text-xs tabular-nums">
                      {formatNumber(composition.totals[part.key], true, locale)}{" "}
                      <span className="text-muted-foreground">
                        {formatPercent(part.share, locale)}
                      </span>
                    </dd>
                  </div>
                ))}
              </dl>
            </div>
          )}

          <ul className="divide-y">
            {ranked.map((client) => {
              const models = [...client.models].sort(
                (a, b) => b.totalTokens - a.totalTokens
              );
              return (
                <li key={client.client} className="px-4 py-3 sm:px-5">
                  <div className="flex items-center justify-between gap-4">
                    <span className="flex min-w-0 items-center gap-2">
                      <SourceLogo sourceId={client.client} height={14} decorative />
                      <span className="truncate text-sm font-medium">{client.client}</span>
                    </span>
                    <span className="flex shrink-0 items-baseline gap-3">
                      <span className="font-mono text-xs tabular-nums text-muted-foreground">
                        {t("profile.today.clientMessages", { count: client.messages })}
                      </span>
                      <span className="font-mono text-xs tabular-nums text-muted-foreground">
                        {formatCurrency(client.cost, true, locale)}
                      </span>
                      <span className="font-mono text-sm tabular-nums">
                        {formatNumber(client.totalTokens, true, locale)}
                      </span>
                    </span>
                  </div>

                  {models.length > 0 && (
                    <ul className="mt-2 flex flex-col gap-1.5 pl-6">
                      {models.map((model) => {
                        const share =
                          client.totalTokens > 0
                            ? model.totalTokens / client.totalTokens
                            : 0;
                        const tiers = getSpeedTierRows(
                          model.speedUsage,
                          model.totalTokens,
                          model.messages
                        );
                        // Same scale as the model rows, so a model's tiers add
                        // up to its own bar and percentage.
                        const tierShares = tiers.map((tier) =>
                          client.totalTokens > 0
                            ? (tier.tokens / client.totalTokens) * 100
                            : 0
                        );
                        const tierPercents = formatWholePercents(tierShares, locale);
                        return (
                          <li key={`${client.client}:${model.modelId}`}>
                            <div className="flex items-center gap-3">
                              <ModelIcon model={model.modelId} size={13} />
                              <span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground">
                                {model.modelId}
                              </span>
                              <div className="hidden h-0.5 w-24 overflow-hidden rounded-full bg-border sm:block">
                                <div
                                  className="h-full rounded-full bg-primary/50"
                                  style={{ width: `${Math.max(share * 100, 2)}%` }}
                                />
                              </div>
                              <span className="w-11 shrink-0 text-right font-mono text-xs tabular-nums text-muted-foreground">
                                {Math.round(share * 100)}%
                              </span>
                              <span className="w-16 shrink-0 text-right font-mono text-xs tabular-nums">
                                {formatNumber(model.totalTokens, true, locale)}
                              </span>
                            </div>

                            {tiers.length > 0 && (
                              // Indented to where the model name starts: the
                              // 13px model icon plus the row's 12px gap.
                              <ul className="mt-1 flex flex-col gap-1 pl-[25px]">
                                {tiers.map((tier, index) => (
                                  <li key={tier.speed} className="flex items-center gap-3">
                                    <div className="flex min-w-0 flex-1">
                                      <SpeedTierLabel
                                        tier={tier}
                                        className={cn(
                                          "text-[0.6875rem]",
                                          tier.speed === "unknown"
                                            ? "text-muted-foreground/70"
                                            : "text-muted-foreground"
                                        )}
                                      />
                                    </div>
                                    <div className="hidden h-0.5 w-24 overflow-hidden rounded-full bg-border sm:block">
                                      {tier.speed !== "unknown" && tier.tokens > 0 && (
                                        <div
                                          className={cn(
                                            "h-full min-w-px rounded-full",
                                            tier.speed === "standard"
                                              ? "bg-muted-foreground/45"
                                              : "bg-amber-500/70 dark:bg-amber-400/70"
                                          )}
                                          style={{ width: `${tierShares[index]}%` }}
                                        />
                                      )}
                                    </div>
                                    <span className="w-11 shrink-0 text-right font-mono text-[0.6875rem] tabular-nums text-muted-foreground">
                                      {tierPercents[index]}
                                    </span>
                                    <span className="w-16 shrink-0 text-right font-mono text-[0.6875rem] tabular-nums text-muted-foreground">
                                      {formatNumber(tier.tokens, true, locale)}
                                    </span>
                                  </li>
                                ))}
                              </ul>
                            )}
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}
