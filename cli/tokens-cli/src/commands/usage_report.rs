//! Local usage report for Menu Bar / machine consumers.
//!
//! `tokens usage --json` scans session files via tokens-core (Layer A source
//! cache), rebuilds a Layer B usage snapshot under the tokens cache dir, and
//! emits a stable JSON schema for the macOS Menu Bar app.

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use tokens_core::{
    bucket_timezone, clear_source_message_cache, generate_local_graph_report, BucketTimezone,
    ClientContribution, DailyContribution, DailyTotals, GraphResult, ReportOptions, TokenBreakdown,
};

use crate::settings;

const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
const SNAPSHOT_FILENAME: &str = "usage-snapshot-v1.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "lower")]
pub enum UsagePeriod {
    Today,
    #[clap(name = "7d")]
    Days7,
    #[clap(name = "30d")]
    Days30,
    All,
}

impl UsagePeriod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Days7 => "7d",
            Self::Days30 => "30d",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageReport {
    schema_version: u32,
    generated_at: String,
    period: String,
    date_range: DateRange,
    scan: ScanInfo,
    summary: UsageSummary,
    token_breakdown: TokenBreakdownDto,
    by_client: Vec<ClientUsage>,
    by_model: Vec<ModelUsageRow>,
    by_day: Vec<DayUsage>,
    meta: UsageMeta,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DateRange {
    start: String,
    end: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanInfo {
    mode: String,
    force_rescan: bool,
    duration_ms: u32,
    cache: ScanCacheInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanCacheInfo {
    /// Layer A hit counts are not exported from core yet; reserved / diagnostic.
    source_hits: u64,
    source_misses: u64,
    snapshot_rebuilt: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageSummary {
    total_tokens: i64,
    total_cost: f64,
    messages: i32,
    active_days: i32,
    clients: Vec<String>,
    models: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenBreakdownDto {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientUsage {
    client: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    share: f64,
    models: Vec<ClientModelUsage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientModelUsage {
    model_id: String,
    provider_id: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    share: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelUsageRow {
    model_id: String,
    provider_id: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    share: f64,
    clients: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DayUsage {
    date: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    intensity: u8,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageMeta {
    cli_version: String,
    timezone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageErrorReport {
    schema_version: u32,
    error: UsageErrorBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageErrorBody {
    code: String,
    message: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageSnapshotFile {
    schema_version: u32,
    generated_at: String,
    timezone: String,
    contributions: Vec<SnapshotDay>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotDay {
    date: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    intensity: u8,
    token_breakdown: TokenBreakdownDtoSerde,
    clients: Vec<SnapshotClientRow>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenBreakdownDtoSerde {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotClientRow {
    client: String,
    model_id: String,
    provider_id: String,
    tokens: TokenBreakdownDtoSerde,
    cost: f64,
    messages: i32,
}

/// Run `tokens usage`.
///
/// * `refresh` — incremental rescan (uses Layer A); Menu Bar timer / Refresh button.
/// * `force_rescan` — clear Layer A + B, then full rescan.
/// * neither — serve from Layer B snapshot when it is still same bucket-day
///   (fast period switches); otherwise scan.
pub(crate) fn run(json: bool, period: UsagePeriod, refresh: bool, force_rescan: bool) -> Result<()> {
    match build_report(period, refresh, force_rescan) {
        Ok(report) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_human(&report);
            }
            Ok(())
        }
        Err(err) => {
            if json {
                let payload = UsageErrorReport {
                    schema_version: 1,
                    error: UsageErrorBody {
                        code: "scan_failed".to_string(),
                        message: format!("{err:#}"),
                    },
                };
                let _ = writeln!(
                    std::io::stdout(),
                    "{}",
                    serde_json::to_string_pretty(&payload)?
                );
            }
            Err(err)
        }
    }
}

fn build_report(period: UsagePeriod, refresh: bool, force_rescan: bool) -> Result<UsageReport> {
    let started = std::time::Instant::now();
    let today = bucket_timezone().today();
    let (since, until) = period_bounds(period, today);

    if force_rescan {
        clear_source_message_cache().map_err(|e| anyhow::anyhow!(e))?;
        let _ = fs::remove_file(snapshot_path());
    } else if !refresh {
        if let Some(report) =
            try_report_from_snapshot(period, today, since.as_deref(), until.as_deref())
        {
            return Ok(report);
        }
    }

    let graph = scan_all_local()?;
    write_snapshot_from_graph(&graph)?;

    let mut contributions = graph.contributions;
    filter_contributions(&mut contributions, since.as_deref(), until.as_deref());

    let duration_ms = started.elapsed().as_millis() as u32;
    let mode = if force_rescan {
        "full"
    } else {
        "incremental"
    };

    Ok(report_from_contributions(
        period,
        force_rescan,
        mode,
        duration_ms,
        true,
        0,
        0,
        &contributions,
        &graph.meta.generated_at,
    ))
}

fn scan_all_local() -> Result<GraphResult> {
    let options = ReportOptions {
        scanner_settings: settings::load_scanner_settings(),
        ..ReportOptions::default()
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start async runtime for usage scan")?;

    rt.block_on(generate_local_graph_report(options))
        .map_err(|e| anyhow::anyhow!(e))
}

fn period_bounds(period: UsagePeriod, today: NaiveDate) -> (Option<String>, Option<String>) {
    match period {
        UsagePeriod::Today => {
            let d = today.format("%Y-%m-%d").to_string();
            (Some(d.clone()), Some(d))
        }
        UsagePeriod::Days7 => {
            let start = today - Duration::days(6);
            (
                Some(start.format("%Y-%m-%d").to_string()),
                Some(today.format("%Y-%m-%d").to_string()),
            )
        }
        UsagePeriod::Days30 => {
            let start = today - Duration::days(29);
            (
                Some(start.format("%Y-%m-%d").to_string()),
                Some(today.format("%Y-%m-%d").to_string()),
            )
        }
        UsagePeriod::All => (None, None),
    }
}

fn filter_contributions(days: &mut Vec<DailyContribution>, since: Option<&str>, until: Option<&str>) {
    if let Some(since) = since {
        days.retain(|d| d.date.as_str() >= since);
    }
    if let Some(until) = until {
        days.retain(|d| d.date.as_str() <= until);
    }
}

fn snapshot_path() -> PathBuf {
    tokens_core::paths::get_cache_dir().join(SNAPSHOT_FILENAME)
}

fn timezone_label() -> String {
    match bucket_timezone() {
        BucketTimezone::Local => iana_time_zone::get_timezone().unwrap_or_else(|_| "local".into()),
        BucketTimezone::Named(tz) => tz.name().to_string(),
    }
}

fn write_snapshot_from_graph(graph: &GraphResult) -> Result<()> {
    let snapshot = UsageSnapshotFile {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        generated_at: graph.meta.generated_at.clone(),
        timezone: timezone_label(),
        contributions: graph
            .contributions
            .iter()
            .map(|c| SnapshotDay {
                date: c.date.clone(),
                tokens: c.totals.tokens,
                cost: c.totals.cost,
                messages: c.totals.messages,
                intensity: c.intensity,
                token_breakdown: TokenBreakdownDtoSerde {
                    input: c.token_breakdown.input,
                    output: c.token_breakdown.output,
                    cache_read: c.token_breakdown.cache_read,
                    cache_write: c.token_breakdown.cache_write,
                    reasoning: c.token_breakdown.reasoning,
                },
                clients: c
                    .clients
                    .iter()
                    .map(|row| SnapshotClientRow {
                        client: row.client.clone(),
                        model_id: row.model_id.clone(),
                        provider_id: row.provider_id.clone(),
                        tokens: TokenBreakdownDtoSerde {
                            input: row.tokens.input,
                            output: row.tokens.output,
                            cache_read: row.tokens.cache_read,
                            cache_write: row.tokens.cache_write,
                            reasoning: row.tokens.reasoning,
                        },
                        cost: row.cost,
                        messages: row.messages,
                    })
                    .collect(),
            })
            .collect(),
    };

    let path = snapshot_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(&snapshot)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn try_report_from_snapshot(
    period: UsagePeriod,
    today: NaiveDate,
    since: Option<&str>,
    until: Option<&str>,
) -> Option<UsageReport> {
    let path = snapshot_path();
    let raw = fs::read_to_string(&path).ok()?;
    let snapshot: UsageSnapshotFile = serde_json::from_str(&raw).ok()?;
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return None;
    }
    let generated_day = snapshot.generated_at.get(..10)?;
    let today_s = today.format("%Y-%m-%d").to_string();
    if generated_day != today_s {
        return None;
    }
    if snapshot.timezone != timezone_label() {
        return None;
    }

    let mut days = snapshot.contributions;
    if let Some(since) = since {
        days.retain(|d| d.date.as_str() >= since);
    }
    if let Some(until) = until {
        days.retain(|d| d.date.as_str() <= until);
    }

    Some(report_from_snapshot_days(
        period,
        false,
        "snapshot",
        0,
        false,
        0,
        0,
        &days,
        &snapshot.generated_at,
    ))
}

fn report_from_snapshot_days(
    period: UsagePeriod,
    force_rescan: bool,
    mode: &str,
    duration_ms: u32,
    snapshot_rebuilt: bool,
    source_hits: u64,
    source_misses: u64,
    days: &[SnapshotDay],
    generated_at: &str,
) -> UsageReport {
    let contributions: Vec<DailyContribution> = days
        .iter()
        .map(|d| DailyContribution {
            date: d.date.clone(),
            totals: DailyTotals {
                tokens: d.tokens,
                cost: d.cost,
                messages: d.messages,
            },
            intensity: d.intensity,
            token_breakdown: TokenBreakdown {
                input: d.token_breakdown.input,
                output: d.token_breakdown.output,
                cache_read: d.token_breakdown.cache_read,
                cache_write: d.token_breakdown.cache_write,
                reasoning: d.token_breakdown.reasoning,
            },
            clients: d
                .clients
                .iter()
                .map(|c| ClientContribution {
                    client: c.client.clone(),
                    model_id: c.model_id.clone(),
                    provider_id: c.provider_id.clone(),
                    tokens: TokenBreakdown {
                        input: c.tokens.input,
                        output: c.tokens.output,
                        cache_read: c.tokens.cache_read,
                        cache_write: c.tokens.cache_write,
                        reasoning: c.tokens.reasoning,
                    },
                    cost: c.cost,
                    messages: c.messages,
                })
                .collect(),
            active_time_ms: None,
        })
        .collect();

    report_from_contributions(
        period,
        force_rescan,
        mode,
        duration_ms,
        snapshot_rebuilt,
        source_hits,
        source_misses,
        &contributions,
        generated_at,
    )
}

fn report_from_contributions(
    period: UsagePeriod,
    force_rescan: bool,
    mode: &str,
    duration_ms: u32,
    snapshot_rebuilt: bool,
    source_hits: u64,
    source_misses: u64,
    contributions: &[DailyContribution],
    generated_at: &str,
) -> UsageReport {
    let (range_start, range_end) = date_range_of(contributions, period);

    let mut total_tokens: i64 = 0;
    let mut total_cost = 0.0;
    let mut total_messages: i32 = 0;
    let mut breakdown = TokenBreakdown::default();
    let mut active_days = 0i32;

    #[derive(Default)]
    struct Agg {
        tokens: i64,
        cost: f64,
        messages: i32,
    }

    let mut by_client: BTreeMap<String, Agg> = BTreeMap::new();
    let mut by_client_model: BTreeMap<(String, String, String), Agg> = BTreeMap::new();
    let mut by_model: BTreeMap<(String, String), Agg> = BTreeMap::new();
    let mut model_clients: BTreeMap<(String, String), BTreeMap<String, ()>> = BTreeMap::new();
    let mut client_set: BTreeMap<String, ()> = BTreeMap::new();
    let mut model_set: BTreeMap<String, ()> = BTreeMap::new();

    for day in contributions {
        total_tokens = total_tokens.saturating_add(day.totals.tokens);
        total_cost += day.totals.cost;
        total_messages = total_messages.saturating_add(day.totals.messages);
        breakdown.input = breakdown.input.saturating_add(day.token_breakdown.input);
        breakdown.output = breakdown.output.saturating_add(day.token_breakdown.output);
        breakdown.cache_read = breakdown
            .cache_read
            .saturating_add(day.token_breakdown.cache_read);
        breakdown.cache_write = breakdown
            .cache_write
            .saturating_add(day.token_breakdown.cache_write);
        breakdown.reasoning = breakdown
            .reasoning
            .saturating_add(day.token_breakdown.reasoning);

        if day.totals.tokens > 0 || day.totals.cost > 0.0 || day.totals.messages > 0 {
            active_days += 1;
        }

        for row in &day.clients {
            client_set.insert(row.client.clone(), ());
            model_set.insert(row.model_id.clone(), ());

            let client_entry = by_client.entry(row.client.clone()).or_default();
            client_entry.tokens = client_entry.tokens.saturating_add(row.tokens.total());
            client_entry.cost += row.cost;
            client_entry.messages = client_entry.messages.saturating_add(row.messages);

            let cm_key = (
                row.client.clone(),
                row.model_id.clone(),
                row.provider_id.clone(),
            );
            let cm = by_client_model.entry(cm_key).or_default();
            cm.tokens = cm.tokens.saturating_add(row.tokens.total());
            cm.cost += row.cost;
            cm.messages = cm.messages.saturating_add(row.messages);

            let m_key = (row.model_id.clone(), row.provider_id.clone());
            let m = by_model.entry(m_key.clone()).or_default();
            m.tokens = m.tokens.saturating_add(row.tokens.total());
            m.cost += row.cost;
            m.messages = m.messages.saturating_add(row.messages);
            model_clients
                .entry(m_key)
                .or_default()
                .insert(row.client.clone(), ());
        }
    }

    let token_denom = if total_tokens > 0 {
        total_tokens as f64
    } else {
        1.0
    };

    let mut by_client_out: Vec<ClientUsage> = by_client
        .into_iter()
        .map(|(client, agg)| {
            let mut models: Vec<ClientModelUsage> = by_client_model
                .iter()
                .filter(|((c, _, _), _)| c == &client)
                .map(|((_, model_id, provider_id), m)| ClientModelUsage {
                    model_id: model_id.clone(),
                    provider_id: provider_id.clone(),
                    tokens: m.tokens,
                    cost: m.cost,
                    messages: m.messages,
                    share: if agg.tokens > 0 {
                        m.tokens as f64 / agg.tokens as f64
                    } else {
                        0.0
                    },
                })
                .collect();
            models.sort_by(|a, b| b.tokens.cmp(&a.tokens));
            ClientUsage {
                client,
                tokens: agg.tokens,
                cost: agg.cost,
                messages: agg.messages,
                share: agg.tokens as f64 / token_denom,
                models,
            }
        })
        .collect();
    by_client_out.sort_by(|a, b| b.tokens.cmp(&a.tokens));

    let mut by_model_out: Vec<ModelUsageRow> = by_model
        .into_iter()
        .map(|(key, agg)| {
            let (model_id, provider_id) = key.clone();
            let clients = model_clients
                .get(&key)
                .map(|set| set.keys().cloned().collect())
                .unwrap_or_default();
            ModelUsageRow {
                model_id,
                provider_id,
                tokens: agg.tokens,
                cost: agg.cost,
                messages: agg.messages,
                share: agg.tokens as f64 / token_denom,
                clients,
            }
        })
        .collect();
    by_model_out.sort_by(|a, b| b.tokens.cmp(&a.tokens));

    let by_day: Vec<DayUsage> = contributions
        .iter()
        .map(|d| DayUsage {
            date: d.date.clone(),
            tokens: d.totals.tokens,
            cost: d.totals.cost,
            messages: d.totals.messages,
            intensity: d.intensity,
        })
        .collect();

    UsageReport {
        schema_version: 1,
        generated_at: generated_at.to_string(),
        period: period.as_str().to_string(),
        date_range: DateRange {
            start: range_start,
            end: range_end,
        },
        scan: ScanInfo {
            mode: mode.to_string(),
            force_rescan,
            duration_ms,
            cache: ScanCacheInfo {
                source_hits,
                source_misses,
                snapshot_rebuilt,
            },
        },
        summary: UsageSummary {
            total_tokens,
            total_cost,
            messages: total_messages,
            active_days,
            clients: client_set.into_keys().collect(),
            models: model_set.into_keys().collect(),
        },
        token_breakdown: TokenBreakdownDto {
            input: breakdown.input,
            output: breakdown.output,
            cache_read: breakdown.cache_read,
            cache_write: breakdown.cache_write,
            reasoning: breakdown.reasoning,
        },
        by_client: by_client_out,
        by_model: by_model_out,
        by_day,
        meta: UsageMeta {
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            timezone: timezone_label(),
        },
    }
}

fn date_range_of(contributions: &[DailyContribution], period: UsagePeriod) -> (String, String) {
    if let (Some(first), Some(last)) = (contributions.first(), contributions.last()) {
        return (first.date.clone(), last.date.clone());
    }
    let today = bucket_timezone().today().format("%Y-%m-%d").to_string();
    match period {
        UsagePeriod::All => (String::new(), String::new()),
        _ => (today.clone(), today),
    }
}

fn print_human(report: &UsageReport) {
    println!("Tokens usage ({})", report.period);
    println!(
        "  tokens: {}  cost: ${:.2}  messages: {}",
        report.summary.total_tokens, report.summary.total_cost, report.summary.messages
    );
    println!(
        "  range: {} → {}  mode: {}  ({} ms)",
        report.date_range.start,
        report.date_range.end,
        report.scan.mode,
        report.scan.duration_ms
    );
    if !report.by_client.is_empty() {
        println!("  by client:");
        for c in report.by_client.iter().take(10) {
            println!(
                "    {:<16} {:>10}  ${:>8.2}",
                c.client, c.tokens, c.cost
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_day() -> DailyContribution {
        DailyContribution {
            date: "2026-07-26".into(),
            totals: DailyTotals {
                tokens: 1000,
                cost: 1.5,
                messages: 3,
            },
            intensity: 2,
            token_breakdown: TokenBreakdown {
                input: 600,
                output: 400,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            clients: vec![ClientContribution {
                client: "claude-code".into(),
                model_id: "claude-opus".into(),
                provider_id: "anthropic".into(),
                tokens: TokenBreakdown {
                    input: 600,
                    output: 400,
                    cache_read: 0,
                    cache_write: 0,
                    reasoning: 0,
                },
                cost: 1.5,
                messages: 3,
            }],
            active_time_ms: None,
        }
    }

    #[test]
    fn report_rolls_up_client_and_model() {
        let report = report_from_contributions(
            UsagePeriod::Today,
            false,
            "incremental",
            10,
            true,
            0,
            0,
            &[sample_day()],
            "2026-07-26T00:00:00Z",
        );
        assert_eq!(report.summary.total_tokens, 1000);
        assert_eq!(report.by_client.len(), 1);
        assert_eq!(report.by_client[0].client, "claude-code");
        assert_eq!(report.by_client[0].models.len(), 1);
        assert_eq!(report.by_model[0].model_id, "claude-opus");
        assert_eq!(report.by_model[0].clients, vec!["claude-code".to_string()]);
        assert!((report.by_client[0].share - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn period_bounds_today() {
        let day = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let (since, until) = period_bounds(UsagePeriod::Today, day);
        assert_eq!(since.as_deref(), Some("2026-07-26"));
        assert_eq!(until.as_deref(), Some("2026-07-26"));
    }

    #[test]
    fn period_bounds_7d_is_inclusive_week() {
        let day = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let (since, until) = period_bounds(UsagePeriod::Days7, day);
        assert_eq!(since.as_deref(), Some("2026-07-20"));
        assert_eq!(until.as_deref(), Some("2026-07-26"));
    }

    #[test]
    fn filter_contributions_respects_bounds() {
        let mut days = vec![
            DailyContribution {
                date: "2026-07-20".into(),
                totals: DailyTotals::default(),
                intensity: 0,
                token_breakdown: TokenBreakdown::default(),
                clients: vec![],
                active_time_ms: None,
            },
            sample_day(),
        ];
        filter_contributions(&mut days, Some("2026-07-26"), Some("2026-07-26"));
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].date, "2026-07-26");
    }
}
