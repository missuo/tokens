import { describe, expect, it } from "vitest";
import {
  aggregateIncomingClientBreakdowns,
  mergeClientBreakdownsWithRegressionGuard,
} from "../../src/lib/db/helpers";

// Minimal client breakdown fixture
function makeClient(tokens: number, messages: number, modelCount: number) {
  const models: Record<string, { tokens: number; cost: number; input: number; output: number; cacheRead: number; cacheWrite: number; reasoning: number; messages: number }> = {};
  for (let i = 0; i < modelCount; i++) {
    models[`model-${i}`] = { tokens, cost: 0, input: tokens, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0, messages };
  }
  return {
    tokens,
    cost: 0,
    input: tokens,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    reasoning: 0,
    messages,
    models,
  };
}

function withRevision(
  client: ReturnType<typeof makeClient>,
  schemaVersion: number
) {
  return {
    ...client,
    provenance: {
      schemaVersion,
      messageCount: client.messages,
      modelCount: Object.keys(client.models).length,
    },
  };
}

describe("mergeClientBreakdownsWithRegressionGuard", () => {
  it("preserves existing when incoming has fewer tokens and equal coverage (A2 regression guard)", () => {
    // Before the A2 fix, equal coverage + fewer tokens would NOT be preserved
    // because the guard required BOTH fewer tokens AND lower coverage.
    const existing = { codex: makeClient(1000, 5, 2) };
    // Same message count and model count, but fewer tokens — signals a parse regression
    const incoming = { codex: makeClient(800, 5, 2) };

    const result = mergeClientBreakdownsWithRegressionGuard(
      existing,
      incoming,
      new Set(["codex"])
    );

    expect(result.merged.codex.tokens).toBe(1000);
    expect(result.warnings).toHaveLength(1);
    expect(result.warnings[0]).toContain("1,000");
    expect(result.warnings[0]).toContain("800");
  });

  it("preserves existing when incoming has fewer tokens and lower coverage", () => {
    const existing = { codex: makeClient(1000, 5, 2) };
    const incoming = { codex: makeClient(800, 3, 1) };

    const result = mergeClientBreakdownsWithRegressionGuard(
      existing,
      incoming,
      new Set(["codex"])
    );

    expect(result.merged.codex.tokens).toBe(1000);
    expect(result.warnings).toHaveLength(1);
  });

  it("accepts incoming when it has more tokens than existing", () => {
    const existing = { codex: makeClient(800, 5, 2) };
    const incoming = { codex: makeClient(1000, 5, 2) };

    const result = mergeClientBreakdownsWithRegressionGuard(
      existing,
      incoming,
      new Set(["codex"])
    );

    expect(result.merged.codex.tokens).toBe(1000);
    expect(result.warnings).toHaveLength(0);
  });

  it("accepts incoming when tokens are equal", () => {
    const existing = { codex: makeClient(1000, 5, 2) };
    const incoming = { codex: makeClient(1000, 5, 2) };

    const result = mergeClientBreakdownsWithRegressionGuard(
      existing,
      incoming,
      new Set(["codex"])
    );

    expect(result.merged.codex.tokens).toBe(1000);
    expect(result.warnings).toHaveLength(0);
  });

  it("preserves existing client that disappeared from incoming resubmit", () => {
    const existing = { codex: makeClient(1000, 5, 2), cursor: makeClient(500, 3, 1) };
    const incoming = { codex: makeClient(1200, 6, 2) };

    const result = mergeClientBreakdownsWithRegressionGuard(
      existing,
      incoming,
      new Set(["codex", "cursor"])
    );

    // codex is updated (more tokens)
    expect(result.merged.codex.tokens).toBe(1200);
    // cursor is preserved (disappeared from incoming but had tokens)
    expect(result.merged.cursor.tokens).toBe(500);
    expect(result.warnings).toHaveLength(1);
    expect(result.warnings[0]).toContain("cursor");
  });

  it("accepts a lower corrected value from a newer parser revision", () => {
    const existing = { codex: withRevision(makeClient(14_000, 80, 2), 1) };
    const incoming = { codex: withRevision(makeClient(950, 12, 2), 2) };

    const result = mergeClientBreakdownsWithRegressionGuard(
      existing,
      incoming,
      new Set(["codex"])
    );

    expect(result.merged.codex.tokens).toBe(950);
    expect(result.merged.codex.provenance?.schemaVersion).toBe(2);
    expect(result.warnings).toEqual([]);
  });

  it("rejects an older parser revision even when it reports more tokens", () => {
    const existing = { codex: withRevision(makeClient(950, 12, 2), 2) };
    const incoming = { codex: withRevision(makeClient(14_000, 80, 2), 1) };

    const result = mergeClientBreakdownsWithRegressionGuard(
      existing,
      incoming,
      new Set(["codex"])
    );

    expect(result.merged.codex.tokens).toBe(950);
    expect(result.merged.codex.provenance?.schemaVersion).toBe(2);
    expect(result.warnings).toHaveLength(1);
    expect(result.warnings[0]).toContain("revision 1");
    expect(result.warnings[0]).toContain("revision 2");
  });

  it("keeps the same-revision token regression guard", () => {
    const existing = { codex: withRevision(makeClient(1_000, 12, 2), 2) };
    const incoming = { codex: withRevision(makeClient(900, 12, 2), 2) };

    const result = mergeClientBreakdownsWithRegressionGuard(
      existing,
      incoming,
      new Set(["codex"])
    );

    expect(result.merged.codex.tokens).toBe(1_000);
    expect(result.merged.codex.provenance?.schemaVersion).toBe(2);
    expect(result.warnings).toHaveLength(1);
  });
});

describe("aggregateIncomingClientBreakdowns", () => {
  it("aggregates multiple models while preserving the incoming parser revision", () => {
    const result = aggregateIncomingClientBreakdowns([
      {
        client: "codex",
        modelId: "gpt-5.5",
        breakdown: makeClient(600, 7, 1).models["model-0"],
        provenance: { schemaVersion: 2, messageCount: 7, modelCount: 1 },
      },
      {
        client: "codex",
        modelId: "gpt-5.5-mini",
        breakdown: makeClient(350, 5, 1).models["model-0"],
        provenance: { schemaVersion: 2, messageCount: 5, modelCount: 1 },
      },
    ]);

    expect(result.codex.tokens).toBe(950);
    expect(result.codex.messages).toBe(12);
    expect(Object.keys(result.codex.models)).toEqual(["gpt-5.5", "gpt-5.5-mini"]);
    expect(result.codex.provenance).toEqual({
      schemaVersion: 2,
      messageCount: 12,
      modelCount: 2,
    });
  });
});
