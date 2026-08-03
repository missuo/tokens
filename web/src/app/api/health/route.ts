import { sql } from "drizzle-orm";
import { NextResponse } from "next/server";
import { getDb } from "@/lib/db";

export const dynamic = "force-dynamic";

/**
 * Liveness for the container healthcheck, and the signal a load balancer would
 * use to decide this origin is gone.
 *
 * It checks the database rather than only answering 200, because the failure
 * this needs to catch is not "the process died" — Docker already restarts on
 * that. It is the process still accepting connections while every page behind
 * it 500s, which is what a Postgres that has not come back after a restart
 * looks like from outside.
 *
 * Deliberately says nothing about versions, table counts, or connection
 * strings: it is reachable without authentication, so it reports exactly one
 * bit plus the latency that produced it.
 */
export async function GET() {
  const started = Date.now();
  try {
    await getDb().execute(sql`select 1`);
    return NextResponse.json(
      { ok: true, db: "up", latencyMs: Date.now() - started },
      { status: 200, headers: { "Cache-Control": "no-store" } },
    );
  } catch {
    return NextResponse.json(
      { ok: false, db: "down", latencyMs: Date.now() - started },
      { status: 503, headers: { "Cache-Control": "no-store" } },
    );
  }
}
