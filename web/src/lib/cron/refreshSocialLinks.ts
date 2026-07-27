import { db, users } from "@/lib/db";
import { syncGitHubSocialLinks } from "@/lib/githubSocials";
import { isVerifiedBySocialLinks } from "@/lib/socialVerification";

const SYNC_CONCURRENCY = 4;

interface RefreshSocialLinksResult {
  users: number;
  verified: number;
}

/**
 * Re-sync every user's GitHub social-links snapshot, which drives the verified
 * badge. Runs in batches so a few hundred users don't open a few hundred
 * simultaneous connections to GitHub.
 *
 * Callers are responsible for keeping the runtime alive for the duration:
 * a Workers isolate stops executing once its response is returned unless the
 * promise is handed to `waitUntil`, so fire-and-forget would be truncated.
 */
export async function refreshAllSocialLinks(): Promise<RefreshSocialLinksResult> {
  const rows = await db.select({ username: users.username }).from(users);

  let verified = 0;
  for (let i = 0; i < rows.length; i += SYNC_CONCURRENCY) {
    const batch = rows.slice(i, i + SYNC_CONCURRENCY);
    const results = await Promise.all(
      batch.map((row) => syncGitHubSocialLinks(row.username)),
    );
    for (const links of results) {
      if (isVerifiedBySocialLinks(links)) verified++;
    }
  }

  return { users: rows.length, verified };
}

/**
 * Count of users to report before the sync starts, so the HTTP endpoint can
 * answer 202 with a meaningful body without waiting for the whole run.
 */
export async function countUsers(): Promise<number> {
  const rows = await db.select({ username: users.username }).from(users);
  return rows.length;
}
