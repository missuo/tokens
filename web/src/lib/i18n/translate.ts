import { formatMessage, type MessageVars } from "./format";
import type { Locale } from "./config";
import type { Messages } from "./messages";

/**
 * A dot path into the catalogue: `leaderboard.table.rank`.
 *
 * Kept as a plain `string` rather than a union of every literal path. A
 * generated union of ~500 keys makes every `t()` call site a wall of red while
 * a namespace is being written, and the win over `string` is a typo caught at
 * build time instead of on screen. The trade is made for velocity; a miss
 * renders the key plus a development warning, so it is still loud.
 */
export type TranslationKey = string;

export type Translator = (key: TranslationKey, vars?: MessageVars) => string;

export function createTranslator(
  locale: Locale,
  messages: Messages,
): Translator {
  return (key, vars) => {
    const message = readPath(messages, key);

    if (typeof message !== "string") {
      if (process.env.NODE_ENV !== "production") {
        console.warn(`[i18n] missing message "${key}" for "${locale}"`);
      }
      return key;
    }

    return formatMessage(message, vars, locale);
  };
}

function readPath(messages: Messages, key: string): unknown {
  let node: unknown = messages;
  for (const segment of key.split(".")) {
    if (node === null || typeof node !== "object") return undefined;
    node = (node as Record<string, unknown>)[segment];
  }
  return node;
}
