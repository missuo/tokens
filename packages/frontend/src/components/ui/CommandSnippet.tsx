"use client";

import { useState } from "react";
import { Button } from "@heroui/react";
import { CopyIcon, CheckIcon } from "@/components/ui/Icons";

interface CommandSnippetProps {
  /** The full command to copy to the clipboard. */
  command: string;
  /** Optional rich display; defaults to the raw command. */
  children?: React.ReactNode;
  /** Leading prompt symbol. */
  prompt?: string;
  className?: string;
}

/**
 * A copyable terminal command line. HeroUI v3 ships no `Snippet`, so this is a
 * small shared primitive used by the hero, the leaderboard CTA and settings.
 */
export function CommandSnippet({
  command,
  children,
  prompt = "$",
  className = "",
}: CommandSnippetProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div
      className={`flex items-center gap-2 rounded-lg border border-line bg-surface-secondary px-3 py-2.5 font-mono text-sm ${className}`}
    >
      <span className="select-none text-muted">{prompt}</span>
      <code className="flex-1 overflow-x-auto whitespace-nowrap text-foreground">
        {children ?? command}
      </code>
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        aria-label="Copy command"
        onPress={handleCopy}
        className="shrink-0 text-muted data-[hover=true]:text-foreground"
      >
        {copied ? <CheckIcon size={16} /> : <CopyIcon size={16} />}
      </Button>
    </div>
  );
}
