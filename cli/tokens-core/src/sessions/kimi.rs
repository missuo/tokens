//! Kimi CLI / Kimi Code session parser
//!
//! Parses wire.jsonl from both `kimi-cli` and `kimi-code`.
//!
//! ~/.kimi/sessions/[GROUP_ID]/[SESSION_UUID]/wire.jsonl
//!   Token data comes from StatusUpdate messages.
//!
//! ~/.kimi-code/sessions/[WORKSPACE]/[SESSION]/agents/[AGENT]/wire.jsonl
//!   Token data comes from usage.record lines.

use super::utils::file_modified_timestamp_ms;
use super::UnifiedMessage;
use crate::provider_identity;
use crate::TokenBreakdown;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Top-level wire.jsonl line: either metadata or a timestamped message
#[derive(Debug, Deserialize)]
struct WireLine {
    timestamp: Option<f64>,
    message: Option<WireMessage>,
    #[serde(rename = "type")]
    line_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireMessage {
    #[serde(rename = "type")]
    msg_type: String,
    payload: Option<StatusPayload>,
}

#[derive(Debug, Deserialize)]
struct StatusPayload {
    token_usage: Option<TokenUsage>,
    #[allow(dead_code)]
    message_id: Option<String>,
}

/// Token usage counts shared by both wire formats.
///
/// Legacy kimi-cli StatusUpdate payloads use snake_case field names;
/// kimi-code usage.record lines use the camelCase aliases.
#[derive(Debug, Deserialize)]
struct TokenUsage {
    #[serde(alias = "inputOther")]
    input_other: Option<i64>,
    output: Option<i64>,
    #[serde(alias = "inputCacheRead")]
    input_cache_read: Option<i64>,
    #[serde(alias = "inputCacheCreation")]
    input_cache_creation: Option<i64>,
}

impl TokenUsage {
    /// Clamp negative counts to zero and build a breakdown.
    /// Returns `None` when every count is zero so callers can skip the entry.
    fn to_breakdown(&self) -> Option<TokenBreakdown> {
        let input = self.input_other.unwrap_or(0).max(0);
        let output = self.output.unwrap_or(0).max(0);
        let cache_read = self.input_cache_read.unwrap_or(0).max(0);
        let cache_write = self.input_cache_creation.unwrap_or(0).max(0);

        if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
            return None;
        }

        Some(TokenBreakdown {
            input,
            output,
            cache_read,
            cache_write,
            // Kimi wire protocols do not expose reasoning tokens; all reasoning included in output
            reasoning: 0,
        })
    }
}

/// Default model name when config.json is not available
const DEFAULT_MODEL: &str = "kimi-for-coding";
const DEFAULT_PROVIDER: &str = "moonshotai";
const UNKNOWN_PROVIDER: &str = "unknown";

/// Locate the legacy Kimi CLI config consumed by `parse_kimi_file`. Kimi Code
/// embeds model information in each wire record and does not use this file.
pub(crate) fn kimi_config_path(wire_path: &Path) -> Option<PathBuf> {
    let sessions_dir = wire_path.parent()?.parent()?.parent()?;
    Some(sessions_dir.parent()?.join("config.json"))
}

/// Read model name from ~/.kimi/config.json if available
fn read_model_from_config(wire_path: &Path) -> String {
    if let Some(config_path) = kimi_config_path(wire_path) {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(bytes) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(model) = bytes.get("model").and_then(|v| v.as_str()) {
                    if !model.is_empty() {
                        return model.to_string();
                    }
                }
            }
        }
    }
    DEFAULT_MODEL.to_string()
}

/// Extract session ID from the wire.jsonl path
/// Path format: ~/.kimi/sessions/GROUP_ID/SESSION_UUID/wire.jsonl
fn extract_session_id(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Check whether a wire.jsonl path belongs to kimi-code.
///
/// kimi-code writes `<root>/sessions/WORKSPACE/SESSION/agents/AGENT/wire.jsonl`
/// while legacy kimi-cli writes `<root>/sessions/GROUP/UUID/wire.jsonl`, so the
/// grandparent directory component (`agents`) distinguishes the formats. The
/// layout under the root is created by kimi-code itself, so this holds for the
/// default `~/.kimi-code` root and custom `KIMI_CODE_HOME` roots alike.
pub fn is_kimi_code_path(path: &Path) -> bool {
    path.parent()
        .and_then(|agent_dir| agent_dir.parent())
        .and_then(|agents_dir| agents_dir.file_name())
        .is_some_and(|name| name == "agents")
}

/// Extract session ID from a kimi-code wire.jsonl path.
/// Path format: ~/.kimi-code/sessions/WORKSPACE/SESSION_UUID/agents/AGENT/wire.jsonl
fn extract_session_id_from_kimi_code_path(path: &Path) -> String {
    // Walk up: wire.jsonl -> AGENT -> agents -> SESSION_UUID -> ...
    path.parent() // AGENT
        .and_then(|p| p.parent()) // agents
        .and_then(|p| p.parent()) // SESSION_UUID
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Strip the "kimi-code/" prefix from model IDs emitted by kimi-code.
fn normalize_kimi_code_model(model: &str) -> String {
    model
        .strip_prefix("kimi-code/")
        .unwrap_or(model)
        .to_string()
}

/// Kimi Code wire.jsonl line structure.
///
/// `llm.request` supplies protocol, concrete model, and runtime alias.
/// `usage.record` supplies the alias/model, token usage, scope, and time.
#[derive(Debug, Deserialize)]
struct KimiCodeWireLine {
    #[serde(rename = "type")]
    line_type: String,
    model: Option<String>,
    #[serde(rename = "modelAlias")]
    model_alias: Option<String>,
    provider: Option<String>,
    usage: Option<TokenUsage>,
    #[serde(rename = "usageScope")]
    usage_scope: Option<String>,
    time: Option<i64>,
}

#[derive(Debug)]
struct PendingKimiRequest {
    model_alias: String,
    model: String,
    provider: Option<String>,
}

impl PendingKimiRequest {
    fn from_wire_line(wire_line: &KimiCodeWireLine) -> Option<Self> {
        let model_alias = wire_line.model_alias.as_deref()?.trim();
        let model = normalize_kimi_code_model(wire_line.model.as_deref()?.trim());
        if model_alias.is_empty() || model.is_empty() {
            return None;
        }

        let provider = wire_line
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        Some(Self {
            model_alias: model_alias.to_string(),
            model,
            provider,
        })
    }
}

/// Select the nearest preceding same-alias request and retire the completed
/// request together with every older pending request. Newer requests remain.
fn consume_matching_kimi_request(
    pending_requests: &mut Vec<PendingKimiRequest>,
    usage_model: &str,
) -> Option<PendingKimiRequest> {
    let matched_index = pending_requests
        .iter()
        .rposition(|request| request.model_alias == usage_model)?;

    pending_requests.drain(..=matched_index).next_back()
}

fn resolve_kimi_code_provider(model_id: &str, provider_hint: Option<&str>) -> String {
    if let Some(provider) = provider_identity::inferred_provider_from_model(model_id) {
        return provider.to_string();
    }

    provider_hint
        .and_then(provider_identity::canonical_provider)
        // Kimi can log `openai` as a compatibility protocol for other owners.
        .filter(|provider| provider != "openai")
        .unwrap_or_else(|| UNKNOWN_PROVIDER.to_string())
}

fn resolve_kimi_code_usage_identity(
    recorded_model: &str,
    matched_request: Option<&PendingKimiRequest>,
) -> (String, String) {
    let normalized_recorded_model = normalize_kimi_code_model(recorded_model);
    let model_id = matched_request
        .map(|request| request.model.clone())
        .unwrap_or(normalized_recorded_model);
    let provider_hint = matched_request.and_then(|request| request.provider.as_deref());
    let provider_id = resolve_kimi_code_provider(&model_id, provider_hint);

    (model_id, provider_id)
}

/// Parse a Kimi Code wire.jsonl file.
pub fn parse_kimi_code_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let session_id = extract_session_id_from_kimi_code_path(path);
    let fallback_timestamp = file_modified_timestamp_ms(path);

    let reader = BufReader::new(file);
    let mut messages: Vec<UnifiedMessage> = Vec::new();
    let mut pending_requests: Vec<PendingKimiRequest> = Vec::new();

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
        let wire_line = match simd_json::from_slice::<KimiCodeWireLine>(&mut bytes) {
            Ok(wl) => wl,
            Err(_) => continue,
        };

        if wire_line.line_type == "llm.request" {
            let model_alias = wire_line
                .model_alias
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(request) = PendingKimiRequest::from_wire_line(&wire_line) {
                pending_requests.push(request);
            } else if let Some(model_alias) = model_alias {
                // A newer unusable request supersedes older candidates for its
                // alias so later usage cannot revive a failed request.
                pending_requests.retain(|request| request.model_alias != model_alias);
            }
            continue;
        }

        // Top-level usage.record remains authoritative. Nested step.end usage
        // is intentionally ignored to avoid double counting.
        if wire_line.line_type != "usage.record" {
            continue;
        }

        // Correlation and retirement occur before scope and zero-token filters,
        // but only a model actually present on the wire can identify an alias.
        let recorded_model = wire_line.model.as_deref();
        let matched_request = recorded_model
            .and_then(|model| consume_matching_kimi_request(&mut pending_requests, model));
        let (model_id, provider_id) = resolve_kimi_code_usage_identity(
            recorded_model.unwrap_or(DEFAULT_MODEL),
            matched_request.as_ref(),
        );

        if wire_line.usage_scope.as_deref() != Some("turn") {
            continue;
        }

        let Some(tokens) = wire_line.usage.as_ref().and_then(TokenUsage::to_breakdown) else {
            continue;
        };

        let timestamp_ms = wire_line.time.unwrap_or(fallback_timestamp);

        messages.push(UnifiedMessage::new(
            "kimi",
            model_id,
            provider_id,
            session_id.clone(),
            timestamp_ms,
            tokens,
            0.0,
        ));
    }

    messages
}

/// Parse a Kimi CLI wire.jsonl file
pub fn parse_kimi_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let model = read_model_from_config(path);
    let session_id = extract_session_id(path);

    let reader = BufReader::new(file);
    let mut messages: Vec<UnifiedMessage> = Vec::new();
    let mut keyed_indices: HashMap<String, usize> = HashMap::new();

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
        let wire_line = match simd_json::from_slice::<WireLine>(&mut bytes) {
            Ok(wl) => wl,
            Err(_) => continue,
        };

        // Skip metadata lines (first line: {"type": "metadata", ...})
        if wire_line.line_type.as_deref() == Some("metadata") {
            continue;
        }

        let message = match wire_line.message {
            Some(m) => m,
            None => continue,
        };

        // Only process StatusUpdate messages
        if message.msg_type != "StatusUpdate" {
            continue;
        }

        let payload = match message.payload {
            Some(p) => p,
            None => continue,
        };

        let token_usage = match payload.token_usage {
            Some(u) => u,
            None => continue,
        };

        // Convert Unix seconds (float) to milliseconds, fallback to file mtime
        let timestamp_ms = wire_line
            .timestamp
            .map(|ts| (ts * 1000.0) as i64)
            .unwrap_or_else(|| file_modified_timestamp_ms(path));

        // Skip entries with zero tokens
        let Some(tokens) = token_usage.to_breakdown() else {
            continue;
        };

        let dedup_key = payload.message_id;

        let message = UnifiedMessage::new_with_dedup(
            "kimi",
            model.clone(),
            DEFAULT_PROVIDER,
            session_id.clone(),
            timestamp_ms,
            tokens,
            0.0,
            dedup_key,
        );
        push_or_replace_status_update(&mut messages, &mut keyed_indices, message);
    }

    messages
}

fn exact_token_total(tokens: &TokenBreakdown) -> i128 {
    i128::from(tokens.input)
        + i128::from(tokens.output)
        + i128::from(tokens.cache_read)
        + i128::from(tokens.cache_write)
        + i128::from(tokens.reasoning)
}

fn should_replace_status_update(existing: &UnifiedMessage, candidate: &UnifiedMessage) -> bool {
    let existing_total = exact_token_total(&existing.tokens);
    let candidate_total = exact_token_total(&candidate.tokens);

    candidate_total > existing_total
        || (candidate_total == existing_total && candidate.timestamp >= existing.timestamp)
}

fn push_or_replace_status_update(
    messages: &mut Vec<UnifiedMessage>,
    keyed_indices: &mut HashMap<String, usize>,
    message: UnifiedMessage,
) {
    let dedup_key = message
        .dedup_key
        .as_ref()
        .filter(|key| !key.is_empty())
        .cloned();

    let Some(dedup_key) = dedup_key else {
        messages.push(message);
        return;
    };

    if let Some(index) = keyed_indices.get(&dedup_key).copied() {
        if should_replace_status_update(&messages[index], &message) {
            messages[index] = message;
        }
        return;
    }

    let index = messages.len();
    messages.push(message);
    keyed_indices.insert(dedup_key, index);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn write_kimi_code_wire(temp_dir: &TempDir, agent: &str, lines: &[String]) -> PathBuf {
        let path = temp_dir
            .path()
            .join("sessions")
            .join("workspace_123")
            .join("session_abc")
            .join("agents")
            .join(agent)
            .join("wire.jsonl");

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut contents = lines.join("\n");
        contents.push('\n');
        fs::write(&path, contents).unwrap();
        path
    }

    fn request(alias: &str, model: &str, provider: &str, time: i64) -> String {
        json!({
            "type": "llm.request",
            "provider": provider,
            "model": model,
            "modelAlias": alias,
            "time": time
        })
        .to_string()
    }

    fn usage(
        model: &str,
        scope: &str,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_write: i64,
        time: i64,
    ) -> String {
        json!({
            "type": "usage.record",
            "model": model,
            "usage": {
                "inputOther": input,
                "output": output,
                "inputCacheRead": cache_read,
                "inputCacheCreation": cache_write
            },
            "usageScope": scope,
            "time": time
        })
        .to_string()
    }

    fn usage_without_model(
        scope: &str,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_write: i64,
        time: i64,
    ) -> String {
        json!({
            "type": "usage.record",
            "usage": {
                "inputOther": input,
                "output": output,
                "inputCacheRead": cache_read,
                "inputCacheCreation": cache_write
            },
            "usageScope": scope,
            "time": time
        })
        .to_string()
    }

    fn step_end_with_usage(time: i64) -> String {
        json!({
            "type": "context.append_loop_event",
            "event": {
                "type": "step.end",
                "usage": {
                    "inputOther": 10,
                    "output": 5,
                    "inputCacheRead": 2,
                    "inputCacheCreation": 1
                }
            },
            "time": time
        })
        .to_string()
    }

    fn assert_identity(message: &UnifiedMessage, model: &str, provider: &str) {
        assert_eq!(message.model_id, model);
        assert_eq!(message.provider_id, provider);
    }

    #[test]
    fn kimi_code_secondary_alias_restores_grok_xai() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "grok-4.5", "openai", 1_000),
                usage("__secondary__", "turn", 10, 5, 2, 1, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "grok-4.5", "xai");
        assert_eq!(messages[0].session_id, "session_abc");
        assert_eq!(messages[0].timestamp, 2_000);
        assert_eq!(messages[0].tokens.input, 10);
        assert_eq!(messages[0].tokens.output, 5);
        assert_eq!(messages[0].tokens.cache_read, 2);
        assert_eq!(messages[0].tokens.cache_write, 1);
    }

    #[test]
    fn kimi_code_arbitrary_alias_restores_differing_concrete_model() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("cheap", "grok-4.5", "openai", 1_000),
                usage("cheap", "turn", 6, 3, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "grok-4.5", "xai");
    }

    #[test]
    fn kimi_code_unmatched_arbitrary_alias_is_retained() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[usage("cheap", "turn", 6, 3, 0, 0, 2_000)],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "cheap", "unknown");
    }

    #[test]
    fn kimi_code_missing_usage_model_does_not_match_or_consume_request() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("kimi-for-coding", "grok-4.5", "openai", 1_000),
                usage_without_model("turn", 6, 3, 0, 0, 2_000),
                usage("kimi-for-coding", "turn", 7, 4, 0, 0, 3_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 2);
        assert_identity(&messages[0], DEFAULT_MODEL, "moonshotai");
        assert_identity(&messages[1], "grok-4.5", "xai");
    }

    #[test]
    fn kimi_code_retry_uses_latest_matching_request() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "claude-sonnet-4", "anthropic", 1_000),
                request("__secondary__", "grok-4.5", "openai", 1_100),
                usage("__secondary__", "turn", 8, 3, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "grok-4.5", "xai");
    }

    #[test]
    fn kimi_code_completed_pair_retires_older_requests() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "claude-sonnet-4", "anthropic", 1_000),
                request("__secondary__", "grok-4.5", "openai", 1_100),
                usage("__secondary__", "turn", 8, 3, 0, 0, 2_000),
                usage("__secondary__", "turn", 7, 2, 0, 0, 3_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 2);
        assert_identity(&messages[0], "grok-4.5", "xai");
        assert_identity(&messages[1], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_zero_usage_consumes_request_before_omission() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "grok-4.5", "openai", 1_000),
                usage("__secondary__", "turn", 0, 0, 0, 0, 2_000),
                usage("__secondary__", "turn", 9, 4, 0, 0, 3_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_session_usage_consumes_request_before_scope_filter() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "grok-4.5", "openai", 1_000),
                usage("__secondary__", "session", 10, 5, 0, 0, 2_000),
                usage("__secondary__", "turn", 9, 4, 0, 0, 3_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_ignores_duplicate_step_end_usage() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "grok-4.5", "openai", 1_000),
                step_end_with_usage(1_900),
                usage("__secondary__", "turn", 10, 5, 2, 1, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "grok-4.5", "xai");
        assert_eq!(messages[0].tokens.total(), 18);
    }

    #[test]
    fn kimi_code_files_do_not_share_request_state() {
        let temp_dir = TempDir::new().unwrap();
        let main_path = write_kimi_code_wire(
            &temp_dir,
            "main",
            &[request("__secondary__", "kimi-k2.5", "openai", 1_000)],
        );
        let child_path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[usage("__secondary__", "turn", 7, 3, 0, 0, 2_000)],
        );

        assert!(parse_kimi_code_file(&main_path).is_empty());
        let child_messages = parse_kimi_code_file(&child_path);

        assert_eq!(child_messages.len(), 1);
        assert_identity(&child_messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_unknown_custom_model_over_openai_protocol_stays_unknown() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "private-model", "openai", 1_000),
                usage("__secondary__", "turn", 4, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "private-model", "unknown");
    }

    #[test]
    fn kimi_code_provider_resolution_prefers_model_ownership() {
        assert_eq!(
            resolve_kimi_code_provider("grok-4.5", Some("openai")),
            "xai"
        );
        assert_eq!(
            resolve_kimi_code_provider("gpt-5.6", Some("openai")),
            "openai"
        );
        assert_eq!(
            resolve_kimi_code_provider("claude-sonnet-4", Some("openai")),
            "anthropic"
        );
        assert_eq!(
            resolve_kimi_code_provider("gemini-2.5-pro", Some("openai")),
            "google"
        );
        assert_eq!(
            resolve_kimi_code_provider("kimi-k2.5", Some("openai")),
            "moonshotai"
        );
        assert_eq!(
            resolve_kimi_code_provider("private-model", Some("openai")),
            "unknown"
        );
    }

    #[test]
    fn kimi_code_moonshot_model_without_provider_hint_is_canonical() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "main",
            &[
                request("fast", "moonshot-v1", "", 1_000),
                usage("fast", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "moonshot-v1", "moonshotai");
    }

    #[test]
    fn kimi_code_logged_kimi_provider_is_canonical_moonshotai() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "main",
            &[
                request("k3", "k3", "kimi", 1_000),
                usage("k3", "turn", 10, 5, 2, 1, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "k3", "moonshotai");
        assert_eq!(messages[0].tokens.input, 10);
        assert_eq!(messages[0].tokens.output, 5);
        assert_eq!(messages[0].tokens.cache_read, 2);
        assert_eq!(messages[0].tokens.cache_write, 1);
    }

    #[test]
    fn kimi_code_concrete_kimi_model_uses_canonical_moonshot_provider() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "main",
            &[
                "{malformed json".to_string(),
                usage("kimi-code/kimi-k2.5", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "kimi-k2.5", "moonshotai");
    }

    #[test]
    fn kimi_code_request_without_nonempty_alias_is_not_a_candidate() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                json!({
                    "type": "llm.request",
                    "provider": "openai",
                    "model": "grok-4.5",
                    "modelAlias": "",
                    "time": 1_000
                })
                .to_string(),
                usage("__secondary__", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_request_without_nonempty_normalized_model_is_not_a_candidate() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "kimi-code/", "openai", 1_000),
                usage("__secondary__", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_invalid_newer_same_alias_retires_older_request() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "claude-sonnet-4", "anthropic", 1_000),
                request("__secondary__", "kimi-code/", "openai", 1_100),
                usage("__secondary__", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn legacy_kimi_parsing_keeps_accounting_and_canonical_provider() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let path = root
            .join("sessions")
            .join("group-1")
            .join("session-legacy")
            .join("wire.jsonl");

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(root.join("config.json"), r#"{"model":"kimi-for-coding"}"#).unwrap();
        fs::write(
            &path,
            concat!(
                r#"{"timestamp":1700000000.0,"message":{"type":"StatusUpdate","payload":{"token_usage":{"input_other":10,"output":5,"input_cache_read":2,"input_cache_creation":1},"message_id":"msg-1"}}}"#,
                "\n"
            ),
        )
        .unwrap();

        let messages = parse_kimi_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "kimi-for-coding", "moonshotai");
        assert_eq!(messages[0].session_id, "session-legacy");
        assert_eq!(messages[0].timestamp, 1_700_000_000_000);
        assert_eq!(messages[0].tokens.total(), 18);
    }
}
