//! Crush session parser
//!
//! Crush persists usage in a per-project SQLite database (`crush.db`).
//! The database exposes reliable session-level cost, but not reliable
//! per-message token accounting for import.
//!
//! IMPORTANT: Crush is COST-ONLY. This parser intentionally emits ZERO token
//! counts (`TokenBreakdown::default()`) for every message and instead
//! distributes the reliable session-level cost across day buckets. There are
//! no trustworthy per-message token columns to populate, so a token-count
//! report showing 0 tokens for crush is EXPECTED behavior, NOT a bug — the
//! signal Crush provides is cost, not tokens.

use super::utils::open_readonly_sqlite;
use super::UnifiedMessage;
use crate::TokenBreakdown;
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const CRUSH_MODEL_ID: &str = "session-total";
const CRUSH_PROVIDER_ID: &str = "crush";

#[derive(Debug)]
struct CrushSession {
    id: String,
    cost: f64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DayBucket {
    timestamp_ms: i64,
    message_count: i32,
}

/// Parse root Crush sessions from a `crush.db` file.
///
/// Crush stores reliable cost at the root-session level, but does not expose a
/// stable per-message token breakdown. Tokens v1 therefore preserves cost
/// and assistant-message counts without fabricating token precision:
/// - assistant messages are grouped by local day
/// - session cost is allocated across those days proportionally
/// - token fields remain zero
pub fn parse_crush_sqlite(db_path: &Path) -> Vec<UnifiedMessage> {
    let Some(conn) = open_readonly_sqlite(db_path) else {
        return Vec::new();
    };

    let root_sessions = load_root_sessions(&conn);
    if root_sessions.is_empty() {
        return Vec::new();
    }

    let assistant_buckets = load_assistant_buckets(&conn);
    let db_namespace = db_path.to_string_lossy().to_string();
    let mut messages = Vec::new();

    for session in root_sessions {
        let session_key = format!("{}:{}", db_namespace, session.id);

        if let Some(day_buckets) = assistant_buckets.get(&session.id) {
            let total_assistant_messages: i32 =
                day_buckets.iter().map(|bucket| bucket.message_count).sum();
            let safe_cost = session.cost.max(0.0);
            let mut allocated_cost = 0.0;

            for (index, bucket) in day_buckets.iter().enumerate() {
                let bucket_cost = if index + 1 == day_buckets.len() {
                    (safe_cost - allocated_cost).max(0.0)
                } else {
                    safe_cost * f64::from(bucket.message_count)
                        / f64::from(total_assistant_messages)
                };
                allocated_cost += bucket_cost;

                let mut message = UnifiedMessage::new(
                    "crush",
                    CRUSH_MODEL_ID,
                    CRUSH_PROVIDER_ID,
                    session_key.clone(),
                    bucket.timestamp_ms,
                    TokenBreakdown::default(),
                    bucket_cost,
                );
                message.message_count = bucket.message_count.max(0);
                messages.push(message);
            }

            continue;
        }

        if session.cost <= 0.0 {
            continue;
        }

        let Some(timestamp_ms) =
            fallback_session_timestamp_ms(session.updated_at, session.created_at)
        else {
            continue;
        };

        let mut message = UnifiedMessage::new(
            "crush",
            CRUSH_MODEL_ID,
            CRUSH_PROVIDER_ID,
            session_key,
            timestamp_ms,
            TokenBreakdown::default(),
            session.cost.max(0.0),
        );
        message.message_count = 0;
        messages.push(message);
    }

    messages.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    messages
}

fn load_root_sessions(conn: &Connection) -> Vec<CrushSession> {
    let query = r#"
        SELECT id, cost, created_at, updated_at
        FROM sessions
        WHERE parent_session_id IS NULL
          AND (COALESCE(message_count, 0) > 0 OR COALESCE(cost, 0) > 0)
        ORDER BY created_at ASC
    "#;

    let mut stmt = match conn.prepare(query) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map([], |row| {
        Ok(CrushSession {
            id: row.get(0)?,
            cost: row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
            created_at: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            updated_at: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
        })
    }) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.flatten().collect()
}

fn load_assistant_buckets(conn: &Connection) -> HashMap<String, Vec<DayBucket>> {
    let query = r#"
        WITH RECURSIVE session_tree(root_session_id, session_id) AS (
            SELECT id, id
            FROM sessions
            WHERE parent_session_id IS NULL

            UNION ALL

            SELECT st.root_session_id, s.id
            FROM sessions s
            JOIN session_tree st ON s.parent_session_id = st.session_id
        )
        SELECT st.root_session_id, m.created_at
        FROM session_tree st
        JOIN messages m ON m.session_id = st.session_id
        WHERE m.role = 'assistant'
        ORDER BY st.root_session_id ASC, m.created_at ASC
    "#;

    let mut stmt = match conn.prepare(query) {
        Ok(stmt) => stmt,
        Err(_) => return HashMap::new(),
    };

    let rows = match stmt.query_map([], |row| {
        let session_id: String = row.get(0)?;
        let created_at: i64 = row.get::<_, Option<i64>>(1)?.unwrap_or(0);
        Ok((session_id, created_at))
    }) {
        Ok(rows) => rows,
        Err(_) => return HashMap::new(),
    };

    let mut session_days: HashMap<String, BTreeMap<String, DayBucket>> = HashMap::new();

    for row in rows.flatten() {
        let (session_id, created_at) = row;
        let Some(timestamp_ms) = normalize_crush_timestamp_ms(created_at) else {
            continue;
        };
        let Some(local_day) = local_day_key(timestamp_ms) else {
            continue;
        };

        let day_map = session_days.entry(session_id).or_default();
        let bucket = day_map.entry(local_day).or_insert(DayBucket {
            timestamp_ms,
            message_count: 0,
        });
        bucket.timestamp_ms = bucket.timestamp_ms.min(timestamp_ms);
        bucket.message_count = bucket.message_count.saturating_add(1);
    }

    session_days
        .into_iter()
        .map(|(session_id, day_map)| (session_id, day_map.into_values().collect()))
        .collect()
}

fn normalize_crush_timestamp_ms(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }

    if raw >= 100_000_000_000 {
        Some(raw)
    } else {
        raw.checked_mul(1000)
    }
}

fn local_day_key(timestamp_ms: i64) -> Option<String> {
    let date = crate::bucket_tz::bucket_timezone().date_of_ms(timestamp_ms);
    if date.is_empty() {
        None
    } else {
        Some(date)
    }
}

fn fallback_session_timestamp_ms(updated_at: i64, created_at: i64) -> Option<i64> {
    normalize_crush_timestamp_ms(updated_at).or_else(|| normalize_crush_timestamp_ms(created_at))
}

