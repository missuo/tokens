import { ImageResponse } from "next/og";
import { getEmbedClientLogo } from "@/lib/embed/clientLogoSvg";
import { formatCurrency, formatNumber } from "@/lib/format";
import { OG_FONT_SOURCES } from "@/lib/og/fonts.generated";
import { decodeOgActivity, OG_ACTIVITY_WEEKS } from "@/lib/og/ogImage";

/**
 * Open Graph card renderer.
 *
 * One endpoint drives every share preview. With no `handle` it draws the site
 * card (optionally titled, for pages like Docs); with one it draws that
 * person's card from the figures in the query string. Rendering here rather
 * than shipping a static image means a shared profile link shows that person's
 * actual standing, which is the whole reason anyone shares one.
 *
 * Sizing is set for how these are actually seen — a ~600px wide thumbnail in a
 * timeline, not a 1200px canvas at full size. Everything is roughly twice the
 * size it would be on a web page, because half of it is what reaches the eye.
 */

const BG = "#FBFBFA";
const CARD = "#FFFFFF";
const BORDER = "#E8E7E3";
const INK = "#1C1D21";
const MUTED = "#6E727B";
const SUBTLE = "#9A9DA4";
const BRAND = "#2F6FDB";
const HEAT = ["#EEF0F3", "#D3E2FB", "#9FC0F5", "#5E95EA", "#2F6FDB"];

const SANS = "Geist";
const MONO = "JetBrains Mono";

const SITE_CLIENTS = [
  "claude",
  "codex",
  "cursor",
  "gemini",
  "opencode",
  "copilot",
  "amp",
  "qwen",
  "grok",
  "kiro",
  "cline",
  "goose",
];

/** Satori fetches `<img src>` itself, from the server, so `avatar` is an
 *  unauthenticated SSRF vector — `next.config.ts` `remotePatterns` governs
 *  next/image and never sees this request. Every avatar we render comes from
 *  the GitHub profile we stored at sign-in (`users.avatar_url`), so an exact
 *  host match over https covers the real traffic and rejects everything else.
 *  Kept as a URL check rather than a DB read so the rendered card stays a pure
 *  function of the query string, which is what the immutable cache below
 *  depends on. */
const AVATAR_HOST = "avatars.githubusercontent.com";

function safeAvatarUrl(value: string | null): string | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" || url.hostname !== AVATAR_HOST) return null;
    // GitHub serves the full upload otherwise; 240px covers the 2x circle.
    url.searchParams.set("s", "240");
    return url.toString();
  } catch {
    return null;
  }
}

function base64ToArrayBuffer(base64: string): ArrayBuffer {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

let fonts:
  | Array<{ name: string; weight: 500 | 600; style: "normal"; data: ArrayBuffer }>
  | undefined;

function loadFonts() {
  fonts ??= OG_FONT_SOURCES.map((font) => ({
    name: font.name,
    weight: font.weight,
    style: "normal" as const,
    data: base64ToArrayBuffer(font.data),
  }));
  return fonts;
}

/** Client logos are inlined SVG markup; Satori takes them as data URIs. */
function clientLogoDataUri(client: string): string | null {
  const logo = getEmbedClientLogo(client);
  if (!logo) return null;
  const rootAttrs = logo.mono ? logo.rootAttrs.replaceAll("currentColor", INK) : logo.rootAttrs;
  // Some colour icons leave a path unfilled and rely on the page's text colour.
  const fill = /\bfill=/.test(rootAttrs) ? "" : ` fill="${INK}"`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="${logo.viewBox}"${rootAttrs ? ` ${rootAttrs}` : ""}${fill}>${logo.body.replaceAll("currentColor", INK)}</svg>`;
  return `data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(svg)))}`;
}

/** The Tokens mark as it appears in the nav: white strokes on a blue tile. */
function Mark({ size }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" fill="none">
      <rect width="64" height="64" rx="14" fill={BRAND} />
      <g stroke="#FFFFFF" strokeWidth={5} strokeLinecap="round">
        <path d="M14 15h36" />
        <path d="M32 15v34" />
        <path d="M22 27h20" opacity="0.75" />
        <path d="M24.5 37h15" opacity="0.5" />
        <path d="M27 47h10" opacity="0.3" />
      </g>
    </svg>
  );
}

function Wordmark({ size = 44 }: { size?: number }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: size * 0.32 }}>
      <Mark size={size} />
      <span style={{ fontSize: size * 0.72, fontWeight: 600, letterSpacing: -0.8, color: INK }}>
        Tokens
      </span>
    </div>
  );
}

/** Column-major contribution grid, oldest day top-left. */
function Heatmap({ levels, weeks, cell, gap }: { levels: number[]; weeks: number; cell: number; gap: number }) {
  const columns = [];
  for (let w = 0; w < weeks; w++) {
    const days = [];
    for (let d = 0; d < 7; d++) {
      const level = levels[w * 7 + d] ?? 0;
      days.push(
        <div
          key={d}
          style={{
            width: cell,
            height: cell,
            borderRadius: Math.max(2, cell * 0.24),
            background: HEAT[level],
            display: "flex",
          }}
        />,
      );
    }
    columns.push(
      <div key={w} style={{ display: "flex", flexDirection: "column", gap }}>
        {days}
      </div>,
    );
  }
  return <div style={{ display: "flex", gap }}>{columns}</div>;
}

/** A fixed, made-up pattern for the site card: sparse on the left, busier
 *  toward the present, so the panel reads as "usage growing" without implying
 *  any real figures. */
function decorativeLevels(weeks: number): number[] {
  const levels: number[] = [];
  for (let i = 0; i < weeks * 7; i++) {
    const progress = Math.floor(i / 7) / (weeks - 1);
    const noise = (Math.sin(i * 12.9898 + 78.233) * 43758.5453) % 1;
    const r = Math.abs(noise);
    const weekend = i % 7 === 0 || i % 7 === 6;
    const v = r * (0.35 + progress * 0.9) - (weekend ? 0.25 : 0);
    levels.push(v < 0.22 ? 0 : v < 0.45 ? 1 : v < 0.65 ? 2 : v < 0.85 ? 3 : 4);
  }
  return levels;
}

function Background() {
  return (
    <div
      style={{
        position: "absolute",
        top: -420,
        left: 560,
        width: 1000,
        height: 1000,
        display: "flex",
        borderRadius: 999,
        background: "radial-gradient(circle, rgba(47,111,219,0.13) 0%, rgba(47,111,219,0) 62%)",
      }}
    />
  );
}

function LogoTile({ client, size }: { client: string; size: number }) {
  const src = clientLogoDataUri(client);
  if (!src) return null;
  return (
    <div
      style={{
        width: size,
        height: size,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: CARD,
        border: `1.5px solid ${BORDER}`,
        borderRadius: size * 0.28,
      }}
    >
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={src} alt="" width={size * 0.56} height={size * 0.56} />
    </div>
  );
}

function SiteCard({ title, subtitle }: { title: string | null; subtitle: string | null }) {
  const headline = title ?? "The leaderboard for";
  const accent = title ? null : "AI coding usage.";
  const sub = subtitle ?? "Track tokens and spend across Claude Code, Codex, Cursor, Gemini and 20+ more.";
  const headlineSize = headline.length > 22 ? 60 : 68;

  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", width: "100%", height: "100%", padding: "60px 64px" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <Wordmark />
        <span style={{ fontFamily: MONO, fontSize: 24, color: MUTED }}>tokens.ci</span>
      </div>

      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 32 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 22, width: 640 }}>
          <div style={{ display: "flex", flexDirection: "column" }}>
            <span style={{ fontSize: headlineSize, fontWeight: 600, letterSpacing: -2.6, lineHeight: 1.04, color: INK }}>
              {headline}
            </span>
            {accent && (
              <span style={{ fontSize: headlineSize, fontWeight: 600, letterSpacing: -2.6, lineHeight: 1.04, color: BRAND }}>
                {accent}
              </span>
            )}
          </div>
          <span style={{ fontSize: 27, fontWeight: 500, lineHeight: 1.38, color: MUTED }}>{sub}</span>
        </div>

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 18,
            padding: 24,
            background: CARD,
            border: `1.5px solid ${BORDER}`,
            borderRadius: 24,
            boxShadow: "0 20px 50px -24px rgba(28,29,33,0.22)",
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <span style={{ fontSize: 20, fontWeight: 600, color: INK }}>Activity</span>
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <span style={{ fontSize: 16, color: SUBTLE, marginRight: 4 }}>Less</span>
              {HEAT.map((c) => (
                <div key={c} style={{ width: 14, height: 14, borderRadius: 4, background: c, display: "flex" }} />
              ))}
              <span style={{ fontSize: 16, color: SUBTLE, marginLeft: 4 }}>More</span>
            </div>
          </div>
          <Heatmap levels={decorativeLevels(12)} weeks={12} cell={22} gap={6} />
        </div>
      </div>

      <div style={{ display: "flex", gap: 12 }}>
        {SITE_CLIENTS.map((client) => (
          <LogoTile key={client} client={client} size={56} />
        ))}
      </div>
    </div>
  );
}

function StatTile({ label, value }: { label: string; value: string }) {
  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        gap: 6,
        padding: "20px 26px",
        background: CARD,
        border: `1.5px solid ${BORDER}`,
        borderRadius: 20,
      }}
    >
      <span style={{ fontSize: 19, fontWeight: 500, letterSpacing: 2.4, color: SUBTLE }}>{label}</span>
      <span style={{ fontFamily: MONO, fontSize: 50, fontWeight: 600, letterSpacing: -1.5, color: INK }}>{value}</span>
    </div>
  );
}

function ProfileCard({
  name,
  handle,
  avatar,
  stats,
  activity,
  activeDays,
  clients,
}: {
  name: string;
  handle: string;
  avatar: string | null;
  stats: Array<{ label: string; value: string }>;
  activity: number[] | null;
  activeDays: string | null;
  clients: string[];
}) {
  const nameSize = name.length > 22 ? 48 : name.length > 15 ? 56 : 64;
  const showHandle = name.toLowerCase() !== handle.toLowerCase();

  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", width: "100%", height: "100%", padding: "56px 64px" }}>
      <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 28 }}>
          {avatar ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img
              src={avatar}
              alt=""
              width={112}
              height={112}
              style={{ borderRadius: 999, border: `4px solid ${CARD}`, boxShadow: `0 0 0 1.5px ${BORDER}` }}
            />
          ) : (
            <div
              style={{
                width: 112,
                height: 112,
                borderRadius: 999,
                background: "#E9F0FC",
                color: BRAND,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontSize: 52,
                fontWeight: 600,
              }}
            >
              {handle.slice(0, 1).toUpperCase()}
            </div>
          )}
          <div style={{ display: "flex", flexDirection: "column", gap: 4, maxWidth: 640 }}>
            <span style={{ fontSize: nameSize, fontWeight: 600, letterSpacing: -2, lineHeight: 1.08, color: INK }}>
              {showHandle ? name : `@${handle}`}
            </span>
            {showHandle && <span style={{ fontSize: 30, fontWeight: 500, color: MUTED }}>@{handle}</span>}
          </div>
        </div>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 14 }}>
          <Wordmark size={40} />
          {clients.length > 0 && (
            <div style={{ display: "flex", gap: 8 }}>
              {clients.map((client) => (
                <LogoTile key={client} client={client} size={42} />
              ))}
            </div>
          )}
        </div>
      </div>

      {stats.length > 0 && (
        <div style={{ display: "flex", gap: 16 }}>
          {stats.map((stat) => (
            <StatTile key={stat.label} label={stat.label} value={stat.value} />
          ))}
        </div>
      )}

      {activity ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div style={{ display: "flex", justifyContent: "space-between" }}>
            <span style={{ fontSize: 20, fontWeight: 500, color: MUTED }}>Past year</span>
            <span style={{ fontFamily: MONO, fontSize: 20, color: SUBTLE }}>tokens.ci/u/{handle}</span>
          </div>
          <Heatmap levels={activity} weeks={OG_ACTIVITY_WEEKS} cell={14.7} gap={6} />
        </div>
      ) : (
        <div style={{ display: "flex", justifyContent: "space-between" }}>
          <span style={{ fontSize: 22, fontWeight: 500, color: MUTED }}>
            {activeDays ? `${activeDays} active days` : "AI coding usage on Tokens"}
          </span>
          <span style={{ fontFamily: MONO, fontSize: 22, color: SUBTLE }}>tokens.ci/u/{handle}</span>
        </div>
      )}
    </div>
  );
}

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);

  const title = searchParams.get("title")?.slice(0, 60) || null;
  const subtitle = searchParams.get("subtitle")?.slice(0, 120) || null;
  const handle = (searchParams.get("handle") || "").slice(0, 40);

  let card: React.ReactElement;
  if (handle) {
    const rank = searchParams.get("rank");
    const tokens = searchParams.get("tokens");
    const cost = searchParams.get("cost");
    const days = searchParams.get("days");
    const activeDays = days && /^\d+$/.test(days) ? formatNumber(Number(days)) : null;

    const stats: Array<{ label: string; value: string }> = [];
    if (tokens) stats.push({ label: "TOKENS", value: formatNumber(Number(tokens), true) });
    if (cost) stats.push({ label: "SPENT", value: formatCurrency(Number(cost), true) });
    if (rank && /^\d+$/.test(rank)) stats.push({ label: "RANK", value: `#${formatNumber(Number(rank))}` });
    if (activeDays) stats.push({ label: "ACTIVE DAYS", value: activeDays });

    const clients = (searchParams.get("clients") || "")
      .split(",")
      .filter((client) => getEmbedClientLogo(client))
      .slice(0, 4);

    card = (
      <ProfileCard
        name={title ?? handle}
        handle={handle}
        avatar={safeAvatarUrl(searchParams.get("avatar"))}
        stats={stats}
        activity={decodeOgActivity(searchParams.get("activity"))}
        activeDays={activeDays}
        clients={clients}
      />
    );
  } else {
    // The site card used to be requested as `?title=Tokens`; that title is the
    // wordmark, not a headline.
    const pageTitle = title && title !== "Tokens" ? title : null;
    card = <SiteCard title={pageTitle} subtitle={pageTitle ? subtitle : null} />;
  }

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          position: "relative",
          background: BG,
          color: INK,
          fontFamily: SANS,
        }}
      >
        <Background />
        {card}
      </div>
    ),
    {
      width: 1200,
      height: 630,
      fonts: loadFonts(),
      headers: {
        // Rendering one of these costs ~700ms of CPU (Satori lays it out, resvg
        // rasterises it) and the result is a pure function of the query string:
        // a profile card carries its rank and totals as params, so the URL
        // changes whenever the image would. Nothing here needs revalidating on
        // a timer, and crawlers refetch the same URL on every share. Design
        // changes are rolled out by bumping `OG_VERSION`, which changes every URL.
        "Cache-Control":
          "public, max-age=3600, s-maxage=31536000, stale-while-revalidate=604800, immutable",
        // Next puts `Vary: rsc, next-router-...` on every response. Those are
        // request headers a crawler never sends, and varying on them stops the
        // edge caching this at all. An image has one representation.
        Vary: "Accept-Encoding",
      },
    },
  );
}
