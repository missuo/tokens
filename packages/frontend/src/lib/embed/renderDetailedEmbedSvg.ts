/**
 * "detailed" embed template — today's usage broken down per client and per
 * model, the embed analogue of the profile page's BreakdownPanel "Detailed
 * Breakdown". Always shows the current UTC day; unaffected by the `today`
 * toggle (which gates the 4th metric on the other templates).
 */
import type { UserEmbedStats, EmbedTodayUsage, EmbedTodayClient } from "./getUserEmbedStats";
import type { ClientType } from "../types";
import { formatNumber, formatCurrency } from "../format";
import { SOURCE_DISPLAY_NAMES, SOURCE_COLORS } from "../constants";
import {
  type EmbedTheme,
  type EmbedColorName,
  type EmbedNumberFormat,
  resolvePalette,
  FIGTREE_FONT_STACK,
  FIGTREE_FONT_IMPORT,
  MONO_FONT_STACK,
  brandIcon,
  formatDateLabel,
  escapeXml,
} from "./embedShared";

export interface RenderDetailedEmbedOptions {
  theme?: EmbedTheme;
  color?: EmbedColorName | null;
  tokensFormat?: EmbedNumberFormat;
  costFormat?: EmbedNumberFormat;
  today?: EmbedTodayUsage | null;
}

const W = 680;
const PAD = 24;
const INNER = W - PAD * 2;
const RX = 16;
const MAX_CLIENTS = 6;
const MAX_MODELS_PER_CLIENT = 4;

function clientLabel(source: string): string {
  return SOURCE_DISPLAY_NAMES[source as ClientType] ?? source;
}

function clientColor(source: string, fallback: string): string {
  return SOURCE_COLORS[source as ClientType] ?? fallback;
}

function truncate(value: string, max: number): string {
  return value.length > max ? `${value.slice(0, max - 1)}…` : value;
}

/** Today's UTC date as "Mon D, YYYY". */
function formatTodayDate(date: string): string {
  const parsed = new Date(`${date}T00:00:00.000Z`);
  if (Number.isNaN(parsed.getTime())) return date;
  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  }).format(parsed);
}

function modelSubLine(model: EmbedTodayClient["models"][number]): string {
  const bits: string[] = [];
  if (model.input > 0) bits.push(`in ${formatNumber(model.input, true)}`);
  if (model.output > 0) bits.push(`out ${formatNumber(model.output, true)}`);
  const cache = model.cacheRead + model.cacheWrite;
  if (cache > 0) bits.push(`cache ${formatNumber(cache, true)}`);
  if (model.reasoning > 0) bits.push(`reason ${formatNumber(model.reasoning, true)}`);
  bits.push(`${model.messages.toLocaleString("en-US")} msg${model.messages === 1 ? "" : "s"}`);
  return bits.join("  ·  ");
}

export function renderDetailedEmbedSvg(
  data: UserEmbedStats,
  options: RenderDetailedEmbedOptions = {},
): string {
  const theme: EmbedTheme = options.theme === "light" ? "light" : "dark";
  const palette = resolvePalette(theme, options.color ?? null);
  const tokensCompact = (options.tokensFormat ?? "compact") === "compact";
  const costCompact = (options.costFormat ?? "compact") === "compact";

  const today = options.today ?? null;
  const username = `@${data.user.username}`;
  const dateLabel = today ? `${formatTodayDate(today.date)} · UTC` : "Today · UTC";

  // Body is built with a running cursor so the card height adapts to content.
  const body: string[] = [];
  const add = (s: string) => body.push(s);

  // ---- Header (shared by the populated and empty states) ----
  add(`  ${brandIcon(PAD, 32, palette.brand)}`);
  add(`  <text x="${PAD + 18}" y="32" fill="${palette.muted}" font-size="12" font-weight="600" font-family="${FIGTREE_FONT_STACK}">Tokens · Daily Detail</text>`);
  add(`  <text x="${W - PAD}" y="32" fill="${palette.text}" font-size="14" font-weight="700" text-anchor="end" font-family="${FIGTREE_FONT_STACK}">${escapeXml(username)}</text>`);
  add(`  <text x="${PAD}" y="62" fill="${palette.title}" font-size="20" font-weight="800" font-family="${FIGTREE_FONT_STACK}">Today's usage</text>`);
  add(`  <text x="${W - PAD}" y="62" fill="${palette.muted}" font-size="12" font-weight="600" text-anchor="end" font-family="${FIGTREE_FONT_STACK}">${escapeXml(dateLabel)}</text>`);
  add(`  <rect x="${PAD}" y="76" width="${INNER}" height="1" fill="url(#dt-divider)"/>`);

  const clients = today?.clients ?? [];
  let height: number;

  if (clients.length === 0) {
    // ---- Empty state ----
    add(`  <text x="${W / 2}" y="132" fill="${palette.text}" font-size="15" font-weight="700" text-anchor="middle" font-family="${FIGTREE_FONT_STACK}">No usage recorded today (UTC)</text>`);
    add(`  <text x="${W / 2}" y="156" fill="${palette.muted}" font-size="12" text-anchor="middle" font-family="${FIGTREE_FONT_STACK}">Submit today's usage to see a per-client breakdown here.</text>`);
    height = 220;
  } else {
    // ---- Summary strip ----
    const modelIds = new Set<string>();
    for (const c of clients) for (const m of c.models) modelIds.add(m.modelId);
    const stat = (value: string, label: string, color: string): string =>
      `<tspan fill="${color}" font-weight="800">${escapeXml(value)}</tspan><tspan fill="${palette.muted}" font-weight="600"> ${label}</tspan>`;
    const sep = `<tspan fill="${palette.divider}">      </tspan>`;
    add(`  <text x="${PAD}" y="102" font-size="13" font-family="${FIGTREE_FONT_STACK}" xml:space="preserve">${[
      stat(formatNumber(today!.tokens, tokensCompact), "tokens", palette.tokenEnd),
      stat(formatCurrency(today!.cost, costCompact), "spent", palette.cost),
      stat(String(clients.length), `client${clients.length === 1 ? "" : "s"}`, palette.text),
      stat(String(modelIds.size), `model${modelIds.size === 1 ? "" : "s"}`, palette.text),
    ].join(sep)}</text>`);

    // ---- Per-client sections ----
    let y = 128;
    const shownClients = clients.slice(0, MAX_CLIENTS);
    for (const client of shownClients) {
      const color = clientColor(client.source, palette.brand);
      add(`  <circle cx="${PAD + 5}" cy="${y - 4}" r="5" fill="${color}"/>`);
      add(`  <text x="${PAD + 18}" y="${y}" fill="${palette.text}" font-size="14" font-weight="700" font-family="${FIGTREE_FONT_STACK}">${escapeXml(clientLabel(client.source))}</text>`);
      add(`  <text x="${W - PAD}" y="${y}" fill="${palette.cost}" font-size="13" font-weight="700" text-anchor="end" font-family="${FIGTREE_FONT_STACK}">${escapeXml(formatCurrency(client.cost, costCompact))}</text>`);
      y += 22;

      const shownModels = client.models.slice(0, MAX_MODELS_PER_CLIENT);
      for (const model of shownModels) {
        add(`  <text x="${PAD + 18}" y="${y}" fill="${palette.text}" font-size="12" font-weight="600" font-family="${MONO_FONT_STACK}">${escapeXml(truncate(model.modelId, 44))}</text>`);
        add(`  <text x="${W - PAD}" y="${y}" fill="${palette.brand}" font-size="12" font-weight="700" text-anchor="end" font-family="${MONO_FONT_STACK}">${escapeXml(formatCurrency(model.cost, costCompact))}</text>`);
        y += 15;
        add(`  <text x="${PAD + 18}" y="${y}" fill="${palette.muted}" font-size="10" font-family="${MONO_FONT_STACK}">${escapeXml(modelSubLine(model))}</text>`);
        y += 19;
      }

      const hiddenModels = client.models.length - shownModels.length;
      if (hiddenModels > 0) {
        add(`  <text x="${PAD + 18}" y="${y}" fill="${palette.muted}" font-size="11" font-family="${FIGTREE_FONT_STACK}">+${hiddenModels} more model${hiddenModels === 1 ? "" : "s"}</text>`);
        y += 18;
      }
      y += 10;
    }

    const hiddenClients = clients.length - shownClients.length;
    if (hiddenClients > 0) {
      add(`  <text x="${PAD}" y="${y}" fill="${palette.muted}" font-size="11" font-weight="600" font-family="${FIGTREE_FONT_STACK}">+${hiddenClients} more client${hiddenClients === 1 ? "" : "s"}</text>`);
      y += 18;
    }

    height = y + 26;
  }

  // ---- Footer ----
  const updated = escapeXml(formatDateLabel(data.stats.updatedAt));
  const footerY = height - 14;
  add(`  <rect x="${PAD}" y="${footerY - 20}" width="${INNER}" height="1" fill="url(#dt-divider)"/>`);
  add(`  <text x="${PAD}" y="${footerY}" fill="${palette.muted}" font-size="11" font-family="${FIGTREE_FONT_STACK}">${updated}</text>`);
  add(`  <text x="${W - PAD}" y="${footerY}" fill="${palette.muted}" font-size="11" text-anchor="end" font-family="${FIGTREE_FONT_STACK}">tokens.ci/u/${escapeXml(data.user.username)}</text>`);

  return `<?xml version="1.0" encoding="UTF-8"?>
<svg width="${W}" height="${height}" viewBox="0 0 ${W} ${height}" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Tokens daily usage detail for ${escapeXml(username)}">
  <defs>
    <style>@import url('${FIGTREE_FONT_IMPORT}');</style>
    <linearGradient id="dt-bg" x1="0" y1="0" x2="${W}" y2="${height}" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="${palette.bgStart}"/>
      <stop offset="1" stop-color="${palette.bgEnd}"/>
    </linearGradient>
    <radialGradient id="dt-glow" cx="0.85" cy="0.05" r="0.8">
      <stop offset="0" stop-color="${palette.glowColor}" stop-opacity="${palette.glowOpacity + 0.03}"/>
      <stop offset="1" stop-color="${palette.glowColor}" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="dt-divider" x1="${PAD}" y1="0" x2="${W - PAD}" y2="0" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="${palette.divider}" stop-opacity="0"/>
      <stop offset="0.5" stop-color="${palette.divider}" stop-opacity="0.6"/>
      <stop offset="1" stop-color="${palette.divider}" stop-opacity="0"/>
    </linearGradient>
    <clipPath id="dt-clip"><rect width="${W}" height="${height}" rx="${RX}"/></clipPath>
  </defs>
  <rect width="${W}" height="${height}" rx="${RX}" fill="url(#dt-bg)"/>
  <rect width="${W}" height="${height}" rx="${RX}" fill="url(#dt-glow)" clip-path="url(#dt-clip)"/>
  <rect x="0.5" y="0.5" width="${W - 1}" height="${height - 1}" rx="${RX - 0.5}" fill="none" stroke="${palette.border}"/>
${body.join("\n")}
</svg>`;
}
