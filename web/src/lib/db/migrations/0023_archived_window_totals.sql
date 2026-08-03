-- Exact per-model aggregates for an imported window, with no day resolution.
--
-- `archived_breakdown` carries what the source file knows per day: input and
-- output. It deliberately refuses cache read/write, because a provider's
-- aggregate file records those only as lifetime totals and splitting them
-- across days would invent precision that does not exist.
--
-- That was right about the split and wrong about the total. For Claude Code,
-- cache read is not a rounding error — measured across this database it is
-- 97.5% of every token the scanner counts, because the model re-reads the
-- conversation on each turn. Dropping it left the archive reporting ~1.5% of
-- the magnitude of the scanned data beside it, which is not conservative, it is
-- wrong in the other direction and silently so.
--
-- The aggregate itself is exactly known: `modelUsage` holds lifetime per-model
-- cache totals, and subtracting the surviving transcripts leaves the
-- pre-install portion with no estimation involved. So it is stored — in a table
-- with no date column, so that a figure with no day resolution remains
-- structurally incapable of claiming one. Same reasoning that keeps
-- `timestamp_ms` off `archived_breakdown`, applied to a field that has a total
-- but no distribution.
--
-- Never summed into a daily row, and never queried by the leaderboard: like
-- `archived_breakdown`, this table is outside the ranking path by construction.
CREATE TABLE IF NOT EXISTS "archived_window_totals" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	-- Matches archived_breakdown.origin, so an import owns both its days and
	-- its window totals and replacing one replaces the other.
	"origin" varchar(64) NOT NULL,
	-- The window these totals cover, inclusive of start and exclusive of end.
	-- A label for the reader ("Mar–Jul, totals only"), not a distribution.
	"window_start" date NOT NULL,
	"window_end" date NOT NULL,
	-- client -> model -> { cacheRead, cacheWrite }. Restricted to the fields
	-- that genuinely have no per-day form; input and output already live in
	-- archived_breakdown with day resolution, and repeating them here would
	-- make double counting expressible.
	"totals" jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "archived_window_totals" ADD CONSTRAINT "archived_window_totals_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "archived_window_totals" ADD CONSTRAINT "archived_window_totals_user_origin_unique" UNIQUE("user_id","origin");
