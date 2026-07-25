/// <reference types="bun-types" />
import postgres from "postgres";
import { classifyFailure } from "./migrate-retry";

// This runs BEFORE `cf:build` in the Workers Builds build command
// (`bun run scripts/migrate-prod.ts && bun run cf:build`), intentionally — not
// after. If a build's new code depends on its own accompanying migration,
// building first would fail and the migration would never get a chance to run,
// permanently blocking that deploy. The residual risk of this order (migrate
// succeeds, build then fails for an unrelated reason, leaving new schema paired
// with old code) is mitigated by this repo's convention of additive-only
// migrations.
//
// MIGRATE_TARGET names the database this run is allowed to touch ("staging",
// or "production" at cutover). Without it the script is a no-op, so a build
// from an arbitrary branch or a local checkout cannot reach a real database —
// which matters because DATABASE_URL is a build-time variable and any build
// that can read it could otherwise apply an unreviewed migration.
const migrateTarget = process.env.MIGRATE_TARGET;
const isExplicitTarget =
  migrateTarget === "staging" || migrateTarget === "production";

if (!isExplicitTarget) {
  console.log(
    `skip - migrate-prod: MIGRATE_TARGET=${migrateTarget ?? "(unset)"}`
  );
  process.exit(0);
}

console.log(
  `migrate-prod: target=${migrateTarget}`
);

const databaseUrl = process.env.DATABASE_URL;
if (!databaseUrl) {
  throw new Error("DATABASE_URL is required");
}

// drizzle-kit/drizzle-orm take no advisory lock of their own, so concurrent
// builds (rapid pushes, a manual redeploy overlapping an in-flight one) can
// race two `drizzle-kit migrate` runs against each other. Hold a session
// lock for the lifetime of this process to serialize them.
const LOCK_KEY = "tokens_drizzle_migrate";
const MAX_LOCK_ATTEMPTS = 60;
const MAX_ATTEMPTS = 5;
const RETRY_DELAY_MS = 3000;

const sql = postgres(databaseUrl, { max: 1 });

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runMigrate(): Promise<{
  ok: boolean;
  retryable: boolean;
  reason: string;
  stderr: string;
}> {
  const proc = Bun.spawn(["bunx", "drizzle-kit", "migrate"], {
    stdout: "inherit",
    stderr: "pipe",
  });
  const stderr = await new Response(proc.stderr).text();
  process.stderr.write(stderr);
  const exitCode = await proc.exited;
  if (exitCode === 0) {
    return { ok: true, retryable: false, reason: "", stderr };
  }
  const { retryable, reason } = classifyFailure(stderr);
  return { ok: false, retryable, reason, stderr };
}

// Acquire (or re-acquire) the session-scoped advisory lock that serializes
// concurrent `drizzle-kit migrate` runs across overlapping builds. Loops on
// two conditions so it survives a wobbly database:
//   - the query itself fails (DB still unreachable, e.g. mid-outage): keep
//     retrying so a transient blip doesn't abort a deploy;
//   - the lock is held by another build: wait for that build to finish, then
//     re-check -- we never run a migration without holding the lock.
// Throws only after MAX_LOCK_ATTEMPTS. Safe to call again on the migrate-retry
// path: a transient connection drop can sever the session and silently release
// the lock (advisory locks die with their connection), so re-establishing it
// before each retry is what prevents two builds from migrating at once.
async function acquireAdvisoryLock(): Promise<void> {
  for (let attempt = 1; attempt <= MAX_LOCK_ATTEMPTS; attempt++) {
    let acquired = false;
    try {
      const [row] = await sql<{ acquired: boolean }[]>`
        SELECT pg_try_advisory_lock(hashtext(${LOCK_KEY})) AS acquired
      `;
      acquired = row?.acquired ?? false;
    } catch (error) {
      // Only wait out a TRANSIENT connectivity failure (same classifier as the
      // migrate retry). A permanent error -- bad credentials, unreachable host,
      // a missing function -- won't fix itself, so fail the deploy immediately
      // instead of burning ~MAX_LOCK_ATTEMPTS of pointless waits.
      const errorText = `${error} ${(error as { code?: unknown })?.code ?? ""}`;
      if (attempt === MAX_LOCK_ATTEMPTS || !classifyFailure(errorText).retryable) {
        throw error;
      }
      console.warn(
        `warn - could not reach DB to acquire migration advisory lock (attempt ${attempt}/${MAX_LOCK_ATTEMPTS}); retrying in ${RETRY_DELAY_MS}ms`
      );
      await sleep(RETRY_DELAY_MS);
      continue;
    }
    if (acquired) return;
    if (attempt < MAX_LOCK_ATTEMPTS) {
      console.warn(
        `warn - migration advisory lock held by another build (attempt ${attempt}/${MAX_LOCK_ATTEMPTS}); retrying in ${RETRY_DELAY_MS}ms`
      );
      await sleep(RETRY_DELAY_MS);
    }
  }
  throw new Error(
    `could not acquire migration advisory lock after ${MAX_LOCK_ATTEMPTS} attempts -- a concurrent build may be stuck`
  );
}

let lockAcquired = false;

try {
  await acquireAdvisoryLock();
  lockAcquired = true;
  console.log(`ok - acquired advisory lock (${LOCK_KEY})`);

  let lastResult: Awaited<ReturnType<typeof runMigrate>> | undefined;
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
    lastResult = await runMigrate();
    if (lastResult.ok) {
      console.log(`ok - drizzle-kit migrate succeeded (attempt ${attempt}/${MAX_ATTEMPTS})`);
      break;
    }
    if (!lastResult.retryable) {
      throw new Error(
        `drizzle-kit migrate failed (attempt ${attempt}/${MAX_ATTEMPTS}, ${lastResult.reason} — not retrying)`
      );
    }
    console.warn(
      `warn - drizzle-kit migrate hit a transient ${lastResult.reason} (attempt ${attempt}/${MAX_ATTEMPTS})`
    );
    if (attempt === MAX_ATTEMPTS) {
      throw new Error(
        `drizzle-kit migrate failed with a transient ${lastResult.reason} ${MAX_ATTEMPTS} times in a row`
      );
    }
    await sleep(RETRY_DELAY_MS);

    // The retryable failure may have severed the parent session holding the
    // advisory lock (see acquireAdvisoryLock). Re-establish it before the next
    // attempt so we never migrate without the lock -- waiting through both a
    // still-recovering DB and a concurrent holder rather than aborting.
    await acquireAdvisoryLock();
  }
} finally {
  if (lockAcquired) {
    try {
      // Release every level held -- the retry path's re-acquire is re-entrant,
      // so the session can hold the lock more than once. pg_advisory_unlock_all
      // clears them all regardless of depth.
      await sql`SELECT pg_advisory_unlock_all()`;
    } catch (error) {
      console.error("warn - failed to release migration advisory lock", error);
    }
  }

  try {
    await sql.end();
  } catch (error) {
    console.error("warn - failed to close migration database connection", error);
  }
}
