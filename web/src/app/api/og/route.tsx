import { ImageResponse } from "next/og";
import { formatCurrency, formatNumber } from "@/lib/format";

/**
 * Open Graph card renderer.
 *
 * One endpoint drives every share preview: pages pass a title and a few
 * figures, profiles pass their own. Rendering here rather than shipping a
 * static image means a shared profile link shows that person's actual standing,
 * which is the whole reason anyone shares one.
 *
 * Sizing is set for how these are actually seen — a ~600px wide thumbnail in a
 * timeline, not a 1200px canvas at full size. Everything is roughly twice the
 * size it would be on a web page, because half of it is what reaches the eye.
 */

const BRAND = "#4f8ff0";
const INK = "#e9ecf3";
const MUTED = "#8b9099";

/** The Tokens mark, drawn rather than referenced: Satori cannot resolve
 *  `currentColor` from an external file, and an <img> to our own asset would
 *  need a network round trip from inside the renderer. */
function Mark({ size }: { size: number }) {
  // The viewBox already maps the 64-unit artwork onto `size`, so the geometry
  // and the stroke are given in source units and scale together.
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" fill="none">
      <g stroke={BRAND} strokeWidth={5} strokeLinecap="round">
        <path d="M14 15h36" />
        <path d="M32 15v34" />
        <path d="M22 27h20" opacity="0.75" />
        <path d="M24.5 37h15" opacity="0.5" />
        <path d="M27 47h10" opacity="0.3" />
      </g>
    </svg>
  );
}

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);

  const title = (searchParams.get("title") || "Tokens").slice(0, 60);
  const subtitle = (searchParams.get("subtitle") || "").slice(0, 120);
  const handle = (searchParams.get("handle") || "").slice(0, 40);
  const avatar = searchParams.get("avatar");
  const rank = searchParams.get("rank");
  const tokens = searchParams.get("tokens");
  const cost = searchParams.get("cost");

  const stats: Array<{ label: string; value: string }> = [];
  if (rank) stats.push({ label: "RANK", value: `#${rank}` });
  if (tokens) stats.push({ label: "TOKENS", value: formatNumber(Number(tokens), true) });
  if (cost) stats.push({ label: "COST", value: formatCurrency(Number(cost), true) });

  // A long display name has to shrink or it collides with the right edge.
  const titleSize = title.length > 26 ? 68 : title.length > 18 ? 84 : 100;

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          padding: 72,
          background: "#0f1012",
          color: INK,
          fontFamily: "sans-serif",
          position: "relative",
        }}
      >
        {/* A wash of brand colour behind the mark, so the card is not a flat
            rectangle at thumbnail size. */}
        <div
          style={{
            position: "absolute",
            top: -260,
            left: -160,
            width: 760,
            height: 760,
            borderRadius: 999,
            background: "radial-gradient(circle, rgba(79,143,240,0.20) 0%, rgba(79,143,240,0) 68%)",
            display: "flex",
          }}
        />
        <div
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            right: 0,
            height: 5,
            background: BRAND,
            display: "flex",
          }}
        />

        <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
          <Mark size={52} />
          <span style={{ fontSize: 34, fontWeight: 600, letterSpacing: -0.6 }}>
            Tokens
          </span>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 36 }}>
          {avatar && (
            // eslint-disable-next-line @next/next/no-img-element
            <img
              src={avatar}
              alt=""
              width={148}
              height={148}
              style={{ borderRadius: 999, border: `3px solid ${BRAND}` }}
            />
          )}
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <span
              style={{ fontSize: titleSize, fontWeight: 700, letterSpacing: -2.5, lineHeight: 1.05 }}
            >
              {title}
            </span>
            {handle && (
              <span style={{ fontSize: 38, color: MUTED }}>@{handle}</span>
            )}
            {subtitle && !handle && (
              <span style={{ fontSize: 38, color: MUTED, lineHeight: 1.25 }}>
                {subtitle}
              </span>
            )}
          </div>
        </div>

        {stats.length > 0 ? (
          <div style={{ display: "flex", gap: 72 }}>
            {stats.map((stat) => (
              <div
                key={stat.label}
                style={{ display: "flex", flexDirection: "column", gap: 10 }}
              >
                <span style={{ fontSize: 22, letterSpacing: 3, color: MUTED }}>
                  {stat.label}
                </span>
                <span style={{ fontSize: 60, fontWeight: 700, letterSpacing: -1.2 }}>
                  {stat.value}
                </span>
              </div>
            ))}
          </div>
        ) : (
          <span style={{ fontSize: 30, color: MUTED, letterSpacing: -0.3 }}>
            tokens.ci
          </span>
        )}
      </div>
    ),
    {
      width: 1200,
      height: 630,
      headers: {
        // Rendering one of these costs ~700ms of CPU (Satori lays it out, resvg
        // rasterises it) and the result is a pure function of the query string:
        // a profile card carries its rank and totals as params, so the URL
        // changes whenever the image would. Nothing here needs revalidating on
        // a timer, and crawlers refetch the same URL on every share.
        "Cache-Control":
          "public, max-age=3600, s-maxage=31536000, stale-while-revalidate=604800, immutable",
        // Next puts `Vary: rsc, next-router-...` on every response. Those are
        // request headers a crawler never sends, and varying on them stops the
        // edge caching this at all. An image has one representation.
        Vary: "Accept-Encoding",
      },
    }
  );
}
