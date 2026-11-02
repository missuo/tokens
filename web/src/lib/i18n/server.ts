import { headers } from "next/headers";
import { DEFAULT_LOCALE, LOCALE_HEADER, isLocale, type Locale } from "./config";
import { resolveMessages } from "./messages";
import { createTranslator, type Translator } from "./translate";

/**
 * Server components cannot see the address bar, and the middleware has already
 * stripped the prefix by the time a page renders, so `LOCALE_HEADER` is the
 * only honest answer to "what language is this document". The middleware sets
 * it on every request the matcher covers, including the unprefixed ones where
 * the answer is English — so its absence means a route outside the matcher,
 * and the default is right.
 */
export { LOCALE_HEADER };

export async function getLocale(): Promise<Locale> {
  const value = (await headers()).get(LOCALE_HEADER);
  return isLocale(value) ? value : DEFAULT_LOCALE;
}

/**
 * Server-side translator for one render.
 *
 * The catalogue is resolved per call rather than cached across requests:
 * `headers()` already opts the render into per-request work, and the merge is
 * a few dozen object spreads against a ~600-key catalogue.
 */
export async function getTranslator(): Promise<{
  locale: Locale;
  t: Translator;
}> {
  const locale = await getLocale();
  return { locale, t: createTranslator(locale, resolveMessages(locale)) };
}
