import { describe, it, expect } from "vitest";
import { formatRelativeTime } from "@/lib/utils";

describe("formatRelativeTime", () => {
  const now = new Date("2026-06-18T12:00:00.000Z");

  it("returns 'never' for null/undefined/invalid input", () => {
    expect(formatRelativeTime(null, now)).toBe("never");
    expect(formatRelativeTime(undefined, now)).toBe("never");
    expect(formatRelativeTime("not-a-date", now)).toBe("never");
  });

  it("clamps sub-minute and future timestamps to 'just now'", () => {
    expect(formatRelativeTime("2026-06-18T11:59:30.000Z", now)).toBe("just now");
    expect(formatRelativeTime("2026-06-18T12:30:00.000Z", now)).toBe("just now");
  });

  it("formats minutes, hours, and days", () => {
    expect(formatRelativeTime("2026-06-18T11:55:00.000Z", now)).toBe("5m ago");
    expect(formatRelativeTime("2026-06-18T09:00:00.000Z", now)).toBe("3h ago");
    expect(formatRelativeTime("2026-06-06T12:00:00.000Z", now)).toBe("12d ago");
  });

  it("formats months and years", () => {
    expect(formatRelativeTime("2026-04-18T12:00:00.000Z", now)).toBe("2mo ago");
    expect(formatRelativeTime("2025-06-18T12:00:00.000Z", now)).toBe("1y ago");
  });
});
