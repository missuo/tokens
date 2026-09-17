//! Hindsight memory backend integration and usage ledger synchronization.
//!
//! # Background & Ledger Contract
//!
//! Hindsight does not persist local session log files. It records each LLM call
//! in its own database and exposes them via `GET /v1/{tenant}/banks/{bank_id}/llm-requests`.
//! That table is a rolling window: entries observed in one run disappear after
//! service restarts or window expiry.
//!
//! To track token usage over time, Tokens mirrors the API into a local append-only
//! ledger cache located at `$HINDSIGHT_HOME/usage/<YYYY-MM>.jsonl` (defaulting to
//! `~/.hindsight/usage/<YYYY-MM>.jsonl`, or `<home>/.hindsight/usage/<YYYY-MM>.jsonl`
//! when `--home` is specified).
//!
//! # Privacy & Content Exclusion
//!
//! The Hindsight API response includes `input`, `output`, and `metadata` fields
//! containing raw user prompts, assistant completions, and memory document contents.
//! These fields are intentionally dropped during sync and are NEVER written to disk.
//! Only token counters, timing metadata, operation, model, and bank identifiers
//! are retained.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// HTTP request timeout for Hindsight API operations.
const HINDSIGHT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// A single persisted LLM request record in the Hindsight usage ledger.
///
/// Note: The Hindsight API response includes `input`, `output`, and `metadata`
/// fields which carry raw user prompts, assistant completions, and extracted
/// memory content. Those fields are intentionally NEVER stored in the local
/// ledger to protect user privacy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HindsightLedgerRow {
    pub id: String,
    pub trace_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub operation: Option<String>,
    pub scope: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub total_tokens: u64,
    pub bank: String,
}

/// Raw LLM request item as returned by Hindsight's HTTP API.
///
/// Deserialized solely to extract token metrics and timing. The content fields
/// (`input`, `output`, `metadata`) have no counterpart here at all — see the
/// note at the end of the field list.
#[derive(Debug, Deserialize)]
pub struct RawApiLlmRequest {
    pub id: String,
    pub trace_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub operation: Option<String>,
    pub scope: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub total_tokens: Option<i64>,
    #[serde(default)]
    pub bank_id: Option<String>,
    // `input`, `output`, and `metadata` are deliberately absent from this
    // struct. They carry the prompt and the memory content Hindsight was
    // reasoning over, and this cache is a usage ledger, not a transcript.
    // Serde ignores unknown fields, so omitting them here is a stronger
    // guarantee than reading and dropping them: the content never enters the
    // process at all.
}

/// One page of `GET /v1/{tenant}/banks/{bank_id}/llm-requests`.
///
/// `total` drives pagination: the endpoint caps `limit` at 500, so a bank with
/// more history than that needs several requests.
#[derive(Debug, Deserialize)]
pub struct ApiLlmRequestsPage {
    #[serde(default)]
    pub total: Option<usize>,
    #[serde(default)]
    pub items: Vec<RawApiLlmRequest>,
}

/// One entry of `GET /v1/{tenant}/banks`.
#[derive(Debug, Deserialize)]
pub struct ApiBank {
    pub bank_id: String,
}

/// Response shape of `GET /v1/{tenant}/banks`.
#[derive(Debug, Deserialize)]
pub struct ApiBanksResponse {
    #[serde(default)]
    pub banks: Vec<ApiBank>,
}

/// Options configuring `tokens hindsight sync`.
#[derive(Debug, Clone)]
pub struct SyncHindsightOptions {
    pub api: String,
    pub tenant: String,
    pub token: Option<String>,
    pub json: bool,
    pub home: Option<PathBuf>,
}

/// Summary result of a Hindsight ledger synchronization run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHindsightResult {
    pub synced: bool,
    pub total_requests: usize,
    pub new_requests: usize,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Resolves the root Hindsight directory (`$HINDSIGHT_HOME` or `~/.hindsight`).
///
/// When an explicit `home_dir` is supplied (from CLI `--home`), that takes precedence
/// over environment variables, maintaining hermetic report isolation.
pub fn resolve_hindsight_dir(home_dir: Option<&Path>) -> PathBuf {
    if let Some(home) = home_dir {
        home.join(".hindsight")
    } else if let Ok(val) = std::env::var("HINDSIGHT_HOME") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            PathBuf::from(trimmed)
        } else if let Some(home) = crate::paths::home_dir() {
            home.join(".hindsight")
        } else {
            PathBuf::from(".hindsight")
        }
    } else if let Some(home) = crate::paths::home_dir() {
        home.join(".hindsight")
    } else {
        PathBuf::from(".hindsight")
    }
}

/// Resolves the usage ledger directory (`$HINDSIGHT_HOME/usage`).
pub fn hindsight_usage_dir(home_dir: Option<&Path>) -> PathBuf {
    resolve_hindsight_dir(home_dir).join("usage")
}

/// Checks whether the Hindsight usage ledger directory exists and contains at least one `.jsonl` ledger.
pub fn has_hindsight_usage_cache_in_home(home_dir: Option<&Path>) -> bool {
    let usage_dir = hindsight_usage_dir(home_dir);
    if !usage_dir.exists() {
        return false;
    }

    match fs::read_dir(usage_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .any(|name| is_hindsight_usage_jsonl_filename(&name)),
        Err(_) => false,
    }
}

/// Helper to validate ledger filenames (e.g. `2026-09.jsonl`).
pub fn is_hindsight_usage_jsonl_filename(name: &str) -> bool {
    name.ends_with(".jsonl") && !name.starts_with('.')
}

/// Derives the month bucket string (`YYYY-MM`) from a request's `started_at` timestamp.
pub fn extract_month_bucket(started_at: &str) -> Option<String> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(started_at) {
        return Some(dt.format("%Y-%m").to_string());
    }
    if started_at.len() >= 7 {
        let prefix = &started_at[..7];
        let bytes = prefix.as_bytes();
        if bytes[0..4].iter().all(|b| b.is_ascii_digit())
            && bytes[4] == b'-'
            && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        {
            return Some(prefix.to_string());
        }
    }
    None
}

/// Converts a raw API request into a [`HindsightLedgerRow`], discarding content fields.
///
/// Returns `None` if `total_tokens` is missing or not strictly positive (> 0),
/// or if essential identifiers (`started_at`) are absent.
pub fn filter_valid_llm_request_row(
    raw: RawApiLlmRequest,
    fallback_bank_id: &str,
) -> Option<HindsightLedgerRow> {
    let total_tokens = raw.total_tokens?;
    if total_tokens <= 0 {
        return None;
    }
    let started_at = raw.started_at.filter(|s| !s.trim().is_empty())?;
    let provider = raw
        .provider
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let model = raw
        .model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let bank = raw
        .bank_id
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| fallback_bank_id.to_string());

    Some(HindsightLedgerRow {
        id: raw.id,
        trace_id: raw.trace_id.filter(|t| !t.trim().is_empty()),
        provider,
        model,
        operation: raw.operation.filter(|o| !o.trim().is_empty()),
        scope: raw.scope.filter(|s| !s.trim().is_empty()),
        started_at,
        ended_at: raw.ended_at.filter(|e| !e.trim().is_empty()),
        duration_ms: raw.duration_ms,
        input_tokens: raw.input_tokens,
        output_tokens: raw.output_tokens,
        cached_tokens: raw.cached_tokens,
        total_tokens: total_tokens as u64,
        bank,
    })
}

/// Reads existing ledger rows from a JSONL file.
pub fn read_ledger_file(path: &Path) -> Result<Vec<HindsightLedgerRow>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file =
        File::open(path).with_context(|| format!("Failed to open ledger file: {:?}", path))?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.with_context(|| {
            format!(
                "Failed to read line {} from ledger file {:?}",
                line_num + 1,
                path
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let row: HindsightLedgerRow = serde_json::from_str(trimmed).with_context(|| {
            format!(
                "Failed to parse JSON ledger row at line {} in {:?}",
                line_num + 1,
                path
            )
        })?;
        rows.push(row);
    }

    Ok(rows)
}

/// Merges incoming rows into existing rows by request `id` dedup identity.
///
/// Existing rows are preserved without modification. New rows are appended
/// and the combined collection is sorted by `(started_at, id)`.
/// Returns `(merged_rows, newly_added_count, newly_added_tokens)`.
pub fn merge_ledger_rows(
    existing: Vec<HindsightLedgerRow>,
    incoming: Vec<HindsightLedgerRow>,
) -> (Vec<HindsightLedgerRow>, usize, u64) {
    let mut seen_ids: HashSet<String> = existing.iter().map(|r| r.id.clone()).collect();
    let mut merged = existing;
    let mut new_count = 0;
    let mut new_tokens = 0;

    for row in incoming {
        if seen_ids.insert(row.id.clone()) {
            new_tokens += row.total_tokens;
            merged.push(row);
            new_count += 1;
        }
    }

    merged.sort_by(|a, b| (&a.started_at, &a.id).cmp(&(&b.started_at, &b.id)));
    (merged, new_count, new_tokens)
}

/// Atomically writes a slice of ledger rows to a `.jsonl` file via a temporary file + rename.
pub fn write_ledger_file_atomic(path: &Path, rows: &[HindsightLedgerRow]) -> Result<()> {
    let parent = path
        .parent()
        .context("Ledger path does not have a parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create directory: {:?}", parent))?;

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ledger.jsonl");
    let temp_name = format!(".{}.tmp.{}", filename, uuid::Uuid::new_v4());
    let temp_path = parent.join(temp_name);

    {
        let mut file = File::create(&temp_path)
            .with_context(|| format!("Failed to create temporary file: {:?}", temp_path))?;
        for row in rows {
            let json = serde_json::to_string(row)
                .with_context(|| format!("Failed to serialize ledger row: {:?}", row.id))?;
            file.write_all(json.as_bytes())
                .with_context(|| format!("Failed to write to temp file: {:?}", temp_path))?;
            file.write_all(b"\n").with_context(|| {
                format!("Failed to write newline to temp file: {:?}", temp_path)
            })?;
        }
        file.flush()
            .with_context(|| format!("Failed to flush temp file: {:?}", temp_path))?;
    }

    fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to atomically rename {:?} to {:?}", temp_path, path))?;

    Ok(())
}

/// Executes the synchronization of Hindsight LLM request logs into local monthly ledgers.
pub async fn sync_hindsight_ledger(options: &SyncHindsightOptions) -> SyncHindsightResult {
    let base_api = options.api.trim_end_matches('/');
    let tenant = options.tenant.trim();
    let auth_token = options
        .token
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            std::env::var("HINDSIGHT_API_API_TOKEN")
                .ok()
                .filter(|t| !t.trim().is_empty())
        });

    let client = match tokens_core::http::client_builder()
        .timeout(HINDSIGHT_HTTP_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return SyncHindsightResult {
                synced: false,
                total_requests: 0,
                new_requests: 0,
                total_tokens: 0,
                error: Some(format!("Failed to build HTTP client: {}", e)),
            };
        }
    };

    // Step 1: Discover banks
    let banks_url = format!("{}/v1/{}/banks", base_api, tenant);
    let mut req = client.get(&banks_url);
    if let Some(token) = &auth_token {
        req = req.bearer_auth(token);
    }

    let banks_resp = match req.send().await {
        Ok(resp) => resp,
        Err(e) => {
            return SyncHindsightResult {
                synced: false,
                total_requests: 0,
                new_requests: 0,
                total_tokens: 0,
                error: Some(format!("Failed to reach Hindsight banks endpoint: {}", e)),
            };
        }
    };

    if !banks_resp.status().is_success() {
        let status = banks_resp.status();
        return SyncHindsightResult {
            synced: false,
            total_requests: 0,
            new_requests: 0,
            total_tokens: 0,
            error: Some(format!("Hindsight banks endpoint returned HTTP {}", status)),
        };
    }

    let banks_body: ApiBanksResponse = match banks_resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return SyncHindsightResult {
                synced: false,
                total_requests: 0,
                new_requests: 0,
                total_tokens: 0,
                error: Some(format!("Failed to parse Hindsight banks response: {}", e)),
            };
        }
    };

    // A bank with no traces still appears here, so an empty request page is
    // normal rather than an error.
    let bank_ids: Vec<String> = banks_body
        .banks
        .into_iter()
        .map(|bank| bank.bank_id)
        .filter(|id| !id.trim().is_empty())
        .collect();

    if bank_ids.is_empty() {
        return SyncHindsightResult {
            synced: true,
            total_requests: 0,
            new_requests: 0,
            total_tokens: 0,
            error: None,
        };
    }

    // Step 2: For each bank, page LLM requests
    let mut fetched_by_month: HashMap<String, Vec<HindsightLedgerRow>> = HashMap::new();
    let mut observed_requests_count = 0;
    let mut sync_warning: Option<String> = None;

    for bank_id in &bank_ids {
        let mut offset = 0;
        let limit = 500;

        loop {
            let requests_url = format!(
                "{}/v1/{}/banks/{}/llm-requests?status=success&limit={}&offset={}",
                base_api, tenant, bank_id, limit, offset
            );
            let mut req = client.get(&requests_url);
            if let Some(token) = &auth_token {
                req = req.bearer_auth(token);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    sync_warning = Some(format!("Error querying bank '{}': {}", bank_id, e));
                    break;
                }
            };

            if !resp.status().is_success() {
                sync_warning = Some(format!(
                    "Bank '{}' requests endpoint returned HTTP {}",
                    bank_id,
                    resp.status()
                ));
                break;
            }

            let page: ApiLlmRequestsPage = match resp.json().await {
                Ok(p) => p,
                Err(e) => {
                    sync_warning = Some(format!(
                        "Failed to parse LLM requests for bank '{}': {}",
                        bank_id, e
                    ));
                    break;
                }
            };

            let total = page.total.unwrap_or(0);
            let items_len = page.items.len();

            for item in page.items {
                observed_requests_count += 1;
                if let Some(row) = filter_valid_llm_request_row(item, bank_id) {
                    if let Some(month) = extract_month_bucket(&row.started_at) {
                        fetched_by_month.entry(month).or_default().push(row);
                    }
                }
            }

            offset += items_len;
            if items_len == 0 || offset >= total {
                break;
            }
        }
    }

    // Step 3: Write out ledgers per month bucket
    let usage_dir = hindsight_usage_dir(options.home.as_deref());
    let mut newly_added_count = 0;
    let mut newly_added_tokens: u64 = 0;

    for (month, incoming_rows) in fetched_by_month {
        let month_file = usage_dir.join(format!("{}.jsonl", month));
        let existing_rows = match read_ledger_file(&month_file) {
            Ok(r) => r,
            Err(e) => {
                return SyncHindsightResult {
                    synced: false,
                    total_requests: observed_requests_count,
                    new_requests: newly_added_count,
                    total_tokens: newly_added_tokens,
                    error: Some(format!(
                        "Failed to read existing ledger {:?}: {}",
                        month_file, e
                    )),
                };
            }
        };

        let (merged, new_count, new_tokens) = merge_ledger_rows(existing_rows, incoming_rows);
        if new_count > 0 {
            newly_added_tokens += new_tokens;
            newly_added_count += new_count;

            if let Err(e) = write_ledger_file_atomic(&month_file, &merged) {
                return SyncHindsightResult {
                    synced: false,
                    total_requests: observed_requests_count,
                    new_requests: newly_added_count,
                    total_tokens: newly_added_tokens,
                    error: Some(format!("Failed to write ledger {:?}: {}", month_file, e)),
                };
            }
        }
    }

    SyncHindsightResult {
        synced: true,
        total_requests: observed_requests_count,
        new_requests: newly_added_count,
        total_tokens: newly_added_tokens,
        error: sync_warning,
    }
}

/// Entry point for `tokens hindsight sync`.
pub fn run_hindsight_sync(options: SyncHindsightOptions) -> Result<()> {
    use colored::Colorize;
    use tokio::runtime::Runtime;

    let json = options.json;
    let rt = Runtime::new().context("Failed to initialize Tokio runtime for Hindsight sync")?;
    let result = rt.block_on(sync_hindsight_ledger(&options));

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("\n  {}\n", "Hindsight - Sync".cyan());
    if result.synced {
        println!(
            "{}",
            format!(
                "  Synced {} new Hindsight LLM request(s) ({} total, {} tokens).",
                result.new_requests, result.total_requests, result.total_tokens
            )
            .green()
        );
        if let Some(error) = result.error {
            println!("{}", format!("  Warning: {}", error).yellow());
        }
    } else if let Some(error) = result.error {
        println!("{}", format!("  Sync failed: {}", error).red());
    } else {
        println!("{}", "  Sync failed.".red());
    }
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_month_bucketing_from_started_at() {
        // Standard ISO 8601 / RFC 3339 timestamps
        assert_eq!(
            extract_month_bucket("2026-09-01T08:16:51.357104+00:00"),
            Some("2026-09".to_string())
        );
        assert_eq!(
            extract_month_bucket("2026-12-31T23:59:59Z"),
            Some("2026-12".to_string())
        );
        assert_eq!(
            extract_month_bucket("2025-01-15T00:00:00.000000+09:00"),
            Some("2025-01".to_string())
        );
        // Prefix fallback
        assert_eq!(
            extract_month_bucket("2026-02-28 some custom format"),
            Some("2026-02".to_string())
        );
        // Invalid inputs
        assert_eq!(extract_month_bucket(""), None);
        assert_eq!(extract_month_bucket("not-a-date"), None);
        assert_eq!(extract_month_bucket("202-09-01"), None);
    }

    #[test]
    fn test_id_based_dedup_and_merge() {
        let existing = vec![
            HindsightLedgerRow {
                id: "req-1".to_string(),
                trace_id: Some("trace-1".to_string()),
                provider: "ollama-cloud".to_string(),
                model: "deepseek-v4-flash".to_string(),
                operation: Some("retain".to_string()),
                scope: Some("scope-1".to_string()),
                started_at: "2026-09-01T10:00:00Z".to_string(),
                ended_at: Some("2026-09-01T10:00:05Z".to_string()),
                duration_ms: Some(5000),
                input_tokens: Some(100),
                output_tokens: Some(50),
                cached_tokens: None,
                total_tokens: 150,
                bank: "omp".to_string(),
            },
            HindsightLedgerRow {
                id: "req-2".to_string(),
                trace_id: Some("trace-2".to_string()),
                provider: "ollama-cloud".to_string(),
                model: "deepseek-v4-flash".to_string(),
                operation: Some("retain".to_string()),
                scope: Some("scope-2".to_string()),
                started_at: "2026-09-01T12:00:00Z".to_string(),
                ended_at: Some("2026-09-01T12:00:04Z".to_string()),
                duration_ms: Some(4000),
                input_tokens: Some(200),
                output_tokens: Some(100),
                cached_tokens: None,
                total_tokens: 300,
                bank: "omp".to_string(),
            },
        ];

        let incoming = vec![
            // Duplicate of req-2 with altered total_tokens: existing must be preserved verbatim
            HindsightLedgerRow {
                id: "req-2".to_string(),
                trace_id: Some("trace-2-modified".to_string()),
                provider: "ollama-cloud".to_string(),
                model: "deepseek-v4-flash".to_string(),
                operation: Some("retain".to_string()),
                scope: Some("scope-2".to_string()),
                started_at: "2026-09-01T12:00:00Z".to_string(),
                ended_at: Some("2026-09-01T12:00:04Z".to_string()),
                duration_ms: Some(4000),
                input_tokens: Some(999),
                output_tokens: Some(999),
                cached_tokens: None,
                total_tokens: 1998,
                bank: "omp".to_string(),
            },
            // Genuinely new row with earlier timestamp (should sort first)
            HindsightLedgerRow {
                id: "req-3".to_string(),
                trace_id: Some("trace-3".to_string()),
                provider: "ollama-cloud".to_string(),
                model: "deepseek-v4-flash".to_string(),
                operation: Some("reflect".to_string()),
                scope: Some("scope-3".to_string()),
                started_at: "2026-09-01T09:00:00Z".to_string(),
                ended_at: Some("2026-09-01T09:00:02Z".to_string()),
                duration_ms: Some(2000),
                input_tokens: Some(50),
                output_tokens: Some(50),
                cached_tokens: None,
                total_tokens: 100,
                bank: "omp".to_string(),
            },
            // Duplicate of req-3 within the incoming batch itself
            HindsightLedgerRow {
                id: "req-3".to_string(),
                trace_id: Some("trace-3".to_string()),
                provider: "ollama-cloud".to_string(),
                model: "deepseek-v4-flash".to_string(),
                operation: Some("reflect".to_string()),
                scope: Some("scope-3".to_string()),
                started_at: "2026-09-01T09:00:00Z".to_string(),
                ended_at: Some("2026-09-01T09:00:02Z".to_string()),
                duration_ms: Some(2000),
                input_tokens: Some(50),
                output_tokens: Some(50),
                cached_tokens: None,
                total_tokens: 100,
                bank: "omp".to_string(),
            },
        ];

        let (merged, new_count, new_tokens) = merge_ledger_rows(existing, incoming);

        assert_eq!(new_count, 1, "Exactly one row is genuinely new");
        assert_eq!(
            new_tokens, 100,
            "New tokens must reflect only the newly added row"
        );
        assert_eq!(
            merged.len(),
            3,
            "Merged ledger should contain 3 rows in total"
        );

        // Verify sorting by (started_at, id)
        assert_eq!(merged[0].id, "req-3");
        assert_eq!(merged[1].id, "req-1");
        assert_eq!(merged[2].id, "req-2");

        // Verify existing req-2 was preserved untouched (total_tokens 300, not 1998)
        assert_eq!(merged[2].total_tokens, 300);
        assert_eq!(merged[2].trace_id, Some("trace-2".to_string()));
    }

    /// Build a raw API row the way the sync path does: straight off the wire.
    /// Constructing it by deserialisation rather than by struct literal is the
    /// point of the exercise — the payloads below carry `input`, `output`, and
    /// `metadata`, and the struct has no field for any of them, so a leak
    /// would have to survive serde ignoring the key entirely.
    fn raw_from_json(json: &str) -> RawApiLlmRequest {
        serde_json::from_str(json).expect("API row should deserialize")
    }

    #[test]
    fn test_skip_tokenless_or_non_positive_rows() {
        let valid_raw = raw_from_json(
            r#"{"id":"valid-1","trace_id":"trace-1","provider":"ollama-cloud","model":"deepseek-v4-flash","operation":"retain","scope":"retain_extract_facts","started_at":"2026-09-01T08:16:51.357104+00:00","ended_at":"2026-09-01T08:17:03.101270+00:00","duration_ms":11744,"input_tokens":3667,"output_tokens":4063,"cached_tokens":null,"total_tokens":7730,"bank_id":"omp","input":{"prompt":"sensitive content"},"output":{"completion":"secret answer"},"metadata":{"memory_id":"mem-123"}}"#,
        );

        let row = filter_valid_llm_request_row(valid_raw, "omp")
            .expect("Valid positive token request should pass filter");
        assert_eq!(row.id, "valid-1");
        assert_eq!(row.total_tokens, 7730);

        // A failed call: Hindsight nulls every token field.
        let null_tokens_raw = raw_from_json(
            r#"{"id":"failed-1","provider":"ollama-cloud","model":"deepseek-v4-flash","started_at":"2026-09-01T08:16:51.357104+00:00","input_tokens":null,"output_tokens":null,"cached_tokens":null,"total_tokens":null,"bank_id":"omp"}"#,
        );
        assert!(filter_valid_llm_request_row(null_tokens_raw, "omp").is_none());

        let zero_tokens_raw = raw_from_json(
            r#"{"id":"zero-1","provider":"ollama-cloud","model":"deepseek-v4-flash","started_at":"2026-09-01T08:16:51.357104+00:00","input_tokens":0,"output_tokens":0,"total_tokens":0,"bank_id":"omp"}"#,
        );
        assert!(filter_valid_llm_request_row(zero_tokens_raw, "omp").is_none());

        let negative_tokens_raw = raw_from_json(
            r#"{"id":"neg-1","provider":"ollama-cloud","model":"deepseek-v4-flash","started_at":"2026-09-01T08:16:51.357104+00:00","total_tokens":-10,"bank_id":"omp"}"#,
        );
        assert!(filter_valid_llm_request_row(negative_tokens_raw, "omp").is_none());

        // No start time means the row cannot be bucketed to a date.
        let missing_started_raw = raw_from_json(
            r#"{"id":"no-start-1","provider":"ollama-cloud","model":"deepseek-v4-flash","input_tokens":10,"output_tokens":10,"total_tokens":20,"bank_id":"omp"}"#,
        );
        assert!(filter_valid_llm_request_row(missing_started_raw, "omp").is_none());
    }

    /// The wire row above carries prompt and memory content; the ledger row
    /// written to disk must not, and cannot, because the fields do not exist.
    #[test]
    fn test_content_fields_are_never_deserialized_or_persisted() {
        let raw = raw_from_json(
            r#"{"id":"leak-check","provider":"ollama-cloud","model":"deepseek-v4-flash","started_at":"2026-09-01T08:16:51.357104+00:00","input_tokens":1,"output_tokens":1,"total_tokens":2,"bank_id":"omp","input":{"prompt":"SENSITIVE_PROMPT"},"output":{"completion":"SENSITIVE_COMPLETION"},"metadata":{"memory_id":"SENSITIVE_MEMORY"}}"#,
        );
        let row = filter_valid_llm_request_row(raw, "omp").expect("row should pass filter");
        let serialized = serde_json::to_string(&row).expect("row should serialize");
        for secret in [
            "SENSITIVE_PROMPT",
            "SENSITIVE_COMPLETION",
            "SENSITIVE_MEMORY",
        ] {
            assert!(
                !serialized.contains(secret),
                "ledger row leaked content: {serialized}"
            );
        }
    }

    #[test]
    fn test_serialization_contract_keys_no_content_leak() {
        let row = HindsightLedgerRow {
            id: "aa4cc970-33b8-4234-996d-b6e05a5a1bb2".to_string(),
            trace_id: Some("51b96021-07fc-4a64-b333-5c8c72833a44".to_string()),
            provider: "ollama-cloud".to_string(),
            model: "deepseek-v4-flash:0731".to_string(),
            operation: Some("retain".to_string()),
            scope: Some("retain_extract_facts".to_string()),
            started_at: "2026-09-01T08:16:51.357104+00:00".to_string(),
            ended_at: Some("2026-09-01T08:17:03.101270+00:00".to_string()),
            duration_ms: Some(11744),
            input_tokens: Some(3667),
            output_tokens: Some(4063),
            cached_tokens: None,
            total_tokens: 7730,
            bank: "omp".to_string(),
        };

        let json_str = serde_json::to_string(&row).expect("Serialization must succeed");
        let val: serde_json::Value =
            serde_json::from_str(&json_str).expect("Serialized JSON must parse");

        let obj = val
            .as_object()
            .expect("Row must serialize to a JSON object");

        // Exactly the 14 contract keys
        let expected_keys = [
            "id",
            "trace_id",
            "provider",
            "model",
            "operation",
            "scope",
            "started_at",
            "ended_at",
            "duration_ms",
            "input_tokens",
            "output_tokens",
            "cached_tokens",
            "total_tokens",
            "bank",
        ];

        assert_eq!(obj.len(), expected_keys.len());
        for key in &expected_keys {
            assert!(obj.contains_key(*key), "Missing expected key: {}", key);
        }

        // Verify cached_tokens is explicitly serialized as null
        assert!(obj.get("cached_tokens").unwrap().is_null());

        // Explicitly verify content fields cannot exist
        let forbidden_keys = [
            "input",
            "output",
            "metadata",
            "prompt",
            "completion",
            "messages",
            "content",
            "body",
        ];
        for forbidden in &forbidden_keys {
            assert!(
                !obj.contains_key(*forbidden),
                "Forbidden content key '{}' found in serialized ledger row",
                forbidden
            );
        }

        // Verify round-trip deserialization
        let deserialized: HindsightLedgerRow =
            serde_json::from_str(&json_str).expect("Deserialization must succeed");
        assert_eq!(row, deserialized);
    }

    #[test]
    fn test_atomic_write_and_read_ledger_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let ledger_file = temp_dir.path().join("usage").join("2026-09.jsonl");

        let rows = vec![
            HindsightLedgerRow {
                id: "req-1".to_string(),
                trace_id: Some("trace-1".to_string()),
                provider: "ollama-cloud".to_string(),
                model: "deepseek-v4-flash".to_string(),
                operation: Some("retain".to_string()),
                scope: Some("retain_extract_facts".to_string()),
                started_at: "2026-09-01T08:16:51.357104+00:00".to_string(),
                ended_at: Some("2026-09-01T08:17:03.101270+00:00".to_string()),
                duration_ms: Some(11744),
                input_tokens: Some(3667),
                output_tokens: Some(4063),
                cached_tokens: None,
                total_tokens: 7730,
                bank: "omp".to_string(),
            },
            HindsightLedgerRow {
                id: "req-2".to_string(),
                trace_id: None,
                provider: "ollama-cloud".to_string(),
                model: "deepseek-v4-flash".to_string(),
                operation: Some("recall".to_string()),
                scope: None,
                started_at: "2026-09-02T12:00:00.000000+00:00".to_string(),
                ended_at: None,
                duration_ms: None,
                input_tokens: Some(500),
                output_tokens: Some(200),
                cached_tokens: None,
                total_tokens: 700,
                bank: "george".to_string(),
            },
        ];

        write_ledger_file_atomic(&ledger_file, &rows)?;
        let read_back = read_ledger_file(&ledger_file)?;

        assert_eq!(rows, read_back);
        Ok(())
    }
}
