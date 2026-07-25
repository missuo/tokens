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
    const cacheable = request.method === "GET" && CACHEABLE.test(url.pathname);

    if (!cacheable) {
      return handler.fetch(request, env, ctx);
    }

    const cache = (caches as unknown as CacheStorageLike).default;
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
