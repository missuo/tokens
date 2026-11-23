//! TRAE CLI session parser.
//!
//! TRAE CLI 2.0 ships as `traex` and reports itself as `traecli 0.2xx`. It is
//! a closed-source fork of codex-rs and writes Codex rollouts —
//! `session_meta` (with `originator: "codex-tui"` and
//! `model_provider: "trae"`), `turn_context`, `response_item`, `event_msg`
//! and `token_count` — one level further down than Codex does:
//!
//! - `${TRAE_HOME:-~/.trae}/cli/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`
//! - `${TRAE_HOME:-~/.trae}/cli/archived_sessions/` (`traex archive <id>`)
//! - `TRAECLI_HOME` relocates that `cli` directory on its own.
//!
//! The rollouts therefore go through the Codex parser and are relabelled onto
//! the `trae-cli` client, so they are neither counted as Codex usage nor mixed
//! with the account-level API dump the `trae` client reads.
//!
//! Newer TRAE CLI rollouts carry the transcript in `history_mutation` records
//! and their per-response usage in `token_usage_record.payload.usage` (same
//! shape as Codex's `last_token_usage`). Upstream Codex writes that record
//! beside a `token_count` event, so it is only read when the rollout has no
//! `token_count` event at all; otherwise the Codex parser's accounting stands
//! and nothing is counted twice.
//!
//! Sources: the store layout and both rollout dialects follow the TraeX
//! readers in kenn-io/agentsview (`internal/parser/traex.go`, `types.go`,
//! `testdata/traex/`) and vshulcz/deja-vu (`docs/registry/trae.md`), and
//! deepcoldy/botmux `src/services/traex-paths.ts` for `TRAE_HOME`.

use super::codex::{
    self, codex_message_is_human_turn, codex_workspace_from_cwd, CodexTokenUsage, CodexTotals,
    CODEX_DEFAULT_AGENT,
};
use super::utils::{file_modified_timestamp_ms, is_zst_path, session_id_from_path};
use super::UnifiedMessage;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

pub const TRAE_CLI_CLIENT_ID: &str = "trae-cli";

/// Added to the Codex parser version for the `trae-cli` cache identity. Bump
/// it when this wrapper (relabel or `token_usage_record` fallback) changes how
/// byte-identical rollouts parse.
pub const TRAE_CLI_PARSER_OFFSET: u32 = 0;

/// Agents-tab bucket for TRAE CLI threads, in place of the Codex names.
const TRAE_CLI_AGENT: &str = "TRAE CLI";

/// Same bound as the Codex parser puts on one decoded compressed rollout.
const MAX_ROLLOUT_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;

pub fn parse_trae_cli_file(path: &Path) -> Vec<UnifiedMessage> {
    let mut messages = codex::parse_codex_file(path);
    if messages.is_empty() {
        messages = parse_token_usage_records(path);
    }
    for message in &mut messages {
        relabel_codex_message(message);
    }
    messages
}

/// Move a Codex-parser message onto the `trae-cli` client: client id, the
/// Codex agent buckets ("Codex", "Codex Subagent", ...) and the `codex:`
/// dedup-key namespace.
fn relabel_codex_message(message: &mut UnifiedMessage) {
    message.client = TRAE_CLI_CLIENT_ID.to_string();
    if let Some(agent) = message.agent.as_mut() {
        if let Some(rest) = agent.strip_prefix(CODEX_DEFAULT_AGENT) {
            *agent = format!("{TRAE_CLI_AGENT}{rest}");
        }
    }
    if let Some(key) = message.dedup_key.as_mut() {
        if let Some(rest) = key.strip_prefix("codex:") {
            *key = format!("{TRAE_CLI_CLIENT_ID}:{rest}");
        }
    }
}

fn open_rollout(path: &Path) -> Option<Box<dyn BufRead>> {
    let file = std::fs::File::open(path).ok()?;
    if is_zst_path(path) {
        let decoder = zstd::stream::read::Decoder::new(file).ok()?;
        Some(Box::new(BufReader::new(
            decoder.take(MAX_ROLLOUT_ARCHIVE_BYTES),
        )))
    } else {
        Some(Box::new(BufReader::new(file)))
    }
}

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// A user item in a `history_mutation` append that a person typed. TRAE CLI
/// also writes runtime injections under the user role; those open with a tag
/// such as `<environment_context>` and are excluded the way the Codex parser
/// excludes them from `user_message` events.
fn history_mutation_has_human_turn(payload: &Value) -> bool {
    let Some(items) = payload.get("items").and_then(Value::as_array) else {
        return false;
    };
    items.iter().any(|item| {
        item.get("type").and_then(Value::as_str) == Some("message")
            && item.get("role").and_then(Value::as_str) == Some("user")
            && item
                .get("content")
                .and_then(Value::as_array)
                .is_some_and(|blocks| {
                    blocks.iter().any(|block| {
                        codex_message_is_human_turn(block.get("text").and_then(Value::as_str))
                    })
                })
    })
}

/// Read the per-response `token_usage_record` rows of a rollout that has no
/// `token_count` event. Returns nothing as soon as a `token_count` event shows
/// up, because the Codex parser already accounted for that rollout.
fn parse_token_usage_records(path: &Path) -> Vec<UnifiedMessage> {
    let Some(reader) = open_rollout(path) else {
        return Vec::new();
    };
    let plain_path = if is_zst_path(path) {
        path.with_extension("")
    } else {
        path.to_path_buf()
    };
    let session_id = session_id_from_path(&plain_path);
    let fallback_timestamp = file_modified_timestamp_ms(path);

    let mut messages = Vec::new();
    let mut seen_responses: HashSet<String> = HashSet::new();
    let mut thread_id: Option<String> = None;
    let mut provider: Option<String> = None;
    let mut model: Option<String> = None;
    let mut workspace: (Option<String>, Option<String>) = (None, None);
    let mut pending_turn_start = false;

    for (line_index, line) in reader.lines().enumerate() {
        let Ok(line) = line else {
            break;
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(payload) = entry.get("payload") else {
            continue;
        };
        match entry.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                // A forked or spawned thread replays ancestors' metadata after
                // its own; the first record names the file's thread.
                if thread_id.is_none() {
                    thread_id = str_field(payload, "id").map(str::to_string);
                    if let Some(value) = str_field(payload, "model_provider") {
                        provider = Some(value.to_string());
                    }
                    if let Some(cwd) = str_field(payload, "cwd") {
                        workspace = codex_workspace_from_cwd(cwd);
                    }
                }
            }
            Some("turn_context") => {
                if let Some(value) = str_field(payload, "model") {
                    model = Some(value.to_string());
                }
                if let Some(value) = str_field(payload, "model_provider") {
                    provider = Some(value.to_string());
                }
            }
            Some("event_msg") => match payload.get("type").and_then(Value::as_str) {
                Some("token_count") => return Vec::new(),
                Some("user_message")
                    if codex_message_is_human_turn(
                        payload.get("message").and_then(Value::as_str),
                    ) =>
                {
                    pending_turn_start = true;
                }
                _ => {}
            },
            Some("history_mutation") => {
                if history_mutation_has_human_turn(payload) {
                    pending_turn_start = true;
                }
            }
            Some("token_usage_record") => {
                if let (Some(owner), Some(record_thread)) =
                    (thread_id.as_deref(), str_field(payload, "thread_id"))
                {
                    if owner != record_thread {
                        continue;
                    }
                }
                let Some(usage) = payload
                    .get("usage")
                    .cloned()
                    .and_then(|usage| serde_json::from_value::<CodexTokenUsage>(usage).ok())
                else {
                    continue;
                };
                let response_id = str_field(payload, "response_id");
                if let Some(id) = response_id {
                    if !seen_responses.insert(id.to_string()) {
                        continue;
                    }
                }
                let tokens = CodexTotals::from_usage(&usage).into_tokens();
                if tokens.total() == 0 {
                    continue;
                }
                let timestamp = entry
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(fallback_timestamp);
                let mut message = UnifiedMessage::new_with_agent(
                    TRAE_CLI_CLIENT_ID,
                    model.clone().unwrap_or_else(|| "unknown".to_string()),
                    provider.clone().unwrap_or_else(|| "trae".to_string()),
                    session_id.clone(),
                    timestamp,
                    tokens,
                    0.0,
                    Some(TRAE_CLI_AGENT.to_string()),
                );
                let thread = thread_id.as_deref().unwrap_or(&session_id);
                message.dedup_key = Some(match response_id {
                    Some(id) => format!("{TRAE_CLI_CLIENT_ID}:response:{thread}:{id}"),
                    None => format!("{TRAE_CLI_CLIENT_ID}:usage:{thread}:{line_index}"),
                });
                message.workspace_key = workspace.0.clone();
                message.workspace_label = workspace.1.clone();
                message.is_turn_start = std::mem::take(&mut pending_turn_start);
                messages.push(message);
            }
            _ => {}
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    // Records copied from the TRAE CLI 0.200.19 fixtures in kenn-io/agentsview
    // (`internal/parser/testdata/traex/`), trimmed to the fields that matter.
    const TOKEN_COUNT_ROLLOUT: &str = concat!(
        r#"{"timestamp":"2026-08-01T18:07:04.550Z","type":"session_meta","payload":{"id":"019fbcca-9fd4-7d20-83dc-0762b2f839b3","timestamp":"2026-08-01T18:07:03.636Z","cwd":"/home/user/code/api","originator":"codex-tui","cli_version":"0.200.19","model_provider":"trae","base_instructions":{"text":"You are TRAE CLI, a coding agent."}}}"#,
        "\n",
        r#"{"timestamp":"2026-08-01T18:07:04.560Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1","started_at":1785561000000,"model_context_window":272000}}"#,
        "\n",
        r#"{"timestamp":"2026-08-01T18:07:04.570Z","type":"turn_context","payload":{"turn_id":"turn-1","cwd":"/home/user/code/api","current_date":"2026-08-01","timezone":"Asia/Shanghai","approval_policy":"on-request","sandbox_policy":{"mode":"workspace-write"},"model":"gpt-5-codex","model_provider":"trae"}}"#,
        "\n",
        r#"{"timestamp":"2026-08-01T18:07:04.580Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Explore the parser package."}]}}"#,
        "\n",
        r#"{"timestamp":"2026-08-01T18:07:04.590Z","type":"event_msg","payload":{"type":"user_message","message":"Explore the parser package.","images":null,"local_images":null,"text_elements":[]}}"#,
        "\n",
        r#"{"timestamp":"2026-08-01T18:07:14.100Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":4200,"cached_input_tokens":3600,"output_tokens":180,"reasoning_output_tokens":64,"total_tokens":4380},"total_token_usage":{"input_tokens":4200,"cached_input_tokens":3600,"output_tokens":180,"reasoning_output_tokens":64,"total_tokens":4380},"model_context_window":272000},"rate_limits":null,"context":{"used_tokens":4380}}}"#,
        "\n",
        r#"{"timestamp":"2026-08-01T18:07:15.010Z","type":"event_msg","payload":{"type":"agent_message","message":"The provider facade owns discovery and parsing.","phase":null}}"#,
        "\n",
        r#"{"timestamp":"2026-08-01T18:07:15.100Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","completed_at":1785561015100,"duration_ms":10540}}"#,
        "\n",
    );

    const HISTORY_MUTATION_ROLLOUT: &str = concat!(
        r#"{"timestamp":"2026-09-01T10:00:00.000Z","type":"session_meta","payload":{"id":"019fcf70-1111-7000-8000-000000000001","timestamp":"2026-09-01T10:00:00.000Z","cwd":"/home/user/code/app","originator":"codex-tui","cli_version":"0.200.19","model_provider":"trae"}}"#,
        "\n",
        r#"{"timestamp":"2026-09-01T10:00:01.000Z","type":"turn_context","payload":{"turn_id":"turn-1","model":"gpt-5-codex","model_provider":"trae"}}"#,
        "\n",
        r#"{"timestamp":"2026-09-01T10:00:02.000Z","type":"history_mutation","payload":{"operation":"append","items":[{"type":"message","role":"user","content":[{"type":"input_text","text":"Inspect the parser."}]},{"type":"reasoning","summary":[],"content":null,"encrypted_content":"REDACTED"},{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I will inspect the parser."}]},{"type":"custom_tool_call","name":"exec","call_id":"call_read","input":"{\"cmd\":\"ls internal/parser\"}"},{"type":"trae_extra_info","extra_info":{"kind":"ui-only"}},{"type":"custom_tool_call_output","call_id":"call_read","output":[{"type":"input_text","text":"codex.go\ntraex.go\n"}]}]}}"#,
        "\n",
        r#"{"timestamp":"2026-09-01T10:00:03.000Z","type":"token_usage_record","payload":{"response_id":"resp_1","usage":{"input_tokens":100,"cached_input_tokens":40,"cache_creation_input_tokens":0,"output_tokens":12,"reasoning_output_tokens":2,"total_tokens":112}}}"#,
        "\n",
        r#"{"timestamp":"2026-09-01T10:00:04.000Z","type":"history_mutation","payload":{"operation":"append","items":[{"type":"message","role":"assistant","phase":"final_answer","content":[{"type":"output_text","text":"The parser files are codex.go and traex.go."}]}]}}"#,
        "\n",
    );

    fn write_rollout(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn token_count_rollout_is_relabelled_onto_trae_cli() {
        let dir = TempDir::new().unwrap();
        let path = write_rollout(
            &dir,
            "rollout-2026-08-01T18-07-04-019fbcca-9fd4-7d20-83dc-0762b2f839b3.jsonl",
            TOKEN_COUNT_ROLLOUT,
        );

        let messages = parse_trae_cli_file(&path);
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.client, "trae-cli");
        assert_eq!(message.model_id, "gpt-5-codex");
        assert_eq!(message.provider_id, "trae");
        // Codex accounting: cached input is inside `input_tokens`, reasoning
        // inside `output_tokens`.
        assert_eq!(message.tokens.input, 600);
        assert_eq!(message.tokens.cache_read, 3600);
        assert_eq!(message.tokens.output, 116);
        assert_eq!(message.tokens.reasoning, 64);
        assert_eq!(message.tokens.total(), 4380);
        assert_eq!(message.agent.as_deref(), Some("TRAE CLI"));
        assert!(message.is_turn_start);
        assert!(message
            .dedup_key
            .as_deref()
            .is_some_and(|key| key.starts_with("trae-cli:")));
    }

    #[test]
    fn history_mutation_rollout_counts_token_usage_records() {
        let dir = TempDir::new().unwrap();
        let path = write_rollout(
            &dir,
            "rollout-2026-09-01T10-00-00-019fcf70-1111-7000-8000-000000000001.jsonl",
            HISTORY_MUTATION_ROLLOUT,
        );

        let messages = parse_trae_cli_file(&path);
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.client, "trae-cli");
        assert_eq!(message.model_id, "gpt-5-codex");
        assert_eq!(message.provider_id, "trae");
        assert_eq!(message.tokens.input, 60);
        assert_eq!(message.tokens.cache_read, 40);
        assert_eq!(message.tokens.output, 10);
        assert_eq!(message.tokens.reasoning, 2);
        assert_eq!(message.tokens.cache_write, 0);
        assert_eq!(
            message.timestamp,
            chrono::DateTime::parse_from_rfc3339("2026-09-01T10:00:03.000Z")
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(
            message.workspace_key.as_deref(),
            Some("/home/user/code/app")
        );
        assert!(message.is_turn_start);
        assert_eq!(
            message.dedup_key.as_deref(),
            Some("trae-cli:response:019fcf70-1111-7000-8000-000000000001:resp_1")
        );
    }

    #[test]
    fn repeated_token_usage_record_for_one_response_counts_once() {
        let dir = TempDir::new().unwrap();
        let repeated = HISTORY_MUTATION_ROLLOUT.lines().nth(3).unwrap();
        let content = format!("{HISTORY_MUTATION_ROLLOUT}{repeated}\n");
        let path = write_rollout(&dir, "rollout-repeat.jsonl", &content);

        let messages = parse_trae_cli_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.total(), 112);
    }

    #[test]
    fn token_usage_record_is_ignored_when_token_count_is_present() {
        // Codex-style rollouts write both records for one response; the
        // `token_count` accounting is the one that counts.
        let dir = TempDir::new().unwrap();
        let usage_record = r#"{"timestamp":"2026-08-01T18:07:14.000Z","type":"token_usage_record","payload":{"thread_id":"019fbcca-9fd4-7d20-83dc-0762b2f839b3","turn_id":"turn-1","response_id":"resp_1","usage":{"input_tokens":4200,"cached_input_tokens":3600,"output_tokens":180,"reasoning_output_tokens":64,"total_tokens":4380}}}"#;
        let mut lines: Vec<&str> = TOKEN_COUNT_ROLLOUT.lines().collect();
        lines.insert(5, usage_record);
        let content = format!("{}\n", lines.join("\n"));
        let path = write_rollout(&dir, "rollout-both.jsonl", &content);

        let messages = parse_trae_cli_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.total(), 4380);
    }

    #[test]
    fn zero_token_count_rollout_does_not_fall_back_to_usage_records() {
        let dir = TempDir::new().unwrap();
        let zeroed = TOKEN_COUNT_ROLLOUT
            .replace(r#""input_tokens":4200"#, r#""input_tokens":0"#)
            .replace(
                r#""cached_input_tokens":3600"#,
                r#""cached_input_tokens":0"#,
            )
            .replace(r#""output_tokens":180"#, r#""output_tokens":0"#)
            .replace(
                r#""reasoning_output_tokens":64"#,
                r#""reasoning_output_tokens":0"#,
            );
        let usage_record = r#"{"timestamp":"2026-08-01T18:07:14.000Z","type":"token_usage_record","payload":{"response_id":"resp_1","usage":{"input_tokens":10,"output_tokens":1}}}"#;
        let content = format!("{zeroed}{usage_record}\n");
        let path = write_rollout(&dir, "rollout-zero.jsonl", &content);

        assert!(parse_trae_cli_file(&path).is_empty());
    }

    #[test]
    fn usage_record_for_another_thread_is_not_counted() {
        let dir = TempDir::new().unwrap();
        let content = HISTORY_MUTATION_ROLLOUT.replace(
            r#""payload":{"response_id":"resp_1""#,
            r#""payload":{"thread_id":"some-other-thread","response_id":"resp_1""#,
        );
        let path = write_rollout(&dir, "rollout-other-thread.jsonl", &content);

        assert!(parse_trae_cli_file(&path).is_empty());
    }

    #[test]
    fn compressed_history_mutation_rollout_is_read() {
        let dir = TempDir::new().unwrap();
        let compressed = zstd::encode_all(HISTORY_MUTATION_ROLLOUT.as_bytes(), 0).unwrap();
        let path = dir.path().join("rollout-compressed.jsonl.zst");
        std::fs::write(&path, compressed).unwrap();

        let messages = parse_trae_cli_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].session_id, "rollout-compressed");
        assert_eq!(messages[0].tokens.total(), 112);
    }

    #[test]
    fn injected_user_context_is_not_a_turn_start() {
        let dir = TempDir::new().unwrap();
        let content = HISTORY_MUTATION_ROLLOUT.replace(
            "Inspect the parser.",
            "<environment_context>cwd=/home/user/code/app</environment_context>",
        );
        let path = write_rollout(&dir, "rollout-injected.jsonl", &content);

        let messages = parse_trae_cli_file(&path);
        assert_eq!(messages.len(), 1);
        assert!(!messages[0].is_turn_start);
    }
}
