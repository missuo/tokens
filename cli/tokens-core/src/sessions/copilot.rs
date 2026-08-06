//! GitHub Copilot OTEL parser
//!
//! Parses file-exported OpenTelemetry JSONL emitted by Copilot CLI and VS Code
//! Copilot Chat monitoring. Chat spans and inference log records are preferred;
//! aggregate agent records are only used as a fallback to avoid double counting.

use super::utils::file_modified_timestamp_ms;
use super::UnifiedMessage;
use crate::provider_identity::inferred_provider_from_model;
use crate::TokenBreakdown;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn parse_copilot_file(path: &Path) -> Vec<UnifiedMessage> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let fallback_timestamp = file_modified_timestamp_ms(path);
    let mut records = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => continue,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Ok(record) = serde_json::from_str::<Value>(trimmed) {
            records.push(record);
        }
    }

    let trace_contexts = collect_trace_contexts(&records);
    let candidates: Vec<CopilotUsageCandidate> = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            usage_candidate_from_record(record, index, fallback_timestamp, &trace_contexts)
        })
        .collect();

    let chat_traces = candidate_trace_contexts(&candidates, CopilotUsageSource::ChatSpan);
    let inference_traces = candidate_trace_contexts(&candidates, CopilotUsageSource::InferenceLog);
    let agent_turn_traces = candidate_trace_contexts(&candidates, CopilotUsageSource::AgentTurnLog);
    let chat_response_ids = candidate_response_ids(&candidates, CopilotUsageSource::ChatSpan);
    let inference_response_ids =
        candidate_response_ids(&candidates, CopilotUsageSource::InferenceLog);
    let agent_turn_response_ids =
        candidate_response_ids(&candidates, CopilotUsageSource::AgentTurnLog);

    let emitted_candidates = candidates
        .into_iter()
        .filter(|candidate| {
            should_emit_candidate(
                candidate,
                &chat_traces,
                &inference_traces,
                &agent_turn_traces,
                &chat_response_ids,
                &inference_response_ids,
                &agent_turn_response_ids,
            )
        })
        .collect();

    merge_duplicate_candidates(emitted_candidates)
        .into_iter()
        .map(CopilotUsageCandidate::into_message)
        .collect()
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CopilotUsageSource {
    ChatSpan,
    InferenceLog,
    AgentTurnLog,
    AgentSummarySpan,
}

struct TraceContext {
    model: Option<String>,
    session_id: Option<String>,
    session_id_priority: SessionIdPriority,
    agent_id: Option<String>,
}

struct CopilotUsageCandidate {
    source: CopilotUsageSource,
    trace_id: Option<String>,
    response_id: Option<String>,
    model: String,
    provider_id: String,
    session_id: String,
    timestamp_ms: i64,
    timestamp_provenance: crate::TimestampProvenance,
    duration_ms: Option<i64>,
    start_timestamp_ms: Option<i64>,
    end_timestamp_ms: Option<i64>,
    inclusive_input_tokens: i64,
    tokens: TokenBreakdown,
    dedup_key: String,
    agent: Option<String>,
    agent_is_direct: bool,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum SessionIdPriority {
    Missing,
    Response,
    Interaction,
    Session,
}

impl CopilotUsageCandidate {
    fn into_message(self) -> UnifiedMessage {
        let mut message = UnifiedMessage::new_with_dedup(
            "copilot",
            self.model,
            self.provider_id,
            self.session_id,
            self.timestamp_ms,
            self.tokens,
            0.0,
            Some(self.dedup_key),
        );
        message.duration_ms = self.duration_ms;
        message.set_timestamp_provenance(self.timestamp_provenance);
        message.agent = self.agent;
        message
    }

    fn merge_duplicate(&mut self, duplicate: Self) {
        self.inclusive_input_tokens = self
            .inclusive_input_tokens
            .max(duplicate.inclusive_input_tokens);
        self.tokens = normalize_input_tokens(
            self.inclusive_input_tokens,
            self.tokens.output.max(duplicate.tokens.output),
            self.tokens.cache_read.max(duplicate.tokens.cache_read),
            self.tokens.cache_write.max(duplicate.tokens.cache_write),
            self.tokens.reasoning.max(duplicate.tokens.reasoning),
        );

        let current_timestamp = (self.timestamp_ms, self.timestamp_provenance);
        let duplicate_timestamp = (duplicate.timestamp_ms, duplicate.timestamp_provenance);
        let current_is_exact = current_timestamp.1.is_trustworthy_for_hourly();
        let duplicate_is_exact = duplicate_timestamp.1.is_trustworthy_for_hourly();
        let authoritative_timestamp = match (current_is_exact, duplicate_is_exact) {
            (true, false) => current_timestamp,
            (false, true) => duplicate_timestamp,
            _ if duplicate_timestamp.0 < current_timestamp.0 => duplicate_timestamp,
            _ => current_timestamp,
        };
        let fallback_duration_ms = self.duration_ms.max(duplicate.duration_ms);
        self.start_timestamp_ms = match (self.start_timestamp_ms, duplicate.start_timestamp_ms) {
            (Some(current), Some(candidate)) => Some(current.min(candidate)),
            (current, candidate) => current.or(candidate),
        };
        self.end_timestamp_ms = self.end_timestamp_ms.max(duplicate.end_timestamp_ms);
        self.timestamp_ms = authoritative_timestamp.0;
        self.timestamp_provenance = authoritative_timestamp.1;
        self.duration_ms = self
            .start_timestamp_ms
            .zip(self.end_timestamp_ms)
            .and_then(|(start_timestamp_ms, end_timestamp_ms)| {
                let duration_ms = end_timestamp_ms.saturating_sub(start_timestamp_ms);
                (duration_ms > 0).then_some(duration_ms)
            })
            .max(fallback_duration_ms);

        let duplicate_agent = duplicate.agent.filter(|agent| !agent.is_empty());
        // Direct attribution outranks fallback; equal-authority conflicts use a
        // stable lexical tie-break so duplicate merging is order-independent.
        let replace_agent = match (
            self.agent.as_deref().filter(|agent| !agent.is_empty()),
            duplicate_agent.as_deref(),
        ) {
            (None, Some(_)) => true,
            (Some(_), Some(_)) if self.agent_is_direct != duplicate.agent_is_direct => {
                duplicate.agent_is_direct
            }
            (Some(current), Some(candidate)) => candidate < current,
            _ => false,
        };
        if replace_agent {
            self.agent = duplicate_agent;
            self.agent_is_direct = duplicate.agent_is_direct;
        }
    }
}

fn collect_trace_contexts(records: &[Value]) -> HashMap<String, TraceContext> {
    let mut contexts = HashMap::new();

    for record in records {
        let Some(trace_id) = trace_id_from_record(record) else {
            continue;
        };

        let Some(attributes) = record.get("attributes").and_then(Value::as_object) else {
            continue;
        };

        let context = contexts
            .entry(trace_id.to_string())
            .or_insert(TraceContext {
                model: None,
                session_id: None,
                session_id_priority: SessionIdPriority::Missing,
                agent_id: None,
            });

        if context.model.is_none() {
            context.model = first_non_empty_attr(attributes, MODEL_ATTRS).map(str::to_string);
        }

        if let Some((session_id, priority)) = best_session_attr(attributes) {
            if priority > context.session_id_priority {
                context.session_id = Some(session_id.to_string());
                context.session_id_priority = priority;
            }
        }
    }

    // Trace-level agent is only a FALLBACK for records that carry no
    // gen_ai.agent.id of their own (see candidate_from_attributes). Prefer the
    // ROOT invoke_agent span's agent id — the invoke_agent span whose parent
    // chain contains no other invoke_agent span — so a nested task/sub-agent
    // invoke inside the main invocation does not become the trace default.
    // This is resolved in a dedicated pass because OTel export order is not
    // guaranteed: the root invoke_agent span may export after a nested one (or
    // after the chat spans it should cover), so the whole span hierarchy must
    // be known before the root can be picked. Per-record agent ids still take
    // precedence at attribution time.
    for (trace_id, agent_id) in resolve_trace_fallback_agents(records) {
        if let Some(context) = contexts.get_mut(&trace_id) {
            context.agent_id = Some(agent_id);
        }
    }

    contexts
}

/// Resolve the trace-level fallback agent id for each trace, preferring the
/// ROOT invoke_agent span (the invoke_agent span whose parent chain contains no
/// other invoke_agent span). A trace can hold several invoke_agent spans when a
/// task/sub-agent is invoked inside the main agent invocation; the sub-agent's
/// invoke_agent is nested and must not become the trace default. When a trace
/// has no invoke_agent span, fall back to the first non-empty gen_ai.agent.id
/// seen in the trace (input order).
fn resolve_trace_fallback_agents(records: &[Value]) -> HashMap<String, String> {
    // Span ids are unique only within a trace, so keep the OTel structural
    // identity scoped by both ids.
    let mut parent_of: HashMap<(&str, &str), &str> = HashMap::new();
    // Span ids of every invoke_agent span, used to detect a nested invoke.
    let mut invoke_agent_span_ids: HashSet<(&str, &str)> = HashSet::new();
    // Per trace: invoke_agent spans in input order, each with its agent id.
    let mut trace_invoke_agents: HashMap<&str, Vec<(&str, Option<&str>)>> = HashMap::new();
    // Per trace: first non-empty agent id seen on any record (ultimate fallback
    // for traces whose invoke_agent spans name no agent, or that have none).
    let mut trace_first_agent: HashMap<&str, &str> = HashMap::new();

    for record in records {
        let Some(trace_id) = trace_id_from_record(record) else {
            continue;
        };

        // Parent edges are OTel structure, not attributes. Collect them before
        // the attributes gate so attribute-less intermediary spans still link
        // nested invokes back to the root.
        let span_id = span_id_from_record(record);
        if let Some(span_id) = span_id {
            if let Some(parent_span_id) = parent_span_id_from_record(record) {
                parent_of.insert((trace_id, span_id), parent_span_id);
            }
        }

        let Some(attributes) = record.get("attributes").and_then(Value::as_object) else {
            continue;
        };

        let agent_id = first_non_empty_attr(attributes, &["gen_ai.agent.id"]);

        if is_agent_summary_span_record(record, attributes) {
            if let Some(span_id) = span_id {
                invoke_agent_span_ids.insert((trace_id, span_id));
                trace_invoke_agents
                    .entry(trace_id)
                    .or_default()
                    .push((span_id, agent_id));
            }
        }

        if let Some(agent_id) = agent_id {
            trace_first_agent.entry(trace_id).or_insert(agent_id);
        }
    }

    let mut fallback = HashMap::new();

    for (trace_id, invokes) in &trace_invoke_agents {
        // Prefer the first ROOT invoke_agent span that carries an agent id.
        // Fall back to any invoke_agent span with an agent id when no root does
        // (e.g. only a nested invoke names an agent) so the trace still
        // resolves to an invoke_agent default rather than a bare chat span.
        let resolved = invokes
            .iter()
            .filter(|(span_id, _)| {
                is_root_invoke_agent(trace_id, span_id, &parent_of, &invoke_agent_span_ids)
            })
            .find_map(|(_, agent_id)| *agent_id)
            .or_else(|| invokes.iter().find_map(|(_, agent_id)| *agent_id));
        if let Some(agent_id) = resolved {
            fallback.insert((*trace_id).to_string(), agent_id.to_string());
        }
    }

    // Traces without any invoke_agent span (or whose invoke_agent spans name no
    // agent) keep the first non-empty agent id seen in the trace.
    for (trace_id, agent_id) in trace_first_agent {
        fallback
            .entry(trace_id.to_string())
            .or_insert_with(|| agent_id.to_string());
    }

    fallback
}

/// An invoke_agent span is a ROOT when no span in its parent chain is itself an
/// invoke_agent span. Nested task/sub-agent invokes therefore resolve to false.
fn is_root_invoke_agent(
    trace_id: &str,
    span_id: &str,
    parent_of: &HashMap<(&str, &str), &str>,
    invoke_agent_span_ids: &HashSet<(&str, &str)>,
) -> bool {
    let mut current = parent_of.get(&(trace_id, span_id)).copied();
    let mut visited: HashSet<(&str, &str)> = HashSet::new();
    while let Some(parent) = current {
        if invoke_agent_span_ids.contains(&(trace_id, parent)) {
            return false;
        }
        if !visited.insert((trace_id, parent)) {
            // Guard against malformed/cyclic parent references.
            break;
        }
        current = parent_of.get(&(trace_id, parent)).copied();
    }
    true
}

fn usage_candidate_from_record(
    record: &Value,
    index: usize,
    fallback_timestamp: i64,
    trace_contexts: &HashMap<String, TraceContext>,
) -> Option<CopilotUsageCandidate> {
    let attributes = record.get("attributes").and_then(Value::as_object)?;
    let trace_id = trace_id_from_record(record).map(str::to_string);
    let trace_context = trace_id
        .as_deref()
        .and_then(|trace_id| trace_contexts.get(trace_id));

    if is_chat_span_record(record, attributes) {
        return candidate_from_attributes(
            CopilotUsageSource::ChatSpan,
            record,
            attributes,
            trace_id,
            trace_context,
            index,
            fallback_timestamp,
        );
    }

    if is_inference_log_record(record, attributes) {
        return candidate_from_attributes(
            CopilotUsageSource::InferenceLog,
            record,
            attributes,
            trace_id,
            trace_context,
            index,
            fallback_timestamp,
        );
    }

    if is_agent_turn_log_record(record, attributes) {
        return candidate_from_attributes(
            CopilotUsageSource::AgentTurnLog,
            record,
            attributes,
            trace_id,
            trace_context,
            index,
            fallback_timestamp,
        );
    }

    if is_agent_summary_span_record(record, attributes) {
        return candidate_from_attributes(
            CopilotUsageSource::AgentSummarySpan,
            record,
            attributes,
            trace_id,
            trace_context,
            index,
            fallback_timestamp,
        );
    }

    None
}

fn candidate_from_attributes(
    source: CopilotUsageSource,
    record: &Value,
    attributes: &Map<String, Value>,
    trace_id: Option<String>,
    trace_context: Option<&TraceContext>,
    index: usize,
    fallback_timestamp: i64,
) -> Option<CopilotUsageCandidate> {
    let input = attr_i64_first(attributes, &["gen_ai.usage.input_tokens"]);
    let output = attr_i64_first(attributes, &["gen_ai.usage.output_tokens"]);
    let cache_read = attr_i64_first(
        attributes,
        &[
            "gen_ai.usage.cache_read.input_tokens",
            "gen_ai.usage.cache_read_input_tokens",
        ],
    );
    let cache_write = attr_i64_first(
        attributes,
        &[
            "gen_ai.usage.cache_write.input_tokens",
            "gen_ai.usage.cache_creation.input_tokens",
            "gen_ai.usage.cache_write_input_tokens",
            "gen_ai.usage.cache_creation_input_tokens",
        ],
    );
    let reasoning = attr_i64_first(
        attributes,
        &[
            "gen_ai.usage.reasoning.output_tokens",
            "gen_ai.usage.reasoning_tokens",
        ],
    );

    let tokens = normalize_input_tokens(input, output, cache_read, cache_write, reasoning);
    if tokens.total() == 0 {
        return None;
    }

    let response_id = attributes
        .get("gen_ai.response.id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let model = first_non_empty_attr(attributes, MODEL_ATTRS)
        .or_else(|| trace_context.and_then(|context| context.model.as_deref()))
        .unwrap_or("unknown")
        .to_string();
    let provider_id = inferred_provider_from_model(&model)
        .unwrap_or("github-copilot")
        .to_string();
    let session_id = best_session_attr(attributes)
        .map(|(session_id, _)| session_id)
        .or_else(|| trace_context.and_then(|context| context.session_id.as_deref()))
        .or(trace_id.as_deref())
        .unwrap_or("unknown-session")
        .to_string();
    let record_timestamp_ms = timestamp_ms_from_record(record);
    let timestamp_ms = record_timestamp_ms.unwrap_or(fallback_timestamp);
    let timestamp_provenance = if source == CopilotUsageSource::AgentSummarySpan {
        crate::TimestampProvenance::Aggregate
    } else if record_timestamp_ms.is_some() {
        crate::TimestampProvenance::Exact
    } else {
        crate::TimestampProvenance::Fallback
    };
    let duration_ms = duration_ms_from_record(record);
    // Preserve explicit interval boundaries separately: an end-only exporter
    // update uses endTime as its timestamp but must not treat that end as a start.
    let explicit_start_ms = record.get("startTime").and_then(timestamp_ms_from_value);
    let explicit_end_ms = record.get("endTime").and_then(timestamp_ms_from_value);
    let start_timestamp_ms = explicit_start_ms.or_else(|| {
        record_timestamp_ms.filter(|_| duration_ms.is_some() || explicit_end_ms.is_none())
    });
    let end_timestamp_ms = explicit_end_ms.or_else(|| {
        record_timestamp_ms
            .zip(duration_ms)
            .map(|(start, duration)| start.saturating_add(duration))
    });
    let dedup_key = dedup_key_for_record(
        source,
        record,
        attributes,
        trace_id.as_deref(),
        &session_id,
        timestamp_ms,
        index,
    );
    let direct_agent = first_non_empty_attr(attributes, &["gen_ai.agent.id"]).map(str::to_string);
    let agent_is_direct = direct_agent.is_some();

    Some(CopilotUsageCandidate {
        source,
        trace_id,
        response_id,
        model,
        provider_id,
        session_id,
        timestamp_ms,
        timestamp_provenance,
        duration_ms,
        start_timestamp_ms,
        end_timestamp_ms,
        inclusive_input_tokens: input.max(0),
        tokens,
        dedup_key,
        // Per-record attribution first: when a chat/inference record carries its
        // own gen_ai.agent.id (e.g. a sub-agent turn inside a shared trace), use
        // it so sub-agents are not mis-attributed to the trace's first agent.
        // Fall back to the trace-level agent (typically from the invoke_agent
        // span) only when the record itself has none.
        agent: direct_agent.or_else(|| trace_context.and_then(|tc| tc.agent_id.clone())),
        agent_is_direct,
    })
}

fn candidate_trace_contexts(
    candidates: &[CopilotUsageCandidate],
    source: CopilotUsageSource,
) -> HashSet<String> {
    candidates
        .iter()
        .filter(|candidate| candidate.source == source)
        .filter_map(|candidate| candidate.trace_id.clone())
        .collect()
}

fn candidate_response_ids(
    candidates: &[CopilotUsageCandidate],
    source: CopilotUsageSource,
) -> HashSet<String> {
    candidates
        .iter()
        .filter(|candidate| candidate.source == source)
        .filter_map(|candidate| candidate.response_id.clone())
        .collect()
}

fn should_emit_candidate(
    candidate: &CopilotUsageCandidate,
    chat_traces: &HashSet<String>,
    inference_traces: &HashSet<String>,
    agent_turn_traces: &HashSet<String>,
    chat_response_ids: &HashSet<String>,
    inference_response_ids: &HashSet<String>,
    agent_turn_response_ids: &HashSet<String>,
) -> bool {
    // Cross-source priority filtering keys off two stable per-event identifiers:
    // the OTel `trace_id` and `gen_ai.response.id`. Either match is sufficient
    // to suppress a lower-priority lane, which closes the mixed-trace gap where
    // one record carries a trace_id and another (describing the same response)
    // does not. Coarse session attributes such as gen_ai.conversation.id span
    // multiple turns and are intentionally NOT used here.
    let trace_id = candidate.trace_id.as_deref();
    let response_id = candidate.response_id.as_deref();

    let trace_match = |traces: &HashSet<String>| trace_id.is_some_and(|id| traces.contains(id));
    let response_match =
        |response_ids: &HashSet<String>| response_id.is_some_and(|id| response_ids.contains(id));

    match candidate.source {
        CopilotUsageSource::ChatSpan => true,
        CopilotUsageSource::InferenceLog => {
            !trace_match(chat_traces) && !response_match(chat_response_ids)
        }
        CopilotUsageSource::AgentTurnLog => {
            !trace_match(chat_traces)
                && !trace_match(inference_traces)
                && !response_match(chat_response_ids)
                && !response_match(inference_response_ids)
        }
        CopilotUsageSource::AgentSummarySpan => {
            !trace_match(chat_traces)
                && !trace_match(inference_traces)
                && !trace_match(agent_turn_traces)
                && !response_match(chat_response_ids)
                && !response_match(inference_response_ids)
                && !response_match(agent_turn_response_ids)
        }
    }
}

fn merge_duplicate_candidates(
    candidates: Vec<CopilotUsageCandidate>,
) -> Vec<CopilotUsageCandidate> {
    let mut merged: Vec<CopilotUsageCandidate> = Vec::with_capacity(candidates.len());
    let mut indexes: HashMap<String, usize> = HashMap::with_capacity(candidates.len());

    for candidate in candidates {
        if let Some(index) = indexes.get(&candidate.dedup_key).copied() {
            merged[index].merge_duplicate(candidate);
        } else {
            indexes.insert(candidate.dedup_key.clone(), merged.len());
            merged.push(candidate);
        }
    }

    merged
}

const MODEL_ATTRS: &[&str] = &["gen_ai.response.model", "gen_ai.request.model"];
const SESSION_ATTRS: &[(&str, SessionIdPriority)] = &[
    ("gen_ai.conversation.id", SessionIdPriority::Session),
    ("copilot_chat.session_id", SessionIdPriority::Session),
    ("copilot_chat.chat_session_id", SessionIdPriority::Session),
    ("session.id", SessionIdPriority::Session),
    (
        "github.copilot.interaction_id",
        SessionIdPriority::Interaction,
    ),
    ("gen_ai.response.id", SessionIdPriority::Response),
];

fn is_chat_span_record(value: &Value, attributes: &Map<String, Value>) -> bool {
    if !is_span_record(value) {
        return false;
    }

    if attr_str(attributes, "gen_ai.operation.name") == Some("chat") {
        return true;
    }

    value
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.starts_with("chat "))
}

fn is_agent_summary_span_record(value: &Value, attributes: &Map<String, Value>) -> bool {
    if !is_span_record(value) {
        return false;
    }

    if attr_str(attributes, "gen_ai.operation.name") == Some("invoke_agent") {
        return true;
    }

    value
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.starts_with("invoke_agent "))
}

fn is_inference_log_record(value: &Value, attributes: &Map<String, Value>) -> bool {
    if is_span_record(value) {
        return false;
    }

    attr_str(attributes, "event.name") == Some("gen_ai.client.inference.operation.details")
        || record_body(value).is_some_and(|body| body.starts_with("GenAI inference:"))
}

fn is_agent_turn_log_record(value: &Value, attributes: &Map<String, Value>) -> bool {
    if is_span_record(value) {
        return false;
    }

    attr_str(attributes, "event.name") == Some("copilot_chat.agent.turn")
        || record_body(value).is_some_and(|body| body.starts_with("copilot_chat.agent.turn"))
}

fn is_span_record(value: &Value) -> bool {
    // VS Code Copilot Chat exports omit `type: "span"`, so when `type` is absent
    // we infer span-ness from a top-level `name` plus span identity (spanId or
    // traceId), span timing, or `kind`. This is intentionally permissive for
    // VS Code support. Inference-log and agent-turn-log records do NOT carry a
    // top-level `name` field — that is the property that disambiguates them
    // here. If a future record shape adds a top-level `name`, revisit this.
    match value.get("type").and_then(Value::as_str) {
        Some("span") => return true,
        Some(_) => return false,
        None => {}
    }

    let has_name = value.get("name").and_then(Value::as_str).is_some();
    let has_span_identity = value.get("spanId").and_then(Value::as_str).is_some()
        || value.get("traceId").and_then(Value::as_str).is_some();
    let has_span_timing = value.get("startTime").is_some()
        || value.get("endTime").is_some()
        || value.get("duration").is_some();

    has_name && (has_span_identity || has_span_timing || value.get("kind").is_some())
}

// A W3C Trace Context id (trace or span) is INVALID when it is all-zero hex
// (32 zero chars for a trace id, 16 for a span id) — the sentinel a
// non-recording span context carries. Empty behaves the same way. Records
// without a recording span context carry these sentinel ids, so treat both
// as absent rather than as a real (and, worse, shared-with-other-records)
// identity.
fn is_valid_span_identity_id(id: &str) -> bool {
    !id.is_empty() && !id.chars().all(|c| c == '0')
}

fn trace_id_from_record(value: &Value) -> Option<&str> {
    // Filter each candidate individually: a zero/empty top-level sentinel must
    // fall through to a valid nested `spanContext` id instead of masking it.
    value
        .get("traceId")
        .and_then(Value::as_str)
        .filter(|trace_id| is_valid_span_identity_id(trace_id))
        .or_else(|| {
            value
                .get("spanContext")
                .and_then(Value::as_object)
                .and_then(|context| context.get("traceId"))
                .and_then(Value::as_str)
                .filter(|trace_id| is_valid_span_identity_id(trace_id))
        })
}

fn span_id_from_record(value: &Value) -> Option<&str> {
    value
        .get("spanId")
        .and_then(Value::as_str)
        .filter(|span_id| is_valid_span_identity_id(span_id))
        .or_else(|| {
            value
                .get("spanContext")
                .and_then(Value::as_object)
                .and_then(|context| context.get("spanId"))
                .and_then(Value::as_str)
                .filter(|span_id| is_valid_span_identity_id(span_id))
        })
}

fn parent_span_id_from_record(value: &Value) -> Option<&str> {
    // OTel exporters may emit an empty, absent, or all-zero parent for a root
    // span; treat those as "no parent" so they never match a real span id —
    // filtering each candidate so a top-level sentinel can't mask a valid
    // nested `spanContext` value.
    value
        .get("parentSpanId")
        .and_then(Value::as_str)
        .filter(|parent_span_id| is_valid_span_identity_id(parent_span_id))
        .or_else(|| {
            value
                .get("spanContext")
                .and_then(Value::as_object)
                .and_then(|context| context.get("parentSpanId"))
                .and_then(Value::as_str)
                .filter(|parent_span_id| is_valid_span_identity_id(parent_span_id))
        })
}

fn dedup_key_for_record(
    source: CopilotUsageSource,
    record: &Value,
    attributes: &Map<String, Value>,
    trace_id: Option<&str>,
    session_id: &str,
    timestamp_ms: i64,
    index: usize,
) -> String {
    let span_id = span_id_from_record(record);

    match source {
        CopilotUsageSource::ChatSpan | CopilotUsageSource::AgentSummarySpan => {
            match (trace_id, span_id) {
                (Some(trace_id), Some(span_id)) => format!("{trace_id}:{span_id}"),
                // No trace id, but a valid span id is still a stable identity
                // (unlike the line-index fallback below): key on it directly
                // so duplicate span-id-only snapshots collapse to one entry.
                (None, Some(span_id)) => format!("span:{session_id}:{span_id}"),
                _ => format!("span:{session_id}:{timestamp_ms}:{index}"),
            }
        }
        CopilotUsageSource::InferenceLog => match (trace_id, span_id) {
            (Some(trace_id), Some(span_id)) => format!("log:{trace_id}:{span_id}"),
            _ => format!("log:{session_id}:{timestamp_ms}:{index}"),
        },
        CopilotUsageSource::AgentTurnLog => {
            // When the record actually carries a turn.index, use it so the key
            // is stable across re-runs. Otherwise fall back to the line index
            // so two turn-less agent-turn records in the same trace do not
            // collide on a `0` sentinel.
            let turn_part = ["turn.index", "copilot_chat.turn.index"]
                .iter()
                .find_map(|key| attributes.get(*key).and_then(value_as_i64))
                .map(|value| value.to_string())
                .unwrap_or_else(|| format!("idx-{index}"));
            if let Some(trace_id) = trace_id {
                format!("agent-turn:{trace_id}:{turn_part}")
            } else {
                format!("agent-turn:{session_id}:{turn_part}:{index}")
            }
        }
    }
}

fn attr_str<'a>(attributes: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    attributes.get(key).and_then(Value::as_str)
}

fn attr_i64(attributes: &Map<String, Value>, key: &str) -> i64 {
    attributes
        .get(key)
        .and_then(value_as_i64)
        .unwrap_or(0)
        .max(0)
}

fn attr_i64_first(attributes: &Map<String, Value>, keys: &[&str]) -> i64 {
    keys.iter()
        .map(|key| attr_i64(attributes, key))
        .find(|value| *value > 0)
        .unwrap_or(0)
}

pub(crate) fn normalize_input_tokens(
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
) -> TokenBreakdown {
    // OTEL reports input_tokens inclusive of cache reads. Normalize only the
    // cached-read portion out of input, but preserve the reported cache buckets
    // intact because pricing totals account for them separately.
    let cache_read_for_input = cache_read.max(0).min(input.max(0));

    TokenBreakdown {
        input: input.saturating_sub(cache_read_for_input).max(0),
        output: output.max(0),
        cache_read: cache_read.max(0),
        cache_write: cache_write.max(0),
        reasoning: reasoning.max(0),
    }
}

fn first_non_empty_attr<'a>(attributes: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .filter_map(|key| attributes.get(*key).and_then(Value::as_str))
        // Return the trimmed slice: callers store this value directly (model,
        // agent id), and a surrounding-whitespace variant like
        // " github.copilot.default " must match the same normalization branch
        // as the trimmed form — otherwise it bypasses agent-name normalization.
        .map(str::trim)
        .find(|value| !value.is_empty())
}

fn best_session_attr(attributes: &Map<String, Value>) -> Option<(&str, SessionIdPriority)> {
    SESSION_ATTRS
        .iter()
        .filter_map(|(key, priority)| {
            let value = attributes.get(*key).and_then(Value::as_str)?;
            if value.trim().is_empty() {
                return None;
            }

            Some((value, *priority))
        })
        .max_by_key(|(_, priority)| *priority)
}

fn record_body(value: &Value) -> Option<&str> {
    value
        .get("body")
        .and_then(Value::as_str)
        .or_else(|| value.get("_body").and_then(Value::as_str))
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_f64().map(|value| value as i64))
        .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
}

fn timestamp_ms_from_record(value: &Value) -> Option<i64> {
    value
        .get("startTime")
        .and_then(timestamp_ms_from_value)
        .or_else(|| {
            // When only endTime is available, back-calculate the start if duration is known.
            let end_ms = value.get("endTime").and_then(timestamp_ms_from_value)?;
            let duration = duration_ms_from_record(value).unwrap_or(0);
            Some(end_ms.saturating_sub(duration))
        })
        .or_else(|| value.get("hrTime").and_then(timestamp_ms_from_value))
        .or_else(|| value.get("_hrTime").and_then(timestamp_ms_from_value))
        .or_else(|| value.get("time").and_then(timestamp_ms_from_value))
        .or_else(|| value.get("timestamp").and_then(timestamp_ms_from_scalar))
        .or_else(|| {
            value
                .get("observedTimestamp")
                .and_then(timestamp_ms_from_scalar)
        })
        .or_else(|| {
            value
                .get("timeUnixNano")
                .and_then(timestamp_ms_from_unix_nanos)
        })
}

fn duration_ms_from_record(value: &Value) -> Option<i64> {
    if let (Some(start_ms), Some(end_ms)) = (
        value.get("startTime").and_then(timestamp_ms_from_value),
        value.get("endTime").and_then(timestamp_ms_from_value),
    ) {
        let duration = end_ms.saturating_sub(start_ms);
        if duration > 0 {
            return Some(duration);
        }
    }

    value.get("duration").and_then(duration_ms_from_value)
}

fn duration_ms_from_value(value: &Value) -> Option<i64> {
    if let Some(parts) = value.as_array() {
        let seconds = parts.first().and_then(value_as_i64)?;
        let nanos = parts.get(1).and_then(value_as_i64).unwrap_or(0);
        let duration = seconds
            .saturating_mul(1000)
            .saturating_add(nanos / 1_000_000);
        return (duration > 0).then_some(duration);
    }

    let duration = value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse::<f64>().ok()))?;
    if !duration.is_finite() || duration <= 0.0 {
        return None;
    }

    let duration_ms = if duration >= 1_000_000.0 {
        (duration / 1_000_000.0) as i64
    } else {
        duration as i64
    };
    (duration_ms > 0).then_some(duration_ms)
}

fn timestamp_ms_from_value(value: &Value) -> Option<i64> {
    let parts = value.as_array()?;
    let seconds = parts.first().and_then(value_as_i64)?;
    let nanos = parts.get(1).and_then(value_as_i64)?;
    Some(seconds.saturating_mul(1000) + nanos / 1_000_000)
}

fn timestamp_ms_from_scalar(value: &Value) -> Option<i64> {
    let raw = value_as_i64(value)?;
    Some(match raw.abs() {
        100_000_000_000_000_000.. => raw / 1_000_000,
        100_000_000_000_000.. => raw / 1_000,
        100_000_000_000.. => raw,
        _ => raw.saturating_mul(1000),
    })
}

fn timestamp_ms_from_unix_nanos(value: &Value) -> Option<i64> {
    // OTel `timeUnixNano` is unsigned-by-spec; a negative or zero value is
    // malformed. Refuse it and let the caller fall through to the next
    // timestamp source (or the file modified time) instead of producing a
    // pre-1970 timestamp downstream.
    value_as_i64(value)
        .filter(|raw| *raw > 0)
        .map(|raw| raw / 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        timestamp_ms: i64,
        timestamp_provenance: crate::TimestampProvenance,
        tokens: i64,
    ) -> CopilotUsageCandidate {
        CopilotUsageCandidate {
            source: CopilotUsageSource::ChatSpan,
            trace_id: Some("trace".to_string()),
            response_id: None,
            model: "gpt-4o".to_string(),
            provider_id: "github-copilot".to_string(),
            session_id: "session".to_string(),
            timestamp_ms,
            timestamp_provenance,
            duration_ms: None,
            start_timestamp_ms: timestamp_provenance
                .is_trustworthy_for_hourly()
                .then_some(timestamp_ms),
            end_timestamp_ms: None,
            inclusive_input_tokens: tokens,
            tokens: normalize_input_tokens(tokens, 0, 0, 0, 0),
            dedup_key: "trace:span".to_string(),
            agent: None,
            agent_is_direct: false,
        }
    }

    #[test]
    fn duplicate_merge_keeps_exact_timestamp_with_winning_usage() {
        let exact_timestamp = 1_704_067_200_000;
        let mut merged = candidate(1_704_153_600_000, crate::TimestampProvenance::Fallback, 5);
        merged.merge_duplicate(candidate(
            exact_timestamp,
            crate::TimestampProvenance::Exact,
            10,
        ));

        let mut message = merged.into_message();
        message.date = "2024-01-01".to_string();
        assert_eq!(message.timestamp, exact_timestamp);
        assert!(message.is_trustworthy_for_hourly());
        assert_eq!(message.tokens.total(), 10);

        let facts = crate::aggregator::aggregate_hourly_usage_facts(
            &[message],
            crate::bucket_tz::BucketTimezone::Named(chrono_tz::UTC),
        );
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].hours.len(), 1);
        assert_eq!(facts[0].unplaced_for_hourly.tokens, 0);
    }

    #[test]
    fn duplicate_merge_recovers_exact_timestamp_when_fallback_has_more_usage() {
        let exact_timestamp = 1_704_067_200_000;
        let mut merged = candidate(exact_timestamp, crate::TimestampProvenance::Exact, 5);
        merged.merge_duplicate(candidate(
            1_704_153_600_000,
            crate::TimestampProvenance::Fallback,
            10,
        ));

        let message = merged.into_message();
        assert_eq!(message.timestamp, exact_timestamp);
        assert!(message.is_trustworthy_for_hourly());
        assert_eq!(message.tokens.total(), 10);
    }
}
