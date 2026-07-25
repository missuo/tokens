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
 * The whole row is the copy target rather than a small button at the end:
 * copying a command is the only thing anyone does here, and hunting for a
 * 32px icon — especially on a phone — is friction for no benefit.
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
        <button
          key={entry.command}
          type="button"
          onClick={() => copy(entry.command, index)}
          aria-label={`Copy command: ${entry.command}`}
          className={cn(
            "group flex w-full items-center gap-3 bg-muted/40 px-3 py-3 text-left transition-colors hover:bg-muted/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring sm:px-4",
            index > 0 && "border-t"
          )}
        >
          <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-[13px] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
            <span className="select-none text-muted-foreground">$ </span>
            {entry.command}
          </code>

          {entry.note && (
            <span className="hidden shrink-0 text-xs text-muted-foreground md:inline">
              {entry.note}
            </span>
          )}

          <span
            className={cn(
              "shrink-0 transition-opacity",
              copied === index
                ? "text-foreground opacity-100"
                : "text-muted-foreground opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100"
            )}
            aria-hidden="true"
          >
            {copied === index ? (
              <CheckIcon className="size-3.5" />
            ) : (
              <CopyIcon className="size-3.5" />
            )}
          </span>
        </button>
      ))}
    </div>
  );
}
