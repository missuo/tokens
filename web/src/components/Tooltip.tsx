"use client";

import { useRef } from "react";
import { useTheme } from "next-themes";
import type { DailyContribution, TooltipPosition, GraphColorPalette } from "@/lib/types";
import { getThemeGradeColor } from "@/lib/themes";
import { formatCurrency, formatTokenCount } from "@/lib/utils";
import { formatContributionDate } from "@/lib/date-utils";

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
  const tooltipRef = useRef<HTMLDivElement>(null);
  const { resolvedTheme } = useTheme();
  const adjustedPosition = useAdjustedPosition(position, visible, tooltipRef);

  if (!visible || !day || !adjustedPosition) return null;

  const { totals, tokenBreakdown } = day;

  return (
    <div ref={tooltipRef} role="tooltip" className="pointer-events-none fixed z-50" style={{ left: adjustedPosition.x, top: adjustedPosition.y }}>
      <div className="min-w-[220px] rounded-2xl border border-border bg-card p-4 text-foreground shadow-2xl backdrop-blur-md">
        <div className="mb-3 text-base font-bold text-foreground">{formatContributionDate(day)}</div>

        <div className="my-3 border-t border-border" />

        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-medium text-muted-foreground">Total Tokens</span>
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
            {formatTokenCount(totals.tokens)}
          </span>
        </div>

        <div className="my-3 border-t border-border" />

        <div className="flex flex-col gap-2 text-sm">
          <TokenRow label="Input" value={tokenBreakdown.input} />
          <TokenRow label="Output" value={tokenBreakdown.output} />
          <TokenRow label="Cache Read" value={tokenBreakdown.cacheRead} />
          <TokenRow label="Cache Write" value={tokenBreakdown.cacheWrite} />
          {tokenBreakdown.reasoning > 0 && <TokenRow label="Reasoning" value={tokenBreakdown.reasoning} />}
        </div>

        <div className="my-3 border-t border-border" />

        <div className="flex items-center justify-between">
          <span className="text-sm font-semibold text-muted-foreground">Cost</span>
          <span className="font-bold text-foreground">{formatCurrency(totals.cost)}</span>
        </div>

        <div className="mt-2 flex items-center justify-between">
          <span className="text-sm font-medium text-muted-foreground">Messages</span>
          <span className="text-sm font-semibold text-foreground">{totals.messages.toLocaleString()}</span>
        </div>
      </div>
    </div>
  );
}

function TokenRow({ label, value }: { label: string; value: number }) {
  if (value === 0) return null;
  return (
    <div className="flex items-center justify-between">
      <span className="font-medium text-muted-foreground">{label}</span>
      <span className="font-mono font-semibold text-foreground">{formatTokenCount(value)}</span>
    </div>
  );
}
