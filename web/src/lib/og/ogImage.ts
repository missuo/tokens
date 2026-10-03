/**
 * Share-card URLs and the compact activity encoding they carry.
 *
 * `/api/og` is served with a one-year immutable cache, so the URL is the only
 * cache key that ever changes. Bump `OG_VERSION` whenever the card's design
 * changes, or every edge keeps handing out the old artwork.
 */
import type { DailyContribution } from "@/lib/types";

export const OG_VERSION = "2";

/** Columns (weeks) in the profile card's activity strip. */
export const OG_ACTIVITY_WEEKS = 52;
export const OG_ACTIVITY_DAYS = OG_ACTIVITY_WEEKS * 7;

export function ogImageUrl(params: Record<string, string | number | null | undefined> = {}): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === null || value === undefined || value === "") continue;
    search.set(key, String(value));
  }
  search.set("v", OG_VERSION);
  return `/api/og?${search.toString()}`;
}

/**
 * Encode the trailing `OG_ACTIVITY_DAYS` (ending today, UTC) as one digit per
 * day, 0–4. Levels are recomputed against the window itself rather than reusing
 * the profile's all-time `intensity`, which is scaled to the busiest day ever
 * and would leave a recent strip washed out for anyone with an old peak.
 *
 * Returns null when the window has no activity, so the card can omit the strip.
 */
export function encodeOgActivity(
  contributions: ReadonlyArray<{ date: string; totals: { tokens: number } }>,
  today: Date = new Date(),
): string | null {
  const byDate = new Map<string, number>();
  for (const day of contributions) {
    byDate.set(day.date, (byDate.get(day.date) ?? 0) + (day.totals.tokens || 0));
  }

  const end = Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate());
  const values: number[] = [];
  for (let i = OG_ACTIVITY_DAYS - 1; i >= 0; i--) {
    const key = new Date(end - i * 86_400_000).toISOString().slice(0, 10);
    values.push(byDate.get(key) ?? 0);
  }

  // Quartiles of the active days, not fractions of the peak: usage is heavy-
  // tailed, and one huge day otherwise flattens the rest of the year to level 1.
  const active = values.filter((v) => v > 0).sort((a, b) => a - b);
  if (active.length === 0) return null;
  const q = (p: number) => active[Math.min(active.length - 1, Math.floor(p * active.length))];
  const [q1, q2, q3] = [q(0.25), q(0.5), q(0.75)];
  return values
    .map((v) => (v <= 0 ? 0 : v <= q1 ? 1 : v <= q2 ? 2 : v <= q3 ? 3 : 4))
    .join("");
}

/** Clients ordered by tokens used across `contributions`, most first. */
export function topOgClients(
  contributions: ReadonlyArray<Pick<DailyContribution, "clients">>,
  limit: number,
): string[] {
  const totals = new Map<string, number>();
  for (const day of contributions) {
    for (const entry of day.clients) {
      let tokens = 0;
      for (const value of Object.values(entry.tokens)) tokens += value || 0;
      totals.set(entry.client, (totals.get(entry.client) ?? 0) + tokens);
    }
  }
  return [...totals.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, limit)
    .map(([client]) => client);
}

export function decodeOgActivity(value: string | null): number[] | null {
  if (!value || value.length !== OG_ACTIVITY_DAYS || !/^[0-4]+$/.test(value)) return null;
  return Array.from(value, Number);
}
