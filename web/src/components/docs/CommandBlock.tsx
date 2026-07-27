"use client";

import { useState } from "react";
import { CheckIcon, CopyIcon } from "lucide-react";
import { cn } from "@/lib/utils";

export interface DocCommand {
  /** The exact line a reader should run. */
  command: string;
  /** One line on what it does, shown beside the command. */
  note?: string;
}

/**
 * Copyable shell snippets.
 *
 * The command text used to sit *inside* the copy button, which made a long
 * command impossible to read: its horizontal scroll had no keyboard equivalent,
 * and dragging it on a touch screen fired the copy instead of scrolling. The
 * row is now a plain container — the scrollable <code> stands on its own, and
 * copying is an ordinary button beside it.
 */
export function CommandBlock({ commands }: { commands: readonly DocCommand[] }) {
  const [copied, setCopied] = useState<number | null>(null);

  const copy = async (text: string, index: number) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(index);
      window.setTimeout(() => setCopied(null), 1600);
    } catch {
      // Clipboard access can be denied or unavailable on an insecure origin.
      // The text stays selectable, so failing quietly is acceptable.
    }
  };

  return (
    <div className="overflow-hidden rounded-lg border">
      {commands.map((entry, index) => (
        <div
          key={entry.command}
          className={cn(
            "group flex w-full items-center gap-3 bg-muted/40 px-3 py-3 text-left transition-colors hover:bg-muted/70 sm:px-4",
            index > 0 && "border-t"
          )}
        >
          {/* tabIndex makes the overflow region focusable, which is what gives
              a keyboard user arrow-key scrolling over a long command. */}
          <code
            tabIndex={0}
            className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-[13px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
          >
            <span className="select-none text-muted-foreground">$ </span>
            {entry.command}
          </code>

          {entry.note && (
            <span className="hidden shrink-0 text-xs text-muted-foreground md:inline">
              {entry.note}
            </span>
          )}

          <button
            type="button"
            onClick={() => copy(entry.command, index)}
            aria-label={`Copy command: ${entry.command}`}
            className="shrink-0 rounded-md p-1 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {copied === index ? (
              <CheckIcon className="size-3.5 text-foreground" />
            ) : (
              <CopyIcon className="size-3.5" />
            )}
          </button>

          {/* Announced rather than only shown: the icon swap alone told a
              screen-reader user nothing about whether the copy worked. */}
          <span role="status" aria-live="polite" className="sr-only">
            {copied === index ? `Copied ${entry.command}` : ""}
          </span>
        </div>
      ))}
    </div>
  );
}
