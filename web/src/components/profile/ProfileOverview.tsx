"use client";

import { useMemo, useState, useSyncExternalStore } from "react";
import Image from "next/image";
import { toast } from "react-toastify";
import { formatCurrency, formatNumber } from "@/lib/utils";
import { Code2Icon, Share2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { VerifiedBadge } from "@/components/ui/VerifiedBadge";
import { ProfileEmbedDialog } from "./ProfileEmbedDialog";
import { ProfileSocialLinks } from "./ProfileSocialLinks";
import type { ProfileSocialLink, ProfileStatsData, ProfileUser } from "./types";
import { tw } from "@/lib/tw";
import { cn } from "@/lib/utils";

export interface ProfileOverviewProps {
  user: ProfileUser;
  stats: ProfileStatsData;
  lastUpdated?: string | null;
  period?: "all" | "month" | "week";
  socialLinks?: ProfileSocialLink[];
  verified?: boolean;
  className?: string;
}

// The metrics row reflows on the panel's own width, not the viewport's, so
// the panel is the container and the grid keys off @[40rem].
const OverviewPanel = tw(
  "section",
  "@container overflow-hidden rounded-xl border bg-card text-foreground"
);

const OverviewHeader = tw(
  "div",
  "flex flex-wrap items-start justify-between gap-x-5 gap-y-3.5 px-4 py-3.5 sm:px-[1.125rem] sm:py-4"
);

const Identity = tw("div", "flex min-w-0 flex-[1_1_19rem] items-center gap-4");
const AvatarShell = tw("div", "relative flex-none");

const Avatar = tw(
  "div",
  "relative size-[72px] flex-none overflow-hidden rounded-xl border border-muted-foreground/30 bg-muted sm:size-20"
);

const AvatarVerifiedBadge = ({ className, ...props }: React.ComponentProps<typeof VerifiedBadge>) => (
  <VerifiedBadge {...props} className={cn("absolute -bottom-1 -right-1", className)} />
);

const AvatarImage = ({ className, ...props }: React.ComponentProps<typeof Image>) => (
  <Image {...props} className={cn("object-cover", className)} />
);

const IdentityCopy = tw("div", "min-w-0");

const DisplayName = tw(
  "h1",
  "m-0 overflow-hidden text-ellipsis whitespace-nowrap text-[clamp(1.125rem,4vw,1.375rem)] font-semibold leading-tight text-foreground"
);

const Handle = tw(
  "p",
  "m-0 mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-sm leading-tight text-muted-foreground"
);

const Metadata = tw(
  "ul",
  "mb-0 mt-1.5 flex list-none flex-wrap items-center gap-x-2 gap-y-1 p-0 text-[0.8125rem] leading-snug text-muted-foreground"
);

// Every item after the first is preceded by a 2px dot, so the separators live
// with the items rather than being spliced into the markup.
const MetadataItem = tw(
  "li",
  "inline-flex items-center gap-2 [&:not(:first-child)]:before:size-0.5 [&:not(:first-child)]:before:rounded-full [&:not(:first-child)]:before:bg-muted-foreground/40 [&:not(:first-child)]:before:content-['']"
);

const RankItem = tw(
  "li",
  "inline-flex items-baseline gap-1 rounded-md border border-[color-mix(in_srgb,var(--primary)_42%,transparent)] bg-primary/10 px-[0.4375rem] py-[0.1875rem] text-xs font-semibold leading-tight text-foreground [&_strong]:font-bold [&_strong]:text-primary"
);

const Metrics = tw(
  "dl",
  "m-0 grid grid-cols-2 border-t text-left @[40rem]:grid-cols-[repeat(4,9.25rem)] @[40rem]:justify-start"
);

// 2x2 on a narrow panel: the right column takes a left edge and the second row
// takes a top edge. Once the four fit on one line, every cell but the first
// takes a left edge and the row rule goes away.
const Metric = tw(
  "div",
  "flex min-w-0 flex-col items-start px-4 py-[0.6875rem] text-left [&:nth-child(even)]:border-l [&:nth-child(even)]:border-border [&:nth-child(n+3)]:border-t [&:nth-child(n+3)]:border-border @[40rem]:px-5 @[40rem]:[&:not(:first-child)]:border-l @[40rem]:[&:nth-child(n+3)]:border-t-0"
);

const MetricLabel = tw(
  "dt",
  "w-full overflow-hidden text-ellipsis whitespace-nowrap text-left text-xs leading-tight text-muted-foreground"
);

const MetricValue = tw(
  "dd",
  "m-0 mt-1.5 w-full overflow-hidden text-ellipsis whitespace-nowrap text-left text-[clamp(1.05rem,4vw,1.35rem)] font-semibold leading-none text-foreground [font-variant-numeric:tabular-nums]"
);

const subscribeNoop = () => () => {};

/** Keep server and first-hydration output in UTC, then use the viewer's zone. */
export function formatLastUpdated(
  lastUpdated: string | null | undefined,
  isMounted: boolean,
): string | null {
  if (!lastUpdated) return null;
  const date = new Date(lastUpdated);
  return isMounted
    ? date.toLocaleString("en-US")
    : date.toLocaleString("en-US", { timeZone: "UTC" });
}

function formatJoined(createdAt: string | null | undefined): string | null {
  if (!createdAt) return null;
  const date = new Date(createdAt);
  if (Number.isNaN(date.getTime())) return null;

  return date.toLocaleDateString("en-US", {
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  });
}

function GitHubIcon() {
  return (
    <svg
      aria-hidden="true"
      width="16"
      height="16"
      viewBox="0 0 20 20"
      fill="none"
    >
      <path
        d="M12.5 18.3333V15C12.6159 13.9561 12.3166 12.9084 11.6666 12.0833C14.1666 12.0833 16.6666 10.4167 16.6666 7.49999C16.7333 6.45832 16.4416 5.43332 15.8333 4.58332C16.0666 3.62499 16.0666 2.62499 15.8333 1.66666C15.8333 1.66666 15 1.66666 13.3333 2.91666C11.1333 2.49999 8.86663 2.49999 6.66663 2.91666C4.99996 1.66666 4.16663 1.66666 4.16663 1.66666C3.91663 2.62499 3.91663 3.62499 4.16663 4.58332C3.55985 5.42989 3.26535 6.46065 3.33329 7.49999C3.33329 10.4167 5.83329 12.0833 8.33329 12.0833C8.00829 12.4917 7.76663 12.9583 7.62496 13.4583C7.48329 13.9583 7.44163 14.4833 7.49996 15M7.49996 15V18.3333M7.49996 15C3.74163 16.6667 3.33329 13.3333 1.66663 13.3333"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.667"
      />
    </svg>
  );
}

export function ProfileOverview({
  user,
  stats,
  lastUpdated,
  period = "all",
  socialLinks,
  verified = false,
  className,
}: ProfileOverviewProps) {
  const [isEmbedDialogOpen, setIsEmbedDialogOpen] = useState(false);
  const isMounted = useSyncExternalStore(
    subscribeNoop,
    () => true,
    () => false,
  );
  const formattedLastUpdated = useMemo(
    () => formatLastUpdated(lastUpdated, isMounted),
    [isMounted, lastUpdated],
  );
  const joined = useMemo(() => formatJoined(user.createdAt), [user.createdAt]);
  const displayName = user.displayName || user.username;
  const avatarUrl = user.avatarUrl || `https://github.com/${user.username}.png`;

  const handleShareClick = async () => {
    try {
      await navigator.clipboard.writeText(window.location.href);
      toast.success("Link copied to clipboard!");
    } catch {
      toast.error("Failed to copy link");
    }
  };

  const headlineMetrics = [
    {
      label:
        period === "all"
          ? "All-time tokens"
          : period === "month"
            ? "30d tokens"
            : "7d tokens",
      value: formatNumber(stats.totalTokens),
      title: stats.totalTokens.toLocaleString("en-US"),
    },
    {
      label:
        period === "all"
          ? "All-time cost"
          : period === "month"
            ? "30d cost"
            : "7d cost",
      value: formatCurrency(stats.totalCost),
      title: stats.totalCost.toLocaleString("en-US", {
        style: "currency",
        currency: "USD",
      }),
    },
    {
      label:
        period === "all"
          ? "Active days (1y)"
          : period === "month"
            ? "Active days (30d)"
            : "Active days (7d)",
      value: stats.activeDays.toLocaleString("en-US"),
      title: stats.activeDays.toLocaleString("en-US"),
    },
    {
      label: "All submissions",
      value: (stats.submissionCount ?? 0).toLocaleString("en-US"),
      title: (stats.submissionCount ?? 0).toLocaleString("en-US"),
    },
  ];

  return (
    <OverviewPanel
      className={className}
      aria-labelledby="profile-overview-heading"
    >
      <OverviewHeader>
        <Identity>
          <AvatarShell>
            <Avatar>
              <AvatarImage
                src={avatarUrl}
                alt={`${displayName}'s avatar`}
                fill
                sizes="(min-width: 640px) 80px, 72px"
                priority
              />
            </Avatar>
            {verified && <AvatarVerifiedBadge size={22} />}
          </AvatarShell>

          <IdentityCopy>
            <DisplayName id="profile-overview-heading">
              {displayName}
            </DisplayName>
            <Handle>@{user.username}</Handle>

            {(user.rank != null || joined || formattedLastUpdated) && (
              <Metadata aria-label="Profile details">
                {user.rank != null && (
                  <RankItem>
                    <span>Rank</span>
                    <strong>#{user.rank.toLocaleString("en-US")}</strong>
                  </RankItem>
                )}
                {joined && <MetadataItem>Joined {joined}</MetadataItem>}
                {formattedLastUpdated && (
                  <MetadataItem suppressHydrationWarning>
                    Updated {formattedLastUpdated}
                  </MetadataItem>
                )}
              </Metadata>
            )}

            {socialLinks && socialLinks.length > 0 && (
              <ProfileSocialLinks links={socialLinks} />
            )}
          </IdentityCopy>
        </Identity>

        {/* Buttons come from the component library so foreground and
            background always travel together; the previous hand-styled pair
            left blue-on-black and grey-on-black combinations behind when the
            palette moved. All three carry an icon so the row reads as one set. */}
        <div className="flex flex-wrap items-center gap-2" aria-label="Profile actions">
          <Button
            type="button"
            size="sm"
            onClick={() => setIsEmbedDialogOpen(true)}
            aria-label={`Open GitHub README embed options for ${displayName}`}
          >
            <Code2Icon data-icon="inline-start" />
            Embed
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleShareClick}
            aria-label={`Share ${displayName}'s profile`}
          >
            <Share2Icon data-icon="inline-start" />
            Share
          </Button>
          <Button
            variant="ghost"
            size="sm"
            aria-label={`View ${user.username}'s GitHub profile (opens in new tab)`}
            render={
              <a
                href={`https://github.com/${user.username}`}
                target="_blank"
                rel="noopener noreferrer"
              />
            }
          >
            <GitHubIcon />
            GitHub
          </Button>
        </div>
      </OverviewHeader>

      <Metrics aria-label="Profile summary">
        {headlineMetrics.map((metric) => (
          <Metric key={metric.label}>
            <MetricLabel>{metric.label}</MetricLabel>
            <MetricValue title={metric.title}>{metric.value}</MetricValue>
          </Metric>
        ))}
      </Metrics>

      <ProfileEmbedDialog
        open={isEmbedDialogOpen}
        username={user.username}
        displayName={user.displayName}
        onClose={() => setIsEmbedDialogOpen(false)}
      />
    </OverviewPanel>
  );
}
