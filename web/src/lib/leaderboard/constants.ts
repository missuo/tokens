export type LeaderboardSortBy = 'tokens' | 'cost';
export type LeaderboardTokenFormat = "full" | "compact";

export const SORT_BY_COOKIE_NAME = "leaderboard-sort-by";
export const VALID_SORT_BY: LeaderboardSortBy[] = ['tokens', 'cost'];
export const DEFAULT_LEADERBOARD_TOKEN_FORMAT: LeaderboardTokenFormat = "full";

export function isValidSortBy(value: unknown): value is LeaderboardSortBy {
  return typeof value === 'string' && VALID_SORT_BY.includes(value as LeaderboardSortBy);
}

export function resolveLeaderboardTokenFormat(value: unknown): LeaderboardTokenFormat {
  return value === "full" || value === "compact"
    ? value
    : DEFAULT_LEADERBOARD_TOKEN_FORMAT;
}

/**
 * Resolve a sort query/cookie value while preserving the distinction between
 * an absent value and an explicitly retired/unknown value. Explicit invalid
 * values must not fall through to a persisted preference: stale links should
 * consistently land on the tokens ranking.
 */
export function resolveSortByParam(value: unknown): LeaderboardSortBy | null {
  if (value === null || value === undefined) {
    return null;
  }

  return isValidSortBy(value) ? value : "tokens";
}
