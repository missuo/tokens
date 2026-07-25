import { defineCloudflareConfig } from "@opennextjs/cloudflare";
import r2IncrementalCache from "@opennextjs/cloudflare/overrides/incremental-cache/r2-incremental-cache";
import doQueue from "@opennextjs/cloudflare/overrides/queue/do-queue";
import doShardedTagCache from "@opennextjs/cloudflare/overrides/tag-cache/do-sharded-tag-cache";
import { purgeCache } from "@opennextjs/cloudflare/overrides/cache-purge/index";

/**
 * Cache topology for Tokens.
 *
 * Every leaderboard/profile read goes through `unstable_cache` with a 60s
 * revalidate, and every accepted submission fans out `revalidateTag` calls
 * ("leaderboard", "user:<name>", …). That combination is write-heavy for the
 * tag cache, so the sharded Durable Object variant is used rather than the D1
 * one, which is documented for lighter revalidation loads.
 */
export default defineCloudflareConfig({
  // Cached page/data payloads live in R2 — cheap, strongly consistent, and
  // unbounded compared with KV.
  incrementalCache: r2IncrementalCache,

  // Deduplicates concurrent time-based revalidations, so a cache miss on a
  // popular page triggers one regeneration instead of one per request.
  queue: doQueue,

  // Sharded so concurrent submits invalidating overlapping tags don't serialise
  // on a single Durable Object.
  tagCache: doShardedTagCache({ baseShardSize: 12 }),

  // Drops the matching responses from Cloudflare's edge cache when a tag is
  // invalidated. "durableObject" batches purges through NEXT_CACHE_DO_PURGE and
  // needs no zone API token.
  cachePurge: purgeCache({ type: "durableObject" }),
});
