/// <reference types="bun-types" />
import { describe, expect, test } from "bun:test";
import { maxSupportedParserRevision, validateSubmission } from "./submission";
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
