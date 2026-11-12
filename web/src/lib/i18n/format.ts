import type { Locale } from "./config";

/**
 * A small ICU subset, because this app needs exactly two things and a message
 * library would have brought a parser, a catalogue loader and a build step for
 * them.
 *
 * What is supported is what the copy actually uses: `{name}` substitution and
 * `{count, plural, ...}` with `=N` and CLDR categories, including `#`. What is
 * not supported is `select`, nested plural inside a branch of another plural,
 * and number reformatting inside `#`. If a future string needs one of those,
 * this is the file that grows — not a reason to swap the approach now.
 */

export type MessageVars = Record<string, string | number>;

/** `formatMessage` is typed against the message catalogue; anything else is a bug. */
export type Message = string;

const PLACEHOLDER = /\{(\w+)(?:,\s*plural\s*,\s*([^{}]*(?:\{[^{}]*\}[^{}]*)*))?\}/g;
const PLURAL_OPTION = /(=\d+|[a-zA-Z]+)\s*\{([^{}]*)\}/g;

/**
 * Format one message for a locale.
 *
 * A plural branch is chosen with `Intl.PluralRules`, so languages that need
 * categories English does not have (Russian's `few`, Arabic's six forms) work
 * by writing the branch, and `=N` still wins for exact cases like "no items".
 *
 * Unknown `{placeholders}` are left as-is rather than rendering `undefined`:
 * a visible `{count}` in a screenshot points at the missing variable, and a
 * silent `undefined` reads like a half-translated string.
 */
export function formatMessage(
  message: Message,
  vars: MessageVars | undefined,
  locale: Locale,
): string {
  if (!vars) return message;

  return message.replace(PLACEHOLDER, (whole, name: string, plural: string | undefined) => {
    const value = vars[name];
    if (value === undefined) return whole;

    if (plural === undefined) return String(value);

    const count = typeof value === "number" ? value : Number(value);
    const exact = `=${count}`;

    const options = new Map<string, string>();
    for (const [, key, text] of plural.matchAll(PLURAL_OPTION)) {
      options.set(key, text);
    }

    const branch =
      options.get(exact) ??
      options.get(new Intl.PluralRules(locale).select(count)) ??
      options.get("other") ??
      "";

    return branch.replace(/#/g, formatNumber(count, locale));
  });
}

/**
 * Numbers through `Intl`, never `toLocaleString()` with no argument: the
 * argument-free form follows the *server's* locale, which is how every
 * deployment ends up rendering American grouping for everyone.
 */
export function formatNumber(value: number, locale: Locale, options?: Intl.NumberFormatOptions) {
  return new Intl.NumberFormat(locale, options).format(value);
}

export function formatCompactNumber(value: number, locale: Locale) {
  return formatNumber(value, locale, { notation: "compact", maximumFractionDigits: 1 });
}

export function formatPercent(value: number, locale: Locale, fractionDigits = 0) {
  return formatNumber(value, locale, {
    style: "percent",
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  });
}

export function formatDate(
  value: Date | number,
  locale: Locale,
  options: Intl.DateTimeFormatOptions,
) {
  return new Intl.DateTimeFormat(locale, options).format(value);
}
