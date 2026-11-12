/// <reference types="bun-types" />
import { describe, expect, test } from "bun:test";
import {
  maxSupportedParserRevision,
  optedInMcpServers,
  validateSubmission,
} from "./submission";
import {
  mergeClientBreakdownsWithRegressionGuard,
  type ClientBreakdownData,
} from "../db/helpers";

// One client, one model, one day; totals are derived so only the revision
// under test can make the payload invalid.
function submissionAt(client: string, revision: number, input = 1000) {
  const tokens = { input, output: 100, cacheRead: 0, cacheWrite: 0, reasoning: 0 };
  const total = input + 100;
  const range = { start: "2026-10-01", end: "2026-10-01" };
  return {
    meta: { generatedAt: "2026-10-02T00:00:00Z", version: "1.0.0", dateRange: range },
    device: { id: "device-1" },
    clientManifest: {
      schemaVersion: 1,
      clients: [{ client, parserRevision: revision }],
    },
    summary: {
      totalTokens: total,
      totalCost: 1,
      totalDays: 1,
      activeDays: 1,
      averagePerDay: 1,
      maxCostInSingleDay: 1,
      clients: [client],
      models: ["gpt-5"],
    },
    years: [{ year: "2026", totalTokens: total, totalCost: 1, range }],
    contributions: [
      {
        date: "2026-10-01",
        totals: { tokens: total, cost: 1, messages: 2 },
        intensity: 1,
        tokenBreakdown: tokens,
        clients: [
          {
            client,
            modelId: "gpt-5",
            tokens,
            cost: 1,
            messages: 2,
            provenance: { schemaVersion: revision, messageCount: 2, modelCount: 1 },
          },
        ],
      },
    ],
  };
}

function breakdownAt(revision: number, tokens: number): ClientBreakdownData {
  return {
    tokens,
    cost: 1,
    input: tokens,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    reasoning: 0,
    messages: 2,
    models: {},
    provenance: { schemaVersion: revision, messageCount: 2, modelCount: 1 },
  };
}

describe("per-client parser revision caps", () => {
  test("caps are pinned per client", () => {
    expect(maxSupportedParserRevision("codex")).toBe(4);
    // missuo/tokens#109: Devin CLI rows sharing a request_id count once.
    expect(maxSupportedParserRevision("devin-cli")).toBe(2);
    expect(maxSupportedParserRevision("devin-desktop")).toBe(1);
    expect(maxSupportedParserRevision("claude")).toBe(1);
    // Prototype keys are not clients with a raised cap.
    expect(maxSupportedParserRevision("constructor")).toBe(1);
  });

  for (const [client, cap] of [
    ["codex", 4],
    ["devin-cli", 2],
    ["claude", 1],
  ] as const) {
    test(`${client} accepts revision ${cap} and rejects ${cap + 1}`, () => {
      expect(validateSubmission(submissionAt(client, cap)).errors).toEqual([]);
      const rejected = validateSubmission(submissionAt(client, cap + 1)).errors.join("\n");
      expect(rejected).toContain(`${client} provenance schemaVersion must be at most ${cap}`);
      expect(rejected).toContain(`${client} parserRevision must be at most ${cap}`);
    });
  }

  test("devin-cli revision 2 replaces a higher revision-1 total; revision 1 cannot", () => {
    const existing = { "devin-cli": breakdownAt(1, 960_000_000) };
    const clients = new Set(["devin-cli"]);

    const sameRevision = mergeClientBreakdownsWithRegressionGuard(
      existing,
      { "devin-cli": breakdownAt(1, 196_000_000) },
      clients
    );
    expect(sameRevision.merged["devin-cli"].tokens).toBe(960_000_000);

    const corrected = mergeClientBreakdownsWithRegressionGuard(
      existing,
      { "devin-cli": breakdownAt(2, 196_000_000) },
      clients
    );
    expect(corrected.merged["devin-cli"].tokens).toBe(196_000_000);
    expect(corrected.merged["devin-cli"].provenance?.schemaVersion).toBe(2);
  });
});

function submission(extra: Record<string, unknown> = {}) {
  const tokens = { input: 600, output: 400, cacheRead: 0, cacheWrite: 0, reasoning: 0 };
  return {
    meta: {
      generatedAt: "2026-10-13T00:00:00.000Z",
      version: "1.0.0",
      dateRange: { start: "2026-10-12", end: "2026-10-12" },
    },
    device: { id: "dev_laptop", name: "laptop" },
    summary: {
      totalTokens: 1000,
      totalCost: 0.5,
      totalDays: 1,
      activeDays: 1,
      averagePerDay: 1000,
      maxCostInSingleDay: 0.5,
      clients: ["codex"],
      models: ["gpt-5"],
    },
    years: [],
    contributions: [
      {
        date: "2026-10-12",
        totals: { tokens: 1000, cost: 0.5, messages: 1 },
        intensity: 1,
        tokenBreakdown: tokens,
        clients: [{ client: "codex", modelId: "gpt-5", tokens, cost: 0.5, messages: 1 }],
      },
    ],
    ...extra,
  };
}

describe("MCP server names opt-in (#121)", () => {
  test("an old CLI's payload still validates, but its names are dropped", () => {
    const payload = submission({ mcpServers: ["example-internal-service"] });
    expect(validateSubmission(payload).valid).toBe(true);
    expect(optedInMcpServers(payload)).toBeNull();
  });

  test("a payload without MCP fields validates and stores no names", () => {
    const payload = submission();
    expect(validateSubmission(payload).valid).toBe(true);
    expect(optedInMcpServers(payload)).toBeNull();
  });

  test("names are kept only with mcpServersOptIn: true", () => {
    const payload = submission({
      mcpServersOptIn: true,
      mcpServers: ["github", "", 7, "slack"],
    });
    expect(validateSubmission(payload).valid).toBe(true);
    expect(optedInMcpServers(payload)).toEqual(["github", "slack"]);
  });

  test("an explicit opt-out or an empty list stores no names", () => {
    expect(
      optedInMcpServers(submission({ mcpServersOptIn: false, mcpServers: ["github"] })),
    ).toBeNull();
    expect(optedInMcpServers(submission({ mcpServersOptIn: true, mcpServers: [] }))).toBeNull();
    expect(optedInMcpServers(submission({ mcpServersOptIn: true }))).toBeNull();
    expect(optedInMcpServers(null)).toBeNull();
  });

  test("a non-boolean opt-in signal is rejected", () => {
    expect(validateSubmission(submission({ mcpServersOptIn: "yes" })).valid).toBe(false);
  });
});
