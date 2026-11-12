"use client";

import { useMemo } from "react";
import type { DailyContribution } from "@/lib/types";
import { formatCurrency, formatNumber } from "@/lib/utils";
import { useLocale, useT } from "@/components/layout/LocaleProvider";
import { formatDate } from "@/lib/i18n/format";
import { tw } from "@/lib/tw";
import { cn } from "@/lib/utils";

/** Monday-first, the order the bar chart draws in. Sunday is 0 as `getUTCDay`. */
const WEEKDAY_ORDER = [
  { index: 1 },
  { index: 2 },
  { index: 3 },
  { index: 4 },
  { index: 5 },
  { index: 6 },
  { index: 0 },
] as const;

/**
 * One reference date per weekday, so `Intl` can name them.
 *
 * 2024-01-07 was a Sunday, and every lookup is pinned to UTC: the label must not
 * depend on the runner's zone any more than the buckets do.
 */
const WEEKDAY_REFERENCE = [7, 8, 9, 10, 11, 12, 13].map(
  (day) => new Date(Date.UTC(2024, 0, day)),
);

function weekdayIndex(date: string): number {
  const [year, month, day] = date.split("-").map(Number);
  return new Date(Date.UTC(year, month - 1, day)).getUTCDay();
}

export interface ProfileHabitsProps {
  contributions: DailyContribution[];
}

export function ProfileHabits({ contributions }: ProfileHabitsProps) {
  const t = useT();
  const locale = useLocale();
  const weekdayNames = useMemo(
    () =>
      WEEKDAY_REFERENCE.map((date) =>
        new Intl.DateTimeFormat(locale, {
          weekday: "long",
          timeZone: "UTC",
        }).format(date),
      ),
    [locale],
  );
  const weekdayShort = useMemo(
    () =>
      WEEKDAY_REFERENCE.map((date) =>
        new Intl.DateTimeFormat(locale, {
          weekday: "narrow",
          timeZone: "UTC",
        }).format(date),
      ),
    [locale],
  );
  const weekdayTokens = [0, 0, 0, 0, 0, 0, 0];
  let totalTokens = 0;
  let biggestDay: DailyContribution | null = null;

  for (const contribution of contributions) {
    const tokens = contribution.totals.tokens;
    if (tokens <= 0) continue;
    weekdayTokens[weekdayIndex(contribution.date)] += tokens;
    totalTokens += tokens;
    if (!biggestDay || tokens > biggestDay.totals.tokens) {
      biggestDay = contribution;
    }
  }

  if (!biggestDay || totalTokens <= 0) return null;

  const topWeekdayIndex = weekdayTokens.indexOf(Math.max(...weekdayTokens));
  const topWeekdayTokens = weekdayTokens[topWeekdayIndex];
  const topWeekdayShare = (topWeekdayTokens / totalTokens) * 100;

  return (
    <Panel aria-labelledby="profile-habits-title">
      <Header>
        <Title id="profile-habits-title">{t("profile.habits.heading")}</Title>
        <Range>{t("profile.habits.range")}</Range>
      </Header>

      <Highlights>
        <Highlight>
          <Label>{t("profile.habits.topWeekday")}</Label>
          <Value>{weekdayNames[topWeekdayIndex]}</Value>
          <Meta title={topWeekdayTokens.toLocaleString(locale)}>
            {t("profile.habits.topWeekdayMeta", {
              tokens: t("profile.units.token", { count: topWeekdayTokens }),
              share: formatNumber(topWeekdayShare),
            })}
          </Meta>
        </Highlight>
        <Highlight>
          <Label>{t("profile.habits.biggestDay")}</Label>
          <Value $accent title={biggestDay.totals.tokens.toLocaleString(locale)}>
            {t("profile.units.token", { count: biggestDay.totals.tokens })}
          </Value>
          <Meta>
            {formatDate(new Date(`${biggestDay.date}T00:00:00`), locale, {
              year: "numeric",
              month: "long",
              day: "numeric",
            })}{" "}
            · {formatCurrency(biggestDay.totals.cost)}
          </Meta>
        </Highlight>
      </Highlights>

      <Distribution>
        <Label>{t("profile.habits.byWeekday")}</Label>
        <Bars>
          {WEEKDAY_ORDER.map(({ index }) => {
            const tokens = weekdayTokens[index];
            const isTop = index === topWeekdayIndex;
            const height = topWeekdayTokens > 0
              ? Math.max((tokens / topWeekdayTokens) * 100, tokens > 0 ? 6 : 2)
              : 2;
            return (
              <BarColumn key={index}>
                <BarTrack>
                  <Bar
                    $height={height}
                    $top={isTop}
                    title={t("profile.habits.barTitle", {
                      weekday: weekdayNames[index],
                      tokens: t("profile.units.token", { count: tokens }),
                    })}
                  />
                </BarTrack>
                <BarLabel $top={isTop}>{weekdayShort[index]}</BarLabel>
              </BarColumn>
            );
          })}
        </Bars>
      </Distribution>
    </Panel>
  );
}

const Panel = tw("section", "overflow-hidden rounded-xl border bg-card");

const Header = tw(
  "div",
  "flex items-baseline justify-between gap-3 border-b px-4 py-3.5"
);

const Title = tw("h2", "m-0 text-base font-medium text-foreground");
const Range = tw("span", "text-xs text-muted-foreground");

const Highlights = tw("div", "grid grid-cols-2 border-b");

// The divider is on the second cell rather than between them, which is what
// `& + &` did — the first column must not carry a left edge.
const Highlight = tw(
  "div",
  "min-w-0 px-4 py-3.5 [&+&]:border-l [&+&]:border-border"
);

const Label = tw(
  "p",
  "m-0 text-[11px] font-semibold uppercase tracking-[0.06em] text-muted-foreground"
);

const Value = ({
  $accent,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"p"> & { $accent?: boolean }) => (
  <p
    {...props}
    className={cn(
      "m-0 mt-[5px] overflow-hidden text-ellipsis whitespace-nowrap text-base font-semibold [font-variant-numeric:tabular-nums]",
      $accent ? "text-primary" : "text-foreground",
      className
    )}
  />
);

const Meta = tw(
  "p",
  "m-0 mt-[3px] overflow-hidden text-ellipsis whitespace-nowrap text-xs text-muted-foreground [font-variant-numeric:tabular-nums]"
);

const Distribution = tw("div", "px-4 pb-4 pt-3.5");
const Bars = tw("div", "mt-2.5 flex items-end gap-2");
const BarColumn = tw("div", "flex min-w-0 flex-1 flex-col items-center gap-1.5");
const BarTrack = tw("div", "flex h-16 w-full items-end");

// Height is a percentage computed per bar, so it stays inline.
const Bar = ({
  $height,
  $top,
  style,
  ...props
}: React.ComponentPropsWithoutRef<"div"> & {
  $height: number;
  $top: boolean;
}) => (
  <div
    {...props}
    style={{ height: `${$height}%`, ...style }}
    className={cn(
      "w-full rounded-b-sm rounded-t",
      $top ? "bg-primary" : "bg-muted"
    )}
  />
);

const BarLabel = ({
  $top,
  className,
  ...props
}: React.ComponentPropsWithoutRef<"span"> & { $top: boolean }) => (
  <span
    {...props}
    className={cn(
      "text-[10px] font-medium",
      $top ? "text-foreground" : "text-muted-foreground",
      className
    )}
  />
);
