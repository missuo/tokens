"use client";

import { useRef, useEffect, useCallback, useState, useMemo } from "react";
import { useTheme } from "next-themes";
import type { DailyContribution, GraphColorPalette, TooltipPosition } from "@/lib/types";
import { getGradeColor } from "@/lib/themes";
import { groupByWeek, hexToNumber, formatCurrency, formatTokenCount } from "@/lib/utils";
import { formatContributionDate } from "@/lib/date-utils";
import { CUBE_SIZE, MAX_CUBE_HEIGHT, MIN_CUBE_HEIGHT, ISO_CANVAS_WIDTH, ISO_CANVAS_HEIGHT } from "@/lib/constants";

interface TokenGraph3DProps {
  contributions: DailyContribution[];
  palette: GraphColorPalette;
  year: string;
  maxTokens: number;
  totalCost: number;
  totalTokens: number;
  activeDays: number;
  bestDay: DailyContribution | null;
  currentStreak: number;
  longestStreak: number;
  dateRange: { start: string; end: string };
  onDayHover: (day: DailyContribution | null, position: TooltipPosition | null) => void;
  onDayClick: (day: DailyContribution | null) => void;
}

export function TokenGraph3D({
  contributions,
  palette,
  year,
  maxTokens,
  totalCost,
  totalTokens,
  activeDays,
  bestDay,
  currentStreak,
  longestStreak,
  dateRange,
  onDayHover,
  onDayClick,
}: TokenGraph3DProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [obeliskLoaded, setObeliskLoaded] = useState(false);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const obeliskRef = useRef<any>(null);
  const weeksData = useMemo(() => groupByWeek(contributions, year), [contributions, year]);
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === "dark";

  useEffect(() => {
    async function loadObelisk() {
      try {
        const obeliskModule = await import("obelisk.js");
        obeliskRef.current = obeliskModule.default || obeliskModule;
        setObeliskLoaded(true);
      } catch (err) {
        console.error("Failed to load obelisk.js:", err);
      }
    }
    loadObelisk();
  }, []);

  useEffect(() => {
    if (!obeliskLoaded || !obeliskRef.current) return;

    const canvas = canvasRef.current;
    if (!canvas) return;

    const obelisk = obeliskRef.current;

    canvas.width = ISO_CANVAS_WIDTH;
    canvas.height = ISO_CANVAS_HEIGHT;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.fillStyle = isDark ? "#12151e" : "#ffffff";
    ctx.fillRect(0, 0, ISO_CANVAS_WIDTH, ISO_CANVAS_HEIGHT);

    const point = new obelisk.Point(130, 90);
    const pixelView = new obelisk.PixelView(canvas, point);

    const GH_OFFSET = 14;
    let transform = GH_OFFSET;

    for (let weekIndex = 0; weekIndex < weeksData.length; weekIndex++) {
      const week = weeksData[weekIndex];
      const x = transform / (GH_OFFSET + 1);
      transform += GH_OFFSET;

      let offsetY = 0;
      for (let dayIndex = 0; dayIndex < 7; dayIndex++) {
        const day = week.days[dayIndex];
        const y = offsetY / GH_OFFSET;
        offsetY += 13;

        let cubeHeight = MIN_CUBE_HEIGHT;
        if (day && maxTokens > 0) {
          cubeHeight = MIN_CUBE_HEIGHT + Math.floor((MAX_CUBE_HEIGHT / maxTokens) * day.totals.tokens);
        }

        const intensity = day?.intensity ?? 0;
        const colorHex = getGradeColor(palette, intensity);
        const resolvedColor = colorHex.startsWith("var(") ? (isDark ? "#171b26" : "#f4f5f7") : colorHex;
        const colorNum = hexToNumber(resolvedColor);

        const dimension = new obelisk.CubeDimension(CUBE_SIZE, CUBE_SIZE, Math.max(cubeHeight, MIN_CUBE_HEIGHT));
        const color = new obelisk.CubeColor().getByHorizontalColor(colorNum);
        const cube = new obelisk.Cube(dimension, color, false);
        const p3d = new obelisk.Point3D(CUBE_SIZE * x, CUBE_SIZE * y, 0);

        pixelView.renderObject(cube, p3d);
      }
    }
  }, [obeliskLoaded, palette, maxTokens, weeksData, isDark]);

  const getDayAtPosition = useCallback(
    (clientX: number, clientY: number): { day: DailyContribution | null; position: TooltipPosition } | null => {
      const canvas = canvasRef.current;
      if (!canvas) return null;

      const rect = canvas.getBoundingClientRect();
      const scaleX = ISO_CANVAS_WIDTH / rect.width;
      const scaleY = ISO_CANVAS_HEIGHT / rect.height;
      const x = (clientX - rect.left) * scaleX;
      const y = (clientY - rect.top) * scaleY;

      const isoX = (x - 130) / (CUBE_SIZE * 0.7);
      const isoY = (y - 90) / (CUBE_SIZE * 0.35) - isoX;

      const weekIndex = Math.floor(isoX);
      const dayIndex = Math.floor(isoY);

      if (weekIndex < 0 || weekIndex >= weeksData.length || dayIndex < 0 || dayIndex >= 7) return null;

      const day = weeksData[weekIndex]?.days[dayIndex] ?? null;
      return { day, position: { x: clientX, y: clientY } };
    },
    [weeksData],
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const result = getDayAtPosition(e.clientX, e.clientY);
      onDayHover(result ? result.day : null, result ? result.position : null);
    },
    [getDayAtPosition, onDayHover],
  );

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const result = getDayAtPosition(e.clientX, e.clientY);
      if (result?.day) onDayClick(result.day);
    },
    [getDayAtPosition, onDayClick],
  );

  const aspect = { aspectRatio: `${ISO_CANVAS_WIDTH} / ${ISO_CANVAS_HEIGHT}` };

  if (!obeliskLoaded) {
    return (
      <div ref={containerRef} className="flex w-full items-center justify-center bg-background" style={aspect}>
        <div className="animate-pulse text-muted">Loading 3D view...</div>
      </div>
    );
  }

  const statValue = "block text-2xl font-bold leading-tight";
  const statLabel = "block text-xs font-bold text-foreground";
  const statSubtext = "hidden text-xs text-muted sm:block";
  const statsBox = "flex justify-between rounded-md border border-line bg-surface px-1 md:px-2";

  return (
    <div ref={containerRef} className="relative w-full">
      <canvas
        ref={canvasRef}
        className="h-auto w-full cursor-pointer"
        onMouseMove={handleMouseMove}
        onMouseLeave={() => onDayHover(null, null)}
        onClick={handleClick}
        style={aspect}
      />

      <div className="absolute top-3 right-5">
        <h5 className="mb-1 text-sm font-semibold text-foreground">Token Usage</h5>
        <div className={statsBox}>
          <div className="p-2">
            <span className={statValue} style={{ color: palette.grade1 }}>{formatCurrency(totalCost)}</span>
            <span className={statLabel}>Total</span>
            <span className={statSubtext}>{dateRange.start} → {dateRange.end}</span>
          </div>
          <div className="hidden p-2 min-[1280px]:block">
            <span className={statValue} style={{ color: palette.grade1 }}>{formatTokenCount(totalTokens)}</span>
            <span className={statLabel}>Tokens</span>
            <span className={statSubtext}>{activeDays} active days</span>
          </div>
          {bestDay && (
            <div className="p-2">
              <span className={statValue} style={{ color: palette.grade1 }}>{formatCurrency(bestDay.totals.cost)}</span>
              <span className={statLabel}>Best day</span>
              <span className={statSubtext}>{formatContributionDate(bestDay).split(",")[0]}</span>
            </div>
          )}
        </div>
        <p className="mt-1 text-right text-xs text-muted">
          Average: <span className="font-bold" style={{ color: palette.grade1 }}>{formatCurrency(activeDays > 0 ? totalCost / activeDays : 0)}</span> / day
        </p>
      </div>

      <div className="absolute bottom-6 left-5">
        <h5 className="mb-1 text-sm font-semibold text-foreground">Streaks</h5>
        <div className={statsBox}>
          <div className="p-2">
            <span className={statValue} style={{ color: palette.grade1 }}>{longestStreak} <span className="text-base">days</span></span>
            <span className={statLabel}>Longest</span>
          </div>
          <div className="p-2">
            <span className={statValue} style={{ color: palette.grade1 }}>{currentStreak} <span className="text-base">days</span></span>
            <span className={statLabel}>Current</span>
          </div>
        </div>
      </div>
    </div>
  );
}
