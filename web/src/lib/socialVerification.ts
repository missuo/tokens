import type { ProfileSocialLink } from "@/components/profile/types";

/** Number of social links (website included) that marks a profile verified. */
export const SOCIAL_VERIFIED_THRESHOLD = 2;

export function isVerifiedBySocialLinks(
  links: ProfileSocialLink[] | null | undefined,
): boolean {
  return (links?.length ?? 0) >= SOCIAL_VERIFIED_THRESHOLD;
}
