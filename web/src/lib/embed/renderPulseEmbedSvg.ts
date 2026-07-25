/**
 * "pulse" embed template — the year of daily usage drawn as a single smooth
 * area curve (the hero), under a compact stat line. Each point is one week's
 * summed activity, so the shape reads as a usage pulse rather than a calendar.
 * The busiest week is marked. No template in the set charts usage as a trend
 * line, so this is the data-visualization-forward option.
 */
import type {
  UserEmbedStats,
  EmbedContributionDay,
  EmbedTodayUsage,
} from "./getUserEmbedStats";
import { formatNumber, formatCurrency } from "../format";
import {
  type EmbedTheme,
  type EmbedColorName,
  type EmbedNumberFormat,
  type EmbedRankFormat,
  resolvePalette,
  layoutContributions,
  formatDateLabel,
  formatRank,
  formatTodayInline,
  brandIcon,
  FIGTREE_FONT_STACK,
  FIGTREE_FONT_IMPORT,
  escapeXml,
} from "./embedShared";

export interface RenderPulseEmbedOptions {
  theme?: EmbedTheme;
  color?: EmbedColorName | null;
  sortBy?: "tokens" | "cost";
  tokensFormat?: EmbedNumberFormat;
  costFormat?: EmbedNumberFormat;
  rankFormat?: EmbedRankFormat;
  contributions?: EmbedContributionDay[] | null;
  today?: EmbedTodayUsage | null;
}

const W = 900;
const PAD = 28;
const INNER = W - PAD * 2;

/** Catmull-Rom → cubic-bezier smoothing through the given points. */
function smoothPath(pts: { x: number; y: number }[]): string {
  if (pts.length === 0) return "";
  if (pts.length === 1) return `M ${pts[0].x.toFixed(1)} ${pts[0].y.toFixed(1)}`;
  let d = `M ${pts[0].x.toFixed(1)} ${pts[0].y.toFixed(1)}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i - 1] ?? pts[i];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[i + 2] ?? p2;
    const c1x = p1.x + (p2.x - p0.x) / 6;
    const c1y = p1.y + (p2.y - p0.y) / 6;
    const c2x = p2.x - (p3.x - p1.x) / 6;
    const c2y = p2.y - (p3.y - p1.y) / 6;
    d += ` C ${c1x.toFixed(1)} ${c1y.toFixed(1)}, ${c2x.toFixed(1)} ${c2y.toFixed(1)}, ${p2.x.toFixed(1)} ${p2.y.toFixed(1)}`;
  }
  return d;
}

export function renderPulseEmbedSvg(
  data: UserEmbedStats,
  options: RenderPulseEmbedOptions = {},
): string {
  const theme: EmbedTheme = options.theme === "light" ? "light" : "dark";
  const palette = resolvePalette(theme, options.color ?? null);
  const tokensFormat: EmbedNumberFormat = options.tokensFormat ?? "compact";
  const costFormat: EmbedNumberFormat = options.costFormat ?? "compact";
  const sortBy = options.sortBy === "cost" ? "cost" : "tokens";

  const layout = layoutContributions(options.contributions ?? []);
  const tokens = formatNumber(data.stats.totalTokens, tokensFormat === "compact");
  const cost = formatCurrency(data.stats.totalCost, costFormat === "compact");
  const rankText = data.stats.rank
    ? formatRank(data.stats.rank, data.stats.rankTotal ?? null, options.rankFormat)
    : "Unranked";
  const updated = escapeXml(formatDateLabel(data.stats.updatedAt));

  // One value per week: the summed daily intensity for that column. Built from
  // the same cells the calendar uses, so the curve and a heatmap would agree.
  const weeks = Math.max(layout.numWeeks, 1);
  const weekly = new Array(weeks).fill(0) as number[];
  for (const c of layout.cells) weekly[c.week] += c.intensity;
  const maxWeekly = Math.max(1, ...weekly);
  let peak = 0;
  for (let w = 1; w < weeks; w++) if (weekly[w] > weekly[peak]) peak = w;

  const chartTop = 98;
  const base = 206;
  const chartH = base - chartTop;
  const xFor = (w: number): number => (weeks <= 1 ? PAD : PAD + (w / (weeks - 1)) * INNER);
  const yFor = (v: number): number => base - (v / maxWeekly) * chartH;
  const points = weekly.map((v, w) => ({ x: xFor(w), y: yFor(v) }));
  const curve = smoothPath(points);

  const footerY = base + 34;
  const height = footerY + 14;

  const parts: string[] = [];
  const add = (s: string) => parts.push(s);

  add(`<?xml version="1.0" encoding="UTF-8"?>`);
  add(`<svg width="${W}" height="${height}" viewBox="0 0 ${W} ${height}" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Tokens daily usage for @${escapeXml(data.user.username)}">`);
  add(`  <defs>`);
  add(`    <style>@import url('${FIGTREE_FONT_IMPORT}');</style>`);
  add(`    <linearGradient id="bg" x1="0" y1="0" x2="${W}" y2="${height}" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="${palette.bgStart}"/><stop offset="1" stop-color="${palette.bgEnd}"/></linearGradient>`);
  add(`    <radialGradient id="glow" cx="0.9" cy="0.0" r="0.7"><stop offset="0" stop-color="${palette.glowColor}" stop-opacity="${palette.glowOpacity + 0.03}"/><stop offset="1" stop-color="${palette.glowColor}" stop-opacity="0"/></radialGradient>`);
  add(`    <linearGradient id="area" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${palette.accentTokens}" stop-opacity="0.42"/><stop offset="1" stop-color="${palette.accentTokens}" stop-opacity="0.02"/></linearGradient>`);
  add(`    <clipPath id="card-clip"><rect width="${W}" height="${height}" rx="16"/></clipPath>`);
  add(`  </defs>`);
  add(`  <rect width="${W}" height="${height}" rx="16" fill="url(#bg)"/>`);
  add(`  <rect width="${W}" height="${height}" rx="16" fill="url(#glow)" clip-path="url(#card-clip)"/>`);
  add(`  <rect x="0.5" y="0.5" width="${W - 1}" height="${height - 1}" rx="15.5" fill="none" stroke="${palette.border}"/>`);

  // Header: brand + username (left), demoted rank (right, muted).
  add(`  ${brandIcon(PAD, 42, palette.brand)}`);
  add(`  <text x="${PAD + 20}" y="42" fill="${palette.title}" font-size="16" font-weight="700" font-family="${FIGTREE_FONT_STACK}">@${escapeXml(data.user.username)}</text>`);
  add(`  <text x="${W - PAD}" y="42" text-anchor="end" fill="${palette.muted}" font-size="11" font-weight="600" font-family="${FIGTREE_FONT_STACK}" letter-spacing="0.4">RANK ${escapeXml(rankText)}</text>`);

  // Stat line.
  const stat = (label: string, value: string, color: string): string =>
    `<tspan fill="${color}" font-weight="800" font-size="22">${escapeXml(value)}</tspan><tspan fill="${palette.muted}" font-weight="600" font-size="13"> ${label}</tspan>`;
  const sep = `<tspan fill="${palette.divider}">      </tspan>`;
  const statLine = [
    stat("tokens", tokens, palette.accentTokens),
    stat(`spent (${sortBy})`, cost, palette.cost),
    stat("active days", String(layout.activeDays), palette.text),
  ];
  if (options.today) {
    statLine.push(
      stat(
        "today",
        formatTodayInline(
          options.today,
          tokensFormat === "compact",
          costFormat === "compact",
        ),
        palette.tokenEnd,
      ),
    );
  }
  add(`  <text x="${PAD}" y="78" font-family="${FIGTREE_FONT_STACK}" xml:space="preserve">${statLine.join(sep)}</text>`);

  // Faint gridlines.
  for (let g = 1; g <= 2; g++) {
    const y = (base - (g / 3) * chartH).toFixed(1);
    add(`  <line x1="${PAD}" y1="${y}" x2="${W - PAD}" y2="${y}" stroke="${palette.divider}" stroke-opacity="0.4" stroke-dasharray="2 4"/>`);
  }

  // The usage curve — the hero element.
  add(`  <path d="${curve} L ${(W - PAD).toFixed(1)} ${base} L ${PAD} ${base} Z" fill="url(#area)"/>`);
  add(`  <path d="${curve}" fill="none" stroke="${palette.accentTokens}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>`);
  add(`  <line x1="${PAD}" y1="${base}" x2="${W - PAD}" y2="${base}" stroke="${palette.border}"/>`);

  // Peak marker.
  const pk = points[peak];
  add(`  <line x1="${pk.x.toFixed(1)}" y1="${pk.y.toFixed(1)}" x2="${pk.x.toFixed(1)}" y2="${base}" stroke="${palette.accentTokens}" stroke-opacity="0.35"/>`);
  add(`  <circle cx="${pk.x.toFixed(1)}" cy="${pk.y.toFixed(1)}" r="3.5" fill="${palette.accentTokens}" stroke="${palette.bgStart}" stroke-width="1.5"/>`);
  add(`  <text x="${pk.x.toFixed(1)}" y="${(pk.y - 9).toFixed(1)}" text-anchor="middle" fill="${palette.muted}" font-size="9" font-weight="700" font-family="${FIGTREE_FONT_STACK}">peak</text>`);

  // Month axis.
  for (const m of layout.months) {
    add(`  <text x="${xFor(m.week).toFixed(1)}" y="${base + 15}" fill="${palette.muted}" font-size="9" font-family="${FIGTREE_FONT_STACK}">${m.label}</text>`);
  }

  // Footer.
  add(`  <text x="${PAD}" y="${footerY}" fill="${palette.muted}" font-size="11" font-family="${FIGTREE_FONT_STACK}">Daily Claude usage · ${layout.activeDays} active days · ${updated}</text>`);
  add(`  <text x="${W - PAD}" y="${footerY}" text-anchor="end" fill="${palette.muted}" font-size="11" font-family="${FIGTREE_FONT_STACK}">tokens.ci/u/${escapeXml(data.user.username)}</text>`);
  add(`</svg>`);

  return parts.join("\n");
}
