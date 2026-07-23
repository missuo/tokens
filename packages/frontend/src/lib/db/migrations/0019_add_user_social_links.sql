ALTER TABLE "users" ADD COLUMN IF NOT EXISTS "social_links" jsonb;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN IF NOT EXISTS "social_links_synced_at" timestamp with time zone;
