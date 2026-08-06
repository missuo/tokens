//! gajae-code (`gjc`) session parser
//!
//! Parses JSONL session files from `~/.gjc/agent/sessions/<project-slug>/*.jsonl`
//! (and depth-2 per-pass sub-agent children `<slug>/<session>/N-*.jsonl`).
//!
//! Each line is tagged by `type`:
//! - `session` — header carrying `id` (session id) and `cwd` (workspace). No
//!   message is emitted for it.
//! - `service_tier_change` — skipped.
//! - `message` — emits ONLY assistant messages. The assistant `message` object
//!   carries `model`/`provider`/`api`, a unix-ms `timestamp`, and a `usage`
//!   object that includes an authoritative `usage.cost` (USD) breakdown.
//!
//! Cost policy (A1): the embedded `usage.cost.total` (USD) is reused verbatim
//! when present, finite, and non-negative. Otherwise cost is left at `0.0` so
//! the lib.rs dispatch Hermes guard can reprice from tokens.
//!
//! Dedup (codebuff-style): a stable `dedup_key` of `<session id>:<message id>`
//! is preferred; when ids are absent a deterministic fallback derived from the
//! session, timestamp, model and token breakdown keeps structurally identical
//! replays (depth-1 vs depth-2 files) collapsed to one message.

use super::utils::file_modified_timestamp_ms;
use super::{normalize_workspace_key, workspace_label_from_key, CostSource, UnifiedMessage};
use crate::provider_identity::inferred_provider_from_model;
use crate::TokenBreakdown;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// A single JSONL entry. The `session` header reuses `id`/`timestamp`/`cwd`;
/// `message` entries carry the assistant payload under `message`.
#[derive(Debug, Deserialize)]
struct GjcEntry {
    #[serde(rename = "type")]
    entry_type: String,
    id: Option<String>,
    /// Entry-level ISO-8601 timestamp (session header and message fallback).
    timestamp: Option<String>,
    /// Session header working directory.
    cwd: Option<String>,
    message: Option<GjcMessage>,
}

#[derive(Debug, Deserialize)]
struct GjcMessage {
    role: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    #[allow(dead_code)]
    api: Option<String>,
    /// Optional source client override (e.g. "9Router"). If absent, defaults to "gjc".
    source: Option<String>,
    /// Unix-ms timestamp (preferred for ordering/date).
    timestamp: Option<i64>,
    usage: Option<GjcUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GjcUsage {
    input: Option<i64>,
    output: Option<i64>,
    cache_read: Option<i64>,
    cache_write: Option<i64>,
    #[allow(dead_code)]
    total_tokens: Option<i64>,
    cost: Option<GjcCost>,
}

#[derive(Debug, Deserialize)]
struct GjcCost {
    /// Authoritative total cost in USD.
    total: Option<f64>,
}

/// Reuse the embedded `usage.cost.total` (USD) only when present, finite, and
/// non-negative. Otherwise return `0.0` so the dispatch pricing guard reprices.
fn embedded_cost(usage: &GjcUsage) -> (f64, CostSource) {
    match usage.cost.as_ref().and_then(|c| c.total) {
        Some(total) if total.is_finite() && total >= 0.0 => (total, CostSource::ProviderReported),
        _ => (0.0, CostSource::Unknown),
    }
}

/// Build a deterministic fallback dedup key for messages lacking a stable
/// upstream id, combining session, timestamp, model and token breakdown so
/// structurally identical replays collapse while distinct messages stay apart.
fn derive_dedup_key(
    session_id: &str,
    ts: i64,
    model: &str,
    provider: &str,
    tokens: &TokenBreakdown,
    line: &str,
) -> String {
    use sha2::{Digest, Sha256};

    let line_hash = Sha256::digest(line.as_bytes());

    format!(
        "gjc:{session_id}:{ts}:{model}:{provider}:{i}-{o}-{cr}-{cw}-{r}:{line_hash:x}",
        i = tokens.input,
        o = tokens.output,
        cr = tokens.cache_read,
        cw = tokens.cache_write,
        r = tokens.reasoning,
    )
}

/// Parse a gajae-code JSONL session file into UnifiedMessages.
///
/// Per-line parse: malformed/partial/legacy lines are skipped, never aborting
/// the file. The `session` header and `service_tier_change` lines emit nothing.
pub fn parse_gjc_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let fallback_timestamp = file_modified_timestamp_ms(path);

    let reader = BufReader::new(file);
    let mut messages: Vec<UnifiedMessage> = Vec::with_capacity(64);
    let mut buffer = Vec::with_capacity(4096);

    let mut session_id: Option<String> = None;
    let mut workspace_key: Option<String> = None;
    let mut workspace_label: Option<String> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        buffer.clear();
        buffer.extend_from_slice(trimmed.as_bytes());
        let entry = match simd_json::from_slice::<GjcEntry>(&mut buffer) {
            Ok(e) => e,
            Err(_) => continue,
        };

        match entry.entry_type.as_str() {
            "session" => {
                if let Some(id) = entry.id {
                    session_id = Some(id);
                }
                if let Some(key) = entry.cwd.as_deref().and_then(normalize_workspace_key) {
                    workspace_label = workspace_label_from_key(&key);
                    workspace_key = Some(key);
                }
                continue;
            }
            "message" => {}
            // service_tier_change and any other entry types: skip.
            _ => continue,
        }

        let message = match entry.message {
            Some(m) => m,
            None => continue,
        };

        if message.role.as_deref() != Some("assistant") {
            continue;
        }

        let usage = match message.usage {
            Some(u) => u,
            None => continue,
        };

        let model = match message.model {
            Some(m) => m,
            None => continue,
        };

        // A missing provider field is recoverable: infer it from the model name
        // (and fall back to "gjc") rather than dropping a message that carries
        // valid tokens.
        let provider = match message.provider {
            Some(p) if !p.is_empty() => p,
            _ => inferred_provider_from_model(&model)
                .unwrap_or("gjc")
                .to_string(),
        };

        // Prefer unix-ms message timestamp; fall back to entry ISO timestamp,
        // then the file mtime.
        let entry_timestamp = entry
            .timestamp
            .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
            .map(|dt| dt.timestamp_millis());
        let used_fallback_timestamp = message.timestamp.is_none() && entry_timestamp.is_none();
        let timestamp = message
            .timestamp
            .or(entry_timestamp)
            .unwrap_or(fallback_timestamp);

        let tokens = TokenBreakdown {
            input: usage.input.unwrap_or(0).max(0),
            output: usage.output.unwrap_or(0).max(0),
            cache_read: usage.cache_read.unwrap_or(0).max(0),
            cache_write: usage.cache_write.unwrap_or(0).max(0),
            reasoning: 0,
        };

        let (cost, cost_source) = embedded_cost(&usage);

        // No `{"type":"session",...}` header in this file: fall back to the file
        // name rather than a shared `"unknown"`, so two independent header-less
        // files do not collide on the same session in the cross-file dedup set.
        //
        // Caveat: a header-less depth-2 replay keys off its own (per-pass) file
        // stem, so it will NOT collapse against a header-less depth-1 parent the
        // way a shared session id would. The documented depth-1/depth-2
        // replay-collapse guarantee (see module doc above and lib.rs dispatch)
        // therefore holds for HEADERED files only — the realistic case, since
        // real gjc sessions always carry a `{"type":"session"}` header.
        let session = session_id.clone().unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| "unknown".to_string())
        });
        let dedup_key = match entry.id.filter(|s| !s.is_empty()) {
            Some(msg_id) => format!("{session}:{msg_id}"),
            None => derive_dedup_key(&session, timestamp, &model, &provider, &tokens, trimmed),
        };

        let client = message
            .source
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("gjc");
        let mut unified = UnifiedMessage::new_with_dedup(
            client,
            model,
            provider,
            session,
            timestamp,
            tokens,
            cost,
            Some(dedup_key),
        );
        if cost_source == CostSource::ProviderReported {
            unified.mark_provider_reported_cost();
        }
        if used_fallback_timestamp {
            unified.set_timestamp_provenance(crate::TimestampProvenance::Fallback);
        }
        unified.set_workspace(workspace_key.clone(), workspace_label.clone());
        messages.push(unified);
    }

    messages
}
