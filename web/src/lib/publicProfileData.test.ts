/// <reference types="bun-types" />
import { expect, mock, test } from "bun:test";
import * as schema from "./db/schema";

// Every db.select() chain resolves to the next queued result, in the order
// getPublicProfileResponse issues them: user, stats, latest submission,
// daily rows, archived count. db.execute() serves the rank query.
const selectResults: unknown[][] = [];
function chain(result: unknown[]) {
  const builder: Record<string, unknown> = {};
  for (const method of ["from", "where", "limit", "orderBy", "innerJoin"]) {
    builder[method] = () => builder;
  }
  builder.then = (resolve: (value: unknown) => unknown, reject: (e: unknown) => unknown) =>
    Promise.resolve(result).then(resolve, reject);
  return builder;
}
mock.module("@/lib/db", () => ({
  ...schema,
  db: {
    select: () => chain(selectResults.shift() ?? []),
    execute: async () => [{ rank: 1 }],
  },
}));

const { getPublicProfileResponse } = await import("./publicProfileData");

const model = (tokens: number, speedUsage?: unknown[]) => ({
  tokens,
  cost: tokens / 100,
  input: tokens,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  reasoning: 0,
  messages: 1,
  ...(speedUsage ? { speedUsage } : {}),
});
const claudeRow = (models: Record<string, ReturnType<typeof model>>) => {
  const tokens = Object.values(models).reduce((sum, m) => sum + m.tokens, 0);
  return {
    date: new Date().toISOString().slice(0, 10),
    timestampMs: null,
    tokens,
    cost: String(tokens / 100),
    inputTokens: tokens,
    outputTokens: 0,
    sourceBreakdown: {
      claude: { ...model(tokens), messages: Object.keys(models).length, models },
    },
  };
};

test("public profile preserves and merges nested model speedUsage across devices", async () => {
  selectResults.push(
    [{ id: "u1", username: "alice", displayName: null, avatarUrl: null, createdAt: new Date("2026-01-01"), bannedAt: null, banReason: null }],
    [{ totalTokens: 600, totalCost: 6 }],
    [{ sourcesUsed: ["claude"], modelsUsed: ["opus", "sonnet"], updatedAt: new Date("2026-10-02"), cliVersion: "1.0.0", schemaVersion: 2, mcpServers: null }],
    [
      // Device A
      claudeRow({
        opus: model(200, [{ speed: "fast", source: "reported", tokens: 150, messages: 1 }]),
        sonnet: model(100),
      }),
      // Device B, same day/client/model
      claudeRow({
        opus: model(300, [
          { speed: "fast", source: "reported", tokens: 50, messages: 1 },
          { speed: "standard", source: "selected", tokens: 25, messages: 0 },
        ]),
      }),
    ],
    [{ days: 0 }],
  );

  const response = await getPublicProfileResponse(
    new Request("http://tokens.internal/api/users/alice"),
    { params: Promise.resolve({ username: "alice" }) },
  );
  expect(response.status).toBe(200);
  const body = await response.json();

  const [day] = body.contributions;
  expect(day.clients).toHaveLength(1);
  const { models } = day.clients[0];
  expect(Object.keys(models).sort()).toEqual(["opus", "sonnet"]);
  expect(models.opus.tokens).toBe(500);
  expect(models.opus.speedUsage).toEqual([
    { speed: "standard", source: "selected", tokens: 25, messages: 0 },
    { speed: "fast", source: "reported", tokens: 200, messages: 2 },
  ]);
  expect(models.sonnet.speedUsage).toBeUndefined();

  // Model identities and totals are unaffected by speed metadata.
  expect(body.modelUsage.map((m: { model: string; tokens: number }) => [m.model, m.tokens])).toEqual([
    ["opus", 500],
    ["sonnet", 100],
  ]);
  expect(body.modelUsage[0].speedUsage).toEqual([
    { speed: "standard", source: "selected", tokens: 25, messages: 0 },
    { speed: "fast", source: "reported", tokens: 200, messages: 2 },
  ]);
  expect(body.modelUsage[1].speedUsage).toBeUndefined();
  expect(day.totals.tokens).toBe(600);
});

test("model speed totals merge clients and follow the same period boundaries as tokens and cost", async () => {
  for (const [period, tokens, reportedFast, ultrafast] of [
    ["week", 600, 150, 0],
    ["month", 1250, 350, 350],
    ["all", 1300, 390, 350],
  ] as const) {
    const row = (client: string, daysAgo: number, tokens: number, entries: unknown[]) => {
      const day = claudeRow({ opus: model(tokens, entries) });
      const date = new Date();
      date.setUTCDate(date.getUTCDate() - daysAgo);
      return {
        ...day,
        date: date.toISOString().slice(0, 10),
        sourceBreakdown: { [client]: day.sourceBreakdown.claude },
      };
    };
    selectResults.push(
      [{ id: "u1", username: "alice", bannedAt: null }],
      [{ totalTokens: 1300, totalCost: 13 }],
      [],
      [
        row("claude", 0, 200, [{ speed: "fast", source: "reported", tokens: 150, messages: 1 }]),
        row("cc-mirror/work", 0, 300, [
          { speed: "fast", source: "selected", tokens: 120, messages: 1 },
          { speed: "standard", source: "reported", tokens: 80, messages: 0 },
        ]),
        row("claude", 6, 100, [{ speed: "standard", source: "selected", tokens: 60, messages: 1 }]),
        row("claude", 7, 400, [{ speed: "ultrafast", source: "selected", tokens: 350, messages: 1 }]),
        row("claude", 29, 250, [{ speed: "fast", source: "reported", tokens: 200, messages: 0 }]),
        row("claude", 30, 50, [{ speed: "fast", source: "reported", tokens: 40, messages: 0 }]),
      ],
      [{ days: 0 }],
    );
    const response = await getPublicProfileResponse(
      new Request(`http://tokens.internal/api/users/alice?period=${period}`),
      { params: Promise.resolve({ username: "alice" }) },
    );
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.modelUsage).toEqual([{
      model: "opus",
      tokens,
      cost: tokens / 100,
      percentage: 100,
      speedUsage: [
        { speed: "standard", source: "reported", tokens: 80, messages: 0 },
        { speed: "standard", source: "selected", tokens: 60, messages: 1 },
        { speed: "fast", source: "reported", tokens: reportedFast, messages: 1 },
        { speed: "fast", source: "selected", tokens: 120, messages: 1 },
        ...(ultrafast ? [{ speed: "ultrafast", source: "selected", tokens: ultrafast, messages: 1 }] : []),
      ],
    }]);
  }
});
