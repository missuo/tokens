"use client";

import { useEffect } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { CONTAINER } from "@/components/layout/Container";
import { cn } from "@/lib/utils";

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
  useEffect(() => {
    console.error(error);
  }, [error]);

  return (
    <main id="main-content" className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")}>
      <header className="flex flex-col gap-1.5">
        <h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">
          Something went wrong
        </h1>
        <p className="max-w-[90ch] text-sm leading-relaxed text-muted-foreground">
          This page could not be loaded. It is usually temporary — trying again
          is worth a shot before anything else.
        </p>
      </header>

      <div className="mt-7 flex flex-wrap items-center gap-2">
        <Button size="sm" onClick={reset}>
          Try again
        </Button>
        <Button variant="outline" size="sm" render={<Link href="/leaderboard" />}>
          Go to the leaderboard
        </Button>
      </div>

      {/* The digest is the only handle support has on a specific failure, and
          it carries nothing sensitive. */}
      {error.digest && (
        <p className="mt-6 font-mono text-xs text-muted-foreground">
          Reference: {error.digest}
        </p>
      )}
    </main>
  );
}
