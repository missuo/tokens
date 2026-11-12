import { describe, expect, test } from "bun:test";
import { formatMessage } from "./format";

describe("formatMessage", () => {
  test("substitutes named placeholders", () => {
    expect(formatMessage("{count} items", { count: 3 }, "en")).toBe("3 items");
  });

  test("picks the CLDR branch for the locale", () => {
    const message = "{n, plural, one {# file} other {# files}}";

    expect(formatMessage(message, { n: 1 }, "en")).toBe("1 file");
    expect(formatMessage(message, { n: 2 }, "en")).toBe("2 files");
    // Japanese has no plural distinction: the single `other` branch is right
    // for every count, including 1, so an English message reads "1 files" under
    // `ja` until the translation collapses the branches.
    expect(formatMessage(message, { n: 1 }, "ja")).toBe("1 files");
    expect(formatMessage(message, { n: 2 }, "ja")).toBe("2 files");
  });

  test("prefers an exact match over a category", () => {
    const message = "{n, plural, =0 {nothing here} one {# item} other {# items}}";
    expect(formatMessage(message, { n: 0 }, "en")).toBe("nothing here");
  });

  test("falls back to `other` when the locale has an uncovered category", () => {
    // Russian `few` is not written, so the message still renders.
    const message = "{n, plural, one {# файл} other {# файлов}}";
    expect(formatMessage(message, { n: 3 }, "en")).toBe("3 файлов");
  });

  test("localizes the number the `#` stands for", () => {
    expect(formatMessage("{n, plural, other {# 個}}", { n: 1234 }, "ja")).toBe(
      "1,234 個",
    );
  });

  test("leaves an unknown placeholder visible", () => {
    expect(formatMessage("{missing} items", {}, "en")).toBe("{missing} items");
  });

  test("returns the message untouched without vars", () => {
    expect(formatMessage("{count} items", undefined, "en")).toBe("{count} items");
  });
});
