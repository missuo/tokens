import Link from "next/link";
import { CONTAINER } from "@/components/layout/Container";
import { cn } from "@/lib/utils";

/**
 * Site footer. Navigation lives in the header, so this only carries the
 * things that belong at the bottom of a page: what this is, what it runs on,
 * and where it came from.
 *
 * The upstream credit stays — this fork is MIT-licensed from Tokscale, and
 * the attribution is both honest and cheap to keep.
 */
export function ServiceFooter() {
  // Read at render rather than hardcoded. This is a server component, so the
  // year comes from the server clock once and ships in the HTML — no hydration
  // mismatch, and no January where the site still claims the previous year.
  const year = new Date().getFullYear();

  return (
    <footer className="mt-auto border-t" aria-label="Site footer">
      <div
        className={cn(
          CONTAINER,
          // Stacked on phones, so centre both the boxes and their text — a
          // column flex container defaults to stretch, which left-aligns two
          // lines of different length against the screen edge.
          "flex flex-col items-center gap-3 py-7 text-center",
          "sm:flex-row sm:justify-between sm:text-left"
        )}
      >
        {/* The policy links live here and nowhere else. They have to be
            reachable from every page, but they are not something anyone came
            for, so they sit at the same weight as the rest of the footer. */}
        <span className="text-xs text-muted-foreground">
          Tokens · © {year} ·{" "}
          <Link href="/privacy" className="transition-colors hover:text-foreground">
            Privacy
          </Link>{" "}
          ·{" "}
          <Link href="/terms" className="transition-colors hover:text-foreground">
            Terms
          </Link>{" "}
          ·{" "}
          <a
            href="https://github.com/junhoyeo/tokscale"
            target="_blank"
            rel="noopener noreferrer"
            className="transition-colors hover:text-foreground"
          >
            Built on Tokscale
          </a>
        </span>

        {/* Only the sponsors. Naming every piece of the stack down here was
            noise — the parts that matter are documented on /docs, where there
            is room to say what they do. What belongs in a footer is the credit
            that is owed: two companies pay for infrastructure this project
            does not. Both marks are checked in rather than hotlinked, so a
            brand site reorganising cannot break the footer. */}
        <span className="flex flex-wrap items-center justify-center gap-x-1.5 gap-y-1 text-xs text-muted-foreground">
          Server sponsored by
          <a
            href="https://v.ps"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 transition-colors hover:text-foreground"
          >
            {/* V.PS publishes its mark white-on-dark with the corner radius
                baked into the SVG, so unlike the other two it carries its own
                background and needs no CSS rounding. Unmodified. */}
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src="/icons/vps.svg" alt="" width={13} height={13} className="size-3.5" />
            V.PS
          </a>
          <span aria-hidden="true" className="opacity-50">·</span>
          Database sponsored by
          <a
            href="https://neon.com"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 transition-colors hover:text-foreground"
          >
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src="/icons/neon.svg" alt="" width={13} height={13} className="size-3.5" />
            Neon
          </a>
        </span>
      </div>
    </footer>
  );
}
