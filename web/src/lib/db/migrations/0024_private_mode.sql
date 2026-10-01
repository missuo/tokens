-- Private mode: an account that is left out of every leaderboard and whose
-- profile, devices and embeds are only readable with the owner's session or
-- one of their read tokens.
ALTER TABLE "users" ADD COLUMN IF NOT EXISTS "is_private" boolean DEFAULT false NOT NULL;--> statement-breakpoint
CREATE TABLE IF NOT EXISTS "read_tokens" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" uuid NOT NULL,
	"token_hash" varchar(64) NOT NULL,
	"name" varchar(100) NOT NULL,
	"last_used_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "read_tokens_token_hash_unique" UNIQUE("token_hash"),
	CONSTRAINT "read_tokens_user_name_unique" UNIQUE("user_id","name")
);--> statement-breakpoint
DO $$ BEGIN
 ALTER TABLE "read_tokens" ADD CONSTRAINT "read_tokens_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
EXCEPTION
 WHEN duplicate_object THEN null;
END $$;--> statement-breakpoint
CREATE INDEX IF NOT EXISTS "idx_read_tokens_user_id" ON "read_tokens" USING btree ("user_id");
