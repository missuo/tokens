import type { Locale } from "@/lib/i18n/config";

/**
 * Day labels go through `Intl`, not a `date-fns` pattern.
 *
 * `date-fns` formats in English unless you also carry its per-locale data into
 * the bundle; `Intl` is already in the platform and gets the month name and the
 * word order right for free — `medium` is "Nov 2, 2026" in English and 2026年11月2日
 * in Japanese, which is the whole point.
 *
 * The date is built as a local-time `Date` from the parts so a `2026-11-02`
 * string does not become the previous day west of UTC.
 */
/**
 * Month and day only, for the "best day" readouts where the year is noise.
 *
 * Its own helper rather than `formatContributionDate(...).split(",")`: English
 * has a comma to split on and Japanese does not, so the split left `246` in
 * "2026年11月2日" behind.
 */
export function formatContributionDayMonth(
  contrib: { date: string; timestampMs?: number | null },
  locale: Locale = "en",
): string {
  const [year, month, day] = getContributionLocalDate(contrib).split("-").map(Number);
  return new Intl.DateTimeFormat(locale, { month: "short", day: "numeric" }).format(
    new Date(year, month - 1, day),
  );
}

function formatDateFromString(dateStr: string, locale: Locale): string {
  return formatParts(dateStr, locale, "medium");
}

function formatDateFullFromString(dateStr: string, locale: Locale): string {
  return formatParts(dateStr, locale, "long");
}

function formatParts(
  dateStr: string,
  locale: Locale,
  dateStyle: "medium" | "long",
): string {
  const [year, month, day] = dateStr.split("-").map(Number);
  return new Intl.DateTimeFormat(locale, { dateStyle }).format(
    new Date(year, month - 1, day),
  );
}

function getContributionLocalDate(contrib: { date: string; timestampMs?: number | null }): string {
  if (contrib.timestampMs != null) {
    const d = new Date(contrib.timestampMs);
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, "0");
    const day = String(d.getDate()).padStart(2, "0");
    return `${y}-${m}-${day}`;
  }
  return contrib.date;
}

export function formatContributionDate(
  contrib: { date: string; timestampMs?: number | null },
  locale: Locale = "en",
): string {
  return formatDateFromString(getContributionLocalDate(contrib), locale);
}

export function formatContributionDateFull(
  contrib: { date: string; timestampMs?: number | null },
  locale: Locale = "en",
): string {
  return formatDateFullFromString(getContributionLocalDate(contrib), locale);
}
