import type { Locale } from "@/lib/i18n/config";

/**
 * Formatters take the locale last and default to `en`.
 *
 * The default matters: badges, embeds and the OG renderer format numbers in
 * route handlers and static SVG where there is no request locale and English
 * grouping is what the artwork was designed against. Only the interactive
 * pages pass a locale, and they get it from `useLocale()`.
 */
export function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

function safeNumber(value: number): number {
  return Number.isFinite(value) ? value : 0;
}

export function formatCompact(
  value: number,
  kind: "number" | "currency",
  locale: Locale = "en",
): string {
  const clamped = Math.max(0, safeNumber(value));

  if (kind === "currency") {
    const formatted = new Intl.NumberFormat(locale, {
      notation: "compact",
      maximumFractionDigits: clamped >= 100 ? 1 : 2,
    }).format(clamped);
    // The symbol is prepended rather than formatted as a currency because
    // `currencyDisplay: "narrowSymbol"` plus compact notation produces "$1.2K"
    // in English but "1.2K US$" in some locales, and these figures sit in
    // fixed-width columns.
    return `$${formatted}`;
  }

  return new Intl.NumberFormat(locale, {
    notation: "compact",
    maximumFractionDigits: clamped >= 100 ? 1 : 2,
  }).format(Math.round(clamped));
}

export function formatNumber(value: number, compact = false, locale: Locale = "en"): string {
  if (compact) return formatCompact(value, "number", locale);
  return new Intl.NumberFormat(locale).format(Math.max(0, Math.round(safeNumber(value))));
}

export function formatCurrency(value: number, compact = false, locale: Locale = "en"): string {
  if (compact) return formatCompact(value, "currency", locale);
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(Math.max(0, safeNumber(value)));
}

/**
 * Format a 0–100 share with one decimal. A non-zero share that would round to
 * "0.0%" shows as "<0.1%" so a used category never reads as unused.
 */
export function formatPercent(percent: number, locale: Locale = "en"): string {
  if (percent > 0 && percent < 0.05) return `<${formatNumber(0.1, false, locale)}%`;
  // One decimal, but through the locale so the decimal separator follows it.
  const digits = new Intl.NumberFormat(locale, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  }).format(percent);
  return `${digits}%`;
}

/**
 * Format 0–100 shares as whole percents that add up to their own rounded total
 * (largest remainder), so the parts of a split never disagree with the whole.
 * A non-zero share that rounds to 0 shows as "<1%".
 */
export function formatWholePercents(
  percents: readonly number[],
  locale: Locale = "en",
): string[] {
  const whole = percents.map((percent) => Math.floor(percent));
  let left =
    Math.round(percents.reduce((sum, percent) => sum + percent, 0)) -
    whole.reduce((sum, value) => sum + value, 0);
  const byRemainder = percents
    .map((percent, index) => ({ index, remainder: percent - whole[index] }))
    .sort((a, b) => b.remainder - a.remainder);
  for (const { index } of byRemainder) {
    if (left <= 0) break;
    whole[index] += 1;
    left -= 1;
  }
  return whole.map((value, index) =>
    value === 0 && percents[index] > 0
      ? `<${formatNumber(1, false, locale)}%`
      : `${formatNumber(value, false, locale)}%`,
  );
}

/**
 * Format an ISO timestamp as a relative time — "5m ago", "3h ago", "12d ago".
 *
 * Delegates the wording to `Intl.RelativeTimeFormat` so "yesterday" is 昨天 is
 * 昨日 without a second dictionary here. `numeric: "always"` keeps the
 * six-unit ladder below intact: "1 day ago" rather than "yesterday", because
 * these strings sit in a column and a qualitative answer would break scanning.
 *
 * An absent or invalid timestamp returns an empty string — callers render
 * `t("common.never")` for it, so that word is translated like any other copy
 * instead of being baked into a formatter.
 *
 * `now` is injectable for tests; a time inside the last minute reads "0
 * seconds ago"-ish rather than a future tense.
 */
export function formatRelativeTime(
  iso: string | null | undefined,
  locale: Locale = "en",
  now: Date = new Date(),
): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";

  const format = (value: number, unit: Intl.RelativeTimeFormatUnit) =>
    new Intl.RelativeTimeFormat(locale, { numeric: "always", style: "short" }).format(
      -value,
      unit,
    );

  const diffMs = Math.max(0, now.getTime() - date.getTime());

  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 60) return format(minutes, "minute");

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return format(hours, "hour");

  const days = Math.floor(hours / 24);
  if (days < 30) return format(days, "day");
  if (days < 365) return format(Math.floor(days / 30), "month");

  return format(Math.floor(days / 365), "year");
}

/**
 * Format milliseconds into a human-readable duration string.
 * e.g. 3661000 → "1h 1m", 90000 → "1m 30s", 500 → "<1s"
 */
export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms <= 0) return "—";

  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
  }
  if (minutes > 0) {
    return seconds > 0 && minutes < 10 ? `${minutes}m ${seconds}s` : `${minutes}m`;
  }
  if (seconds > 0) {
    return `${seconds}s`;
  }
  return "<1s";
}
