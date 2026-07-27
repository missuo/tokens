//! Gemini CLI session parser
//!
//! Parses JSON and JSONL session files from ~/.gemini/tmp/* supporting legacy
//! `session-*.json` files, UUID-named files in `chats/`, and current
//! `session-*.jsonl` chat recordings.

use super::utils::{
    extract_i64, extract_string, file_modified_timestamp_ms, parse_timestamp_value,
    read_file_or_none,
};
use super::UnifiedMessage;
use crate::TokenBreakdown;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Gemini session structure
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct GeminiSession {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "projectHash")]
    pub project_hash: String,
    #[serde(rename = "startTime")]
    pub start_time: String,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
    pub messages: Vec<GeminiMessage>,
}

/// Gemini message structure
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct GeminiMessage {
    pub id: String,
    pub timestamp: Option<String>,
    #[serde(rename = "type")]
    pub message_type: String,
    pub tokens: Option<Value>,
    pub model: Option<String>,
}

fn first_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|k| value.get(k).and_then(|v| v.as_i64()))
}

fn deserialize_tokens(value: &Value) -> Option<GeminiTokens> {
    Some(GeminiTokens {
        input: first_i64(
            value,
            &[
                "input",
                "prompt",
                "input_tokens",
                "prompt_tokens",
                "promptTokenCount",
            ],
        ),
        output: first_i64(
            value,
            &[
                "output",
                "candidates",
                "output_tokens",
                "completion_tokens",
                "candidatesTokenCount",
            ],
        ),
        cached: first_i64(
            value,
            &["cached", "cached_tokens", "cachedContentTokenCount"],
        ),
        thoughts: first_i64(value, &["thoughts", "reasoning", "thoughts_tokens"]),
        tool: first_i64(value, &["tool", "tool_tokens"]),
        total: first_i64(value, &["total", "totalTokenCount", "total_tokens"]),
    })
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct GeminiTokens {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cached: Option<i64>,
    pub thoughts: Option<i64>,
    pub tool: Option<i64>,
    pub total: Option<i64>,
}

pub(crate) struct GeminiParseResult {
    pub messages: Vec<UnifiedMessage>,
    pub cacheable: bool,
}

/// Parse a Gemini session file.
pub fn parse_gemini_file(path: &Path) -> Vec<UnifiedMessage> {
    parse_gemini_file_with_cache_status(path).messages
}

pub(crate) fn parse_gemini_file_with_cache_status(path: &Path) -> GeminiParseResult {
    let fallback_timestamp = file_modified_timestamp_ms(path);

    if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
        return parse_gemini_headless_jsonl(path, fallback_timestamp);
    }

    // Filter to expected Gemini layouts only:
    // - Legacy: files starting with "session-"
    // - Modern: path structure .../.gemini/tmp/<some_id>/chats/<file>.json
    let file_name_os = path.file_name().unwrap_or_default();

    // Fast path: legacy files are always accepted
    if !file_name_os
        .to_str()
        .map(|s| s.starts_with("session-"))
        .unwrap_or(false)
    {
        use std::ffi::OsStr;
        // Enforce the expected subdirectory pattern: tmp/<some_id>/chats/<file>
        let comps: Vec<&OsStr> = path.components().map(|c| c.as_os_str()).collect();
        let mut ok = false;
        'outer: for i in 0..comps.len().saturating_sub(1) {
            if comps[i] == "tmp" {
                // After "tmp", expect exactly 3 components: <some_id>, "chats", and the filename.
                let after_tmp = &comps[i + 1..];
                if after_tmp.len() == 3 {
                    let chats_dir = after_tmp[1];
                    let last = after_tmp[2];
                    if chats_dir == OsStr::new("chats") && last == file_name_os {
                        ok = true;
                        break 'outer;
                    }
                }
            }
        }
        if !ok {
            return GeminiParseResult {
                messages: Vec::new(),
                cacheable: true,
            };
        }
    }

    let Some(data) = read_file_or_none(path) else {
        return GeminiParseResult {
            messages: Vec::new(),
            cacheable: true,
        };
    };

    let mut bytes = data.clone();
    if let Ok(session) = simd_json::from_slice::<GeminiSession>(&mut bytes) {
        return GeminiParseResult {
            messages: parse_gemini_session(session, fallback_timestamp),
            cacheable: true,
        };
    }

    let mut bytes = data;
    if let Ok(value) = simd_json::from_slice::<Value>(&mut bytes) {
        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let messages = parse_gemini_headless_value(&value, &session_id, fallback_timestamp);
        if !messages.is_empty() {
            return GeminiParseResult {
                messages,
                cacheable: true,
            };
        }
    }

    parse_gemini_headless_jsonl(path, fallback_timestamp)
}

fn parse_gemini_session(session: GeminiSession, fallback_timestamp: i64) -> Vec<UnifiedMessage> {
    let mut messages = Vec::with_capacity(session.messages.len());
    let session_id = session.session_id.clone();

    for msg in session.messages {
        // Only process messages with token data
        let tokens = match msg.tokens.as_ref().and_then(deserialize_tokens) {
            Some(t) => t,
            None => continue,
        };

        let model = match msg.model {
            Some(m) => m,
            None => continue,
        };

        let timestamp = msg
            .timestamp
            .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(fallback_timestamp);
        messages.push(build_gemini_token_message(
            model,
            &session_id,
            timestamp,
            tokens,
        ));
    }

    messages
}

fn build_gemini_token_message(
    model: String,
    session_id: &str,
    timestamp: i64,
    tokens: GeminiTokens,
) -> UnifiedMessage {
    let (input, cache_read) = normalize_gemini_session_input_and_cache(
        tokens.input.unwrap_or(0),
        tokens.cached.unwrap_or(0),
        tokens.output.unwrap_or(0),
        tokens.thoughts.unwrap_or(0),
        tokens.tool.unwrap_or(0),
        tokens.total,
    );

    let tool = tokens.tool.unwrap_or(0).max(0);

    UnifiedMessage::new(
        "gemini",
        model,
        "google",
        session_id.to_string(),
        timestamp,
        TokenBreakdown {
            input: input.saturating_add(tool),
            output: tokens.output.unwrap_or(0).max(0),
            cache_read,
            cache_write: 0,
            reasoning: tokens.thoughts.unwrap_or(0).max(0),
        },
        0.0,
    )
}

fn parse_direct_gemini_token_message(
    value: &Value,
    model_hint: Option<String>,
    session_id: &str,
    fallback_timestamp: i64,
) -> Option<UnifiedMessage> {
    let model = extract_string(value.get("model")).or(model_hint)?;
    let tokens_value = value.get("tokens")?;
    let tokens = deserialize_tokens(tokens_value)?;
    let timestamp = extract_timestamp_from_value(value).unwrap_or(fallback_timestamp);

    Some(build_gemini_token_message(
        model, session_id, timestamp, tokens,
    ))
}

fn parse_gemini_headless_jsonl(path: &Path, fallback_timestamp: i64) -> GeminiParseResult {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return GeminiParseResult {
                messages: Vec::new(),
                cacheable: true,
            };
        }
    };

    let mut session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mut current_model: Option<String> = None;
    let mut reader = BufReader::new(file);
    let mut messages = Vec::with_capacity(64);
    let mut direct_message_indices: HashMap<String, usize> = HashMap::new();
    let mut line_buffer = Vec::with_capacity(4096);
    let mut json_buffer = Vec::with_capacity(4096);
    let mut skipped_malformed_line = false;

    loop {
        line_buffer.clear();
        let bytes_read = match reader.read_until(b'\n', &mut line_buffer) {
            Ok(n) => n,
            Err(_) => {
                skipped_malformed_line = true;
                break;
            }
        };
        if bytes_read == 0 {
            break;
        }

        let trimmed = trim_ascii_bytes(&line_buffer);
        if trimmed.is_empty() {
            continue;
        }

        json_buffer.clear();
        json_buffer.extend_from_slice(trimmed);
        let value: Value = match simd_json::from_slice(&mut json_buffer) {
            Ok(v) => v,
            Err(_) => {
                skipped_malformed_line = true;
                continue;
            }
        };

        let event_type = value.get("type").and_then(|val| val.as_str()).unwrap_or("");
        if event_type == "init" {
            if let Some(model) = extract_string(value.get("model")) {
                current_model = Some(model);
            }
            if let Some(id) =
                extract_string(value.get("session_id").or_else(|| value.get("sessionId")))
            {
                session_id = id;
            }
            continue;
        }

        if let Some(id) = extract_string(value.get("session_id").or_else(|| value.get("sessionId")))
        {
            session_id = id;
        }

        if event_type == "gemini" || value.get("tokens").is_some() {
            if let Some(model) = extract_string(value.get("model")) {
                current_model = Some(model);
            }

            if let Some(message) = parse_direct_gemini_token_message(
                &value,
                current_model.clone(),
                &session_id,
                fallback_timestamp,
            ) {
                if let Some(id) = extract_string(value.get("id")) {
                    if let Some(index) = direct_message_indices.get(&id).copied() {
                        messages[index] = message;
                    } else {
                        direct_message_indices.insert(id, messages.len());
                        messages.push(message);
                    }
                } else {
                    messages.push(message);
                }
            }
            continue;
        }

        let stats = value
            .get("stats")
            .or_else(|| value.get("result").and_then(|result| result.get("stats")));
        if let Some(stats) = stats {
            let timestamp = extract_timestamp_from_value(&value).unwrap_or(fallback_timestamp);
            messages.extend(build_messages_from_stats(
                stats,
                current_model.clone(),
                &session_id,
                timestamp,
            ));
        }
    }

    GeminiParseResult {
        messages,
        cacheable: !skipped_malformed_line,
    }
}

fn trim_ascii_bytes(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace());
    let Some(start) = start else {
        return &[];
    };

    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map(|idx| idx + 1)
        .unwrap_or(start);

    &bytes[start..end]
}

fn parse_gemini_headless_value(
    value: &Value,
    session_id: &str,
    fallback_timestamp: i64,
) -> Vec<UnifiedMessage> {
    if value.get("type").and_then(|val| val.as_str()) == Some("gemini")
        || value.get("tokens").is_some()
    {
        if let Some(message) =
            parse_direct_gemini_token_message(value, None, session_id, fallback_timestamp)
        {
            return vec![message];
        }
    }

    let stats = match value
        .get("stats")
        .or_else(|| value.get("result").and_then(|result| result.get("stats")))
    {
        Some(s) => s,
        None => return Vec::new(),
    };

    let model_hint = extract_string(value.get("model"));
    let timestamp = extract_timestamp_from_value(value).unwrap_or(fallback_timestamp);

    build_messages_from_stats(stats, model_hint, session_id, timestamp)
}

fn build_messages_from_stats(
    stats: &Value,
    model_hint: Option<String>,
    session_id: &str,
    timestamp: i64,
) -> Vec<UnifiedMessage> {
    let usages = extract_gemini_usages(stats, model_hint);
    usages
        .into_iter()
        .map(|usage| {
            let (input, cache_read) = if usage.input_includes_cache {
                normalize_gemini_headless_input_and_cache(usage.input, usage.cached)
            } else {
                (usage.input.max(0), usage.cached.max(0))
            };
            UnifiedMessage::new(
                "gemini",
                usage.model,
                "google",
                session_id.to_string(),
                timestamp,
                TokenBreakdown {
                    input,
                    output: usage.output.max(0),
                    cache_read,
                    cache_write: 0,
                    reasoning: usage.reasoning.max(0),
                },
                0.0,
            )
        })
        .collect()
}

fn subtract_cached_overlap(input: i64, cached: i64) -> (i64, i64) {
    let input = input.max(0);
    let cached = cached.max(0);
    let cached_portion = cached.min(input);
    (input.saturating_sub(cached_portion), cached)
}

fn normalize_gemini_headless_input_and_cache(input: i64, cached: i64) -> (i64, i64) {
    // Gemini usage_metadata promptTokenCount is cache-inclusive, while Tokens
    // represents non-cached input and cache hits as separate buckets.
    subtract_cached_overlap(input, cached)
}

fn normalize_gemini_session_input_and_cache(
    input: i64,
    cached: i64,
    output: i64,
    reasoning: i64,
    tool: i64,
    total: Option<i64>,
) -> (i64, i64) {
    let input = input.max(0);
    let cached = cached.max(0);

    let Some(total) = total.map(|value| value.max(0)) else {
        return (input, cached);
    };

    let inclusive_total = input
        .saturating_add(output.max(0))
        .saturating_add(reasoning.max(0))
        .saturating_add(tool.max(0));
    let exclusive_total = inclusive_total.saturating_add(cached);

    if cached > 0 && total == inclusive_total && total != exclusive_total {
        return subtract_cached_overlap(input, cached);
    }

    (input, cached)
}

struct GeminiHeadlessUsage {
    model: String,
    input: i64,
    output: i64,
    cached: i64,
    reasoning: i64,
    input_includes_cache: bool,
}

fn extract_gemini_usages(stats: &Value, model_hint: Option<String>) -> Vec<GeminiHeadlessUsage> {
    if let Some(models) = stats.get("models").and_then(|val| val.as_object()) {
        let mut usages = Vec::new();
        for (model, data) in models {
            if let Some(usage) = extract_gemini_usage_from_value(model.clone(), data) {
                usages.push(usage);
            }
        }

        if !usages.is_empty() {
            return usages;
        }
    }

    extract_gemini_usage_from_value(model_hint.unwrap_or_else(|| "unknown".to_string()), stats)
        .into_iter()
        .collect()
}

fn extract_gemini_usage_from_value(model: String, value: &Value) -> Option<GeminiHeadlessUsage> {
    let has_tokens_wrapper = value.get("tokens").is_some();
    let tokens = value.get("tokens").unwrap_or(value);
    let prompt_input = extract_i64(tokens.get("prompt"))
        .or_else(|| extract_i64(tokens.get("input_tokens")))
        .or_else(|| extract_i64(tokens.get("prompt_tokens")));
    let net_input = extract_i64(tokens.get("input"));
    let wrapper_input = if has_tokens_wrapper { net_input } else { None };
    let input = prompt_input.or(wrapper_input).or(net_input).unwrap_or(0);
    let output = extract_i64(tokens.get("candidates"))
        .or_else(|| extract_i64(tokens.get("output")))
        .or_else(|| extract_i64(tokens.get("output_tokens")))
        .or_else(|| extract_i64(tokens.get("candidates_tokens")))
        .unwrap_or(0);
    let cached = extract_i64(tokens.get("cached"))
        .or_else(|| extract_i64(tokens.get("cached_tokens")))
        .unwrap_or(0);
    let reasoning = extract_i64(tokens.get("thoughts"))
        .or_else(|| extract_i64(tokens.get("thoughts_tokens")))
        .or_else(|| extract_i64(tokens.get("reasoning")))
        .or_else(|| extract_i64(tokens.get("reasoning_tokens")))
        .unwrap_or(0);

    if input == 0 && output == 0 && cached == 0 && reasoning == 0 {
        return None;
    }

    Some(GeminiHeadlessUsage {
        model,
        input,
        output,
        cached,
        reasoning,
        input_includes_cache: prompt_input.is_some()
            || wrapper_input.is_some()
            || net_input.is_none(),
    })
}

fn extract_timestamp_from_value(value: &Value) -> Option<i64> {
    value
        .get("timestamp")
        .or_else(|| value.get("created_at"))
        .and_then(parse_timestamp_value)
}

