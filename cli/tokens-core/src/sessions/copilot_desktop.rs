//! GitHub Copilot Desktop SQLite parser.
//!
//! The macOS desktop app stores aggregate token totals in `~/.copilot/data.db`
//! and per-session event metadata in `~/.copilot/session-state/{session_id}`.

use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::provider_identity::inferred_provider_from_model;
use chrono::{DateTime, NaiveDateTime};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::warn;

#[derive(Debug)]
struct CopilotDesktopSessionRow {
    id: String,
    model: Option<String>,
    total_input_tokens: i64,
    total_output_tokens: i64,
    total_cached_tokens: i64,
    total_reasoning_tokens: i64,
    created_at: Option<String>,
    is_forked: bool,
}

#[derive(Debug, Default)]
struct SessionStateMetadata {
    model: Option<String>,
    cwd: Option<String>,
    assistant_message_count: i32,
}

fn forked_session_sql_expression(conn: &Connection) -> &'static str {
    if conn
        .prepare("SELECT forked_from_session_id FROM sessions LIMIT 0")
        .is_ok()
    {
        "forked_from_session_id IS NOT NULL"
    } else {
        "0"
    }
}

pub fn parse_copilot_desktop_db(db_path: &Path) -> Vec<UnifiedMessage> {
    let conn = match Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => conn,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to open Copilot Desktop database"
            );
            return Vec::new();
        }
    };

    // Fork metadata was added after the original sessions schema. Select a
    // constant fallback for older databases instead of making the whole parser
    // fail when the column is absent.
    let forked_expression = forked_session_sql_expression(&conn);
    let sessions_query = format!(
        r#"
        SELECT
            id,
            title,
            model,
            total_input_tokens,
            total_output_tokens,
            total_cached_tokens,
            total_reasoning_tokens,
            total_nano_aiu,
            created_at,
            {forked_expression} AS is_forked
        FROM sessions
        WHERE total_input_tokens > 0
           OR total_output_tokens > 0
           OR total_cached_tokens > 0
           OR total_reasoning_tokens > 0
        "#
    );

    let mut stmt = match conn.prepare(&sessions_query) {
        Ok(stmt) => stmt,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to prepare Copilot Desktop sessions query"
            );
            return Vec::new();
        }
    };

    let rows = match stmt.query_map([], |row| {
        Ok(CopilotDesktopSessionRow {
            id: row.get(0)?,
            model: row.get(2)?,
            total_input_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            total_output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
            total_cached_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            total_reasoning_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            created_at: row.get(8)?,
            is_forked: row.get::<_, i64>(9)? != 0,
        })
    }) {
        Ok(rows) => rows,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to execute Copilot Desktop sessions query"
            );
            return Vec::new();
        }
    };

    rows.filter_map(|row| match row {
        Ok(row) => Some(session_row_to_message(db_path, row)),
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to decode Copilot Desktop session row"
            );
            None
        }
    })
    .collect()
}

fn session_row_to_message(db_path: &Path, row: CopilotDesktopSessionRow) -> UnifiedMessage {
    let metadata = read_session_state_metadata(db_path, &row.id, !row.is_forked);
    let model_id = metadata
        .model
        .as_deref()
        .or(row.model.as_deref())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or("auto")
        .to_string();
    let provider_id = inferred_provider_from_model(&model_id)
        .unwrap_or("github-copilot")
        .to_string();

    let timestamp_ms = row
        .created_at
        .as_deref()
        .and_then(parse_iso8601_timestamp_ms)
        .unwrap_or_else(|| {
            warn!(
                session_id = %row.id,
                created_at = ?row.created_at,
                "Copilot Desktop session has unparseable created_at; defaulting to 0"
            );
            0
        });

    let mut message = UnifiedMessage::new_with_dedup(
        "copilot",
        model_id,
        provider_id,
        row.id.clone(),
        timestamp_ms,
        // Copilot reports input tokens inclusive of cache reads (same convention
        // as the OTEL exporter that feeds this same session data). Reuse the
        // shared normalizer so the desktop-DB and OTEL paths never diverge and
        // additive pricing does not double-charge the cached portion.
        super::copilot::normalize_input_tokens(
            row.total_input_tokens,
            row.total_output_tokens,
            row.total_cached_tokens,
            0,
            row.total_reasoning_tokens,
        ),
        0.0,
        Some(format!("copilot-desktop:{}", row.id)),
    );

    if let Some(workspace_key) = metadata.cwd.as_deref().and_then(normalize_workspace_key) {
        let workspace_label = workspace_label_from_key(&workspace_key);
        message.set_workspace(Some(workspace_key), workspace_label);
    }

    // The database row contains aggregate usage for the whole session, so the
    // UnifiedMessage default of one would otherwise count sessions instead of
    // assistant messages. Preserve that default as a compatibility fallback
    // when the event history is unavailable, does not expose message events,
    // or belongs to a fork that persists inherited parent history.
    if metadata.assistant_message_count > 0 {
        message.message_count = metadata.assistant_message_count;
    }

    message
}

fn read_session_state_metadata(
    db_path: &Path,
    session_id: &str,
    count_assistant_messages: bool,
) -> SessionStateMetadata {
    let Some(copilot_root) = db_path.parent() else {
        return SessionStateMetadata::default();
    };
    let events_path = copilot_root
        .join("session-state")
        .join(session_id)
        .join("events.jsonl");

    read_events_metadata(&events_path, count_assistant_messages)
}

fn read_events_metadata(
    events_path: &Path,
    count_assistant_messages: bool,
) -> SessionStateMetadata {
    let file = match std::fs::File::open(events_path) {
        Ok(file) => file,
        Err(_) => return SessionStateMetadata::default(),
    };

    let mut metadata = SessionStateMetadata::default();
    let mut seen_assistant_messages = HashSet::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(event) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let Some(event_type) = event.get("type").and_then(Value::as_str) else {
            continue;
        };

        match event_type {
            "assistant.message" if count_assistant_messages => {
                let message_id = event
                    .pointer("/data/messageId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .or_else(|| {
                        event
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|id| !id.is_empty())
                    });

                if message_id.is_none_or(|id| seen_assistant_messages.insert(id.to_string())) {
                    metadata.assistant_message_count =
                        metadata.assistant_message_count.saturating_add(1);
                }
            }
            "session.start" if metadata.cwd.is_none() => {
                metadata.cwd = event
                    .pointer("/data/context/cwd")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|cwd| !cwd.is_empty())
                    .map(str::to_string);
            }
            "session.model_change" => {
                if let Some(model) = event
                    .pointer("/data/newModel")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|model| !model.is_empty() && model != &"auto")
                {
                    metadata.model = Some(model.to_string());
                }
            }
            _ => {}
        }
    }

    metadata
}

fn parse_iso8601_timestamp_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.timestamp_millis())
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|timestamp| timestamp.and_utc().timestamp_millis())
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|timestamp| timestamp.and_utc().timestamp_millis())
        })
        .or_else(|| {
            // SQLite's default datetime() text form is space-separated and may
            // carry fractional seconds ("2026-07-01 12:34:56.789"); without this
            // branch it fails every parse above and the session lands in 1970.
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .map(|timestamp| timestamp.and_utc().timestamp_millis())
        })
        .or_else(|| {
            let numeric = value.parse::<i64>().ok()?;
            // Distinguish seconds vs milliseconds: values < 10 billion are
            // assumed to be Unix seconds (common in SQLite), otherwise millis.
            if numeric > 10_000_000_000 {
                Some(numeric)
            } else {
                Some(numeric.saturating_mul(1000))
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn session_row(id: &str) -> CopilotDesktopSessionRow {
        CopilotDesktopSessionRow {
            id: id.to_string(),
            model: Some("claude-sonnet-4".to_string()),
            total_input_tokens: 100,
            total_output_tokens: 20,
            total_cached_tokens: 30,
            total_reasoning_tokens: 5,
            created_at: Some("2026-08-06T00:00:00Z".to_string()),
            is_forked: false,
        }
    }

    fn write_session_events(temp_dir: &TempDir, session_id: &str, events: &str) {
        let session_dir = temp_dir.path().join("session-state").join(session_id);
        fs::create_dir_all(&session_dir).expect("create session-state fixture");
        fs::write(session_dir.join("events.jsonl"), events).expect("write events fixture");
    }

    #[test]
    fn counts_unique_assistant_messages_on_the_session_aggregate() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let session_id = "session-with-events";
        write_session_events(
            &temp_dir,
            session_id,
            concat!(
                "{\"type\":\"session.start\",\"data\":{\"context\":{\"cwd\":\"/tmp/project\"}}}\n",
                "{\"type\":\"assistant.message\",\"data\":{\"messageId\":\"message-1\"},\"id\":\"event-1\"}\n",
                "{\"type\":\"assistant.message\",\"data\":{\"messageId\":\"message-1\"},\"id\":\"event-replay\"}\n",
                "{\"type\":\"assistant.message\",\"data\":{\"messageId\":\"message-2\"},\"id\":\"event-2\"}\n",
                "{\"type\":\"session.compaction_complete\",\"id\":\"compaction-1\"}\n",
                "not-json\n",
            ),
        );

        let message =
            session_row_to_message(&temp_dir.path().join("data.db"), session_row(session_id));

        assert_eq!(message.message_count, 2);
        assert_eq!(message.timestamp, 1_785_974_400_000);
        assert_eq!(message.tokens.input, 70);
        assert_eq!(message.tokens.output, 20);
        assert_eq!(message.tokens.cache_read, 30);
        assert_eq!(message.tokens.reasoning, 5);
        assert_eq!(message.workspace_key.as_deref(), Some("/tmp/project"));
    }

    #[test]
    fn falls_back_to_one_message_without_session_events() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let message = session_row_to_message(
            &temp_dir.path().join("data.db"),
            session_row("session-without-events"),
        );

        assert_eq!(message.message_count, 1);
    }

    #[test]
    fn falls_back_to_event_id_for_blank_message_ids() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let session_id = "session-with-blank-message-ids";
        write_session_events(
            &temp_dir,
            session_id,
            concat!(
                "{\"type\":\"assistant.message\",\"data\":{\"messageId\":\"  \"},\"id\":\"event-1\"}\n",
                "{\"type\":\"assistant.message\",\"data\":{\"messageId\":\"\"},\"id\":\"event-1\"}\n",
                "{\"type\":\"assistant.message\",\"data\":{\"messageId\":\"\"},\"id\":\"event-2\"}\n",
            ),
        );

        let metadata =
            read_session_state_metadata(&temp_dir.path().join("data.db"), session_id, true);

        assert_eq!(metadata.assistant_message_count, 2);
    }

    #[test]
    fn forked_sessions_keep_the_compatibility_message_count() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let session_id = "forked-session";
        write_session_events(
            &temp_dir,
            session_id,
            concat!(
                "{\"type\":\"session.start\",\"data\":{\"context\":{\"cwd\":\"/tmp/fork-project\"}}}\n",
                "{\"type\":\"assistant.message\",\"data\":{\"messageId\":\"inherited-message\"},\"id\":\"event-1\"}\n",
                "{\"type\":\"assistant.message\",\"data\":{\"messageId\":\"child-message\"},\"id\":\"event-2\"}\n",
            ),
        );
        let mut row = session_row(session_id);
        row.is_forked = true;

        let message = session_row_to_message(&temp_dir.path().join("data.db"), row);

        assert_eq!(message.message_count, 1);
        assert_eq!(message.workspace_key.as_deref(), Some("/tmp/fork-project"));
    }

    #[test]
    fn selects_a_non_fork_fallback_for_legacy_schema() {
        let conn = Connection::open_in_memory().expect("create in-memory database");
        conn.execute("CREATE TABLE sessions (id TEXT)", [])
            .expect("create legacy sessions table");

        assert_eq!(forked_session_sql_expression(&conn), "0");

        conn.execute(
            "ALTER TABLE sessions ADD COLUMN forked_from_session_id TEXT",
            [],
        )
        .expect("add fork metadata column");
        assert_eq!(
            forked_session_sql_expression(&conn),
            "forked_from_session_id IS NOT NULL"
        );
    }
}
