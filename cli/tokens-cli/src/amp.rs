//! Amp thread usage sync.
//!
//! Current Amp keeps threads on its server instead of writing them to
//! `~/.local/share/amp/threads`. `tokens amp sync` asks the local `amp` CLI for
//! the account's threads (`amp threads list --json`) and exports the ones that
//! changed (`amp threads export`), keeping only each assistant message's usage
//! object. The Amp parser reads the result from `<config>/amp-cache/threads`.
//!
//! Exports leave out usage that does not belong to a main-thread turn: Task
//! subagents, compaction, thread titles and similar side requests.
//!
//! Sync is account-wide, so it is opt-in per device: `submit` (and therefore
//! `serve`) only refreshes the cache on a device where `tokens amp sync` has
//! been run once. Two devices syncing the same account would each report the
//! same days.

use crate::paths;
use crate::{t, tr};
use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tokens_core::scanner::AMP_CACHE_THREADS_RELATIVE;
use tokens_core::ClientId;

const LIST_PAGE_SIZE: usize = 500;
const EXPORT_WORKERS: usize = 6;

fn threads_dir() -> PathBuf {
    let mut dir = paths::get_config_dir();
    for component in Path::new(AMP_CACHE_THREADS_RELATIVE).components() {
        dir.push(component.as_os_str());
    }
    dir
}

fn index_path() -> PathBuf {
    threads_dir().with_file_name("index.json")
}

/// True once `tokens amp sync` has run on this device.
pub fn is_sync_enabled() -> bool {
    threads_dir().is_dir()
}

#[derive(Debug, Default, Serialize)]
pub struct SyncAmpResult {
    pub synced: bool,
    /// Threads the account has.
    pub threads: usize,
    /// Threads exported in this run because they changed.
    pub exported: usize,
    pub failed: usize,
    /// Threads left to the files an older Amp build wrote locally.
    pub skipped_local: usize,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListedThread {
    id: String,
    updated: String,
}

/// `updated` of every thread as of its last successful export.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SyncIndex {
    threads: HashMap<String, String>,
}

fn amp_command() -> Command {
    // npm installs a `.cmd` shim on Windows, which `Command` does not resolve
    // from a bare name.
    #[cfg(windows)]
    {
        if which_on_path("amp.exe").is_none() {
            return Command::new("amp.cmd");
        }
    }
    Command::new("amp")
}

#[cfg(windows)]
fn which_on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

fn run_amp(args: &[&str]) -> Result<Vec<u8>> {
    let output = amp_command()
        .args(args)
        .output()
        .context("could not run `amp`; is the Amp CLI installed and on PATH?")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`amp {}` failed: {}", args.join(" "), stderr.trim());
    }
    Ok(output.stdout)
}

fn list_threads() -> Result<Vec<ListedThread>> {
    let mut threads = Vec::new();
    let limit = LIST_PAGE_SIZE.to_string();
    loop {
        let offset = threads.len().to_string();
        let stdout = run_amp(&[
            "threads",
            "list",
            "--json",
            "--include-archived",
            "--limit",
            &limit,
            "--offset",
            &offset,
        ])?;
        let page: Vec<ListedThread> =
            serde_json::from_slice(&stdout).context("unexpected `amp threads list` output")?;
        let page_len = page.len();
        threads.extend(page);
        if page_len < LIST_PAGE_SIZE {
            return Ok(threads);
        }
    }
}

/// Thread ids become file names, so anything but `T-` plus id characters is
/// refused.
fn is_safe_thread_id(id: &str) -> bool {
    id.len() > 2
        && id.starts_with("T-")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Keep what the Amp parser reads: the thread's id and creation time, and the
/// usage of each assistant message. Prompts, replies and tool calls are dropped.
fn slim_thread(thread: &Value) -> Value {
    let messages: Vec<Value> = thread
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.get("role").and_then(Value::as_str) == Some("assistant")
                        && message.get("usage").is_some_and(Value::is_object)
                })
                .map(|message| {
                    serde_json::json!({
                        "role": "assistant",
                        "messageId": message.get("messageId"),
                        "usage": message.get("usage"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    serde_json::json!({
        "id": thread.get("id"),
        "created": thread.get("created"),
        "messages": messages,
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    tokens_core::fs_atomic::replace_file(&tmp, path)
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

fn export_thread(id: &str, dir: &Path) -> Result<()> {
    let stdout = run_amp(&["threads", "export", id])?;
    let thread: Value =
        serde_json::from_slice(&stdout).context("unexpected `amp threads export` output")?;
    let slim = serde_json::to_vec(&slim_thread(&thread))?;
    write_atomic(&dir.join(format!("{id}.json")), &slim)
}

/// Where older Amp builds wrote thread files. A thread found there is already
/// counted from that file, so exporting it too would count it twice.
fn local_threads_dir() -> Option<PathBuf> {
    let home = paths::home_dir()?;
    Some(PathBuf::from(
        ClientId::Amp
            .data()
            .resolve_path_with_env_strategy(&home.to_string_lossy(), true),
    ))
}

fn load_index() -> SyncIndex {
    std::fs::read(index_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn sync_amp_cache() -> SyncAmpResult {
    let mut result = SyncAmpResult::default();
    if let Err(error) = sync_into(&mut result) {
        result.error = Some(format!("{error:#}"));
    }
    result
}

fn sync_into(result: &mut SyncAmpResult) -> Result<()> {
    let threads = list_threads()?;
    result.threads = threads.len();

    let dir = threads_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let local_dir = local_threads_dir();
    let mut index = load_index();

    let mut pending = Vec::new();
    for thread in threads {
        if !is_safe_thread_id(&thread.id) {
            continue;
        }
        if local_dir
            .as_ref()
            .is_some_and(|local| local.join(format!("{}.json", thread.id)).is_file())
        {
            result.skipped_local += 1;
            continue;
        }
        let unchanged = index.threads.get(&thread.id) == Some(&thread.updated)
            && dir.join(format!("{}.json", thread.id)).is_file();
        if !unchanged {
            pending.push(thread);
        }
    }

    let queue = Mutex::new(pending.into_iter());
    let done = Mutex::new(Vec::new());
    let errors = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..EXPORT_WORKERS {
            scope.spawn(|| loop {
                let Some(thread) = queue.lock().unwrap().next() else {
                    return;
                };
                match export_thread(&thread.id, &dir) {
                    Ok(()) => done.lock().unwrap().push(thread),
                    Err(error) => errors
                        .lock()
                        .unwrap()
                        .push(format!("{}: {error:#}", thread.id)),
                }
            });
        }
    });

    let done = done.into_inner().unwrap();
    let errors = errors.into_inner().unwrap();
    result.exported = done.len();
    result.failed = errors.len();
    for thread in done {
        index.threads.insert(thread.id, thread.updated);
    }
    write_atomic(&index_path(), &serde_json::to_vec(&index)?)?;

    // A failed export keeps the thread's previous file and is retried next
    // run; the sync still counts as done unless nothing could be exported.
    if let Some(first) = errors.first() {
        if result.exported == 0 {
            bail!("{first}");
        }
        result.error = Some(first.clone());
    }
    result.synced = true;
    Ok(())
}

pub fn run_amp_sync(json: bool) -> Result<()> {
    let result = sync_amp_cache();
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.synced {
        println!(
            "{}",
            tr!(
                "cli.amp.synced",
                threads = result.threads,
                exported = result.exported
            )
            .green()
        );
        if result.failed > 0 {
            eprintln!(
                "{}",
                tr!(
                    "cli.amp.partial",
                    failed = result.failed,
                    error = result.error.as_deref().unwrap_or_default()
                )
                .yellow()
            );
        }
        println!("{}", t!("cli.amp.subagentNote").bright_black());
    } else if let Some(error) = result.error {
        eprintln!("{}", tr!("cli.amp.syncFailed", error = error).yellow());
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slim_thread_keeps_only_assistant_usage() {
        let thread = serde_json::json!({
            "v": 3,
            "id": "T-abc",
            "created": 1795258909212_i64,
            "title": "secret title",
            "env": { "initial": { "trees": ["file:///Users/me/repo"] } },
            "messages": [
                { "role": "user", "messageId": 0, "content": [{ "type": "text", "text": "prompt" }] },
                {
                    "role": "assistant",
                    "messageId": 1,
                    "content": [{ "type": "text", "text": "reply" }],
                    "usage": { "model": "claude-opus-5-5", "inputTokens": 2, "timestamp": "2026-11-21T11:02:03.011Z" }
                },
                { "role": "assistant", "messageId": 2, "content": [] }
            ]
        });

        assert_eq!(
            slim_thread(&thread),
            serde_json::json!({
                "id": "T-abc",
                "created": 1795258909212_i64,
                "messages": [{
                    "role": "assistant",
                    "messageId": 1,
                    "usage": { "model": "claude-opus-5-5", "inputTokens": 2, "timestamp": "2026-11-21T11:02:03.011Z" }
                }]
            })
        );
    }

    #[test]
    fn thread_ids_cannot_escape_the_cache_dir() {
        assert!(is_safe_thread_id("T-01a202f3-0507-700b-aa61-f06491e47bc3"));
        assert!(!is_safe_thread_id("T-../../etc/passwd"));
        assert!(!is_safe_thread_id("x-123"));
        assert!(!is_safe_thread_id("T-"));
    }
}
