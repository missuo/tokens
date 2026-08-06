//! External v3 usage reporting built from validated range-independent snapshots.
//!
//! The deep builder is pure: callers provide snapshot facts, selection,
//! reporting-now, and scan metadata. Range planning, rollups, and time-series
//! assembly therefore stay deterministic and independent of cache orchestration.

use anyhow::{bail, Context, Result};
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, Local, LocalResult, NaiveDate, NaiveDateTime,
    Offset, TimeZone, Timelike, Utc,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use tokens_core::BucketTimezone;

use super::cost_checks::{checked_cost_sum, cost_matches};
use super::usage_report::{self, UsagePeriod, UsageReportSelection};
use super::usage_snapshot::{
    self, UsageSnapshot, UsageSnapshotDay, UsageSnapshotTokenBreakdown, UsageSnapshotTotals,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BucketGranularity {
    Hour,
    Day,
    NaturalWeek,
    NaturalMonth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageBucketMetadata {
    pub id: String,
    pub nominal_start: String,
    pub nominal_end_exclusive: String,
    pub covered_start: String,
    pub covered_end_exclusive: String,
    pub context_only: bool,
    pub incomplete_edge: bool,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageRangePlan {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub granularity: BucketGranularity,
    pub selection_start: String,
    pub buckets: Vec<UsageBucketMetadata>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportTotals {
    pub(crate) tokens: i64,
    pub(crate) cost: f64,
    pub(crate) messages: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum UsageReportSelectionDto {
    Preset {
        preset: String,
    },
    Custom {
        #[serde(rename = "startDate")]
        start_date: String,
        #[serde(rename = "endDate")]
        end_date: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportDateRange {
    start_date: String,
    end_date: String,
    timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportCacheInfo {
    pub(crate) source_hits: u64,
    pub(crate) source_misses: u64,
    pub(crate) snapshot_rebuilt: bool,
    pub(crate) snapshot_schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportScanInfo {
    pub(crate) mode: String,
    pub(crate) force_rescan: bool,
    pub(crate) duration_ms: u32,
    pub(crate) cache: UsageReportCacheInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportSummary {
    pub(crate) total_tokens: i64,
    pub(crate) total_cost: f64,
    pub(crate) messages: i32,
    pub(crate) active_days: i32,
    pub(crate) clients: Vec<String>,
    pub(crate) models: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportTokenBreakdown {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportModelTotal {
    model_id: String,
    provider_id: String,
    tokens: i64,
    cost: f64,
    messages: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportClientRow {
    client: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    share: f64,
    models: Vec<UsageReportModelTotal>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportProjectRow {
    project_key: Option<String>,
    display_name: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    models: Vec<UsageReportModelTotal>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportModelRow {
    model_id: String,
    provider_id: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    share: f64,
    clients: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportTimeBucket {
    id: String,
    nominal_start: String,
    nominal_end_exclusive: String,
    covered_start: String,
    covered_end_exclusive: String,
    pub(crate) totals: UsageReportTotals,
    pub(crate) context_only: bool,
    pub(crate) incomplete_edge: bool,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportTimeSeries {
    pub(crate) granularity: BucketGranularity,
    pub(crate) selection_start: String,
    pub(crate) buckets: Vec<UsageReportTimeBucket>,
    pub(crate) unplaced: UsageReportTotals,
}

/// One cell of the weekday × hour heatmap: usage placed at this ISO weekday
/// and reporting-timezone hour of day across the selected range.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportWeekdayHourCell {
    /// ISO-8601 weekday in the reporting timezone: 1 = Monday … 7 = Sunday.
    weekday: u8,
    /// Reporting-timezone hour of day: 0…23.
    hour: u8,
    tokens: i64,
    cost: f64,
    messages: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportMeta {
    cli_version: String,
    timezone: String,
    report_contract: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageReportV3 {
    schema_version: u32,
    generated_at: String,
    selection: UsageReportSelectionDto,
    date_range: UsageReportDateRange,
    scan: UsageReportScanInfo,
    pub(crate) summary: UsageReportSummary,
    token_breakdown: UsageReportTokenBreakdown,
    by_client: Vec<UsageReportClientRow>,
    by_project: Vec<UsageReportProjectRow>,
    by_model: Vec<UsageReportModelRow>,
    pub(crate) time_series: UsageReportTimeSeries,
    /// Full 7 × 24 weekday × hour grid over the selected range, zero-filled so
    /// the Menu Bar Advanced heatmap can render empty cells explicitly.
    /// Unplaced usage (no reliable hour) is excluded by construction.
    weekday_hour: Vec<UsageReportWeekdayHourCell>,
    meta: UsageReportMeta,
}

#[derive(Debug, Clone, Default)]
struct RollupTotals {
    tokens: i64,
    cost: f64,
    messages: i32,
}

#[derive(Debug, Clone, Default)]
struct ProjectRollup {
    totals: RollupTotals,
    display_name: String,
}

const REPORT_SCHEMA_VERSION: u32 = 3;
const REPORT_CONTRACT: &str = "v3";
const MIN_TODAY_HOUR_BUCKETS: usize = 12;

pub(crate) fn run(
    json: bool,
    selection: UsageReportSelection,
    refresh: bool,
    force_rescan: bool,
) -> Result<()> {
    let report = build_current_usage_report(
        &selection,
        refresh,
        force_rescan,
        Utc::now,
        usage_report::acquire_usage_snapshot,
    )?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }
    Ok(())
}

fn build_current_usage_report<Now, Acquire>(
    selection: &UsageReportSelection,
    refresh: bool,
    force_rescan: bool,
    mut now: Now,
    mut acquire: Acquire,
) -> Result<UsageReportV3>
where
    Now: FnMut() -> DateTime<Utc>,
    Acquire: FnMut(DateTime<Utc>, bool, bool) -> Result<usage_report::UsageSnapshotAcquisition>,
{
    let acquisition_started_at = now();
    let mut acquisition = acquire(acquisition_started_at, refresh, force_rescan)?;

    for attempt in 0..2 {
        let reporting_now = now();
        let timezone = parse_snapshot_timezone(&acquisition.snapshot.timezone)?;
        let reporting_date = date_in_timezone(timezone, reporting_now)
            .format("%Y-%m-%d")
            .to_string();
        if reporting_date == acquisition.snapshot.bucket_date {
            let scan_info = UsageReportScanInfo {
                mode: acquisition.mode,
                force_rescan,
                duration_ms: acquisition.duration_ms,
                cache: UsageReportCacheInfo {
                    source_hits: acquisition.source_hits,
                    source_misses: acquisition.source_misses,
                    snapshot_rebuilt: acquisition.snapshot_rebuilt,
                    snapshot_schema_version: acquisition.snapshot.schema_version,
                },
            };
            return build_usage_report(&acquisition.snapshot, selection, reporting_now, scan_info);
        }
        if attempt == 1 {
            break;
        }
        acquisition = acquire(reporting_now, false, false)?;
    }

    bail!("reporting day changed repeatedly while acquiring the usage snapshot")
}

fn print_human(report: &UsageReportV3) {
    println!("Tokens usage (v3)");
    println!(
        "  tokens: {}  cost: ${:.2}  messages: {}",
        report.summary.total_tokens, report.summary.total_cost, report.summary.messages
    );
    println!(
        "  range: {} → {}  mode: {}  ({} ms)",
        report.date_range.start_date,
        report.date_range.end_date,
        report.scan.mode,
        report.scan.duration_ms
    );
}

pub(crate) fn build_usage_report(
    snapshot: &UsageSnapshot,
    selection: &UsageReportSelection,
    reporting_now: DateTime<Utc>,
    scan_info: UsageReportScanInfo,
) -> Result<UsageReportV3> {
    usage_snapshot::validate_snapshot(snapshot)?;
    let snapshot_generated_at = DateTime::parse_from_rfc3339(&snapshot.generated_at)
        .context("invalid snapshot generatedAt")?
        .with_timezone(&Utc);
    if snapshot_generated_at > reporting_now {
        bail!("snapshot generatedAt must not be after reporting now");
    }
    let timezone = parse_snapshot_timezone(&snapshot.timezone)?;
    let earliest_known_date = snapshot
        .days
        .first()
        .map(|day| parse_snapshot_date(&day.date))
        .transpose()?;
    let plan = plan_usage_range(selection, earliest_known_date, timezone, reporting_now)?;
    let selected_days = selected_snapshot_days(snapshot, plan.start_date, plan.end_date)?;
    let rollups = build_rollups(&selected_days)?;
    let time_series = build_time_series(snapshot, &plan, timezone)?;
    let weekday_hour = build_weekday_hour_cells(&selected_days, timezone)?;
    require_summary_conservation(&rollups.summary, &time_series)?;
    require_weekday_hour_conservation(&rollups.summary, &selected_days, &weekday_hour)?;

    Ok(UsageReportV3 {
        schema_version: REPORT_SCHEMA_VERSION,
        generated_at: snapshot.generated_at.clone(),
        selection: selection_dto(selection),
        date_range: UsageReportDateRange {
            start_date: plan.start_date.format("%Y-%m-%d").to_string(),
            end_date: plan.end_date.format("%Y-%m-%d").to_string(),
            timezone: snapshot.timezone.clone(),
        },
        scan: scan_info,
        summary: rollups.summary,
        token_breakdown: rollups.token_breakdown,
        by_client: rollups.by_client,
        by_project: rollups.by_project,
        by_model: rollups.by_model,
        time_series,
        weekday_hour,
        meta: UsageReportMeta {
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            timezone: snapshot.timezone.clone(),
            report_contract: REPORT_CONTRACT.to_string(),
        },
    })
}

struct ReportRollups {
    summary: UsageReportSummary,
    token_breakdown: UsageReportTokenBreakdown,
    by_client: Vec<UsageReportClientRow>,
    by_project: Vec<UsageReportProjectRow>,
    by_model: Vec<UsageReportModelRow>,
}

fn build_rollups(days: &[&UsageSnapshotDay]) -> Result<ReportRollups> {
    let mut summary_totals = RollupTotals::default();
    let mut token_breakdown = UsageReportTokenBreakdown::default();
    let mut active_days = 0i32;
    let mut by_client: BTreeMap<String, RollupTotals> = BTreeMap::new();
    let mut by_client_model: BTreeMap<(String, String, String), RollupTotals> = BTreeMap::new();
    let mut by_project: BTreeMap<Option<String>, ProjectRollup> = BTreeMap::new();
    let mut by_project_model: BTreeMap<(Option<String>, String, String), RollupTotals> =
        BTreeMap::new();
    let mut by_model: BTreeMap<(String, String), RollupTotals> = BTreeMap::new();
    let mut model_clients: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();

    for day in days {
        add_snapshot_totals(&mut summary_totals, &day.totals, "summary")?;
        add_token_breakdown(&mut token_breakdown, &day.token_breakdown)?;
        if day.totals.tokens > 0 || day.totals.cost > 0.0 || day.totals.messages > 0 {
            active_days = active_days
                .checked_add(1)
                .context("active day count overflow")?;
        }

        for client in &day.clients {
            let tokens = token_breakdown_total(&client.token_breakdown)?;
            let totals = RollupTotals {
                tokens,
                cost: client.cost,
                messages: client.messages,
            };
            add_rollup_totals(
                by_client.entry(client.client.clone()).or_default(),
                &totals,
                "client rollup",
            )?;
            add_rollup_totals(
                by_client_model
                    .entry((
                        client.client.clone(),
                        client.model_id.clone(),
                        client.provider_id.clone(),
                    ))
                    .or_default(),
                &totals,
                "client model rollup",
            )?;
            let model_key = (client.model_id.clone(), client.provider_id.clone());
            add_rollup_totals(
                by_model.entry(model_key.clone()).or_default(),
                &totals,
                "model rollup",
            )?;
            model_clients
                .entry(model_key)
                .or_default()
                .insert(client.client.clone());
        }

        for project in &day.projects {
            let project_rollup = by_project.entry(project.project_key.clone()).or_default();
            add_snapshot_totals(
                &mut project_rollup.totals,
                &project.totals,
                "project rollup",
            )?;
            if !project.display_name.trim().is_empty() {
                project_rollup.display_name = project.display_name.clone();
            }
            for model in &project.models {
                add_snapshot_totals(
                    by_project_model
                        .entry((
                            project.project_key.clone(),
                            model.model_id.clone(),
                            model.provider_id.clone(),
                        ))
                        .or_default(),
                    &model.totals,
                    "project model rollup",
                )?;
            }
        }
    }

    let summary_clients = by_client.keys().cloned().collect();
    let summary_models = by_model
        .keys()
        .map(|(model_id, _)| model_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let token_denominator = (summary_totals.tokens > 0).then_some(summary_totals.tokens as f64);

    let mut models_by_client: BTreeMap<String, Vec<UsageReportModelTotal>> = BTreeMap::new();
    for ((client, model_id, provider_id), model) in by_client_model {
        models_by_client
            .entry(client)
            .or_default()
            .push(UsageReportModelTotal {
                model_id,
                provider_id,
                tokens: model.tokens,
                cost: model.cost,
                messages: model.messages,
            });
    }
    let mut client_rows = Vec::with_capacity(by_client.len());
    for (client, totals) in by_client {
        let mut client_models = models_by_client.remove(&client).unwrap_or_default();
        sort_model_totals_by_tokens(&mut client_models);
        client_rows.push(UsageReportClientRow {
            client,
            tokens: totals.tokens,
            cost: totals.cost,
            messages: totals.messages,
            share: token_denominator
                .map(|denominator| totals.tokens as f64 / denominator)
                .unwrap_or(0.0),
            models: client_models,
        });
    }
    client_rows.sort_by(|left, right| {
        right
            .tokens
            .cmp(&left.tokens)
            .then_with(|| left.client.cmp(&right.client))
    });

    let mut models_by_project: BTreeMap<Option<String>, Vec<UsageReportModelTotal>> =
        BTreeMap::new();
    for ((project_key, model_id, provider_id), totals) in by_project_model {
        models_by_project
            .entry(project_key)
            .or_default()
            .push(UsageReportModelTotal {
                model_id,
                provider_id,
                tokens: totals.tokens,
                cost: totals.cost,
                messages: totals.messages,
            });
    }
    let mut project_rows = by_project
        .into_iter()
        .map(|(project_key, project)| {
            let mut project_models = models_by_project.remove(&project_key).unwrap_or_default();
            project_models.sort_by(|left, right| {
                right
                    .cost
                    .total_cmp(&left.cost)
                    .then_with(|| right.tokens.cmp(&left.tokens))
                    .then_with(|| left.model_id.cmp(&right.model_id))
                    .then_with(|| left.provider_id.cmp(&right.provider_id))
            });
            UsageReportProjectRow {
                project_key,
                display_name: if project.display_name.is_empty() {
                    "Unattributed".to_string()
                } else {
                    project.display_name
                },
                tokens: project.totals.tokens,
                cost: project.totals.cost,
                messages: project.totals.messages,
                models: project_models,
            }
        })
        .collect::<Vec<_>>();
    project_rows.sort_by(|left, right| {
        right
            .cost
            .total_cmp(&left.cost)
            .then_with(|| right.tokens.cmp(&left.tokens))
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.project_key.cmp(&right.project_key))
    });

    let mut model_rows = by_model
        .into_iter()
        .map(|(key, totals)| UsageReportModelRow {
            model_id: key.0.clone(),
            provider_id: key.1.clone(),
            tokens: totals.tokens,
            cost: totals.cost,
            messages: totals.messages,
            share: token_denominator
                .map(|denominator| totals.tokens as f64 / denominator)
                .unwrap_or(0.0),
            clients: model_clients
                .remove(&key)
                .unwrap_or_default()
                .into_iter()
                .collect(),
        })
        .collect::<Vec<_>>();
    model_rows.sort_by(|left, right| {
        right
            .tokens
            .cmp(&left.tokens)
            .then_with(|| left.model_id.cmp(&right.model_id))
            .then_with(|| left.provider_id.cmp(&right.provider_id))
    });

    Ok(ReportRollups {
        summary: UsageReportSummary {
            total_tokens: summary_totals.tokens,
            total_cost: summary_totals.cost,
            messages: summary_totals.messages,
            active_days,
            clients: summary_clients,
            models: summary_models,
        },
        token_breakdown,
        by_client: client_rows,
        by_project: project_rows,
        by_model: model_rows,
    })
}

fn build_time_series(
    snapshot: &UsageSnapshot,
    plan: &UsageRangePlan,
    timezone: BucketTimezone,
) -> Result<UsageReportTimeSeries> {
    let bucket_totals = match plan.granularity {
        BucketGranularity::Hour => hour_bucket_totals(snapshot, plan)?,
        BucketGranularity::Day
        | BucketGranularity::NaturalWeek
        | BucketGranularity::NaturalMonth => calendar_bucket_totals(snapshot, plan, timezone)?,
    };
    let buckets = plan
        .buckets
        .iter()
        .zip(bucket_totals)
        .map(|(metadata, totals)| UsageReportTimeBucket {
            id: metadata.id.clone(),
            nominal_start: metadata.nominal_start.clone(),
            nominal_end_exclusive: metadata.nominal_end_exclusive.clone(),
            covered_start: metadata.covered_start.clone(),
            covered_end_exclusive: metadata.covered_end_exclusive.clone(),
            totals,
            context_only: metadata.context_only,
            incomplete_edge: metadata.incomplete_edge,
            active: metadata.active,
        })
        .collect();

    let selected_date = plan.start_date.format("%Y-%m-%d").to_string();
    let unplaced = if plan.granularity == BucketGranularity::Hour {
        snapshot
            .days
            .iter()
            .find(|day| day.date == selected_date)
            .map(|day| report_totals(&day.unplaced_for_hourly))
            .unwrap_or_default()
    } else {
        UsageReportTotals::default()
    };

    Ok(UsageReportTimeSeries {
        granularity: plan.granularity,
        selection_start: plan.selection_start.clone(),
        buckets,
        unplaced,
    })
}

/// Aggregate the selected days' exact hourly facts into the full 7 × 24
/// (ISO weekday × reporting-timezone hour) grid. Cells are zero-filled so
/// empty weekday/hour combinations are explicit, matching the chart contract.
fn build_weekday_hour_cells(
    days: &[&UsageSnapshotDay],
    timezone: BucketTimezone,
) -> Result<Vec<UsageReportWeekdayHourCell>> {
    let mut totals = vec![RollupTotals::default(); 7 * 24];
    for day in days {
        for hour in &day.hours {
            let (weekday, hour_of_day) = weekday_hour_of_ms(timezone, hour.start_ms)?;
            let index = (usize::from(weekday) - 1) * 24 + usize::from(hour_of_day);
            add_snapshot_totals(&mut totals[index], &hour.totals, "weekday-hour cell")?;
        }
    }

    let mut cells = Vec::with_capacity(7 * 24);
    for weekday in 1u8..=7 {
        for hour in 0u8..24 {
            let cell = &totals[(usize::from(weekday) - 1) * 24 + usize::from(hour)];
            cells.push(UsageReportWeekdayHourCell {
                weekday,
                hour,
                tokens: cell.tokens,
                cost: cell.cost,
                messages: cell.messages,
            });
        }
    }
    Ok(cells)
}

/// Reporting-timezone (ISO weekday, hour of day) for an absolute instant.
fn weekday_hour_of_ms(timezone: BucketTimezone, timestamp_ms: i64) -> Result<(u8, u8)> {
    let local = match timezone {
        BucketTimezone::Local => chrono::Local
            .timestamp_millis_opt(timestamp_ms)
            .single()
            .map(|instant| instant.naive_local()),
        BucketTimezone::Named(tz) => tz
            .timestamp_millis_opt(timestamp_ms)
            .single()
            .map(|instant| instant.naive_local()),
    }
    .with_context(|| format!("invalid weekday-hour instant {timestamp_ms}"))?;
    let weekday =
        u8::try_from(local.weekday().number_from_monday()).context("ISO weekday out of range")?;
    let hour = u8::try_from(local.hour()).context("local hour out of range")?;
    Ok((weekday, hour))
}

/// The heatmap covers exactly the placed hourly usage of the selected range:
/// cells plus unplaced-for-hourly must conserve the selected-range summary.
fn require_weekday_hour_conservation(
    summary: &UsageReportSummary,
    days: &[&UsageSnapshotDay],
    cells: &[UsageReportWeekdayHourCell],
) -> Result<()> {
    let mut totals = RollupTotals::default();
    for cell in cells {
        add_rollup_totals(
            &mut totals,
            &RollupTotals {
                tokens: cell.tokens,
                cost: cell.cost,
                messages: cell.messages,
            },
            "weekday-hour conservation",
        )?;
    }
    for day in days {
        add_snapshot_totals(
            &mut totals,
            &day.unplaced_for_hourly,
            "weekday-hour unplaced conservation",
        )?;
    }
    if totals.tokens != summary.total_tokens
        || totals.messages != summary.messages
        || !cost_matches(totals.cost, summary.total_cost)
    {
        bail!("weekday-hour cells plus unplaced usage do not conserve the selected-range summary");
    }
    Ok(())
}

fn hour_bucket_totals(
    snapshot: &UsageSnapshot,
    plan: &UsageRangePlan,
) -> Result<Vec<UsageReportTotals>> {
    let hours: BTreeMap<(i64, i64), &UsageSnapshotTotals> = snapshot
        .days
        .iter()
        .flat_map(|day| &day.hours)
        .map(|hour| ((hour.start_ms, hour.end_ms), &hour.totals))
        .collect();
    plan.buckets
        .iter()
        .map(|metadata| {
            let bounds = (
                parse_report_instant(&metadata.nominal_start)?.timestamp_millis(),
                parse_report_instant(&metadata.nominal_end_exclusive)?.timestamp_millis(),
            );
            Ok(hours
                .get(&bounds)
                .map(|totals| report_totals(totals))
                .unwrap_or_default())
        })
        .collect()
}

fn calendar_bucket_totals(
    snapshot: &UsageSnapshot,
    plan: &UsageRangePlan,
    timezone: BucketTimezone,
) -> Result<Vec<UsageReportTotals>> {
    let days = snapshot
        .days
        .iter()
        .map(|day| {
            let date = parse_snapshot_date(&day.date)?;
            Ok((
                local_midnight(timezone, date)?.with_timezone(&Utc),
                &day.totals,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut day_index = 0;
    let mut buckets = Vec::with_capacity(plan.buckets.len());
    for metadata in &plan.buckets {
        let covered_start = parse_report_instant(&metadata.covered_start)?;
        let covered_end = parse_report_instant(&metadata.covered_end_exclusive)?;
        while day_index < days.len() && days[day_index].0 < covered_start {
            day_index += 1;
        }
        let mut totals = RollupTotals::default();
        while day_index < days.len() && days[day_index].0 < covered_end {
            add_snapshot_totals(&mut totals, days[day_index].1, "time-series bucket")?;
            day_index += 1;
        }
        buckets.push(UsageReportTotals {
            tokens: totals.tokens,
            cost: totals.cost,
            messages: totals.messages,
        });
    }
    Ok(buckets)
}

fn selected_snapshot_days(
    snapshot: &UsageSnapshot,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<&UsageSnapshotDay>> {
    snapshot
        .days
        .iter()
        .filter_map(|day| match parse_snapshot_date(&day.date) {
            Ok(date) if date >= start && date <= end => Some(Ok(day)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn selection_dto(selection: &UsageReportSelection) -> UsageReportSelectionDto {
    match selection {
        UsageReportSelection::Preset { period } => UsageReportSelectionDto::Preset {
            preset: period.as_str().to_string(),
        },
        UsageReportSelection::Custom { since, until } => UsageReportSelectionDto::Custom {
            start_date: since.format("%Y-%m-%d").to_string(),
            end_date: until.format("%Y-%m-%d").to_string(),
        },
    }
}

fn parse_snapshot_timezone(value: &str) -> Result<BucketTimezone> {
    if value == "local" {
        Ok(BucketTimezone::Local)
    } else {
        tokens_core::parse_bucket_timezone(value)
            .with_context(|| format!("invalid snapshot timezone {value}"))
    }
}

fn parse_snapshot_date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("invalid snapshot date {value}"))
}

fn parse_report_instant(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid report bucket instant {value}"))?
        .with_timezone(&Utc))
}

fn report_totals(totals: &UsageSnapshotTotals) -> UsageReportTotals {
    UsageReportTotals {
        tokens: totals.tokens,
        cost: totals.cost,
        messages: totals.messages,
    }
}

fn token_breakdown_total(tokens: &UsageSnapshotTokenBreakdown) -> Result<i64> {
    [
        tokens.input,
        tokens.output,
        tokens.cache_read,
        tokens.cache_write,
        tokens.reasoning,
    ]
    .into_iter()
    .try_fold(0i64, |total, value| {
        total.checked_add(value).context("token rollup overflow")
    })
}

fn add_token_breakdown(
    target: &mut UsageReportTokenBreakdown,
    source: &UsageSnapshotTokenBreakdown,
) -> Result<()> {
    target.input = target
        .input
        .checked_add(source.input)
        .context("input token rollup overflow")?;
    target.output = target
        .output
        .checked_add(source.output)
        .context("output token rollup overflow")?;
    target.cache_read = target
        .cache_read
        .checked_add(source.cache_read)
        .context("cache-read token rollup overflow")?;
    target.cache_write = target
        .cache_write
        .checked_add(source.cache_write)
        .context("cache-write token rollup overflow")?;
    target.reasoning = target
        .reasoning
        .checked_add(source.reasoning)
        .context("reasoning token rollup overflow")?;
    Ok(())
}

fn add_snapshot_totals(
    target: &mut RollupTotals,
    source: &UsageSnapshotTotals,
    label: &str,
) -> Result<()> {
    add_rollup_totals(
        target,
        &RollupTotals {
            tokens: source.tokens,
            cost: source.cost,
            messages: source.messages,
        },
        label,
    )
}

fn add_rollup_totals(target: &mut RollupTotals, source: &RollupTotals, label: &str) -> Result<()> {
    target.tokens = target
        .tokens
        .checked_add(source.tokens)
        .with_context(|| format!("{label} token overflow"))?;
    target.messages = target
        .messages
        .checked_add(source.messages)
        .with_context(|| format!("{label} message overflow"))?;
    target.cost = checked_cost_sum(target.cost, source.cost, label)?;
    Ok(())
}

fn sort_model_totals_by_tokens(rows: &mut [UsageReportModelTotal]) {
    rows.sort_by(|left, right| {
        right
            .tokens
            .cmp(&left.tokens)
            .then_with(|| left.model_id.cmp(&right.model_id))
            .then_with(|| left.provider_id.cmp(&right.provider_id))
    });
}

fn require_summary_conservation(
    summary: &UsageReportSummary,
    time_series: &UsageReportTimeSeries,
) -> Result<()> {
    let mut totals = RollupTotals::default();
    for bucket in time_series
        .buckets
        .iter()
        .filter(|bucket| !bucket.context_only)
    {
        add_rollup_totals(
            &mut totals,
            &RollupTotals {
                tokens: bucket.totals.tokens,
                cost: bucket.totals.cost,
                messages: bucket.totals.messages,
            },
            "time-series conservation",
        )?;
    }
    add_rollup_totals(
        &mut totals,
        &RollupTotals {
            tokens: time_series.unplaced.tokens,
            cost: time_series.unplaced.cost,
            messages: time_series.unplaced.messages,
        },
        "time-series unplaced conservation",
    )?;
    if totals.tokens != summary.total_tokens
        || totals.messages != summary.messages
        || !cost_matches(totals.cost, summary.total_cost)
    {
        bail!("summary does not equal selected time-series buckets plus unplaced usage");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InclusiveDateRange {
    start: NaiveDate,
    end: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CalendarBucketDates {
    nominal_start: NaiveDate,
    nominal_end_exclusive: NaiveDate,
    covered_start: NaiveDate,
    covered_end_exclusive: NaiveDate,
}

impl InclusiveDateRange {
    fn inclusive_days(self) -> i64 {
        self.end.signed_duration_since(self.start).num_days() + 1
    }
}

pub(crate) fn plan_usage_range(
    selection: &UsageReportSelection,
    earliest_known_date: Option<NaiveDate>,
    timezone: BucketTimezone,
    reporting_now: DateTime<Utc>,
) -> Result<UsageRangePlan> {
    let reporting_today = date_in_timezone(timezone, reporting_now);
    let range = resolve_range(selection, earliest_known_date, reporting_today)?;
    let granularity = choose_granularity(range);
    // Context fill keys on the resolved range (single day == reporting today),
    // not the selection enum, so Custom[today,today] matches Preset Today.
    let include_today_context =
        range.start == reporting_today && range.end == reporting_today;
    let buckets = match granularity {
        BucketGranularity::Hour => hourly_buckets(
            range,
            timezone,
            reporting_now,
            reporting_today,
            include_today_context,
        )?,
        BucketGranularity::Day => day_buckets(range, timezone, reporting_now, reporting_today)?,
        BucketGranularity::NaturalWeek => {
            week_buckets(range, timezone, reporting_now, reporting_today)?
        }
        BucketGranularity::NaturalMonth => {
            month_buckets(range, timezone, reporting_now, reporting_today)?
        }
    };
    let selection_start = buckets
        .iter()
        .find(|bucket| !bucket.context_only)
        .map(|bucket| bucket.covered_start.clone())
        .context("planned range did not emit a selected bucket")?;

    Ok(UsageRangePlan {
        start_date: range.start,
        end_date: range.end,
        granularity,
        selection_start,
        buckets,
    })
}

fn resolve_range(
    selection: &UsageReportSelection,
    earliest_known_date: Option<NaiveDate>,
    reporting_today: NaiveDate,
) -> Result<InclusiveDateRange> {
    let range = match selection {
        UsageReportSelection::Preset { period } => match period {
            UsagePeriod::Today => InclusiveDateRange {
                start: reporting_today,
                end: reporting_today,
            },
            UsagePeriod::Days7 => InclusiveDateRange {
                start: reporting_today - Duration::days(6),
                end: reporting_today,
            },
            UsagePeriod::Days30 => InclusiveDateRange {
                start: reporting_today - Duration::days(29),
                end: reporting_today,
            },
            UsagePeriod::All => InclusiveDateRange {
                start: earliest_known_date.unwrap_or(reporting_today),
                end: reporting_today,
            },
        },
        UsageReportSelection::Custom { since, until } => InclusiveDateRange {
            start: *since,
            end: *until,
        },
    };

    if range.start > reporting_today || range.end > reporting_today {
        bail!("future dates are unavailable; latest allowed date is {reporting_today}");
    }
    if range.start > range.end {
        bail!("start date must not be after end date");
    }
    Ok(range)
}

fn choose_granularity(range: InclusiveDateRange) -> BucketGranularity {
    match range.inclusive_days() {
        1 => BucketGranularity::Hour,
        2..=14 => BucketGranularity::Day,
        15..=90 => BucketGranularity::NaturalWeek,
        _ => BucketGranularity::NaturalMonth,
    }
}

fn hourly_buckets(
    range: InclusiveDateRange,
    timezone: BucketTimezone,
    reporting_now: DateTime<Utc>,
    reporting_today: NaiveDate,
    include_today_context: bool,
) -> Result<Vec<UsageBucketMetadata>> {
    let day_start = local_midnight(timezone, range.start)?.with_timezone(&Utc);
    let next_date = range
        .start
        .succ_opt()
        .context("hourly range end is outside the supported calendar")?;
    let day_end = local_midnight(timezone, next_date)?.with_timezone(&Utc);
    let ends_today = range.end == reporting_today;

    let mut selected_starts = Vec::new();
    let mut cursor = day_start;
    while cursor < day_end && (!ends_today || cursor <= reporting_now) {
        selected_starts.push(cursor);
        cursor += Duration::hours(1);
    }
    let context_hours = if include_today_context {
        MIN_TODAY_HOUR_BUCKETS.saturating_sub(selected_starts.len())
    } else {
        0
    };
    let context_starts = (0..context_hours)
        .map(|index| day_start - Duration::hours((context_hours.saturating_sub(index)) as i64));

    Ok(context_starts
        .chain(selected_starts)
        .map(|start| {
            let full_hour_end = start + Duration::hours(1);
            let end = if start >= day_start {
                full_hour_end.min(day_end)
            } else {
                full_hour_end
            };
            let context_only = start < day_start;
            let active =
                ends_today && !context_only && start <= reporting_now && reporting_now < end;
            let nominal_start = instant_in_timezone(timezone, start).to_rfc3339();
            let nominal_end_exclusive = instant_in_timezone(timezone, end).to_rfc3339();
            let covered_end_exclusive = if active {
                instant_in_timezone(timezone, reporting_now).to_rfc3339()
            } else {
                nominal_end_exclusive.clone()
            };
            UsageBucketMetadata {
                id: nominal_start.clone(),
                nominal_start: nominal_start.clone(),
                nominal_end_exclusive,
                covered_start: nominal_start,
                covered_end_exclusive,
                context_only,
                incomplete_edge: false,
                active,
            }
        })
        .collect())
}

fn day_buckets(
    range: InclusiveDateRange,
    timezone: BucketTimezone,
    reporting_now: DateTime<Utc>,
    reporting_today: NaiveDate,
) -> Result<Vec<UsageBucketMetadata>> {
    let mut buckets = Vec::with_capacity(range.inclusive_days() as usize);
    let mut date = range.start;
    while date <= range.end {
        let next = date
            .succ_opt()
            .context("daily bucket end is outside the supported calendar")?;
        buckets.push(calendar_bucket(
            timezone,
            reporting_now,
            reporting_today,
            range,
            CalendarBucketDates {
                nominal_start: date,
                nominal_end_exclusive: next,
                covered_start: date,
                covered_end_exclusive: next,
            },
        )?);
        date = next;
    }
    Ok(buckets)
}

fn week_buckets(
    range: InclusiveDateRange,
    timezone: BucketTimezone,
    reporting_now: DateTime<Utc>,
    reporting_today: NaiveDate,
) -> Result<Vec<UsageBucketMetadata>> {
    let selection_end_exclusive = range
        .end
        .succ_opt()
        .context("weekly selection end is outside the supported calendar")?;
    let mut nominal_start =
        range.start - Duration::days(range.start.weekday().num_days_from_monday() as i64);
    let mut buckets = Vec::with_capacity((range.inclusive_days() as usize / 7) + 2);
    while nominal_start <= range.end {
        let nominal_end_exclusive = nominal_start + Duration::days(7);
        let covered_start = nominal_start.max(range.start);
        let covered_end_exclusive = nominal_end_exclusive.min(selection_end_exclusive);
        buckets.push(calendar_bucket(
            timezone,
            reporting_now,
            reporting_today,
            range,
            CalendarBucketDates {
                nominal_start,
                nominal_end_exclusive,
                covered_start,
                covered_end_exclusive,
            },
        )?);
        nominal_start = nominal_end_exclusive;
    }
    Ok(buckets)
}

fn month_buckets(
    range: InclusiveDateRange,
    timezone: BucketTimezone,
    reporting_now: DateTime<Utc>,
    reporting_today: NaiveDate,
) -> Result<Vec<UsageBucketMetadata>> {
    let selection_end_exclusive = range
        .end
        .succ_opt()
        .context("monthly selection end is outside the supported calendar")?;
    let mut nominal_start = range
        .start
        .with_day(1)
        .context("monthly bucket start is outside the supported calendar")?;
    let mut buckets = Vec::with_capacity((range.inclusive_days() as usize / 28) + 2);
    while nominal_start <= range.end {
        let nominal_end_exclusive = next_month(nominal_start)?;
        let covered_start = nominal_start.max(range.start);
        let covered_end_exclusive = nominal_end_exclusive.min(selection_end_exclusive);
        buckets.push(calendar_bucket(
            timezone,
            reporting_now,
            reporting_today,
            range,
            CalendarBucketDates {
                nominal_start,
                nominal_end_exclusive,
                covered_start,
                covered_end_exclusive,
            },
        )?);
        nominal_start = nominal_end_exclusive;
    }
    Ok(buckets)
}

fn calendar_bucket(
    timezone: BucketTimezone,
    reporting_now: DateTime<Utc>,
    reporting_today: NaiveDate,
    range: InclusiveDateRange,
    dates: CalendarBucketDates,
) -> Result<UsageBucketMetadata> {
    let nominal_start = local_midnight(timezone, dates.nominal_start)?;
    let nominal_end_exclusive = local_midnight(timezone, dates.nominal_end_exclusive)?;
    let covered_start = if dates.covered_start == dates.nominal_start {
        nominal_start
    } else {
        local_midnight(timezone, dates.covered_start)?
    };
    let covered_end_civil = if dates.covered_end_exclusive == dates.nominal_end_exclusive {
        nominal_end_exclusive
    } else {
        local_midnight(timezone, dates.covered_end_exclusive)?
    };
    let covered_start_utc = covered_start.with_timezone(&Utc);
    let covered_end_utc = covered_end_civil.with_timezone(&Utc);
    let active = range.end == reporting_today
        && covered_start_utc <= reporting_now
        && reporting_now < covered_end_utc;
    let covered_end_exclusive = if active {
        instant_in_timezone(timezone, reporting_now)
    } else {
        covered_end_civil
    };

    let nominal_start = nominal_start.to_rfc3339();
    Ok(UsageBucketMetadata {
        id: nominal_start.clone(),
        nominal_start,
        nominal_end_exclusive: nominal_end_exclusive.to_rfc3339(),
        covered_start: covered_start.to_rfc3339(),
        covered_end_exclusive: covered_end_exclusive.to_rfc3339(),
        context_only: false,
        incomplete_edge: dates.covered_start != dates.nominal_start
            || dates.covered_end_exclusive != dates.nominal_end_exclusive,
        active,
    })
}

fn next_month(date: NaiveDate) -> Result<NaiveDate> {
    let (year, month) = if date.month() == 12 {
        (
            date.year().checked_add(1).context("month year overflow")?,
            1,
        )
    } else {
        (date.year(), date.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1).context("month end is outside the supported calendar")
}

fn date_in_timezone(timezone: BucketTimezone, instant: DateTime<Utc>) -> NaiveDate {
    match timezone {
        BucketTimezone::Local => instant.with_timezone(&Local).date_naive(),
        BucketTimezone::Named(tz) => instant.with_timezone(&tz).date_naive(),
    }
}

fn instant_in_timezone(timezone: BucketTimezone, instant: DateTime<Utc>) -> DateTime<FixedOffset> {
    match timezone {
        BucketTimezone::Local => {
            let local = instant.with_timezone(&Local);
            local.with_timezone(&local.offset().fix())
        }
        BucketTimezone::Named(tz) => {
            let local = instant.with_timezone(&tz);
            local.with_timezone(&local.offset().fix())
        }
    }
}

fn local_midnight(timezone: BucketTimezone, date: NaiveDate) -> Result<DateTime<FixedOffset>> {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .context("civil midnight is outside the supported calendar")?;
    match timezone {
        BucketTimezone::Local => resolve_local_datetime(&Local, midnight),
        BucketTimezone::Named(tz) => resolve_local_datetime(&tz, midnight),
    }
    .with_context(|| format!("cannot resolve reporting boundary for {date}"))
}

fn resolve_local_datetime<Tz>(timezone: &Tz, value: NaiveDateTime) -> Result<DateTime<FixedOffset>>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    for minute in 0..(24 * 60) {
        let candidate = value + Duration::minutes(minute);
        if candidate.date() != value.date() {
            break;
        }
        match timezone.from_local_datetime(&candidate) {
            LocalResult::Single(datetime) => {
                return Ok(datetime.with_timezone(&datetime.offset().fix()));
            }
            LocalResult::Ambiguous(first, second) => {
                let chosen = if first.timestamp() <= second.timestamp() {
                    first
                } else {
                    second
                };
                return Ok(chosen.with_timezone(&chosen.offset().fix()));
            }
            LocalResult::None => {}
        }
    }
    bail!("civil date has no representable local instant")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::usage_report::{UsagePeriod, UsageReportSelection};
    use chrono::{DateTime, Duration, NaiveDate, Utc};
    use serde_json::json;
    use tokens_core::BucketTimezone;

    use crate::commands::usage_snapshot::{
        UsageSnapshot, UsageSnapshotClient, UsageSnapshotHour, UsageSnapshotProject,
        UsageSnapshotProjectModel, UsageSnapshotTokenBreakdown, UsageSnapshotTotals,
        SNAPSHOT_SCHEMA_VERSION,
    };

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn now(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn timezone(value: &str) -> BucketTimezone {
        BucketTimezone::Named(value.parse().unwrap())
    }

    fn preset(period: UsagePeriod) -> UsageReportSelection {
        UsageReportSelection::Preset { period }
    }

    fn custom(since: &str, until: &str) -> UsageReportSelection {
        UsageReportSelection::Custom {
            since: date(since),
            until: date(until),
        }
    }

    #[test]
    fn resolves_all_selection_kinds_to_inclusive_reporting_dates() {
        let tz = timezone("America/Los_Angeles");
        let reporting_now = now("2026-08-04T17:30:00-07:00");

        for (selection, earliest, expected_start, expected_end) in [
            (preset(UsagePeriod::Today), None, "2026-08-04", "2026-08-04"),
            (preset(UsagePeriod::Days7), None, "2026-07-29", "2026-08-04"),
            (
                preset(UsagePeriod::Days30),
                None,
                "2026-07-06",
                "2026-08-04",
            ),
            (
                preset(UsagePeriod::All),
                Some(date("2025-09-12")),
                "2025-09-12",
                "2026-08-04",
            ),
            (preset(UsagePeriod::All), None, "2026-08-04", "2026-08-04"),
            (
                custom("2026-07-01", "2026-07-05"),
                None,
                "2026-07-01",
                "2026-07-05",
            ),
        ] {
            let plan = plan_usage_range(&selection, earliest, tz, reporting_now).unwrap();
            assert_eq!(plan.start_date, date(expected_start));
            assert_eq!(plan.end_date, date(expected_end));
        }
    }

    #[test]
    fn rejects_inverted_and_future_ranges_defensively() {
        let tz = timezone("UTC");
        let reporting_now = now("2026-08-04T17:30:00Z");

        let inverted =
            plan_usage_range(&custom("2026-08-04", "2026-08-03"), None, tz, reporting_now)
                .unwrap_err();
        assert!(inverted
            .to_string()
            .contains("start date must not be after end date"));

        let future = plan_usage_range(&custom("2026-08-04", "2026-08-05"), None, tz, reporting_now)
            .unwrap_err();
        assert!(future.to_string().contains("future dates are unavailable"));

        let future_all = plan_usage_range(
            &preset(UsagePeriod::All),
            Some(date("2026-08-05")),
            tz,
            reporting_now,
        )
        .unwrap_err();
        assert!(future_all
            .to_string()
            .contains("future dates are unavailable"));
    }

    #[test]
    fn chooses_granularity_at_every_exact_threshold() {
        let tz = timezone("UTC");
        let reporting_now = now("2027-01-01T12:00:00Z");
        let start = date("2026-01-01");

        for (days, expected) in [
            (1, BucketGranularity::Hour),
            (2, BucketGranularity::Day),
            (14, BucketGranularity::Day),
            (15, BucketGranularity::NaturalWeek),
            (90, BucketGranularity::NaturalWeek),
            (91, BucketGranularity::NaturalMonth),
        ] {
            let end = start + Duration::days(days - 1);
            let selection = UsageReportSelection::Custom {
                since: start,
                until: end,
            };
            let plan = plan_usage_range(&selection, None, tz, reporting_now).unwrap();
            assert_eq!(plan.granularity, expected, "{days} inclusive days");
        }
    }

    #[test]
    fn day_plan_zero_fills_the_selection_and_stops_at_reporting_now() {
        let plan = plan_usage_range(
            &preset(UsagePeriod::Days7),
            None,
            timezone("America/Los_Angeles"),
            now("2026-08-04T17:30:00-07:00"),
        )
        .unwrap();

        assert_eq!(plan.buckets.len(), 7);
        assert_eq!(plan.buckets[0].id, "2026-07-29T00:00:00-07:00");
        assert_eq!(plan.buckets[0].covered_start, "2026-07-29T00:00:00-07:00");
        assert_eq!(
            plan.buckets[0].covered_end_exclusive,
            "2026-07-30T00:00:00-07:00"
        );
        assert_eq!(
            plan.buckets[6].nominal_end_exclusive,
            "2026-08-05T00:00:00-07:00"
        );
        assert_eq!(
            plan.buckets[6].covered_end_exclusive,
            "2026-08-04T17:30:00-07:00"
        );
        assert_eq!(
            plan.buckets.iter().filter(|bucket| bucket.active).count(),
            1
        );
        assert!(plan.buckets[6].active);
        assert!(!plan.buckets[6].incomplete_edge);
        assert!(plan.buckets.iter().all(|bucket| !bucket.context_only));
    }

    #[test]
    fn natural_weeks_keep_monday_identity_and_clip_selection_edges() {
        let plan = plan_usage_range(
            &custom("2026-08-04", "2026-08-18"),
            None,
            timezone("UTC"),
            now("2026-08-20T12:00:00Z"),
        )
        .unwrap();

        assert_eq!(plan.granularity, BucketGranularity::NaturalWeek);
        assert_eq!(plan.buckets.len(), 3);
        assert_eq!(plan.buckets[0].id, "2026-08-03T00:00:00+00:00");
        assert_eq!(plan.buckets[0].nominal_start, "2026-08-03T00:00:00+00:00");
        assert_eq!(
            plan.buckets[0].nominal_end_exclusive,
            "2026-08-10T00:00:00+00:00"
        );
        assert_eq!(plan.buckets[0].covered_start, "2026-08-04T00:00:00+00:00");
        assert_eq!(
            plan.buckets[0].covered_end_exclusive,
            "2026-08-10T00:00:00+00:00"
        );
        assert!(plan.buckets[0].incomplete_edge);
        assert!(!plan.buckets[1].incomplete_edge);
        assert_eq!(plan.buckets[2].nominal_start, "2026-08-17T00:00:00+00:00");
        assert_eq!(
            plan.buckets[2].nominal_end_exclusive,
            "2026-08-24T00:00:00+00:00"
        );
        assert_eq!(
            plan.buckets[2].covered_end_exclusive,
            "2026-08-19T00:00:00+00:00"
        );
        assert!(plan.buckets[2].incomplete_edge);
        assert!(plan.buckets.iter().all(|bucket| !bucket.active));
    }

    #[test]
    fn natural_months_keep_calendar_identity_and_clip_selection_edges() {
        let plan = plan_usage_range(
            &custom("2026-01-15", "2026-05-05"),
            None,
            timezone("America/Los_Angeles"),
            now("2026-06-01T12:00:00-07:00"),
        )
        .unwrap();

        assert_eq!(plan.granularity, BucketGranularity::NaturalMonth);
        assert_eq!(plan.buckets.len(), 5);
        assert_eq!(plan.buckets[0].id, "2026-01-01T00:00:00-08:00");
        assert_eq!(
            plan.buckets[0].nominal_end_exclusive,
            "2026-02-01T00:00:00-08:00"
        );
        assert_eq!(plan.buckets[0].covered_start, "2026-01-15T00:00:00-08:00");
        assert!(plan.buckets[0].incomplete_edge);
        assert_eq!(plan.buckets[4].id, "2026-05-01T00:00:00-07:00");
        assert_eq!(
            plan.buckets[4].nominal_end_exclusive,
            "2026-06-01T00:00:00-07:00"
        );
        assert_eq!(
            plan.buckets[4].covered_end_exclusive,
            "2026-05-06T00:00:00-07:00"
        );
        assert!(plan.buckets[4].incomplete_edge);
    }

    #[test]
    fn today_at_0130_emits_exactly_twelve_hours_with_ten_context_buckets() {
        let plan = plan_usage_range(
            &preset(UsagePeriod::Today),
            None,
            timezone("America/Los_Angeles"),
            now("2026-08-04T01:30:00-07:00"),
        )
        .unwrap();

        assert_eq!(plan.selection_start, "2026-08-04T00:00:00-07:00");
        assert_eq!(plan.buckets.len(), 12);
        assert_eq!(
            plan.buckets
                .iter()
                .filter(|bucket| bucket.context_only)
                .count(),
            10
        );
        assert_eq!(plan.buckets[0].id, "2026-08-03T14:00:00-07:00");
        assert_eq!(plan.buckets[10].id, "2026-08-04T00:00:00-07:00");
        let active: Vec<_> = plan.buckets.iter().filter(|bucket| bucket.active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "2026-08-04T01:00:00-07:00");
        assert_eq!(active[0].covered_end_exclusive, "2026-08-04T01:30:00-07:00");
        assert!(plan.buckets.iter().all(|bucket| {
            bucket.covered_start < bucket.covered_end_exclusive || bucket.active
        }));
    }

    #[test]
    fn today_at_or_after_eleven_needs_no_context() {
        for (reporting_now, expected_count) in [
            ("2026-08-04T11:00:00-07:00", 12),
            ("2026-08-04T23:30:00-07:00", 24),
        ] {
            let plan = plan_usage_range(
                &preset(UsagePeriod::Today),
                None,
                timezone("America/Los_Angeles"),
                now(reporting_now),
            )
            .unwrap();
            assert_eq!(plan.buckets.len(), expected_count);
            assert!(plan.buckets.iter().all(|bucket| !bucket.context_only));
            assert_eq!(
                plan.buckets.iter().filter(|bucket| bucket.active).count(),
                1
            );
        }
    }

    #[test]
    fn custom_single_day_equal_to_reporting_today_gets_prior_day_context() {
        let plan = plan_usage_range(
            &custom("2026-08-04", "2026-08-04"),
            None,
            timezone("America/Los_Angeles"),
            now("2026-08-04T01:30:00-07:00"),
        )
        .unwrap();

        assert_eq!(plan.selection_start, "2026-08-04T00:00:00-07:00");
        assert_eq!(plan.buckets.len(), 12);
        assert_eq!(
            plan.buckets
                .iter()
                .filter(|bucket| bucket.context_only)
                .count(),
            10
        );
        assert_eq!(plan.buckets[0].id, "2026-08-03T14:00:00-07:00");
        assert_eq!(plan.buckets[10].id, "2026-08-04T00:00:00-07:00");
        let active: Vec<_> = plan.buckets.iter().filter(|bucket| bucket.active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "2026-08-04T01:00:00-07:00");
        assert_eq!(active[0].covered_end_exclusive, "2026-08-04T01:30:00-07:00");
    }

    #[test]
    fn custom_ranges_other_than_reporting_today_get_no_context() {
        // Historical single day: full day of hours, no context fill.
        let historical = plan_usage_range(
            &custom("2026-03-08", "2026-03-08"),
            None,
            timezone("America/Los_Angeles"),
            now("2026-08-04T08:00:00-07:00"),
        )
        .unwrap();
        assert!(historical
            .buckets
            .iter()
            .all(|bucket| !bucket.context_only));

        // Multi-day range ending today uses day granularity, not hourly context.
        let multi_day = plan_usage_range(
            &custom("2026-08-01", "2026-08-04"),
            None,
            timezone("America/Los_Angeles"),
            now("2026-08-04T08:00:00-07:00"),
        )
        .unwrap();
        assert_eq!(multi_day.granularity, BucketGranularity::Day);
        assert!(multi_day
            .buckets
            .iter()
            .all(|bucket| !bucket.context_only));
    }

    #[test]
    fn los_angeles_spring_day_enumerates_twenty_three_real_hours() {
        let plan = plan_usage_range(
            &custom("2026-03-08", "2026-03-08"),
            None,
            timezone("America/Los_Angeles"),
            now("2026-03-10T12:00:00-07:00"),
        )
        .unwrap();

        assert_eq!(plan.buckets.len(), 23);
        assert_eq!(
            plan.buckets.first().unwrap().id,
            "2026-03-08T00:00:00-08:00"
        );
        assert_eq!(plan.buckets.last().unwrap().id, "2026-03-08T23:00:00-07:00");
        assert!(!plan
            .buckets
            .iter()
            .any(|bucket| bucket.id.starts_with("2026-03-08T02:")));
        assert_eq!(
            plan.buckets.last().unwrap().nominal_end_exclusive,
            "2026-03-09T00:00:00-07:00"
        );
    }

    #[test]
    fn los_angeles_fall_day_enumerates_distinct_repeated_hours() {
        let plan = plan_usage_range(
            &custom("2026-11-01", "2026-11-01"),
            None,
            timezone("America/Los_Angeles"),
            now("2026-11-03T12:00:00-08:00"),
        )
        .unwrap();

        assert_eq!(plan.buckets.len(), 25);
        let repeated: Vec<_> = plan
            .buckets
            .iter()
            .filter(|bucket| bucket.id.starts_with("2026-11-01T01:"))
            .collect();
        assert_eq!(repeated.len(), 2);
        assert_eq!(repeated[0].id, "2026-11-01T01:00:00-07:00");
        assert_eq!(repeated[1].id, "2026-11-01T01:00:00-08:00");
        assert_ne!(repeated[0].id, repeated[1].id);
        assert_eq!(
            plan.buckets.last().unwrap().nominal_end_exclusive,
            "2026-11-02T00:00:00-08:00"
        );
    }

    #[test]
    fn lord_howe_fractional_dst_days_cover_every_instant_through_next_midnight() {
        for (day, reporting_now, expected_count, first_midnight, final_start, next_midnight) in [
            (
                "2026-10-04",
                "2026-10-06T12:00:00+11:00",
                24,
                "2026-10-04T00:00:00+10:30",
                "2026-10-04T23:30:00+11:00",
                "2026-10-05T00:00:00+11:00",
            ),
            (
                "2026-04-05",
                "2026-04-07T12:00:00+10:30",
                25,
                "2026-04-05T00:00:00+11:00",
                "2026-04-05T23:30:00+10:30",
                "2026-04-06T00:00:00+10:30",
            ),
        ] {
            let plan = plan_usage_range(
                &custom(day, day),
                None,
                timezone("Australia/Lord_Howe"),
                now(reporting_now),
            )
            .unwrap();

            assert_eq!(plan.buckets.len(), expected_count, "{day}");
            assert_eq!(plan.buckets.first().unwrap().nominal_start, first_midnight);
            for pair in plan.buckets.windows(2) {
                assert_eq!(
                    pair[0].nominal_end_exclusive, pair[1].nominal_start,
                    "gap on {day}"
                );
            }
            let final_bucket = plan.buckets.last().unwrap();
            assert_eq!(final_bucket.nominal_start, final_start);
            assert_eq!(final_bucket.nominal_end_exclusive, next_midnight);
            let start = DateTime::parse_from_rfc3339(&final_bucket.nominal_start).unwrap();
            let end = DateTime::parse_from_rfc3339(&final_bucket.nominal_end_exclusive).unwrap();
            assert_eq!(end.signed_duration_since(start), Duration::minutes(30));
        }
    }

    #[test]
    fn lord_howe_final_fractional_interval_is_the_only_active_bucket() {
        for (day, reporting_now, expected_count, final_start) in [
            (
                "2026-10-04",
                "2026-10-04T23:45:00+11:00",
                24,
                "2026-10-04T23:30:00+11:00",
            ),
            (
                "2026-04-05",
                "2026-04-05T23:45:00+10:30",
                25,
                "2026-04-05T23:30:00+10:30",
            ),
        ] {
            let plan = plan_usage_range(
                &preset(UsagePeriod::Today),
                None,
                timezone("Australia/Lord_Howe"),
                now(reporting_now),
            )
            .unwrap();

            assert_eq!(plan.start_date, date(day));
            assert_eq!(plan.buckets.len(), expected_count);
            let active: Vec<_> = plan.buckets.iter().filter(|bucket| bucket.active).collect();
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].nominal_start, final_start);
            assert_eq!(active[0].covered_end_exclusive, reporting_now);
        }
    }

    fn approved_snapshot() -> UsageSnapshot {
        serde_json::from_str(include_str!(
            "../../../../docs/wayfinder/time-range-cost-chart/prototypes/report-cache-contract/fixtures/snapshot-v3-sample.json"
        ))
        .unwrap()
    }

    fn approved_custom_snapshot() -> UsageSnapshot {
        // One placed hour per day, matching the prototype fixture grid
        // (weekday 1..5 at hours 9..13) so weekdayHour conserves the summary.
        const HOUR_START_MS: [i64; 5] = [
            1_780_329_600_000, // 2026-06-01 09:00 PDT
            1_780_419_600_000, // 2026-06-02 10:00 PDT
            1_780_509_600_000, // 2026-06-03 11:00 PDT
            1_780_599_600_000, // 2026-06-04 12:00 PDT
            1_780_689_600_000, // 2026-06-05 13:00 PDT
        ];
        let days = (1..=5)
            .map(|day| {
                let index = (day - 1) as usize;
                let tokens = 100_000 + i64::from(day - 1) * 10_000;
                let totals = UsageSnapshotTotals {
                    tokens,
                    cost: 1.25 + f64::from(day - 1) * 0.25,
                    messages: 40 + day - 1,
                };
                let token_breakdown = UsageSnapshotTokenBreakdown {
                    input: tokens * 35 / 100,
                    output: tokens * 25 / 100,
                    cache_read: tokens * 25 / 100,
                    cache_write: tokens * 5 / 100,
                    reasoning: tokens * 10 / 100,
                };
                let start_ms = HOUR_START_MS[index];
                UsageSnapshotDay {
                    date: format!("2026-06-{day:02}"),
                    totals: totals.clone(),
                    token_breakdown: token_breakdown.clone(),
                    clients: vec![UsageSnapshotClient {
                        client: "claude-code".into(),
                        model_id: "claude-sonnet-5".into(),
                        provider_id: "anthropic".into(),
                        token_breakdown,
                        cost: totals.cost,
                        messages: totals.messages,
                    }],
                    projects: vec![UsageSnapshotProject {
                        project_key: Some("/workspace/tokens".into()),
                        display_name: "tokens".into(),
                        totals: totals.clone(),
                        models: vec![UsageSnapshotProjectModel {
                            model_id: "claude-sonnet-5".into(),
                            provider_id: "anthropic".into(),
                            totals: totals.clone(),
                        }],
                    }],
                    hours: vec![UsageSnapshotHour {
                        start_ms,
                        end_ms: start_ms + 3_600_000,
                        totals: totals.clone(),
                    }],
                    unplaced_for_hourly: UsageSnapshotTotals::default(),
                }
            })
            .collect();
        UsageSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            generated_at: "2026-08-05T00:30:00Z".into(),
            bucket_date: "2026-08-04".into(),
            timezone: "America/Los_Angeles".into(),
            days,
        }
    }

    fn snapshot_scan_info() -> UsageReportScanInfo {
        UsageReportScanInfo {
            mode: "snapshot".into(),
            force_rescan: false,
            duration_ms: 12,
            cache: UsageReportCacheInfo {
                source_hits: 0,
                source_misses: 0,
                snapshot_rebuilt: false,
                snapshot_schema_version: SNAPSHOT_SCHEMA_VERSION,
            },
        }
    }

    #[test]
    fn report_time_is_sampled_after_snapshot_acquisition_completes() {
        let mut times = std::collections::VecDeque::from([
            now("2026-08-04T01:00:00-07:00"),
            now("2026-08-04T01:30:00-07:00"),
        ]);
        let report = build_current_usage_report(
            &preset(UsagePeriod::Today),
            false,
            false,
            || times.pop_front().unwrap(),
            |_, _, _| {
                Ok(usage_report::UsageSnapshotAcquisition {
                    snapshot: approved_snapshot(),
                    mode: "snapshot".into(),
                    force_rescan: false,
                    duration_ms: 12,
                    source_hits: 0,
                    source_misses: 0,
                    snapshot_rebuilt: false,
                })
            },
        )
        .unwrap();

        assert_eq!(report.generated_at, "2026-08-04T08:30:00Z");
        let active = report
            .time_series
            .buckets
            .iter()
            .find(|bucket| bucket.active)
            .unwrap();
        assert_eq!(active.covered_end_exclusive, "2026-08-04T01:30:00-07:00");
    }

    #[test]
    fn reused_snapshot_preserves_fact_freshness_in_generated_at() {
        let snapshot = approved_snapshot();
        let report = build_usage_report(
            &snapshot,
            &custom("2026-06-01", "2026-06-05"),
            now("2026-08-04T17:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();

        assert_eq!(report.generated_at, snapshot.generated_at);
        assert_eq!(report.generated_at, "2026-08-04T08:30:00Z");
    }

    #[test]
    fn snapshot_facts_newer_than_reporting_now_are_rejected() {
        let mut snapshot = approved_snapshot();
        snapshot.generated_at = "2026-08-04T08:31:00Z".into();

        let error = build_usage_report(
            &snapshot,
            &preset(UsagePeriod::Today),
            now("2026-08-04T01:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("snapshot generatedAt must not be after reporting now"));
    }

    #[test]
    fn approved_today_fixture_serializes_exact_report_shape_and_values() {
        let report = build_usage_report(
            &approved_snapshot(),
            &preset(UsagePeriod::Today),
            now("2026-08-04T01:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();
        let mut actual = serde_json::to_value(report).unwrap();
        actual["meta"]["cliVersion"] = json!("prototype");
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/wayfinder/time-range-cost-chart/prototypes/report-cache-contract/fixtures/report-v3-today.json"
        ))
        .unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn approved_custom_fixture_matches_production_serializer() {
        let report = build_usage_report(
            &approved_custom_snapshot(),
            &custom("2026-06-01", "2026-06-05"),
            now("2026-08-05T00:30:00Z"),
            snapshot_scan_info(),
        )
        .unwrap();
        let mut actual = serde_json::to_value(report).unwrap();
        actual["meta"]["cliVersion"] = json!("prototype");
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/wayfinder/time-range-cost-chart/prototypes/report-cache-contract/fixtures/report-v3-custom-historical.json"
        ))
        .unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn preset_selection_serializes_every_approved_wire_value() {
        for (period, expected) in [
            (UsagePeriod::Today, "today"),
            (UsagePeriod::Days7, "7d"),
            (UsagePeriod::Days30, "30d"),
            (UsagePeriod::All, "all"),
        ] {
            assert_eq!(
                serde_json::to_value(selection_dto(&preset(period))).unwrap(),
                json!({"kind": "preset", "preset": expected})
            );
        }
    }

    #[test]
    fn selected_facts_drive_sorted_rollups_shares_and_latest_project_label() {
        let mut snapshot = approved_snapshot();
        snapshot.days[0].projects[0].display_name = "Old tokens label".into();
        let day = &mut snapshot.days[1];
        day.clients = vec![
            UsageSnapshotClient {
                client: "alpha".into(),
                model_id: "model-a".into(),
                provider_id: "provider-a".into(),
                token_breakdown: UsageSnapshotTokenBreakdown {
                    input: 70_000,
                    output: 50_000,
                    cache_read: 50_000,
                    cache_write: 10_000,
                    reasoning: 20_000,
                },
                cost: 3.0,
                messages: 80,
            },
            UsageSnapshotClient {
                client: "beta".into(),
                model_id: "model-b".into(),
                provider_id: "provider-b".into(),
                token_breakdown: UsageSnapshotTokenBreakdown {
                    input: 39_200,
                    output: 28_000,
                    cache_read: 28_000,
                    cache_write: 5_600,
                    reasoning: 11_200,
                },
                cost: 1.62,
                messages: 38,
            },
        ];
        day.projects = vec![
            UsageSnapshotProject {
                project_key: Some("/workspace/tokens".into()),
                display_name: "Latest tokens label".into(),
                totals: UsageSnapshotTotals {
                    tokens: 200_000,
                    cost: 3.0,
                    messages: 80,
                },
                models: vec![UsageSnapshotProjectModel {
                    model_id: "model-a".into(),
                    provider_id: "provider-a".into(),
                    totals: UsageSnapshotTotals {
                        tokens: 200_000,
                        cost: 3.0,
                        messages: 80,
                    },
                }],
            },
            UsageSnapshotProject {
                project_key: Some("/workspace/other".into()),
                display_name: "Other".into(),
                totals: UsageSnapshotTotals {
                    tokens: 112_000,
                    cost: 1.62,
                    messages: 38,
                },
                models: vec![UsageSnapshotProjectModel {
                    model_id: "model-b".into(),
                    provider_id: "provider-b".into(),
                    totals: UsageSnapshotTotals {
                        tokens: 112_000,
                        cost: 1.62,
                        messages: 38,
                    },
                }],
            },
        ];

        let today = build_usage_report(
            &snapshot,
            &preset(UsagePeriod::Today),
            now("2026-08-04T01:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();
        assert_eq!(today.by_client[0].client, "alpha");
        assert_eq!(today.by_client[0].tokens, 200_000);
        assert!((today.by_client[0].share - (200_000.0 / 312_000.0)).abs() <= 1e-12);
        assert_eq!(today.by_client[1].client, "beta");
        assert_eq!(today.by_model[0].model_id, "model-a");
        assert_eq!(today.by_model[0].clients, vec!["alpha"]);
        assert_eq!(today.by_project[0].display_name, "Latest tokens label");
        assert_eq!(today.by_project[0].models[0].model_id, "model-a");
        assert_eq!(today.summary.clients, vec!["alpha", "beta"]);
        assert_eq!(today.summary.models, vec!["model-a", "model-b"]);

        let historical = build_usage_report(
            &snapshot,
            &custom("2026-08-03", "2026-08-03"),
            now("2026-08-05T12:00:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();
        assert_eq!(historical.summary.total_tokens, 625_000);
        assert_eq!(historical.by_project.len(), 1);
        assert_eq!(historical.by_project[0].display_name, "Old tokens label");
    }

    #[test]
    fn finite_daily_costs_that_overflow_the_selected_rollup_are_rejected() {
        let mut snapshot = approved_snapshot();
        for day in &mut snapshot.days {
            day.totals.cost = f64::MAX;
            day.clients[0].cost = f64::MAX;
            day.projects[0].totals.cost = f64::MAX;
            day.projects[0].models[0].totals.cost = f64::MAX;
            for (index, hour) in day.hours.iter_mut().enumerate() {
                hour.totals.cost = if index == 0 { f64::MAX } else { 0.0 };
            }
            day.unplaced_for_hourly.cost = 0.0;
        }

        let error = build_usage_report(
            &snapshot,
            &preset(UsagePeriod::All),
            now("2026-08-04T17:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("cost accumulation overflow"));
    }

    #[test]
    fn custom_selection_before_first_usage_zero_fills_without_rollups() {
        let report = build_usage_report(
            &approved_snapshot(),
            &custom("2026-06-01", "2026-06-05"),
            now("2026-08-04T17:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();
        let value = serde_json::to_value(&report).unwrap();

        assert_eq!(
            value["selection"],
            json!({"kind": "custom", "startDate": "2026-06-01", "endDate": "2026-06-05"})
        );
        assert_eq!(value["summary"]["totalTokens"], 0);
        assert_eq!(value["summary"]["totalCost"], 0.0);
        assert_eq!(value["summary"]["messages"], 0);
        assert_eq!(value["summary"]["activeDays"], 0);
        assert_eq!(value["byClient"], json!([]));
        assert_eq!(value["byProject"], json!([]));
        assert_eq!(value["byModel"], json!([]));
        assert_eq!(value["timeSeries"]["granularity"], "day");
        assert_eq!(report.time_series.buckets.len(), 5);
        assert!(report
            .time_series
            .buckets
            .iter()
            .all(|bucket| bucket.totals == UsageReportTotals::default()));
        assert_eq!(report.time_series.unplaced, UsageReportTotals::default());
    }

    #[test]
    fn missing_prior_day_context_is_zero_and_never_changes_today_totals() {
        let mut snapshot = approved_snapshot();
        snapshot.days.remove(0);

        let report = build_usage_report(
            &snapshot,
            &preset(UsagePeriod::Today),
            now("2026-08-04T01:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();

        assert_eq!(report.time_series.buckets.len(), 12);
        assert!(report.time_series.buckets[..10]
            .iter()
            .all(|bucket| bucket.context_only && bucket.totals == UsageReportTotals::default()));
        assert_eq!(report.summary.total_tokens, 312_000);
        assert_eq!(report.summary.total_cost, 4.62);
        assert_eq!(report.summary.messages, 118);
        assert_eq!(report.time_series.unplaced.tokens, 12_000);
    }

    #[test]
    fn natural_week_buckets_fold_selected_days_and_zero_fill_clipped_edges() {
        let report = build_usage_report(
            &approved_snapshot(),
            &custom("2026-07-22", "2026-08-05"),
            now("2026-08-06T12:00:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();

        assert_eq!(
            report.time_series.granularity,
            BucketGranularity::NaturalWeek
        );
        assert_eq!(
            report.time_series.selection_start,
            "2026-07-22T00:00:00-07:00"
        );
        assert_eq!(report.time_series.buckets.len(), 3);
        assert_eq!(
            report.time_series.buckets[0].totals,
            UsageReportTotals::default()
        );
        assert_eq!(
            report.time_series.buckets[1].totals,
            UsageReportTotals::default()
        );
        assert!(report.time_series.buckets[0].incomplete_edge);
        assert!(!report.time_series.buckets[1].incomplete_edge);
        assert!(report.time_series.buckets[2].incomplete_edge);
        assert_eq!(
            report.time_series.buckets[2].covered_end_exclusive,
            "2026-08-06T00:00:00-07:00"
        );
        assert_eq!(report.time_series.buckets[2].totals.tokens, 937_000);
        assert_eq!(report.summary.total_tokens, 937_000);
        assert_eq!(report.summary.messages, 263);
        assert!((report.summary.total_cost - 14.12).abs() <= 1e-12);
        assert_eq!(report.time_series.unplaced, UsageReportTotals::default());
    }

    fn weekday_hour_cell(
        report: &UsageReportV3,
        weekday: u8,
        hour: u8,
    ) -> &UsageReportWeekdayHourCell {
        report
            .weekday_hour
            .iter()
            .find(|cell| cell.weekday == weekday && cell.hour == hour)
            .expect("grid covers every weekday × hour cell")
    }

    #[test]
    fn weekday_hour_grid_is_full_ordered_and_zero_filled() {
        let report = build_usage_report(
            &approved_snapshot(),
            &custom("2026-08-03", "2026-08-04"),
            now("2026-08-04T01:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();

        assert_eq!(report.weekday_hour.len(), 7 * 24);
        for (index, cell) in report.weekday_hour.iter().enumerate() {
            let expected_weekday = u8::try_from(index / 24).unwrap() + 1;
            let expected_hour = u8::try_from(index % 24).unwrap();
            assert_eq!((cell.weekday, cell.hour), (expected_weekday, expected_hour));
        }

        // 2026-08-03 is a Monday with placed hours 14:00–23:00 (LA).
        assert_eq!(weekday_hour_cell(&report, 1, 14).tokens, 40_000);
        assert_eq!(weekday_hour_cell(&report, 1, 21).cost, 1.2000000000000002);
        assert_eq!(weekday_hour_cell(&report, 1, 23).messages, 19);
        // 2026-08-04 is a Tuesday with placed hours 00:00 and 01:00.
        assert_eq!(weekday_hour_cell(&report, 2, 0).tokens, 120_000);
        assert_eq!(weekday_hour_cell(&report, 2, 1).cost, 2.62);
        // Empty combinations stay explicit zero cells.
        assert_eq!(
            weekday_hour_cell(&report, 7, 12),
            &UsageReportWeekdayHourCell {
                weekday: 7,
                hour: 12,
                ..UsageReportWeekdayHourCell::default()
            }
        );
    }

    #[test]
    fn weekday_hour_grid_respects_the_selected_range() {
        let report = build_usage_report(
            &approved_snapshot(),
            &preset(UsagePeriod::Today),
            now("2026-08-04T01:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();

        // Today (2026-08-04) is Tuesday-only: Monday cells are zero …
        assert!(report
            .weekday_hour
            .iter()
            .filter(|cell| cell.weekday == 1)
            .all(|cell| cell.tokens == 0 && cell.cost == 0.0 && cell.messages == 0));
        // … and Tuesday carries only its placed hours (unplaced stays out).
        assert_eq!(weekday_hour_cell(&report, 2, 0).tokens, 120_000);
        assert_eq!(weekday_hour_cell(&report, 2, 1).tokens, 180_000);
        assert!(report
            .weekday_hour
            .iter()
            .filter(|cell| cell.weekday == 2 && cell.hour >= 2)
            .all(|cell| cell.tokens == 0));
    }

    #[test]
    fn weekday_hour_cells_plus_unplaced_conserve_the_summary() {
        let report = build_usage_report(
            &approved_snapshot(),
            &preset(UsagePeriod::Today),
            now("2026-08-04T01:30:00-07:00"),
            snapshot_scan_info(),
        )
        .unwrap();

        let placed_tokens: i64 = report.weekday_hour.iter().map(|cell| cell.tokens).sum();
        let placed_cost: f64 = report.weekday_hour.iter().map(|cell| cell.cost).sum();
        let placed_messages: i32 = report.weekday_hour.iter().map(|cell| cell.messages).sum();
        assert_eq!(
            placed_tokens + report.time_series.unplaced.tokens,
            report.summary.total_tokens
        );
        assert!(
            cost_matches(
                placed_cost + report.time_series.unplaced.cost,
                report.summary.total_cost
            ),
            "weekday-hour cost conservation failed: placed={placed_cost} unplaced={} summary={}",
            report.time_series.unplaced.cost,
            report.summary.total_cost
        );
        assert_eq!(
            placed_messages + report.time_series.unplaced.messages,
            report.summary.messages
        );
    }
}
