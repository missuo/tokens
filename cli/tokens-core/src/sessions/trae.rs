//! Trae usage API response parser.
//!
//! Reads `trae-cache/sessions/*.json` — the raw JSON dumped
//! from the usage API — and converts each entry into a `UnifiedMessage`.
//!
//! The API already returns exact token counts, so this parser does not
//! go through `pricing/lookup`.

use super::UnifiedMessage;
use crate::TokenBreakdown;

/// Known mapping from Trae display names to tiktoken-style model ids.
/// Unknown names fall through to the raw `model_name` from the API
/// (mixed-case, space-separated).
fn normalize_trae_model(name: &str) -> String {
    match name {
        "GPT-5.4" => "gpt-5.4",
        "GPT-5.3-Codex" | "GPT-5.3 Codex" => "gpt-5.3-codex",
        "GPT-5.3" => "gpt-5.3",
        "GPT-5.2-Codex" | "GPT-5.2 Codex" => "gpt-5.2-codex",
        "GPT-5.2" => "gpt-5.2",
        "GPT-5.1-Codex" | "GPT-5.1 Codex" => "gpt-5.1-codex",
        "GPT-5.1" => "gpt-5.1",
        "Gemini 3.1 Pro" => "gemini-3.1-pro",
        "Gemini 3.1" => "gemini-3.1",
        "GLM 5.1" | "GLM-5.1" => "glm-5.1",
        "Claude Sonnet 4.6" | "Claude-Sonnet-4.6" => "claude-sonnet-4.6",
        "Claude Sonnet 4.5" | "Claude-Sonnet-4.5" => "claude-sonnet-4.5",
        other => other,
    }
    .to_string()
}

/// Infer the provider from the display name.
fn provider_for_model(name: &str) -> &'static str {
    if name.contains("GPT") || name.contains("gpt") {
        "openai"
    } else if name.contains("Claude") || name.contains("claude") {
        "anthropic"
    } else if name.contains("Gemini") || name.contains("gemini") {
        "google"
    } else if name.contains("GLM") || name.contains("glm") {
        "zhipu"
    } else {
        "trae"
    }
}

/// Parse a single session JSON object into a `UnifiedMessage`.
fn parse_session(client: &str, session: &serde_json::Value) -> Option<UnifiedMessage> {
    let model_raw = session["model_name"].as_str().unwrap_or("");
    let mode = session["mode"].as_str().unwrap_or("");
    // Auto-mode sessions come back with `model_name: ""` because the
    // system picks a model per turn. Bucket them under `trae-<mode>` (e.g.
    // `trae-auto`) so the cost is still attributed instead of disappearing
    // into an empty Model cell.
    let model_id = if !model_raw.is_empty() {
        normalize_trae_model(model_raw)
    } else if !mode.is_empty() {
        format!("trae-{}", mode.to_ascii_lowercase())
    } else {
        "trae-unknown".to_string()
    };
    let provider = provider_for_model(&model_id);
    // Records without a real `session_id` cannot be deduplicated correctly
    // (every "missing-id" record would collide on the same key); records
    // without a positive `usage_time` would land at epoch 0. Drop them
    // rather than fabricating placeholders.
    let session_id = session["session_id"].as_str()?;
    let usage_time = session["usage_time"].as_i64()?;
    if session_id.is_empty() || usage_time <= 0 {
        return None;
    }
    // API returns epoch seconds; UnifiedMessage expects milliseconds. Use
    // `checked_mul` because the JSON cache is untrusted input — a crafted
    // `usage_time` near `i64::MAX` would panic in debug builds and silently
    // wrap to a negative timestamp in release builds.
    let timestamp_ms = usage_time.checked_mul(1000)?;
    let cost = session["dollar_float"].as_f64().unwrap_or(0.0);

    let extra = &session["extra_info"];
    let input = extra["input_token"].as_i64().unwrap_or(0);
    let output = extra["output_token"].as_i64().unwrap_or(0);
    let cache_read = extra["cache_read_token"].as_i64().unwrap_or(0);
    let cache_write = extra["cache_write_token"].as_i64().unwrap_or(0);

    if input + output + cache_read + cache_write == 0 {
        return None;
    }

    let dedup_key = Some(format!("trae:{}:{}", session_id, usage_time));

    Some(UnifiedMessage::new_with_dedup(
        client,
        model_id,
        provider,
        session_id,
        timestamp_ms,
        TokenBreakdown {
            input,
            output,
            cache_read,
            cache_write,
            reasoning: 0,
        },
        cost,
        dedup_key,
    ))
}

/// Parse a cache file containing an array of sessions as returned by the API.
pub fn parse_trae_file(client: &str, path: &std::path::Path) -> Vec<UnifiedMessage> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let sessions = match value.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    sessions
        .iter()
        .filter_map(|s| parse_session(client, s))
        .collect()
}

