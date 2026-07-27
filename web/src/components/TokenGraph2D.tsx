"use client";

import { useRef, useEffect, useCallback, useMemo } from "react";
import { useTheme } from "next-themes";
import type { DailyContribution, GraphColorPalette, TooltipPosition } from "@/lib/types";
import { getThemeGradeColor } from "@/lib/themes";
import { groupByWeek } from "@/lib/utils";
import { BOX_WIDTH, CELL_SIZE, CANVAS_MARGIN, HEADER_HEIGHT, TEXT_HEIGHT, FONT_SIZE, FONT_FAMILY, DAY_LABELS_SHORT, MONTH_LABELS_SHORT } from "@/lib/constants";
import { parseISO, getMonth } from "date-fns";

interface TokenGraph2DProps {
  contributions: DailyContribution[];
  palette: GraphColorPalette;
  year: string;
  onDayHover: (day: DailyContribution | null, position: TooltipPosition | null) => void;
  onDayClick: (day: DailyContribution | null) => void;
}

export function TokenGraph2D({ contributions, palette, year, onDayHover, onDayClick }: TokenGraph2DProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const weeksData = useMemo(() => groupByWeek(contributions, year), [contributions, year]);
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === "dark";

  // Theme colors are derived from `isDark` (next-themes' resolvedTheme — the
  // React source of truth) rather than read via getComputedStyle. Child effects
  // fire before the ancestor ThemeProvider flips the `.dark`/`.light` class on
  // <html>, so a getComputedStyle read here would lag one theme behind on every
  // toggle. These hexes mirror --surface / --surface-secondary / --muted in
  // globals.css.
  const graphBg = isDark ? "#12151e" : "#ffffff";
  const graphEmptyCell = isDark ? "#171b26" : "#f4f5f7";
  const graphMuted = isDark ? "#8b94a7" : "#5b6473";

  const CANVAS_LABEL_RIGHT_PADDING = 32;
  const canvasWidth = CANVAS_MARGIN * 2 + TEXT_HEIGHT + weeksData.length * CELL_SIZE + CANVAS_LABEL_RIGHT_PADDING;
  const canvasHeight = HEADER_HEIGHT + 7 * CELL_SIZE + CANVAS_MARGIN;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = canvasWidth * dpr;
    canvas.height = canvasHeight * dpr;
    canvas.style.width = `${canvasWidth}px`;
    canvas.style.height = `${canvasHeight}px`;
    ctx.scale(dpr, dpr);

    ctx.fillStyle = graphBg;
    ctx.fillRect(0, 0, canvasWidth, canvasHeight);

    ctx.font = `${FONT_SIZE}px ${FONT_FAMILY}`;
    ctx.fillStyle = graphMuted;
    ctx.textAlign = "left";

    let lastMonth = -1;
    for (let weekIndex = 0; weekIndex < weeksData.length; weekIndex++) {
      const week = weeksData[weekIndex];
      const firstDay = week.days.find((d) => d !== null);
      if (firstDay) {
        const month = getMonth(parseISO(firstDay.date));
        if (month !== lastMonth) {
          const x = CANVAS_MARGIN + TEXT_HEIGHT + weekIndex * CELL_SIZE;
          ctx.fillText(MONTH_LABELS_SHORT[month], x, CANVAS_MARGIN + FONT_SIZE);
          lastMonth = month;
        }
      }
    }

    ctx.textAlign = "right";
    for (const dayIndex of [1, 3, 5]) {
      const y = HEADER_HEIGHT + dayIndex * CELL_SIZE + BOX_WIDTH / 2 + FONT_SIZE / 3;
      ctx.fillText(DAY_LABELS_SHORT[dayIndex], CANVAS_MARGIN + TEXT_HEIGHT - 4, y);
    }

    for (let weekIndex = 0; weekIndex < weeksData.length; weekIndex++) {
      const week = weeksData[weekIndex];
      for (let dayIndex = 0; dayIndex < 7; dayIndex++) {
        const day = week.days[dayIndex];
        const x = CANVAS_MARGIN + TEXT_HEIGHT + weekIndex * CELL_SIZE;
        const y = HEADER_HEIGHT + dayIndex * CELL_SIZE;

        const intensity = day?.intensity ?? 0;
        const colorHex = getThemeGradeColor(palette, intensity, isDark);
        const resolvedColor = colorHex.startsWith("var(") ? graphEmptyCell : colorHex;
        ctx.fillStyle = resolvedColor;

        roundRect(ctx, x, y, BOX_WIDTH, BOX_WIDTH, 2);
        ctx.fill();
      }
    }
  }, [contributions, palette, year, weeksData, canvasWidth, canvasHeight, graphBg, graphEmptyCell, graphMuted, isDark]);

  const getDayAtPosition = useCallback(
    (clientX: number, clientY: number): { day: DailyContribution | null; position: TooltipPosition } | null => {
      const canvas = canvasRef.current;
      if (!canvas) return null;

      const rect = canvas.getBoundingClientRect();
      const x = clientX - rect.left;
      const y = clientY - rect.top;

      const gridX = x - CANVAS_MARGIN - TEXT_HEIGHT;
      const gridY = y - HEADER_HEIGHT;

      if (gridX < 0 || gridY < 0) return null;

      const weekIndex = Math.floor(gridX / CELL_SIZE);
      const dayIndex = Math.floor(gridY / CELL_SIZE);

      if (weekIndex < 0 || weekIndex >= weeksData.length || dayIndex < 0 || dayIndex >= 7) return null;

      const day = weeksData[weekIndex]?.days[dayIndex] ?? null;
      return { day, position: { x: clientX, y: clientY } };
    },
    [weeksData]
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const result = getDayAtPosition(e.clientX, e.clientY);
      if (result) {
        onDayHover(result.day, result.position);
      } else {
        onDayHover(null, null);
      }
    },
    [getDayAtPosition, onDayHover]
  );

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const result = getDayAtPosition(e.clientX, e.clientY);
      if (result?.day) onDayClick(result.day);
    },
    [getDayAtPosition, onDayClick]
  );

  return (
    <div className="overflow-x-auto">
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={`Token usage contribution graph for ${year}. A list of the same days follows.`}
        className="cursor-pointer"
        onMouseMove={handleMouseMove}
        onMouseLeave={() => onDayHover(null, null)}
        onClick={handleClick}
        style={{ minWidth: canvasWidth }}
      />

      {/* A canvas cannot hold focusable children, so the day panel — the whole
          point of the graph — is unreachable by keyboard through the pixels.
          This is the same grid as a list of buttons, visually hidden but in the
          tab order, so every day the pointer can open the keyboard can too. */}
      <ul className="sr-only">
        {contributions.map((day) => (
          <li key={day.date}>
            <button type="button" onClick={() => onDayClick(day)}>
              {dayButtonLabel(day)}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function dayButtonLabel(day: DailyContribution): string {
  return `${day.date}: ${day.totals.tokens.toLocaleString("en-US")} tokens, ${day.totals.cost.toLocaleString(
    "en-US",
    { style: "currency", currency: "USD" }
  )}`;
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number, radius: number) {
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.lineTo(x + width - radius, y);
  ctx.quadraticCurveTo(x + width, y, x + width, y + radius);
  ctx.lineTo(x + width, y + height - radius);
  ctx.quadraticCurveTo(x + width, y + height, x + width - radius, y + height);
  ctx.lineTo(x + radius, y + height);
  ctx.quadraticCurveTo(x, y + height, x, y + height - radius);
  ctx.lineTo(x, y + radius);
  ctx.quadraticCurveTo(x, y, x + radius, y);
  ctx.closePath();
}
