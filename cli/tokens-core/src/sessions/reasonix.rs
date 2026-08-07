//! Parser for Reasonix's authoritative append-only statistics records.
//!
//! Reasonix writes one JSON object per provider request to
//! `<REASONIX_HOME>/stats/YYYY-MM-DD.jsonl`. Each record carries per-call
//! token totals (`prompt`, `completion`, `reasoning`, `cache_hit`,
//! `cache_miss`) and an authoritative `requests` count that we surface as
//! `message_count`. Session transcript JSONL is intentionally not scanned:
//! it has no authoritative usage counters and would overlap these records.

use super::utils::parse_timestamp_value;
use super::UnifiedMessage;
use crate::provider_identity::{canonical_provider, inferred_provider_from_model};
use crate::TokenBreakdown;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

const CLIENT_ID: &str = "reasonix";
/// Sentinel model/provider used when a record's model ref is empty or its
/// upstream provider cannot be inferred. Keeps the row attributable so spend
/// still lands on a stable bucket rather than being dropped.
const UNKNOWN_MODEL: &str = "reasonix-unknown";

#[derive(Debug, Deserialize)]
struct ReasonixStat {
    ts: serde_json::Value,
    #[serde(default)]
    model: String,
    #[serde(default)]
    prompt: i64,
    #[serde(default)]
    completion: i64,
    #[serde(default)]
    reasoning: i64,
    #[serde(default)]
    cache_hit: i64,
    // Newer records carry an explicit ordinary-input bucket; older ones omit
    // it, in which case we derive input from `prompt` minus cache hits.
    cache_miss: Option<i64>,
    #[serde(default)]
    total: i64,
    #[serde(default)]
    requests: i64,
    // `turn` markers delimit user turns and carry no usage; skip them.
    #[serde(default)]
    turn: bool,
}

/// Split a `provider/model` ref into its components. A bare model name is
/// paired with its inferred upstream provider, falling back to the Reasonix
/// client id only when neither the canonical-provider table nor the model
/// inference heuristics recognise it.
fn split_model_ref(model_ref: &str) -> (String, String) {
    let model_ref = model_ref.trim();
    if let Some((provider, model)) = model_ref.split_once('/') {
        let provider = canonical_provider(provider).unwrap_or_else(|| provider.to_string());
        return (provider, model.trim().to_string());
    }
    let provider = inferred_provider_from_model(model_ref)
        .unwrap_or(CLIENT_ID)
        .to_string();
    (provider, model_ref.to_string())
}

fn non_negative(value: i64) -> i64 {
    value.max(0)
}

/// Parse a Reasonix stats JSONL file into unified messages.
///
/// Token buckets stay additive: the `prompt` field already includes cached
/// tokens and `completion` already includes reasoning, so the parser stores
/// the non-overlapping components. An explicit nonzero `cache_miss` is the
/// authoritative ordinary-input bucket; when it's absent the parser derives
/// that bucket from `prompt` minus `cache_hit` (clamped at zero).
pub fn parse_reasonix_file(path: &Path) -> Vec<UnifiedMessage> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };

    let source_id = format!("reasonix-stats:{}", path.display());

    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let line = line.ok()?;
            let record: ReasonixStat = serde_json::from_str(line.trim()).ok()?;
            // Drop non-usage rows: turn markers, model-only records, and rows
            // with neither tokens nor an authoritative request count. A row
            // with `total: 0, requests: N` is kept so its request count is
            // still reported.
            if record.turn
                || record.model.trim().is_empty()
                || (record.total <= 0 && record.requests <= 0)
            {
                return None;
            }

            let timestamp = parse_timestamp_value(&record.ts)?;
            let (provider_id, model_id) = split_model_ref(&record.model);

            let cache_read = non_negative(record.cache_hit);
            let raw_prompt = non_negative(record.prompt);
            // An explicit nonzero cache miss is Reasonix's authoritative
            // ordinary-input bucket. Older records omit it, so derive that
            // bucket from prompt tokens minus cache hits in that case.
            let input = match record.cache_miss {
                Some(cache_miss) if cache_miss != 0 => non_negative(cache_miss),
                _ => raw_prompt.saturating_sub(cache_read),
            };
            // `completion` already includes reasoning, so keep the buckets
            // additive: clamp reasoning into `[0, completion]` and report the
            // remainder as output.
            let reasoning = non_negative(record.reasoning).min(non_negative(record.completion));
            let output = non_negative(record.completion).saturating_sub(reasoning);
            let tokens = TokenBreakdown {
                input,
                output,
                cache_read,
                cache_write: 0,
                reasoning,
            };

            // Dedup by (path, line index, requests, total). The line index
            // keeps distinct rows apart when a fixture repeats the same
            // `(requests, total)` pair, while still collapsing byte-identical
            // re-emissions of the same record.
            let dedup_key = format!(
                "reasonix:{source_id}:{line_index}:{requests}:{total}",
                requests = record.requests,
                total = record.total,
            );

            let mut message = UnifiedMessage::new_with_dedup(
                CLIENT_ID,
                if model_id.is_empty() {
                    UNKNOWN_MODEL.to_string()
                } else {
                    model_id
                },
                provider_id,
                source_id.clone(),
                timestamp,
                tokens,
                0.0,
                Some(dedup_key),
            );
            // `requests` is the authoritative per-call count. Clamp into the
            // `i32` message-count range so a zero maps to 1 (the row exists,
            // so the call happened at least once) and an overflowing value
            // saturates rather than wrapping.
            message.message_count = record.requests.clamp(1, i64::from(i32::MAX)) as i32;
            Some(message)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn write_stats(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        use std::io::Write;
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn parses_authoritative_stats_with_provider_usage_and_timestamp() {
        let file = write_stats(
            r#"{"ts":"2026-08-04T09:10:11Z","model":"deepseek/chat","prompt":100,"completion":20,"cache_hit":10,"total":130,"requests":3}"#,
        );
        let messages = parse_reasonix_file(file.path());

        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.client, CLIENT_ID);
        assert_eq!(message.provider_id, "deepseek");
        assert_eq!(message.model_id, "chat");
        assert_eq!(message.timestamp, 1_785_834_611_000);
        // `prompt` already includes cache_hit, so ordinary input is 100 - 10.
        assert_eq!(message.tokens.input, 90);
        assert_eq!(message.tokens.cache_read, 10);
        assert_eq!(message.tokens.output, 20);
        assert_eq!(message.tokens.reasoning, 0);
        assert_eq!(message.tokens.total(), 120);
        assert_eq!(message.message_count, 3);
    }

    #[test]
    fn skips_turn_markers_malformed_and_zero_usage_records() {
        let file = write_stats(
            r#"{"ts":"2026-08-04T09:00:00Z","turn":true}
not json
{"ts":"2026-08-04T09:01:00Z","model":"","total":10}
{"ts":"2026-08-04T09:02:00Z","model":"x","total":0,"requests":0}
{"ts":"2026-08-04T09:03:00Z","model":"x","prompt":3,"completion":2,"total":5,"requests":0}"#,
        );
        let messages = parse_reasonix_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.total(), 5);
        // `requests: 0` clamps to 1 because the row itself represents a call.
        assert_eq!(messages[0].message_count, 1);
    }

    #[test]
    fn preserves_unknown_model_provider_as_reasonix_only_when_not_inferable() {
        let file = write_stats(
            r#"{"ts":"2026-08-04T09:00:00Z","model":"zzz-unknown-vendor-model","total":7,"requests":1}"#,
        );
        let messages = parse_reasonix_file(file.path());

        assert_eq!(messages.len(), 1);
        // No `provider/model` slash and not a recognisable model: provider
        // falls back to the client id.
        assert_eq!(messages[0].provider_id, CLIENT_ID);
    }

    #[test]
    fn split_model_ref_handles_provider_slash_and_inference() {
        // Explicit provider passes through canonicalisation.
        let (provider, model) = split_model_ref("deepseek/chat");
        assert_eq!(provider, "deepseek");
        assert_eq!(model, "chat");

        // Multi-segment model names keep everything after the first slash.
        let (provider, model) = split_model_ref("openrouter/google/gemini-2.5-pro");
        assert_eq!(provider, "openrouter");
        assert_eq!(model, "google/gemini-2.5-pro");

        // Bare recognisable model resolves to the inferred provider.
        let (provider, _) = split_model_ref("claude-sonnet-4");
        assert_eq!(provider, "anthropic");
    }

    #[test]
    fn preserves_explicit_cache_miss_when_it_disagrees_with_prompt_input() {
        let file = write_stats(
            r#"{"ts":"2026-08-04T09:00:00Z","model":"x/y","prompt":100,"completion":1,"cache_hit":10,"cache_miss":50,"total":61,"requests":1}"#,
        );
        let messages = parse_reasonix_file(file.path());

        assert_eq!(messages.len(), 1);
        // Explicit nonzero cache_miss wins over the `prompt - cache_hit` derivation.
        assert_eq!(messages[0].tokens.input, 50);
        assert_eq!(messages[0].tokens.cache_read, 10);
    }

    #[test]
    fn falls_back_to_prompt_minus_cache_hit_when_cache_miss_is_absent() {
        let file = write_stats(
            r#"{"ts":"2026-08-04T09:00:00Z","model":"x/y","prompt":40,"completion":1,"cache_hit":15,"total":56,"requests":1}"#,
        );
        let messages = parse_reasonix_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.input, 25);
        assert_eq!(messages[0].tokens.cache_read, 15);
    }

    #[test]
    fn reasoning_is_clamped_into_completion_so_buckets_stay_additive() {
        let file = write_stats(
            r#"{"ts":"2026-08-04T09:00:00Z","model":"x/y","prompt":10,"completion":8,"reasoning":20,"total":26,"requests":1}"#,
        );
        let messages = parse_reasonix_file(file.path());

        assert_eq!(messages.len(), 1);
        // reasoning clamps to completion (8), output becomes 0; nothing is
        // double-counted.
        assert_eq!(messages[0].tokens.reasoning, 8);
        assert_eq!(messages[0].tokens.output, 0);
        assert_eq!(messages[0].tokens.total(), 18);
    }

    #[test]
    fn maps_authoritative_request_count_to_bounded_message_count() {
        let file = write_stats(
            r#"{"ts":"2026-08-04T09:00:00Z","model":"x/y","total":1,"requests":9999999999}"#,
        );
        let messages = parse_reasonix_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message_count, i32::MAX);
    }

    #[test]
    fn preserves_tokenless_request_counts_but_skips_plain_zero_rows() {
        let file = write_stats(
            r#"{"ts":"2026-08-04T09:00:00Z","model":"x/y","total":0,"requests":2}
{"ts":"2026-08-04T09:01:00Z","model":"x/y","total":0,"requests":0}"#,
        );
        let messages = parse_reasonix_file(file.path());

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.total(), 0);
        assert_eq!(messages[0].message_count, 2);
    }
}
