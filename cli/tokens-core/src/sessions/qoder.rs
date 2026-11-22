//! Qoder and Qoder CN session parsers.
//!
//! Qoder is Alibaba's agentic coding product, shipped as two fully separate
//! editions with separate accounts, model pools and data directories:
//!
//! | | international | China |
//! |---|---|---|
//! | CLI transcripts | `~/.qoder/projects/` | `~/.qoder-cn/projects/` |
//! | CLI config override | `QODER_CONFIG_DIR` | `QODERCN_CONFIG_DIR` |
//! | Structured logs | `~/.qoder/logs/sessions/` | `~/.qoder-cn/logs/sessions/` |
//! | IDE SQLite | `Qoder/SharedClientCache/cache/db/local.db` | `QoderCN/…` |
//!
//! Both editions are exposed through one client (`qoder`). The edition only
//! namespaces the deduplication keys and the diagnostics; it never becomes a
//! provider, and the two editions never deduplicate each other's requests.
//!
//! # Three local sources
//!
//! 1. **Transcripts** (`<config>/projects/<project>/<session>.jsonl` and
//!    `<session>/subagents/agent-*.jsonl`): Claude-Code-shaped JSONL. One
//!    assistant reply is written as several `type:"assistant"` lines sharing
//!    `message.id`, and only the last one carries `message.usage`. Requests are
//!    keyed by `message.usage.request_id` so a forked session replaying the
//!    parent's history cannot count a request twice.
//! 2. **Segment logs** (`<config>/logs/sessions/<project>/<session>/segments/*.jsonl`):
//!    Qoder CLI 1.1.13+ writes requests made with `--no-session-persistence`
//!    here only, so a transcript-only reader silently loses that usage. Only
//!    `type:"model.response.completed"` is a per-request record; the
//!    `turn.finished` roll-up is ignored so a turn is never counted twice.
//! 3. **IDE SQLite** (`…/SharedClientCache/cache/db/local.db`): the desktop IDE
//!    persists `chat_message.token_info` (`prompt_tokens` already *includes*
//!    `cached_tokens`) even when its embedded agent reports zeroed token
//!    columns on the stream-json channel.
//!
//! # What is counted
//!
//! Only token counts Qoder actually recorded. Managed (first-party) models bill
//! in credits: Qoder zeroes every token field and leaves `credits` /
//! `context_usage_ratio` behind, or omits `usage` entirely (schema v1.1.5).
//! Those rows are **not** estimated from reply length, `credits`, or context
//! occupancy — they produce no message at all. `QODER_EXPOSE_TOKEN_USAGE=1`
//! (international) / `QODERCN_EXPOSE_TOKEN_USAGE=1` (China) make Qoder record
//! real counters for *subsequent* requests; nothing backfills history.
//!
//! # Cache-read accounting
//!
//! Qoder's agent engine reports `input_tokens` as the whole prompt, with
//! `cache_read_input_tokens` a subset of it (OpenAI/`prompt_tokens` semantics,
//! unlike Anthropic's net `input_tokens`). The recorded
//! `context_usage_ratio == input_tokens / context_window` relationship proves
//! it on real 1.1.76+ transcripts, and it matches the Go engine's documented
//! usage conversion in usageBar. Net input is therefore `input_tokens -
//! cache_read_input_tokens`, and the cached subset is never added twice. The
//! IDE's `token_info.prompt_tokens` carries the same relationship.
//!
//! # Model attribution
//!
//! `message.model` wins; Qoder 1.1.82+ writes `"auto"` there and puts the real
//! slot in the preceding `user.businessModelKey`, with mid-session switches in
//! `runtime-config.model`. Slot identifiers (`qmodel`, `qfmodel`, `dfmodel`,
//! `gfmodel`, …) are routing slots whose backing model changes over time, so
//! they are kept verbatim and attributed to the `qoder` provider — never mapped
//! to a guessed vendor, and never priced through another client's `auto` entry.
//! BYOK models keep their vendor (`deepseek/chat/deepseek-v3` →
//! `deepseek-v3` + `deepseek`), including when Qoder qualifies them with its own
//! name (`qoder/claude-sonnet-4-6` → `claude-sonnet-4-6` + `anthropic`).

use super::utils::{
    extract_i64, file_modified_timestamp_ms, for_each_json_line, open_readonly_sqlite,
    parse_timestamp_value, sqlite_for_each_row_on, SqliteScan,
};
use super::{
    normalize_agent_name, normalize_workspace_key, workspace_label_from_key, UnifiedMessage,
};
use crate::TokenBreakdown;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Bumped whenever a change makes byte-identical input parse differently.
///
/// - Version 1: initial implementation.
pub const QODER_PARSER_BASE_VERSION: u32 = 1;

/// Public client id. Both editions report under it.
pub const CLIENT_ID: &str = "qoder";

/// Provider recorded for rows whose slot names a router, not a vendor.
const QODER_PROVIDER: &str = "qoder";

/// Model recorded when the record names none at all.
const DEFAULT_MODEL: &str = "unknown";

/// `BUSY` wait (ms) for the IDE database, which the IDE may hold write-locked.
const QODER_SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;

/// Which edition a path belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoderSource {
    International,
    China,
}

impl QoderSource {
    /// Client/source spelling used in dedup keys and diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            QoderSource::International => "qoder",
            QoderSource::China => "qoder-cn",
        }
    }

    /// Which edition a path belongs to, from the directory names Qoder writes.
    ///
    /// The China edition uses `.qoder-cn` for the CLI root and `QoderCN` /
    /// `com.qodercn.app.stable` for the desktop app. A root redirected through
    /// `QODERCN_CONFIG_DIR` carries none of those markers; the edition then only
    /// affects the dedup namespace, which is still unique per file, and both
    /// editions share the same token semantics, so the fallback is harmless.
    pub fn from_path(path: &Path) -> Self {
        let looks_china = path.components().any(|component| {
            let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
            name == ".qoder-cn" || name == "qodercn" || name.starts_with("com.qodercn.")
        });
        if looks_china {
            QoderSource::China
        } else {
            QoderSource::International
        }
    }
}

/// True for `.jsonl` files under a `segments/` directory.
pub fn is_qoder_segment_path(path: &Path) -> bool {
    path.extension()
        .map(|ext| ext.eq_ignore_ascii_case("jsonl"))
        .unwrap_or(false)
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name.eq_ignore_ascii_case("segments"))
}

/// True for Qoder's non-transcript bookkeeping files that share `.jsonl`.
///
/// `subagents/**/journal.jsonl` is a subagent run journal and
/// `<session>.checkpoints.jsonl` a checkpoint index; neither carries usage.
pub fn should_skip_qoder_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    if file_name.ends_with(".checkpoints.jsonl") {
        return true;
    }
    file_name.eq_ignore_ascii_case("journal.jsonl")
        && path.ancestors().any(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("subagents"))
        })
}

/// Routing slots. These name a router or a moving business slot, not a model.
fn is_routing_slot(model: &str) -> bool {
    let lower = model.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "auto"
            | "ultimate"
            | "performance"
            | "efficient"
            | "lite"
            | "qmodel"
            | "qfmodel"
            | "dfmodel"
            | "gfmodel"
            | "kmodel"
            | "gmodel"
            | "mmodel"
            | "q35model"
    ) || lower.starts_with("qmodel_")
        || lower.starts_with("qmodel-")
}

fn is_synthetic_placeholder_model(model: &str) -> bool {
    let trimmed = model.trim();
    trimmed.is_empty() || trimmed.eq_ignore_ascii_case("<synthetic>")
}

/// Split a client-qualified or vendor-qualified model into its bare id.
///
/// `qoder/claude-sonnet-4-6` and `qoder-cn/deepseek-v3` are Qoder's own
/// namespaces, not providers; `deepseek/chat/deepseek-v3` is
/// `<provider>/<type>/<model>`.
fn split_model_qualifier(model: &str) -> (String, Option<String>) {
    let trimmed = model.trim();
    let Some((head, _)) = trimmed.split_once('/') else {
        return (trimmed.to_string(), None);
    };
    let client_namespace =
        head.eq_ignore_ascii_case("qoder") || head.eq_ignore_ascii_case("qoder-cn");
    let bare = trimmed
        .rsplit('/')
        .next()
        .map(str::trim)
        .filter(|bare| !bare.is_empty())
        .unwrap_or(trimmed);
    if client_namespace {
        (bare.to_string(), None)
    } else {
        (
            bare.to_string(),
            crate::provider_identity::canonical_provider(head),
        )
    }
}

/// Resolve the recorded model string into `(model_id, provider_id)`.
fn resolve_model_and_provider(
    raw_model: &str,
    explicit_provider: Option<&str>,
) -> (String, String) {
    let (bare, prefix_provider) = split_model_qualifier(raw_model);
    if bare.is_empty() {
        return (DEFAULT_MODEL.to_string(), QODER_PROVIDER.to_string());
    }

    // A routing slot has no evidence-backed vendor. Both the model id and the
    // provider stay Qoder's own so no price table (including another client's
    // `auto` row) can claim it.
    if is_routing_slot(&bare) {
        return (bare, QODER_PROVIDER.to_string());
    }

    let explicit = explicit_provider
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
        .filter(|provider| {
            !provider.eq_ignore_ascii_case("custom")
                && !provider.eq_ignore_ascii_case(QODER_PROVIDER)
                && !provider.eq_ignore_ascii_case("qoder-cn")
        })
        .and_then(crate::provider_identity::canonical_provider);

    let provider = explicit
        .or(prefix_provider)
        .or_else(|| {
            crate::provider_identity::inferred_provider_from_model_delimited(&bare)
                .map(str::to_string)
        })
        .unwrap_or_else(|| QODER_PROVIDER.to_string());

    (bare, provider)
}

/// Token counters from one `usage` block, already net of the cached subset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct QoderUsage {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    cache_write_1h: i64,
}

impl QoderUsage {
    fn total(self) -> i64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }

    fn merge_max(&mut self, other: Self) {
        self.input = self.input.max(other.input);
        self.output = self.output.max(other.output);
        self.cache_read = self.cache_read.max(other.cache_read);
        self.cache_write = self.cache_write.max(other.cache_write);
        self.cache_write_1h = self.cache_write_1h.max(other.cache_write_1h);
    }

    fn to_breakdown(self) -> TokenBreakdown {
        TokenBreakdown {
            input: self.input,
            output: self.output,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            reasoning: 0,
        }
    }
}

fn first_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| extract_i64(value.get(*key)))
}

/// Parse one `usage`-shaped JSON object into a net token breakdown.
///
/// Returns `None` when the block names no token counter at all (the managed
/// model credits-only shape) or when every counter is zero — neither is
/// measured usage, and neither may be estimated.
fn usage_from_value(usage: &Value) -> Option<QoderUsage> {
    if !usage.is_object() {
        return None;
    }

    let input_raw = first_i64(
        usage,
        &[
            "input_tokens",
            "inputTokens",
            "prompt_tokens",
            "promptTokens",
            "promptTokenCount",
        ],
    )?;
    let output_raw = first_i64(
        usage,
        &[
            "output_tokens",
            "outputTokens",
            "completion_tokens",
            "completionTokens",
            "candidatesTokenCount",
        ],
    )
    .unwrap_or(0);
    let cache_read_raw = first_i64(
        usage,
        &[
            "cache_read_input_tokens",
            "cacheReadInputTokens",
            "cache_read_tokens",
            "cacheReadTokens",
            "cached_tokens",
            "cachedTokens",
            "cachedContentTokenCount",
        ],
    )
    .unwrap_or(0);
    let cache_write_raw = first_i64(
        usage,
        &[
            "cache_creation_input_tokens",
            "cacheCreationInputTokens",
            "cache_write_tokens",
            "cacheWriteTokens",
        ],
    )
    .unwrap_or(0);

    let input_raw = input_raw.max(0);
    let output = output_raw.max(0);
    let cache_read = cache_read_raw.max(0).min(input_raw);
    let input = input_raw - cache_read;
    let cache_write = cache_write_raw.max(0);
    let cache_write_1h = cache_write_1h_from_value(usage, cache_write);

    let usage = QoderUsage {
        input,
        output,
        cache_read,
        cache_write,
        cache_write_1h,
    };
    (usage.total() > 0).then_some(usage)
}

/// The 1-hour cache-write subset, clamped to the recorded cache-write total.
fn cache_write_1h_from_value(usage: &Value, cache_write: i64) -> i64 {
    let parsed = usage
        .get("cache_creation")
        .and_then(|split| {
            first_i64(
                split,
                &["ephemeral_1h_input_tokens", "ephemeral1hInputTokens"],
            )
        })
        .unwrap_or(0)
        .max(0);
    parsed.min(cache_write)
}

fn usage_request_id(usage: &Value) -> Option<String> {
    ["request_id", "requestId"]
        .iter()
        .find_map(|key| usage.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// One assembled assistant turn.
struct Turn {
    /// Identity recorded in the dedup key: the request id when the reply
    /// carried one, else the message id, else the file-position anchor.
    key: String,
    session_id: String,
    agent: Option<String>,
    workspace_key: Option<String>,
    workspace_label: Option<String>,
    model_id: String,
    provider_id: String,
    timestamp: i64,
    last_timestamp: i64,
    duration_ms: Option<i64>,
    is_turn_start: bool,
    usage: Option<QoderUsage>,
}

impl Turn {
    fn into_message(self, source: QoderSource) -> Option<UnifiedMessage> {
        let usage = self.usage?;
        if usage.total() <= 0 {
            return None;
        }
        let mut message = UnifiedMessage::new_with_agent(
            CLIENT_ID,
            self.model_id,
            self.provider_id,
            self.session_id,
            self.timestamp,
            usage.to_breakdown(),
            0.0,
            self.agent,
        );
        message.dedup_key = Some(format!("{CLIENT_ID}:{}:{}", source.as_str(), self.key));
        message.duration_ms = self.duration_ms;
        message.is_turn_start = self.is_turn_start;
        message.cache_write_1h = usage.cache_write_1h;
        message.set_workspace(self.workspace_key, self.workspace_label);
        Some(message)
    }
}

fn workspace_for_cwd(cwd: &str) -> Option<(String, Option<String>)> {
    let key = normalize_workspace_key(cwd)?;
    let label = workspace_label_from_key(&key);
    Some((key, label))
}

/// Project workspace from a transcript path.
///
/// `<config>/projects/<project>/<session>.jsonl` and
/// `<project>/<session>/subagents/agent-*.jsonl` both name the project
/// directory. A pre-project-version transcript directly under `projects/` has
/// no project, and the layout directory itself must never be reported as one.
fn workspace_key_from_qoder_path(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    let components: Vec<&str> = parent
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();

    for window in components.windows(2).rev() {
        if window[0].eq_ignore_ascii_case("projects")
            && !window[1].is_empty()
            && !window[1].eq_ignore_ascii_case("subagents")
        {
            if let Some(key) = normalize_workspace_key(window[1]) {
                return Some(key);
            }
        }
    }

    None
}

/// Session id recorded in a segment path:
/// `<config>/logs/sessions/<project>/<session>/segments/<file>.jsonl`.
fn segment_session_id(path: &Path) -> Option<String> {
    path.parent()?
        .parent()?
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Subagent identity for a transcript: `(normalized name, agent id)`.
fn subagent_context(path: &Path) -> Option<(String, Option<String>)> {
    let in_subagents_dir = path.ancestors().any(|ancestor| {
        ancestor
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("subagents"))
    });
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let agent_id = stem
        .strip_prefix("agent-")
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);

    if !in_subagents_dir && agent_id.is_none() {
        return None;
    }

    if !stem.is_empty() {
        let meta_path = path.with_file_name(format!("{stem}.meta.json"));
        if let Ok(text) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<Value>(&text) {
                let agent_type = meta
                    .get("agentType")
                    .or_else(|| meta.get("agent_type"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|agent| !agent.is_empty());
                if let Some(agent_type) = agent_type {
                    return Some((normalize_agent_name(agent_type), agent_id));
                }
            }
        }
    }

    if let Some(id) = agent_id.clone() {
        return Some((normalize_agent_name(&id), agent_id));
    }

    Some((normalize_agent_name("qoder-subagent"), None))
}

fn content_len(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Array(blocks)) if !blocks.is_empty())
        || matches!(value, Some(Value::String(text)) if !text.trim().is_empty())
}

fn is_tool_result_only(content: Option<&Value>) -> bool {
    let Some(blocks) = content.and_then(Value::as_array) else {
        return false;
    };
    let mut has_tool_result = false;
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_result") => has_tool_result = true,
            Some(_) => return false,
            None => return false,
        }
    }
    has_tool_result
}

/// Parse a Qoder or Qoder CN transcript into one message per model request.
pub fn parse_qoder_transcript_file(path: &Path) -> Vec<UnifiedMessage> {
    if should_skip_qoder_file(path) || is_qoder_segment_path(path) {
        return Vec::new();
    }

    let source = QoderSource::from_path(path);
    let fallback_timestamp = file_modified_timestamp_ms(path);
    let fallback_session_id = super::utils::session_id_from_path(path);
    let path_workspace_key = workspace_key_from_qoder_path(path);
    let subagent = subagent_context(path);

    let mut session_id: Option<String> = None;
    let mut workspace: Option<(String, Option<String>)> = path_workspace_key
        .as_deref()
        .map(|key| (key.to_string(), workspace_label_from_key(key)));

    // Session-wide model state.
    let mut runtime_model: Option<String> = None;
    let mut last_business_model: Option<String> = None;
    let mut active_model: Option<String> = None;
    let mut active_provider: Option<String> = None;

    // Per-turn state.
    let mut pending_turn_start = false;
    let mut pending_prompt_ms: Option<i64> = None;

    let mut turns: Vec<Turn> = Vec::new();
    let mut index_by_key: HashMap<String, usize> = HashMap::new();
    let mut anonymous_turns: usize = 0;

    for_each_json_line(path, &mut |line_index, trimmed| {
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            return;
        };
        let entry_type = value
            .get("type")
            .or_else(|| value.get("role"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let message = value.get("message");

        if session_id.is_none() {
            let recorded = value
                .get("sessionId")
                .or_else(|| value.get("session_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty());
            if let Some(recorded) = recorded {
                session_id = Some(recorded.to_string());
            }
        }

        if let Some(cwd) = value
            .get("cwd")
            .or_else(|| value.get("relocatedCwd"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|cwd| !cwd.is_empty())
        {
            if let Some(resolved) = workspace_for_cwd(cwd) {
                workspace = Some(resolved);
            }
        }

        match entry_type {
            "runtime-config" => {
                if let Some(model) = value
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|model| {
                        !is_synthetic_placeholder_model(model)
                            && !model.eq_ignore_ascii_case("auto")
                    })
                {
                    runtime_model = Some(model.to_string());
                    active_model = Some(model.to_string());
                    last_business_model = Some(model.to_string());
                }
                if let Some(provider) = value
                    .get("provider")
                    .or_else(|| value.get("providerId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|provider| !provider.is_empty())
                {
                    active_provider = Some(provider.to_string());
                }
            }
            "user" => {
                let business_model = value
                    .get("businessModelKey")
                    .or_else(|| message.and_then(|message| message.get("businessModelKey")))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|model| !model.is_empty() && !model.eq_ignore_ascii_case("auto"));
                if let Some(business_model) = business_model {
                    last_business_model = Some(business_model.to_string());
                    active_model = Some(business_model.to_string());
                }

                let content = message
                    .and_then(|message| message.get("content"))
                    .or_else(|| value.get("content"));
                let is_synthetic = value
                    .get("isSynthetic")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || value
                        .get("isMeta")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                if !is_synthetic && !is_tool_result_only(content) && content_len(content) {
                    pending_turn_start = true;
                    pending_prompt_ms = value.get("timestamp").and_then(parse_timestamp_value);
                }
            }
            "assistant" => {
                if value
                    .get("isApiErrorMessage")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    return;
                }

                let fragment_model = message
                    .and_then(|message| message.get("model"))
                    .or_else(|| value.get("model"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|model| !is_synthetic_placeholder_model(model));

                let fragment_business = value
                    .get("businessModelKey")
                    .or_else(|| message.and_then(|message| message.get("businessModelKey")))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|model| !model.is_empty() && !model.eq_ignore_ascii_case("auto"));

                if let Some(business_model) = fragment_business {
                    last_business_model = Some(business_model.to_string());
                }
                if let Some(model) =
                    fragment_model.filter(|model| !model.eq_ignore_ascii_case("auto"))
                {
                    active_model = Some(model.to_string());
                }

                let raw_model = match fragment_model {
                    Some(model) if !model.eq_ignore_ascii_case("auto") => model.to_string(),
                    _ => fragment_business
                        .or(last_business_model.as_deref())
                        .or(runtime_model.as_deref())
                        .or(active_model.as_deref())
                        .or(fragment_model)
                        .unwrap_or(DEFAULT_MODEL)
                        .to_string(),
                };

                let explicit_provider = message
                    .and_then(|message| {
                        message
                            .get("providerId")
                            .or_else(|| message.get("provider_id"))
                            .or_else(|| message.get("provider"))
                    })
                    .or_else(|| {
                        value
                            .get("providerId")
                            .or_else(|| value.get("provider_id"))
                            .or_else(|| value.get("provider"))
                    })
                    .and_then(Value::as_str)
                    .or(active_provider.as_deref());

                let usage_value = message
                    .and_then(|message| message.get("usage"))
                    .or_else(|| value.get("usage"))
                    .or_else(|| value.get("token_usage"));
                let usage = usage_value.and_then(usage_from_value);

                let fragment_ts = value.get("timestamp").and_then(parse_timestamp_value);

                let message_id = message
                    .and_then(|message| message.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty());
                let request_id = value
                    .get("requestId")
                    .or_else(|| value.get("request_id"))
                    .and_then(Value::as_str)
                    .and_then(|id| {
                        let id = id.trim();
                        (!id.is_empty()).then_some(id)
                    })
                    .map(str::to_string)
                    .or_else(|| usage_value.and_then(usage_request_id));
                let uuid = value
                    .get("uuid")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|uuid| !uuid.is_empty());

                let resolved_session = session_id
                    .clone()
                    .unwrap_or_else(|| fallback_session_id.clone());

                let group_key = if let Some(message_id) = message_id {
                    format!("msg:{message_id}")
                } else if let Some(request_id) = &request_id {
                    format!("req:{request_id}")
                } else if let Some(uuid) = uuid {
                    format!("uuid:{uuid}")
                } else {
                    anchor_anonymous_turn(&mut anonymous_turns, &resolved_session, line_index)
                };
                let turn_key = match &request_id {
                    Some(request_id) => format!("req:{request_id}"),
                    None => group_key.clone(),
                };

                if let Some(existing) = index_by_key.get(&group_key).copied() {
                    let turn = &mut turns[existing];
                    if request_id.is_some() {
                        turn.key = turn_key.clone();
                    }
                    if let Some(usage) = usage {
                        match &mut turn.usage {
                            Some(existing) => existing.merge_max(usage),
                            None => turn.usage = Some(usage),
                        }
                    }
                    if let Some(ts) = fragment_ts {
                        turn.last_timestamp = turn.last_timestamp.max(ts);
                        let span = turn.last_timestamp.saturating_sub(turn.timestamp);
                        if span > 0 {
                            turn.duration_ms = Some(span);
                        }
                    }
                    pending_turn_start = false;
                    pending_prompt_ms = None;
                    return;
                }

                let is_sidechain = value
                    .get("isSidechain")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let entry_agent_id = value
                    .get("agentId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty());

                let (agent, effective_session) = match (&subagent, is_sidechain) {
                    (Some((name, id)), _) => {
                        let child_session = id
                            .clone()
                            .or_else(|| entry_agent_id.map(str::to_string))
                            .map(|id| format!("{resolved_session}:agent:{id}"))
                            .unwrap_or_else(|| resolved_session.clone());
                        (Some(name.clone()), child_session)
                    }
                    (None, true) => {
                        let child_session = entry_agent_id
                            .map(|id| format!("{resolved_session}:agent:{id}"))
                            .unwrap_or_else(|| resolved_session.clone());
                        (Some(normalize_agent_name("qoder-subagent")), child_session)
                    }
                    (None, false) => (None, resolved_session.clone()),
                };

                let timestamp = pending_prompt_ms
                    .or(fragment_ts)
                    .unwrap_or(fallback_timestamp);
                let last_timestamp = fragment_ts.unwrap_or(timestamp);
                let duration_ms = last_timestamp
                    .checked_sub(timestamp)
                    .filter(|span| *span > 0);
                let (model_id, provider_id) =
                    resolve_model_and_provider(&raw_model, explicit_provider);

                index_by_key.insert(group_key.clone(), turns.len());
                turns.push(Turn {
                    key: turn_key,
                    session_id: effective_session,
                    agent,
                    workspace_key: workspace.as_ref().map(|(key, _)| key.clone()),
                    workspace_label: workspace.as_ref().and_then(|(_, label)| label.clone()),
                    model_id,
                    provider_id,
                    timestamp,
                    last_timestamp,
                    duration_ms,
                    is_turn_start: pending_turn_start,
                    usage,
                });

                pending_turn_start = false;
                pending_prompt_ms = None;
            }
            _ => {}
        }
    });

    let (messages, without_usage) = collect_measured_turns(turns, source);
    if without_usage > 0 {
        // Not a fault: managed-model rows record credits and zero out (or omit)
        // every token counter. Saying so at debug level is the whole diagnostic
        // story — the rows are deliberately absent, never estimated.
        tracing::debug!(
            path = %path.display(),
            turns_without_recorded_usage = without_usage,
            "Qoder transcript rows with no recorded token counts were skipped"
        );
    }
    messages
}

/// Convert turns to messages, reporting how many had no recorded usage.
fn collect_measured_turns(turns: Vec<Turn>, source: QoderSource) -> (Vec<UnifiedMessage>, usize) {
    let mut messages = Vec::with_capacity(turns.len());
    let mut without_usage = 0usize;
    for turn in turns {
        match turn.into_message(source) {
            Some(message) => messages.push(message),
            None => without_usage += 1,
        }
    }
    (messages, without_usage)
}

fn anchor_anonymous_turn(counter: &mut usize, session_id: &str, line_index: usize) -> String {
    let anchor = format!("anon:{session_id}:{line_index}");
    *counter += 1;
    anchor
}

/// Parse a structured segment log into one message per completed model request.
///
/// Only `model.response.completed` events are per-request records. The
/// `turn.finished` roll-up carries the same counters a second time and is
/// deliberately ignored.
pub fn parse_qoder_segment_file(path: &Path) -> Vec<UnifiedMessage> {
    if should_skip_qoder_file(path) || !is_qoder_segment_path(path) {
        return Vec::new();
    }

    let source = QoderSource::from_path(path);
    let fallback_timestamp = file_modified_timestamp_ms(path);
    let fallback_session = segment_session_id(path)
        .or_else(|| path.parent().map(super::utils::session_id_from_path))
        .unwrap_or_else(|| "unknown".to_string());

    let mut session_id: Option<String> = None;
    let mut workspace: Option<(String, Option<String>)> = None;
    let mut requests: Vec<Turn> = Vec::new();
    let mut index_by_key: HashMap<String, usize> = HashMap::new();
    let mut anonymous_events: usize = 0;
    let mut rollups_with_tokens: usize = 0;

    for_each_json_line(path, &mut |line_index, trimmed| {
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            return;
        };
        let entry_type = value.get("type").and_then(Value::as_str).unwrap_or("");

        if entry_type == "session.config.loaded" {
            if workspace.is_none() {
                if let Some(data) = value.get("data") {
                    for key in ["project_root", "target_dir"] {
                        if let Some(root) = data.get(key).and_then(Value::as_str) {
                            if let Some(resolved) = workspace_for_cwd(root) {
                                workspace = Some(resolved);
                                break;
                            }
                        }
                    }
                }
            }
            return;
        }

        if entry_type == "turn.finished" {
            // A whole-turn roll-up. Adding it to the per-request events above
            // would double count a turn, so it is never counted — but a build
            // that stops emitting `model.response.completed` and only fills
            // this row would silently lose usage, which is worth flagging.
            if value
                .get("data")
                .and_then(usage_from_value)
                .is_some_and(|usage| usage.total() > 0)
            {
                rollups_with_tokens += 1;
            }
            return;
        }

        if entry_type != "model.response.completed" {
            return;
        }

        let Some(data) = value.get("data") else {
            return;
        };
        let Some(usage) = usage_from_value(data) else {
            return;
        };

        let recorded_session = value
            .get("session_id")
            .or_else(|| value.get("sessionId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty());
        if let Some(recorded) = recorded_session {
            session_id = Some(recorded.to_string());
        }

        let request_id = value
            .get("request_id")
            .or_else(|| data.get("request_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string);

        let event_key = match &request_id {
            Some(request_id) => format!("req:{request_id}"),
            None => {
                // No stable request identity: anchor on the event's own file
                // position so distinct events never merge, and accept that a
                // replayed copy in another file cannot be recognized.
                anonymous_events += 1;
                format!(
                    "anon:{}:{line_index}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("segment")
                )
            }
        };

        let timestamp = value
            .get("ts")
            .or_else(|| value.get("timestamp"))
            .and_then(parse_timestamp_value)
            .unwrap_or(fallback_timestamp);

        let raw_model = data
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !is_synthetic_placeholder_model(model))
            .unwrap_or(DEFAULT_MODEL);
        let explicit_provider = data.get("provider").and_then(Value::as_str);
        let (model_id, provider_id) = resolve_model_and_provider(raw_model, explicit_provider);

        if let Some(existing) = index_by_key.get(&event_key).copied() {
            let turn = &mut requests[existing];
            if let Some(existing_usage) = &mut turn.usage {
                existing_usage.merge_max(usage);
            } else {
                turn.usage = Some(usage);
            }
            turn.timestamp = turn.timestamp.min(timestamp);
            turn.last_timestamp = turn.last_timestamp.max(timestamp);
            return;
        }

        index_by_key.insert(event_key.clone(), requests.len());
        requests.push(Turn {
            key: event_key,
            session_id: session_id
                .clone()
                .unwrap_or_else(|| fallback_session.clone()),
            agent: None,
            workspace_key: workspace.as_ref().map(|(key, _)| key.clone()),
            workspace_label: workspace.as_ref().and_then(|(_, label)| label.clone()),
            model_id,
            provider_id,
            timestamp,
            last_timestamp: timestamp,
            duration_ms: None,
            is_turn_start: false,
            usage: Some(usage),
        });
    });

    if rollups_with_tokens > 0 {
        tracing::debug!(
            path = %path.display(),
            rollups_with_token_counts = rollups_with_tokens,
            "Qoder turn roll-up events carry token counts but are never added to per-request usage"
        );
    }

    let (messages, without_usage) = collect_measured_turns(requests, source);
    if without_usage > 0 {
        tracing::debug!(
            path = %path.display(),
            requests_without_recorded_usage = without_usage,
            "Qoder segment events with no recorded token counts were skipped"
        );
    }
    messages
}

/// One IDE database read.
pub struct QoderIdeScan {
    pub messages: Vec<UnifiedMessage>,
    /// False when the database could not be opened or the scan stopped partway.
    ///
    /// A partial read is reported for this scan but must not be cached: cached,
    /// it would stand in for the whole table on every warm scan until the file
    /// happened to change.
    pub complete: bool,
}

/// Session/project metadata joined onto a `chat_message` row.
struct IdeRow {
    id: String,
    session_id: String,
    request_id: Option<String>,
    token_info: String,
    model_info: Option<String>,
    created: Option<i64>,
    project_uri: Option<String>,
    project_name: Option<String>,
    /// The session's configured model, used only when the message names none.
    preferred_model_info: Option<String>,
}

/// Read one text column, tolerating the numeric/BLOB spellings rusqlite would
/// otherwise reject outright.
fn text_column(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<String>> {
    use rusqlite::types::ValueRef;
    Ok(match row.get_ref(index)? {
        ValueRef::Null => None,
        ValueRef::Text(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Integer(value) => Some(value.to_string()),
        ValueRef::Real(value) => Some(value.to_string()),
        ValueRef::Blob(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
    })
}

/// Read the message creation time, which the IDE stores as milliseconds but
/// some builds persist as a numeric string.
fn timestamp_column(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<i64>> {
    use rusqlite::types::ValueRef;
    Ok(match row.get_ref(index)? {
        ValueRef::Null => None,
        ValueRef::Integer(value) => Some(value),
        ValueRef::Real(value) => Some(value as i64),
        ValueRef::Text(bytes) => std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| extract_i64(Some(&Value::String(text.trim().to_string())))),
        ValueRef::Blob(_) => None,
    })
}

fn collect_ide_rows(rows: &mut Vec<IdeRow>, row: &rusqlite::Row<'_>) -> rusqlite::Result<()> {
    rows.push(IdeRow {
        id: text_column(row, 0)?.unwrap_or_default(),
        session_id: text_column(row, 1)?.unwrap_or_default(),
        request_id: text_column(row, 2)?,
        token_info: text_column(row, 3)?.unwrap_or_default(),
        model_info: text_column(row, 4)?,
        created: timestamp_column(row, 5)?,
        project_uri: text_column(row, 6)?,
        project_name: text_column(row, 7)?,
        preferred_model_info: text_column(row, 8)?,
    });
    Ok(())
}

/// Read the IDE's local `chat_message` table.
///
/// Only token/model/timing columns are selected. Message bodies, tool output,
/// titles and credentials are never read.
pub fn scan_qoder_ide_db(db_path: &Path) -> QoderIdeScan {
    let conn = match open_readonly_sqlite(db_path) {
        Ok(conn) => conn,
        Err(err) => {
            tracing::debug!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to open Qoder IDE database"
            );
            return QoderIdeScan {
                messages: Vec::new(),
                complete: false,
            };
        }
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(
        QODER_SQLITE_BUSY_TIMEOUT_MS,
    ));

    // The `chat_session` join supplies the project/workspace attribution. Older
    // Qoder CN builds have no `chat_session` table at all; usage is still valid
    // without it, so that case degrades to the un-attributed query instead of
    // dropping the source.
    const JOINED_SQL: &str = "SELECT m.id, m.session_id, m.request_id, m.token_info, m.model_info,          m.gmt_create, s.project_uri, s.project_name, s.preferred_model_info          FROM chat_message AS m LEFT JOIN chat_session AS s ON s.session_id = m.session_id          WHERE m.role = 'assistant'";
    const PLAIN_SQL: &str = "SELECT id, session_id, request_id, token_info, model_info, gmt_create,          NULL, NULL, NULL FROM chat_message WHERE role = 'assistant'";

    let mut rows: Vec<IdeRow> = Vec::new();
    let mut scan = sqlite_for_each_row_on(&conn, db_path, JOINED_SQL, None, &mut |row| {
        collect_ide_rows(&mut rows, row)
    });
    if !scan.prepared() {
        rows.clear();
        scan = sqlite_for_each_row_on(&conn, db_path, PLAIN_SQL, None, &mut |row| {
            collect_ide_rows(&mut rows, row)
        });
    }

    if matches!(scan, SqliteScan::NotPrepared | SqliteScan::NotOpened) {
        // No `chat_message` table: a pre-usage schema, which is an empty source
        // rather than an unread one.
        return QoderIdeScan {
            messages: Vec::new(),
            complete: true,
        };
    }

    let mut messages = Vec::new();
    let mut without_usage = 0usize;
    for row in &rows {
        match ide_row_message(row) {
            Some(message) => messages.push(message),
            None => without_usage += 1,
        }
    }
    if without_usage > 0 {
        // Interrupted or errored chats persist an empty `token_info`.
        tracing::debug!(
            db_path = %db_path.display(),
            rows_without_recorded_usage = without_usage,
            "Qoder IDE rows with no recorded token counts were skipped"
        );
    }

    QoderIdeScan {
        messages,
        complete: scan.completed(),
    }
}

fn ide_row_message(row: &IdeRow) -> Option<UnifiedMessage> {
    let tokens = parse_ide_token_info(&row.token_info)?;

    let timestamp = row
        .created
        .filter(|timestamp| *timestamp > 0)
        .map(|timestamp| {
            if timestamp >= 1_000_000_000_000 {
                timestamp
            } else {
                timestamp.saturating_mul(1000)
            }
        })?;

    let model_key = row
        .model_info
        .as_deref()
        .and_then(parse_ide_model_info)
        .or_else(|| {
            row.preferred_model_info
                .as_deref()
                .and_then(parse_ide_model_info)
        })
        .filter(|model| !is_synthetic_placeholder_model(model))
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let (model_id, provider_id) = resolve_model_and_provider(&model_key, None);

    let session_id = if row.session_id.trim().is_empty() {
        "unknown".to_string()
    } else {
        row.session_id.trim().to_string()
    };
    let dedup_key = match row.request_id.as_deref().map(str::trim) {
        Some(request_id) if !request_id.is_empty() => {
            format!("{CLIENT_ID}:ide:{session_id}:req:{request_id}")
        }
        _ => {
            let anchor = if row.id.trim().is_empty() {
                format!("{session_id}:{timestamp}")
            } else {
                row.id.trim().to_string()
            };
            format!("{CLIENT_ID}:ide:{anchor}")
        }
    };

    let mut message = UnifiedMessage::new_with_dedup(
        CLIENT_ID,
        model_id,
        provider_id,
        session_id,
        timestamp,
        tokens.to_breakdown(),
        0.0,
        Some(dedup_key),
    );
    message.set_workspace(ide_workspace_key(row), None);
    Some(message)
}

fn ide_workspace_key(row: &IdeRow) -> Option<String> {
    let uri = row
        .project_uri
        .as_deref()
        .map(str::trim)
        .filter(|uri| !uri.is_empty());
    if let Some(uri) = uri {
        let path = uri.strip_prefix("file://").unwrap_or(uri);
        if let Some(key) = normalize_workspace_key(path) {
            return Some(key);
        }
    }
    // `.` is Qoder's "no project" sentinel.
    row.project_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != ".")
        .map(str::to_string)
}

fn parse_ide_json(value: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(value).ok()?;
    parsed.is_object().then_some(parsed)
}

fn parse_ide_model_info(value: &str) -> Option<String> {
    let info = parse_ide_json(value)?;
    ["model_key", "modelKey", "model"]
        .iter()
        .find_map(|key| info.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}

/// `{"prompt_tokens":39005,"cached_tokens":37485,"completion_tokens":67}`.
///
/// `prompt_tokens` already includes `cached_tokens`, so the net input is the
/// difference. A row with no counters at all (an interrupted or errored chat)
/// is not measured usage.
fn parse_ide_token_info(value: &str) -> Option<QoderUsage> {
    if value.trim().is_empty() {
        return None;
    }
    usage_from_value(&parse_ide_json(value)?)
}

/// Candidate IDE databases, in preference order.
///
/// `QODER_HOME` / `QODER_CN_HOME` are Qoder's own IDE data-root overrides: when
/// one is set it replaces that edition's default location (as the community
/// implementations read it) rather than adding to it, so a redirected install
/// never reaches into the machine's real profile. Each override is accepted in
/// both documented spellings — the data root itself, or the full
/// `SharedClientCache/...` parent.
pub fn qoder_ide_db_candidates(app_data_root: &Path, use_env_roots: bool) -> Vec<PathBuf> {
    const RELATIVE_DB: &str = "SharedClientCache/cache/db/local.db";

    let env_root = |var: &str| -> Option<PathBuf> {
        if !use_env_roots {
            return None;
        }
        let value = std::env::var(var).ok()?;
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    };

    let mut candidates = Vec::new();
    for (var, edition_dir) in [("QODER_HOME", "Qoder"), ("QODER_CN_HOME", "QoderCN")] {
        match env_root(var) {
            Some(root) => {
                candidates.push(root.join("cache/db/local.db"));
                candidates.push(root.join(RELATIVE_DB));
            }
            None => candidates.push(app_data_root.join(edition_dir).join(RELATIVE_DB)),
        }
    }
    candidates
}

/// Merge duplicates and order the lane deterministically.
///
/// Qoder writes the same request more than once — a forked session replays the
/// parent's turns, a segment log repeats a completed event, and one request can
/// appear in both a transcript and a segment file. Every duplicate carries the
/// same dedup key (request id first), so collapsing on that key counts each
/// request once regardless of the order files were scanned in.
///
/// The surviving row keeps the earliest timestamp and the metadata recorded
/// with it; token counters merge by per-field maximum, which is monotonic in a
/// reply's fragments and never sums two observations of one request.
pub fn collapse_qoder_messages(messages: Vec<UnifiedMessage>) -> Vec<UnifiedMessage> {
    let mut ordered = messages;
    ordered.sort_by(|left, right| {
        let left_key = left.dedup_key.as_deref().unwrap_or("");
        let right_key = right.dedup_key.as_deref().unwrap_or("");
        left_key
            .cmp(right_key)
            .then(left.timestamp.cmp(&right.timestamp))
            .then(left.session_id.cmp(&right.session_id))
            .then(left.model_id.cmp(&right.model_id))
            .then(left.provider_id.cmp(&right.provider_id))
    });

    let mut collapsed: Vec<UnifiedMessage> = Vec::with_capacity(ordered.len());
    let mut index_by_key: HashMap<String, usize> = HashMap::new();
    for message in ordered {
        let Some(key) = message.dedup_key.clone() else {
            collapsed.push(message);
            continue;
        };
        match index_by_key.get(&key).copied() {
            Some(index) => {
                let existing = &mut collapsed[index];
                existing.tokens.input = existing.tokens.input.max(message.tokens.input);
                existing.tokens.output = existing.tokens.output.max(message.tokens.output);
                existing.tokens.cache_read =
                    existing.tokens.cache_read.max(message.tokens.cache_read);
                existing.tokens.cache_write =
                    existing.tokens.cache_write.max(message.tokens.cache_write);
                existing.tokens.reasoning = existing.tokens.reasoning.max(message.tokens.reasoning);
                existing.cache_write_1h = existing.cache_write_1h.max(message.cache_write_1h);
                existing.is_turn_start |= message.is_turn_start;
                if existing.duration_ms.is_none() {
                    existing.duration_ms = message.duration_ms;
                }
                if existing.workspace_key.is_none() {
                    existing.workspace_key = message.workspace_key;
                }
                if existing.workspace_label.is_none() {
                    existing.workspace_label = message.workspace_label;
                }
                if existing.agent.is_none() {
                    existing.agent = message.agent;
                }
            }
            None => {
                index_by_key.insert(key, collapsed.len());
                collapsed.push(message);
            }
        }
    }

    collapsed.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then(left.dedup_key.cmp(&right.dedup_key))
    });
    collapsed
}

/// Whether `path` is one of the two Qoder IDE database spellings.
pub fn is_qoder_ide_db(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "local.db")
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|dir| dir == "db")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_transcript(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        path
    }

    #[test]
    fn usage_treats_prompt_as_including_the_cached_subset() {
        let usage: Value = serde_json::from_str(
            r#"{"input_tokens":39005,"output_tokens":67,"cache_read_input_tokens":37485,
                "cache_creation_input_tokens":0}"#,
        )
        .unwrap();
        let parsed = usage_from_value(&usage).unwrap();
        assert_eq!(parsed.input, 1520);
        assert_eq!(parsed.cache_read, 37485);
        assert_eq!(parsed.output, 67);
        assert_eq!(parsed.total(), 39072);
    }

    #[test]
    fn usage_ignores_credit_only_and_zero_token_blocks() {
        let credits: Value = serde_json::from_str(
            r#"{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,
                "cache_creation_input_tokens":0,"credits":8.229,"billable":true}"#,
        )
        .unwrap();
        assert_eq!(usage_from_value(&credits), None);

        let credits_only: Value =
            serde_json::from_str(r#"{"credits":8.229,"billable":true,"context_usage_ratio":0.2}"#)
                .unwrap();
        assert_eq!(usage_from_value(&credits_only), None);
    }

    #[test]
    fn usage_clamps_impossible_cache_and_1h_subsets() {
        let usage: Value = serde_json::from_str(
            r#"{"input_tokens":100,"output_tokens":10,"cache_read_input_tokens":500,
                "cache_creation_input_tokens":20,
                "cache_creation":{"ephemeral_1h_input_tokens":80}}"#,
        )
        .unwrap();
        let parsed = usage_from_value(&usage).unwrap();
        // A cached subset larger than the prompt cannot be gross input, so the
        // prompt is kept whole and the subset is clamped to it.
        assert_eq!(parsed.cache_read, 100);
        assert_eq!(parsed.input, 0);
        assert_eq!(parsed.cache_write, 20);
        assert_eq!(parsed.cache_write_1h, 20);
    }

    #[test]
    fn routing_slots_never_borrow_a_vendor() {
        for slot in [
            "auto",
            "qmodel",
            "qmodel_38max",
            "qfmodel",
            "dfmodel",
            "gfmodel",
        ] {
            assert_eq!(
                resolve_model_and_provider(slot, None),
                (slot.to_string(), QODER_PROVIDER.to_string())
            );
        }
        assert_eq!(
            resolve_model_and_provider("deepseek/chat/deepseek-v3", None),
            ("deepseek-v3".to_string(), "deepseek".to_string())
        );
        assert_eq!(
            resolve_model_and_provider("qoder/claude-sonnet-4-6", None),
            ("claude-sonnet-4-6".to_string(), "anthropic".to_string())
        );
        assert_eq!(
            resolve_model_and_provider("qoder-cn/qwen3-coder-plus", None),
            ("qwen3-coder-plus".to_string(), "qwen".to_string())
        );
    }

    #[test]
    fn parses_multi_fragment_turn_once_with_request_key() {
        let dir = TempDir::new().unwrap();
        let project = dir.path().join(".qoder/projects/-work-app");
        let path = write_transcript(
            &project,
            "sess-1.jsonl",
            &[
                r#"{"type":"runtime-config","sessionId":"sess-1","model":"qmodel","timestamp":1794041573406}"#,
                r#"{"type":"user","uuid":"u1","sessionId":"sess-1","cwd":"/work/app","timestamp":"2026-11-07T10:00:00.000Z","message":{"role":"user","content":"hello"}}"#,
                r#"{"type":"assistant","uuid":"a1","sessionId":"sess-1","timestamp":"2026-11-07T10:00:01.000Z","message":{"id":"msg_1","role":"assistant","model":"qmodel","content":[{"type":"thinking","thinking":"plan"}]}}"#,
                r#"{"type":"assistant","uuid":"a2","sessionId":"sess-1","timestamp":"2026-11-07T10:00:04.000Z","message":{"id":"msg_1","role":"assistant","model":"qmodel","content":[{"type":"text","text":"done"}],"usage":{"input_tokens":1200,"output_tokens":40,"cache_read_input_tokens":800,"cache_creation_input_tokens":100,"cache_creation":{"ephemeral_1h_input_tokens":50},"request_id":"req_1"}}}"#,
            ],
        );

        let messages = parse_qoder_transcript_file(&path);
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.client, "qoder");
        assert_eq!(message.model_id, "qmodel");
        assert_eq!(message.provider_id, "qoder");
        assert_eq!(message.session_id, "sess-1");
        assert_eq!(message.tokens.input, 400);
        assert_eq!(message.tokens.output, 40);
        assert_eq!(message.tokens.cache_read, 800);
        assert_eq!(message.tokens.cache_write, 100);
        assert_eq!(message.cache_write_1h, 50);
        assert_eq!(message.tokens.reasoning, 0);
        assert_eq!(message.timestamp, 1_794_045_600_000);
        assert_eq!(message.duration_ms, Some(4000));
        assert!(message.is_turn_start);
        assert_eq!(message.workspace_key.as_deref(), Some("/work/app"));
        assert_eq!(message.workspace_label.as_deref(), Some("app"));
        assert_eq!(message.dedup_key.as_deref(), Some("qoder:qoder:req:req_1"));
    }

    #[test]
    fn zeroed_and_missing_usage_produce_no_message() {
        let dir = TempDir::new().unwrap();
        let project = dir.path().join(".qoder-cn/projects/-work-app");
        let path = write_transcript(
            &project,
            "sess-cn.jsonl",
            &[
                r#"{"type":"runtime-config","sessionId":"sess-cn","model":"qmodel_preview","timestamp":1784988512000}"#,
                r#"{"type":"user","uuid":"u1","sessionId":"sess-cn","timestamp":"2026-07-25T14:08:32.000Z","message":{"role":"user","content":"hello"}}"#,
                r#"{"type":"assistant","uuid":"a1","sessionId":"sess-cn","timestamp":"2026-07-25T14:12:05.000Z","message":{"id":"msg_no_usage","role":"assistant","model":"qmodel_preview","content":[{"type":"text","text":"reply"}]}}"#,
                r#"{"type":"assistant","uuid":"a2","sessionId":"sess-cn","timestamp":"2026-07-25T14:12:06.000Z","message":{"id":"msg_credits","role":"assistant","model":"qmodel_preview","content":[{"type":"text","text":"reply"}],"usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"credits":1.25,"billable":true}}}"#,
            ],
        );

        assert!(parse_qoder_transcript_file(&path).is_empty());
        assert_eq!(QoderSource::from_path(&path), QoderSource::China);
    }

    #[test]
    fn resolves_auto_model_from_business_key_and_follows_switches() {
        let dir = TempDir::new().unwrap();
        let project = dir.path().join(".qoder-cn/projects/-work-app");
        let path = write_transcript(
            &project,
            "sess-auto.jsonl",
            &[
                r#"{"type":"runtime-config","sessionId":"sess-auto","model":"gfmodel","timestamp":1794041573406}"#,
                r#"{"type":"user","uuid":"u1","sessionId":"sess-auto","businessModelKey":"gfmodel","timestamp":"2026-11-07T08:56:16.000Z","message":{"role":"user","content":"run tests"}}"#,
                r#"{"type":"assistant","uuid":"a1","sessionId":"sess-auto","timestamp":"2026-11-07T08:56:20.000Z","message":{"id":"msg_auto_1","role":"assistant","model":"auto","content":[{"type":"text","text":"one"}],"usage":{"input_tokens":28000,"output_tokens":270,"cache_read_input_tokens":9000,"request_id":"req_auto_1"}}}"#,
                r#"{"type":"user","uuid":"u2","sessionId":"sess-auto","timestamp":"2026-11-07T08:56:25.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","content":"ok"}]}}"#,
                r#"{"type":"runtime-config","sessionId":"sess-auto","model":"dfmodel","timestamp":1794041600000}"#,
                r#"{"type":"assistant","uuid":"a2","sessionId":"sess-auto","timestamp":"2026-11-07T08:57:00.000Z","message":{"id":"msg_auto_2","role":"assistant","model":"auto","content":[{"type":"text","text":"two"}],"usage":{"input_tokens":32000,"output_tokens":150,"cache_read_input_tokens":12000,"request_id":"req_auto_2"}}}"#,
            ],
        );

        let messages = parse_qoder_transcript_file(&path);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].model_id, "gfmodel");
        assert_eq!(messages[0].provider_id, "qoder");
        assert_eq!(messages[0].tokens.input, 19000);
        assert_eq!(messages[0].tokens.cache_read, 9000);
        assert_eq!(messages[0].tokens.output, 270);
        assert!(messages[0].is_turn_start);
        assert!(!messages[1].is_turn_start);
        assert_eq!(messages[1].model_id, "dfmodel");
        assert_eq!(messages[1].tokens.input, 20000);
        assert_eq!(messages[1].tokens.cache_read, 12000);
    }

    #[test]
    fn subagent_transcript_keeps_parent_and_agent() {
        let dir = TempDir::new().unwrap();
        let session_dir = dir
            .path()
            .join(".qoder-cn/projects/-repo/sess-main/subagents");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join("agent-abc.meta.json"),
            r#"{"agentType":"code-reviewer"}"#,
        )
        .unwrap();
        let path = write_transcript(
            &session_dir,
            "agent-abc.jsonl",
            &[
                r#"{"type":"assistant","isSidechain":true,"sessionId":"sess-main","agentId":"abc","timestamp":"2026-11-07T11:00:00.000Z","message":{"id":"msg_sub","model":"deepseek/chat/deepseek-v3","usage":{"input_tokens":500,"output_tokens":200,"request_id":"req_sub"}}}"#,
            ],
        );

        let messages = parse_qoder_transcript_file(&path);
        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.agent.as_deref(), Some("Code Reviewer"));
        assert_eq!(message.session_id, "sess-main:agent:abc");
        assert_eq!(message.model_id, "deepseek-v3");
        assert_eq!(message.provider_id, "deepseek");
        assert_eq!(message.workspace_key.as_deref(), Some("-repo"));
    }

    #[test]
    fn skips_journal_and_checkpoint_files() {
        let dir = TempDir::new().unwrap();
        let subagents = dir.path().join(".qoder/projects/-repo/sess/subagents");
        let journal = write_transcript(
            &subagents,
            "journal.jsonl",
            &[
                r#"{"type":"assistant","message":{"id":"j","model":"qmodel","usage":{"input_tokens":9,"output_tokens":9}}}"#,
            ],
        );
        assert!(parse_qoder_transcript_file(&journal).is_empty());

        let checkpoints = write_transcript(
            &dir.path().join(".qoder/projects/-repo"),
            "sess.checkpoints.jsonl",
            &[
                r#"{"type":"assistant","message":{"id":"c","model":"qmodel","usage":{"input_tokens":9,"output_tokens":9}}}"#,
            ],
        );
        assert!(parse_qoder_transcript_file(&checkpoints).is_empty());
    }

    #[test]
    fn malformed_and_half_written_lines_do_not_drop_valid_records() {
        let dir = TempDir::new().unwrap();
        let project = dir.path().join(".qoder/projects/-repo");
        let path = write_transcript(
            &project,
            "sess-broken.jsonl",
            &[
                r#"{"type":"assistant","sessionId":"sess-broken","timestamp":"2026-11-07T10:00:00.000Z","message":{"id":"m1","model":"qmodel","usage":{"input_tokens":100,"output_tokens":10,"request_id":"req_1"}}}"#,
                r#"{"type":"assistant","sessionId":"sess-broken","timestamp":"2026-11-07T10:00:01.000Z","message":{"id":"m2","model":"qm"#,
                r#"{"type":"assistant","sessionId":"sess-broken","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"m3","model":"qmodel","usage":{"input_tokens":200,"output_tokens":20,"request_id":"req_3"}}}"#,
            ],
        );

        let messages = parse_qoder_transcript_file(&path);
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[0].dedup_key.as_deref(),
            Some("qoder:qoder:req:req_1")
        );
        assert_eq!(
            messages[1].dedup_key.as_deref(),
            Some("qoder:qoder:req:req_3")
        );
        assert_eq!(messages[1].tokens.input, 200);
    }

    #[test]
    fn negative_and_huge_counters_are_safe() {
        let dir = TempDir::new().unwrap();
        let project = dir.path().join(".qoder/projects/-repo");
        let path = write_transcript(
            &project,
            "sess-max.jsonl",
            &[
                r#"{"type":"assistant","sessionId":"sess-max","timestamp":"2026-11-07T10:00:00.000Z","message":{"id":"m1","model":"qmodel","usage":{"input_tokens":-5,"output_tokens":-7,"cache_read_input_tokens":-9}}}"#,
                r#"{"type":"assistant","sessionId":"sess-max","timestamp":"2026-11-07T10:00:01.000Z","message":{"id":"m2","model":"qmodel","usage":{"input_tokens":9223372036854775807,"output_tokens":9223372036854775807,"cache_read_input_tokens":9223372036854775807}}}"#,
                r#"{"type":"assistant","sessionId":"sess-max","timestamp":"99999999999999999999999","message":{"id":"m3","model":"qmodel","usage":{"input_tokens":10,"output_tokens":1}}}"#,
            ],
        );

        let messages = parse_qoder_transcript_file(&path);
        // The all-negative row is not measured usage.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].tokens.input, 0);
        assert_eq!(messages[0].tokens.cache_read, i64::MAX);
        assert!(messages[0].tokens.total() > 0);
        assert_eq!(messages[1].tokens.input, 10);
    }

    #[test]
    fn segment_completed_events_are_counted_and_rollups_ignored() {
        let dir = TempDir::new().unwrap();
        let segments = dir
            .path()
            .join(".qoder/logs/sessions/-work-app/sess-seg/segments");
        std::fs::create_dir_all(&segments).unwrap();
        let path = segments.join("run-1.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"ts":"2026-09-12T17:08:58.000+08:00","type":"session.config.loaded","data":{"project_root":"/work/app"}}"#,
                "\n",
                r#"{"ts":"2026-09-12T17:09:02.500+08:00","type":"model.response.completed","request_id":"r1","data":{"model":"pro","input_tokens":1000,"output_tokens":45,"cache_read_input_tokens":400}}"#,
                "\n",
                r#"{"ts":"2026-09-12T17:09:10.000+08:00","type":"model.response.completed","request_id":"r2","data":{"model":"pro","input_tokens":1000,"output_tokens":30,"cache_read_input_tokens":0}}"#,
                "\n",
                r#"{"ts":"2026-09-12T17:09:11.000+08:00","type":"turn.finished","data":{"model":"pro","input_tokens":2000,"output_tokens":75,"cache_read_input_tokens":400}}"#,
                "\n",
            ),
        )
        .unwrap();

        let messages = parse_qoder_segment_file(&path);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].session_id, "sess-seg");
        assert_eq!(messages[0].workspace_key.as_deref(), Some("/work/app"));
        assert_eq!(messages[0].model_id, "pro");
        assert_eq!(messages[0].provider_id, "qoder");
        assert_eq!(messages[0].tokens.input, 600);
        assert_eq!(messages[0].tokens.cache_read, 400);
        assert_eq!(messages[0].tokens.output, 45);
        assert_eq!(messages[0].dedup_key.as_deref(), Some("qoder:qoder:req:r1"));
        assert_eq!(messages[1].tokens.input, 1000);
        assert_eq!(messages[1].tokens.output, 30);
    }

    #[test]
    fn segment_without_request_id_never_merges_distinct_events() {
        let dir = TempDir::new().unwrap();
        let segments = dir
            .path()
            .join(".qoder/logs/sessions/-work-app/sess-seg/segments");
        std::fs::create_dir_all(&segments).unwrap();
        let path = segments.join("run-2.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"ts":"2026-09-12T17:09:02.000+08:00","type":"model.response.completed","data":{"model":"pro","input_tokens":100,"output_tokens":10}}"#,
                "\n",
                r#"{"ts":"2026-09-12T17:09:03.000+08:00","type":"model.response.completed","data":{"model":"pro","input_tokens":200,"output_tokens":20}}"#,
                "\n",
            ),
        )
        .unwrap();

        let messages = parse_qoder_segment_file(&path);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].tokens.input + messages[1].tokens.input, 300);
    }

    #[test]
    fn collapse_counts_a_request_once_across_transcript_and_segment() {
        let dir = TempDir::new().unwrap();
        let transcript = write_transcript(
            &dir.path().join(".qoder/projects/-work-app"),
            "sess-1.jsonl",
            &[
                r#"{"type":"assistant","sessionId":"sess-1","timestamp":"2026-11-07T10:00:00.000Z","message":{"id":"msg_1","model":"qmodel","usage":{"input_tokens":1200,"output_tokens":40,"request_id":"req_1"}}}"#,
            ],
        );
        let segments = dir
            .path()
            .join(".qoder/logs/sessions/-work-app/sess-1/segments");
        std::fs::create_dir_all(&segments).unwrap();
        let segment = segments.join("run.jsonl");
        std::fs::write(
            &segment,
            concat!(
                r#"{"ts":"2026-11-07T10:00:02.000Z","type":"model.response.completed","request_id":"req_1","data":{"model":"qmodel","input_tokens":1200,"output_tokens":40}}"#,
                "\n",
                r#"{"ts":"2026-11-07T10:05:00.000Z","type":"model.response.completed","request_id":"req_2","data":{"model":"qmodel","input_tokens":300,"output_tokens":10}}"#,
                "\n",
            ),
        )
        .unwrap();

        let mut parsed = parse_qoder_transcript_file(&transcript);
        parsed.extend(parse_qoder_segment_file(&segment));
        let collapsed = collapse_qoder_messages(parsed);

        assert_eq!(collapsed.len(), 2);
        assert_eq!(collapsed[0].timestamp, 1_794_045_600_000);
        assert_eq!(collapsed[0].tokens.input, 1200);
        assert_eq!(collapsed[1].tokens.input, 300);
    }

    #[test]
    fn collapse_is_independent_of_input_order() {
        let build = |session: &str, timestamp: i64, tokens: i64| {
            let mut message = UnifiedMessage::new_with_dedup(
                CLIENT_ID,
                "qmodel",
                QODER_PROVIDER,
                session,
                timestamp,
                TokenBreakdown {
                    input: tokens,
                    output: 1,
                    ..Default::default()
                },
                0.0,
                Some("qoder:qoder:req:shared".to_string()),
            );
            message.is_turn_start = true;
            message
        };
        let first = build("s1", 1_000, 100);
        let second = build("s2", 900, 120);

        let forward = collapse_qoder_messages(vec![first.clone(), second.clone()]);
        let reverse = collapse_qoder_messages(vec![second, first]);
        assert_eq!(forward.len(), 1);
        assert_eq!(reverse.len(), 1);
        assert_eq!(forward[0].session_id, reverse[0].session_id);
        assert_eq!(forward[0].timestamp, reverse[0].timestamp);
        assert_eq!(forward[0].tokens.input, 120);
        assert_eq!(reverse[0].tokens.input, 120);
    }

    #[test]
    fn editions_do_not_deduplicate_each_other() {
        let dir = TempDir::new().unwrap();
        let intl = write_transcript(
            &dir.path().join(".qoder/projects/-app"),
            "sess.jsonl",
            &[
                r#"{"type":"assistant","sessionId":"sess","timestamp":"2026-11-07T10:00:00.000Z","message":{"id":"msg_1","model":"qmodel","usage":{"input_tokens":100,"output_tokens":10,"request_id":"shared"}}}"#,
            ],
        );
        let cn = write_transcript(
            &dir.path().join(".qoder-cn/projects/-app"),
            "sess.jsonl",
            &[
                r#"{"type":"assistant","sessionId":"sess","timestamp":"2026-11-07T11:00:00.000Z","message":{"id":"msg_1","model":"qmodel","usage":{"input_tokens":200,"output_tokens":20,"request_id":"shared"}}}"#,
            ],
        );

        let mut parsed = parse_qoder_transcript_file(&intl);
        parsed.extend(parse_qoder_transcript_file(&cn));
        let collapsed = collapse_qoder_messages(parsed);

        assert_eq!(collapsed.len(), 2);
        let total: i64 = collapsed.iter().map(|message| message.tokens.input).sum();
        assert_eq!(total, 300);
    }

    fn create_ide_db(path: &Path, with_chat_session: bool, rows: &[(i64, &str, &str, &str)]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE chat_message (
                 id TEXT PRIMARY KEY,
                 session_id TEXT,
                 request_id TEXT,
                 role TEXT,
                 token_info TEXT,
                 model_info TEXT,
                 gmt_create INTEGER
             );",
        )
        .unwrap();
        if with_chat_session {
            conn.execute_batch(
                "CREATE TABLE chat_session (
                     session_id TEXT PRIMARY KEY,
                     project_uri TEXT,
                     project_name TEXT,
                     preferred_model_info TEXT
                 );
                 INSERT INTO chat_session VALUES
                     ('sess-ide', 'file:///work/ide-app', 'ide-app', '{\"model_key\":\"qmodel\"}');",
            )
            .unwrap();
        }
        for (created, token_info, model_info, request_id) in rows {
            conn.execute(
                "INSERT INTO chat_message (id, session_id, request_id, role, token_info, model_info, gmt_create)
                 VALUES (?1, 'sess-ide', ?2, 'assistant', ?3, ?4, ?5)",
                rusqlite::params![
                    format!("row-{created}"),
                    request_id,
                    token_info,
                    model_info,
                    created
                ],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO chat_message (id, session_id, request_id, role, token_info, model_info, gmt_create)
             VALUES ('user-row', 'sess-ide', NULL, 'user', '{\"prompt_tokens\":999}', NULL, 1);",
        )
        .unwrap();
    }

    #[test]
    fn ide_token_info_splits_cached_from_prompt() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("Qoder/SharedClientCache/cache/db/local.db");
        create_ide_db(
            &db,
            true,
            &[(
                1_762_509_600_000,
                r#"{"prompt_tokens":39005,"cached_tokens":37485,"completion_tokens":67,"max_input_tokens":200000}"#,
                r#"{"model_key":"auto"}"#,
                "req-ide-1",
            )],
        );

        let scan = scan_qoder_ide_db(&db);
        assert!(scan.complete);
        assert_eq!(scan.messages.len(), 1);
        let message = &scan.messages[0];
        assert_eq!(message.client, "qoder");
        assert_eq!(message.tokens.input, 1520);
        assert_eq!(message.tokens.cache_read, 37485);
        assert_eq!(message.tokens.output, 67);
        assert_eq!(message.tokens.total(), 39072);
        assert_eq!(message.model_id, "auto");
        assert_eq!(message.provider_id, "qoder");
        assert_eq!(message.session_id, "sess-ide");
        assert_eq!(message.workspace_key.as_deref(), Some("/work/ide-app"));
        assert_eq!(
            message.dedup_key.as_deref(),
            Some("qoder:ide:sess-ide:req:req-ide-1")
        );
    }

    #[test]
    fn ide_without_chat_session_still_reports_usage() {
        let dir = TempDir::new().unwrap();
        let db = dir
            .path()
            .join("QoderCN/SharedClientCache/cache/db/local.db");
        create_ide_db(
            &db,
            false,
            &[(
                1_762_509_600_000,
                r#"{"prompt_tokens":100,"cached_tokens":0,"completion_tokens":5}"#,
                r#"{"model_key":"qmodel_latest"}"#,
                "",
            )],
        );

        let scan = scan_qoder_ide_db(&db);
        assert!(scan.complete);
        assert_eq!(scan.messages.len(), 1);
        assert_eq!(scan.messages[0].tokens.input, 100);
        assert_eq!(scan.messages[0].tokens.output, 5);
        assert_eq!(scan.messages[0].workspace_key, None);
        assert_eq!(
            scan.messages[0].dedup_key.as_deref(),
            Some("qoder:ide:row-1762509600000")
        );
    }

    #[test]
    fn ide_preferred_model_fills_in_a_missing_message_model() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("Qoder/SharedClientCache/cache/db/local.db");
        create_ide_db(
            &db,
            true,
            &[(
                1_762_509_600_000,
                r#"{"prompt_tokens":10,"cached_tokens":0,"completion_tokens":1}"#,
                "{}",
                "req-nomodel",
            )],
        );

        let scan = scan_qoder_ide_db(&db);
        assert!(scan.complete);
        assert_eq!(scan.messages.len(), 1);
        assert_eq!(scan.messages[0].model_id, "qmodel");
        assert_eq!(scan.messages[0].provider_id, "qoder");
    }

    #[test]
    fn ide_missing_database_is_an_empty_complete_read() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("Qoder/SharedClientCache/cache/db/local.db");
        let scan = scan_qoder_ide_db(&db);
        assert!(scan.messages.is_empty());
        assert!(!scan.complete);

        // A database with no `chat_message` table is an empty *read*: no usage
        // exists, and the source should be cached as such.
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE other (id INTEGER);")
            .unwrap();
        drop(conn);
        let scan = scan_qoder_ide_db(&db);
        assert!(scan.messages.is_empty());
        assert!(scan.complete);
    }

    #[test]
    fn ide_ignores_rows_without_measured_tokens() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("Qoder/SharedClientCache/cache/db/local.db");
        create_ide_db(
            &db,
            true,
            &[
                (
                    1_762_509_600_000,
                    r#"{"prompt_tokens":0,"cached_tokens":0,"completion_tokens":0}"#,
                    r#"{"model_key":"auto"}"#,
                    "req-zero",
                ),
                (
                    1_762_509_601_000,
                    "",
                    r#"{"model_key":"auto"}"#,
                    "req-empty",
                ),
            ],
        );

        let scan = scan_qoder_ide_db(&db);
        assert!(scan.complete);
        assert!(scan.messages.is_empty());
    }

    #[test]
    fn ide_wal_writes_are_visible_to_a_second_reader() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("Qoder/SharedClientCache/cache/db/local.db");
        create_ide_db(
            &db,
            true,
            &[(
                1_762_509_600_000,
                r#"{"prompt_tokens":100,"cached_tokens":0,"completion_tokens":5}"#,
                r#"{"model_key":"auto"}"#,
                "req-1",
            )],
        );
        assert_eq!(scan_qoder_ide_db(&db).messages.len(), 1);

        // The IDE keeps its connection open, so the newest row lives in the
        // -wal sidecar and the main database's mtime does not move.
        let writer = rusqlite::Connection::open(&db).unwrap();
        writer
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;")
            .unwrap();
        writer
            .execute(
                "INSERT INTO chat_message (id, session_id, request_id, role, token_info, model_info, gmt_create)
                 VALUES ('row-2', 'sess-ide', 'req-2', 'assistant', ?1, '{\"model_key\":\"auto\"}', 1762509601000)",
                rusqlite::params![r#"{"prompt_tokens":200,"cached_tokens":50,"completion_tokens":7}"#],
            )
            .unwrap();

        let wal = PathBuf::from(format!("{}-wal", db.display()));
        assert!(wal.exists(), "the writer must leave a -wal sidecar");

        let scan = scan_qoder_ide_db(&db);
        assert_eq!(scan.messages.len(), 2);
        assert_eq!(scan.messages[1].tokens.input, 150);
        assert_eq!(scan.messages[1].tokens.cache_read, 50);
        drop(writer);
    }

    #[test]
    fn ide_candidates_honour_edition_layouts() {
        let app_data = Path::new("/data");
        let candidates = qoder_ide_db_candidates(app_data, false);
        assert!(candidates
            .contains(&Path::new("/data/Qoder/SharedClientCache/cache/db/local.db").to_path_buf()));
        assert!(candidates.contains(
            &Path::new("/data/QoderCN/SharedClientCache/cache/db/local.db").to_path_buf()
        ));
    }
}
