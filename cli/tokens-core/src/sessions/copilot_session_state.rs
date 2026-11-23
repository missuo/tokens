//! GitHub Copilot SDK session-state parser.
//!
//! The Copilot SDK runtime persists every session as an event journal at
//! `$COPILOT_HOME/session-state/<sessionId>/events.jsonl`, where an unset or
//! empty `COPILOT_HOME` means `~/.copilot`. Copilot CLI writes there, and so
//! does the "Copilot" harness of VS Code's Agent mode (VS Code 1.147+), which
//! runs the same SDK inside the Agent Host instead of the Copilot Chat
//! extension (microsoft/vscode `src/vs/platform/environment/common/copilotHome.ts`
//! and `src/vs/platform/agentHost/node/copilot/copilotAgent.ts`). Those turns
//! never reach the `workspaceStorage/*/chatSessions` files that
//! [`super::copilot_vscode`] reads.
//!
//! Per-call `assistant.usage` events are ephemeral and are never written to
//! the journal (microsoft/copilot-sdk `nodejs/src/generated/session-events.ts`,
//! `ephemeral: true`). The durable usage records are `session.usage_checkpoint`
//! and `session.shutdown`, and both carry the session's *cumulative* per-model
//! totals. Each snapshot is therefore differenced against the running peak of
//! the same model, so a record contributes only what was spent since the
//! previous one, at its own timestamp.

use super::utils::lossy_lines;
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::provider_identity::inferred_provider_from_model;
use chrono::DateTime;
use rayon::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Upper bound on the journal files followed through `session.journal_link`
/// records, so a corrupted chain cannot loop or fan out without limit.
const MAX_JOURNAL_FILES: usize = 64;

/// Cumulative counters in a fixed order so snapshots can be differenced
/// bucket by bucket: input (inclusive of cache reads), output, cache read,
/// cache write, reasoning, and API request count.
type Buckets = [i64; 6];
const INPUT: usize = 0;
const OUTPUT: usize = 1;
const CACHE_READ: usize = 2;
const CACHE_WRITE: usize = 3;
const REASONING: usize = 4;
const REQUESTS: usize = 5;

pub fn parse_copilot_session_state_files(paths: &[PathBuf]) -> Vec<UnifiedMessage> {
    paths
        .par_iter()
        .flat_map_iter(|path| parse_session_events(path))
        .collect()
}

#[derive(Debug)]
enum Record {
    /// `session.start` / `session.resume`.
    Start {
        cwd: Option<String>,
        model: Option<String>,
    },
    ModelChange(String),
    Usage(UsageRecord),
}

#[derive(Debug)]
struct UsageRecord {
    event_id: String,
    timestamp_ms: i64,
    current_model: Option<String>,
    /// Cumulative totals keyed by the model id the runtime accounted them to.
    trackers: Vec<(String, Buckets)>,
}

#[derive(Debug, Default)]
struct Journal {
    /// Whether the file opens with an original (non-replayed) `session.start`.
    starts_at_session_start: bool,
    /// `session.journal_link.previousFile`: the numbered archive this replay
    /// head continues from.
    previous_file: Option<String>,
    records: Vec<Record>,
}

#[derive(Debug)]
struct Snapshot {
    event_id: String,
    timestamp_ms: i64,
    tracker: String,
    attributed_model: Option<String>,
    buckets: Buckets,
}

fn parse_session_events(path: &Path) -> Vec<UnifiedMessage> {
    let Some(session_dir) = path.parent() else {
        return Vec::new();
    };
    let Some(session_id) = session_dir
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
    else {
        return Vec::new();
    };

    // A rotated journal is a replay head linking back to its numbered
    // predecessor in the same directory. Follow the chain so the archive's
    // usage keeps its own timestamps, newest file first.
    let mut journals = Vec::new();
    let mut visited = HashSet::new();
    let mut next = Some(path.to_path_buf());
    while let Some(file) = next.take() {
        if journals.len() >= MAX_JOURNAL_FILES || !visited.insert(file.clone()) {
            break;
        }
        let Some(journal) = read_journal(&file) else {
            break;
        };
        next = journal
            .previous_file
            .as_deref()
            .filter(|name| is_plain_file_name(name))
            .map(|name| session_dir.join(name));
        journals.push(journal);
    }
    // When the oldest readable file still opens with the session's own
    // `session.start`, the first snapshot of each model really is its first
    // and zero is the right baseline. Otherwise earlier usage was lost with
    // the missing head, and the first survivor only sets the baseline.
    let complete_from_start = journals
        .last()
        .is_some_and(|journal| journal.starts_at_session_start);

    let mut cwd: Option<String> = None;
    let mut model: Option<String> = None;
    let mut snapshots = Vec::new();
    for record in journals
        .into_iter()
        .rev()
        .flat_map(|journal| journal.records)
    {
        match record {
            Record::Start {
                cwd: start_cwd,
                model: start_model,
            } => {
                if cwd.is_none() {
                    cwd = start_cwd;
                }
                if start_model.is_some() {
                    model = start_model;
                }
            }
            Record::ModelChange(new_model) => model = Some(new_model),
            Record::Usage(usage) => {
                for (tracker, buckets) in usage.trackers {
                    // `auto` (and usage without a recorded model) is a counter
                    // of its own; it is attributed to the concrete model in use
                    // for display and pricing, but keeps its own baseline.
                    let attributed_model = match tracker.as_str() {
                        "" | "auto" => usage.current_model.clone().or_else(|| model.clone()),
                        _ => Some(tracker.clone()),
                    };
                    snapshots.push(Snapshot {
                        event_id: usage.event_id.clone(),
                        timestamp_ms: usage.timestamp_ms,
                        tracker,
                        attributed_model,
                        buckets,
                    });
                }
            }
        }
    }

    let workspace_key = cwd.as_deref().and_then(normalize_workspace_key);
    snapshot_deltas(snapshots, complete_from_start)
        .into_iter()
        .filter_map(|(snapshot, delta)| {
            let tokens = super::copilot::normalize_input_tokens(
                delta[INPUT],
                delta[OUTPUT],
                delta[CACHE_READ],
                delta[CACHE_WRITE],
                delta[REASONING],
            );
            if tokens.total() == 0 {
                return None;
            }
            let model_id = snapshot
                .attributed_model
                .unwrap_or_else(|| "auto".to_string());
            let provider_id = inferred_provider_from_model(&model_id)
                .unwrap_or("github-copilot")
                .to_string();
            let dedup_key = format!(
                "copilot-session-state:{}:{}:{}",
                session_id, snapshot.event_id, snapshot.tracker
            );
            let mut message = UnifiedMessage::new_with_dedup(
                "copilot",
                model_id,
                provider_id,
                session_id.clone(),
                snapshot.timestamp_ms,
                tokens,
                0.0,
                Some(dedup_key),
            );
            // The request counter is cumulative too, so its delta is the number
            // of model calls this increment covers.
            message.message_count = i32::try_from(delta[REQUESTS].max(1)).unwrap_or(i32::MAX);
            if let Some(workspace_key) = workspace_key.clone() {
                let workspace_label = workspace_label_from_key(&workspace_key);
                message.set_workspace(Some(workspace_key), workspace_label);
            }
            Some(message)
        })
        .collect()
}

/// Turn cumulative snapshots into the increment each one added.
///
/// Snapshots are grouped by tracker (the model id the runtime accounted the
/// usage to). A snapshot that reports less than an earlier one, or repeats it
/// (a replayed checkpoint record), adds nothing: each tracker's running peak
/// is the baseline, so the emitted total never exceeds the highest total the
/// journal ever recorded.
fn snapshot_deltas(
    mut snapshots: Vec<Snapshot>,
    complete_from_start: bool,
) -> Vec<(Snapshot, Buckets)> {
    let mut seen = HashSet::new();
    snapshots.retain(|snapshot| seen.insert((snapshot.event_id.clone(), snapshot.tracker.clone())));
    // Stable: records that share a timestamp keep journal order.
    snapshots.sort_by_key(|snapshot| snapshot.timestamp_ms);

    let mut peaks: HashMap<String, Buckets> = HashMap::new();
    let mut deltas = Vec::with_capacity(snapshots.len());
    for snapshot in snapshots {
        let current = snapshot.buckets;
        let baseline = match peaks.get(&snapshot.tracker) {
            Some(peak) => Some(*peak),
            None if complete_from_start => Some(Buckets::default()),
            None => None,
        };
        let peak = peaks.entry(snapshot.tracker.clone()).or_insert(current);
        for (index, value) in current.iter().enumerate() {
            peak[index] = peak[index].max(*value);
        }
        let Some(baseline) = baseline else {
            continue;
        };
        let mut delta = Buckets::default();
        for (index, value) in current.iter().enumerate() {
            delta[index] = value.saturating_sub(baseline[index]).max(0);
        }
        // Input includes cache reads, so cache-read growth can never exceed
        // input growth in the same snapshot without minting tokens.
        delta[CACHE_READ] = delta[CACHE_READ].min(delta[INPUT]);
        if delta[..REQUESTS].iter().all(|bucket| *bucket == 0) {
            continue;
        }
        deltas.push((snapshot, delta));
    }
    deltas
}

fn read_journal(path: &Path) -> Option<Journal> {
    let file = std::fs::File::open(path).ok()?;
    let mut journal = Journal::default();
    let mut first = true;
    for line in lossy_lines(BufReader::new(file)) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_first = std::mem::replace(&mut first, false);
        // Every record this parser reads is a `session.*` event. Transcript
        // records (messages, tool output) dominate the journal, so skip them
        // before paying for a JSON parse.
        if !is_first && !trimmed.contains("\"session.") {
            continue;
        }
        let Ok(event) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let Some(event_type) = event.get("type").and_then(Value::as_str) else {
            continue;
        };
        let replayed = event.get("checkpoint").and_then(Value::as_bool) == Some(true);
        if is_first {
            journal.starts_at_session_start = event_type == "session.start" && !replayed;
        }
        if event.get("ephemeral").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        // Session-level usage carries no `agentId`; a sub-agent's records are
        // already folded into the session totals.
        if event
            .get("agentId")
            .and_then(Value::as_str)
            .is_some_and(|agent| !agent.trim().is_empty())
        {
            continue;
        }
        let data = event.get("data").unwrap_or(&Value::Null);
        match event_type {
            "session.start" | "session.resume" => journal.records.push(Record::Start {
                cwd: non_empty_str(data.pointer("/context/cwd")),
                model: concrete_model(data.get("selectedModel")),
            }),
            "session.model_change" => {
                if let Some(model) = concrete_model(data.get("newModel")) {
                    journal.records.push(Record::ModelChange(model));
                }
            }
            "session.journal_link" => {
                if journal.previous_file.is_none() {
                    journal.previous_file = non_empty_str(data.get("previousFile"));
                }
            }
            "session.usage_checkpoint" | "session.shutdown" => {
                if let Some(usage) = usage_record(&event, data) {
                    journal.records.push(Record::Usage(usage));
                }
            }
            _ => {}
        }
    }
    Some(journal)
}

fn usage_record(event: &Value, data: &Value) -> Option<UsageRecord> {
    let timestamp_ms = event
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|timestamp| timestamp.timestamp_millis())?;
    let trackers = usage_trackers(data);
    if trackers.is_empty() {
        return None;
    }
    let event_id = non_empty_str(event.get("id")).unwrap_or_else(|| {
        // Real records carry a UUID. Hash the content of one that does not so
        // its key stays stable when earlier lines rotate away.
        let digest = Sha256::digest(event.to_string().as_bytes());
        format!("anon-{digest:x}")
    });
    let current_model = concrete_model(data.get("currentModel"))
        .or_else(|| concrete_model(data.pointer("/accountingSnapshot/currentModel")));
    Some(UsageRecord {
        event_id,
        timestamp_ms,
        current_model,
        trackers,
    })
}

/// Per-model cumulative totals of a `session.usage_checkpoint` or
/// `session.shutdown` payload.
///
/// `providerModelMetrics` is the public per-provider/model breakdown both
/// events carry; entries are summed per model id so the tracker identity is
/// the same as the model-keyed `modelMetrics` map that older shutdowns (and
/// the checkpoint's internal `accountingSnapshot`) carry instead.
fn usage_trackers(data: &Value) -> Vec<(String, Buckets)> {
    let mut trackers: Vec<(String, Buckets)> = Vec::new();
    let mut add = |model: &str, metric: &Value| {
        let Some(buckets) = metric_buckets(metric) else {
            return;
        };
        let model = model.trim();
        match trackers.iter_mut().find(|(tracker, _)| tracker == model) {
            Some((_, total)) => {
                for (index, value) in buckets.iter().enumerate() {
                    total[index] = total[index].saturating_add(*value);
                }
            }
            None => trackers.push((model.to_string(), buckets)),
        }
    };

    if let Some(entries) = data
        .get("providerModelMetrics")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty())
    {
        for entry in entries {
            if let Some(metrics) = entry.get("metrics") {
                let model = entry.get("modelId").and_then(Value::as_str).unwrap_or("");
                add(model, metrics);
            }
        }
    } else if let Some(metrics) = data
        .get("modelMetrics")
        .or_else(|| data.pointer("/accountingSnapshot/modelMetrics"))
        .and_then(Value::as_object)
    {
        for (model, metric) in metrics {
            add(model, metric);
        }
    }

    trackers.retain(|(_, buckets)| buckets[..REQUESTS].iter().any(|value| *value > 0));
    trackers
}

fn metric_buckets(metric: &Value) -> Option<Buckets> {
    let usage = metric.get("usage")?;
    let read = |value: Option<&Value>| value.and_then(Value::as_i64).unwrap_or(0).max(0);
    Some([
        read(usage.get("inputTokens")),
        read(usage.get("outputTokens")),
        read(usage.get("cacheReadTokens")),
        read(usage.get("cacheWriteTokens")),
        read(usage.get("reasoningTokens")),
        read(metric.pointer("/requests/count")),
    ])
}

fn non_empty_str(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn concrete_model(value: Option<&Value>) -> Option<String> {
    non_empty_str(value).filter(|model| model != "auto")
}

/// `previousFile` names a sibling in the session directory; anything that
/// could leave it is ignored.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && Path::new(name).file_name().and_then(|n| n.to_str()) == Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const SESSION_ID: &str = "8d3f1c2a-5b6e-4f70-9a81-2c3d4e5f6a7b";

    // Envelopes and payloads follow microsoft/copilot-sdk
    // `nodejs/src/generated/session-events.ts` at v1.0.33-preview.0, the SDK
    // VS Code 1.147.0 ships: `StartEvent`, `UsageCheckpointEvent`,
    // `ShutdownEvent`, `ModelChangeEvent`, `JournalLinkEvent`.
    const START: &str = r#"{"type":"session.start","data":{"sessionId":"8d3f1c2a-5b6e-4f70-9a81-2c3d4e5f6a7b","version":1,"producer":"copilot-agent","copilotVersion":"1.0.111-0","startTime":"2026-11-18T10:00:00.000Z","selectedModel":"gpt-5.4","context":{"cwd":"/Users/alice/project","gitRoot":"/Users/alice/project","branch":"main"}},"id":"0b6c1f1e-0000-4000-8000-000000000001","timestamp":"2026-11-18T10:00:00.000Z","parentId":null}"#;
    const USER: &str = r#"{"type":"user.message","data":{"content":"fix the session.shutdown test","interactionId":"i-1"},"id":"0b6c1f1e-0000-4000-8000-000000000002","timestamp":"2026-11-18T10:00:01.000Z","parentId":"0b6c1f1e-0000-4000-8000-000000000001"}"#;
    const CHECKPOINT_1: &str = r#"{"type":"session.usage_checkpoint","data":{"totalNanoAiu":1200000000,"totalPremiumRequests":1,"providerModelMetrics":[{"modelId":"gpt-5.4","provider":{"id":"github-copilot","kind":"copilot","label":"GitHub Copilot"},"metrics":{"requests":{"count":3,"cost":1},"usage":{"inputTokens":30000,"outputTokens":600,"cacheReadTokens":20000,"cacheWriteTokens":0,"reasoningTokens":100},"totalNanoAiu":1200000000}}],"usageAccountingWatermarks":{"8d3f1c2a-5b6e-4f70-9a81-2c3d4e5f6a7b":3}},"id":"0b6c1f1e-0000-4000-8000-000000000003","timestamp":"2026-11-18T10:05:00.000Z","parentId":"0b6c1f1e-0000-4000-8000-000000000002"}"#;
    const CHECKPOINT_2: &str = r#"{"type":"session.usage_checkpoint","data":{"totalNanoAiu":2000000000,"totalPremiumRequests":2,"providerModelMetrics":[{"modelId":"gpt-5.4","provider":{"id":"github-copilot","kind":"copilot","label":"GitHub Copilot"},"metrics":{"requests":{"count":5,"cost":2},"usage":{"inputTokens":50000,"outputTokens":900,"cacheReadTokens":35000,"cacheWriteTokens":0,"reasoningTokens":150},"totalNanoAiu":2000000000}}]},"id":"0b6c1f1e-0000-4000-8000-000000000004","timestamp":"2026-11-19T09:00:00.000Z","parentId":"0b6c1f1e-0000-4000-8000-000000000003"}"#;
    // Older shutdowns carry only the model-keyed `modelMetrics` map.
    const SHUTDOWN: &str = r#"{"type":"session.shutdown","data":{"shutdownType":"routine","totalPremiumRequests":2,"totalApiDurationMs":41000,"sessionStartTime":1794996000000,"codeChanges":{"linesAdded":4,"linesRemoved":1,"filesModified":["src/main.rs"]},"modelMetrics":{"gpt-5.4":{"requests":{"count":6,"cost":2},"usage":{"inputTokens":60000,"outputTokens":1000,"cacheReadTokens":40000,"cacheWriteTokens":0,"reasoningTokens":150}}},"currentModel":"gpt-5.4","currentTokens":22592},"id":"0b6c1f1e-0000-4000-8000-000000000005","timestamp":"2026-11-19T09:30:00.000Z","parentId":"0b6c1f1e-0000-4000-8000-000000000004"}"#;

    fn write_session(root: &Path, files: &[(&str, &[&str])]) -> PathBuf {
        let dir = root.join("session-state").join(SESSION_ID);
        fs::create_dir_all(&dir).unwrap();
        for (name, lines) in files {
            fs::write(dir.join(name), format!("{}\n", lines.join("\n"))).unwrap();
        }
        dir.join("events.jsonl")
    }

    fn totals(messages: &[UnifiedMessage]) -> (i64, i64, i64, i64) {
        messages.iter().fold((0, 0, 0, 0), |acc, m| {
            (
                acc.0 + m.tokens.input,
                acc.1 + m.tokens.output,
                acc.2 + m.tokens.cache_read,
                acc.3 + m.tokens.reasoning,
            )
        })
    }

    #[test]
    fn checkpoints_are_differenced_into_dated_increments() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            &[("events.jsonl", &[START, USER, CHECKPOINT_1, CHECKPOINT_2])],
        );

        let mut messages = parse_copilot_session_state_files(&[path]);
        messages.sort_by_key(|m| m.timestamp);

        assert_eq!(messages.len(), 2);
        let first = &messages[0];
        assert_eq!(first.client, "copilot");
        assert_eq!(first.session_id, SESSION_ID);
        assert_eq!(first.model_id, "gpt-5.4");
        assert_eq!(first.timestamp, 1_794_996_300_000);
        // inputTokens is inclusive of cache reads.
        assert_eq!(first.tokens.input, 10_000);
        assert_eq!(first.tokens.cache_read, 20_000);
        assert_eq!(first.tokens.output, 600);
        assert_eq!(first.tokens.reasoning, 100);
        assert_eq!(first.message_count, 3);
        assert_eq!(
            first.dedup_key.as_deref(),
            Some("copilot-session-state:8d3f1c2a-5b6e-4f70-9a81-2c3d4e5f6a7b:0b6c1f1e-0000-4000-8000-000000000003:gpt-5.4")
        );
        assert!(first.workspace_key.is_some());

        let second = &messages[1];
        assert_eq!(second.timestamp, 1_795_078_800_000);
        assert_eq!(second.tokens.input, 5_000);
        assert_eq!(second.tokens.cache_read, 15_000);
        assert_eq!(second.tokens.output, 300);
        assert_eq!(second.tokens.reasoning, 50);
        assert_eq!(second.message_count, 2);
    }

    #[test]
    fn shutdown_model_metrics_continue_the_checkpoint_counter() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            &[(
                "events.jsonl",
                &[START, CHECKPOINT_1, CHECKPOINT_2, SHUTDOWN],
            )],
        );

        let messages = parse_copilot_session_state_files(&[path]);

        assert_eq!(messages.len(), 3);
        // The session total is the final cumulative snapshot, counted once.
        assert_eq!(totals(&messages), (20_000, 1_000, 40_000, 150));
        assert_eq!(messages.iter().map(|m| m.message_count).sum::<i32>(), 6);
    }

    #[test]
    fn transcript_and_ephemeral_records_add_nothing() {
        // `assistant.usage` is ephemeral and should never be on disk; if one
        // is, it must not be counted on top of the checkpoints.
        let usage = r#"{"type":"assistant.usage","data":{"model":"gpt-5.4","inputTokens":999999,"outputTokens":999},"ephemeral":true,"id":"0b6c1f1e-0000-4000-8000-0000000000aa","timestamp":"2026-11-18T10:04:00.000Z","parentId":"0b6c1f1e-0000-4000-8000-000000000002"}"#;
        let ephemeral_checkpoint = CHECKPOINT_2.replace(
            r#""type":"session.usage_checkpoint","#,
            r#""type":"session.usage_checkpoint","ephemeral":true,"#,
        );
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            &[(
                "events.jsonl",
                &[START, USER, usage, CHECKPOINT_1, &ephemeral_checkpoint],
            )],
        );

        let messages = parse_copilot_session_state_files(&[path]);

        assert_eq!(messages.len(), 1);
        assert_eq!(totals(&messages), (10_000, 600, 20_000, 100));
    }

    #[test]
    fn repeated_and_subagent_records_count_once() {
        let subagent = CHECKPOINT_2
            .replace(
                "0b6c1f1e-0000-4000-8000-000000000004",
                "0b6c1f1e-0000-4000-8000-0000000000bb",
            )
            .replace(
                r#""type":"session.usage_checkpoint","#,
                r#""type":"session.usage_checkpoint","agentId":"explore-1","#,
            );
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            &[(
                "events.jsonl",
                &[START, CHECKPOINT_1, CHECKPOINT_1, &subagent, CHECKPOINT_2],
            )],
        );

        let messages = parse_copilot_session_state_files(&[path]);

        assert_eq!(messages.len(), 2);
        assert_eq!(totals(&messages), (15_000, 900, 35_000, 150));
    }

    #[test]
    fn auto_usage_is_attributed_to_the_model_in_use() {
        let change = r#"{"type":"session.model_change","data":{"previousModel":"gpt-5.4","newModel":"claude-sonnet-4.6","cause":"user"},"id":"0b6c1f1e-0000-4000-8000-000000000010","timestamp":"2026-11-18T10:01:00.000Z","parentId":"0b6c1f1e-0000-4000-8000-000000000001"}"#;
        let auto = CHECKPOINT_1.replace(r#""modelId":"gpt-5.4""#, r#""modelId":"auto""#);
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path(), &[("events.jsonl", &[START, change, &auto])]);

        let messages = parse_copilot_session_state_files(&[path]);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, "claude-sonnet-4.6");
        assert!(messages[0]
            .dedup_key
            .as_deref()
            .is_some_and(|key| key.ends_with(":auto")));
    }

    fn replay_head(previous_file: &str) -> Vec<String> {
        let start = START.replace(
            r#""type":"session.start","#,
            r#""type":"session.start","checkpoint":true,"#,
        );
        let link = format!(
            r#"{{"type":"session.journal_link","data":{{"previousFile":"{previous_file}","sessionId":"{SESSION_ID}","version":1}},"id":"0b6c1f1e-0000-4000-8000-000000000020","timestamp":"2026-11-19T08:00:00.000Z","parentId":null}}"#
        );
        let replayed_checkpoint = CHECKPOINT_1.replace(
            r#""type":"session.usage_checkpoint","#,
            r#""type":"session.usage_checkpoint","checkpoint":true,"#,
        );
        vec![start, link, replayed_checkpoint, CHECKPOINT_2.to_string()]
    }

    #[test]
    fn rotated_journal_follows_its_archive() {
        let head = replay_head("events.1.jsonl");
        let head: Vec<&str> = head.iter().map(String::as_str).collect();
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(
            dir.path(),
            &[
                ("events.1.jsonl", &[START, USER, CHECKPOINT_1]),
                ("events.jsonl", &head),
            ],
        );

        let messages = parse_copilot_session_state_files(&[path]);

        assert_eq!(messages.len(), 2);
        assert_eq!(totals(&messages), (15_000, 900, 35_000, 150));
    }

    #[test]
    fn missing_archive_makes_the_first_snapshot_a_baseline() {
        let head = replay_head("events.1.jsonl");
        let head: Vec<&str> = head.iter().map(String::as_str).collect();
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path(), &[("events.jsonl", &head)]);

        let messages = parse_copilot_session_state_files(&[path]);

        // Only the growth after the surviving baseline is attributable.
        assert_eq!(messages.len(), 1);
        assert_eq!(totals(&messages), (5_000, 300, 15_000, 50));
    }

    #[test]
    fn journal_link_cannot_escape_the_session_directory() {
        let head = replay_head("../other/events.jsonl");
        let head: Vec<&str> = head.iter().map(String::as_str).collect();
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path(), &[("events.jsonl", &head)]);
        let other = dir.path().join("session-state").join("other");
        fs::create_dir_all(&other).unwrap();
        fs::write(
            other.join("events.jsonl"),
            format!("{START}\n{CHECKPOINT_1}\n"),
        )
        .unwrap();

        let messages = parse_copilot_session_state_files(&[path]);

        assert_eq!(totals(&messages), (5_000, 300, 15_000, 50));
    }

    #[test]
    fn session_without_usage_records_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session(dir.path(), &[("events.jsonl", &[START, USER])]);
        assert!(parse_copilot_session_state_files(&[path]).is_empty());
    }
}
