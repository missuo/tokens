//! MiMo Code session parser
//!
//! Parses messages from:
//! - SQLite database: ~/.local/share/mimocode/mimocode.db

use super::utils::open_readonly_sqlite;
use super::{
    normalize_opencode_agent_name, normalize_workspace_key, workspace_label_from_key,
    UnifiedMessage,
};
use crate::{provider_identity, TokenBreakdown};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// MiMo Code message structure (from SQLite data column)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MiMoCodeMessage {
    #[serde(default)]
    pub id: Option<String>,
    pub role: String,
    #[serde(rename = "modelID")]
    pub model_id: Option<String>,
    #[serde(rename = "providerID")]
    pub provider_id: Option<String>,
    pub cost: Option<f64>,
    pub tokens: Option<MiMoCodeTokens>,
    pub time: MiMoCodeTime,
    pub agent: Option<String>,
    pub mode: Option<String>,
    #[serde(default, deserialize_with = "deserialize_micode_path")]
    pub path: Option<MiMoCodePath>,
}

#[derive(Debug, Deserialize)]
pub struct MiMoCodePath {
    pub root: Option<String>,
}

fn deserialize_micode_path<'de, D>(deserializer: D) -> Result<Option<MiMoCodePath>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let root = value
        .get("root")
        .and_then(|root| root.as_str())
        .map(str::to_string);

    Ok(Some(MiMoCodePath { root }))
}

#[derive(Debug, Deserialize)]
pub struct MiMoCodeTokens {
    pub input: i64,
    pub output: i64,
    pub reasoning: Option<i64>,
    // MiMo assistant messages may omit `cache` (or its read/write); without a
    // default a missing field would fail deserialization and silently drop the
    // message in the parse loop's `Err(_) => continue` arm.
    #[serde(default)]
    pub cache: Option<MiMoCodeCache>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MiMoCodeCache {
    #[serde(default)]
    pub read: i64,
    #[serde(default)]
    pub write: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MiMoCodeTime {
    pub created: f64,
    pub completed: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MiMoCodeSqliteFingerprint {
    created_bits: u64,
    completed_bits: Option<u64>,
    model_id: String,
    provider_id: String,
    input: i64,
    output: i64,
    reasoning: i64,
    cache_read: i64,
    cache_write: i64,
    cost_bits: u64,
    agent: Option<String>,
}

#[derive(Debug, Clone)]
struct MiMoCodeSqliteDedupState {
    has_embedded_message_id: bool,
    has_workspace_conflict: bool,
}

fn workspace_from_root(root: Option<&str>) -> (Option<String>, Option<String>) {
    let workspace_key = root.and_then(normalize_workspace_key);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
    (workspace_key, workspace_label)
}

fn set_workspace_from_root(message: &mut UnifiedMessage, root: Option<&str>) {
    let (workspace_key, workspace_label) = workspace_from_root(root);
    message.set_workspace(workspace_key, workspace_label);
}

fn merge_duplicate_workspace(
    message: &mut UnifiedMessage,
    state: &mut MiMoCodeSqliteDedupState,
    root: Option<&str>,
) {
    if state.has_workspace_conflict {
        return;
    }

    let (candidate_key, candidate_label) = workspace_from_root(root);
    match (message.workspace_key.as_deref(), candidate_key) {
        (None, Some(key)) => message.set_workspace(Some(key), candidate_label),
        (Some(existing), Some(candidate)) if existing != candidate => {
            state.has_workspace_conflict = true;
            message.set_workspace(None, None);
        }
        _ => {}
    }
}

/// Normalize an epoch `time.created`/`time.completed` value to milliseconds.
///
/// MiMo Code is expected to store epoch milliseconds (matching OpenCode), but
/// some builds/channels have been observed writing epoch *seconds*, which made
/// dates land ~1000x in the past (1970-era). A recent epoch is ~1.7e12 in ms
/// versus ~1.7e9 in seconds, so a value at/under the `1e12` threshold is
/// treated as seconds and scaled up. This mirrors `timestamp_secs_to_ms` in the
/// goose/hermes parsers.
fn micode_timestamp_to_ms(timestamp: f64) -> f64 {
    if timestamp > 1e12 {
        timestamp
    } else {
        timestamp * 1000.0
    }
}

fn micode_duration_ms(time: &MiMoCodeTime) -> Option<i64> {
    // Normalize both endpoints so a seconds/ms mismatch (or both-in-seconds)
    // still yields a millisecond duration rather than a value 1000x too small.
    let duration = micode_timestamp_to_ms(time.completed?) - micode_timestamp_to_ms(time.created);
    if duration.is_finite() && duration > 0.0 {
        Some(duration as i64)
    } else {
        None
    }
}

pub fn parse_micode_sqlite(db_path: &Path) -> Vec<UnifiedMessage> {
    let Some(conn) = open_readonly_sqlite(db_path) else {
        return Vec::new();
    };

    let modern_query = r#"
        SELECT m.id, m.session_id, m.data, NULLIF(s.directory, '') AS workspace_root
        FROM message m
        LEFT JOIN session s ON s.id = m.session_id
        WHERE json_extract(m.data, '$.role') = 'assistant'
          AND json_extract(m.data, '$.tokens') IS NOT NULL
        ORDER BY m.id, m.session_id
    "#;

    let legacy_query = r#"
        SELECT m.id, m.session_id, m.data, NULL AS workspace_root
        FROM message m
        WHERE json_extract(m.data, '$.role') = 'assistant'
          AND json_extract(m.data, '$.tokens') IS NOT NULL
        ORDER BY m.id, m.session_id
    "#;

    let mut stmt = match conn
        .prepare(modern_query)
        .or_else(|_| conn.prepare(legacy_query))
    {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let session_id: String = row.get(1)?;
        let data_json: String = row.get(2)?;
        let workspace_root: Option<String> = row.get(3)?;
        Ok((id, session_id, data_json, workspace_root))
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut messages: Vec<UnifiedMessage> = Vec::new();
    let mut fingerprint_indices: HashMap<MiMoCodeSqliteFingerprint, usize> = HashMap::new();
    let mut dedup_states: Vec<MiMoCodeSqliteDedupState> = Vec::new();

    // Namespace ONLY the row-id fallback by the database. MiMo Code uses
    // channel-suffixed databases (mimocode.db and mimocode-<channel>.db), and a
    // mid-session channel switch can write the SAME message to both files. The
    // embedded message id is globally unique, so it must stay un-namespaced to
    // collapse those duplicates across files. SQLite rowids, by contrast, are
    // per-database and not globally unique, so the fallback path namespaces them
    // to avoid falsely merging two different messages that share a rowid.
    let db_namespace = db_path.to_string_lossy().to_string();

    for row_result in rows {
        let (row_id, session_id, data_json, row_workspace_root) = match row_result {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut bytes = data_json.into_bytes();
        let msg: MiMoCodeMessage = match simd_json::from_slice(&mut bytes) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if msg.role != "assistant" {
            continue;
        }

        let message_id = msg.id.clone();
        let embedded_workspace_root = msg
            .path
            .as_ref()
            .and_then(|path| path.root.as_deref())
            .map(str::to_string);

        let tokens = match msg.tokens {
            Some(t) => t,
            None => continue,
        };

        let model_id = match msg.model_id {
            Some(m) => m,
            None => continue,
        };

        let provider_id = msg.provider_id.unwrap_or_else(|| "unknown".to_string());
        let provider_id =
            provider_identity::canonical_provider(&provider_id).unwrap_or(provider_id);
        let agent_or_mode = msg.mode.or(msg.agent);
        let agent = agent_or_mode.map(|a| normalize_opencode_agent_name(&a));
        let input = tokens.input.max(0);
        let output = tokens.output.max(0);
        let reasoning = tokens.reasoning.unwrap_or(0).max(0);
        let cache = tokens.cache.unwrap_or_default();
        let cache_read = cache.read.max(0);
        let cache_write = cache.write.max(0);
        let cost = msg.cost.unwrap_or(0.0).max(0.0);
        // Normalize epoch values to milliseconds up front so the timestamp, the
        // dedup fingerprint, and the duration all agree even when MiMo writes
        // seconds instead of milliseconds.
        let created_ms = micode_timestamp_to_ms(msg.time.created);
        let completed_ms = msg.time.completed.map(micode_timestamp_to_ms);
        let dedup_key = match message_id.clone() {
            // Embedded ids are globally unique: keep them un-namespaced so the
            // same message in mimocode.db and mimocode-<channel>.db collapses.
            Some(id) => id,
            // Rowids are per-database: namespace to avoid false cross-DB merges.
            None => format!("{db_namespace}:{row_id}"),
        };
        let fingerprint = MiMoCodeSqliteFingerprint {
            created_bits: created_ms.to_bits(),
            completed_bits: completed_ms.map(f64::to_bits),
            model_id: model_id.clone(),
            provider_id: provider_id.clone(),
            input,
            output,
            reasoning,
            cache_read,
            cache_write,
            cost_bits: cost.to_bits(),
            agent: agent.clone(),
        };

        let mut unified = UnifiedMessage::new_with_agent(
            "micode",
            model_id,
            provider_id,
            session_id,
            // `time.created` is normalized to epoch milliseconds above (MiMo
            // matches OpenCode's ms, but some channels write seconds);
            // UnifiedMessage's timestamp_to_date treats it as ms.
            created_ms as i64,
            TokenBreakdown {
                input,
                output,
                cache_read,
                cache_write,
                reasoning,
            },
            cost,
            agent,
        );
        unified.duration_ms = micode_duration_ms(&msg.time);
        unified.dedup_key = Some(dedup_key);
        let workspace_root = row_workspace_root
            .as_deref()
            .or(embedded_workspace_root.as_deref());
        set_workspace_from_root(&mut unified, workspace_root);

        if let Some(index) = fingerprint_indices.get(&fingerprint).copied() {
            let dedup_state = &mut dedup_states[index];
            if message_id.is_some() && !dedup_state.has_embedded_message_id {
                dedup_state.has_embedded_message_id = true;
                messages[index].dedup_key = unified.dedup_key;
            }
            merge_duplicate_workspace(&mut messages[index], dedup_state, workspace_root);
            continue;
        }

        dedup_states.push(MiMoCodeSqliteDedupState {
            has_embedded_message_id: message_id.is_some(),
            has_workspace_conflict: false,
        });
        fingerprint_indices.insert(fingerprint, messages.len());
        messages.push(unified);
    }

    messages
}
