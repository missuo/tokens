//! Local usage report for Menu Bar / machine consumers.
//!
//! `tokens usage --json` scans session files via tokens-core (Layer A source
//! cache), rebuilds a Layer B usage snapshot under the tokens cache dir, and
//! emits a stable JSON schema for the macOS Menu Bar app.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use tokens_core::{
    bucket_timezone, clear_source_message_cache, generate_local_usage_scan, BucketTimezone,
    DailyContribution, LocalUsageScan, ReportOptions, TokenBreakdown,
};

use crate::commands::unattributed_diagnostics::{update_diagnostics, DIAGNOSTIC_FILENAME};
use crate::commands::usage_snapshot;
use crate::settings;

const V2_REPORT_SCHEMA_VERSION: u32 = 2;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "lower")]
pub(crate) enum UsageContract {
    V2,
    V3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageRequestArgs {
    pub contract: Option<UsageContract>,
    pub period: Option<UsagePeriod>,
    pub since: Option<String>,
    pub until: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UsageReportRequest {
    V2Preset { period: UsagePeriod },
    V3 { selection: UsageReportSelection },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UsageReportSelection {
    Preset { period: UsagePeriod },
    Custom { since: NaiveDate, until: NaiveDate },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UsageReportAction {
    LegacyV2 { period: UsagePeriod },
    V3 { selection: UsageReportSelection },
}

pub(crate) fn resolve_usage_action(
    args: UsageRequestArgs,
    reporting_today: NaiveDate,
) -> Result<UsageReportAction> {
    Ok(match validate_usage_request(args, reporting_today)? {
        UsageReportRequest::V2Preset { period } => UsageReportAction::LegacyV2 { period },
        UsageReportRequest::V3 { selection } => UsageReportAction::V3 { selection },
    })
}

pub(crate) fn validate_usage_request(
    args: UsageRequestArgs,
    reporting_today: NaiveDate,
) -> Result<UsageReportRequest> {
    let has_custom_date = args.since.is_some() || args.until.is_some();

    if has_custom_date && args.period.is_some() {
        anyhow::bail!("--period cannot be combined with --since or --until");
    }

    if has_custom_date {
        let (Some(since), Some(until)) = (args.since.as_deref(), args.until.as_deref()) else {
            anyhow::bail!("custom usage ranges require both --since and --until");
        };
        if args.contract != Some(UsageContract::V3) {
            anyhow::bail!("--since and --until require --contract v3");
        }

        let since = parse_usage_date("--since", since)?;
        let until = parse_usage_date("--until", until)?;
        if until < since {
            anyhow::bail!("--until must be on or after --since");
        }
        if since > reporting_today {
            anyhow::bail!("--since must not be after reporting today ({reporting_today})");
        }
        if until > reporting_today {
            anyhow::bail!("--until must not be after reporting today ({reporting_today})");
        }

        return Ok(UsageReportRequest::V3 {
            selection: UsageReportSelection::Custom { since, until },
        });
    }

    let period = args.period.unwrap_or(UsagePeriod::Today);
    match args.contract.unwrap_or(UsageContract::V2) {
        UsageContract::V2 => Ok(UsageReportRequest::V2Preset { period }),
        UsageContract::V3 => Ok(UsageReportRequest::V3 {
            selection: UsageReportSelection::Preset { period },
        }),
    }
}

fn parse_usage_date(flag: &str, value: &str) -> Result<NaiveDate> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("{flag} must use YYYY-MM-DD"))?;
    if date.format("%Y-%m-%d").to_string() != value {
        anyhow::bail!("{flag} must use YYYY-MM-DD");
    }
    Ok(date)
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
    by_project: Vec<ProjectUsage>,
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
struct ProjectUsage {
    project_key: Option<String>,
    display_name: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    models: Vec<ProjectModelUsage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectModelUsage {
    model_id: String,
    provider_id: String,
    tokens: i64,
    cost: f64,
    messages: i32,
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

fn build_v2_error_report(error: &anyhow::Error) -> UsageErrorReport {
    UsageErrorReport {
        schema_version: V2_REPORT_SCHEMA_VERSION,
        error: UsageErrorBody {
            code: "scan_failed".to_string(),
            message: format!("{error:#}"),
        },
    }
}

/// Run `tokens usage`.
///
/// * `refresh` — incremental rescan (uses Layer A); Menu Bar timer / Refresh button.
/// * `force_rescan` — clear Layer A + B, then full rescan.
/// * neither — serve from Layer B snapshot when it is still same bucket-day
///   (fast period switches); otherwise scan.
pub(crate) fn run(
    json: bool,
    period: UsagePeriod,
    refresh: bool,
    force_rescan: bool,
) -> Result<()> {
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
                let payload = build_v2_error_report(&err);
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

fn with_snapshot_operation<T, ReadSnapshot, Rebuild>(
    cache_dir: &Path,
    bypass_snapshot: bool,
    mut read_snapshot: ReadSnapshot,
    rebuild: Rebuild,
) -> Result<T>
where
    ReadSnapshot: FnMut() -> Option<T>,
    Rebuild: FnOnce() -> Result<T>,
{
    if !bypass_snapshot {
        let _shared = usage_snapshot::acquire_shared_operation_lock(cache_dir)?;
        if let Some(value) = read_snapshot() {
            return Ok(value);
        }
    }

    let _exclusive = usage_snapshot::acquire_exclusive_operation_lock(cache_dir)?;
    if !bypass_snapshot {
        if let Some(value) = read_snapshot() {
            return Ok(value);
        }
    }
    rebuild()
}

pub(crate) struct UsageSnapshotAcquisition {
    pub(crate) snapshot: usage_snapshot::UsageSnapshot,
    pub(crate) mode: String,
    pub(crate) force_rescan: bool,
    pub(crate) duration_ms: u32,
    pub(crate) source_hits: u64,
    pub(crate) source_misses: u64,
    pub(crate) snapshot_rebuilt: bool,
}

pub(crate) fn acquire_usage_snapshot(
    reporting_now: DateTime<Utc>,
    refresh: bool,
    force_rescan: bool,
) -> Result<UsageSnapshotAcquisition> {
    let started = std::time::Instant::now();
    let reporting_timezone = bucket_timezone();
    let bucket_date = reporting_timezone.date_of_ms(reporting_now.timestamp_millis());
    if bucket_date.is_empty() {
        anyhow::bail!("reporting time is outside the configured timezone calendar");
    }
    let timezone = timezone_label_for(reporting_timezone);
    let cache_dir = tokens_core::paths::get_cache_dir();

    with_snapshot_operation(
        &cache_dir,
        refresh || force_rescan,
        || {
            usage_snapshot::load_reusable_snapshot(&cache_dir, &bucket_date, &timezone).map(
                |snapshot| UsageSnapshotAcquisition {
                    snapshot,
                    mode: "snapshot".to_string(),
                    force_rescan: false,
                    duration_ms: 0,
                    source_hits: 0,
                    source_misses: 0,
                    snapshot_rebuilt: false,
                },
            )
        },
        || {
            if force_rescan {
                clear_force_rescan_caches(&cache_dir, || {
                    clear_source_message_cache().map_err(anyhow::Error::msg)
                })?;
            }

            let scan = scan_all_local()?;
            let snapshot = usage_snapshot::build_snapshot(&scan, &bucket_date, &timezone)?;
            usage_snapshot::write_snapshot(&cache_dir, &snapshot)?;
            let diagnostics_path = cache_dir.join(DIAGNOSTIC_FILENAME);
            if let Err(error) = update_diagnostics(
                &diagnostics_path,
                &scan.graph.meta.generated_at,
                &timezone,
                &scan.unattributed_sessions,
            ) {
                eprintln!("tokens: warning: {error:#}");
            }

            Ok(UsageSnapshotAcquisition {
                snapshot,
                mode: if force_rescan { "full" } else { "incremental" }.to_string(),
                force_rescan,
                duration_ms: elapsed_millis_u32(&started),
                source_hits: 0,
                source_misses: 0,
                snapshot_rebuilt: true,
            })
        },
    )
}

fn build_report(period: UsagePeriod, refresh: bool, force_rescan: bool) -> Result<UsageReport> {
    build_current_v2_report(
        period,
        refresh,
        force_rescan,
        Utc::now,
        acquire_usage_snapshot,
    )
}

fn build_current_v2_report<Now, Acquire>(
    period: UsagePeriod,
    refresh: bool,
    force_rescan: bool,
    mut now: Now,
    mut acquire: Acquire,
) -> Result<UsageReport>
where
    Now: FnMut() -> DateTime<Utc>,
    Acquire: FnMut(DateTime<Utc>, bool, bool) -> Result<UsageSnapshotAcquisition>,
{
    let acquisition_started_at = now();
    let mut acquisition = acquire(acquisition_started_at, refresh, force_rescan)?;

    for attempt in 0..2 {
        let reporting_now = now();
        let reporting_timezone = parse_snapshot_timezone(&acquisition.snapshot.timezone)?;
        let reporting_date = reporting_timezone.date_of_ms(reporting_now.timestamp_millis());
        if reporting_date.is_empty() {
            anyhow::bail!("reporting time is outside the snapshot timezone calendar");
        }
        if reporting_date == acquisition.snapshot.bucket_date {
            let today = NaiveDate::parse_from_str(&acquisition.snapshot.bucket_date, "%Y-%m-%d")
                .context("invalid acquired usage snapshot bucket date")?;
            let (since, until) = period_bounds(period, today);
            let chart_source = usage_snapshot::daily_contributions(&acquisition.snapshot);
            let contributions =
                selected_contributions(&chart_source, since.as_deref(), until.as_deref());

            return Ok(report_from_contributions(
                period,
                force_rescan || acquisition.force_rescan,
                &acquisition.mode,
                acquisition.duration_ms,
                acquisition.snapshot_rebuilt,
                acquisition.source_hits,
                acquisition.source_misses,
                &contributions,
                &chart_source,
                today,
                &acquisition.snapshot.generated_at,
            ));
        }
        if attempt == 1 {
            break;
        }
        acquisition = acquire(reporting_now, false, false)?;
    }

    anyhow::bail!("reporting day changed repeatedly while acquiring the usage snapshot")
}

fn elapsed_millis_u32(started: &std::time::Instant) -> u32 {
    started.elapsed().as_millis().min(u32::MAX as u128) as u32
}

fn clear_force_rescan_caches<F>(cache_dir: &Path, clear_source: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let source_result = clear_source();
    let snapshot_result = usage_snapshot::clear_usage_snapshots(cache_dir);
    let mut failures = Vec::new();
    if let Err(error) = source_result {
        failures.push(format!("clear Layer A source cache: {error:#}"));
    }
    if let Err(error) = snapshot_result {
        failures.push(format!("clear Layer B usage snapshots: {error:#}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("force rescan cache clear failed: {}", failures.join("; "))
    }
}

fn scan_all_local() -> Result<LocalUsageScan> {
    let options = ReportOptions {
        scanner_settings: settings::load_scanner_settings(),
        ..ReportOptions::default()
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start async runtime for usage scan")?;

    rt.block_on(generate_local_usage_scan(options))
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

fn selected_contributions(
    days: &[DailyContribution],
    since: Option<&str>,
    until: Option<&str>,
) -> Vec<DailyContribution> {
    days.iter()
        .filter(|day| contribution_is_selected(day, since, until))
        .cloned()
        .collect()
}

#[cfg(test)]
fn filter_contributions(
    days: &mut Vec<DailyContribution>,
    since: Option<&str>,
    until: Option<&str>,
) {
    days.retain(|day| contribution_is_selected(day, since, until));
}

fn contribution_is_selected(
    day: &DailyContribution,
    since: Option<&str>,
    until: Option<&str>,
) -> bool {
    since.is_none_or(|since| day.date.as_str() >= since)
        && until.is_none_or(|until| day.date.as_str() <= until)
}

fn timezone_label() -> String {
    timezone_label_for(bucket_timezone())
}

fn parse_snapshot_timezone(value: &str) -> Result<BucketTimezone> {
    if value == "local" {
        Ok(BucketTimezone::Local)
    } else {
        tokens_core::parse_bucket_timezone(value)
            .with_context(|| format!("invalid snapshot timezone {value}"))
    }
}

fn timezone_label_for(timezone: BucketTimezone) -> String {
    match timezone {
        BucketTimezone::Local => iana_time_zone::get_timezone().unwrap_or_else(|_| "local".into()),
        BucketTimezone::Named(tz) => tz.name().to_string(),
    }
}

#[cfg(test)]
fn try_report_from_snapshot_in(
    cache_dir: &Path,
    period: UsagePeriod,
    today: NaiveDate,
    since: Option<&str>,
    until: Option<&str>,
    expected_timezone: &str,
) -> Option<UsageReport> {
    let today_string = today.format("%Y-%m-%d").to_string();
    let snapshot =
        usage_snapshot::load_reusable_snapshot(cache_dir, &today_string, expected_timezone)?;

    // Full snapshot history drives the 14-day cost chart even when the selected
    // period is `today`.
    let chart_source = usage_snapshot::daily_contributions(&snapshot);
    let contributions = selected_contributions(&chart_source, since, until);

    Some(report_from_contributions(
        period,
        false,
        "snapshot",
        0,
        false,
        0,
        0,
        &contributions,
        &chart_source,
        today,
        &snapshot.generated_at,
    ))
}

fn report_from_contributions(
    period: UsagePeriod,
    force_rescan: bool,
    mode: &str,
    duration_ms: u32,
    snapshot_rebuilt: bool,
    source_hits: u64,
    source_misses: u64,
    // contributions: period-filtered (TOTAL / BREAKDOWN / CLIENT / MODEL)
    // chart_contributions: wider history for the fixed 14-day cost chart
    contributions: &[DailyContribution],
    chart_contributions: &[DailyContribution],
    today: NaiveDate,
    generated_at: &str,
) -> UsageReport {
    let (range_start, range_end) = date_range_of(contributions, period, today);

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

    #[derive(Default)]
    struct ProjectAgg {
        totals: Agg,
        display_name: String,
        label_date: String,
    }

    let mut by_client: BTreeMap<String, Agg> = BTreeMap::new();
    let mut by_client_model: BTreeMap<(String, String, String), Agg> = BTreeMap::new();
    let mut by_model: BTreeMap<(String, String), Agg> = BTreeMap::new();
    let mut model_clients: BTreeMap<(String, String), BTreeMap<String, ()>> = BTreeMap::new();
    let mut by_project: BTreeMap<Option<String>, ProjectAgg> = BTreeMap::new();
    let mut by_project_model: BTreeMap<(Option<String>, String, String), Agg> = BTreeMap::new();
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

        for project in &day.projects {
            let project_entry = by_project.entry(project.project_key.clone()).or_default();
            project_entry.totals.tokens = project_entry
                .totals
                .tokens
                .saturating_add(project.totals.tokens);
            project_entry.totals.cost += project.totals.cost;
            project_entry.totals.messages = project_entry
                .totals
                .messages
                .saturating_add(project.totals.messages);
            if !project.project_label.trim().is_empty()
                && (project_entry.display_name.is_empty() || day.date >= project_entry.label_date)
            {
                project_entry.display_name = project.project_label.clone();
                project_entry.label_date = day.date.clone();
            }
            for model in &project.models {
                let model_entry = by_project_model
                    .entry((
                        project.project_key.clone(),
                        model.model_id.clone(),
                        model.provider_id.clone(),
                    ))
                    .or_default();
                model_entry.tokens = model_entry.tokens.saturating_add(model.tokens);
                model_entry.cost += model.cost;
                model_entry.messages = model_entry.messages.saturating_add(model.messages);
            }
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

    // Group project-model rows once so project assembly stays O(project-models)
    // instead of rescanning the full map for every project.
    let mut models_by_project: BTreeMap<Option<String>, Vec<ProjectModelUsage>> = BTreeMap::new();
    for ((project_key, model_id, provider_id), model) in by_project_model {
        models_by_project
            .entry(project_key)
            .or_default()
            .push(ProjectModelUsage {
                model_id,
                provider_id,
                tokens: model.tokens,
                cost: model.cost,
                messages: model.messages,
            });
    }

    let mut by_project_out: Vec<ProjectUsage> = by_project
        .into_iter()
        .map(|(project_key, project)| {
            let mut models = models_by_project.remove(&project_key).unwrap_or_default();
            models.sort_by(|a, b| {
                b.cost
                    .total_cmp(&a.cost)
                    .then_with(|| b.tokens.cmp(&a.tokens))
                    .then_with(|| a.model_id.cmp(&b.model_id))
                    .then_with(|| a.provider_id.cmp(&b.provider_id))
            });
            let display_name = if project_key.is_none() {
                "Unattributed".to_string()
            } else {
                project.display_name
            };
            ProjectUsage {
                project_key,
                display_name,
                tokens: project.totals.tokens,
                cost: project.totals.cost,
                messages: project.totals.messages,
                models,
            }
        })
        .collect();
    by_project_out.sort_by(|a, b| {
        b.cost
            .total_cmp(&a.cost)
            .then_with(|| b.tokens.cmp(&a.tokens))
            .then_with(|| a.display_name.cmp(&b.display_name))
            .then_with(|| a.project_key.cmp(&b.project_key))
    });

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

    // Cost chart is always the last 14 calendar days, independent of period.
    // Zero-fill missing days so the axis stays continuous after midnight.
    let by_day = chart_by_day(chart_contributions, today, 14);

    UsageReport {
        schema_version: V2_REPORT_SCHEMA_VERSION,
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
        by_project: by_project_out,
        by_model: by_model_out,
        by_day,
        meta: UsageMeta {
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            timezone: timezone_label(),
        },
    }
}

/// Last `limit` calendar days ending on `today`, ascending, zero-filled.
fn chart_by_day(source: &[DailyContribution], today: NaiveDate, limit: i64) -> Vec<DayUsage> {
    let limit = limit.max(1);
    let start = today - Duration::days(limit - 1);
    let mut by_date: BTreeMap<String, &DailyContribution> = BTreeMap::new();
    for day in source {
        if let Ok(d) = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") {
            if d >= start && d <= today {
                by_date.insert(day.date.clone(), day);
            }
        }
    }

    let mut out = Vec::with_capacity(limit as usize);
    let mut cursor = start;
    while cursor <= today {
        let key = cursor.format("%Y-%m-%d").to_string();
        if let Some(day) = by_date.get(&key) {
            out.push(DayUsage {
                date: day.date.clone(),
                tokens: day.totals.tokens,
                cost: day.totals.cost,
                messages: day.totals.messages,
                intensity: day.intensity,
            });
        } else {
            out.push(DayUsage {
                date: key,
                tokens: 0,
                cost: 0.0,
                messages: 0,
                intensity: 0,
            });
        }
        cursor += Duration::days(1);
    }
    out
}

fn date_range_of(
    contributions: &[DailyContribution],
    period: UsagePeriod,
    today: NaiveDate,
) -> (String, String) {
    if let (Some(first), Some(last)) = (contributions.first(), contributions.last()) {
        return (first.date.clone(), last.date.clone());
    }
    let today = today.format("%Y-%m-%d").to_string();
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
        report.date_range.start, report.date_range.end, report.scan.mode, report.scan.duration_ms
    );
    if !report.by_client.is_empty() {
        println!("  by client:");
        for c in report.by_client.iter().take(10) {
            println!("    {:<16} {:>10}  ${:>8.2}", c.client, c.tokens, c.cost);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokens_core::{
        ClientContribution, DailyHourlyUsageFacts, DailyTotals, ProjectContribution,
        ProjectModelContribution,
    };

    fn reporting_today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 4).unwrap()
    }

    fn usage_request(
        contract: Option<UsageContract>,
        period: Option<UsagePeriod>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> UsageRequestArgs {
        UsageRequestArgs {
            contract,
            period,
            since: since.map(str::to_string),
            until: until.map(str::to_string),
        }
    }

    #[test]
    fn v2_wire_schema_stays_separate_from_snapshot_schema() {
        let today = reporting_today();
        let report = report_from_contributions(
            UsagePeriod::Today,
            false,
            "snapshot",
            0,
            false,
            0,
            0,
            &[],
            &[],
            today,
            "2026-08-04T00:00:00Z",
        );
        let error = build_v2_error_report(&anyhow::anyhow!("scan failed"));

        assert_eq!(V2_REPORT_SCHEMA_VERSION, 2);
        assert_eq!(report.schema_version, V2_REPORT_SCHEMA_VERSION);
        assert_eq!(error.schema_version, V2_REPORT_SCHEMA_VERSION);
    }

    #[test]
    fn v2_json_error_envelope_remains_the_exact_external_v2_shape() {
        assert_eq!(
            serde_json::to_value(build_v2_error_report(&anyhow::anyhow!(
                "representative scan failure"
            )))
            .unwrap(),
            json!({
                "schemaVersion": 2,
                "error": {
                    "code": "scan_failed",
                    "message": "representative scan failure"
                }
            })
        );
    }

    #[test]
    fn v2_presets_route_to_legacy_report_without_scanning() {
        for (args, period) in [
            (usage_request(None, None, None, None), UsagePeriod::Today),
            (
                usage_request(None, Some(UsagePeriod::Days7), None, None),
                UsagePeriod::Days7,
            ),
            (
                usage_request(
                    Some(UsageContract::V2),
                    Some(UsagePeriod::Days30),
                    None,
                    None,
                ),
                UsagePeriod::Days30,
            ),
        ] {
            assert_eq!(
                resolve_usage_action(args, reporting_today()).unwrap(),
                UsageReportAction::LegacyV2 { period }
            );
        }
    }

    #[test]
    fn v3_preset_and_custom_route_to_v3_seam_without_scanning() {
        assert_eq!(
            resolve_usage_action(
                usage_request(Some(UsageContract::V3), Some(UsagePeriod::All), None, None,),
                reporting_today(),
            )
            .unwrap(),
            UsageReportAction::V3 {
                selection: UsageReportSelection::Preset {
                    period: UsagePeriod::All,
                },
            }
        );
        assert_eq!(
            resolve_usage_action(
                usage_request(
                    Some(UsageContract::V3),
                    None,
                    Some("2026-08-01"),
                    Some("2026-08-04"),
                ),
                reporting_today(),
            )
            .unwrap(),
            UsageReportAction::V3 {
                selection: UsageReportSelection::Custom {
                    since: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
                    until: reporting_today(),
                },
            }
        );
    }

    #[test]
    fn explicit_v2_contract_rejects_custom_dates() {
        let error = validate_usage_request(
            usage_request(
                Some(UsageContract::V2),
                None,
                Some("2026-07-01"),
                Some("2026-08-04"),
            ),
            reporting_today(),
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--since and --until require --contract v3"
        );
    }

    #[test]
    fn omitted_contract_and_period_validate_as_v2_today_preset() {
        let request =
            validate_usage_request(usage_request(None, None, None, None), reporting_today())
                .unwrap();

        assert_eq!(
            request,
            UsageReportRequest::V2Preset {
                period: UsagePeriod::Today,
            }
        );
    }

    #[test]
    fn explicit_v2_period_validates_as_v2_preset() {
        let request = validate_usage_request(
            usage_request(
                Some(UsageContract::V2),
                Some(UsagePeriod::Days30),
                None,
                None,
            ),
            reporting_today(),
        )
        .unwrap();

        assert_eq!(
            request,
            UsageReportRequest::V2Preset {
                period: UsagePeriod::Days30,
            }
        );
    }

    #[test]
    fn v3_period_validates_as_v3_preset() {
        let request = validate_usage_request(
            usage_request(
                Some(UsageContract::V3),
                Some(UsagePeriod::Days7),
                None,
                None,
            ),
            reporting_today(),
        )
        .unwrap();

        assert_eq!(
            request,
            UsageReportRequest::V3 {
                selection: UsageReportSelection::Preset {
                    period: UsagePeriod::Days7,
                },
            }
        );
    }

    #[test]
    fn v3_dates_validate_as_custom_selection() {
        let request = validate_usage_request(
            usage_request(
                Some(UsageContract::V3),
                None,
                Some("2026-07-01"),
                Some("2026-08-04"),
            ),
            reporting_today(),
        )
        .unwrap();

        assert_eq!(
            request,
            UsageReportRequest::V3 {
                selection: UsageReportSelection::Custom {
                    since: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                    until: reporting_today(),
                },
            }
        );
    }

    #[test]
    fn custom_dates_require_v3_contract() {
        let error = validate_usage_request(
            usage_request(None, None, Some("2026-07-01"), Some("2026-08-04")),
            reporting_today(),
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--since and --until require --contract v3"
        );
    }

    #[test]
    fn explicit_period_cannot_be_mixed_with_custom_dates() {
        let error = validate_usage_request(
            usage_request(
                Some(UsageContract::V3),
                Some(UsagePeriod::Today),
                Some("2026-08-01"),
                Some("2026-08-04"),
            ),
            reporting_today(),
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--period cannot be combined with --since or --until"
        );
    }

    #[test]
    fn custom_selection_requires_both_dates() {
        for request in [
            usage_request(Some(UsageContract::V3), None, Some("2026-08-01"), None),
            usage_request(Some(UsageContract::V3), None, None, Some("2026-08-04")),
        ] {
            let error = validate_usage_request(request, reporting_today()).unwrap_err();
            assert_eq!(
                error.to_string(),
                "custom usage ranges require both --since and --until"
            );
        }
    }

    #[test]
    fn custom_dates_must_use_strict_calendar_format() {
        for request in [
            usage_request(
                Some(UsageContract::V3),
                None,
                Some("2026-8-01"),
                Some("2026-08-04"),
            ),
            usage_request(
                Some(UsageContract::V3),
                None,
                Some("2026-08-01"),
                Some("2026-02-30"),
            ),
        ] {
            let error = validate_usage_request(request, reporting_today()).unwrap_err();
            assert!(error.to_string().contains("must use YYYY-MM-DD"));
        }
    }

    #[test]
    fn custom_until_cannot_precede_since() {
        let error = validate_usage_request(
            usage_request(
                Some(UsageContract::V3),
                None,
                Some("2026-08-04"),
                Some("2026-08-03"),
            ),
            reporting_today(),
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "--until must be on or after --since");
    }

    #[test]
    fn custom_dates_cannot_be_after_reporting_today() {
        for (request, flag) in [
            (
                usage_request(
                    Some(UsageContract::V3),
                    None,
                    Some("2026-08-05"),
                    Some("2026-08-05"),
                ),
                "--since",
            ),
            (
                usage_request(
                    Some(UsageContract::V3),
                    None,
                    Some("2026-08-04"),
                    Some("2026-08-05"),
                ),
                "--until",
            ),
        ] {
            let error = validate_usage_request(request, reporting_today()).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("{flag} must not be after reporting today (2026-08-04)")
            );
        }
    }

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
            projects: vec![ProjectContribution {
                project_key: Some("/work/example".into()),
                project_label: "example".into(),
                totals: DailyTotals {
                    tokens: 1000,
                    cost: 1.5,
                    messages: 3,
                },
                models: vec![ProjectModelContribution {
                    model_id: "claude-opus".into(),
                    provider_id: "anthropic".into(),
                    tokens: 1000,
                    cost: 1.5,
                    messages: 3,
                }],
            }],
            active_time_ms: None,
        }
    }

    fn contract_day(
        date: &str,
        totals: DailyTotals,
        intensity: u8,
        token_breakdown: TokenBreakdown,
        clients: Vec<ClientContribution>,
        projects: Vec<ProjectContribution>,
    ) -> DailyContribution {
        DailyContribution {
            date: date.into(),
            totals,
            intensity,
            token_breakdown,
            clients,
            projects,
            active_time_ms: None,
        }
    }

    fn adapter_client(
        client: &str,
        model_id: &str,
        provider_id: &str,
        tokens: i64,
        cost: f64,
        messages: i32,
    ) -> ClientContribution {
        ClientContribution {
            client: client.into(),
            model_id: model_id.into(),
            provider_id: provider_id.into(),
            tokens: TokenBreakdown {
                input: tokens,
                ..TokenBreakdown::default()
            },
            cost,
            messages,
        }
    }

    fn adapter_project_model(
        model_id: &str,
        provider_id: &str,
        tokens: i64,
        cost: f64,
        messages: i32,
    ) -> ProjectModelContribution {
        ProjectModelContribution {
            model_id: model_id.into(),
            provider_id: provider_id.into(),
            tokens,
            cost,
            messages,
        }
    }

    fn adapter_project(
        project_key: Option<&str>,
        project_label: &str,
        tokens: i64,
        cost: f64,
        messages: i32,
        models: Vec<ProjectModelContribution>,
    ) -> ProjectContribution {
        ProjectContribution {
            project_key: project_key.map(str::to_string),
            project_label: project_label.into(),
            totals: DailyTotals {
                tokens,
                cost,
                messages,
            },
            models,
        }
    }

    fn v2_snapshot_adapter_days() -> Vec<DailyContribution> {
        vec![
            contract_day(
                "2026-07-25",
                DailyTotals {
                    tokens: 25,
                    cost: 0.25,
                    messages: 1,
                },
                1,
                TokenBreakdown {
                    input: 25,
                    ..TokenBreakdown::default()
                },
                vec![adapter_client("history", "old", "p0", 25, 0.25, 1)],
                vec![adapter_project(
                    Some("/work/history"),
                    "History",
                    25,
                    0.25,
                    1,
                    vec![adapter_project_model("old", "p0", 25, 0.25, 1)],
                )],
            ),
            contract_day(
                "2026-08-01",
                DailyTotals {
                    tokens: 100,
                    cost: 2.0,
                    messages: 3,
                },
                2,
                TokenBreakdown {
                    input: 100,
                    ..TokenBreakdown::default()
                },
                vec![
                    adapter_client("alpha", "m1", "p1", 60, 1.2, 1),
                    adapter_client("alpha", "m2", "p2", 20, 0.4, 1),
                    adapter_client("beta", "m1", "p1", 20, 0.4, 1),
                ],
                vec![
                    adapter_project(
                        Some("/work/alpha"),
                        "Alpha Old",
                        80,
                        1.6,
                        2,
                        vec![
                            adapter_project_model("m1", "p1", 60, 1.2, 1),
                            adapter_project_model("m2", "p2", 20, 0.4, 1),
                        ],
                    ),
                    adapter_project(
                        None,
                        "",
                        20,
                        0.4,
                        1,
                        vec![adapter_project_model("m1", "p1", 20, 0.4, 1)],
                    ),
                ],
            ),
            contract_day(
                "2026-08-04",
                DailyTotals {
                    tokens: 200,
                    cost: 4.0,
                    messages: 4,
                },
                4,
                TokenBreakdown {
                    input: 200,
                    ..TokenBreakdown::default()
                },
                vec![
                    adapter_client("alpha", "m1", "p1", 20, 0.4, 1),
                    adapter_client("beta", "m2", "p2", 180, 3.6, 3),
                ],
                vec![
                    adapter_project(
                        Some("/work/alpha"),
                        "Alpha New",
                        50,
                        1.0,
                        1,
                        vec![
                            adapter_project_model("m1", "p1", 20, 0.4, 1),
                            adapter_project_model("m2", "p2", 30, 0.6, 0),
                        ],
                    ),
                    adapter_project(
                        Some("/work/beta"),
                        "Beta",
                        150,
                        3.0,
                        3,
                        vec![adapter_project_model("m2", "p2", 150, 3.0, 3)],
                    ),
                ],
            ),
        ]
    }

    fn expected_v2_snapshot_adapter_report() -> UsageReport {
        let day = |date: &str, tokens, cost, messages, intensity| DayUsage {
            date: date.into(),
            tokens,
            cost,
            messages,
            intensity,
        };
        UsageReport {
            schema_version: 2,
            generated_at: "2026-08-04T12:34:56Z".into(),
            period: "7d".into(),
            date_range: DateRange {
                start: "2026-08-01".into(),
                end: "2026-08-04".into(),
            },
            scan: ScanInfo {
                mode: "snapshot".into(),
                force_rescan: false,
                duration_ms: 0,
                cache: ScanCacheInfo {
                    source_hits: 0,
                    source_misses: 0,
                    snapshot_rebuilt: false,
                },
            },
            summary: UsageSummary {
                total_tokens: 300,
                total_cost: 6.0,
                messages: 7,
                active_days: 2,
                clients: vec!["alpha".into(), "beta".into()],
                models: vec!["m1".into(), "m2".into()],
            },
            token_breakdown: TokenBreakdownDto {
                input: 300,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            by_client: vec![
                ClientUsage {
                    client: "beta".into(),
                    tokens: 200,
                    cost: 4.0,
                    messages: 4,
                    share: 0.6666666666666666,
                    models: vec![
                        ClientModelUsage {
                            model_id: "m2".into(),
                            provider_id: "p2".into(),
                            tokens: 180,
                            cost: 3.6,
                            messages: 3,
                            share: 0.9,
                        },
                        ClientModelUsage {
                            model_id: "m1".into(),
                            provider_id: "p1".into(),
                            tokens: 20,
                            cost: 0.4,
                            messages: 1,
                            share: 0.1,
                        },
                    ],
                },
                ClientUsage {
                    client: "alpha".into(),
                    tokens: 100,
                    cost: 2.0,
                    messages: 3,
                    share: 0.3333333333333333,
                    models: vec![
                        ClientModelUsage {
                            model_id: "m1".into(),
                            provider_id: "p1".into(),
                            tokens: 80,
                            cost: 1.6,
                            messages: 2,
                            share: 0.8,
                        },
                        ClientModelUsage {
                            model_id: "m2".into(),
                            provider_id: "p2".into(),
                            tokens: 20,
                            cost: 0.4,
                            messages: 1,
                            share: 0.2,
                        },
                    ],
                },
            ],
            by_project: vec![
                ProjectUsage {
                    project_key: Some("/work/beta".into()),
                    display_name: "Beta".into(),
                    tokens: 150,
                    cost: 3.0,
                    messages: 3,
                    models: vec![ProjectModelUsage {
                        model_id: "m2".into(),
                        provider_id: "p2".into(),
                        tokens: 150,
                        cost: 3.0,
                        messages: 3,
                    }],
                },
                ProjectUsage {
                    project_key: Some("/work/alpha".into()),
                    display_name: "Alpha New".into(),
                    tokens: 130,
                    cost: 2.6,
                    messages: 3,
                    models: vec![
                        ProjectModelUsage {
                            model_id: "m1".into(),
                            provider_id: "p1".into(),
                            tokens: 80,
                            cost: 1.6,
                            messages: 2,
                        },
                        ProjectModelUsage {
                            model_id: "m2".into(),
                            provider_id: "p2".into(),
                            tokens: 50,
                            cost: 1.0,
                            messages: 1,
                        },
                    ],
                },
                ProjectUsage {
                    project_key: None,
                    display_name: "Unattributed".into(),
                    tokens: 20,
                    cost: 0.4,
                    messages: 1,
                    models: vec![ProjectModelUsage {
                        model_id: "m1".into(),
                        provider_id: "p1".into(),
                        tokens: 20,
                        cost: 0.4,
                        messages: 1,
                    }],
                },
            ],
            by_model: vec![
                ModelUsageRow {
                    model_id: "m2".into(),
                    provider_id: "p2".into(),
                    tokens: 200,
                    cost: 4.0,
                    messages: 4,
                    share: 0.6666666666666666,
                    clients: vec!["alpha".into(), "beta".into()],
                },
                ModelUsageRow {
                    model_id: "m1".into(),
                    provider_id: "p1".into(),
                    tokens: 100,
                    cost: 2.0,
                    messages: 3,
                    share: 0.3333333333333333,
                    clients: vec!["alpha".into(), "beta".into()],
                },
            ],
            by_day: vec![
                day("2026-07-22", 0, 0.0, 0, 0),
                day("2026-07-23", 0, 0.0, 0, 0),
                day("2026-07-24", 0, 0.0, 0, 0),
                day("2026-07-25", 25, 0.25, 1, 1),
                day("2026-07-26", 0, 0.0, 0, 0),
                day("2026-07-27", 0, 0.0, 0, 0),
                day("2026-07-28", 0, 0.0, 0, 0),
                day("2026-07-29", 0, 0.0, 0, 0),
                day("2026-07-30", 0, 0.0, 0, 0),
                day("2026-07-31", 0, 0.0, 0, 0),
                day("2026-08-01", 100, 2.0, 3, 3),
                day("2026-08-02", 0, 0.0, 0, 0),
                day("2026-08-03", 0, 0.0, 0, 0),
                day("2026-08-04", 200, 4.0, 4, 4),
            ],
            meta: UsageMeta {
                cli_version: "27.0.1".into(),
                timezone: "UTC".into(),
            },
        }
    }

    #[test]
    fn v2_report_serializes_the_complete_established_contract() {
        tokens_core::set_bucket_timezone(BucketTimezone::Named(chrono_tz::UTC));
        let first = contract_day(
            "2026-08-01",
            DailyTotals {
                tokens: 100,
                cost: 2.0,
                messages: 3,
            },
            2,
            TokenBreakdown {
                input: 50,
                output: 20,
                cache_read: 10,
                cache_write: 10,
                reasoning: 10,
            },
            vec![
                ClientContribution {
                    client: "alpha".into(),
                    model_id: "m1".into(),
                    provider_id: "p1".into(),
                    tokens: TokenBreakdown {
                        input: 60,
                        ..TokenBreakdown::default()
                    },
                    cost: 1.2,
                    messages: 1,
                },
                ClientContribution {
                    client: "alpha".into(),
                    model_id: "m2".into(),
                    provider_id: "p2".into(),
                    tokens: TokenBreakdown {
                        output: 20,
                        ..TokenBreakdown::default()
                    },
                    cost: 0.4,
                    messages: 1,
                },
                ClientContribution {
                    client: "beta".into(),
                    model_id: "m1".into(),
                    provider_id: "p1".into(),
                    tokens: TokenBreakdown {
                        reasoning: 20,
                        ..TokenBreakdown::default()
                    },
                    cost: 0.4,
                    messages: 1,
                },
            ],
            vec![
                ProjectContribution {
                    project_key: Some("/work/alpha".into()),
                    project_label: "Alpha Old".into(),
                    totals: DailyTotals {
                        tokens: 80,
                        cost: 1.6,
                        messages: 2,
                    },
                    models: vec![
                        ProjectModelContribution {
                            model_id: "m1".into(),
                            provider_id: "p1".into(),
                            tokens: 60,
                            cost: 1.2,
                            messages: 1,
                        },
                        ProjectModelContribution {
                            model_id: "m2".into(),
                            provider_id: "p2".into(),
                            tokens: 20,
                            cost: 0.4,
                            messages: 1,
                        },
                    ],
                },
                ProjectContribution {
                    project_key: None,
                    project_label: String::new(),
                    totals: DailyTotals {
                        tokens: 20,
                        cost: 0.4,
                        messages: 1,
                    },
                    models: vec![ProjectModelContribution {
                        model_id: "m1".into(),
                        provider_id: "p1".into(),
                        tokens: 20,
                        cost: 0.4,
                        messages: 1,
                    }],
                },
            ],
        );
        let last = contract_day(
            "2026-08-04",
            DailyTotals {
                tokens: 200,
                cost: 4.0,
                messages: 4,
            },
            4,
            TokenBreakdown {
                input: 80,
                output: 50,
                cache_read: 30,
                cache_write: 20,
                reasoning: 20,
            },
            vec![
                ClientContribution {
                    client: "alpha".into(),
                    model_id: "m1".into(),
                    provider_id: "p1".into(),
                    tokens: TokenBreakdown {
                        input: 20,
                        ..TokenBreakdown::default()
                    },
                    cost: 0.4,
                    messages: 1,
                },
                ClientContribution {
                    client: "beta".into(),
                    model_id: "m2".into(),
                    provider_id: "p2".into(),
                    tokens: TokenBreakdown {
                        output: 180,
                        ..TokenBreakdown::default()
                    },
                    cost: 3.6,
                    messages: 3,
                },
            ],
            vec![
                ProjectContribution {
                    project_key: Some("/work/alpha".into()),
                    project_label: "Alpha New".into(),
                    totals: DailyTotals {
                        tokens: 50,
                        cost: 1.0,
                        messages: 1,
                    },
                    models: vec![
                        ProjectModelContribution {
                            model_id: "m1".into(),
                            provider_id: "p1".into(),
                            tokens: 20,
                            cost: 0.4,
                            messages: 1,
                        },
                        ProjectModelContribution {
                            model_id: "m2".into(),
                            provider_id: "p2".into(),
                            tokens: 30,
                            cost: 0.6,
                            messages: 0,
                        },
                    ],
                },
                ProjectContribution {
                    project_key: Some("/work/beta".into()),
                    project_label: "Beta".into(),
                    totals: DailyTotals {
                        tokens: 150,
                        cost: 3.0,
                        messages: 3,
                    },
                    models: vec![ProjectModelContribution {
                        model_id: "m2".into(),
                        provider_id: "p2".into(),
                        tokens: 150,
                        cost: 3.0,
                        messages: 3,
                    }],
                },
            ],
        );
        let chart_only = contract_day(
            "2026-07-25",
            DailyTotals {
                tokens: 25,
                cost: 0.25,
                messages: 1,
            },
            1,
            TokenBreakdown::default(),
            vec![],
            vec![],
        );
        let report = report_from_contributions(
            UsagePeriod::Days7,
            false,
            "incremental",
            37,
            true,
            2,
            3,
            &[first.clone(), last.clone()],
            &[chart_only, first, last],
            NaiveDate::from_ymd_opt(2026, 8, 4).unwrap(),
            "2026-08-04T12:34:56Z",
        );

        assert_eq!(
            serde_json::to_value(report).unwrap(),
            json!({
                "schemaVersion": 2,
                "generatedAt": "2026-08-04T12:34:56Z",
                "period": "7d",
                "dateRange": {
                    "start": "2026-08-01",
                    "end": "2026-08-04"
                },
                "scan": {
                    "mode": "incremental",
                    "forceRescan": false,
                    "durationMs": 37,
                    "cache": {
                        "sourceHits": 2,
                        "sourceMisses": 3,
                        "snapshotRebuilt": true
                    }
                },
                "summary": {
                    "totalTokens": 300,
                    "totalCost": 6.0,
                    "messages": 7,
                    "activeDays": 2,
                    "clients": ["alpha", "beta"],
                    "models": ["m1", "m2"]
                },
                "tokenBreakdown": {
                    "input": 130,
                    "output": 70,
                    "cacheRead": 40,
                    "cacheWrite": 30,
                    "reasoning": 30
                },
                "byClient": [
                    {
                        "client": "beta",
                        "tokens": 200,
                        "cost": 4.0,
                        "messages": 4,
                        "share": 0.6666666666666666,
                        "models": [
                            {
                                "modelId": "m2",
                                "providerId": "p2",
                                "tokens": 180,
                                "cost": 3.6,
                                "messages": 3,
                                "share": 0.9
                            },
                            {
                                "modelId": "m1",
                                "providerId": "p1",
                                "tokens": 20,
                                "cost": 0.4,
                                "messages": 1,
                                "share": 0.1
                            }
                        ]
                    },
                    {
                        "client": "alpha",
                        "tokens": 100,
                        "cost": 2.0,
                        "messages": 3,
                        "share": 0.3333333333333333,
                        "models": [
                            {
                                "modelId": "m1",
                                "providerId": "p1",
                                "tokens": 80,
                                "cost": 1.6,
                                "messages": 2,
                                "share": 0.8
                            },
                            {
                                "modelId": "m2",
                                "providerId": "p2",
                                "tokens": 20,
                                "cost": 0.4,
                                "messages": 1,
                                "share": 0.2
                            }
                        ]
                    }
                ],
                "byProject": [
                    {
                        "projectKey": "/work/beta",
                        "displayName": "Beta",
                        "tokens": 150,
                        "cost": 3.0,
                        "messages": 3,
                        "models": [{
                            "modelId": "m2",
                            "providerId": "p2",
                            "tokens": 150,
                            "cost": 3.0,
                            "messages": 3
                        }]
                    },
                    {
                        "projectKey": "/work/alpha",
                        "displayName": "Alpha New",
                        "tokens": 130,
                        "cost": 2.6,
                        "messages": 3,
                        "models": [
                            {
                                "modelId": "m1",
                                "providerId": "p1",
                                "tokens": 80,
                                "cost": 1.6,
                                "messages": 2
                            },
                            {
                                "modelId": "m2",
                                "providerId": "p2",
                                "tokens": 50,
                                "cost": 1.0,
                                "messages": 1
                            }
                        ]
                    },
                    {
                        "projectKey": null,
                        "displayName": "Unattributed",
                        "tokens": 20,
                        "cost": 0.4,
                        "messages": 1,
                        "models": [{
                            "modelId": "m1",
                            "providerId": "p1",
                            "tokens": 20,
                            "cost": 0.4,
                            "messages": 1
                        }]
                    }
                ],
                "byModel": [
                    {
                        "modelId": "m2",
                        "providerId": "p2",
                        "tokens": 200,
                        "cost": 4.0,
                        "messages": 4,
                        "share": 0.6666666666666666,
                        "clients": ["alpha", "beta"]
                    },
                    {
                        "modelId": "m1",
                        "providerId": "p1",
                        "tokens": 100,
                        "cost": 2.0,
                        "messages": 3,
                        "share": 0.3333333333333333,
                        "clients": ["alpha", "beta"]
                    }
                ],
                "byDay": [
                    {"date": "2026-07-22", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-07-23", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-07-24", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-07-25", "tokens": 25, "cost": 0.25, "messages": 1, "intensity": 1},
                    {"date": "2026-07-26", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-07-27", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-07-28", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-07-29", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-07-30", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-07-31", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-08-01", "tokens": 100, "cost": 2.0, "messages": 3, "intensity": 2},
                    {"date": "2026-08-02", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-08-03", "tokens": 0, "cost": 0.0, "messages": 0, "intensity": 0},
                    {"date": "2026-08-04", "tokens": 200, "cost": 4.0, "messages": 4, "intensity": 4}
                ],
                "meta": {
                    "cliVersion": "27.0.1",
                    "timezone": "UTC"
                }
            })
        );
    }

    #[test]
    fn bare_claude_cwd_stays_private_through_json_report() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts_dir = dir.path().join(".claude").join("transcripts");
        std::fs::create_dir_all(&transcripts_dir).unwrap();
        let session = transcripts_dir.join("session.jsonl");
        std::fs::write(
            &session,
            r#"{"type":"assistant","timestamp":"2026-08-04T12:00:00.000Z","cwd":"/Users/example/secret-project","requestId":"req-1","message":{"id":"msg-1","model":"claude-sonnet-4-5","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}
"#,
        )
        .unwrap();

        let messages = tokens_core::sessions::claudecode::parse_claude_file(&session);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].workspace_key, None);
        assert_eq!(messages[0].workspace_label, None);

        let contributions = tokens_core::aggregate_by_date(messages);
        assert_eq!(contributions.len(), 1);
        assert_eq!(contributions[0].projects.len(), 1);
        assert_eq!(contributions[0].projects[0].project_key, None);
        assert_eq!(contributions[0].projects[0].project_label, "Unattributed");

        let today = NaiveDate::parse_from_str(&contributions[0].date, "%Y-%m-%d").unwrap();
        let report = report_from_contributions(
            UsagePeriod::All,
            false,
            "incremental",
            0,
            true,
            0,
            0,
            &contributions,
            &contributions,
            today,
            "2026-08-04T12:00:00Z",
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["byProject"][0]["projectKey"], serde_json::Value::Null);
        assert_eq!(json["byProject"][0]["displayName"], "Unattributed");
        assert!(!json.to_string().contains("secret-project"));
    }

    #[test]
    fn report_preserves_empty_name_for_attributed_project_key() {
        let mut day = sample_day();
        day.projects[0].project_label.clear();
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let report = report_from_contributions(
            UsagePeriod::Today,
            false,
            "incremental",
            0,
            true,
            0,
            0,
            &[day.clone()],
            &[day],
            today,
            "2026-07-26T00:00:00Z",
        );

        assert_eq!(report.by_project.len(), 1);
        assert_eq!(
            report.by_project[0].project_key.as_deref(),
            Some("/work/example")
        );
        assert_eq!(report.by_project[0].display_name, "");
    }

    #[test]
    fn report_forces_unattributed_name_for_nil_project_key() {
        let mut day = sample_day();
        day.projects = vec![ProjectContribution {
            project_key: None,
            project_label: "/Users/example/secret-project".into(),
            totals: day.totals.clone(),
            models: vec![],
        }];
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let report = report_from_contributions(
            UsagePeriod::Today,
            false,
            "incremental",
            0,
            true,
            0,
            0,
            &[day.clone()],
            &[day],
            today,
            "2026-07-26T00:00:00Z",
        );

        assert_eq!(report.by_project.len(), 1);
        assert_eq!(report.by_project[0].project_key, None);
        assert_eq!(report.by_project[0].display_name, "Unattributed");
    }

    #[test]
    fn report_rolls_up_client_and_model() {
        let day = sample_day();
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let report = report_from_contributions(
            UsagePeriod::Today,
            false,
            "incremental",
            10,
            true,
            0,
            0,
            &[day.clone()],
            &[day],
            today,
            "2026-07-26T00:00:00Z",
        );
        assert_eq!(report.summary.total_tokens, 1000);
        assert_eq!(report.by_client.len(), 1);
        assert_eq!(report.by_client[0].client, "claude-code");
        assert_eq!(report.by_client[0].models.len(), 1);
        assert_eq!(report.by_model[0].model_id, "claude-opus");
        assert_eq!(report.by_model[0].clients, vec!["claude-code".to_string()]);
        assert_eq!(report.by_project.len(), 1);
        assert_eq!(report.by_project[0].display_name, "example");
        assert_eq!(report.by_project[0].models[0].model_id, "claude-opus");
        assert!((report.by_client[0].share - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn project_report_sorts_cost_and_keeps_keys_and_unattributed_distinct() {
        let mut day = sample_day();
        day.projects = vec![
            ProjectContribution {
                project_key: Some("/one/app".into()),
                project_label: "app".into(),
                totals: DailyTotals {
                    tokens: 200,
                    cost: 2.0,
                    messages: 1,
                },
                models: vec![
                    ProjectModelContribution {
                        model_id: "cheap".into(),
                        provider_id: "p".into(),
                        tokens: 150,
                        cost: 0.5,
                        messages: 1,
                    },
                    ProjectModelContribution {
                        model_id: "expensive".into(),
                        provider_id: "p".into(),
                        tokens: 50,
                        cost: 1.5,
                        messages: 1,
                    },
                ],
            },
            ProjectContribution {
                project_key: Some("/two/app".into()),
                project_label: "app".into(),
                totals: DailyTotals {
                    tokens: 300,
                    cost: 3.0,
                    messages: 1,
                },
                models: vec![],
            },
            ProjectContribution {
                project_key: None,
                project_label: "Unattributed".into(),
                totals: DailyTotals {
                    tokens: 100,
                    cost: 1.0,
                    messages: 1,
                },
                models: vec![],
            },
        ];
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let report = report_from_contributions(
            UsagePeriod::Today,
            false,
            "incremental",
            0,
            true,
            0,
            0,
            &[day.clone()],
            &[day],
            today,
            "2026-07-26T00:00:00Z",
        );
        assert_eq!(report.by_project.len(), 3);
        assert_eq!(
            report.by_project[0].project_key.as_deref(),
            Some("/two/app")
        );
        assert_eq!(
            report.by_project[1].project_key.as_deref(),
            Some("/one/app")
        );
        assert_eq!(report.by_project[1].models[0].model_id, "expensive");
        assert_eq!(report.by_project[2].project_key, None);
    }

    #[test]
    fn v3_snapshot_adapter_preserves_the_complete_period_filtered_v2_contract() {
        tokens_core::set_bucket_timezone(BucketTimezone::Named(chrono_tz::UTC));
        let dir = tempfile::tempdir().unwrap();
        let days = v2_snapshot_adapter_days();
        let mut graph = tokens_core::generate_graph_result(days.clone(), 1);
        graph.meta.generated_at = "2026-08-04T12:34:56Z".into();
        let scan = LocalUsageScan {
            graph,
            unattributed_sessions: vec![],
            hourly_facts: days
                .iter()
                .map(|day| DailyHourlyUsageFacts {
                    date: day.date.clone(),
                    hours: vec![],
                    unplaced_for_hourly: day.totals.clone(),
                })
                .collect(),
        };
        let snapshot =
            crate::commands::usage_snapshot::build_snapshot(&scan, "2026-08-04", "UTC").unwrap();
        crate::commands::usage_snapshot::write_snapshot(dir.path(), &snapshot).unwrap();

        let from_snapshot = try_report_from_snapshot_in(
            dir.path(),
            UsagePeriod::Days7,
            NaiveDate::from_ymd_opt(2026, 8, 4).unwrap(),
            Some("2026-07-29"),
            Some("2026-08-04"),
            "UTC",
        )
        .unwrap();

        assert_eq!(
            serde_json::to_value(from_snapshot).unwrap(),
            serde_json::to_value(expected_v2_snapshot_adapter_report()).unwrap()
        );
    }

    #[test]
    fn empty_v2_preset_range_uses_the_accepted_reporting_day() {
        let accepted_today = NaiveDate::from_ymd_opt(2026, 8, 5).unwrap();

        assert_eq!(
            date_range_of(&[], UsagePeriod::Today, accepted_today),
            ("2026-08-05".into(), "2026-08-05".into())
        );
    }

    #[test]
    fn v2_today_reacquires_after_reporting_timezone_midnight() {
        fn acquisition(date: &str, generated_at: &str, tokens: i64) -> UsageSnapshotAcquisition {
            let totals = DailyTotals {
                tokens,
                cost: tokens as f64 / 100.0,
                messages: 1,
            };
            let day = contract_day(
                date,
                totals.clone(),
                1,
                TokenBreakdown {
                    input: tokens,
                    ..TokenBreakdown::default()
                },
                vec![adapter_client(
                    "client",
                    "model",
                    "provider",
                    tokens,
                    totals.cost,
                    totals.messages,
                )],
                vec![adapter_project(
                    Some("/work/project"),
                    "project",
                    tokens,
                    totals.cost,
                    totals.messages,
                    vec![adapter_project_model(
                        "model",
                        "provider",
                        tokens,
                        totals.cost,
                        totals.messages,
                    )],
                )],
            );
            let mut graph = tokens_core::generate_graph_result(vec![day.clone()], 1);
            graph.meta.generated_at = generated_at.into();
            let scan = LocalUsageScan {
                graph,
                unattributed_sessions: vec![],
                hourly_facts: vec![DailyHourlyUsageFacts {
                    date: day.date.clone(),
                    hours: vec![],
                    unplaced_for_hourly: day.totals,
                }],
            };

            UsageSnapshotAcquisition {
                snapshot: usage_snapshot::build_snapshot(&scan, date, "America/Los_Angeles")
                    .unwrap(),
                mode: "incremental".into(),
                force_rescan: false,
                duration_ms: 0,
                source_hits: 0,
                source_misses: 0,
                snapshot_rebuilt: true,
            }
        }

        let parse_time = |value: &str| {
            DateTime::parse_from_rfc3339(value)
                .unwrap()
                .with_timezone(&Utc)
        };
        let mut times = std::collections::VecDeque::from([
            parse_time("2026-08-04T23:59:59-07:00"),
            parse_time("2026-08-05T00:00:01-07:00"),
            parse_time("2026-08-05T00:00:02-07:00"),
        ]);
        let mut snapshots = std::collections::VecDeque::from([
            acquisition("2026-08-04", "2026-08-05T06:59:59Z", 100),
            acquisition("2026-08-05", "2026-08-05T07:00:02Z", 200),
        ]);
        let mut calls = Vec::new();

        let report = build_current_v2_report(
            UsagePeriod::Today,
            true,
            true,
            || times.pop_front().unwrap(),
            |reporting_now, refresh, force_rescan| {
                calls.push((reporting_now, refresh, force_rescan));
                Ok(snapshots.pop_front().unwrap())
            },
        )
        .unwrap();

        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0],
            (parse_time("2026-08-04T23:59:59-07:00"), true, true)
        );
        assert_eq!(
            calls[1],
            (parse_time("2026-08-05T00:00:01-07:00"), false, false)
        );
        assert_eq!(report.generated_at, "2026-08-05T07:00:02Z");
        assert_eq!(report.date_range.start, "2026-08-05");
        assert_eq!(report.date_range.end, "2026-08-05");
        assert_eq!(report.summary.total_tokens, 200);
        assert_eq!(report.scan.force_rescan, true);
        assert_eq!(report.by_day.last().unwrap().date, "2026-08-05");
    }

    #[test]
    fn cache_miss_rechecks_after_exclusive_operation_lock_before_scanning() {
        use std::cell::Cell;

        let dir = tempfile::tempdir().unwrap();
        let reads = Cell::new(0);
        let scanned = Cell::new(false);

        let value = with_snapshot_operation(
            dir.path(),
            false,
            || {
                let next = reads.get() + 1;
                reads.set(next);
                (next == 2).then_some(7)
            },
            || {
                scanned.set(true);
                Ok(9)
            },
        )
        .unwrap();

        assert_eq!(value, 7);
        assert_eq!(reads.get(), 2);
        assert!(!scanned.get());
    }

    #[test]
    fn refresh_bypasses_snapshot_reads_under_exclusive_operation_lock() {
        let dir = tempfile::tempdir().unwrap();
        let value = with_snapshot_operation(
            dir.path(),
            true,
            || panic!("refresh must not read Layer B"),
            || Ok(11),
        )
        .unwrap();

        assert_eq!(value, 11);
    }

    #[test]
    fn force_clear_attempts_layer_a_and_every_layer_b_file_and_combines_errors() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(usage_snapshot::SNAPSHOT_FILENAME)).unwrap();
        std::fs::create_dir(dir.path().join(usage_snapshot::V2_SNAPSHOT_FILENAME)).unwrap();
        std::fs::write(
            dir.path().join(usage_snapshot::V1_SNAPSHOT_FILENAME),
            b"stale",
        )
        .unwrap();
        let source_attempted = AtomicBool::new(false);

        let error = clear_force_rescan_caches(dir.path(), || {
            source_attempted.store(true, Ordering::SeqCst);
            Err(anyhow::anyhow!("Layer A clear denied"))
        })
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(source_attempted.load(Ordering::SeqCst));
        assert!(message.contains("Layer A clear denied"));
        assert!(message.contains(usage_snapshot::SNAPSHOT_FILENAME));
        assert!(message.contains(usage_snapshot::V2_SNAPSHOT_FILENAME));
        assert!(!dir
            .path()
            .join(usage_snapshot::V1_SNAPSHOT_FILENAME)
            .exists());
    }

    #[test]
    fn live_project_contributions_match_after_snapshot_dto_round_trip() {
        fn live_message(
            date: &str,
            timestamp: i64,
            session: &str,
            model: &str,
            cost: f64,
            project_key: Option<&str>,
            project_label: Option<&str>,
        ) -> tokens_core::UnifiedMessage {
            let mut message = tokens_core::UnifiedMessage::new_with_dedup(
                "client",
                model,
                "provider",
                session,
                timestamp,
                TokenBreakdown {
                    input: 17,
                    output: 5,
                    cache_read: 2,
                    cache_write: 1,
                    reasoning: 0,
                },
                cost,
                Some(format!("{date}:{session}:{model}")),
            );
            message.date = date.to_string();
            message.message_count = 2;
            message.set_workspace(
                project_key.map(str::to_string),
                project_label.map(str::to_string),
            );
            message
        }

        let live = tokens_core::aggregate_by_date(vec![
            live_message(
                "2026-07-25",
                1_700_000_000_000,
                "one",
                "model-a",
                1.25,
                Some("/work/example"),
                Some("Old label"),
            ),
            live_message(
                "2026-07-25",
                1_700_000_001_000,
                "unattributed-one",
                "model-u",
                0.75,
                None,
                None,
            ),
            live_message(
                "2026-07-26",
                1_700_000_002_000,
                "two",
                "model-b",
                2.5,
                Some("/work/example"),
                Some("Latest label"),
            ),
            live_message(
                "2026-07-26",
                1_700_000_003_000,
                "unattributed-two",
                "model-v",
                1.125,
                None,
                None,
            ),
        ]);
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let live_report = report_from_contributions(
            UsagePeriod::All,
            false,
            "incremental",
            1,
            true,
            0,
            0,
            &live,
            &live,
            today,
            "2026-07-26T00:00:00Z",
        );

        let mut graph = tokens_core::generate_graph_result(live.clone(), 1);
        graph.meta.generated_at = "2026-07-26T00:00:00Z".into();
        let scan = LocalUsageScan {
            graph,
            unattributed_sessions: vec![],
            hourly_facts: live
                .iter()
                .map(|day| DailyHourlyUsageFacts {
                    date: day.date.clone(),
                    hours: vec![],
                    unplaced_for_hourly: day.totals.clone(),
                })
                .collect(),
        };
        let snapshot = usage_snapshot::build_snapshot(&scan, "2026-07-26", "UTC").unwrap();
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        let decoded: usage_snapshot::UsageSnapshot = serde_json::from_slice(&encoded).unwrap();
        let restored = usage_snapshot::daily_contributions(&decoded);
        let snapshot_report = report_from_contributions(
            UsagePeriod::All,
            false,
            "snapshot",
            0,
            false,
            0,
            0,
            &restored,
            &restored,
            today,
            "2026-07-26T00:00:00Z",
        );

        assert_eq!(live_report.by_project.len(), 2);
        assert_eq!(
            live_report.by_project[0].project_key.as_deref(),
            Some("/work/example")
        );
        assert_eq!(live_report.by_project[0].display_name, "Latest label");
        assert_eq!(live_report.by_project[1].project_key, None);
        assert_eq!(live_report.by_project[1].display_name, "Unattributed");

        for (live_project, snapshot_project) in live_report
            .by_project
            .iter()
            .zip(&snapshot_report.by_project)
        {
            assert_eq!(live_project.project_key, snapshot_project.project_key);
            assert_eq!(live_project.display_name, snapshot_project.display_name);
            assert_eq!(live_project.tokens, snapshot_project.tokens);
            assert_eq!(live_project.messages, snapshot_project.messages);
            assert!((live_project.cost - snapshot_project.cost).abs() <= 1e-10);
            assert_eq!(live_project.models.len(), snapshot_project.models.len());
            for (live_model, snapshot_model) in
                live_project.models.iter().zip(&snapshot_project.models)
            {
                assert_eq!(live_model.model_id, snapshot_model.model_id);
                assert_eq!(live_model.provider_id, snapshot_model.provider_id);
                assert_eq!(live_model.tokens, snapshot_model.tokens);
                assert_eq!(live_model.messages, snapshot_model.messages);
                assert!((live_model.cost - snapshot_model.cost).abs() <= 1e-10);
            }
        }
    }

    #[test]
    fn by_day_is_always_last_14_calendar_days() {
        // Period-filtered "today" summary must not strip the cost chart history.
        let history: Vec<DailyContribution> = (13..=26)
            .map(|d| {
                let mut day = sample_day();
                day.date = format!("2026-07-{d:02}");
                day.totals.cost = d as f64;
                day
            })
            .collect();
        let today_only = vec![history.last().unwrap().clone()];
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let report = report_from_contributions(
            UsagePeriod::Today,
            false,
            "snapshot",
            0,
            false,
            0,
            0,
            &today_only,
            &history,
            today,
            "2026-07-26T00:00:00Z",
        );
        assert_eq!(report.summary.total_tokens, 1000);
        assert_eq!(report.by_day.len(), 14);
        assert_eq!(report.by_day.first().unwrap().date, "2026-07-13");
        assert_eq!(report.by_day.last().unwrap().date, "2026-07-26");
        assert!((report.by_day.last().unwrap().cost - 26.0).abs() < f64::EPSILON);
        // Missing intermediate days would be zero-filled; all present here.
        assert!(report.by_day.iter().all(|d| d.cost > 0.0));
    }

    #[test]
    fn chart_by_day_zero_fills_gaps() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let sparse = vec![sample_day()]; // only 2026-07-26
        let days = chart_by_day(&sparse, today, 14);
        assert_eq!(days.len(), 14);
        assert_eq!(days.first().unwrap().date, "2026-07-13");
        assert_eq!(days.last().unwrap().date, "2026-07-26");
        assert_eq!(days.first().unwrap().cost, 0.0);
        assert!((days.last().unwrap().cost - 1.5).abs() < f64::EPSILON);
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
                projects: vec![],
                active_time_ms: None,
            },
            sample_day(),
        ];
        filter_contributions(&mut days, Some("2026-07-26"), Some("2026-07-26"));
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].date, "2026-07-26");
    }
}
