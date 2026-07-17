/// <reference types="bun-types" />
import postgres from "postgres";

// This runs BEFORE `next build` in vercel.json's buildCommand
// (`bun run scripts/migrate-prod.ts && next build`), intentionally — not after.
// `src/app/(main)/page.tsx` (HomePage) has no dynamic rendering signal, so it's
// statically prerendered at build time, and its render path calls
// `getLeaderboardData`, which queries the DB directly (through an
// `unstable_cache` wrapper — caching the fetch doesn't defer *when* it first
// runs). Reordering to build-then-migrate would make `next build` fail
// whenever a PR's new code depends on its own accompanying migration,
// permanently blocking that deploy since the migration never gets a chance to
// run. The residual risk of the current order (migrate succeeds, then build
// fails for an unrelated reason, leaving new schema paired with old code) is
// mitigated by this repo's convention of additive-only migrations.
//
// Vercel has no buildCommand-level distinction between "preview build for a
// WIP branch" and "production build" other than VERCEL_ENV — and DATABASE_URL
// is the SAME value across Production/Preview/Development in this project.
// Without this gate, pushing an unreviewed migration to any branch would
// apply it to prod the moment its preview build runs.
if (process.env.VERCEL_ENV !== "production") {
  console.log(
    `skip - migrate-prod: VERCEL_ENV=${process.env.VERCEL_ENV ?? "(unset)"}, not production`
  );
  process.exit(0);
}

const databaseUrl = process.env.DATABASE_URL;
if (!databaseUrl) {
  throw new Error("DATABASE_URL is required");
}

// drizzle-kit/drizzle-orm take no advisory lock of their own, so concurrent
// builds (rapid pushes, a manual redeploy overlapping an in-flight one) can
// race two `drizzle-kit migrate` runs against each other. Hold a session
// lock for the lifetime of this process to serialize them.
const LOCK_KEY = "tokscale_drizzle_migrate";
const MAX_LOCK_ATTEMPTS = 60;
const MAX_ATTEMPTS = 5;
const RETRY_DELAY_MS = 3000;

const sql = postgres(databaseUrl, { max: 1 });

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function runMigrate(): Promise<{ ok: boolean; deadlock: boolean; stderr: string }> {
  const proc = Bun.spawn(["bunx", "drizzle-kit", "migrate"], {
    stdout: "inherit",
    stderr: "pipe",
  });
  const stderr = await new Response(proc.stderr).text();
  process.stderr.write(stderr);
  const exitCode = await proc.exited;
  if (exitCode === 0) {
    return { ok: true, deadlock: false, stderr };
  }
  // Postgres deadlock_detected is SQLSTATE 40P01.
  const deadlock = /40P01|deadlock detected/i.test(stderr);
  return { ok: false, deadlock, stderr };
}

let lockAcquired = false;

try {
  for (let attempt = 1; attempt <= MAX_LOCK_ATTEMPTS; attempt++) {
    const [result] = await sql<{ acquired: boolean }[]>`
      SELECT pg_try_advisory_lock(hashtext(${LOCK_KEY})) AS acquired
    `;
    if (result?.acquired) {
      lockAcquired = true;
      break;
    }

    if (attempt < MAX_LOCK_ATTEMPTS) {
      console.warn(
        `warn - migration advisory lock unavailable (attempt ${attempt}/${MAX_LOCK_ATTEMPTS}); retrying in ${RETRY_DELAY_MS}ms`
      );
      await sleep(RETRY_DELAY_MS);
    }
  }

  if (!lockAcquired) {
    throw new Error(
      `could not acquire migration advisory lock after ${MAX_LOCK_ATTEMPTS} attempts -- a concurrent build may be stuck`
    );
  }

  console.log(`ok - acquired advisory lock (${LOCK_KEY})`);

  let lastResult: Awaited<ReturnType<typeof runMigrate>> | undefined;
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
    lastResult = await runMigrate();
    if (lastResult.ok) {
      console.log(`ok - drizzle-kit migrate succeeded (attempt ${attempt}/${MAX_ATTEMPTS})`);
      break;
    }
    if (!lastResult.deadlock) {
      throw new Error(
        `drizzle-kit migrate failed (attempt ${attempt}/${MAX_ATTEMPTS}, not a deadlock — not retrying)`
      );
    }
    console.warn(
      `warn - drizzle-kit migrate hit a deadlock (attempt ${attempt}/${MAX_ATTEMPTS})`
    );
    if (attempt === MAX_ATTEMPTS) {
      throw new Error(`drizzle-kit migrate deadlocked ${MAX_ATTEMPTS} times in a row`);
    }
    await sleep(RETRY_DELAY_MS);
  }
} finally {
  if (lockAcquired) {
    try {
      await sql`SELECT pg_advisory_unlock(hashtext(${LOCK_KEY}))`;
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
