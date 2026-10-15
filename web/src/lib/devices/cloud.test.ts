/// <reference types="bun-types" />
import { describe, expect, test } from "bun:test";
import { groupCloudDevices, parseCloudDeviceKey } from "./cloud";
import { deviceDisplayLabel, publicDeviceDisplayLabel } from "./shared";
import { validateSubmission } from "../validation/submission";

const CLAUDE_KEY = "cloud_claude_f4a8a3a17173af3e2df865616af63456";
const CLAUDE_KEY_2 = "cloud_claude_0123456789abcdef0123456789abcdef";
const CODEX_KEY = "cloud_codex_0123456789abcdef0123456789abcdef";

describe("parseCloudDeviceKey", () => {
  test("recognises the keys the CLI generates", () => {
    expect(parseCloudDeviceKey(CLAUDE_KEY)).toEqual({
      provider: "claude",
      client: "claude",
      label: "Claude Code cloud",
    });
    expect(parseCloudDeviceKey(CODEX_KEY)?.client).toBe("codex");
  });

  test("rejects malformed and unknown-provider keys", () => {
    expect(parseCloudDeviceKey("dev_abc")).toBeNull();
    expect(parseCloudDeviceKey("cloud_claude_xyz")).toBeNull();
    expect(parseCloudDeviceKey("cloud_claude_F4A8A3A17173AF3E2DF865616AF63456")).toBeNull();
    expect(parseCloudDeviceKey("cloud_devin_0123456789abcdef0123456789abcdef")).toBeNull();
  });
});

describe("device labels", () => {
  test("a cloud device shows its fixed, unmasked label", () => {
    expect(deviceDisplayLabel(CLAUDE_KEY, null)).toBe("Claude Code cloud");
    expect(publicDeviceDisplayLabel(CLAUDE_KEY, null)).toBe("Claude Code cloud");
  });
});

describe("groupCloudDevices", () => {
  const base = { totalCost: 1, activeDays: 1, lastSubmittedAt: "2026-10-01T00:00:00.000Z" };

  test("folds each provider's sessions into one row and leaves machines alone", () => {
    const rows = groupCloudDevices([
      { ...base, id: "a", deviceKey: CLAUDE_KEY, displayName: "x", totalTokens: 100, lastSubmittedAt: "2026-10-03T00:00:00.000Z" },
      { ...base, id: "b", deviceKey: "dev_laptop", displayName: "la****op", totalTokens: 50 },
      { ...base, id: "c", deviceKey: CLAUDE_KEY_2, displayName: "y", totalTokens: 200, activeDays: 3 },
      { ...base, id: "d", deviceKey: CODEX_KEY, displayName: "z", totalTokens: 7 },
    ]);

    expect(rows.map((row) => row.id)).toEqual(["cloud:claude", "b", "cloud:codex"]);
    expect(rows[0]).toMatchObject({
      displayName: "Claude Code cloud",
      cloudSessions: 2,
      totalTokens: 300,
      totalCost: 2,
      activeDays: 3,
      lastSubmittedAt: "2026-10-03T00:00:00.000Z",
    });
    expect(rows[1].cloudSessions).toBeUndefined();
    expect(rows[2]).toMatchObject({ displayName: "Codex cloud", cloudSessions: 1 });
  });
});

function submission(deviceId: string, client: string, extra: Record<string, unknown> = {}) {
  const tokens = { input: 600, output: 400, cacheRead: 0, cacheWrite: 0, reasoning: 0 };
  return {
    meta: {
      generatedAt: "2026-10-13T00:00:00.000Z",
      version: "1.0.0",
      dateRange: { start: "2026-10-12", end: "2026-10-12" },
    },
    device: { id: deviceId, name: "Claude Code cloud" },
    summary: {
      totalTokens: 1000,
      totalCost: 0.5,
      totalDays: 1,
      activeDays: 1,
      averagePerDay: 1000,
      maxCostInSingleDay: 0.5,
      clients: [client],
      models: ["claude-sonnet-4-5"],
    },
    years: [],
    contributions: [
      {
        date: "2026-10-12",
        totals: { tokens: 1000, cost: 0.5, messages: 1 },
        intensity: 1,
        tokenBreakdown: tokens,
        clients: [
          { client, modelId: "claude-sonnet-4-5", tokens, cost: 0.5, messages: 1 },
        ],
      },
    ],
    ...extra,
  };
}

describe("cloud device submission rules", () => {
  test("a Claude Code cloud session may report claude usage", () => {
    expect(validateSubmission(submission(CLAUDE_KEY, "claude")).valid).toBe(true);
  });

  test("a cloud session may not report another client", () => {
    const result = validateSubmission(submission(CLAUDE_KEY, "cursor"));
    expect(result.valid).toBe(false);
    expect(result.errors.join("\n")).toContain("may only report claude usage");
  });

  test("the reserved cloud_ namespace only accepts well-formed keys", () => {
    const result = validateSubmission(submission("cloud_claude_fake", "claude"));
    expect(result.valid).toBe(false);
    expect(result.errors.join("\n")).toContain("cloud_<provider>_<32 hex>");
  });

  test("a cloud session may not submit imported history", () => {
    const result = validateSubmission(
      submission(CLAUDE_KEY, "claude", { provenance: { origin: "backfill" } })
    );
    expect(result.valid).toBe(false);
  });

  test("a cloud session may not declare replacement coverage", () => {
    const result = validateSubmission(
      submission(CLAUDE_KEY, "claude", {
        clientManifest: {
          schemaVersion: 1,
          clients: [
            {
              client: "claude",
              parserRevision: 1,
              coverage: { mode: "full", start: "2026-10-12", end: "2026-10-12", missingData: "tombstone" },
            },
          ],
        },
      })
    );
    expect(result.valid).toBe(false);
    expect(result.errors.join("\n")).toContain("replacement coverage");
  });

  test("ordinary devices are unaffected", () => {
    expect(validateSubmission(submission("dev_laptop", "cursor")).valid).toBe(true);
  });
});
