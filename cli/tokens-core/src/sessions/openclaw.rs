//! OpenClaw session parser
//!
//! Parses OpenClaw transcript JSONL files from agent directories.
//! Supports legacy sessions.json index parsing for compatibility.

use super::utils::read_file_or_none;
use super::UnifiedMessage;
use crate::TokenBreakdown;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct SessionIndex {
    #[serde(flatten)]
    sessions: HashMap<String, SessionEntry>,
}

#[derive(Debug, Deserialize)]
struct SessionEntry {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "sessionFile")]
    session_file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawEntry {
    #[serde(rename = "type")]
    entry_type: String,
    message: Option<OpenClawMessage>,
    #[serde(rename = "customType")]
    custom_type: Option<String>,
    data: Option<OpenClawModelData>,
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    provider: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawMessage {
    role: Option<String>,
    usage: Option<OpenClawUsage>,
    timestamp: Option<i64>,
    provider: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawModelData {
    provider: Option<String>,
    #[serde(rename = "modelId")]
    model_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawUsage {
    input: Option<i64>,
    output: Option<i64>,
    #[serde(rename = "cacheRead")]
    cache_read: Option<i64>,
    #[serde(rename = "cacheWrite")]
    cache_write: Option<i64>,
    #[serde(rename = "totalTokens")]
    #[allow(dead_code)]
    total_tokens: Option<i64>,
    cost: Option<OpenClawCost>,
}

#[derive(Debug, Deserialize)]
struct OpenClawCost {
    total: Option<f64>,
}

pub fn parse_openclaw_index(index_path: &Path) -> Vec<UnifiedMessage> {
    let Some(data) = read_file_or_none(index_path) else {
        return Vec::new();
    };

    let mut bytes = data;
    let index: SessionIndex = match simd_json::from_slice(&mut bytes) {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };

    let mut all_messages = Vec::new();
    let index_dir = index_path.parent().unwrap_or_else(|| Path::new("."));

    for (_key, entry) in index.sessions {
        let session_path = resolve_session_path(index_dir, &entry);
        if session_path.exists() {
            let messages = parse_openclaw_session(&session_path, &entry.session_id);
            all_messages.extend(messages);
        }
    }

    all_messages
}

pub fn parse_openclaw_transcript(transcript_path: &Path) -> Vec<UnifiedMessage> {
    let session_id = match transcript_path
        .file_name()
        .and_then(|n| {
            n.to_string_lossy()
                .split_once(".jsonl")
                .map(|(id, _)| id.to_string())
        })
        .filter(|id| !id.is_empty())
    {
        Some(id) => id,
        None => return Vec::new(),
    };

    parse_openclaw_session(transcript_path, &session_id)
}

fn resolve_session_path(index_dir: &Path, entry: &SessionEntry) -> PathBuf {
    match entry
        .session_file
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(session_file) => {
            let path = Path::new(session_file);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                index_dir.join(path)
            }
        }
        None => index_dir.join(format!("{}.jsonl", entry.session_id)),
    }
}

fn parse_openclaw_session(session_path: &Path, session_id: &str) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(session_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    // Get file modification time as fallback for missing timestamps
    let file_mtime_ms = std::fs::metadata(session_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let reader = BufReader::new(file);
    let mut messages = Vec::with_capacity(64);
    let mut current_model: Option<String> = None;
    let mut current_provider: Option<String> = None;
    let mut buffer = Vec::with_capacity(4096);

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
        let entry: OpenClawEntry = match simd_json::from_slice(&mut buffer) {
            Ok(e) => e,
            Err(_) => continue,
        };

        match entry.entry_type.as_str() {
            "model_change" => {
                if let Some(model) = entry.model_id {
                    current_model = Some(model);
                }
                if let Some(provider) = entry.provider {
                    current_provider = Some(provider);
                }
            }
            "custom" => {
                if entry.custom_type.as_deref() != Some("model-snapshot") {
                    continue;
                }

                if let Some(data) = entry.data {
                    if let Some(model) = data.model_id {
                        current_model = Some(model);
                    }
                    if let Some(provider) = data.provider {
                        current_provider = Some(provider);
                    }
                }
            }
            "message" => {
                if let Some(msg) = entry.message {
                    if msg.role.as_deref() != Some("assistant") {
                        continue;
                    }

                    let usage = match msg.usage {
                        Some(u) => u,
                        None => continue,
                    };

                    let model = msg
                        .model
                        .clone()
                        .filter(|m| !m.is_empty())
                        .or_else(|| current_model.clone().filter(|m| !m.is_empty()));
                    let provider = msg
                        .provider
                        .clone()
                        .filter(|p| !p.is_empty())
                        .or_else(|| current_provider.clone().filter(|p| !p.is_empty()))
                        .unwrap_or_else(|| "unknown".to_string());

                    let model = match model {
                        Some(model) => model,
                        None => continue,
                    };

                    current_model = Some(model.clone());
                    current_provider = Some(provider.clone());
                    let timestamp = msg.timestamp.unwrap_or(file_mtime_ms);
                    let cost = usage.cost.and_then(|c| c.total).unwrap_or(0.0);

                    messages.push(UnifiedMessage::new(
                        "openclaw",
                        model,
                        provider,
                        session_id.to_string(),
                        timestamp,
                        TokenBreakdown {
                            input: usage.input.unwrap_or(0).max(0),
                            output: usage.output.unwrap_or(0).max(0),
                            cache_read: usage.cache_read.unwrap_or(0).max(0),
                            cache_write: usage.cache_write.unwrap_or(0).max(0),
                            reasoning: 0,
                        },
                        cost.max(0.0),
                    ));
                }
            }
            _ => {}
        }
    }

    messages
}

