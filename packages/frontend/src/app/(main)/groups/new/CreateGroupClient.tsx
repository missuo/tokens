"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "nextjs-toploader/app";

export default function CreateGroupClient() {
  const router = useRouter();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [isPublic, setIsPublic] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSubmitting(true);
    setError(null);
    try {
      const response = await fetch("/api/groups", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name, description, isPublic }),
      });
      const payload = await response.json();
      if (!response.ok) throw new Error(payload.error || "Failed to create group");
      router.push(`/groups/${payload.slug}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create group");
      setIsSubmitting(false);
    }
  }

  const fieldClass = "min-h-10 rounded-lg border border-line bg-surface-secondary px-3 py-2 text-sm text-foreground outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/25";

  return (
    <section className="mx-auto max-w-[640px]">
      <h1 className="text-2xl font-bold tracking-tight text-foreground sm:text-[1.75rem]">Create group</h1>
      <p className="mt-1 mb-6 text-sm leading-relaxed text-muted">Start a scoped leaderboard and invite people by link or GitHub username.</p>

      <form onSubmit={handleSubmit} className="grid gap-4 rounded-xl border border-line bg-surface p-5">
        <label className="grid gap-2 text-sm font-semibold text-foreground">
          Group name
          <input value={name} onChange={(e) => setName(e.target.value)} maxLength={100} required autoFocus placeholder="e.g. My Team" className={fieldClass} />
        </label>
        <label className="grid gap-2 text-sm font-semibold text-foreground">
          Description <span className="font-normal text-muted">(optional)</span>
          <textarea value={description} onChange={(e) => setDescription(e.target.value)} maxLength={500} className={`${fieldClass} min-h-24 resize-y`} />
        </label>
        <label className="flex items-start gap-2.5 text-sm text-foreground">
          <input type="checkbox" checked={isPublic} onChange={(e) => setIsPublic(e.target.checked)} className="mt-0.5 accent-[var(--accent)]" />
          <span>
            Make this group public
            <span className="mt-0.5 block text-xs font-normal text-muted">Anyone can discover and join. Private groups are invite-only.</span>
          </span>
        </label>
        {error && <p className="text-sm text-danger">{error}</p>}
        <div className="mt-1 flex flex-wrap justify-end gap-2">
          <Link href="/leaderboard?view=groups" className="inline-flex h-10 items-center rounded-lg border border-line px-4 text-sm font-medium text-foreground transition hover:bg-surface-secondary">
            Cancel
          </Link>
          <button disabled={isSubmitting || !name.trim()} type="submit" className="h-10 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-foreground transition hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-65">
            {isSubmitting ? "Creating..." : "Create group"}
          </button>
        </div>
      </form>
    </section>
  );
}
