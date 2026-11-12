"use client";

import { createContext, useContext, useMemo } from "react";
import type { Locale } from "@/lib/i18n/config";
import type { Messages } from "@/lib/i18n/messages";
import { createTranslator, type Translator } from "@/lib/i18n/translate";

interface LocaleContextValue {
  locale: Locale;
  t: Translator;
}

const LocaleContext = createContext<LocaleContextValue | null>(null);

/**
 * Carries the resolved locale into client components.
 *
 * Client components cannot call `headers()`, and threading a `locale` prop
 * through four layers of a profile tree would touch every intermediate
 * signature. A provider rendered once in the root layout does it in one place,
 * and because the tree is remounted per URL the value is never stale.
 *
 * The messages arrive already merged over English from the server, so a client
 * miss also lands on English — same behaviour as the server, no second merge.
 */
export function LocaleProvider({
  locale,
  messages,
  children,
}: {
  locale: Locale;
  messages: Messages;
  children: React.ReactNode;
}) {
  const value = useMemo(
    () => ({ locale, t: createTranslator(locale, messages) }),
    [locale, messages],
  );

  return <LocaleContext value={value}>{children}</LocaleContext>;
}

export function useLocale(): Locale {
  return useContext(LocaleContext)?.locale ?? "en";
}

/**
 * Throws rather than falling back to English when the provider is absent: a
 * silently English component inside a Chinese page is the exact bug this is
 * meant to prevent, and it is invisible in review.
 */
export function useT(): Translator {
  const context = useContext(LocaleContext);

  if (!context) {
    throw new Error("useT() must be used inside <LocaleProvider>");
  }

  return context.t;
}
