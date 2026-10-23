/**
 * Client-level merge helpers for submission API
 */

import { mergeSpeedUsage, type SpeedUsage } from "../speedUsage";

export interface ModelBreakdownData {
  tokens: number;
  cost: number;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  reasoning: number;
  messages: number;
  /**
   * Known-speed usage for this model only (absent === all Unknown). Lives on
   * the nested model, never on the client-level totals.
   */
  speedUsage?: SpeedUsage[];
}

interface ClientBreakdownProvenanceData {
  schemaVersion: number;
  messageCount: number;
  modelCount: number;
  /**
   * "backfill" when this client's contribution was written by a
   * backfill-origin submission (`tokens import`); absent (or "cli") for
   * locally-scanned CLI usage. Preserved by deriveClientBreakdownProvenance
   * so merges do not silently drop the tag.
   */
  origin?: "cli" | "backfill";
}

export interface ClientBreakdownData {
  tokens: number;
  cost: number;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  reasoning: number;
  messages: number;
  models: Record<string, ModelBreakdownData>;
  provenance?: ClientBreakdownProvenanceData;
  /** @deprecated Legacy field for backward compat - use models instead */
  modelId?: string;
}

interface MergeClientBreakdownsResult {
  merged: Record<string, ClientBreakdownData>;
  warnings: string[];
}

interface IncomingClientModelBreakdown {
  client: string;
  modelId: string;
  breakdown: ModelBreakdownData;
  provenance?: ClientBreakdownProvenanceData;
}

interface RejectedClientRevision {
  client: string;
  incomingRevision: number;
  storedRevision: number;
}

interface FilterClientBreakdownsResult {
  accepted: Record<string, ClientBreakdownData>;
  rejected: RejectedClientRevision[];
}

interface AuthoritativeClientCoverage {
  client: string;
  parserRevision: number;
  start: string;
  end: string;
}

interface ApplyAuthoritativeClientCoverageResult {
  mergeBase: Record<string, ClientBreakdownData>;
  tombstonedClients: string[];
}

interface DayTotals {
  tokens: number;
  cost: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  reasoningTokens: number;
}

export function recalculateDayTotals(
  clientBreakdown: Record<string, ClientBreakdownData>
): DayTotals {
  let tokens = 0;
  let cost = 0;
  let inputTokens = 0;
  let outputTokens = 0;
  let cacheReadTokens = 0;
  let cacheWriteTokens = 0;
  let reasoningTokens = 0;

  for (const client of Object.values(clientBreakdown)) {
    tokens += client.tokens || 0;
    cost += client.cost || 0;
    inputTokens += client.input || 0;
    outputTokens += client.output || 0;
    cacheReadTokens += client.cacheRead || 0;
    cacheWriteTokens += client.cacheWrite || 0;
    reasoningTokens += client.reasoning || 0;
  }

  return {
    tokens,
    cost,
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheWriteTokens,
    reasoningTokens,
  };
}

function formatTokens(value: number): string {
  return Math.round(value).toLocaleString("en-US");
}

export function deriveClientBreakdownProvenance(
  breakdown: ClientBreakdownData
): ClientBreakdownProvenanceData {
  const modelCount = breakdown.models
    ? Object.keys(breakdown.models).length
    : breakdown.modelId
    ? 1
    : 0;

  const origin = breakdown.provenance?.origin;

  return {
    schemaVersion: Math.max(1, breakdown.provenance?.schemaVersion ?? 1),
    messageCount: Math.max(
      0,
      breakdown.provenance?.messageCount ?? 0,
      breakdown.messages ?? 0
    ),
    modelCount: Math.max(0, breakdown.provenance?.modelCount ?? 0, modelCount),
    // Carry the origin tag through re-derivation (merges, alias folding) so
    // a backfill-tagged client row keeps its tag.
    ...(origin ? { origin } : {}),
  };
}

function withDerivedProvenance(breakdown: ClientBreakdownData): ClientBreakdownData {
  return {
    ...breakdown,
    provenance: deriveClientBreakdownProvenance(breakdown),
  };
}

/**
 * Add `model` into `models[modelId]`, summing counters and speed metadata.
 * Never mutates `model` or any speedUsage array it references.
 */
export function addModelBreakdown(
  models: Record<string, ModelBreakdownData>,
  modelId: string,
  model: ModelBreakdownData
): void {
  const existing = models[modelId];
  const speedUsage = mergeSpeedUsage(existing?.speedUsage, model.speedUsage);
  const next: ModelBreakdownData = {
    tokens: (existing?.tokens || 0) + (model.tokens || 0),
    cost: (existing?.cost || 0) + (model.cost || 0),
    input: (existing?.input || 0) + (model.input || 0),
    output: (existing?.output || 0) + (model.output || 0),
    cacheRead: (existing?.cacheRead || 0) + (model.cacheRead || 0),
    cacheWrite: (existing?.cacheWrite || 0) + (model.cacheWrite || 0),
    reasoning: (existing?.reasoning || 0) + (model.reasoning || 0),
    messages: (existing?.messages || 0) + (model.messages || 0),
  };
  if (existing?.speedUsage || model.speedUsage) {
    next.speedUsage = speedUsage ?? [];
  }
  models[modelId] = next;
}

/**
 * "kilocode" is a legacy alias of "kilo". A stored source_breakdown can still
 * carry a "kilocode" key written before submit-time normalization existed;
 * fold it into "kilo" so merges treat both as the SAME client instead of
 * summing them as disjoint clients.
 */
const LEGACY_CLIENT_ALIASES: Record<string, string> = { kilocode: "kilo" };

export function normalizeClientBreakdownAliases(
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
    for (const [modelId, modelData] of Object.entries(data.models || {})) {
      addModelBreakdown(existing.models, modelId, modelData);
    }
    existing.provenance = deriveClientBreakdownProvenance(existing);
  }

  return normalized;
}

export function aggregateIncomingClientBreakdowns(
  contributions: IncomingClientModelBreakdown[]
): Record<string, ClientBreakdownData> {
  const aggregated: Record<string, ClientBreakdownData> = {};

  for (const contribution of contributions) {
    const { client, modelId, breakdown, provenance } = contribution;
    const existing = aggregated[client];

    if (!existing) {
      const models: Record<string, ModelBreakdownData> = {};
      addModelBreakdown(models, modelId, breakdown);
      const clientBreakdown: ClientBreakdownData = {
        // Client-level totals only: speed metadata stays on the nested model.
        tokens: breakdown.tokens,
        cost: breakdown.cost,
        input: breakdown.input,
        output: breakdown.output,
        cacheRead: breakdown.cacheRead,
        cacheWrite: breakdown.cacheWrite,
        reasoning: breakdown.reasoning,
        messages: breakdown.messages,
        models,
        provenance: {
          schemaVersion: Math.max(1, provenance?.schemaVersion ?? 1),
          messageCount: Math.max(0, provenance?.messageCount ?? 0),
          modelCount: Math.max(0, provenance?.modelCount ?? 0),
        },
      };
      aggregated[client] = withDerivedProvenance(clientBreakdown);
      continue;
    }

    existing.tokens += breakdown.tokens;
    existing.cost += breakdown.cost;
    existing.input += breakdown.input;
    existing.output += breakdown.output;
    existing.cacheRead += breakdown.cacheRead;
    existing.cacheWrite += breakdown.cacheWrite;
    existing.reasoning = (existing.reasoning || 0) + breakdown.reasoning;
    existing.messages += breakdown.messages;
    addModelBreakdown(existing.models, modelId, breakdown);

    existing.provenance = deriveClientBreakdownProvenance({
      ...existing,
      provenance: {
        schemaVersion: Math.min(
          existing.provenance?.schemaVersion ?? 1,
          provenance?.schemaVersion ?? 1
        ),
        messageCount: Math.max(
          existing.provenance?.messageCount ?? 0,
          provenance?.messageCount ?? 0
        ),
        modelCount: Math.max(
          existing.provenance?.modelCount ?? 0,
          provenance?.modelCount ?? 0
        ),
      },
    });
  }

  return aggregated;
}

export function deriveStoredClientRevisionFloors(
  storedDays: Array<Record<string, ClientBreakdownData> | null | undefined>
): Map<string, number> {
  const floors = new Map<string, number>();

  for (const storedDay of storedDays) {
    for (const [client, breakdown] of Object.entries(storedDay ?? {})) {
      const revision = deriveClientBreakdownProvenance(breakdown).schemaVersion;
      floors.set(client, Math.max(floors.get(client) ?? 1, revision));
    }
  }

  return floors;
}

export function filterClientBreakdownsByRevisionFloor(
  incoming: Record<string, ClientBreakdownData>,
  storedRevisionFloors: Map<string, number>
): FilterClientBreakdownsResult {
  const accepted: Record<string, ClientBreakdownData> = {};
  const rejected: RejectedClientRevision[] = [];

  for (const [client, breakdown] of Object.entries(incoming)) {
    const normalized = withDerivedProvenance(breakdown);
    const incomingRevision = normalized.provenance?.schemaVersion ?? 1;
    const storedRevision = storedRevisionFloors.get(client) ?? 1;

    if (incomingRevision < storedRevision) {
      rejected.push({ client, incomingRevision, storedRevision });
    } else {
      accepted[client] = normalized;
    }
  }

  return { accepted, rejected };
}

export function applyAuthoritativeClientCoverage(
  existing: Record<string, ClientBreakdownData>,
  incoming: Record<string, ClientBreakdownData>,
  date: string,
  coverages: AuthoritativeClientCoverage[]
): ApplyAuthoritativeClientCoverageResult {
  const mergeBase = { ...existing };
  const tombstonedClients: string[] = [];

  for (const coverage of coverages) {
    if (
      date >= coverage.start &&
      date <= coverage.end &&
      mergeBase[coverage.client]
    ) {
      delete mergeBase[coverage.client];
      if (!incoming[coverage.client]) {
        tombstonedClients.push(coverage.client);
      }
    }
  }

  return { mergeBase, tombstonedClients };
}

export function mergeClientBreakdownsWithRegressionGuard(
  existing: Record<string, ClientBreakdownData> | null | undefined,
  incoming: Record<string, ClientBreakdownData>,
  incomingClients: Set<string>
): MergeClientBreakdownsResult {
  const merged: Record<string, ClientBreakdownData> = { ...(existing || {}) };
  const warnings: string[] = [];

  for (const clientName of incomingClients) {
    const existingClient = existing?.[clientName];
    const incomingClient = incoming[clientName];

    if (!incomingClient) {
      if (existingClient && existingClient.tokens > 0) {
        merged[clientName] = withDerivedProvenance(existingClient);
        warnings.push(
          `Preserved ${clientName} because it disappeared from this same-device resubmit; kept ${formatTokens(existingClient.tokens)} tokens.`
        );
      } else {
        delete merged[clientName];
      }
      continue;
    }

    const nextClient = withDerivedProvenance(incomingClient);
    const existingWithProvenance = existingClient
      ? withDerivedProvenance(existingClient)
      : undefined;
    const existingSchemaVersion = existingWithProvenance?.provenance?.schemaVersion ?? 1;
    const incomingSchemaVersion = nextClient.provenance?.schemaVersion ?? 1;

    if (existingWithProvenance && incomingSchemaVersion < existingSchemaVersion) {
      merged[clientName] = existingWithProvenance;
      warnings.push(
        `Preserved ${clientName} because parser revision ${incomingSchemaVersion} is older than stored revision ${existingSchemaVersion}.`
      );
      continue;
    }

    if (
      existingWithProvenance &&
      incomingSchemaVersion === existingSchemaVersion &&
      nextClient.tokens < existingWithProvenance.tokens
    ) {
      // Within one parser revision, a token decrease signals a regression
      // (e.g. the CLI re-parsed only a subset of history). A newer revision is
      // allowed to submit a deliberate lower correction before reaching here.
      merged[clientName] = existingWithProvenance;
      const existingTokens = formatTokens(existingWithProvenance.tokens);
      const nextTokens = formatTokens(nextClient.tokens);
      warnings.push(
        `Preserved ${clientName} because this same-device resubmit would reduce ${existingTokens} tokens to ${nextTokens}.`
      );
      continue;
    }

    // An older CLI can still add usage without knowing this optional field.
    // Keep previously identified usage as a subset; only the added remainder
    // becomes Unknown. A new CLI's explicit [] can clear stale attribution.
    if (existingWithProvenance && incomingSchemaVersion === existingSchemaVersion) {
      nextClient.models = { ...nextClient.models };
      for (const [modelId, model] of Object.entries(nextClient.models)) {
        const previous = existingWithProvenance.models?.[modelId];
        if (
          model.speedUsage === undefined && previous?.speedUsage &&
          model.tokens >= previous.tokens && model.messages >= previous.messages
        ) {
          nextClient.models[modelId] = { ...model, speedUsage: previous.speedUsage };
        }
      }
    }
    merged[clientName] = nextClient;
  }

  return { merged, warnings };
}

export function clientContributionToBreakdownData(
  client_contrib: {
    tokens: { input: number; output: number; cacheRead: number; cacheWrite: number; reasoning?: number };
    cost: number;
    modelId: string;
    messages: number;
    speedUsage?: readonly SpeedUsage[];
  }
): ModelBreakdownData {
  const { input, output, cacheRead, cacheWrite, reasoning = 0 } = client_contrib.tokens;
  const speedUsage = mergeSpeedUsage(client_contrib.speedUsage);
  return {
    tokens: input + output + cacheRead + cacheWrite + reasoning,
    cost: client_contrib.cost,
    input,
    output,
    cacheRead,
    cacheWrite,
    reasoning,
    messages: client_contrib.messages,
    ...(client_contrib.speedUsage ? { speedUsage: speedUsage ?? [] } : {}),
  };
}

/**
 * Merge two nullable timestamps, keeping the earliest non-null value.
 * Used by both submit and profile aggregation to maintain consistent merge semantics.
 */
export function mergeTimestampMs(
  existing: number | null | undefined,
  incoming: number | null | undefined,
): number | null {
  if (incoming != null && existing != null) return Math.min(existing, incoming);
  return incoming ?? existing ?? null;
}
