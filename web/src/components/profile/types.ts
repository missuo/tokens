/**
 * Providers we render icons for. GitHub's social_accounts API also returns
 * "generic" and other unrecognized providers — those are intentionally
 * dropped so the profile only shows known platforms.
 */
export type ProfileSocialProvider =
  | "website"
  | "twitter"
  | "linkedin"
  | "instagram"
  | "facebook"
  | "mastodon"
  | "bluesky"
  | "youtube"
  | "twitch"
  | "reddit"
  | "npm";

export interface ProfileSocialLink {
  provider: ProfileSocialProvider;
  url: string;
}

export interface ProfileUser {
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
  rank: number | null;
  createdAt?: string | null;
}

export interface ProfileStatsData {
  totalTokens: number;
  totalCost: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  reasoningTokens?: number;
  activeDays: number;
  submissionCount?: number;
  sessionCount?: number;
}

export interface ModelUsage {
  model: string;
  tokens: number;
  cost: number;
  percentage: number;
}
