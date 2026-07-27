"use client";

import { useMemo, useState } from "react";
import { ChevronRightIcon, Code2Icon, Share2Icon } from "lucide-react";
import { toast } from "react-toastify";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { VerifiedBadge } from "@/components/ui/VerifiedBadge";
import { CONTAINER } from "@/components/layout/Container";
import { SourceLogo } from "@/components/SourceLogo";
import { ProfileSocialLinks } from "./ProfileSocialLinks";
import { cn } from "@/lib/utils";
import { formatCurrency, formatNumber } from "@/lib/format";
import type { ProfileSocialLink } from "./types";

export interface ProfileViewStats {
  totalTokens: number;
  totalCost: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  reasoningTokens?: number;
  submissionCount: number;
  activeDays: number;
  sessionCount: number;
}

export interface ProfileViewModel {
  name: string;
  tokens: number;
  cost: number;
}

export interface ProfileViewProps {
  user: {
    username: string;
    displayName: string | null;
    avatarUrl: string | null;
    createdAt: string;
    rank: number | null;
  };
  stats: ProfileViewStats;
  updatedAt: string | null;
  clients: string[];
  models: ProfileViewModel[];
  mcpServers?: string[];
  socialLinks?: ProfileSocialLink[];
  verified?: boolean;
  hasBackfill?: boolean;
  period: "all" | "month" | "week";
  onPeriodChange: (period: "all" | "month" | "week") => void;
  onEmbedClick: () => void;
  /**
   * The heavy sections are passed in rather than constructed here, so this
   * file stays a layout. Every one of them is rendered — the page lays its
   * sections out end to end instead of hiding them behind tabs, because on a
   * profile the whole point is seeing the picture at once.
   */
  activity: React.ReactNode;
  /** Today's figures. Sits directly under the headline totals. */
  today?: React.ReactNode;
  usageChart?: React.ReactNode;
  habits?: React.ReactNode;
  breakdown?: React.ReactNode;
  modelsSection?: React.ReactNode;
  devices?: React.ReactNode;
}

const PERIODS = [
  { value: "all", label: "All time" },
  { value: "month", label: "Month" },
  { value: "week", label: "Week" },
] as const;

function Stat({
  label,
  value,
  hint,
}: {
  label: string;
  value: string;
  hint?: string;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
        {label}
      </span>
      <span className="font-mono text-xl leading-none tabular-nums sm:text-2xl">
        {value}
      </span>
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </div>
  );
}

/**
 * Composition of the token total.
 *
 * Rendered as one proportional bar rather than five separate figures: the
 * useful question here is "how much of this is cache reads", which is a shape,
 * not a number. Exact values stay underneath for anyone who wants them.
 */
function TokenComposition({ stats }: { stats: ProfileViewStats }) {
  const parts = useMemo(
    () =>
      [
        { key: "Input", value: stats.inputTokens, className: "bg-chart-1" },
        { key: "Output", value: stats.outputTokens, className: "bg-chart-2" },
        { key: "Cache read", value: stats.cacheReadTokens, className: "bg-chart-3" },
        { key: "Cache write", value: stats.cacheWriteTokens, className: "bg-chart-4" },
        { key: "Reasoning", value: stats.reasoningTokens ?? 0, className: "bg-chart-5" },
      ].filter((p) => p.value > 0),
    [stats]
  );

  const total = parts.reduce((sum, p) => sum + p.value, 0);
  if (total === 0) return null;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex h-1.5 w-full overflow-hidden rounded-full">
        {parts.map((p) => (
          <div
            key={p.key}
            className={p.className}
            style={{ width: `${(p.value / total) * 100}%` }}
            title={`${p.key}: ${formatNumber(p.value, true)}`}
          />
        ))}
      </div>
      <dl className="flex flex-wrap gap-x-6 gap-y-2">
        {parts.map((p) => (
          <div key={p.key} className="flex items-center gap-2">
            <span className={cn("size-2 rounded-full", p.className)} aria-hidden="true" />
            <dt className="text-xs text-muted-foreground">{p.key}</dt>
            <dd className="font-mono text-xs tabular-nums">
              {formatNumber(p.value, true)}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold tracking-tight">{title}</h2>
      {children}
    </section>
  );
}

/**
 * A section that starts closed. Used for reference material — content worth
 * having but not worth the vertical space on arrival.
 */
function CollapsibleSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <details className="group flex flex-col gap-4">
      <summary className="flex cursor-pointer list-none items-center gap-2 text-sm font-semibold tracking-tight">
        <ChevronRightIcon className="size-4 text-muted-foreground transition-transform group-open:rotate-90" />
        {title}
      </summary>
      <div className="mt-4">{children}</div>
    </details>
  );
}

/** GitHub's mark. lucide has no brand icons, and a generic external-link arrow
 *  does not tell you where the button goes. */
function GitHubMark(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0016 8c0-4.42-3.58-8-8-8z" />
    </svg>
  );
}

export function ProfileView({
  user,
  stats,
  updatedAt,
  clients,
  models,
  mcpServers,
  socialLinks,
  verified,
  hasBackfill,
  period,
  onPeriodChange,
  onEmbedClick,
  activity,
  today,
  usageChart,
  habits,
  breakdown,
  modelsSection,
  devices,
}: ProfileViewProps) {
  const [copied, setCopied] = useState(false);
  const avatar = user.avatarUrl || `https://github.com/${user.username}.png`;
  const joined = new Date(user.createdAt).toLocaleDateString("en-US", {
    month: "short",
    year: "numeric",
  });

  const share = async () => {
    const url = `${window.location.origin}/u/${user.username}`;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      toast.success("Profile link copied");
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      toast.error("Could not copy the link");
    }
  };

  const topModels = models.slice(0, 12);

  return (
    <main id="main-content" className={cn(CONTAINER, "pb-24 pt-10 sm:pt-14")}>
      {/* ---- Identity -------------------------------------------------- */}
      <header className="flex flex-col gap-5 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex min-w-0 items-start gap-4">
          <Avatar className="size-16 shrink-0 sm:size-20">
            <AvatarImage src={avatar} alt="" />
            <AvatarFallback className="text-lg">
              {user.username.slice(0, 2).toUpperCase()}
            </AvatarFallback>
          </Avatar>

          <div className="flex min-w-0 flex-col gap-1.5">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="truncate text-xl font-semibold tracking-tight sm:text-2xl">
                {user.displayName || user.username}
              </h1>
              {verified && <VerifiedBadge size={15} />}
              {user.rank != null && (
                <Badge variant="secondary" className="font-mono tabular-nums">
                  Rank #{user.rank.toLocaleString("en-US")}
                </Badge>
              )}
              {hasBackfill && <Badge variant="outline">Includes imported history</Badge>}
            </div>

            <span className="truncate font-mono text-sm text-muted-foreground">
              @{user.username}
            </span>

            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
              {/* toLocaleDateString reads the runtime's locale and zone, so the
                  server and the browser can disagree — same reason as Updated
                  below. */}
              <span suppressHydrationWarning>Joined {joined}</span>
              {updatedAt && (
                <>
                  <span aria-hidden="true">·</span>
                  <span suppressHydrationWarning>
                    Updated {new Date(updatedAt).toLocaleDateString("en-US")}
                  </span>
                </>
              )}
            </div>

            {socialLinks && socialLinks.length > 0 && (
              <ProfileSocialLinks links={socialLinks} />
            )}
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" onClick={onEmbedClick}>
            <Code2Icon data-icon="inline-start" />
            Embed
          </Button>
          <Button variant="outline" size="sm" onClick={share}>
            <Share2Icon data-icon="inline-start" />
            {copied ? "Copied" : "Share"}
          </Button>
          {/* Outline, like Share: three actions sitting together read as one
              control group, and a borderless third one looks like a mistake. */}
          <Button
            variant="outline"
            size="sm"
            render={
              <a
                href={`https://github.com/${user.username}`}
                target="_blank"
                rel="noopener noreferrer"
              />
            }
          >
            <GitHubMark data-icon="inline-start" />
            GitHub
          </Button>
        </div>
      </header>

      <Separator className="my-7" />

      {/* ---- Headline figures ------------------------------------------ */}
      <section className="grid grid-cols-2 gap-6 sm:grid-cols-4" aria-label="Totals">
        <Stat label="Tokens" value={formatNumber(stats.totalTokens, true)} />
        <Stat label="Cost" value={formatCurrency(stats.totalCost, true)} />
        <Stat
          label="Active days"
          value={formatNumber(stats.activeDays, false)}
          hint={`${formatNumber(stats.sessionCount, false)} sessions`}
        />
        <Stat
          label="Submissions"
          value={formatNumber(stats.submissionCount, false)}
        />
      </section>

      {/* The composition bar belongs to the figures above it. Putting the day
          card between them made it read as the day's split, and left two
          unexplained bars on the page. */}
      <div className="mt-5">
        <TokenComposition stats={stats} />
      </div>

      {today && <div className="mt-8">{today}</div>}

      {/* ---- Sections ---------------------------------------------------- */}
      {/* Period applies to everything below it, so the control sits here
          rather than inside any one section. */}
      <div className="mt-9 flex items-center justify-between gap-3 border-b pb-3">
        <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
          Usage detail
        </span>
        {/* Same primitive and the same shape as the leaderboard's period
            control, so the two pages do not offer the identical choice through
            two different-looking widgets. */}
        <ToggleGroup
          value={[period]}
          onValueChange={(value) => {
            const next = value[0] as ProfileViewProps["period"] | undefined;
            if (next) onPeriodChange(next);
          }}
          variant="outline"
          aria-label="Period"
          className="[&>*]:h-8 [&>*]:px-2.5 [&>*]:text-xs"
        >
          {PERIODS.map((p) => (
            <ToggleGroupItem key={p.value} value={p.value}>
              {p.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      <div className="mt-8 flex flex-col gap-12">
        <Section title="Contributions">{activity}</Section>

        {usageChart && <Section title="Usage">{usageChart}</Section>}

        {breakdown && <Section title="Token breakdown">{breakdown}</Section>}

        {habits && <Section title="Habits">{habits}</Section>}

        <CollapsibleSection title="Models">
          {modelsSection ?? (
            topModels.length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">
                No model usage recorded for this period.
              </p>
            ) : (
              <div className="overflow-hidden rounded-lg border">
                <Table>
                  <TableHeader>
                    <TableRow className="hover:bg-transparent">
                      <TableHead className="pl-4 sm:pl-6">Model</TableHead>
                      <TableHead className="text-right">Tokens</TableHead>
                      <TableHead className="pr-4 text-right sm:pr-6">Cost</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {topModels.map((model) => (
                      <TableRow key={model.name}>
                        <TableCell className="pl-4 font-mono text-sm sm:pl-6">
                          {model.name}
                        </TableCell>
                        <TableCell className="text-right font-mono text-sm tabular-nums">
                          {formatNumber(model.tokens, true)}
                        </TableCell>
                        <TableCell className="pr-4 text-right font-mono text-sm tabular-nums sm:pr-6">
                          {formatCurrency(model.cost, true)}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            )
          )}
        </CollapsibleSection>

        <Section title="Clients">
          <div className="flex flex-col gap-6">
            <div className="flex flex-wrap gap-2">
              {clients.length === 0 ? (
                <span className="text-sm text-muted-foreground">None recorded.</span>
              ) : (
                clients.map((client) => (
                  <span
                    key={client}
                    className="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-sm"
                  >
                    <SourceLogo sourceId={client} height={14} decorative />
                    {client}
                  </span>
                ))
              )}
            </div>

            {mcpServers && mcpServers.length > 0 && (
              <div>
                <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                  MCP servers
                </span>
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {mcpServers.map((server) => (
                    <Badge key={server} variant="outline" className="font-mono">
                      {server}
                    </Badge>
                  ))}
                </div>
              </div>
            )}
          </div>
        </Section>

        {/* ProfileDevices carries its own "Devices · all-time" heading, so no
            Section wrapper here — one would give the block two headings, and an
            empty device list (ProfileDevices returns null) a bare one. */}
        {devices}
      </div>
    </main>
  );
}
