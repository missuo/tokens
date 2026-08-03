import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";

// Routes that require authentication
const PROTECTED_ROUTES = ["/settings"];

/**
 * Pages that render the same bytes for every reader, and the query parameters
 * that actually change what each one renders.
 *
 * These are safe in a shared cache because none of them reads a cookie or a
 * session on the server — the only identity that appears in a public profile's
 * HTML belongs to the profile's owner. Several render dynamically anyway, and
 * Next marks *every* dynamic response `private, no-store`. That default is
 * right in general and wrong here: it costs a full render and several queries
 * on every view of pages nobody needs a private copy of.
 *
 * `/leaderboard` is deliberately absent. It renders "Your position" from the
 * session, so its HTML genuinely differs per reader, and Cloudflare keys its
 * cache on URL alone — no response header can make it vary on a cookie, so an
 * anonymous copy would be served to signed-in readers. Making that page
 * cacheable means taking the personalisation out of the server render, not
 * relabelling the response.
 */
const SHARED_CACHE_ROUTES: ReadonlyArray<{
  pattern: RegExp;
  query: ReadonlySet<string>;
}> = [
  { pattern: /^\/$/, query: new Set() },
  { pattern: /^\/docs$/, query: new Set() },
  { pattern: /^\/terms$/, query: new Set() },
  { pattern: /^\/privacy$/, query: new Set() },
  // Renders a notice when a banned account is turned away at sign-in.
  { pattern: /^\/shame$/, query: new Set(["error"]) },
  { pattern: /^\/u\/[^/]+$/, query: new Set(["period"]) },
];

/** 60s at the edge, matching the `revalidate` on the data these pages read, so
 *  a cached copy is never staler than what the origin would have produced. The
 *  stale window lets the edge keep answering while it refreshes behind the
 *  reader instead of making someone wait for the regeneration. */
const SHARED_CACHE_CONTROL =
  "public, max-age=0, s-maxage=60, stale-while-revalidate=300";

/**
 * Next fetches the RSC payload for a client-side navigation from the same path,
 * distinguished only by an `_rsc` query parameter and an `RSC` header.
 *
 * Normalising those away would hand a browser asking for flight data a full
 * HTML document — which is not hypothetical: stripping `_rsc` from a cache key
 * is exactly what once served React flight data as a profile page in
 * production. Cloudflare keys on the whole query string, so `?_rsc=…` is
 * already its own cache entry and needs no help from us; it only needs to be
 * left alone.
 */
function isRscRequest(request: NextRequest): boolean {
  return (
    request.headers.has("RSC") || request.nextUrl.searchParams.has("_rsc")
  );
}

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  if (PROTECTED_ROUTES.some((route) => pathname.startsWith(route))) {
    const sessionToken = request.cookies.get("tt_session")?.value;

    if (!sessionToken) {
      const loginUrl = new URL("/api/auth/github", request.url);
      loginUrl.searchParams.set("returnTo", pathname);
      return NextResponse.redirect(loginUrl);
    }

    // Session exists, allow access.
    // Note: We don't validate the session here to avoid database calls in
    // middleware. The actual validation happens in the page/API route.
    return NextResponse.next();
  }

  const route = SHARED_CACHE_ROUTES.find((entry) =>
    entry.pattern.test(pathname),
  );
  if (!route) {
    return NextResponse.next();
  }

  if (!isRscRequest(request)) {
    // Cloudflare's cache key is the full URL, and customising it is an
    // Enterprise feature — so on this plan every distinct query string is a
    // distinct cache entry. A shared link carrying a utm tag, or anyone
    // appending a counter, would otherwise split one page into unlimited
    // copies and push the entries that matter out of the cache.
    //
    // Redirecting rather than silently ignoring the extras keeps one canonical
    // URL per page, which is also what the cache, the logs and any share of
    // that link want. 307 rather than 308: the normalisation is a policy this
    // code owns, and a permanent redirect would be cached by browsers long
    // after a future parameter became meaningful.
    const canonical = canonicalUrl(request, route.query);
    if (canonical) {
      return NextResponse.redirect(canonical, 307);
    }
  }

  const response = NextResponse.next();
  response.headers.set("Cache-Control", SHARED_CACHE_CONTROL);
  return response;
}

/** The URL this request should have used, or null when it already did. */
function canonicalUrl(
  request: NextRequest,
  allowed: ReadonlySet<string>,
): URL | null {
  const { searchParams } = request.nextUrl;
  const extras = [...searchParams.keys()].filter((key) => !allowed.has(key));
  if (extras.length === 0) return null;

  const url = new URL(request.nextUrl);
  // Rebuilt from the allowlist rather than deleting the extras, so parameter
  // order is fixed too — `?a=1&b=2` and `?b=2&a=1` are one cache entry, not two.
  url.search = "";
  for (const key of [...allowed].sort()) {
    const value = searchParams.get(key);
    if (value !== null) url.searchParams.set(key, value);
  }
  return url;
}

export const config = {
  // Every path whose auth or caching is decided above. One absent from this
  // list never reaches the function, however its conditions are written.
  matcher: [
    "/",
    "/settings/:path*",
    "/docs",
    "/terms",
    "/privacy",
    "/shame",
    "/u/:username",
  ],
};
