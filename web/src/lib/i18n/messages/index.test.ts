import { describe, expect, test } from "bun:test";
import { resolveMessages } from "./index";
import { createTranslator } from "../translate";

describe("resolveMessages", () => {
  test("translates the leaves it has", () => {
    const t = createTranslator("zh-CN", resolveMessages("zh-CN"));
    expect(t("nav.leaderboard")).toBe("排行榜");
  });

  test("keeps English for keys the translation has not reached", () => {
    // `common.copied` exists in a namespace where the merge must not collapse
    // the whole object just because one leaf is absent.
    const messages = resolveMessages("ja");
    expect(messages.nav.leaderboard).toBe("リーダーボード");
    expect(messages.common.backToLeaderboard).toBe("リーダーボードへ戻る");
  });

  test("leaves English untouched for the default locale", () => {
    expect(resolveMessages("en").nav.leaderboard).toBe("Leaderboard");
  });

  test("does not mutate the source catalogue between calls", () => {
    resolveMessages("zh-CN");
    expect(resolveMessages("en").nav.leaderboard).toBe("Leaderboard");
  });
});
