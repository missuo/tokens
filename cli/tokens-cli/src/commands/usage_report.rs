//! Local usage report for Menu Bar / machine consumers.
//!
//! `tokens usage --json` scans session files via tokens-core (Layer A source
//! cache), rebuilds a Layer B usage snapshot under the tokens cache dir, and
//! emits a stable JSON schema for the macOS Menu Bar app.

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use fs2::FileExt;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tokens_core::{
    bucket_timezone, clear_source_message_cache, generate_local_usage_scan, BucketTimezone,
    ClientContribution, DailyContribution, DailyTotals, GraphResult, LocalUsageScan,
    ProjectContribution, ProjectModelContribution, ReportOptions, TokenBreakdown,
};

use crate::commands::unattributed_diagnostics::{update_diagnostics, DIAGNOSTIC_FILENAME};
use crate::settings;

const SNAPSHOT_SCHEMA_VERSION: u32 = 2;
const SNAPSHOT_FILENAME: &str = "usage-snapshot-v2.json";
const LEGACY_SNAPSHOT_FILENAME: &str = "usage-snapshot-v1.json";
static SNAPSHOT_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageSnapshotFile {
    schema_version: u32,
    generated_at: String,
    /// Calendar day in the bucket timezone when this snapshot was written
    /// (`YYYY-MM-DD`). Used for same-day reuse — never derive this from the
    /// UTC prefix of `generated_at` (that breaks after local midnight when
    /// UTC has already rolled over).
    #[serde(default)]
    bucket_date: String,
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
    projects: Vec<SnapshotProject>,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotProject {
    project_key: Option<String>,
    project_label: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    models: Vec<SnapshotProjectModel>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotProjectModel {
    model_id: String,
    provider_id: String,
    tokens: i64,
    cost: f64,
    messages: i32,
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
                let payload = UsageErrorReport {
                    schema_version: SNAPSHOT_SCHEMA_VERSION,
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
        let _ = fs::remove_file(legacy_snapshot_path());
    } else if !refresh {
        if let Some(report) =
            try_report_from_snapshot(period, today, since.as_deref(), until.as_deref())
        {
            return Ok(report);
        }
    }

    let scan = scan_all_local()?;
    write_snapshot_from_graph(&scan.graph)?;
    let diagnostics_path = tokens_core::paths::get_cache_dir().join(DIAGNOSTIC_FILENAME);
    if let Err(error) = update_diagnostics(
        &diagnostics_path,
        &scan.graph.meta.generated_at,
        &timezone_label(),
        &scan.unattributed_sessions,
    ) {
        eprintln!("tokens: warning: {error:#}");
    }
    let graph = scan.graph;

    // Keep full history for the always-14-day cost chart; filter only for
    // summary / client / model rolls (period-scoped).
    let chart_source = graph.contributions.clone();
    let mut contributions = graph.contributions;
    filter_contributions(&mut contributions, since.as_deref(), until.as_deref());

    let duration_ms = started.elapsed().as_millis() as u32;
    let mode = if force_rescan { "full" } else { "incremental" };

    Ok(report_from_contributions(
        period,
        force_rescan,
        mode,
        duration_ms,
        true,
        0,
        0,
        &contributions,
        &chart_source,
        today,
        &graph.meta.generated_at,
    ))
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

fn filter_contributions(
    days: &mut Vec<DailyContribution>,
    since: Option<&str>,
    until: Option<&str>,
) {
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

fn legacy_snapshot_path() -> PathBuf {
    tokens_core::paths::get_cache_dir().join(LEGACY_SNAPSHOT_FILENAME)
}

fn timezone_label() -> String {
    match bucket_timezone() {
        BucketTimezone::Local => iana_time_zone::get_timezone().unwrap_or_else(|_| "local".into()),
        BucketTimezone::Named(tz) => tz.name().to_string(),
    }
}

/// Resolve the bucket-local calendar day this snapshot belongs to.
fn snapshot_bucket_day(snapshot: &UsageSnapshotFile) -> Option<String> {
    if !snapshot.bucket_date.is_empty() {
        return Some(snapshot.bucket_date.clone());
    }
    // Legacy snapshots (no bucket_date): convert generated_at from absolute
    // time into the process bucket timezone instead of slicing the UTC date.
    bucket_day_from_generated_at(&snapshot.generated_at)
}

fn bucket_day_from_generated_at(generated_at: &str) -> Option<String> {
    // Accept RFC3339 with or without fractional seconds.
    let dt = chrono::DateTime::parse_from_rfc3339(generated_at)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|| {
            chrono::DateTime::parse_from_str(generated_at, "%Y-%m-%dT%H:%M:%S%.f%z")
                .ok()
                .map(|d| d.with_timezone(&chrono::Utc))
        })?;
    let day = match bucket_timezone() {
        BucketTimezone::Local => dt.with_timezone(&chrono::Local).date_naive(),
        BucketTimezone::Named(tz) => dt.with_timezone(&tz).date_naive(),
    };
    Some(day.format("%Y-%m-%d").to_string())
}

fn snapshot_day_from_contribution(c: &DailyContribution) -> SnapshotDay {
    SnapshotDay {
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
        projects: c
            .projects
            .iter()
            .map(|project| SnapshotProject {
                project_key: project.project_key.clone(),
                project_label: project.project_label.clone(),
                tokens: project.totals.tokens,
                cost: project.totals.cost,
                messages: project.totals.messages,
                models: project
                    .models
                    .iter()
                    .map(|model| SnapshotProjectModel {
                        model_id: model.model_id.clone(),
                        provider_id: model.provider_id.clone(),
                        tokens: model.tokens,
                        cost: model.cost,
                        messages: model.messages,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn write_snapshot_from_graph(graph: &GraphResult) -> Result<()> {
    let snapshot = UsageSnapshotFile {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        generated_at: graph.meta.generated_at.clone(),
        bucket_date: bucket_timezone().today().format("%Y-%m-%d").to_string(),
        timezone: timezone_label(),
        contributions: graph
            .contributions
            .iter()
            .map(snapshot_day_from_contribution)
            .collect(),
    };

    let path = snapshot_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(&snapshot)?;
    write_private_snapshot(&path, &body)?;
    let _ = fs::remove_file(legacy_snapshot_path());
    Ok(())
}

/// Persist a snapshot through an exclusive lock + process-unique temporary file.
/// Concurrent refreshes queue on the lock; each published snapshot is complete
/// and workspace keys remain owner-only on Unix.
fn write_private_snapshot(path: &Path, body: &[u8]) -> Result<()> {
    let lock_path = snapshot_lock_path(path);
    let lock = open_private_snapshot_lock(&lock_path)?;
    lock.lock_exclusive()
        .with_context(|| format!("lock usage snapshot {}", lock_path.display()))?;

    let sequence = SNAPSHOT_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.{}.{}.tmp", std::process::id(), sequence));
    let result = (|| -> Result<()> {
        #[cfg(unix)]
        let mut output = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&tmp)?
        };
        #[cfg(not(unix))]
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)?;

        output.write_all(body)?;
        output.sync_all()?;
        tokens_core::fs_atomic::replace_file(&tmp, path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.with_context(|| format!("write usage snapshot {}", path.display()))
}

fn snapshot_lock_path(path: &Path) -> PathBuf {
    path.with_extension("json.lock")
}

fn open_private_snapshot_lock(path: &Path) -> Result<fs::File> {
    #[cfg(unix)]
    let lock = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)
    };
    #[cfg(not(unix))]
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path);

    let lock = lock.with_context(|| format!("open usage snapshot lock {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(lock)
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
    let today_s = today.format("%Y-%m-%d").to_string();
    let snapshot_day = snapshot_bucket_day(&snapshot)?;
    if snapshot_day != today_s {
        return None;
    }
    if snapshot.timezone != timezone_label() {
        return None;
    }

    // Full snapshot history drives the 14-day cost chart even when the selected
    // period is `today` (which would otherwise leave byDay empty/almost empty
    // right after midnight).
    let chart_source = snapshot_days_to_contributions(&snapshot.contributions);
    let mut days = snapshot.contributions;
    if let Some(since) = since {
        days.retain(|d| d.date.as_str() >= since);
    }
    if let Some(until) = until {
        days.retain(|d| d.date.as_str() <= until);
    }

    let contributions = snapshot_days_to_contributions(&days);
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

fn snapshot_days_to_contributions(days: &[SnapshotDay]) -> Vec<DailyContribution> {
    days.iter()
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
            projects: d
                .projects
                .iter()
                .map(|project| ProjectContribution {
                    project_key: project.project_key.clone(),
                    project_label: project.project_label.clone(),
                    totals: DailyTotals {
                        tokens: project.tokens,
                        cost: project.cost,
                        messages: project.messages,
                    },
                    models: project
                        .models
                        .iter()
                        .map(|model| ProjectModelContribution {
                            model_id: model.model_id.clone(),
                            provider_id: model.provider_id.clone(),
                            tokens: model.tokens,
                            cost: model.cost,
                            messages: model.messages,
                        })
                        .collect(),
                })
                .collect(),
            active_time_ms: None,
        })
        .collect()
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
            ProjectUsage {
                project_key,
                display_name: if project.display_name.is_empty() {
                    "Unattributed".to_string()
                } else {
                    project.display_name
                },
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
        schema_version: SNAPSHOT_SCHEMA_VERSION,
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

        let snapshot_days: Vec<SnapshotDay> =
            live.iter().map(snapshot_day_from_contribution).collect();
        let encoded = serde_json::to_vec(&snapshot_days).unwrap();
        let decoded: Vec<SnapshotDay> = serde_json::from_slice(&encoded).unwrap();
        let restored = snapshot_days_to_contributions(&decoded);
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
    fn snapshot_project_round_trip_restores_daily_projects() {
        let project = SnapshotProject {
            project_key: Some("/work/example".into()),
            project_label: "example".into(),
            tokens: 10,
            cost: 2.0,
            messages: 1,
            models: vec![SnapshotProjectModel {
                model_id: "model".into(),
                provider_id: "provider".into(),
                tokens: 10,
                cost: 2.0,
                messages: 1,
            }],
        };
        let day = SnapshotDay {
            date: "2026-07-26".into(),
            tokens: 10,
            cost: 2.0,
            messages: 1,
            intensity: 4,
            token_breakdown: TokenBreakdownDtoSerde::default(),
            clients: vec![],
            projects: vec![project],
        };
        let encoded = serde_json::to_vec(&day).unwrap();
        let decoded: SnapshotDay = serde_json::from_slice(&encoded).unwrap();
        let contributions = snapshot_days_to_contributions(&[decoded]);
        assert_eq!(contributions[0].projects[0].project_label, "example");
        assert_eq!(contributions[0].projects[0].models[0].cost, 2.0);
    }

    #[test]
    fn concurrent_snapshot_writers_publish_valid_private_files() {
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join(SNAPSHOT_FILENAME));
        let body = br#"{"schemaVersion":2,"projects":["/private/workspace"]}"#.to_vec();
        let writers: Vec<_> = (0..20)
            .map(|_| {
                let path = Arc::clone(&path);
                let body = body.clone();
                std::thread::spawn(move || write_private_snapshot(&path, &body))
            })
            .collect();

        for writer in writers {
            writer.join().unwrap().unwrap();
        }
        let published = fs::read(&*path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&published).unwrap();
        assert_eq!(value["schemaVersion"], SNAPSHOT_SCHEMA_VERSION);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&*path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(snapshot_lock_path(&path))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
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

    #[test]
    fn bucket_day_from_utc_generated_at_uses_local_bucket() {
        // 2026-07-27 03:33 UTC is still 2026-07-26 evening in America/Los_Angeles.
        tokens_core::set_bucket_timezone(
            tokens_core::parse_bucket_timezone("America/Los_Angeles").unwrap(),
        );
        let day = bucket_day_from_generated_at("2026-07-27T03:33:08.153036+00:00");
        assert_eq!(day.as_deref(), Some("2026-07-26"));
    }

    #[test]
    fn snapshot_bucket_date_field_wins() {
        let snap = UsageSnapshotFile {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            generated_at: "2026-07-27T03:33:08.153036+00:00".into(),
            bucket_date: "2026-07-26".into(),
            timezone: "America/Los_Angeles".into(),
            contributions: vec![],
        };
        assert_eq!(snapshot_bucket_day(&snap).as_deref(), Some("2026-07-26"));
    }
}
