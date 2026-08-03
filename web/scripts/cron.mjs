// Replaces the Worker's cron trigger.
//
// wrangler.jsonc carries `"crons": ["20 3 * * *"]`, which fired the daily
// GitHub social-links refresh in-process. Nothing on the self-hosted box did:
// no systemd timer, no crontab entry. Left alone, every verified badge on the
// site would have quietly stopped updating the day production moved.
//
// Runs as its own container from the *same image* as the app, so it ships with
// every deploy and is versioned with the code. A systemd timer on the host
// would work too and would survive the stack being torn down — but it would
// live in /etc on one machine, reviewable by nobody, which is the state this
// project has already been bitten by.
//
// It calls the ordinary HTTP endpoint rather than importing the job. That
// endpoint already exists for manual runs, is already guarded by CRON_SECRET,
// and already answers 202 and continues in the background — so this process
// stays a scheduler and nothing more, and a failure here cannot take the
// refresh down with it.

const ENDPOINT =
  process.env.CRON_TARGET_URL ?? "http://app:3000/api/cron/refresh-social-links";
const SECRET = process.env.CRON_SECRET;

/** UTC hour and minute, matching the Worker trigger it replaces. */
const HOUR = Number(process.env.CRON_UTC_HOUR ?? 3);
const MINUTE = Number(process.env.CRON_UTC_MINUTE ?? 20);

if (!SECRET) {
  console.error("[cron] CRON_SECRET is not set; refusing to start");
  process.exit(1);
}

/** Milliseconds until the next occurrence, always strictly in the future. */
function msUntilNextRun() {
  const now = new Date();
  const next = new Date(now);
  next.setUTCHours(HOUR, MINUTE, 0, 0);
  if (next <= now) next.setUTCDate(next.getUTCDate() + 1);
  return next.getTime() - now.getTime();
}

async function run() {
  const startedAt = new Date().toISOString();
  try {
    const response = await fetch(ENDPOINT, {
      method: "POST",
      headers: { Authorization: `Bearer ${SECRET}` },
      // The endpoint answers as soon as it has counted the work and does the
      // rest in the background, so this only ever waits on the acknowledgement.
      signal: AbortSignal.timeout(30_000),
    });
    const body = await response.text();
    console.log(
      `[cron] ${startedAt} refresh-social-links -> ${response.status} ${body.slice(0, 200)}`,
    );
  } catch (error) {
    // Logged and dropped. A daily badge refresh that failed is a thing to fix
    // tomorrow; a scheduler that exited because of it is a thing nobody
    // notices until the badges are a month stale.
    console.error(`[cron] ${startedAt} refresh-social-links failed:`, error);
  }
}

function scheduleNext() {
  const delay = msUntilNextRun();
  console.log(
    `[cron] next refresh-social-links in ${Math.round(delay / 60_000)} min ` +
      `(${String(HOUR).padStart(2, "0")}:${String(MINUTE).padStart(2, "0")} UTC)`,
  );
  setTimeout(() => {
    void run().finally(scheduleNext);
  }, delay);
}

// Deliberately does not fire on start. Deploys are frequent and this job walks
// every user's GitHub profile; running it on each one would turn a daily job
// into a per-deploy one for no benefit. A restart that happens to land inside
// the scheduled minute skips that day, which for a badge refresh is cheaper
// than the alternative.
console.log(`[cron] scheduler up, target ${ENDPOINT}`);
scheduleNext();
