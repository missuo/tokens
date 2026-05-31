"use client";

import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { toast } from "react-toastify";
import { EMBED_TEMPLATES, type EmbedTemplate } from "@/lib/embed/embedShared";
import { getPaletteNames, getPalette, type ColorPaletteName } from "@/lib/themes";

type EmbedTheme = "dark" | "light";
type EmbedSortBy = "tokens" | "cost";
type EmbedView = "2d" | "3d";
type EmbedNumberFormat = "compact" | "full";
type EmbedRankFormat = "plain" | "percent" | "total";

function titleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

interface ProfileEmbedDialogProps {
  open: boolean;
  username: string;
  displayName?: string | null;
  onClose: () => void;
}

// Absolute base for shareable snippets (README image links must resolve publicly).
const SITE_URL = process.env.NEXT_PUBLIC_URL || "https://tokens.ci";

function Segmented<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { id: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {options.map((opt) => {
        const active = value === opt.id;
        return (
          <button
            key={opt.id}
            type="button"
            onClick={() => onChange(opt.id)}
            className={`inline-flex min-h-10 items-center justify-center rounded-full border px-3.5 py-2.5 text-sm font-semibold transition hover:-translate-y-px ${
              active ? "border-[rgba(133,202,255,0.24)] bg-gradient-to-br from-[rgba(22,154,255,0.18)] to-[rgba(133,202,255,0.1)] text-foreground" : "border-line bg-[var(--surface-tertiary)] text-muted hover:text-foreground"
            }`}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}

function OptionGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2.5 rounded-2xl border border-line bg-surface/70 p-4">
      <span className="text-sm font-semibold text-foreground">{label}</span>
      {children}
    </div>
  );
}

export function ProfileEmbedDialog({ open, username, displayName, onClose }: ProfileEmbedDialogProps) {
  const [theme, setTheme] = useState<EmbedTheme>("dark");
  const [sortBy, setSortBy] = useState<EmbedSortBy>("tokens");
  const [compact, setCompact] = useState(false);
  const [view, setView] = useState<EmbedView>("2d");
  const [template, setTemplate] = useState<EmbedTemplate>("classic");
  const [color, setColor] = useState<ColorPaletteName | null>(null);
  const [tokensFormat, setTokensFormat] = useState<EmbedNumberFormat>("compact");
  const [costFormat, setCostFormat] = useState<EmbedNumberFormat>("compact");
  const [rankFormat, setRankFormat] = useState<EmbedRankFormat>("plain");
  const [graph, setGraph] = useState(false);

  const graphCapable = template !== "graph" && template !== "vitals";

  useEffect(() => {
    if (!open) return;
    const previousOverflow = document.body.style.overflow;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [open, onClose]);

  const { embedUrl, previewSrc, markdownSnippet, htmlSnippet, profileUrl } = useMemo(() => {
    const params = new URLSearchParams();
    if (view === "3d") params.set("view", "3d");
    if (theme !== "dark") params.set("theme", theme);
    if (sortBy !== "tokens") params.set("sort", sortBy);
    if (template !== "classic") params.set("template", template);
    if (color) params.set("color", color);
    if (template === "classic" && compact) params.set("compact", "1");
    if (graph && graphCapable) params.set("graph", "1");
    if (rankFormat !== "plain") params.set("rank", rankFormat);
    params.set("tokens", tokensFormat);
    params.set("cost", costFormat);

    const query = params.toString();
    const path = `/api/embed/${username}/svg${query ? `?${query}` : ""}`;
    const resolvedEmbedUrl = `${SITE_URL}${path}`;
    const resolvedProfileUrl = `${SITE_URL}/u/${username}`;

    return {
      embedUrl: resolvedEmbedUrl,
      previewSrc: path,
      markdownSnippet: `[![Tokens Stats](${resolvedEmbedUrl})](${resolvedProfileUrl})`,
      htmlSnippet: `<a href="${resolvedProfileUrl}"><img alt="Tokens Stats for @${username}" src="${resolvedEmbedUrl}" /></a>`,
      profileUrl: resolvedProfileUrl,
    };
  }, [color, compact, costFormat, graph, graphCapable, rankFormat, sortBy, template, theme, tokensFormat, username, view]);

  const copyToClipboard = async (value: string, label: string) => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`${label} copied`);
    } catch {
      toast.error(`Failed to copy ${label.toLowerCase()}`);
    }
  };

  if (!open || typeof document === "undefined") return null;

  return createPortal(
    <div
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/80 p-6 backdrop-blur-lg max-[640px]:items-end max-[640px]:p-3"
    >
      <div role="dialog" aria-modal="true" aria-labelledby="profile-embed-dialog-title" className="max-h-[88vh] w-full max-w-[1040px] overflow-auto rounded-[28px] border border-[rgba(133,202,255,0.16)] bg-surface shadow-2xl max-[640px]:rounded-t-3xl max-[640px]:rounded-b-none">
        <div className="flex items-start justify-between gap-4 px-7 pt-7 max-[640px]:px-[18px] max-[640px]:pt-[22px]">
          <div className="flex min-w-0 flex-col gap-2.5">
            <span className="w-fit rounded-full border border-[rgba(133,202,255,0.18)] bg-[rgba(133,202,255,0.08)] px-3 py-1.5 text-xs font-bold tracking-widest text-[var(--color-accent-blue)] uppercase">
              GitHub README embed
            </span>
            <h2 id="profile-embed-dialog-title" className="text-[clamp(28px,3vw,38px)] leading-tight font-bold tracking-tight text-foreground">
              Share @{username} with a polished Tokens card
            </h2>
            <p className="max-w-[720px] text-[15px] leading-relaxed text-muted">
              Preview the live embed, tweak the presentation, and copy a ready-to-paste snippet for your README.
            </p>
          </div>

          <button type="button" onClick={onClose} aria-label="Close embed dialog" className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full border border-line bg-[var(--surface-tertiary)] text-foreground transition hover:-translate-y-px">
            <svg aria-hidden="true" width="20" height="20" viewBox="0 0 24 24" fill="none">
              <path d="M18 6L6 18" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
              <path d="M6 6L18 18" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
          </button>
        </div>

        <div className="grid grid-cols-[minmax(0,1.1fr)_minmax(320px,420px)] gap-6 px-7 pt-7 pb-10 max-[920px]:grid-cols-1 max-[640px]:gap-[18px] max-[640px]:px-[18px] max-[640px]:pt-[18px] max-[640px]:pb-8">
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-4 rounded-3xl border border-[rgba(133,202,255,0.12)] bg-background p-5">
              <span className="text-xs font-semibold tracking-wider text-muted uppercase">Live preview</span>
              <div className="flex min-h-[360px] items-center justify-center rounded-[18px] border border-dashed border-[rgba(133,202,255,0.16)] bg-white/[0.02] p-6 max-[640px]:min-h-[220px] max-[640px]:p-4">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img src={previewSrc} alt={`Tokens README embed preview for ${displayName || username}`} className="h-auto w-full max-w-full drop-shadow-2xl" />
              </div>
            </div>

            <ul className="grid list-none gap-2.5 p-0">
              {["GitHub-ready markdown with a linked image card", "Matches the Tokens visual language", "Automatically refreshes as profile stats update"].map((t) => (
                <li key={t} className="relative pl-[18px] text-sm leading-relaxed text-muted before:absolute before:top-2 before:left-0 before:h-[7px] before:w-[7px] before:rounded-full before:bg-gradient-to-br before:from-[#169aff] before:to-[#85caff]">
                  {t}
                </li>
              ))}
            </ul>
          </div>

          <div className="flex flex-col gap-4">
            <OptionGroup label="Template">
              <Segmented options={EMBED_TEMPLATES.map((tpl) => ({ id: tpl, label: titleCase(tpl) }))} value={template} onChange={setTemplate} />
            </OptionGroup>

            <OptionGroup label="View">
              <Segmented options={[{ id: "2d", label: "2D" }, { id: "3d", label: "3D" }]} value={view} onChange={setView} />
            </OptionGroup>

            <OptionGroup label="Theme">
              <Segmented options={[{ id: "dark", label: "Dark" }, { id: "light", label: "Light" }]} value={theme} onChange={setTheme} />
            </OptionGroup>

            <OptionGroup label="Accent color">
              <div className="flex flex-wrap gap-2.5">
                <button
                  type="button"
                  aria-label="Default accent color"
                  title="Default"
                  onClick={() => setColor(null)}
                  className="h-[34px] w-[34px] rounded-full ring-1 ring-line transition hover:-translate-y-px"
                  style={{ background: "var(--border)", border: color === null ? "2px solid var(--foreground)" : "2px solid transparent" }}
                />
                {getPaletteNames().map((name) => (
                  <button
                    key={name}
                    type="button"
                    aria-label={`${name} accent color`}
                    title={titleCase(name)}
                    onClick={() => setColor(name)}
                    className="h-[34px] w-[34px] rounded-full ring-1 ring-line transition hover:-translate-y-px"
                    style={{ background: getPalette(name).grade3, border: color === name ? "2px solid var(--foreground)" : "2px solid transparent" }}
                  />
                ))}
              </div>
            </OptionGroup>

            <OptionGroup label="Ranking">
              <Segmented options={[{ id: "tokens", label: "Tokens" }, { id: "cost", label: "Cost" }]} value={sortBy} onChange={setSortBy} />
            </OptionGroup>

            <OptionGroup label="Rank format">
              <Segmented options={(["plain", "percent", "total"] as const).map((m) => ({ id: m, label: titleCase(m) }))} value={rankFormat} onChange={setRankFormat} />
            </OptionGroup>

            {template === "classic" && (
              <OptionGroup label="Layout">
                <Segmented options={[{ id: "full", label: "Full" }, { id: "compact", label: "Compact" }]} value={compact ? "compact" : "full"} onChange={(v) => setCompact(v === "compact")} />
              </OptionGroup>
            )}

            {graphCapable && (
              <OptionGroup label="Contribution graph">
                <Segmented options={[{ id: "off", label: "Off" }, { id: "on", label: "On" }]} value={graph ? "on" : "off"} onChange={(v) => setGraph(v === "on")} />
              </OptionGroup>
            )}

            <OptionGroup label="Token number format">
              <Segmented options={[{ id: "compact", label: "Compact" }, { id: "full", label: "Full" }]} value={tokensFormat} onChange={setTokensFormat} />
            </OptionGroup>

            <OptionGroup label="Cost number format">
              <Segmented options={[{ id: "compact", label: "Compact" }, { id: "full", label: "Full" }]} value={costFormat} onChange={setCostFormat} />
            </OptionGroup>

            <div className="flex flex-col gap-3.5 rounded-3xl border border-[rgba(133,202,255,0.16)] bg-surface p-5">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <h3 className="text-[15px] font-bold text-foreground">Markdown snippet</h3>
                <div className="flex flex-wrap gap-2">
                  <button type="button" onClick={() => copyToClipboard(embedUrl, "Image URL")} className="inline-flex min-h-[34px] items-center justify-center rounded-full border border-line bg-[var(--surface-tertiary)] px-3 py-2 text-[13px] font-semibold text-muted transition hover:text-foreground">
                    Copy image URL
                  </button>
                  <button type="button" onClick={() => copyToClipboard(htmlSnippet, "HTML snippet")} className="inline-flex min-h-[34px] items-center justify-center rounded-full border border-line bg-[var(--surface-tertiary)] px-3 py-2 text-[13px] font-semibold text-muted transition hover:text-foreground">
                    Copy HTML
                  </button>
                </div>
              </div>

              <pre className="overflow-auto rounded-[18px] border border-white/5 bg-black/40 p-4 font-mono text-[13px] leading-relaxed break-words whitespace-pre-wrap text-[#d9edff]">{markdownSnippet}</pre>

              <div className="flex flex-wrap gap-2.5">
                <button type="button" onClick={() => copyToClipboard(markdownSnippet, "Markdown snippet")} className="inline-flex min-h-11 items-center justify-center rounded-full border border-[rgba(133,202,255,0.26)] bg-gradient-to-br from-[#169aff] to-[#0073ff] px-4 py-3 text-sm font-bold text-white transition hover:brightness-105">
                  Copy markdown
                </button>
                <a href={profileUrl} target="_blank" rel="noopener noreferrer" className="inline-flex min-h-11 items-center justify-center rounded-full border border-line bg-[var(--surface-tertiary)] px-4 py-3 text-sm font-semibold text-foreground transition hover:-translate-y-px">
                  View profile
                </a>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
