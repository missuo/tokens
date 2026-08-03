"use client";

import { useMemo } from "react";
import { SourceLogo } from "@/components/SourceLogo";
import { ModelIcon } from "./ModelIcon";
import { cn } from "@/lib/utils";
import { formatCurrency, formatNumber } from "@/lib/format";
import {
  createContributionClientDetails,
  getContributionDayMessageCount,
} from "./ProfileContributionGraph";
import type { DailyContribution, TokenBreakdown } from "@/lib/types";

export interface ProfileTodayProps {
  /** Today for the viewer, or null when they have submitted nothing today. */
  day: DailyContribution | null | undefined;
  className?: string;
}

const COMPOSITION: ReadonlyArray<{
  key: keyof TokenBreakdown;
  label: string;
  className: string;
}> = [
  { key: "input", label: "Input", className: "bg-chart-1" },
  { key: "output", label: "Output", className: "bg-chart-2" },
  { key: "cacheRead", label: "Cache read", className: "bg-chart-3" },
  { key: "cacheWrite", label: "Cache write", className: "bg-chart-4" },
  { key: "reasoning", label: "Reasoning", className: "bg-chart-5" },
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
export function ProfileToday({ day, className }: ProfileTodayProps) {
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
  const heading = "Today";
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
    return { totals, sum, parts: COMPOSITION.filter((p) => totals[p.key] > 0) };
  }, [clients]);

  // Busiest client first: the one that did the work should read first.
  const ranked = useMemo(
    () => [...clients].sort((a, b) => b.totalTokens - a.totalTokens),
    [clients]
  );

  return (
    <section
      className={cn("overflow-hidden rounded-lg border bg-card", className)}
      aria-label={`${heading} usage`}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2 border-b px-4 py-3 sm:px-5">
        <div className="flex items-baseline gap-3">
          <h2 className="text-sm font-semibold tracking-tight">{heading}</h2>
          {day?.date && (
            <span className="font-mono text-xs text-muted-foreground">{day.date}</span>
          )}
        </div>
        <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
          <Figure label="Tokens" value={formatNumber(tokens, true)} />
          <Figure label="Cost" value={formatCurrency(cost, true)} />
          <Figure label="Messages" value={formatNumber(messages, true)} />
        </div>
      </div>

      {ranked.length === 0 ? (
        <p className="px-4 py-5 text-sm text-muted-foreground sm:px-5">
          Nothing recorded for this day yet.
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
                    style={{
                      width: `${(composition.totals[part.key] / composition.sum) * 100}%`,
                    }}
                    title={`${part.label}: ${formatNumber(composition.totals[part.key], true)}`}
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
                    <dt className="text-xs text-muted-foreground">{part.label}</dt>
                    <dd className="font-mono text-xs tabular-nums">
                      {formatNumber(composition.totals[part.key], true)}
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
                        {formatNumber(client.messages, true)} msg
                      </span>
                      <span className="font-mono text-xs tabular-nums text-muted-foreground">
                        {formatCurrency(client.cost, true)}
                      </span>
                      <span className="font-mono text-sm tabular-nums">
                        {formatNumber(client.totalTokens, true)}
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
                        return (
                          <li
                            key={`${client.client}:${model.modelId}`}
                            className="flex items-center gap-3"
                          >
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
                              {formatNumber(model.totalTokens, true)}
                            </span>
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
