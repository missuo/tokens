//! Claude Code session parser
//!
//! Parses JSONL files from ~/.claude/projects/

use super::utils::{
    extract_i64, extract_string, file_modified_timestamp_ms, parse_timestamp_value,
    read_file_or_none,
};
use super::{
    normalize_agent_name, normalize_workspace_key, workspace_label_from_key, UnifiedMessage,
};
use crate::{pricing, provider_identity, TokenBreakdown};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

type ParentSubagentTypeCache = HashMap<PathBuf, HashMap<String, String>>;

/// Claude Code entry structure (from JSONL files)
#[derive(Debug, Deserialize)]
pub struct ClaudeEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub timestamp: Option<String>,
    pub message: Option<ClaudeMessage>,
    /// Request ID for deduplication (used with message.id)
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    /// True for subagent (sidechain) transcript lines
    #[serde(rename = "isSidechain", default)]
    pub is_sidechain: bool,
    /// Stable subagent identifier within its parent session
    #[serde(rename = "agentId")]
    pub agent_id: Option<String>,
    /// Parent session UUID (present on every sidechain line)
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    /// Optional billing or routing provider emitted by wrappers around Claude Code.
    #[serde(rename = "providerId", alias = "provider_id", alias = "provider")]
    pub provider_id: Option<String>,
    /// Working directory recorded on session entries. Used only for display labels;
    /// project identity remains the path-derived workspace key.
    pub cwd: Option<String>,
}

/// Meta sidecar written next to nested-layout sidechain transcripts.
/// e.g. `agent-abc123.meta.json` alongside `agent-abc123.jsonl`
#[derive(Debug, Deserialize)]
struct AgentMetaFile {
    #[serde(rename = "agentType")]
    agent_type: Option<String>,
}

#[derive(Debug, Clone)]
struct CcMirrorVariantMetadata {
    name: String,
    provider_id: Option<String>,
}

impl CcMirrorVariantMetadata {
    fn client_id(&self) -> String {
        format!("cc-mirror/{}", sanitize_cc_mirror_segment(&self.name))
    }
}

#[derive(Debug, Deserialize)]
pub struct ClaudeMessage {
    pub model: Option<String>,
    pub usage: Option<ClaudeUsage>,
    /// Message ID for deduplication (used with requestId)
    pub id: Option<String>,
    /// Optional billing or routing provider emitted by wrappers around Claude Code.
    #[serde(rename = "providerId", alias = "provider_id", alias = "provider")]
    pub provider_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
}

/// Resolve the subagent display name for a sidechain transcript file.
///
/// Tier 1: Read the sibling `.meta.json` sidecar for the `agentType` field.
/// Tier 2: Scan the parent session JSONL for the tool_use that spawned this agent.
/// Tier 3: Fall back to a generic "claude-code-subagent" label.
fn resolve_subagent_name(
    path: &Path,
    parent_session_id: Option<&str>,
    entry_agent_id: Option<&str>,
    parent_cache: &mut ParentSubagentTypeCache,
) -> String {
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return normalize_agent_name("claude-code-subagent"),
    };

    // Tier 1: sibling meta.json (e.g. agent-abc123.meta.json next to agent-abc123.jsonl)
    let meta_path = path.with_file_name(format!("{}.meta.json", stem));
    if let Ok(text) = std::fs::read_to_string(&meta_path) {
        if let Ok(meta) = serde_json::from_str::<AgentMetaFile>(&text) {
            if let Some(ref agent_type) = meta.agent_type {
                if !agent_type.trim().is_empty() {
                    return normalize_agent_name(agent_type);
                }
            }
        }
    }

    // Tier 2: parent session tool_use inference
    let lookup_agent_id = entry_agent_id
        .filter(|agent_id| !agent_id.trim().is_empty())
        .map(|agent_id| agent_id.to_string())
        .or_else(|| sidechain_agent_id_from_stem(stem));
    if let (Some(parent_id), Some(agent_id)) = (parent_session_id, lookup_agent_id.as_deref()) {
        if let Some(parent_path) = find_parent_session_path(path, parent_id) {
            if let Some(subagent_type) =
                lookup_subagent_type_in_parent(&parent_path, agent_id, parent_cache)
            {
                return normalize_agent_name(&subagent_type);
            }
        }
    }

    // Tier 3: generic fallback (still visible in the Agents tab)
    normalize_agent_name("claude-code-subagent")
}

/// True for nested-layout workflow orchestration journals
/// (`.../<session>/subagents/**/journal.jsonl`).
///
/// Claude Code writes a `journal.jsonl` alongside `agent-*.jsonl` transcripts to
/// record subagent workflow orchestration (spawn/verdict/result events). It shares
/// the `.jsonl` extension and lives under the recursively-scanned project dir, so
/// the dir-walk discovers it — but it is metadata, NOT a message transcript, and
/// must never be ingested as usage. Its lines carry `type: "started"`/`"result"`
/// (not `user`/`assistant`) so they currently parse to zero usage, but we drop it
/// explicitly so a future journal schema can't silently leak token-like fields.
fn is_workflow_journal(path: &Path) -> bool {
    if path.file_name().and_then(|n| n.to_str()) != Some("journal.jsonl") {
        return false;
    }
    path.ancestors()
        .any(|ancestor| ancestor.file_name().and_then(|n| n.to_str()) == Some("subagents"))
}

fn is_in_transcripts_dir(path: &Path) -> bool {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        == Some("transcripts")
}

/// Locate the parent main-session JSONL for a sidechain transcript.
///
/// Nested layout: `.../projects/<key>/<session>/subagents/agent-X.jsonl`
///   → parent at `.../projects/<key>/<session>.jsonl`
/// Deep nested layout (workflows): `.../projects/<key>/<session>/subagents/workflows/<wf>/agent-X.jsonl`
///   → parent at `.../projects/<key>/<session>.jsonl`
/// Flat layout: `.../projects/<key>/agent-X.jsonl`
///   → parent at `.../projects/<key>/<session-id>.jsonl`
fn parent_session_paths(sidechain_path: &Path, parent_session_id: &str) -> Vec<PathBuf> {
    let parent_filename = format!("{}.jsonl", parent_session_id);
    let mut candidates = Vec::with_capacity(2);

    // Nested layout: locate the `subagents` directory anywhere in the ancestry.
    // The session dir is its parent and the project dir its grandparent, so the
    // parent session file sits at `<project>/<session>.jsonl`. Anchoring on the
    // `subagents` marker (rather than a fixed depth) handles both the shallow
    // `subagents/agent-X.jsonl` and the deeper `subagents/workflows/<wf>/agent-X.jsonl`.
    for ancestor in sidechain_path.ancestors() {
        if ancestor.file_name().and_then(|n| n.to_str()) == Some("subagents") {
            if let Some(project_dir) = ancestor.parent().and_then(|d| d.parent()) {
                candidates.push(project_dir.join(&parent_filename));
            }
            break;
        }
    }

    // Flat layout, and the existing nested-layout fallback: parent dir is one
    // level up. Preserve this as the lower-priority candidate when nested.
    if let Some(parent_dir) = sidechain_path.parent() {
        let candidate = parent_dir.join(parent_filename);
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

fn find_parent_session_path(sidechain_path: &Path, parent_session_id: &str) -> Option<PathBuf> {
    parent_session_paths(sidechain_path, parent_session_id)
        .into_iter()
        .find(|path| path.exists())
}

/// How far the parent probe reads before giving up. A sidechain transcript's
/// first row is already a sidechain row in practice, so the probe stops almost
/// immediately; this cap only prevents a mislabeled or corrupt file that
/// matches the `agent-*` / `subagents/` layout but carries no sidechain row
/// from triggering a whole-file read on every warm cache validation.
const PARENT_PROBE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Resolve the parent transcript that can influence a sidechain's cached agent
/// attribution. The probe follows the parser until its first parseable
/// sidechain row, then returns every candidate in parser precedence order.
/// Missing candidates are retained so their later appearance invalidates the
/// cache.
pub(crate) fn parent_session_paths_for_cache(sidechain_path: &Path) -> Vec<PathBuf> {
    parent_session_paths_for_cache_bounded(sidechain_path, PARENT_PROBE_MAX_BYTES)
}

/// Cap-parameterized core of [`parent_session_paths_for_cache`]. The current
/// line is always read and parsed in full, so a marker on the first row is
/// found regardless of `max_probe_bytes`; the cap only bounds how many *later*
/// rows a marker-less file is scanned for before the probe gives up.
fn parent_session_paths_for_cache_bounded(
    sidechain_path: &Path,
    max_probe_bytes: u64,
) -> Vec<PathBuf> {
    if is_workflow_journal(sidechain_path) {
        return Vec::new();
    }
    let likely_nested = sidechain_path
        .ancestors()
        .any(|ancestor| ancestor.file_name().and_then(|name| name.to_str()) == Some("subagents"));
    let likely_flat = sidechain_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.starts_with("agent-"));
    if !likely_nested && !likely_flat {
        return Vec::new();
    }

    let Ok(file) = std::fs::File::open(sidechain_path) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(file);
    let mut consumed: u64 = 0;
    let mut line = String::new();
    loop {
        line.clear();
        let read = match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => break,
        };
        consumed = consumed.saturating_add(read as u64);
        if let Ok(entry) = serde_json::from_str::<ClaudeEntry>(line.trim_end()) {
            if entry.is_sidechain {
                if let Some(parent_session_id) = entry
                    .session_id
                    .as_deref()
                    .filter(|session_id| !session_id.trim().is_empty())
                {
                    return parent_session_paths(sidechain_path, parent_session_id);
                }
            }
        }
        if consumed >= max_probe_bytes {
            break;
        }
    }

    Vec::new()
}

/// Scan a parent session JSONL to recover `subagent_type` for a given `agent_id`.
///
/// The parent session contains:
/// - Assistant messages with `tool_use` blocks (`name: "Agent"`, `input.subagent_type`)
/// - User messages with `tool_result` blocks whose text contains `agentId: <hex>`
///
/// We join on `tool_use_id` to map `agentId → subagent_type`.
fn lookup_subagent_type_in_parent(
    parent_path: &Path,
    target_agent_id: &str,
    parent_cache: &mut ParentSubagentTypeCache,
) -> Option<String> {
    if !parent_cache.contains_key(parent_path) {
        parent_cache.insert(
            parent_path.to_path_buf(),
            build_parent_subagent_type_lookup(parent_path)?,
        );
    }

    parent_cache
        .get(parent_path)
        .and_then(|lookup| lookup.get(target_agent_id).cloned())
}

fn build_parent_subagent_type_lookup(parent_path: &Path) -> Option<HashMap<String, String>> {
    let file = std::fs::File::open(parent_path).ok()?;
    let reader = BufReader::new(file);

    // tool_use.id → subagent_type
    let mut tool_use_types: HashMap<String, String> = HashMap::new();
    // tool_use_id → agentId (from tool_result text)
    let mut agent_id_links: HashMap<String, String> = HashMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Quick pre-filter: skip lines that can't contain what we need
        let has_subagent_type = trimmed.contains("subagent_type");
        let has_agent_id_text = trimmed.contains("agentId:");
        if !has_subagent_type && !has_agent_id_text {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let content = match value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        {
            Some(arr) => arr,
            None => continue,
        };

        for block in content {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match block_type {
                "tool_use" if has_subagent_type => {
                    if let (Some(id), Some(subagent_type)) = (
                        block.get("id").and_then(|i| i.as_str()),
                        block
                            .get("input")
                            .and_then(|inp| inp.get("subagent_type"))
                            .and_then(|s| s.as_str()),
                    ) {
                        tool_use_types.insert(id.to_string(), subagent_type.to_string());
                    }
                }
                "tool_result" if has_agent_id_text => {
                    let tool_use_id = match block.get("tool_use_id").and_then(|i| i.as_str()) {
                        Some(id) => id.to_string(),
                        None => continue,
                    };
                    // Walk content blocks looking for "agentId: <hex>" in text
                    let result_content = match block.get("content").and_then(|c| c.as_array()) {
                        Some(arr) => arr,
                        None => continue,
                    };
                    for cb in result_content {
                        if let Some(text) = cb.get("text").and_then(|t| t.as_str()) {
                            if let Some(aid) = extract_agent_id_from_text(text) {
                                agent_id_links.insert(tool_use_id.clone(), aid);
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let mut subagent_types = HashMap::new();
    for (tool_use_id, agent_id) in &agent_id_links {
        if let Some(subagent_type) = tool_use_types.get(tool_use_id) {
            subagent_types.insert(agent_id.clone(), subagent_type.clone());
        }
    }

    Some(subagent_types)
}

fn sidechain_agent_id_from_stem(stem: &str) -> Option<String> {
    let agent_stem = stem.strip_prefix("agent-")?;
    if !agent_stem.contains('-') {
        return Some(agent_stem.to_string());
    }

    let trailing_segment = agent_stem.rsplit('-').next()?;
    if trailing_segment.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(trailing_segment.to_string())
    } else {
        Some(agent_stem.to_string())
    }
}

/// Extract the `agentId` hex string from a tool_result text block.
/// Matches the pattern `agentId: <alphanumeric>` written by Claude Code's Agent tool.
fn extract_agent_id_from_text(text: &str) -> Option<String> {
    let marker = "agentId: ";
    let pos = text.find(marker)?;
    let start = pos + marker.len();
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    if end > 0 {
        Some(rest[..end].to_string())
    } else {
        None
    }
}

/// Parse a Claude Code JSONL file
pub fn parse_claude_file(path: &Path) -> Vec<UnifiedMessage> {
    let home_dir = dirs::home_dir();
    parse_claude_file_with_home(path, home_dir.as_deref())
}

pub fn parse_claude_file_with_home(path: &Path, home_dir: Option<&Path>) -> Vec<UnifiedMessage> {
    let mut parent_cache = ParentSubagentTypeCache::new();
    parse_claude_file_with_cache_and_home(path, &mut parent_cache, home_dir)
}

pub fn parse_claude_file_with_cache(
    path: &Path,
    parent_cache: &mut ParentSubagentTypeCache,
) -> Vec<UnifiedMessage> {
    let home_dir = dirs::home_dir();
    parse_claude_file_with_cache_and_home(path, parent_cache, home_dir.as_deref())
}

pub fn parse_claude_file_with_cache_and_home(
    path: &Path,
    parent_cache: &mut ParentSubagentTypeCache,
    home_dir: Option<&Path>,
) -> Vec<UnifiedMessage> {
    // Workflow orchestration journals are metadata, not transcripts — never ingest.
    if is_workflow_journal(path) {
        return Vec::new();
    }

    let (workspace_key, path_workspace_label) = claude_workspace_from_path(path);
    // Display label may be refined by later `cwd` values in the session JSONL.
    // Project identity (`workspace_key`) stays path-derived and never switches to cwd.
    let mut workspace_label = path_workspace_label;
    let cc_mirror_metadata = cc_mirror_variant_metadata_from_path(path, home_dir);
    let client_id = cc_mirror_metadata
        .as_ref()
        .map(CcMirrorVariantMetadata::client_id)
        .unwrap_or_else(|| "claude".to_string());
    let metadata_provider_hint = cc_mirror_metadata
        .as_ref()
        .and_then(|metadata| metadata.provider_id.as_deref());
    let mut session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Bare transcripts (files under ~/.claude/transcripts/ with no workspace/project
    // context) must not use char-based token estimation. These files may be written by
    // third-party tools (e.g. OpenCode) that log tool outputs without Claude API usage
    // metadata. Estimating tokens from their content would double-count usage already
    // tracked by the originating client's own parser. Explicit tool-result token counts
    // are still honored — only the char-based fallback estimate is suppressed.
    let is_bare_transcript =
        is_in_transcripts_dir(path) && cc_mirror_metadata.is_none() && workspace_key.is_none();

    let fallback_timestamp = file_modified_timestamp_ms(path);

    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        let json_messages = parse_claude_headless_json(
            path,
            &session_id,
            fallback_timestamp,
            workspace_key.clone(),
            workspace_label.clone(),
            &client_id,
            metadata_provider_hint,
        );
        if !json_messages.is_empty() {
            return json_messages;
        }
    }

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let reader = BufReader::new(file);
    let mut messages: Vec<UnifiedMessage> = Vec::with_capacity(64);
    let mut provider_confidences: Vec<u8> = Vec::with_capacity(64);
    // Maps dedup_key to the index in `messages` of the first occurrence.
    // CC's streaming API writes the same messageId:requestId multiple times as the
    // response streams in; later entries often carry more complete token counts.
    // We merge duplicates using per-field max to always keep the highest value seen
    // for each token type, ensuring we capture the most complete record.
    let mut processed_hashes: HashMap<String, usize> = HashMap::new();
    let mut headless_state = ClaudeHeadlessState::default();
    let mut buffer = Vec::with_capacity(4096);
    // Tracks whether the previous entry was a user message,
    // so the next assistant message can be marked as a turn start.
    let mut pending_turn_start = false;
    let mut pending_request_start_timestamp_ms: Option<i64> = None;
    let mut last_model: Option<String> = None;
    let mut last_provider_hint: Option<String> = None;
    // Sidechain detection state (resolved lazily on first parseable entry)
    let mut sidechain_agent: Option<String> = None;
    let mut sidechain_detected = false;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut handled = false;
        buffer.clear();
        buffer.extend_from_slice(trimmed.as_bytes());
        if let Ok(entry) = simd_json::from_slice::<ClaudeEntry>(&mut buffer) {
            if let Some(label) = entry.cwd.as_deref().and_then(cwd_label_from_raw) {
                workspace_label = Some(label);
            }

            // Detect sidechain on the first parseable entry (any type).
            // All lines in a subagent file carry isSidechain: true.
            if !sidechain_detected {
                sidechain_detected = true;
                if entry.is_sidechain {
                    // Use parent session ID to fix inflated session counts
                    if let Some(ref parent_id) = entry.session_id {
                        session_id = parent_id.clone();
                    }
                    sidechain_agent = Some(resolve_subagent_name(
                        path,
                        entry.session_id.as_deref(),
                        entry.agent_id.as_deref(),
                        parent_cache,
                    ));
                }
            }

            if entry.entry_type == "user" || entry.entry_type == "tool_result" {
                let tool_result_message = extract_claude_tool_result_message(
                    trimmed,
                    ClaudeToolResultContext {
                        entry: &entry,
                        last_model: last_model.as_deref(),
                        last_provider_hint: last_provider_hint.as_deref(),
                        client_id: &client_id,
                        default_provider_hint: metadata_provider_hint,
                        session_id: &session_id,
                        fallback_timestamp,
                        workspace_key: workspace_key.clone(),
                        workspace_label: workspace_label.clone(),
                        sidechain_agent: sidechain_agent.clone(),
                        allow_char_estimate: !is_bare_transcript,
                    },
                );

                if let Some(timestamp_ms) = parse_claude_entry_timestamp(entry.timestamp.as_deref())
                {
                    pending_request_start_timestamp_ms = Some(timestamp_ms);
                }

                if entry.entry_type == "user" && is_human_turn(trimmed) {
                    pending_turn_start = true;
                }

                if let Some(tool_message) = tool_result_message {
                    if let Some(ref dedup_key) = tool_message.dedup_key {
                        if let Some(&existing_idx) = processed_hashes.get(dedup_key) {
                            merge_claude_tool_result_duplicate(
                                &mut messages[existing_idx],
                                tool_message.tokens.input,
                                tool_message.timestamp,
                            );
                            update_workspace_labels_after_duplicate(
                                &mut messages,
                                existing_idx,
                                tool_message.workspace_label.as_deref(),
                            );
                            continue;
                        }
                        processed_hashes.insert(dedup_key.clone(), messages.len());
                    }
                    let provider_confidence =
                        stored_claude_provider_confidence(&tool_message.provider_id);
                    messages.push(tool_message);
                    provider_confidences.push(provider_confidence);
                }
                continue;
            }

            // Only process assistant messages with usage data
            if entry.entry_type == "assistant" {
                let message = match entry.message {
                    Some(m) => m,
                    None => continue,
                };

                if let Some(model) = message.model.as_deref() {
                    last_model = Some(model.to_string());
                    last_provider_hint = message
                        .provider_id
                        .as_deref()
                        .or(entry.provider_id.as_deref())
                        .map(str::to_string);
                }

                let usage = match message.usage {
                    Some(u) => u,
                    None => continue,
                };

                let duplicate_provider_choice = claude_provider_choice_from_parts(
                    message.model.as_deref(),
                    message
                        .provider_id
                        .as_deref()
                        .or(entry.provider_id.as_deref())
                        .or(metadata_provider_hint),
                );

                // Build dedup key for global deduplication (messageId:requestId composite).
                // For streaming responses, merge using per-field max to capture the most
                // complete token counts across all duplicate entries.
                let pending_hash = match (&message.id, &entry.request_id) {
                    (Some(msg_id), Some(req_id)) => {
                        let hash = format!("{}:{}", msg_id, req_id);
                        if let Some(&existing_idx) = processed_hashes.get(&hash) {
                            merge_claude_duplicate(
                                &mut messages[existing_idx],
                                &usage,
                                parse_claude_entry_timestamp(entry.timestamp.as_deref()),
                            );
                            if let Some(choice) = duplicate_provider_choice {
                                update_claude_provider_id(
                                    &mut messages[existing_idx].provider_id,
                                    &mut provider_confidences[existing_idx],
                                    choice,
                                );
                            }
                            update_workspace_labels_after_duplicate(
                                &mut messages,
                                existing_idx,
                                workspace_label.as_deref(),
                            );
                            continue;
                        }
                        Some(hash)
                    }
                    (Some(msg_id), None) => {
                        let hash = format!("message:{}", msg_id);
                        if let Some(&existing_idx) = processed_hashes.get(&hash) {
                            merge_claude_duplicate(
                                &mut messages[existing_idx],
                                &usage,
                                parse_claude_entry_timestamp(entry.timestamp.as_deref()),
                            );
                            if let Some(choice) = duplicate_provider_choice {
                                update_claude_provider_id(
                                    &mut messages[existing_idx].provider_id,
                                    &mut provider_confidences[existing_idx],
                                    choice,
                                );
                            }
                            update_workspace_labels_after_duplicate(
                                &mut messages,
                                existing_idx,
                                workspace_label.as_deref(),
                            );
                            continue;
                        }
                        Some(hash)
                    }
                    _ => None,
                };

                let raw_model = match message.model {
                    Some(m) => m,
                    None => continue,
                };
                let provider_choice = claude_provider_choice(
                    &raw_model,
                    message
                        .provider_id
                        .as_deref()
                        .or(entry.provider_id.as_deref())
                        .or(metadata_provider_hint),
                );
                let provider_confidence = provider_choice.confidence;
                let model = canonicalize_claude_model(&raw_model);

                let parsed_timestamp = parse_claude_entry_timestamp(entry.timestamp.as_deref());
                let timestamp = pending_request_start_timestamp_ms
                    .unwrap_or_else(|| parsed_timestamp.unwrap_or(fallback_timestamp));
                let duration_ms =
                    duration_between_ms(pending_request_start_timestamp_ms, parsed_timestamp);

                // Insert dedup index only after all checks pass, right before push
                let dedup_key = pending_hash.inspect(|hash| {
                    processed_hashes.insert(hash.clone(), messages.len());
                });

                let mut unified = UnifiedMessage::new_with_dedup(
                    client_id.clone(),
                    model,
                    provider_choice.id,
                    session_id.clone(),
                    timestamp,
                    TokenBreakdown {
                        input: usage.input_tokens.unwrap_or(0).max(0),
                        output: usage.output_tokens.unwrap_or(0).max(0),
                        cache_read: usage.cache_read_input_tokens.unwrap_or(0).max(0),
                        cache_write: usage.cache_creation_input_tokens.unwrap_or(0).max(0),
                        reasoning: 0,
                    },
                    0.0,
                    dedup_key,
                );
                unified.duration_ms = duration_ms;
                unified.agent = sidechain_agent.clone();
                unified.set_workspace(workspace_key.clone(), workspace_label.clone());
                // Mark the first assistant response after a user message as a turn start
                if pending_turn_start {
                    unified.is_turn_start = true;
                    pending_turn_start = false;
                }
                messages.push(unified);
                provider_confidences.push(provider_confidence);
                // Consume the pending request-start timestamp so a back-to-back
                // assistant message with no intervening user entry doesn't reuse
                // it and report an inflated duration. Streaming duplicates of
                // this same message have already been captured in the dedup map
                // above, so they merge via merge_claude_duplicate without needing
                // the global pending value again.
                pending_request_start_timestamp_ms = None;
                handled = true;
            }
        }

        if handled {
            continue;
        }

        if let Some(message) = process_claude_headless_line(
            trimmed,
            &session_id,
            &mut headless_state,
            fallback_timestamp,
            &client_id,
            metadata_provider_hint,
        ) {
            let mut message = message;
            message.set_workspace(workspace_key.clone(), workspace_label.clone());
            let provider_confidence = stored_claude_provider_confidence(&message.provider_id);
            messages.push(message);
            provider_confidences.push(provider_confidence);
        }
    }

    if let Some(message) = finalize_headless_state(
        &mut headless_state,
        &session_id,
        fallback_timestamp,
        &client_id,
        metadata_provider_hint,
    ) {
        let mut message = message;
        message.set_workspace(workspace_key, workspace_label);
        let provider_confidence = stored_claude_provider_confidence(&message.provider_id);
        messages.push(message);
        provider_confidences.push(provider_confidence);
    }

    messages
}

fn claude_workspace_from_path(path: &Path) -> (Option<String>, Option<String>) {
    let components: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();

    for window in components.windows(3) {
        if window[0] == ".claude" && window[1] == "projects" {
            let key = normalize_workspace_key(&window[2]);
            let label = key.as_deref().and_then(workspace_label_from_key);
            return (key, label);
        }
    }

    for window in components.windows(5) {
        if window[0] == ".cc-mirror" && window[2] == "config" && window[3] == "projects" {
            let key = normalize_workspace_key(&window[4]);
            let label = key.as_deref().and_then(workspace_label_from_key);
            return (key, label);
        }
    }

    for window in components.windows(2).rev() {
        if window[0] == "projects" {
            let key = normalize_workspace_key(&window[1]);
            let label = key.as_deref().and_then(workspace_label_from_key);
            return (key, label);
        }
    }

    (None, None)
}

/// Derive a short folder label from a Claude entry `cwd` without using it as identity.
fn cwd_label_from_raw(raw: &str) -> Option<String> {
    let normalized = normalize_workspace_key(raw)?;
    workspace_label_from_key(&normalized)
}

fn sanitize_cc_mirror_segment(raw: &str) -> String {
    let mut segment: String = raw
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    while segment.contains("--") {
        segment = segment.replace("--", "-");
    }
    let mut segment = segment
        .trim_matches(|ch| matches!(ch, '-' | '_' | '.'))
        .to_string();
    if segment.len() > 96 {
        segment.truncate(96);
        segment = segment
            .trim_matches(|ch| matches!(ch, '-' | '_' | '.'))
            .to_string();
    }
    if segment.is_empty() {
        "variant".to_string()
    } else {
        segment
    }
}

fn cc_mirror_provider_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("mirror") {
        return Some("anthropic".to_string());
    }
    provider_identity::canonical_provider(trimmed)
}

fn cc_mirror_variant_metadata_from_path(
    path: &Path,
    home_dir: Option<&Path>,
) -> Option<CcMirrorVariantMetadata> {
    let variant_dir = crate::cc_mirror::variant_dir_from_session_path(path, home_dir)?;
    let variant_name = variant_dir.file_name()?.to_string_lossy().to_string();
    let variant_path = crate::cc_mirror::variant_file_path(&variant_dir);
    let metadata = crate::cc_mirror::read_variant_file(&variant_path);

    let name = metadata
        .as_ref()
        .and_then(|metadata| metadata.name.as_deref())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&variant_name)
        .to_string();
    let provider_id = metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .provider_id
                .as_deref()
                .or(metadata.provider.as_deref())
        })
        .and_then(cc_mirror_provider_id);

    Some(CcMirrorVariantMetadata { name, provider_id })
}

fn parse_claude_entry_timestamp(timestamp: Option<&str>) -> Option<i64> {
    timestamp
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.timestamp_millis())
}

fn duration_between_ms(start_ms: Option<i64>, end_ms: Option<i64>) -> Option<i64> {
    let duration = end_ms?.saturating_sub(start_ms?);
    (duration > 0).then_some(duration)
}

fn merge_claude_duplicate(
    existing: &mut UnifiedMessage,
    usage: &ClaudeUsage,
    parsed_timestamp: Option<i64>,
) {
    // Per-field max merge: each token field is updated independently.
    let t = &mut existing.tokens;
    t.input = t.input.max(usage.input_tokens.unwrap_or(0).max(0));
    t.output = t.output.max(usage.output_tokens.unwrap_or(0).max(0));
    t.cache_read = t
        .cache_read
        .max(usage.cache_read_input_tokens.unwrap_or(0).max(0));
    t.cache_write = t
        .cache_write
        .max(usage.cache_creation_input_tokens.unwrap_or(0).max(0));

    if let Some(timestamp_ms) = parsed_timestamp {
        if timestamp_ms >= existing.timestamp {
            let new_duration = timestamp_ms.saturating_sub(existing.timestamp);
            if new_duration > 0 {
                // Duplicates can arrive out of order (e.g. late-processed
                // streaming chunks), so never let a later-processed duplicate
                // with an earlier completion timestamp shrink a duration
                // already established by another duplicate.
                existing.duration_ms = Some(existing.duration_ms.unwrap_or(0).max(new_duration));
            }
        }
    }
}

fn merge_claude_tool_result_duplicate(
    existing: &mut UnifiedMessage,
    input_tokens: i64,
    timestamp_ms: i64,
) {
    existing.tokens.input = existing.tokens.input.max(input_tokens.max(0));
    if timestamp_ms >= existing.timestamp {
        existing.set_timestamp(timestamp_ms);
    }
}

fn update_workspace_labels_after_duplicate(
    messages: &mut [UnifiedMessage],
    duplicate_idx: usize,
    candidate: Option<&str>,
) {
    let Some(label) = candidate.map(str::trim).filter(|label| !label.is_empty()) else {
        return;
    };
    let duplicate_timestamp = messages[duplicate_idx].timestamp;
    let workspace_key = messages[duplicate_idx].workspace_key.clone();

    for (index, message) in messages.iter_mut().enumerate() {
        if index == duplicate_idx
            || (workspace_key.is_some()
                && message.workspace_key == workspace_key
                && message.timestamp >= duplicate_timestamp)
        {
            message.workspace_label = Some(label.to_string());
        }
    }
}

struct ClaudeToolResultUsage {
    input_tokens: i64,
    dedup_key: Option<String>,
}

struct ClaudeToolResultContext<'a> {
    entry: &'a ClaudeEntry,
    last_model: Option<&'a str>,
    last_provider_hint: Option<&'a str>,
    client_id: &'a str,
    default_provider_hint: Option<&'a str>,
    session_id: &'a str,
    fallback_timestamp: i64,
    workspace_key: Option<String>,
    workspace_label: Option<String>,
    sidechain_agent: Option<String>,
    /// Whether char-based token estimation may be used as a fallback when no
    /// explicit tool-result token count is present. Bare transcripts (see
    /// `is_bare_transcript`) set this to `false` to avoid double-counting
    /// usage already tracked by the originating client's own parser, while
    /// still honoring any explicit tool-result token counts.
    allow_char_estimate: bool,
}

fn extract_claude_tool_result_message(
    line: &str,
    context: ClaudeToolResultContext<'_>,
) -> Option<UnifiedMessage> {
    let value: Value = serde_json::from_str(line).ok()?;
    let usage = extract_claude_tool_result_usage(&value, context.allow_char_estimate)?;

    let raw_model = extract_claude_model(&value)
        .or_else(|| {
            context
                .entry
                .message
                .as_ref()
                .and_then(|message| message.model.clone())
        })
        .or_else(|| context.last_model.map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let provider_hint = extract_claude_provider(&value)
        .or_else(|| {
            context
                .entry
                .message
                .as_ref()
                .and_then(|message| message.provider_id.clone())
        })
        .or_else(|| context.entry.provider_id.clone())
        .or_else(|| context.last_provider_hint.map(str::to_string))
        .or_else(|| context.default_provider_hint.map(str::to_string));

    let provider_choice = claude_provider_choice(&raw_model, provider_hint.as_deref());
    let model = canonicalize_claude_model(&raw_model);
    let timestamp = parse_claude_entry_timestamp(context.entry.timestamp.as_deref())
        .or_else(|| extract_claude_timestamp(&value))
        .unwrap_or(context.fallback_timestamp);

    let mut message = UnifiedMessage::new_with_dedup(
        context.client_id,
        model,
        provider_choice.id,
        context.session_id.to_string(),
        timestamp,
        TokenBreakdown {
            input: usage.input_tokens,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        usage.dedup_key.map(|key| {
            format!(
                "{}:tool_result:{}:{key}",
                context.client_id, context.session_id
            )
        }),
    );
    message.message_count = 0;
    message.agent = context.sidechain_agent;
    message.set_workspace(context.workspace_key, context.workspace_label);
    Some(message)
}

fn extract_claude_tool_result_usage(
    value: &Value,
    allow_char_estimate: bool,
) -> Option<ClaudeToolResultUsage> {
    let mut total_tokens = 0;
    let mut first_dedup_id: Option<String> = None;
    let mut seen_ids = HashSet::new();

    for tool_result in claude_tool_result_values(value) {
        let tool_result_id = extract_tool_result_id(tool_result);
        if let Some(id) = tool_result_id.as_ref() {
            if !seen_ids.insert(id.clone()) {
                continue;
            }
        }
        if first_dedup_id.is_none() {
            first_dedup_id = tool_result_id;
        }
        total_tokens +=
            extract_tool_result_input_tokens(tool_result, allow_char_estimate).unwrap_or(0);
    }

    if total_tokens <= 0 {
        return None;
    }

    Some(ClaudeToolResultUsage {
        input_tokens: total_tokens,
        dedup_key: first_dedup_id.map(|id| format!("tool_result:{id}")),
    })
}

fn claude_tool_result_values(value: &Value) -> Vec<&Value> {
    let mut results = Vec::new();

    if value
        .get("type")
        .and_then(|kind| kind.as_str())
        .is_some_and(|kind| kind == "tool_result")
    {
        results.push(value);
    }

    if let Some(tool_result) = value.get("tool_result") {
        results.push(tool_result);
    }

    if let Some(message_tool_result) = value
        .get("message")
        .and_then(|message| message.get("tool_result"))
    {
        results.push(message_tool_result);
    }

    if let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .or_else(|| value.get("content"))
    {
        collect_tool_result_blocks(content, &mut results);
    }

    results
}

fn collect_tool_result_blocks<'a>(value: &'a Value, results: &mut Vec<&'a Value>) {
    if let Some(blocks) = value.as_array() {
        for block in blocks {
            if block
                .get("type")
                .and_then(|kind| kind.as_str())
                .is_some_and(|kind| kind == "tool_result")
            {
                results.push(block);
            }
        }
    }
}

fn extract_tool_result_id(tool_result: &Value) -> Option<String> {
    extract_string(tool_result.get("tool_use_id"))
        .or_else(|| extract_string(tool_result.get("id")))
        .or_else(|| extract_string(tool_result.get("tool_result_id")))
}

fn extract_tool_result_input_tokens(tool_result: &Value, allow_char_estimate: bool) -> Option<i64> {
    explicit_tool_result_input_tokens(tool_result).or_else(|| {
        if !allow_char_estimate {
            return None;
        }
        let chars = tool_result_output_char_count(tool_result);
        (chars > 0).then(|| estimate_tokens_from_chars(chars))
    })
}

fn explicit_tool_result_input_tokens(tool_result: &Value) -> Option<i64> {
    for candidate in [
        tool_result.get("input_tokens"),
        tool_result.get("token_count"),
        tool_result.get("tokens"),
        tool_result
            .get("usage")
            .and_then(|usage| usage.get("input_tokens")),
        tool_result
            .get("tool_output")
            .and_then(|tool_output| tool_output.get("input_tokens")),
        tool_result
            .get("tool_output")
            .and_then(|tool_output| tool_output.get("token_count")),
        tool_result
            .get("tool_output")
            .and_then(|tool_output| tool_output.get("tokens")),
        tool_result
            .get("tool_output")
            .and_then(|tool_output| tool_output.get("usage"))
            .and_then(|usage| usage.get("input_tokens")),
    ] {
        if let Some(tokens) = extract_i64(candidate) {
            return Some(tokens.max(0));
        }
    }
    None
}

fn tool_result_output_char_count(tool_result: &Value) -> usize {
    let mut chars = 0;

    if let Some(output) = tool_result
        .get("tool_output")
        .and_then(|tool_output| tool_output.get("output"))
        .and_then(|output| output.as_str())
    {
        chars += output.chars().count();
    }

    match tool_result.get("content") {
        Some(content) if content.is_string() => {
            chars += content
                .as_str()
                .map(str::chars)
                .map(Iterator::count)
                .unwrap_or(0);
        }
        Some(content) => {
            chars += tool_result_content_output_chars(content);
        }
        None => {}
    }

    chars
}

fn tool_result_content_output_chars(content: &Value) -> usize {
    content
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .map(|block| {
                    block
                        .get("tool_output")
                        .and_then(|tool_output| tool_output.get("output"))
                        .and_then(|output| output.as_str())
                        .or_else(|| block.get("text").and_then(|text| text.as_str()))
                        .map(str::chars)
                        .map(Iterator::count)
                        .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0)
}

fn estimate_tokens_from_chars(chars: usize) -> i64 {
    // Claude Code tool outputs may not include token metadata. Match the
    // existing Kiro fallback of one token per four characters, rounded up.
    chars.div_ceil(4) as i64
}

fn canonicalize_claude_model(model: &str) -> String {
    pricing::aliases::resolve_alias(model)
        .unwrap_or(model)
        .to_string()
}

#[derive(Default)]
struct ClaudeHeadlessState {
    model: Option<String>,
    provider_id: Option<String>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    timestamp_ms: Option<i64>,
}

fn parse_claude_headless_json(
    path: &Path,
    session_id: &str,
    fallback_timestamp: i64,
    workspace_key: Option<String>,
    workspace_label: Option<String>,
    client_id: &str,
    default_provider_hint: Option<&str>,
) -> Vec<UnifiedMessage> {
    let Some(data) = read_file_or_none(path) else {
        return Vec::new();
    };

    let mut bytes = data;
    let value: Value = match simd_json::from_slice(&mut bytes) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut messages = Vec::with_capacity(1);
    if let Some(message) = extract_claude_headless_message(
        &value,
        session_id,
        fallback_timestamp,
        client_id,
        default_provider_hint,
    ) {
        let mut message = message;
        message.set_workspace(workspace_key, workspace_label);
        messages.push(message);
    }

    messages
}

fn process_claude_headless_line(
    line: &str,
    session_id: &str,
    state: &mut ClaudeHeadlessState,
    fallback_timestamp: i64,
    client_id: &str,
    default_provider_hint: Option<&str>,
) -> Option<UnifiedMessage> {
    let mut bytes = line.as_bytes().to_vec();
    let value: Value = simd_json::from_slice(&mut bytes).ok()?;

    let event_type = value.get("type").and_then(|val| val.as_str()).unwrap_or("");
    let mut completed_message: Option<UnifiedMessage> = None;

    match event_type {
        "message_start" => {
            completed_message = finalize_headless_state(
                state,
                session_id,
                fallback_timestamp,
                client_id,
                default_provider_hint,
            );

            state.model = extract_claude_model(&value);
            state.provider_id = extract_claude_provider(&value);
            state.timestamp_ms = extract_claude_timestamp(&value).or(state.timestamp_ms);
            if let Some(usage) = value
                .get("message")
                .and_then(|msg| msg.get("usage"))
                .or_else(|| value.get("usage"))
            {
                update_claude_usage(state, usage);
            }
        }
        "message_delta" => {
            if let Some(usage) = value
                .get("usage")
                .or_else(|| value.get("delta").and_then(|delta| delta.get("usage")))
            {
                update_claude_usage(state, usage);
            }
        }
        "message_stop" => {
            completed_message = finalize_headless_state(
                state,
                session_id,
                fallback_timestamp,
                client_id,
                default_provider_hint,
            );
        }
        _ => {
            if let Some(message) = extract_claude_headless_message(
                &value,
                session_id,
                fallback_timestamp,
                client_id,
                default_provider_hint,
            ) {
                completed_message = Some(message);
            }
        }
    }

    completed_message
}

fn extract_claude_headless_message(
    value: &Value,
    session_id: &str,
    fallback_timestamp: i64,
    client_id: &str,
    default_provider_hint: Option<&str>,
) -> Option<UnifiedMessage> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("message").and_then(|msg| msg.get("usage")))?;
    let raw_model = extract_claude_model(value)?;
    let provider_hint = extract_claude_provider(value);
    let provider_id = claude_provider_id(
        &raw_model,
        provider_hint.as_deref().or(default_provider_hint),
    );
    let model = canonicalize_claude_model(&raw_model);
    let timestamp = extract_claude_timestamp(value).unwrap_or(fallback_timestamp);

    Some(UnifiedMessage::new(
        client_id,
        model,
        provider_id,
        session_id.to_string(),
        timestamp,
        TokenBreakdown {
            input: extract_i64(usage.get("input_tokens")).unwrap_or(0).max(0),
            output: extract_i64(usage.get("output_tokens")).unwrap_or(0).max(0),
            cache_read: extract_i64(usage.get("cache_read_input_tokens"))
                .unwrap_or(0)
                .max(0),
            cache_write: extract_i64(usage.get("cache_creation_input_tokens"))
                .unwrap_or(0)
                .max(0),
            reasoning: 0,
        },
        0.0,
    ))
}

/// Internal Claude Code system/tool tags that should NOT be counted as human turns.
/// User prompts containing arbitrary HTML/XML (e.g. `<div>hello</div>`) are still
/// counted, only this narrow allowlist is excluded.
const CLAUDECODE_INTERNAL_USER_TAGS: &[&str] = &[
    "<local-command-stdout>",
    "<local-command-stderr>",
    "<command-name>",
    "<command-message>",
    "<system-reminder>",
    "<bash-input>",
    "<bash-stdout>",
    "<bash-stderr>",
];

/// Returns true if a `type: "user"` JSONL entry is genuine human input (not tool results or system messages).
fn is_human_turn(raw_line: &str) -> bool {
    if let Some(pos) = raw_line.find("\"content\":") {
        let after = &raw_line[pos + 10..];
        let after_trimmed = after.trim_start();
        if after_trimmed.starts_with('[') {
            return false;
        }
        if let Some(content_start) = after_trimmed.strip_prefix('"') {
            // Only filter out content that begins with a known internal tag.
            // Anything else (including `<div>`, `<table>`, etc. in genuine prompts)
            // is treated as a real human turn.
            for tag in CLAUDECODE_INTERNAL_USER_TAGS {
                if content_start.starts_with(tag) {
                    return false;
                }
            }
            return true;
        }
    }
    false
}

fn extract_claude_model(value: &Value) -> Option<String> {
    extract_string(value.get("model")).or_else(|| {
        value
            .get("message")
            .and_then(|msg| extract_string(msg.get("model")))
    })
}

fn extract_claude_provider(value: &Value) -> Option<String> {
    extract_string(value.get("providerId"))
        .or_else(|| extract_string(value.get("provider_id")))
        .or_else(|| extract_string(value.get("provider")))
        .or_else(|| {
            value.get("message").and_then(|msg| {
                extract_string(msg.get("providerId"))
                    .or_else(|| extract_string(msg.get("provider_id")))
                    .or_else(|| extract_string(msg.get("provider")))
            })
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClaudeProviderChoice {
    id: String,
    confidence: u8,
}

impl ClaudeProviderChoice {
    fn new(id: impl Into<String>, confidence: u8) -> Self {
        Self {
            id: id.into(),
            confidence,
        }
    }
}

const CLAUDE_PROVIDER_DEFAULT_CONFIDENCE: u8 = 1;
const CLAUDE_PROVIDER_INFERRED_CONFIDENCE: u8 = 2;
const CLAUDE_PROVIDER_EXPLICIT_CONFIDENCE: u8 = 3;

fn claude_provider_id(model: &str, provider_hint: Option<&str>) -> String {
    claude_provider_choice(model, provider_hint).id
}

fn claude_provider_choice_from_parts(
    model: Option<&str>,
    provider_hint: Option<&str>,
) -> Option<ClaudeProviderChoice> {
    match model {
        Some(model) => Some(claude_provider_choice(model, provider_hint)),
        None => claude_provider_choice_from_hint(None, provider_hint),
    }
}

fn claude_provider_choice(model: &str, provider_hint: Option<&str>) -> ClaudeProviderChoice {
    if let Some(choice) = claude_provider_choice_from_hint(Some(model), provider_hint) {
        return choice;
    }

    let inferred = provider_identity::inferred_provider_from_model(model);

    if let Some(provider) = provider_from_model_prefix(model) {
        return ClaudeProviderChoice::new(provider, CLAUDE_PROVIDER_EXPLICIT_CONFIDENCE);
    }

    if let Some(provider) = inferred {
        return ClaudeProviderChoice::new(provider, CLAUDE_PROVIDER_INFERRED_CONFIDENCE);
    }

    ClaudeProviderChoice::new("unknown", 0)
}

fn claude_provider_choice_from_hint(
    model: Option<&str>,
    provider_hint: Option<&str>,
) -> Option<ClaudeProviderChoice> {
    let hint = provider_hint.and_then(provider_identity::canonical_provider)?;

    if hint == "anthropic" {
        if let Some(inferred_provider) =
            model.and_then(provider_identity::inferred_provider_from_model)
        {
            if inferred_provider != "anthropic" {
                return Some(ClaudeProviderChoice::new(
                    inferred_provider,
                    CLAUDE_PROVIDER_INFERRED_CONFIDENCE,
                ));
            }
        }
        return Some(ClaudeProviderChoice::new(
            hint,
            CLAUDE_PROVIDER_DEFAULT_CONFIDENCE,
        ));
    }

    Some(ClaudeProviderChoice::new(
        hint,
        CLAUDE_PROVIDER_EXPLICIT_CONFIDENCE,
    ))
}

fn update_claude_provider_id(
    existing: &mut String,
    existing_confidence: &mut u8,
    candidate: ClaudeProviderChoice,
) {
    if candidate.confidence > *existing_confidence {
        *existing_confidence = candidate.confidence;
        *existing = candidate.id;
    }
}

fn stored_claude_provider_confidence(provider_id: &str) -> u8 {
    match provider_identity::canonical_provider(provider_id) {
        None => 0,
        Some(provider) if provider == "anthropic" => CLAUDE_PROVIDER_DEFAULT_CONFIDENCE,
        Some(_) => CLAUDE_PROVIDER_INFERRED_CONFIDENCE,
    }
}

fn provider_from_model_prefix(model: &str) -> Option<String> {
    if model.trim().contains('/') {
        provider_identity::canonical_provider(model)
    } else {
        None
    }
}

fn extract_claude_timestamp(value: &Value) -> Option<i64> {
    value
        .get("timestamp")
        .or_else(|| value.get("created_at"))
        .or_else(|| value.get("message").and_then(|msg| msg.get("created_at")))
        .and_then(parse_timestamp_value)
}

fn update_claude_usage(state: &mut ClaudeHeadlessState, usage: &Value) {
    if let Some(input) = extract_i64(usage.get("input_tokens")) {
        state.input = state.input.max(input);
    }
    if let Some(output) = extract_i64(usage.get("output_tokens")) {
        state.output = state.output.max(output);
    }
    if let Some(cache_read) = extract_i64(usage.get("cache_read_input_tokens")) {
        state.cache_read = state.cache_read.max(cache_read);
    }
    if let Some(cache_write) = extract_i64(usage.get("cache_creation_input_tokens")) {
        state.cache_write = state.cache_write.max(cache_write);
    }
}

fn finalize_headless_state(
    state: &mut ClaudeHeadlessState,
    session_id: &str,
    fallback_timestamp: i64,
    client_id: &str,
    default_provider_hint: Option<&str>,
) -> Option<UnifiedMessage> {
    let raw_model = state.model.clone()?;
    let provider_id = claude_provider_id(
        &raw_model,
        state.provider_id.as_deref().or(default_provider_hint),
    );
    let model = canonicalize_claude_model(&raw_model);
    let timestamp = state.timestamp_ms.unwrap_or(fallback_timestamp);
    if state.input == 0 && state.output == 0 && state.cache_read == 0 && state.cache_write == 0 {
        *state = ClaudeHeadlessState::default();
        return None;
    }

    let message = UnifiedMessage::new(
        client_id,
        model,
        provider_id,
        session_id.to_string(),
        timestamp,
        TokenBreakdown {
            input: state.input.max(0),
            output: state.output.max(0),
            cache_read: state.cache_read.max(0),
            cache_write: state.cache_write.max(0),
            reasoning: 0,
        },
        0.0,
    );

    *state = ClaudeHeadlessState::default();
    Some(message)
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_session(path: &Path, lines: &[&str]) {
        let mut file = std::fs::File::create(path).expect("create session");
        for line in lines {
            writeln!(file, "{line}").expect("write session line");
        }
    }

    fn assistant_line(id: &str, request_id: &str, cwd: Option<&str>) -> String {
        assistant_line_at(id, request_id, "2026-08-04T12:00:00.000Z", cwd)
    }

    fn assistant_line_at(id: &str, request_id: &str, timestamp: &str, cwd: Option<&str>) -> String {
        let cwd_json = match cwd {
            Some(value) => format!(r#","cwd":"{value}""#),
            None => String::new(),
        };
        format!(
            r#"{{"type":"assistant","timestamp":"{timestamp}","requestId":"{request_id}","message":{{"id":"{id}","model":"claude-sonnet-4-5","usage":{{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}{cwd_json}}}"#
        )
    }

    #[test]
    fn cwd_label_replaces_encoded_path_label_without_changing_key() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir
            .path()
            .join(".claude")
            .join("projects")
            .join("-Users-example-Documents-Codebase-tokens");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let session = project_dir.join("session.jsonl");
        write_session(
            &session,
            &[&assistant_line(
                "msg-1",
                "req-1",
                Some("/Users/example/Documents/Codebase/tokens"),
            )],
        );

        let messages = parse_claude_file(&session);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].workspace_key.as_deref(),
            Some("-Users-example-Documents-Codebase-tokens")
        );
        assert_eq!(messages[0].workspace_label.as_deref(), Some("tokens"));
    }

    #[test]
    fn cwd_label_preserves_hyphenated_worktree_folder_name() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir
            .path()
            .join(".claude")
            .join("projects")
            .join("-Users-example-Documents-Codebase-tokens--claude-worktrees-project-folder-name-display");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let session = project_dir.join("session.jsonl");
        let long_folder = "project-folder-name-display";
        let cwd = format!(
            "/Users/example/Documents/Codebase/tokens/.claude/worktrees/{long_folder}"
        );
        write_session(
            &session,
            &[&assistant_line("msg-1", "req-1", Some(&cwd))],
        );

        let messages = parse_claude_file(&session);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].workspace_key.as_deref(),
            Some(
                "-Users-example-Documents-Codebase-tokens--claude-worktrees-project-folder-name-display"
            )
        );
        assert_eq!(messages[0].workspace_label.as_deref(), Some(long_folder));
    }

    #[test]
    fn later_valid_cwd_updates_label_for_later_messages() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir
            .path()
            .join(".claude")
            .join("projects")
            .join("-Users-example-Documents-Codebase-tokens");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let session = project_dir.join("session.jsonl");
        write_session(
            &session,
            &[
                &assistant_line(
                    "msg-1",
                    "req-1",
                    Some("/Users/example/Documents/Codebase/tokens"),
                ),
                &assistant_line(
                    "msg-2",
                    "req-2",
                    Some(
                        "/Users/example/Documents/Codebase/tokens/.claude/worktrees/project-folder-name-display",
                    ),
                ),
            ],
        );

        let messages = parse_claude_file(&session);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].workspace_label.as_deref(), Some("tokens"));
        assert_eq!(
            messages[1].workspace_label.as_deref(),
            Some("project-folder-name-display")
        );
        assert_eq!(
            messages[0].workspace_key.as_deref(),
            messages[1].workspace_key.as_deref()
        );
        assert_eq!(
            messages[0].workspace_key.as_deref(),
            Some("-Users-example-Documents-Codebase-tokens")
        );
    }

    #[test]
    fn missing_or_unusable_cwd_falls_back_to_path_derived_label() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir
            .path()
            .join(".claude")
            .join("projects")
            .join("-Users-example-Documents-Codebase-tokens");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let session = project_dir.join("session.jsonl");
        write_session(
            &session,
            &[
                &assistant_line("msg-1", "req-1", None),
                &assistant_line("msg-2", "req-2", Some("///")),
                &assistant_line("msg-3", "req-3", Some("   ")),
            ],
        );

        let messages = parse_claude_file(&session);
        assert_eq!(messages.len(), 3);
        for message in &messages {
            assert_eq!(
                message.workspace_key.as_deref(),
                Some("-Users-example-Documents-Codebase-tokens")
            );
            assert_eq!(
                message.workspace_label.as_deref(),
                Some("-Users-example-Documents-Codebase-tokens")
            );
        }
    }

    #[test]
    fn assistant_duplicate_with_later_cwd_updates_existing_label() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir
            .path()
            .join(".claude")
            .join("projects")
            .join("-Users-example-Documents-Codebase-tokens");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let session = project_dir.join("session.jsonl");
        write_session(
            &session,
            &[
                &assistant_line("msg-1", "req-1", None),
                &assistant_line(
                    "msg-1",
                    "req-1",
                    Some(
                        "/Users/example/Documents/Codebase/tokens/.claude/worktrees/project-folder-name-display",
                    ),
                ),
            ],
        );

        let messages = parse_claude_file(&session);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].workspace_key.as_deref(),
            Some("-Users-example-Documents-Codebase-tokens")
        );
        assert_eq!(
            messages[0].workspace_label.as_deref(),
            Some("project-folder-name-display")
        );
    }

    #[test]
    fn tool_result_duplicate_with_later_cwd_updates_existing_label() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir
            .path()
            .join(".claude")
            .join("projects")
            .join("-Users-example-Documents-Codebase-tokens");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let session = project_dir.join("session.jsonl");
        let first = r#"{"type":"tool_result","timestamp":"2026-08-04T12:00:00.000Z","tool_use_id":"tool-1","input_tokens":10}"#;
        let second = r#"{"type":"tool_result","timestamp":"2026-08-04T12:00:01.000Z","tool_use_id":"tool-1","input_tokens":20,"cwd":"/Users/example/Documents/Codebase/tokens/.claude/worktrees/project-folder-name-display"}"#;
        write_session(&session, &[first, second]);

        let messages = parse_claude_file(&session);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.input, 20);
        assert_eq!(
            messages[0].workspace_key.as_deref(),
            Some("-Users-example-Documents-Codebase-tokens")
        );
        assert_eq!(
            messages[0].workspace_label.as_deref(),
            Some("project-folder-name-display")
        );
    }

    #[test]
    fn late_duplicate_cwd_wins_project_label_without_changing_usage() {
        let dir = tempdir().expect("tempdir");
        let project_key = "-Users-example-Documents-Codebase-tokens";
        let project_dir = dir
            .path()
            .join(".claude")
            .join("projects")
            .join(project_key);
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let session = project_dir.join("session.jsonl");
        write_session(
            &session,
            &[
                &assistant_line_at(
                    "msg-a",
                    "req-a",
                    "2026-08-04T12:00:00.000Z",
                    Some("/Users/example/Documents/Codebase/tokens"),
                ),
                &assistant_line_at(
                    "msg-b",
                    "req-b",
                    "2026-08-04T12:00:02.000Z",
                    None,
                ),
                &assistant_line_at(
                    "msg-a",
                    "req-a",
                    "2026-08-05T12:00:03.000Z",
                    Some(
                        "/Users/example/Documents/Codebase/tokens/.claude/worktrees/project-folder-name-display",
                    ),
                ),
            ],
        );

        let messages = parse_claude_file(&session);
        assert_eq!(messages.len(), 2);
        let days = crate::aggregator::aggregate_by_date(messages);
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].totals.tokens, 30);
        assert_eq!(days[0].totals.messages, 2);
        assert_eq!(days[0].projects.len(), 1);
        assert_eq!(
            days[0].projects[0].project_key.as_deref(),
            Some(project_key)
        );
        assert_eq!(
            days[0].projects[0].project_label,
            "project-folder-name-display"
        );
    }
}
