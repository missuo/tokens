import Link from "next/link";
import { Button } from "@/components/ui/button";
import { CONTAINER } from "@/components/layout/Container";
import { cn } from "@/lib/utils";
import { getTranslator } from "@/lib/i18n/server";
import { localizeHref } from "@/lib/i18n/config";

/**
 * The 404 every unmatched route lands on.
 *
 * Without this file Next serves its own English page, which was the last place
 * a Chinese or Japanese reader fell out of the language — and it happens on the
 * routes most likely to be typed by hand or reached from a stale link.
 */
export default async function NotFound() {
  const { locale, t } = await getTranslator();

  return (
    <main id="main-content" className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")}>
      <header className="flex flex-col gap-1.5">
        <h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">
          {t("errors.notFoundTitle")}
        </h1>
        <p className="max-w-[90ch] text-sm leading-relaxed text-muted-foreground">
          {t("errors.notFoundDescription")}
        </p>
      </header>

      <div className="mt-7">
        <Button
          variant="outline"
          size="sm"
          render={<Link href={localizeHref("/leaderboard", locale)} />}
        >
          {t("common.backToLeaderboard")}
        </Button>
      </div>
    </main>
  );
}
