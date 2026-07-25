//! Amp (Sourcegraph) session parser
//!
//! Parses JSON files from ~/.local/share/amp/threads/

use super::utils::read_file_or_none;
use super::UnifiedMessage;
use crate::{provider_identity, TokenBreakdown};
use serde::Deserialize;
use std::path::Path;

/// Amp usage event from usageLedger
#[derive(Debug, Deserialize)]
pub struct AmpUsageEvent {
    pub timestamp: Option<String>,
    pub model: Option<String>,
    pub credits: Option<f64>,
    pub tokens: Option<AmpTokens>,
    #[serde(rename = "operationType")]
    pub _operation_type: Option<String>,
    #[serde(rename = "fromMessageId")]
    pub _from_message_id: Option<i64>,
    #[serde(rename = "toMessageId")]
    pub to_message_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AmpTokens {
    pub input: Option<i64>,
    pub output: Option<i64>,
    #[serde(rename = "cacheReadInputTokens")]
    pub cache_read_input_tokens: Option<i64>,
    #[serde(rename = "cacheCreationInputTokens")]
    pub cache_creation_input_tokens: Option<i64>,
}

/// Amp message usage (per-message, more detailed)
#[derive(Debug, Deserialize)]
pub struct AmpMessageUsage {
    pub model: Option<String>,
    #[serde(rename = "inputTokens")]
    pub input_tokens: Option<i64>,
    #[serde(rename = "outputTokens")]
    pub output_tokens: Option<i64>,
    #[serde(rename = "cacheReadInputTokens")]
    pub cache_read_input_tokens: Option<i64>,
    #[serde(rename = "cacheCreationInputTokens")]
    pub cache_creation_input_tokens: Option<i64>,
    pub credits: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct AmpMessage {
    pub role: Option<String>,
    #[serde(rename = "messageId")]
    pub message_id: Option<i64>,
    pub usage: Option<AmpMessageUsage>,
}

#[derive(Debug, Deserialize)]
pub struct AmpUsageLedger {
    pub events: Option<Vec<AmpUsageEvent>>,
}

#[derive(Debug, Deserialize)]
pub struct AmpThread {
    pub id: Option<String>,
    pub created: Option<i64>,
    pub messages: Option<Vec<AmpMessage>>,
    #[serde(rename = "usageLedger")]
    pub usage_ledger: Option<AmpUsageLedger>,
}

/// Get provider from model name
fn get_provider_from_model(model: &str) -> &'static str {
    provider_identity::inferred_provider_from_model(model).unwrap_or("anthropic")
}

#[derive(Debug, Clone)]
struct AmpUsageRecord {
    model: String,
    timestamp: i64,
    has_explicit_timestamp: bool,
    message_id: Option<i64>,
    ledger_to_message_id: Option<i64>,
    tokens: TokenBreakdown,
    cost: f64,
}

impl AmpUsageRecord {
    fn matches_message_usage(&self, other: &Self) -> bool {
        self.model == other.model && self.tokens == other.tokens
    }

    fn into_unified(self, thread_id: &str) -> UnifiedMessage {
        UnifiedMessage::new(
            "amp",
            &self.model,
            get_provider_from_model(&self.model),
            thread_id.to_string(),
            self.timestamp,
            self.tokens,
            self.cost,
        )
    }
}

fn parse_amp_timestamp(timestamp: Option<String>) -> Option<i64> {
    timestamp
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
        .map(|dt| dt.timestamp_millis())
        .filter(|timestamp| *timestamp != 0)
}

fn fallback_amp_timestamp(
    explicit: Option<i64>,
    thread_created_ms: i64,
    file_mtime_ms: i64,
) -> i64 {
    explicit
        .filter(|timestamp| *timestamp != 0)
        .or_else(|| (thread_created_ms != 0).then_some(thread_created_ms))
        .unwrap_or(file_mtime_ms)
}

fn parse_amp_ledger_records(
    usage_ledger: Option<AmpUsageLedger>,
    thread_created_ms: i64,
    file_mtime_ms: i64,
) -> Vec<AmpUsageRecord> {
    let Some(ledger) = usage_ledger else {
        return Vec::new();
    };
    let Some(events) = ledger.events else {
        return Vec::new();
    };

    events
        .into_iter()
        .filter_map(|event| {
            let model = event.model?;
            let explicit_timestamp = parse_amp_timestamp(event.timestamp);
            let timestamp =
                fallback_amp_timestamp(explicit_timestamp, thread_created_ms, file_mtime_ms);
            let tokens = event.tokens.unwrap_or(AmpTokens {
                input: Some(0),
                output: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation_input_tokens: Some(0),
            });

            Some(AmpUsageRecord {
                model,
                timestamp,
                has_explicit_timestamp: explicit_timestamp.is_some(),
                message_id: None,
                ledger_to_message_id: event.to_message_id.filter(|id| *id > 0),
                tokens: TokenBreakdown {
                    input: tokens.input.unwrap_or(0).max(0),
                    output: tokens.output.unwrap_or(0).max(0),
                    cache_read: tokens.cache_read_input_tokens.unwrap_or(0).max(0),
                    cache_write: tokens.cache_creation_input_tokens.unwrap_or(0).max(0),
                    reasoning: 0,
                },
                cost: event.credits.unwrap_or(0.0).max(0.0),
            })
        })
        .collect()
}

fn parse_amp_message_records(
    thread_messages: Option<Vec<AmpMessage>>,
    thread_created_ms: i64,
    file_mtime_ms: i64,
) -> Vec<AmpUsageRecord> {
    let Some(thread_messages) = thread_messages else {
        return Vec::new();
    };

    let base_timestamp = if thread_created_ms != 0 {
        thread_created_ms
    } else {
        file_mtime_ms
    };

    thread_messages
        .into_iter()
        .filter_map(|msg| {
            if msg.role.as_deref() != Some("assistant") {
                return None;
            }

            let usage = msg.usage?;
            let model = usage.model?;
            let message_id = msg.message_id.unwrap_or(0).max(0);
            let timestamp = base_timestamp.saturating_add(message_id.saturating_mul(1000));

            Some(AmpUsageRecord {
                model,
                timestamp,
                has_explicit_timestamp: false,
                message_id: Some(message_id).filter(|id| *id > 0),
                ledger_to_message_id: None,
                tokens: TokenBreakdown {
                    input: usage.input_tokens.unwrap_or(0).max(0),
                    output: usage.output_tokens.unwrap_or(0).max(0),
                    cache_read: usage.cache_read_input_tokens.unwrap_or(0).max(0),
                    cache_write: usage.cache_creation_input_tokens.unwrap_or(0).max(0),
                    reasoning: 0,
                },
                cost: usage.credits.unwrap_or(0.0).max(0.0),
            })
        })
        .collect()
}

fn find_matching_ledger_record(
    ledger_records: &[AmpUsageRecord],
    consumed: &[bool],
    search_start: usize,
    message_record: &AmpUsageRecord,
) -> Option<usize> {
    let find_match = |predicate: &dyn Fn(usize) -> bool| {
        (search_start..ledger_records.len())
            .find(|&index| predicate(index))
            .or_else(|| (0..search_start).find(|&index| predicate(index)))
    };

    if let Some(message_id) = message_record.message_id {
        if let Some(index) = find_match(&|index| {
            !consumed[index] && ledger_records[index].ledger_to_message_id == Some(message_id)
        }) {
            return Some(index);
        }
    }

    find_match(&|index| {
        !consumed[index] && ledger_records[index].matches_message_usage(message_record)
    })
}

fn merge_amp_records(
    ledger_record: AmpUsageRecord,
    message_record: &AmpUsageRecord,
) -> AmpUsageRecord {
    if ledger_record.has_explicit_timestamp {
        if ledger_record.cost > 0.0 || message_record.cost <= 0.0 {
            ledger_record
        } else {
            AmpUsageRecord {
                cost: message_record.cost,
                message_id: message_record.message_id,
                ..ledger_record
            }
        }
    } else {
        AmpUsageRecord {
            model: ledger_record.model,
            timestamp: message_record.timestamp,
            has_explicit_timestamp: false,
            message_id: message_record.message_id,
            ledger_to_message_id: ledger_record.ledger_to_message_id,
            tokens: ledger_record.tokens,
            cost: if ledger_record.cost > 0.0 {
                ledger_record.cost
            } else {
                message_record.cost
            },
        }
    }
}

/// Parse an Amp thread JSON file
pub fn parse_amp_file(path: &Path) -> Vec<UnifiedMessage> {
    let Some(content) = read_file_or_none(path) else {
        return Vec::new();
    };

    // Get file mtime as last-resort timestamp fallback
    let file_mtime_ms = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let mut bytes = content;
    let thread: AmpThread = match simd_json::from_slice(&mut bytes) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let thread_id = thread.id.clone().unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    let thread_created_ms = thread.created.unwrap_or(0);
    let mut ledger_records =
        parse_amp_ledger_records(thread.usage_ledger, thread_created_ms, file_mtime_ms);
    let message_records =
        parse_amp_message_records(thread.messages, thread_created_ms, file_mtime_ms);

    if ledger_records.is_empty() {
        let mut message_records = message_records;
        message_records.sort_by_key(|record| record.timestamp);
        return message_records
            .into_iter()
            .map(|record| record.into_unified(&thread_id))
            .collect();
    }

    let mut consumed = vec![false; ledger_records.len()];
    let mut search_start = 0usize;
    let mut unmatched_message_records = Vec::new();

    for message_record in &message_records {
        if let Some(index) =
            find_matching_ledger_record(&ledger_records, &consumed, search_start, message_record)
        {
            consumed[index] = true;
            search_start = index.saturating_add(1);
            let merged = merge_amp_records(ledger_records[index].clone(), message_record);
            ledger_records[index] = merged;
        } else {
            unmatched_message_records.push(message_record.clone());
        }
    }

    ledger_records.extend(unmatched_message_records);
    ledger_records.sort_by_key(|record| record.timestamp);
    ledger_records
        .into_iter()
        .map(|record| record.into_unified(&thread_id))
        .collect()
}

