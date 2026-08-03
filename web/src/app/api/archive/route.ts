import { NextResponse } from "next/server";
import { and, eq, notInArray, sql } from "drizzle-orm";
import { z } from "zod";
import { db, archivedBreakdown, archivedWindowTotals } from "@/lib/db";
import { getSessionFromRequest } from "@/lib/auth/requestSession";
import { revalidateUsernamePaths, normalizeUsernameCacheKey } from "@/lib/db/usernameLookup";

/**
 * Ingest usage that predates the CLI install, reconstructed from a provider's
 * own aggregate file.
 *
 * Separate from `/api/submit` on purpose. Submitted usage is scanned from
 * session transcripts and can be checked message by message; this cannot — the
 * transcripts it describes were deleted before the CLI ever ran. Keeping the
 * two endpoints and the two tables apart means reconstructed totals can never
 * reach a ranking, because the leaderboard queries neither this route nor
 * `archived_breakdown`. That is a property of the shape rather than of anyone
 * remembering a filter.
 *
 * The consequence, stated plainly for anyone considering sending fabricated
 * numbers here: it does not move you up the board. Nothing here is ranked.
 */

const DateSchema = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/, "date must be YYYY-MM-DD")
  .refine((value) => {
    const [y, m, d] = value.split("-").map(Number);
    const parsed = new Date(Date.UTC(y, m - 1, d));
    return (
      parsed.getUTCFullYear() === y &&
      parsed.getUTCMonth() === m - 1 &&
      parsed.getUTCDate() === d
    );
  }, "date must be a real calendar date");

const NonNegativeInt = z.number().finite().int().min(0);
const NonNegativeNum = z.number().finite().min(0);

const ModelSchema = z.object({
  tokens: NonNegativeInt,
  cost: NonNegativeNum,
  input: NonNegativeInt,
  output: NonNegativeInt,
  cacheRead: NonNegativeInt.default(0),
  cacheWrite: NonNegativeInt.default(0),
  reasoning: NonNegativeInt.default(0),
  messages: NonNegativeInt.default(0),
});

const ClientSchema = ModelSchema.extend({
  models: z.record(z.string().min(1).max(200), ModelSchema).default({}),
});

/**
 * Bounds exist here even though nothing is ranked. They are not an anti-cheat
 * measure — they stop a malformed import from becoming a 500 at the driver, or
 * from writing a row no column can hold.
 */
const MAX_DAYS = 4000; // ~11 years, well past any provider's retention
const MAX_CLIENTS = 64;

const DaySchema = z.object({
  date: DateSchema,
  tokens: NonNegativeInt,
  cost: NonNegativeNum,
  input: NonNegativeInt,
  output: NonNegativeInt,
  clients: z.record(z.string().min(1).max(64), ClientSchema).refine(
    (clients) => Object.keys(clients).length <= MAX_CLIENTS,
    `at most ${MAX_CLIENTS} clients per day`,
  ),
});

const BodySchema = z.object({
  /**
   * What the numbers were reconstructed from. Part of the row key, so importing
   * a second source cannot silently merge into the first, and re-importing the
   * same source replaces it rather than adding to it.
   */
  origin: z
    .string()
    .min(1)
    .max(64)
    .regex(/^[a-z0-9][a-z0-9-]*$/, "origin must be lowercase kebab-case"),
  /**
   * The cutoff the importer derived: the earliest date still covered by
   * surviving transcripts. Every submitted day must fall strictly before it, so
   * reconstructed and scanned usage cannot describe the same day. Rejecting
   * here rather than trusting the client keeps the two sources disjoint even if
   * the importer computes its cutoff wrongly.
   */
  scannedFrom: DateSchema,
  days: z.array(DaySchema).min(1).max(MAX_DAYS),
  /**
   * Exact per-model aggregates for the same window, for the fields that exist
   * only as lifetime totals in the source file.
   *
   * Cache read is ~97.5% of every token this service counts, so omitting it
   * would leave the archive reporting a fraction of the magnitude of the
   * scanned data beside it. The total is exactly known even though its
   * distribution across days is not, so it is accepted here and stored apart
   * from `days` — never distributed into them, never summed into a daily row.
   *
   * Restricted to cacheRead/cacheWrite deliberately: input and output already
   * arrive per day, and accepting them again here would make double counting
   * expressible.
   */
  windowTotals: z
    .record(
      z.string().min(1).max(64),
      z.record(
        z.string().min(1).max(200),
        z.object({ cacheRead: NonNegativeInt, cacheWrite: NonNegativeInt }),
      ),
    )
    .optional(),
});

export async function POST(request: Request) {
  const session = await getSessionFromRequest(request);
  if (!session) {
    return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
  }

  let rawBody: unknown;
  try {
    rawBody = await request.json();
  } catch {
    return NextResponse.json({ error: "Invalid JSON body" }, { status: 400 });
  }

  const parsed = BodySchema.safeParse(rawBody);
  if (!parsed.success) {
    return NextResponse.json(
      {
        error: "Invalid archive payload",
        details: parsed.error.issues.map(
          (issue) => `${issue.path.join(".")}: ${issue.message}`,
        ),
      },
      { status: 400 },
    );
  }

  const { origin, scannedFrom, days, windowTotals } = parsed.data;

  const overlapping = days.filter((day) => day.date >= scannedFrom);
  if (overlapping.length > 0) {
    return NextResponse.json(
      {
        error: "Archived days must precede the scanned range",
        detail:
          `${overlapping.length} day(s) fall on or after ${scannedFrom}. ` +
          "Those days are covered by session transcripts and belong in /api/submit.",
        examples: overlapping.slice(0, 5).map((day) => day.date),
      },
      { status: 400 },
    );
  }

  const seen = new Set<string>();
  for (const day of days) {
    if (seen.has(day.date)) {
      return NextResponse.json(
        { error: "Duplicate date in payload", detail: day.date },
        { status: 400 },
      );
    }
    seen.add(day.date);
  }

  const rows = days.map((day) => ({
    userId: session.id,
    date: day.date,
    origin,
    tokens: day.tokens,
    cost: day.cost.toFixed(4),
    inputTokens: day.input,
    outputTokens: day.output,
    sourceBreakdown: day.clients,
  }));

  const earliest = days.reduce((a, b) => (a.date < b.date ? a : b)).date;
  const latest = days.reduce((a, b) => (a.date > b.date ? a : b)).date;

  // Replace rather than accumulate. An import describes a fixed historical
  // window, so running it twice must leave the same rows behind — adding to
  // them would double the history, which is the failure this whole separation
  // exists to avoid.
  await db.transaction(async (tx) => {
    for (let i = 0; i < rows.length; i += 500) {
      await tx
        .insert(archivedBreakdown)
        .values(rows.slice(i, i + 500))
        .onConflictDoUpdate({
          target: [
            archivedBreakdown.userId,
            archivedBreakdown.date,
            archivedBreakdown.origin,
          ],
          set: {
            tokens: sql`EXCLUDED.tokens`,
            cost: sql`EXCLUDED.cost`,
            inputTokens: sql`EXCLUDED.input_tokens`,
            outputTokens: sql`EXCLUDED.output_tokens`,
            sourceBreakdown: sql`EXCLUDED.source_breakdown`,
            updatedAt: new Date(),
          },
        });
    }

    // Window totals live in their own table because they have no day
    // resolution — a table with no date column cannot be made to claim one.
    // Replaced wholesale for the same reason the days are: an import describes
    // a fixed window, so running it twice must not accumulate.
    if (windowTotals) {
      await tx
        .insert(archivedWindowTotals)
        .values({
          userId: session.id,
          origin,
          windowStart: earliest,
          windowEnd: scannedFrom,
          totals: windowTotals,
        })
        .onConflictDoUpdate({
          target: [archivedWindowTotals.userId, archivedWindowTotals.origin],
          set: {
            windowStart: sql`EXCLUDED.window_start`,
            windowEnd: sql`EXCLUDED.window_end`,
            totals: sql`EXCLUDED.totals`,
            updatedAt: new Date(),
          },
        });
    } else {
      // An import that no longer sends totals should not leave the previous
      // ones attached to it.
      await tx
        .delete(archivedWindowTotals)
        .where(
          and(
            eq(archivedWindowTotals.userId, session.id),
            eq(archivedWindowTotals.origin, origin),
          ),
        );
    }

    // Days this origin covered on a previous import but no longer claims. The
    // source file shrank, or the cutoff moved because more transcripts aged
    // out — either way the stale rows would otherwise linger forever.
    await tx.delete(archivedBreakdown).where(
      and(
        eq(archivedBreakdown.userId, session.id),
        eq(archivedBreakdown.origin, origin),
        notInArray(
          archivedBreakdown.date,
          days.map((day) => day.date),
        ),
      ),
    );
  });

  revalidateUsernamePaths(normalizeUsernameCacheKey(session.username));

  return NextResponse.json({
    success: true,
    origin,
    days: rows.length,
    earliest,
    latest,
    windowTotals: windowTotals ? Object.keys(windowTotals).length : 0,
    ranked: false,
    note: "Archived usage is shown on the profile and never counted toward leaderboard rank.",
  });
}
