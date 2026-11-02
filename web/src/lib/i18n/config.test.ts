import { describe, expect, test } from "bun:test";
import {
  DEFAULT_LOCALE,
  isLocale,
  localizeHref,
  negotiateLocale,
  stripLocale,
} from "./config";

describe("stripLocale", () => {
  test("splits a prefixed path and keeps the remainder", () => {
    expect(stripLocale("/zh-CN/leaderboard")).toEqual({
      locale: "zh-CN",
      path: "/leaderboard",
    });
    expect(stripLocale("/ja/u/someone")).toEqual({
      locale: "ja",
      path: "/u/someone",
    });
  });

  test("treats a bare prefix as the root", () => {
    expect(stripLocale("/zh-CN")).toEqual({ locale: "zh-CN", path: "/" });
  });

  test("leaves unprefixed paths in English", () => {
    expect(stripLocale("/leaderboard")).toEqual({
      locale: DEFAULT_LOCALE,
      path: "/leaderboard",
    });
    expect(stripLocale("/")).toEqual({ locale: DEFAULT_LOCALE, path: "/" });
  });

  test("only matches whole segments", () => {
    // `/zhcn` is not Chinese, and `/zhx/leaderboard` keeps its own name.
    expect(stripLocale("/zhcn").locale).toBe(DEFAULT_LOCALE);
    expect(stripLocale("/zhx/leaderboard").path).toBe("/zhx/leaderboard");
  });

  test("does not strip a path that merely starts with the locale", () => {
    expect(stripLocale("/zhcn/docs")).toEqual({
      locale: DEFAULT_LOCALE,
      path: "/zhcn/docs",
    });
  });
});

describe("localizeHref", () => {
  test("is the identity for English, so existing HTML is unchanged", () => {
    expect(localizeHref("/leaderboard", "en")).toBe("/leaderboard");
    expect(localizeHref("/u/x?period=week", "en")).toBe("/u/x?period=week");
  });

  test("prefixes other locales", () => {
    expect(localizeHref("/leaderboard", "zh-CN")).toBe("/zh-CN/leaderboard");
    expect(localizeHref("/", "ja")).toBe("/ja");
  });

  test("carries the query string and hash across", () => {
    expect(localizeHref("/leaderboard?period=week", "ja")).toBe(
      "/ja/leaderboard?period=week",
    );
    expect(localizeHref("/docs#install", "zh-CN")).toBe("/zh-CN/docs#install");
  });

  test("leaves external, protocol-relative and anchor links alone", () => {
    expect(localizeHref("https://neon.com", "zh-CN")).toBe("https://neon.com");
    expect(localizeHref("//cdn.example/x", "zh-CN")).toBe("//cdn.example/x");
    expect(localizeHref("#main-content", "zh-CN")).toBe("#main-content");
  });

  test("does not double-prefix a path that already carries one", () => {
    // Callers always pass an unprefixed path, but a prefixed one should not
    // silently grow a second segment.
    expect(localizeHref("/zh-CN/leaderboard", "ja")).toBe("/ja/zh-CN/leaderboard");
  });
});

describe("negotiateLocale", () => {
  test("matches Chinese before English by q-value", () => {
    expect(negotiateLocale("zh-CN,zh;q=0.9,en;q=0.8")).toBe("zh-CN");
  });

  test("matches any Chinese or Japanese tag by prefix", () => {
    expect(negotiateLocale("zh-Hant-TW")).toBe("zh-CN");
    expect(negotiateLocale("ja-JP")).toBe("ja");
  });

  test("honours q-values over header order", () => {
    expect(negotiateLocale("en;q=0.5,ja;q=0.9")).toBe("ja");
  });

  test("skips the wildcard and zero-quality entries", () => {
    expect(negotiateLocale("*")).toBeNull();
    expect(negotiateLocale("de;q=1, zh;q=0")).toBeNull();
  });

  test("returns null for an absent or unsupported header", () => {
    expect(negotiateLocale(null)).toBeNull();
    expect(negotiateLocale("")).toBeNull();
    expect(negotiateLocale("de-DE,fr;q=0.8")).toBeNull();
  });
});

describe("isLocale", () => {
  test("accepts registered locales only", () => {
    expect(isLocale("zh-CN")).toBe(true);
    expect(isLocale("en")).toBe(true);
    expect(isLocale("zh")).toBe(false);
    expect(isLocale(undefined)).toBe(false);
  });
});
