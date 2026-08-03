import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";

// Routes that require authentication
const PROTECTED_ROUTES = ["/settings"];

/**
 * Pages that render the same bytes for every reader and are therefore safe in
 * a shared cache.
 *
 * A public profile is the clearest case: it reads no cookie and no session on
 * the server, and the only identity in the HTML belongs to the profile's owner.
 * It renders dynamically only because it accepts a `period` search param, and
 * Next marks *every* dynamic response `private, no-store` — a safe default that
 * here costs a full render, several queries and ~140 ms on every single view.
 *
 * `/leaderboard` is deliberately absent. It renders "Your position" from the
 * session, so its HTML is not the same for everyone, and Cloudflare keys its
 * cache on URL alone — it cannot be told to vary on a cookie by a response
 * header, so an anonymous copy would be handed to signed-in readers. Making
 * that page cacheable means removing the personalisation from the server
 * render, not relabelling it.
 */
const SHARED_CACHE_ROUTES = [/^\/u\/[^/]+$/];

/** 60s at the edge, matching the `revalidate` on the data these pages read, so
 *  the cached copy is never staler than what the origin would have produced.
 *  The stale window lets the edge keep answering while it refreshes behind the
 *  reader rather than making someone wait for the regeneration. */
const SHARED_CACHE_CONTROL =
  "public, max-age=0, s-maxage=60, stale-while-revalidate=300";

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

  if (SHARED_CACHE_ROUTES.some((route) => route.test(pathname))) {
    const response = NextResponse.next();
    response.headers.set("Cache-Control", SHARED_CACHE_CONTROL);
    return response;
  }

  return NextResponse.next();
}

export const config = {
  // Both the protected routes and the ones whose caching is decided here. A
  // path absent from this list never reaches the function above, however the
  // conditions inside it are written.
  matcher: ["/settings/:path*", "/u/:username"],
};
