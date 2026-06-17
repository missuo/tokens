//! Pins the process-wide date-bucketing timezone (see
//! `tokscale_core::bucket_tz`).
//!
//! Usage events only carry UTC timestamps, so the calendar date a row belongs to
//! is reconstructed at scan time. Doing that with the machine's *current*
//! timezone makes dates drift when the user travels, which the server's per-day
//! "keep the max" merge then double-counts. To keep bucketing stable we detect
//! the system IANA timezone once, persist it in settings.json, and install it
//! here before any scanning. See https://github.com/missuo/tokens/issues/15.

use std::path::Path;

use crate::tui::settings::Settings;

/// Resolve the configured (or detected) timezone and install it as the
/// process-wide bucketing timezone. Pure in-memory: never writes settings.
/// Honors a `--home` override so sandboxed/explicit-home runs stay hermetic.
pub fn install(home: &Option<String>) {
    let settings = Settings::load_for_home_override(home.as_deref().map(Path::new));
    let name = settings.timezone.or_else(detect_system_timezone);
    if let Some(name) = name {
        if let Some(tz) = tokscale_core::parse_bucket_timezone(&name) {
            tokscale_core::set_bucket_timezone(tz);
        }
    }
}

/// Persist the detected system timezone into settings.json when none is pinned
/// yet, so future submissions — including ones made while traveling — keep
/// bucketing in the same reference frame. Best-effort: a write failure leaves
/// the in-memory default (machine-local) in place. Call from the submit path.
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
    iana_time_zone::get_timezone()
        .ok()
        .filter(|name| !name.is_empty())
}
