//! PROTOTYPE — versioned bucketed report + snapshot-cache contract.
//! Typed fixtures only; this does not implement production aggregation.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const REPORT_SCHEMA_VERSION: u32 = 3;
const SNAPSHOT_SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Totals {
    tokens: i64,
    cost: f64,
    messages: i32,
}

impl Totals {
    fn sum(items: impl IntoIterator<Item = Totals>) -> Self {
        items.into_iter().fold(Self::default(), |mut total, item| {
            total.tokens += item.tokens;
            total.cost += item.cost;
            total.messages += item.messages;
            total
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenBreakdown {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
}

impl TokenBreakdown {
    fn from_tokens(tokens: i64) -> Self {
        Self {
            input: tokens * 35 / 100,
            output: tokens * 25 / 100,
            cache_read: tokens * 25 / 100,
            cache_write: tokens * 5 / 100,
            reasoning: tokens - (tokens * 90 / 100),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ReportSelection {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportRange {
    start_date: String,
    end_date: String,
    timezone: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanInfo {
    mode: String,
    force_rescan: bool,
    duration_ms: u32,
    cache: CacheInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheInfo {
    source_hits: u64,
    source_misses: u64,
    snapshot_rebuilt: bool,
    snapshot_schema_version: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    total_tokens: i64,
    total_cost: f64,
    messages: i32,
    active_days: i32,
    clients: Vec<String>,
    models: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelTotal {
    model_id: String,
    provider_id: String,
    tokens: i64,
    cost: f64,
    messages: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientRow {
    client: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    share: f64,
    models: Vec<ModelTotal>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRow {
    project_key: Option<String>,
    display_name: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    models: Vec<ModelTotal>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelRow {
    model_id: String,
    provider_id: String,
    tokens: i64,
    cost: f64,
    messages: i32,
    share: f64,
    clients: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Granularity {
    Hour,
    Day,
    NaturalWeek,
    NaturalMonth,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimeBucket {
    id: String,
    nominal_start: String,
    nominal_end_exclusive: String,
    covered_start: String,
    covered_end_exclusive: String,
    totals: Totals,
    context_only: bool,
    incomplete_edge: bool,
    active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimeSeries {
    granularity: Granularity,
    selection_start: String,
    buckets: Vec<TimeBucket>,
    unplaced: Totals,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeekdayHourCell {
    /// ISO-8601 weekday in the reporting timezone: 1 = Monday … 7 = Sunday.
    weekday: u8,
    /// Reporting-timezone hour of day: 0…23.
    hour: u8,
    tokens: i64,
    cost: f64,
    messages: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMeta {
    cli_version: String,
    timezone: String,
    report_contract: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageReportV3 {
    schema_version: u32,
    generated_at: String,
    selection: ReportSelection,
    date_range: ReportRange,
    scan: ScanInfo,
    summary: Summary,
    token_breakdown: TokenBreakdown,
    by_client: Vec<ClientRow>,
    by_project: Vec<ProjectRow>,
    by_model: Vec<ModelRow>,
    time_series: TimeSeries,
    weekday_hour: Vec<WeekdayHourCell>,
    meta: UsageMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageFactsSnapshotV3 {
    schema_version: u32,
    generated_at: String,
    bucket_date: String,
    timezone: String,
    days: Vec<SnapshotDay>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotDay {
    date: String,
    totals: Totals,
    token_breakdown: TokenBreakdown,
    clients: Vec<SnapshotClientContribution>,
    projects: Vec<SnapshotProjectContribution>,
    hours: Vec<SnapshotHour>,
    unplaced_for_hourly: Totals,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotClientContribution {
    client: String,
    model_id: String,
    provider_id: String,
    token_breakdown: TokenBreakdown,
    cost: f64,
    messages: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotProjectContribution {
    project_key: Option<String>,
    display_name: String,
    totals: Totals,
    models: Vec<SnapshotProjectModelContribution>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotProjectModelContribution {
    model_id: String,
    provider_id: String,
    totals: Totals,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotHour {
    start_ms: i64,
    end_ms: i64,
    totals: Totals,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => inspect_in_memory(),
        [command, dir] if command == "emit" => emit(Path::new(dir)),
        [command, file] if command == "inspect" => inspect(Path::new(file)),
        _ => Err(anyhow!(
            "usage: report_contract_prototype [emit <directory> | inspect <json-file>]"
        )),
    }
}

fn inspect_in_memory() -> Result<()> {
    println!("PROTOTYPE — bucketed report + snapshot-cache contract\n");
    let snapshot = sample_snapshot();
    let reports = sample_reports();
    round_trip("snapshot", &snapshot)?;
    validate_snapshot(&snapshot)?;
    for (name, report) in &reports {
        round_trip(name, report)?;
        validate_report(report)?;
        print_report(name, report);
    }
    println!("contract seam");
    println!("  build_usage_report(facts_snapshot, selection, reporting_now) -> UsageReportV3");
    println!(
        "  live scan and Layer B decode both produce the same range-independent facts snapshot"
    );
    println!("  active/incomplete state is derived at report time, never persisted\n");
    println!("Today chart density");
    println!("  Today totals remain midnight through reporting_now");
    println!(
        "  the chart emits at least 12 hourly buckets, prepending context-only hours when needed"
    );
    println!("  context-only buckets are excluded from all Today totals; selectionStart anchors the divider\n");
    println!("cache reuse");
    println!("  range switches reuse one full-history snapshot when schema, reporting day, and timezone match");
    println!("  timer/manual refresh bypasses Layer B, incrementally refreshes Layer A, and replaces the snapshot");
    println!("  force rescan clears both layers; day/timezone/schema mismatch rebuilds Layer B");
    println!("  v2 snapshots rebuild from Layer A because they do not contain hourly facts\n");
    println!("compatibility");
    println!("  old callers: tokens usage --json --period <preset>                 -> v2");
    println!("  new callers: tokens usage --json --contract v3 --period <preset>   -> v3");
    println!("  new Custom:  tokens usage --json --contract v3 --since D --until D -> v3");
    println!("  v3 snapshot is rebuilt from Layer A; report and snapshot versions are independent");
    Ok(())
}

fn emit(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    write_json(dir.join("snapshot-v3-sample.json"), &sample_snapshot())?;
    for (name, report) in sample_reports() {
        write_json(dir.join(format!("report-v3-{name}.json")), &report)?;
    }
    println!("wrote typed contract fixtures to {}", dir.display());
    Ok(())
}

fn inspect(path: &Path) -> Result<()> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    if value.get("selection").is_some() {
        let report: UsageReportV3 = serde_json::from_value(value)?;
        validate_report(&report)?;
        print_report(
            path.file_name().unwrap().to_string_lossy().as_ref(),
            &report,
        );
    } else {
        let snapshot: UsageFactsSnapshotV3 = serde_json::from_value(value)?;
        validate_snapshot(&snapshot)?;
        println!("snapshot schema     {}", snapshot.schema_version);
        println!("snapshot timezone   {}", snapshot.timezone);
        println!("snapshot days       {}", snapshot.days.len());
        println!(
            "snapshot hours      {}",
            snapshot
                .days
                .iter()
                .map(|day| day.hours.len())
                .sum::<usize>()
        );
    }
    Ok(())
}

fn round_trip<T>(name: &str, value: &T) -> Result<()>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    let encoded = serde_json::to_vec(value)?;
    let _: T = serde_json::from_slice(&encoded)?;
    println!("round_trip {:<16} {} bytes", name, encoded.len());
    Ok(())
}

fn validate_report(report: &UsageReportV3) -> Result<()> {
    if report.schema_version != REPORT_SCHEMA_VERSION {
        return Err(anyhow!("unexpected report schema version"));
    }
    if report.scan.cache.snapshot_schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(anyhow!("unexpected snapshot schema version"));
    }
    let placed = Totals::sum(
        report
            .time_series
            .buckets
            .iter()
            .filter(|bucket| !bucket.context_only)
            .map(|bucket| bucket.totals.clone()),
    );
    let expected = Totals {
        tokens: placed.tokens + report.time_series.unplaced.tokens,
        cost: placed.cost + report.time_series.unplaced.cost,
        messages: placed.messages + report.time_series.unplaced.messages,
    };
    if report.summary.total_tokens != expected.tokens
        || report.summary.messages != expected.messages
        || (report.summary.total_cost - expected.cost).abs() > 0.000_001
    {
        return Err(anyhow!(
            "summary does not equal placed buckets plus unplaced usage"
        ));
    }
    if report.weekday_hour.len() != 7 * 24 {
        return Err(anyhow!("weekday-hour grid must contain all 168 cells"));
    }
    for (index, cell) in report.weekday_hour.iter().enumerate() {
        let expected_weekday = (index / 24) as u8 + 1;
        let expected_hour = (index % 24) as u8;
        if cell.weekday != expected_weekday || cell.hour != expected_hour {
            return Err(anyhow!(
                "weekday-hour grid cells must be ordered weekday-major"
            ));
        }
    }
    Ok(())
}

fn validate_snapshot(snapshot: &UsageFactsSnapshotV3) -> Result<()> {
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(anyhow!("unexpected snapshot schema version"));
    }
    for day in &snapshot.days {
        let placed = Totals::sum(day.hours.iter().map(|hour| hour.totals.clone()));
        let expected = Totals {
            tokens: placed.tokens + day.unplaced_for_hourly.tokens,
            cost: placed.cost + day.unplaced_for_hourly.cost,
            messages: placed.messages + day.unplaced_for_hourly.messages,
        };
        if day.totals.tokens != expected.tokens
            || day.totals.messages != expected.messages
            || (day.totals.cost - expected.cost).abs() > 0.000_001
        {
            return Err(anyhow!(
                "snapshot day does not equal hourly facts plus unplaced usage"
            ));
        }
    }
    Ok(())
}

fn print_report(name: &str, report: &UsageReportV3) {
    let selected_bucket_totals = Totals::sum(
        report
            .time_series
            .buckets
            .iter()
            .filter(|bucket| !bucket.context_only)
            .map(|bucket| bucket.totals.clone()),
    );
    let context_bucket_count = report
        .time_series
        .buckets
        .iter()
        .filter(|bucket| bucket.context_only)
        .count();
    println!("\nreport {name}");
    println!("  schema            {}", report.schema_version);
    println!(
        "  range             {}…{} ({})",
        report.date_range.start_date, report.date_range.end_date, report.date_range.timezone
    );
    println!("  granularity       {:?}", report.time_series.granularity);
    println!("  buckets           {}", report.time_series.buckets.len());
    println!("  context buckets   {context_bucket_count}");
    println!("  summary tokens    {}", report.summary.total_tokens);
    println!("  selected tokens   {}", selected_bucket_totals.tokens);
    println!("  unplaced tokens   {}", report.time_series.unplaced.tokens);
}

fn write_json(path: PathBuf, value: &impl Serialize) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}

fn sample_reports() -> Vec<(&'static str, UsageReportV3)> {
    vec![
        (
            "today",
            report_fixture(
                ReportSelection::Preset {
                    preset: "today".into(),
                },
                "2026-08-04",
                "2026-08-04",
                "2026-08-04T08:30:00Z",
                Granularity::Hour,
                today_hour_buckets(),
                1,
                today_weekday_hour_grid(),
            ),
        ),
        (
            "30d",
            report_fixture(
                ReportSelection::Preset {
                    preset: "30d".into(),
                },
                "2026-07-06",
                "2026-08-04",
                "2026-08-05T00:30:00Z",
                Granularity::NaturalWeek,
                thirty_day_week_buckets(),
                30,
                thirty_day_weekday_hour_grid(),
            ),
        ),
        (
            "custom-historical",
            report_fixture(
                ReportSelection::Custom {
                    start_date: "2026-06-01".into(),
                    end_date: "2026-06-05".into(),
                },
                "2026-06-01",
                "2026-06-05",
                "2026-08-05T00:30:00Z",
                Granularity::Day,
                custom_day_buckets(),
                5,
                // Same filled totals as the 30d sample so cells + unplaced
                // conserve the fixture summary (600000 / $8.75 / 210).
                thirty_day_weekday_hour_grid(),
            ),
        ),
    ]
}

/// Full 7 × 24 grid, zero-filled — the report always emits every cell.
fn zero_weekday_hour_grid() -> Vec<WeekdayHourCell> {
    (1u8..=7)
        .flat_map(|weekday| {
            (0u8..24).map(move |hour| WeekdayHourCell {
                weekday,
                hour,
                ..WeekdayHourCell::default()
            })
        })
        .collect()
}

fn set_weekday_hour_cell(grid: &mut [WeekdayHourCell], weekday: u8, hour: u8, totals: Totals) {
    let index = (usize::from(weekday) - 1) * 24 + usize::from(hour);
    grid[index].tokens = totals.tokens;
    grid[index].cost = totals.cost;
    grid[index].messages = totals.messages;
}

/// Today (2026-08-04, a Tuesday) carries its two placed hours only.
fn today_weekday_hour_grid() -> Vec<WeekdayHourCell> {
    let mut grid = zero_weekday_hour_grid();
    set_weekday_hour_cell(
        &mut grid,
        2,
        0,
        Totals {
            tokens: 120_000,
            cost: 1.82,
            messages: 45,
        },
    );
    set_weekday_hour_cell(
        &mut grid,
        2,
        1,
        Totals {
            tokens: 180_000,
            cost: 2.62,
            messages: 69,
        },
    );
    grid
}

/// Typed 30d fixture has no underlying hourly facts; spread one cell per week
/// so the grid still conserves the summary totals.
fn thirty_day_weekday_hour_grid() -> Vec<WeekdayHourCell> {
    let mut grid = zero_weekday_hour_grid();
    for index in 0..5 {
        set_weekday_hour_cell(
            &mut grid,
            (index + 1) as u8,
            (9 + index) as u8,
            fixture_totals(index),
        );
    }
    grid
}

fn report_fixture(
    selection: ReportSelection,
    start_date: &str,
    end_date: &str,
    generated_at: &str,
    granularity: Granularity,
    buckets: Vec<TimeBucket>,
    active_days: i32,
    weekday_hour: Vec<WeekdayHourCell>,
) -> UsageReportV3 {
    let unplaced = if matches!(granularity, Granularity::Hour) {
        Totals {
            tokens: 12_000,
            cost: 0.18,
            messages: 4,
        }
    } else {
        Totals::default()
    };
    let placed = Totals::sum(
        buckets
            .iter()
            .filter(|bucket| !bucket.context_only)
            .map(|bucket| bucket.totals.clone()),
    );
    let all = Totals {
        tokens: placed.tokens + unplaced.tokens,
        cost: placed.cost + unplaced.cost,
        messages: placed.messages + unplaced.messages,
    };
    let model = ModelTotal {
        model_id: "claude-sonnet-5".into(),
        provider_id: "anthropic".into(),
        tokens: all.tokens,
        cost: all.cost,
        messages: all.messages,
    };
    UsageReportV3 {
        schema_version: REPORT_SCHEMA_VERSION,
        generated_at: generated_at.into(),
        selection,
        date_range: ReportRange {
            start_date: start_date.into(),
            end_date: end_date.into(),
            timezone: "America/Los_Angeles".into(),
        },
        scan: ScanInfo {
            mode: "snapshot".into(),
            force_rescan: false,
            duration_ms: 12,
            cache: CacheInfo {
                source_hits: 0,
                source_misses: 0,
                snapshot_rebuilt: false,
                snapshot_schema_version: SNAPSHOT_SCHEMA_VERSION,
            },
        },
        summary: Summary {
            total_tokens: all.tokens,
            total_cost: all.cost,
            messages: all.messages,
            active_days,
            clients: vec!["claude-code".into()],
            models: vec!["claude-sonnet-5".into()],
        },
        token_breakdown: TokenBreakdown::from_tokens(all.tokens),
        by_client: vec![ClientRow {
            client: "claude-code".into(),
            tokens: all.tokens,
            cost: all.cost,
            messages: all.messages,
            share: 1.0,
            models: vec![model.clone()],
        }],
        by_project: vec![ProjectRow {
            project_key: Some("/workspace/tokens".into()),
            display_name: "tokens".into(),
            tokens: all.tokens,
            cost: all.cost,
            messages: all.messages,
            models: vec![model.clone()],
        }],
        by_model: vec![ModelRow {
            model_id: model.model_id,
            provider_id: model.provider_id,
            tokens: all.tokens,
            cost: all.cost,
            messages: all.messages,
            share: 1.0,
            clients: vec!["claude-code".into()],
        }],
        time_series: TimeSeries {
            granularity,
            selection_start: format!("{start_date}T00:00:00-07:00"),
            buckets,
            unplaced,
        },
        weekday_hour,
        meta: UsageMeta {
            cli_version: "prototype".into(),
            timezone: "America/Los_Angeles".into(),
            report_contract: "v3".into(),
        },
    }
}

fn today_hour_buckets() -> Vec<TimeBucket> {
    let mut buckets: Vec<TimeBucket> = (14..24)
        .enumerate()
        .map(|(index, hour)| {
            let nominal_start = format!("2026-08-03T{hour:02}:00:00-07:00");
            let nominal_end = if hour == 23 {
                "2026-08-04T00:00:00-07:00".into()
            } else {
                format!("2026-08-03T{:02}:00:00-07:00", hour + 1)
            };
            time_bucket(
                &nominal_start,
                &nominal_start,
                &nominal_end,
                &nominal_end,
                context_totals(index),
                true,
                false,
                false,
            )
        })
        .collect();

    buckets.push(time_bucket(
        "2026-08-04T00:00:00-07:00",
        "2026-08-04T00:00:00-07:00",
        "2026-08-04T01:00:00-07:00",
        "2026-08-04T01:00:00-07:00",
        Totals {
            tokens: 120_000,
            cost: 1.82,
            messages: 45,
        },
        false,
        false,
        false,
    ));
    buckets.push(time_bucket(
        "2026-08-04T01:00:00-07:00",
        "2026-08-04T01:00:00-07:00",
        "2026-08-04T02:00:00-07:00",
        "2026-08-04T01:30:00-07:00",
        Totals {
            tokens: 180_000,
            cost: 2.62,
            messages: 69,
        },
        false,
        false,
        true,
    ));
    buckets
}

fn thirty_day_week_buckets() -> Vec<TimeBucket> {
    [
        (
            "2026-07-06T00:00:00-07:00",
            "2026-07-13T00:00:00-07:00",
            "2026-07-06T00:00:00-07:00",
            "2026-07-13T00:00:00-07:00",
            false,
            false,
        ),
        (
            "2026-07-13T00:00:00-07:00",
            "2026-07-20T00:00:00-07:00",
            "2026-07-13T00:00:00-07:00",
            "2026-07-20T00:00:00-07:00",
            false,
            false,
        ),
        (
            "2026-07-20T00:00:00-07:00",
            "2026-07-27T00:00:00-07:00",
            "2026-07-20T00:00:00-07:00",
            "2026-07-27T00:00:00-07:00",
            false,
            false,
        ),
        (
            "2026-07-27T00:00:00-07:00",
            "2026-08-03T00:00:00-07:00",
            "2026-07-27T00:00:00-07:00",
            "2026-08-03T00:00:00-07:00",
            false,
            false,
        ),
        (
            "2026-08-03T00:00:00-07:00",
            "2026-08-10T00:00:00-07:00",
            "2026-08-03T00:00:00-07:00",
            "2026-08-04T17:30:00-07:00",
            true,
            true,
        ),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(index, (nominal_start, nominal_end, covered_start, covered_end, incomplete, active))| {
            time_bucket(
                nominal_start,
                covered_start,
                nominal_end,
                covered_end,
                fixture_totals(index),
                false,
                incomplete,
                active,
            )
        },
    )
    .collect()
}

fn custom_day_buckets() -> Vec<TimeBucket> {
    (1..=5)
        .map(|day| {
            let nominal_start = format!("2026-06-{day:02}T00:00:00-07:00");
            let nominal_end = format!("2026-06-{:02}T00:00:00-07:00", day + 1);
            time_bucket(
                &nominal_start,
                &nominal_start,
                &nominal_end,
                &nominal_end,
                fixture_totals(day - 1),
                false,
                false,
                false,
            )
        })
        .collect()
}

fn context_totals(index: usize) -> Totals {
    Totals {
        tokens: 40_000 + index as i64 * 5_000,
        cost: 0.50 + index as f64 * 0.10,
        messages: 10 + index as i32,
    }
}

fn fixture_totals(index: usize) -> Totals {
    Totals {
        tokens: 100_000 + index as i64 * 10_000,
        cost: 1.25 + index as f64 * 0.25,
        messages: 40 + index as i32,
    }
}

fn time_bucket(
    nominal_start: &str,
    covered_start: &str,
    nominal_end_exclusive: &str,
    covered_end_exclusive: &str,
    totals: Totals,
    context_only: bool,
    incomplete_edge: bool,
    active: bool,
) -> TimeBucket {
    TimeBucket {
        id: nominal_start.into(),
        nominal_start: nominal_start.into(),
        nominal_end_exclusive: nominal_end_exclusive.into(),
        covered_start: covered_start.into(),
        covered_end_exclusive: covered_end_exclusive.into(),
        totals,
        context_only,
        incomplete_edge,
        active,
    }
}

fn sample_snapshot() -> UsageFactsSnapshotV3 {
    let previous_hours = (0..10)
        .map(|index| SnapshotHour {
            start_ms: 1_785_790_800_000 + index as i64 * 3_600_000,
            end_ms: 1_785_794_400_000 + index as i64 * 3_600_000,
            totals: context_totals(index),
        })
        .collect();
    let today_hours = vec![
        SnapshotHour {
            start_ms: 1_785_826_800_000,
            end_ms: 1_785_830_400_000,
            totals: Totals {
                tokens: 120_000,
                cost: 1.82,
                messages: 45,
            },
        },
        SnapshotHour {
            start_ms: 1_785_830_400_000,
            end_ms: 1_785_834_000_000,
            totals: Totals {
                tokens: 180_000,
                cost: 2.62,
                messages: 69,
            },
        },
    ];

    UsageFactsSnapshotV3 {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        generated_at: "2026-08-04T08:30:00Z".into(),
        bucket_date: "2026-08-04".into(),
        timezone: "America/Los_Angeles".into(),
        days: vec![
            snapshot_day("2026-08-03", previous_hours, Totals::default()),
            snapshot_day(
                "2026-08-04",
                today_hours,
                Totals {
                    tokens: 12_000,
                    cost: 0.18,
                    messages: 4,
                },
            ),
        ],
    }
}

fn snapshot_day(date: &str, hours: Vec<SnapshotHour>, unplaced: Totals) -> SnapshotDay {
    let placed = Totals::sum(hours.iter().map(|hour| hour.totals.clone()));
    let totals = Totals {
        tokens: placed.tokens + unplaced.tokens,
        cost: placed.cost + unplaced.cost,
        messages: placed.messages + unplaced.messages,
    };
    SnapshotDay {
        date: date.into(),
        totals: totals.clone(),
        token_breakdown: TokenBreakdown::from_tokens(totals.tokens),
        clients: vec![SnapshotClientContribution {
            client: "claude-code".into(),
            model_id: "claude-sonnet-5".into(),
            provider_id: "anthropic".into(),
            token_breakdown: TokenBreakdown::from_tokens(totals.tokens),
            cost: totals.cost,
            messages: totals.messages,
        }],
        projects: vec![SnapshotProjectContribution {
            project_key: Some("/workspace/tokens".into()),
            display_name: "tokens".into(),
            totals: totals.clone(),
            models: vec![SnapshotProjectModelContribution {
                model_id: "claude-sonnet-5".into(),
                provider_id: "anthropic".into(),
                totals,
            }],
        }],
        hours,
        unplaced_for_hourly: unplaced,
    }
}
