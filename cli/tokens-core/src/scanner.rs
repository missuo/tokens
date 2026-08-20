//! Parallel file scanner for session directories
//!
//! Uses walkdir with rayon for parallel directory traversal.

use rayon::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::clients::ClientId;
use crate::sessions::{normalize_workspace_key, workspace_label_from_key};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Emit a one-time `tracing::warn!` if `path` does not start with the scan's
/// supplied home directory. The scan is NOT blocked — this is a heads-up only.
fn warn_if_escapes_home(home: &Path, client_id: ClientId, path: &Path) {
    if !path.starts_with(home) {
        tracing::warn!(
            client = client_id.as_str(),
            path = %path.display(),
            home = %home.display(),
            "extra scan path is outside $HOME — verify this is intentional"
        );
    }
}

/// User-controlled scanner settings loaded from a config file.
///
/// This is the persistent, declarative counterpart to environment variables
/// like `TOKENS_EXTRA_DIRS` — it lives on the `scanner` key inside
/// `~/.config/tokens/settings.json` and is threaded down into
/// [`scan_all_clients_with_scanner_settings`].
///
/// `#[serde(default)]` at both the struct and field level guarantees that
/// older settings.json files (which have no `scanner` key at all, or an
/// empty `{}`) deserialize cleanly without errors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScannerSettings {
    /// Absolute paths to additional OpenCode SQLite databases to scan.
    ///
    /// Use this when the opencode binary was launched with `OPENCODE_DB`
    /// pointing at a location outside the default `~/.local/share/opencode`
    /// data directory, so Tokens' auto-discovery can't find it.
    ///
    /// Paths are merged into the auto-discovered
    /// [`ScanResult::opencode_dbs`] list; duplicates (by canonical path)
    /// are removed and non-existent entries are silently skipped so stale
    /// config does not break the scan. WAL/SHM sidecar files are rejected
    /// with the same [`is_opencode_db_filename`] check used for
    /// auto-discovery.
    #[serde(default)]
    pub opencode_db_paths: Vec<PathBuf>,
    /// Additional per-client scan roots loaded from settings.json.
    ///
    /// Keys use public client ids like `codex`, `gemini`, and `openclaw`
    /// so the JSON stays stable and human-editable.
    #[serde(default)]
    pub extra_scan_paths: BTreeMap<String, Vec<PathBuf>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrushDbSource {
    pub db_path: PathBuf,
    pub workspace_key: Option<String>,
    pub workspace_label: Option<String>,
}

/// Result of scanning all session directories
#[derive(Debug)]
pub struct ScanResult {
    pub files: [Vec<PathBuf>; ClientId::COUNT],
    /// All OpenCode SQLite databases discovered under the data dir.
    ///
    /// Includes the default `opencode.db` (used by `latest`/`beta` channels
    /// and anyone with `OPENCODE_DISABLE_CHANNEL_DB=1`) as well as any
    /// channel-suffixed variants such as `opencode-stable.db`,
    /// `opencode-nightly.db`, etc. See upstream logic in opencode's
    /// `packages/opencode/src/storage/db.ts` (`getChannelPath`).
    pub opencode_dbs: Vec<PathBuf>,
    pub copilot_desktop_db: Option<PathBuf>,
    pub synthetic_db: Option<PathBuf>,
    pub kilo_db: Option<PathBuf>,
    pub hermes_db: Option<PathBuf>,
    pub goose_db: Option<PathBuf>,
    pub zed_db: Option<PathBuf>,
    pub kiro_db: Option<PathBuf>,
    pub crush_dbs: Vec<CrushDbSource>,
    /// ZCode v2 CLI usage database at `~/.zcode/cli/db/db.sqlite`.
    pub zcode_db: Option<PathBuf>,
    /// MiMo Code SQLite databases discovered under the data dir.
    pub micode_dbs: Vec<PathBuf>,
    /// Path to the OpenCode legacy JSON directory (for migration cache stat checks)
    pub opencode_json_dir: Option<PathBuf>,
    /// Devin CLI SQLite databases, including the default data path and any
    /// user-configured scan roots.
    pub devin_dbs: Vec<PathBuf>,
    /// VS Code Copilot chat session JSONL files discovered under
    /// `workspaceStorage/*/chatSessions/*.jsonl`.
    pub copilot_vscode_sessions: Vec<PathBuf>,
}

impl Default for ScanResult {
    fn default() -> Self {
        Self {
            files: std::array::from_fn(|_| Vec::new()),
            opencode_dbs: Vec::new(),
            copilot_desktop_db: None,
            synthetic_db: None,
            kilo_db: None,
            hermes_db: None,
            goose_db: None,
            zed_db: None,
            kiro_db: None,
            crush_dbs: Vec::new(),
            zcode_db: None,
            micode_dbs: Vec::new(),
            opencode_json_dir: None,
            devin_dbs: Vec::new(),
            copilot_vscode_sessions: Vec::new(),
        }
    }
}

impl ScanResult {
    pub fn get(&self, client: ClientId) -> &Vec<PathBuf> {
        &self.files[client as usize]
    }

    pub fn get_mut(&mut self, client: ClientId) -> &mut Vec<PathBuf> {
        &mut self.files[client as usize]
    }

    /// Drop JSONL transcript files last modified before `since_ms`, so a
    /// today-only scan never reads historical files. SQLite sources are left
    /// intact (they're read whole, not per-file). A file whose mtime can't be
    /// read reports "now" and is kept, so a fresh-but-unreadable file is never
    /// dropped.
    pub fn retain_files_modified_since(&mut self, since_ms: i64) {
        for client_files in &mut self.files {
            client_files.retain(|path| {
                crate::sessions::utils::file_modified_timestamp_ms(path) >= since_ms
            });
        }
    }

    /// Get total number of files found
    pub fn total_files(&self) -> usize {
        self.files.iter().map(|v| v.len()).sum()
    }

    /// Get all files as a single vector
    pub fn all_files(&self) -> Vec<(ClientId, PathBuf)> {
        let mut result = Vec::with_capacity(self.total_files());

        for client in ClientId::iter() {
            for path in self.get(client) {
                result.push((client, path.clone()));
            }
        }

        result
    }

    /// Return every Hermes SQLite database that should be parsed.
    ///
    /// Hermes has a default `state.db` path plus optional profile databases
    /// discovered through `scanner.extraScanPaths.hermes`. The generic
    /// `files` bucket carries the extra profile DBs, so this helper gives
    /// callers a single deduped view without changing older `hermes_db`
    /// consumers that only expect the default path.
    pub fn hermes_db_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();

        let mut push = |path: &Path| {
            let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if seen.insert(key) {
                paths.push(path.to_path_buf());
            }
        };

        if let Some(path) = &self.hermes_db {
            push(path);
        }

        for path in self.get(ClientId::Hermes) {
            push(path);
        }

        paths
    }

    /// Return every Zed threads SQLite database that should be parsed.
    pub fn zed_db_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();

        let mut push = |path: &Path| {
            let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if seen.insert(key) {
                paths.push(path.to_path_buf());
            }
        };

        if let Some(path) = &self.zed_db {
            push(path);
        }

        for path in self.get(ClientId::Zed) {
            push(path);
        }

        paths
    }
}

pub fn headless_roots_with_env_strategy(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    if use_env_roots {
        if let Ok(path) = std::env::var("TOKENS_HEADLESS_DIR") {
            return vec![PathBuf::from(path)];
        }
    }

    let mut roots = Vec::new();
    roots.push(PathBuf::from(format!(
        "{}/.config/tokens/headless",
        home_dir
    )));

    let mac_root = PathBuf::from(format!(
        "{}/Library/Application Support/tokens/headless",
        home_dir
    ));
    roots.push(mac_root);

    roots
}

pub fn headless_roots(home_dir: &str) -> Vec<PathBuf> {
    headless_roots_with_env_strategy(home_dir, true)
}

pub fn copilot_exporter_path_with_env_strategy(use_env_roots: bool) -> Option<PathBuf> {
    if !use_env_roots {
        return None;
    }

    let path = std::env::var("COPILOT_OTEL_FILE_EXPORTER_PATH").ok()?;
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(PathBuf::from(trimmed))
}

/// Scan a single directory for session files
pub fn scan_directory(root: &str, pattern: &str) -> Vec<PathBuf> {
    if !std::path::Path::new(root).exists() {
        return Vec::new();
    }

    let mut paths: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .par_bridge()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            // WalkDir already knows the entry type from the directory read, so
            // trust it for the common regular-file case and avoid a redundant
            // stat() per file (warm scans over huge trees were stat-bound).
            // Symlinks still fall back to a following stat to preserve behavior.
            let file_type = e.file_type();
            let is_file = file_type.is_file() || (file_type.is_symlink() && path.is_file());
            if !is_file {
                return false;
            }

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            let is_in_archive_dir = path.components().any(|c| {
                c.as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("archive")
            });

            match pattern {
                "*.json" => file_name.ends_with(".json"),
                "*.json|*.jsonl" => file_name.ends_with(".json") || file_name.ends_with(".jsonl"),
                "*.jsonl" => file_name.ends_with(".jsonl"),
                "*.ndjson" => file_name.ends_with(".ndjson"),
                "*.log" => file_name.ends_with(".log"),
                "codebuddy-extension-log" => {
                    file_name.ends_with(".log")
                        && path.components().any(|component| {
                            component
                                .as_os_str()
                                .to_string_lossy()
                                .eq_ignore_ascii_case("Tencent-Cloud.coding-copilot")
                        })
                }
                // OpenClaw: also match archived transcripts
                // (<uuid>.jsonl.deleted.<ts>, <uuid>.jsonl.reset.<ts>)
                "*.jsonl*" => {
                    file_name.ends_with(".jsonl")
                        || file_name.contains(".jsonl.deleted.")
                        || file_name.contains(".jsonl.reset.")
                }
                "*.csv" => file_name.ends_with(".csv"),
                "usage*.csv" => {
                    if is_in_archive_dir {
                        return false;
                    }

                    if file_name == "usage.csv" {
                        return true;
                    }

                    // Accept only per-account files: usage.<account>.csv
                    if !file_name.starts_with("usage.") || !file_name.ends_with(".csv") {
                        return false;
                    }

                    // Exclude legacy backups like usage.backup-<ts>.csv
                    if file_name.starts_with("usage.backup") {
                        return false;
                    }

                    true
                }
                "usage*.json" => {
                    if is_in_archive_dir {
                        return false;
                    }

                    if file_name == "usage.json" {
                        return true;
                    }

                    if !file_name.starts_with("usage.") || !file_name.ends_with(".json") {
                        return false;
                    }

                    if file_name.starts_with("usage.backup") {
                        return false;
                    }

                    true
                }
                "session-*.json" => {
                    file_name.starts_with("session-") && file_name.ends_with(".json")
                }
                "session_*.json" => {
                    file_name.starts_with("session_") && file_name.ends_with(".json")
                }
                "T-*.json" => file_name.starts_with("T-") && file_name.ends_with(".json"),
                "*.settings.json" => file_name.ends_with(".settings.json"),
                "kiro-globalstorage" => {
                    file_name.ends_with(".chat")
                        || file_name.ends_with(".json")
                        || path.extension().is_none()
                }
                // Kiro IDE (VS Code-based) session layout on disk:
                //   ~/.kiro/sessions/<workspace>/sess_<uuid>/session.json
                //   ~/.kiro/sessions/<workspace>/sess_<uuid>/messages.jsonl
                // Anchor discovery on `session.json` (the metadata file); the
                // parser reads the sibling `messages.jsonl` itself. Requiring a
                // `sess_*` parent keeps this from colliding with the CLI layout
                // (`~/.kiro/sessions/cli/*.json`) that shares the same tree.
                "kiro-ide-session" => {
                    file_name == "session.json"
                        && path
                            .parent()
                            .and_then(|parent| parent.file_name())
                            .and_then(|name| name.to_str())
                            .map(|name| name.starts_with("sess_"))
                            .unwrap_or(false)
                }
                "sessions.json" => file_name == "sessions.json",
                "wire.jsonl" => file_name == "wire.jsonl",
                "updates.jsonl" => file_name == "updates.jsonl",
                "events.jsonl" => file_name == "events.jsonl",
                "ui_messages.json" => file_name == "ui_messages.json",
                // Cline CLI transcripts are `<id>.messages.json`; the suffix
                // cannot collide with the VS Code `ui_messages.json` format.
                "cline-cli-messages" => file_name.ends_with(".messages.json"),
                "session-usage.json" => file_name == "session-usage.json",
                "usage-v2.json" => file_name == "usage-v2.json",
                "chat-messages.json" => file_name == "chat-messages.json",
                "workbuddy.db" => file_name == "workbuddy.db",
                "sessions.db" => file_name == "sessions.db",
                "state.db" => file_name == "state.db",
                "threads.db" => file_name == "threads.db",
                // Antigravity CLI conversation databases. `ends_with(".db")`
                // naturally rejects the `.db-wal`/`.db-shm`/`.db-journal`
                // sidecars SQLite writes alongside the main file.
                "*.db" => file_name.ends_with(".db"),
                _ => false,
            }
        })
        .map(|e| e.path().to_path_buf())
        .collect();
    // Sort for deterministic ordering. sort_unstable() is sufficient (no stability
    // requirement for PathBuf) and avoids allocation. Note: ordering is byte-lexical,
    // not case-normalized (known Windows/macOS caveat for mixed-case paths).
    paths.sort_unstable();
    paths
}

/// Parse a `TOKENS_EXTRA_DIRS`-formatted string into (ClientId, path) pairs.
///
/// Format: comma-separated `client:path` pairs.
/// Example: `"claude:/path/to/mac/sessions,openclaw:/other/path"`
///
/// Only returns entries whose client is present in `enabled`.
/// This is a pure function — the caller is responsible for reading the
/// environment variable and passing its value here.
pub fn parse_extra_dirs(value: &str, enabled: &HashSet<ClientId>) -> Vec<(ClientId, String)> {
    if value.is_empty() {
        return Vec::new();
    }

    value
        .split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            let (client_str, path) = entry.split_once(':')?;
            let client_id = ClientId::from_str(client_str.trim())?;
            if !enabled.contains(&client_id) || !supports_extra_dir_scanning(client_id) {
                return None;
            }
            let path = path.trim().to_string();
            if path.is_empty() {
                return None;
            }
            Some((client_id, path))
        })
        .collect()
}

pub fn extra_scan_paths_for(
    settings: &ScannerSettings,
    enabled: &HashSet<ClientId>,
) -> Vec<(ClientId, PathBuf)> {
    settings
        .extra_scan_paths
        .iter()
        .filter_map(|(client_str, paths)| {
            let client_id = ClientId::from_str(client_str)?;
            if !enabled.contains(&client_id) || !supports_extra_dir_scanning(client_id) {
                return None;
            }
            Some(
                paths
                    .iter()
                    .filter(|path| !path.as_os_str().is_empty())
                    .cloned()
                    .map(move |path| (client_id, path)),
            )
        })
        .flatten()
        .collect()
}

pub fn built_in_extra_scan_paths_for(
    home_dir: &str,
    enabled: &HashSet<ClientId>,
) -> Vec<(ClientId, PathBuf)> {
    let mut paths = Vec::new();

    if enabled.contains(&ClientId::Claude) {
        paths.push((
            ClientId::Claude,
            PathBuf::from(format!("{}/.claude/transcripts", home_dir)),
        ));
        paths.extend(
            crate::cc_mirror::discover_claude_project_roots(Path::new(home_dir))
                .into_iter()
                .map(|path| (ClientId::Claude, path)),
        );
    }

    paths
}

/// Discover Hermes profile databases under a Hermes home directory.
///
/// Hermes stores the default profile at `<hermes-home>/state.db` and named
/// profiles at `<hermes-home>/profiles/<profile>/state.db`.
///
/// Data-isolation rule: sibling and default profiles are ONLY discovered when
/// scanning from the *root* Hermes home. When `HERMES_HOME` points at a
/// specific named profile (for example `<root>/profiles/coder`, i.e. its parent
/// directory is `profiles/`), the user has expressed intent to isolate that one
/// profile, so we scan ONLY that profile. We deliberately do NOT climb up to
/// sibling profiles under `<root>/profiles/*` or the default profile at
/// `<root>/state.db`. Auto-discovering (and therefore making uploadable via
/// `tokens submit`) sibling/default profiles from a profile-scoped
/// `HERMES_HOME` would silently break the isolation boundary the user set up.
/// The active profile's own `state.db` is resolved separately as the primary
/// Hermes database, so this function returns no extra paths in that case.
///
/// `read_dir` keeps profile discovery intentionally shallow: each immediate
/// child of the root home's `profiles/` directory is treated as one profile
/// directory, matching Hermes' profile layout without walking arbitrary user
/// data.
pub(crate) fn discover_hermes_profile_state_dbs(hermes_home: &Path) -> Vec<PathBuf> {
    // Profile-scoped `HERMES_HOME` (parent directory is `profiles/`): isolate to
    // this single profile and perform no sibling/default discovery.
    if hermes_home
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "profiles")
    {
        return Vec::new();
    }

    // Root Hermes home: discover every named profile under `profiles/`.
    let mut dbs: Vec<PathBuf> = std::fs::read_dir(hermes_home.join("profiles"))
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .filter_map(|entry| {
            let state_db = entry.path().join("state.db");
            state_db.is_file().then_some(state_db)
        })
        .collect();
    dbs.sort_unstable();
    dbs.dedup();
    dbs
}

/// Candidate Hermes home directories to scan for `state.db` and profiles.
///
/// Resolution order mirrors the Crush discovery's Windows rigor
/// ([`crush_registry_candidates`]):
/// 1. `HERMES_HOME` when set, otherwise `~/.hermes` — the `PathRoot::EnvVar`
///    strategy for [`ClientId::Hermes`].
/// 2. `%LOCALAPPDATA%\hermes` on native Windows (env roots enabled).
/// 3. `<home>/AppData/Local/hermes` — the literal Windows fallback, always
///    appended so it is exercised cross-platform (matching Crush's
///    `AppData/Local` fallback).
///
/// The native Windows roots are only consulted when `HERMES_HOME` is *not* set:
/// an explicit `HERMES_HOME` is authoritative and may be profile-scoped for data
/// isolation, so widening discovery to the default Windows home in that case
/// would reintroduce the isolation leak that the profile-scoping rule prevents.
fn hermes_home_candidates(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut homes = vec![PathBuf::from(
        ClientId::Hermes
            .data()
            .root
            .resolve_with_env_strategy(home_dir, use_env_roots),
    )];

    let hermes_home_set = use_env_roots
        && std::env::var("HERMES_HOME")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
    if !hermes_home_set {
        if cfg!(target_os = "windows") && use_env_roots {
            if let Some(local_app_data) =
                std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty())
            {
                homes.push(PathBuf::from(local_app_data).join("hermes"));
            }
        }
        homes.push(PathBuf::from(home_dir).join("AppData/Local/hermes"));
    }

    homes
}

#[derive(Debug, Deserialize, Default)]
struct CrushProjectList {
    #[serde(default)]
    projects: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct CrushProject {
    path: String,
    data_dir: String,
}

/// Discover every OpenCode SQLite database under the opencode data dir.
///
/// Matches:
/// - `opencode.db` (default, used by `latest`/`beta` channels or when
///   `OPENCODE_DISABLE_CHANNEL_DB=1` is set)
/// - `opencode-<channel>.db` where `<channel>` is the sanitized channel name
///   opencode bakes into the build (e.g. `stable`, `nightly`). Upstream
///   sanitizes channels with `/[^a-zA-Z0-9._-]/g -> "-"`, so the suffix we
///   accept here mirrors that character class exactly.
///
/// Ignores WAL/SHM sidecar files (`opencode.db-wal`, `opencode.db-shm`, etc.)
/// and anything that does not end in `.db`.
///
/// Returns a sorted, deterministic list for stable downstream behavior.
pub(crate) fn discover_opencode_dbs(data_dir: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(data_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut dbs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                // Could be a symlink — accept it if it resolves to a file.
                if !entry.path().is_file() {
                    return None;
                }
            }
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !is_opencode_db_filename(name) {
                return None;
            }
            Some(path)
        })
        .collect();

    dbs.sort_unstable();
    dbs
}

/// Returns true if `name` matches the opencode db naming rule:
/// `opencode.db` or `opencode-<channel>.db` with `<channel>` drawn from the
/// same `[a-zA-Z0-9._-]` character class that opencode's `getChannelPath`
/// normalizes to. Sidecar files (`.db-wal`, `.db-shm`, `.db-journal`) are
/// rejected because they do not end in `.db`.
fn is_opencode_db_filename(name: &str) -> bool {
    // Strip the trailing `.db` — reject anything else so WAL/SHM sidecars
    // (e.g. `opencode.db-wal`) are ignored.
    let stem = match name.strip_suffix(".db") {
        Some(stem) => stem,
        None => return false,
    };
    if stem == "opencode" {
        return true;
    }
    let channel = match stem.strip_prefix("opencode-") {
        Some(channel) => channel,
        None => return false,
    };
    if channel.is_empty() {
        return false;
    }
    channel
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Discover MiMo Code SQLite databases under the given data directory.
///
/// Matches `mimocode.db` and `mimocode-<channel>.db` (channel names
/// sanitized with the same `[a-zA-Z0-9._-]` character class that MiMo
/// Code's `getChannelPath` normalizes to). Ignores WAL/SHM sidecar files.
pub(crate) fn discover_micode_dbs(data_dir: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(data_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut dbs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() && !entry.path().is_file() {
                return None;
            }
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !is_micode_db_filename(name) {
                return None;
            }
            Some(path)
        })
        .collect();

    dbs.sort_unstable();
    dbs
}

/// Collapse a stream of discovered database paths into a sorted, duplicate-free
/// list, keeping the first spelling seen of any file.
///
/// Two paths that `canonicalize` to the same file (e.g. one scan root is a
/// symlink to another) are treated as one: the row-id dedup fallback in the
/// SQLite parsers namespaces by the *path string*, so scanning a single file
/// under two spellings would otherwise double-count any message that lacks an
/// embedded id. Paths that fail to canonicalize (missing file, permissions)
/// fall back to their literal form, so an identical spelling still dedups.
fn dedup_dbs_by_canonical_path(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut dbs = Vec::new();

    for db_path in paths {
        let key = std::fs::canonicalize(&db_path).unwrap_or_else(|_| db_path.clone());
        if seen.insert(key) {
            dbs.push(db_path);
        }
    }

    dbs.sort_unstable();
    dbs
}

/// Discover MiMo Code SQLite databases across several data directories,
/// returning a single sorted list with duplicate files removed.
///
/// MiMo Code can be reached from more than one root (the XDG data dir and
/// orca's hook sandbox), so this unions `discover_micode_dbs` over each
/// directory and collapses duplicates via `dedup_dbs_by_canonical_path`.
pub(crate) fn discover_micode_dbs_in_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    dedup_dbs_by_canonical_path(dirs.into_iter().flat_map(|dir| discover_micode_dbs(&dir)))
}

/// Discover Devin CLI `sessions.db` files from the default path and any
/// configured extra scan roots. Extra roots preserve the generic scanner's
/// behavior: a root may be the database itself or a directory containing one
/// or more `sessions.db` files.
fn discover_devin_cli_dbs(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    dedup_dbs_by_canonical_path(
        roots
            .into_iter()
            .flat_map(|root| scan_directory(&root.to_string_lossy(), "sessions.db")),
    )
}

/// Returns true if `name` matches the MiMo Code db naming rule:
/// `mimocode.db` or `mimocode-<channel>.db`.
fn is_micode_db_filename(name: &str) -> bool {
    let stem = match name.strip_suffix(".db") {
        Some(stem) => stem,
        None => return false,
    };
    if stem == "mimocode" {
        return true;
    }
    let channel = match stem.strip_prefix("mimocode-") {
        Some(channel) => channel,
        None => return false,
    };
    if channel.is_empty() {
        return false;
    }
    channel
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn crush_db_path(data_dir: &Path) -> Option<PathBuf> {
    let candidate = data_dir.join("crush.db");
    candidate.is_file().then_some(candidate)
}

fn resolve_crush_data_dir(project: &CrushProject) -> PathBuf {
    let data_dir = PathBuf::from(&project.data_dir);
    if data_dir.is_absolute() {
        data_dir
    } else {
        PathBuf::from(&project.path).join(data_dir)
    }
}

fn scan_crush_registry(registry_path: &Path) -> Vec<CrushDbSource> {
    let registry = match std::fs::read_to_string(registry_path) {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };

    let list: CrushProjectList = match serde_json::from_str(&registry) {
        Ok(list) => list,
        Err(_) => return Vec::new(),
    };

    list.projects
        .into_iter()
        .filter_map(|project| serde_json::from_value::<CrushProject>(project).ok())
        .filter_map(|project| {
            let db_path = crush_db_path(&resolve_crush_data_dir(&project))?;
            let workspace_key = normalize_workspace_key(&project.path);
            let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
            Some(CrushDbSource {
                db_path,
                workspace_key,
                workspace_label,
            })
        })
        .collect()
}

/// Candidate locations for Crush's `projects.json` registry, mirroring
/// Crush's own resolution order (`internal/config/load.go::GlobalConfigData`):
/// `$CRUSH_GLOBAL_DATA` first, then `$XDG_DATA_HOME/crush`, then
/// `%LOCALAPPDATA%\crush` on Windows, then `~/.local/share/crush`.
fn crush_registry_candidates(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if use_env_roots {
        if let Some(global_data) =
            std::env::var_os("CRUSH_GLOBAL_DATA").filter(|value| !value.is_empty())
        {
            candidates.push(PathBuf::from(global_data).join("projects.json"));
        }
    }

    candidates.push(PathBuf::from(
        ClientId::Crush
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots),
    ));

    if cfg!(target_os = "windows") && use_env_roots {
        if let Some(local_app_data) =
            std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty())
        {
            candidates.push(
                PathBuf::from(local_app_data)
                    .join("crush")
                    .join("projects.json"),
            );
        }
    }
    candidates.push(PathBuf::from(home_dir).join("AppData/Local/crush/projects.json"));

    candidates
}

fn discover_crush_dbs(home_dir: &str, use_env_roots: bool) -> Vec<CrushDbSource> {
    let mut dbs = Vec::new();
    for registry_path in crush_registry_candidates(home_dir, use_env_roots) {
        dbs.extend(scan_crush_registry(&registry_path));
    }
    dbs.sort_by(|a, b| a.db_path.cmp(&b.db_path));
    dbs.dedup_by(|a, b| a.db_path == b.db_path);
    dbs
}

fn cline_additional_vscode_task_roots(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from(home_dir)
        .join("Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks")];

    if cfg!(target_os = "windows") && use_env_roots {
        if let Some(app_data) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            roots.push(
                PathBuf::from(app_data)
                    .join("Code/User/globalStorage/saoudrizwan.claude-dev/tasks"),
            );
        }
    }

    roots.push(
        PathBuf::from(home_dir)
            .join("AppData/Roaming/Code/User/globalStorage/saoudrizwan.claude-dev/tasks"),
    );
    roots.push(
        PathBuf::from(home_dir)
            .join(".vscode-server/data/User/globalStorage/saoudrizwan.claude-dev/tasks"),
    );

    roots
}

/// Session roots for the Cline CLI / desktop runtime
/// (`~/.cline/data/sessions/<id>/<id>.messages.json`). Env overrides are
/// honoured in priority order so CI or non-standard installs can relocate the
/// data dir without symlinks.
fn cline_cli_session_roots(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let home_fallback = || PathBuf::from(home_dir).join(".cline/data/sessions");

    if !use_env_roots {
        return vec![home_fallback()];
    }

    let non_blank_env_path = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.to_string_lossy().trim().is_empty())
            .map(PathBuf::from)
    };

    if let Some(path) = non_blank_env_path("CLINE_SESSION_DATA_DIR") {
        return vec![path];
    }
    if let Some(path) = non_blank_env_path("CLINE_DATA_DIR") {
        return vec![path.join("sessions")];
    }
    if let Some(path) = non_blank_env_path("CLINE_DIR") {
        return vec![path.join("data/sessions")];
    }

    vec![home_fallback()]
}

pub fn devin_desktop_additional_roots(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from(home_dir).join(".config/Devin/User/acp-events"),
        PathBuf::from(home_dir).join(".config/devin/User/acp-events"),
    ];

    if cfg!(target_os = "windows") && use_env_roots {
        if let Some(app_data) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(app_data).join("Devin/User/acp-events"));
        }
    }

    roots.push(PathBuf::from(home_dir).join("AppData/Roaming/Devin/User/acp-events"));

    roots
}

fn supports_extra_dir_scanning(client_id: ClientId) -> bool {
    // Kilo CLI currently loads a single SQLite DB via `scan_result.kilo_db`
    // Roo/KiloCode require local + remote and server task roots, and Crush
    // discovers SQLite DBs via the project registry rather than scanned file
    // paths. Hermes/Zed profile databases are named consistently enough for
    // `scan_directory` to find them from user-provided roots.
    !matches!(
        client_id,
        ClientId::Kilo | ClientId::Crush | ClientId::Goose
    )
}

fn push_unique_scan_task(
    tasks: &mut Vec<(ClientId, String, &'static str)>,
    seen: &mut HashSet<(ClientId, PathBuf)>,
    client_id: ClientId,
    raw_path: impl Into<PathBuf>,
) {
    push_unique_scan_task_with_pattern(tasks, seen, client_id, raw_path, client_id.data().pattern);
}

/// Additional Codex-compatible homes owned by desktop wrappers that isolate
/// their runtime from the shell's `CODEX_HOME`. Orca stores standard Codex
/// rollout JSONL under this macOS application-support path, so a standalone
/// `tokens` process would otherwise miss those sessions entirely.
fn discover_codex_compat_homes(
    home_dir: &str,
    use_env_roots: bool,
    codex_home_is_explicit: bool,
) -> Vec<PathBuf> {
    if !use_env_roots || codex_home_is_explicit {
        return Vec::new();
    }

    let orca_home = PathBuf::from(home_dir)
        .join("Library")
        .join("Application Support")
        .join("orca")
        .join("codex-runtime-home")
        .join("home");

    if orca_home.join("sessions").is_dir() || orca_home.join("archived_sessions").is_dir() {
        vec![orca_home]
    } else {
        Vec::new()
    }
}

fn push_unique_scan_task_with_pattern(
    tasks: &mut Vec<(ClientId, String, &'static str)>,
    seen: &mut HashSet<(ClientId, PathBuf)>,
    client_id: ClientId,
    raw_path: impl Into<PathBuf>,
    pattern: &'static str,
) {
    let raw_path = raw_path.into();
    if raw_path.as_os_str().is_empty() {
        return;
    }

    let key = std::fs::canonicalize(&raw_path).unwrap_or_else(|_| raw_path.clone());
    if seen.insert((client_id, key)) {
        tasks.push((client_id, raw_path.to_string_lossy().to_string(), pattern));
    }
}

fn kiro_global_storage_roots(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from(format!(
            "{}/Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent",
            home_dir
        )),
        PathBuf::from(format!(
            "{}/Library/Application Support/kiro/User/globalStorage/kiro.kiroagent",
            home_dir
        )),
        PathBuf::from(format!(
            "{}/.config/Kiro/User/globalStorage/kiro.kiroagent",
            home_dir
        )),
        PathBuf::from(format!(
            "{}/.config/kiro/User/globalStorage/kiro.kiroagent",
            home_dir
        )),
    ];

    if cfg!(target_os = "windows") {
        if use_env_roots {
            if let Some(app_data) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
                roots.push(PathBuf::from(&app_data).join("Kiro/User/globalStorage/kiro.kiroagent"));
                roots.push(PathBuf::from(&app_data).join("kiro/User/globalStorage/kiro.kiroagent"));
            }
        }

        roots.push(PathBuf::from(format!(
            "{}/AppData/Roaming/Kiro/User/globalStorage/kiro.kiroagent",
            home_dir
        )));
        roots.push(PathBuf::from(format!(
            "{}/AppData/Roaming/kiro/User/globalStorage/kiro.kiroagent",
            home_dir
        )));
    }

    roots
}

/// Merge user-configured OpenCode db paths from [`ScannerSettings`] into the
/// auto-discovered list, in-place.
///
/// Rules:
/// - Non-existent paths are silently skipped so stale config never aborts a
///   scan (the config outlives any single opencode install).
/// - WAL/SHM/journal sidecars are rejected via [`is_opencode_db_filename`].
/// - Duplicates are removed by canonicalized path comparison, so a user who
///   explicitly lists an auto-discovered db in their config does not cause
///   it to be parsed twice.
///
/// Kept as a separate helper so the unit tests can exercise the merge
/// semantics without spinning up a full `scan_all_clients` run.
pub(crate) fn merge_user_opencode_db_paths(discovered: &mut Vec<PathBuf>, extra_paths: &[PathBuf]) {
    if extra_paths.is_empty() {
        return;
    }

    // Build a canonical-path set of what we already have so we can dedup
    // against auto-discovered entries. Fall back to the raw path if
    // canonicalize fails (e.g. on a filesystem that doesn't support it),
    // which preserves the pre-canonicalization behavior without silently
    // dropping entries.
    let mut seen: HashSet<PathBuf> = discovered
        .iter()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();

    for raw in extra_paths {
        if !raw.is_file() {
            // Stale config or wrong path — silently skip.
            continue;
        }
        let Some(name) = raw.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_opencode_db_filename(name) {
            // Reject sidecars (`.db-wal`, `.db-shm`) and anything that does
            // not match the upstream channel-db naming rule.
            continue;
        }
        let canonical = std::fs::canonicalize(raw).unwrap_or_else(|_| raw.clone());
        if seen.insert(canonical) {
            discovered.push(raw.clone());
        }
    }
}

fn discover_copilot_vscode_sessions(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    roots.push(PathBuf::from(format!(
        "{}/Library/Application Support/Code/User/workspaceStorage",
        home_dir
    )));
    roots.push(PathBuf::from(format!(
        "{}/.config/Code/User/workspaceStorage",
        home_dir
    )));

    if cfg!(target_os = "windows") && use_env_roots {
        if let Some(app_data) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
            roots.push(PathBuf::from(app_data).join("Code/User/workspaceStorage"));
        }
    }
    roots.push(PathBuf::from(home_dir).join("AppData/Roaming/Code/User/workspaceStorage"));

    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for workspace_storage in &roots {
        let hash_dirs = match std::fs::read_dir(workspace_storage) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in hash_dirs.filter_map(|e| e.ok()) {
            let chat_sessions_dir = entry.path().join("chatSessions");
            if !chat_sessions_dir.is_dir() {
                continue;
            }
            let chat_entries = match std::fs::read_dir(&chat_sessions_dir) {
                Ok(rd) => rd,
                Err(_) => continue,
            };
            for chat_entry in chat_entries.filter_map(|e| e.ok()) {
                let path = chat_entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.ends_with(".jsonl") {
                    continue;
                }
                if !path.is_file() {
                    continue;
                }
                let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                if seen.insert(key) {
                    files.push(path);
                }
            }
        }
    }

    files.sort_unstable();
    files
}

/// Scan all session client directories in parallel, with user-controlled
/// [`ScannerSettings`] merged in.
///
/// This is the preferred entry point when you have loaded persistent
/// settings (e.g. from `~/.config/tokens/settings.json`). Thin wrappers
/// [`scan_all_clients_with_env_strategy`] and [`scan_all_clients`] call
/// into this with `ScannerSettings::default()` for callers that don't care
/// about the persistent config.
pub fn scan_all_clients_with_scanner_settings(
    home_dir: &str,
    clients: &[String],
    use_env_roots: bool,
    scanner_settings: &ScannerSettings,
) -> ScanResult {
    scan_all_clients_with_env_strategy_inner(home_dir, clients, use_env_roots, scanner_settings)
}

/// Scan all session client directories in parallel
pub fn scan_all_clients_with_env_strategy(
    home_dir: &str,
    clients: &[String],
    use_env_roots: bool,
) -> ScanResult {
    scan_all_clients_with_scanner_settings(
        home_dir,
        clients,
        use_env_roots,
        &ScannerSettings::default(),
    )
}

fn scan_all_clients_with_env_strategy_inner(
    home_dir: &str,
    clients: &[String],
    use_env_roots: bool,
    scanner_settings: &ScannerSettings,
) -> ScanResult {
    let mut result = ScanResult::default();

    let include_all = clients.is_empty();
    let include_synthetic = include_all || clients.iter().any(|s| s == "synthetic");

    let enabled: HashSet<ClientId> = if include_all || include_synthetic {
        ClientId::iter().collect()
    } else {
        clients
            .iter()
            .filter_map(|s| {
                ClientId::from_str(s).or_else(|| {
                    // "9Router" is a gjc-format bridge client overseen by the
                    // 9Router bridge script. Map it to Gjc so the scanner
                    // discovers files under gjc scan roots.
                    if s.eq_ignore_ascii_case("9router") {
                        Some(ClientId::Gjc)
                    } else {
                        None
                    }
                })
            })
            .collect()
    };

    // Desktop ACP filenames need Devin CLI database titles to recover their
    // session/model/workspace metadata. Treat configured CLI roots as lookup
    // inputs for a Desktop-only scan without enabling CLI usage output.
    let mut enabled_with_devin_lookup = enabled.clone();
    if enabled.contains(&ClientId::DevinDesktop) {
        enabled_with_devin_lookup.insert(ClientId::DevinCli);
    }

    let headless_roots = headless_roots_with_env_strategy(home_dir, use_env_roots);

    // Define scan tasks
    let mut tasks: Vec<(ClientId, String, &str)> = Vec::new();
    let mut seen_scan_roots: HashSet<(ClientId, PathBuf)> = HashSet::new();
    let mut devin_cli_roots: Vec<PathBuf> = Vec::new();

    for client_id in &enabled {
        if matches!(
            client_id,
            ClientId::OpenCode
                | ClientId::Codex
                | ClientId::OpenClaw
                | ClientId::RooCode
                | ClientId::KiloCode
                | ClientId::Cline
                | ClientId::Kilo
                | ClientId::Hermes
                | ClientId::Goose
                | ClientId::Zed
                | ClientId::Crush
                | ClientId::Codebuff
                | ClientId::Kimi
                | ClientId::Gjc
                | ClientId::MiMoCode
                | ClientId::DevinCli
        ) {
            continue;
        }

        let def = client_id.data();
        let path = def.resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task(&mut tasks, &mut seen_scan_roots, *client_id, path);
    }

    for (client_id, path) in extra_scan_paths_for(scanner_settings, &enabled_with_devin_lookup) {
        warn_if_escapes_home(Path::new(home_dir), client_id, &path);
        if client_id == ClientId::DevinCli {
            devin_cli_roots.push(path);
        } else {
            push_unique_scan_task(&mut tasks, &mut seen_scan_roots, client_id, path);
        }
    }

    for (client_id, path) in built_in_extra_scan_paths_for(home_dir, &enabled) {
        push_unique_scan_task(&mut tasks, &mut seen_scan_roots, client_id, path);
    }

    if enabled.contains(&ClientId::CodeBuddy) {
        let home_path = PathBuf::from(home_dir);
        let mut codebuddy_log_roots = vec![(
            home_path
                .join("AppData")
                .join("Local")
                .join("CodeBuddyExtension")
                .join("Logs"),
            "*.log",
        )];
        let roaming_codebuddy_roots = [
            home_path
                .join("AppData")
                .join("Roaming")
                .join("CodeBuddy CN")
                .join("logs"),
            home_path
                .join("AppData")
                .join("Roaming")
                .join("Code")
                .join("logs"),
        ];
        codebuddy_log_roots.extend(
            roaming_codebuddy_roots
                .into_iter()
                .map(|root| (root, "codebuddy-extension-log")),
        );
        if use_env_roots {
            if let Some(local_app_data) = dirs::data_local_dir() {
                codebuddy_log_roots.push((
                    local_app_data.join("CodeBuddyExtension").join("Logs"),
                    "*.log",
                ));
            }
            if let Some(roaming_app_data) = dirs::config_dir() {
                codebuddy_log_roots.push((
                    roaming_app_data.join("CodeBuddy CN").join("logs"),
                    "codebuddy-extension-log",
                ));
                codebuddy_log_roots.push((
                    roaming_app_data.join("Code").join("logs"),
                    "codebuddy-extension-log",
                ));
            }
        }

        for (log_root, pattern) in codebuddy_log_roots {
            if pattern == "*.log" {
                for root in ["CodeBuddyIDE", "VSCode"] {
                    push_unique_scan_task_with_pattern(
                        &mut tasks,
                        &mut seen_scan_roots,
                        ClientId::CodeBuddy,
                        log_root.join(root),
                        pattern,
                    );
                }
                continue;
            }

            push_unique_scan_task_with_pattern(
                &mut tasks,
                &mut seen_scan_roots,
                ClientId::CodeBuddy,
                log_root,
                pattern,
            );
        }
    }

    if enabled.contains(&ClientId::WorkBuddy) {
        push_unique_scan_task_with_pattern(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::WorkBuddy,
            PathBuf::from(home_dir).join(".workbuddy/projects"),
            "*.jsonl",
        );
    }

    // Extra scan directories are part of the caller's environment, so they are
    // intentionally ignored when an explicit --home override disables env roots.
    if use_env_roots {
        let extra_dirs_val = std::env::var("TOKENS_EXTRA_DIRS").unwrap_or_default();
        for (client_id, path) in parse_extra_dirs(&extra_dirs_val, &enabled_with_devin_lookup) {
            warn_if_escapes_home(Path::new(home_dir), client_id, &PathBuf::from(&path));
            if client_id == ClientId::DevinCli {
                devin_cli_roots.push(PathBuf::from(path));
            } else {
                push_unique_scan_task(&mut tasks, &mut seen_scan_roots, client_id, path);
            }
        }
    }

    if enabled.contains(&ClientId::OpenCode) {
        let xdg_data = if use_env_roots {
            std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| format!("{}/.local/share", home_dir))
        } else {
            format!("{}/.local/share", home_dir)
        };

        // OpenCode 1.2+: SQLite database(s) at ~/.local/share/opencode/opencode*.db
        //
        // opencode picks its db filename at build time based on the release
        // channel: `latest`/`beta` use `opencode.db`, other channels use
        // `opencode-<channel>.db` (e.g. `opencode-stable.db`). A single user
        // can run multiple channels side by side, so we pick up every match
        // under the data dir. See `getChannelPath` in
        // opencode/packages/opencode/src/storage/db.ts for the source of
        // the naming rule.
        let opencode_data_dir = PathBuf::from(format!("{}/opencode", xdg_data));
        result.opencode_dbs = discover_opencode_dbs(&opencode_data_dir);

        // Merge user-configured `scanner.opencodeDbPaths` here, INSIDE the
        // `enabled.contains(&ClientId::OpenCode)` guard, so a request like
        // `tokens --claude` does not pull in OpenCode dbs the user pinned
        // for unrelated reasons. Inflated OpenCode `counts` and wasted
        // SQLite parsing work otherwise sneak past the message-level
        // client filter that runs much later in the pipeline.
        merge_user_opencode_db_paths(
            &mut result.opencode_dbs,
            &scanner_settings.opencode_db_paths,
        );
        result.opencode_dbs.sort_unstable();
        result.opencode_dbs.dedup();

        // OpenCode legacy: JSON files at ~/.local/share/opencode/storage/message/*/*.json
        let opencode_path = ClientId::OpenCode
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        result.opencode_json_dir = Some(PathBuf::from(&opencode_path));
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::OpenCode,
            opencode_path,
        );
    }

    // MiMo Code: SQLite database(s). The primary location is the XDG data dir
    // (`~/.local/share/mimocode/mimocode*.db`). MiMo Code driven through orca's
    // hook sandbox additionally writes to
    // `~/Library/Application Support/orca/mimocode-hooks/shared/data/`, and that
    // copy can hold sessions the XDG copy is missing (scanning only XDG then
    // undercounts). Scan both so the totals are the union; the cross-file dedup
    // in the parse loop (keyed on the globally unique embedded message id)
    // collapses any message present in both locations, so overlapping data is
    // never double-counted.
    if enabled.contains(&ClientId::MiMoCode) {
        // Derive the primary data dir from the client metadata so the scan path
        // stays in sync with `ClientId::MiMoCode` (XdgData root + `mimocode`)
        // rather than duplicating it here.
        let micode_data_dir = PathBuf::from(
            ClientId::MiMoCode
                .data()
                .resolve_path_with_env_strategy(home_dir, use_env_roots),
        );
        let orca_data_dir = PathBuf::from(format!(
            "{}/Library/Application Support/orca/mimocode-hooks/shared/data",
            home_dir
        ));
        result.micode_dbs = discover_micode_dbs_in_dirs([micode_data_dir, orca_data_dir]);
    }

    if enabled.contains(&ClientId::Kimi) {
        // Legacy Kimi (KIMI CLI): ~/.kimi/sessions/**/wire.jsonl
        let kimi_path = ClientId::Kimi
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task(&mut tasks, &mut seen_scan_roots, ClientId::Kimi, kimi_path);

        // Kimi Code: ~/.kimi-code/sessions/**/wire.jsonl (supports KIMI_CODE_HOME)
        let kimi_code_home = if use_env_roots {
            std::env::var("KIMI_CODE_HOME").unwrap_or_else(|_| format!("{}/.kimi-code", home_dir))
        } else {
            format!("{}/.kimi-code", home_dir)
        };
        let kimi_code_path = format!("{}/sessions", kimi_code_home);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::Kimi,
            kimi_code_path,
        );
    }

    if enabled.contains(&ClientId::Codex) {
        // Codex: ~/.codex/sessions/**/*.jsonl
        let codex_home = if use_env_roots {
            std::env::var("CODEX_HOME").unwrap_or_else(|_| format!("{}/.codex", home_dir))
        } else {
            format!("{}/.codex", home_dir)
        };
        let codex_path = ClientId::Codex
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::Codex,
            codex_path,
        );

        // Codex archived sessions: ~/.codex/archived_sessions/**/*.jsonl
        let codex_archived_path = format!("{}/archived_sessions", codex_home);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::Codex,
            codex_archived_path,
        );

        // Orca launches Codex with a private CODEX_HOME that is not inherited
        // when `tokens` runs later from a shell. Its files use the normal Codex
        // rollout format, and the downstream dedup-key pass collapses sessions
        // mirrored in both Orca and ~/.codex without double-counting them.
        let codex_home_is_explicit =
            use_env_roots && std::env::var_os("CODEX_HOME").is_some_and(|value| !value.is_empty());
        for compat_home in
            discover_codex_compat_homes(home_dir, use_env_roots, codex_home_is_explicit)
        {
            push_unique_scan_task(
                &mut tasks,
                &mut seen_scan_roots,
                ClientId::Codex,
                compat_home.join("sessions"),
            );
            push_unique_scan_task(
                &mut tasks,
                &mut seen_scan_roots,
                ClientId::Codex,
                compat_home.join("archived_sessions"),
            );
        }

        // Codex headless: <headless_root>/codex/*.jsonl
        for root in &headless_roots {
            push_unique_scan_task(
                &mut tasks,
                &mut seen_scan_roots,
                ClientId::Codex,
                root.join("codex"),
            );
        }
    }

    if enabled.contains(&ClientId::OpenClaw) {
        // OpenClaw transcripts: ~/.openclaw/agents/**/*.jsonl
        let openclaw_path = ClientId::OpenClaw
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::OpenClaw,
            openclaw_path,
        );

        // Legacy paths (Clawd -> Moltbot -> OpenClaw rebrand history)
        let clawdbot_path = format!("{}/.clawdbot/agents", home_dir);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::OpenClaw,
            clawdbot_path,
        );

        let moltbot_path = format!("{}/.moltbot/agents", home_dir);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::OpenClaw,
            moltbot_path,
        );

        let moldbot_path = format!("{}/.moldbot/agents", home_dir);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::OpenClaw,
            moldbot_path,
        );
    }

    // Oh My Pi fork (https://github.com/can1357/oh-my-pi) — same JSONL format, different root
    if enabled.contains(&ClientId::Pi) {
        let omp_path = format!("{}/.omp/agent/sessions", home_dir);
        push_unique_scan_task(&mut tasks, &mut seen_scan_roots, ClientId::Pi, omp_path);
    }

    if include_synthetic {
        let xdg_data = if use_env_roots {
            std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| format!("{}/.local/share", home_dir))
        } else {
            format!("{}/.local/share", home_dir)
        };
        let octofriend_db_path = PathBuf::from(format!("{}/octofriend/sqlite.db", xdg_data));
        if octofriend_db_path.exists() {
            result.synthetic_db = Some(octofriend_db_path);
        }
    }

    if enabled.contains(&ClientId::RooCode) {
        let local_path = ClientId::RooCode
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::RooCode,
            local_path,
        );

        let server_path = format!(
            "{}/.vscode-server/data/User/globalStorage/rooveterinaryinc.roo-cline/tasks",
            home_dir
        );
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::RooCode,
            server_path,
        );
    }

    if enabled.contains(&ClientId::KiloCode) {
        let local_path = ClientId::KiloCode
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::KiloCode,
            local_path,
        );

        let server_path = format!(
            "{}/.vscode-server/data/User/globalStorage/kilocode.kilo-code/tasks",
            home_dir
        );
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::KiloCode,
            server_path,
        );
    }

    if enabled.contains(&ClientId::Cline) {
        let local_path = ClientId::Cline
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::Cline,
            local_path,
        );

        for root in cline_additional_vscode_task_roots(home_dir, use_env_roots) {
            push_unique_scan_task(&mut tasks, &mut seen_scan_roots, ClientId::Cline, root);
        }

        for root in cline_cli_session_roots(home_dir, use_env_roots) {
            push_unique_scan_task_with_pattern(
                &mut tasks,
                &mut seen_scan_roots,
                ClientId::Cline,
                root,
                "cline-cli-messages",
            );
        }
    }

    if enabled.contains(&ClientId::DevinDesktop) {
        let local_path = ClientId::DevinDesktop
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::DevinDesktop,
            local_path,
        );

        for root in devin_desktop_additional_roots(home_dir, use_env_roots) {
            push_unique_scan_task(
                &mut tasks,
                &mut seen_scan_roots,
                ClientId::DevinDesktop,
                root,
            );
        }
    }

    if enabled.contains(&ClientId::Kilo) {
        let kilo_db_path = ClientId::Kilo
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        if std::path::Path::new(&kilo_db_path).exists() {
            result.kilo_db = Some(PathBuf::from(kilo_db_path));
        }
    }

    if enabled.contains(&ClientId::DevinCli) || enabled.contains(&ClientId::DevinDesktop) {
        let devin_db_path = ClientId::DevinCli
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        devin_cli_roots.push(PathBuf::from(devin_db_path));
        result.devin_dbs = discover_devin_cli_dbs(devin_cli_roots);
    }

    if enabled.contains(&ClientId::Hermes) {
        // Scan each candidate Hermes home (primary root plus native Windows
        // fallbacks). The first candidate whose `state.db` exists becomes the
        // primary `hermes_db`; every other default/profile db is collected as an
        // extra path. Profile-scoped homes contribute only their own profile
        // (see `discover_hermes_profile_state_dbs`).
        let mut extra_dbs: Vec<PathBuf> = Vec::new();
        for hermes_home in hermes_home_candidates(home_dir, use_env_roots) {
            let default_db = hermes_home.join("state.db");
            if default_db.is_file() {
                if result.hermes_db.is_none() {
                    result.hermes_db = Some(default_db);
                } else if result.hermes_db.as_ref() != Some(&default_db) {
                    extra_dbs.push(default_db);
                }
            }
            extra_dbs.extend(discover_hermes_profile_state_dbs(&hermes_home));
        }
        extra_dbs.sort_unstable();
        extra_dbs.dedup();
        result.get_mut(ClientId::Hermes).extend(extra_dbs);
    }

    if enabled.contains(&ClientId::Goose) {
        if use_env_roots {
            if let Ok(custom_root) = std::env::var("GOOSE_PATH_ROOT") {
                let trimmed = custom_root.trim();
                if !trimmed.is_empty() {
                    let custom_path = PathBuf::from(trimmed).join("data/sessions/sessions.db");
                    if custom_path.is_file() {
                        result.goose_db = Some(custom_path);
                    }
                }
            }
        }
        if result.goose_db.is_none() {
            let xdg_path = ClientId::Goose
                .data()
                .resolve_path_with_env_strategy(home_dir, use_env_roots);
            let xdg = PathBuf::from(xdg_path);
            if xdg.is_file() {
                result.goose_db = Some(xdg);
            }
        }
        if result.goose_db.is_none() {
            let macos_path = PathBuf::from(format!(
                "{}/Library/Application Support/goose/sessions/sessions.db",
                home_dir
            ));
            if macos_path.is_file() {
                result.goose_db = Some(macos_path);
            }
        }
        if result.goose_db.is_none() {
            let legacy_macos_path = PathBuf::from(format!(
                "{}/Library/Application Support/Block/goose/sessions/sessions.db",
                home_dir
            ));
            if legacy_macos_path.is_file() {
                result.goose_db = Some(legacy_macos_path);
            }
        }
        if result.goose_db.is_none() {
            let legacy_xdg_path = PathBuf::from(format!(
                "{}/.local/share/Block/goose/sessions/sessions.db",
                home_dir
            ));
            if legacy_xdg_path.is_file() {
                result.goose_db = Some(legacy_xdg_path);
            }
        }
    }

    if enabled.contains(&ClientId::Zed) {
        let zed_db_path = ClientId::Zed
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        let xdg = PathBuf::from(zed_db_path);
        if xdg.is_file() {
            result.zed_db = Some(xdg);
        }
        #[cfg(target_os = "macos")]
        if result.zed_db.is_none() {
            let macos_path = PathBuf::from(format!(
                "{}/Library/Application Support/Zed/threads/threads.db",
                home_dir
            ));
            if macos_path.is_file() {
                result.zed_db = Some(macos_path);
            }
        }
        if !use_env_roots && result.zed_db.is_none() {
            let windows_path = PathBuf::from(home_dir).join("AppData/Local/Zed/threads/threads.db");
            if windows_path.is_file() {
                result.zed_db = Some(windows_path);
            }
        }
        #[cfg(target_os = "windows")]
        if use_env_roots && result.zed_db.is_none() {
            if let Some(local_app_data) = dirs::data_local_dir() {
                let windows_path = local_app_data.join("Zed/threads/threads.db");
                if windows_path.is_file() {
                    result.zed_db = Some(windows_path);
                }
            }
        }
    }

    if enabled.contains(&ClientId::Crush) {
        result.crush_dbs = discover_crush_dbs(home_dir, use_env_roots);
    }

    if enabled.contains(&ClientId::Zcode) {
        let zcode_db_path = PathBuf::from(format!("{}/.zcode/cli/db/db.sqlite", home_dir));
        if zcode_db_path.is_file() {
            result.zcode_db = Some(zcode_db_path);
        }
    }

    if enabled.contains(&ClientId::Kiro) {
        let kiro_cli_path = ClientId::Kiro
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        push_unique_scan_task_with_pattern(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::Kiro,
            kiro_cli_path,
            "*.json",
        );

        for root in kiro_global_storage_roots(home_dir, use_env_roots) {
            push_unique_scan_task_with_pattern(
                &mut tasks,
                &mut seen_scan_roots,
                ClientId::Kiro,
                root,
                "kiro-globalstorage",
            );
        }

        // Kiro IDE (VS Code-based) writes per-workspace sessions under
        // ~/.kiro/sessions/<workspace>/sess_<uuid>/ (session.json + messages.jsonl),
        // NOT the ~/.kiro/sessions/cli/*.json layout the base client path targets.
        // Scan the sessions root and match session.json inside sess_* dirs. This
        // resolves via home_dir on Windows too (Kiro IDE uses ~/.kiro there).
        let kiro_ide_sessions_root = PathBuf::from(format!("{}/.kiro/sessions", home_dir));
        push_unique_scan_task_with_pattern(
            &mut tasks,
            &mut seen_scan_roots,
            ClientId::Kiro,
            kiro_ide_sessions_root,
            "kiro-ide-session",
        );

        let xdg_path = PathBuf::from(format!("{}/.local/share/kiro-cli/data.sqlite3", home_dir));
        if xdg_path.is_file() {
            result.kiro_db = Some(xdg_path);
        }
        if result.kiro_db.is_none() {
            let macos_path = PathBuf::from(format!(
                "{}/Library/Application Support/kiro-cli/data.sqlite3",
                home_dir
            ));
            if macos_path.is_file() {
                result.kiro_db = Some(macos_path);
            }
        }
    }

    if enabled.contains(&ClientId::Codebuff) {
        // Codebuff persists per-channel chat history under
        // ~/.config/<channel>/projects/<project>/chats/<chatId>/chat-messages.json.
        // When CODEBUFF_DATA_DIR is set to a non-empty value (via
        // PathRoot::EnvVar), scan only that root; otherwise — including when
        // the env var is unset *or* set to an empty/whitespace string — walk
        // the three known channel roots:
        //   - ~/.config/manicode (primary / legacy name — Codebuff was "Manicode")
        //   - ~/.config/manicode-dev
        //   - ~/.config/manicode-staging
        let trimmed_override = if use_env_roots {
            std::env::var("CODEBUFF_DATA_DIR")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        } else {
            None
        };

        let mut codebuff_roots: Vec<String> = Vec::new();
        if let Some(root) = trimmed_override {
            codebuff_roots.push(format!("{}/projects", root.trim_end_matches('/')));
        } else {
            let config_dir = format!("{}/.config", home_dir);
            for channel in ["manicode", "manicode-dev", "manicode-staging"] {
                codebuff_roots.push(format!("{}/{}/projects", config_dir, channel));
            }
        }

        for root in codebuff_roots {
            push_unique_scan_task(&mut tasks, &mut seen_scan_roots, ClientId::Codebuff, root);
        }
    }

    if enabled.contains(&ClientId::Gjc) {
        // gajae-code (gjc) persists sessions as
        // <agent-dir>/sessions/<project-slug>/*.jsonl, with depth-2 per-pass
        // sub-agent children <slug>/<session>/N-*.jsonl. scan_directory's
        // WalkDir + "*.jsonl" suffix match covers both depths.
        //
        // The agent dir is resolved under several env overrides gjc honors,
        // plus the Linux/macOS $XDG_DATA_HOME/gjc redirect (which FLATTENS the
        // `agent/` segment to `<xdg>/gjc/sessions`). Binding note N4: push
        // EVERY resolved root that exists (NOT first-match), letting the
        // cross-directory file dedup collapse overlap — first-match could read
        // a wrong empty root when the XDG redirect is the populated one.
        // Everything is gated on use_env_roots so `--home` disables overrides.
        let mut gjc_roots: Vec<PathBuf> = Vec::new();

        // (1) GJC_CODING_AGENT_DIR/sessions (the PathRoot::EnvVar default also
        // resolves here; existence-gated push + dedup keep it single).
        let agent_dir_root = ClientId::Gjc
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots);
        gjc_roots.push(PathBuf::from(agent_dir_root));

        if use_env_roots {
            // (2) GJC_CONFIG_DIR / PI_CONFIG_DIR joined with agent/sessions.
            for var in ["GJC_CONFIG_DIR", "PI_CONFIG_DIR"] {
                if let Ok(config_dir) = std::env::var(var) {
                    let trimmed = config_dir.trim();
                    if !trimmed.is_empty() {
                        gjc_roots.push(
                            PathBuf::from(trimmed.trim_end_matches('/')).join("agent/sessions"),
                        );
                    }
                }
            }

            // (3) $XDG_DATA_HOME/gjc/sessions — the redirect flattens `agent/`.
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
                let trimmed = xdg_data.trim();
                if !trimmed.is_empty() {
                    gjc_roots
                        .push(PathBuf::from(trimmed.trim_end_matches('/')).join("gjc/sessions"));
                }
            }
        }

        // (4) ~/.gjc/agent/sessions home fallback (always available).
        gjc_roots.push(PathBuf::from(format!("{}/.gjc/agent/sessions", home_dir)));

        for root in gjc_roots {
            if root.exists() {
                push_unique_scan_task(&mut tasks, &mut seen_scan_roots, ClientId::Gjc, root);
            }
        }
    }

    // Execute scans in parallel
    let scan_results: Vec<(ClientId, Vec<PathBuf>)> = tasks
        .into_par_iter()
        .map(|(client_id, path, pattern)| {
            let files = scan_directory(&path, pattern);
            (client_id, files)
        })
        .collect();

    // Aggregate results, deduplicating file paths across overlapping directories
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for (client_id, files) in scan_results {
        for file in files {
            if seen.insert(file.clone()) {
                result.get_mut(client_id).push(file);
            }
        }
    }

    if enabled.contains(&ClientId::Copilot) {
        let desktop_db = PathBuf::from(format!("{}/.copilot/data.db", home_dir));
        if desktop_db.is_file() {
            result.copilot_desktop_db = Some(desktop_db);
        }

        result.copilot_vscode_sessions = discover_copilot_vscode_sessions(home_dir, use_env_roots);

        if let Some(path) = copilot_exporter_path_with_env_strategy(use_env_roots) {
            if path.is_file() && seen.insert(path.clone()) {
                let copilot_files = result.get_mut(ClientId::Copilot);
                copilot_files.push(path);
                copilot_files.sort_unstable();
            }
        }
    }

    result
}

pub fn scan_all_clients(home_dir: &str, clients: &[String]) -> ScanResult {
    scan_all_clients_with_env_strategy(home_dir, clients, true)
}
