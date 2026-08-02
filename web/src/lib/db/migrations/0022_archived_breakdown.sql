-- Reconstructed pre-install usage, kept out of daily_breakdown on purpose.
--
-- Claude Code deletes transcripts after cleanupPeriodDays (30 by default), so
-- anyone who installs the CLI with existing history permanently loses whatever
-- predates that window. The totals survive in ~/.claude/stats-cache.json, but
-- they are aggregates: there is no per-message record to check them against,
-- which is exactly why `tokens import` has always refused to upload them.
--
-- A separate table rather than a flag on daily_breakdown, for three reasons:
--
--   1. The leaderboard never queries this table, so reconstructed usage cannot
--      reach a ranking. That is a structural guarantee rather than a WHERE
--      clause every future query has to remember. This repository already has
--      one flag that was designed and never read (submissions.has_backfill,
--      dropped below) — the same mistake twice is a pattern, not bad luck.
--   2. The two kinds of data have different lifecycles. Scanned days are
--      re-submitted continuously and merged with monotonic rules; reconstructed
--      days are imported once and never change. One table would mean one set of
--      merge rules serving both.
--   3. A single calendar day can hold both. Codex sessions from March may still
--      be on disk while March's Claude transcripts are long gone, so that day is
--      partly scannable and partly reconstructed. Splitting by row keeps each
--      side whole.
--
-- Keyed by (user_id, date, origin) rather than by submission: an import is not
-- tied to a submission cycle, and keeping origin in the key means importing
-- from a second source cannot silently merge into the first — re-importing the
-- same source replaces it, which is the intended idempotency.
CREATE TABLE IF NOT EXISTS "archived_breakdown" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"date" date NOT NULL,
	-- Where the numbers were reconstructed from, e.g. 'claude-stats-cache'.
	-- Part of the unique key, and shown to the reader so "reconstructed" is
	-- never an unexplained label.
	"origin" varchar(64) NOT NULL,
	"tokens" bigint NOT NULL,
	"cost" numeric(14, 4) NOT NULL,
	"input_tokens" bigint NOT NULL,
	"output_tokens" bigint NOT NULL,
	-- Same per-client shape as daily_breakdown.source_breakdown so the profile
	-- can render both with one component. Deliberately no timestamp_ms: these
	-- rows have no intra-day resolution and must not pretend otherwise.
	"source_breakdown" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "archived_breakdown" ADD CONSTRAINT "archived_breakdown_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "archived_breakdown" ADD CONSTRAINT "archived_breakdown_user_date_origin_unique" UNIQUE("user_id","date","origin");--> statement-breakpoint
CREATE INDEX IF NOT EXISTS "idx_archived_breakdown_user_date" ON "archived_breakdown" ("user_id","date");--> statement-breakpoint

-- submissions.has_backfill goes with it. It was added to mark backfilled
-- submissions distinctly, but nothing ever read it: the leaderboard does not
-- reference it, and production carries 0 rows with it set out of 23,789 daily
-- rows. It also sits at the wrong granularity — one submissions row per user
-- makes it a sticky per-account flag, unable to express "rank this user's
-- scanned days, exclude their reconstructed ones", which is the whole point.
ALTER TABLE "submissions" DROP COLUMN IF EXISTS "has_backfill";
