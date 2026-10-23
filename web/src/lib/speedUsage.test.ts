/// <reference types="bun-types" />
import { describe, expect, test } from "bun:test";
import { generateSubmissionHash, validateSubmission } from "./validation/submission";
import {
  aggregateIncomingClientBreakdowns,
  clientContributionToBreakdownData,
  mergeClientBreakdownsWithRegressionGuard,
  normalizeClientBreakdownAliases,
  type ClientBreakdownData,
} from "./db/helpers";
import type { SpeedUsage } from "./speedUsage";

type Contribution = {
  client: string;
  modelId: string;
  providerId?: string;
  tokens: { input: number; output: number; cacheRead: number; cacheWrite: number; reasoning: number };
  cost: number;
  messages: number;
  provenance?: { schemaVersion: number; messageCount: number; modelCount: number };
  speedUsage?: unknown[];
};

// Each contribution totals input + output + 16 tokens.
function contrib(
  client: string,
  modelId: string,
  input: number,
  output: number,
  messages: number,
  extra: Partial<Contribution> = {}
): Contribution {
  return {
    client,
    modelId,
    tokens: { input, output, cacheRead: 10, cacheWrite: 5, reasoning: 1 },
    cost: 0.5,
    messages,
    ...extra,
  };
}

function withRevision(c: Contribution, schemaVersion: number): Contribution {
  return { ...c, provenance: { schemaVersion, messageCount: c.messages, modelCount: 1 } };
}

function submission(clients: Contribution[]) {
  const sum = (pick: (c: Contribution) => number) => clients.reduce((s, c) => s + pick(c), 0);
  const tokens = sum((c) => Object.values(c.tokens).reduce((a, b) => a + b, 0));
  const cost = sum((c) => c.cost);
  const range = { start: "2026-10-01", end: "2026-10-01" };
  const revisions = new Map(clients.map((c) => [c.client, c.provenance?.schemaVersion ?? 1]));
  const needsManifest = [...revisions.values()].some((r) => r > 1);
  return {
    meta: { generatedAt: "2026-10-02T00:00:00Z", version: "1.0.0", dateRange: range },
    ...(needsManifest
      ? {
          device: { id: "device-1" },
          clientManifest: {
            schemaVersion: 1,
            clients: [...revisions].map(([client, parserRevision]) => ({ client, parserRevision })),
          },
        }
      : {}),
    summary: {
      totalTokens: tokens,
      totalCost: cost,
      totalDays: 1,
      activeDays: 1,
      averagePerDay: cost,
      maxCostInSingleDay: cost,
      clients: [...revisions.keys()],
      models: [...new Set(clients.map((c) => c.modelId))],
    },
    years: [{ year: "2026", totalTokens: tokens, totalCost: cost, range }],
    contributions: [
      {
        date: "2026-10-01",
        totals: { tokens, cost, messages: sum((c) => c.messages) },
        intensity: 1,
        tokenBreakdown: {
          input: sum((c) => c.tokens.input),
          output: sum((c) => c.tokens.output),
          cacheRead: sum((c) => c.tokens.cacheRead),
          cacheWrite: sum((c) => c.tokens.cacheWrite),
          reasoning: sum((c) => c.tokens.reasoning),
        },
        clients,
      },
    ],
  };
}

function hashOf(clients: Contribution[]): string {
  const result = validateSubmission(submission(clients));
  expect(result.errors).toEqual([]);
  return generateSubmissionHash(result.data!);
}

const legacyClients = () => [
  contrib("codex", "gpt-5", 1000, 200, 4, { providerId: "openai" }),
  contrib("claude", "claude-opus-4", 500, 100, 2),
];

// Computed with the pre-speedUsage implementation of generateSubmissionHash.
const LEGACY_HASH = "f8e6a787f523f85e2292adbacbb50234ca0c603859156834c515a59198e657a5";

const fast = (source: "reported" | "selected", tokens: number, messages: number) => ({
  speed: "fast",
  source,
  tokens,
  messages,
});

describe("speedUsage submission contract", () => {
  test("legacy payloads validate and keep their previous hash (absent or empty)", () => {
    expect(hashOf(legacyClients())).toBe(LEGACY_HASH);
    const empty = legacyClients().map((c) => ({ ...c, speedUsage: [] }));
    expect(hashOf(empty)).toBe(LEGACY_HASH);
  });

  test("accepts mixed reported/selected speeds and stores them on the nested model only", () => {
    const codex = withRevision(
      contrib("codex", "gpt-5", 1000, 200, 4, {
        speedUsage: [
          { speed: "ultrafast", source: "selected", tokens: 100, messages: 1 },
          fast("reported", 600, 2),
          { speed: "standard", source: "reported", tokens: 300, messages: 1 },
        ],
      }),
      2
    );
    const result = validateSubmission(submission([codex]));
    expect(result.errors).toEqual([]);

    const day = result.data!.contributions[0];
    const aggregated = aggregateIncomingClientBreakdowns(
      day.clients.map((c) => ({
        client: c.client,
        modelId: c.modelId,
        breakdown: clientContributionToBreakdownData(c),
        provenance: c.provenance,
      }))
    );
    expect(aggregated.codex.tokens).toBe(1216);
    expect("speedUsage" in aggregated.codex).toBe(false);
    expect(aggregated.codex.models["gpt-5"].tokens).toBe(1216);
    // Canonical (speed, source) order; uncovered 216 tokens remain Unknown.
    expect(aggregated.codex.models["gpt-5"].speedUsage).toEqual([
      { speed: "standard", source: "reported", tokens: 300, messages: 1 },
      { speed: "fast", source: "reported", tokens: 600, messages: 2 },
      { speed: "ultrafast", source: "selected", tokens: 100, messages: 1 },
    ]);
  });

  test("speed metadata alone changes the hash; entry order does not", () => {
    const tagged = (entries: unknown[]) =>
      legacyClients().map((c) => (c.client === "codex" ? { ...c, speedUsage: entries } : c));
    const a = hashOf(tagged([fast("reported", 600, 2), fast("selected", 100, 1)]));
    const b = hashOf(tagged([fast("selected", 100, 1), fast("reported", 600, 2)]));
    const c = hashOf(tagged([fast("reported", 601, 2), fast("selected", 100, 1)]));
    expect(a).not.toBe(LEGACY_HASH);
    expect(a).toBe(b);
    expect(c).not.toBe(a);
  });

  test("rejects invalid counts, duplicate pairs, unknown values, and over-total sums", () => {
    const invalid: unknown[][] = [
      [fast("reported", -1, 0)],
      [fast("reported", 1.5, 0)],
      [fast("reported", Number.MAX_SAFE_INTEGER + 1, 0)],
      [fast("reported", 10, 1), fast("reported", 20, 1)],
      [{ speed: "turbo", source: "reported", tokens: 1, messages: 0 }],
      [{ speed: "fast", source: "guessed", tokens: 1, messages: 0 }],
      // Contribution totals: 1216 tokens, 4 messages.
      [fast("reported", 1000, 1), fast("selected", 217, 1)],
      [fast("reported", 10, 3), fast("selected", 10, 2)],
      Array.from({ length: 7 }, () => fast("reported", 0, 0)),
    ];
    for (const speedUsage of invalid) {
      const result = validateSubmission(
        submission([contrib("codex", "gpt-5", 1000, 200, 4, { speedUsage })])
      );
      expect(result.valid).toBe(false);
    }

    // Exactly the totals is fine (all usage known).
    const exact = validateSubmission(
      submission([
        contrib("codex", "gpt-5", 1000, 200, 4, {
          speedUsage: [fast("reported", 1000, 3), fast("selected", 216, 1)],
        }),
      ])
    );
    expect(exact.errors).toEqual([]);
  });
});

describe("speedUsage storage merges", () => {
  const model = (tokens: number, messages: number, speedUsage?: SpeedUsage[]) => ({
    tokens,
    cost: 1,
    input: tokens,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    reasoning: 0,
    messages,
    ...(speedUsage ? { speedUsage } : {}),
  });

  test("incoming contributions for one model sum per pair without changing totals", () => {
    const first: SpeedUsage[] = [{ speed: "fast", source: "reported", tokens: 40, messages: 1 }];
    const aggregated = aggregateIncomingClientBreakdowns([
      { client: "claude", modelId: "opus", breakdown: model(100, 2, first) },
      {
        client: "claude",
        modelId: "opus",
        breakdown: model(50, 1, [
          { speed: "fast", source: "reported", tokens: 10, messages: 1 },
          { speed: "standard", source: "selected", tokens: 5, messages: 0 },
        ]),
      },
      { client: "claude", modelId: "sonnet", breakdown: model(30, 1) },
    ]);
    expect(Object.keys(aggregated.claude.models).sort()).toEqual(["opus", "sonnet"]);
    expect(aggregated.claude.tokens).toBe(180);
    expect(aggregated.claude.models.opus.tokens).toBe(150);
    expect(aggregated.claude.models.opus.speedUsage).toEqual([
      { speed: "standard", source: "selected", tokens: 5, messages: 0 },
      { speed: "fast", source: "reported", tokens: 50, messages: 2 },
    ]);
    expect("speedUsage" in aggregated.claude.models.sonnet).toBe(false);
    // Inputs are not mutated.
    expect(first).toEqual([{ speed: "fast", source: "reported", tokens: 40, messages: 1 }]);
  });

  test("legacy kilocode/kilo alias folding merges speed metadata", () => {
    const client = (m: ReturnType<typeof model>): ClientBreakdownData => ({
      ...model(m.tokens, m.messages),
      models: { k2: m },
    });
    const normalized = normalizeClientBreakdownAliases({
      kilocode: client(model(10, 1, [{ speed: "fast", source: "selected", tokens: 4, messages: 1 }])),
      kilo: client(model(20, 1, [{ speed: "fast", source: "selected", tokens: 6, messages: 1 }])),
    });
    expect(Object.keys(normalized)).toEqual(["kilo"]);
    expect(normalized.kilo.tokens).toBe(30);
    expect(normalized.kilo.models.k2.speedUsage).toEqual([
      { speed: "fast", source: "selected", tokens: 10, messages: 2 },
    ]);
  });

  test("legacy resubmits add usage without erasing speed; explicit empty clears it", () => {
    const stored: Record<string, ClientBreakdownData> = {
      claude: {
        ...model(100, 2),
        models: { opus: model(100, 2, [{ speed: "fast", source: "reported", tokens: 100, messages: 2 }]) },
        provenance: { schemaVersion: 1, messageCount: 2, modelCount: 1 },
      },
    };
    const incoming = aggregateIncomingClientBreakdowns([
      {
        client: "claude",
        modelId: "opus",
        breakdown: model(120, 3),
        provenance: { schemaVersion: 1, messageCount: 3, modelCount: 1 },
      },
    ]);

    const merged = mergeClientBreakdownsWithRegressionGuard(stored, incoming, new Set(["claude"]));
    expect(merged.warnings).toEqual([]);
    expect(merged.merged.claude.tokens).toBe(120);
    expect(merged.merged.claude.models.opus.speedUsage).toEqual([
      { speed: "fast", source: "reported", tokens: 100, messages: 2 },
    ]);
    expect(incoming.claude.models.opus.speedUsage).toBeUndefined();

    const reclassified = aggregateIncomingClientBreakdowns([{
      client: "claude", modelId: "opus",
      breakdown: clientContributionToBreakdownData({
        modelId: "opus", tokens: { input: 120, output: 0, cacheRead: 0, cacheWrite: 0 },
        cost: 1, messages: 3, speedUsage: [],
      }),
    }]);
    const cleared = mergeClientBreakdownsWithRegressionGuard(stored, reclassified, new Set(["claude"]));
    expect(cleared.merged.claude.models.opus.speedUsage).toEqual([]);
  });
});
