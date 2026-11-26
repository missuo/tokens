//! Amp (ampcode.com) thread sync.
//!
//! Current Amp keeps threads on ampcode.com and no longer writes the local
//! `~/.local/share/amp/threads` directory the Amp parser reads, so usage from
//! any recent thread, including threads that ran in the cloud, was missing
//! (missuo/tokens#64).
//!
//! `tokens amp sync` asks the signed-in `amp` CLI for the account's threads,
//! exports each new or changed one (`amp threads export`), and writes only
//! what the Amp parser reads to
//! `<tokens config>/amp-cache/threads/T-<id>.json`: the thread id, its
//! creation time, and each message's role, id and usage. Prompts, replies,
//! tool output and titles are dropped before anything is written. tokens never
//! reads Amp's credentials; the `amp` CLI makes the requests with its own
//! login.
//!
//! Threads are found two ways, because neither is enough alone:
//! `amp threads list --json` pages through every thread but leaves out some
//! that ran on Amp's servers, and its `updated` time moves with the user's
//! messages, not the agent's, so a thread exported mid-turn would keep the
//! rest of that turn out of the cache. `amp threads search author:me` returns
//! only the most recently active threads, unlisted ones included, with an
//! `updatedAt` that follows every message. A thread found by search is
//! re-exported when that time changes; any other thread, when its listed time
//! does. A thread with new activity is always among the most recent, so the
//! cap on search results only limits which old, unlisted threads a first sync
//! finds.
//!
//! An export holds the requests of the thread's main agent only. Amp bills a
//! thread's subagent (Task) and system-model requests (titles and the like) to
//! the same thread, but they are not in the export, so they are not counted.

use crate::{t, tr};
use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LIST_PAGE_SIZE: usize = 500;
/// The most `amp threads search` returns.
const SEARCH_LIMIT: usize = 100;
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const EXPORT_TIMEOUT: Duration = Duration::from_secs(300);
const EXPORT_WORKERS: usize = 4;
const STATE_VERSION: u32 = 1;

pub fn get_amp_cache_dir() -> PathBuf {
    crate::paths::get_config_dir().join("amp-cache")
}

/// Must match `tokens_core::scanner::amp_sync_cache_root`.
fn threads_dir() -> PathBuf {
    get_amp_cache_dir().join("threads")
}

fn state_path() -> PathBuf {
    get_amp_cache_dir().join("state.json")
}

/// Whether `tokens amp sync` has completed here. Only then does
/// `tokens submit` keep the cache current on its own.
pub fn has_amp_cache() -> bool {
    state_path().is_file()
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AmpSyncState {
    version: u32,
    synced_at: Option<String>,
    /// Thread id -> the update time the cached copy was exported at.
    threads: BTreeMap<String, String>,
}

/// One entry of `amp threads list --json`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListedThread {
    id: String,
    updated: String,
    message_count: Option<i64>,
}

/// One entry of `amp threads search --json`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchedThread {
    id: String,
    updated_at: String,
}

/// The parts of `amp threads export` kept in the cache; serde skips the rest
/// of the payload without reading it into memory.
#[derive(Debug, Serialize, Deserialize)]
struct CachedThread {
    id: String,
    created: Option<i64>,
    messages: Vec<CachedMessage>,
    #[serde(
        rename = "usageLedger",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    usage_ledger: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedMessage {
    role: Option<String>,
    #[serde(rename = "messageId")]
    message_id: Option<i64>,
    usage: Option<Value>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncAmpResult {
    pub synced: bool,
    /// Threads with messages on the account.
    pub threads: usize,
    /// Threads exported this run because they were new or changed.
    pub exported: usize,
    /// Exports that failed; they are retried by the next sync.
    pub failed: usize,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AmpStatus {
    amp_cli: Option<String>,
    cache_dir: String,
    has_cache: bool,
    cached_threads: usize,
    last_synced_at: Option<String>,
}

pub fn run_amp_sync(force: bool, json: bool) -> Result<()> {
    let result = sync_amp_cache(force);
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.synced {
        println!(
            "{}",
            tr!(
                "top.amp.synced",
                threads = result.threads,
                exported = result.exported
            )
            .green()
        );
        if result.failed > 0 {
            eprintln!(
                "{}",
                tr!(
                    "top.amp.exportsFailed",
                    failed = result.failed,
                    error = result.error.unwrap_or_default()
                )
                .yellow()
            );
        }
    } else if let Some(error) = result.error {
        eprintln!("{}", tr!("top.amp.syncFailed", error = error).yellow());
    }
    Ok(())
}

pub fn run_amp_status(json: bool) -> Result<()> {
    let state = load_state();
    let status = AmpStatus {
        amp_cli: find_amp_binary().map(|path| path.display().to_string()),
        cache_dir: threads_dir().display().to_string(),
        has_cache: state.is_some(),
        cached_threads: state.as_ref().map_or(0, |state| state.threads.len()),
        last_synced_at: state.and_then(|state| state.synced_at),
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!("\n  {}", t!("top.amp.statusTitle").cyan());
    println!(
        "  {} {}",
        t!("top.amp.cliLabel").bright_black(),
        match &status.amp_cli {
            Some(path) => path.green(),
            None => t!("top.amp.cliMissing").yellow(),
        }
    );
    println!(
        "  {} {}",
        t!("top.amp.cacheLabel").bright_black(),
        if status.has_cache {
            status.cache_dir.green()
        } else {
            status.cache_dir.yellow()
        }
    );
    if status.has_cache {
        println!(
            "  {} {}",
            t!("top.amp.threadsLabel").bright_black(),
            status.cached_threads
        );
        if let Some(synced_at) = &status.last_synced_at {
            println!(
                "  {} {}",
                t!("top.amp.lastSyncLabel").bright_black(),
                synced_at
            );
        }
    } else {
        println!("{}", t!("top.amp.syncHint").bright_black());
    }
    println!();
    Ok(())
}

pub fn sync_amp_cache(force: bool) -> SyncAmpResult {
    sync_amp_cache_with(force).unwrap_or_else(|error| SyncAmpResult {
        error: Some(format!("{error:#}")),
        ..SyncAmpResult::default()
    })
}

fn sync_amp_cache_with(force: bool) -> Result<SyncAmpResult> {
    let amp = find_amp_binary().ok_or_else(|| anyhow::anyhow!(t!("top.amp.cliNotFound")))?;
    let threads = find_threads(&amp)?;

    let mut state = load_state().unwrap_or_default();
    let dir = threads_dir();
    let pending: Vec<(&String, &String)> = threads
        .iter()
        .filter(|(id, updated)| {
            force
                || state.threads.get(*id) != Some(*updated)
                || !dir.join(format!("{id}.json")).is_file()
        })
        .collect();

    // A first sync exports every thread; a few at a time keeps it to minutes
    // without flooding ampcode.com.
    let next = AtomicUsize::new(0);
    let exported = Mutex::new(Vec::new());
    let errors = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..EXPORT_WORKERS.min(pending.len()) {
            scope.spawn(|| {
                while let Some((id, updated)) = pending.get(next.fetch_add(1, Ordering::Relaxed)) {
                    match export_thread(&amp, &dir, id) {
                        Ok(()) => exported
                            .lock()
                            .unwrap()
                            .push(((*id).clone(), (*updated).clone())),
                        Err(error) => errors.lock().unwrap().push(format!("{id}: {error:#}")),
                    }
                }
            });
        }
    });
    let exported = exported.into_inner().unwrap();
    let errors = errors.into_inner().unwrap();

    let exported_count = exported.len();
    state.version = STATE_VERSION;
    state.synced_at = Some(chrono::Utc::now().to_rfc3339());
    // Threads deleted on ampcode.com keep their cached copy: their usage
    // happened.
    state.threads.extend(exported);
    save_state(&state)?;

    Ok(SyncAmpResult {
        synced: true,
        threads: threads.len(),
        exported: exported_count,
        failed: errors.len(),
        error: errors.into_iter().next(),
    })
}

/// Every thread to keep in the cache, with the update time that marks a
/// change (see the module docs for why it comes from two commands).
fn find_threads(amp: &Path) -> Result<BTreeMap<String, String>> {
    let mut threads: BTreeMap<String, String> = list_threads(amp)?
        .into_iter()
        .filter(|thread| thread.message_count != Some(0))
        .map(|thread| (thread.id, thread.updated))
        .collect();
    threads.extend(
        search_recent_threads(amp)?
            .into_iter()
            .map(|thread| (thread.id, thread.updated_at)),
    );
    threads.retain(|id, _| is_thread_id(id));
    Ok(threads)
}

fn search_recent_threads(amp: &Path) -> Result<Vec<SearchedThread>> {
    let limit = SEARCH_LIMIT.to_string();
    let args = [
        "threads",
        "search",
        "author:me",
        "--json",
        "--limit",
        &limit,
    ];
    let output = run_amp(amp, &args, LIST_TIMEOUT)?;
    serde_json::from_slice(&output).with_context(|| unexpected_output(&args))
}

fn list_threads(amp: &Path) -> Result<Vec<ListedThread>> {
    let mut threads = Vec::new();
    loop {
        let offset = threads.len().to_string();
        let limit = LIST_PAGE_SIZE.to_string();
        let args = [
            "threads",
            "list",
            "--json",
            "--include-archived",
            "--limit",
            &limit,
            "--offset",
            &offset,
        ];
        let output = run_amp(amp, &args, LIST_TIMEOUT)?;
        let page: Vec<ListedThread> =
            serde_json::from_slice(&output).with_context(|| unexpected_output(&args))?;
        let last_page = page.len() < LIST_PAGE_SIZE;
        threads.extend(page);
        if last_page {
            return Ok(threads);
        }
    }
}

fn export_thread(amp: &Path, dir: &Path, thread_id: &str) -> Result<()> {
    let args = ["threads", "export", thread_id];
    let output = run_amp(amp, &args, EXPORT_TIMEOUT)?;
    let contents =
        usage_only_thread(thread_id, &output).with_context(|| unexpected_output(&args))?;
    crate::commands::usage::helpers::atomic_write_secret(
        &dir.join(format!("{thread_id}.json")),
        &contents,
    )?;
    Ok(())
}

/// The export reduced to the fields the Amp parser reads, with only the
/// messages that carry usage.
fn usage_only_thread(thread_id: &str, export: &[u8]) -> Result<Vec<u8>> {
    let mut thread: CachedThread = serde_json::from_slice(export)?;
    anyhow::ensure!(
        thread.id == thread_id,
        tr!("top.amp.exportOfOtherThread", id = thread.id)
    );
    thread
        .messages
        .retain(|message| message.usage.as_ref().is_some_and(Value::is_object));
    Ok(serde_json::to_vec(&thread)?)
}

fn unexpected_output(args: &[&str]) -> String {
    tr!(
        "top.amp.unexpectedOutput",
        command = format!("amp {}", args.join(" "))
    )
}

/// Thread ids become file names, so anything but `T-` and ASCII letters,
/// digits and dashes is refused.
fn is_thread_id(id: &str) -> bool {
    id.strip_prefix("T-").is_some_and(|rest| {
        !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// The `amp` executable: on `PATH`, else where Amp's installer or Homebrew
/// puts it. `tokens serve` often runs under launchd or systemd with a minimal
/// `PATH` that leaves those directories out.
fn find_amp_binary() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["amp.exe", "amp.cmd"]
    } else {
        &["amp"]
    };
    let path_dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut install_dirs = crate::paths::home_dir()
        .map(|home| {
            vec![
                home.join(".amp").join("bin"),
                home.join(".local").join("bin"),
            ]
        })
        .unwrap_or_default();
    if cfg!(unix) {
        install_dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    }

    path_dirs
        .into_iter()
        .chain(install_dirs)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|path| path.is_file())
}

/// Runs `amp` and returns its stdout, failing on a non-zero exit (with the
/// last line Amp printed to stderr) or once `timeout` passes.
fn run_amp(amp: &Path, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    let command = format!("amp {}", args.join(" "));
    let mut child = Command::new(amp)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| tr!("top.amp.runFailed", command = command))?;

    // Both pipes are drained while the child runs: an export can be tens of
    // megabytes, far more than a pipe buffers.
    let mut stdout = child.stdout.take().context(t!("top.amp.missingStreams"))?;
    let mut stderr = child.stderr.take().context(t!("top.amp.missingStreams"))?;
    let stdout_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = stderr.read_to_string(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(tr!(
                "top.amp.commandTimedOut",
                command = command,
                seconds = timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        let reason = stderr
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .unwrap_or_default()
            .to_string();
        anyhow::bail!(tr!(
            "top.amp.commandFailed",
            command = command,
            error = reason
        ));
    }
    Ok(stdout)
}

fn load_state() -> Option<AmpSyncState> {
    let content = fs::read(state_path()).ok()?;
    serde_json::from_slice(&content).ok()
}

fn save_state(state: &AmpSyncState) -> Result<()> {
    let contents = serde_json::to_vec_pretty(state)?;
    crate::commands::usage::helpers::atomic_write_secret(&state_path(), &contents)
        .context(t!("top.amp.writeStateFailed"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_thread_keeps_usage_and_drops_conversation() {
        let export = serde_json::json!({
            "id": "T-1",
            "title": "secret project",
            "created": 1_775_304_000_000_i64,
            "messages": [
                {
                    "role": "user",
                    "messageId": 1,
                    "content": [{ "type": "text", "text": "my prompt" }]
                },
                {
                    "role": "assistant",
                    "messageId": 2,
                    "content": [{ "type": "text", "text": "my reply" }],
                    "usage": {
                        "model": "claude-opus-5-5",
                        "timestamp": "2026-11-19T05:20:21.230Z",
                        "inputTokens": 2,
                        "outputTokens": 1630,
                        "cacheReadInputTokens": 0,
                        "cacheCreationInputTokens": 71025
                    }
                }
            ]
        });

        let cached = usage_only_thread("T-1", export.to_string().as_bytes()).unwrap();
        let text = String::from_utf8(cached.clone()).unwrap();
        for private in ["secret project", "my prompt", "my reply", "content"] {
            assert!(!text.contains(private), "{private:?} leaked into {text}");
        }

        // The Amp parser reads the reduced file exactly as it read the export.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("T-1.json");
        fs::write(&path, &cached).unwrap();
        let messages = tokens_core::sessions::amp::parse_amp_file(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].session_id, "T-1");
        assert_eq!(messages[0].tokens.output, 1630);
        assert_eq!(messages[0].tokens.cache_write, 71025);
        assert_eq!(
            messages[0].timestamp,
            chrono::DateTime::parse_from_rfc3339("2026-11-19T05:20:21.230Z")
                .unwrap()
                .timestamp_millis()
        );
    }

    #[test]
    fn export_of_another_thread_is_refused() {
        let export = serde_json::json!({ "id": "T-2", "messages": [] });
        assert!(usage_only_thread("T-1", export.to_string().as_bytes()).is_err());
    }

    #[test]
    fn thread_ids_that_are_not_file_names_are_refused() {
        assert!(is_thread_id("T-01a206d8-0e69-75aa-bd05-ec4b6664d307"));
        for id in ["", "T-", "01a206d8", "T-../../x", "T-a/b", "T-a.json"] {
            assert!(!is_thread_id(id), "{id:?}");
        }
    }
}
