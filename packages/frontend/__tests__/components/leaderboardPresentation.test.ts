import { describe, expect, it } from "vitest";
import {
  getLeaderboardPeriodLabel,
} from "@/components/leaderboard/presentation";

describe("leaderboard presentation helpers", () => {
  it("describes predefined and custom leaderboard periods", () => {
    expect(getLeaderboardPeriodLabel("all")).toBe("All time");
    expect(getLeaderboardPeriodLabel("last-month")).toBe("Last month");
    expect(getLeaderboardPeriodLabel("month")).toBe("This month");
    expect(getLeaderboardPeriodLabel("week")).toBe("This week");
    expect(
      getLeaderboardPeriodLabel("custom", "2026-06-01", "2026-06-30"),
    ).toBe("Jun 1–30, 2026");
    expect(getLeaderboardPeriodLabel("custom")).toBe("Custom range");
  });
});
