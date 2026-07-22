import { describe, expect, it } from "vitest";
import type { UserEmbedStats, EmbedTodayUsage } from "../../src/lib/embed/getUserEmbedStats";
import { renderDetailedEmbedSvg } from "../../src/lib/embed/renderDetailedEmbedSvg";
import { clientLogoSvg, getEmbedClientLogo } from "../../src/lib/embed/clientLogoSvg";
import { THEMES } from "../../src/lib/embed/embedShared";

const mockStats: UserEmbedStats = {
  user: {
    id: "user-id",
    username: "octocat",
    displayName: "The Octocat",
    avatarUrl: null,
  },
  stats: {
    totalTokens: 1234567,
    totalCost: 42.42,
    submissionCount: 7,
    rank: 3,
    hasBackfill: false,
    rankTotal: 80,
    updatedAt: "2026-02-24T00:00:00.000Z",
  },
};

function mockModel(modelId: string) {
  return {
    modelId,
    tokens: 1000,
    cost: 1.5,
    messages: 3,
    input: 500,
    output: 300,
    cacheRead: 150,
    cacheWrite: 50,
    reasoning: 0,
  };
}

function mockClient(source: string): EmbedTodayUsage["clients"][number] {
  return {
    source,
    tokens: 1000,
    cost: 1.5,
    messages: 3,
    models: [mockModel(`${source}-model`)],
  };
}

const mockToday: EmbedTodayUsage = {
  date: "2026-02-24",
  tokens: 3000,
  cost: 4.5,
  clients: [mockClient("claude"), mockClient("grok"), mockClient("unknown-cli")],
};

describe("client logo inlining", () => {
  it("resolves lobehub icons for known clients", () => {
    for (const source of ["claude", "codex", "grok", "gemini", "cursor"]) {
      expect(getEmbedClientLogo(source), source).not.toBeNull();
    }
    expect(getEmbedClientLogo("unknown-cli")).toBeNull();
  });

  it("maps cc-mirror variants to the Claude mark", () => {
    expect(getEmbedClientLogo("cc-mirror/foo")).toEqual(
      getEmbedClientLogo("claude"),
    );
  });

  it("fills monochrome icons with the caller color", () => {
    const svg = clientLogoSvg("grok", {
      x: 0,
      y: 0,
      size: 14,
      monoColor: "#ABCDEF",
    });
    expect(svg).toContain('fill="#ABCDEF"');
    expect(svg).not.toContain("currentColor");
  });

  it("keeps brand fills on color icons", () => {
    const svg = clientLogoSvg("claude", {
      x: 0,
      y: 0,
      size: 14,
      monoColor: "#ABCDEF",
    });
    expect(svg).toContain('fill="#D97757"');
    expect(svg).not.toContain("#ABCDEF");
  });
});

describe("renderDetailedEmbedSvg client logos", () => {
  it("renders inline logos for known clients and a dot fallback otherwise", () => {
    const svg = renderDetailedEmbedSvg(mockStats, { today: mockToday });

    // Claude keeps its brand color; Grok is monochrome and themed.
    expect(svg).toContain('fill="#D97757"');
    expect(svg).toContain(`fill="${THEMES.dark.text}" fill-rule="evenodd"`);
    // The unknown client falls back to the colored dot.
    expect(svg.match(/<circle[^>]*r="5"/g)).toHaveLength(1);
    // No external image references — embeds must stay self-contained.
    expect(svg).not.toContain("<image");
  });
});
