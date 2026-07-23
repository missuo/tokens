import { timingSafeEqual } from "crypto";
import { NextResponse } from "next/server";
import { db, users } from "@/lib/db";
import { syncGitHubSocialLinks } from "@/lib/githubSocials";
import { isVerifiedBySocialLinks } from "@/lib/socialVerification";

export const dynamic = "force-dynamic";

const SYNC_CONCURRENCY = 4;

function safeEqual(a: string, b: string): boolean {
  const bufferA = Buffer.from(a);
  const bufferB = Buffer.from(b);
  return bufferA.length === bufferB.length && timingSafeEqual(bufferA, bufferB);
}

/**
 * Daily refresh of every user's GitHub social-links snapshot (drives the
 * verified badge). Triggered by the refresh-social-links GitHub Actions
 * schedule; guarded by CRON_SECRET. Responds immediately and syncs in the
 * background so reverse-proxy timeouts can't cut the run short.
 */
export async function POST(request: Request) {
  const secret = process.env.CRON_SECRET;
  if (!secret) {
    return NextResponse.json(
      { error: "CRON_SECRET is not configured" },
      { status: 503 },
    );
  }

  const authorization = request.headers.get("authorization") ?? "";
  if (!safeEqual(authorization, `Bearer ${secret}`)) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const rows = await db.select({ username: users.username }).from(users);

  void (async () => {
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
    console.log(
      `[cron] refresh-social-links: synced ${rows.length} users, ${verified} verified`,
    );
  })();

  return NextResponse.json(
    { accepted: true, users: rows.length },
    { status: 202 },
  );
}
