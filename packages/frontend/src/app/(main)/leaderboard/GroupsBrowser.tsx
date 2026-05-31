"use client";

import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import { CreateGroupDialog } from "./CreateGroupDialog";

// Inlined view of the groups list under /leaderboard ?view=groups.

type GroupRole = "owner" | "admin" | "member";

interface SessionUser {
  id: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
}

export interface GroupCardData {
  id: string;
  name: string;
  slug: string;
  description: string | null;
  avatarUrl: string | null;
  isPublic: boolean;
  memberCount: number;
  role?: GroupRole;
}

interface GroupPagination {
  page: number;
  limit: number;
  total: number;
  totalPages: number;
  hasNext: boolean;
  hasPrev: boolean;
}

interface GroupsBrowserProps {
  currentUser: SessionUser | null;
  initialPublicGroups: GroupCardData[];
  initialMyGroups: GroupCardData[];
  initialPublicPagination: GroupPagination;
  initialMyPagination: GroupPagination;
}

type ActiveTab = "public" | "mine";

function GroupCard({ group }: { group: GroupCardData }) {
  return (
    <Link
      href={`/groups/${group.slug}`}
      className="group flex min-h-[150px] flex-col rounded-xl border border-line bg-surface p-4 text-inherit transition hover:border-accent/50 hover:bg-surface-secondary focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2"
    >
      <div className="mb-3 flex items-center gap-3">
        <div
          className="h-11 w-11 flex-none rounded-lg border border-line"
          style={{ background: group.avatarUrl ? `url(${group.avatarUrl}) center/cover` : "linear-gradient(135deg, var(--accent), #13a10e)" }}
        />
        <div className="min-w-0">
          <h2 className="truncate text-base font-semibold text-foreground transition group-hover:text-accent">{group.name}</h2>
          <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted">
            <span className={`rounded px-1.5 py-0.5 font-medium ${group.isPublic ? "bg-surface-tertiary" : "bg-warning/15 text-warning"}`}>
              {group.isPublic ? "Public" : "Private"}
            </span>
            <span className="font-mono tabular-nums">{group.memberCount} members</span>
            {group.role && <span className="capitalize">· {group.role}</span>}
          </div>
        </div>
      </div>
      <p className="line-clamp-2 text-sm leading-relaxed text-muted">{group.description || "A scoped Tokens leaderboard."}</p>
    </Link>
  );
}

export default function GroupsBrowser({
  currentUser,
  initialPublicGroups,
  initialMyGroups,
  initialPublicPagination,
  initialMyPagination,
}: GroupsBrowserProps) {
  const [activeTab, setActiveTab] = useState<ActiveTab>(currentUser ? "mine" : "public");
  const [publicGroups, setPublicGroups] = useState(initialPublicGroups);
  const [myGroups, setMyGroups] = useState(initialMyGroups);
  const [publicPagination, setPublicPagination] = useState(initialPublicPagination);
  const [myPagination, setMyPagination] = useState(initialMyPagination);
  const [loadingState, setLoadingState] = useState<Record<ActiveTab, boolean>>({ public: false, mine: false });
  const [error, setError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const inflightByTab = useRef<Map<ActiveTab, AbortController>>(new Map());

  const setTabLoading = useCallback((tab: ActiveTab, isLoading: boolean) => {
    setLoadingState((current) => ({ ...current, [tab]: isLoading }));
  }, []);

  const loadGroups = useCallback(
    (tab: ActiveTab, append = false) => {
      inflightByTab.current.get(tab)?.abort();
      const controller = new AbortController();
      inflightByTab.current.set(tab, controller);

      const page = append && tab === "mine" ? myPagination.page + 1 : append ? publicPagination.page + 1 : 1;
      const params = new URLSearchParams({ page: String(page), limit: "20" });
      if (tab === "mine") params.set("my", "true");

      const url = `/api/groups?${params.toString()}`;
      setTabLoading(tab, true);
      setError(null);

      fetch(url, { signal: controller.signal })
        .then((response) => {
          if (!response.ok) throw new Error(`HTTP ${response.status}`);
          return response.json();
        })
        .then((payload) => {
          if (!Array.isArray(payload.groups)) throw new Error("Invalid response");
          const nextPagination = payload.pagination;
          if (tab === "mine") {
            setMyGroups((prev) => (append ? [...prev, ...payload.groups] : payload.groups));
            if (nextPagination) setMyPagination(nextPagination);
          } else {
            setPublicGroups((prev) => (append ? [...prev, ...payload.groups] : payload.groups));
            if (nextPagination) setPublicPagination(nextPagination);
          }
        })
        .catch((err) => {
          if (err.name !== "AbortError") setError(err.message || "Failed to load groups");
        })
        .finally(() => {
          if (inflightByTab.current.get(tab) === controller) {
            setTabLoading(tab, false);
            inflightByTab.current.delete(tab);
          }
        });
    },
    [myPagination.page, publicPagination.page, setTabLoading],
  );

  useEffect(() => {
    const inflight = inflightByTab.current;
    return () => {
      for (const controller of inflight.values()) controller.abort();
      inflight.clear();
    };
  }, []);

  const groups = activeTab === "mine" ? myGroups : publicGroups;
  const activePagination = activeTab === "mine" ? myPagination : publicPagination;
  const isLoading = loadingState[activeTab];

  const handleTabChange = (tab: ActiveTab) => {
    if (activeTab === tab) return;
    setActiveTab(tab);
    const currentGroups = tab === "mine" ? myGroups : publicGroups;
    const currentPagination = tab === "mine" ? myPagination : publicPagination;
    if (currentPagination.page === 1 && currentGroups.length > 0) return;
    loadGroups(tab);
  };

  const handleLoadMore = () => loadGroups(activeTab, true);

  const signInHref = "/api/auth/github?returnTo=/leaderboard?view=groups";
  const tabClass = (active: boolean) =>
    `rounded-md px-3.5 py-1.5 text-sm font-medium transition focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50 ${
      active ? "bg-surface text-foreground shadow-sm ring-1 ring-line" : "text-muted hover:text-foreground"
    }`;
  const primaryLinkClass = "inline-flex h-10 items-center justify-center rounded-lg bg-accent px-4 text-sm font-semibold whitespace-nowrap text-accent-foreground transition hover:opacity-90";

  return (
    <>
      <section className="mt-6 mb-5 flex items-center justify-between gap-4 max-[640px]:flex-col max-[640px]:items-stretch">
        <p className="max-w-[680px] text-sm leading-relaxed text-muted">
          Create private or public leaderboards for teams, friends, and workspaces without changing the global Tokens rankings.
        </p>
        {currentUser ? (
          <button type="button" onClick={() => setCreateOpen(true)} className={primaryLinkClass}>+ New group</button>
        ) : (
          <Link href={signInHref} className={primaryLinkClass}>Sign in</Link>
        )}
      </section>

      <CreateGroupDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={() => {
          setActiveTab("mine");
          loadGroups("mine");
        }}
      />

      <div aria-label="Group filters" className="mb-4 inline-flex rounded-xl border border-line bg-surface-secondary p-1">
        <button className={tabClass(activeTab === "public")} onClick={() => handleTabChange("public")}>Public</button>
        <button className={tabClass(activeTab === "mine")} onClick={() => handleTabChange("mine")} disabled={!currentUser}>My groups</button>
      </div>

      {error && <p role="alert" className="text-danger">{error}</p>}
      {isLoading && groups.length === 0 ? (
        <div aria-busy="true" aria-live="polite" aria-label="Loading groups" className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="min-h-[150px] animate-pulse rounded-xl border border-line bg-surface" />
          ))}
        </div>
      ) : (
        <>
          {groups.length === 0 ? (
            <div className="rounded-xl border border-line bg-surface p-7 text-muted">
              {activeTab === "mine" ? "You are not in any groups yet." : "No public groups yet."}
            </div>
          ) : (
            <>
              <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3">
                {groups.map((group) => (
                  <GroupCard key={group.id} group={group} />
                ))}
              </div>
              {activePagination.hasNext ? (
                <button type="button" onClick={handleLoadMore} disabled={isLoading} className="mt-4 min-h-9 rounded-xl border border-line bg-surface px-4 text-foreground disabled:cursor-not-allowed disabled:opacity-60">
                  {isLoading ? "Loading..." : "Load more"}
                </button>
              ) : null}
            </>
          )}
        </>
      )}
    </>
  );
}
