//! PROTOTYPE — canonical time-range and chart-bucket rules.
//! Pure logic only; the sibling TUI is throwaway.

use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum Selection {
    Today,
    Last7Days,
    Last30Days,
    All,
    Custom { start: NaiveDate, end: NaiveDate },
}

impl Selection {
    pub fn label(&self) -> String {
        match self {
            Self::Today => "Today".into(),
            Self::Last7Days => "7D".into(),
            Self::Last30Days => "30D".into(),
            Self::All => "All".into(),
            Self::Custom { start, end } if start == end => format!("Custom · {start}"),
            Self::Custom { start, end } => format!("Custom · {start}…{end}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Granularity {
    Hour,
    Day,
    NaturalWeek,
    NaturalMonth,
}

impl Granularity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::NaturalWeek => "natural week (Mon–Sun)",
            Self::NaturalMonth => "natural month",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TimeRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl TimeRange {
    pub fn inclusive_days(self) -> i64 {
        self.end.signed_duration_since(self.start).num_days() + 1
    }
}

#[derive(Clone, Debug)]
pub struct Bucket {
    pub label: String,
    pub nominal: String,
    pub covered: String,
    pub context_only: bool,
    pub incomplete_edge: bool,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct PrototypeInput {
    pub reporting_today: NaiveDate,
    pub reporting_now: DateTime<Tz>,
    pub earliest_known_date: Option<NaiveDate>,
    pub selection: Selection,
}

#[derive(Clone, Debug)]
pub struct Resolution {
    pub range: TimeRange,
    pub granularity: Granularity,
    pub buckets: Vec<Bucket>,
}

pub fn resolve(input: &PrototypeInput) -> Result<Resolution> {
    if input.reporting_now.date_naive() != input.reporting_today {
        bail!(
            "reporting_now ({}) must fall on reporting_today ({})",
            input.reporting_now.date_naive(),
            input.reporting_today
        );
    }

    let range = resolve_range(
        &input.selection,
        input.reporting_today,
        input.earliest_known_date,
    )?;
    let granularity = choose_granularity(range);
    let minimum_hour_buckets = matches!(&input.selection, Selection::Today).then_some(12);
    let buckets = build_buckets(
        range,
        granularity,
        input.reporting_now,
        minimum_hour_buckets,
    )?;
    Ok(Resolution {
        range,
        granularity,
        buckets,
    })
}

pub fn resolve_range(
    selection: &Selection,
    today: NaiveDate,
    earliest_known_date: Option<NaiveDate>,
) -> Result<TimeRange> {
    let range = match selection {
        Selection::Today => TimeRange {
            start: today,
            end: today,
        },
        Selection::Last7Days => TimeRange {
            start: today - Duration::days(6),
            end: today,
        },
        Selection::Last30Days => TimeRange {
            start: today - Duration::days(29),
            end: today,
        },
        Selection::All => TimeRange {
            start: earliest_known_date.unwrap_or(today),
            end: today,
        },
        Selection::Custom { start, end } => TimeRange {
            start: *start,
            end: *end,
        },
    };

    if range.start > range.end {
        bail!("start date must not be after end date");
    }
    if range.end > today {
        bail!("future dates are unavailable; latest allowed date is {today}");
    }
    Ok(range)
}

pub fn choose_granularity(range: TimeRange) -> Granularity {
    match range.inclusive_days() {
        1 => Granularity::Hour,
        2..=14 => Granularity::Day,
        15..=90 => Granularity::NaturalWeek,
        _ => Granularity::NaturalMonth,
    }
}

pub fn local_datetime(tz: Tz, value: NaiveDateTime) -> Result<DateTime<Tz>> {
    match tz.from_local_datetime(&value) {
        LocalResult::Single(dt) => Ok(dt),
        LocalResult::Ambiguous(first, _) => Ok(first),
        LocalResult::None => {
            for minutes in 1..=180 {
                let shifted = value + Duration::minutes(minutes);
                if let LocalResult::Single(dt) = tz.from_local_datetime(&shifted) {
                    return Ok(dt);
                }
            }
            Err(anyhow!("cannot resolve local datetime {value} in {tz}"))
        }
    }
}

fn local_midnight(tz: Tz, date: NaiveDate) -> Result<DateTime<Tz>> {
    local_datetime(tz, date.and_hms_opt(0, 0, 0).unwrap())
}

fn build_buckets(
    range: TimeRange,
    granularity: Granularity,
    now: DateTime<Tz>,
    minimum_hour_buckets: Option<usize>,
) -> Result<Vec<Bucket>> {
    match granularity {
        Granularity::Hour => hourly_buckets(range.start, now, minimum_hour_buckets),
        Granularity::Day => Ok(day_buckets(range, now.date_naive())),
        Granularity::NaturalWeek => Ok(week_buckets(range, now.date_naive())),
        Granularity::NaturalMonth => Ok(month_buckets(range, now.date_naive())),
    }
}

fn hourly_buckets(
    date: NaiveDate,
    now: DateTime<Tz>,
    minimum_bucket_count: Option<usize>,
) -> Result<Vec<Bucket>> {
    let tz = now.timezone();
    let day_start = local_midnight(tz, date)?;
    let next_day = local_midnight(tz, date.succ_opt().unwrap())?;
    let is_today = date == now.date_naive();
    let mut starts = Vec::new();
    let mut cursor = day_start;
    while cursor < next_day && (!is_today || cursor <= now) {
        starts.push(cursor);
        cursor += Duration::hours(1);
    }
    if let Some(minimum) = minimum_bucket_count {
        let mut context_cursor = day_start;
        while starts.len() < minimum {
            context_cursor -= Duration::hours(1);
            starts.insert(0, context_cursor);
        }
    }

    let mut local_labels: HashMap<String, usize> = HashMap::new();
    for start in &starts {
        *local_labels
            .entry(start.format("%H:%M").to_string())
            .or_default() += 1;
    }

    Ok(starts
        .into_iter()
        .map(|start| {
            let end = start + Duration::hours(1);
            let context_only = start < day_start;
            let active = is_today && !context_only && start <= now && now < end;
            let local_label = start.format("%H:%M").to_string();
            let label = if local_labels[&local_label] > 1 {
                format!("{} {}", local_label, start.format("%:z"))
            } else {
                local_label
            };
            Bucket {
                label,
                nominal: format!("{} → {}", start.to_rfc3339(), end.to_rfc3339()),
                covered: if active {
                    format!("{} → {}", start.to_rfc3339(), now.to_rfc3339())
                } else {
                    format!("{} → {}", start.to_rfc3339(), end.to_rfc3339())
                },
                context_only,
                incomplete_edge: false,
                active,
            }
        })
        .collect())
}

fn day_buckets(range: TimeRange, today: NaiveDate) -> Vec<Bucket> {
    dates(range.start, range.end)
        .into_iter()
        .map(|date| Bucket {
            label: date.format("%b %-d").to_string(),
            nominal: date.to_string(),
            covered: if date == today {
                format!("{date} through reporting_now")
            } else {
                date.to_string()
            },
            context_only: false,
            incomplete_edge: false,
            active: date == today,
        })
        .collect()
}

fn week_buckets(range: TimeRange, today: NaiveDate) -> Vec<Bucket> {
    let mut buckets = Vec::new();
    let mut nominal_start =
        range.start - Duration::days(range.start.weekday().num_days_from_monday() as i64);
    while nominal_start <= range.end {
        let nominal_end = nominal_start + Duration::days(6);
        let covered_start = nominal_start.max(range.start);
        let covered_end = nominal_end.min(range.end);
        buckets.push(calendar_bucket(
            format!(
                "{}–{}",
                covered_start.format("%b %-d"),
                covered_end.format("%b %-d")
            ),
            nominal_start,
            nominal_end,
            covered_start,
            covered_end,
            today,
        ));
        nominal_start += Duration::days(7);
    }
    buckets
}

fn month_buckets(range: TimeRange, today: NaiveDate) -> Vec<Bucket> {
    let mut buckets = Vec::new();
    let mut nominal_start = range.start.with_day(1).unwrap();
    while nominal_start <= range.end {
        let next_month = if nominal_start.month() == 12 {
            NaiveDate::from_ymd_opt(nominal_start.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(nominal_start.year(), nominal_start.month() + 1, 1).unwrap()
        };
        let nominal_end = next_month.pred_opt().unwrap();
        let covered_start = nominal_start.max(range.start);
        let covered_end = nominal_end.min(range.end);
        buckets.push(calendar_bucket(
            covered_start.format("%b %Y").to_string(),
            nominal_start,
            nominal_end,
            covered_start,
            covered_end,
            today,
        ));
        nominal_start = next_month;
    }
    buckets
}

fn calendar_bucket(
    label: String,
    nominal_start: NaiveDate,
    nominal_end: NaiveDate,
    covered_start: NaiveDate,
    covered_end: NaiveDate,
    today: NaiveDate,
) -> Bucket {
    Bucket {
        label,
        nominal: format!("{nominal_start}…{nominal_end}"),
        covered: if covered_start <= today && today <= covered_end {
            format!("{covered_start}…{covered_end} through reporting_now")
        } else {
            format!("{covered_start}…{covered_end}")
        },
        context_only: false,
        incomplete_edge: covered_start != nominal_start || covered_end != nominal_end,
        active: covered_start <= today && today <= covered_end,
    }
}

fn dates(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let mut result = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        result.push(cursor);
        cursor = cursor.succ_opt().unwrap();
    }
    result
}

pub fn parse_date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|error| anyhow!("invalid date {value:?}: {error}"))
}

pub fn parse_reporting_now(tz: Tz, value: &str) -> Result<DateTime<Tz>> {
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .map_err(|error| anyhow!("invalid local datetime {value:?}: {error}"))?;
    local_datetime(tz, naive)
}

pub fn format_resolution(input: &PrototypeInput, resolution: &Resolution) -> String {
    let mut lines = vec![
        format!("selection       {}", input.selection.label()),
        format!("reporting_tz    {}", input.reporting_now.timezone()),
        format!("reporting_now   {}", input.reporting_now.to_rfc3339()),
        format!(
            "canonical_range  {}…{} ({} inclusive days)",
            resolution.range.start,
            resolution.range.end,
            resolution.range.inclusive_days()
        ),
        format!("granularity     {}", resolution.granularity.label()),
        format!("bucket_count    {}", resolution.buckets.len()),
        String::new(),
        "buckets".into(),
    ];
    for bucket in &resolution.buckets {
        let mut flags = Vec::new();
        if bucket.context_only {
            flags.push("CONTEXT ONLY");
        }
        if bucket.incomplete_edge {
            flags.push("INCOMPLETE EDGE");
        }
        if bucket.active {
            flags.push("ACTIVE");
        }
        let flags = if flags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", flags.join(", "))
        };
        lines.push(format!(
            "  {:<18} covered {:<44} nominal {}{}",
            bucket.label, bucket.covered, bucket.nominal, flags
        ));
    }
    lines.join("\n")
}
