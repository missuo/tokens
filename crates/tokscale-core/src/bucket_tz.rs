//! Process-wide "bucketing timezone": the timezone used to assign every usage
//! event — and "today" — to a calendar date.
//!
//! Session logs only record UTC timestamps, so "which local day did this usage
//! belong to" is always *reconstructed* at scan time. If we reconstruct it with
//! the machine's *current* timezone (`chrono::Local`), the answer changes when
//! the user travels, and events near local midnight drift between days. Combined
//! with the server's per-day "keep the max" merge, that drift double-counts
//! usage. See https://github.com/missuo/tokens/issues/15.
//!
//! To make bucketing stable regardless of where `submit` runs from, the CLI
//! pins a single IANA timezone (detected once, then persisted / synced) and
//! installs it here before any scanning. Everything that turns a timestamp into
//! a date — or computes "today" — routes through [`bucket_timezone`] so the
//! whole process agrees on one stable reference frame.
//!
//! When nothing is pinned the default is [`BucketTimezone::Local`], preserving
//! the previous machine-local behavior for embedders and tests.

use std::sync::OnceLock;

use chrono::{LocalResult, NaiveDate, TimeZone};

/// The timezone every date bucket is computed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketTimezone {
    /// The machine's current system timezone (`chrono::Local`). Default when no
    /// timezone has been pinned.
    Local,
    /// A fixed IANA timezone pinned by the user, so bucketing stays stable
    /// across travel and across the account's machines.
    Named(chrono_tz::Tz),
}

static BUCKET_TZ: OnceLock<BucketTimezone> = OnceLock::new();

/// Pin the process-wide bucketing timezone. The first call wins; later calls are
/// ignored so the reference frame is fixed for the process lifetime. Call this
/// once at startup, before any scanning.
pub fn set_bucket_timezone(tz: BucketTimezone) {
    let _ = BUCKET_TZ.set(tz);
}

/// The configured bucketing timezone, or [`BucketTimezone::Local`] when unset.
pub fn bucket_timezone() -> BucketTimezone {
    BUCKET_TZ.get().copied().unwrap_or(BucketTimezone::Local)
}

/// Parse an IANA timezone name (e.g. `"Asia/Shanghai"`) into a pinned
/// [`BucketTimezone`]. Returns `None` for names the tz database doesn't know.
pub fn parse_bucket_timezone(name: &str) -> Option<BucketTimezone> {
    name.parse::<chrono_tz::Tz>()
        .ok()
        .map(BucketTimezone::Named)
}

impl BucketTimezone {
    /// Format a Unix-millisecond timestamp as a `YYYY-MM-DD` date in this tz.
    /// Returns an empty string for timestamps the tz can't represent.
    pub fn date_of_ms(&self, timestamp_ms: i64) -> String {
        match self {
            BucketTimezone::Local => fmt_date(&chrono::Local, timestamp_ms),
            BucketTimezone::Named(tz) => fmt_date(tz, timestamp_ms),
        }
    }

    /// Format a Unix-millisecond timestamp as an `YYYY-MM-DD HH:00` hour bucket
    /// in this tz. Returns `None` for timestamps the tz can't represent.
    pub fn date_hour_of_ms(&self, timestamp_ms: i64) -> Option<String> {
        match self {
            BucketTimezone::Local => fmt_hour(&chrono::Local, timestamp_ms),
            BucketTimezone::Named(tz) => fmt_hour(tz, timestamp_ms),
        }
    }

    /// Today's calendar date in this tz.
    pub fn today(&self) -> NaiveDate {
        match self {
            BucketTimezone::Local => chrono::Local::now().date_naive(),
            BucketTimezone::Named(tz) => chrono::Utc::now().with_timezone(tz).date_naive(),
        }
    }

    /// Today's local midnight (00:00) as Unix milliseconds in this tz. Used by
    /// today-only incremental scans to decide which files to look at.
    pub fn midnight_today_ms(&self) -> Option<i64> {
        let today = self.today();
        match self {
            BucketTimezone::Local => midnight_ms(&chrono::Local, today),
            BucketTimezone::Named(tz) => midnight_ms(tz, today),
        }
    }
}

fn fmt_date<Tz>(tz: &Tz, timestamp_ms: i64) -> String
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    match tz.timestamp_millis_opt(timestamp_ms) {
        LocalResult::Single(dt) => dt.format("%Y-%m-%d").to_string(),
        _ => String::new(),
    }
}

fn fmt_hour<Tz>(tz: &Tz, timestamp_ms: i64) -> Option<String>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    match tz.timestamp_millis_opt(timestamp_ms) {
        LocalResult::Single(dt) => Some(dt.format("%Y-%m-%d %H:00").to_string()),
        _ => None,
    }
}

fn midnight_ms<Tz>(tz: &Tz, date: NaiveDate) -> Option<i64>
where
    Tz: TimeZone,
{
    let midnight = date.and_hms_opt(0, 0, 0)?;
    match tz.from_local_datetime(&midnight) {
        LocalResult::Single(dt) => Some(dt.timestamp_millis()),
        LocalResult::Ambiguous(dt, _) => Some(dt.timestamp_millis()),
        LocalResult::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_iana_names() {
        assert!(matches!(
            parse_bucket_timezone("Asia/Shanghai"),
            Some(BucketTimezone::Named(_))
        ));
        assert!(parse_bucket_timezone("Not/AZone").is_none());
    }

    #[test]
    fn named_zone_buckets_independently_of_machine_tz() {
        // 2026-06-16T20:00:00Z. In UTC+8 (Asia/Shanghai) this is 2026-06-17
        // 04:00 local -> bucket 06-17, regardless of where the process runs.
        let ts = chrono::DateTime::parse_from_rfc3339("2026-06-16T20:00:00Z")
            .unwrap()
            .timestamp_millis();
        let shanghai = parse_bucket_timezone("Asia/Shanghai").unwrap();
        assert_eq!(shanghai.date_of_ms(ts), "2026-06-17");

        // Same instant in US/Pacific (UTC-7 DST) is 2026-06-16 13:00 -> 06-16.
        let pacific = parse_bucket_timezone("America/Los_Angeles").unwrap();
        assert_eq!(pacific.date_of_ms(ts), "2026-06-16");
    }
}
