//! Jcode session parser
//!
//! Parses compact JSON session snapshots from `~/.jcode/sessions/session_*.json`.
//! Jcode stores authoritative assistant token usage on messages under
//! `token_usage`; user/tool messages without usage are skipped.

use super::utils::{back_anchor_timestamp, file_modified_timestamp_ms, parse_timestamp_str};
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::{provider_identity, TokenBreakdown};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct JcodeSession {
    id: Option<String>,
    provider_key: Option<String>,
    model: Option<String>,
    working_dir: Option<String>,
    #[serde(default)]
    messages: Vec<JcodeMessage>,
}

/// Same envelope, but with `messages` left as raw JSON so a single malformed
/// element can be skipped instead of failing the whole snapshot deserialize.
#[derive(Debug, Deserialize)]
struct JcodeSessionEnvelope {
    id: Option<String>,
    provider_key: Option<String>,
    model: Option<String>,
    working_dir: Option<String>,
    #[serde(default)]
    messages: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct JcodeJournalEntry {
    meta: Option<JcodeJournalMeta>,
    // Raw values for the same reason as `JcodeSessionEnvelope::messages`: one
    // malformed sibling in a journal batch must not drop the line's valid
    // messages (or its meta).
    #[serde(default)]
    append_messages: Vec<serde_json::Value>,
}

/// Parse each raw message independently: a single wrong-typed field (e.g. a
/// string `token_usage`) must only drop that message, not its whole snapshot
/// or journal batch, mirroring how kimi/opencodereview skip bad lines.
fn lenient_jcode_messages(values: Vec<serde_json::Value>) -> Vec<JcodeMessage> {
    values
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect()
}

#[derive(Debug, Deserialize)]
struct JcodeJournalMeta {
    provider_key: Option<String>,
    model: Option<String>,
    working_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JcodeMessage {
    id: Option<String>,
    role: Option<String>,
    timestamp: Option<String>,
    token_usage: Option<JcodeTokenUsage>,
    tool_duration_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct JcodeTokenUsage {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
    cache_creation_input_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
}

fn provider_id(provider_key: Option<&str>) -> String {
    let provider = provider_key.unwrap_or("jcode").trim();
    let provider = if provider.is_empty() {
        "jcode".to_string()
    } else {
        provider.to_string()
    };
    provider_identity::canonical_provider(&provider).unwrap_or(provider)
}

fn model_id(model: Option<&str>) -> String {
    let model = model.unwrap_or("unknown").trim();
    if model.is_empty() {
        "unknown".to_string()
    } else {
        model.to_string()
    }
}

fn uses_split_cache_accounting(usage: &JcodeTokenUsage, input: i64, cache_read: i64) -> bool {
    // Jcode stores provider/model only at session scope, so either value may
    // describe a later route after a mid-session switch. Use message-local usage
    // shape instead. Anthropic-style reports preserve the cache-creation field
    // even when its value is zero; OpenAI/OpenRouter cached_tokens omit it and
    // report cache reads as a subset of input_tokens.
    usage.cache_creation_input_tokens.is_some() || cache_read > input
}

fn tokens_from_usage(usage: &JcodeTokenUsage) -> TokenBreakdown {
    let reported_input = usage.input_tokens.unwrap_or(0).max(0);
    let cache_read = usage.cache_read_input_tokens.unwrap_or(0).max(0);
    let cache_write = usage.cache_creation_input_tokens.unwrap_or(0).max(0);
    let input = if uses_split_cache_accounting(usage, reported_input, cache_read) {
        reported_input
    } else {
        // OpenAI-style APIs report cached tokens as a subset of input_tokens.
        // Tokens prices input and cache buckets independently, so remove that
        // overlap here rather than charging cached reads twice.
        reported_input.saturating_sub(cache_read.min(reported_input))
    };

    TokenBreakdown {
        input,
        output: usage.output_tokens.unwrap_or(0).max(0),
        cache_read,
        cache_write,
        reasoning: usage.reasoning_output_tokens.unwrap_or(0).max(0),
    }
}

#[derive(Debug, Clone)]
struct JcodeSessionContext {
    session_id: String,
    model: String,
    provider: String,
    workspace_key: Option<String>,
    workspace_label: Option<String>,
    pending_turn_start: bool,
    // User messages never carry `token_usage`, so they never enter
    // `index_by_dedup_key`/`known_dedup_keys` (which only track messages
    // that were emitted). This seen-set spans the snapshot and journal
    // passes so a journal replay of an already-seen user id can't re-arm
    // `pending_turn_start` and mint a spurious extra turn.
    seen_user_dedup_keys: std::collections::HashSet<String>,
}

impl JcodeSessionContext {
    fn new(session_id: String, session: &JcodeSession) -> Self {
        let workspace_key = session
            .working_dir
            .as_deref()
            .and_then(normalize_workspace_key);
        let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
        Self {
            session_id,
            model: model_id(session.model.as_deref()),
            provider: provider_id(session.provider_key.as_deref()),
            workspace_key,
            workspace_label,
            pending_turn_start: false,
            seen_user_dedup_keys: std::collections::HashSet::new(),
        }
    }

    fn apply_meta(&mut self, meta: JcodeJournalMeta) {
        if let Some(model) = meta.model.as_deref() {
            self.model = model_id(Some(model));
        }
        if let Some(provider_key) = meta.provider_key.as_deref() {
            self.provider = provider_id(Some(provider_key));
        }
        if let Some(working_dir) = meta.working_dir.as_deref() {
            self.workspace_key = normalize_workspace_key(working_dir);
            self.workspace_label = self
                .workspace_key
                .as_deref()
                .and_then(workspace_label_from_key);
        }
    }
}

/// Resolve the append-only journal sidecar path for a Jcode session snapshot.
///
/// Jcode persists recent changes in `session_*.journal.jsonl` until the next
/// checkpoint rewrites the snapshot. This is the single source of truth for the
/// snapshot→journal mapping; `message_cache.rs` reuses it so the parser and the
/// cache-fingerprint logic can never disagree about which sidecar to read.
pub(crate) fn jcode_journal_path(path: &Path) -> std::path::PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        let mut os = std::ffi::OsString::from(path.as_os_str());
        os.push(".journal.jsonl");
        return std::path::PathBuf::from(os);
    };
    let journal_name = file_name
        .strip_suffix(".json")
        .map(|stem| format!("{stem}.journal.jsonl"))
        .unwrap_or_else(|| format!("{file_name}.journal.jsonl"));
    path.with_file_name(journal_name)
}

fn parse_jcode_messages(
    messages: Vec<JcodeMessage>,
    context: &mut JcodeSessionContext,
    fallback_timestamp: i64,
    fallback_id_scope: &str,
    known_dedup_keys: Option<&std::collections::HashMap<String, usize>>,
) -> Vec<UnifiedMessage> {
    messages
        .into_iter()
        .enumerate()
        .filter_map(|(ordinal, message)| {
            let message_id = message
                .id
                // Real Jcode messages include stable IDs; this fallback keeps
                // malformed/custom files parseable without colliding across
                // snapshot and journal batches.
                .unwrap_or_else(|| format!("{fallback_id_scope}:{ordinal}"));
            let dedup_key = format!("jcode:{}:{message_id}", context.session_id);

            // A journal correction that only replaces an already-emitted message
            // is turn-neutral: the merge in `parse_jcode_file` overwrites its
            // is_turn_start with the snapshot entry's flag, so letting it advance
            // the turn-state machine would consume a pending turn-start that a
            // following brand-new journal message should have received (an
            // under-count of that session's turn_count). `known_dedup_keys` is
            // None for the snapshot pass, so snapshot parsing is unchanged.
            let is_replacement = known_dedup_keys.is_some_and(|keys| keys.contains_key(&dedup_key));

            if message.role.as_deref() == Some("user") {
                // Only a user id not already seen (snapshot or journal) arms a
                // new turn; a replay of the same id is turn-neutral.
                if context.seen_user_dedup_keys.insert(dedup_key.clone()) {
                    context.pending_turn_start = true;
                }
            }

            let usage = message.token_usage?;
            let tokens = tokens_from_usage(&usage);
            if tokens.total() <= 0 {
                return None;
            }
            // `explicit_timestamp` is the message's own recorded `timestamp`
            // field, as opposed to `fallback_timestamp` (a session/file-level
            // fallback used when it's absent or unparseable).
            let explicit_timestamp = message.timestamp.as_deref().and_then(parse_timestamp_str);
            let recorded_timestamp = explicit_timestamp.unwrap_or(fallback_timestamp);
            // The assistant message's `timestamp` is written once the message
            // (including `token_usage`) is finalized, i.e. the turn's *end*,
            // not its start. `tool_duration_ms` is that turn's elapsed time,
            // so `sessionize()`'s `[timestamp, timestamp + duration_ms]` span
            // would otherwise project forward past completion into phantom
            // idle time. Back-calculate the start anchor the same way #890
            // did for Copilot's `endTime`-only records.
            //
            // Only do this when `explicit_timestamp` is a real recorded end
            // timestamp: when it's absent, `recorded_timestamp` is the
            // session/file-level fallback, not this message's own completion
            // time, and subtracting `tool_duration_ms` from it would shift
            // the message into the wrong day rather than anchor it correctly.
            let duration_ms = message.tool_duration_ms.filter(|duration| *duration > 0);
            let timestamp = match (explicit_timestamp, duration_ms) {
                (Some(end), Some(duration)) => back_anchor_timestamp(end, duration),
                _ => recorded_timestamp,
            };
            let mut unified = UnifiedMessage::new_with_dedup(
                "jcode",
                context.model.clone(),
                context.provider.clone(),
                context.session_id.clone(),
                timestamp,
                tokens,
                0.0,
                Some(dedup_key),
            );
            unified.duration_ms = duration_ms;
            if !is_replacement
                && message.role.as_deref() == Some("assistant")
                && context.pending_turn_start
            {
                unified.is_turn_start = true;
                context.pending_turn_start = false;
            }
            unified.set_workspace(
                context.workspace_key.clone(),
                context.workspace_label.clone(),
            );
            Some(unified)
        })
        .collect()
}

pub fn parse_jcode_file(path: &Path) -> Vec<UnifiedMessage> {
    let mut data = match std::fs::read(path) {
        Ok(data) => data,
        Err(_) => return Vec::new(),
    };
    let envelope: JcodeSessionEnvelope = match simd_json::from_slice(&mut data) {
        Ok(envelope) => envelope,
        Err(_) => return Vec::new(),
    };
    let messages = lenient_jcode_messages(envelope.messages);
    let session = JcodeSession {
        id: envelope.id,
        provider_key: envelope.provider_key,
        model: envelope.model,
        working_dir: envelope.working_dir,
        messages,
    };

    let session_id = session.id.clone().unwrap_or_else(|| {
        path.file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string()
    });
    let fallback_timestamp = file_modified_timestamp_ms(path);
    let mut context = JcodeSessionContext::new(session_id, &session);
    let mut parsed = parse_jcode_messages(
        session.messages,
        &mut context,
        fallback_timestamp,
        "snapshot",
        None,
    );

    // Track where each dedup_key landed in `parsed`. The journal is written
    // after the snapshot, so a journal entry that repeats a snapshotted
    // message_id carries the *authoritative* (updated) token_usage. The
    // downstream dedup (`should_keep_deduped_message`) keeps the FIRST
    // occurrence per dedup_key, so emitting the snapshot then appending the
    // journal would silently drop the journal's correction. Instead, replace
    // the snapshot entry in place when the journal repeats its id — journal
    // wins, and the message_id still collapses to exactly one entry.
    // Downstream dedup is first-wins, so when the snapshot itself replays a
    // duplicate dedup_key we must map the key to its FIRST index — that's the
    // entry that survives dedup. Mapping to the last index would let a journal
    // update overwrite a row that is later discarded, preserving the stale
    // first snapshot row. `collect()` keeps the last insertion, so build the
    // map explicitly with `or_insert` to keep the first.
    let mut index_by_dedup_key: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (idx, message) in parsed.iter().enumerate() {
        if let Some(key) = message.dedup_key.clone() {
            index_by_dedup_key.entry(key).or_insert(idx);
        }
    }

    let journal_path = jcode_journal_path(path);
    if let Ok(file) = std::fs::File::open(&journal_path) {
        use std::io::{BufRead, BufReader};
        let journal_fallback_timestamp = file_modified_timestamp_ms(&journal_path);
        for (line_index, line) in BufReader::new(file).lines().enumerate() {
            let Ok(line) = line else {
                continue;
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(entry) = serde_json::from_str::<JcodeJournalEntry>(trimmed) else {
                continue;
            };
            if let Some(meta) = entry.meta {
                context.apply_meta(meta);
            }
            let journal_messages = parse_jcode_messages(
                lenient_jcode_messages(entry.append_messages),
                &mut context,
                journal_fallback_timestamp,
                &format!("journal:{line_index}"),
                Some(&index_by_dedup_key),
            );
            for mut message in journal_messages {
                match message
                    .dedup_key
                    .as_ref()
                    .and_then(|key| index_by_dedup_key.get(key).copied())
                {
                    Some(existing_index) => {
                        // Preserve the snapshot's turn-start flag: turn structure
                        // is derived from snapshot ordering, while the journal only
                        // carries the corrected token_usage for this message_id.
                        message.is_turn_start = parsed[existing_index].is_turn_start;
                        parsed[existing_index] = message;
                    }
                    None => {
                        if let Some(key) = message.dedup_key.clone() {
                            index_by_dedup_key.insert(key, parsed.len());
                        }
                        parsed.push(message);
                    }
                }
            }
        }
    }

    parsed
}

