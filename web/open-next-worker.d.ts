/**
 * Shape of the Worker that `opennextjs-cloudflare build` emits at
 * `.open-next/worker.js`.
 *
 * It is declared rather than inferred because the artifact is gitignored and
 * does not exist during a clean `tsc --noEmit` (CI typechecks before it
 * builds). Only the members `worker.ts` re-uses are described.
 */
declare module "*/.open-next/worker.js" {
  const handler: {
    fetch(request: Request, env: unknown, ctx: unknown): Promise<Response>;
  };
  export default handler;

  /** Durable Object classes the cache overrides bind to. */
  export const DOQueueHandler: unknown;
  export const DOShardedTagCache: unknown;
  export const BucketCachePurge: unknown;
}
