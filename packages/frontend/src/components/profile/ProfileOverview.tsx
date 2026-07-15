"use client";

import { useMemo, useState, useSyncExternalStore } from "react";
import Image from "next/image";
import styled, { css } from "styled-components";
import { toast } from "react-toastify";
import { formatCurrency, formatNumber } from "@/lib/utils";
import { ProfileEmbedDialog } from "./ProfileEmbedDialog";
import type { ProfileStatsData, ProfileUser } from "./types";

export interface ProfileOverviewProps {
  user: ProfileUser;
  stats: ProfileStatsData;
  lastUpdated?: string | null;
  period?: "all" | "month" | "week";
  className?: string;
}

const OverviewPanel = styled.section`
  overflow: hidden;
  border: 1px solid var(--service-border);
  border-radius: 12px;
  background: var(--service-surface);
  color: var(--service-text);
  container-type: inline-size;
`;

const OverviewHeader = styled.div`
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.875rem 1.25rem;
  padding: 0.875rem 1rem;

  @media (min-width: 640px) {
    padding: 1rem 1.125rem;
  }
`;

const Identity = styled.div`
  display: flex;
  min-width: 0;
  flex: 1 1 19rem;
  align-items: center;
  gap: 1rem;
`;

const Avatar = styled.div`
  position: relative;
  width: 72px;
  height: 72px;
  overflow: hidden;
  flex: 0 0 auto;
  border: 1px solid var(--service-border-strong);
  border-radius: 12px;
  background: var(--service-surface-muted);

  @media (min-width: 640px) {
    width: 80px;
    height: 80px;
  }
`;

const AvatarImage = styled(Image)`
  object-fit: cover;
`;

const IdentityCopy = styled.div`
  min-width: 0;
`;

const DisplayName = styled.h1`
  overflow: hidden;
  margin: 0;
  color: var(--service-text);
  font-size: clamp(1.125rem, 4vw, 1.375rem);
  font-weight: 600;
  line-height: 1.2;
  text-overflow: ellipsis;
  white-space: nowrap;
`;

const Handle = styled.p`
  overflow: hidden;
  margin: 0.2rem 0 0;
  color: var(--service-text-muted);
  font-size: 0.875rem;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
`;

const Metadata = styled.ul`
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.25rem 0.5rem;
  margin-top: 0.4rem;
  margin-bottom: 0;
  padding: 0;
  color: var(--service-text-muted);
  font-size: 0.8125rem;
  line-height: 1.3;
  list-style: none;
`;

const MetadataItem = styled.li`
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;

  &:not(:first-child)::before {
    width: 2px;
    height: 2px;
    border-radius: 50%;
    background: var(--service-border-strong);
    content: "";
  }
`;

const RankItem = styled.li`
  display: inline-flex;
  align-items: baseline;
  gap: 0.25rem;
  padding: 0.1875rem 0.4375rem;
  border: 1px solid color-mix(in srgb, var(--service-accent) 42%, transparent);
  border-radius: 0.375rem;
  background: var(--service-accent-soft);
  color: var(--service-text);
  font-size: 0.75rem;
  font-weight: 600;
  line-height: 1.2;

  strong {
    color: var(--service-accent);
    font-weight: 700;
  }
`;

const Actions = styled.div`
  display: flex;
  flex: 0 0 auto;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.375rem;

  @media (max-width: 479px) {
    width: 100%;
  }
`;

const actionStyles = css`
  display: inline-flex;
  min-height: 34px;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  border: 1px solid var(--service-border-strong);
  border-radius: 8px;
  padding: 0.4rem 0.625rem;
  color: var(--service-text);
  font: inherit;
  font-size: 0.8125rem;
  font-weight: 550;
  line-height: 1;
  text-decoration: none;
  cursor: pointer;
  transition:
    border-color 140ms ease,
    background-color 140ms ease,
    color 140ms ease;

  &:focus-visible {
    outline: 2px solid var(--service-focus);
    outline-offset: 2px;
  }

  @media (hover: hover) {
    &:hover {
      border-color: var(--service-border-strong);
    }
  }

  @media (pointer: coarse) {
    min-height: 44px;
    padding-top: 0.65rem;
    padding-bottom: 0.65rem;
  }

  @media (prefers-reduced-motion: reduce) {
    transition: none;
  }
`;

const PrimaryAction = styled.button`
  ${actionStyles}
  border-color: var(--service-accent);
  background: var(--service-accent);
  color: var(--service-accent-foreground);

  @media (hover: hover) {
    &:hover {
      border-color: var(--service-accent-hover);
      background: var(--service-accent-hover);
    }
  }
`;

const SecondaryAction = styled.button`
  ${actionStyles}
  background: var(--service-surface-muted);

  @media (hover: hover) {
    &:hover {
      background: var(--service-accent-soft);
    }
  }
`;

const GhostAction = styled.a`
  ${actionStyles}
  background: transparent;

  @media (hover: hover) {
    &:hover {
      background: var(--service-surface-muted);
    }
  }
`;

const Metrics = styled.dl`
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  margin: 0;
  border-top: 1px solid var(--service-border);
  text-align: left;

  @container (min-width: 40rem) {
    grid-template-columns: repeat(4, 9.25rem);
    justify-content: start;
  }
`;

const Metric = styled.div`
  display: flex;
  min-width: 0;
  flex-direction: column;
  align-items: flex-start;
  padding: 0.6875rem 1rem;
  text-align: left;

  &:nth-child(even) {
    border-left: 1px solid var(--service-border);
  }

  &:nth-child(n + 3) {
    border-top: 1px solid var(--service-border);
  }

  @container (min-width: 40rem) {
    padding-right: 1.25rem;
    padding-left: 1.25rem;

    &:not(:first-child) {
      border-left: 1px solid var(--service-border);
    }

    &:nth-child(n + 3) {
      border-top: 0;
    }
  }
`;

const MetricLabel = styled.dt`
  width: 100%;
  overflow: hidden;
  color: var(--service-text-muted);
  font-size: 0.75rem;
  line-height: 1.2;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
`;

const MetricValue = styled.dd`
  width: 100%;
  overflow: hidden;
  margin: 0.3rem 0 0;
  color: var(--service-text);
  font-size: clamp(1.05rem, 4vw, 1.35rem);
  font-variant-numeric: tabular-nums;
  font-weight: 600;
  line-height: 1.1;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
`;

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

function ShareIcon() {
  return (
    <svg
      aria-hidden="true"
      width="16"
      height="16"
      viewBox="0 0 20 20"
      fill="none"
    >
      <path
        d="M7.15833 11.2583L12.85 14.575M12.8417 5.42499L7.15833 8.74166M17.5 4.16666C17.5 5.54737 16.3807 6.66666 15 6.66666C13.6193 6.66666 12.5 5.54737 12.5 4.16666C12.5 2.78594 13.6193 1.66666 15 1.66666C16.3807 1.66666 17.5 2.78594 17.5 4.16666ZM7.5 9.99999C7.5 11.3807 6.38071 12.5 5 12.5C3.61929 12.5 2.5 11.3807 2.5 9.99999C2.5 8.61928 3.61929 7.49999 5 7.49999C6.38071 7.49999 7.5 8.61928 7.5 9.99999ZM17.5 15.8333C17.5 17.214 16.3807 18.3333 15 18.3333C13.6193 18.3333 12.5 17.214 12.5 15.8333C12.5 14.4526 13.6193 13.3333 15 13.3333C16.3807 13.3333 17.5 14.4526 17.5 15.8333Z"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.667"
      />
    </svg>
  );
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
          <Avatar>
            <AvatarImage
              src={avatarUrl}
              alt={`${displayName}'s avatar`}
              fill
              sizes="(min-width: 640px) 80px, 72px"
              priority
            />
          </Avatar>

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
          </IdentityCopy>
        </Identity>

        <Actions aria-label="Profile actions">
          <PrimaryAction
            type="button"
            onClick={() => setIsEmbedDialogOpen(true)}
            aria-label={`Open GitHub README embed options for ${displayName}`}
          >
            Embed
          </PrimaryAction>
          <SecondaryAction
            type="button"
            onClick={handleShareClick}
            aria-label={`Share ${displayName}'s profile`}
          >
            <ShareIcon />
            Share
          </SecondaryAction>
          <GhostAction
            href={`https://github.com/${user.username}`}
            target="_blank"
            rel="noopener noreferrer"
            aria-label={`View ${user.username}'s GitHub profile (opens in new tab)`}
          >
            <GitHubIcon />
            GitHub
          </GhostAction>
        </Actions>
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
