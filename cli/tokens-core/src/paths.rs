//! Cross-platform resolution for Tokens' user config and cache dirs.
//!
//! Tokens-core needs the same path helpers tokens-cli uses (settings
//! and message/pricing caches read from related directories), so the
//! resolver lives here and is re-exported from tokens-cli for callers
//! that already imported it from there. macOS users following the docs
//! expect `~/.config/tokens/` because that is what `auth.rs`,
//! `cursor.rs`, and `antigravity.rs` already write to.
//! `dirs::config_dir()` would instead return `~/Library/Application Support/`
//! on macOS, splitting state across two roots and silently ignoring
//! settings.json edits the user made via the documented path. This module
//! enforces the unified `~/.config/tokens/` location on macOS + Linux,
//! while keeping the platform default on Windows.

use std::path::PathBuf;

/// Resolve the tokens config dir, honoring `TOKENS_CONFIG_DIR` first.
///
/// Resolution order:
/// 1. `TOKENS_CONFIG_DIR` taken verbatim when set to a non-empty value.
///    Absolute paths are recommended; relative paths are accepted and
///    resolved against the process CWD. Empty strings are treated as
///    unset so the user gets the platform default instead of a surprise
///    `./` write — keeps the resolver consistent with
///    [`is_config_dir_overridden`], which also rejects empty strings.
/// 2. macOS: `$HOME/.config/tokens` (overrides `dirs::config_dir()`,
///    which would return `~/Library/Application Support/` and split state
///    across two roots — see module docs).
/// 3. Linux: `dirs::config_dir().join("tokens")` so XDG_CONFIG_HOME is
///    honored. Falls through to `$HOME/.config/tokens` when neither
///    `XDG_CONFIG_HOME` nor `HOME` resolve.
/// 4. Windows (and any other platform): `dirs::config_dir().join("tokens")`.
/// 5. Last-ditch fallback: `./.tokens` so a missing HOME never panics.
pub fn get_config_dir() -> PathBuf {
    if let Some(custom) = std::env::var_os("TOKENS_CONFIG_DIR") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(".config").join("tokens");
        }
    }

    dirs::config_dir()
        .map(|d| d.join("tokens"))
        .unwrap_or_else(|| PathBuf::from(".tokens"))
}

/// Resolve the tokens cache dir as `<config_dir>/cache`.
///
/// Caches (TUI display data, source-message bincode, pricing JSON, the
/// OpenCode migration record, Wrapped fonts/images) all live under this
/// single subdirectory so an isolated profile (`TOKENS_CONFIG_DIR=...`)
/// covers everything in one shot, and so `rm -rf <cache_dir>` is always
/// safe — no durable state mixed in.
pub fn get_cache_dir() -> PathBuf {
    get_config_dir().join("cache")
}

/// Whether `TOKENS_CONFIG_DIR` is explicitly set in the environment.
///
/// Callers that want to read a legacy on-disk location during a path
/// transition MUST gate that fallback on this returning `false`. When the
/// override is set (CI sandbox, tests, isolated profile), the user has
/// asked for an explicit, hermetic root — silently ingesting files from
/// the historic `~/.cache/tokens/` or `~/Library/Caches/tokens/`
/// locations defeats that contract.
pub fn is_config_dir_overridden() -> bool {
    std::env::var_os("TOKENS_CONFIG_DIR").is_some_and(|v| !v.is_empty())
}

/// Pre-#470 cache directory at `dirs::cache_dir()/tokens`.
///
/// On macOS this resolves to `~/Library/Caches/tokens/` (where the
/// source-message-cache, pricing caches, and opencode-migration.json
/// historically lived). On Linux this resolves to `$XDG_CACHE_HOME/tokens`
/// or `~/.cache/tokens/`.
///
/// Returns `None` when `TOKENS_CONFIG_DIR` is set so the override stays
/// hermetic (no legacy-data leak into isolated profiles).
pub fn legacy_dirs_cache_dir() -> Option<PathBuf> {
    if is_config_dir_overridden() {
        return None;
    }
    dirs::cache_dir().map(|d| d.join("tokens"))
}

/// Pre-#470 cache directory at `~/.cache/tokens`.
///
/// This is where the TUI display cache (`tui-data-cache.json`) and the
/// Wrapped image / font caches lived before #470 consolidated everything
/// under `<config_dir>/cache`. On Linux this typically equals
/// [`legacy_dirs_cache_dir`]; on macOS it does NOT (Library/Caches vs
/// `.cache`), so both legacy probes need to run during migration.
///
/// Returns `None` when `TOKENS_CONFIG_DIR` is set or HOME cannot be
/// resolved.
pub fn legacy_dot_cache_dir() -> Option<PathBuf> {
    if is_config_dir_overridden() {
        return None;
    }
    dirs::home_dir().map(|h| h.join(".cache").join("tokens"))
}

