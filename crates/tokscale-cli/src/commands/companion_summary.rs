#![allow(dead_code)]

use crate::paths;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const COMPANION_SUMMARY_SCHEMA_VERSION: u32 = 1;
const COMPANION_SUMMARY_FILE_NAME: &str = "companion-summary.json";
const STALE_AFTER_SECONDS: i64 = 2 * 60 * 60;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSummary {
    pub version: u32,
    pub generated_at: String,
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
    pub collapsed: CompanionCollapsed,
    pub today: CompanionToday,
    pub totals: CompanionTotals,
    #[serde(default)]
    pub providers: Vec<CompanionProvider>,
    #[serde(default)]
    pub quota: Vec<CompanionQuotaProvider>,
    #[serde(default)]
    pub history: Vec<CompanionHistoryDay>,
    #[serde(default)]
    pub contribution: Vec<CompanionContributionDay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagents: Option<CompanionSubagents>,
    pub top: CompanionTop,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_submit: Option<CompanionLatestSubmit>,
    pub health: CompanionHealth,
    pub accuracy: CompanionAccuracy,
}

/// Subagent (Agent-tool / sidechain) usage rollup surfaced in the menu bar.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSubagents {
    pub sessions: i64,
    pub invocations: i64,
    pub tokens: i64,
    pub messages: i64,
    /// Subagent tokens as a fraction of all tokens (0..1).
    pub share: f64,
    #[serde(default)]
    pub top: Vec<CompanionSubagentEntry>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSubagentEntry {
    pub name: String,
    pub tokens: i64,
    pub sessions: i64,
    pub invocations: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionCollapsed {
    pub metric: String,
    pub label: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionToday {
    pub date: String,
    pub cost_usd: f64,
    pub tokens: i64,
    pub messages: i32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionTotals {
    pub cost_usd: f64,
    pub tokens: i64,
    pub active_days: i32,
    pub clients: Vec<String>,
    pub models: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionProvider {
    pub client: String,
    pub cost_usd: f64,
    pub tokens: i64,
    pub messages: i32,
    pub today_cost_usd: f64,
    pub today_tokens: i64,
    pub today_messages: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionQuotaProvider {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub windows: Vec<CompanionQuotaWindow>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionQuotaWindow {
    pub label: String,
    pub used_percent: f64,
    pub remaining_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionHistoryDay {
    pub date: String,
    pub cost_usd: f64,
    pub tokens: i64,
    pub messages: i32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionContributionDay {
    pub date: String,
    pub cost_usd: f64,
    pub intensity: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionTop {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionLatestSubmit {
    pub status: String,
    pub finished_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submission_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionHealth {
    pub summary_path: String,
    pub last_scan_duration_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_refreshed_at: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionAccuracy {
    pub confidence: String,
    pub source_kinds: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn summary_path() -> PathBuf {
    paths::get_cache_dir().join(COMPANION_SUMMARY_FILE_NAME)
}

pub fn read_latest() -> Result<Option<CompanionSummary>> {
    let mut summary = read_from_path(&summary_path())?;
    if let Some(summary) = &mut summary {
        mark_stale(summary, chrono::Utc::now());
    }
    Ok(summary)
}

pub(crate) fn read_from_path(path: &Path) -> Result<Option<CompanionSummary>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read companion summary at {}", path.display()))?;
    let mut summary: CompanionSummary = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse companion summary at {}", path.display()))?;
    dedupe_strings(&mut summary.accuracy.source_kinds);
    Ok(Some(summary))
}

pub fn write_latest(summary: &CompanionSummary) -> Result<()> {
    write_to_path(&summary_path(), summary)
}

pub(crate) fn write_to_path(path: &Path, summary: &CompanionSummary) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create companion summary dir {}",
                parent.display()
            )
        })?;
    }
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, serde_json::to_vec_pretty(summary)?).with_context(|| {
        format!(
            "failed to write companion summary at {}",
            tmp_path.display()
        )
    })?;
    tokscale_core::fs_atomic::replace_file(&tmp_path, path).with_context(|| {
        format!(
            "failed to replace companion summary {} with {}",
            path.display(),
            tmp_path.display()
        )
    })
}

pub(crate) fn mark_stale(summary: &mut CompanionSummary, now: chrono::DateTime<chrono::Utc>) {
    let Ok(generated_at) = chrono::DateTime::parse_from_rfc3339(&summary.generated_at) else {
        summary.stale = true;
        summary.stale_reason = Some("invalid-generated-at".to_string());
        summary.collapsed.state = "stale".to_string();
        return;
    };

    if now
        .signed_duration_since(generated_at.with_timezone(&chrono::Utc))
        .num_seconds()
        > STALE_AFTER_SECONDS
    {
        summary.stale = true;
        summary.stale_reason = Some("summary-older-than-2h".to_string());
        summary.collapsed.state = "stale".to_string();
    }
}

pub(crate) fn format_compact_cost(cost: f64) -> String {
    if cost >= 1000.0 {
        format!("${:.1}K", cost / 1000.0)
    } else if cost >= 100.0 {
        format!("${cost:.0}")
    } else {
        format!("${cost:.2}")
    }
}

pub(crate) fn format_compact_tokens(tokens: i64) -> String {
    if tokens >= 1_000_000_000 {
        format!("{:.1}B", tokens as f64 / 1_000_000_000.0)
    } else if tokens >= 1_000_000 {
        format!("{:.0}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.0}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub fn from_graph(
    graph: &tokscale_core::GraphResult,
    latest_submit: Option<CompanionLatestSubmit>,
    today_date: &str,
    summary_path: &str,
) -> CompanionSummary {
    from_graph_with_usage(graph, latest_submit, today_date, summary_path, &[])
}

pub fn from_graph_with_usage(
    graph: &tokscale_core::GraphResult,
    latest_submit: Option<CompanionLatestSubmit>,
    today_date: &str,
    summary_path: &str,
    usage_outputs: &[crate::commands::usage::UsageOutput],
) -> CompanionSummary {
    let today = graph
        .contributions
        .iter()
        .find(|day| day.date == today_date);
    let today_cost = today.map(|day| day.totals.cost).unwrap_or(0.0);
    let today_tokens = today.map(|day| day.totals.tokens).unwrap_or(0);
    let today_messages = today.map(|day| day.totals.messages).unwrap_or(0);
    let accuracy = tokscale_core::accuracy_report_for_graph(graph);
    let mut source_kinds = accuracy
        .sources
        .iter()
        .map(|source| accuracy_source_kind_label(source.kind).to_string())
        .collect();
    dedupe_strings(&mut source_kinds);

    CompanionSummary {
        version: COMPANION_SUMMARY_SCHEMA_VERSION,
        generated_at: graph.meta.generated_at.clone(),
        stale: false,
        stale_reason: None,
        collapsed: CompanionCollapsed {
            metric: "todayCost".to_string(),
            label: format_compact_cost(today_cost),
            state: if today_tokens > 0 { "normal" } else { "idle" }.to_string(),
        },
        today: CompanionToday {
            date: today_date.to_string(),
            cost_usd: today_cost,
            tokens: today_tokens,
            messages: today_messages,
        },
        totals: CompanionTotals {
            cost_usd: graph.summary.total_cost,
            tokens: graph.summary.total_tokens,
            active_days: graph.summary.active_days,
            clients: graph.summary.clients.clone(),
            models: graph.summary.models.len(),
        },
        providers: provider_breakdown(graph, today_date),
        quota: quota_breakdown(usage_outputs),
        history: history_breakdown(graph, today_date),
        contribution: contribution_breakdown(graph),
        subagents: subagent_breakdown(graph),
        top: CompanionTop {
            client: top_client(graph),
            model: top_model(graph),
        },
        latest_submit,
        health: CompanionHealth {
            summary_path: summary_path.to_string(),
            last_scan_duration_ms: graph.meta.processing_time_ms,
            quota_refreshed_at: (!usage_outputs.is_empty())
                .then(|| chrono::Utc::now().to_rfc3339()),
            warnings: Vec::new(),
        },
        accuracy: CompanionAccuracy {
            confidence: accuracy_confidence_label(accuracy.confidence).to_string(),
            source_kinds,
            warnings: accuracy.warnings.clone(),
        },
    }
}

fn subagent_breakdown(graph: &tokscale_core::GraphResult) -> Option<CompanionSubagents> {
    let s = graph.subagents.as_ref()?;
    if s.total_tokens == 0 && s.agents.is_empty() {
        return None;
    }
    let total = graph.summary.total_tokens.max(1);
    Some(CompanionSubagents {
        sessions: s.session_count,
        invocations: s.invocation_count,
        tokens: s.total_tokens,
        messages: s.total_messages,
        share: s.total_tokens as f64 / total as f64,
        top: s
            .agents
            .iter()
            .take(6)
            .map(|a| CompanionSubagentEntry {
                name: a.name.clone(),
                tokens: a.tokens,
                sessions: a.sessions,
                invocations: a.invocations,
            })
            .collect(),
    })
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = std::collections::BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn accuracy_confidence_label(confidence: tokscale_core::AccuracyConfidence) -> &'static str {
    match confidence {
        tokscale_core::AccuracyConfidence::High => "high",
        tokscale_core::AccuracyConfidence::Medium => "medium",
        tokscale_core::AccuracyConfidence::Low => "low",
    }
}

fn accuracy_source_kind_label(kind: tokscale_core::AccuracySourceKind) -> &'static str {
    match kind {
        tokscale_core::AccuracySourceKind::LocalScan => "local-scan",
        tokscale_core::AccuracySourceKind::ProviderOfficial => "provider-official",
        tokscale_core::AccuracySourceKind::EstimatedPricing => "estimated-pricing",
        tokscale_core::AccuracySourceKind::CustomPricing => "custom-pricing",
        tokscale_core::AccuracySourceKind::SubmittedServer => "submitted-server",
        tokscale_core::AccuracySourceKind::Unknown => "unknown",
    }
}

fn top_client(graph: &tokscale_core::GraphResult) -> Option<String> {
    let mut totals = std::collections::HashMap::<String, f64>::new();
    for day in &graph.contributions {
        for client in &day.clients {
            *totals.entry(client.client.clone()).or_default() += client.cost;
        }
    }
    totals
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(client, _)| client)
}

fn top_model(graph: &tokscale_core::GraphResult) -> Option<String> {
    let mut totals = std::collections::HashMap::<String, f64>::new();
    for day in &graph.contributions {
        for client in &day.clients {
            *totals.entry(client.model_id.clone()).or_default() += client.cost;
        }
    }
    totals
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(model, _)| model)
}

fn provider_breakdown(
    graph: &tokscale_core::GraphResult,
    today_date: &str,
) -> Vec<CompanionProvider> {
    #[derive(Default)]
    struct ProviderAccumulator {
        cost_usd: f64,
        tokens: i64,
        messages: i32,
        today_cost_usd: f64,
        today_tokens: i64,
        today_messages: i32,
        model_costs: std::collections::HashMap<String, f64>,
    }

    let mut providers = std::collections::HashMap::<String, ProviderAccumulator>::new();
    for day in &graph.contributions {
        let is_today = day.date == today_date;
        for client in &day.clients {
            let entry = providers.entry(client.client.clone()).or_default();
            let token_count = client.tokens.total();
            entry.cost_usd += client.cost;
            entry.tokens += token_count;
            entry.messages += client.messages;
            *entry
                .model_costs
                .entry(client.model_id.clone())
                .or_default() += client.cost;
            if is_today {
                entry.today_cost_usd += client.cost;
                entry.today_tokens += token_count;
                entry.today_messages += client.messages;
            }
        }
    }

    let mut providers: Vec<CompanionProvider> = providers
        .into_iter()
        .map(|(client, entry)| {
            let top_model = entry
                .model_costs
                .into_iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(model, _)| model);
            CompanionProvider {
                client,
                cost_usd: entry.cost_usd,
                tokens: entry.tokens,
                messages: entry.messages,
                today_cost_usd: entry.today_cost_usd,
                today_tokens: entry.today_tokens,
                today_messages: entry.today_messages,
                top_model,
            }
        })
        .collect();
    providers.sort_by(|a, b| {
        b.cost_usd
            .partial_cmp(&a.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.client.cmp(&b.client))
    });
    providers
}

pub(crate) fn quota_breakdown(
    usage_outputs: &[crate::commands::usage::UsageOutput],
) -> Vec<CompanionQuotaProvider> {
    let mut providers: Vec<CompanionQuotaProvider> = usage_outputs
        .iter()
        .filter_map(|output| {
            let windows: Vec<CompanionQuotaWindow> = output
                .metrics
                .iter()
                .map(|metric| CompanionQuotaWindow {
                    label: metric.label.clone(),
                    used_percent: metric.used_percent.clamp(0.0, 100.0),
                    remaining_percent: metric.remaining_percent.clamp(0.0, 100.0),
                    remaining_label: metric.remaining_label.clone(),
                    resets_at: metric.resets_at.clone(),
                })
                .collect();
            if windows.is_empty() {
                return None;
            }
            Some(CompanionQuotaProvider {
                provider: output.provider.clone(),
                plan: output.plan.clone(),
                windows,
            })
        })
        .collect();
    providers.sort_by(|a, b| a.provider.cmp(&b.provider));
    providers
}

fn contribution_breakdown(graph: &tokscale_core::GraphResult) -> Vec<CompanionContributionDay> {
    let mut by_date = std::collections::BTreeMap::<String, f64>::new();
    for day in &graph.contributions {
        *by_date.entry(day.date.clone()).or_insert(0.0) += day.totals.cost;
    }
    let max_cost = by_date.values().copied().fold(0.0_f64, f64::max);
    by_date
        .into_iter()
        .map(|(date, cost_usd)| CompanionContributionDay {
            date,
            cost_usd,
            intensity: contribution_intensity(cost_usd, max_cost),
        })
        .collect()
}

fn contribution_intensity(cost: f64, max_cost: f64) -> u8 {
    if cost <= 0.0 || max_cost <= 0.0 {
        return 0;
    }
    let ratio = (cost / max_cost).clamp(0.0, 1.0);
    if ratio <= 0.25 {
        1
    } else if ratio <= 0.5 {
        2
    } else if ratio <= 0.75 {
        3
    } else {
        4
    }
}

fn history_breakdown(
    graph: &tokscale_core::GraphResult,
    today_date: &str,
) -> Vec<CompanionHistoryDay> {
    let mut totals_by_date = std::collections::HashMap::<String, CompanionHistoryDay>::new();
    for day in &graph.contributions {
        let entry = totals_by_date
            .entry(day.date.clone())
            .or_insert_with(|| CompanionHistoryDay {
                date: day.date.clone(),
                cost_usd: 0.0,
                tokens: 0,
                messages: 0,
            });
        entry.cost_usd += day.totals.cost;
        entry.tokens += day.totals.tokens;
        entry.messages += day.totals.messages;
    }

    let Ok(today) = chrono::NaiveDate::parse_from_str(today_date, "%Y-%m-%d") else {
        let mut values: Vec<CompanionHistoryDay> = totals_by_date.into_values().collect();
        values.sort_by(|a, b| a.date.cmp(&b.date));
        return values
            .into_iter()
            .rev()
            .take(14)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    };

    (0..14)
        .rev()
        .map(|days_ago| {
            let date = today - chrono::Duration::days(days_ago);
            let date_key = date.format("%Y-%m-%d").to_string();
            totals_by_date
                .remove(&date_key)
                .unwrap_or(CompanionHistoryDay {
                    date: date_key,
                    cost_usd: 0.0,
                    tokens: 0,
                    messages: 0,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_summary(generated_at: &str) -> CompanionSummary {
        CompanionSummary {
            version: COMPANION_SUMMARY_SCHEMA_VERSION,
            generated_at: generated_at.to_string(),
            stale: false,
            stale_reason: None,
            collapsed: CompanionCollapsed {
                metric: "todayCost".to_string(),
                label: "$1.24".to_string(),
                state: "normal".to_string(),
            },
            today: CompanionToday {
                date: "2026-06-04".to_string(),
                cost_usd: 1.24,
                tokens: 18_000_000,
                messages: 42,
            },
            totals: CompanionTotals {
                cost_usd: 23.91,
                tokens: 35_202_912_831,
                active_days: 120,
                clients: vec!["codex".to_string()],
                models: 18,
            },
            providers: vec![CompanionProvider {
                client: "codex".to_string(),
                cost_usd: 23.91,
                tokens: 35_202_912_831,
                messages: 120,
                today_cost_usd: 1.24,
                today_tokens: 18_000_000,
                today_messages: 42,
                top_model: Some("gpt-5".to_string()),
            }],
            quota: Vec::new(),
            history: Vec::new(),
            contribution: Vec::new(),
            subagents: None,
            top: CompanionTop {
                client: Some("codex".to_string()),
                model: Some("gpt-5".to_string()),
            },
            latest_submit: None,
            health: CompanionHealth {
                summary_path: "/tmp/companion-summary.json".to_string(),
                last_scan_duration_ms: 1800,
                quota_refreshed_at: None,
                warnings: Vec::new(),
            },
            accuracy: CompanionAccuracy {
                confidence: "medium".to_string(),
                source_kinds: vec!["local-scan".to_string(), "estimated-pricing".to_string()],
                warnings: Vec::new(),
            },
        }
    }

    #[test]
    fn read_missing_companion_summary_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("companion-summary.json");

        let summary = read_from_path(&path).unwrap();

        assert!(summary.is_none());
    }

    #[test]
    fn read_companion_summary_dedupes_legacy_accuracy_source_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("companion-summary.json");
        let mut summary = sample_summary("2026-06-04T00:00:00Z");
        summary.accuracy.source_kinds = vec![
            "local-scan".to_string(),
            "local-scan".to_string(),
            "estimated-pricing".to_string(),
            "local-scan".to_string(),
        ];
        write_to_path(&path, &summary).unwrap();

        let summary = read_from_path(&path).unwrap().unwrap();

        assert_eq!(
            summary.accuracy.source_kinds,
            vec!["local-scan", "estimated-pricing"]
        );
    }

    #[test]
    fn read_companion_summary_accepts_legacy_cache_without_provider_breakdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("companion-summary.json");
        std::fs::write(
            &path,
            r#"{
              "version": 1,
              "generatedAt": "2026-06-04T00:00:00Z",
              "stale": false,
              "collapsed": {"metric": "todayCost", "label": "$1", "state": "normal"},
              "today": {"date": "2026-06-04", "costUsd": 1.0, "tokens": 100, "messages": 2},
              "totals": {"costUsd": 1.0, "tokens": 100, "activeDays": 1, "clients": ["codex"], "models": 1},
              "top": {"client": "codex", "model": "gpt-5"},
              "health": {"summaryPath": "/tmp/summary.json", "lastScanDurationMs": 10, "warnings": []},
              "accuracy": {"confidence": "medium", "sourceKinds": ["local-scan"], "warnings": []}
            }"#,
        )
        .unwrap();

        let summary = read_from_path(&path).unwrap().unwrap();

        assert!(summary.providers.is_empty());
    }

    #[test]
    fn mark_stale_flags_old_summary_without_changing_label() {
        let mut summary = sample_summary("2026-06-04T00:00:00Z");
        mark_stale(
            &mut summary,
            chrono::DateTime::parse_from_rfc3339("2026-06-04T02:00:01Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );

        assert!(summary.stale);
        assert_eq!(
            summary.stale_reason.as_deref(),
            Some("summary-older-than-2h")
        );
        assert_eq!(summary.collapsed.label, "$1.24");
    }

    #[test]
    fn format_compact_cost_keeps_menu_bar_label_short() {
        assert_eq!(format_compact_cost(0.0), "$0.00");
        assert_eq!(format_compact_cost(1.236), "$1.24");
        assert_eq!(format_compact_cost(123.8), "$124");
        assert_eq!(format_compact_cost(1200.0), "$1.2K");
    }

    #[test]
    fn format_compact_tokens_keeps_menu_bar_label_short() {
        assert_eq!(format_compact_tokens(42), "42");
        assert_eq!(format_compact_tokens(12_340), "12K");
        assert_eq!(format_compact_tokens(18_000_000), "18M");
        assert_eq!(format_compact_tokens(3_520_000_000), "3.5B");
    }

    #[test]
    fn summary_from_graph_uses_today_for_collapsed_cost() {
        let graph = graph_result_for_test(vec![
            daily_contribution_for_test("2026-06-03", "claude", "claude-sonnet", 1000, 0.50, 3),
            daily_contribution_for_test("2026-06-04", "codex", "gpt-5", 18_000_000, 1.236, 42),
        ]);
        let latest_submit = CompanionLatestSubmit {
            status: "success".to_string(),
            finished_at: "2026-06-04T00:00:05Z".to_string(),
            submission_id: Some("sub_test".to_string()),
        };

        let summary = from_graph(
            &graph,
            Some(latest_submit),
            "2026-06-04",
            "/tmp/companion-summary.json",
        );

        assert_eq!(summary.collapsed.metric, "todayCost");
        assert_eq!(summary.collapsed.label, "$1.24");
        assert_eq!(summary.today.date, "2026-06-04");
        assert_eq!(summary.today.tokens, 18_000_000);
        assert_eq!(summary.today.messages, 42);
        assert_eq!(summary.top.client.as_deref(), Some("codex"));
        assert_eq!(summary.top.model.as_deref(), Some("gpt-5"));
        assert_eq!(
            summary
                .latest_submit
                .as_ref()
                .unwrap()
                .submission_id
                .as_deref(),
            Some("sub_test")
        );
    }

    #[test]
    fn summary_from_graph_includes_provider_breakdown_sorted_by_total_cost() {
        let graph = graph_result_for_test(vec![
            daily_contribution_for_test("2026-06-03", "claude", "claude-sonnet", 1_000, 0.50, 3),
            daily_contribution_for_test("2026-06-03", "gemini", "gemini-pro", 2_000, 0.20, 4),
            daily_contribution_for_test("2026-06-04", "codex", "gpt-5", 18_000, 1.24, 42),
        ]);

        let summary = from_graph(&graph, None, "2026-06-04", "/tmp/companion-summary.json");

        assert_eq!(summary.providers.len(), 3);
        assert_eq!(summary.providers[0].client, "codex");
        assert_eq!(summary.providers[0].cost_usd, 1.24);
        assert_eq!(summary.providers[0].tokens, 18_000);
        assert_eq!(summary.providers[0].messages, 42);
        assert_eq!(summary.providers[0].today_cost_usd, 1.24);
        assert_eq!(summary.providers[0].top_model.as_deref(), Some("gpt-5"));
        assert_eq!(summary.providers[1].client, "claude");
        assert_eq!(summary.providers[1].today_cost_usd, 0.0);
        assert_eq!(summary.providers[2].client, "gemini");
    }

    #[test]
    fn summary_from_graph_includes_subagent_invocations() {
        let mut graph = graph_result_for_test(vec![daily_contribution_for_test(
            "2026-06-04",
            "claude",
            "claude-sonnet",
            10_000,
            0.50,
            3,
        )]);
        graph.subagents = Some(tokscale_core::SubagentSummary {
            total_tokens: 2_500,
            total_messages: 8,
            session_count: 1,
            invocation_count: 3,
            agents: vec![tokscale_core::SubagentEntry {
                name: "Explore".to_string(),
                tokens: 2_500,
                messages: 8,
                sessions: 1,
                invocations: 3,
            }],
        });

        let summary = from_graph(&graph, None, "2026-06-04", "/tmp/companion-summary.json");
        let subagents = summary.subagents.unwrap();

        assert_eq!(subagents.sessions, 1);
        assert_eq!(subagents.invocations, 3);
        assert_eq!(subagents.tokens, 2_500);
        assert_eq!(subagents.top[0].name, "Explore");
        assert_eq!(subagents.top[0].invocations, 3);
    }

    #[test]
    fn summary_from_graph_uses_zero_today_when_today_is_absent() {
        let graph = graph_result_for_test(vec![daily_contribution_for_test(
            "2026-06-03",
            "claude",
            "claude-sonnet",
            1000,
            0.50,
            3,
        )]);

        let summary = from_graph(&graph, None, "2026-06-04", "/tmp/companion-summary.json");

        assert_eq!(summary.collapsed.label, "$0.00");
        assert_eq!(summary.today.date, "2026-06-04");
        assert_eq!(summary.today.tokens, 0);
        assert_eq!(summary.today.messages, 0);
        assert_eq!(summary.top.client.as_deref(), Some("claude"));
    }

    #[test]
    fn summary_from_graph_dedupes_accuracy_source_kinds() {
        let graph = graph_result_for_test(vec![
            daily_contribution_for_test("2026-06-03", "claude", "claude-sonnet", 1000, 0.50, 3),
            daily_contribution_for_test("2026-06-04", "codex", "gpt-5", 18_000, 1.24, 42),
        ]);

        let summary = from_graph(&graph, None, "2026-06-04", "/tmp/companion-summary.json");

        assert_eq!(summary.accuracy.source_kinds, vec!["local-scan"]);
    }

    #[test]
    fn summary_from_graph_includes_recent_history_days() {
        let graph = graph_result_for_test(vec![
            daily_contribution_for_test("2026-05-22", "claude", "claude-sonnet", 50, 0.05, 1),
            daily_contribution_for_test("2026-05-29", "claude", "claude-sonnet", 100, 0.10, 1),
            daily_contribution_for_test("2026-06-01", "codex", "gpt-5", 200, 0.20, 2),
            daily_contribution_for_test("2026-06-04", "gemini", "gemini-pro", 300, 0.30, 3),
        ]);

        let summary = from_graph(&graph, None, "2026-06-04", "/tmp/companion-summary.json");

        assert_eq!(summary.history.len(), 14);
        assert_eq!(summary.history[0].date, "2026-05-22");
        assert_eq!(summary.history[0].cost_usd, 0.05);
        assert_eq!(summary.history[7].date, "2026-05-29");
        assert_eq!(summary.history[7].cost_usd, 0.10);
        assert_eq!(summary.history[10].date, "2026-06-01");
        assert_eq!(summary.history[10].messages, 2);
        assert_eq!(summary.history[13].date, "2026-06-04");
        assert_eq!(summary.history[13].tokens, 300);
    }

    #[test]
    fn summary_from_graph_with_usage_includes_claude_quota_windows() {
        let graph = graph_result_for_test(vec![daily_contribution_for_test(
            "2026-06-04",
            "claude",
            "claude-sonnet",
            300,
            0.30,
            3,
        )]);
        let usage = vec![crate::commands::usage::UsageOutput {
            provider: "Claude".to_string(),
            plan: Some("Pro 5x".to_string()),
            email: None,
            metrics: vec![
                crate::commands::usage::UsageMetric {
                    label: "Session".to_string(),
                    used_percent: 72.0,
                    remaining_percent: 28.0,
                    remaining_label: None,
                    resets_at: Some("2026-06-04T10:00:00Z".to_string()),
                },
                crate::commands::usage::UsageMetric {
                    label: "Weekly".to_string(),
                    used_percent: 41.0,
                    remaining_percent: 59.0,
                    remaining_label: None,
                    resets_at: Some("2026-06-08T00:00:00Z".to_string()),
                },
            ],
        }];

        let summary = from_graph_with_usage(
            &graph,
            None,
            "2026-06-04",
            "/tmp/companion-summary.json",
            &usage,
        );

        assert_eq!(summary.quota.len(), 1);
        assert_eq!(summary.quota[0].provider, "Claude");
        assert_eq!(summary.quota[0].plan.as_deref(), Some("Pro 5x"));
        assert_eq!(summary.quota[0].windows.len(), 2);
        assert_eq!(summary.quota[0].windows[0].label, "Session");
        assert_eq!(summary.quota[0].windows[0].used_percent, 72.0);
        assert_eq!(summary.quota[0].windows[1].label, "Weekly");
        assert_eq!(
            summary.quota[0].windows[1].resets_at.as_deref(),
            Some("2026-06-08T00:00:00Z")
        );
    }

    fn daily_contribution_for_test(
        date: &str,
        client: &str,
        model: &str,
        tokens: i64,
        cost: f64,
        messages: i32,
    ) -> tokscale_core::DailyContribution {
        tokscale_core::DailyContribution {
            date: date.to_string(),
            totals: tokscale_core::DailyTotals {
                tokens,
                cost,
                messages,
            },
            intensity: 1,
            token_breakdown: tokscale_core::TokenBreakdown {
                input: tokens,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            clients: vec![tokscale_core::ClientContribution {
                client: client.to_string(),
                model_id: model.to_string(),
                provider_id: "openai".to_string(),
                tokens: tokscale_core::TokenBreakdown {
                    input: tokens,
                    output: 0,
                    cache_read: 0,
                    cache_write: 0,
                    reasoning: 0,
                },
                cost,
                messages,
            }],
            active_time_ms: None,
        }
    }

    fn graph_result_for_test(
        contributions: Vec<tokscale_core::DailyContribution>,
    ) -> tokscale_core::GraphResult {
        let summary = tokscale_core::calculate_summary(&contributions);
        let years = tokscale_core::calculate_years(&contributions);
        tokscale_core::GraphResult {
            meta: tokscale_core::GraphMeta {
                generated_at: "2026-06-04T00:00:00Z".to_string(),
                version: "3.0.3-test".to_string(),
                date_range_start: contributions
                    .first()
                    .map(|day| day.date.clone())
                    .unwrap_or_default(),
                date_range_end: contributions
                    .last()
                    .map(|day| day.date.clone())
                    .unwrap_or_default(),
                processing_time_ms: 1800,
            },
            summary,
            years,
            contributions,
            time_metrics: None,
            subagents: None,
        }
    }
}
