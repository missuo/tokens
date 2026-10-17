//! Muse session parser
//!
//! Parses the Muse usage export cached locally at
//! `~/.config/tokens/muse-cache/muse-usage.json` — the same cache pattern
//! Cursor uses for its dashboard export (`~/.config/tokens/cursor-cache/`).
//!
//! The Muse runtime keeps its token usage in its own records and writes no
//! session files to disk, so there is nothing for a scanner to read directly.
//! Instead, an export of those records — daily aggregates in the clawdboard
//! export shape — is placed in the cache directory, and this parser turns it
//! back into unified messages: one per (day, model) pair.
//!
//! The export carries no cost data (Muse records none) and no cache split
//! (input is full-context processed volume), so messages are emitted with
//! cost 0 and `CostSource::Unknown`: local pricing may estimate later, and
//! the tokens still count even though the internal model ids
//! (`ipnext/avocado-*`) price at $0.

use super::UnifiedMessage;
use crate::TokenBreakdown;
use serde::Deserialize;
use std::path::Path;

/// Top level of the export file: `{ "dailyAggregates": [...] }`. Unknown
/// fields (export metadata such as `exportedAt`) are ignored.
#[derive(Debug, Deserialize)]
struct MuseExport {
    #[serde(default, rename = "dailyAggregates")]
    daily_aggregates: Vec<MuseDailyAggregate>,
}

/// One day of Muse usage. Mirrors the clawdboard daily-aggregate shape the
/// importer in `tokens-cli` already consumes.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MuseDailyAggregate {
    /// `YYYY-MM-DD`, bucketed by the exporter in the runtime's local day.
    date: String,
    /// Aggregate totals, used only when a row names exactly one model and
    /// carries no per-model breakdown.
    #[serde(default)]
    input_tokens: i64,
    #[serde(default)]
    output_tokens: i64,
    #[serde(default)]
    models_used: Vec<String>,
    #[serde(default)]
    model_breakdowns: Vec<MuseModelBreakdown>,
}

/// Per-model slice of one day's aggregate.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MuseModelBreakdown {
    model_name: String,
    #[serde(default)]
    input_tokens: i64,
    #[serde(default)]
    output_tokens: i64,
    #[serde(default)]
    cache_read_tokens: i64,
    #[serde(default)]
    cache_creation_tokens: i64,
}

/// Parse a date-only string (`YYYY-MM-DD`) to Unix milliseconds at noon UTC,
/// the same convention the Cursor parser uses for date-only rows: noon keeps
/// the local calendar date stable across timezones, so the derived
/// `UnifiedMessage::date` matches the export row's date.
fn parse_date_to_timestamp(date_str: &str) -> i64 {
    use chrono::{NaiveDate, TimeZone, Utc};

    match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        Ok(date) => date
            .and_hms_opt(12, 0, 0)
            .map(|dt| Utc.from_utc_datetime(&dt).timestamp_millis())
            .unwrap_or(0),
        Err(_) => 0,
    }
}

fn build_message(
    date: &str,
    timestamp: i64,
    model: &str,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
) -> UnifiedMessage {
    let mut message = UnifiedMessage::new(
        "muse",
        model,
        // The export's model ids are internal to the Muse runtime and name
        // no external provider; the client itself is the honest attribution.
        "muse",
        format!("muse-{date}"),
        timestamp,
        TokenBreakdown {
            input: input.max(0),
            output: output.max(0),
            cache_read: cache_read.max(0),
            cache_write: cache_write.max(0),
            reasoning: 0,
        },
        0.0,
    );
    message.dedup_key = Some(format!("muse:{date}:{model}"));
    message
}

/// Parse a Muse usage export file into unified messages.
///
/// One message per (day, model) pair from `modelBreakdowns`. A missing,
/// unreadable, or malformed file yields no messages rather than failing the
/// caller's whole scan — an install without the export simply has no Muse
/// data yet. Rows whose date does not parse, or that name no model, are
/// skipped for the same reason.
pub fn parse_muse_file(path: &Path) -> Vec<UnifiedMessage> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let export: MuseExport = match serde_json::from_str(&content) {
        Ok(export) => export,
        Err(_) => return vec![],
    };

    let mut messages = Vec::with_capacity(export.daily_aggregates.len());
    for day in &export.daily_aggregates {
        let timestamp = parse_date_to_timestamp(&day.date);
        if timestamp == 0 {
            continue;
        }

        if day.model_breakdowns.is_empty() {
            // No per-model split: the aggregate totals can only be attributed
            // when the row names exactly one model. Anything else would be a
            // guess, so the row is dropped rather than misattributed.
            if day.models_used.len() == 1 {
                let model = day.models_used[0].trim();
                if !model.is_empty() {
                    messages.push(build_message(
                        &day.date,
                        timestamp,
                        model,
                        day.input_tokens,
                        day.output_tokens,
                        0,
                        0,
                    ));
                }
            }
            continue;
        }

        for breakdown in &day.model_breakdowns {
            let model = breakdown.model_name.trim();
            if model.is_empty() {
                continue;
            }
            messages.push(build_message(
                &day.date,
                timestamp,
                model,
                breakdown.input_tokens,
                breakdown.output_tokens,
                breakdown.cache_read_tokens,
                breakdown.cache_creation_tokens,
            ));
        }
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_export(json: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("muse-usage.json");
        std::fs::write(&path, json).unwrap();
        (dir, path)
    }

    #[test]
    fn test_parse_muse_export_emits_one_message_per_day_model() {
        let json = r#"{
            "dailyAggregates": [
                {
                    "date": "2026-10-14",
                    "source": "muse",
                    "inputTokens": 300,
                    "outputTokens": 30,
                    "cacheCreationTokens": 0,
                    "cacheReadTokens": 0,
                    "totalCost": "0.0000",
                    "modelsUsed": ["ipnext/avocado-5.16-v1", "ipnext/avocado-5.16-v4"],
                    "modelBreakdowns": [
                        {"modelName": "ipnext/avocado-5.16-v1", "inputTokens": 200, "outputTokens": 20, "cost": 0, "cacheCreationTokens": 0, "cacheReadTokens": 0},
                        {"modelName": "ipnext/avocado-5.16-v4", "inputTokens": 100, "outputTokens": 10, "cost": 0, "cacheCreationTokens": 0, "cacheReadTokens": 0}
                    ]
                },
                {
                    "date": "2026-10-15",
                    "source": "muse",
                    "inputTokens": 50,
                    "outputTokens": 5,
                    "totalCost": "0.0000",
                    "modelsUsed": ["ipnext/avocado-5.16-v1"],
                    "modelBreakdowns": [
                        {"modelName": "ipnext/avocado-5.16-v1", "inputTokens": 50, "outputTokens": 5, "cost": 0}
                    ]
                }
            ]
        }"#;

        let (_dir, path) = write_export(json);
        let messages = parse_muse_file(&path);
        assert_eq!(messages.len(), 3);

        let first = &messages[0];
        assert_eq!(first.client, "muse");
        assert_eq!(first.model_id, "ipnext/avocado-5.16-v1");
        assert_eq!(first.provider_id, "muse");
        assert_eq!(first.session_id, "muse-2026-10-14");
        assert_eq!(first.date, "2026-10-14");
        assert_eq!(first.tokens.input, 200);
        assert_eq!(first.tokens.output, 20);
        assert_eq!(first.tokens.cache_read, 0);
        assert_eq!(first.tokens.cache_write, 0);
        assert_eq!(first.tokens.reasoning, 0);
        assert_eq!(first.cost, 0.0);
        assert_eq!(
            first.cost_source,
            super::super::CostSource::Unknown
        );
        assert_eq!(
            first.dedup_key.as_deref(),
            Some("muse:2026-10-14:ipnext/avocado-5.16-v1")
        );

        // Timestamp is noon UTC on the row's date (2026-10-14T12:00:00Z).
        assert_eq!(
            first.timestamp,
            chrono::DateTime::parse_from_rfc3339("2026-10-14T12:00:00Z")
                .unwrap()
                .timestamp_millis()
        );

        assert_eq!(messages[1].model_id, "ipnext/avocado-5.16-v4");
        assert_eq!(messages[1].tokens.input, 100);
        assert_eq!(messages[2].date, "2026-10-15");
    }

    #[test]
    fn test_parse_muse_export_single_model_fallback_without_breakdowns() {
        let json = r#"{
            "dailyAggregates": [
                {
                    "date": "2026-10-14",
                    "source": "muse",
                    "inputTokens": 700,
                    "outputTokens": 70,
                    "modelsUsed": ["ipnext/avocado-5.16-v1"]
                },
                {
                    "date": "2026-10-15",
                    "source": "muse",
                    "inputTokens": 900,
                    "outputTokens": 90,
                    "modelsUsed": ["ipnext/avocado-5.16-v1", "ipnext/avocado-5.16-v4"]
                }
            ]
        }"#;

        let (_dir, path) = write_export(json);
        let messages = parse_muse_file(&path);
        // Only the single-model row can be attributed honestly.
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tokens.input, 700);
        assert_eq!(messages[0].tokens.output, 70);
    }

    #[test]
    fn test_parse_muse_export_missing_or_malformed_file_is_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("muse-usage.json");
        assert!(parse_muse_file(&missing).is_empty());

        let (_dir, path) = write_export("{ not json");
        assert!(parse_muse_file(&path).is_empty());

        let (_dir, path) = write_export(r#"{"dailyAggregates": []}"#);
        assert!(parse_muse_file(&path).is_empty());
    }

    #[test]
    fn test_parse_muse_export_skips_unparseable_dates_and_empty_models() {
        let json = r#"{
            "dailyAggregates": [
                {
                    "date": "2026-13-40",
                    "source": "muse",
                    "inputTokens": 10,
                    "outputTokens": 1,
                    "modelBreakdowns": [
                        {"modelName": "ipnext/avocado-5.16-v1", "inputTokens": 10, "outputTokens": 1}
                    ]
                },
                {
                    "date": "2026-10-14",
                    "source": "muse",
                    "inputTokens": 10,
                    "outputTokens": 1,
                    "modelBreakdowns": [
                        {"modelName": "  ", "inputTokens": 10, "outputTokens": 1}
                    ]
                }
            ]
        }"#;

        let (_dir, path) = write_export(json);
        assert!(parse_muse_file(&path).is_empty());
    }
}
