"use client";

import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { LanguagesIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  LOCALE_COOKIE,
  LOCALE_COOKIE_MAX_AGE,
  LOCALE_LABELS,
  LOCALES,
  localizeHref,
  stripLocale,
  type Locale,
} from "@/lib/i18n/config";
import { useLocale, useT } from "@/components/layout/LocaleProvider";

/**
 * Switch language in place.
 *
 * The new address is computed from the current one with the prefix swapped
 * rather than from a fixed route, so a reader deep in a filtered leaderboard or
 * on someone's profile stays exactly where they were. The query string is
 * copied across for the same reason: `?period=week` is part of what they chose
 * to look at.
 *
 * The cookie records the choice for `/`, which is the only route with no page
 * of its own — everywhere else the URL already says which language it is.
 */
export function LanguageSwitcher() {
  const locale = useLocale();
  const t = useT();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const router = useRouter();

  const switchTo = (next: Locale) => {
    const { path } = stripLocale(pathname || "/");
    const query = searchParams?.toString();
    const href = localizeHref(query ? `${path}?${query}` : path, next);

    // `document.cookie` is a browser global the compiler has no model for, so
    // assigning it reads as mutating an import. It is the only way to write a
    // cookie without a round trip.
    // eslint-disable-next-line react-hooks/immutability
    document.cookie = `${LOCALE_COOKIE}=${next}; path=/; max-age=${LOCALE_COOKIE_MAX_AGE}; samesite=lax`;
    router.push(href);
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon"
            className="size-8"
            aria-label={t("nav.language")}
          />
        }
      >
        <LanguagesIcon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-40">
        {LOCALES.map((candidate) => (
          <DropdownMenuItem
            key={candidate}
            onClick={() => switchTo(candidate)}
            aria-current={candidate === locale ? "true" : undefined}
            className={candidate === locale ? "font-medium" : undefined}
          >
            {/* Endonyms, so the list is readable before anything is switched. */}
            {LOCALE_LABELS[candidate]}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
