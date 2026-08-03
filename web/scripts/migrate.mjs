// Applies pending Drizzle migrations, then exits.
//
// Runs as its own one-shot container ahead of the app rather than from the
// app's entrypoint. That ordering is the whole point: deploying code before its
// migration has already taken this site down once — `publicProfileData` queried
// a table that did not exist yet and every profile page 500'd. Compose gates
// the app on this container exiting 0, so the new code cannot start against an
// old schema.
//
// Uses drizzle-orm's migrator rather than drizzle-kit. drizzle-kit is a
// devDependency and pulls in a compiler toolchain; the migrator is part of the
// runtime dependency the app already ships, and it reads the same journal, so
// the two remain interchangeable.
import postgres from "postgres";
import { drizzle } from "drizzle-orm/postgres-js";
import { migrate } from "drizzle-orm/postgres-js/migrator";

const url = process.env.DATABASE_URL;
if (!url) {
  console.error("[migrate] DATABASE_URL is not set");
  process.exit(1);
}

// `max: 1` because migrations must run in order on one connection, and
// `prepare: false` because a failed migration can leave a statement cached
// against a schema that no longer matches it.
const sql = postgres(url, { max: 1, prepare: false, connect_timeout: 30 });

try {
  await migrate(drizzle(sql), { migrationsFolder: "./migrations" });
  console.log("[migrate] up to date");
  await sql.end();
} catch (error) {
  console.error("[migrate] failed:", error);
  // Close the socket before exiting so Postgres does not log an aborted
  // connection on top of whatever actually went wrong.
  await sql.end({ timeout: 5 }).catch(() => {});
  process.exit(1);
}
