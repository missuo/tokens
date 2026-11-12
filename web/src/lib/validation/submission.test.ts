import { describe, expect, test } from "bun:test";
import { optedInMcpServers, validateSubmission } from "./submission";

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
