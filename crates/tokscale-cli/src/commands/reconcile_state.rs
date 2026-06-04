#![allow(dead_code)]

use crate::paths;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub const CURRENT_RECONCILE_CACHE_SCHEMA_VERSION: u32 = 1;
const FULL_RECONCILE_INTERVAL_DAYS: i64 = 7;
const RECONCILE_STATE_FILE_NAME: &str = "reconcile-state.json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileState {
    pub last_full_reconcile_at: Option<String>,
    pub cache_schema_version: u32,
}

impl Default for ReconcileState {
    fn default() -> Self {
        Self {
            last_full_reconcile_at: None,
            cache_schema_version: CURRENT_RECONCILE_CACHE_SCHEMA_VERSION,
        }
    }
}

impl ReconcileState {
    pub fn load() -> Result<Self> {
        Self::load_from_path(&state_path())
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read reconcile state at {}", path.display()))?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn save(&self) -> Result<()> {
        self.save_to_path(&state_path())
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create reconcile state dir {}", parent.display())
            })?;
        }

        let raw = serde_json::to_string_pretty(self)?;
        fs::write(path, raw)
            .with_context(|| format!("failed to write reconcile state at {}", path.display()))
    }

    pub fn mark_full_reconciled(&mut self, now: chrono::DateTime<chrono::Utc>) {
        self.last_full_reconcile_at = Some(now.to_rfc3339());
        self.cache_schema_version = CURRENT_RECONCILE_CACHE_SCHEMA_VERSION;
    }

    pub fn should_full_reconcile(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        if self.cache_schema_version != CURRENT_RECONCILE_CACHE_SCHEMA_VERSION {
            return true;
        }

        let Some(last_full_reconcile_at) = &self.last_full_reconcile_at else {
            return true;
        };

        let Ok(last_full_reconcile_at) =
            chrono::DateTime::parse_from_rfc3339(last_full_reconcile_at)
        else {
            return true;
        };

        now.signed_duration_since(last_full_reconcile_at.with_timezone(&chrono::Utc))
            .num_days()
            >= FULL_RECONCILE_INTERVAL_DAYS
    }
}

pub fn state_path() -> PathBuf {
    paths::get_cache_dir().join(RECONCILE_STATE_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_reconcile_state_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reconcile-state.json");

        let state = ReconcileState::load_from_path(&path).unwrap();

        assert!(state.last_full_reconcile_at.is_none());
        assert_eq!(
            state.cache_schema_version,
            CURRENT_RECONCILE_CACHE_SCHEMA_VERSION
        );
    }

    #[test]
    fn should_full_reconcile_after_seven_days() {
        let state = ReconcileState {
            last_full_reconcile_at: Some("2026-06-01T00:00:00Z".to_string()),
            cache_schema_version: CURRENT_RECONCILE_CACHE_SCHEMA_VERSION,
        };
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-08T00:00:01Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        assert!(state.should_full_reconcile(now));
    }
}
