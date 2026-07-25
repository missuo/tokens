//! Command Code session parser
//!
//! Parses JSONL transcripts from `~/.commandcode/projects/<slug>/<session>.jsonl`.
//!
//! Unlike most sources, Command Code does NOT persist token usage locally: the
//! CLI computes per-request usage in memory and ships it to its backend
//! (`api.commandcode.ai`, surfaced in the web Usage dashboard). The on-disk
//! transcript only contains message text (one JSON object per line with
//! `role`/`content`/`timestamp`/`sessionId`), so token counts are ESTIMATED
//! from message text at ~4 characters per token, consistent with this crate's
//! other estimated sources (see Kiro).
//!
//! These estimates approximate tokens processed; they will not match Command
//! Code's server-reported usage, which reflects tool-output truncation and
//! auxiliary model runs (e.g. tool-desc, taste-1) absent from the transcript.
//!
//! **Input estimation is per-turn, not cumulative.**
//! Command Code stores no local token counts and re-sends prior context on each
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
//! available from the transcript. Do not silently change the estimation model
//! without a corresponding update to this doc-comment and the pinning test
//! `test_commandcode_input_is_per_turn_delta`.
//!
//! Output is estimated from the assistant message's own content. The model id
//! is not stored per message, so it is read from `~/.commandcode/config.json`
//! (the configured agent model), falling back to "unknown".

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
    role: Option<String>,
    content: Option<serde_json::Value>,
    timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
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
    let raw_model = model_from_config(path);
    // Recover the real provider from the configured gateway id (e.g.
    // `MiniMaxAI/MiniMax-M3-Free` -> `minimax`) so pricing resolves to that
    // provider's catalog. The client's own `command-code` provider is not a
    // pricing provider, so without this a MiniMax model would never reach a
    // `minimax/...` key. Falls back to `command-code` when nothing is inferred.
    let provider_id = raw_model
        .as_deref()
        .and_then(provider_hint_for_model)
        .unwrap_or(PROVIDER_ID);
    let model_id = raw_model
        .map(|model| canonicalize_model(&model))
        .unwrap_or_else(|| UNKNOWN_MODEL.to_string());
    let session_id_from_path = session_id_from_path(path);
    let workspace_key = workspace_key_from_path(path);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

    let mut messages = Vec::new();
    let mut session_id: Option<String> = None;
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

        if session_id.is_none() {
            if let Some(id) = entry.session_id.as_deref().filter(|id| !id.is_empty()) {
                session_id = Some(id.to_string());
            }
        }

        let chars = entry.content.as_ref().map(content_chars).unwrap_or(0);

        match entry.role.as_deref() {
            Some("assistant") => {
                let input = estimate_tokens(turn_input_chars);
                let output = estimate_tokens(chars);
                // This turn's input has been consumed; the next turn's input is
                // only the *new* context that follows this response. The
                // assistant's own output is not part of any input estimate.
                turn_input_chars = 0;

                if input + output == 0 {
                    pending_turn_start = false;
                    continue;
                }

                let resolved_session = session_id
                    .clone()
                    .unwrap_or_else(|| session_id_from_path.clone());
                let timestamp = entry
                    .timestamp
                    .as_deref()
                    .and_then(parse_rfc3339_ms)
                    .unwrap_or(fallback_timestamp);

                let mut message = UnifiedMessage::new_with_dedup(
                    CLIENT_ID,
                    model_id.clone(),
                    provider_id,
                    resolved_session.clone(),
                    timestamp,
                    TokenBreakdown {
                        input,
                        output,
                        cache_read: 0,
                        cache_write: 0,
                        reasoning: 0,
                    },
                    0.0,
                    Some(format!("{}:{}", resolved_session, assistant_index)),
                );
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

