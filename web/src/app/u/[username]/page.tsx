import type { Metadata } from 'next';
import { notFound, permanentRedirect } from 'next/navigation';
import type { ProfileDevice } from '@/components/profile';
import { formatCurrency, formatNumber } from '@/lib/format';
import { getGitHubSocialLinks } from '@/lib/githubSocials';
import { encodeOgActivity, ogImageUrl, topOgClients } from '@/lib/og/ogImage';
import { loadPublicProfileDevicesForPage } from '@/lib/publicProfileDevices';
import { loadPublicProfileForPage } from '@/lib/publicProfileData';
import ProfilePageClient, { type ProfileData } from './ProfilePageClient';
import BannedProfileView, { type BannedProfileData } from './BannedProfileView';
import { localizeHref } from '@/lib/i18n/config';
import { getTranslator } from '@/lib/i18n/server';

export const revalidate = 60;

const PROFILE_PERIODS = ["all", "week", "month"] as const;
type ProfilePeriod = (typeof PROFILE_PERIODS)[number];

function parseProfilePeriod(value: string | string[] | undefined): ProfilePeriod {
  const period = Array.isArray(value) ? value[0] : value;
  return PROFILE_PERIODS.includes(period as ProfilePeriod)
    ? (period as ProfilePeriod)
    : "all";
}

async function getProfileData(
  username: string,
  period: ProfilePeriod,
): Promise<ProfileData | BannedProfileData | null> {
  // Calling the shared server handler keeps Vercel Deployment Protection out
  // of the render path. A server-side HTTP self-fetch is anonymous and is
  // redirected to Vercel's HTML login page on protected preview deployments.
  const result = await loadPublicProfileForPage(username, period);

  if (result.kind === "redirect") {
    if (result.location) {
      const canonicalUsername = decodeURIComponent(
        new URL(result.location).pathname.split("/").at(-1) ?? "",
      );
      if (canonicalUsername && canonicalUsername !== username) {
        return getProfileData(canonicalUsername, period);
      }
    }
  }

  // "We could not find out" is not "this person does not exist", and collapsing
  // both into null made every failure a 404. That was survivable while this
  // page was `no-store` and a bad render reached one reader. It is not now: a
  // 404 carrying `s-maxage` is stored by the edge and handed to everyone in
  // that colo for the life of the entry, so a few seconds of database trouble
  // would delete a real person's profile for a minute and a half.
  //
  // Throwing puts a 5xx on the wire instead, which Caddy strips the shareable
  // cache-control from — the failure stays a failure and nothing keeps it.
  if (result.kind === "error" && result.status >= 500) {
    throw new Error(
      `Profile lookup for ${username} failed upstream with ${result.status}`,
    );
  }

  if (result.kind !== "data") {
    return null;
  }

  const data = result.data as ProfileData | BannedProfileData;
  return data;
}

function isBannedProfile(
  data: ProfileData | BannedProfileData,
): data is BannedProfileData {
  return "banned" in data && data.banned === true;
}

// Devices are an enrichment on top of the core profile: if this fetch fails
// we still render the profile, just without the Devices section.
async function getProfileDevices(username: string) {
  try {
    return (await loadPublicProfileDevicesForPage(username)) as ProfileDevice[];
  } catch {
    return [];
  }
}

export async function generateMetadata({ params }: { params: Promise<{ username: string }> }): Promise<Metadata> {
  const { username } = await params;
  const { locale, t } = await getTranslator();

  // Built from the profile's own figures so a shared link previews that
  // person's standing rather than a generic banner. Falls back to the plain
  // card when the profile cannot be loaded — a preview is never worth failing
  // the page render for.
  const data = await getProfileData(username, "all").catch(() => null);
  const stats = data && !isBannedProfile(data) ? data.stats : null;
  const rank = data && !isBannedProfile(data) ? data.user?.rank : null;

  // The card draws `title` large with `@handle` beneath it. Passing the
  // username as both printed the same word twice and never showed the person's
  // name; fall back to the username only when there is no display name.
  const profile = data && !isBannedProfile(data) ? data : null;
  const displayName = data?.user?.displayName?.trim();
  // Canonical casing, so the card and og:url match the page the link lands on.
  const handle = data?.user?.username || username;
  const image = ogImageUrl({
    title: displayName || handle,
    handle,
    avatar: data?.user?.avatarUrl,
    rank,
    tokens: stats?.totalTokens || null,
    cost: stats?.totalCost || null,
    days: stats?.activeDays || null,
    // More than the card shows: the renderer drops clients it has no logo for.
    clients: profile ? topOgClients(profile.contributions, 8).join(",") : null,
    activity: profile ? encodeOgActivity(profile.contributions) : null,
    locale,
  });

  const shareTitle = displayName && displayName !== handle
    ? t("profile.meta.shareTitleNamed", { displayName, handle })
    : t("profile.meta.shareTitlePlain", { handle });
  // The figures are formatted for the reader's locale here rather than inside
  // the catalogue: `formatNumber` needs the number, not a slot it would have to
  // re-parse.
  const shareDescription = stats
    ? t(rank != null ? "meta.statsWithRank" : "meta.statsNoRank", {
        tokens: formatNumber(stats.totalTokens, true, locale),
        cost: formatCurrency(stats.totalCost, true, locale),
        rank: rank ?? 0,
      })
    : t("meta.description", { handle });

  return {
    title: t("meta.title", { handle }),
    description: shareDescription,
    openGraph: {
      title: shareTitle,
      description: shareDescription,
      type: "profile",
      url: `https://tokens.ci${localizeHref(`/u/${handle}`, locale)}`,
      siteName: "Tokens",
      images: [{ url: image, width: 1200, height: 630, alt: shareTitle }],
    },
    twitter: {
      card: "summary_large_image",
      title: shareTitle,
      description: shareDescription,
      images: [image],
    },
  };
}

export default async function ProfilePage({
  params,
  searchParams,
}: {
  params: Promise<{ username: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const { username } = await params;
  const resolvedSearchParams = await searchParams;
  const period = parseProfilePeriod(resolvedSearchParams.period);
  const { locale } = await getTranslator();
  const [data, devices, socialLinks] = await Promise.all([
    // `getProfileData` throws on an upstream 5xx so the failure is not cached
    // (see the note there). The page turns that back into "no such page", which
    // still lands the reader on a translated page they can act on instead of a
    // bare error screen — at the cost of blurring "could not load" and "does
    // not exist" for one revalidate window.
    getProfileData(username, period).catch(() => null),
    getProfileDevices(username),
    getGitHubSocialLinks(username),
  ]);

  if (!data) {
    notFound();
  }

  // Canonical casing, in the language being read: `/zh/u/old` must land on
  // `/zh/u/new` rather than dropping the reader back into English.
  if (data.user?.username && data.user.username !== username) {
    permanentRedirect(
      localizeHref(
        `/u/${data.user.username}${period === "all" ? "" : `?period=${period}`}`,
        locale,
      ),
    );
  }

  if (isBannedProfile(data)) {
    return <BannedProfileView data={data} />;
  }

  return (
    <ProfilePageClient
      initialData={data}
      initialDevices={devices}
      socialLinks={socialLinks}
      username={username}
    />
  );
}
