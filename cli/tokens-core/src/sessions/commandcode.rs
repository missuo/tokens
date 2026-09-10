//! Command Code session parser
//!
//! Parses JSONL transcripts from `~/.commandcode/projects/<slug>/<session>.jsonl`.
//!
//! Assistant entries persist the real per-request token usage and cost on the
//! line itself (`usage` with `inputTokens`/`outputTokens`/`cacheReadTokens`/
//! `cacheWriteTokens`/`costUsd`, plus `model`); when present, they are used
//! verbatim and the cost is marked authoritative. Transcripts without `usage`
//! (older versions or truncated turns) only contain message text, so token
//! counts are ESTIMATED from message text at ~4 characters per token,
//! consistent with this crate's other estimated sources (see Kiro).
//!
//! These estimates approximate tokens processed; they will not match Command
//! Code's server-reported usage, which reflects tool-output truncation and
//! auxiliary model runs (e.g. tool-desc, taste-1) absent from the transcript.
//!
//! **Input estimation is per-turn, not cumulative.**
//! Command Code re-sends prior context on each
//! request, but the on-disk transcript does not say how much of that context is
//! cached versus re-billed. Each assistant turn's input is therefore estimated
//! from only the *new* context that turn introduced — the user prompt plus any
//! tool results since the previous assistant response — and attributed entirely
//! as fresh (non-cached) input (`cache_read = 0`). Counting the *cumulative*
//! conversation context on every turn instead (the previous behavior) grows the
//! per-turn input across the session, costs O(N^2) characters scanned for an
//! N-turn session, and inflates reported input far beyond comparable clients.
//! The per-turn delta sums to each message's own content exactly once across the
//! whole session, which is the same accounting other estimated clients use.
//! Whether re-sent context should be attributed to `cache_read` remains a
//! maintainer decision requiring Command Code's real billing model, which is not
//! available for turns without `usage`. Do not silently change the estimation
//! model without a corresponding update to this doc-comment and the pinning
//! test `test_commandcode_input_is_per_turn_delta`.
//!
//! Output (for usage-less turns) is estimated from the assistant message's own
//! content. The model id is read from the entry, falling back to the last
//! `model_change` entry, then `~/.commandcode/config.json` (the configured
//! agent model), and finally "unknown".

use super::utils::file_modified_timestamp_ms;
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

const CLIENT_ID: &str = "commandcode";
const PROVIDER_ID: &str = "command-code";
const UNKNOWN_MODEL: &str = "unknown";

#[derive(Debug, Deserialize)]
struct CommandCodeEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    id: Option<String>,
    timestamp: Option<String>,
    /// Flat (legacy) shape: one JSON object per line with the message fields
    /// at the top level.
    role: Option<String>,
    content: Option<serde_json::Value>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    /// v3 tree shape: the message is wrapped in a `message` object, and
    /// assistant entries carry real usage and the model on the line itself.
    message: Option<CommandCodeMessage>,
    usage: Option<CommandCodeUsage>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommandCodeMessage {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CommandCodeUsage {
    #[serde(rename = "inputTokens")]
    input_tokens: Option<i64>,
    #[serde(rename = "outputTokens")]
    output_tokens: Option<i64>,
    #[serde(rename = "cacheReadTokens")]
    cache_read_tokens: Option<i64>,
    #[serde(rename = "cacheWriteTokens")]
    cache_write_tokens: Option<i64>,
    #[serde(rename = "costUsd")]
    cost_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CommandCodeConfig {
    model: Option<String>,
}

pub fn parse_commandcode_file(path: &Path) -> Vec<UnifiedMessage> {
    // The `*.jsonl` glob also matches the per-session checkpoint log
    // (`<session>.checkpoints.jsonl`), which is a snapshot stream, not a
    // transcript. Skip it explicitly rather than relying on schema mismatch.
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".checkpoints.jsonl"))
    {
        return Vec::new();
    }

    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let fallback_timestamp = file_modified_timestamp_ms(path);
    let config_model = model_from_config(path);
    let session_id_from_path = session_id_from_path(path);
    let workspace_key = workspace_key_from_path(path);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

    let mut messages = Vec::new();
    let mut session_id: Option<String> = None;
    // Model from the most recent `model_change` entry; used when a message
    // entry does not carry its own `model` field.
    let mut last_changed_model: Option<String> = None;
    // Char count of the *new* context added since the previous assistant
    // response (the user prompt plus any tool results for this turn). This
    // stands in for the input (prompt) tokens of the current request without
    // re-counting the entire conversation history every turn — counting the
    // cumulative context instead grows the per-turn input across the session
    // (O(N^2) total) and inflates input versus other clients.
    let mut turn_input_chars: usize = 0;
    // The first assistant message after a user message starts a new turn.
    let mut pending_turn_start = false;
    let mut assistant_index = 0usize;

    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let entry = match serde_json::from_str::<CommandCodeEntry>(trimmed) {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        // The header line carries the session id.
        if entry.entry_type.as_deref() == Some("session") {
            session_id = session_id
                .or_else(|| entry.id.clone())
                .filter(|id| !id.is_empty());
            continue;
        }

        if entry.entry_type.as_deref() == Some("model_change") {
            last_changed_model = entry.model.clone().filter(|model| !model.trim().is_empty());
            continue;
        }

        if session_id.is_none() {
            if let Some(id) = entry.session_id.as_deref().filter(|id| !id.is_empty()) {
                session_id = Some(id.to_string());
            }
        }

        // Resolve role/content from whichever shape the entry uses.
        let role = entry
            .role
            .clone()
            .or_else(|| entry.message.as_ref().and_then(|m| m.role.clone()));
        let content = entry
            .content
            .clone()
            .or_else(|| entry.message.as_ref().and_then(|m| m.content.clone()));
        let chars = content.as_ref().map(content_chars).unwrap_or(0);

        match role.as_deref() {
            Some("assistant") => {
                let (tokens, cost, has_real_usage) = match entry.usage.as_ref() {
                    Some(usage) => (
                        TokenBreakdown {
                            input: usage.input_tokens.unwrap_or(0),
                            output: usage.output_tokens.unwrap_or(0),
                            cache_read: usage.cache_read_tokens.unwrap_or(0),
                            cache_write: usage.cache_write_tokens.unwrap_or(0),
                            reasoning: 0,
                        },
                        usage.cost_usd.unwrap_or(0.0),
                        true,
                    ),
                    None => (
                        TokenBreakdown {
                            input: estimate_tokens(turn_input_chars),
                            output: estimate_tokens(chars),
                            cache_read: 0,
                            cache_write: 0,
                            reasoning: 0,
                        },
                        0.0,
                        false,
                    ),
                };
                // This turn's input has been consumed; the next turn's input is
                // only the *new* context that follows this response. The
                // assistant's own output is not part of any input estimate.
                turn_input_chars = 0;

                if tokens.total() == 0 {
                    pending_turn_start = false;
                    continue;
                }

                // Per-entry model, falling back to the most recent model
                // change, then to the configured agent model.
                let raw_model = entry
                    .model
                    .clone()
                    .or(last_changed_model.clone())
                    .or_else(|| config_model.clone())
                    .filter(|model| !model.trim().is_empty());
                // Recover the real provider from the gateway id (e.g.
                // `MiniMaxAI/MiniMax-M3-Free` -> `minimax`) so pricing resolves
                // to that provider's catalog. The client's own `command-code`
                // provider is not a pricing provider, so without this a
                // MiniMax model would never reach a `minimax/...` key.
                // Falls back to `command-code` when nothing is inferred.
                let provider_id = raw_model
                    .as_deref()
                    .and_then(provider_hint_for_model)
                    .unwrap_or(PROVIDER_ID)
                    .to_string();
                let model_id = raw_model
                    .map(|model| canonicalize_model(&model))
                    .unwrap_or_else(|| UNKNOWN_MODEL.to_string());

                let resolved_session = session_id
                    .clone()
                    .unwrap_or_else(|| session_id_from_path.clone());
                let dedup_key = format!("{}:{}", resolved_session, assistant_index);
                let timestamp = entry
                    .timestamp
                    .as_deref()
                    .and_then(parse_rfc3339_ms)
                    .unwrap_or(fallback_timestamp);

                let mut message = UnifiedMessage::new_with_dedup(
                    CLIENT_ID,
                    model_id,
                    provider_id,
                    resolved_session,
                    timestamp,
                    tokens,
                    cost,
                    Some(dedup_key),
                );
                if has_real_usage {
                    // The provider reported both the tokens and the cost;
                    // local pricing must not overwrite either.
                    message.mark_provider_reported_cost();
                }
                message.message_count = 1;
                message.is_turn_start = pending_turn_start;
                message.set_workspace(workspace_key.clone(), workspace_label.clone());
                messages.push(message);

                assistant_index += 1;
                pending_turn_start = false;
            }
            Some("user") => {
                pending_turn_start = true;
                turn_input_chars += chars;
            }
            // Tool results (and any other roles) are part of the new context the
            // model sees on the next turn.
            _ => {
                turn_input_chars += chars;
            }
        }
    }

    messages
}

/// Char count of a message's `content` for token estimation, measured from its
/// canonical JSON serialization. Counting the serialized form keeps every
/// prompt-bearing byte the model receives — object keys (`command`, `path`, …),
/// tool-call arguments, tool-result payloads, and numeric/boolean values — and
/// avoids guessing which fields are structural versus content.
///
/// Genuinely empty content (null, `[]`, `{}`) counts as zero so that contentless
/// turns are not charged for their structural brackets.
fn content_chars(content: &serde_json::Value) -> usize {
    match content {
        serde_json::Value::Null => 0,
        serde_json::Value::Array(items) if items.is_empty() => 0,
        serde_json::Value::Object(map) if map.is_empty() => 0,
        _ => serde_json::to_string(content)
            .map(|serialized| serialized.chars().count())
            .unwrap_or(0),
    }
}

fn estimate_tokens(chars: usize) -> i64 {
    chars.div_ceil(4) as i64
}

/// Canonicalize the configured model id for pricing. Command Code reports
/// gateway ids such as `MiniMaxAI/MiniMax-M3-Free`; the `-Free` suffix is a
/// temporary promo and the org prefix is not a key the pricing resolver
/// recognizes verbatim. Dropping the org segment yields the real paid model
/// (e.g. `MiniMax-M3`) so output pricing resolves; the provider hint that the
/// org segment carried (e.g. `minimax`) is recovered separately by
/// [`provider_hint_for_model`] and applied to `provider_id`, so pricing keys
/// like `minimax/minimax-m3` are still reached.
fn canonicalize_model(model: &str) -> String {
    let base = model.rsplit('/').next().unwrap_or(model);
    // Char-safe, case-insensitive suffix strip. The original code byte-sliced
    // `base[base.len() - N..]` guarded only by a length check, which panics on a
    // non-ASCII model id from the untrusted `~/.commandcode/config.json` when
    // the byte index lands mid-codepoint. `-free` is pure ASCII, so when the
    // lowercased tail matches, the matched bytes are guaranteed ASCII and
    // `base.len() - PROMO_SUFFIX.len()` is a valid char boundary.
    const PROMO_SUFFIX: &str = "-free";
    if base.len() > PROMO_SUFFIX.len()
        && base
            .get(base.len() - PROMO_SUFFIX.len()..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(PROMO_SUFFIX))
    {
        base[..base.len() - PROMO_SUFFIX.len()].to_string()
    } else {
        base.to_string()
    }
}

/// Recover the provider hint that the configured model id carries (e.g.
/// `MiniMaxAI/MiniMax-M3-Free` -> `minimax`) so pricing resolves to the real
/// provider's catalog. Command Code's own `command-code` provider id is not a
/// pricing provider, so without this hint a MiniMax model would never reach a
/// `minimax/...` pricing key. Returns `None` when no known provider can be
/// inferred, leaving the default `command-code` provider in place.
fn provider_hint_for_model(model: &str) -> Option<&'static str> {
    crate::provider_identity::inferred_provider_from_model(model)
}

fn parse_rfc3339_ms(timestamp: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

/// Read the configured agent model from `~/.commandcode/config.json`.
///
/// `session_path` is `<root>/.commandcode/projects/<slug>/<session>.jsonl`, so
/// the config file lives three directories up.
fn model_from_config(session_path: &Path) -> Option<String> {
    let commandcode_root = session_path.parent()?.parent()?.parent()?;
    let config_path = commandcode_root.join("config.json");
    let bytes = std::fs::read(config_path).ok()?;
    let config: CommandCodeConfig = serde_json::from_slice(&bytes).ok()?;
    config.model.filter(|model| !model.trim().is_empty())
}

fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Command Code names project directories after a slugified working directory
/// (e.g. `users-alice-development-repo`). The original path is not recoverable
/// (lowercased, separators collapsed), so the slug itself is used as the
/// workspace key.
fn workspace_key_from_path(path: &Path) -> Option<String> {
    path.parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .and_then(normalize_workspace_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CostSource;

    /// Build a transcript at `<root>/.commandcode/projects/<slug>/<id>.jsonl`
    /// plus an optional `<root>/.commandcode/config.json`.
    fn fixture(
        root: &std::path::Path,
        slug: &str,
        session_id: &str,
        config_model: Option<&str>,
        lines: &[&str],
    ) -> std::path::PathBuf {
        let cc_root = root.join(".commandcode");
        let projects = cc_root.join("projects").join(slug);
        std::fs::create_dir_all(&projects).unwrap();
        if let Some(model) = config_model {
            std::fs::write(
                cc_root.join("config.json"),
                format!("{{\"model\": {model:?}}}"),
            )
            .unwrap();
        }
        let path = projects.join(format!("{session_id}.jsonl"));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    const TS: &str = "2026-09-10T03:11:06.582Z";
    const TS_MS: i64 = 1_789_009_866_582;

    #[test]
    fn test_commandcode_parses_real_usage_and_cost_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "sess-1",
            None,
            &[
                r#"{"type":"session","version":3,"id":"sess-1","timestamp":"2026-09-10T03:10:55.728Z","cwd":"/Users/alice/repo"}"#,
                r#"{"type":"message","id":"m1","parentId":null,"timestamp":"2026-09-10T03:10:56Z","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
                r#"{"type":"message","id":"m2","parentId":"m1","timestamp":"2026-09-10T03:11:06.582Z","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]},"usage":{"inputTokens":20783,"outputTokens":59,"cacheReadTokens":7296,"cacheWriteTokens":128,"costUsd":0.003057152},"model":"deepseek/deepseek-v4-flash"}"#,
            ],
        );
        assert_eq!(parse_rfc3339_ms(TS), Some(TS_MS));

        let messages = parse_commandcode_file(&path);
        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        assert_eq!(msg.client, "commandcode");
        assert_eq!(msg.session_id, "sess-1");
        assert_eq!(msg.model_id, "deepseek-v4-flash");
        assert_eq!(msg.provider_id, "deepseek");
        assert_eq!(msg.timestamp, TS_MS);
        assert_eq!(
            msg.tokens,
            TokenBreakdown {
                input: 20783,
                output: 59,
                cache_read: 7296,
                cache_write: 128,
                reasoning: 0,
            }
        );
        assert!((msg.cost - 0.003057152).abs() < 1e-9);
        assert_eq!(msg.cost_source, CostSource::ProviderReported);
        assert!(msg.is_turn_start);
        assert_eq!(msg.workspace_key.as_deref(), Some("users-alice-repo"));
    }

    #[test]
    fn test_commandcode_zero_input_all_cache_read_is_not_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "sess-cache",
            None,
            &[
                r#"{"type":"session","version":3,"id":"sess-cache","timestamp":"2026-09-10T03:10:55Z"}"#,
                r#"{"type":"message","id":"m1","parentId":null,"timestamp":"2026-09-10T03:10:56Z","message":{"role":"user","content":[{"type":"text","text":"hello there"}]}}"#,
                r#"{"type":"message","id":"m2","parentId":"m1","timestamp":"2026-09-10T03:11:06Z","message":{"role":"assistant","content":[{"type":"text","text":"hi"}]},"usage":{"inputTokens":0,"outputTokens":10,"cacheReadTokens":9999,"cacheWriteTokens":0,"costUsd":0.001},"model":"org/model-x"}"#,
            ],
        );

        let messages = parse_commandcode_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.input, 0);
        assert_eq!(messages[0].tokens.cache_read, 9999);
    }

    #[test]
    fn test_commandcode_input_is_per_turn_delta() {
        // Pinning test for the estimation fallback: with no `usage` fields,
        // each assistant turn's input is estimated from only the new context
        // that turn introduced (the user prompt), never the cumulative
        // conversation.
        let user_content = serde_json::json!([{"type": "text", "text": "aaaa"}]);
        let expected = estimate_tokens(content_chars(&user_content));
        let user_line = format!(
            r#"{{"type":"message","id":"m","parentId":null,"timestamp":"2026-09-10T03:10:56Z","message":{{"role":"user","content":{content}}}}}"#,
            content = user_content
        );
        let assistant_line = r#"{"type":"message","id":"a","parentId":"m","timestamp":"2026-09-10T03:11:06Z","message":{"role":"assistant","content":[{"type":"text","text":"bbbb"}]}}"#;

        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "sess-est",
            None,
            &[
                r#"{"type":"session","version":3,"id":"sess-est","timestamp":"2026-09-10T03:10:55Z"}"#,
                &user_line,
                assistant_line,
                &user_line,
                assistant_line,
            ],
        );

        let messages = parse_commandcode_file(&path);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].tokens.input, expected);
        assert_eq!(messages[1].tokens.input, expected);
        assert!(messages[0].is_turn_start);
        assert!(messages[1].is_turn_start);
        assert_eq!(messages[0].cost_source, CostSource::Unknown);
    }

    #[test]
    fn test_commandcode_model_falls_back_to_model_change_then_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "sess-model",
            Some("Config/Config-Model"),
            &[
                r#"{"type":"session","version":3,"id":"sess-model","timestamp":"2026-09-10T03:10:55Z"}"#,
                r#"{"type":"model_change","id":"mc1","parentId":null,"timestamp":"2026-09-10T03:11:00Z","model":"org/changed-model"}"#,
                r#"{"type":"message","id":"m1","parentId":null,"timestamp":"2026-09-10T03:10:56Z","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
                // No `model` on this entry -> falls back to model_change.
                r#"{"type":"message","id":"m2","parentId":"m1","timestamp":"2026-09-10T03:11:06Z","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]},"usage":{"inputTokens":10,"outputTokens":5,"cacheReadTokens":0,"cacheWriteTokens":0,"costUsd":0.001}}"#,
            ],
        );

        let messages = parse_commandcode_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, "changed-model");
    }

    #[test]
    fn test_commandcode_config_model_used_when_entry_has_no_model() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "sess-config",
            Some("org/Config-Model-Free"),
            &[
                r#"{"type":"session","version":3,"id":"sess-config","timestamp":"2026-09-10T03:10:55Z"}"#,
                r#"{"type":"message","id":"m1","parentId":null,"timestamp":"2026-09-10T03:10:56Z","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
                r#"{"type":"message","id":"m2","parentId":"m1","timestamp":"2026-09-10T03:11:06Z","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]},"usage":{"inputTokens":10,"outputTokens":5,"cacheReadTokens":0,"cacheWriteTokens":0,"costUsd":0.0}}"#,
            ],
        );

        let messages = parse_commandcode_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, "Config-Model");
    }

    #[test]
    fn test_commandcode_session_id_from_header_beats_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "path-id",
            None,
            &[
                r#"{"type":"session","version":3,"id":"header-id","timestamp":"2026-09-10T03:10:55Z"}"#,
                r#"{"type":"message","id":"m1","parentId":null,"timestamp":"2026-09-10T03:10:56Z","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
                r#"{"type":"message","id":"m2","parentId":"m1","timestamp":"2026-09-10T03:11:06Z","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]},"usage":{"inputTokens":10,"outputTokens":5,"cacheReadTokens":0,"cacheWriteTokens":0,"costUsd":0.001},"model":"org/m"}"#,
            ],
        );

        let messages = parse_commandcode_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].session_id, "header-id");
        assert_eq!(messages[0].dedup_key.as_deref(), Some("header-id:0"));
    }

    #[test]
    fn test_commandcode_session_id_from_path_when_no_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "path-fallback",
            None,
            &[
                r#"{"type":"message","id":"m1","parentId":null,"timestamp":"2026-09-10T03:10:56Z","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
                r#"{"type":"message","id":"m2","parentId":"m1","timestamp":"2026-09-10T03:11:06Z","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]},"usage":{"inputTokens":10,"outputTokens":5,"cacheReadTokens":0,"cacheWriteTokens":0,"costUsd":0.001},"model":"org/m"}"#,
            ],
        );

        let messages = parse_commandcode_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].session_id, "path-fallback");
    }

    #[test]
    fn test_commandcode_flat_legacy_shape_still_parses() {
        let dir = tempfile::tempdir().unwrap();
        let user_content = serde_json::json!([{"type": "text", "text": "hello there world"}]);
        let expected = estimate_tokens(content_chars(&user_content));
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "sess-legacy",
            Some("org/Legacy-Model"),
            &[
                r#"{"role":"user","content":[{"type":"text","text":"hello there world"}],"timestamp":"2026-09-10T03:10:56Z","sessionId":"legacy-id"}"#,
                r#"{"role":"assistant","content":[{"type":"text","text":"hi there"}],"timestamp":"2026-09-10T03:11:06Z","sessionId":"legacy-id"}"#,
            ],
        );

        let messages = parse_commandcode_file(&path);
        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        assert_eq!(msg.session_id, "legacy-id");
        assert_eq!(msg.model_id, "Legacy-Model");
        assert_eq!(msg.tokens.input, expected);
        assert_eq!(msg.cost_source, CostSource::Unknown);
    }

    #[test]
    fn test_commandcode_skips_checkpoint_files() {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir
            .path()
            .join(".commandcode")
            .join("projects")
            .join("users-alice-repo");
        std::fs::create_dir_all(&projects).unwrap();
        let path = projects.join("sess-1.checkpoints.jsonl");
        std::fs::write(
            &path,
            r#"{"id":"x","messageId":"x","turnNumber":1,"createdAt":"2026-09-10T03:11:03Z","prompt":"hi"}"#,
        )
        .unwrap();

        assert!(parse_commandcode_file(&path).is_empty());
    }

    #[test]
    fn test_commandcode_skips_contentless_assistant_turns() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            "users-alice-repo",
            "sess-empty",
            None,
            &[
                r#"{"type":"session","version":3,"id":"sess-empty","timestamp":"2026-09-10T03:10:55Z"}"#,
                r#"{"type":"message","id":"m1","parentId":null,"timestamp":"2026-09-10T03:10:56Z","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
                r#"{"type":"message","id":"m2","parentId":"m1","timestamp":"2026-09-10T03:11:06Z","message":{"role":"assistant","content":[]},"usage":{"inputTokens":0,"outputTokens":0,"cacheReadTokens":0,"cacheWriteTokens":0,"costUsd":0.0},"model":"org/m"}"#,
            ],
        );

        assert!(parse_commandcode_file(&path).is_empty());
    }
}
