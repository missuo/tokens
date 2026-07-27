import { NextResponse } from "next/server";
import { revalidateTag } from "next/cache";
import { db, apiTokens, submissions, submittedDevices, dailyBreakdown } from "@/lib/db";
import { and, eq, inArray, sql } from "drizzle-orm";
import {
  validateSubmission,
  generateSubmissionHash,
  type SubmissionData,
} from "@/lib/validation/submission";
import { authenticatePersonalToken } from "@/lib/auth/personalTokens";
import { getBearerToken } from "../../../lib/auth/bearerToken";
import {
  mergeClientBreakdownsWithRegressionGuard,
  recalculateDayTotals,
  clientContributionToBreakdownData,
  deriveClientBreakdownProvenance,
  aggregateIncomingClientBreakdowns,
  deriveStoredClientRevisionFloors,
  filterClientBreakdownsByRevisionFloor,
  applyAuthoritativeClientCoverage,
  mergeTimestampMs,
  type ClientBreakdownData,
} from "@/lib/db/helpers";
import { normalizeUsernameCacheKey, revalidateUsernamePaths } from "@/lib/db/usernameLookup";
import { LEGACY_DEVICE_KEY } from "@/lib/devices/shared";

const LEGACY_SUBMIT_DEVICE_KEY = LEGACY_DEVICE_KEY;
const LEGACY_SUBMIT_DEVICE_NAME = "Legacy submissions";
// PostgreSQL caps a single statement at 65,535 bound parameters. Each
// inserted row binds 10 params, so chunk large backfills (e.g. ~6,500+ days)
// across multiple INSERT statements to stay well under that limit.
const INSERT_CHUNK_SIZE = 1000;
// Below this many tokens, an exact cross-device (tokens, input) match could
// plausibly be a coincidence, so the reclaim logic leaves it alone.
const CROSS_DEVICE_DUP_MIN_TOKENS = 10_000;

// "kilocode" is a legacy alias of "kilo" (mirrors LEGACY_CLIENT_ALIASES in
// lib/validation/submission.ts and lib/publicProfileData.ts). Incoming
// contributions are already normalized to "kilo" by validateSubmission's
// preprocessing, but a daily_breakdown row's stored source_breakdown JSON
// can still carry a "kilocode" key written before that normalization
// existed. Fold it into "kilo" before merging so the day-level merge treats
// the two as the SAME client instead of summing them as disjoint clients
// (which would double-count the same underlying usage).
const LEGACY_CLIENT_ALIASES: Record<string, string> = { kilocode: "kilo" };

function mergeModelBreakdowns(
  target: Record<string, ClientBreakdownData["models"][string]>,
  incoming: Record<string, ClientBreakdownData["models"][string]>
): void {
  for (const [modelId, modelData] of Object.entries(incoming)) {
    const existingModel = target[modelId];
    if (existingModel) {
      existingModel.tokens += modelData.tokens || 0;
      existingModel.cost += modelData.cost || 0;
      existingModel.input += modelData.input || 0;
      existingModel.output += modelData.output || 0;
      existingModel.cacheRead += modelData.cacheRead || 0;
      existingModel.cacheWrite += modelData.cacheWrite || 0;
      existingModel.reasoning = (existingModel.reasoning || 0) + (modelData.reasoning || 0);
      existingModel.messages += modelData.messages || 0;
    } else {
      target[modelId] = { ...modelData };
    }
  }
}

function normalizeClientBreakdownAliases(
  breakdown: Record<string, ClientBreakdownData>
): Record<string, ClientBreakdownData> {
  const normalized: Record<string, ClientBreakdownData> = {};

  for (const [rawClientName, data] of Object.entries(breakdown)) {
    const clientName = LEGACY_CLIENT_ALIASES[rawClientName] ?? rawClientName;
    const existing = normalized[clientName];

    if (!existing) {
      normalized[clientName] = { ...data, models: { ...data.models } };
      continue;
    }

    existing.tokens += data.tokens || 0;
    existing.cost += data.cost || 0;
    existing.input += data.input || 0;
    existing.output += data.output || 0;
    existing.cacheRead += data.cacheRead || 0;
    existing.cacheWrite += data.cacheWrite || 0;
    existing.reasoning = (existing.reasoning || 0) + (data.reasoning || 0);
    existing.messages += data.messages || 0;
    mergeModelBreakdowns(existing.models, data.models || {});
    existing.provenance = deriveClientBreakdownProvenance(existing);
  }

  return normalized;
}

function normalizeSubmissionData(data: unknown): void {
  if (!data || typeof data !== "object") return;
  const obj = data as Record<string, unknown>;
  if (!Array.isArray(obj.contributions)) return;

  for (const contribution of obj.contributions) {
    if (!contribution || typeof contribution !== "object") continue;
    const day = contribution as Record<string, unknown>;
    // Handle both legacy "sources" and new "clients" formats
    const items = Array.isArray(day.sources)
      ? day.sources
      : Array.isArray(day.clients)
      ? day.clients
      : null;
    if (!items) continue;
    for (const entry of items) {
      if (!entry || typeof entry !== "object") continue;
      const s = entry as Record<string, unknown>;
      if (s.modelId == null || typeof s.modelId !== "string") {
        s.modelId = "unknown";
      } else {
        const trimmed = s.modelId.trim();
        s.modelId = trimmed === "" ? "unknown" : trimmed;
      }
    }
  }
}

// Submission schema versions:
//   0 = legacy CLI: no per-day timestamps, no device metadata.
//   1 = timestamp-aware CLI (>=v2.1): per-day `timestampMs` set, still no device.
//   2 = device-aware CLI (>=v2.1.x post-#517): caller sends a `device` object,
//       so daily_breakdown rows are keyed by submittedDeviceId.
// The submissions row keeps the GREATEST() of stored vs. incoming so a single
// device-aware submit cannot regress an account back to v1 hash semantics.
function getSubmitDevice(data: SubmissionData): { key: string; name: string | null; schemaVersion: number } {
  if (data.device) {
    return {
      key: data.device.id,
      name: data.device.name ?? null,
      schemaVersion: 2,
    };
  }

  return {
    key: LEGACY_SUBMIT_DEVICE_KEY,
    name: LEGACY_SUBMIT_DEVICE_NAME,
    schemaVersion: data.contributions.some((c) => c.timestampMs != null) ? 1 : 0,
  };
}

/**
 * Session-shape metrics only ever grow.
 *
 * A resubmit from the same device can legitimately report less than we already
 * hold — the usual cause is the user clearing old local session files — and
 * taking the incoming value verbatim would silently shrink their history.
 */
function mergeMonotonic(
  existing: number | null | undefined,
  incoming: number | null | undefined
): number | null {
  if (existing == null) return incoming ?? null;
  if (incoming == null) return existing;
  return Math.max(existing, incoming);
}

function isUniqueConstraintViolation(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;
  const maybeError = error as { code?: unknown; cause?: unknown };
  if (maybeError.code === "23505") return true;
  const cause = maybeError.cause;
  return Boolean(cause && typeof cause === "object" && (cause as { code?: unknown }).code === "23505");
}

/**
 * POST /api/submit
 * Submit token usage data from CLI
 * 
 * IMPLEMENTS CLIENT-LEVEL MERGE:
 * - Only updates clients present in submission
 * - Preserves data for clients NOT in submission
 * - Recalculates totals from dailyBreakdown
 *
 * Headers:
 *   Authorization: Bearer <api_token>
 *
 * Body: TokenContributionData JSON
 */
export async function POST(request: Request) {
  try {
    // ========================================
    // STEP 1: Authentication
    // ========================================
    const token = getBearerToken(request.headers.get("Authorization"));
    if (!token) {
      return NextResponse.json(
        { error: "Missing or invalid Authorization header" },
        { status: 401 }
      );
    }

    const authResult = await authenticatePersonalToken(token, {
      touchLastUsedAt: false,
    });

    if (authResult.status === "invalid") {
      return NextResponse.json({ error: "Invalid API token" }, { status: 401 });
    }

    if (authResult.status === "expired") {
      return NextResponse.json({ error: "API token has expired" }, { status: 401 });
    }

    if (authResult.status === "banned") {
      return NextResponse.json(
        { error: "This account has been banned for violating the platform rules" },
        { status: 403 }
      );
    }

    const tokenRecord = authResult;

    // ========================================
    // STEP 2: Parse and Validate
    // ========================================
    let rawData: unknown;
    try {
      rawData = await request.json();
    } catch {
      return NextResponse.json({ error: "Invalid JSON body" }, { status: 400 });
    }

    normalizeSubmissionData(rawData);

    const mcpServers: string[] | null =
      rawData != null && typeof rawData === "object" &&
      Array.isArray((rawData as Record<string, unknown>).mcpServers)
        ? ((rawData as Record<string, unknown>).mcpServers as unknown[]).filter(
            (s): s is string => typeof s === "string" && s.length > 0
          )
        : null;

    const validation = validateSubmission(rawData);

    if (!validation.valid || !validation.data) {
      return NextResponse.json(
        { error: "Validation failed", details: validation.errors },
        { status: 400 }
      );
    }

    const data = validation.data;
    const warnings = [...validation.warnings];

    // Phase 1 backfill-provenance persistence (issue #888): a submission
    // tagged `provenance.origin === "backfill"` (from `tokens import`)
    // sets the sticky submissions.has_backfill flag and stamps a per-client
    // origin tag into daily_breakdown.source_breakdown. The tag is excluded
    // from generateSubmissionHash, so it never affects idempotency.
    const isBackfill = data.provenance?.origin === "backfill";

    if (data.contributions.length === 0) {
      return NextResponse.json(
        { error: "No contribution data to submit" },
        { status: 400 }
      );
    }

    const submittedClients = new Set<SubmissionData["summary"]["clients"][number]>(data.summary.clients);
    for (const contribution of data.contributions) {
      for (const client_contrib of contribution.clients) {
        submittedClients.add(client_contrib.client);
      }
    }
    if (submittedClients.has("kilo")) {
      submittedClients.add("kilocode" as SubmissionData["summary"]["clients"][number]);
    }
    const hashData: SubmissionData = {
      ...data,
      summary: {
        ...data.summary,
        clients: Array.from(submittedClients).sort(),
      },
    };

    // ========================================
    // STEP 3: DATABASE OPERATIONS IN TRANSACTION
    // ========================================
    const result = await db.transaction(async (tx) => {
      await tx
        .update(apiTokens)
        .set({ lastUsedAt: new Date() })
        .where(eq(apiTokens.id, tokenRecord.tokenId));

      // ------------------------------------------
      // STEP 3a: Get or create user's submission
      // ------------------------------------------
      const [existingSubmission] = await tx
        .select({
          id: submissions.id,
          longestContinuousMs: submissions.longestContinuousMs,
          maxConcurrentSessions: submissions.maxConcurrentSessions,
          sessionCount: submissions.sessionCount,
        })
        .from(submissions)
        .where(eq(submissions.userId, tokenRecord.userId))
        .for('update')
        .limit(1);

      let submissionId: string;
      let isNewSubmission = false;
      // Whichever row we end up writing to, keep the metrics it already holds
      // so the update below can floor against them.
      let storedSessionMetrics = existingSubmission;

      if (existingSubmission) {
        submissionId = existingSubmission.id;
      } else {
        try {
          const [newSubmission] = await tx.transaction(async (sp) =>
            sp
              .insert(submissions)
              .values({
                userId: tokenRecord.userId,
                totalTokens: 0,
                totalCost: "0",
                inputTokens: 0,
                outputTokens: 0,
                cacheCreationTokens: 0,
                cacheReadTokens: 0,
                dateStart: data.meta.dateRange.start,
                dateEnd: data.meta.dateRange.end,
                sourcesUsed: [],
                modelsUsed: [],
                cliVersion: data.meta.version,
                submissionHash: generateSubmissionHash(hashData),
                hasBackfill: isBackfill,
              })
              .returning({ id: submissions.id })
          );

          submissionId = newSubmission.id;
          isNewSubmission = true;
        } catch (creationErr) {
          if (!isUniqueConstraintViolation(creationErr)) {
            throw creationErr;
          }

          const [racedSubmission] = await tx
            .select({
              id: submissions.id,
              longestContinuousMs: submissions.longestContinuousMs,
              maxConcurrentSessions: submissions.maxConcurrentSessions,
              sessionCount: submissions.sessionCount,
            })
            .from(submissions)
            .where(eq(submissions.userId, tokenRecord.userId))
            .for('update')
            .limit(1);

          if (!racedSubmission) {
            throw creationErr;
          }

          storedSessionMetrics = racedSubmission;

          submissionId = racedSubmission.id;
        }
      }

      const submitDevice = getSubmitDevice(data);
      const submittedAt = new Date();
      const [submittedDevice] = await tx
        .insert(submittedDevices)
        .values({
          userId: tokenRecord.userId,
          deviceKey: submitDevice.key,
          // Only ever the name the payload carried. This column is rendered on
          // the public profile (lib/publicProfileDevices.ts), and the
          // authenticating token's name is `CLI on <hostname>` — falling back
          // to it published the user's machine hostname. Unnamed devices get
          // the "Unnamed device" label from deviceDisplayLabel instead.
          displayName: submitDevice.name,
          lastSubmittedAt: submittedAt,
          updatedAt: submittedAt,
        })
        .onConflictDoUpdate({
          target: [submittedDevices.userId, submittedDevices.deviceKey],
          set: {
            displayName: sql`COALESCE(EXCLUDED.display_name, ${submittedDevices.displayName})`,
            lastSubmittedAt: submittedAt,
            updatedAt: submittedAt,
          },
        })
        .returning({ id: submittedDevices.id });

      // ------------------------------------------
      // STEP 3b: Fetch existing daily breakdown for merge
      // ------------------------------------------
      const fetchExistingDeviceDays = () =>
        tx
          .select({
            id: dailyBreakdown.id,
            date: dailyBreakdown.date,
            timestampMs: dailyBreakdown.timestampMs,
            activeTimeMs: dailyBreakdown.activeTimeMs,
            sourceBreakdown: dailyBreakdown.sourceBreakdown,
          })
          .from(dailyBreakdown)
          .where(
            and(
              eq(dailyBreakdown.submissionId, submissionId),
              eq(dailyBreakdown.submittedDeviceId, submittedDevice.id)
            )
          )
          .for('update');

      let existingDeviceDays = await fetchExistingDeviceDays();

      if (
        existingDeviceDays.length === 0 &&
        !isNewSubmission &&
        submitDevice.key !== LEGACY_SUBMIT_DEVICE_KEY
      ) {
        // The first device-aware submit after the migration should continue
        // the user's legacy bucket instead of counting the same history twice.
        // Once any modern device rows exist, attribution is ambiguous, so the
        // legacy bucket stays separate.
        //
        // Race note: two concurrent submits from the same user can both reach
        // this branch before either has committed. The second UPDATE will try
        // to re-stamp submitted_device_id on rows the first already claimed,
        // which can violate the (submission_id, submitted_device_id, date)
        // unique constraint. The NOT EXISTS dup guard below makes the UPDATE
        // skip rows that would collide, and the savepoint + outer try/catch
        // fall through to the normal insert path if a unique violation still
        // escapes (e.g. via a concurrent INSERT racing the UPDATE window).
        try {
          // Wrap the UPDATE in a savepoint so a unique-constraint violation
          // from a concurrent submit does not poison the enclosing
          // transaction. Drizzle's nested transaction maps to a Postgres
          // SAVEPOINT; throwing inside the inner block rolls back to the
          // savepoint and leaves the outer tx in a usable state.
          await tx.transaction(async (sp) => {
            await sp.execute(sql`
              UPDATE daily_breakdown AS db
              SET submitted_device_id = ${submittedDevice.id}
              WHERE db.submission_id = ${submissionId}
                AND db.submitted_device_id IN (
                  SELECT sd.id
                  FROM submitted_devices AS sd
                  WHERE sd.user_id = ${tokenRecord.userId}
                    AND sd.device_key = ${LEGACY_SUBMIT_DEVICE_KEY}
                )
                AND NOT EXISTS (
                  SELECT 1
                  FROM daily_breakdown AS modern
                  WHERE modern.submission_id = db.submission_id
                    AND modern.submitted_device_id NOT IN (
                      SELECT sd2.id
                      FROM submitted_devices AS sd2
                      WHERE sd2.user_id = ${tokenRecord.userId}
                        AND sd2.device_key = ${LEGACY_SUBMIT_DEVICE_KEY}
                    )
                )
                AND NOT EXISTS (
                  SELECT 1
                  FROM daily_breakdown AS dup
                  WHERE dup.submission_id = db.submission_id
                    AND dup.submitted_device_id = ${submittedDevice.id}
                    AND dup.date = db.date
                )
            `);
          });
        } catch (adoptionErr) {
          // Only a unique-constraint violation (23505) from a concurrent submit
          // racing this UPDATE is a recoverable fall-through: the savepoint
          // rolled back, the outer tx is still usable, and fetchExistingDeviceDays()
          // below picks up rows the other request already claimed so subsequent
          // logic merges rather than re-adopts.
          //
          // Any other failure (timeout, deadlock, permission error) leaves the
          // legacy rows unclaimed. Falling through would then insert the incoming
          // device's overlapping history as a SECOND row, silently inflating
          // totals. Re-throw so the request fails loudly instead of double-counting.
          if (!isUniqueConstraintViolation(adoptionErr)) {
            throw adoptionErr;
          }
          console.warn("Legacy adoption conflict (concurrent submit), falling through:", adoptionErr);
        }
        existingDeviceDays = await fetchExistingDeviceDays();
      }

      // Fetch every device's rows for this submission so the account-wide
      // parser revision floors below consider data submitted from other
      // devices (and the legacy bucket), not just the submitting device.
      const allExistingDays = await tx
        .select({
          id: dailyBreakdown.id,
          date: dailyBreakdown.date,
          sourceBreakdown: dailyBreakdown.sourceBreakdown,
          submittedDeviceId: dailyBreakdown.submittedDeviceId,
        })
        .from(dailyBreakdown)
        .where(eq(dailyBreakdown.submissionId, submissionId))
        .for('update');


      const existingDaysMap = new Map(
        existingDeviceDays.map((d) => [d.date, d])
      );

      // Cross-device reclaim. The v4.6.1 per-device migration handed the
      // pre-migration merged day rows to whichever device happened to hold
      // them, so a user's history can sit under the wrong device. A device
      // submitting a client-day from its own local files is the proof of
      // ownership: an identical copy (same tokens AND same input — counts
      // independent machines cannot coincide on) found on ANOTHER device is
      // always the legacy misattributed twin, so ownership moves to the
      // submitter and the stale twin is stripped. This lets every account's
      // per-device attribution converge to local truth through the normal
      // auto-submit cycle, with no manual repair.
      interface LegacyTwinRef {
        rowId: string;
        originalKey: string;
      }
      const otherDeviceClientSignatures = new Map<
        string,
        Map<string, Map<string, LegacyTwinRef[]>>
      >();
      for (const day of allExistingDays) {
        if (day.submittedDeviceId === submittedDevice.id) continue;
        const rawBreakdown = (day.sourceBreakdown || {}) as Record<string, ClientBreakdownData>;
        let dayMap = otherDeviceClientSignatures.get(day.date);
        if (!dayMap) {
          dayMap = new Map();
          otherDeviceClientSignatures.set(day.date, dayMap);
        }
        for (const [originalKey, clientData] of Object.entries(rawBreakdown)) {
          const client = originalKey === "kilocode" ? "kilo" : originalKey;
          let sigMap = dayMap.get(client);
          if (!sigMap) {
            sigMap = new Map();
            dayMap.set(client, sigMap);
          }
          const signature = `${clientData.tokens ?? 0}:${clientData.input ?? 0}`;
          const refs = sigMap.get(signature) ?? [];
          refs.push({ rowId: day.id, originalKey });
          sigMap.set(signature, refs);
        }
      }
      // rowId -> original client keys to strip from that (other-device) row.
      const reclaimedTwins = new Map<string, Set<string>>();
      const storedClientRevisionFloors = deriveStoredClientRevisionFloors(
        allExistingDays.map((day) =>
          (day.sourceBreakdown || {}) as Record<string, ClientBreakdownData>
        )
      );
      const authoritativeCoverages = (data.clientManifest?.clients ?? [])
        .filter(
          (entry) =>
            entry.coverage &&
            entry.parserRevision >= (storedClientRevisionFloors.get(entry.client) ?? 1)
        )
        .map((entry) => ({
          client: entry.client,
          parserRevision: entry.parserRevision,
          start: entry.coverage!.start,
          end: entry.coverage!.end,
        }));

      // ------------------------------------------
      // STEP 3c: Compute merge results in memory, then batch write
      // ------------------------------------------
      const toInsert: Array<{
        submissionId: string;
        submittedDeviceId: string;
        date: string;
        tokens: number;
        cost: string;
        inputTokens: number;
        outputTokens: number;
        timestampMs: number | null;
        activeTimeMs: number | null;
        sourceBreakdown: Record<string, ClientBreakdownData>;
      }> = [];

      const toUpdate: Array<{
        id: string;
        tokens: number;
        cost: string;
        inputTokens: number;
        outputTokens: number;
        timestampMs: number | null;
        activeTimeMs: number | null;
        sourceBreakdown: Record<string, ClientBreakdownData>;
      }> = [];
      const toDelete: string[] = [];
      const incomingDates = new Set(data.contributions.map((day) => day.date));

      for (const incomingDay of data.contributions) {
        const aggregatedIncomingClientBreakdown = aggregateIncomingClientBreakdowns(
          incomingDay.clients.map((clientContribution) => ({
            client: clientContribution.client,
            modelId: clientContribution.modelId,
            breakdown: clientContributionToBreakdownData(clientContribution),
            provenance: clientContribution.provenance,
          }))
        );
        const revisionFilter = filterClientBreakdownsByRevisionFloor(
          aggregatedIncomingClientBreakdown,
          storedClientRevisionFloors
        );
        const incomingClientBreakdown = revisionFilter.accepted;
        const rejectedClients = new Set(
          revisionFilter.rejected.map((rejected) => rejected.client)
        );
        warnings.push(
          ...revisionFilter.rejected.map(
            (rejected) =>
              `Day ${incomingDay.date}: Ignored ${rejected.client} parser revision ${rejected.incomingRevision} because this account already stored revision ${rejected.storedRevision}.`
          )
        );

        if (isBackfill) {
          // Stamp the per-client origin tag AFTER provenance derivation so it
          // is persisted alongside the coverage metrics. The merge helper
          // re-derives provenance via deriveClientBreakdownProvenance, which
          // carries `origin` through, so the tag survives the merge path too.
          for (const clientBreakdown of Object.values(incomingClientBreakdown)) {
            clientBreakdown.provenance = {
              ...(clientBreakdown.provenance ??
                deriveClientBreakdownProvenance(clientBreakdown)),
              origin: "backfill",
            };
          }
        }

        const existingDay = existingDaysMap.get(incomingDay.date);

        // Reclaim identical legacy twins from other devices (see map
        // construction above): the incoming copy is attached to this device
        // normally, and the stale twin is queued for stripping.
        const daySignatures = otherDeviceClientSignatures.get(incomingDay.date);
        if (daySignatures) {
          for (const [client, clientData] of Object.entries(incomingClientBreakdown)) {
            const tokens = clientData.tokens ?? 0;
            if (tokens < CROSS_DEVICE_DUP_MIN_TOKENS) continue;
            const refs = daySignatures
              .get(client)
              ?.get(`${tokens}:${clientData.input ?? 0}`);
            if (!refs || refs.length === 0) continue;
            for (const ref of refs) {
              let keys = reclaimedTwins.get(ref.rowId);
              if (!keys) {
                keys = new Set();
                reclaimedTwins.set(ref.rowId, keys);
              }
              keys.add(ref.originalKey);
            }
            warnings.push(
              `Day ${incomingDay.date}: Reclaimed ${client} from another of your devices (identical legacy copy).`
            );
          }
        }

        if (existingDay) {
          const storedClientBreakdown = normalizeClientBreakdownAliases(
            (existingDay.sourceBreakdown || {}) as Record<string, ClientBreakdownData>
          );
          const coverageResult = applyAuthoritativeClientCoverage(
            storedClientBreakdown,
            incomingClientBreakdown,
            incomingDay.date,
            authoritativeCoverages
          );
          warnings.push(
            ...coverageResult.tombstonedClients.map(
              (client) =>
                `Day ${incomingDay.date}: Removed stale ${client} data under authoritative replacement coverage.`
            )
          );
          const mergeResult = mergeClientBreakdownsWithRegressionGuard(
            coverageResult.mergeBase,
            incomingClientBreakdown,
            new Set(
              Array.from(submittedClients).filter(
                (client) => !rejectedClients.has(client)
              )
            )
          );
          warnings.push(
            ...mergeResult.warnings.map((warning) => `Day ${incomingDay.date}: ${warning}`)
          );
          const mergedClientBreakdown = mergeResult.merged;
          if (Object.keys(mergedClientBreakdown).length === 0) {
            toDelete.push(existingDay.id);
            continue;
          }
          const dayTotals = recalculateDayTotals(mergedClientBreakdown);

          toUpdate.push({
            id: existingDay.id,
            tokens: dayTotals.tokens,
            cost: dayTotals.cost.toFixed(4),
            inputTokens: dayTotals.inputTokens,
            outputTokens: dayTotals.outputTokens,
            timestampMs: mergeTimestampMs(existingDay.timestampMs, incomingDay.timestampMs ?? null),
            activeTimeMs: mergeMonotonic(existingDay.activeTimeMs, incomingDay.activeTimeMs),
            sourceBreakdown: mergedClientBreakdown,
          });
        } else if (Object.keys(incomingClientBreakdown).length > 0) {
          const dayTotals = recalculateDayTotals(incomingClientBreakdown);

          toInsert.push({
            submissionId,
            submittedDeviceId: submittedDevice.id,
            date: incomingDay.date,
            tokens: dayTotals.tokens,
            cost: dayTotals.cost.toFixed(4),
            inputTokens: dayTotals.inputTokens,
            outputTokens: dayTotals.outputTokens,
            timestampMs: incomingDay.timestampMs ?? null,
            activeTimeMs: incomingDay.activeTimeMs ?? null,
            sourceBreakdown: incomingClientBreakdown,
          });
        }
      }

      for (const existingDay of existingDeviceDays) {
        if (incomingDates.has(existingDay.date)) continue;
        const storedClientBreakdown = normalizeClientBreakdownAliases(
          (existingDay.sourceBreakdown || {}) as Record<string, ClientBreakdownData>
        );
        const coverageResult = applyAuthoritativeClientCoverage(
          storedClientBreakdown,
          {},
          existingDay.date,
          authoritativeCoverages
        );
        if (coverageResult.tombstonedClients.length === 0) continue;

        warnings.push(
          ...coverageResult.tombstonedClients.map(
            (client) =>
              `Day ${existingDay.date}: Removed stale ${client} data under authoritative replacement coverage.`
          )
        );
        if (Object.keys(coverageResult.mergeBase).length === 0) {
          toDelete.push(existingDay.id);
          continue;
        }
        const dayTotals = recalculateDayTotals(coverageResult.mergeBase);
        toUpdate.push({
          id: existingDay.id,
          tokens: dayTotals.tokens,
          cost: dayTotals.cost.toFixed(4),
          inputTokens: dayTotals.inputTokens,
          outputTokens: dayTotals.outputTokens,
          timestampMs: existingDay.timestampMs ?? null,
          activeTimeMs: existingDay.activeTimeMs ?? null,
          sourceBreakdown: coverageResult.mergeBase,
        });
      }

      // Apply queued reclaims: strip moved client-days from their old
      // (other-device) rows and recompute those rows' totals. Runs before the
      // submission-total recompute below, so totals net out exactly — a
      // reclaim moves usage between devices, never changes the account total.
      if (reclaimedTwins.size > 0) {
        const rowsById = new Map(allExistingDays.map((day) => [day.id, day]));
        for (const [rowId, keys] of reclaimedTwins) {
          const row = rowsById.get(rowId);
          if (!row) continue;
          const remaining = {
            ...((row.sourceBreakdown || {}) as Record<string, ClientBreakdownData>),
          };
          for (const key of keys) {
            delete remaining[key];
          }
          if (Object.keys(remaining).length === 0) {
            await tx.delete(dailyBreakdown).where(eq(dailyBreakdown.id, rowId));
          } else {
            const totals = recalculateDayTotals(remaining);
            await tx
              .update(dailyBreakdown)
              .set({
                tokens: totals.tokens,
                cost: totals.cost.toFixed(4),
                inputTokens: totals.inputTokens,
                outputTokens: totals.outputTokens,
                sourceBreakdown: remaining,
              })
              .where(eq(dailyBreakdown.id, rowId));
          }
        }
      }

      // Batch INSERT new days via raw SQL VALUES list, chunked to stay under
      // PostgreSQL's 65,535 bound-parameter limit (10 params/row here --
      // a large historical backfill can otherwise exceed it in one statement).
      // ON CONFLICT (submission_id, submitted_device_id, date) is a defensive
      // fallback for concurrent submits from the same device racing between
      // the SELECT above and this INSERT. Distinct devices own distinct rows,
      // so their independent usage remains additive.
      for (let i = 0; i < toInsert.length; i += INSERT_CHUNK_SIZE) {
        const chunk = toInsert.slice(i, i + INSERT_CHUNK_SIZE);
        const insertValuesClauses = chunk.map(
          (row) =>
            sql`(${row.submissionId}::uuid, ${row.submittedDeviceId}::uuid, ${row.date}, ${row.tokens}::bigint, ${row.cost}::numeric(14,4), ${row.inputTokens}::bigint, ${row.outputTokens}::bigint, ${row.timestampMs}::bigint, ${row.activeTimeMs}::bigint, ${JSON.stringify(row.sourceBreakdown)}::jsonb)`
        );

        const insertValuesList = sql.join(insertValuesClauses, sql`, `);

        await tx.execute(sql`
          INSERT INTO daily_breakdown (
            submission_id, submitted_device_id, date, tokens, cost,
            input_tokens, output_tokens, timestamp_ms, active_time_ms, source_breakdown
          )
          VALUES ${insertValuesList}
          ON CONFLICT (submission_id, submitted_device_id, date) DO UPDATE SET
            tokens = EXCLUDED.tokens,
            cost = EXCLUDED.cost,
            input_tokens = EXCLUDED.input_tokens,
            output_tokens = EXCLUDED.output_tokens,
            timestamp_ms = EXCLUDED.timestamp_ms,
            active_time_ms = EXCLUDED.active_time_ms,
            source_breakdown = EXCLUDED.source_breakdown
        `);
      }

      if (toDelete.length > 0) {
        await tx.delete(dailyBreakdown).where(inArray(dailyBreakdown.id, toDelete));
      }

      // Batch UPDATE existing rows for this device via a raw SQL VALUES list,
      // chunked for the same parameter-limit reason as the INSERT above.
      // Device ownership is immutable here: another device writes its own row.
      for (let i = 0; i < toUpdate.length; i += INSERT_CHUNK_SIZE) {
        const chunk = toUpdate.slice(i, i + INSERT_CHUNK_SIZE);
        const valuesClauses = chunk.map(
          (row) =>
            sql`(${row.id}::uuid, ${row.tokens}::bigint, ${row.cost}::numeric(14,4), ${row.inputTokens}::bigint, ${row.outputTokens}::bigint, ${row.timestampMs}::bigint, ${row.activeTimeMs}::bigint, ${JSON.stringify(row.sourceBreakdown)}::jsonb)`
        );

        const valuesList = sql.join(valuesClauses, sql`, `);

        await tx.execute(sql`
          UPDATE daily_breakdown AS d SET
            tokens = batch.tokens,
            cost = batch.cost,
            input_tokens = batch.input_tokens,
            output_tokens = batch.output_tokens,
            timestamp_ms = batch.timestamp_ms,
            active_time_ms = batch.active_time_ms,
            source_breakdown = batch.source_breakdown
          FROM (VALUES ${valuesList})
            AS batch(id, tokens, cost, input_tokens, output_tokens, timestamp_ms, active_time_ms, source_breakdown)
          WHERE d.id = batch.id
        `);
      }

      // ------------------------------------------
      // STEP 3d: Recalculate submission totals from ALL daily breakdown
      // ------------------------------------------
      const [aggregates] = await tx
        .select({
          totalTokens: sql<number>`COALESCE(SUM(${dailyBreakdown.tokens}), 0)::bigint`,
          totalCost: sql<string>`COALESCE(SUM(CAST(${dailyBreakdown.cost} AS DECIMAL(14,4))), 0)::text`,
          inputTokens: sql<number>`COALESCE(SUM(${dailyBreakdown.inputTokens}), 0)::bigint`,
          outputTokens: sql<number>`COALESCE(SUM(${dailyBreakdown.outputTokens}), 0)::bigint`,
          dateStart: sql<string>`MIN(${dailyBreakdown.date})`,
          dateEnd: sql<string>`MAX(${dailyBreakdown.date})`,
          activeDays: sql<number>`COUNT(DISTINCT CASE WHEN ${dailyBreakdown.tokens} > 0 THEN ${dailyBreakdown.date} END)::int`,
          totalActiveTimeMs: sql<number>`COALESCE(SUM(${dailyBreakdown.activeTimeMs}), 0)::bigint`,
          rowCount: sql<number>`COUNT(*)::int`,
        })
        .from(dailyBreakdown)
        .where(eq(dailyBreakdown.submissionId, submissionId));

      const allDays = await tx
        .select({
          sourceBreakdown: dailyBreakdown.sourceBreakdown,
        })
        .from(dailyBreakdown)
        .where(eq(dailyBreakdown.submissionId, submissionId));

      const allClients = new Set<string>();
      const allModels = new Set<string>();
      let totalCacheRead = 0;
      let totalCacheCreation = 0;
      let totalReasoning = 0;

      for (const day of allDays) {
        if (day.sourceBreakdown) {
          for (const [rawClientName, clientData] of Object.entries(day.sourceBreakdown)) {
            const clientName = rawClientName === "kilocode" ? "kilo" : rawClientName;
            allClients.add(clientName);
            const cd = clientData as ClientBreakdownData;
            if (cd.models) {
              for (const modelId of Object.keys(cd.models)) {
                allModels.add(modelId);
              }
            } else if (cd.modelId) {
              allModels.add(cd.modelId);
            }
            totalCacheRead += cd.cacheRead || 0;
            totalCacheCreation += cd.cacheWrite || 0;
            totalReasoning += cd.reasoning || 0;
          }
        }
      }

      // ------------------------------------------
      // STEP 3e: Update submission record
      // ------------------------------------------
      await tx
        .update(submissions)
        .set({
          totalTokens: aggregates.totalTokens,
          totalCost: aggregates.totalCost,
          inputTokens: aggregates.inputTokens,
          outputTokens: aggregates.outputTokens,
          cacheReadTokens: totalCacheRead,
          cacheCreationTokens: totalCacheCreation,
          reasoningTokens: totalReasoning,
          dateStart: aggregates.dateStart,
          dateEnd: aggregates.dateEnd,
           sourcesUsed: Array.from(allClients),
           modelsUsed: Array.from(allModels),
          cliVersion: data.meta.version,
          submissionHash: generateSubmissionHash(hashData),
          // Sticky: only ever set to true. A later live CLI submit omits the
          // key entirely, so it can never reset an account's backfill flag —
          // the merged totals still include the imported history.
          ...(isBackfill ? { hasBackfill: true } : {}),
          submitCount: sql`COALESCE(submit_count, 0) + 1`,
          schemaVersion: sql`GREATEST(COALESCE(${submissions.schemaVersion}, 0), ${submitDevice.schemaVersion})`,
          totalActiveTimeMs: aggregates.totalActiveTimeMs,
          // Session-shape metrics cannot be safely recomputed from daily active-time buckets.
          ...(data.timeMetrics ? {
            longestContinuousMs: mergeMonotonic(
              storedSessionMetrics?.longestContinuousMs,
              data.timeMetrics.longestContinuousMs
            ),
            maxConcurrentSessions: mergeMonotonic(
              storedSessionMetrics?.maxConcurrentSessions,
              data.timeMetrics.maxConcurrentSessions
            ),
            sessionCount: mergeMonotonic(
              storedSessionMetrics?.sessionCount,
              data.timeMetrics.sessionCount
            ),
          } : {}),
          mcpServers: mcpServers && mcpServers.length > 0 ? mcpServers : null,
          updatedAt: new Date(),
        })
        .where(eq(submissions.id, submissionId));

      return {
        submissionId,
        isNewSubmission,
        metrics: {
          // `::bigint` reaches us as a string — the driver will not coerce int8
          // to a JS number on its own. Send a real number: the field is typed
          // `sql<number>` above, /api/leaderboard already serializes it that
          // way, and a string here breaks strictly-typed clients. Totals are
          // far below 2^53, so this is lossless.
          totalTokens: Number(aggregates.totalTokens),
          totalCost: parseFloat(aggregates.totalCost),
          dateRange: {
            start: aggregates.dateStart,
            end: aggregates.dateEnd,
          },
          activeDays: aggregates.activeDays,
          clients: Array.from(allClients),
        },
      };
    });

    const usernameCacheKey = normalizeUsernameCacheKey(tokenRecord.username);
    try {
      revalidateTag("leaderboard", "max");
      revalidateTag(`user:${usernameCacheKey}`, "max");
      revalidateTag("user-rank", "max");
      revalidateTag(`user-rank:${usernameCacheKey}`, "max");
    } catch (e) {
      console.error("Public cache invalidation failed:", e);
    }

    try {
      revalidateUsernamePaths(tokenRecord.username);
    } catch (e) {
      console.error("Username path revalidation failed:", e);
    }

    return NextResponse.json({
      success: true,
      submissionId: result.submissionId,
      username: tokenRecord.username,
      metrics: result.metrics,
      mode: result.isNewSubmission ? "create" : "merge",
      warnings: warnings.length > 0 ? warnings : undefined,
    });
  } catch (error) {
    console.error("Submit error:", error);
    return NextResponse.json(
      { error: "Internal server error" },
      { status: 500 }
    );
  }
}
