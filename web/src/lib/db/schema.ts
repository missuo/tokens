import {
  pgTable,
  uuid,
  varchar,
  text,
  timestamp,
  bigint,
  decimal,
  date,
  jsonb,
  integer,
  index,
  unique,
  uniqueIndex,
} from "drizzle-orm/pg-core";
import {
  USERS_USERNAME_LOWER_UNIQUE_INDEX,
  usernameLowerExpression,
} from "./usernameIndex";

// ============================================================================
// USERS
// ============================================================================
export const users = pgTable(
  "users",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    githubId: integer("github_id").notNull().unique(),
    username: varchar("username", { length: 39 }).notNull().unique(),
    displayName: varchar("display_name", { length: 255 }),
    avatarUrl: text("avatar_url"),
    email: varchar("email", { length: 255 }),
    /**
     * Snapshot of the user's public GitHub social links (website + recognized
     * social accounts), refreshed on login and on profile views. An array of
     * {provider, url}; >= 2 entries marks the user as verified.
     */
    socialLinks: jsonb("social_links"),
    socialLinksSyncedAt: timestamp("social_links_synced_at", {
      withTimezone: true,
    }),
    /**
     * Non-null once the user is banned. Banned users cannot authenticate
     * (web session, OAuth login, or API token) and are excluded from every
     * leaderboard, but their submitted rows are retained as evidence and
     * listed on the Hall of Shame together with banReason.
     */
    bannedAt: timestamp("banned_at", { withTimezone: true }),
    banReason: text("ban_reason"),
    createdAt: timestamp("created_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [
    // Both indexes on username are intentional: the prod planner consistently
    // picks the explicit non-unique idx_users_username (30k scans) over the
    // unique-constraint sibling (0 scans). Removing this is a real re-plan
    // event; don't.
    index("idx_users_username").on(table.username),
    uniqueIndex(USERS_USERNAME_LOWER_UNIQUE_INDEX).on(
      usernameLowerExpression(table.username)
    ),
    index("idx_users_github_id").on(table.githubId),
  ]
);

// ============================================================================
// SESSIONS
// ============================================================================
export const sessions = pgTable(
  "sessions",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    tokenHash: varchar("token_hash", { length: 64 }).notNull().unique(),
    expiresAt: timestamp("expires_at", { withTimezone: true }).notNull(),
    source: varchar("source", { length: 10 }).notNull().default("web"),
    userAgent: text("user_agent"),
    createdAt: timestamp("created_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [
    index("idx_sessions_token_hash").on(table.tokenHash),
    index("idx_sessions_user_id").on(table.userId),
    index("idx_sessions_expires_at").on(table.expiresAt),
  ]
);

// ============================================================================
// API TOKENS
// ============================================================================
export const apiTokens = pgTable(
  "api_tokens",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    token: varchar("token", { length: 64 }).notNull().unique(),
    name: varchar("name", { length: 100 }).notNull(),
    lastUsedAt: timestamp("last_used_at", { withTimezone: true }),
    expiresAt: timestamp("expires_at", { withTimezone: true }),
    createdAt: timestamp("created_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [
    // Planner picks the explicit non-unique idx (~27k scans) over the
    // unique-constraint sibling (0 scans); keep both.
    index("idx_api_tokens_token").on(table.token),
    index("idx_api_tokens_user_id").on(table.userId),
    unique("api_tokens_user_name_unique").on(table.userId, table.name),
  ]
);

// ============================================================================
// DEVICE CODES
// ============================================================================
export const deviceCodes = pgTable(
  "device_codes",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    deviceCode: varchar("device_code", { length: 32 }).notNull().unique(),
    userCode: varchar("user_code", { length: 9 }).notNull().unique(),
    userId: uuid("user_id").references(() => users.id, { onDelete: "cascade" }),
    deviceName: varchar("device_name", { length: 100 }),
    expiresAt: timestamp("expires_at", { withTimezone: true }).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [
    // The .unique() siblings exist for device_code / user_code but the
    // planner picks the explicit non-unique indexes; keep them.
    index("idx_device_codes_device_code").on(table.deviceCode),
    index("idx_device_codes_user_code").on(table.userCode),
    // idx_device_codes_user_id covers the FK so cascade-delete of a user
    // doesn't seq scan this table.
    index("idx_device_codes_user_id").on(table.userId),
    index("idx_device_codes_expires_at").on(table.expiresAt),
  ]
);

// ============================================================================
// SUBMISSIONS
// ============================================================================
export const submissions = pgTable(
  "submissions",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),

    totalTokens: bigint("total_tokens", { mode: "number" }).notNull(),
    totalCost: decimal("total_cost", { precision: 18, scale: 4 }).notNull(),
    inputTokens: bigint("input_tokens", { mode: "number" }).notNull(),
    outputTokens: bigint("output_tokens", { mode: "number" }).notNull(),
    cacheCreationTokens: bigint("cache_creation_tokens", { mode: "number" })
      .notNull()
      .default(0),
    cacheReadTokens: bigint("cache_read_tokens", { mode: "number" })
      .notNull()
      .default(0),
    reasoningTokens: bigint("reasoning_tokens", { mode: "number" })
      .notNull()
      .default(0),

    dateStart: date("date_start").notNull(),
    dateEnd: date("date_end").notNull(),

    sourcesUsed: text("sources_used").array().notNull(),
    modelsUsed: text("models_used").array().notNull(),

    cliVersion: varchar("cli_version", { length: 20 }),
    submissionHash: varchar("submission_hash", { length: 64 }),
    submitCount: integer("submit_count").notNull().default(1),
    /** 0=legacy (no timestamps), 1=timestamp-aware CLI */
    schemaVersion: integer("schema_version").notNull().default(0),
    totalActiveTimeMs: bigint("total_active_time_ms", { mode: "number" }),
    longestContinuousMs: bigint("longest_continuous_ms", { mode: "number" }),
    maxConcurrentSessions: integer("max_concurrent_sessions"),
    sessionCount: integer("session_count"),

    mcpServers: jsonb("mcp_servers").$type<string[]>(),

    createdAt: timestamp("created_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  (table) => [
    index("idx_submissions_created_at").on(table.createdAt),
    // idx_submissions_leaderboard serves every user_id lookup as a left-prefix
    // index, so a plain idx_submissions_user_id would be redundant. Do not
    // re-add it without first checking pg_stat_user_indexes on the composite.
    index("idx_submissions_leaderboard").on(table.userId, table.totalTokens, table.totalCost, table.createdAt),
    unique("submissions_user_id_unique").on(table.userId),
  ]
);

// ============================================================================
// SUBMITTED DEVICES
// ============================================================================
export const submittedDevices = pgTable(
  "submitted_devices",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    deviceKey: varchar("device_key", { length: 96 }).notNull(),
    displayName: varchar("display_name", { length: 120 }),
    createdAt: timestamp("created_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true })
      .notNull()
      .defaultNow(),
    lastSubmittedAt: timestamp("last_submitted_at", { withTimezone: true }),
  },
  (table) => [
    index("idx_submitted_devices_user_id").on(table.userId),
    unique("submitted_devices_user_device_key_unique").on(table.userId, table.deviceKey),
  ]
);

// ============================================================================
// DAILY BREAKDOWN
// ============================================================================
export const dailyBreakdown = pgTable(
  "daily_breakdown",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    submissionId: uuid("submission_id")
      .notNull()
      .references(() => submissions.id, { onDelete: "cascade" }),
    submittedDeviceId: uuid("submitted_device_id")
      .notNull()
      .references(() => submittedDevices.id, { onDelete: "cascade" }),

    date: date("date").notNull(),
    tokens: bigint("tokens", { mode: "number" }).notNull(),
    cost: decimal("cost", { precision: 14, scale: 4 }).notNull(),
    inputTokens: bigint("input_tokens", { mode: "number" }).notNull(),
    outputTokens: bigint("output_tokens", { mode: "number" }).notNull(),
    /** Unix ms timestamp of earliest message in this UTC day bucket. NULL for legacy data. */
    timestampMs: bigint("timestamp_ms", { mode: "number" }),

    sourceBreakdown: jsonb("source_breakdown").$type<
      Record<
        string,
        {
          tokens: number;
          cost: number;
          input: number;
          output: number;
          cacheRead: number;
          cacheWrite: number;
          reasoning: number;
          messages: number;
          models: Record<string, {
            tokens: number;
            cost: number;
            input: number;
            output: number;
            cacheRead: number;
            cacheWrite: number;
            reasoning: number;
            messages: number;
          }>;
          provenance?: {
            schemaVersion: number;
            messageCount: number;
            modelCount: number;
            /**
             * "backfill" when this client's contribution came from a
             * backfill-origin submission (`tokens import`); absent/"cli"
             * for locally-scanned usage.
             */
            origin?: "cli" | "backfill";
          };
          modelId?: string;
        }
      >
    >(),
    /** Total active coding time in this UTC day bucket (milliseconds). NULL for legacy data. */
    activeTimeMs: bigint("active_time_ms", { mode: "number" }),
  },
  (table) => [
    index("idx_daily_breakdown_submission_id").on(table.submissionId),
    index("idx_daily_breakdown_submitted_device_id").on(table.submittedDeviceId),
    index("idx_daily_breakdown_date").on(table.date),
    unique("daily_breakdown_submission_device_date_unique").on(
      table.submissionId,
      table.submittedDeviceId,
      table.date
    ),
  ]
);

/**
 * Usage that predates the CLI install, reconstructed from a provider's own
 * aggregate file rather than scanned from session transcripts.
 *
 * Claude Code deletes transcripts after `cleanupPeriodDays` (30 by default), so
 * installing the CLI with existing history silently loses everything older than
 * that window. The totals survive in `~/.claude/stats-cache.json`, but they are
 * aggregates — there is no per-message record to check them against, which is
 * why `tokens import` has always refused to upload them.
 *
 * This lives outside `dailyBreakdown` deliberately. **The leaderboard never
 * queries this table**, so reconstructed usage cannot reach a ranking by
 * construction rather than by every future query remembering a filter. The
 * previous attempt at this was `submissions.has_backfill`, which was designed,
 * written, and never read by anything.
 *
 * Keyed by `(userId, date, origin)` rather than by submission: an import is not
 * part of a submission cycle, and keeping `origin` in the key stops a second
 * import source from silently merging into the first. Re-importing the same
 * source replaces it.
 *
 * There is no `timestampMs` here on purpose. These rows have no intra-day
 * resolution — it died with the transcripts — and inventing one to satisfy a
 * finer schema would be fabricating data.
 */
export const archivedBreakdown = pgTable(
  "archived_breakdown",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),

    date: date("date").notNull(),
    /** What the numbers were reconstructed from, e.g. `claude-stats-cache`. */
    origin: varchar("origin", { length: 64 }).notNull(),

    tokens: bigint("tokens", { mode: "number" }).notNull(),
    cost: decimal("cost", { precision: 14, scale: 4 }).notNull(),
    inputTokens: bigint("input_tokens", { mode: "number" }).notNull(),
    outputTokens: bigint("output_tokens", { mode: "number" }).notNull(),

    /** Same per-client shape as `dailyBreakdown.sourceBreakdown`, so one
     *  component can render both. Cache and reasoning splits are whatever the
     *  source file could supply; absent fields stay 0 rather than being
     *  guessed. */
    sourceBreakdown: jsonb("source_breakdown").$type<
      Record<
        string,
        {
          tokens: number;
          cost: number;
          input: number;
          output: number;
          cacheRead: number;
          cacheWrite: number;
          reasoning: number;
          messages: number;
          models: Record<
            string,
            {
              tokens: number;
              cost: number;
              input: number;
              output: number;
              cacheRead: number;
              cacheWrite: number;
              reasoning: number;
              messages: number;
            }
          >;
        }
      >
    >(),

    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    index("idx_archived_breakdown_user_date").on(table.userId, table.date),
    unique("archived_breakdown_user_date_origin_unique").on(
      table.userId,
      table.date,
      table.origin
    ),
  ]
);

/**
 * Exact per-model aggregates for an imported window, with no day resolution.
 *
 * `archivedBreakdown` carries what a provider's aggregate file knows per day —
 * input and output — and refuses cache read/write, because those exist only as
 * lifetime totals and splitting them across days would invent precision.
 *
 * That was right about the split and wrong about the total. Cache read is 97.5%
 * of every token this database counts; dropping it left the archive showing
 * roughly 1.5% of the magnitude of the scanned figures next to it. The
 * aggregate is exactly known — lifetime per-model totals minus the surviving
 * transcripts — so it is kept, in a table with **no date column**, which is
 * what stops a figure with no day resolution from ever claiming one.
 *
 * Never summed into a daily row, and never read by the leaderboard.
 */
export const archivedWindowTotals = pgTable(
  "archived_window_totals",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    /** Matches `archivedBreakdown.origin`, so one import owns both. */
    origin: varchar("origin", { length: 64 }).notNull(),

    /** Inclusive start, exclusive end. A label for the reader, not a
     *  distribution. */
    windowStart: date("window_start").notNull(),
    windowEnd: date("window_end").notNull(),

    /** client → model → the fields that have no per-day form. Input and output
     *  are excluded on purpose: they already exist in `archivedBreakdown` with
     *  day resolution, and repeating them here would make double counting
     *  expressible. */
    totals: jsonb("totals")
      .$type<Record<string, Record<string, { cacheRead: number; cacheWrite: number }>>>()
      .notNull(),

    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    unique("archived_window_totals_user_origin_unique").on(table.userId, table.origin),
  ]
);

// ============================================================================
// TYPE EXPORTS
// ============================================================================
export type User = typeof users.$inferSelect;
export type NewUser = typeof users.$inferInsert;
export type Session = typeof sessions.$inferSelect;
export type NewSession = typeof sessions.$inferInsert;
export type ApiToken = typeof apiTokens.$inferSelect;
export type NewApiToken = typeof apiTokens.$inferInsert;
export type DeviceCode = typeof deviceCodes.$inferSelect;
export type NewDeviceCode = typeof deviceCodes.$inferInsert;
export type Submission = typeof submissions.$inferSelect;
export type NewSubmission = typeof submissions.$inferInsert;
export type SubmittedDevice = typeof submittedDevices.$inferSelect;
export type NewSubmittedDevice = typeof submittedDevices.$inferInsert;
export type DailyBreakdown = typeof dailyBreakdown.$inferSelect;
export type NewDailyBreakdown = typeof dailyBreakdown.$inferInsert;
