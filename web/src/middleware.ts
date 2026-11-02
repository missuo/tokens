import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import {
  DEFAULT_LOCALE,
  LOCALE_COOKIE,
  isLocale,
  negotiateLocale,
  stripLocale,
} from "@/lib/i18n/config";
import { LOCALE_HEADER } from "@/lib/i18n/config";

// Routes that require authentication
const PROTECTED_ROUTES = ["/settings"];

/** The default locale as a prefix, which redirects to the unprefixed path. */
const PREFIXED_DEFAULT = "/en";

/**
 * Pages that render the same bytes for every reader, and the query parameters
 * that actually change what each one renders.
 *
 * These are safe in a shared cache because none of them reads a cookie or a
 * session on the server. Several render dynamically anyway, and Next marks
 * *every* dynamic response `private, no-store`. That default is right in
 * general and wrong here: it costs a full render and several queries on every
 * view of pages nobody needs a private copy of.
 *
 * `/leaderboard` earned its place rather than being relabelled into it. It used
 * to render "Your position" from the session, which genuinely made its HTML
 * differ per reader — and Cloudflare keys on URL alone, so no response header
 * could have made it vary on a cookie. It now ships the whole ranking and the
 * client finds the viewer's own row in it, which removes the personalisation
 * instead of hiding it, and has the better property besides: the card and the
 * table are read from one array, so they cannot disagree.
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
  // The board no longer reads a cookie or a session: it ships the whole
  // ranking and the client finds the viewer's own row in it, so the document
  // is the same for everyone. Of the parameters kept here only `period`,
  // `from`/`to` and a `client:`/`model:` search change what the server
  // computes — `sortBy`, `page` and plain text are applied to rows the client
  // already holds. They stay in the allowlist anyway because these links get
  // shared, and they cost nothing: the client writes them with `replaceState`,
  // which is not a navigation, so ordinary use never mints a cache entry for
  // them. Only somebody opening a shared link does.
  {
    pattern: /^\/leaderboard$/,
    query: new Set(["period", "from", "to", "sortBy", "page", "search"]),
  },
];

/**
 * How long the edge may answer without asking.
 *
 * Half the 60s `revalidate` on the data underneath, so the edge is never the
 * thing deciding how old a page gets — at worst it repeats one data-cache
 * generation, rather than adding a second window on top of it.
 *
 * The short stale window is the part that matters. A submission invalidates the
 * origin's data cache immediately by tag, but nothing purges Cloudflare, so
 * whatever is set here *is* how long a new number takes to appear. At
 * `stale-while-revalidate=300` a rarely-visited profile could answer from a
 * copy six minutes old; 60s bounds that to two. What it costs is that some
 * requests wait for a regeneration instead of getting a stale copy instantly —
 * and that regeneration is 40ms against a database 2ms away, which is not worth
 * five minutes of staleness to avoid.
 */
const SHARED_CACHE_CONTROL =
  "public, max-age=0, s-maxage=30, stale-while-revalidate=60";

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

  // The URL is the only source of the locale: a prefix means that language,
  // and its absence means English. Nothing here reads a cookie to decide what
  // an unprefixed page renders, because Cloudflare caches on the URL alone and
  // a cookie-dependent body would let the first visitor to reach a colo decide
  // what everyone behind it gets for the next 30 seconds.
  const { locale, path } = stripLocale(pathname);
  const prefixed = locale !== DEFAULT_LOCALE;

  // English is a real prefix too, just not the canonical one, so `/en/docs`
  // redirects to `/docs` rather than 404ing. That keeps `/en` a valid thing to
  // type or link to — and the switcher can generate it like any other language
  // without a special case — while leaving exactly one address per page in the
  // cache.
  if (pathname === PREFIXED_DEFAULT || pathname.startsWith(`${PREFIXED_DEFAULT}/`)) {
    // `/en` alone has no page of its own for the same reason `/` does not, and
    // the board is where both end up — going via `/` would cost a second hop.
    const withoutPrefix =
      pathname.slice(PREFIXED_DEFAULT.length) || "/leaderboard";
    const target = new URL(request.nextUrl);
    target.pathname = withoutPrefix;
    return NextResponse.redirect(target, 307);
  }

  // `/zh-CN` on its own is a second name for a page that lives at `/zh/...`.
  if (prefixed && path === "/") {
    return NextResponse.redirect(new URL(`/${locale}/leaderboard`, request.url));
  }

  // The root is the one route with no content of its own — it redirects to the
  // board. That makes it the only place a remembered or negotiated language can
  // be applied by redirect rather than by rendering two documents on one URL.
  if (!prefixed && path === "/") {
    const remembered = request.cookies.get(LOCALE_COOKIE)?.value;
    const preferred = isLocale(remembered)
      ? remembered
      : negotiateLocale(request.headers.get("accept-language"));

    if (preferred && preferred !== DEFAULT_LOCALE) {
      return NextResponse.redirect(
        new URL(`/${preferred}/leaderboard`, request.url),
      );
    }
  }

  const requestHeaders = new Headers(request.headers);
  requestHeaders.set(LOCALE_HEADER, locale);

  /**
   * The two ways this can let a request through.
   *
   * A prefixed URL has to be *rewritten* to the unprefixed path the app routes
   * on — the address bar keeps `/zh/leaderboard`, the renderer sees
   * `/leaderboard`. An unprefixed one is already where it needs to be and only
   * carries the header. Both stamp the locale, so pages never have to guess.
   */
  const pass = () => {
    if (!prefixed) {
      return NextResponse.next({ request: { headers: requestHeaders } });
    }
    const target = request.nextUrl.clone();
    target.pathname = path;
    return NextResponse.rewrite(target, { request: { headers: requestHeaders } });
  };

  if (PROTECTED_ROUTES.some((route) => path.startsWith(route))) {
    const sessionToken = request.cookies.get("tt_session")?.value;

    if (!sessionToken) {
      const loginUrl = new URL("/api/auth/github", request.url);
      // The prefix is kept so signing in lands back on the language the reader
      // was already using.
      loginUrl.searchParams.set("returnTo", pathname);
      return NextResponse.redirect(loginUrl);
    }

    // Session exists, allow access.
    // Note: We don't validate the session here to avoid database calls in
    // middleware. The actual validation happens in the page/API route.
    return pass();
  }

  const route = SHARED_CACHE_ROUTES.find((entry) => entry.pattern.test(path));
  if (!route) {
    return pass();
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

  const response = pass();
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
  //
  // Localised routes are listed rather than matched with a catch-all: the
  // matcher also has to keep the middleware off `/api` (the CLI, badges and
  // embeds have no locale prefix and no HTML to cache) and off every static
  // asset. A `/((?!api|_next).*)` pattern would sweep both in, and a plain
  // `.*\..*` exclusion would also catch a GitHub username with a dot in it
  // (`/u/foo.bar`) — which has to reach the function like any other profile.
  //
  // Adding a locale means adding its two lines here; there is no way to spot
  // that from the prefix list alone, hence this note. `/en` is listed as well
  // even though it only ever redirects: without an entry here it never reaches
  // the function and `/en/leaderboard` 404s instead.
  matcher: [
    "/",
    "/en",
    "/en/:path*",
    "/zh-CN",
    "/zh-CN/:path*",
    "/ja",
    "/ja/:path*",
    "/settings/:path*",
    "/docs",
    "/terms",
    "/privacy",
    "/shame",
    "/leaderboard",
    "/u/:username",
  ],
};
