//! The date-bucketing timezone this device submits in (see
//! `tokens_core::bucket_tz`).
//!
//! Usage events only carry UTC timestamps, so the calendar date a row belongs to
//! is reconstructed at scan time. Doing that with the machine's *current*
//! timezone makes dates drift when the user travels, which the server's per-day
//! "keep the max" merge then double-counts. To keep bucketing stable we detect
//! the system IANA timezone once, persist it in settings.json, and hand it to
//! every scan through `ScannerSettings::bucket_timezone`. See
//! https://github.com/missuo/tokens/issues/15.
//!
//! The pin lives in the top-level `timezone` key, which every release of this
//! fork has written. Upstream keeps its pin in `scanner.bucketTimezone`; that
//! key is honoured as a fallback, but the top-level key wins so no existing
//! device changes the zone it buckets into on upgrade.

use crate::settings::Settings;
use tokens_core::BucketTimezone;

/// The zone name scans on this device bucket into: the pinned `timezone`,
/// else `scanner.bucketTimezone`, else the machine's current zone. Never
/// writes settings.
pub fn effective_name(settings: &Settings) -> Option<String> {
    settings
        .timezone
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            settings
                .scanner
                .bucket_timezone
                .clone()
                .filter(|name| !name.trim().is_empty())
        })
        .or_else(detect_system_timezone)
}

/// The resolved bucketing zone for this device.
pub fn current() -> BucketTimezone {
    BucketTimezone::from_pinned_name(effective_name(&Settings::load()).as_deref())
}

/// Persist the detected system timezone into settings.json when none is pinned
/// yet, so future submissions — including ones made while traveling — keep
/// bucketing in the same reference frame. Best-effort: a write failure leaves
/// the machine-local zone in place for this run. Call from the submit path.
pub fn ensure_pinned() {
    let mut settings = Settings::load();
    if settings.timezone.is_some() {
        return;
    }
    if let Some(name) = detect_system_timezone() {
        settings.timezone = Some(name);
        let _ = settings.save();
    }
}

fn detect_system_timezone() -> Option<String> {
    tokens_core::bucket_tz::detect_local_iana_name()
}
