//! Kiro session parser
//!
//! Parses session data from four sources:
//! 1. File-based (Kiro CLI): ~/.kiro/sessions/cli/*.json + *.jsonl
//! 2. Kiro IDE globalStorage snapshots
//! 3. SQLite-based: ~/Library/Application Support/kiro-cli/data.sqlite3
//!    (conversations_v2 table with history[*].request_metadata)
//! 4. File-based (Kiro IDE): ~/.kiro/sessions/<workspace>/sess_<uuid>/
//!    session.json (metadata) + messages.jsonl (conversation). This is the
//!    VS Code-based Kiro IDE layout, distinct from the CLI's cli/*.json layout.
//!
//! Turn-level token counts are currently zero in both sources, so usage is
//! estimated from context_usage_percentage * context_window (input) and
//! response_size / 4 (output).

use super::utils::{back_anchor_timestamp, file_modified_timestamp_ms};
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tracing::warn;

const CLIENT_ID: &str = "kiro";
const PROVIDER_ID: &str = "amazon-bedrock";
const UNKNOWN_MODEL: &str = "auto";

#[derive(Debug, Deserialize)]
struct KiroSessionHeader {
    session_id: Option<String>,
    cwd: Option<String>,
    session_state: Option<KiroSessionState>,
}

#[derive(Debug, Deserialize)]
struct KiroSessionState {
    rts_model_state: Option<KiroRtsModelState>,
    conversation_metadata: Option<KiroConversationMetadata>,
}

#[derive(Debug, Deserialize)]
struct KiroRtsModelState {
    model_info: Option<KiroModelInfo>,
}

#[derive(Debug, Deserialize)]
struct KiroModelInfo {
    model_id: Option<String>,
    context_window_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct KiroConversationMetadata {
    user_turn_metadatas: Option<Vec<KiroTurnMetadata>>,
}

#[derive(Debug, Deserialize)]
struct KiroTurnMetadata {
    input_token_count: Option<i64>,
    output_token_count: Option<i64>,
    end_timestamp: Option<serde_json::Value>,
    total_request_count: Option<i32>,
    message_ids: Option<Vec<Option<String>>>,
    context_usage_percentage: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct KiroJsonlEntry {
    kind: String,
    data: Option<KiroJsonlData>,
}

#[derive(Debug, Deserialize)]
struct KiroJsonlData {
    message_id: Option<String>,
    content: Option<Vec<KiroContentPart>>,
    meta: Option<KiroEntryMeta>,
}

#[derive(Debug, Deserialize)]
struct KiroContentPart {
    kind: Option<String>,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KiroEntryMeta {
    timestamp: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct KiroMessageContent {
    prompt_chars: usize,
    assistant_chars: usize,
    prompt_timestamp_ms: Option<i64>,
}

/// Metadata half of the Kiro IDE session layout (`session.json`, schemaVersion
/// 1.0.0). The conversation itself lives in the sibling `messages.jsonl`.
#[derive(Debug, Deserialize)]
struct KiroIdeSession {
    id: Option<String>,
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    #[serde(rename = "workspacePaths")]
    workspace_paths: Option<Vec<String>>,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "lastModifiedAt")]
    last_modified_at: Option<String>,
}

pub fn parse_kiro_file(path: &Path) -> Vec<UnifiedMessage> {
    if is_kiro_ide_session_path(path) {
        return parse_kiro_ide_session_file(path);
    }

    if is_kiro_global_storage_path(path) || is_kiro_chat_path(path) {
        return parse_kiro_global_storage_file(path);
    }

    let fallback_timestamp = file_modified_timestamp_ms(path);

    let mut json_bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };

    let header = match simd_json::from_slice::<KiroSessionHeader>(&mut json_bytes) {
        Ok(header) => header,
        Err(_) => return Vec::new(),
    };

    let session_id = header
        .session_id
        .unwrap_or_else(|| session_id_from_path(path));
    let model_id = header
        .session_state
        .as_ref()
        .and_then(|state| state.rts_model_state.as_ref())
        .and_then(|state| state.model_info.as_ref())
        .and_then(|info| info.model_id.as_deref())
        .filter(|model| !model.trim().is_empty())
        .unwrap_or(UNKNOWN_MODEL)
        .to_string();
    let workspace_key = header.cwd.as_deref().and_then(normalize_workspace_key);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
    let context_window = header
        .session_state
        .as_ref()
        .and_then(|state| state.rts_model_state.as_ref())
        .and_then(|state| state.model_info.as_ref())
        .and_then(|info| info.context_window_tokens)
        .unwrap_or(0);
    let turns = header
        .session_state
        .and_then(|state| state.conversation_metadata)
        .and_then(|metadata| metadata.user_turn_metadatas)
        .unwrap_or_default();

    let Some(jsonl_path) = kiro_related_messages_path(path) else {
        return Vec::new();
    };
    let mut content_by_message_id: HashMap<String, KiroMessageContent> = HashMap::new();

    if let Ok(jsonl_file) = std::fs::File::open(&jsonl_path) {
        let reader = BufReader::new(jsonl_file);
        let mut pending_prompt: Option<(usize, Option<i64>)> = None;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let mut bytes = trimmed.as_bytes().to_vec();
            let entry = match simd_json::from_slice::<KiroJsonlEntry>(&mut bytes) {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            let Some(data) = entry.data else {
                continue;
            };
            let Some(message_id) = data.message_id else {
                continue;
            };

            let text_chars = text_char_count(data.content.as_deref());

            match entry.kind.as_str() {
                "Prompt" => {
                    let timestamp_ms = data
                        .meta
                        .and_then(|meta| meta.timestamp)
                        .map(seconds_to_millis);
                    pending_prompt = Some((text_chars, timestamp_ms));
                }
                "AssistantMessage" => {
                    let message = content_by_message_id.entry(message_id).or_default();
                    if let Some((prompt_chars, prompt_ts)) = pending_prompt.take() {
                        message.prompt_chars += prompt_chars;
                        if message.prompt_timestamp_ms.is_none() {
                            message.prompt_timestamp_ms = prompt_ts;
                        }
                    }
                    message.assistant_chars += text_chars;
                }
                _ => {}
            }
        }
    }

    turns
        .into_iter()
        .enumerate()
        .filter_map(|(index, turn)| {
            let message_ids = turn.message_ids.unwrap_or_default();
            let mut prompt_chars = 0;
            let mut assistant_chars = 0;
            let mut prompt_timestamp_ms = None;

            for message_id in message_ids.iter().flatten() {
                let Some(content) = content_by_message_id.get(message_id) else {
                    continue;
                };
                prompt_chars += content.prompt_chars;
                assistant_chars += content.assistant_chars;
                if prompt_timestamp_ms.is_none() {
                    prompt_timestamp_ms = content.prompt_timestamp_ms;
                }
            }

            // NOTE: when explicit per-turn counts are absent (the common case —
            // Kiro currently reports zero), input/output below are ESTIMATED, not
            // measured: input is derived from context_usage_percentage *
            // context_window and output from char_count / 4. Downstream must not
            // treat these as exact token counts.
            let explicit_input = turn.input_token_count.unwrap_or(0).max(0);
            let explicit_output = turn.output_token_count.unwrap_or(0).max(0);
            let input = if explicit_input > 0 {
                explicit_input
            } else if context_window > 0 {
                let ctx_pct = turn.context_usage_percentage.unwrap_or(0.0);
                if ctx_pct > 0.0 {
                    ((context_window as f64) * ctx_pct / 100.0) as i64
                } else {
                    estimate_tokens(prompt_chars)
                }
            } else {
                estimate_tokens(prompt_chars)
            };
            let output = if explicit_output > 0 {
                explicit_output
            } else {
                estimate_tokens(assistant_chars)
            };

            if input + output == 0 {
                return None;
            }

            let end_timestamp_ms = parse_timestamp_value(turn.end_timestamp.as_ref());
            let duration_ms = duration_between_ms(prompt_timestamp_ms, end_timestamp_ms);
            let timestamp = prompt_timestamp_ms
                .or(end_timestamp_ms)
                .unwrap_or(fallback_timestamp);

            let mut message = UnifiedMessage::new_with_dedup(
                CLIENT_ID,
                model_id.clone(),
                PROVIDER_ID,
                session_id.clone(),
                timestamp,
                TokenBreakdown {
                    input,
                    output,
                    cache_read: 0,
                    cache_write: 0,
                    reasoning: 0,
                },
                0.0,
                Some(format!("{}:{}", session_id, index)),
            );
            message.message_count = turn.total_request_count.unwrap_or(1).max(1);
            if prompt_timestamp_ms.is_none() && end_timestamp_ms.is_none() {
                message.set_timestamp_provenance(crate::TimestampProvenance::Fallback);
            }
            message.duration_ms = duration_ms;
            message.is_turn_start = true;
            message.set_workspace(workspace_key.clone(), workspace_label.clone());
            Some(message)
        })
        .collect()
}

fn text_char_count(content: Option<&[KiroContentPart]>) -> usize {
    content
        .unwrap_or_default()
        .iter()
        .filter(|part| part.kind.as_deref().is_none_or(|kind| kind == "text"))
        .filter_map(|part| part.data.as_deref())
        .map(str::chars)
        .map(Iterator::count)
        .sum()
}

fn estimate_tokens(chars: usize) -> i64 {
    chars.div_ceil(4) as i64
}

fn seconds_to_millis(seconds: f64) -> i64 {
    // Scale fractional seconds to milliseconds (preserving sub-second
    // precision), then clamp into i64 range. The `f64 as i64` cast saturates
    // rather than wrapping on out-of-range/garbage timestamps, so the
    // seconds->ms conversion cannot overflow.
    let millis = seconds * 1000.0;
    if millis.is_nan() {
        0
    } else {
        millis.clamp(i64::MIN as f64, i64::MAX as f64) as i64
    }
}

fn duration_between_ms(start_ms: Option<i64>, end_ms: Option<i64>) -> Option<i64> {
    let duration = end_ms?.saturating_sub(start_ms?);
    (duration > 0).then_some(duration)
}

fn parse_timestamp_value(value: Option<&serde_json::Value>) -> Option<i64> {
    match value? {
        serde_json::Value::Number(number) => number.as_f64().map(|timestamp| {
            if timestamp.abs() < 1_000_000_000_000.0 {
                seconds_to_millis(timestamp)
            } else {
                timestamp as i64
            }
        }),
        serde_json::Value::String(timestamp) => chrono::DateTime::parse_from_rfc3339(timestamp)
            .ok()
            .map(|dt| dt.timestamp_millis())
            .or_else(|| timestamp.parse::<f64>().ok().map(seconds_to_millis)),
        _ => None,
    }
}

fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn is_kiro_global_storage_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains("globalStorage") && path_str.contains("kiro.kiroagent")
}

fn is_kiro_chat_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("chat"))
}

/// Return the conversation sidecar consumed by `parse_kiro_file`, if this
/// Kiro source format has one. IDE sessions use `messages.jsonl`; CLI session
/// headers use the same stem with a `.jsonl` extension. Global-storage and
/// `.chat` snapshots are self-contained.
pub(crate) fn kiro_related_messages_path(path: &Path) -> Option<PathBuf> {
    if is_kiro_ide_session_path(path) {
        return Some(path.with_file_name("messages.jsonl"));
    }
    if is_kiro_global_storage_path(path) || is_kiro_chat_path(path) {
        return None;
    }
    Some(path.with_extension("jsonl"))
}

/// A Kiro IDE session file is `session.json` sitting inside a `sess_<uuid>`
/// directory (`~/.kiro/sessions/<workspace>/sess_<uuid>/session.json`). The
/// `sess_` parent requirement keeps this from matching the CLI layout, whose
/// arbitrary `~/.kiro/sessions/cli/*.json` files share the same tree.
pub(crate) fn is_kiro_ide_session_path(path: &Path) -> bool {
    let is_session_json = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == "session.json")
        .unwrap_or(false);
    if !is_session_json {
        return false;
    }
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with("sess_"))
        .unwrap_or(false)
}

/// Parse the Kiro IDE session layout: `session.json` (metadata) plus the
/// sibling `messages.jsonl` (conversation).
///
/// The IDE does NOT record per-turn token usage in these files (confirmed
/// against issue #813's sample, which carries only session metadata), so — like
/// every other Kiro path — token counts here are ESTIMATED from message text
/// (chars / 4), never measured. `messages.jsonl`'s exact schema is not
/// documented, so each line is parsed as generic JSON and fed through the
/// role-tolerant snapshot text collector (user/assistant/human/bot/prompt/
/// response). Lines with no role-tagged text contribute nothing rather than
/// being guessed at, so a session with no recognizable content is dropped
/// instead of fabricating usage.
fn parse_kiro_ide_session_file(path: &Path) -> Vec<UnifiedMessage> {
    let fallback_timestamp = file_modified_timestamp_ms(path);

    let session_json = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };
    let session: KiroIdeSession = match serde_json::from_str(&session_json) {
        Ok(session) => session,
        Err(_) => return Vec::new(),
    };

    let sess_dir = path.parent();
    let sess_dir_name = sess_dir
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str());
    let session_id = session
        .id
        .filter(|id| !id.trim().is_empty())
        .or_else(|| sess_dir_name.map(|name| name.to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    let session_model_id = session.model_id.filter(|m| !m.trim().is_empty());

    let workspace_path = session
        .workspace_paths
        .as_ref()
        .and_then(|paths| paths.first())
        .map(|s| s.as_str());
    let workspace_from_dir = sess_dir
        .and_then(|dir| dir.parent())
        .and_then(|ws_dir| ws_dir.file_name())
        .and_then(|name| name.to_str())
        .map(|name| name.to_string());
    let ws_str = workspace_path.map(|s| s.to_string()).or(workspace_from_dir);
    let workspace_key = ws_str.as_deref().and_then(normalize_workspace_key);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

    let messages_path = path.with_file_name("messages.jsonl");
    if !messages_path.is_file() {
        return Vec::new();
    }

    let jsonl_file = match std::fs::File::open(&messages_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let reader = BufReader::new(jsonl_file);

    const DEFAULT_CONTEXT_WINDOW: i64 = 200_000;

    #[derive(Default)]
    struct IdeTurn {
        prompt_chars: usize,
        assistant_chars: usize,
        prompt_timestamp_ms: Option<i64>,
        end_timestamp_ms: Option<i64>,
        context_usage_percentage: f64,
        elapsed_ms: Option<i64>,
    }

    let mut turns: Vec<IdeTurn> = Vec::new();
    let mut current_turn: Option<IdeTurn> = None;
    let mut has_structured_format = false;

    // Fallback accumulators for flat-JSON messages.jsonl (no payload wrapper)
    let mut flat_counts = KiroSnapshotTextCounts::default();
    let mut flat_model_id: Option<String> = None;
    let mut flat_assistant_turns: i32 = 0;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let entry: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Structured format: each line has `payload.type`
        if let Some(payload) = entry.get("payload") {
            if let Some(msg_type) = payload.get("type").and_then(|v| v.as_str()) {
                has_structured_format = true;

                let timestamp_ms = entry
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp_millis());

                match msg_type {
                    "user" => {
                        let chars = payload
                            .get("content")
                            .and_then(|v| v.as_str())
                            .map(|s| s.chars().count())
                            .unwrap_or(0);
                        let turn = current_turn.get_or_insert_with(IdeTurn::default);
                        turn.prompt_chars += chars;
                        if turn.prompt_timestamp_ms.is_none() {
                            turn.prompt_timestamp_ms = timestamp_ms;
                        }
                    }
                    "assistant" => {
                        let chars = payload
                            .get("content")
                            .and_then(|v| v.as_str())
                            .map(|s| s.chars().count())
                            .unwrap_or(0);
                        if let Some(turn) = current_turn.as_mut() {
                            turn.assistant_chars += chars;
                        }
                    }
                    "tool_call" => {
                        let args_chars = payload
                            .get("args")
                            .map(|v| match v {
                                Value::String(s) => s.chars().count(),
                                other => other.to_string().chars().count(),
                            })
                            .unwrap_or(0);
                        if let Some(turn) = current_turn.as_mut() {
                            turn.assistant_chars += args_chars;
                        }
                    }
                    "session_metadata"
                        if payload.get("key").and_then(|v| v.as_str()) == Some("contextUsage") =>
                    {
                        if let Some(pct) = payload
                            .get("value")
                            .and_then(|v| v.get("usagePercentage"))
                            .and_then(|v| v.as_f64())
                        {
                            if let Some(turn) = current_turn.as_mut() {
                                turn.context_usage_percentage = pct;
                            }
                        }
                    }
                    "usage_summary" => {
                        if let Some(elapsed) = payload.get("elapsedTime").and_then(|v| v.as_i64()) {
                            if let Some(turn) = current_turn.as_mut() {
                                turn.elapsed_ms = Some(elapsed);
                            }
                        }
                    }
                    "turn_end" => {
                        if let Some(turn) = current_turn.as_mut() {
                            turn.end_timestamp_ms = timestamp_ms;
                        }
                        if let Some(turn) = current_turn.take() {
                            if turn.prompt_chars > 0 || turn.assistant_chars > 0 {
                                turns.push(turn);
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }
        }

        // Flat format fallback: lines like {"role":"user","content":"..."}
        if flat_model_id.is_none() {
            flat_model_id = find_kiro_snapshot_model_id(&entry);
        }
        let assistant_before = flat_counts.assistant_chars;
        collect_kiro_snapshot_text(&entry, &mut flat_counts, None);
        if flat_counts.assistant_chars > assistant_before {
            flat_assistant_turns += 1;
        }
    }

    // Flush any in-flight structured turn
    if let Some(turn) = current_turn.take() {
        if turn.prompt_chars > 0 || turn.assistant_chars > 0 {
            turns.push(turn);
        }
    }

    if has_structured_format && !turns.is_empty() {
        // Per-turn structured output
        let model_id = session_model_id.unwrap_or_else(|| UNKNOWN_MODEL.to_string());
        return turns
            .into_iter()
            .enumerate()
            .filter_map(|(index, turn)| {
                let input = if turn.context_usage_percentage > 0.0 {
                    ((DEFAULT_CONTEXT_WINDOW as f64) * turn.context_usage_percentage / 100.0) as i64
                } else {
                    estimate_tokens(turn.prompt_chars)
                };
                let output = estimate_tokens(turn.assistant_chars);

                if input + output == 0 {
                    return None;
                }

                let duration_ms = turn.elapsed_ms.or_else(|| {
                    duration_between_ms(turn.prompt_timestamp_ms, turn.end_timestamp_ms)
                });
                // Prefer the user prompt's own timestamp. When it's absent or
                // unparseable (e.g. `usage_summary.elapsedTime` supplied
                // `duration_ms` but the prompt timestamp couldn't be
                // resolved), back-calculate the start anchor from
                // `turn_end - elapsed` instead of falling through to
                // `end_timestamp_ms` directly — otherwise sessionize()'s
                // `[timestamp, timestamp + duration_ms]` span would project
                // forward past the turn's actual end into phantom idle time.
                // The back-calculation is guarded against a non-positive
                // result (which sessionize() silently drops) by falling back
                // to the unadjusted `end_timestamp_ms`.
                let timestamp = turn
                    .prompt_timestamp_ms
                    .or_else(|| match (turn.end_timestamp_ms, duration_ms) {
                        (Some(end), Some(elapsed)) => Some(back_anchor_timestamp(end, elapsed)),
                        _ => None,
                    })
                    .or(turn.end_timestamp_ms)
                    .unwrap_or(fallback_timestamp);

                let mut message = UnifiedMessage::new_with_dedup(
                    CLIENT_ID,
                    model_id.clone(),
                    PROVIDER_ID,
                    session_id.clone(),
                    timestamp,
                    TokenBreakdown {
                        input,
                        output,
                        cache_read: 0,
                        cache_write: 0,
                        reasoning: 0,
                    },
                    0.0,
                    Some(format!("{}:ide:{}", session_id, index)),
                );
                message.message_count = 1;
                if turn.prompt_timestamp_ms.is_none() && turn.end_timestamp_ms.is_none() {
                    message.set_timestamp_provenance(crate::TimestampProvenance::Fallback);
                }
                message.is_turn_start = true;
                message.duration_ms = duration_ms;
                message.set_workspace(workspace_key.clone(), workspace_label.clone());
                Some(message)
            })
            .collect();
    }

    // Flat format fallback: single aggregated message (original behavior)
    let input = estimate_tokens(flat_counts.prompt_chars);
    let output = estimate_tokens(flat_counts.assistant_chars);
    if input + output == 0 {
        return Vec::new();
    }

    let created_value = session
        .created_at
        .as_deref()
        .map(|s| Value::String(s.to_string()));
    let created_ms = parse_timestamp_value(created_value.as_ref());
    let modified_value = session
        .last_modified_at
        .as_deref()
        .map(|s| Value::String(s.to_string()));
    let modified_ms = parse_timestamp_value(modified_value.as_ref());

    let timestamp = created_ms.or(modified_ms).unwrap_or(fallback_timestamp);
    let duration_ms = duration_between_ms(created_ms, modified_ms);
    let model_id = session_model_id
        .or(flat_model_id)
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| UNKNOWN_MODEL.to_string());

    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        model_id,
        PROVIDER_ID,
        session_id.clone(),
        timestamp,
        TokenBreakdown {
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some(format!("{}:ide-session", session_id)),
    );
    message.message_count = flat_assistant_turns.max(1);
    message.set_timestamp_provenance(crate::TimestampProvenance::Aggregate);
    message.duration_ms = duration_ms;
    message.is_turn_start = true;
    message.set_workspace(workspace_key, workspace_label);
    vec![message]
}

/// Extract the workspace folder name from a Kiro globalStorage path.
///
/// Snapshots live under `.../globalStorage/kiro.kiroagent/<workspace>/...`,
/// so the workspace folder is the path segment immediately following the
/// `kiro.kiroagent` component. Returns `None` when no such segment exists.
fn kiro_global_storage_workspace(path: &Path) -> Option<String> {
    let mut components = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned());
    while let Some(component) = components.next() {
        if component == "kiro.kiroagent" {
            return components.next();
        }
    }
    None
}

#[derive(Debug, Default)]
struct KiroSnapshotTextCounts {
    prompt_chars: usize,
    assistant_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KiroSnapshotRole {
    Prompt,
    Assistant,
}

fn collect_kiro_snapshot_text(
    value: &Value,
    counts: &mut KiroSnapshotTextCounts,
    mut role: Option<KiroSnapshotRole>,
) {
    match value {
        Value::Object(map) => {
            // Real IDE-private `.chat` files use "human"/"bot" (with "tool" for
            // injected context, deliberately left unmatched); other snapshot
            // shapes use "user"/"assistant" or "prompt"/"response".
            if let Some(kind) = map.get("role").and_then(|v| v.as_str()) {
                role = match kind {
                    "user" | "prompt" | "human" => Some(KiroSnapshotRole::Prompt),
                    "assistant" | "response" | "bot" => Some(KiroSnapshotRole::Assistant),
                    _ => role,
                };
            }
            if let Some(kind) = map.get("type").and_then(|v| v.as_str()) {
                role = match kind {
                    "user" | "prompt" | "human" => Some(KiroSnapshotRole::Prompt),
                    "assistant" | "response" | "bot" => Some(KiroSnapshotRole::Assistant),
                    _ => role,
                };
            }

            // Each group below is an ordered list of *aliases* for the same
            // logical payload (text body, conversation list, sub-parts). Kiro
            // snapshots frequently store the identical text under more than one
            // alias in a single object (e.g. both `content` and `text`, or both
            // `messages` and `entries`). Descending into every present alias
            // would count that text once per alias and inflate token totals.
            //
            // However, an object may also legitimately hold *distinct* payloads
            // under several keys of the same group (e.g. a turn with both
            // `prompt` and `response`, or a chat with both `messages` and
            // `history` pointing at different subtrees). Visiting only the first
            // present key would silently drop those, undercounting tokens.
            //
            // So we descend into every present key in the group but de-duplicate
            // by VALUE: subtrees structurally equal to one already visited in the
            // same group are skipped. Distinct subtrees are all counted; repeated
            // (aliased) subtrees are counted once.
            for group in [
                // Inline text body of a single message.
                &["prompt", "response", "content", "text", "message"][..],
                // Container holding a list of messages/turns.
                &[
                    "messages",
                    "conversation",
                    "chat",
                    "transcript",
                    "entries",
                    "events",
                    "history",
                ][..],
                // Sub-parts of a single message.
                &["parts", "items", "nodes"][..],
            ] {
                let mut visited: Vec<&Value> = Vec::new();
                for key in group {
                    if let Some(item) = map.get(*key) {
                        if visited.contains(&item) {
                            continue;
                        }
                        visited.push(item);
                        collect_kiro_snapshot_text(item, counts, role);
                    }
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_kiro_snapshot_text(item, counts, role);
            }
        }
        Value::String(text) => match role {
            Some(KiroSnapshotRole::Assistant) => counts.assistant_chars += text.chars().count(),
            Some(KiroSnapshotRole::Prompt) => counts.prompt_chars += text.chars().count(),
            None => {}
        },
        _ => {}
    }
}

fn find_kiro_snapshot_model_id(value: &Value) -> Option<String> {
    static KIRO_INTERNAL_MODELS: &[&str] = &["agent", "auto", "qdev"];

    match value {
        Value::Object(map) => {
            for key in ["model_id", "modelId", "model"] {
                if let Some(model) = map.get(key).and_then(|v| v.as_str()) {
                    let model = model.trim();
                    if !model.is_empty()
                        && !KIRO_INTERNAL_MODELS.contains(&model.to_lowercase().as_str())
                    {
                        return Some(model.to_string());
                    }
                }
            }

            for key in [
                "messages",
                "conversation",
                "chat",
                "transcript",
                "entries",
                "events",
                "history",
                "prompt",
                "response",
                "content",
                "text",
                "message",
                "parts",
                "items",
                "nodes",
                "promptLogs",
                "completionOptions",
            ] {
                if let Some(item) = map.get(key) {
                    if let Some(model) = find_kiro_snapshot_model_id(item) {
                        return Some(model);
                    }
                }
            }

            None
        }
        Value::Array(items) => items.iter().find_map(find_kiro_snapshot_model_id),
        _ => None,
    }
}

fn parse_kiro_global_storage_file(path: &Path) -> Vec<UnifiedMessage> {
    let fallback_timestamp = file_modified_timestamp_ms(path);
    let json = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };

    let value: Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    if let Some(messages) = try_parse_kiro_execution_file(&value, path) {
        return messages;
    }

    if value.get("executions").is_some() && value.get("version").is_some() {
        return Vec::new();
    }

    if let Some(messages) = try_parse_kiro_workspace_session(&value, path, fallback_timestamp) {
        return messages;
    }

    let file_stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let workspace = kiro_global_storage_workspace(path);
    let workspace_key = workspace.as_deref().and_then(normalize_workspace_key);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
    let session_id = match workspace.as_deref() {
        Some(ws) => format!("{}/{}", ws, file_stem),
        None => file_stem.to_string(),
    };
    let model_id = find_kiro_snapshot_model_id(&value).unwrap_or_else(|| "auto".to_string());

    let mut counts = KiroSnapshotTextCounts::default();
    collect_kiro_snapshot_text(&value, &mut counts, None);

    let input = estimate_tokens(counts.prompt_chars);
    let output = estimate_tokens(counts.assistant_chars);
    if input + output == 0 {
        return Vec::new();
    }

    let snapshot_timestamp = fallback_timestamp;

    // IDE-private `.chat` files carry a top-level executionId referencing the
    // execution record stored under the sibling execution-store directory
    // (verified against real globalStorage trees: the same UUID appears as the
    // `.chat`'s executionId and the execution file's executionId). Tag the
    // dedup key with it so suppress_snapshots_covered_by_executions can drop
    // this snapshot when its execution is counted. `try_parse_kiro_execution_file`
    // already returned above for files that have `actions`, so this only tags
    // action-less chat/validation artifacts.
    let dedup_key = match value.get("executionId").and_then(|id| id.as_str()) {
        Some(execution_id) => format!("{}:globalstorage:exec:{}", session_id, execution_id),
        None => format!("{}:globalstorage", session_id),
    };

    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        model_id,
        PROVIDER_ID,
        session_id.clone(),
        snapshot_timestamp,
        TokenBreakdown {
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some(dedup_key),
    );
    message.message_count = 1;
    message.set_timestamp_provenance(crate::TimestampProvenance::Aggregate);
    message.is_turn_start = true;
    message.set_workspace(workspace_key, workspace_label);
    vec![message]
}

fn try_parse_kiro_execution_file(value: &Value, path: &Path) -> Option<Vec<UnifiedMessage>> {
    let obj = value.as_object()?;
    let execution_id = obj.get("executionId")?.as_str()?;
    let actions = obj.get("actions")?.as_array()?;
    let status = obj.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status != "succeed" {
        return Some(Vec::new());
    }

    let session_id = obj
        .get("chatSessionId")
        .and_then(|v| v.as_str())
        .unwrap_or(execution_id)
        .to_string();
    // Reuse the shared timestamp parser so epoch-seconds, epoch-millis, RFC3339
    // strings, and float values are all bucketed to the correct day (raw
    // `as_i64` silently mis-buckets everything except integer milliseconds).
    let start_time = parse_timestamp_value(obj.get("startTime"));
    let timestamp = start_time.unwrap_or_else(|| file_modified_timestamp_ms(path));
    let end_time = parse_timestamp_value(obj.get("endTime"));
    let duration_ms = duration_between_ms(start_time.or(Some(timestamp)), end_time);

    let mut output_chars = 0usize;
    for action in actions {
        let action_type = action
            .get("actionType")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !matches!(action_type, "say" | "reasoning") {
            continue;
        }
        let msg = action
            .get("output")
            .and_then(|o| {
                if let Some(s) = o.as_str() {
                    Some(s.to_string())
                } else {
                    o.get("message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                }
            })
            .unwrap_or_default();
        output_chars += msg.chars().count();
    }

    let input_chars = obj
        .get("context")
        .and_then(|ctx| ctx.get("messages"))
        .and_then(|msgs| msgs.as_array())
        .map(|msgs| {
            msgs.iter()
                .map(|m| {
                    m.get("entries")
                        .and_then(|e| e.as_array())
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|entry| {
                                    if entry.get("type").and_then(|t| t.as_str()) == Some("text") {
                                        entry
                                            .get("text")
                                            .and_then(|t| t.as_str())
                                            .map(|s| s.chars().count())
                                    } else {
                                        None
                                    }
                                })
                                .sum::<usize>()
                        })
                        .unwrap_or(0)
                })
                .sum::<usize>()
        })
        .unwrap_or(0)
        + obj
            .get("input")
            .and_then(|inp| inp.get("data"))
            .and_then(|data| data.get("messages"))
            .and_then(|msgs| msgs.as_array())
            .map(|msgs| {
                msgs.iter()
                    .map(|msg| {
                        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                            content
                                .iter()
                                .filter_map(|part| {
                                    if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                                        part.get("text")
                                            .and_then(|t| t.as_str())
                                            .map(|s| s.chars().count())
                                    } else {
                                        None
                                    }
                                })
                                .sum::<usize>()
                        } else if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                            text.chars().count()
                        } else {
                            0
                        }
                    })
                    .sum::<usize>()
            })
            .unwrap_or(0);

    let input = estimate_tokens(input_chars);
    let output = estimate_tokens(output_chars);
    if input + output == 0 {
        return Some(Vec::new());
    }

    // Prefer a real model id from the execution payload (context/completionOptions),
    // skipping Kiro-internal placeholders, and fall back to "auto" — mirroring the
    // snapshot path so pricing can resolve these messages.
    let model_id = find_kiro_snapshot_model_id(value).unwrap_or_else(|| "auto".to_string());

    // Attribute execution usage to its workspace, matching every other
    // globalStorage Kiro message.
    let workspace = kiro_global_storage_workspace(path);
    let workspace_key = workspace.as_deref().and_then(normalize_workspace_key);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        model_id,
        PROVIDER_ID,
        session_id,
        timestamp,
        TokenBreakdown {
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some(format!("execution:{}", execution_id)),
    );
    message.message_count = 1;
    message.set_timestamp_provenance(crate::TimestampProvenance::Aggregate);
    message.is_turn_start = true;
    message.duration_ms = duration_ms;
    message.set_workspace(workspace_key, workspace_label);
    Some(vec![message])
}

fn try_parse_kiro_workspace_session(
    value: &Value,
    path: &Path,
    fallback_timestamp: i64,
) -> Option<Vec<UnifiedMessage>> {
    let history = value.get("history")?.as_array()?;
    if value.get("sessionId").is_none() && value.get("selectedModel").is_none() {
        return None;
    }

    let file_stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let workspace = kiro_global_storage_workspace(path);
    let workspace_key = workspace.as_deref().and_then(normalize_workspace_key);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
    let session_id = value
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| match workspace.as_deref() {
            Some(ws) => format!("{}/{}", ws, file_stem),
            None => file_stem.to_string(),
        });

    let model_id = value
        .get("selectedModel")
        .and_then(|v| v.as_str())
        .filter(|m| !m.is_empty())
        .unwrap_or("auto")
        .to_string();

    let mut total_prompt_chars: usize = 0;
    let mut prompt_log_count: i32 = 0;
    let mut assistant_chars: usize = 0;

    for entry in history {
        if let Some(prompt_logs) = entry.get("promptLogs").and_then(|v| v.as_array()) {
            for pl in prompt_logs {
                if let Some(prompt) = pl.get("prompt").and_then(|v| v.as_str()) {
                    total_prompt_chars += prompt.chars().count();
                    prompt_log_count += 1;
                }
            }
        }
        if let Some(msg) = entry.get("message") {
            if msg.get("role").and_then(|v| v.as_str()) == Some("assistant") {
                if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                    assistant_chars += content.chars().count();
                }
            }
        }
    }

    if total_prompt_chars == 0 {
        return None;
    }

    let input = estimate_tokens(total_prompt_chars);
    let output = estimate_tokens(assistant_chars);

    if input + output == 0 {
        return Some(Vec::new());
    }

    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        model_id,
        PROVIDER_ID,
        session_id.clone(),
        fallback_timestamp,
        TokenBreakdown {
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some(format!("{}:workspace-session", session_id)),
    );
    message.message_count = prompt_log_count.max(1);
    message.set_timestamp_provenance(crate::TimestampProvenance::Aggregate);
    message.is_turn_start = true;
    message.set_workspace(workspace_key, workspace_label);
    Some(vec![message])
}

/// Drop globalStorage snapshot messages whose execution is already counted.
///
/// Kiro IDE's globalStorage (verified against real trees) holds, per workspace
/// hash directory: `<hash>.chat` artifacts carrying a top-level `executionId`
/// plus chat/context text, and extensionless execution records (in a nested
/// store directory) carrying the same `executionId` with the full `context`
/// history and `actions`. Counting both counts the same conversation text
/// twice; the execution record's input is a superset of the `.chat` content,
/// so the `.chat` message is redundant once its execution is present.
///
/// Matching is exact and workspace-scoped on the shared `executionId` (with a
/// legacy fallback matching an execution's `chatSessionId` against a snapshot
/// file stem). Workspace-session promptLogs snapshots are matched globally on
/// the session UUID instead, because they live under a different
/// `kiro.kiroagent` subdirectory than executions and so never share a
/// workspace key. Anything unmatched is kept — the pass can only remove
/// verified duplicates, never unrelated usage.
pub(crate) fn suppress_snapshots_covered_by_executions(
    messages: Vec<UnifiedMessage>,
) -> Vec<UnifiedMessage> {
    let mut executed_sessions: std::collections::HashSet<(Option<String>, String)> =
        std::collections::HashSet::new();
    let mut executed_ids: std::collections::HashSet<(Option<String>, String)> =
        std::collections::HashSet::new();
    let mut executed_session_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for message in &messages {
        let Some(execution_id) = message
            .dedup_key
            .as_deref()
            .and_then(|key| key.strip_prefix("execution:"))
        else {
            continue;
        };
        executed_sessions.insert((message.workspace_key.clone(), message.session_id.clone()));
        executed_ids.insert((message.workspace_key.clone(), execution_id.to_string()));
        executed_session_ids.insert(message.session_id.clone());
    }
    if executed_ids.is_empty() {
        return messages;
    }

    messages
        .into_iter()
        .filter(|message| {
            let Some(key) = message.dedup_key.as_deref() else {
                return true;
            };
            // `.chat` artifacts tagged with the execution they belong to.
            if let Some((_, execution_id)) = key.split_once(":globalstorage:exec:") {
                return !executed_ids
                    .contains(&(message.workspace_key.clone(), execution_id.to_string()));
            }
            // Workspace-session promptLogs snapshots duplicate the cumulative
            // request payloads already captured by that session's execution
            // records. They live under `kiro.kiroagent/workspace-sessions/`
            // while executions live under `kiro.kiroagent/<workspace-hash>/`,
            // so their workspace keys can never agree — match globally on the
            // session UUID (execution `chatSessionId` == workspace-session
            // `sessionId`). Sessions with no counted execution are kept.
            if key.ends_with(":workspace-session") {
                return !executed_session_ids.contains(&message.session_id);
            }
            if !key.ends_with(":globalstorage") {
                return true;
            }
            // Legacy fallback: snapshot session ids are `<workspace>/<file-stem>`
            // (or bare stem); match the stem against execution chatSessionIds.
            let stem = message
                .session_id
                .rsplit('/')
                .next()
                .unwrap_or(&message.session_id);
            !executed_sessions.contains(&(message.workspace_key.clone(), stem.to_string()))
        })
        .collect()
}

pub fn parse_kiro_sqlite(db_path: &Path) -> Vec<UnifiedMessage> {
    let conn = match Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to open Kiro CLI database"
            );
            return Vec::new();
        }
    };

    let query = "SELECT key, conversation_id, value FROM conversations_v2";
    let mut stmt = match conn.prepare(query) {
        Ok(s) => s,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to prepare Kiro conversations query"
            );
            return Vec::new();
        }
    };

    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) {
        Ok(r) => r,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to execute Kiro conversations query"
            );
            return Vec::new();
        }
    };

    let fallback_timestamp = file_modified_timestamp_ms(db_path);
    let mut messages = Vec::new();

    for row in rows.flatten() {
        let (cwd, conversation_id, json_str) = row;
        let parsed = match serde_json::from_str::<KiroDbConversation>(&json_str) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let context_window = parsed
            .model_info
            .as_ref()
            .and_then(|info| info.context_window_tokens)
            .unwrap_or(0);
        let model_id = parsed
            .model_info
            .as_ref()
            .and_then(|info| info.model_id.as_deref())
            .filter(|m| !m.trim().is_empty())
            .unwrap_or(UNKNOWN_MODEL)
            .to_string();
        let workspace_key = normalize_workspace_key(&cwd);
        let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

        let history = parsed.history.unwrap_or_default();
        for (index, turn) in history.into_iter().enumerate() {
            let Some(meta) = turn.request_metadata else {
                continue;
            };

            // NOTE: these are ESTIMATED, not measured token counts. Kiro's
            // conversations_v2 does not record real per-turn token usage, so
            // input is derived from context_usage_percentage * context_window
            // and output from response_size (char_count) / 4. Downstream must
            // not treat these as exact.
            let ctx_pct = meta.context_usage_percentage.unwrap_or(0.0);
            let response_size = meta.response_size.unwrap_or(0);

            let input = if context_window > 0 && ctx_pct > 0.0 {
                ((context_window as f64) * ctx_pct / 100.0) as i64
            } else {
                0
            };
            let output = estimate_tokens(response_size);

            if input + output == 0 {
                continue;
            }

            let request_start_timestamp_ms =
                meta.request_start_timestamp_ms.filter(|value| *value > 0);
            let stream_end_timestamp_ms = meta.stream_end_timestamp_ms.filter(|value| *value > 0);
            let duration_ms =
                duration_between_ms(request_start_timestamp_ms, stream_end_timestamp_ms);
            let timestamp = request_start_timestamp_ms
                .or(stream_end_timestamp_ms)
                .unwrap_or(fallback_timestamp);

            let mut message = UnifiedMessage::new_with_dedup(
                CLIENT_ID,
                model_id.clone(),
                PROVIDER_ID,
                conversation_id.clone(),
                timestamp,
                TokenBreakdown {
                    input,
                    output,
                    cache_read: 0,
                    cache_write: 0,
                    reasoning: 0,
                },
                0.0,
                Some(format!("{}:{}", conversation_id, index)),
            );
            message.message_count = 1;
            message.duration_ms = duration_ms;
            message.is_turn_start = true;
            if request_start_timestamp_ms.is_none() && stream_end_timestamp_ms.is_none() {
                message.set_timestamp_provenance(crate::TimestampProvenance::Fallback);
            }
            message.set_workspace(workspace_key.clone(), workspace_label.clone());
            messages.push(message);
        }
    }

    messages
}

#[derive(Debug, Deserialize)]
struct KiroDbConversation {
    history: Option<Vec<KiroDbTurn>>,
    model_info: Option<KiroModelInfo>,
}

#[derive(Debug, Deserialize)]
struct KiroDbTurn {
    request_metadata: Option<KiroDbRequestMetadata>,
}

#[derive(Debug, Deserialize)]
struct KiroDbRequestMetadata {
    context_usage_percentage: Option<f64>,
    response_size: Option<usize>,
    request_start_timestamp_ms: Option<i64>,
    stream_end_timestamp_ms: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_v2_without_timestamps_use_untrusted_file_day() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let conn = Connection::open(file.path()).unwrap();
        conn.execute(
            "CREATE TABLE conversations_v2 (key TEXT, conversation_id TEXT, value TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations_v2 (key, conversation_id, value) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "/tmp/project",
                "conversation",
                r#"{"model_info":{"context_window_tokens":1000,"model_id":"model"},"history":[{"request_metadata":{"context_usage_percentage":10.0,"response_size":40}}]}"#,
            ],
        )
        .unwrap();
        drop(conn);

        let messages = parse_kiro_sqlite(file.path());

        assert_eq!(messages.len(), 1);
        assert!(messages[0].timestamp > 0);
        assert_eq!(
            messages[0].timestamp_provenance,
            crate::TimestampProvenance::Fallback
        );
        assert_eq!(
            messages[0].date,
            crate::bucket_tz::bucket_timezone().date_of_ms(messages[0].timestamp)
        );
        assert!(!messages[0].is_trustworthy_for_hourly());
    }
}
