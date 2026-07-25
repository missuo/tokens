//! CLI-side path helpers.
//!
//! The cross-platform config and cache directory resolution lives in
//! `tokens_core::paths` so the core crate's caches can resolve the same
//! locations without depending on tokens-cli. This module re-exports
//! those helpers and adds the macOS legacy-config helper that
//! `Settings::load()` and `load_star_cache()` need (they have to read
//! `~/Library/Application Support/tokens/` once on upgrade — see #468).

use std::path::PathBuf;

#[allow(unused_imports)]
pub use tokens_core::paths::{
    get_cache_dir, get_config_dir, is_config_dir_overridden, legacy_dirs_cache_dir,
    legacy_dot_cache_dir,
};

/// Legacy macOS config dir (`~/Library/Application Support/tokens`).
///
/// Returns `None` off macOS, when HOME cannot be resolved, or when
/// `TOKENS_CONFIG_DIR` is set (so the env override stays hermetic).
/// Used by `Settings::load()` and `load_star_cache()` so users upgrading
/// from a release that wrote files under `~/Library/Application Support/`
/// keep their preferences on first launch after upgrade.
#[cfg(target_os = "macos")]
pub fn legacy_macos_config_dir() -> Option<PathBuf> {
    if is_config_dir_overridden() {
        return None;
    }
    dirs::config_dir().map(|d| d.join("tokens"))
}

#[cfg(not(target_os = "macos"))]
pub fn legacy_macos_config_dir() -> Option<PathBuf> {
    None
}

