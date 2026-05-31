"use client";

import Link from "next/link";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "nextjs-toploader/app";
import { CheckIcon, CopyIcon, SearchIcon, XIcon } from "@/components/ui/Icons";
import { StatGrid, StatTile } from "@/components/ui/primitives";
import { TabBar } from "@/components/TabBar";
import { formatCurrency, formatNumber } from "@/lib/utils";
import type { GroupLeaderboardData, GroupLeaderboardUser } from "@/lib/groups/getGroupLeaderboard";
import type { Period, SortBy } from "@/lib/leaderboard/types";

type GroupRole = "owner" | "admin" | "member";

interface SessionUser {
  id: string;
  username: string;
  displayName: string | null;
  avatarUrl: string | null;
}

interface GroupDetail {
  id: string;
  name: string;
  slug: string;
  description: string | null;
  avatarUrl: string | null;
  isPublic: boolean;
  memberCount: number;
  membership: { role: GroupRole } | null;
}

interface GroupDetailClientProps {
  group: GroupDetail;
  currentUser: SessionUser | null;
  initialData: GroupLeaderboardData;
}

function isAdminRole(role: GroupRole | undefined): boolean {
  return role === "owner" || role === "admin";
}

function roleLabel(role: string): string {
  return role.charAt(0).toUpperCase() + role.slice(1);
}

const fieldClass = "h-[38px] rounded-lg border border-line bg-surface-secondary px-3 text-sm text-foreground outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/25";
const buttonClass = "inline-flex h-[38px] items-center justify-center gap-2 rounded-lg border border-line bg-surface px-3.5 text-sm font-medium text-foreground transition hover:border-foreground/20 hover:bg-surface-secondary disabled:cursor-not-allowed disabled:opacity-65";
const badgeClass = "inline-flex items-center rounded-md bg-surface-tertiary px-2 py-0.5 text-xs font-medium text-muted";
const thClass = "border-b border-line bg-surface-secondary px-4 py-3 text-left text-xs font-semibold tracking-wider text-muted uppercase";
const tdClass = "border-b border-line px-4 py-3 text-foreground last:border-b-0";

const rankColor: Record<number, string> = { 1: "text-[#EAB308]", 2: "text-[#9CA3AF]", 3: "text-[#D97706]" };

function GroupRow({ user, showSubmissionCount }: { user: GroupLeaderboardUser; showSubmissionCount: boolean }) {
  return (
    <tr className="transition-colors hover:bg-foreground/[0.03]">
      <td className={tdClass}>
        <span className={`font-mono text-sm font-bold tabular-nums ${rankColor[user.rank] ?? "text-muted"}`}>#{user.rank}</span>
      </td>
      <td className={tdClass}>
        <Link href={`/u/${user.username}`} className="inline-flex items-center gap-2.5 text-inherit">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={user.avatarUrl || `https://github.com/${user.username}.png`} alt={user.username} className="h-9 w-9 rounded-full object-cover ring-1 ring-line" />
          <span className="min-w-0">
            <span className="block truncate text-sm font-medium">{user.displayName || user.username}</span>
            <span className="block truncate font-mono text-xs text-muted">@{user.username}</span>
          </span>
        </Link>
      </td>
      <td className={tdClass}><span className="text-sm text-muted capitalize">{roleLabel(user.role)}</span></td>
      <td className={`${tdClass} text-right`}><span className="font-mono text-sm font-medium tabular-nums">{formatCurrency(user.totalCost)}</span></td>
      <td className={`${tdClass} text-right`}><span className="font-mono text-sm font-semibold text-accent tabular-nums">{formatNumber(user.totalTokens)}</span></td>
      {showSubmissionCount && <td className={`${tdClass} text-right`}><span className="font-mono text-sm text-muted tabular-nums">{user.submissionCount ?? "—"}</span></td>}
    </tr>
  );
}

export default function GroupDetailClient({ group, initialData }: GroupDetailClientProps) {
  const router = useRouter();
  const [data, setData] = useState(initialData);
  const [period, setPeriod] = useState<Period>(initialData.period);
  const [sortBy, setSortBy] = useState<SortBy>(initialData.sortBy);
  const [page, setPage] = useState(1);
  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [inviteRole, setInviteRole] = useState<Exclude<GroupRole, "owner">>("member");
  const [inviteUsername, setInviteUsername] = useState("");
  const [inviteUrl, setInviteUrl] = useState<string | null>(null);
  const [inviteError, setInviteError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const didMountLeaderboard = useRef(false);

  const canInvite = isAdminRole(group.membership?.role);
  const showSubmissionCount = period === "all";

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(search);
      setPage(1);
    }, 250);
    return () => clearTimeout(timer);
  }, [search]);

  const loadLeaderboard = useCallback(
    (signal?: AbortSignal) => {
      const params = new URLSearchParams({ period, sortBy, page: String(page), limit: "50" });
      if (debouncedSearch) params.set("search", debouncedSearch);

      setIsLoading(true);
      setError(null);

      fetch(`/api/groups/${group.slug}/leaderboard?${params}`, { signal })
        .then((response) => {
          if (!response.ok) throw new Error(`HTTP ${response.status}`);
          return response.json();
        })
        .then((payload) => setData(payload))
        .catch((err) => {
          if (err.name !== "AbortError") setError(err.message || "Failed to load leaderboard");
        })
        .finally(() => {
          if (!signal?.aborted) setIsLoading(false);
        });
    },
    [debouncedSearch, group.slug, page, period, sortBy],
  );

  useEffect(() => {
    if (!didMountLeaderboard.current) {
      didMountLeaderboard.current = true;
      return;
    }
    const abortController = new AbortController();
    loadLeaderboard(abortController.signal);
    return () => abortController.abort();
  }, [loadLeaderboard]);

  async function createInvite() {
    setInviteError(null);
    setInviteUrl(null);
    try {
      const response = await fetch(`/api/groups/${group.slug}/invite`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ role: inviteRole, invitedUsername: inviteUsername.trim() || null }),
      });
      const payload = await response.json();
      if (!response.ok) throw new Error(payload.error || "Failed to create invite");
      setInviteUrl(`${window.location.origin}${payload.joinUrl}`);
      setInviteUsername("");
    } catch (err) {
      setInviteError(err instanceof Error ? err.message : "Failed to create invite");
    }
  }

  async function copyInvite() {
    if (!inviteUrl) return;
    try {
      setInviteError(null);
      await navigator.clipboard.writeText(inviteUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
      setInviteError("Could not copy invite link.");
    }
  }

  async function leaveGroup() {
    const response = await fetch(`/api/groups/${group.slug}/leave`, { method: "POST" });
    if (response.ok) router.push("/groups");
  }

  const sortedUsers = useMemo(() => data.users || [], [data.users]);

  return (
    <>
      <section className="mt-6 mb-8 grid gap-5">
        <div className="flex items-start justify-between gap-4 max-[720px]:flex-col">
          <div className="flex items-center gap-3.5">
            <div className="h-14 w-14 flex-none rounded-xl border border-line" style={{ background: group.avatarUrl ? `url(${group.avatarUrl}) center/cover` : "linear-gradient(135deg, var(--accent), #13a10e)" }} />
            <div className="min-w-0">
              <h1 className="truncate text-2xl font-bold tracking-tight text-foreground sm:text-[1.75rem]">{group.name}</h1>
              <div className="mt-1.5 flex flex-wrap items-center gap-2">
                <span className={`${badgeClass} ${group.isPublic ? "" : "bg-warning/15 text-warning"}`}>{group.isPublic ? "Public" : "Private"}</span>
                <span className={badgeClass}><span className="font-mono tabular-nums">{group.memberCount}</span>&nbsp;members</span>
                {group.membership && <span className={`${badgeClass} capitalize`}>{roleLabel(group.membership.role)}</span>}
              </div>
            </div>
          </div>
          <div className="flex flex-wrap gap-2">
            <Link href="/leaderboard?view=groups" className={buttonClass}>All groups</Link>
            {group.membership && group.membership.role !== "owner" && (
              <button onClick={leaveGroup} className={`${buttonClass} hover:border-danger/40 hover:text-danger`}>Leave</button>
            )}
          </div>
        </div>
        {group.description && <p className="max-w-[680px] text-sm leading-relaxed text-muted">{group.description}</p>}

        <StatGrid cols={4}>
          <StatTile label="Active users" value={data.stats.activeUsers} />
          <StatTile label="Members" value={data.stats.totalMembers || group.memberCount} />
          <StatTile label="Total tokens" value={formatNumber(data.stats.totalTokens)} accent />
          <StatTile label="Total cost" value={formatCurrency(data.stats.totalCost)} />
        </StatGrid>

        {canInvite && (
          <div className="grid gap-3 rounded-lg border border-line bg-surface p-3.5">
            <div className="grid grid-cols-[minmax(180px,1fr)_140px_auto] gap-2.5 max-[720px]:grid-cols-1">
              <input value={inviteUsername} onChange={(e) => setInviteUsername(e.target.value)} placeholder="GitHub username (optional)" className={fieldClass} />
              <select value={inviteRole} onChange={(e) => setInviteRole(e.target.value as Exclude<GroupRole, "owner">)} className={fieldClass}>
                <option value="member">Member</option>
                <option value="admin">Admin</option>
              </select>
              <button onClick={createInvite} className={buttonClass}>Create invite</button>
            </div>
            {inviteError && <p className="text-danger">{inviteError}</p>}
            {inviteUrl && (
              <div className="flex items-center justify-between gap-2 overflow-hidden rounded-lg border border-line bg-surface-secondary px-3 py-2.5 text-foreground">
                <code className="truncate">{inviteUrl}</code>
                <button onClick={copyInvite} aria-label="Copy invite link" className={buttonClass}>
                  {copied ? <CheckIcon size={16} /> : <CopyIcon size={16} />}
                  {copied ? "Copied" : "Copy"}
                </button>
              </div>
            )}
          </div>
        )}
      </section>

      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <TabBar<Period>
          aria-label="Period"
          size="sm"
          tabs={[
            { id: "all", label: "All time" },
            { id: "month", label: "Month" },
            { id: "week", label: "Week" },
          ]}
          activeTab={period}
          onTabChange={(value) => { setPeriod(value); setPage(1); }}
        />

        <div className="flex flex-wrap items-center gap-2 max-[560px]:w-full">
          <div className="flex h-[38px] flex-1 items-center gap-2 rounded-lg border border-line bg-surface px-2.5 max-[560px]:w-full">
            <SearchIcon size={16} />
            <input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search members" className="w-[160px] flex-1 border-0 bg-transparent text-sm text-foreground outline-none placeholder:text-muted" />
            {search && (
              <button onClick={() => setSearch("")} aria-label="Clear search" className="text-muted hover:text-foreground">
                <XIcon size={16} />
              </button>
            )}
          </div>
          <TabBar<SortBy>
            aria-label="Sort"
            size="sm"
            tabs={[
              { id: "tokens", label: "Tokens" },
              { id: "cost", label: "Cost" },
            ]}
            activeTab={sortBy}
            onTabChange={(value) => { setSortBy(value); setPage(1); }}
          />
        </div>
      </div>

      <div className="overflow-hidden rounded-xl border border-line bg-surface">
        {error ? (
          <div className="p-8 text-center text-muted">{error}</div>
        ) : isLoading ? (
          <div className="p-8 text-center text-muted">Loading leaderboard...</div>
        ) : sortedUsers.length === 0 ? (
          <div className="p-8 text-center text-muted">No submitted usage for this group yet.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[680px]">
              <thead>
                <tr>
                  <th className={thClass}>Rank</th>
                  <th className={thClass}>User</th>
                  <th className={thClass}>Role</th>
                  <th className={`${thClass} text-right`}>Cost</th>
                  <th className={`${thClass} text-right`}>Tokens</th>
                  {showSubmissionCount && <th className={`${thClass} text-right`}>Submits</th>}
                </tr>
              </thead>
              <tbody>
                {sortedUsers.map((user) => (
                  <GroupRow key={user.userId} user={user} showSubmissionCount={showSubmissionCount} />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}
