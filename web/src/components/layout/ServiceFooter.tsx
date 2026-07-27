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

        {/* Both marks are checked in rather than hotlinked, so a brand site
            reorganising cannot break the footer. */}
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          Powered by
          <a
            href="https://workers.cloudflare.com"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 transition-colors hover:text-foreground"
          >
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src="/icons/cloudflare.svg" alt="" width={13} height={13} className="size-3.5" />
            Workers
          </a>
          <span aria-hidden="true" className="opacity-50">·</span>
          <a
            href="https://aiven.io"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 transition-colors hover:text-foreground"
          >
            {/* Aiven ships its mark white-on-dark rather than as a coloured
                glyph on transparent, so unlike the other two this one carries
                its own background. That is what makes it legible in both
                themes — an `<img>`-loaded SVG cannot inherit `currentColor`.
                The asset is unmodified; the rounding is CSS so the square
                reads as an icon rather than a block. */}
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src="/icons/aiven.svg"
              alt=""
              width={13}
              height={13}
              className="size-3.5 rounded-[3px]"
            />
            Aiven
          </a>
        </span>
      </div>
    </footer>
  );
}
