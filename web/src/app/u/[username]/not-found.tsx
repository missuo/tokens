import { Suspense } from "react";
import Link from "next/link";
import PrivateProfileOwnerView from "./PrivateProfileOwnerView";
import { getTranslator } from "@/lib/i18n/server";
import type { Translator } from "@/lib/i18n/translate";
import type { Locale } from "@/lib/i18n/config";
import { localizeHref } from "@/lib/i18n/config";

/**
 * Kept synchronous and told the locale, because both places it is used are a
 * Suspense `fallback` — an async fallback resolves before it can stand in for
 * anything, so the page reads the locale once and passes it down.
 */
function NotFoundContent({ locale, t }: { locale: Locale; t: Translator }) {
  return (
    <div className="flex min-h-screen flex-col bg-background pt-16">
      <main className="mx-auto w-full max-w-[800px] flex-1 px-6 py-10">
        <div className="py-20 text-center">
          <h1 className="mb-2 text-2xl font-bold text-foreground">
            {t("errors.profileNotFoundTitle")}
          </h1>
          <p className="mb-6 text-muted-foreground">
            {t("errors.profileNotFoundDescription")}
          </p>
          <Link
            href={localizeHref("/leaderboard", locale)}
            className="inline-flex items-center rounded-lg bg-primary px-4 py-2 font-medium text-primary-foreground transition hover:opacity-90"
          >
            {t("errors.backToLeaderboard")}
          </Link>
        </div>
      </main>
    </div>
  );
}

// A private profile lands here for every reader, so the cached page cannot
// tell it apart from a missing one. The owner view upgrades it in the browser
// when the signed-in user is the one in the URL.
export default async function ProfileNotFound() {
  const { locale, t } = await getTranslator();

  return (
    <Suspense fallback={<NotFoundContent locale={locale} t={t} />}>
      <PrivateProfileOwnerView fallback={<NotFoundContent locale={locale} t={t} />} />
    </Suspense>
  );
}
