//! Codex CLI session parser
//!
//! Parses JSONL files from `~/.codex/sessions/` and its sibling
//! `~/.codex/archived_sessions/` (where Codex CLI moves older sessions). Both
//! directories share an identical JSONL schema, so a single parser handles
//! both; the scan-root wiring that discovers `archived_sessions` lives in
//! `crate::scanner`. Session identity for dedup is derived from in-file
//! `session_meta` content (not the file path), so a session that happens to
//! be present in both directories at once is still counted only once — see
//! `codex_token_count_dedup_key`.
//! Note: This parser has stateful logic to track model and delta calculations.

use super::utils::{
    extract_i64, extract_string, file_modified_timestamp_ms, parse_timestamp_value,
};
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::provider_identity::inferred_provider_from_model;
use crate::TokenBreakdown;
use serde::Deserialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

/// Codex entry structure (from JSONL files)
#[derive(Debug, Deserialize)]
pub struct CodexEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub timestamp: Option<String>,
    pub payload: Option<CodexPayload>,
}

#[derive(Debug, Deserialize)]
pub struct CodexPayload {
    pub id: Option<String>,
    pub forked_from_id: Option<String>,
    #[serde(rename = "type")]
    pub payload_type: Option<String>,
    pub model: Option<String>,
    pub model_name: Option<String>,
    pub model_info: Option<CodexModelInfo>,
    pub info: Option<CodexInfo>,
    pub turn_id: Option<String>,
    /// Unix timestamp (seconds) from `task_started` events. Legacy Codex turns
    /// may use UUID v4 ids, so this is their only causal ordering signal.
    /// Confirmed against codex-rs (`TurnStartedEvent::started_at`, serialized
    /// under `task_started`): documented as "Unix timestamp (in seconds)",
    /// `Option<i64>`. Deserialized leniently anyway: int/float values coerce
    /// to `i64`, and any other JSON type (string, object, ...) decodes as
    /// `None` rather than failing the whole `task_started` entry. A strict
    /// `Option<i64>` would make a wrong-typed value reject deserialization of
    /// the entire JSONL line, silently dropping the rest of that payload too.
    #[serde(default, deserialize_with = "deserialize_lenient_i64")]
    pub started_at: Option<i64>,
    pub source: Option<Value>,
    /// Thread origin from session_meta. `"user"` marks a human-initiated fork
    /// (e.g. a VS Code "fork conversation"), which replays parent history but
    /// never emits a `task_started` for the child's own turn.
    pub thread_source: Option<String>,
    /// Current working directory from session_meta.
    pub cwd: Option<String>,
    /// Provider identity from session_meta (e.g. "openai", "azure")
    pub model_provider: Option<String>,
    /// Agent name from session_meta
    pub agent_nickname: Option<String>,
    /// Free-text body of an `event_msg` `user_message` payload. Used to detect
    /// human turn boundaries: real human input is plain text, whereas
    /// system-injected context (`<environment_context>`, `<system-reminder>`,
    /// `<user_instructions>`, …) begins with `<`.
    pub message: Option<String>,
}

/// Lenient `Option<i64>` deserializer for `CodexPayload::started_at`. Coerces
/// JSON integers and floats to `i64`; any other type (string, bool, object,
/// array) or `null`/absent decodes as `None` instead of failing the entry.
fn deserialize_lenient_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_u64().map(|u| u as i64))
            .or_else(|| v.as_f64().map(|f| f as i64))
    }))
}

#[derive(Debug, Deserialize)]
pub struct CodexModelInfo {
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CodexInfo {
    pub model: Option<String>,
    pub model_name: Option<String>,
    pub last_token_usage: Option<CodexTokenUsage>,
    pub total_token_usage: Option<CodexTokenUsage>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CodexTokenUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub reasoning_output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CodexTotals {
    input: i64,
    output: i64,
    cached: i64,
    reasoning: i64,
}

impl CodexTotals {
    fn from_usage(usage: &CodexTokenUsage) -> Self {
        Self {
            input: usage.input_tokens.unwrap_or(0).max(0),
            output: usage.output_tokens.unwrap_or(0).max(0),
            cached: usage
                .cached_input_tokens
                .unwrap_or(0)
                .max(usage.cache_read_input_tokens.unwrap_or(0))
                .max(0),
            reasoning: usage.reasoning_output_tokens.unwrap_or(0).max(0),
        }
    }

    fn delta_from(self, previous: Self) -> Option<Self> {
        if self.input < previous.input
            || self.output < previous.output
            || self.cached < previous.cached
            || self.reasoning < previous.reasoning
        {
            return None;
        }

        Some(Self {
            input: self.input - previous.input,
            output: self.output - previous.output,
            cached: self.cached - previous.cached,
            reasoning: self.reasoning - previous.reasoning,
        })
    }

    fn saturating_add(self, other: Self) -> Self {
        Self {
            input: self.input.saturating_add(other.input),
            output: self.output.saturating_add(other.output),
            cached: self.cached.saturating_add(other.cached),
            reasoning: self.reasoning.saturating_add(other.reasoning),
        }
    }

    fn total(self) -> i64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cached)
            .saturating_add(self.reasoning)
    }

    fn is_within(self, baseline: Self) -> bool {
        self.input <= baseline.input
            && self.output <= baseline.output
            && self.cached <= baseline.cached
            && self.reasoning <= baseline.reasoning
    }

    fn looks_like_stale_regression(self, previous: Self, last: Self) -> bool {
        let previous_total = previous.total();
        let current_total = self.total();
        let last_total = last.total();

        if previous_total <= 0 || current_total <= 0 || last_total <= 0 {
            return false;
        }

        // Some Codex token_count snapshots arrive slightly out of order: the cumulative
        // total regresses by roughly one recent increment, then resumes from the true
        // higher watermark on the next row. Treat those as stale snapshots rather than
        // hard resets so we do not count `last_token_usage` twice.
        current_total.saturating_mul(100) >= previous_total.saturating_mul(98)
            || current_total.saturating_add(last_total.saturating_mul(2)) >= previous_total
    }

    fn into_tokens(self) -> TokenBreakdown {
        // Clamp cached to not exceed input to prevent inflated totals when
        // malformed data reports more cached tokens than input tokens.
        let clamped_cached = self.cached.min(self.input).max(0);
        TokenBreakdown {
            input: (self.input - clamped_cached).max(0),
            output: self.output.max(0),
            cache_read: clamped_cached,
            cache_write: 0,
            reasoning: self.reasoning.max(0),
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct CodexParseState {
    pub current_model: Option<String>,
    #[serde(default)]
    pub current_turn_start_ms: Option<i64>,
    #[serde(default)]
    pub last_accepted_token_timestamp_ms: Option<i64>,
    pub previous_totals: Option<CodexTotals>,
    pub session_is_headless: bool,
    pub session_id_from_meta: Option<String>,
    pub session_forked_from_id: Option<String>,
    pub forked_child_session_id: Option<String>,
    pub forked_child_replay_session_id: Option<String>,
    pub session_provider: Option<String>,
    pub session_agent: Option<String>,
    pub session_workspace_key: Option<String>,
    pub session_workspace_label: Option<String>,
    pub forked_child_waiting_for_turn_context: bool,
    pub forked_child_inherited_baseline: Option<CodexTotals>,
    pub forked_child_inherited_reported_total: Option<i64>,
    /// Set when a human `user_message` event is seen; consumed by the next
    /// token_count-derived message to mark it as a turn start. `#[serde(default)]`
    /// keeps a pending turn alive across incremental re-parses of appended chunks.
    #[serde(default)]
    pub pending_turn_start: bool,
    /// `turn_id`s announced by a `task_started` event while a forked child is
    /// still skipping its replayed parent history. The child's own turn is
    /// preceded by `task_started`; replayed parent turns are not. Used only to
    /// disambiguate a same-millisecond turn, where the UUID v7 timestamp ties
    /// and the random tail is meaningless — there, only a task-started turn_id
    /// ends the skip. Cleared when the skip ends or a new fork begins.
    /// `#[serde(default)]` keeps it across incremental re-parses.
    #[serde(default)]
    pub forked_child_task_started_turn_ids: std::collections::HashSet<String>,
    /// Set when the active forked child is a human-initiated (`thread_source:
    /// "user"`) fork. Such forks replay parent history but never emit a
    /// `task_started`, so the same-millisecond gate cannot lean on
    /// `task_started` to recognize the child's own turn — there the millisecond
    /// prefix tie is enough (a user fork's replayed parent turns carry the
    /// parent's millisecond prefix, not the child's). `#[serde(default)]` keeps
    /// it across incremental re-parses.
    #[serde(default)]
    pub forked_child_is_user_fork: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedCodexFile {
    pub messages: Vec<UnifiedMessage>,
    pub fallback_timestamp_indices: Vec<usize>,
    pub consumed_offset: u64,
    pub parse_succeeded: bool,
    /// True when model-less token_count rows were emitted without a later model.
    pub unresolved_model_events: bool,
    pub state: CodexParseState,
}

fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn codex_workspace_from_cwd(cwd: &str) -> (Option<String>, Option<String>) {
    let workspace_key = normalize_codex_workspace_key(cwd);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

    if workspace_label.is_none() {
        return (None, None);
    }

    (workspace_key, workspace_label)
}

fn normalize_codex_workspace_key(raw: &str) -> Option<String> {
    let normalized = normalize_workspace_key(raw)?;
    if normalized.chars().any(char::is_control) {
        return None;
    }

    if looks_like_explicit_workspace_path(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

fn looks_like_explicit_workspace_path(path: &str) -> bool {
    if path.starts_with("//") || path.starts_with('/') {
        return true;
    }

    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

fn parse_codex_reader<R: BufRead>(
    mut reader: R,
    session_id: &str,
    fallback_timestamp: i64,
    start_offset: u64,
    mut state: CodexParseState,
) -> ParsedCodexFile {
    let mut messages = Vec::with_capacity(64);
    let mut fallback_timestamp_indices = Vec::new();
    let mut buffer = Vec::with_capacity(4096);
    let mut line = String::with_capacity(4096);
    let mut consumed_offset = start_offset;
    let mut parse_succeeded = true;
    let mut pending_model_messages = Vec::new();
    let mut unresolved_model_events = false;

    loop {
        line.clear();
        let bytes_read = match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(bytes_read) => bytes_read,
            Err(_) => {
                parse_succeeded = false;
                break;
            }
        };
        consumed_offset += bytes_read as u64;

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut handled = false;
        buffer.clear();
        buffer.extend_from_slice(trimmed.as_bytes());
        if let Ok(entry) = simd_json::from_slice::<CodexEntry>(&mut buffer) {
            if let Some(payload) = entry.payload {
                let payload_model = extract_model(&payload);
                let is_token_count = entry.entry_type == "event_msg"
                    && payload.payload_type.as_deref() == Some("token_count");
                let info_model = if is_token_count {
                    payload.info.as_ref().and_then(extract_model_from_info)
                } else {
                    None
                };
                let event_model = payload_model.clone().or(info_model.clone());

                if state.forked_child_waiting_for_turn_context {
                    if entry.entry_type == "turn_context"
                        && forked_child_turn_starts_own_session(&state, payload.turn_id.as_deref())
                    {
                        state.forked_child_waiting_for_turn_context = false;
                        state.forked_child_replay_session_id = None;
                        state.forked_child_task_started_turn_ids.clear();
                        state.forked_child_is_user_fork = false;
                        if let Some(ref id) = state.forked_child_session_id {
                            state.session_id_from_meta = Some(id.clone());
                        }
                        state.current_model = payload_model.clone();
                        handled = true;
                    } else {
                        if entry.entry_type == "event_msg"
                            && payload.payload_type.as_deref() == Some("task_started")
                        {
                            // The child's own turn is introduced by a
                            // `task_started`; remember it only when its id or
                            // timestamp places it at/after the child session.
                            // Nested child logs can replay ancestor task_started
                            // events before the child's live turn.
                            if forked_child_task_starts_own_session(
                                &state,
                                payload.turn_id.as_deref(),
                                payload.started_at,
                            ) {
                                // Safety of this branch is coupled to
                                // `forked_child_task_starts_own_session`
                                // returning false when `turn_id` is `None`.
                                if let Some(turn_id) = payload.turn_id.as_deref() {
                                    state
                                        .forked_child_task_started_turn_ids
                                        .insert(turn_id.to_string());
                                }
                            }
                        }
                        if entry.entry_type == "session_meta" {
                            if let Some(ref id) = payload.id {
                                if state
                                    .forked_child_session_id
                                    .as_deref()
                                    .is_some_and(|child_id| child_id != id)
                                {
                                    // Newer Codex fork logs can embed the
                                    // parent session metadata before replaying
                                    // parent token_count history. Keep
                                    // skipping while that copied upstream
                                    // transcript is active.
                                    state.forked_child_replay_session_id = Some(id.clone());
                                }
                            }
                        }
                        if is_token_count {
                            if let Some(info) = payload.info.as_ref() {
                                remember_forked_child_inherited_baseline(&mut state, info);
                            }
                        }
                        continue;
                    }
                }

                if !pending_model_messages.is_empty()
                    && event_model.is_none()
                    && !is_token_count
                    && entry.entry_type != "session_meta"
                {
                    flush_pending_model_messages_as_unknown(
                        &mut pending_model_messages,
                        &mut messages,
                        &mut fallback_timestamp_indices,
                        &mut unresolved_model_events,
                    );
                }

                if entry.entry_type == "session_meta" {
                    if codex_source_is_exec(payload.source.as_ref()) {
                        state.session_is_headless = true;
                    }
                    if let Some(ref id) = payload.id {
                        state.session_id_from_meta = Some(id.clone());
                    }
                    let forked_from_id = payload
                        .forked_from_id
                        .as_deref()
                        .filter(|id| !id.is_empty())
                        .or_else(|| forked_from_id_from_source(payload.source.as_ref()));
                    if let Some(forked_from_id) = forked_from_id {
                        let repeated_active_child_meta = !state
                            .forked_child_waiting_for_turn_context
                            && payload.id.as_deref().is_some()
                            && state.forked_child_session_id.as_deref() == payload.id.as_deref();
                        state.session_forked_from_id = Some(forked_from_id.to_string());
                        state.forked_child_session_id = payload.id.clone();
                        if !repeated_active_child_meta {
                            state.forked_child_waiting_for_turn_context = true;
                            state.forked_child_replay_session_id = None;
                            state.forked_child_inherited_baseline = None;
                            state.forked_child_inherited_reported_total = None;
                            state.forked_child_task_started_turn_ids.clear();
                            state.forked_child_is_user_fork =
                                payload.thread_source.as_deref() == Some("user");
                        }
                    }
                    if let Some(ref provider) = payload.model_provider {
                        state.session_provider = Some(provider.clone());
                    }
                    if let Some(ref nickname) = payload.agent_nickname {
                        state.session_agent = Some(nickname.clone());
                    }
                    if let Some(ref cwd) = payload.cwd {
                        let (workspace_key, workspace_label) = codex_workspace_from_cwd(cwd);
                        state.session_workspace_key = workspace_key;
                        state.session_workspace_label = workspace_label;
                    }
                }
                // Extract model from turn_context
                if entry.entry_type == "turn_context" {
                    state.current_model = payload_model.clone();
                    let turn_start_ms = parse_codex_entry_timestamp(entry.timestamp.as_deref());
                    state.current_turn_start_ms = turn_start_ms;
                    state.last_accepted_token_timestamp_ms = turn_start_ms;
                    if let Some(model) = state.current_model.clone() {
                        flush_pending_model_messages(
                            &mut pending_model_messages,
                            &mut messages,
                            &mut fallback_timestamp_indices,
                            &model,
                        );
                    }
                    handled = true;
                }

                // A human `user_message` event starts a new turn. The event
                // itself carries no tokens, so we defer the flag to the next
                // token_count-derived message (the assistant's reply). This
                // counts `codex exec` one-shots too: they are headless but still
                // carry a real human prompt, so each is one turn. Only
                // system-injected messages (leading `<`, e.g.
                // <environment_context>, <system-reminder>) are excluded as
                // non-human input. Forked-child replays of the parent prompt
                // arrive before turn_context and are skipped by the
                // `forked_child_waiting_for_turn_context` branch above, so they
                // never reach here.
                if entry.entry_type == "event_msg"
                    && payload.payload_type.as_deref() == Some("user_message")
                {
                    if codex_message_is_human_turn(payload.message.as_deref()) {
                        state.pending_turn_start = true;
                        // Defensively reset the start-anchor cursor here too.
                        // Normally `turn_context` resets it every turn (see
                        // above), but a resumed/compacted session can emit a
                        // `token_count` after this `user_message` with no
                        // intervening `turn_context`. Without this reset the
                        // cursor would still hold the previous turn's last
                        // token time and bridge backward across the idle gap.
                        state.last_accepted_token_timestamp_ms =
                            parse_codex_entry_timestamp(entry.timestamp.as_deref());
                    }
                    handled = true;
                }

                // Process token_count events
                if is_token_count {
                    let info = match payload.info {
                        Some(i) => i,
                        None => continue,
                    };

                    let model = payload_model
                        .or(info_model)
                        .or_else(|| state.current_model.clone());
                    if let Some(ref model) = model {
                        state.current_model = Some(model.clone());
                        flush_pending_model_messages(
                            &mut pending_model_messages,
                            &mut messages,
                            &mut fallback_timestamp_indices,
                            model,
                        );
                    }

                    // Use last_token_usage as the primary increment source.
                    // Upstream totals are mutable snapshots (compaction, context-window
                    // capping can rewrite them), so we only use total_token_usage for
                    // dedup and monotonicity checks — never as a direct delta source.
                    let total_usage = info.total_token_usage.as_ref().map(CodexTotals::from_usage);
                    let last_usage = info.last_token_usage.as_ref().map(CodexTotals::from_usage);

                    // Forked child logs can replay more than one parent
                    // token_count row after the first child turn_context,
                    // often with child-local timestamps. Keep the inherited
                    // baseline active until totals move beyond it.
                    if forked_child_should_skip_inherited_snapshot(
                        &state,
                        info.total_token_usage.as_ref(),
                        total_usage,
                    ) {
                        continue;
                    }
                    state.forked_child_inherited_baseline = None;
                    state.forked_child_inherited_reported_total = None;

                    let (tokens, next_totals) =
                        match (total_usage, last_usage, state.previous_totals) {
                            // Both present with previous baseline (standard path)
                            (Some(total), Some(last), Some(previous)) => {
                                if total == previous {
                                    continue;
                                }
                                if total.delta_from(previous).is_none()
                                    && total.looks_like_stale_regression(previous, last)
                                {
                                    continue;
                                }
                                (last.into_tokens(), Some(total))
                            }
                            // Both present, first event — use last (NOT full total) to
                            // avoid overcounting tokens carried from a resumed session.
                            (Some(total), Some(last), None) => (last.into_tokens(), Some(total)),
                            // Only total, have previous (defensive — upstream schema
                            // requires both when info is present)
                            (Some(total), None, Some(previous)) => {
                                if total == previous {
                                    continue;
                                }
                                if let Some(delta) = total.delta_from(previous) {
                                    (delta.into_tokens(), Some(total))
                                } else {
                                    state.previous_totals = Some(total);
                                    continue;
                                }
                            }
                            // Only total, first event, no last — legacy/degraded path
                            (Some(total), None, None) => (total.into_tokens(), Some(total)),
                            // Only last, have previous
                            (None, Some(last), Some(previous)) => {
                                (last.into_tokens(), Some(previous.saturating_add(last)))
                            }
                            // Only last, no previous
                            (None, Some(last), None) => (last.into_tokens(), None),
                            // Neither
                            (None, None, _) => continue,
                        };

                    // Skip zero-token snapshots without advancing the baseline so
                    // that post-compaction zero totals don't inflate later deltas.
                    if tokens.input == 0
                        && tokens.output == 0
                        && tokens.cache_read == 0
                        && tokens.reasoning == 0
                    {
                        continue;
                    }

                    state.previous_totals = next_totals;

                    let parsed_timestamp = parse_codex_entry_timestamp(entry.timestamp.as_deref());
                    let timestamp = state
                        .last_accepted_token_timestamp_ms
                        .unwrap_or_else(|| parsed_timestamp.unwrap_or(fallback_timestamp));
                    let duration_ms = duration_between_ms(
                        state.last_accepted_token_timestamp_ms,
                        parsed_timestamp,
                    );

                    let agent = if state.session_is_headless {
                        Some("headless".to_string())
                    } else {
                        state.session_agent.clone()
                    };

                    let provider = state
                        .session_provider
                        .as_deref()
                        .or_else(|| model.as_deref().and_then(inferred_provider_from_model))
                        .unwrap_or("openai");

                    let mut message = UnifiedMessage::new_with_agent(
                        "codex",
                        model.clone().unwrap_or_else(|| "unknown".to_string()),
                        provider,
                        session_id.to_string(),
                        timestamp,
                        tokens,
                        0.0,
                        agent,
                    );
                    message.duration_ms = duration_ms;
                    // Apply a deferred human-turn marker from a preceding
                    // user_message to this assistant reply — the first
                    // token-bearing message after the human input.
                    if state.pending_turn_start {
                        message.is_turn_start = true;
                        state.pending_turn_start = false;
                    }
                    if parsed_timestamp.is_some() || total_usage.is_some() {
                        // Fork/subagent children replay the same upstream
                        // token_count history into many sibling files. Those
                        // replays carry identical cumulative totals but a
                        // distinct per-file session id, so a session-scoped key
                        // never collapses them and the totals get counted once
                        // per sibling. Scope the key to the fork parent instead
                        // so sibling replays share one key. Unrelated sessions
                        // keep their own id and never merge.
                        let dedup_scope_id = state
                            .session_forked_from_id
                            .as_deref()
                            .or(state.session_id_from_meta.as_deref())
                            .unwrap_or(session_id);
                        set_codex_dedup_key(
                            &mut message,
                            model.as_deref().unwrap_or("unknown"),
                            dedup_scope_id,
                            total_usage,
                        );
                    }
                    message.set_workspace(
                        state.session_workspace_key.clone(),
                        state.session_workspace_label.clone(),
                    );
                    if let Some(timestamp_ms) = parsed_timestamp {
                        if state
                            .last_accepted_token_timestamp_ms
                            .is_none_or(|cursor_ms| timestamp_ms > cursor_ms)
                        {
                            state.last_accepted_token_timestamp_ms = Some(timestamp_ms);
                        }
                    }
                    if model.is_some() {
                        messages.push(message);
                        if parsed_timestamp.is_none() {
                            fallback_timestamp_indices.push(messages.len() - 1);
                        }
                    } else {
                        pending_model_messages.push((message, parsed_timestamp.is_none()));
                    }
                    handled = true;
                }
            }

            // Mark session_meta as handled (even if payload was processed above)
            if entry.entry_type == "session_meta" {
                handled = true;
            }
        }

        if handled {
            continue;
        }

        if state.forked_child_waiting_for_turn_context {
            let mut json_probe = trimmed.as_bytes().to_vec();
            if simd_json::from_slice::<Value>(&mut json_probe).is_ok() {
                continue;
            }
        }

        let headless_message = parse_codex_headless_line(
            trimmed,
            session_id,
            &mut state.current_model,
            fallback_timestamp,
            state.session_provider.as_deref(),
            &state.session_agent,
            state.session_is_headless,
        );
        if !pending_model_messages.is_empty() {
            if let Some(model) = state.current_model.clone() {
                flush_pending_model_messages(
                    &mut pending_model_messages,
                    &mut messages,
                    &mut fallback_timestamp_indices,
                    &model,
                );
            } else {
                flush_pending_model_messages_as_unknown(
                    &mut pending_model_messages,
                    &mut messages,
                    &mut fallback_timestamp_indices,
                    &mut unresolved_model_events,
                );
            }
        }

        if let Some((mut msg, used_fallback_timestamp)) = headless_message {
            msg.set_workspace(
                state.session_workspace_key.clone(),
                state.session_workspace_label.clone(),
            );
            messages.push(msg);
            if used_fallback_timestamp {
                fallback_timestamp_indices.push(messages.len() - 1);
            }
            continue;
        }

        let mut json_probe = trimmed.as_bytes().to_vec();
        if simd_json::from_slice::<Value>(&mut json_probe).is_err() {
            parse_succeeded = false;
            continue;
        }
    }

    flush_pending_model_messages_as_unknown(
        &mut pending_model_messages,
        &mut messages,
        &mut fallback_timestamp_indices,
        &mut unresolved_model_events,
    );

    ParsedCodexFile {
        messages,
        fallback_timestamp_indices,
        consumed_offset,
        parse_succeeded,
        unresolved_model_events,
        state,
    }
}

fn codex_source_is_exec(source: Option<&Value>) -> bool {
    source.and_then(Value::as_str) == Some("exec")
}

fn forked_from_id_from_source(source: Option<&Value>) -> Option<&str> {
    source?
        .get("subagent")?
        .get("thread_spawn")?
        .get("parent_thread_id")?
        .as_str()
        .filter(|id| !id.is_empty())
}

fn forked_child_turn_starts_own_session(state: &CodexParseState, turn_id: Option<&str>) -> bool {
    if state.forked_child_replay_session_id.is_none() {
        return true;
    }

    let Some(child_session_id) = state.forked_child_session_id.as_deref() else {
        return true;
    };

    match (turn_id, codex_uuid_v7_order_key(child_session_id)) {
        (Some(turn_id), Some(child_key)) => {
            let Some(turn_key) = codex_uuid_v7_order_key(turn_id) else {
                // Nested child logs can replay legacy UUID v4 turns from an
                // ancestor. Only a child-local task_started event may end the
                // replay gate for a non-v7 subagent turn. Human forks do not
                // emit task_started, so retain their existing fallback.
                return state.forked_child_is_user_fork
                    || state.forked_child_task_started_turn_ids.contains(turn_id);
            };
            // Compare only the UUID v7 48-bit millisecond timestamp (the first
            // 12 hex of the order key), not the full id. The child's own turn is
            // minted at or after its session_meta and the replayed parent turns
            // strictly earlier, so the millisecond prefix is the causal signal;
            // the version nibble + random tail of two independently-minted v7
            // UUIDs is a coin flip.
            match turn_key[..12].cmp(&child_key[..12]) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                // Same millisecond: the timestamp ties and the random tail
                // cannot order the child's own turn against a replayed parent
                // turn that happens to share the fork's millisecond. A subagent
                // fork announces its own turn with a `task_started` event while
                // replayed parent turns are not, so only a task-started turn_id
                // ends the skip there — otherwise an equal-prefix replayed parent
                // turn would be miscounted as child-local. A human (`thread_source:
                // "user"`) fork never emits `task_started`, but its replayed
                // parent turns carry the *parent's* millisecond prefix rather
                // than the child's, so reaching this equal-prefix branch already
                // means the turn shares the child's fork millisecond and is the
                // child's own turn — end the skip.
                //
                // Residual (accepted): for a user fork this resolves on
                // millisecond-prefix equality alone. If a replayed parent turn
                // were itself minted within the exact same 1ms as the child's
                // fork (so it shares the *child's* prefix, not the parent's),
                // this branch would end the skip one turn early. That requires a
                // sub-millisecond, human-paced fork coincidence and is accepted;
                // subagent forks are hardened separately via `task_started`,
                // which user forks do not emit.
                std::cmp::Ordering::Equal => {
                    state.forked_child_is_user_fork
                        || state.forked_child_task_started_turn_ids.contains(turn_id)
                }
            }
        }
        _ => true,
    }
}

fn forked_child_task_starts_own_session(
    state: &CodexParseState,
    turn_id: Option<&str>,
    started_at: Option<i64>,
) -> bool {
    let (Some(turn_id), Some(child_session_id)) =
        (turn_id, state.forked_child_session_id.as_deref())
    else {
        return false;
    };
    let Some(child_key) = codex_uuid_v7_order_key(child_session_id) else {
        return true;
    };

    if let Some(turn_key) = codex_uuid_v7_order_key(turn_id) {
        return turn_key[..12] >= child_key[..12];
    }

    let Some(started_at) = started_at else {
        return false;
    };
    let Ok(child_started_at_ms) = i64::from_str_radix(&child_key[..12], 16) else {
        return false;
    };

    // `child_started_at_ms / 1000` floors to the child's fork second, so a
    // legacy replay whose `started_at` lands in that same integer second
    // (but strictly before the child's sub-second fork instant) is admitted.
    started_at >= child_started_at_ms / 1000
}

fn codex_uuid_v7_order_key(id: &str) -> Option<String> {
    let mut parts = id.split('-');
    let first = parts.next()?;
    let second = parts.next()?;
    let third = parts.next()?;
    let fourth = parts.next()?;
    let fifth = parts.next()?;

    if parts.next().is_some()
        || first.len() != 8
        || second.len() != 4
        || third.len() != 4
        || fourth.len() != 4
        || fifth.len() != 12
        || !third.starts_with('7')
    {
        return None;
    }

    let mut key = String::with_capacity(32);
    for part in [first, second, third, fourth, fifth] {
        if !part.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        key.push_str(&part.to_ascii_lowercase());
    }
    Some(key)
}

fn parse_codex_entry_timestamp(timestamp: Option<&str>) -> Option<i64> {
    timestamp
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.timestamp_millis())
}

fn duration_between_ms(start_ms: Option<i64>, end_ms: Option<i64>) -> Option<i64> {
    let duration = end_ms?.saturating_sub(start_ms?);
    (duration > 0).then_some(duration)
}

fn codex_token_count_dedup_key(
    message: &UnifiedMessage,
    model: &str,
    upstream_session_id: &str,
    total_usage: Option<CodexTotals>,
) -> String {
    if let Some(total) = total_usage {
        // Codex fork/subagent logs can replay the same upstream token_count
        // history into many child files with child-local timestamps. The
        // cumulative total is the stable upstream identity; timestamp is only
        // a fallback when older rows do not carry totals.
        return format!(
            "codex:token_count-total:{}:{}:{}:{}:{}:{}:{}",
            upstream_session_id,
            message.provider_id,
            model,
            total.input,
            total.output,
            total.cached,
            total.reasoning
        );
    }

    format!(
        "codex:token_count:{}:{}:{}:{}:{}:{}:{}:{}",
        message.timestamp,
        message.provider_id,
        model,
        message.tokens.input,
        message.tokens.output,
        message.tokens.cache_read,
        message.tokens.cache_write,
        message.tokens.reasoning
    )
}

fn set_codex_dedup_key(
    message: &mut UnifiedMessage,
    model: &str,
    upstream_session_id: &str,
    total_usage: Option<CodexTotals>,
) {
    if message.dedup_key.is_none() {
        message.dedup_key = Some(codex_token_count_dedup_key(
            message,
            model,
            upstream_session_id,
            total_usage,
        ));
    }
}

fn flush_pending_model_messages(
    pending_model_messages: &mut Vec<(UnifiedMessage, bool)>,
    messages: &mut Vec<UnifiedMessage>,
    fallback_timestamp_indices: &mut Vec<usize>,
    model: &str,
) {
    for (mut message, used_fallback_timestamp) in pending_model_messages.drain(..) {
        if !used_fallback_timestamp {
            let upstream_session_id = message.session_id.clone();
            set_codex_dedup_key(&mut message, model, &upstream_session_id, None);
        }
        message.model_id = model.to_string();
        messages.push(message);
        if used_fallback_timestamp {
            fallback_timestamp_indices.push(messages.len() - 1);
        }
    }
}

fn flush_pending_model_messages_as_unknown(
    pending_model_messages: &mut Vec<(UnifiedMessage, bool)>,
    messages: &mut Vec<UnifiedMessage>,
    fallback_timestamp_indices: &mut Vec<usize>,
    unresolved_model_events: &mut bool,
) {
    if pending_model_messages.is_empty() {
        return;
    }

    *unresolved_model_events = true;
    flush_pending_model_messages(
        pending_model_messages,
        messages,
        fallback_timestamp_indices,
        "unknown",
    );
}

/// Parse a Codex JSONL file with stateful tracking
pub fn parse_codex_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let session_id = session_id_from_path(path);
    let fallback_timestamp = file_modified_timestamp_ms(path);
    let reader = BufReader::new(file);
    let mut parsed = parse_codex_reader(
        reader,
        &session_id,
        fallback_timestamp,
        0,
        CodexParseState::default(),
    );
    for index in parsed.fallback_timestamp_indices {
        if let Some(message) = parsed.messages.get_mut(index) {
            message.set_timestamp_provenance(crate::TimestampProvenance::Fallback);
        }
    }
    parsed.messages
}

fn reported_total_tokens(usage: &CodexTokenUsage) -> Option<i64> {
    usage.total_tokens.filter(|total| *total >= 0)
}

fn remember_forked_child_inherited_baseline(state: &mut CodexParseState, info: &CodexInfo) {
    let Some(total_usage) = info.total_token_usage.as_ref() else {
        return;
    };

    let totals = CodexTotals::from_usage(total_usage);
    state.previous_totals = Some(totals);
    state.forked_child_inherited_baseline = Some(totals);
    state.forked_child_inherited_reported_total = reported_total_tokens(total_usage);
}

fn forked_child_should_skip_inherited_snapshot(
    state: &CodexParseState,
    total_usage: Option<&CodexTokenUsage>,
    totals: Option<CodexTotals>,
) -> bool {
    if let (Some(usage), Some(baseline)) =
        (total_usage, state.forked_child_inherited_reported_total)
    {
        if reported_total_tokens(usage).is_some_and(|total| total <= baseline) {
            return true;
        }
    }

    if let (Some(totals), Some(baseline)) = (totals, state.forked_child_inherited_baseline) {
        return totals.is_within(baseline);
    }

    false
}

pub(crate) fn parse_codex_file_incremental(
    path: &Path,
    start_offset: u64,
    state: CodexParseState,
) -> ParsedCodexFile {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => {
            return ParsedCodexFile {
                messages: Vec::new(),
                fallback_timestamp_indices: Vec::new(),
                consumed_offset: start_offset,
                parse_succeeded: false,
                unresolved_model_events: false,
                state,
            };
        }
    };

    if file.seek(SeekFrom::Start(start_offset)).is_err() {
        return ParsedCodexFile {
            messages: Vec::new(),
            fallback_timestamp_indices: Vec::new(),
            consumed_offset: start_offset,
            parse_succeeded: false,
            unresolved_model_events: false,
            state,
        };
    }

    let session_id = session_id_from_path(path);
    let fallback_timestamp = file_modified_timestamp_ms(path);
    let reader = BufReader::new(file);
    parse_codex_reader(reader, &session_id, fallback_timestamp, start_offset, state)
}

fn extract_model(payload: &CodexPayload) -> Option<String> {
    payload
        .model_info
        .as_ref()
        .and_then(|mi| mi.slug.clone())
        .filter(|s| !s.is_empty())
        .or(payload.model.clone().filter(|s| !s.is_empty()))
        .or(payload.model_name.clone().filter(|s| !s.is_empty()))
        .or(payload.info.as_ref().and_then(extract_model_from_info))
}

fn extract_model_from_info(info: &CodexInfo) -> Option<String> {
    info.model
        .clone()
        .filter(|s| !s.is_empty())
        .or(info.model_name.clone().filter(|s| !s.is_empty()))
}

struct CodexHeadlessUsage {
    input: i64,
    output: i64,
    cached: i64,
    model: Option<String>,
    timestamp_ms: Option<i64>,
}

fn parse_codex_headless_line(
    line: &str,
    session_id: &str,
    current_model: &mut Option<String>,
    fallback_timestamp: i64,
    session_provider: Option<&str>,
    session_agent: &Option<String>,
    session_is_headless: bool,
) -> Option<(UnifiedMessage, bool)> {
    let mut bytes = line.as_bytes().to_vec();
    let value: Value = simd_json::from_slice(&mut bytes).ok()?;

    if let Some(model) = extract_model_from_value(&value) {
        *current_model = Some(model);
    }

    let usage = extract_headless_usage(&value)?;
    let model = usage
        .model
        .or_else(|| current_model.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let timestamp = usage.timestamp_ms.unwrap_or(fallback_timestamp);

    if usage.input == 0 && usage.output == 0 && usage.cached == 0 {
        return None;
    }

    let provider = session_provider
        .or_else(|| inferred_provider_from_model(&model))
        .unwrap_or("openai");
    let agent = if session_is_headless {
        Some("headless".to_string())
    } else {
        session_agent.clone()
    };

    Some((
        UnifiedMessage::new_with_agent(
            "codex",
            model,
            provider,
            session_id.to_string(),
            timestamp,
            TokenBreakdown {
                input: usage.input.max(0),
                output: usage.output.max(0),
                cache_read: usage.cached.max(0),
                cache_write: 0,
                reasoning: 0,
            },
            0.0,
            agent,
        ),
        usage.timestamp_ms.is_none(),
    ))
}

fn extract_headless_usage(value: &Value) -> Option<CodexHeadlessUsage> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("data").and_then(|data| data.get("usage")))
        .or_else(|| value.get("result").and_then(|data| data.get("usage")))
        .or_else(|| value.get("response").and_then(|data| data.get("usage")))?;

    let input_tokens = extract_i64(usage.get("input_tokens"))
        .or_else(|| extract_i64(usage.get("prompt_tokens")))
        .or_else(|| extract_i64(usage.get("input")))
        .unwrap_or(0);
    let output_tokens = extract_i64(usage.get("output_tokens"))
        .or_else(|| extract_i64(usage.get("completion_tokens")))
        .or_else(|| extract_i64(usage.get("output")))
        .unwrap_or(0);
    let cached_tokens = extract_i64(usage.get("cached_input_tokens"))
        .or_else(|| extract_i64(usage.get("cache_read_input_tokens")))
        .or_else(|| extract_i64(usage.get("cached_tokens")))
        .unwrap_or(0);

    let model = extract_model_from_value(value)
        .or_else(|| value.get("data").and_then(extract_model_from_value));
    let timestamp_ms = extract_timestamp_from_value(value);

    Some(CodexHeadlessUsage {
        input: input_tokens.saturating_sub(cached_tokens),
        output: output_tokens,
        cached: cached_tokens,
        model,
        timestamp_ms,
    })
}

fn extract_model_from_value(value: &Value) -> Option<String> {
    extract_string(value.get("model"))
        .or_else(|| extract_string(value.get("model_name")))
        .or_else(|| {
            value
                .get("data")
                .and_then(|data| extract_string(data.get("model")))
        })
        .or_else(|| {
            value
                .get("data")
                .and_then(|data| extract_string(data.get("model_name")))
        })
        .or_else(|| {
            value
                .get("response")
                .and_then(|data| extract_string(data.get("model")))
        })
}

fn extract_timestamp_from_value(value: &Value) -> Option<i64> {
    value
        .get("timestamp")
        .or_else(|| value.get("time"))
        .or_else(|| value.get("created_at"))
        .or_else(|| value.get("data").and_then(|data| data.get("timestamp")))
        .and_then(parse_timestamp_value)
}

/// Prefixes Codex prepends to context it injects as `user_message` events.
/// These are the bodies that must NOT be counted as human turns.
const CODEX_SYSTEM_INJECTED_PREFIXES: [&str; 3] = [
    "<environment_context>",
    "<system-reminder>",
    "<user_instructions>",
];

/// Returns true when a Codex `user_message` payload represents real human input
/// rather than system-injected context. Codex stores the body as a plain string
/// in `payload.message`; the harness injects context blocks that open with one of
/// the known tags in [`CODEX_SYSTEM_INJECTED_PREFIXES`] after trimming. Matching
/// those specific prefixes — rather than any leading `<` — avoids dropping
/// legitimate human prompts that happen to start with markup (asking about a
/// `<div>`, pasting an XML snippet, etc.). The `kind` field can't be used to
/// distinguish them: both human and injected bodies appear as `kind:"plain"` or
/// with no `kind` at all.
fn codex_message_is_human_turn(message: Option<&str>) -> bool {
    match message {
        Some(text) => {
            let trimmed = text.trim_start();
            !CODEX_SYSTEM_INJECTED_PREFIXES
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn public_codex_parser_marks_fallback_timestamps_untrusted() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"{{"type":"turn_context","payload":{{"model":"gpt-5"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":10,"output_tokens":2}},"total_token_usage":{{"input_tokens":10,"output_tokens":2}}}}}}}}"#
        )
        .unwrap();

        let messages = parse_codex_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].timestamp_provenance,
            crate::TimestampProvenance::Fallback
        );
        assert!(!messages[0].is_trustworthy_for_hourly());
    }
}
