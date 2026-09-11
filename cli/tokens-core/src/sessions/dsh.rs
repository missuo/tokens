//! DSH Desktop session parser.
//!
//! DSH (`ai.deepseek.dsh.desktop`) is an Electron agent client that persists one
//! whole session as a single zstd-compressed JSONL file:
//!
//! ```text
//! <root>/sessions/<urlencoded-cwd>/<session-id>/session.jsonl.zstd
//! ```
//!
//! `<root>` is `$DSH_HOME` when set, otherwise `~/.dsh`.
//!
//! Every line is an event `{type, seq, time, data}`. Token accounting arrives on
//! `assistant/chunk` events whose chunk is `{"type":"usage", ...}`, and the model
//! that produced them is carried by that *same step's* `finish` chunk — which is
//! emitted after the usage chunk — on `replayState`. Steps are therefore buffered
//! and attributed when their own naming event arrives.
//!
//! Two properties of the counters drive the aggregation below:
//!
//! 1. All four counters — `inputTokens`, `outputTokens`, `cacheReadTokens` and
//!    `cacheWriteTokens` — are per-call deltas and are summed.
//!
//!    `cacheReadTokens` is the one worth being explicit about, because it climbs
//!    steadily across a session and looks like a running total. It is not: a
//!    longer conversation genuinely reads more cache on each call. The
//!    authoritative check is the client's own bookkeeping — summing the usage
//!    chunks by hand reproduces `tokenUsage.totals` in
//!    `~/.dsh/storages/session_projcache.json` exactly, field for field
//!    (32,247,434 tokens on one machine). Taking a per-group maximum instead
//!    under-reports the same corpus by roughly ninefold.
//! 2. `reasoningTokens` is a subset of `outputTokens` (1189 of 1428, 1007 of 1705),
//!    and `TokenBreakdown::total()` adds `reasoning` on top of `output`. Carrying
//!    the value through would therefore inflate every total by the reasoning
//!    share, so it is deliberately dropped instead of surfaced as its own field.
//!
//! Sessions imported from Claude Code (`session-id` prefixed `import-`) are
//! skipped: DSH mirrors those transcripts but drops their usage counters, and the
//! originals under `~/.claude` are already counted by the Claude Code parser.

use super::utils::file_modified_timestamp_ms;
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

const CLIENT_ID: &str = "dsh";
/// Fallback used when no surrounding event names a model. Keeps the tokens
/// attributable instead of dropping them.
const UNKNOWN_MODEL: &str = "dsh-unknown";
/// Session-id prefix marking a transcript mirrored from Claude Code.
const IMPORTED_SESSION_PREFIX: &str = "import-";

/// A single JSONL event.
#[derive(Debug, Deserialize)]
struct DshEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    #[serde(default)]
    time: Option<i64>,
    #[serde(default)]
    data: Option<DshEventData>,
    /// Present on `session` events only.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DshEventData {
    #[serde(default)]
    turn: Option<u64>,
    #[serde(default)]
    step: Option<u64>,
    #[serde(default)]
    chunk: Option<DshChunk>,
    #[serde(default)]
    message: Option<DshMessage>,
}

#[derive(Debug, Deserialize)]
struct DshChunk {
    #[serde(rename = "type")]
    chunk_type: Option<String>,
    #[serde(default)]
    usage: Option<DshUsage>,
    #[serde(rename = "replayState", default)]
    replay_state: Option<DshReplayState>,
}

#[derive(Debug, Deserialize)]
struct DshReplayState {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DshMessage {
    #[serde(default)]
    source: Option<DshReplayState>,
}

/// The optional per-call counters. Every field is absent in some real record, so
/// all five combinations occur.
#[derive(Debug, Default, Clone, Deserialize)]
struct DshUsage {
    #[serde(rename = "inputTokens", default)]
    input_tokens: Option<i64>,
    #[serde(rename = "outputTokens", default)]
    output_tokens: Option<i64>,
    #[serde(rename = "cacheReadTokens", default)]
    cache_read_tokens: Option<i64>,
    #[serde(rename = "cacheWriteTokens", default)]
    cache_write_tokens: Option<i64>,
    /// Parsed but intentionally unused: it is a subset of `outputTokens` and
    /// recording it would double-count through `TokenBreakdown::total()`.
    #[serde(rename = "reasoningTokens", default)]
    #[allow(dead_code)]
    reasoning_tokens: Option<i64>,
}

/// A model known to have produced one or more usage chunks inside a turn.
#[derive(Debug, Clone)]
struct DshModel {
    provider: String,
    model: String,
}

/// Accumulator for one `(turn, model)` pair.
#[derive(Debug, Default)]
struct DshGroup {
    /// Provider as named by the source event; empty when the event omitted it.
    provider_name: Option<String>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    last_timestamp_ms: i64,
}

impl DshGroup {
    fn record(&mut self, usage: &DshUsage, timestamp_ms: i64) {
        // Every counter, `cacheReadTokens` included, is a per-call delta and is
        // summed. The authoritative check is the client's own
        // `tokenUsage.totals` in `~/.dsh/storages/session_projcache.json`: flat
        // summation of the usage chunks reproduces it exactly (32,247,434 across
        // one machine). The counters do climb between steps, which looks like a
        // running total, but that is genuine growth — a longer conversation reads
        // more cache each turn.
        self.input += usage.input_tokens.unwrap_or(0).max(0);
        self.output += usage.output_tokens.unwrap_or(0).max(0);
        self.cache_read += usage.cache_read_tokens.unwrap_or(0).max(0);
        self.cache_write += usage.cache_write_tokens.unwrap_or(0).max(0);
        // `reasoningTokens` is not accumulated: it is already contained in
        // `outputTokens`, and `TokenBreakdown::total()` adds `reasoning` on top of
        // `output`, so recording it here would double-count it.
        if timestamp_ms > 0 {
            self.last_timestamp_ms = timestamp_ms;
        }
    }

    /// A group is only worth emitting when it carries real tokens: failed turns
    /// emit `{"inputTokens":0,"outputTokens":0}` usage chunks that would
    /// otherwise create empty rows.
    fn is_empty(&self) -> bool {
        self.input == 0 && self.output == 0 && self.cache_read == 0 && self.cache_write == 0
    }

    fn provider_id(&self) -> &str {
        self.provider_name.as_deref().unwrap_or(CLIENT_ID)
    }
}

pub fn parse_dsh_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    // A truncated or corrupt frame mid-file would abort the whole read, so the
    // decoder is driven line by line with failures treated as end-of-input.
    let decoder = match zstd::stream::read::Decoder::new(file) {
        Ok(decoder) => decoder,
        Err(_) => return Vec::new(),
    };

    let fallback_session_id = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("dsh-session")
        .to_string();
    let fallback_timestamp_ms = file_modified_timestamp_ms(path);

    let mut session_id: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut groups: BTreeMap<(u64, String), DshGroup> = BTreeMap::new();

    // Usage chunks carry no model; the `finish` chunk for the *same* step names
    // it, and appears after the usage chunk. Buffering on `(turn, step)` — rather
    // than applying every usage to whichever model was named most recently — is
    // what keeps attribution correct: a turn that switches model between steps
    // would otherwise charge the new step's tokens to the previous model, and the
    // new model's own tokens to the one before that.
    let mut pending: BTreeMap<(u64, u64), Vec<(DshUsage, i64)>> = BTreeMap::new();
    // Model named by the most recent `finish`/`assistant/message`, used only to
    // attribute steps whose own naming event never arrived.
    let mut last_model: Option<DshModel> = None;

    for line in BufReader::new(decoder).lines() {
        let Ok(line) = line else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<DshEvent>(trimmed) else {
            continue;
        };

        match event.event_type.as_deref() {
            Some("session") => {
                // The header is the first line, so an imported transcript is
                // abandoned here rather than after decompressing the whole file.
                if event
                    .id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(IMPORTED_SESSION_PREFIX))
                {
                    return Vec::new();
                }
                if session_id.is_none() {
                    session_id = event.id.clone();
                }
                if cwd.is_none() {
                    cwd = event.cwd.clone();
                }
            }
            Some("assistant/chunk") => {
                let Some(data) = event.data.as_ref() else {
                    continue;
                };
                let Some(chunk) = data.chunk.as_ref() else {
                    continue;
                };
                let timestamp_ms = event.time.filter(|value| *value > 0).unwrap_or(0);

                match chunk.chunk_type.as_deref() {
                    Some("usage") if data.turn.is_some() && chunk.usage.is_some() => {
                        // Never attributed here: the model for this step is only
                        // known once that step's own naming event is read, and
                        // `last_model` may still belong to the previous step.
                        let step_key = (data.turn.unwrap_or(0), data.step.unwrap_or(0));
                        let usage = chunk.usage.clone().unwrap_or_default();
                        pending
                            .entry(step_key)
                            .or_default()
                            .push((usage, timestamp_ms));
                    }
                    Some("finish") => {
                        if let Some(model) = model_from_replay_state(chunk.replay_state.as_ref()) {
                            let step_key = (data.turn.unwrap_or(0), data.step.unwrap_or(0));
                            // This step's own usage now knows its model.
                            if let Some(buffered) = pending.remove(&step_key) {
                                for (usage, timestamp_ms) in buffered {
                                    apply_usage(
                                        &mut groups,
                                        step_key.0,
                                        &model,
                                        &usage,
                                        timestamp_ms,
                                    );
                                }
                            }
                            last_model = Some(model);
                        }
                    }
                    _ => {}
                }
            }
            Some("assistant/message") => {
                let source = event.data.as_ref().and_then(|data| data.message.as_ref());
                if let Some(model) =
                    model_from_replay_state(source.and_then(|message| message.source.as_ref()))
                {
                    let step_key = (
                        event.data.as_ref().and_then(|data| data.turn).unwrap_or(0),
                        event.data.as_ref().and_then(|data| data.step).unwrap_or(0),
                    );
                    if let Some(buffered) = pending.remove(&step_key) {
                        for (usage, timestamp_ms) in buffered {
                            apply_usage(&mut groups, step_key.0, &model, &usage, timestamp_ms);
                        }
                    }
                    last_model = Some(model);
                }
            }
            _ => {}
        }
    }

    // Backstop for a transcript that carries no `session` header at all: the id
    // then falls back to the parent directory name, which is also `import-<uuid>`
    // for a mirrored session.
    if session_id
        .as_deref()
        .is_some_and(|id| id.starts_with(IMPORTED_SESSION_PREFIX))
    {
        return Vec::new();
    }

    // Steps whose own `finish` never named a model — the session ended, or the
    // turn errored before one arrived. Attributed to whatever model was named last,
    // or to the fallback when the file named none at all, so the tokens are
    // counted rather than silently dropped.
    for ((turn, _step), buffered) in std::mem::take(&mut pending) {
        let model = last_model.clone().unwrap_or_else(|| DshModel {
            provider: String::new(),
            model: UNKNOWN_MODEL.to_string(),
        });
        for (usage, timestamp_ms) in buffered {
            apply_usage(&mut groups, turn, &model, &usage, timestamp_ms);
        }
    }

    let session_id = session_id.unwrap_or(fallback_session_id);
    let workspace_key = cwd.as_deref().and_then(normalize_workspace_key);
    let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);

    groups
        .into_iter()
        .filter(|(_, group)| !group.is_empty())
        .map(|((turn, model_name), group)| {
            let timestamp = if group.last_timestamp_ms > 0 {
                group.last_timestamp_ms
            } else {
                fallback_timestamp_ms
            };
            let mut message = UnifiedMessage::new(
                CLIENT_ID,
                model_name.clone(),
                group.provider_id(),
                session_id.clone(),
                timestamp,
                TokenBreakdown {
                    input: group.input,
                    output: group.output,
                    cache_read: group.cache_read,
                    cache_write: group.cache_write,
                    reasoning: 0,
                },
                0.0,
            );
            message.dedup_key = Some(format!("{CLIENT_ID}:{session_id}:{turn}:{model_name}"));
            message.set_workspace(workspace_key.clone(), workspace_label.clone());
            message
        })
        .collect()
}

fn apply_usage(
    groups: &mut BTreeMap<(u64, String), DshGroup>,
    turn: u64,
    model: &DshModel,
    usage: &DshUsage,
    timestamp_ms: i64,
) {
    let model_name = if model.model.trim().is_empty() {
        UNKNOWN_MODEL.to_string()
    } else {
        model.model.trim().to_string()
    };
    let group = groups.entry((turn, model_name)).or_default();
    if group.provider_name.is_none() && !model.provider.trim().is_empty() {
        group.provider_name = Some(model.provider.trim().to_string());
    }
    group.record(usage, timestamp_ms);
}

fn model_from_replay_state(state: Option<&DshReplayState>) -> Option<DshModel> {
    let state = state?;
    let model = state.model.as_deref()?.trim();
    if model.is_empty() {
        return None;
    }
    Some(DshModel {
        provider: state.provider.clone().unwrap_or_default(),
        model: model.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const CWD: &str = "/Users/dev/proj";

    /// Write `lines` as a zstd-compressed transcript, matching the on-disk
    /// `session.jsonl.zstd` shape.
    fn write_session(lines: &[serde_json::Value]) -> NamedTempFile {
        let body = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let file = NamedTempFile::new().unwrap();
        let mut encoder = zstd::stream::write::Encoder::new(file, 0).unwrap();
        encoder.write_all(body.as_bytes()).unwrap();
        encoder.finish().unwrap()
    }

    fn session_header(id: &str) -> serde_json::Value {
        json!({ "type": "session", "id": id, "createdAt": 1_786_930_276_802_i64, "cwd": CWD })
    }

    /// A usage chunk followed by the `finish` chunk that names its model, which is
    /// how real transcripts attribute tokens. Built through `serde_json` rather
    /// than a hand-escaped template so the nesting cannot drift.
    fn usage_step(
        turn: u64,
        step: u64,
        usage: serde_json::Value,
        model: &str,
    ) -> Vec<serde_json::Value> {
        vec![
            json!({
                "type": "assistant/chunk",
                "time": 1_786_930_276_802_i64,
                "data": { "turn": turn, "step": step, "chunk": { "type": "usage", "usage": usage } }
            }),
            json!({
                "type": "assistant/chunk",
                "time": 1_786_930_276_803_i64,
                "data": {
                    "turn": turn,
                    "step": step,
                    "chunk": {
                        "type": "finish",
                        "reason": { "kind": "completed" },
                        "replayState": { "kind": "pi-ai", "provider": "anthropic", "model": model }
                    }
                }
            }),
        ]
    }

    #[test]
    fn sums_every_counter_because_they_are_all_per_call_deltas() {
        // `cacheReadTokens` climbs between steps of a session, which looks like a
        // running total; the client's own `tokenUsage.totals` proves it is a
        // per-call delta, so it is summed like the rest.
        let mut lines = vec![session_header("session-abc")];
        lines.extend(usage_step(
            1,
            1,
            json!({ "inputTokens": 10, "outputTokens": 5, "cacheReadTokens": 1000 }),
            "claude-fable-5",
        ));
        lines.extend(usage_step(
            1,
            2,
            json!({ "inputTokens": 20, "outputTokens": 7, "cacheReadTokens": 2000 }),
            "claude-fable-5",
        ));

        let session = write_session(&lines);
        let messages = parse_dsh_file(session.path());

        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.client, CLIENT_ID);
        assert_eq!(message.model_id, "claude-fable-5");
        assert_eq!(message.provider_id, "anthropic");
        assert_eq!(message.session_id, "session-abc");
        assert_eq!(message.tokens.input, 30);
        assert_eq!(message.tokens.output, 12);
        // 1000 + 2000, not max(1000, 2000).
        assert_eq!(message.tokens.cache_read, 3000);
        assert_eq!(message.tokens.total(), 3042);
    }

    #[test]
    fn keeps_turns_and_models_in_separate_groups() {
        // Grouping is per (turn, model): steps of one model in one turn fold into
        // one row, a model switch inside a turn starts a new row, and a later turn
        // of the same model is its own row.
        let mut lines = vec![session_header("session-abc")];
        // (turn 1, glm-5.2) step 1
        lines.extend(usage_step(
            1,
            1,
            json!({ "inputTokens": 10, "outputTokens": 5, "cacheReadTokens": 5000 }),
            "glm-5.2",
        ));
        // (turn 1, glm-5.2) step 2 — same row, values add up
        lines.extend(usage_step(
            1,
            2,
            json!({ "inputTokens": 2, "outputTokens": 3, "cacheReadTokens": 7000 }),
            "glm-5.2",
        ));
        // (turn 1, gpt-5.5) — different model, its own row
        lines.extend(usage_step(
            1,
            3,
            json!({ "inputTokens": 4, "outputTokens": 6, "cacheReadTokens": 700 }),
            "gpt-5.5",
        ));
        // (turn 2, glm-5.2) — later turn, its own row
        lines.extend(usage_step(
            2,
            1,
            json!({ "inputTokens": 1, "outputTokens": 1, "cacheReadTokens": 100 }),
            "glm-5.2",
        ));

        let session = write_session(&lines);
        let messages = parse_dsh_file(session.path());

        assert_eq!(messages.len(), 3);
        let group = |model: &str, turn: u64| {
            let expected = format!("{CLIENT_ID}:session-abc:{turn}:{model}");
            messages
                .iter()
                .find(|message| message.dedup_key.as_deref() == Some(expected.as_str()))
                .unwrap_or_else(|| panic!("missing group {expected}"))
        };
        // Steps of one model in one turn fold into a single row.
        let glm_turn1 = group("glm-5.2", 1);
        assert_eq!(glm_turn1.tokens.cache_read, 5000 + 7000);
        assert_eq!(glm_turn1.tokens.input, 12);
        assert_eq!(glm_turn1.tokens.output, 8);
        // A different model in that same turn is its own row.
        assert_eq!(group("gpt-5.5", 1).tokens.cache_read, 700);
        // A later turn of the first model is a third row.
        assert_eq!(group("glm-5.2", 2).tokens.cache_read, 100);
    }

    #[test]
    fn does_not_double_count_reasoning_which_is_part_of_output() {
        // `reasoningTokens` is contained in `outputTokens`, and
        // `TokenBreakdown::total()` adds `reasoning` on top of `output`, so
        // carrying it through would inflate the total by the reasoning share.
        let mut lines = vec![session_header("session-abc")];
        lines.extend(usage_step(
            1,
            1,
            json!({ "inputTokens": 320, "outputTokens": 1428, "reasoningTokens": 1189 }),
            "claude-fable-5",
        ));

        let session = write_session(&lines);
        let messages = parse_dsh_file(session.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.reasoning, 0);
        assert_eq!(messages[0].tokens.total(), 1748);
    }

    #[test]
    fn skips_sessions_imported_from_claude_code() {
        // The mirror carries no usage of its own, and the originals under
        // `~/.claude` are already counted by the Claude Code parser.
        let mut lines = vec![session_header("import-098c451d")];
        lines.extend(usage_step(
            1,
            1,
            json!({ "inputTokens": 10, "outputTokens": 5 }),
            "claude-fable-5",
        ));

        let session = write_session(&lines);
        assert!(parse_dsh_file(session.path()).is_empty());
    }

    #[test]
    fn drops_error_turns_that_report_zero_tokens() {
        // Failed turns emit an all-zero usage chunk; a row for each would add
        // empty groups to every session.
        let mut lines = vec![session_header("session-abc")];
        lines.extend(usage_step(
            1,
            1,
            json!({ "inputTokens": 0, "outputTokens": 0 }),
            "gpt-5.5",
        ));

        let session = write_session(&lines);
        assert!(parse_dsh_file(session.path()).is_empty());
    }

    #[test]
    fn attributes_usage_that_precedes_the_naming_event_to_the_fallback() {
        // A session that ends before any `finish` names a model: the tokens are
        // still counted, under the fallback bucket.
        let lines = vec![
            session_header("session-abc"),
            json!({
                "type": "assistant/chunk",
                "time": 1_786_930_276_802_i64,
                "data": {
                    "turn": 1,
                    "step": 1,
                    "chunk": { "type": "usage", "usage": { "inputTokens": 42, "outputTokens": 8 } }
                }
            }),
        ];

        let session = write_session(&lines);
        let messages = parse_dsh_file(session.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, UNKNOWN_MODEL);
        assert_eq!(messages[0].provider_id, CLIENT_ID);
        assert_eq!(messages[0].tokens.total(), 50);
    }

    #[test]
    fn reads_the_workspace_from_the_session_header() {
        let mut lines = vec![session_header("session-abc")];
        lines.extend(usage_step(
            1,
            1,
            json!({ "inputTokens": 1, "outputTokens": 1 }),
            "claude-fable-5",
        ));

        let session = write_session(&lines);
        let messages = parse_dsh_file(session.path());

        assert_eq!(messages[0].workspace_label.as_deref(), Some("proj"));
    }

    #[test]
    fn ignores_malformed_lines_and_non_zstd_files() {
        // Raw (uncompressed) JSONL is not a zstd frame, so it yields nothing
        // rather than panicking.
        let plain = NamedTempFile::new().unwrap();
        assert!(parse_dsh_file(plain.path()).is_empty());

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"not json\n{}\n").unwrap();
        file.flush().unwrap();
        assert!(parse_dsh_file(file.path()).is_empty());
    }

    #[test]
    fn ignores_chunks_that_are_not_usage_records() {
        // A model-naming chunk on its own carries no tokens.
        let lines = vec![
            session_header("session-abc"),
            json!({
                "type": "assistant/chunk",
                "data": {
                    "turn": 1,
                    "step": 1,
                    "chunk": { "type": "finish", "replayState": { "model": "gpt-5.5" } }
                }
            }),
        ];

        let session = write_session(&lines);
        assert!(parse_dsh_file(session.path()).is_empty());
    }
}
