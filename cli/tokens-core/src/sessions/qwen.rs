//! Qwen CLI session parser
//!
//! Parses JSONL files from ~/.qwen/projects/{projectPath}/chats/*.jsonl
//! Token data comes from assistant messages with usageMetadata field.

use super::utils::{file_modified_timestamp_ms, parse_timestamp_str};
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Qwen CLI JSONL line structure
#[derive(Debug, Deserialize)]
struct QwenLine {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    model: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,

    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct UsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<i64>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<i64>,
    #[serde(rename = "thoughtsTokenCount")]
    thoughts_token_count: Option<i64>,
    #[serde(rename = "cachedContentTokenCount")]
    cached_content_token_count: Option<i64>,
}

/// Default model name when not specified
const DEFAULT_MODEL: &str = "unknown";
const DEFAULT_PROVIDER: &str = "qwen";

/// Extract session ID with fallback logic:
/// 1. Use JSON session_id if present and non-empty
/// 2. Otherwise derive from path including project name to avoid collisions
///
/// Path format: ~/.qwen/projects/{project}/chats/{filename}.jsonl
pub fn extract_session_id_with_fallback(path: &Path, json_session_id: Option<&str>) -> String {
    // Priority 1: Use JSON sessionId if present and non-empty
    if let Some(id) = json_session_id {
        if !id.is_empty() {
            return id.to_string();
        }
    }

    // Priority 2: Derive from path with project context
    // Extract project name from path structure: .../projects/{project}/chats/{file}.jsonl
    let filename = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    // Try to extract project name from the path
    let project_name = path
        .parent() // .../chats
        .and_then(|p| p.parent()) // .../projects/{project}
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    // Combine project and filename for unique session ID
    format!("{}-{}", project_name, filename)
}

/// Parse a Qwen CLI JSONL file
pub fn parse_qwen_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let file_mtime = file_modified_timestamp_ms(path);
    let (workspace_key, workspace_label) = qwen_workspace_from_path(path);

    let reader = BufReader::new(file);
    let mut messages: Vec<UnifiedMessage> = Vec::new();
    // Qwen JSONL lines carry no per-message id, so anchor the dedup key to the
    // stable position of the emitted message within its session.
    let mut message_index: usize = 0;

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
        let qwen_line = match simd_json::from_slice::<QwenLine>(&mut bytes) {
            Ok(q) => q,
            Err(_) => continue,
        };

        // Only process assistant type messages with usageMetadata
        if qwen_line.msg_type.as_deref() != Some("assistant") {
            continue;
        }

        let usage = match qwen_line.usage_metadata {
            Some(u) => u,
            None => continue,
        };

        // Parse timestamp, fallback to file mtime
        let explicit_timestamp = qwen_line.timestamp.and_then(|ts| parse_timestamp_str(&ts));
        let timestamp_ms = explicit_timestamp.unwrap_or(file_mtime);

        // Extract token counts with defaults
        let input = usage.prompt_token_count.unwrap_or(0).max(0);
        let output = usage.candidates_token_count.unwrap_or(0).max(0);
        let reasoning = usage.thoughts_token_count.unwrap_or(0).max(0);
        let cache_read = usage.cached_content_token_count.unwrap_or(0).max(0);
        let cache_write = 0; // Qwen CLI doesn't report cache write tokens

        // Skip entries with zero tokens
        if input + output + cache_read + reasoning == 0 {
            continue;
        }

        // Use model from line or fallback to "unknown"
        let model = qwen_line.model.unwrap_or_else(|| DEFAULT_MODEL.to_string());

        // Resolve session ID: prefer JSON sessionId, fallback to path-derived
        let line_session_id =
            extract_session_id_with_fallback(path, qwen_line.session_id.as_deref());

        let dedup_key = Some(format!("qwen:{line_session_id}:{message_index}"));
        message_index += 1;

        let mut unified = UnifiedMessage::new_with_dedup(
            "qwen",
            model,
            DEFAULT_PROVIDER,
            line_session_id,
            timestamp_ms,
            TokenBreakdown {
                input,
                output,
                cache_read,
                cache_write,
                reasoning,
            },
            0.0, // Cost calculated later by pricing resolver
            dedup_key,
        );
        if explicit_timestamp.is_none() {
            unified.set_timestamp_provenance(crate::TimestampProvenance::Fallback);
        }
        unified.set_workspace(workspace_key.clone(), workspace_label.clone());
        messages.push(unified);
    }

    messages
}

fn qwen_workspace_from_path(path: &Path) -> (Option<String>, Option<String>) {
    let components: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();

    for window in components.windows(4).rev() {
        if window[0] == "projects" && !window[1].is_empty() && window[2] == "chats" {
            let key = normalize_workspace_key(&window[1]);
            let label = key.as_deref().and_then(workspace_label_from_key);
            return (key, label);
        }
    }

    (None, None)
}
