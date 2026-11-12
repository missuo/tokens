/**
 * The locale registry, and the one rule everything else follows from: a URL
 * either carries a locale prefix or it is English.
 *
 * The deployment caches HTML at Cloudflare's edge keyed on the full URL alone.
 * Any scheme where one URL renders different bytes depending on a cookie or an
 * `Accept-Language` header puts those variants in a fight over a single cache
 * entry — the first request to land fills it and everyone else is served the
 * wrong language until it expires. Prefixes move that ambiguity into the URL,
 * so `/zh/leaderboard` and `/leaderboard` are simply two documents and the
 * cache never has to guess.
 *
 * English stays unprefixed because it is the default: every existing link
 * (README, the iOS app, badges and embeds, anything anyone pasted into Slack)
 * keeps pointing at a page that renders English, and nothing needed a redirect
 * to stay working.
 */
export const LOCALES = ["en", "zh-CN", "ja"] as const;

export type Locale = (typeof LOCALES)[number];

/** The locale with no prefix, and the fallback for every unmatched request. */
export const DEFAULT_LOCALE: Locale = "en";

/**
 * Remembers the choice only to preselect the switcher and to answer `/`, where
 * there is no page of its own to read a prefix from. It never decides what an
 * unprefixed page renders — that is always English, for the cache reason above.
 */
export const LOCALE_COOKIE = "tt_locale";

/**
 * The request header the middleware stamps with the locale it resolved.
 *
 * Lives here rather than beside `getLocale()` because the middleware imports
 * it too, and `next/headers` is not loadable in that runtime — importing the
 * constant from `server.ts` would drag the module in and fail the Worker
 * bundle.
 */
export const LOCALE_HEADER = "x-tokens-locale";

/** Long enough to outlive a session, short enough to be a choice rather than a scar. */
export const LOCALE_COOKIE_MAX_AGE = 60 * 60 * 24 * 365;

/**
 * Endonyms, not English exonyms: a reader who cannot read "Japanese" still
 * recognises 日本語, and the switcher is the one place they are guaranteed to
 * be looking at before any translation has loaded.
 */
export const LOCALE_LABELS: Record<Locale, string> = {
  en: "English",
  "zh-CN": "简体中文",
  ja: "日本語",
};

/**
 * `lang` is what goes on `<html>` — the BCP 47 tag browsers, hyphenation and
 * screen readers actually key on. `dir` is here rather than derived ad hoc so
 * a future RTL locale is a one-line change instead of an audit.
 */
export const LOCALE_TAGS: Record<Locale, { lang: string; dir: "ltr" | "rtl" }> = {
  en: { lang: "en", dir: "ltr" },
  "zh-CN": { lang: "zh-CN", dir: "ltr" },
  ja: { lang: "ja", dir: "ltr" },
};

export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && (LOCALES as readonly string[]).includes(value);
}

/**
 * Split a prefixed pathname into its locale and the path the app routes on.
 *
 * `/zh` alone yields `{ locale: "zh-CN", path: "/" }` so callers can treat the
 * bare prefix uniformly, but the middleware redirects it to `/zh/leaderboard`
 * rather than rewriting it — a page whose canonical URL is `/zh` and whose
 * content is the leaderboard is a second name for the same document.
 *
 * Only a whole path segment counts: `/zhcn` is not Chinese.
 */
export function stripLocale(pathname: string): { locale: Locale; path: string } {
  for (const locale of LOCALES) {
    if (locale === DEFAULT_LOCALE) continue;
    if (pathname === `/${locale}`) return { locale, path: "/" };
    if (pathname.startsWith(`/${locale}/`)) {
      return { locale, path: pathname.slice(locale.length + 1) };
    }
  }
  return { locale: DEFAULT_LOCALE, path: pathname };
}

/**
 * The address a link should point at for a locale.
 *
 * Hashes and query strings are carried across untouched, and English is the
 * identity function so the default-language HTML is byte-identical in its
 * links to what it was before i18n existed.
 */
export function localizeHref(href: string, locale: Locale): string {
  if (locale === DEFAULT_LOCALE) return href;

  // External, protocol-relative and anchor-only links already point where they
  // point; prefixing them would corrupt the URL.
  if (/^(?:[a-z][a-z0-9+.-]*:|\/\/|#)/i.test(href)) return href;

  const match = /^([^?#]*)(.*)$/.exec(href);
  const path = match?.[1] ?? href;
  const suffix = match?.[2] ?? "";

  return `${path === "/" ? `/${locale}` : `/${locale}${path}`}${suffix}`;
}

/**
 * Pick a locale out of an `Accept-Language` header.
 *
 * Deliberately conservative. It only reaches here for `/` — no other request
 * has to guess — so the cost of a wrong answer is a visitor on the wrong board
 * rather than English content under a Chinese URL. Tags are matched by
 * prefix, so `zh-Hant-TW` and `zh-CN` both land on Simplified; q-values are
 * honoured, `*` is skipped, and anything under `en` stays unprefixed.
 */
export function negotiateLocale(acceptLanguage: string | null): Locale | null {
  if (!acceptLanguage) return null;

  const ranked = acceptLanguage
    .split(",")
    .map((part) => {
      const [tag, ...params] = part.trim().split(";");
      const q = params
        .map((p) => /^\s*q\s*=\s*([0-9.]+)\s*$/i.exec(p))
        .find(Boolean)?.[1];
      return { tag: tag.trim().toLowerCase(), q: q === undefined ? 1 : Number(q) };
    })
    .filter((entry) => entry.tag && entry.tag !== "*" && !Number.isNaN(entry.q) && entry.q > 0)
    .sort((a, b) => b.q - a.q);

  for (const { tag } of ranked) {
    if (tag === "zh" || tag.startsWith("zh-")) return "zh-CN";
    if (tag === "ja" || tag.startsWith("ja-")) return "ja";
    if (tag === "en" || tag.startsWith("en-")) return "en";
  }

  return null;
}
