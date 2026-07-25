//! Pi (badlogic/pi-mono) session parser
//!
//! Parses JSONL files from `~/.pi/agent/sessions/<encoded-cwd>/*.jsonl` (and,
//! via the `pi` client's OMP scan root, `~/.omp/agent/sessions/...`). Current
//! OMP builds write a `title` metadata record before the `session` header in
//! newly-created session files; see [`PRE_SESSION_METADATA_TYPES`].

use super::utils::file_modified_timestamp_ms;
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::provider_identity::inferred_provider_from_model;
use crate::TokenBreakdown;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Pi session header (first line of JSONL)
#[derive(Debug, Deserialize)]
pub struct PiSessionHeader {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub id: String,
    #[allow(dead_code)]
    pub timestamp: Option<String>,
    #[allow(dead_code)]
    pub cwd: Option<String>,
}

/// Loose type-only probe for a JSONL line, used to identify pre-session
/// metadata records without requiring their full schema.
#[derive(Debug, Deserialize)]
struct PiEntryTypeProbe {
    #[serde(rename = "type")]
    entry_type: String,
}

/// Record types OMP may write before the `session` header (e.g. an
/// auto-generated-title record). The parser skips these while looking for
/// `session` rather than discarding the whole file. Any other unrecognized
/// type before `session` is still treated as a malformed file.
const PRE_SESSION_METADATA_TYPES: &[&str] = &["title"];

/// Pi session entry (subsequent lines of JSONL)
#[derive(Debug, Deserialize)]
pub struct PiSessionEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    #[allow(dead_code)]
    pub id: Option<String>,
    #[serde(rename = "parentId")]
    #[allow(dead_code)]
    pub parent_id: Option<String>,
    pub timestamp: Option<String>,
    pub message: Option<PiMessage>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PiMessage {
    pub role: Option<String>,
    pub usage: Option<PiUsage>,
    pub model: Option<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiUsage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
    #[allow(dead_code)]
    pub total_tokens: Option<i64>,
}

fn is_generated_id(value: &str) -> bool {
    (value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || (value.len() == 36
            && value.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            }))
}

fn strip_generated_id(value: &str) -> Option<&str> {
    for id_len in [36, 8] {
        if value.len() <= id_len || value.as_bytes()[value.len() - id_len - 1] != b'-' {
            continue;
        }
        let id = &value[value.len() - id_len..];
        if is_generated_id(id) {
            return Some(&value[..value.len() - id_len - 1]);
        }
    }
    None
}

fn pi_subagent_name(session_name: &str) -> Option<String> {
    let name = session_name.strip_prefix("subagent-")?;
    let without_id = strip_generated_id(name).or_else(|| {
        let (without_index, index) = name.rsplit_once('-')?;
        if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        strip_generated_id(without_index)
    })?;

    (!without_id.is_empty()).then(|| without_id.to_string())
}

/// Parse a Pi JSONL session file
pub fn parse_pi_file(path: &Path) -> Vec<UnifiedMessage> {
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
    let mut agent: Option<String> = None;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if session_id.is_none() {
            buffer.clear();
            buffer.extend_from_slice(trimmed.as_bytes());
            let entry_type = match simd_json::from_slice::<PiEntryTypeProbe>(&mut buffer) {
                Ok(probe) => probe.entry_type,
                Err(_) => return Vec::new(),
            };

            if entry_type != "session" {
                if PRE_SESSION_METADATA_TYPES.contains(&entry_type.as_str()) {
                    continue;
                }
                return Vec::new();
            }

            buffer.clear();
            buffer.extend_from_slice(trimmed.as_bytes());
            let header = match simd_json::from_slice::<PiSessionHeader>(&mut buffer) {
                Ok(h) => h,
                Err(_) => return Vec::new(),
            };

            session_id = Some(header.id);
            workspace_key = header.cwd.as_deref().and_then(normalize_workspace_key);
            workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
            continue;
        }

        buffer.clear();
        buffer.extend_from_slice(trimmed.as_bytes());
        let entry = match simd_json::from_slice::<PiSessionEntry>(&mut buffer) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.entry_type == "session_info" {
            agent = entry.name.as_deref().and_then(pi_subagent_name);
            continue;
        }

        if entry.entry_type != "message" {
            continue;
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

        // A missing/blank provider field is recoverable: infer it from the
        // model name (e.g. a Pi "gpt-5" message with no provider maps to
        // "openai"), falling back to "pi" only when inference can't
        // identify the model, rather than dropping a message that carries
        // valid tokens.
        let provider = match message.provider {
            Some(p) if !p.is_empty() => p,
            _ => inferred_provider_from_model(&model)
                .unwrap_or("pi")
                .to_string(),
        };

        let timestamp = entry
            .timestamp
            .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(fallback_timestamp);

        let mut unified = UnifiedMessage::new_with_agent(
            "pi",
            model,
            provider,
            session_id.clone().unwrap_or_else(|| "unknown".to_string()),
            timestamp,
            TokenBreakdown {
                input: usage.input.unwrap_or(0).max(0),
                output: usage.output.unwrap_or(0).max(0),
                cache_read: usage.cache_read.unwrap_or(0).max(0),
                cache_write: usage.cache_write.unwrap_or(0).max(0),
                reasoning: 0,
            },
            0.0,
            agent.clone(),
        );
        unified.set_workspace(workspace_key.clone(), workspace_label.clone());
        messages.push(unified);
    }

    messages
}

