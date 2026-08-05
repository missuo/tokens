//! Grok Build session parser.
//!
//! Grok Build writes JSON-RPC session updates under
//! `~/.grok/sessions/<urlencoded-workspace>/<session-id>/updates.jsonl`.
//!
//! Current Grok Build versions emit authoritative per-turn usage on
//! `sessionUpdate = "turn_completed"`:
//!
//! ```json
//! "usage": {
//!   "inputTokens": 429343,
//!   "outputTokens": 5113,
//!   "totalTokens": 434456,
//!   "cachedReadTokens": 384512,
//!   "reasoningTokens": 1268,
//!   "modelCalls": 13,
//!   "apiDurationMs": 93698,
//!   "modelUsage": { "grok-4.5": { ... } }
//! }
//! ```
//!
//! Prefer that payload. Fall back to cumulative `_meta.totalTokens` deltas only
//! for older transcripts that never recorded `turn_completed.usage`. Session
//! rollups in sibling `signals.json` still reconcile residual totals in the
//! fallback path so compacted legacy sessions are not under-counted.

use super::utils::{
    extract_i64, extract_string, file_modified_timestamp_ms, parse_timestamp_value,
    read_file_or_none,
};
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const CLIENT_ID: &str = "grok";
const PROVIDER_ID: &str = "xai";
const UNKNOWN_MODEL: &str = "grok-unknown";

#[derive(Debug, Clone)]
struct GrokMetadata {
    session_id: String,
    model_id: Option<String>,
    timestamp: i64,
    workspace_key: Option<String>,
    workspace_label: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct GrokUsageTotals {
    input: i64,
    output: i64,
    cache_read: i64,
    reasoning: i64,
    model_calls: i64,
    api_duration_ms: i64,
}

impl GrokUsageTotals {
    fn from_usage_object(usage: &Value) -> Self {
        let raw_input = non_negative_i64(usage.get("inputTokens"));
        let output = non_negative_i64(usage.get("outputTokens"));
        let cache_read = non_negative_i64(usage.get("cachedReadTokens"))
            .max(non_negative_i64(usage.get("cacheReadTokens")))
            .min(raw_input);
        let reasoning = non_negative_i64(usage.get("reasoningTokens"));
        Self {
            // Grok's inputTokens is the full prompt size and already includes
            // cache hits. Split like the Codex parser so TokenBreakdown.total()
            // and pricing do not double-count cache reads.
            input: raw_input.saturating_sub(cache_read),
            output,
            cache_read,
            reasoning,
            model_calls: non_negative_i64(usage.get("modelCalls")),
            api_duration_ms: non_negative_i64(usage.get("apiDurationMs")),
        }
    }

    fn has_signal(self) -> bool {
        self.input > 0 || self.output > 0 || self.cache_read > 0 || self.reasoning > 0
    }

    fn into_tokens(self) -> TokenBreakdown {
        TokenBreakdown {
            input: self.input,
            output: self.output,
            cache_read: self.cache_read,
            cache_write: 0,
            reasoning: self.reasoning,
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveTurn {
    baseline_total: i64,
    max_total: i64,
    timestamp: i64,
    model_id: String,
    turn_index: usize,
}

impl ActiveTurn {
    fn new(baseline_total: i64, timestamp: i64, model_id: String, turn_index: usize) -> Self {
        Self {
            baseline_total,
            max_total: baseline_total,
            timestamp,
            model_id,
            turn_index,
        }
    }

    fn observe_total(&mut self, total: i64, timestamp: i64) {
        if total > self.max_total {
            self.max_total = total;
            self.timestamp = timestamp;
        }
    }

    fn into_message(self, metadata: &GrokMetadata) -> Option<UnifiedMessage> {
        let token_delta = self.max_total.saturating_sub(self.baseline_total);
        if token_delta <= 0 {
            return None;
        }

        let model_id = if self.model_id.trim().is_empty() {
            UNKNOWN_MODEL.to_string()
        } else {
            self.model_id
        };

        let mut message = UnifiedMessage::new_with_dedup(
            CLIENT_ID,
            model_id,
            PROVIDER_ID,
            metadata.session_id.clone(),
            self.timestamp,
            TokenBreakdown {
                input: token_delta,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            0.0,
            Some(format!("grok:{}:{}", metadata.session_id, self.turn_index)),
        );
        message.set_timestamp_provenance(crate::TimestampProvenance::Aggregate);
        message.set_workspace(
            metadata.workspace_key.clone(),
            metadata.workspace_label.clone(),
        );
        message.is_turn_start = true;
        Some(message)
    }
}

pub fn parse_grok_updates_file(path: &Path) -> Vec<UnifiedMessage> {
    if path.file_name().and_then(|name| name.to_str()) != Some("updates.jsonl") {
        return Vec::new();
    }

    let metadata = read_metadata(path);
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let mut usage_messages = Vec::new();
    let mut fallback_messages = Vec::new();
    let mut current_model = metadata
        .model_id
        .clone()
        .unwrap_or_else(|| UNKNOWN_MODEL.to_string());
    let mut last_total: Option<i64> = None;
    let mut last_total_timestamp = metadata.timestamp;
    let mut active_turn: Option<ActiveTurn> = None;
    let mut turn_index = 0usize;
    let mut usage_turn_index = 0usize;
    let mut saw_turn_completed_usage = false;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if let Some(model_id) = extract_model_id(&value) {
            current_model = model_id;
            if let Some(turn) = active_turn.as_mut() {
                if turn.model_id == UNKNOWN_MODEL {
                    turn.model_id = current_model.clone();
                }
            }
        }

        let timestamp = extract_timestamp_ms(&value).unwrap_or(metadata.timestamp);

        if let Some(messages) = extract_turn_completed_usage_messages(
            &value,
            &metadata,
            &current_model,
            usage_turn_index,
        ) {
            saw_turn_completed_usage = true;
            usage_turn_index = usage_turn_index.saturating_add(1);
            for message in messages {
                if let Some(model_id) =
                    Some(message.model_id.clone()).filter(|id| id != UNKNOWN_MODEL)
                {
                    current_model = model_id;
                }
                usage_messages.push(message);
            }
            // Authoritative usage already covers this turn; do not also invent a
            // cumulative-total fallback turn from the same rows.
            active_turn = None;
            continue;
        }

        // Once a transcript has real turn usage, skip the legacy cumulative
        // counter path entirely so we never double-count.
        if saw_turn_completed_usage {
            continue;
        }

        if is_user_message_chunk(&value) {
            if let Some(turn) = active_turn.take() {
                if let Some(message) = turn.into_message(&metadata) {
                    fallback_messages.push(message);
                }
            }

            active_turn = Some(ActiveTurn::new(
                last_total.unwrap_or(0),
                timestamp,
                current_model.clone(),
                turn_index,
            ));
            turn_index = turn_index.saturating_add(1);
        }

        let Some(total_tokens) = extract_total_tokens(&value) else {
            continue;
        };
        if total_tokens < 0 {
            continue;
        }

        match last_total {
            Some(previous) if total_tokens < previous => {
                // Compaction / rewind lowers the live context counter. Finalize
                // the in-flight turn against the pre-drop high-water mark, then
                // restart tracking from the post-compaction baseline so later
                // growth is not permanently ignored.
                if let Some(turn) = active_turn.take() {
                    if let Some(message) = turn.into_message(&metadata) {
                        fallback_messages.push(message);
                    }
                }
                last_total = Some(total_tokens);
                last_total_timestamp = timestamp;
                active_turn = Some(ActiveTurn::new(
                    total_tokens,
                    timestamp,
                    current_model.clone(),
                    turn_index,
                ));
                turn_index = turn_index.saturating_add(1);
            }
            Some(previous) if total_tokens == previous => {
                last_total_timestamp = timestamp;
            }
            Some(previous) => {
                if active_turn.is_none() {
                    active_turn = Some(ActiveTurn::new(
                        previous,
                        timestamp,
                        current_model.clone(),
                        turn_index,
                    ));
                    turn_index = turn_index.saturating_add(1);
                }
                if let Some(turn) = active_turn.as_mut() {
                    turn.observe_total(total_tokens, timestamp);
                }
                last_total_timestamp = timestamp;
                last_total = Some(total_tokens);
            }
            None => {
                if let Some(turn) = active_turn.as_mut() {
                    turn.observe_total(total_tokens, timestamp);
                }
                last_total_timestamp = timestamp;
                last_total = Some(total_tokens);
            }
        }
    }

    if saw_turn_completed_usage {
        return usage_messages;
    }

    if let Some(turn) = active_turn {
        if let Some(message) = turn.into_message(&metadata) {
            fallback_messages.push(message);
        }
    }

    if fallback_messages.is_empty() {
        if let Some(total_tokens) = last_total.filter(|tokens| *tokens > 0) {
            let aggregate_turn = ActiveTurn {
                baseline_total: 0,
                max_total: total_tokens,
                timestamp: last_total_timestamp,
                model_id: current_model.clone(),
                turn_index: 0,
            };
            if let Some(message) = aggregate_turn.into_message(&metadata) {
                fallback_messages.push(message);
            }
        }
    }

    append_signals_reconciliation(path, &metadata, &mut fallback_messages, &current_model);
    fallback_messages
}

fn extract_turn_completed_usage_messages(
    value: &Value,
    metadata: &GrokMetadata,
    fallback_model: &str,
    turn_index: usize,
) -> Option<Vec<UnifiedMessage>> {
    let update = get_path(value, &["params", "update"])?;
    if update.get("sessionUpdate").and_then(|v| v.as_str()) != Some("turn_completed") {
        return None;
    }

    let usage = update.get("usage")?;
    if !usage.is_object() {
        return None;
    }

    let timestamp = extract_timestamp_ms(value)
        .or_else(|| {
            get_path(value, &["params", "_meta", "agentTimestampMs"])
                .and_then(parse_timestamp_value)
        })
        .unwrap_or(metadata.timestamp);

    let mut messages = Vec::new();
    if let Some(model_usage) = usage.get("modelUsage").and_then(|v| v.as_object()) {
        for (model_id, model_usage_value) in model_usage {
            if !model_usage_value.is_object() {
                continue;
            }
            let totals = GrokUsageTotals::from_usage_object(model_usage_value);
            if !totals.has_signal() {
                continue;
            }
            let model = if model_id.trim().is_empty() {
                fallback_model.to_string()
            } else {
                model_id.clone()
            };
            messages.push(build_usage_message(
                metadata, &model, timestamp, totals, turn_index, model_id,
            ));
        }
    }

    if messages.is_empty() {
        let totals = GrokUsageTotals::from_usage_object(usage);
        if !totals.has_signal() {
            return None;
        }
        let model = metadata
            .model_id
            .clone()
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| fallback_model.to_string());
        messages.push(build_usage_message(
            metadata, &model, timestamp, totals, turn_index, "top",
        ));
    }

    if messages.is_empty() {
        None
    } else {
        Some(messages)
    }
}

fn build_usage_message(
    metadata: &GrokMetadata,
    model_id: &str,
    timestamp: i64,
    totals: GrokUsageTotals,
    turn_index: usize,
    model_key: &str,
) -> UnifiedMessage {
    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        model_id.to_string(),
        PROVIDER_ID,
        metadata.session_id.clone(),
        timestamp,
        totals.into_tokens(),
        0.0,
        Some(format!(
            "grok:{}:usage:{}:{}",
            metadata.session_id, turn_index, model_key
        )),
    );
    message.set_timestamp_provenance(crate::TimestampProvenance::Aggregate);
    message.set_workspace(
        metadata.workspace_key.clone(),
        metadata.workspace_label.clone(),
    );
    message.is_turn_start = true;
    if totals.api_duration_ms > 0 {
        message.duration_ms = Some(totals.api_duration_ms);
    }
    if totals.model_calls > 0 {
        // One turn_completed can cover multiple internal model calls.
        message.message_count = totals.model_calls.min(i64::from(i32::MAX)) as i32;
    }
    message
}

fn non_negative_i64(value: Option<&Value>) -> i64 {
    extract_i64(value).unwrap_or(0).max(0)
}

fn effective_total_from_signals(value: &Value) -> i64 {
    let before = non_negative_i64(value.get("totalTokensBeforeCompaction"));
    let total = non_negative_i64(value.get("totalTokens"));
    match value.get("contextTokensUsed") {
        None => before.saturating_add(total),
        Some(ctx) => total.max(before.saturating_add(non_negative_i64(Some(ctx)))),
    }
}

fn model_id_from_signals(value: &Value) -> Option<String> {
    extract_string(value.get("primaryModelId")).or_else(|| {
        value
            .get("modelsUsed")
            .and_then(|models| models.as_array())
            .and_then(|models| models.first())
            .and_then(|model| extract_string(Some(model)))
    })
}

fn append_signals_reconciliation(
    updates_path: &Path,
    metadata: &GrokMetadata,
    messages: &mut Vec<UnifiedMessage>,
    fallback_model: &str,
) {
    let signals_path = match sibling(updates_path, "signals.json") {
        Some(path) => path,
        None => return,
    };
    let data = match read_file_or_none(&signals_path) {
        Some(data) => data,
        None => return,
    };
    let value: Value = match serde_json::from_slice(&data) {
        Ok(value) => value,
        Err(_) => return,
    };

    let signals_total = effective_total_from_signals(&value);
    if signals_total <= 0 {
        return;
    }

    let updates_total: i64 = messages.iter().map(|message| message.tokens.input).sum();
    let extra = signals_total.saturating_sub(updates_total);
    if extra <= 0 {
        return;
    }

    let model_id = model_id_from_signals(&value)
        .filter(|model| !model.trim().is_empty())
        .or_else(|| metadata.model_id.clone())
        .unwrap_or_else(|| fallback_model.to_string());
    // Anchor the reconciliation delta to the last recorded update activity rather
    // than signals.json's mtime. The mtime advances every time Grok rewrites the
    // rollup for a live session, which would migrate this whole (potentially
    // multi-million-token) extra to a new day on each rescan and retroactively
    // shrink the prior day's total. The last update timestamp only moves when
    // genuine new activity is recorded, so the delta stays put across rescans.
    let timestamp = messages
        .iter()
        .map(|message| message.timestamp)
        .max()
        .unwrap_or(metadata.timestamp);

    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        model_id,
        PROVIDER_ID,
        metadata.session_id.clone(),
        timestamp,
        TokenBreakdown {
            input: extra,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some(format!("grok:{}:signals", metadata.session_id)),
    );
    message.set_timestamp_provenance(crate::TimestampProvenance::Aggregate);
    message.set_workspace(
        metadata.workspace_key.clone(),
        metadata.workspace_label.clone(),
    );
    messages.push(message);
}

fn read_metadata(path: &Path) -> GrokMetadata {
    let session_dir = path.parent();
    let session_id = session_dir
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .filter(|id| !id.trim().is_empty())
        .unwrap_or("unknown")
        .to_string();

    let workspace_key = session_dir
        .and_then(|dir| dir.parent())
        .and_then(|workspace_dir| workspace_dir.file_name())
        .and_then(|name| name.to_str())
        .map(percent_decode_lossy)
        .and_then(|decoded| normalize_workspace_key(&decoded));
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

    let fallback_timestamp = file_modified_timestamp_ms(path);
    let mut metadata = GrokMetadata {
        session_id,
        model_id: None,
        timestamp: fallback_timestamp,
        workspace_key,
        workspace_label,
    };

    if let Some(summary_path) = sibling(path, "summary.json") {
        read_summary_metadata(&summary_path, &mut metadata);
    }
    if let Some(events_path) = sibling(path, "events.jsonl") {
        read_events_metadata(&events_path, &mut metadata);
    }
    if let Some(signals_path) = sibling(path, "signals.json") {
        read_signals_metadata(&signals_path, &mut metadata);
    }

    metadata
}

fn read_signals_metadata(path: &Path, metadata: &mut GrokMetadata) {
    let Some(data) = read_file_or_none(path) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&data) else {
        return;
    };

    if metadata.model_id.is_none() {
        metadata.model_id = model_id_from_signals(&value);
    }
}

fn read_summary_metadata(path: &Path, metadata: &mut GrokMetadata) {
    let Some(data) = read_file_or_none(path) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&data) else {
        return;
    };

    if metadata.model_id.is_none() {
        metadata.model_id = extract_string(value.get("current_model_id"))
            .or_else(|| extract_string(value.get("model_id")));
    }

    if let Some(timestamp) = value
        .get("updated_at")
        .or_else(|| value.get("created_at"))
        .and_then(parse_timestamp_value)
    {
        metadata.timestamp = timestamp;
    }
}

fn read_events_metadata(path: &Path, metadata: &mut GrokMetadata) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };

    for line in BufReader::new(file).lines().map_while(Result::ok).take(500) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if metadata.model_id.is_none() {
            metadata.model_id = extract_string(value.get("model_id"));
        }
        if metadata.session_id == "unknown" {
            if let Some(session_id) = extract_string(value.get("session_id")) {
                metadata.session_id = session_id;
            }
        }
        if let Some(timestamp) = value.get("ts").and_then(parse_timestamp_value) {
            metadata.timestamp = timestamp;
        }

        if metadata.model_id.is_some() && metadata.session_id != "unknown" {
            break;
        }
    }
}

fn sibling(path: &Path, file_name: &str) -> Option<PathBuf> {
    Some(path.parent()?.join(file_name))
}

fn extract_model_id(value: &Value) -> Option<String> {
    for path in [
        &["params", "update", "_meta", "modelId"][..],
        &["params", "_meta", "modelId"][..],
        &["params", "modelId"][..],
        &["model_id"][..],
        &["modelId"][..],
        &["model"][..],
    ] {
        if let Some(model_id) = get_path(value, path).and_then(|value| extract_string(Some(value)))
        {
            if !model_id.trim().is_empty() {
                return Some(model_id);
            }
        }
    }
    None
}

fn extract_total_tokens(value: &Value) -> Option<i64> {
    // Only the live context counter paths. Do not read turn_completed.usage.totalTokens
    // here — that is absolute per-turn API usage and is handled separately.
    for path in [
        &["params", "_meta", "totalTokens"][..],
        &["params", "update", "_meta", "totalTokens"][..],
        &["params", "update", "totalTokens"][..],
        &["params", "totalTokens"][..],
        &["totalTokens"][..],
    ] {
        if let Some(total) = get_path(value, path).and_then(|value| extract_i64(Some(value))) {
            return Some(total);
        }
    }
    None
}

fn extract_timestamp_ms(value: &Value) -> Option<i64> {
    for path in [
        &["params", "_meta", "agentTimestampMs"][..],
        &["params", "update", "_meta", "agentTimestampMs"][..],
        &["params", "timestamp"][..],
        &["timestamp"][..],
        &["ts"][..],
    ] {
        if let Some(timestamp) = get_path(value, path).and_then(parse_timestamp_value) {
            return Some(timestamp);
        }
    }
    None
}

fn is_user_message_chunk(value: &Value) -> bool {
    get_path(value, &["params", "update", "sessionUpdate"]).and_then(|value| value.as_str())
        == Some("user_message_chunk")
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
}

fn percent_decode_lossy(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                decoded.push((high << 4) | low);
                i += 3;
                continue;
            }
        }

        decoded.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
