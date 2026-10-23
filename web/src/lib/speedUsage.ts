/**
 * Inference speed metadata attached to a client/model contribution.
 *
 * Entries count only usage whose speed is known. Whatever part of a
 * contribution's total tokens/messages is not covered by entries is Unknown —
 * never implicitly Standard. Counters are raw totals (every token category),
 * never speed-adjusted costs.
 */

export const INFERENCE_SPEEDS = ["standard", "fast", "ultrafast"] as const;
export const SPEED_SOURCES = ["reported", "selected"] as const;
export const MAX_SPEED_USAGE_ENTRIES = INFERENCE_SPEEDS.length * SPEED_SOURCES.length;

export type InferenceSpeed = (typeof INFERENCE_SPEEDS)[number];
export type SpeedSource = (typeof SPEED_SOURCES)[number];

export interface SpeedUsage {
  speed: InferenceSpeed;
  source: SpeedSource;
  tokens: number;
  messages: number;
}

function pairRank(entry: Pick<SpeedUsage, "speed" | "source">): number {
  return (
    INFERENCE_SPEEDS.indexOf(entry.speed) * SPEED_SOURCES.length +
    SPEED_SOURCES.indexOf(entry.source)
  );
}

/**
 * Sum any number of speed breakdowns per speed+source pair and return them in
 * canonical (speed, source) order. Returns undefined when nothing is known so
 * callers can omit the field entirely (absent === all Unknown).
 */
export function mergeSpeedUsage(
  ...breakdowns: Array<readonly SpeedUsage[] | null | undefined>
): SpeedUsage[] | undefined {
  const byPair = new Map<number, SpeedUsage>();

  for (const breakdown of breakdowns) {
    for (const entry of breakdown ?? []) {
      const rank = pairRank(entry);
      const existing = byPair.get(rank);
      if (existing) {
        existing.tokens += entry.tokens;
        existing.messages += entry.messages;
      } else {
        byPair.set(rank, { ...entry });
      }
    }
  }

  if (byPair.size === 0) return undefined;
  return Array.from(byPair.entries())
    .sort(([a], [b]) => a - b)
    .map(([, entry]) => entry);
}
