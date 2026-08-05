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

    /// Return the absolute Unix-millisecond bounds of the local civil hour that
    /// contains `timestamp_ms`. Ambiguous fall-back hours retain the offset of
    /// the input instant, so their absolute buckets remain distinct.
    pub fn hour_bounds_of_ms(&self, timestamp_ms: i64) -> Option<(i64, i64)> {
        match self {
            BucketTimezone::Local => hour_bounds(&chrono::Local, timestamp_ms),
            BucketTimezone::Named(tz) => hour_bounds(tz, timestamp_ms),
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

fn hour_bounds<Tz>(tz: &Tz, timestamp_ms: i64) -> Option<(i64, i64)>
where
    Tz: TimeZone,
{
    let instant = match tz.timestamp_millis_opt(timestamp_ms) {
        LocalResult::Single(instant) => instant,
        _ => return None,
    };
    let day_start_ms = local_day_start_ms(tz, instant.date_naive())?;
    let elapsed_ms = timestamp_ms.checked_sub(day_start_ms)?;
    if elapsed_ms < 0 {
        return None;
    }
    let hour_offset_ms = elapsed_ms.div_euclid(3_600_000).checked_mul(3_600_000)?;
    let start_ms = day_start_ms.checked_add(hour_offset_ms)?;
    let full_hour_end_ms = start_ms.checked_add(3_600_000)?;
    let next_date = instant.date_naive().succ_opt()?;
    let next_day_start_ms = local_day_start_ms(tz, next_date)?;
    Some((start_ms, full_hour_end_ms.min(next_day_start_ms)))
}

fn local_day_start_ms<Tz>(tz: &Tz, date: NaiveDate) -> Option<i64>
where
    Tz: TimeZone,
{
    let midnight = date.and_hms_opt(0, 0, 0)?;
    for minute in 0..(24 * 60) {
        let local = midnight.checked_add_signed(chrono::Duration::minutes(minute))?;
        match tz.from_local_datetime(&local) {
            LocalResult::Single(start) => return Some(start.timestamp_millis()),
            LocalResult::Ambiguous(first, second) => {
                return Some(first.timestamp_millis().min(second.timestamp_millis()));
            }
            LocalResult::None => {}
        }
    }
    None
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
    use chrono::TimeZone;

    #[test]
    fn lord_howe_final_absolute_hour_is_clipped_at_next_local_midnight() {
        let timezone = chrono_tz::Australia::Lord_Howe;
        let instant = timezone
            .with_ymd_and_hms(2026, 10, 4, 23, 45, 0)
            .single()
            .unwrap();

        let (start_ms, end_ms) = BucketTimezone::Named(timezone)
            .hour_bounds_of_ms(instant.timestamp_millis())
            .unwrap();

        assert_eq!(
            timezone
                .timestamp_millis_opt(start_ms)
                .single()
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-04 23:30"
        );
        assert_eq!(
            timezone
                .timestamp_millis_opt(end_ms)
                .single()
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-05 00:00"
        );
        assert_eq!(end_ms - start_ms, 1_800_000);
    }

    #[test]
    fn ordinary_absolute_hours_remain_sixty_minutes() {
        let timezone = BucketTimezone::Named(chrono_tz::UTC);
        let instant_ms = chrono_tz::UTC
            .with_ymd_and_hms(2026, 8, 4, 12, 34, 56)
            .single()
            .unwrap()
            .timestamp_millis();

        let (start_ms, end_ms) = timezone.hour_bounds_of_ms(instant_ms).unwrap();

        assert_eq!(end_ms - start_ms, 3_600_000);
    }

    #[test]
    fn los_angeles_dst_days_keep_twenty_three_and_twenty_five_hour_partitions() {
        let timezone_name = chrono_tz::America::Los_Angeles;
        let timezone = BucketTimezone::Named(timezone_name);

        for (year, month, day, expected_hours) in [(2026, 3, 8, 23), (2026, 11, 1, 25)] {
            let start_ms = timezone_name
                .with_ymd_and_hms(year, month, day, 0, 0, 0)
                .single()
                .unwrap()
                .timestamp_millis();
            let next_date = NaiveDate::from_ymd_opt(year, month, day)
                .unwrap()
                .succ_opt()
                .unwrap();
            let end_ms = timezone_name
                .from_local_datetime(&next_date.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .unwrap()
                .timestamp_millis();
            let mut cursor = start_ms;
            let mut count = 0;
            while cursor < end_ms {
                let (hour_start, hour_end) = timezone.hour_bounds_of_ms(cursor).unwrap();
                assert_eq!(hour_start, cursor);
                assert!(hour_end <= end_ms);
                cursor = hour_end;
                count += 1;
            }

            assert_eq!(cursor, end_ms);
            assert_eq!(count, expected_hours);
        }
    }
}
