"use client";

import Link from "next/link";
import { useEffect, useRef, useState } from "react";

// Top-of-page segmented control that swaps between the global user leaderboard
// and the group browser. Pure-link nav (no client state), so SSR + back/forward
// behave naturally and the URL is shareable.

export type LeaderboardView = "users" | "groups";

export type LeaderboardSearchParams = Record<string, string | string[] | undefined>;

interface ViewSelectorProps {
  current: LeaderboardView;
  searchParams: LeaderboardSearchParams;
}

export function buildLeaderboardViewHref(searchParams: LeaderboardSearchParams, view: LeaderboardView): string {
  const params = new URLSearchParams();
  const currentPeriod = typeof searchParams.period === "string" ? searchParams.period : undefined;

  for (const [key, value] of Object.entries(searchParams)) {
    if (key === "page" || key === "view" || value === undefined) continue;
    // Only carry from/to when the current period is "custom"; otherwise they
    // would prime stale date inputs on the destination view.
    if ((key === "from" || key === "to") && currentPeriod !== "custom") continue;
    if (Array.isArray(value)) {
      for (const item of value) params.append(key, item);
    } else {
      params.set(key, value);
    }
  }

  params.set("view", view);
  return `/leaderboard?${params.toString()}`;
}

const VIEW_LABELS: Record<LeaderboardView, string> = { users: "Users", groups: "Groups" };

function UsersIcon() {
  return (
    <svg aria-hidden="true" width="15" height="15" viewBox="0 0 16 16" fill="currentColor">
      <path d="M10.561 8.073a6.005 6.005 0 0 1 3.432 5.142.75.75 0 1 1-1.498.07 4.5 4.5 0 0 0-8.99 0 .75.75 0 0 1-1.498-.07 6.004 6.004 0 0 1 3.431-5.142 3.999 3.999 0 1 1 5.123 0ZM10.5 5a2.5 2.5 0 1 0-5 0 2.5 2.5 0 0 0 5 0Z" />
    </svg>
  );
}

function GroupsIcon() {
  return (
    <svg aria-hidden="true" width="15" height="15" viewBox="0 0 16 16" fill="currentColor">
      <path d="M2 5.5a3.5 3.5 0 1 1 5.898 2.549 5.508 5.508 0 0 1 3.034 4.084.75.75 0 1 1-1.482.235 4 4 0 0 0-7.9 0 .75.75 0 0 1-1.482-.236A5.507 5.507 0 0 1 3.102 8.05 3.49 3.49 0 0 1 2 5.5ZM11 4a3.001 3.001 0 0 1 2.22 5.018 5.01 5.01 0 0 1 2.56 3.012.749.749 0 0 1-.885.954.752.752 0 0 1-.549-.514 3.507 3.507 0 0 0-2.522-2.372.75.75 0 0 1-.574-.73v-.352a.75.75 0 0 1 .416-.672A1.5 1.5 0 0 0 11 5.5.75.75 0 0 1 11 4Zm-5.5-.5a2 2 0 1 0-.001 3.999A2 2 0 0 0 5.5 3.5Z" />
    </svg>
  );
}

export default function ViewSelector({ current, searchParams }: ViewSelectorProps) {
  const [announcement, setAnnouncement] = useState("");
  const isFirstRender = useRef(true);

  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      return;
    }
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setAnnouncement(`Showing ${VIEW_LABELS[current]}`);
  }, [current]);

  const itemClass = (active: boolean) =>
    `inline-flex flex-1 items-center justify-center gap-1.5 rounded-md px-3.5 py-1.5 text-sm font-medium transition focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2 sm:flex-none ${
      active ? "bg-surface text-foreground shadow-sm ring-1 ring-line" : "text-muted hover:text-foreground"
    }`;

  return (
    <nav aria-label="Leaderboard view" className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <h1 className="text-2xl font-bold tracking-tight text-foreground sm:text-[1.75rem]">
          {current === "groups" ? "Groups" : "Leaderboard"}
        </h1>
        <p className="mt-1 text-sm text-muted">
          {current === "groups" ? "Browse and compete in token usage groups" : "See who's burning the most tokens"}
        </p>
      </div>
      <div className="inline-flex items-center gap-1 rounded-lg border border-line bg-surface-secondary p-1">
        <Link href={buildLeaderboardViewHref(searchParams, "users")} className={itemClass(current === "users")} aria-current={current === "users" ? "page" : undefined}>
          <UsersIcon />
          Users
        </Link>
        <Link href={buildLeaderboardViewHref(searchParams, "groups")} className={itemClass(current === "groups")} aria-current={current === "groups" ? "page" : undefined}>
          <GroupsIcon />
          Groups
        </Link>
      </div>
      <span role="status" aria-live="polite" className="sr-only">
        {announcement}
      </span>
    </nav>
  );
}
