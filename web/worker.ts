// Custom Worker entrypoint wrapping the OpenNext-generated handler.
//
// It exists so the Worker can own its own cron trigger: the daily verified-badge
// refresh runs here instead of being poked over HTTP by an external scheduler.
// That removes the CRON_SECRET round trip and the WAF exception the GitHub
// Actions + SSH path needs today, and the job runs with the same bindings as the
// request path.
import { default as handler } from "./.open-next/worker.js";
import { refreshAllSocialLinks } from "./src/lib/cron/refreshSocialLinks";

/**
 * Routes whose responses are a pure function of their URL and cost real work to
 * produce: /api/og lays out and rasterises a PNG (~700ms of CPU), the embed and
 * badge cards serialise an SVG after several database reads. Every parameter
 * that changes the output is already in the query string, so the URL is a
 * complete cache key.
 *
 * Cloudflare does not put Worker responses in the edge cache on its own — the
 * `Cache-Control` on them is honoured by browsers and by nothing else, which is
 * why these were recomputed on every single request. Reading and writing
 * `caches.default` explicitly is what actually puts them there.
 */
const CACHEABLE = /^\/api\/(og|embed\/[^/]+\/svg|badge\/[^/]+\/svg)/;

/**
 * HTML pages worth putting in the edge cache for signed-out readers.
 *
 * The Worker is pinned to `aws:us-west-2` so it sits beside the database, which
 * means every request — cache hit or not — crosses to Oregon before anything
 * is decided. `unstable_cache` saves the queries but never the flight. For a
 * reader in Asia that flight *is* the page load, and no amount of data caching
 * touches it.
 *
 * Putting the rendered HTML in `caches.default` is what removes it: the colo
 * nearest the reader answers, and Oregon is only involved when the entry is
 * cold. Same mechanism the SVG routes above already use, applied to the pages
 * that actually carry the traffic.
 *
 * Deliberately limited to signed-out requests. These pages personalise — the
 * leaderboard renders "Your position" from the session — and a shared cache is
 * exactly the wrong place for that. Anyone carrying a session cookie takes the
 * normal path and sees precisely what they see today.
 */
const PAGE_CACHEABLE = /^\/(leaderboard|shame)?$/;

/**
 * Public profiles, cacheable for *every* reader rather than signed-out ones.
 *
 * Unlike the leaderboard, this page renders nothing that depends on who is
 * looking: it reads no cookie and no session server-side, and the only
 * identity in the HTML belongs to the profile's owner. Restricting it to
 * anonymous requests the way the leaderboard is restricted would halve the
 * benefit for no gain in safety.
 *
 * The 308 that canonicalises username case passes straight through — only 200s
 * are stored — so a mixed-case URL keeps redirecting rather than caching a
 * redirect under the wrong key.
 */
const PROFILE_CACHEABLE = /^\/u\/[^/]+$/;

/** Query parameters that change what a profile renders. Anything else — a
 *  utm tag, a cache-buster someone pasted — is dropped from the cache key so
 *  it cannot split one page into unlimited entries. */
const PROFILE_QUERY_ALLOWLIST = new Set(["period"]);

const SESSION_COOKIE = "tt_session";
const SORT_BY_COOKIE = "leaderboard-sort-by";

/** 60s, matching the `revalidate` on the data these pages read: the edge copy
 *  is never staler than what the origin would have served anyway. */
const PAGE_EDGE_TTL = "public, max-age=0, s-maxage=60";

function readCookie(request: Request, name: string): string | null {
  const header = request.headers.get("cookie");
  if (!header) return null;
  for (const part of header.split(";")) {
    const [key, ...rest] = part.trim().split("=");
    if (key === name) return rest.join("=");
  }
  return null;
}

/**
 * The cache key for a page request.
 *
 * `sortBy` lives in a cookie rather than the URL, so two readers on the same
 * path can be looking at different orderings. Folding it into the key keeps
 * them from being served each other's page — the Cache API keys on URL alone,
 * and a cookie it cannot see is a cookie it cannot vary on.
 */
/**
 * Strip the shared-cache header off a hit before it reaches the browser.
 *
 * The stored copy has to advertise itself as cacheable or the edge will not
 * keep it, but that header must not survive to the client. Cloudflare rewrites
 * an origin `max-age=0` to the zone's Browser Cache TTL — 4 hours here — so a
 * signed-out reader's browser would hold this page across a login and keep
 * serving the version rendered without a session, silently dropping "Your
 * position" until it expired.
 *
 * The client gets the same `no-store` it would have received from the origin.
 * Only the copy inside `caches.default` is marked cacheable.
 */
function withPrivateHeaders(response: Response): Response {
  const out = new Response(response.body, response);
  out.headers.set("Cache-Control", "private, no-cache, no-store, max-age=0, must-revalidate");
  return out;
}

function pageCacheKey(request: Request, url: URL): Request {
  const sort = readCookie(request, SORT_BY_COOKIE);
  const keyUrl = new URL(url.toString());
  if (sort) keyUrl.searchParams.set("__sort", sort);
  return new Request(keyUrl.toString(), { method: "GET" });
}

function profileCacheKey(url: URL): Request {
  const keyUrl = new URL(url.origin + url.pathname);
  for (const [name, value] of [...url.searchParams].sort()) {
    if (PROFILE_QUERY_ALLOWLIST.has(name)) keyUrl.searchParams.set(name, value);
  }
  return new Request(keyUrl.toString(), { method: "GET" });
}

/**
 * The cache key for this request, or null when it must not be shared.
 *
 * Two different rules on purpose: a profile is the same page for everyone, so
 * it is keyed on its URL alone; the leaderboard personalises, so it is cached
 * only when no session is present and its key carries the sort cookie that the
 * URL does not.
 */
function sharedCacheKey(request: Request, url: URL): Request | null {
  // Next fetches the RSC payload for a client-side navigation from the same
  // path, distinguished only by an `_rsc` query parameter and an `RSC` header.
  // The Cache API keys on URL alone and this profile key drops unknown query
  // parameters, so an RSC response would be stored under the HTML key and then
  // served to a browser asking for a page — which is exactly what happened.
  // These are navigations, already fast, and not worth the class of bug.
  if (request.headers.has("RSC") || url.searchParams.has("_rsc")) {
    return null;
  }

  if (PROFILE_CACHEABLE.test(url.pathname)) {
    return profileCacheKey(url);
  }
  if (
    PAGE_CACHEABLE.test(url.pathname) &&
    readCookie(request, SESSION_COOKIE) === null
  ) {
    return pageCacheKey(request, url);
  }
  return null;
}

interface CacheStorageLike {
  default: {
    match(request: Request): Promise<Response | undefined>;
    put(request: Request, response: Response): Promise<void>;
  };
}

interface FetchExecutionContext {
  waitUntil(promise: Promise<unknown>): void;
}

/**
 * Minimal structural types for the scheduled handler.
 *
 * The generated Cloudflare types are binding-only (`--include-runtime=false`),
 * deliberately: pulling in the full Workers runtime types would redefine DOM
 * globals across the Next.js app and turn every `response.json()` into
 * `unknown`. Only the two members used here are needed.
 */
interface ScheduledController {
  readonly cron: string;
  readonly scheduledTime: number;
}

interface ScheduledExecutionContext {
  waitUntil(promise: Promise<unknown>): void;
}

export default {
  async fetch(
    request: Request,
    env: CloudflareEnv,
    ctx: FetchExecutionContext,
  ): Promise<Response> {
    const url = new URL(request.url);
    const isGet = request.method === "GET";
    const cache = (caches as unknown as CacheStorageLike).default;

    if (isGet && CACHEABLE.test(url.pathname)) {
      const hit = await cache.match(request);
      if (hit) return hit;

      const response = await handler.fetch(request, env, ctx);

      // Only success is worth storing; an error page cached for a year would
      // outlive whatever caused it.
      if (response.status === 200) {
        // The body can only be read once, so the copy goes to the cache and the
        // original goes to the client. `waitUntil` keeps the write from delaying
        // the response.
        ctx.waitUntil(cache.put(request, response.clone()));
      }
      return response;
    }

    const key = isGet ? sharedCacheKey(request, url) : null;
    if (!key) {
      return handler.fetch(request, env, ctx);
    }

    const pageHit = await cache.match(key);
    if (pageHit) return withPrivateHeaders(pageHit);

    const response = await handler.fetch(request, env, ctx);
    if (response.status !== 200) return response;

    // Next marks these responses `private, no-store` because they are rendered
    // per request, and the Cache API refuses to store that — correctly, for a
    // browser. Here the decision has already been made one layer up: this
    // request carries no session, so the render is not personal to anyone. The
    // stored copy gets a header that says so; the client keeps the original,
    // so nothing lands in a *browser* cache that was not meant to.
    const stored = new Response(response.clone().body, response);
    stored.headers.set("Cache-Control", PAGE_EDGE_TTL);
    stored.headers.delete("Set-Cookie");
    ctx.waitUntil(cache.put(key, stored));

    return response;
  },

  async scheduled(
    _controller: ScheduledController,
    _env: CloudflareEnv,
    ctx: ScheduledExecutionContext,
  ): Promise<void> {
    // `waitUntil` keeps the isolate alive across the whole batched loop; the
    // runtime may otherwise consider the event finished once the handler
    // returns its first await.
    ctx.waitUntil(
      refreshAllSocialLinks()
        .then(({ users, verified }) => {
          console.log(
            `[cron] refresh-social-links: synced ${users} users, ${verified} verified`,
          );
        })
        .catch((error: unknown) => {
          console.error("[cron] refresh-social-links failed", error);
        }),
    );
  },
};

// Re-exported so the Durable Object bindings in wrangler.jsonc resolve; the
// cache overrides in open-next.config.ts depend on all three.
export {
  DOQueueHandler,
  DOShardedTagCache,
  BucketCachePurge,
} from "./.open-next/worker.js";
