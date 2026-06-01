use crate::paths;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_MAX_ENTRIES: usize = 100;
const HISTORY_FILE_NAME: &str = "submit-history.jsonl";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubmitHistoryStatus {
    Success,
    Failed,
    Partial,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitHistoryEntry {
    pub id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: SubmitHistoryStatus,
    pub clients: Vec<String>,
    pub rows_submitted: usize,
    pub tokens_submitted: i64,
    pub cost_submitted: f64,
    pub active_days: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submission_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
    pub source_version: String,
}

pub fn history_path() -> PathBuf {
    paths::get_cache_dir().join(HISTORY_FILE_NAME)
}

pub fn append_entry(entry: &SubmitHistoryEntry) -> Result<()> {
    append_entry_to_path(&history_path(), entry, DEFAULT_MAX_ENTRIES)
}

pub fn latest_entry() -> Result<Option<SubmitHistoryEntry>> {
    latest_entry_from_path(&history_path())
}

pub(crate) fn append_entry_to_path(
    path: &Path,
    entry: &SubmitHistoryEntry,
    max_entries: usize,
) -> Result<()> {
    if max_entries == 0 {
        return Ok(());
    }

    let mut entries = read_entries_from_path(path)?;
    entries.push(entry.clone());
    if entries.len() > max_entries {
        entries.drain(0..entries.len() - max_entries);
    }
    write_entries_to_path(path, &entries)
}

pub(crate) fn latest_entry_from_path(path: &Path) -> Result<Option<SubmitHistoryEntry>> {
    Ok(read_entries_from_path(path)?.pop())
}

pub(crate) fn read_entries_from_path(path: &Path) -> Result<Vec<SubmitHistoryEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read submit history at {}", path.display()))?;
    Ok(raw
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                None
            } else {
                serde_json::from_str::<SubmitHistoryEntry>(line).ok()
            }
        })
        .collect())
}

fn write_entries_to_path(path: &Path, entries: &[SubmitHistoryEntry]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create submit history dir {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("jsonl.tmp");
    let mut raw = String::new();
    for entry in entries {
        raw.push_str(&serde_json::to_string(entry)?);
        raw.push('\n');
    }

    fs::write(&tmp_path, raw).with_context(|| {
        format!(
            "failed to write temporary submit history at {}",
            tmp_path.display()
        )
    })?;
    tokscale_core::fs_atomic::replace_file(&tmp_path, path).with_context(|| {
        format!(
            "failed to replace submit history {} with {}",
            path.display(),
            tmp_path.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_entry(id: &str, status: SubmitHistoryStatus) -> SubmitHistoryEntry {
        SubmitHistoryEntry {
            id: id.to_string(),
            started_at: format!("2026-06-01T00:00:0{id}Z"),
            finished_at: format!("2026-06-01T00:00:1{id}Z"),
            status,
            clients: vec!["claude".to_string(), "codex".to_string()],
            rows_submitted: 7,
            tokens_submitted: 12345,
            cost_submitted: 1.25,
            active_days: 3,
            device_id: Some("dev_test".to_string()),
            submission_id: Some(format!("sub_{id}")),
            error_summary: None,
            source_version: "3.0.0-test".to_string(),
        }
    }

    #[test]
    fn submit_history_appends_jsonl_and_reads_latest_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("submit-history.jsonl");

        append_entry_to_path(&path, &sample_entry("1", SubmitHistoryStatus::Success), 100).unwrap();
        append_entry_to_path(&path, &sample_entry("2", SubmitHistoryStatus::Failed), 100).unwrap();

        let latest = latest_entry_from_path(&path).unwrap().unwrap();
        assert_eq!(latest.id, "2");
        assert_eq!(latest.status, SubmitHistoryStatus::Failed);

        let raw = fs::read_to_string(path).unwrap();
        assert!(raw.contains("\"startedAt\""));
        assert!(raw.contains("\"rowsSubmitted\""));
        assert!(raw.contains("\"tokensSubmitted\""));
        assert!(raw.contains("\"costSubmitted\""));
    }

    #[test]
    fn submit_history_retains_only_newest_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("submit-history.jsonl");

        append_entry_to_path(&path, &sample_entry("1", SubmitHistoryStatus::Success), 2).unwrap();
        append_entry_to_path(&path, &sample_entry("2", SubmitHistoryStatus::Success), 2).unwrap();
        append_entry_to_path(&path, &sample_entry("3", SubmitHistoryStatus::Success), 2).unwrap();

        let entries = read_entries_from_path(&path).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "3"]
        );
    }
}
