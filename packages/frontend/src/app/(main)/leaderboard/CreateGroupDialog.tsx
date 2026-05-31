"use client";

import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useRouter } from "nextjs-toploader/app";
import { toast } from "react-toastify";

// Two-step "New group" wizard rendered as a modal (no page navigation):
//   Step 1 — Details: name / description / visibility -> POST /api/groups
//   Step 2 — Invite:  auto-generates a shareable invite link to copy.
// Same-origin fetches let the browser set the Origin header automatically, so
// the CSRF origin gate on the mutating routes is satisfied without extra work.

interface CreatedGroup {
  slug: string;
  name: string;
}

interface CreateGroupDialogProps {
  open: boolean;
  onClose: () => void;
  /** Called right after a group is created so the list behind the modal refreshes. */
  onCreated?: () => void;
}

const fieldClass =
  "min-h-10 w-full rounded-lg border border-line bg-surface-secondary px-3 text-foreground outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/30";

export function CreateGroupDialog({ open, onClose, onCreated }: CreateGroupDialogProps) {
  const router = useRouter();
  const [step, setStep] = useState<1 | 2>(1);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [isPublic, setIsPublic] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<CreatedGroup | null>(null);
  const [inviteUrl, setInviteUrl] = useState("");
  const [inviteState, setInviteState] = useState<"idle" | "loading" | "error">("idle");
  const [copied, setCopied] = useState(false);

  // Reset all state once the close animation/unmount settles.
  useEffect(() => {
    if (open) return;
    const t = setTimeout(() => {
      setStep(1);
      setName("");
      setDescription("");
      setIsPublic(false);
      setBusy(false);
      setError(null);
      setCreated(null);
      setInviteUrl("");
      setInviteState("idle");
      setCopied(false);
    }, 200);
    return () => clearTimeout(t);
  }, [open]);

  // Escape closes the dialog.
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  async function generateInvite(slug: string) {
    setInviteState("loading");
    try {
      const res = await fetch(`/api/groups/${slug}/invite`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ role: "member" }),
      });
      const payload = await res.json();
      if (!res.ok || !payload.joinUrl) throw new Error(payload.error || "Failed");
      setInviteUrl(`${window.location.origin}${payload.joinUrl}`);
      setInviteState("idle");
    } catch {
      setInviteState("error");
    }
  }

  async function handleCreate(event: React.FormEvent) {
    event.preventDefault();
    if (busy || !name.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const res = await fetch("/api/groups", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: name.trim(), description, isPublic }),
      });
      const payload = await res.json();
      if (!res.ok) throw new Error(payload.error || "Failed to create group");
      setCreated({ slug: payload.slug, name: payload.name ?? name.trim() });
      onCreated?.();
      setStep(2);
      void generateInvite(payload.slug);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create group");
    } finally {
      setBusy(false);
    }
  }

  async function copyInvite() {
    try {
      await navigator.clipboard.writeText(inviteUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error("Failed to copy link");
    }
  }

  function finish(navigate: boolean) {
    const slug = created?.slug;
    onClose();
    if (navigate && slug) router.push(`/groups/${slug}`);
  }

  if (!open || typeof document === "undefined") return null;

  return createPortal(
    <div
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm max-[560px]:items-end max-[560px]:p-0"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-group-title"
        className="w-full max-w-[520px] overflow-hidden rounded-2xl border border-line bg-surface shadow-2xl max-[560px]:max-w-none max-[560px]:rounded-b-none"
      >
        <div className="flex items-start justify-between gap-3 border-b border-line px-5 py-4">
          <div className="min-w-0">
            <h2 id="create-group-title" className="text-lg font-bold text-foreground">
              {step === 1 ? "New group" : "Invite people"}
            </h2>
            <p className="mt-0.5 truncate text-xs text-muted">
              Step {step} of 2 · {step === 1 ? "Details" : created?.name}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-muted transition hover:bg-foreground/5 hover:text-foreground"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
              <path d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.749.749 0 0 1 1.06 1.06L9.06 8l3.22 3.22a.749.749 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.749.749 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z" />
            </svg>
          </button>
        </div>

        {step === 1 ? (
          <form onSubmit={handleCreate} className="grid gap-4 p-5">
            <label className="grid gap-1.5 text-sm font-semibold text-foreground">
              Group name
              <input value={name} onChange={(e) => setName(e.target.value)} maxLength={100} required autoFocus placeholder="e.g. My Team" className={fieldClass} />
            </label>
            <label className="grid gap-1.5 text-sm font-semibold text-foreground">
              Description <span className="font-normal text-muted">(optional)</span>
              <textarea value={description} onChange={(e) => setDescription(e.target.value)} maxLength={500} className={`${fieldClass} min-h-20 resize-y py-2.5`} />
            </label>
            <label className="flex items-start gap-2.5 text-sm text-foreground">
              <input type="checkbox" checked={isPublic} onChange={(e) => setIsPublic(e.target.checked)} className="mt-0.5 accent-[var(--accent)]" />
              <span>
                Make this group public
                <span className="mt-0.5 block text-xs font-normal text-muted">Anyone can discover and join. Private groups are invite-only.</span>
              </span>
            </label>
            {error && <p className="text-sm text-danger">{error}</p>}
            <div className="mt-1 flex justify-end gap-2">
              <button type="button" onClick={onClose} className="min-h-10 rounded-lg border border-line px-4 text-sm font-semibold text-foreground transition hover:bg-foreground/5">
                Cancel
              </button>
              <button type="submit" disabled={busy || !name.trim()} className="min-h-10 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-foreground transition hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-60">
                {busy ? "Creating…" : "Next →"}
              </button>
            </div>
          </form>
        ) : (
          <div className="grid gap-4 p-5">
            <p className="text-sm text-muted">
              <span className="font-semibold text-foreground">{created?.name}</span> is ready. Share this link to invite people — or skip and manage members later from the group page.
            </p>

            <div className="flex items-center gap-2 rounded-lg border border-line bg-surface-secondary py-1.5 pr-1.5 pl-3">
              <input
                readOnly
                value={inviteState === "loading" ? "Generating invite link…" : inviteState === "error" ? "Couldn't generate a link" : inviteUrl}
                onFocus={(e) => e.currentTarget.select()}
                className="min-w-0 flex-1 bg-transparent font-mono text-sm text-foreground outline-none"
              />
              {inviteState === "error" ? (
                <button type="button" onClick={() => created && generateInvite(created.slug)} className="shrink-0 rounded-md border border-line px-3 py-1.5 text-sm font-semibold text-foreground transition hover:bg-foreground/5">
                  Retry
                </button>
              ) : (
                <button type="button" onClick={copyInvite} disabled={!inviteUrl} className="shrink-0 rounded-md bg-accent px-3 py-1.5 text-sm font-semibold text-accent-foreground transition hover:opacity-90 disabled:opacity-50">
                  {copied ? "Copied!" : "Copy"}
                </button>
              )}
            </div>

            <div className="mt-1 flex justify-end gap-2">
              <button type="button" onClick={() => finish(false)} className="min-h-10 rounded-lg border border-line px-4 text-sm font-semibold text-foreground transition hover:bg-foreground/5">
                Done
              </button>
              <button type="button" onClick={() => finish(true)} className="min-h-10 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-foreground transition hover:opacity-90">
                Open group →
              </button>
            </div>
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
