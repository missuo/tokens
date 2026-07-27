use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokens_core::scanner::ScannerSettings;

const DEFAULT_AUTO_REFRESH_MS: u64 = 60_000;
const MIN_AUTO_REFRESH_MS: u64 = 30_000;
const MAX_AUTO_REFRESH_MS: u64 = 3_600_000;

const DEFAULT_NATIVE_TIMEOUT_MS: u64 = 300_000;
const MIN_NATIVE_TIMEOUT_MS: u64 = 5_000;
const MAX_NATIVE_TIMEOUT_MS: u64 = 3_600_000;

pub const DEFAULT_AUTOSUBMIT_INTERVAL_MINUTES: u64 = 24 * 60;
pub const MIN_AUTOSUBMIT_INTERVAL_MINUTES: u64 = 15;
pub const MAX_AUTOSUBMIT_INTERVAL_MINUTES: u64 = 7 * 24 * 60;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightSettings {
    /// When true, every `tokens --light` run atomically overwrites the
    /// TUI cache (same semantics as `--light --write-cache`). The CLI
    /// flags `--write-cache` / `--no-write-cache` override this per-invocation.
    #[serde(default)]
    pub write_cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutosubmitSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_autosubmit_interval_minutes")]
    pub interval_minutes: u64,
    #[serde(default, deserialize_with = "deserialize_string_array_lossy")]
    pub clients: Vec<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub year: Option<String>,
    #[serde(default)]
    pub today: bool,
    #[serde(default)]
    pub yesterday: bool,
    #[serde(default)]
    pub week: bool,
    #[serde(default)]
    pub month: bool,
    #[serde(default)]
    pub scheduler: Option<String>,
    #[serde(default)]
    pub last_run_at_ms: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl Default for AutosubmitSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: DEFAULT_AUTOSUBMIT_INTERVAL_MINUTES,
            clients: Vec::new(),
            since: None,
            until: None,
            year: None,
            today: false,
            yesterday: false,
            week: false,
            month: false,
            scheduler: None,
            last_run_at_ms: None,
            last_error: None,
        }
    }
}

impl AutosubmitSettings {
    fn normalize(mut self) -> Self {
        self.interval_minutes = self.interval_minutes.clamp(
            MIN_AUTOSUBMIT_INTERVAL_MINUTES,
            MAX_AUTOSUBMIT_INTERVAL_MINUTES,
        );
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_color_palette")]
    pub color_palette: String,
    #[serde(default)]
    pub auto_refresh_enabled: bool,
    #[serde(default = "default_auto_refresh_ms")]
    pub auto_refresh_ms: u64,
    #[serde(default)]
    pub include_unused_models: bool,
    #[serde(default = "default_native_timeout_ms")]
    pub native_timeout_ms: u64,
    /// Persistent scanner configuration. Allows users to pin additional
    /// OpenCode SQLite paths (and, in future, other scanner overrides)
    /// without having to set env vars on every invocation.
    ///
    /// `#[serde(default)]` makes this a drop-in addition — settings.json
    /// files written before the field existed still load cleanly, and an
    /// empty `"scanner": {}` is equivalent to not setting it at all.
    #[serde(default)]
    pub scanner: ScannerSettings,
    /// Default `--client` filter applied when the user does not pass any
    /// CLI client flag. Lets people pin "I only care about my OpenCode and
    /// Claude usage" without typing `--client opencode,claude` on every
    /// invocation.
    ///
    /// Stored as canonical lowercase ids matching `ClientFilter::as_filter_str`
    /// (e.g. `["opencode", "claude", "synthetic"]`). Unknown ids are dropped
    /// silently at load time so a typo or stale entry never breaks Tokens.
    /// CLI flags always override this list completely — no merging.
    #[serde(default, deserialize_with = "deserialize_string_array_lossy")]
    pub default_clients: Vec<String>,
    #[serde(default)]
    pub light: LightSettings,
    /// Opt-in toggle for the per-minute breakdown tab. Default is `false`
    /// to keep the tab strip focused on the daily/hourly views most users
    /// want and to skip the minute-bucket aggregation cost in DataLoader
    /// for users who never need it. Set to `true` to surface the Minutely
    /// tab and enable its aggregation in subsequent loads.
    #[serde(default)]
    pub minutely_tab_enabled: bool,
    #[serde(default)]
    pub autosubmit: AutosubmitSettings,
    /// User-defined model-name aliases folded at grouping time. Different
    /// name-strings for one physical model (e.g. `claude-opus-4-8-cc`,
    /// `anthropic/claude-opus-4-8`) map to a single canonical name so usage
    /// stats do not split across rows. Keys and values are matched
    /// case-insensitively against the normalized model name.
    ///
    /// `#[serde(default)]` keeps settings.json files written before the field
    /// existed loading cleanly; an absent or empty map means no folding.
    #[serde(default)]
    pub model_aliases: tokens_core::ModelAliasMap,
    /// Pinned IANA timezone (e.g. `"Asia/Shanghai"`) used to bucket usage into
    /// calendar dates. Detected from the system once and persisted so date
    /// bucketing stays stable when the user travels or submits from another
    /// machine — see `tokens_core::bucket_tz` and
    /// https://github.com/missuo/tokens/issues/15. `None` falls back to the
    /// machine's current local timezone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// Lossy deserializer for `defaultClients`: accepts an array of arbitrary
/// JSON values, keeps only string elements, and silently drops anything
/// else. Hand-edited settings.json files sometimes end up with stray nulls,
/// numbers, or trailing trash; failing the whole load over one bad element
/// would silently fall back to defaults for *every* setting in the file
/// (theme, scanner paths, etc.), which is a much worse user experience
/// than dropping the bad entry.
fn deserialize_string_array_lossy<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<Vec<serde_json::Value>> = Option::deserialize(deserializer).ok().flatten();
    Ok(value
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect())
}

fn default_color_palette() -> String {
    "blue".to_string()
}

fn default_auto_refresh_ms() -> u64 {
    DEFAULT_AUTO_REFRESH_MS
}

fn default_native_timeout_ms() -> u64 {
    DEFAULT_NATIVE_TIMEOUT_MS
}

fn default_autosubmit_interval_minutes() -> u64 {
    DEFAULT_AUTOSUBMIT_INTERVAL_MINUTES
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            color_palette: default_color_palette(),
            auto_refresh_enabled: false,
            auto_refresh_ms: DEFAULT_AUTO_REFRESH_MS,
            include_unused_models: false,
            native_timeout_ms: DEFAULT_NATIVE_TIMEOUT_MS,
            scanner: ScannerSettings::default(),
            default_clients: Vec::new(),
            light: LightSettings::default(),
            minutely_tab_enabled: false,
            autosubmit: AutosubmitSettings::default(),
            model_aliases: tokens_core::ModelAliasMap::default(),
            timezone: None,
        }
    }
}

/// Thin helper that loads settings and returns just the scanner portion.
///
/// Every CLI entry point that builds a `ReportOptions`
/// calls this so user-configured scanner paths are honored on every
/// invocation. Errors during load fall through to
/// [`ScannerSettings::default`] — a missing or malformed settings.json
/// should never break `tokens` runs.
pub fn load_scanner_settings() -> ScannerSettings {
    Settings::load().scanner
}

/// Loads the user's configured model aliases, honoring a `--home` override the
/// same way [`load_scanner_settings_for_home`] does. A missing or malformed
/// settings.json yields an empty map (no folding); this never errors.
pub fn load_model_aliases() -> tokens_core::ModelAliasMap {
    Settings::load().model_aliases
}

impl Settings {
    fn normalize(mut self) -> Self {
        self.auto_refresh_ms = self
            .auto_refresh_ms
            .clamp(MIN_AUTO_REFRESH_MS, MAX_AUTO_REFRESH_MS);
        self.native_timeout_ms = self
            .native_timeout_ms
            .clamp(MIN_NATIVE_TIMEOUT_MS, MAX_NATIVE_TIMEOUT_MS);
        self.autosubmit = self.autosubmit.normalize();
        self
    }

    fn config_path() -> Result<PathBuf> {
        let config_dir = crate::paths::get_config_dir();

        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700))?;
            }
        }

        Ok(config_dir.join("settings.json"))
    }

    /// Returns the legacy `~/Library/Application Support/tokens/settings.json`
    /// path on macOS so `load()` can fall back to it during the transition.
    /// Returns `None` on other platforms or when HOME cannot be resolved.
    fn legacy_macos_path() -> Option<PathBuf> {
        crate::paths::legacy_macos_config_dir().map(|d| d.join("settings.json"))
    }

    pub fn load() -> Self {
        let primary = Self::config_path()
            .ok()
            .and_then(|path| Some((fs::read_to_string(&path).ok()?, path)));

        // Transparent macOS fallback: pre-fix releases wrote settings.json under
        // `~/Library/Application Support/tokens/`. Read it once if the new
        // path is empty so users don't lose theme / scanner / defaultClients
        // preferences after upgrading. The next `save()` lands at the new
        // canonical path under `~/.config/tokens/`. Skipped when the user
        // has explicitly pinned a config root via `TOKENS_CONFIG_DIR` so
        // CI sandboxes and isolated profiles stay hermetic instead of
        // silently ingesting personal settings from the legacy macOS path.
        let raw = primary.or_else(|| {
            if crate::paths::is_config_dir_overridden() {
                return None;
            }
            Self::legacy_macos_path()
                .and_then(|legacy| Some((fs::read_to_string(&legacy).ok()?, legacy)))
        });

        let Some((content, path)) = raw else {
            return Settings::default();
        };

        match serde_json::from_str::<Settings>(&content) {
            Ok(settings) => settings.normalize(),
            // Falling back to defaults silently discards defaultClients, the
            // pinned timezone, scanner.extraScanPaths and autosubmit — and a
            // reverted timezone is exactly the drift that double-counts days.
            // Name the file and the parse error instead.
            Err(error) => {
                // `load()` runs several times per command, so warn once per
                // process instead of repeating the same message.
                static WARNED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    eprintln!("  Warning: failed to parse {}: {}", path.display(), error);
                    eprintln!("  Continuing with default settings.");
                }
                Settings::default()
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;

        // Atomic write: write to temp file, sync, then rename
        // Matches the pattern used in tui/cache.rs and pricing/cache.rs
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let tmp_filename = format!(".settings.{}.{:x}.tmp", std::process::id(), nanos);
        let temp_path = path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join(&tmp_filename);

        let write_result = (|| -> Result<()> {
            let mut file = fs::File::create(&temp_path)?;
            use std::io::Write;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            tokens_core::fs_atomic::replace_file(&temp_path, &path)?;
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }

        write_result
    }

    pub fn get_native_timeout(&self) -> Duration {
        let timeout_ms = if let Ok(env_val) = std::env::var("TOKENS_NATIVE_TIMEOUT_MS") {
            env_val.parse::<u64>().unwrap_or(self.native_timeout_ms)
        } else {
            self.native_timeout_ms
        };

        let clamped = timeout_ms.clamp(MIN_NATIVE_TIMEOUT_MS, MAX_NATIVE_TIMEOUT_MS);
        Duration::from_millis(clamped)
    }
}

