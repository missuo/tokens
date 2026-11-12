"use client";

import { useRef } from "react";
import { useTheme } from "next-themes";
import type { DailyContribution, TooltipPosition, GraphColorPalette } from "@/lib/types";
import { getThemeGradeColor } from "@/lib/themes";
import { formatContributionDate } from "@/lib/date-utils";
import { formatCurrency, formatNumber } from "@/lib/format";
import { useLocale, useT } from "@/components/layout/LocaleProvider";

interface TooltipProps {
  day: DailyContribution | null;
  position: TooltipPosition | null;
  visible: boolean;
  palette: GraphColorPalette;
}

function useAdjustedPosition(
  position: TooltipPosition | null,
  visible: boolean,
  tooltipRef: React.RefObject<HTMLDivElement | null>,
): TooltipPosition | null {
  if (!visible || !position) return null;
  const tooltip = tooltipRef.current;
  if (!tooltip) return { x: position.x + 15, y: position.y + 15 };

  const rect = tooltip.getBoundingClientRect();
  const viewportWidth = typeof window !== "undefined" ? window.innerWidth : 1920;
  const viewportHeight = typeof window !== "undefined" ? window.innerHeight : 1080;

  let x = position.x + 15;
  let y = position.y + 15;
  if (x + rect.width > viewportWidth - 10) x = position.x - rect.width - 15;
  if (y + rect.height > viewportHeight - 10) y = position.y - rect.height - 15;
  return { x: Math.max(10, x), y: Math.max(10, y) };
}

export function Tooltip({ day, position, visible, palette }: TooltipProps) {
  const t = useT();
  const locale = useLocale();
  const tooltipRef = useRef<HTMLDivElement>(null);
  const { resolvedTheme } = useTheme();
  const adjustedPosition = useAdjustedPosition(position, visible, tooltipRef);

  if (!visible || !day || !adjustedPosition) return null;

  const { totals, tokenBreakdown } = day;

  return (
    <div ref={tooltipRef} role="tooltip" className="pointer-events-none fixed z-50" style={{ left: adjustedPosition.x, top: adjustedPosition.y }}>
      <div className="min-w-[220px] rounded-2xl border border-border bg-card p-4 text-foreground shadow-2xl backdrop-blur-md">
        <div className="mb-3 text-base font-bold text-foreground">{formatContributionDate(day, locale)}</div>

        <div className="my-3 border-t border-border" />

        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-medium text-muted-foreground">
            {t("common.tooltip.totalTokens")}
          </span>
          <span
            className="text-xl font-bold tracking-tight"
            style={{
              color:
                day.intensity >= 2
                  ? getThemeGradeColor(
                      palette,
                      day.intensity,
                      resolvedTheme === "dark",
                    )
                  : "var(--foreground)",
            }}
          >
            {formatNumber(totals.tokens, true, locale)}
          </span>
        </div>

        <div className="my-3 border-t border-border" />

        <div className="flex flex-col gap-2 text-sm">
          <TokenRow label={t("common.breakdownPanel.input")} value={tokenBreakdown.input} />
          <TokenRow label={t("common.breakdownPanel.output")} value={tokenBreakdown.output} />
          <TokenRow label={t("common.breakdownPanel.cacheRead")} value={tokenBreakdown.cacheRead} />
          <TokenRow label={t("common.breakdownPanel.cacheWrite")} value={tokenBreakdown.cacheWrite} />
          {tokenBreakdown.reasoning > 0 && (
            <TokenRow label={t("common.breakdownPanel.reasoning")} value={tokenBreakdown.reasoning} />
          )}
        </div>

        <div className="my-3 border-t border-border" />

        <div className="flex items-center justify-between">
          <span className="text-sm font-semibold text-muted-foreground">
            {t("common.tooltip.cost")}
          </span>
          <span className="font-bold text-foreground">
            {formatCurrency(totals.cost, totals.cost >= 1000, locale)}
          </span>
        </div>

        <div className="mt-2 flex items-center justify-between">
          <span className="text-sm font-medium text-muted-foreground">
            {t("common.tooltip.messages")}
          </span>
          <span className="text-sm font-semibold text-foreground">
            {formatNumber(totals.messages, false, locale)}
          </span>
        </div>
      </div>
    </div>
  );
}

function TokenRow({ label, value }: { label: string; value: number }) {
  const locale = useLocale();
  if (value === 0) return null;
  return (
    <div className="flex items-center justify-between">
      <span className="font-medium text-muted-foreground">{label}</span>
      <span className="font-mono font-semibold text-foreground">
        {formatNumber(value, true, locale)}
      </span>
    </div>
  );
}
