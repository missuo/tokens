//! Hermes Agent session parser
//!
//! Parses aggregated session rows from Hermes Agent's SQLite state database:
//! - `~/.hermes/state.db`
//! - `$HERMES_HOME/state.db`
//!
//! Hermes stores usage only as lifetime totals per session (and per model in
//! `session_model_usage`), never per API call. Each total is spread across the
//! assistant messages that fall in its time window, so a session that runs
//! for days books its usage on the days it was used rather than the day it
//! started (missuo/tokens#129).

use super::utils::{
    open_readonly_sqlite, resolved_provider, sqlite_for_each_row_on, timestamp_secs_to_ms,
};
use super::UnifiedMessage;
use crate::TokenBreakdown;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::warn;

const HERMES_AGENT_NAME: &str = "Hermes Agent";

/// Stands in for a NULL `billing_provider` inside a per-model dedup key. Angle
/// brackets keep it out of the slug space real provider ids live in.
const NULL_PROVIDER_KEY: &str = "<null>";

/// Per-model breakdown, available once Hermes started recording
/// `session_model_usage`.
///
/// Grouping keeps `billing_provider` because the composite primary key lets a
/// single (session, model) pair span several providers; collapsing them would
/// erase one provider from the breakdown and credit its tokens to the other.
/// Cost is resolved per row (actual when non-zero, otherwise estimated) before
/// the SUM, so a reconciled sibling row cannot discard estimate-only siblings.
/// `ORDER BY` sorts the row matching `sessions.model` first within each session
/// and is otherwise total, so the caller can hand `sessions.message_count` to
/// the first row it sees per session and get the primary model whenever one
/// survives grouping — and a deterministic row when none does.
const PER_MODEL_QUERY: &str = r#"
        SELECT
            smu.session_id,
            smu.model,
            smu.billing_provider,
            s.started_at,
            COALESCE(s.message_count, 0) AS message_count,
            SUM(smu.input_tokens)        AS input_tokens,
            SUM(smu.output_tokens)       AS output_tokens,
            SUM(smu.cache_read_tokens)   AS cache_read_tokens,
            SUM(smu.cache_write_tokens)  AS cache_write_tokens,
            SUM(smu.reasoning_tokens)    AS reasoning_tokens,
            SUM(COALESCE(NULLIF(smu.actual_cost_usd, 0), smu.estimated_cost_usd, 0)) AS cost_usd,
            MIN(smu.first_seen)          AS first_seen,
            MAX(smu.last_seen)           AS last_seen
        FROM session_model_usage smu
        JOIN sessions s ON s.id = smu.session_id
        WHERE smu.model IS NOT NULL
          AND TRIM(smu.model) != ''
        GROUP BY smu.session_id, smu.model, smu.billing_provider, s.started_at, s.message_count
        HAVING SUM(smu.input_tokens) > 0
            OR SUM(smu.output_tokens) > 0
            OR SUM(smu.cache_read_tokens) > 0
            OR SUM(smu.cache_write_tokens) > 0
            OR SUM(smu.reasoning_tokens) > 0
            OR SUM(COALESCE(NULLIF(smu.actual_cost_usd, 0), smu.estimated_cost_usd, 0)) > 0
        ORDER BY smu.session_id,
                 CASE WHEN smu.model = s.model THEN 0 ELSE 1 END,
                 smu.model,
                 smu.billing_provider
"#;

/// Session-level totals credited to `sessions.model`. Covers Hermes builds that
/// predate `session_model_usage` and, on builds that have it, sessions with no
/// child rows in it; column order matches `PER_MODEL_QUERY`. `sessions` has no
/// usage window, so the whole session from `started_at` on is the window.
const SESSION_TOTALS_QUERY: &str = r#"
        SELECT
            id,
            model,
            billing_provider,
            started_at,
            message_count,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            reasoning_tokens,
            COALESCE(actual_cost_usd, estimated_cost_usd, 0) AS cost_usd,
            NULL AS first_seen,
            NULL AS last_seen
        FROM sessions
        WHERE model IS NOT NULL
          AND TRIM(model) != ''
          AND (
            COALESCE(input_tokens, 0) > 0 OR
            COALESCE(output_tokens, 0) > 0 OR
            COALESCE(cache_read_tokens, 0) > 0 OR
            COALESCE(cache_write_tokens, 0) > 0 OR
            COALESCE(reasoning_tokens, 0) > 0 OR
            COALESCE(actual_cost_usd, estimated_cost_usd, 0) > 0
          )
"#;

/// Every assistant message time per session, the only per-call time signal
/// Hermes keeps. `messages` has carried `session_id`, `role` and `timestamp`
/// since its first schema. Compaction re-inserts surviving rows as clones with
/// the original timestamp, so DISTINCT keeps one API call from counting once
/// per generation. Inactive (compacted or rewound) rows stay: each still
/// stands for a call that was billed.
const ASSISTANT_TIMESTAMPS_QUERY: &str = r#"
        SELECT DISTINCT session_id, timestamp
        FROM messages
        WHERE role = 'assistant'
          AND typeof(timestamp) IN ('integer', 'real')
        ORDER BY session_id, timestamp
"#;

/// One usage row, decoded from either query — both project the same columns.
struct HermesUsageRow {
    session_id: String,
    model_id: String,
    billing_provider: Option<String>,
    started_at: f64,
    message_count: i32,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    cost: f64,
    /// When Hermes first and last recorded usage for this row (epoch seconds);
    /// NULL for session totals.
    first_seen: Option<f64>,
    last_seen: Option<f64>,
}

fn decode_usage_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HermesUsageRow> {
    Ok(HermesUsageRow {
        session_id: row.get(0)?,
        model_id: row.get(1)?,
        billing_provider: row.get(2)?,
        started_at: row.get(3)?,
        message_count: row.get::<_, Option<i32>>(4)?.unwrap_or(0),
        input: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        output: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
        cache_read: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
        cache_write: row.get::<_, Option<i64>>(8)?.unwrap_or(0),
        reasoning: row.get::<_, Option<i64>>(9)?.unwrap_or(0),
        cost: row.get::<_, Option<f64>>(10)?.unwrap_or(0.0),
        first_seen: row.get(11)?,
        last_seen: row.get(12)?,
    })
}

/// Runs one of the usage queries. Returns `None` when the query cannot run at
/// all (prepare or execute failure) so the caller can fall back, and `Some` —
/// possibly empty — when it ran and simply matched nothing.
fn query_usage_rows(db_path: &Path, conn: &Connection, query: &str) -> Option<Vec<HermesUsageRow>> {
    let mut rows = Vec::new();
    let scan = sqlite_for_each_row_on(conn, db_path, query, Some("Hermes session"), &mut |row| {
        rows.push(decode_usage_row(row)?);
        Ok(())
    });
    scan.ran().then_some(rows)
}

/// Assistant message times per session, ascending. Empty when the table is
/// missing or unreadable, which leaves every row on its fallback time.
fn query_assistant_timestamps(db_path: &Path, conn: &Connection) -> HashMap<String, Vec<f64>> {
    let mut timestamps: HashMap<String, Vec<f64>> = HashMap::new();
    let scan = sqlite_for_each_row_on(
        conn,
        db_path,
        ASSISTANT_TIMESTAMPS_QUERY,
        Some("Hermes message timestamps"),
        &mut |row| {
            timestamps.entry(row.get(0)?).or_default().push(row.get(1)?);
            Ok(())
        },
    );
    if scan.ran() {
        timestamps
    } else {
        HashMap::new()
    }
}

/// The assistant messages a row's usage is spread over: those inside the
/// window in which Hermes recorded that usage. Bounding by the window keeps a
/// model's tokens off messages answered by another model of the same session,
/// and keeps a session rotated out of a compaction from spreading onto the
/// parent messages it carries over with their original timestamps.
fn usage_window<'a>(row: &HermesUsageRow, timestamps: &'a [f64]) -> &'a [f64] {
    let lo = row.first_seen.unwrap_or(row.started_at);
    let hi = row.last_seen.unwrap_or(f64::INFINITY);
    let start = timestamps.partition_point(|&t| t < lo);
    let end = timestamps.partition_point(|&t| t <= hi).max(start);
    &timestamps[start..end]
}

/// Piece `index` of `total` split into `pieces` near-equal integer shares.
/// Differences of cumulative floors sum back to exactly `total`.
fn integer_share(total: i64, index: usize, pieces: usize) -> i64 {
    let cumulative = |k: usize| (i128::from(total) * k as i128 / pieces as i128) as i64;
    cumulative(index + 1) - cumulative(index)
}

/// Splits one usage row into a message per assistant message in its window,
/// each carrying an equal share of every token bucket and of the cost. A row
/// with no message in its window stays whole at the time its usage was first
/// recorded, or at `started_at` when that is unknown.
fn push_spread_messages(
    row: HermesUsageRow,
    row_key: &str,
    timestamps: &[f64],
    messages: &mut Vec<UnifiedMessage>,
) {
    let fallback = [row.first_seen.unwrap_or(row.started_at)];
    let window = usage_window(&row, timestamps);
    let times = if window.is_empty() {
        &fallback[..]
    } else {
        window
    };
    let pieces = times.len();

    let provider = resolved_provider(row.billing_provider, &row.model_id, "hermes");
    let tokens = TokenBreakdown {
        input: row.input.max(0),
        output: row.output.max(0),
        cache_read: row.cache_read.max(0),
        cache_write: row.cache_write.max(0),
        reasoning: row.reasoning.max(0),
    };
    let cost = row.cost.max(0.0);
    let cumulative_cost = |k: usize| cost * k as f64 / pieces as f64;

    for (index, &timestamp) in times.iter().enumerate() {
        let piece_tokens = TokenBreakdown {
            input: integer_share(tokens.input, index, pieces),
            output: integer_share(tokens.output, index, pieces),
            cache_read: integer_share(tokens.cache_read, index, pieces),
            cache_write: integer_share(tokens.cache_write, index, pieces),
            reasoning: integer_share(tokens.reasoning, index, pieces),
        };
        let piece_cost = if index + 1 == pieces {
            cost - cumulative_cost(index)
        } else {
            cumulative_cost(index + 1) - cumulative_cost(index)
        };
        // `sessions.message_count` stays on one piece, as it stayed on one
        // row: per-day message counts are unchanged, and the other pieces stay
        // count-neutral so time metrics still see one session.
        let message_count = if index == 0 {
            row.message_count.max(0)
        } else {
            0
        };
        if piece_tokens.total() == 0 && piece_cost <= 0.0 && message_count == 0 {
            continue;
        }

        let mut msg = UnifiedMessage::new_with_agent(
            "hermes",
            row.model_id.clone(),
            provider.clone(),
            row.session_id.clone(),
            timestamp_secs_to_ms(timestamp),
            piece_tokens,
            piece_cost.max(0.0),
            Some(HERMES_AGENT_NAME.to_string()),
        );
        msg.message_count = message_count;
        msg.dedup_key = Some(format!("{row_key}#{index}"));
        messages.push(msg);
    }
}

fn has_session_model_usage(db_path: &Path, conn: &Connection) -> bool {
    match conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'session_model_usage'",
        [],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(_) => true,
        Err(rusqlite::Error::QueryReturnedNoRows) => false,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to probe Hermes session_model_usage table; using session totals"
            );
            false
        }
    }
}

pub fn parse_hermes_sqlite(db_path: &Path) -> Vec<UnifiedMessage> {
    parse_hermes_databases(&[db_path])
}

/// Parses every Hermes database (the default home plus any profile or extra
/// path), crediting each usage row to the first database that holds it. The
/// dedup runs on rows, before they are spread: two copies of one session can
/// hold different numbers of messages, and deduping the spread pieces would
/// let the larger copy's extra pieces through on top of the first copy.
pub fn parse_hermes_databases<P: AsRef<Path>>(db_paths: &[P]) -> Vec<UnifiedMessage> {
    let mut seen_rows: HashSet<String> = HashSet::new();
    let mut messages = Vec::new();
    for db_path in db_paths {
        let db_path = db_path.as_ref();
        let Some(conn) = open_hermes_database(db_path) else {
            continue;
        };
        let rows = query_hermes_usage(db_path, &conn);
        if rows.is_empty() {
            continue;
        }
        let timestamps = query_assistant_timestamps(db_path, &conn);
        for (row, row_key) in rows {
            if !seen_rows.insert(row_key.clone()) {
                continue;
            }
            let session_timestamps = timestamps
                .get(&row.session_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            push_spread_messages(row, &row_key, session_timestamps, &mut messages);
        }
    }
    messages
}

fn open_hermes_database(db_path: &Path) -> Option<Connection> {
    match open_readonly_sqlite(db_path) {
        Ok(conn) => Some(conn),
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to open Hermes state database"
            );
            None
        }
    }
}

/// Every usage row of one database with the key it dedups under.
fn query_hermes_usage(db_path: &Path, conn: &Connection) -> Vec<(HermesUsageRow, String)> {
    // Hermes builds that predate `session_model_usage` cannot run the per-model
    // query at all; probing first keeps that expected case quiet instead of
    // logging a prepare failure for every such install. When the table does
    // exist but the query still fails to prepare — schema drift, e.g. an older
    // `session_model_usage` without `reasoning_tokens` — `query_usage_rows`
    // returns None and the session totals below carry the whole database, so
    // drift degrades to coarser data instead of losing all of it.
    let per_model_rows = if has_session_model_usage(db_path, conn) {
        query_usage_rows(db_path, conn, PER_MODEL_QUERY)
    } else {
        None
    };

    let mut rows: Vec<(HermesUsageRow, String)> = Vec::new();
    // Sessions the per-model pass emitted at least one row for.
    let mut covered_sessions: HashSet<String> = HashSet::new();
    // Sessions whose `message_count` has already been handed to a row.
    let mut counted_sessions: HashSet<String> = HashSet::new();

    for mut row in per_model_rows.unwrap_or_default() {
        covered_sessions.insert(row.session_id.clone());
        // `sessions.message_count` is a per-session total, so exactly one row
        // per session may carry it. ORDER BY sorts the row matching
        // `sessions.model` first, so "the first row of the session" is that
        // primary model when it survives grouping, and a deterministic
        // stand-in when it does not — a NULL `sessions.model`, a rename
        // between the two tables, or a primary model whose rows were all zero
        // and got filtered out. Zeroing every later row keeps the per-session
        // sum equal to `sessions.message_count` instead of dropping it.
        if !counted_sessions.insert(row.session_id.clone()) {
            row.message_count = 0;
        }
        // SQLite groups NULL and '' separately, so one (session, model) can
        // reach this loop as two rows that differ only in a NULL vs empty
        // billing_provider. Rendering NULL as a sentinel keeps their keys
        // apart; folding NULL to "" would collide and the row dedup in
        // `parse_hermes_databases` would then drop the second row's tokens
        // outright.
        let provider_key = row.billing_provider.as_deref().unwrap_or(NULL_PROVIDER_KEY);
        let dedup_key = format!(
            "hermes:{}:{}:{}",
            row.session_id, row.model_id, provider_key
        );
        rows.push((row, dedup_key));
    }

    // Reconcile per session, not per table: a Hermes upgrade can create
    // `session_model_usage` without backfilling older sessions, and those
    // sessions still carry their totals on `sessions`. Emit totals only for
    // sessions the per-model pass did not cover, which makes the two result
    // sets disjoint by construction — they cannot double count.
    //
    // Deliberately no "top up" of a session whose smu rows only partially cover
    // its session totals: if a session has ANY smu row we trust smu, because
    // adding the difference would double count the models smu did record. Do
    // not "fix" this into a per-column reconciliation.
    for row in query_usage_rows(db_path, conn, SESSION_TOTALS_QUERY).unwrap_or_default() {
        if covered_sessions.contains(&row.session_id) {
            continue;
        }
        // This path yields at most one row per session, so it keeps the bare
        // session id as its dedup key; the per-model keys are namespaced and
        // cannot collide with it.
        let dedup_key = row.session_id.clone();
        rows.push((row, dedup_key));
    }

    rows
}
