import { timingSafeEqual } from "crypto";
import { NextResponse } from "next/server";
import { getCloudflareContext } from "@opennextjs/cloudflare";
import { countUsers, refreshAllSocialLinks } from "@/lib/cron/refreshSocialLinks";

export const dynamic = "force-dynamic";

function safeEqual(a: string, b: string): boolean {
  const bufferA = Buffer.from(a);
  const bufferB = Buffer.from(b);
  return bufferA.length === bufferB.length && timingSafeEqual(bufferA, bufferB);
}

/**
 * Keep the sync running after the 202 has been sent.
 *
 * On Workers an isolate stops executing once its response is returned, so the
 * promise has to be handed to `waitUntil` or the run is truncated partway
 * through the user list. Outside Workers there is no such constraint and the
 * floating promise settles on its own.
 */
function runInBackground(work: Promise<unknown>): void {
  // Nothing downstream awaits this promise, so it has to carry its own handler.
  // Under Workers an unhandled rejection ends the isolate and little else; on
  // the self-hosted server it is fatal to the *process* — Node treats unhandled
  // rejections as fatal by default, so one failing GitHub call during the daily
  // refresh would take the whole site down.
  const guarded = work.catch((error: unknown) => {
    console.error("[cron] refresh-social-links failed", error);
  });

  try {
    getCloudflareContext().ctx.waitUntil(guarded);
    return;
  } catch {
    // Not on Workers: no isolate to keep alive, and the process outlives the
    // response on its own.
  }

  void guarded;
}

/**
 * Daily refresh of every user's GitHub social-links snapshot (drives the
 * verified badge). Triggered by the Worker's cron trigger, or over HTTP for
 * manual runs; guarded by CRON_SECRET. Responds immediately and syncs in the
 * background so proxy timeouts can't cut the run short.
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

  const total = await countUsers();

  runInBackground(
    refreshAllSocialLinks().then(({ users, verified, skipped }) => {
      console.log(
        `[cron] refresh-social-links: ${users} users, ${verified} verified, ` +
          `${skipped} skipped (GitHub did not answer)`,
      );
    }),
  );

  return NextResponse.json({ accepted: true, users: total }, { status: 202 });
}
