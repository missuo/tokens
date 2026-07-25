import { CONTAINER } from "@/components/layout/Container";
import { cn } from "@/lib/utils";

const YEAR = 2026;

/**
 * Site footer. Navigation lives in the header, so this only carries the
 * things that belong at the bottom of a page: what this is, what it runs on,
 * and where it came from.
 *
 * The upstream credit stays — this fork is MIT-licensed from Tokscale, and
 * the attribution is both honest and cheap to keep.
 */
export function ServiceFooter() {
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
        <span className="text-xs text-muted-foreground">
          Tokens · © {YEAR} ·{" "}
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
