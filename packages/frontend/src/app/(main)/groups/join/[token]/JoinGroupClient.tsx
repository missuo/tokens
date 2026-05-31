"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { useRouter } from "nextjs-toploader/app";

interface InvitePreview {
  group: { name: string; slug: string; isPublic: boolean };
  role: "admin" | "member";
  invitedUsername: string | null;
  expiresAt: string;
}

function formatRole(role: InvitePreview["role"]): string {
  return role.charAt(0).toUpperCase() + role.slice(1);
}

const shellClass = "mx-auto mt-10 max-w-[560px] rounded-xl border border-line bg-surface p-6";
const secondaryLinkClass = "inline-flex h-10 items-center rounded-lg border border-line px-4 text-sm font-medium text-foreground transition hover:bg-surface-secondary";
const primaryBtnClass = "inline-flex h-10 items-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-foreground transition hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-65";

export default function JoinGroupClient({ token }: { token: string }) {
  const router = useRouter();
  const [preview, setPreview] = useState<InvitePreview | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isJoining, setIsJoining] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const abortController = new AbortController();
    fetch(`/api/groups/join/${token}`, { signal: abortController.signal })
      .then((response) => {
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        return response.json();
      })
      .then(setPreview)
      .catch((err) => {
        if (err.name !== "AbortError") setError("This invite is invalid or expired.");
      })
      .finally(() => {
        if (!abortController.signal.aborted) setIsLoading(false);
      });
    return () => abortController.abort();
  }, [token]);

  async function acceptInvite() {
    setIsJoining(true);
    setError(null);
    try {
      const response = await fetch(`/api/groups/join/${token}`, { method: "POST" });
      const payload = await response.json();
      if (response.status === 401) {
        window.location.href = `/api/auth/github?returnTo=/groups/join/${token}`;
        return;
      }
      if (!response.ok) throw new Error(payload.error || "Failed to join group");
      router.push(`/groups/${payload.group.slug}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to join group");
      setIsJoining(false);
    }
  }

  if (isLoading) {
    return (
      <section className={shellClass}>
        <p className="text-muted">Loading invite...</p>
      </section>
    );
  }

  if (!preview) {
    return (
      <section className={shellClass}>
        <h1 className="mb-2 text-2xl font-bold tracking-tight text-foreground">Invite unavailable</h1>
        <p className="text-sm text-danger">{error || "This invite is invalid or expired."}</p>
        <div className="mt-5 flex flex-wrap gap-2">
          <Link href="/leaderboard?view=groups" className={secondaryLinkClass}>Browse groups</Link>
        </div>
      </section>
    );
  }

  return (
    <section className={shellClass}>
      <h1 className="text-2xl font-bold tracking-tight text-foreground">Join {preview.group.name}</h1>
      <p className="mt-1 text-sm leading-relaxed text-muted">You were invited to join this group leaderboard.</p>
      <dl className="my-5 grid gap-2.5 rounded-lg border border-line bg-surface-secondary p-4 text-sm">
        <div className="flex justify-between gap-3"><dt className="text-muted">Role</dt><dd className="font-medium text-foreground capitalize">{formatRole(preview.role)}</dd></div>
        <div className="flex justify-between gap-3"><dt className="text-muted">Visibility</dt><dd className="font-medium text-foreground">{preview.group.isPublic ? "Public" : "Private"}</dd></div>
        {preview.invitedUsername && <div className="flex justify-between gap-3"><dt className="text-muted">For</dt><dd className="font-mono font-medium text-foreground">@{preview.invitedUsername}</dd></div>}
      </dl>
      {error && <p className="mb-2 text-sm text-danger">{error}</p>}
      <div className="flex flex-wrap gap-2">
        <button onClick={acceptInvite} disabled={isJoining} className={primaryBtnClass}>
          {isJoining ? "Joining..." : "Join group"}
        </button>
        <Link href="/leaderboard?view=groups" className={secondaryLinkClass}>Cancel</Link>
      </div>
    </section>
  );
}
