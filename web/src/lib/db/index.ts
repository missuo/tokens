import { drizzle } from "drizzle-orm/postgres-js";
import { getCloudflareContext } from "@opennextjs/cloudflare";
import * as schema from "./schema";

/**
 * On Workers the Postgres URL comes from the Hyperdrive binding rather than an
 * environment variable. Hyperdrive keeps a warm, pooled connection next to the
 * database, which is what makes a page issuing several sequential queries
 * viable from the edge — without it each query pays a full WAN round trip.
 *
 * Importing `@opennextjs/cloudflare` is safe under Node (drizzle-kit, vitest,
 * `next build`); only the call throws there, and that is treated as "not on
 * Workers".
 */
function getHyperdriveConnectionString(): string | null {
  try {
    // Narrowed locally rather than read off `CloudflareEnv`. That global is
    // written by `wrangler types` into a generated, gitignored file, so it
    // carries the Hyperdrive binding on a developer's machine and an empty
    // interface anywhere the Cloudflare toolchain has not run — which is every
    // clean checkout, including the one that builds the self-hosted image.
    // Typing the single property this needs keeps the Node build from
    // depending on an artifact of the other target's build.
    const env = getCloudflareContext().env as unknown as {
      HYPERDRIVE?: { connectionString?: string };
    };
    return env.HYPERDRIVE?.connectionString ?? null;
  } catch {
    return null;
  }
}

function getConnectionString(): string {
  const connectionString = process.env.DATABASE_URL;

  if (!connectionString) {
    throw new Error("DATABASE_URL environment variable is not set");
  }

  return connectionString;
}

// Decide whether to require TLS to Postgres. Neon needs it; a local development
// Postgres usually has no TLS configured at all, and forcing "require" there
// fails the connection outright. `DATABASE_SSL` opts in/out explicitly.
//
// Hyperdrive terminates TLS to the origin database itself, so the hop the driver
// sees is already secure and must not negotiate TLS a second time.
function resolveSsl(usingHyperdrive: boolean): "require" | false {
  if (usingHyperdrive) return false;

  const mode = process.env.DATABASE_SSL?.toLowerCase();
  if (mode === "disable" || mode === "false" || mode === "off") return false;
  if (mode === "require" || mode === "true" || mode === "on") return "require";
  return process.env.NODE_ENV === "production" ? "require" : false;
}

// Singleton pattern: prevent creating multiple connection pools across
// serverless invocations sharing the same runtime (hot-start reuse).
//
// Use drizzle's config-based API to create the postgres client internally.
// Passing a `postgres` Sql instance directly causes type errors in the monorepo
// due to duplicate package resolution (two copies of postgres with incompatible
// branded types).
/**
 * How many sockets one client may open.
 *
 * Three shapes, and the wrong one is expensive in each direction:
 *
 * - Hyperdrive: these sockets end at Hyperdrive, which owns the real pool. The
 *   only thing this decides is whether a page's parallel queries actually run
 *   in parallel — `/u/[username]` fires three at once.
 * - Serverless without Hyperdrive: every socket is a real Postgres connection
 *   and dozens of concurrent cold starts exhaust `max_connections` (53300), so
 *   one apiece is the safe answer.
 * - Long-running server (the self-hosted image): there is exactly one process,
 *   it lives for the life of the container, and Postgres is on the other end of
 *   a loopback socket. Here `1` is actively wrong — it serialises every
 *   concurrent request in the whole application behind a single connection.
 *
 * `DB_POOL_MAX` selects the third case explicitly rather than inferring it, so
 * nothing silently changes shape when an environment variable goes missing.
 */
function resolvePoolMax(usingHyperdrive: boolean): number {
  const configured = Number(process.env.DB_POOL_MAX);
  if (Number.isInteger(configured) && configured > 0) return configured;
  return usingHyperdrive ? 3 : 1;
}

function createDb() {
  const hyperdriveUrl = getHyperdriveConnectionString();
  const usingHyperdrive = hyperdriveUrl !== null;
  const poolMax = resolvePoolMax(usingHyperdrive);

  // Prepared statements are connection-scoped, which makes them a liability
  // wherever the connection under a request is not stable: a serverless
  // invocation may lose the connection that prepared one, and Hyperdrive
  // multiplexes across connections — both surface as "prepared statement does
  // not exist". A dedicated pool in a long-running process has neither problem,
  // and the leaderboard runs the same few shapes over and over.
  const stableConnections = !usingHyperdrive && poolMax > 1;

  return drizzle({
    connection: {
      url: hyperdriveUrl ?? getConnectionString(),
      ssl: resolveSsl(usingHyperdrive),

      // Behind Hyperdrive these sockets terminate at Hyperdrive, not at
      // Postgres — it owns the real pool and its own origin_connection_limit
      // caps what the database ever sees. So the only thing `max` decides here
      // is whether the queries a single request issues in parallel actually run
      // in parallel: `/u/[username]` fires three at once and the leaderboard
      // two, and at max:1 they queued behind each other for no reason. Three
      // covers the widest page; Cloudflare advises staying at or below five
      // concurrent external connections per request.
      //
      // Without Hyperdrive the sockets are Postgres connections and the old
      // reasoning stands: dozens of concurrent cold-starts would exhaust
      // max_connections (error 53300), so that path stays at one. See
      // resolvePoolMax for why the self-hosted server is a third case.
      max: poolMax,

      // Idle sockets are waste in a serverless invocation and an asset in a
      // long-running server: reconnecting to loopback is cheap but not free,
      // and holding a warm pool is the entire point of having a process.
      idle_timeout: stableConnections ? 0 : 20,

      // Hard cap: recycle every connection after 5 minutes regardless of
      // activity. Prevents stale connections after deploys / DB restarts.
      // Left in place for the pooled case too — Postgres restarts under it
      // during migrations, and a recycled socket is how that heals itself.
      max_lifetime: 60 * 5,

      // Fail fast when the DB is unreachable instead of hanging the request.
      connect_timeout: 10,

      prepare: stableConnections,
    },
    schema,
  });
}

type DbClient = ReturnType<typeof createDb>;

const globalForDb = globalThis as unknown as {
  _db: DbClient | undefined;
};

/**
 * Per-request clients, keyed by the Cloudflare execution context.
 *
 * A Workers isolate is reused across requests, but the sockets inside a
 * connection pool belong to the request that opened them — touching one from a
 * later request is an error, which surfaced as intermittent 500s on the pages
 * that always hit the database (cached pages hid it by not querying at all).
 * Keying on the context object gives one client per request and lets the
 * garbage collector drop it with the request; Hyperdrive keeps the real pool
 * warm remotely, so building a client per request is cheap.
 */
const requestClients = new WeakMap<object, DbClient>();

export function getDb(): DbClient {
  let ctx: object | null = null;
  try {
    ctx = getCloudflareContext().ctx as unknown as object;
  } catch {
    // Not on Workers: a process-wide singleton is correct and cheaper.
  }

  if (!ctx) {
    if (!globalForDb._db) {
      globalForDb._db = createDb();
    }
    return globalForDb._db;
  }

  let client = requestClients.get(ctx);
  if (!client) {
    client = createDb();
    requestClients.set(ctx, client);
  }
  return client;
}

export const db: DbClient = new Proxy({} as DbClient, {
  get(_target, prop) {
    const value = Reflect.get(getDb(), prop);
    return typeof value === "function" ? value.bind(getDb()) : value;
  },
});

export * from "./schema";
