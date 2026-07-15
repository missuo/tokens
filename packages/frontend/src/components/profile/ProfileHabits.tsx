"use client";

import styled from "styled-components";
import type { DailyContribution } from "@/lib/types";
import { formatCurrency, formatDateFull, formatNumber } from "@/lib/utils";

const WEEKDAY_NAMES = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
] as const;

const WEEKDAY_ORDER = [
  { index: 1, short: "Mo" },
  { index: 2, short: "Tu" },
  { index: 3, short: "We" },
  { index: 4, short: "Th" },
  { index: 5, short: "Fr" },
  { index: 6, short: "Sa" },
  { index: 0, short: "Su" },
] as const;

function weekdayIndex(date: string): number {
  const [year, month, day] = date.split("-").map(Number);
  return new Date(Date.UTC(year, month - 1, day)).getUTCDay();
}

export interface ProfileHabitsProps {
  contributions: DailyContribution[];
}

export function ProfileHabits({ contributions }: ProfileHabitsProps) {
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
        <Title id="profile-habits-title">Coding patterns</Title>
        <Range>Latest 12 months</Range>
      </Header>

      <Highlights>
        <Highlight>
          <Label>Most productive day</Label>
          <Value>{WEEKDAY_NAMES[topWeekdayIndex]}</Value>
          <Meta title={topWeekdayTokens.toLocaleString("en-US")}>
            {formatNumber(topWeekdayTokens)} tokens · {topWeekdayShare.toFixed(0)}%
            of total
          </Meta>
        </Highlight>
        <Highlight>
          <Label>Biggest day</Label>
          <Value $accent title={biggestDay.totals.tokens.toLocaleString("en-US")}>
            {formatNumber(biggestDay.totals.tokens)} tokens
          </Value>
          <Meta>
            {formatDateFull(biggestDay.date)} · {formatCurrency(biggestDay.totals.cost)}
          </Meta>
        </Highlight>
      </Highlights>

      <Distribution>
        <Label>Tokens by weekday</Label>
        <Bars>
          {WEEKDAY_ORDER.map(({ index, short }) => {
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
                    title={`${WEEKDAY_NAMES[index]}: ${formatNumber(tokens)} tokens`}
                  />
                </BarTrack>
                <BarLabel $top={isTop}>{short}</BarLabel>
              </BarColumn>
            );
          })}
        </Bars>
      </Distribution>
    </Panel>
  );
}

const Panel = styled.section`
  overflow: hidden;
  border: 1px solid var(--service-border);
  border-radius: 12px;
  background: var(--service-surface);
`;

const Header = styled.div`
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--service-border);
`;

const Title = styled.h2`
  margin: 0;
  color: var(--service-text);
  font-size: 16px;
  font-weight: 500;
`;

const Range = styled.span`
  color: var(--service-text-muted);
  font-size: 12px;
`;

const Highlights = styled.div`
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  border-bottom: 1px solid var(--service-border);
`;

const Highlight = styled.div`
  min-width: 0;
  padding: 14px 16px;

  & + & {
    border-left: 1px solid var(--service-border);
  }
`;

const Label = styled.p`
  margin: 0;
  color: var(--service-text-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
`;

const Value = styled.p<{ $accent?: boolean }>`
  overflow: hidden;
  margin: 5px 0 0;
  color: ${({ $accent }) =>
    $accent ? "var(--service-accent)" : "var(--service-text)"};
  font-size: 16px;
  font-variant-numeric: tabular-nums;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
`;

const Meta = styled.p`
  overflow: hidden;
  margin: 3px 0 0;
  color: var(--service-text-muted);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  text-overflow: ellipsis;
  white-space: nowrap;
`;

const Distribution = styled.div`
  padding: 14px 16px 16px;
`;

const Bars = styled.div`
  display: flex;
  align-items: flex-end;
  gap: 8px;
  margin-top: 10px;
`;

const BarColumn = styled.div`
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  align-items: center;
  gap: 6px;
`;

const BarTrack = styled.div`
  display: flex;
  width: 100%;
  height: 64px;
  align-items: flex-end;
`;

const Bar = styled.div<{ $height: number; $top: boolean }>`
  width: 100%;
  height: ${({ $height }) => `${$height}%`};
  border-radius: 4px 4px 2px 2px;
  background: ${({ $top }) =>
    $top ? "var(--service-accent)" : "var(--service-surface-muted)"};
`;

const BarLabel = styled.span<{ $top: boolean }>`
  color: ${({ $top }) =>
    $top ? "var(--service-text)" : "var(--service-text-muted)"};
  font-size: 10px;
  font-weight: 500;
`;
