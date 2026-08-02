use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokens_core::{UnattributedModelDiagnostic, UnattributedSessionDiagnostic};

const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;
pub(crate) const DIAGNOSTIC_FILENAME: &str = "unattributed-sessions-v1.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticFile {
    schema_version: u32,
    generated_at: String,
    timezone: String,
    sessions: Vec<StoredSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSession {
    client: String,
    session_id: String,
    first_seen: i64,
    last_seen: i64,
    tokens: i64,
    cost: f64,
    messages: i32,
    models: Vec<UnattributedModelDiagnostic>,
    source_identifiers: Vec<String>,
    source_identifier_count: u64,
    source_identifiers_truncated: bool,
    first_observed_at: String,
    last_observed_at: String,
}

impl StoredSession {
    fn from_scan(session: &UnattributedSessionDiagnostic, observed_at: &str) -> Self {
        Self {
            client: session.client.clone(),
            session_id: session.session_id.clone(),
            first_seen: session.first_seen,
            last_seen: session.last_seen,
            tokens: session.tokens,
            cost: session.cost,
            messages: session.messages,
            models: session.models.clone(),
            source_identifiers: session.source_identifiers.clone(),
            source_identifier_count: session.source_identifier_count,
            source_identifiers_truncated: session.source_identifiers_truncated,
            first_observed_at: observed_at.to_string(),
            last_observed_at: observed_at.to_string(),
        }
    }

    fn update_from_scan(&mut self, session: &UnattributedSessionDiagnostic, observed_at: &str) {
        self.first_seen = session.first_seen;
        self.last_seen = session.last_seen;
        self.tokens = session.tokens;
        self.cost = session.cost;
        self.messages = session.messages;
        self.models = session.models.clone();

        let previous_count = self.source_identifier_count;
        let previous_truncated = self.source_identifiers_truncated;
        let mut sources: BTreeSet<String> = self.source_identifiers.drain(..).collect();
        sources.extend(session.source_identifiers.iter().cloned());
        let union_count = sources.len() as u64;
        self.source_identifiers = sources
            .into_iter()
            .take(tokens_core::UNATTRIBUTED_SOURCE_IDENTIFIER_LIMIT)
            .collect();
        self.source_identifier_count = previous_count
            .max(session.source_identifier_count)
            .max(union_count);
        self.source_identifiers_truncated = previous_truncated
            || session.source_identifiers_truncated
            || self.source_identifier_count > self.source_identifiers.len() as u64;
        self.last_observed_at = observed_at.to_string();
    }
}

pub(crate) fn update_diagnostics(
    path: &Path,
    generated_at: &str,
    timezone: &str,
    current: &[UnattributedSessionDiagnostic],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create diagnostic directory {}", parent.display()))?;
    }

    let lock_path = diagnostic_lock_path(path);
    let lock = open_private_lock(&lock_path)?;
    lock.lock_exclusive()
        .with_context(|| format!("lock unattributed diagnostics {}", lock_path.display()))?;

    // This ledger intentionally remains cumulative while the feature is
    // unreleased so we can collect diagnostic samples. TODO: define a bounded
    // retention/deletion policy before release. Empty/unknown session IDs can
    // still collide; Windows ACL hardening is also tracked as residual work.
    update_diagnostics_locked(path, generated_at, timezone, current)
}

fn diagnostic_lock_path(path: &Path) -> PathBuf {
    path.with_extension("json.lock")
}

fn open_private_lock(path: &Path) -> Result<fs::File> {
    #[cfg(unix)]
    let lock = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)
    };
    #[cfg(not(unix))]
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path);

    let lock = lock.with_context(|| format!("open diagnostic lock {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(lock)
}

fn update_diagnostics_locked(
    path: &Path,
    generated_at: &str,
    timezone: &str,
    current: &[UnattributedSessionDiagnostic],
) -> Result<()> {
    let existing = match fs::read_to_string(path) {
        Ok(raw) => {
            let file: DiagnosticFile = serde_json::from_str(&raw)
                .with_context(|| format!("parse unattributed diagnostics {}", path.display()))?;
            if file.schema_version != DIAGNOSTIC_SCHEMA_VERSION {
                anyhow::bail!(
                    "unsupported unattributed diagnostics schema {} in {} (expected {})",
                    file.schema_version,
                    path.display(),
                    DIAGNOSTIC_SCHEMA_VERSION
                );
            }
            Some(file)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("read unattributed diagnostics {}", path.display()));
        }
    };

    let mut sessions: BTreeMap<(String, String), StoredSession> = existing
        .map(|file| {
            file.sessions
                .into_iter()
                .map(|session| {
                    (
                        (session.client.clone(), session.session_id.clone()),
                        session,
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    for session in current {
        let key = (session.client.clone(), session.session_id.clone());
        match sessions.get_mut(&key) {
            Some(stored) => stored.update_from_scan(session, generated_at),
            None => {
                sessions.insert(key, StoredSession::from_scan(session, generated_at));
            }
        }
    }

    let file = DiagnosticFile {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
        generated_at: generated_at.to_string(),
        timezone: timezone.to_string(),
        sessions: sessions.into_values().collect(),
    };
    let body = serde_json::to_vec_pretty(&file)?;

    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        #[cfg(unix)]
        let mut output = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&tmp)?
        };
        #[cfg(not(unix))]
        let mut output = fs::File::create(&tmp)?;

        output.write_all(&body)?;
        output.sync_all()?;
        tokens_core::fs_atomic::replace_file(&tmp, path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.with_context(|| format!("write unattributed diagnostics {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(client: &str, session_id: &str, cost: f64) -> UnattributedSessionDiagnostic {
        UnattributedSessionDiagnostic {
            client: client.into(),
            session_id: session_id.into(),
            first_seen: 10,
            last_seen: 20,
            tokens: 100,
            cost,
            messages: 2,
            models: vec![UnattributedModelDiagnostic {
                model_id: "model".into(),
                provider_id: "provider".into(),
                tokens: 100,
                cost,
                messages: 2,
            }],
            source_identifiers: vec!["source-a".into()],
            source_identifier_count: 1,
            source_identifiers_truncated: false,
        }
    }

    #[test]
    fn upserts_current_sessions_and_retains_missing_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DIAGNOSTIC_FILENAME);
        update_diagnostics(&path, "first", "UTC", &[diagnostic("a", "1", 1.0)]).unwrap();
        update_diagnostics(&path, "second", "UTC", &[diagnostic("b", "2", 2.0)]).unwrap();

        let file: DiagnosticFile = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(file.sessions.len(), 2);
        let old = file
            .sessions
            .iter()
            .find(|session| session.client == "a")
            .unwrap();
        assert_eq!(old.first_observed_at, "first");
        assert_eq!(old.last_observed_at, "first");
        let current = file
            .sessions
            .iter()
            .find(|session| session.client == "b")
            .unwrap();
        assert_eq!(current.first_observed_at, "second");
        assert_eq!(current.last_observed_at, "second");
    }

    #[test]
    fn writes_valid_empty_file_with_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DIAGNOSTIC_FILENAME);
        update_diagnostics(&path, "now", "UTC", &[]).unwrap();

        let file: DiagnosticFile = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(file.sessions.is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(diagnostic_lock_path(&path))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn repeated_scan_updates_without_duplicate_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DIAGNOSTIC_FILENAME);
        update_diagnostics(&path, "first", "UTC", &[diagnostic("a", "1", 1.0)]).unwrap();
        update_diagnostics(&path, "second", "UTC", &[diagnostic("a", "1", 3.0)]).unwrap();

        let file: DiagnosticFile = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(file.sessions.len(), 1);
        assert_eq!(file.sessions[0].cost, 3.0);
        assert_eq!(file.sessions[0].first_observed_at, "first");
        assert_eq!(file.sessions[0].last_observed_at, "second");
    }

    #[test]
    fn concurrent_writers_preserve_every_session() {
        use std::sync::{Arc, Barrier};

        const WRITERS: usize = 12;
        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join(DIAGNOSTIC_FILENAME));
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        // Hold the same cross-process lock so every writer queues before the
        // read-modify-write race is released.
        let held_lock = open_private_lock(&diagnostic_lock_path(&path)).unwrap();
        held_lock.lock_exclusive().unwrap();
        let barrier = Arc::new(Barrier::new(WRITERS + 1));
        let mut writers = Vec::new();
        for index in 0..WRITERS {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            writers.push(std::thread::spawn(move || {
                barrier.wait();
                update_diagnostics(
                    &path,
                    &format!("writer-{index}"),
                    "UTC",
                    &[diagnostic("client", &index.to_string(), index as f64)],
                )
            }));
        }
        barrier.wait();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(writers.iter().all(|writer| !writer.is_finished()));
        held_lock.unlock().unwrap();

        for writer in writers {
            writer.join().unwrap().unwrap();
        }
        let file: DiagnosticFile = serde_json::from_slice(&fs::read(&*path).unwrap()).unwrap();
        assert_eq!(file.sessions.len(), WRITERS);
        for index in 0..WRITERS {
            assert!(file
                .sessions
                .iter()
                .any(|session| session.session_id == index.to_string()));
        }
    }

    #[test]
    fn invalid_existing_files_are_returned_as_errors_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DIAGNOSTIC_FILENAME);

        for original in [
            b"{ definitely not json".to_vec(),
            br#"{"schemaVersion":99,"generatedAt":"old","timezone":"UTC","sessions":[]}"#.to_vec(),
        ] {
            fs::write(&path, &original).unwrap();
            let error =
                update_diagnostics(&path, "new", "UTC", &[diagnostic("client", "new", 1.0)])
                    .unwrap_err();
            assert!(!format!("{error:#}").is_empty());
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }
}
