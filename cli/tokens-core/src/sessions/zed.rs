//! Zed Agent session parser
//!
//! Parses hosted Zed Agent thread rows from Zed's SQLite database:
//! - Linux/FreeBSD: `$XDG_DATA_HOME/zed/threads/threads.db`
//! - macOS: `~/Library/Application Support/Zed/threads/threads.db`
//! - Windows: `%LOCALAPPDATA%\Zed\threads\threads.db`
//!
//! Only Zed-hosted model rows (`provider == "zed.dev"`) are counted. External
//! ACP agents are billed and logged by their own providers/CLIs, and counting
//! their Zed UI rows would duplicate those sources.

use super::utils::parse_timestamp_str;
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use tracing::warn;

pub(crate) const ZED_HOSTED_PROVIDER: &str = "zed.dev";
const MAX_ZED_THREAD_JSON_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug)]
struct ZedThreadRow {
    id: String,
    updated_at: String,
    created_at: Option<String>,
    folder_paths: Option<String>,
    folder_paths_order: Option<String>,
    data_type: String,
    data: Vec<u8>,
}

pub fn parse_zed_sqlite(db_path: &Path) -> Vec<UnifiedMessage> {
    let conn = match Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => conn,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to open Zed threads database"
            );
            return Vec::new();
        }
    };

    let query = build_threads_query(&conn);
    let mut stmt = match conn.prepare(&query) {
        Ok(stmt) => stmt,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to prepare Zed thread query"
            );
            return Vec::new();
        }
    };

    let rows = match stmt.query_map([], |row| {
        Ok(ZedThreadRow {
            id: row.get(0)?,
            updated_at: row.get(1)?,
            created_at: row.get(2)?,
            folder_paths: row.get(3)?,
            folder_paths_order: row.get(4)?,
            data_type: row.get(5)?,
            data: row.get(6)?,
        })
    }) {
        Ok(rows) => rows,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to execute Zed thread query"
            );
            return Vec::new();
        }
    };

    rows.filter_map(|row| match row {
        Ok(row) => parse_thread_row(db_path, row),
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to decode Zed thread row"
            );
            None
        }
    })
    .collect()
}

fn build_threads_query(conn: &Connection) -> String {
    let columns = thread_columns(conn);
    let created_at = optional_column(&columns, "created_at");
    let folder_paths = optional_column(&columns, "folder_paths");
    let folder_paths_order = optional_column(&columns, "folder_paths_order");

    format!(
        "SELECT id, updated_at, {created_at}, {folder_paths}, {folder_paths_order}, data_type, data FROM threads"
    )
}

fn optional_column(columns: &HashSet<String>, column: &'static str) -> &'static str {
    if columns.contains(column) {
        column
    } else {
        "NULL"
    }
}

fn thread_columns(conn: &Connection) -> HashSet<String> {
    let mut stmt = match conn.prepare("PRAGMA table_info(threads)") {
        Ok(stmt) => stmt,
        Err(_) => return HashSet::new(),
    };

    let rows = match stmt.query_map([], |row| row.get::<_, String>(1)) {
        Ok(rows) => rows,
        Err(_) => return HashSet::new(),
    };

    rows.filter_map(Result::ok).collect()
}

fn parse_thread_row(db_path: &Path, row: ZedThreadRow) -> Option<UnifiedMessage> {
    let json = match decode_thread_json(&row.data_type, &row.data) {
        Ok(json) => json,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                thread_id = %row.id,
                error = %err,
                "Failed to decode Zed thread payload"
            );
            return None;
        }
    };

    let thread: Value = match serde_json::from_slice(&json) {
        Ok(thread) => thread,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                thread_id = %row.id,
                error = %err,
                "Failed to parse Zed thread JSON"
            );
            return None;
        }
    };

    if thread
        .get("imported")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }

    let model = thread.get("model")?;
    let provider = model.get("provider")?.as_str()?.trim();
    if !provider.eq_ignore_ascii_case(ZED_HOSTED_PROVIDER) {
        return None;
    }

    let model_id = model.get("model")?.as_str()?.trim();
    if model_id.is_empty() {
        return None;
    }

    let (tokens, message_count) = thread_usage(&thread)?;
    let timestamp = timestamp_ms(&row, &thread)?;

    let mut message = UnifiedMessage::new_with_dedup(
        "zed",
        model_id,
        ZED_HOSTED_PROVIDER,
        row.id.clone(),
        timestamp,
        tokens,
        0.0,
        Some(format!("zed:{}", row.id)),
    );
    message.message_count = message_count;

    if let Some(workspace_key) = workspace_key_from_folders(
        row.folder_paths.as_deref(),
        row.folder_paths_order.as_deref(),
    ) {
        let workspace_label = workspace_label_from_key(&workspace_key);
        message.set_workspace(Some(workspace_key), workspace_label);
    }

    Some(message)
}

fn decode_thread_json(data_type: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    match data_type.trim().to_ascii_lowercase().as_str() {
        "json" => {
            if data.len() as u64 > MAX_ZED_THREAD_JSON_BYTES {
                return Err(format!(
                    "decoded thread payload exceeds {} bytes",
                    MAX_ZED_THREAD_JSON_BYTES
                ));
            }
            Ok(data.to_vec())
        }
        "zstd" => {
            let decoder = zstd::Decoder::new(data).map_err(|err| err.to_string())?;
            let mut decoded = Vec::new();
            decoder
                .take(MAX_ZED_THREAD_JSON_BYTES + 1)
                .read_to_end(&mut decoded)
                .map_err(|err| err.to_string())?;
            if decoded.len() as u64 > MAX_ZED_THREAD_JSON_BYTES {
                return Err(format!(
                    "decoded thread payload exceeds {} bytes",
                    MAX_ZED_THREAD_JSON_BYTES
                ));
            }
            Ok(decoded)
        }
        other => Err(format!("unsupported data_type {other:?}")),
    }
}

fn thread_usage(thread: &Value) -> Option<(TokenBreakdown, i32)> {
    let (request_usage, request_count) = sum_request_token_usage(thread.get("request_token_usage"));
    if request_usage.total() > 0 {
        return Some((request_usage, request_count.max(1)));
    }

    let cumulative = token_usage_from_value(thread.get("cumulative_token_usage")?)?;
    if cumulative.total() > 0 {
        Some((cumulative, 1))
    } else {
        None
    }
}

fn sum_request_token_usage(value: Option<&Value>) -> (TokenBreakdown, i32) {
    let mut total = TokenBreakdown::default();
    let mut count = 0_i32;

    let Some(value) = value else {
        return (total, count);
    };

    let usages: Box<dyn Iterator<Item = &Value> + '_> = match value {
        Value::Object(map) => Box::new(map.values()),
        Value::Array(values) => Box::new(values.iter()),
        _ => return (total, count),
    };

    for usage_value in usages {
        let Some(usage) = token_usage_from_value(usage_value) else {
            continue;
        };
        if usage.total() <= 0 {
            continue;
        }
        total.input = total.input.saturating_add(usage.input);
        total.output = total.output.saturating_add(usage.output);
        total.cache_read = total.cache_read.saturating_add(usage.cache_read);
        total.cache_write = total.cache_write.saturating_add(usage.cache_write);
        total.reasoning = total.reasoning.saturating_add(usage.reasoning);
        count = count.saturating_add(1);
    }

    (total, count)
}

// Zed persists `language_model::TokenUsage`, which currently stores only
// input/output/cache fields in `threads.db`. Until upstream adds a dedicated
// reasoning token field there, `reasoning` stays zero in Tokens.
fn token_usage_from_value(value: &Value) -> Option<TokenBreakdown> {
    Some(TokenBreakdown {
        input: usage_field(value, "input_tokens"),
        output: usage_field(value, "output_tokens"),
        cache_read: usage_field(value, "cache_read_input_tokens"),
        cache_write: usage_field(value, "cache_creation_input_tokens"),
        reasoning: 0,
    })
}

fn usage_field(value: &Value, field: &str) -> i64 {
    let Some(value) = value.get(field) else {
        return 0;
    };

    let parsed = value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| i64::try_from(n).unwrap_or(i64::MAX)))
        .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
        .unwrap_or(0);

    parsed.max(0)
}

fn timestamp_ms(row: &ZedThreadRow, thread: &Value) -> Option<i64> {
    row.created_at
        .as_deref()
        .and_then(parse_timestamp_str)
        .or_else(|| parse_timestamp_str(&row.updated_at))
        .or_else(|| {
            thread
                .get("updated_at")
                .and_then(Value::as_str)
                .and_then(parse_timestamp_str)
        })
}

fn workspace_key_from_folders(paths: Option<&str>, order: Option<&str>) -> Option<String> {
    let paths: Vec<&str> = paths?
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .collect();
    if paths.is_empty() {
        return None;
    }

    let selected = order
        .and_then(|order| first_ordered_path_index(order, paths.len()))
        .and_then(|index| paths.get(index).copied())
        .unwrap_or(paths[0]);

    normalize_workspace_key(selected)
}

fn first_ordered_path_index(order: &str, path_count: usize) -> Option<usize> {
    order
        .split(',')
        .map(str::trim)
        .enumerate()
        .filter_map(|(index, order)| {
            let order = order.parse::<usize>().ok()?;
            (index < path_count).then_some((index, order))
        })
        .min_by_key(|(_, order)| *order)
        .map(|(index, _)| index)
}

