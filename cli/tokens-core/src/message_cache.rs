use crate::clients::ClientId;
use crate::sessions::codex::CodexParseState;
use crate::UnifiedMessage;
use bincode::Options;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

// CACHE_FORMAT_VERSION changes only when the serialized storage layout or a
// cross-client type such as UnifiedMessage changes incompatibly. Parser-only
// changes belong in parser_version() so one client cannot evict every other
// client's cached transcripts.
// 2: Related-file fingerprints now retain their paths and whether they were
// absent when cached. Claude sidechain parent candidates can therefore be
// revalidated without reparsing the sidechain on every warm scan, while a
// later-created parent transcript still invalidates the entry.
// 3: UnifiedMessage gained agent_run_id for the opt-in subagent breakdown.
// 4: UnifiedMessage gained session_title, changing the bincode payload layout.
// 5: agent_run_id removed with the subagent breakdown, shrinking the payload.
// 6: UnifiedMessage gained timestamp_provenance for trustworthy hourly facts.
// Old shards must read as Stale (silent rebuild), not Invalid (corruption
// warning), so the format version moves with the struct.
const CACHE_FORMAT_VERSION: u32 = 6;
// V2 intentionally starts cold and leaves source-message-cache.bin untouched:
// the monolith did not record a trustworthy parser owner for migration.
const CACHE_SHARD_DIRNAME: &str = "source-message-cache-v2";
const CACHE_LOCK_FILENAME: &str = "source-message-cache.lock";
const CACHE_GENERATION_RECORD_BYTES: usize = 16;
const CACHE_SHARD_COUNT: usize = 256;
const MAX_CACHE_SHARD_BYTES: u64 = 256 * 1024 * 1024;
const FINGERPRINT_SAMPLE_BYTES: usize = 4096;
const FINGERPRINT_SAMPLE_POINTS: usize = 5;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

fn cache_dir() -> Option<PathBuf> {
    if crate::paths::is_config_dir_overridden()
        || dirs::config_dir().is_some()
        || cfg!(target_os = "macos") && dirs::home_dir().is_some()
    {
        Some(crate::paths::get_cache_dir())
    } else {
        fallback_cache_dir()
    }
}

fn cache_shard_dir() -> Option<PathBuf> {
    Some(cache_dir()?.join(CACHE_SHARD_DIRNAME))
}

fn cache_lock_path() -> Option<PathBuf> {
    Some(cache_dir()?.join(CACHE_LOCK_FILENAME))
}

fn fallback_cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|path| path.join("tokens"))
        .or_else(user_scoped_temp_dir)
}

#[cfg(unix)]
fn user_scoped_temp_dir() -> Option<PathBuf> {
    let uid = unsafe { libc::geteuid() };
    Some(std::env::temp_dir().join(format!("tokens-uid-{uid}")))
}

#[cfg(not(unix))]
fn user_scoped_temp_dir() -> Option<PathBuf> {
    std::env::var_os("USERNAME")
        .or_else(|| std::env::var_os("USER"))
        .map(|user| {
            let mut path = std::env::temp_dir();
            path.push(format!("tokens-user-{}", user.to_string_lossy()));
            path
        })
}

fn ensure_cache_dir(dir: &Path) -> std::io::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(dir) {
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(std::io::Error::other(
                "cache directory is not a real directory",
            ));
        }
    }
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn open_cache_lock(path: &Path) -> std::io::Result<File> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("source message cache lock has no parent"))?;
    ensure_cache_dir(parent)?;
    #[cfg(unix)]
    let lock = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?
    };
    #[cfg(not(unix))]
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(lock)
}

fn read_cache_generation(lock: &mut File) -> std::io::Result<u64> {
    lock.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    lock.read_to_end(&mut bytes)?;
    if bytes.is_empty() {
        return Ok(0);
    }
    if bytes.len() == std::mem::size_of::<u64>() {
        return Ok(u64::from_le_bytes(
            bytes.try_into().expect("checked length"),
        ));
    }

    let mut generation = None;
    for record in bytes.chunks_exact(CACHE_GENERATION_RECORD_BYTES) {
        let value = u64::from_le_bytes(record[..8].try_into().expect("fixed record"));
        let check = u64::from_le_bytes(record[8..].try_into().expect("fixed record"));
        if check == !value {
            generation = Some(value);
        }
    }
    generation.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid source message cache generation",
        )
    })
}

fn write_cache_generation(lock: &mut File, generation: u64) -> std::io::Result<()> {
    let end = lock.seek(SeekFrom::End(0))?;
    let remainder = end % CACHE_GENERATION_RECORD_BYTES as u64;
    if remainder != 0 {
        let padding = CACHE_GENERATION_RECORD_BYTES as u64 - remainder;
        lock.write_all(&vec![0; padding as usize])?;
    }
    lock.write_all(&generation.to_le_bytes())?;
    lock.write_all(&(!generation).to_le_bytes())?;
    lock.sync_all()
}

fn warn_cache_failure_once(context: &'static str, path: &Path, error: &impl std::fmt::Display) {
    tracing::warn!(path = %path.display(), %error, %context, "source message cache failure");

    // The submit path does not install a tracing subscriber, so surface
    // persistence failures directly, once per process, rather than letting a
    // permanently cold cache fail silently.
    static WARNED_CONTEXTS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let warned = WARNED_CONTEXTS.get_or_init(|| Mutex::new(HashSet::new()));
    if warned.lock().is_ok_and(|mut warned| warned.insert(context)) {
        eprintln!("tokens: warning: {context} ({}): {error}", path.display());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct FileSampleHash {
    pub offset: u64,
    pub len: u64,
    pub hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceFingerprint {
    pub size: u64,
    pub modified_ns: u64,
    pub sample_hashes: Vec<FileSampleHash>,
    pub content_hash: [u8; 32],
    pub related_files: Vec<RelatedFileFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct RelatedFileFingerprint {
    pub suffix: String,
    pub path: CachedPath,
    pub exists: bool,
    pub size: u64,
    pub modified_ns: u64,
    pub sample_hashes: Vec<FileSampleHash>,
    pub content_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FingerprintStatus {
    /// Size and nanosecond mtime still match for the source and every parser
    /// sidecar, and their bounded samples still match. No full-file SHA-256 was
    /// computed, so a warm scan reads at most 20 KiB per watched file.
    Unchanged,
    /// Metadata changed, so a complete fingerprint was rebuilt to distinguish
    /// a real content change from a metadata-only touch.
    Changed(SourceFingerprint),
}

impl SourceFingerprint {
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        Self::from_path_with_related(path, std::iter::empty())
    }

    /// Fingerprint for a Jcode session snapshot and its append-only journal
    /// sidecar. Jcode persists recent changes in `session_*.journal.jsonl`
    /// until the next checkpoint rewrites the snapshot, so the source-message
    /// cache must invalidate when either file changes.
    /// Fingerprint for a Roo-family task (`ui_messages.json`) and its sibling
    /// `api_conversation_history.json`. `parse_roo_kilo_file` reads the history
    /// sibling for the model and agent, so a history-only rewrite (the UI file
    /// unchanged) must still invalidate the cache or reports keep stale
    /// model/agent/pricing.
    /// Fingerprint for a Claude Code JSONL file that may have a sibling `.meta.json`
    /// sidecar. When the sidecar appears or changes (e.g. after a Claude Code upgrade),
    /// the fingerprint changes and the cache invalidates.
    /// Fingerprint for a Grok `updates.jsonl` session and every sibling read by
    /// its parser for rollup and session metadata.
    /// Fingerprint for a Kiro source file. IDE sessions consume a sibling
    /// `messages.jsonl`, while CLI `*.json` headers consume same-stem `*.jsonl`.
    /// Global-storage and `.chat` snapshots are self-contained.
    pub(crate) fn check_path(path: &Path, cached: Option<&Self>) -> Option<FingerprintStatus> {
        Self::check_path_with_related(path, std::iter::empty(), cached)
    }

    /// Check a non-Codex source without rebuilding its write-only whole-file
    /// hash when metadata or samples changed. Codex uses `check_path` because
    /// its incremental resume state compares the full content hash; generic
    /// parsers only need the bounded samples for invalidation.
    pub(crate) fn check_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_path_with_related_mode(
            path,
            std::iter::empty(),
            cached,
            ContentHashMode::SamplesOnly,
        )
    }

    pub(crate) fn check_sqlite_path(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        let related_paths = ["-wal"]
            .into_iter()
            .map(|suffix| (suffix.to_string(), append_path_suffix(path, suffix)));
        // SQLite databases can be tens of GB; skip the whole-file content hash
        // (size + mtime + samples detect changes, and no SQLite source reads
        // content_hash). See ContentHashMode.
        Self::check_path_with_related_mode(
            path,
            related_paths,
            cached,
            ContentHashMode::SamplesOnly,
        )
    }

    /// Fingerprint a Devin Desktop ACP stream together with every CLI database
    /// that can resolve its title to a model/session id. A database or WAL
    /// change can alter a cached Desktop message even when the NDJSON stream is
    /// untouched, so the lookup inputs must be watched as related files.
    pub(crate) fn check_devin_desktop_path_samples_only(
        path: &Path,
        devin_db_paths: &[PathBuf],
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        let related_paths = devin_db_paths
            .iter()
            .enumerate()
            .flat_map(|(index, db_path)| {
                let prefix = format!("devin-cli-db-{index}");
                [
                    (prefix.clone(), db_path.clone()),
                    (format!("{prefix}-wal"), append_path_suffix(db_path, "-wal")),
                ]
            });
        Self::check_path_with_related_mode(
            path,
            related_paths,
            cached,
            ContentHashMode::SamplesOnly,
        )
    }

    pub(crate) fn check_jcode_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_jcode_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_jcode_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let related_paths = std::iter::once((
            ".journal.jsonl".to_string(),
            crate::sessions::jcode::jcode_journal_path(path),
        ));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_roo_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_roo_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_roo_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let history = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("api_conversation_history.json");
        let related_paths = std::iter::once(("api_conversation_history.json".to_string(), history));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_claude_code_path_with_home_samples_only(
        path: &Path,
        cached: Option<&Self>,
        home_dir: Option<&Path>,
    ) -> Option<FingerprintStatus> {
        Self::check_claude_code_path_with_home_mode(
            path,
            cached,
            home_dir,
            ContentHashMode::SamplesOnly,
        )
    }

    fn check_claude_code_path_with_home_mode(
        path: &Path,
        cached: Option<&Self>,
        home_dir: Option<&Path>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let mut related = Vec::new();

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let meta_filename = format!("{}.meta.json", stem);
            related.push((".meta.json".to_string(), path.with_file_name(meta_filename)));
        }

        if let Some(variant_path) = crate::cc_mirror::variant_file_for_session_path(path, home_dir)
        {
            related.push(("cc-mirror/variant.json".to_string(), variant_path));
        }

        let primary_matches =
            cached.and_then(|fingerprint| primary_fingerprint_matches(path, fingerprint));
        let parent_paths = cached
            .filter(|_| primary_matches == Some(true))
            .map(cached_claude_parent_paths)
            .unwrap_or_else(|| {
                crate::sessions::claudecode::parent_session_paths_for_cache(path)
                    .into_iter()
                    .enumerate()
                    .map(|(index, parent_path)| {
                        (format!("parent-session-{index}.jsonl"), parent_path)
                    })
                    .collect()
            });
        related.extend(parent_paths);

        Self::check_path_with_related_mode_and_primary(path, related, cached, mode, primary_matches)
    }

    pub(crate) fn check_grok_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_grok_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_grok_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let related_paths = ["signals.json", "summary.json", "events.jsonl"]
            .into_iter()
            .map(|name| (name.to_string(), parent.join(name)));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_kiro_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_kiro_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_kiro_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let Some(messages) = crate::sessions::kiro::kiro_related_messages_path(path) else {
            return Self::check_path_with_related_mode(path, std::iter::empty(), cached, mode);
        };
        let related_paths = std::iter::once(("messages.jsonl".to_string(), messages));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_droid_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_droid_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_droid_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        let Some(jsonl) = crate::sessions::droid::droid_jsonl_path(path) else {
            return Self::check_path_with_related_mode(path, std::iter::empty(), cached, mode);
        };
        let related_paths = std::iter::once(("session.jsonl".to_string(), jsonl));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    pub(crate) fn check_kimi_path_samples_only(
        path: &Path,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus> {
        Self::check_kimi_path_with_mode(path, cached, ContentHashMode::SamplesOnly)
    }

    fn check_kimi_path_with_mode(
        path: &Path,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus> {
        if crate::sessions::kimi::is_kimi_code_path(path) {
            return Self::check_path_with_related_mode(path, std::iter::empty(), cached, mode);
        }
        let Some(config) = crate::sessions::kimi::kimi_config_path(path) else {
            return Self::check_path_with_related_mode(path, std::iter::empty(), cached, mode);
        };
        let related_paths = std::iter::once(("config.json".to_string(), config));
        Self::check_path_with_related_mode(path, related_paths, cached, mode)
    }

    fn check_path_with_related<I>(
        path: &Path,
        related_paths: I,
        cached: Option<&Self>,
    ) -> Option<FingerprintStatus>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        Self::check_path_with_related_mode(path, related_paths, cached, ContentHashMode::Full)
    }

    fn check_path_with_related_mode<I>(
        path: &Path,
        related_paths: I,
        cached: Option<&Self>,
        mode: ContentHashMode,
    ) -> Option<FingerprintStatus>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        Self::check_path_with_related_mode_and_primary(path, related_paths, cached, mode, None)
    }

    fn check_path_with_related_mode_and_primary<I>(
        path: &Path,
        related_paths: I,
        cached: Option<&Self>,
        mode: ContentHashMode,
        primary_matches: Option<bool>,
    ) -> Option<FingerprintStatus>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        let related_paths: Vec<(String, PathBuf)> = related_paths.into_iter().collect();
        let cache_hit = cached.is_some_and(|fingerprint| {
            primary_matches
                .unwrap_or_else(|| primary_fingerprint_matches(path, fingerprint).unwrap_or(false))
                && related_fingerprint_metadata_matches(&related_paths, fingerprint)
                    .unwrap_or(false)
        });
        if cache_hit {
            return Some(FingerprintStatus::Unchanged);
        }

        Self::from_path_with_related_mode(path, related_paths, mode).map(FingerprintStatus::Changed)
    }

    fn from_path_with_related<I>(path: &Path, related_paths: I) -> Option<Self>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        Self::from_path_with_related_mode(path, related_paths, ContentHashMode::Full)
    }

    fn from_path_with_related_mode<I>(
        path: &Path,
        related_paths: I,
        mode: ContentHashMode,
    ) -> Option<Self>
    where
        I: IntoIterator<Item = (String, PathBuf)>,
    {
        let (size, modified_ns, sample_hashes, content_hash) = file_fingerprint_parts(path, mode)?;
        let mut related_files: Vec<RelatedFileFingerprint> = related_paths
            .into_iter()
            .map(|(suffix, related_path)| {
                RelatedFileFingerprint::from_path(suffix, &related_path, mode)
            })
            .collect::<Option<_>>()?;
        related_files.sort_by(|left, right| left.suffix.cmp(&right.suffix));

        Some(Self {
            size,
            modified_ns,
            sample_hashes,
            content_hash,
            related_files,
        })
    }
}

impl RelatedFileFingerprint {
    fn from_path(suffix: String, path: &Path, mode: ContentHashMode) -> Option<Self> {
        let cached_path = CachedPath::from_path(path);
        match path.metadata() {
            Ok(_) => {
                let (size, modified_ns, sample_hashes, content_hash) =
                    file_fingerprint_parts(path, mode)?;
                Some(Self {
                    suffix,
                    path: cached_path,
                    exists: true,
                    size,
                    modified_ns,
                    sample_hashes,
                    content_hash,
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(Self {
                suffix,
                path: cached_path,
                exists: false,
                size: 0,
                modified_ns: 0,
                sample_hashes: Vec::new(),
                content_hash: [0; 32],
            }),
            Err(_) => None,
        }
    }
}

fn cached_claude_parent_paths(cached: &SourceFingerprint) -> Vec<(String, PathBuf)> {
    cached
        .related_files
        .iter()
        .filter(|related| related.suffix.starts_with("parent-session-"))
        .map(|related| (related.suffix.clone(), related.path.to_path_buf()))
        .collect()
}

fn primary_fingerprint_matches(path: &Path, cached: &SourceFingerprint) -> Option<bool> {
    let (size, modified_ns) = metadata_signature(path).ok()?;
    if size != cached.size || modified_ns != cached.modified_ns {
        return Some(false);
    }
    Some(compute_sample_hashes(path, size)? == cached.sample_hashes)
}

fn metadata_signature(path: &Path) -> std::io::Result<(u64, u64)> {
    let metadata = path.metadata()?;
    let modified_ns = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_nanos() as u64;
    Ok((metadata.len(), modified_ns))
}

fn related_fingerprint_metadata_matches(
    related_paths: &[(String, PathBuf)],
    cached: &SourceFingerprint,
) -> Option<bool> {
    if cached.related_files.len() != related_paths.len() {
        return Some(false);
    }

    for (suffix, related_path) in related_paths {
        let Some(related) = cached
            .related_files
            .iter()
            .find(|related| related.suffix == *suffix)
        else {
            return Some(false);
        };
        if related.path != CachedPath::from_path(related_path) {
            return Some(false);
        }
        match metadata_signature(related_path) {
            Ok((size, modified_ns)) => {
                if !related.exists || related.size != size || related.modified_ns != modified_ns {
                    return Some(false);
                }
                if compute_sample_hashes(related_path, size)? != related.sample_hashes {
                    return Some(false);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if related.exists {
                    return Some(false);
                }
            }
            Err(_) => return None,
        }
    }

    Some(true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CodexIncrementalCache {
    pub state: CodexParseState,
    pub consumed_offset: u64,
    pub ends_with_newline: bool,
    pub prefix_hash: [u8; 32],
}

#[cfg(unix)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct CachedPath(Vec<u8>);

#[cfg(unix)]
impl CachedPath {
    pub(crate) fn from_path(path: &Path) -> Self {
        use std::os::unix::ffi::OsStrExt;

        Self(path.as_os_str().as_bytes().to_vec())
    }

    pub(crate) fn to_path_buf(&self) -> PathBuf {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        PathBuf::from(OsString::from_vec(self.0.clone()))
    }

    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(&self.0);
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct CachedPath(Vec<u16>);

#[cfg(windows)]
impl CachedPath {
    pub(crate) fn from_path(path: &Path) -> Self {
        use std::os::windows::ffi::OsStrExt;

        Self(path.as_os_str().encode_wide().collect())
    }

    pub(crate) fn to_path_buf(&self) -> PathBuf {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        PathBuf::from(OsString::from_wide(&self.0))
    }

    fn update_digest(&self, hasher: &mut Sha256) {
        for code_unit in &self.0 {
            hasher.update(code_unit.to_le_bytes());
        }
    }
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct CachedPath(String);

#[cfg(not(any(unix, windows)))]
impl CachedPath {
    pub(crate) fn from_path(path: &Path) -> Self {
        Self(path.to_string_lossy().into_owned())
    }

    pub(crate) fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.0)
    }

    fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(self.0.as_bytes());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CacheIdentity {
    namespace: &'static str,
    parser_version: u32,
}

impl CacheIdentity {
    pub(crate) fn for_client(client: ClientId) -> Self {
        Self {
            namespace: client.as_str(),
            parser_version: parser_version(client),
        }
    }

    pub(crate) const fn synthetic() -> Self {
        Self {
            namespace: "synthetic",
            parser_version: 1,
        }
    }

    fn current_for_namespace(namespace: &str) -> Option<Self> {
        if namespace == "synthetic" {
            return Some(Self::synthetic());
        }
        ClientId::from_str(namespace).map(Self::for_client)
    }

    fn all() -> impl Iterator<Item = Self> {
        ClientId::iter()
            .map(Self::for_client)
            .chain(std::iter::once(Self::synthetic()))
    }
}

fn parser_version(client: ClientId) -> u32 {
    match client {
        // These clients accumulated parser-only invalidations under the old
        // global schema. Their independent counters start from those histories
        // so future changes have an obvious local version to increment.
        ClientId::Codex => 6,
        // v4->v5: jcode's assistant-message timestamp is now back-calculated
        // to the turn start (timestamp - tool_duration_ms) instead of using
        // the recorded (end-anchored) timestamp directly. Follow-up to #890.
        // v5->v6: OpenAI-style Jcode usage now removes cache-read overlap from
        // input_tokens before pricing and aggregation.
        // v6->v7: snapshot and journal message arrays are now parsed
        // leniently (a single wrong-typed token_usage no longer drops the
        // whole session or its journal line), and a journal replay of an
        // already-seen user message id no longer re-arms pending_turn_start
        // and mints a spurious turn.
        ClientId::Jcode => 7,
        // v5->v6: merge same-dedup-key Copilot spans before emitting messages.
        // v6->v7: all-zero trace/span ids (the W3C sentinel for "no recording
        // span context") are now treated as absent instead of as a real,
        // shared identity, and a valid span_id alone (no trace_id) is now a
        // stable dedup key instead of falling through to the line-index key.
        // v7->v8: stabilize duplicate agent attribution and partial timing boundaries.
        ClientId::Copilot => 8,
        // Pi subagent sessions now derive agent attribution from session_info
        // names; version-1 caches carry those messages without agent metadata.
        ClientId::Pi => 2,
        // Devin CLI v1 could stop at a malformed chat_message. v2->v3:
        // message timestamp is now back-calculated to the turn start
        // (created_at - total_time_ms) instead of the recorded (end-anchored)
        // created_at. Follow-up to #890.
        ClientId::DevinCli => 3,
        // Desktop v1 parsed a non-ACP shape and did not track its CLI title
        // lookup; its timestamp handling is unaffected by the #890 follow-up.
        ClientId::DevinDesktop => 2,
        // v2->v3: Claude session labels prefer the latest valid JSONL `cwd`
        // final folder component while keeping the path-derived workspace key;
        // wrong-typed cwd metadata is ignored without rejecting usage entries.
        ClientId::Claude => 3,
        // Junie's usage-event timestamp is now back-calculated to the call
        // start (timestampMs - usage.time) instead of the recorded
        // (end-anchored) timestampMs. Follow-up to #890.
        ClientId::Junie => 2,
        // zcode's model_usage timestamp now prefers `started_at` over
        // `completed_at`. Follow-up to #890. v2->v3: rows with a NULL
        // `started_at` now back-calculate `completed_at - duration_ms`
        // instead of staying end-anchored at `completed_at`, and
        // `is_turn_start` is now assigned to the earliest-STARTED request
        // per turn instead of the first one seen in completed_at order.
        // Second-round follow-up to #890.
        ClientId::Zcode => 3,
        // opencodereview's llm_response timestamp is now back-calculated to
        // the call start (timestamp - duration_ms) instead of the recorded
        // (end-anchored) timestamp. Follow-up to #890.
        ClientId::OpenCodeReview => 2,
        // Kiro's structured messages.jsonl turns now back-calculate the
        // start anchor from `turn_end - elapsedTime` when the user prompt's
        // own timestamp is missing/unparseable, instead of falling through
        // to the (end-anchored) turn_end timestamp. Second-round follow-up
        // to #890.
        ClientId::Kiro => 2,
        // v1->v2: check each token bucket independently when deciding whether
        // a usage record is empty, avoiding an overflowing sum.
        // v2->v3: correlate Kimi Code usage aliases with preceding request
        // records to restore concrete model and provider identity.
        ClientId::Kimi => 3,
        _ => 1,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    namespace: String,
    path: CachedPath,
}

impl CacheKey {
    fn new(identity: CacheIdentity, path: &Path) -> Self {
        Self {
            namespace: identity.namespace.to_string(),
            path: CachedPath::from_path(path),
        }
    }

    fn from_entry(entry: &CachedSourceEntry) -> Self {
        Self {
            namespace: entry.parser_namespace.clone(),
            path: entry.path.clone(),
        }
    }

    fn shard(&self) -> CacheShardKey {
        let mut hasher = Sha256::new();
        hasher.update(self.namespace.as_bytes());
        hasher.update([0]);
        self.path.update_digest(&mut hasher);
        let digest = hasher.finalize();
        CacheShardKey {
            namespace: self.namespace.clone(),
            index: usize::from(digest[0]) % CACHE_SHARD_COUNT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheShardKey {
    namespace: String,
    index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedSourceEntry {
    parser_namespace: String,
    parser_version: u32,
    pub path: CachedPath,
    pub fingerprint: SourceFingerprint,
    pub messages: Vec<UnifiedMessage>,
    pub fallback_timestamp_indices: Vec<usize>,
    pub codex_incremental: Option<CodexIncrementalCache>,
}

impl CachedSourceEntry {
    pub(crate) fn new(
        identity: CacheIdentity,
        path: &Path,
        fingerprint: SourceFingerprint,
        messages: Vec<UnifiedMessage>,
        fallback_timestamp_indices: Vec<usize>,
        codex_incremental: Option<CodexIncrementalCache>,
    ) -> Self {
        Self {
            parser_namespace: identity.namespace.to_string(),
            parser_version: identity.parser_version,
            path: CachedPath::from_path(path),
            fingerprint,
            messages,
            fallback_timestamp_indices,
            codex_incremental,
        }
    }

    fn identity_is_current(&self) -> bool {
        CacheIdentity::current_for_namespace(&self.parser_namespace)
            .is_some_and(|identity| identity.parser_version == self.parser_version)
    }
}

/// The envelope is deliberately independent from CachedSourceEntry's binary
/// layout. A parser version can therefore be checked before its payload is
/// deserialized, so (for example) a CodexParseState layout change cannot make
/// Claude's independently sharded cache unreadable.
#[derive(Debug, Serialize, Deserialize)]
struct CachedShardEnvelope {
    format_version: u32,
    parser_namespace: String,
    parser_version: u32,
    payload: Vec<u8>,
}

#[derive(Debug, Clone)]
enum DeletionReason {
    Invalidated(SourceFingerprint),
    Missing,
}

#[derive(Default)]
pub(crate) struct SourceMessageCache {
    pub entries: HashMap<CacheKey, CachedSourceEntry>,
    generation: u64,
    lock_file: Option<File>,
    dirty: bool,
    dirty_keys: HashSet<CacheKey>,
    deleted_keys: HashMap<CacheKey, DeletionReason>,
    rewrite_shards: HashSet<CacheShardKey>,
}

impl SourceMessageCache {
    pub(crate) fn load() -> Self {
        let (Some(shard_root), Some(lock_path)) = (cache_shard_dir(), cache_lock_path()) else {
            return Self::default();
        };
        Self::load_from_paths(&shard_root, &lock_path)
    }

    fn load_from_paths(shard_root: &Path, lock_path: &Path) -> Self {
        let mut lock_file = match open_cache_lock(lock_path) {
            Ok(file) => file,
            Err(error) => {
                warn_cache_failure_once(
                    "source message cache lock is unavailable",
                    lock_path,
                    &error,
                );
                return Self::default();
            }
        };
        if let Err(error) = fs2::FileExt::lock_shared(&lock_file) {
            warn_cache_failure_once("source message cache lock failed", lock_path, &error);
            return Self::default();
        }
        let generation = match read_cache_generation(&mut lock_file) {
            Ok(generation) => generation,
            Err(error) => {
                warn_cache_failure_once(
                    "source message cache generation is invalid",
                    lock_path,
                    &error,
                );
                return Self::default();
            }
        };
        if let Err(error) = ensure_cache_dir(shard_root) {
            warn_cache_failure_once(
                "source message cache directory is unavailable",
                shard_root,
                &error,
            );
            return Self::default();
        }

        let mut cache = Self {
            generation,
            lock_file: Some(lock_file),
            ..Self::default()
        };
        for identity in CacheIdentity::all() {
            let parser_dir = shard_root.join(identity.namespace);
            let read_dir = match fs::read_dir(&parser_dir) {
                Ok(read_dir) => read_dir,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    warn_cache_failure_once(
                        "source message cache parser directory is unreadable",
                        &parser_dir,
                        &error,
                    );
                    continue;
                }
            };

            for dir_entry in read_dir.filter_map(Result::ok) {
                let Some(index) = parse_shard_filename(&dir_entry.file_name()) else {
                    continue;
                };
                let shard_key = CacheShardKey {
                    namespace: identity.namespace.to_string(),
                    index,
                };
                let path = dir_entry.path();
                match read_shard(&path, identity) {
                    ShardReadStatus::Loaded(entries) => {
                        for entry in entries {
                            let key = CacheKey::from_entry(&entry);
                            if key.shard() == shard_key && entry.identity_is_current() {
                                cache.entries.insert(key, entry);
                            } else {
                                cache.rewrite_shards.insert(shard_key.clone());
                            }
                        }
                    }
                    ShardReadStatus::Missing => {}
                    ShardReadStatus::Stale => {
                        cache.rewrite_shards.insert(shard_key);
                    }
                    ShardReadStatus::Invalid(error) => {
                        warn_cache_failure_once(
                            "source message cache shard is invalid",
                            &path,
                            &error,
                        );
                        cache.rewrite_shards.insert(shard_key);
                    }
                }
            }
        }

        cache.dirty = !cache.rewrite_shards.is_empty();
        cache
    }

    pub(crate) fn insert(&mut self, entry: CachedSourceEntry) {
        let key = CacheKey::from_entry(&entry);
        self.entries.insert(key.clone(), entry);
        self.deleted_keys.remove(&key);
        self.dirty_keys.insert(key);
        self.dirty = true;
    }

    pub(crate) fn get(&self, identity: CacheIdentity, path: &Path) -> Option<&CachedSourceEntry> {
        let key = CacheKey::new(identity, path);
        self.entries.get(&key).filter(|entry| {
            entry.parser_namespace == identity.namespace
                && entry.parser_version == identity.parser_version
        })
    }

    pub(crate) fn remove(&mut self, identity: CacheIdentity, path: &Path) {
        let key = CacheKey::new(identity, path);
        if let Some(entry) = self.entries.remove(&key) {
            self.dirty_keys.remove(&key);
            self.deleted_keys
                .insert(key, DeletionReason::Invalidated(entry.fingerprint));
            self.dirty = true;
        }
    }

    pub(crate) fn prune_missing_files(&mut self) {
        let removed_keys: Vec<CacheKey> = self
            .entries
            .keys()
            .filter(|key| !key.path.to_path_buf().exists())
            .cloned()
            .collect();

        for key in removed_keys {
            self.entries.remove(&key);
            self.dirty_keys.remove(&key);
            self.deleted_keys.insert(key, DeletionReason::Missing);
            self.dirty = true;
        }
    }

    pub(crate) fn save_if_dirty(&mut self) {
        self.save_if_dirty_with_limit(MAX_CACHE_SHARD_BYTES);
    }

    fn save_if_dirty_with_limit(&mut self, max_shard_bytes: u64) {
        let (Some(shard_root), Some(lock_path)) = (cache_shard_dir(), cache_lock_path()) else {
            return;
        };
        self.save_if_dirty_with_limit_at(max_shard_bytes, &shard_root, &lock_path);
    }

    fn save_if_dirty_with_limit_at(
        &mut self,
        max_shard_bytes: u64,
        shard_root: &Path,
        lock_path: &Path,
    ) {
        let mut lock_file = match self.lock_file.take() {
            Some(file) => {
                if let Err(error) = fs2::FileExt::unlock(&file) {
                    warn_cache_failure_once(
                        "source message cache shared lock could not be released",
                        lock_path,
                        &error,
                    );
                    return;
                }
                file
            }
            None => match open_cache_lock(lock_path) {
                Ok(file) => file,
                Err(error) => {
                    warn_cache_failure_once(
                        "source message cache lock is unavailable",
                        lock_path,
                        &error,
                    );
                    return;
                }
            },
        };
        if !self.dirty {
            return;
        }
        if let Err(error) = fs2::FileExt::lock_exclusive(&lock_file) {
            warn_cache_failure_once("source message cache lock failed", lock_path, &error);
            return;
        }
        let generation = match read_cache_generation(&mut lock_file) {
            Ok(generation) => generation,
            Err(error) => {
                warn_cache_failure_once(
                    "source message cache generation is invalid",
                    lock_path,
                    &error,
                );
                return;
            }
        };
        if generation != self.generation {
            *self = Self {
                generation,
                ..Self::default()
            };
            return;
        }
        if let Err(error) = ensure_cache_dir(shard_root) {
            warn_cache_failure_once(
                "source message cache directory is unavailable",
                shard_root,
                &error,
            );
            return;
        }

        // Bucket dirty and deleted keys by shard up front. CacheKey::shard()
        // computes a SHA-256 digest, so grouping once keeps hashing at O(keys).
        // The previous per-shard `.filter(|k| k.shard() == shard_key)` recomputed
        // that digest for every key on every shard — O(shards * keys) — which
        // dominated cold-cache builds (hundreds of shards * tens of thousands of
        // files re-hashed).
        let mut dirty_by_shard: HashMap<CacheShardKey, Vec<CacheKey>> = HashMap::new();
        for key in &self.dirty_keys {
            dirty_by_shard
                .entry(key.shard())
                .or_default()
                .push(key.clone());
        }
        let mut deleted_by_shard: HashMap<CacheShardKey, Vec<(CacheKey, DeletionReason)>> =
            HashMap::new();
        for (key, reason) in &self.deleted_keys {
            deleted_by_shard
                .entry(key.shard())
                .or_default()
                .push((key.clone(), reason.clone()));
        }

        let mut affected_shards = self.rewrite_shards.clone();
        affected_shards.extend(dirty_by_shard.keys().cloned());
        affected_shards.extend(deleted_by_shard.keys().cloned());

        let mut successful_shards = HashSet::new();
        for shard_key in affected_shards {
            let Some(identity) = CacheIdentity::current_for_namespace(&shard_key.namespace) else {
                continue;
            };
            let parser_dir = shard_root.join(identity.namespace);
            if let Err(error) = ensure_cache_dir(&parser_dir) {
                warn_cache_failure_once(
                    "source message cache parser directory is unavailable",
                    &parser_dir,
                    &error,
                );
                continue;
            }
            let final_path = shard_path(&shard_root, &shard_key);

            let mut merged_entries: HashMap<CacheKey, CachedSourceEntry> =
                match read_shard_with_limit(&final_path, identity, max_shard_bytes) {
                    ShardReadStatus::Loaded(entries) => entries
                        .into_iter()
                        .filter(|entry| entry.identity_is_current())
                        .map(|entry| (CacheKey::from_entry(&entry), entry))
                        .filter(|(key, _)| key.shard() == shard_key)
                        .collect(),
                    ShardReadStatus::Missing | ShardReadStatus::Stale => HashMap::new(),
                    ShardReadStatus::Invalid(error) => {
                        warn_cache_failure_once(
                            "source message cache shard is invalid",
                            &final_path,
                            &error,
                        );
                        HashMap::new()
                    }
                };

            if let Some(deleted) = deleted_by_shard.get(&shard_key) {
                for (key, reason) in deleted {
                    let should_remove = match reason {
                        DeletionReason::Missing => !key.path.to_path_buf().exists(),
                        DeletionReason::Invalidated(expected) => merged_entries
                            .get(key)
                            .is_some_and(|entry| entry.fingerprint == *expected),
                    };
                    if should_remove {
                        merged_entries.remove(key);
                    }
                }
            }
            if let Some(dirty) = dirty_by_shard.get(&shard_key) {
                for key in dirty {
                    if let Some(entry) = self.entries.get(key) {
                        merged_entries.insert(key.clone(), entry.clone());
                    }
                }
            }

            let mut entries: Vec<CachedSourceEntry> = merged_entries.into_values().collect();
            entries.sort_by_key(|left| left.path.to_path_buf());
            match write_shard_with_limit(&final_path, identity, &entries, max_shard_bytes) {
                Ok(()) => {
                    successful_shards.insert(shard_key);
                }
                Err(error) => {
                    warn_cache_failure_once(
                        "source message cache shard could not be saved; future scans may remain cold",
                        &final_path,
                        &error,
                    );
                }
            }
        }

        self.dirty_keys
            .retain(|key| !successful_shards.contains(&key.shard()));
        self.deleted_keys
            .retain(|key, _| !successful_shards.contains(&key.shard()));
        self.rewrite_shards
            .retain(|shard| !successful_shards.contains(shard));
        self.dirty = !(self.dirty_keys.is_empty()
            && self.deleted_keys.is_empty()
            && self.rewrite_shards.is_empty());
    }
}

fn shard_filename(index: usize) -> String {
    format!("shard-{index:02x}.bin")
}

fn parse_shard_filename(filename: &std::ffi::OsStr) -> Option<usize> {
    let filename = filename.to_str()?;
    let encoded = filename.strip_prefix("shard-")?.strip_suffix(".bin")?;
    let index = usize::from_str_radix(encoded, 16).ok()?;
    (index < CACHE_SHARD_COUNT).then_some(index)
}

fn shard_path(root: &Path, key: &CacheShardKey) -> PathBuf {
    root.join(&key.namespace).join(shard_filename(key.index))
}

enum ShardReadStatus {
    Missing,
    Stale,
    Invalid(String),
    Loaded(Vec<CachedSourceEntry>),
}

fn read_shard(path: &Path, identity: CacheIdentity) -> ShardReadStatus {
    read_shard_with_limit(path, identity, MAX_CACHE_SHARD_BYTES)
}

fn read_shard_with_limit(
    path: &Path,
    identity: CacheIdentity,
    max_shard_bytes: u64,
) -> ShardReadStatus {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ShardReadStatus::Missing
        }
        Err(error) => return ShardReadStatus::Invalid(error.to_string()),
    };
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(error) => return ShardReadStatus::Invalid(error.to_string()),
    };
    if metadata.len() > max_shard_bytes {
        return ShardReadStatus::Invalid(format!(
            "{} bytes exceeds the {}-byte shard limit",
            metadata.len(),
            max_shard_bytes
        ));
    }

    let envelope: CachedShardEnvelope = match bincode::options()
        .with_limit(max_shard_bytes)
        .deserialize_from(BufReader::new(file))
    {
        Ok(envelope) => envelope,
        Err(error) => return ShardReadStatus::Invalid(error.to_string()),
    };
    if envelope.format_version != CACHE_FORMAT_VERSION {
        return ShardReadStatus::Stale;
    }
    if envelope.parser_namespace != identity.namespace
        || envelope.parser_version != identity.parser_version
    {
        return ShardReadStatus::Stale;
    }

    match bincode::options()
        .with_limit(max_shard_bytes)
        .deserialize(&envelope.payload)
    {
        Ok(entries) => ShardReadStatus::Loaded(entries),
        Err(error) => ShardReadStatus::Invalid(error.to_string()),
    }
}

fn write_shard_with_limit(
    final_path: &Path,
    identity: CacheIdentity,
    entries: &[CachedSourceEntry],
    max_shard_bytes: u64,
) -> std::io::Result<()> {
    let payload = bincode::options()
        .with_limit(max_shard_bytes)
        .serialize(entries)
        .map_err(std::io::Error::other)?;
    let envelope = CachedShardEnvelope {
        format_version: CACHE_FORMAT_VERSION,
        parser_namespace: identity.namespace.to_string(),
        parser_version: identity.parser_version,
        payload,
    };
    let parent = final_path
        .parent()
        .ok_or_else(|| std::io::Error::other("cache shard has no parent directory"))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let tmp_path = parent.join(format!(
        ".{}.{}.{nanos:x}.tmp",
        final_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("source-message-cache"),
        std::process::id(),
    ));

    // INVARIANT: shard writes use atomic temp-file replacement. Never remove
    // the canonical shard before the replacement is completely serialized and
    // fsynced, or one failed large shard write could destroy its last good copy.
    let write_result = (|| -> std::io::Result<()> {
        let file = File::create(&tmp_path)?;
        let mut writer = BufWriter::new(file);
        bincode::options()
            .with_limit(max_shard_bytes)
            .serialize_into(&mut writer, &envelope)
            .map_err(std::io::Error::other)?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        crate::fs_atomic::replace_file(&tmp_path, final_path)?;
        let final_file = OpenOptions::new().read(true).write(true).open(final_path)?;
        final_file.sync_all()?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_result
}

fn read_sample_hash(file: &mut File, offset: u64, len: usize) -> Option<FileSampleHash> {
    if len == 0 {
        return Some(FileSampleHash {
            offset,
            len: 0,
            hash: 0,
        });
    }

    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buffer = vec![0_u8; len];
    file.read_exact(&mut buffer).ok()?;

    Some(FileSampleHash {
        offset,
        len: len as u64,
        hash: hash_bytes(&buffer),
    })
}

fn compute_sample_hashes(path: &Path, size: u64) -> Option<Vec<FileSampleHash>> {
    if size == 0 {
        return Some(Vec::new());
    }

    let mut file = File::open(path).ok()?;
    let offsets = sample_offsets(size);
    offsets
        .into_iter()
        .map(|(offset, len)| read_sample_hash(&mut file, offset, len))
        .collect()
}

fn sample_offsets(size: u64) -> Vec<(u64, usize)> {
    let sample_len = size.min(FINGERPRINT_SAMPLE_BYTES as u64) as usize;
    if sample_len == 0 {
        return Vec::new();
    }

    let max_offset = size.saturating_sub(sample_len as u64);
    let mut offsets = if max_offset == 0 {
        vec![0]
    } else {
        vec![
            0,
            max_offset / 4,
            max_offset / 2,
            max_offset.saturating_mul(3) / 4,
            max_offset,
        ]
    };
    offsets.sort_unstable();
    offsets.dedup();
    offsets.truncate(FINGERPRINT_SAMPLE_POINTS);
    offsets
        .into_iter()
        .map(|offset| (offset, sample_len))
        .collect()
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Whether a fingerprint carries a whole-file `content_hash`.
///
/// Validation uses size + mtime + samples ([`primary_fingerprint_matches`] and
/// [`related_fingerprint_metadata_matches`]) for every source. Only Codex reads
/// `content_hash` for incremental resume;
/// generic parsers and SQLite sources store a zero sentinel so changed or cold
/// files do not pay for a second whole-file hash that cannot affect parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentHashMode {
    Full,
    SamplesOnly,
}

fn file_fingerprint_parts(
    path: &Path,
    mode: ContentHashMode,
) -> Option<(u64, u64, Vec<FileSampleHash>, [u8; 32])> {
    let metadata = path.metadata().ok()?;
    let size = metadata.len();
    let modified_ns = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos() as u64;
    let sample_hashes = compute_sample_hashes(path, size)?;
    let content_hash = match mode {
        ContentHashMode::Full => hash_prefix(path, size)?,
        ContentHashMode::SamplesOnly => [0_u8; 32],
    };
    Some((size, modified_ns, sample_hashes, content_hash))
}

fn append_path_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut os = OsString::from(path.as_os_str());
    os.push(suffix);
    PathBuf::from(os)
}

fn hash_prefix(path: &Path, len: u64) -> Option<[u8; 32]> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut remaining = len;
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];

    while remaining > 0 {
        let bytes_to_read = remaining.min(HASH_BUFFER_BYTES as u64) as usize;
        let read = file.read(&mut buffer[..bytes_to_read]).ok()?;
        if read == 0 {
            return None;
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }

    Some(hasher.finalize().into())
}

/// Build Codex incremental state when the caller already hashed the complete
/// consumed prefix. Full-file Codex fingerprints are also the prefix hash when
/// `consumed_offset` equals the current file size, so accepting that digest
/// avoids a second read of the transcript.
pub(crate) fn build_codex_incremental_cache_with_prefix_hash(
    path: &Path,
    consumed_offset: u64,
    state: CodexParseState,
    prefix_hash: [u8; 32],
) -> Option<CodexIncrementalCache> {
    let ends_with_newline = consumed_offset == 0 || file_ends_with_newline(path, consumed_offset);
    if !ends_with_newline {
        return None;
    }

    Some(CodexIncrementalCache {
        state,
        consumed_offset,
        ends_with_newline,
        prefix_hash,
    })
}

fn file_ends_with_newline(path: &Path, size: u64) -> bool {
    if size == 0 {
        return true;
    }

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    if file.seek(SeekFrom::Start(size.saturating_sub(1))).is_err() {
        return false;
    }

    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte).is_ok() && byte[0] == b'\n'
}

pub(crate) fn codex_prefix_matches(path: &Path, cached: &CodexIncrementalCache) -> bool {
    if cached.consumed_offset > 0 && !cached.ends_with_newline {
        return false;
    }

    match hash_prefix(path, cached.consumed_offset) {
        Some(prefix_hash) => prefix_hash == cached.prefix_hash,
        None => false,
    }
}

pub(crate) fn codex_cache_entry_matches_fingerprint(
    cached: &CachedSourceEntry,
    fingerprint: &SourceFingerprint,
) -> bool {
    let Some(codex_incremental) = cached.codex_incremental.as_ref() else {
        return false;
    };

    codex_incremental.consumed_offset == fingerprint.size
        && codex_incremental.ends_with_newline
        && codex_incremental.prefix_hash == fingerprint.content_hash
}

/// Delete on-disk source-message cache shards so the next scan reparses everything.
///
/// Used by `tokens usage --force-rescan` (and similar UI "full rescan" actions).
/// Clear holds the persistent Layer A lock and advances its generation. A scan
/// that loaded an older generation may finish, but its later save is discarded
/// instead of republishing pre-clear cache entries.
pub fn clear_source_message_cache() -> Result<(), String> {
    let (Some(shard_root), Some(lock_path)) = (cache_shard_dir(), cache_lock_path()) else {
        return Ok(());
    };
    clear_source_message_cache_at(&shard_root, &lock_path)
}

fn clear_source_message_cache_at(shard_root: &Path, lock_path: &Path) -> Result<(), String> {
    let mut lock_file = open_cache_lock(lock_path).map_err(|error| {
        format!(
            "failed to open source message cache lock at {}: {error}",
            lock_path.display()
        )
    })?;
    fs2::FileExt::lock_exclusive(&lock_file).map_err(|error| {
        format!(
            "failed to lock source message cache at {}: {error}",
            lock_path.display()
        )
    })?;
    let generation = match read_cache_generation(&mut lock_file) {
        Ok(generation) => generation,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => 0,
        Err(error) => {
            return Err(format!(
                "failed to read source message cache generation at {}: {error}",
                lock_path.display()
            ))
        }
    };
    if shard_root.exists() {
        fs::remove_dir_all(shard_root).map_err(|error| {
            format!(
                "failed to clear source message cache at {}: {error}",
                shard_root.display()
            )
        })?;
    }
    let next_generation = generation.checked_add(1).ok_or_else(|| {
        format!(
            "source message cache generation overflow at {}",
            lock_path.display()
        )
    })?;
    write_cache_generation(&mut lock_file, next_generation).map_err(|error| {
        format!(
            "failed to advance source message cache generation at {}: {error}",
            lock_path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::Options;

    #[test]
    fn force_clear_blocks_stale_cache_republish_and_preserves_the_lock_inode() {
        use std::sync::mpsc;

        let dir = tempfile::tempdir().unwrap();
        let shard_root = dir.path().join(CACHE_SHARD_DIRNAME);
        let lock_path = dir.path().join(CACHE_LOCK_FILENAME);
        let source_path = dir.path().join("source.jsonl");
        std::fs::write(&source_path, b"source").unwrap();
        let entry = CachedSourceEntry::new(
            CacheIdentity::synthetic(),
            &source_path,
            SourceFingerprint::from_path(&source_path).unwrap(),
            vec![],
            vec![],
            None,
        );

        let mut seed = SourceMessageCache::load_from_paths(&shard_root, &lock_path);
        seed.insert(entry.clone());
        seed.save_if_dirty_with_limit_at(MAX_CACHE_SHARD_BYTES, &shard_root, &lock_path);
        assert!(shard_root.exists());

        #[cfg(unix)]
        let original_lock_inode = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&lock_path).unwrap().ino()
        };

        let (loaded_tx, loaded_rx) = mpsc::channel();
        let (save_tx, save_rx) = mpsc::channel();
        let stale_writer = {
            let shard_root = shard_root.clone();
            let lock_path = lock_path.clone();
            let source_path = source_path.clone();
            let entry = entry.clone();
            std::thread::spawn(move || {
                let mut stale = SourceMessageCache::load_from_paths(&shard_root, &lock_path);
                assert!(stale
                    .get(CacheIdentity::synthetic(), &source_path)
                    .is_some());
                stale.insert(entry);
                loaded_tx.send(()).unwrap();
                save_rx.recv().unwrap();
                stale.save_if_dirty_with_limit_at(MAX_CACHE_SHARD_BYTES, &shard_root, &lock_path);
            })
        };

        loaded_rx.recv().unwrap();
        let probe = open_cache_lock(&lock_path).unwrap();
        let shared_lock_is_held = match fs2::FileExt::try_lock_exclusive(&probe) {
            Ok(()) => {
                fs2::FileExt::unlock(&probe).unwrap();
                false
            }
            Err(_) => true,
        };
        if !shared_lock_is_held {
            save_tx.send(()).unwrap();
            stale_writer.join().unwrap();
            panic!("loaded cache must retain its shared lock until scanning finishes");
        }

        let clearer = {
            let shard_root = shard_root.clone();
            let lock_path = lock_path.clone();
            std::thread::spawn(move || {
                clear_source_message_cache_at(&shard_root, &lock_path).unwrap();
            })
        };
        save_tx.send(()).unwrap();
        stale_writer.join().unwrap();
        clearer.join().unwrap();
        assert!(!shard_root.exists());

        let reloaded = SourceMessageCache::load_from_paths(&shard_root, &lock_path);
        assert!(reloaded
            .get(CacheIdentity::synthetic(), &source_path)
            .is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                std::fs::metadata(&lock_path).unwrap().ino(),
                original_lock_inode
            );
        }
    }

    #[test]
    fn force_clear_recovers_from_an_interrupted_generation_record() {
        let dir = tempfile::tempdir().unwrap();
        let shard_root = dir.path().join(CACHE_SHARD_DIRNAME);
        let lock_path = dir.path().join(CACHE_LOCK_FILENAME);
        let mut lock = open_cache_lock(&lock_path).unwrap();
        fs2::FileExt::lock_exclusive(&lock).unwrap();
        lock.write_all(&1_u64.to_le_bytes()[..3]).unwrap();
        lock.sync_all().unwrap();
        fs2::FileExt::unlock(&lock).unwrap();
        drop(lock);

        clear_source_message_cache_at(&shard_root, &lock_path).unwrap();

        let mut lock = open_cache_lock(&lock_path).unwrap();
        fs2::FileExt::lock_shared(&lock).unwrap();
        assert_eq!(read_cache_generation(&mut lock).unwrap(), 1);
        fs2::FileExt::unlock(&lock).unwrap();
        fs2::FileExt::lock_exclusive(&lock).unwrap();
        lock.seek(SeekFrom::End(0)).unwrap();
        lock.write_all(&2_u64.to_le_bytes()[..3]).unwrap();
        lock.sync_all().unwrap();
        drop(lock);

        clear_source_message_cache_at(&shard_root, &lock_path).unwrap();

        let mut lock = open_cache_lock(&lock_path).unwrap();
        fs2::FileExt::lock_shared(&lock).unwrap();
        assert_eq!(read_cache_generation(&mut lock).unwrap(), 2);
    }

    #[test]
    fn old_format_is_stale_before_incompatible_payload_decode() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let identity = CacheIdentity::synthetic();
        let envelope = CachedShardEnvelope {
            format_version: CACHE_FORMAT_VERSION - 1,
            parser_namespace: identity.namespace.to_string(),
            parser_version: identity.parser_version,
            payload: vec![0xff, 0x00, 0xff],
        };
        bincode::options()
            .serialize_into(file.as_file_mut(), &envelope)
            .unwrap();
        file.as_file_mut().sync_all().unwrap();

        assert!(matches!(
            read_shard(file.path(), identity),
            ShardReadStatus::Stale
        ));
    }

    #[test]
    fn kimi_parser_version_bump_is_client_scoped() {
        assert_eq!(CACHE_FORMAT_VERSION, 6);
        assert_eq!(parser_version(ClientId::Kimi), 3);
    }
}
