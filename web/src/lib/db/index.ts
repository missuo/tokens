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
    return getCloudflareContext().env.HYPERDRIVE?.connectionString ?? null;
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
function createDb() {
  const hyperdriveUrl = getHyperdriveConnectionString();
  const usingHyperdrive = hyperdriveUrl !== null;

  return drizzle({
    connection: {
      url: hyperdriveUrl ?? getConnectionString(),
      ssl: resolveSsl(usingHyperdrive),

      // Serverless-optimized pool settings:
      // Each isolate gets its own pool. Behind Hyperdrive the real pooling
      // happens remotely and one socket per isolate is enough; without it,
      // dozens of concurrent cold-starts at max:5 would exceed the database's
      // max_connections (error 53300).
      max: 1,

      // Close idle connections after 20 s so they don't linger between
      // infrequent invocations.
      idle_timeout: 20,

      // Hard cap: recycle every connection after 5 minutes regardless of
      // activity. Prevents stale connections after deploys / DB restarts.
      max_lifetime: 60 * 5,

      // Fail fast when the DB is unreachable instead of hanging the request.
      connect_timeout: 10,

      // Prepared statements are connection-scoped. In serverless the connection
      // that prepared a statement may be gone by the next invocation, and
      // Hyperdrive multiplexes requests across connections — both surface as
      // "prepared statement does not exist".
      prepare: false,
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
