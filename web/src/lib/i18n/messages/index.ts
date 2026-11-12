import { DEFAULT_LOCALE, type Locale } from "../config";

import commonEn from "./common/en";
import navEn from "./nav/en";
import leaderboardEn from "./leaderboard/en";
import profileEn from "./profile/en";
import settingsEn from "./settings/en";
import metaEn from "./meta/en";
import footerEn from "./footer/en";
import errorsEn from "./errors/en";
import shameEn from "./shame/en";
import docsEn from "./docs/en";
import legalEn from "./legal/en";
import ogEn from "./og/en";

import commonZh from "./common/zh-CN";
import navZh from "./nav/zh-CN";
import leaderboardZh from "./leaderboard/zh-CN";
import profileZh from "./profile/zh-CN";
import settingsZh from "./settings/zh-CN";
import metaZh from "./meta/zh-CN";
import footerZh from "./footer/zh-CN";
import errorsZh from "./errors/zh-CN";
import shameZh from "./shame/zh-CN";
import docsZh from "./docs/zh-CN";
import legalZh from "./legal/zh-CN";
import ogZh from "./og/zh-CN";

import commonJa from "./common/ja";
import navJa from "./nav/ja";
import leaderboardJa from "./leaderboard/ja";
import profileJa from "./profile/ja";
import settingsJa from "./settings/ja";
import metaJa from "./meta/ja";
import footerJa from "./footer/ja";
import errorsJa from "./errors/ja";
import shameJa from "./shame/ja";
import docsJa from "./docs/ja";
import legalJa from "./legal/ja";
import ogJa from "./og/ja";

/**
 * English is the source catalogue. Every other locale is typed as a deep
 * partial of it, so a key that exists only in the translation is a type error
 * (a typo, or copy that was never wired up), while a key the translation has
 * not reached yet is legal and resolves through the fallback chain at runtime.
 *
 * That asymmetry is the point: translations arrive over weeks, and a missing
 * key should degrade to English rather than break the build for whoever is
 * mid-way through a namespace.
 */
export type Messages = {
  common: typeof commonEn;
  nav: typeof navEn;
  leaderboard: typeof leaderboardEn;
  profile: typeof profileEn;
  settings: typeof settingsEn;
  meta: typeof metaEn;
  footer: typeof footerEn;
  errors: typeof errorsEn;
  shame: typeof shameEn;
  docs: typeof docsEn;
  legal: typeof legalEn;
  og: typeof ogEn;
};

export type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends Record<string, unknown> ? DeepPartial<T[K]> : T[K];
};

const EN: Messages = {
  common: commonEn,
  nav: navEn,
  leaderboard: leaderboardEn,
  profile: profileEn,
  settings: settingsEn,
  meta: metaEn,
  footer: footerEn,
  errors: errorsEn,
  shame: shameEn,
  docs: docsEn,
  legal: legalEn,
  og: ogEn,
};

/**
 * Bare `{}` files type as `{}` and would satisfy `DeepPartial<Messages>` while
 * providing nothing, so locales with unused namespaces are filled in by merge
 * rather than trusted by shape.
 */
const PARTIALS: Record<Locale, DeepPartial<Messages>> = {
  en: EN,
  "zh-CN": {
    common: commonZh,
    nav: navZh,
    leaderboard: leaderboardZh,
    profile: profileZh,
    settings: settingsZh,
    meta: metaZh,
    footer: footerZh,
    errors: errorsZh,
    shame: shameZh,
    docs: docsZh,
    legal: legalZh,
    og: ogZh,
  },
  ja: {
    common: commonJa,
    nav: navJa,
    leaderboard: leaderboardJa,
    profile: profileJa,
    settings: settingsJa,
    meta: metaJa,
    footer: footerJa,
    errors: errorsJa,
    shame: shameJa,
    docs: docsJa,
    legal: legalJa,
    og: ogJa,
  },
};

function mergeDeep<T>(base: T, override: unknown): T {
  if (override === undefined) return base;
  // A leaf wins outright. Testing "override is not an object" against `base`
  // rather than returning base is what makes a translated string replace the
  // English one — the first version of this returned the base leaf and every
  // locale silently rendered English.
  if (override === null || typeof override !== "object") return override as T;
  if (base === null || typeof base !== "object") return override as T;

  const result: Record<string, unknown> = { ...(base as Record<string, unknown>) };
  for (const [key, value] of Object.entries(override as Record<string, unknown>)) {
    result[key] = mergeDeep(result[key], value);
  }
  return result as T;
}

/**
 * Resolve a catalogue for a locale, already merged over English.
 *
 * Merging once here rather than falling back per lookup keeps the lookup in
 * `translate` a plain property walk — translation happens on every render of
 * every row, and a miss should cost nothing at that point.
 */
export function resolveMessages(locale: Locale): Messages {
  if (locale === DEFAULT_LOCALE) return EN;
  return mergeDeep(EN, PARTIALS[locale]);
}
