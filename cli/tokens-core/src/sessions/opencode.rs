//! OpenCode session parser
//!
//! Parses messages from:
//! - SQLite database (OpenCode 1.2+): ~/.local/share/opencode/opencode.db
//! - Legacy JSON files: ~/.local/share/opencode/storage/message/

use super::utils::{open_readonly_sqlite, read_file_or_none};
use super::{
    normalize_opencode_agent_name, normalize_workspace_key, workspace_label_from_key,
    UnifiedMessage,
};
use crate::{provider_identity, TokenBreakdown};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// OpenCode message structure (from JSON files and SQLite data column).
///
/// Handles two on-disk shapes:
/// - **v1** (`opencode.db` `message` table, legacy JSON files): a `role`
///   field, and top-level `modelID` / `providerID` strings.
/// - **v2** (`opencode-next.db` `session_message` table): no `role` field
///   (the row's `type` column carries it), and the model identifiers nested
///   under a `model` object (`model.id` / `model.providerID`).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct OpenCodeMessage {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "sessionID", default)]
    pub session_id: Option<String>,
    /// Absent in v2 `session_message` rows (the `type` column is the role
    /// there and the SQL query already filters to `assistant`).
    #[serde(default)]
    pub role: Option<String>,
    #[serde(rename = "modelID", default)]
    pub model_id: Option<String>,
    #[serde(rename = "providerID", default)]
    pub provider_id: Option<String>,
    /// v2 nests model + provider under a `model` object.
    #[serde(default)]
    pub model: Option<OpenCodeModel>,
    pub cost: Option<f64>,
    pub tokens: Option<OpenCodeTokens>,
    pub time: OpenCodeTime,
    pub agent: Option<String>,
    pub mode: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opencode_path")]
    pub path: Option<OpenCodePath>,
}

impl OpenCodeMessage {
    /// Resolve the model id from the top-level v1 field or the nested v2
    /// `model.id`, preferring the explicit top-level value when both exist.
    fn resolve_model_id(&self) -> Option<String> {
        self.model_id
            .clone()
            .or_else(|| self.model.as_ref().and_then(|m| m.id.clone()))
    }

    /// Resolve the provider id from the top-level v1 field or the nested v2
    /// `model.providerID`, preferring the explicit top-level value.
    fn resolve_provider_id(&self) -> Option<String> {
        self.provider_id
            .clone()
            .or_else(|| self.model.as_ref().and_then(|m| m.provider_id.clone()))
    }

    /// True when this row is an assistant turn. v1 rows carry an explicit
    /// `role`; v2 rows omit it and are pre-filtered by the SQL `type` column,
    /// so a missing role is treated as assistant.
    fn is_assistant(&self) -> bool {
        self.role.as_deref().is_none_or(|role| role == "assistant")
    }
}

/// v2 nested model descriptor: `{"id": "...", "providerID": "...", ...}`.
#[derive(Debug, Deserialize)]
pub struct OpenCodeModel {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "providerID", default)]
    pub provider_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodePath {
    pub root: Option<String>,
}

fn deserialize_opencode_path<'de, D>(deserializer: D) -> Result<Option<OpenCodePath>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let root = value
        .get("root")
        .and_then(|root| root.as_str())
        .map(str::to_string);

    Ok(Some(OpenCodePath { root }))
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeTokens {
    pub input: i64,
    pub output: i64,
    pub reasoning: Option<i64>,
    pub cache: OpenCodeCache,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeCache {
    pub read: i64,
    pub write: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct OpenCodeTime {
    pub created: f64, // Unix timestamp in milliseconds (as float)
    pub completed: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OpenCodeSqliteFingerprint {
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
struct OpenCodeSqliteDedupState {
    /// The entry's embedded (`$.id`) message id, if any. Two rows that share
    /// every fingerprint field but carry *different* embedded ids are distinct
    /// messages, not fork copies, and must not be merged. A fork copies the id,
    /// so equal ids (or an id absent on either side) still merge.
    message_id: Option<String>,
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
    state: &mut OpenCodeSqliteDedupState,
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

fn opencode_duration_ms(time: &OpenCodeTime) -> Option<i64> {
    let duration = time.completed? - time.created;
    if duration.is_finite() && duration > 0.0 {
        Some(duration as i64)
    } else {
        None
    }
}

fn embedded_cost(cost: Option<f64>) -> f64 {
    match cost {
        Some(cost) if cost.is_finite() && cost >= 0.0 => cost,
        _ => 0.0,
    }
}

pub fn parse_opencode_file(path: &Path) -> Option<UnifiedMessage> {
    let data = read_file_or_none(path)?;
    let mut bytes = data;

    let msg: OpenCodeMessage = simd_json::from_slice(&mut bytes).ok()?;

    // OpenCode JSON files (v1) always carry an explicit role, so require it to
    // be "assistant" here. Missing-role acceptance (is_assistant) is reserved
    // for the v2 `session_message` SQLite path, whose SQL already filters
    // `type = 'assistant'`; applying it to files would count a role-less or
    // malformed file as assistant usage (previously it was skipped when the
    // required `role` field failed to deserialize).
    if msg.role.as_deref() != Some("assistant") {
        return None;
    }

    let workspace_root = msg
        .path
        .as_ref()
        .and_then(|path| path.root.as_deref())
        .map(str::to_string);
    // Resolve model + provider before moving any fields out of `msg`, since
    // both borrow the whole struct to fall back onto the nested `model` object.
    let model_id = msg.resolve_model_id()?;
    let provider_id = msg
        .resolve_provider_id()
        .unwrap_or_else(|| "unknown".to_string());
    let provider_id = provider_identity::canonical_provider(&provider_id).unwrap_or(provider_id);

    let tokens = msg.tokens?;
    let agent_or_mode = msg.mode.or(msg.agent);
    let agent = agent_or_mode.map(|a| normalize_opencode_agent_name(&a));

    let session_id = msg.session_id.unwrap_or_else(|| "unknown".to_string());

    // Use message ID from JSON or derive from filename for deduplication
    let dedup_key = msg.id.or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    });
    let cost = embedded_cost(msg.cost);

    let mut unified = UnifiedMessage::new_with_agent(
        "opencode",
        model_id,
        provider_id,
        session_id,
        msg.time.created as i64,
        TokenBreakdown {
            input: tokens.input.max(0),
            output: tokens.output.max(0),
            cache_read: tokens.cache.read.max(0),
            cache_write: tokens.cache.write.max(0),
            reasoning: tokens.reasoning.unwrap_or(0).max(0),
        },
        cost,
        agent,
    );
    unified.duration_ms = opencode_duration_ms(&msg.time);
    unified.dedup_key = dedup_key;
    set_workspace_from_root(&mut unified, workspace_root.as_deref());
    mark_opencode_cost_source(&mut unified);
    Some(unified)
}

/// OpenCode computes per-message cost at request time from its own pricing
/// data (models.dev), so a positive `cost` is authoritative and must survive
/// tokens's LiteLLM repricing pass. A zero cost usually means OpenCode
/// itself had no pricing for the model — leave it `Unknown` so
/// `apply_pricing_if_available` can still estimate.
fn mark_opencode_cost_source(unified: &mut UnifiedMessage) {
    if unified.cost > 0.0 {
        unified.mark_provider_reported_cost();
    }
}

/// Column layout shared by every OpenCode SQLite query variant:
/// `(row_id, session_id, data_json, workspace_root, session_title)`.
type OpenCodeSqliteRow = (String, String, String, Option<String>, Option<String>);

/// Accumulates parsed assistant messages across OpenCode's v1 (`message`) and
/// v2 (`session_message`) tables, applying fingerprint-based deduplication so
/// forked-history copies — and any overlap between the two tables — collapse
/// into a single entry. A fingerprint maps to a *list* of entries, one per
/// distinct embedded message id, so two genuinely different messages that
/// happen to collide on every fingerprint field are kept apart.
#[derive(Default)]
struct OpenCodeSqliteAccumulator {
    messages: Vec<UnifiedMessage>,
    fingerprint_indices: HashMap<OpenCodeSqliteFingerprint, Vec<usize>>,
    dedup_states: Vec<OpenCodeSqliteDedupState>,
}

impl OpenCodeSqliteAccumulator {
    /// Parse one SQLite row's JSON payload and merge it into the accumulator,
    /// deduplicating against previously ingested rows.
    fn ingest_row(&mut self, row: OpenCodeSqliteRow) {
        let (row_id, session_id, data_json, row_workspace_root, row_session_title) = row;

        let mut bytes = data_json.into_bytes();
        let msg: OpenCodeMessage = match simd_json::from_slice(&mut bytes) {
            Ok(m) => m,
            Err(_) => return,
        };

        if !msg.is_assistant() {
            return;
        }

        let message_id = msg.id.clone();
        let embedded_workspace_root = msg
            .path
            .as_ref()
            .and_then(|path| path.root.as_deref())
            .map(str::to_string);

        let tokens = match msg.tokens {
            Some(ref t) => t,
            None => return,
        };

        let model_id = match msg.resolve_model_id() {
            Some(m) => m,
            None => return,
        };

        let provider_id = msg
            .resolve_provider_id()
            .unwrap_or_else(|| "unknown".to_string());
        let provider_id =
            provider_identity::canonical_provider(&provider_id).unwrap_or(provider_id);
        let agent_or_mode = msg.mode.clone().or_else(|| msg.agent.clone());
        let agent = agent_or_mode.map(|a| normalize_opencode_agent_name(&a));
        let input = tokens.input.max(0);
        let output = tokens.output.max(0);
        let reasoning = tokens.reasoning.unwrap_or(0).max(0);
        let cache_read = tokens.cache.read.max(0);
        let cache_write = tokens.cache.write.max(0);
        let cost = embedded_cost(msg.cost);
        let dedup_key = message_id.clone().unwrap_or(row_id);
        let fingerprint = OpenCodeSqliteFingerprint {
            created_bits: msg.time.created.to_bits(),
            completed_bits: msg.time.completed.map(f64::to_bits),
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
            "opencode",
            model_id,
            provider_id,
            session_id,
            msg.time.created as i64,
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
        unified.duration_ms = opencode_duration_ms(&msg.time);
        unified.dedup_key = Some(dedup_key);
        let workspace_root = row_workspace_root
            .as_deref()
            .or(embedded_workspace_root.as_deref());
        set_workspace_from_root(&mut unified, workspace_root);
        mark_opencode_cost_source(&mut unified);
        if let Some(ref title) = row_session_title {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                unified.session_title = Some(trimmed.to_string());
            }
        }

        // Among entries sharing this fingerprint, merge into the first one that
        // is NOT a definitively-different message -- i.e. skip any whose stored
        // embedded id conflicts with this row's. (Cloning the small index list
        // avoids holding a borrow of `fingerprint_indices` while we read
        // `dedup_states`.)
        let candidate = {
            let slots = self
                .fingerprint_indices
                .get(&fingerprint)
                .cloned()
                .unwrap_or_default();
            slots.into_iter().find(|&index| {
                !matches!(
                    (&self.dedup_states[index].message_id, &message_id),
                    (Some(existing), Some(incoming)) if existing != incoming
                )
            })
        };

        if let Some(index) = candidate {
            let dedup_state = &mut self.dedup_states[index];
            // First copy carrying an embedded id promotes the entry's stable
            // dedup key (and records the id so later rows can be told apart).
            if message_id.is_some() && dedup_state.message_id.is_none() {
                dedup_state.message_id = message_id.clone();
                self.messages[index].dedup_key = unified.dedup_key.clone();
            }
            merge_duplicate_workspace(&mut self.messages[index], dedup_state, workspace_root);
            return;
        }

        let new_index = self.messages.len();
        self.dedup_states.push(OpenCodeSqliteDedupState {
            message_id: message_id.clone(),
            has_workspace_conflict: false,
        });
        self.fingerprint_indices
            .entry(fingerprint)
            .or_default()
            .push(new_index);
        self.messages.push(unified);
    }
}

/// Run one query (whose columns are `id, session_id, data, workspace_root,
/// session_title`) against `conn` and feed every row into `acc`. A prepare/query
/// failure — for example a table that does not exist in this schema variant —
/// is treated as "no rows", so callers can attempt several schema variants
/// against the same database without an error aborting the scan.
fn collect_opencode_rows(
    conn: &rusqlite::Connection,
    query: &str,
    acc: &mut OpenCodeSqliteAccumulator,
) {
    let mut stmt = match conn.prepare(query) {
        Ok(s) => s,
        Err(_) => return,
    };

    let rows = match stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let session_id: String = row.get(1)?;
        let data_json: String = row.get(2)?;
        let workspace_root: Option<String> = row.get(3)?;
        let session_title: Option<String> = row.get(4)?;
        Ok((id, session_id, data_json, workspace_root, session_title))
    }) {
        Ok(r) => r,
        Err(_) => return,
    };

    for row_result in rows.flatten() {
        acc.ingest_row(row_result);
    }
}

pub fn parse_opencode_sqlite(db_path: &Path) -> Vec<UnifiedMessage> {
    let Some(conn) = open_readonly_sqlite(db_path) else {
        return Vec::new();
    };

    let mut acc = OpenCodeSqliteAccumulator::default();

    // OpenCode v2 (`opencode-next.db`): per-message rows live in
    // `session_message`, keyed by a `type` column, with model + provider nested
    // under `$.model`. Absent in v1 databases, where the prepare fails and this
    // is a no-op.
    //
    // Try the title-bearing query first; older v2 databases whose `session`
    // table predates the `title` column fall back to a title-less variant so
    // they still produce rows (the title is optional, not a gating column).
    let v2_query = r#"
        SELECT sm.id, sm.session_id, sm.data, NULLIF(s.directory, '') AS workspace_root, s.title AS session_title
        FROM session_message sm
        LEFT JOIN session s ON s.id = sm.session_id
        WHERE sm.type = 'assistant'
          AND json_extract(sm.data, '$.tokens') IS NOT NULL
        ORDER BY sm.id, sm.session_id
    "#;
    let v2_query_no_title = r#"
        SELECT sm.id, sm.session_id, sm.data, NULLIF(s.directory, '') AS workspace_root, NULL AS session_title
        FROM session_message sm
        LEFT JOIN session s ON s.id = sm.session_id
        WHERE sm.type = 'assistant'
          AND json_extract(sm.data, '$.tokens') IS NOT NULL
        ORDER BY sm.id, sm.session_id
    "#;
    if conn.prepare(v2_query).is_ok() {
        collect_opencode_rows(&conn, v2_query, &mut acc);
    } else {
        collect_opencode_rows(&conn, v2_query_no_title, &mut acc);
    }

    // OpenCode v1 (`opencode.db`, 1.2+): per-message rows in `message`, role in
    // the JSON `$.role`. The `session` join supplies the workspace directory
    // and title. Three fallback tiers:
    //   1. modern: session table has both `directory` and `title`
    //   2. directory-only: session table has `directory` but not `title`
    //   3. legacy: no `session` table at all (drops workspace + title)
    let v1_modern_query = r#"
        SELECT m.id, m.session_id, m.data, NULLIF(s.directory, '') AS workspace_root, s.title AS session_title
        FROM message m
        LEFT JOIN session s ON s.id = m.session_id
        WHERE json_extract(m.data, '$.role') = 'assistant'
          AND json_extract(m.data, '$.tokens') IS NOT NULL
        ORDER BY m.id, m.session_id
    "#;
    let v1_directory_query = r#"
        SELECT m.id, m.session_id, m.data, NULLIF(s.directory, '') AS workspace_root, NULL AS session_title
        FROM message m
        LEFT JOIN session s ON s.id = m.session_id
        WHERE json_extract(m.data, '$.role') = 'assistant'
          AND json_extract(m.data, '$.tokens') IS NOT NULL
        ORDER BY m.id, m.session_id
    "#;
    let v1_legacy_query = r#"
        SELECT m.id, m.session_id, m.data, NULL AS workspace_root, NULL AS session_title
        FROM message m
        WHERE json_extract(m.data, '$.role') = 'assistant'
          AND json_extract(m.data, '$.tokens') IS NOT NULL
        ORDER BY m.id, m.session_id
    "#;
    if conn.prepare(v1_modern_query).is_ok() {
        collect_opencode_rows(&conn, v1_modern_query, &mut acc);
    } else if conn.prepare(v1_directory_query).is_ok() {
        collect_opencode_rows(&conn, v1_directory_query, &mut acc);
    } else {
        collect_opencode_rows(&conn, v1_legacy_query, &mut acc);
    }

    acc.messages
}

// =============================================================================
// Migration cache: skip redundant legacy JSON scanning after full migration
// =============================================================================

const MIGRATION_CACHE_FILENAME: &str = "opencode-migration.json";

/// Persisted migration status for OpenCode JSON → SQLite migration.
/// Stored at <config_dir>/cache/opencode-migration.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenCodeMigrationCache {
    /// True when every legacy JSON message was already present in SQLite.
    pub migration_complete: bool,
    /// Number of JSON files in the message directory at detection time.
    pub json_file_count: u64,
    /// Modification time of the JSON directory (Unix seconds) at detection time.
    pub json_dir_mtime_secs: u64,
    /// When this entry was written (Unix seconds).
    pub checked_at_secs: u64,
}

fn migration_cache_dir() -> std::path::PathBuf {
    crate::paths::get_cache_dir()
}

fn migration_cache_path() -> std::path::PathBuf {
    migration_cache_dir().join(MIGRATION_CACHE_FILENAME)
}

fn legacy_migration_cache_paths() -> Vec<std::path::PathBuf> {
    if crate::paths::is_config_dir_overridden() {
        return Vec::new();
    }

    [
        crate::paths::legacy_dirs_cache_dir().map(|d| d.join(MIGRATION_CACHE_FILENAME)),
        crate::paths::legacy_dot_cache_dir().map(|d| d.join(MIGRATION_CACHE_FILENAME)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Load the migration cache from disk. Returns `None` if the file is missing or
/// unparseable.
pub fn load_opencode_migration_cache() -> Option<OpenCodeMigrationCache> {
    let canonical = migration_cache_path();
    match std::fs::read_to_string(&canonical) {
        Ok(content) => serde_json::from_str(&content).ok(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            legacy_migration_cache_paths().into_iter().find_map(|path| {
                let content = std::fs::read_to_string(path).ok()?;
                serde_json::from_str(&content).ok()
            })
        }
        Err(_) => None,
    }
}

/// Persist the migration cache atomically (write to temp file, then rename).
pub fn save_opencode_migration_cache(cache: &OpenCodeMigrationCache) {
    use std::io::Write as _;

    let dir = migration_cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }

    let content = match serde_json::to_string(cache) {
        Ok(c) => c,
        Err(_) => return,
    };

    let final_path = migration_cache_path();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let tmp_name = format!(".opencode-migration.{}.{:x}.tmp", std::process::id(), nanos);
    let tmp_path = dir.join(tmp_name);

    // INVARIANT: All cache writes use atomic temp-file rename. NEVER delete
    // the canonical cache file before writing — a partial save or process
    // crash between delete and rename would lose the cache. The temp-file
    // pattern makes corruption-on-crash impossible.
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        crate::fs_atomic::replace_file(&tmp_path, &final_path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
}

/// Return the modification time of `json_dir` as Unix seconds, or `None` on
/// error (directory absent, permissions, etc.).
pub fn get_json_dir_mtime(json_dir: &Path) -> Option<u64> {
    std::fs::metadata(json_dir)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Current Unix timestamp in seconds.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
