import { describe, expect, it } from "vitest";

import {
  resolveLeaderboardTokenFormat,
  resolveSortByParam,
} from "@/lib/leaderboard/constants";

describe("resolveSortByParam", () => {
  it("keeps absent values available for persisted preference fallback", () => {
    expect(resolveSortByParam(null)).toBeNull();
    expect(resolveSortByParam(undefined)).toBeNull();
  });

  it("preserves supported explicit sort values", () => {
    expect(resolveSortByParam("tokens")).toBe("tokens");
    expect(resolveSortByParam("cost")).toBe("cost");
  });

  it("maps retired and unknown explicit values to tokens", () => {
    expect(resolveSortByParam("time")).toBe("tokens");
    expect(resolveSortByParam("unknown")).toBe("tokens");
    expect(resolveSortByParam("")).toBe("tokens");
  });
});

describe("resolveLeaderboardTokenFormat", () => {
  it("preserves supported display formats", () => {
    expect(resolveLeaderboardTokenFormat("full")).toBe("full");
    expect(resolveLeaderboardTokenFormat("compact")).toBe("compact");
  });

  it("defaults missing or invalid stored preferences to full values", () => {
    expect(resolveLeaderboardTokenFormat(undefined)).toBe("full");
    expect(resolveLeaderboardTokenFormat(null)).toBe("full");
    expect(resolveLeaderboardTokenFormat("abbreviated")).toBe("full");
  });
});
