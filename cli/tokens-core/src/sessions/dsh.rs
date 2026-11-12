//! DeepSeek Harness (DSH) session parser
//!
//! DSH persists one JSONL transcript per session under
//! `<DSH_HOME>/sessions/<encoded-cwd>/<session-id>/session.jsonl.zstd`
//! (`DSH_HOME` defaults to `~/.dsh`). The `.zstd` suffix marks the physical
//! encoding only: a backend configured with `compression: none` writes the
//! same rows to a plain `session.jsonl` in the same directory, so this parser
//! dispatches on the zstd frame magic rather than on the file name.
//!
//! The transcript is an append-only event stream; the rows Tokens needs are:
//!
//! - `session`: session id, `createdAt` (ms), `cwd` (workspace root), and the
//!   `seedLength` fork boundary.
//! - `request/header`: the provider/model the request was routed to (fallback
//!   for messages whose `source` is absent).
//! - `assistant/message`: authoritative per-call usage on `data.usage`
//!   (`inputTokens`, `outputTokens`, `cacheReadTokens`, ...) plus the serving
//!   provider and model on `data.message.source`. `source.model` is the model
//!   configured for the request; when the provider serves a different model,
//!   the response records its concrete identity on
//!   `source.replayState.response.responseModel`, which takes precedence.
//! - `compaction/summary`: the same usage and routing shape for the summarize
//!   call DSH makes when it compacts a range. Real spend on the same account,
//!   and disjoint from the loop steps around it.
//!
//! DSH never embeds a cost, so every message leaves the parser at `0.0` and
//! pricing is its only cost source — the generic source cache is safe here.

use super::{workspace_label_from_key, UnifiedMessage};
use crate::TokenBreakdown;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use tracing::warn;

/// Zstandard frame magic number (RFC 8478 section 3.1.1).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Buffer for the streaming reader, on both the decoded and the plain side.
const ZSTD_CHUNK_BYTES: usize = 128 * 1024;

/// Ceiling on one transcript line, which is all this parser ever holds.
///
/// The transcript is streamed line by line, so its total size no longer
/// decides how much is allocated — a line does. Zstandard's expansion ratio is
/// unbounded (a long run of one byte compresses to almost nothing), so a few
/// kilobytes of frames can decode to one gigantic line; this ceiling is
/// enforced while the line is being assembled, never on a finished buffer, so
/// a high-ratio frame cannot materialise before it is refused.
///
/// A line past it is skipped on its own and the rest of the transcript still
/// counts (#119). The rows Tokens reads are single API calls; what grows past
/// this is a tool result or similar payload, which carries no usage. 64 MiB is
/// the ceiling the whole transcript used to answer to, so no line that parsed
/// before is refused now, and the per-worker memory bound is unchanged.
const MAX_TRANSCRIPT_LINE_BYTES: usize = 64 * 1024 * 1024;

/// Ceiling on the decoded bytes walked for one transcript.
///
/// Streaming bounds memory, not work: an expansion bomb of a few megabytes on
/// disk can decode to terabytes, and the scan would grind through all of it.
/// This budget is a work bound, far above any real transcript (single-digit to
/// tens of MiB). A transcript that exhausts it keeps the records read so far
/// and is reported with a warning, rather than discarded whole.
const MAX_TRANSCRIPT_WALK_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// What a [`TranscriptLines`] walk did not hand to the parser.
#[derive(Debug, Default, PartialEq, Eq)]
struct TranscriptWalkGaps {
    /// Lines skipped because they ran past the per-line ceiling.
    oversized_lines: usize,
    /// The walk stopped at the decoded byte budget before the end of input.
    budget_exhausted: bool,
}

/// Bounded line iterator over a DSH transcript, decoding zstd frames when the
/// payload carries them.
///
/// A live DSH session appends one zstd frame per flush, so a scan racing a
/// writer routinely sees a torn trailing frame. DSH itself treats that as
/// expected and recovers the complete frames plus whatever prefix of the torn
/// one decodes (`session-persistence-jsonl/src/index.ts`, `readZstdPrefix`):
/// a decode error therefore ends the walk quietly, keeping every line before
/// it, and the torn final record fails to parse on its own.
///
/// Lines are decoded lossily, the terminator (`\n`, optional `\r`) and a
/// leading BOM are stripped, and a final line without a newline is still
/// yielded — the same contract as `utils::lossy_lines`, plus the ceilings.
struct TranscriptLines {
    reader: Box<dyn BufRead>,
    max_line_bytes: usize,
    max_walk_bytes: u64,
    walked: u64,
    line: Vec<u8>,
    at_start: bool,
    done: bool,
    gaps: TranscriptWalkGaps,
}

impl TranscriptLines {
    /// Open a transcript under the production ceilings. A missing or
    /// unreadable file yields no lines: a scan races session deletion
    /// routinely, and the pre-existing behaviour for it is silence.
    fn open(path: &Path) -> Option<Self> {
        Self::open_bounded(path, MAX_TRANSCRIPT_LINE_BYTES, MAX_TRANSCRIPT_WALK_BYTES)
    }

    /// Split from [`TranscriptLines::open`] so the ceiling tests can drive
    /// caps small enough to reach in a test.
    fn open_bounded(path: &Path, max_line_bytes: usize, max_walk_bytes: u64) -> Option<Self> {
        let mut file = std::fs::File::open(path).ok()?;
        // Sniff the frame magic without assuming one read returns it whole.
        let mut head = Vec::with_capacity(ZSTD_MAGIC.len());
        (&mut file)
            .take(ZSTD_MAGIC.len() as u64)
            .read_to_end(&mut head)
            .ok()?;
        let is_zstd = head == ZSTD_MAGIC;
        let source = std::io::Cursor::new(head).chain(file);
        // `compression: none` writes the same rows uncompressed, so those
        // bytes are already the decoded transcript.
        let reader: Box<dyn BufRead> = if is_zstd {
            let decoder = zstd::stream::read::Decoder::new(source).ok()?;
            Box::new(BufReader::with_capacity(ZSTD_CHUNK_BYTES, decoder))
        } else {
            Box::new(BufReader::with_capacity(ZSTD_CHUNK_BYTES, source))
        };
        Some(Self {
            reader,
            max_line_bytes,
            max_walk_bytes,
            walked: 0,
            line: Vec::new(),
            at_start: true,
            done: false,
            gaps: TranscriptWalkGaps::default(),
        })
    }

    /// Assemble the next line into `self.line`. Returns `None` at the end of
    /// input, `Some(false)` for a line skipped past the ceiling, and
    /// `Some(true)` for a line held in full.
    fn read_line(&mut self) -> Option<bool> {
        self.line.clear();
        let mut read_any = false;
        let mut oversized = false;
        loop {
            let available = match self.reader.fill_buf() {
                Ok(available) => available,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                // Torn trailing frame (or foreign payload): keep the prefix.
                Err(_) => {
                    self.done = true;
                    break;
                }
            };
            if available.is_empty() {
                self.done = true;
                break;
            }
            let budget = self.max_walk_bytes.saturating_sub(self.walked);
            if budget == 0 {
                // A partial line cut by the budget is not a record; drop it.
                self.done = true;
                self.gaps.budget_exhausted = true;
                return None;
            }
            let window = &available[..available.len().min(budget as usize)];
            let newline = window.iter().position(|&byte| byte == b'\n');
            let payload = &window[..newline.unwrap_or(window.len())];
            if !oversized {
                let needed = self.line.len() + payload.len();
                if needed > self.max_line_bytes {
                    // Never hold more than the ceiling: drop what was kept
                    // and discard the rest of this line as it streams past.
                    oversized = true;
                    self.line = Vec::new();
                } else {
                    // Amortized growth, but never reserving past the ceiling.
                    if needed > self.line.capacity() {
                        let target = needed
                            .max(self.line.capacity().saturating_mul(2))
                            .min(self.max_line_bytes);
                        self.line.reserve_exact(target - self.line.len());
                    }
                    self.line.extend_from_slice(payload);
                }
            }
            let consumed = newline.map_or(window.len(), |index| index + 1);
            self.reader.consume(consumed);
            self.walked += consumed as u64;
            read_any = true;
            if newline.is_some() {
                break;
            }
        }
        if !read_any {
            return None;
        }
        if oversized {
            self.gaps.oversized_lines += 1;
            return Some(false);
        }
        Some(true)
    }
}

impl Iterator for TranscriptLines {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        while !self.done {
            match self.read_line()? {
                false => continue,
                true => {
                    if self.line.last() == Some(&b'\r') {
                        self.line.pop();
                    }
                    let bom = "\u{feff}".as_bytes();
                    let start = if std::mem::take(&mut self.at_start) && self.line.starts_with(bom)
                    {
                        bom.len()
                    } else {
                        0
                    };
                    return Some(String::from_utf8_lossy(&self.line[start..]).into_owned());
                }
            }
        }
        None
    }
}

/// Parse one DSH `session.jsonl.zstd` transcript into unified messages.
///
/// Each `assistant/message` event with a non-zero `data.usage` becomes one
/// [`UnifiedMessage`]. Messages without usable timestamps are skipped; usage
/// with a zero total is skipped so noise rows (e.g. echoed tool-call-only
/// messages) do not produce zero-token contributions.
pub fn parse_dsh_file(path: &Path) -> Vec<UnifiedMessage> {
    let Some(mut lines) = TranscriptLines::open(path) else {
        return Vec::new();
    };

    // The transcript directory is named after the session id; it is the
    // fallback when the leading `session` event is missing.
    let session_id_from_path = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("unknown")
        .to_string();

    let mut session_id: Option<String> = None;
    let mut workspace_key: Option<String> = None;
    // Fork boundary: how many leading events this session inherited verbatim
    // from its parent. Zero for a session that was never forked.
    let mut seed_length: i64 = 0;
    // Most recent request routing, used when a message lacks its own `source`.
    let mut fallback_provider: Option<String> = None;
    let mut fallback_model: Option<String> = None;

    let mut messages = Vec::new();
    let mut seen = HashSet::new();
    // Turn numbers that already emitted a turn-start message.
    let mut turn_started: HashSet<i64> = HashSet::new();
    // Fallback turn-start marker for transcripts without turn numbers: a
    // `user/message` arms the next assistant message as a turn start.
    let mut pending_user_turn = false;

    for line in lines.by_ref() {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(event_type) = value.get("type").and_then(Value::as_str) else {
            continue;
        };
        match event_type {
            "session" => {
                session_id = value.get("id").and_then(Value::as_str).map(str::to_string);
                workspace_key = value.get("cwd").and_then(Value::as_str).map(str::to_string);
                seed_length = value
                    .get("seedLength")
                    .and_then(Value::as_i64)
                    .filter(|length| *length > 0)
                    .unwrap_or(0);
            }
            "request/header" => {
                let config = value.pointer("/data/header/config");
                fallback_provider = config
                    .and_then(|c| c.get("provider"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                fallback_model = config
                    .and_then(|c| c.get("model"))
                    .and_then(non_empty_string)
                    .map(str::to_string);
            }
            "user/message" => {
                pending_user_turn = true;
            }
            // A compaction summary is a real provider call, not bookkeeping:
            // DSH sends the shadowed range to the model and persists what that
            // call spent on `data.usage`, with the routing fields in the same
            // place as on an assistant message. It is not a loop step and
            // shares no `(turn, step)` with one, so it is counted in addition
            // to the messages around it rather than replacing any of them.
            // Falling through to `_` billed those calls at zero (#1152).
            "assistant/message" | "compaction/summary" => {
                let is_summary = event_type == "compaction/summary";
                // Fork/continuation ownership boundary. Forking copies the
                // parent's completed prefix into the child transcript verbatim
                // — same `seq`, `time`, `usage` and `message.id` — and records
                // how many events were inherited as the header's `seedLength`
                // (`core/session/src/index.ts`, `SessionStore::fork`). Only
                // events at or after that boundary are this session's own work;
                // counting the seed again bills the parent's calls twice.
                if seed_length > 0
                    && value
                        .get("seq")
                        .and_then(Value::as_i64)
                        .is_some_and(|seq| seq < seed_length)
                {
                    continue;
                }
                let Some(usage) = value.pointer("/data/usage") else {
                    continue;
                };
                let tokens = tokens_from_usage(usage);
                if tokens.total() == 0 {
                    continue;
                }
                let Some(timestamp) = value.get("time").and_then(Value::as_i64) else {
                    continue;
                };
                if timestamp <= 0 {
                    continue;
                }

                let source = value.pointer("/data/message/source");
                let model_id = served_model(source)
                    .or(fallback_model.as_deref())
                    .unwrap_or("unknown")
                    .to_string();
                let provider_id = source
                    .and_then(|s| s.get("provider"))
                    .and_then(Value::as_str)
                    .or(fallback_provider.as_deref())
                    .unwrap_or("unknown")
                    .to_string();

                let sid = session_id
                    .clone()
                    .unwrap_or_else(|| session_id_from_path.clone());

                // A summary is not a loop step, so it neither claims a turn
                // start nor consumes the marker a `user/message` armed for the
                // next assistant reply — taking it here would hand the turn to
                // the summary and leave the real reply looking like a
                // continuation.
                let is_turn_start = if is_summary {
                    false
                } else {
                    let turn = value.pointer("/data/turn").and_then(Value::as_i64);
                    match turn {
                        Some(turn) => turn_started.insert(turn),
                        None => std::mem::take(&mut pending_user_turn),
                    }
                };

                // `data.message.id` is a per-call `crypto.randomUUID()`
                // (`llm/llm/src/message.ts`) that a fork copies verbatim, so
                // scoping the key to it instead of the session id collapses a
                // seeded copy against the parent's original even when the two
                // live in different files under different session ids — the
                // seq boundary above only fires for headers that actually
                // carry `seedLength`. The rest of the identity stays in the
                // key: a sanitized or otherwise non-unique id then still
                // separates calls that differ in time, routing or usage
                // instead of silently folding them into one.
                // Falling back to the session id breaks the very case the
                // paragraph above describes. A `compaction/summary` carries no
                // `message.id`, so a fork that lost its `seedLength` copies the
                // summary into a file with a *different* session id, the two
                // keys differ, and the cross-file pass -- which only collapses
                // identical keys -- bills the summarize call twice.
                //
                // Summaries carry their own per-call UUID on
                // `data.compactionId`, and a fork copies it verbatim. Prefer it
                // before `seq`: sequence numbers restart in every transcript,
                // so unrelated summaries can otherwise collapse when their
                // timestamp, routing and usage happen to agree (#1187).
                //
                // Keep `seq` as the final cross-file fallback for older or
                // damaged summaries without a compaction id. It remains better
                // than the session id for the seedLength-less fork handled by
                // #1173, because the fork copies the sequence number too.
                let identity = value
                    .pointer("/data/message/id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(|id| format!("msg:{id}"))
                    .or_else(|| {
                        if !is_summary {
                            return None;
                        }
                        value
                            .pointer("/data/compactionId")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|id| !id.is_empty())
                            .map(|id| format!("cmp:{id}"))
                    })
                    .or_else(|| {
                        value
                            .get("seq")
                            .and_then(Value::as_i64)
                            .map(|seq| format!("seq:{seq}"))
                    })
                    .unwrap_or_else(|| format!("sid:{sid}"));
                // Namespace the summary so it can never collapse against a loop
                // step: a summary carries no `message.id` of its own, so both
                // fall back to `sid:` and a summary that happened to match a
                // reply's timestamp, routing and buckets would otherwise be
                // dropped as a duplicate of it.
                let kind = if is_summary { "summary:" } else { "" };
                let dedup_key = format!(
                    "dsh:{kind}{identity}:{timestamp}:{provider_id}:{model_id}:{}:{}:{}:{}:{}",
                    tokens.input,
                    tokens.output,
                    tokens.cache_read,
                    tokens.cache_write,
                    tokens.reasoning
                );
                if !seen.insert(dedup_key.clone()) {
                    continue;
                }

                let mut message = UnifiedMessage::new_with_dedup(
                    "dsh",
                    model_id,
                    provider_id,
                    &sid,
                    timestamp,
                    tokens,
                    0.0,
                    Some(dedup_key),
                );
                message.is_turn_start = is_turn_start;
                if let Some(cwd) = &workspace_key {
                    if let Some(key) = super::normalize_workspace_key(cwd) {
                        let label = workspace_label_from_key(&key);
                        message.set_workspace(Some(key), label);
                    }
                }
                messages.push(message);
            }
            _ => {}
        }
    }

    // A gap costs only the lines it covers, but the count is then a lower
    // bound for this transcript; say so rather than pass it off as complete.
    if lines.gaps.oversized_lines > 0 {
        warn!(
            path = %path.display(),
            skipped_lines = lines.gaps.oversized_lines,
            line_ceiling_bytes = lines.max_line_bytes,
            "Skipped oversized DSH transcript lines; usage on them is not counted"
        );
    }
    if lines.gaps.budget_exhausted {
        warn!(
            path = %path.display(),
            walk_ceiling_bytes = lines.max_walk_bytes,
            "DSH transcript decodes past the walk ceiling; only its prefix is counted"
        );
    }

    messages
}

/// Return the concrete model that served a DSH call.
///
/// `source.model` is the configured request model. Floating aliases and
/// provider-side substitutions can resolve to another model, which pi-ai
/// records as `replayState.response.responseModel` when it differs from the
/// request. That response identity is authoritative for attribution, pricing,
/// and the model slot in the dedup key. Rows without it keep the configured
/// source model, then the caller falls back to the latest request header.
fn served_model(source: Option<&Value>) -> Option<&str> {
    source
        .and_then(|value| value.pointer("/replayState/response/responseModel"))
        .and_then(non_empty_string)
        .or_else(|| {
            source
                .and_then(|value| value.get("model"))
                .and_then(non_empty_string)
        })
}

fn non_empty_string(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Split DSH's usage row into Tokens's five additive buckets.
///
/// DSH documents `TokenUsage` as disjoint on the input side — `inputTokens` is
/// uncached input only, with cache hits reported separately, and the DeepSeek
/// adapter subtracts them out of `prompt_tokens` before persisting
/// (`llm/llm/src/types.ts`, `llm-deepseek/src/translate.ts`). `reasoningTokens`
/// is the exception: it is `completion_tokens_details.reasoning_tokens`, a
/// SUBSET of the `completion_tokens` that becomes `outputTokens`, which is why
/// DSH's own token meter sums input + cache + output and omits reasoning
/// entirely (`llm/token-meter/src/index.ts`, `usageTokens`).
///
/// [`TokenBreakdown`] buckets are additive and pricing bills `output` and
/// `reasoning` at the same output rate, so mapping both fields through would
/// bill every reasoning token twice. Subtract the overlap, as `senpi.rs`,
/// `grok.rs` and `zcode.rs` do for the same shape.
fn tokens_from_usage(usage: &Value) -> TokenBreakdown {
    let output = int_field(usage, "outputTokens").max(0);
    // Normalize before pricing, deduplication and aggregation: clamping only
    // the final breakdown leaves totals containing the negative counts.
    // Reasoning is a subset of output, even when a malformed row says otherwise.
    let reasoning = int_field(usage, "reasoningTokens").clamp(0, output);
    TokenBreakdown {
        input: int_field(usage, "inputTokens").max(0),
        output: output - reasoning,
        cache_read: int_field(usage, "cacheReadTokens").max(0),
        cache_write: int_field(usage, "cacheWriteTokens").max(0),
        reasoning,
    }
}

fn int_field(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_zstd_session(lines: &[&str]) -> tempfile::NamedTempFile {
        let payload = lines.join("\n");
        let compressed = zstd::encode_all(payload.as_bytes(), 3).unwrap();
        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, &compressed).unwrap();
        file
    }

    /// Ceilings the bound tests drive. Small on purpose: the production
    /// constants would have to be decoded in full before they proved
    /// anything, and the code under test is the same either way.
    const TEST_LINE_CAP: usize = 64 * 1024;
    const TEST_WALK_CAP: u64 = 4 * 1024 * 1024;

    /// A transcript that is trivial on disk and enormous once decoded: one
    /// newline-free run of a single byte, repeated frame after frame, so the
    /// whole payload is one line. Returns the file and its on-disk size.
    fn write_high_expansion_zstd(decoded_mib: usize) -> (tempfile::NamedTempFile, usize) {
        let frame = zstd::encode_all(vec![b'a'; 1024 * 1024].as_slice(), 3).unwrap();
        let mut payload = Vec::with_capacity(frame.len() * decoded_mib);
        for _ in 0..decoded_mib {
            payload.extend_from_slice(&frame);
        }
        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, &payload).unwrap();
        let on_disk = payload.len();
        (file, on_disk)
    }

    fn usage_row(seq: i64, input: i64, output: i64) -> String {
        format!(
            r#"{{"type":"assistant/message","seq":{seq},"time":{},"data":{{"turn":{seq},"message":{{"id":"synthetic-message-{seq}","source":{{"provider":"deepseek","model":"deepseek-v4.1-flash"}}}},"usage":{{"inputTokens":{input},"outputTokens":{output}}}}}}}"#,
            1_793_851_200_000 + seq
        )
    }

    #[test]
    fn an_oversized_line_is_skipped_without_dropping_its_neighbours() {
        // given: plain JSONL whose middle row runs past the line ceiling.
        let dir = tempfile::tempdir().unwrap();
        let session_dir = dir.path().join("session-oversized");
        std::fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("session.jsonl");
        let body = [
            r#"{"type":"session","id":"session-oversized","createdAt":1,"cwd":"/work"}"#
                .to_string(),
            usage_row(1, 10, 2),
            format!(
                r#"{{"type":"tool/result","seq":2,"time":1,"data":{{"content":"{}"}}}}"#,
                "x".repeat(TEST_LINE_CAP * 4)
            ),
            usage_row(3, 5, 3),
        ]
        .join("\n");
        std::fs::write(&path, &body).unwrap();

        // when
        let mut lines = TranscriptLines::open_bounded(&path, TEST_LINE_CAP, TEST_WALK_CAP).unwrap();
        let held: Vec<String> = lines.by_ref().collect();

        // then: only the oversized row is missing, and it is accounted for.
        assert_eq!(held.len(), 3);
        assert!(held.iter().all(|line| line.len() <= TEST_LINE_CAP));
        assert_eq!(
            lines.gaps,
            TranscriptWalkGaps {
                oversized_lines: 1,
                budget_exhausted: false,
            }
        );
    }

    #[test]
    fn a_high_expansion_zstd_transcript_stays_bounded() {
        // given: 64 MiB of one newline-free line carried by a few kilobytes of
        // frames. Bounding the compressed read cannot catch this: the ratio is
        // the number the file controls, so the ceilings hold on the output.
        const DECODED_MIB: usize = 64;
        let (file, on_disk) = write_high_expansion_zstd(DECODED_MIB);
        assert!(on_disk < 64 * 1024, "the payload must be tiny on disk");

        // when
        let mut lines =
            TranscriptLines::open_bounded(file.path(), TEST_LINE_CAP, TEST_WALK_CAP).unwrap();
        let held: Vec<String> = lines.by_ref().collect();

        // then: nothing past the line ceiling was ever held, and the walk
        // stopped at its budget rather than decoding all 64 MiB.
        assert!(held.is_empty());
        assert_eq!(
            lines.gaps.oversized_lines, 0,
            "a line cut by the budget is not counted"
        );
        assert!(lines.gaps.budget_exhausted);
        assert_eq!(lines.walked, TEST_WALK_CAP);
        assert!(parse_dsh_file(file.path()).is_empty());
    }

    #[test]
    fn usage_around_a_payload_past_64_mib_decoded_is_counted() {
        // given: the #119 shape — usage before and after 70 MiB of tool-result
        // text, one zstd frame per row the way DSH flushes. Under the old
        // whole-transcript ceiling this counted zero tokens.
        let filler = format!(
            "{}\n",
            r#"{"type":"tool/result","seq":2,"time":1793851200000,"data":{"message":{"content":"PAD"}}}"#
                .replace("PAD", &"x".repeat(1024 * 1024))
        );
        let filler_frame = zstd::encode_all(filler.as_bytes(), 1).unwrap();
        let frame = |row: String| zstd::encode_all(format!("{row}\n").as_bytes(), 3).unwrap();

        let mut payload = frame(
            r#"{"type":"session","id":"synthetic-cap-repro","createdAt":1793851200000,"cwd":"/synthetic-fixture"}"#
                .to_string(),
        );
        payload.extend(frame(usage_row(1, 10, 2)));
        let mut decoded = 0usize;
        while decoded <= 70 * 1024 * 1024 {
            payload.extend_from_slice(&filler_frame);
            decoded += filler.len();
        }
        payload.extend(frame(usage_row(100, 5, 3)));
        assert!(decoded > 64 * 1024 * 1024);

        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, &payload).unwrap();

        // when
        let messages = parse_dsh_file(file.path());

        // then: both usage rows survive, matching the small control.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].session_id, "synthetic-cap-repro");
        let total: i64 = messages.iter().map(|m| m.tokens.total()).sum();
        assert_eq!(total, 20);
    }

    #[test]
    fn a_normal_transcript_is_read_line_for_line_under_the_ceilings() {
        // The ceilings must be invisible to every transcript that fits: the
        // lines handed to the parser are the rows written, in both spellings.
        let rows = [
            r#"{"type":"session","id":"session-normal","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"assistant/message","time":1786669454772,"data":{"turn":1,"message":{"id":"m-1","source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20}}}"#,
        ];

        let compressed = write_zstd_session(&rows);
        let held: Vec<String> = TranscriptLines::open(compressed.path()).unwrap().collect();
        assert_eq!(held, rows);

        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("session.jsonl");
        std::fs::write(&plain, format!("\u{feff}{}\r\n{}\n", rows[0], rows[1])).unwrap();
        let held: Vec<String> = TranscriptLines::open(&plain).unwrap().collect();
        assert_eq!(held, rows);

        // and: the transcript still parses to the same message it always did.
        let messages = parse_dsh_file(compressed.path());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].session_id, "session-normal");
        assert_eq!(messages[0].tokens.input, 10);
        assert_eq!(messages[0].tokens.output, 20);
    }

    #[test]
    fn parses_assistant_messages_with_usage() {
        let file = write_zstd_session(&[
            r#"{"type":"session","version":0,"id":"session-abc","createdAt":1786669406484,"cwd":"E:\\repo\\proj","delegationDepth":0,"agentPreset":"cordis"}"#,
            r#"{"type":"turn/start","seq":4,"time":1786669450000,"data":{"turn":1}}"#,
            r#"{"type":"user/message","seq":7,"time":1786669450001,"data":{"turn":1}}"#,
            r#"{"type":"assistant/message","seq":301,"time":1786669454772,"data":{"turn":1,"step":1,"message":{"role":"assistant","content":[],"source":{"kind":"model","provider":"irix","model":"deepseek-v4-flash"}},"usage":{"inputTokens":130,"outputTokens":159,"cacheReadTokens":13824}}}"#,
            r#"{"type":"assistant/message","seq":414,"time":1786669459063,"data":{"turn":1,"step":2,"message":{"role":"assistant","content":[],"source":{"kind":"model","provider":"irix","model":"deepseek-v4-flash"}},"usage":{"inputTokens":130,"outputTokens":159,"cacheReadTokens":13824}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());
        assert_eq!(messages.len(), 2);

        let first = &messages[0];
        assert_eq!(first.client, "dsh");
        assert_eq!(first.model_id, "deepseek-v4-flash");
        assert_eq!(first.provider_id, "irix");
        assert_eq!(first.session_id, "session-abc");
        assert_eq!(first.timestamp, 1786669454772);
        assert_eq!(first.tokens.input, 130);
        assert_eq!(first.tokens.output, 159);
        assert_eq!(first.tokens.cache_read, 13824);
        assert_eq!(first.tokens.cache_write, 0);
        assert_eq!(first.tokens.reasoning, 0);
        assert_eq!(first.cost, 0.0);
        assert!(first.is_turn_start);
        assert_eq!(first.workspace_key.as_deref(), Some("E:/repo/proj"));
        assert_eq!(first.workspace_label.as_deref(), Some("proj"));
        // This row carries no `message.id`, so the key falls back to `seq`
        // rather than the session id: a fork copies `seq` verbatim, so the key
        // survives the copy and the cross-file pass can still collapse the two.
        assert_eq!(
            first.dedup_key.as_deref(),
            Some("dsh:seq:301:1786669454772:irix:deepseek-v4-flash:130:159:13824:0:0")
        );

        // Same turn, later step: not a turn start.
        assert!(!messages[1].is_turn_start);
    }

    #[test]
    fn attributes_usage_to_the_model_reported_by_the_provider() {
        // Real DSH/pi-ai response shape: the session requested glm-5.2, while
        // the provider returned glm-5.3 and pi-ai preserved that substitution
        // in replayState.response.responseModel.
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-served","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"assistant/message","seq":42,"time":1787122684043,"data":{"turn":1,"message":{"id":"m-served","source":{"kind":"model","provider":"zai-coding-cn","model":"glm-5.2","replayState":{"response":{"kind":"pi-ai","version":2,"api":"openai-completions","provider":"zai-coding-cn","model":"glm-5.2","responseModel":"glm-5.3","stopReason":"toolUse"}}}},"usage":{"inputTokens":8425,"outputTokens":207,"cacheReadTokens":576}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, "glm-5.3");
        assert_eq!(messages[0].provider_id, "zai-coding-cn");
        assert_eq!(
            messages[0].dedup_key.as_deref(),
            Some("dsh:msg:m-served:1787122684043:zai-coding-cn:glm-5.3:8425:207:576:0:0")
        );
    }

    #[test]
    fn resolves_a_floating_request_alias_to_its_concrete_served_model() {
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-alias","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"assistant/message","time":1787122684043,"data":{"turn":1,"message":{"id":"m-alias","source":{"kind":"model","provider":"openrouter","model":"~x-ai/grok-latest","replayState":{"response":{"kind":"pi-ai","version":2,"api":"openai-completions","provider":"openrouter","model":"~x-ai/grok-latest","responseModel":"x-ai/grok-4.6","stopReason":"stop"}}}},"usage":{"inputTokens":100,"outputTokens":20}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, "x-ai/grok-4.6");
        assert_eq!(messages[0].provider_id, "openrouter");
    }

    #[test]
    fn served_model_uses_only_non_empty_string_response_values() {
        for (source, expected) in [
            (
                serde_json::json!({
                    "model": " configured ",
                    "replayState": { "response": { "responseModel": " served " } }
                }),
                Some("served"),
            ),
            (
                serde_json::json!({
                    "model": " configured ",
                    "replayState": { "response": { "responseModel": "   " } }
                }),
                Some("configured"),
            ),
            (
                serde_json::json!({
                    "model": " configured ",
                    "replayState": { "response": { "responseModel": 123 } }
                }),
                Some("configured"),
            ),
            (serde_json::json!({ "model": "   " }), None),
        ] {
            assert_eq!(served_model(Some(&source)), expected);
        }
        assert_eq!(served_model(None), None);
    }

    #[test]
    fn counts_compaction_summary_usage() {
        // The summarize call DSH makes when it compacts a range. Real spend on
        // the same account, disjoint from the loop steps around it (#1152).
        let file = write_zstd_session(&[
            r#"{"type":"session","version":0,"id":"session-abc","createdAt":1786669406484,"cwd":"/work"}"#,
            r#"{"type":"user/message","seq":7,"time":1786669450001,"data":{"turn":1}}"#,
            r#"{"type":"assistant/message","seq":301,"time":1786669454772,"data":{"turn":1,"step":1,"message":{"source":{"provider":"minimax-cn","model":"MiniMax-M3"}},"usage":{"inputTokens":130,"outputTokens":159,"cacheReadTokens":13824}}}"#,
            r#"{"type":"compaction/summary","seq":402,"time":1786669470000,"data":{"message":{"source":{"provider":"minimax-cn","model":"MiniMax-M3"}},"usage":{"inputTokens":536,"outputTokens":2436,"cacheReadTokens":41472}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());
        assert_eq!(messages.len(), 2);

        let summary = &messages[1];
        assert_eq!(summary.model_id, "MiniMax-M3");
        assert_eq!(summary.provider_id, "minimax-cn");
        assert_eq!(summary.session_id, "session-abc");
        assert_eq!(summary.timestamp, 1786669470000);
        assert_eq!(summary.tokens.input, 536);
        assert_eq!(summary.tokens.output, 2436);
        assert_eq!(summary.tokens.cache_read, 41472);
        // The 44,444-token summarize call from #1152, counted in addition to
        // the reply rather than replacing it.
        assert_eq!(summary.tokens.total(), 44_444);
        assert_eq!(summary.workspace_key.as_deref(), Some("/work"));

        // A summary is not a loop step, so it never claims the turn.
        assert!(messages[0].is_turn_start);
        assert!(!summary.is_turn_start);
    }

    #[test]
    fn compaction_summary_uses_the_concrete_served_model() {
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-summary-model","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"compaction/summary","seq":8,"time":1786669450002,"data":{"message":{"source":{"provider":"openrouter","model":"~x-ai/grok-latest","replayState":{"response":{"responseModel":"x-ai/grok-4.6"}}}},"usage":{"inputTokens":10,"outputTokens":20}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, "x-ai/grok-4.6");
        assert!(messages[0]
            .dedup_key
            .as_deref()
            .is_some_and(|key| key.contains(":openrouter:x-ai/grok-4.6:")));
    }

    #[test]
    fn compaction_summary_does_not_steal_the_turn_start() {
        // A summary landing between the user's prompt and the reply it
        // precedes must leave the turn marker for the reply — under both the
        // numbered-turn and the `user/message`-armed fallback paths.
        let file = write_zstd_session(&[
            r#"{"type":"session","version":0,"id":"session-abc","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"user/message","seq":7,"time":1786669450001,"data":{"turn":2}}"#,
            r#"{"type":"compaction/summary","seq":8,"time":1786669450002,"data":{"turn":2,"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20}}}"#,
            r#"{"type":"assistant/message","seq":9,"time":1786669450003,"data":{"turn":2,"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":30,"outputTokens":40}}}"#,
            r#"{"type":"user/message","seq":10,"time":1786669450004,"data":{}}"#,
            r#"{"type":"compaction/summary","seq":11,"time":1786669450005,"data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":50,"outputTokens":60}}}"#,
            r#"{"type":"assistant/message","seq":12,"time":1786669450006,"data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":70,"outputTokens":80}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());
        assert_eq!(messages.len(), 4);

        let turn_starts: Vec<bool> = messages.iter().map(|m| m.is_turn_start).collect();
        // summary, reply, summary, reply — only the replies begin a turn.
        assert_eq!(turn_starts, vec![false, true, false, true]);
    }

    #[test]
    fn compaction_summary_without_usage_is_skipped() {
        // `usage` is optional on the event; an absent value has to stay absent
        // rather than becoming a zero-token contribution.
        let file = write_zstd_session(&[
            r#"{"type":"session","version":0,"id":"session-abc","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"compaction/summary","seq":8,"time":1786669450002,"data":{"shadowedTokenCount":19962}}"#,
            r#"{"type":"compaction/summary","seq":9,"time":1786669450003,"data":{"usage":{"inputTokens":0,"outputTokens":0}}}"#,
        ]);

        assert!(parse_dsh_file(file.path()).is_empty());
    }

    #[test]
    fn compaction_summary_inside_the_forked_seed_is_not_recounted() {
        // The seed prefix is the parent's work, verbatim. A summary inside it
        // is billed to the parent exactly like an assistant message is.
        let file = write_zstd_session(&[
            r#"{"type":"session","version":0,"id":"child","createdAt":1,"cwd":"/work","seedLength":10}"#,
            r#"{"type":"compaction/summary","seq":4,"time":1786669450002,"data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20}}}"#,
            r#"{"type":"compaction/summary","seq":11,"time":1786669450003,"data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":30,"outputTokens":40}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.input, 30);
    }

    #[test]
    fn negative_usage_keeps_daily_and_session_totals_consistent() {
        for event in ["assistant/message", "compaction/summary"] {
            let row = serde_json::json!({
                "type": event, "time": 1787184000000_i64,
                "data": {"usage": {
                    "inputTokens": -861899, "outputTokens": 75387,
                    "cacheReadTokens": 1562880
                }}
            })
            .to_string();
            let file = write_zstd_session(&[&row]);
            let messages = parse_dsh_file(file.path());
            assert_eq!(messages.len(), 1);
            let days = crate::aggregator::aggregate_by_date(messages.clone());
            let sessions = crate::aggregator::aggregate_by_session(messages);
            assert_eq!(days[0].totals.tokens, days[0].token_breakdown.total());
            assert_eq!(
                days[0].totals.tokens,
                days[0]
                    .clients
                    .iter()
                    .map(|c| c.tokens.total())
                    .sum::<i64>()
            );
            assert_eq!(
                sessions[0].totals.tokens,
                sessions[0].token_breakdown.total()
            );
            assert_eq!(days[0].totals.tokens, 1_638_267);
        }
    }

    #[test]
    fn malformed_usage_cannot_create_negative_buckets() {
        let tokens = tokens_from_usage(&serde_json::json!({
            "inputTokens": -1, "outputTokens": 20, "reasoningTokens": 30,
            "cacheReadTokens": -2, "cacheWriteTokens": -3
        }));
        assert_eq!(tokens.input, 0);
        assert_eq!(tokens.cache_read, 0);
        assert_eq!(tokens.cache_write, 0);
        assert_eq!(tokens.output, 0);
        assert_eq!(tokens.reasoning, 20);
        assert_eq!(tokens.total(), 20);
        let tokens = tokens_from_usage(&serde_json::json!({
            "outputTokens": -1, "reasoningTokens": -2
        }));
        assert_eq!(tokens.total(), 0);
    }

    #[test]
    fn supports_cache_write_and_reasoning_buckets() {
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-xyz","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"assistant/message","time":1786669454772,"data":{"turn":1,"message":{"source":{"provider":"deepseek","model":"deepseek-reasoner"}},"usage":{"inputTokens":10,"outputTokens":60,"cacheReadTokens":30,"cacheWriteTokens":40,"reasoningTokens":50}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());
        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        assert_eq!(msg.tokens.input, 10);
        // `reasoningTokens` is a subset of `outputTokens`, so the additive
        // output bucket keeps only the non-reasoning remainder.
        assert_eq!(msg.tokens.output, 10);
        assert_eq!(msg.tokens.cache_read, 30);
        assert_eq!(msg.tokens.cache_write, 40);
        assert_eq!(msg.tokens.reasoning, 50);
        assert_eq!(msg.model_id, "deepseek-reasoner");
        assert_eq!(msg.provider_id, "deepseek");
    }

    #[test]
    fn reasoning_tokens_do_not_inflate_the_additive_output_bucket() {
        // given: DSH's `outputTokens` is the provider's `completion_tokens`
        // and `reasoningTokens` is `completion_tokens_details.reasoning_tokens`
        // — a subset of it, which is why DSH's own token meter sums
        // input + cache + output and never adds reasoning. Tokens's buckets
        // are additive and pricing bills output and reasoning at the same
        // output rate, so mapping both fields through bills reasoning twice.
        // Numbers taken from a committed DSH transcript
        // (`examples/acp-agent/tests/snapshots/subagent-fork-in-process`).
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-reasoning","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"assistant/message","seq":39,"time":1785730448979,"data":{"turn":1,"message":{"id":"m-1","source":{"provider":"deepseek","model":"deepseek-reasoner"}},"usage":{"inputTokens":2885,"outputTokens":25,"cacheReadTokens":0,"reasoningTokens":23}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());

        assert_eq!(messages.len(), 1);
        let tokens = &messages[0].tokens;
        assert_eq!(tokens.output, 2);
        assert_eq!(tokens.reasoning, 23);
        // Mirrors DSH's own `usageTokens`: input + cacheRead + cacheWrite +
        // output, with reasoning already inside output.
        assert_eq!(tokens.total(), 2885 + 25);
    }

    #[test]
    fn reasoning_equal_to_output_leaves_a_non_zero_message() {
        // A reasoning-only completion (all output tokens were reasoning)
        // must survive the zero-usage filter with an empty output bucket.
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-all-reasoning","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"assistant/message","time":1786669454772,"data":{"turn":1,"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":0,"outputTokens":31,"reasoningTokens":31}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.output, 0);
        assert_eq!(messages[0].tokens.reasoning, 31);
        assert_eq!(messages[0].tokens.total(), 31);
    }

    #[test]
    fn falls_back_to_request_header_routing_and_folder_session_id() {
        let file = write_zstd_session(&[
            r#"{"type":"request/header","seq":11,"time":1786669450062,"data":{"header":{"config":{"provider":"irix","model":"deepseek-v4-flash"}}}}"#,
            // No `session` event and no `source` on the message: session id
            // comes from the folder, model/provider from the header.
            r#"{"type":"assistant/message","time":1786669454772,"data":{"turn":1,"message":{"role":"assistant","content":[]},"usage":{"inputTokens":5,"outputTokens":7}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());
        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        let folder = file
            .path()
            .parent()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str())
            .unwrap();
        assert_eq!(msg.session_id, folder);
        assert_eq!(msg.model_id, "deepseek-v4-flash");
        assert_eq!(msg.provider_id, "irix");
    }

    #[test]
    fn skips_zero_usage_and_missing_timestamp() {
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-zero","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"assistant/message","time":1786669454772,"data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":0,"outputTokens":0}}}"#,
            r#"{"type":"assistant/message","data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":1,"outputTokens":1}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());
        assert!(messages.is_empty());
    }

    #[test]
    fn dedups_identical_replayed_rows_within_a_file() {
        let line = r#"{"type":"assistant/message","time":1786669454772,"data":{"turn":1,"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20}}}"#;
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-dedup","createdAt":1,"cwd":"/work"}"#,
            line,
            line,
        ]);

        let messages = parse_dsh_file(file.path());
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn a_repeated_message_id_still_separates_calls_that_differ() {
        // The sanitized snapshots DSH commits redact `message.id` to a single
        // placeholder shared by every call in the file, so the id alone is not
        // a safe dedup key: the rest of the call identity has to stay in it or
        // distinct calls disappear.
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-placeholder","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"assistant/message","time":1786669454772,"data":{"turn":1,"message":{"id":"{{sessionId}}","source":{"provider":"p","model":"m"}},"usage":{"inputTokens":20,"outputTokens":8}}}"#,
            r#"{"type":"assistant/message","time":1786669455000,"data":{"turn":1,"message":{"id":"{{sessionId}}","source":{"provider":"p","model":"m"}},"usage":{"inputTokens":28,"outputTokens":2}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());

        assert_eq!(messages.len(), 2);
        assert_ne!(messages[0].dedup_key, messages[1].dedup_key);
    }

    #[test]
    fn skips_the_seeded_prefix_a_fork_inherits_from_its_parent() {
        // given: DSH forks by copying the parent's completed prefix into the
        // child transcript verbatim and recording its length as `seedLength`.
        // Both rows below are the real duplicated pair from
        // `examples/acp-agent/tests/snapshots/subagent-fork-in-process`:
        // the parent's seq-39 message reappears in the child under a different
        // session id with the same time, usage and message id.
        let parent = write_zstd_session(&[
            r#"{"type":"session","version":0,"id":"96cf59c9-b347-48b9-b234-a5200913ad05","createdAt":1783352134832,"cwd":"/work","delegationDepth":0}"#,
            r#"{"type":"assistant/message","seq":39,"time":1785730448979,"data":{"turn":1,"message":{"id":"7ac2e3d7-d558-4b24-b71e-40fc2f42216d","source":{"provider":"deepseek","model":"deepseek-reasoner"}},"usage":{"inputTokens":2885,"outputTokens":25,"cacheReadTokens":0,"reasoningTokens":23}}}"#,
        ]);
        let child = write_zstd_session(&[
            r#"{"type":"session","version":0,"id":"ada8966c-9fa3-441b-8721-37ff1e795e6a","createdAt":1783352137161,"cwd":"/work","parentSession":"96cf59c9-b347-48b9-b234-a5200913ad05","seedLength":42,"origin":"subagent","delegationDepth":1}"#,
            r#"{"type":"assistant/message","seq":39,"time":1785730448979,"data":{"turn":1,"message":{"id":"7ac2e3d7-d558-4b24-b71e-40fc2f42216d","source":{"provider":"deepseek","model":"deepseek-reasoner"}},"usage":{"inputTokens":2885,"outputTokens":25,"cacheReadTokens":0,"reasoningTokens":23}}}"#,
            r#"{"type":"assistant/message","seq":96,"time":1786358035361,"data":{"turn":2,"message":{"id":"cdc56e00-c648-4669-92b2-7299e41cb743","source":{"provider":"deepseek","model":"deepseek-reasoner"}},"usage":{"inputTokens":97,"outputTokens":39,"cacheReadTokens":2816,"reasoningTokens":34}}}"#,
        ]);

        // when
        let parent_messages = parse_dsh_file(parent.path());
        let child_messages = parse_dsh_file(child.path());

        // then: the child contributes only its own post-seed work.
        assert_eq!(parent_messages.len(), 1);
        assert_eq!(child_messages.len(), 1);
        assert_eq!(child_messages[0].timestamp, 1786358035361);
        assert_eq!(child_messages[0].tokens.input, 97);
    }

    #[test]
    fn a_copied_summary_shares_its_key_across_a_fork_that_lost_seedlength() {
        // The sibling test above covers the same shape for an assistant row,
        // which survives on its `message.id`. A `compaction/summary` has none,
        // so before the `seq` fallback the two files produced
        // `dsh:summary:sid:parent...` and `dsh:summary:sid:child...`; the
        // cross-file pass only collapses identical keys, so the summarize call
        // was billed twice.
        let row = r#"{"type":"compaction/summary","seq":4,"time":1786669450002,"data":{"compactionId":"1ad33c8f-5255-4158-b607-7555f3c26cd0","message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20}}}"#;
        let parent = write_zstd_session(&[
            r#"{"type":"session","id":"96cf59c9-b347-48b9-b234-a5200913ad05","createdAt":1,"cwd":"/work"}"#,
            row,
        ]);
        // No `seedLength`, so the seq boundary never fires and the copy is parsed.
        let child = write_zstd_session(&[
            r#"{"type":"session","id":"ada8966c-9fa3-441b-8721-37ff1e795e6a","createdAt":2,"cwd":"/work","parentSession":"96cf59c9-b347-48b9-b234-a5200913ad05"}"#,
            row,
        ]);

        let parent_messages = parse_dsh_file(parent.path());
        let child_messages = parse_dsh_file(child.path());

        assert_eq!(parent_messages.len(), 1);
        assert_eq!(child_messages.len(), 1);
        assert_ne!(
            parent_messages[0].session_id, child_messages[0].session_id,
            "the two files must be distinct sessions for this to test anything"
        );
        assert_eq!(
            parent_messages[0].dedup_key.as_deref(),
            Some(
                "dsh:summary:cmp:1ad33c8f-5255-4158-b607-7555f3c26cd0:1786669450002:p:m:10:20:0:0:0"
            )
        );
        assert_eq!(
            parent_messages[0].dedup_key, child_messages[0].dedup_key,
            "a copied summary must collapse across the fork, or its cost is counted twice"
        );
    }

    #[test]
    fn distinct_compaction_ids_keep_otherwise_identical_summaries() {
        let first = write_zstd_session(&[
            r#"{"type":"session","id":"session-a","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"compaction/summary","seq":4,"time":1786669450002,"data":{"compactionId":"compact-a","message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20,"cacheReadTokens":30}}}"#,
        ]);
        let second = write_zstd_session(&[
            r#"{"type":"session","id":"session-b","createdAt":2,"cwd":"/work"}"#,
            r#"{"type":"compaction/summary","seq":4,"time":1786669450002,"data":{"compactionId":"compact-b","message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20,"cacheReadTokens":30}}}"#,
        ]);

        let first_messages = parse_dsh_file(first.path());
        let second_messages = parse_dsh_file(second.path());

        assert_eq!(first_messages.len(), 1);
        assert_eq!(second_messages.len(), 1);
        assert_eq!(
            first_messages[0].dedup_key.as_deref(),
            Some("dsh:summary:cmp:compact-a:1786669450002:p:m:10:20:30:0:0")
        );
        assert_eq!(
            second_messages[0].dedup_key.as_deref(),
            Some("dsh:summary:cmp:compact-b:1786669450002:p:m:10:20:30:0:0")
        );
        assert_ne!(
            first_messages[0].dedup_key, second_messages[0].dedup_key,
            "globally distinct summarize calls must both survive lane dedup"
        );
    }

    #[test]
    fn summary_without_compaction_id_keeps_seq_fallback() {
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"legacy-summary","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"compaction/summary","seq":9,"time":1786669450002,"data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].dedup_key.as_deref(),
            Some("dsh:summary:seq:9:1786669450002:p:m:10:20:0:0:0")
        );
    }

    #[test]
    fn seeded_rows_share_the_parent_dedup_key_across_files() {
        // The seq boundary only fires when the header carries `seedLength`;
        // a resumed or re-exported transcript that lost it still repeats the
        // parent's per-call `message.id`, so the dedup key must be keyed on
        // that id rather than on the session id, and stay identical across the
        // two files for the cross-file pass in `lib.rs` to collapse them.
        let row = r#"{"type":"assistant/message","seq":39,"time":1785730448979,"data":{"turn":1,"message":{"id":"7ac2e3d7-d558-4b24-b71e-40fc2f42216d","source":{"provider":"deepseek","model":"deepseek-reasoner"}},"usage":{"inputTokens":2885,"outputTokens":25,"reasoningTokens":23}}}"#;
        let parent = write_zstd_session(&[
            r#"{"type":"session","id":"96cf59c9-b347-48b9-b234-a5200913ad05","createdAt":1,"cwd":"/work"}"#,
            row,
        ]);
        let child = write_zstd_session(&[
            r#"{"type":"session","id":"ada8966c-9fa3-441b-8721-37ff1e795e6a","createdAt":2,"cwd":"/work","parentSession":"96cf59c9-b347-48b9-b234-a5200913ad05"}"#,
            row,
        ]);

        let parent_messages = parse_dsh_file(parent.path());
        let child_messages = parse_dsh_file(child.path());

        assert_eq!(parent_messages.len(), 1);
        assert_eq!(child_messages.len(), 1);
        assert_ne!(parent_messages[0].session_id, child_messages[0].session_id);
        assert_eq!(
            parent_messages[0].dedup_key.as_deref(),
            Some(
                "dsh:msg:7ac2e3d7-d558-4b24-b71e-40fc2f42216d:1785730448979:deepseek:deepseek-reasoner:2885:2:0:0:23"
            )
        );
        assert_eq!(parent_messages[0].dedup_key, child_messages[0].dedup_key);
    }

    #[test]
    fn recovers_the_decodable_prefix_of_a_torn_trailing_frame() {
        // given: DSH appends one zstd frame per flush, so a scan racing a live
        // writer sees a complete prefix plus a truncated final frame. DSH's own
        // reader recovers the complete frames rather than refusing the log, and
        // `decode_all` would report zero tokens for the whole session.
        let header = zstd::encode_all(
            concat!(
                r#"{"type":"session","id":"session-torn","createdAt":1,"cwd":"/work"}"#,
                "
"
            )
            .as_bytes(),
            3,
        )
        .unwrap();
        let committed = zstd::encode_all(
            concat!(
                r#"{"type":"assistant/message","time":1786669454772,"data":{"turn":1,"message":{"id":"m-committed","source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20}}}"#,
                "
"
            )
            .as_bytes(),
            3,
        )
        .unwrap();
        let torn = zstd::encode_all(
            concat!(
                r#"{"type":"assistant/message","time":1786669455000,"data":{"turn":2,"message":{"id":"m-torn","source":{"provider":"p","model":"m"}},"usage":{"inputTokens":11,"outputTokens":21}}}"#,
                "
"
            )
            .as_bytes(),
            3,
        )
        .unwrap();

        let mut payload = header;
        payload.extend_from_slice(&committed);
        // Cut the final frame short, the way an interrupted append leaves it.
        payload.extend_from_slice(&torn[..torn.len() / 2]);

        // Non-vacuity: the one-shot decoder this parser used to call refuses
        // the whole payload, which is exactly the 0-token report being fixed.
        assert!(zstd::stream::decode_all(payload.as_slice()).is_err());

        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, &payload).unwrap();

        // when
        let messages = parse_dsh_file(file.path());

        // then: the committed frame still counts.
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].session_id, "session-torn");
        assert_eq!(messages[0].tokens.input, 10);
        assert_eq!(messages[0].tokens.output, 20);
    }

    #[test]
    fn parses_the_uncompressed_session_jsonl_spelling() {
        // `compression: none` writes the same rows to a plain `session.jsonl`
        // in the same session directory, so dispatch on the frame magic rather
        // than the file name.
        let dir = tempfile::tempdir().unwrap();
        let session_dir = dir.path().join("session-plain");
        std::fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("session.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"type":"session","id":"session-plain","createdAt":1,"cwd":"/work"}"#,
                "
",
                r#"{"type":"assistant/message","time":1786669454772,"data":{"turn":1,"message":{"id":"m-plain","source":{"provider":"p","model":"m"}},"usage":{"inputTokens":12,"outputTokens":34,"reasoningTokens":4}}}"#,
                "
"
            ),
        )
        .unwrap();

        let messages = parse_dsh_file(&path);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].session_id, "session-plain");
        assert_eq!(messages[0].tokens.input, 12);
        assert_eq!(messages[0].tokens.output, 30);
        assert_eq!(messages[0].tokens.reasoning, 4);
    }

    #[test]
    fn missing_or_corrupt_files_yield_no_messages() {
        assert!(parse_dsh_file(Path::new("/nonexistent/dsh/session.jsonl.zstd")).is_empty());
        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, b"this is not zstd").unwrap();
        assert!(parse_dsh_file(file.path()).is_empty());
    }

    #[test]
    fn marks_turn_start_when_no_turn_numbers_are_present() {
        let file = write_zstd_session(&[
            r#"{"type":"session","id":"session-noturn","createdAt":1,"cwd":"/work"}"#,
            r#"{"type":"user/message","time":1,"data":{}}"#,
            r#"{"type":"assistant/message","time":1786669454772,"data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":10,"outputTokens":20}}}"#,
            r#"{"type":"assistant/message","time":1786669455000,"data":{"message":{"source":{"provider":"p","model":"m"}},"usage":{"inputTokens":11,"outputTokens":21}}}"#,
        ]);

        let messages = parse_dsh_file(file.path());
        assert_eq!(messages.len(), 2);
        assert!(messages[0].is_turn_start);
        assert!(!messages[1].is_turn_start);
    }
}
