"use client";

import { useEffect } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { CONTAINER } from "@/components/layout/Container";
import { cn } from "@/lib/utils";
import { useLocale, useT } from "@/components/layout/LocaleProvider";
import { localizeHref } from "@/lib/i18n/config";

/**
 * Root error boundary.
 *
 * /leaderboard, /shame and /u/[username] all do live database work during
 * render, so any of them can throw. Without this, that lands on Next's default
 * error screen — no navigation, no way back into the app.
 *
 * It renders inside the root layout, so the header and footer are still there;
 * this only has to explain what happened and offer the two useful actions.
 */
export default function RootError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  const t = useT();
  const locale = useLocale();

  useEffect(() => {
    console.error(error);
  }, [error]);

  return (
    <main id="main-content" className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")}>
      <header className="flex flex-col gap-1.5">
        <h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">
          {t("errors.rootTitle")}
        </h1>
        <p className="max-w-[90ch] text-sm leading-relaxed text-muted-foreground">
          {t("errors.rootDescription")}
        </p>
      </header>

      <div className="mt-7 flex flex-wrap items-center gap-2">
        <Button size="sm" onClick={reset}>
          {t("common.retry")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          render={<Link href={localizeHref("/leaderboard", locale)} />}
        >
          {t("common.backToLeaderboard")}
        </Button>
      </div>

      {/* The digest is the only handle support has on a specific failure, and
          it carries nothing sensitive. */}
      {error.digest && (
        <p className="mt-6 font-mono text-xs text-muted-foreground">
          {t("errors.reference")} {error.digest}
        </p>
      )}
    </main>
  );
}
