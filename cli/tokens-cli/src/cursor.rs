use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Timeout for every Cursor HTTP request. Picked to bound the worst case for
/// auto-sync (which runs synchronously before local reports and the TUI) while
/// still tolerating routine API latency. If the network is hung, the report
/// proceeds against cached data after this timeout instead of stalling forever.
const CURSOR_HTTP_TIMEOUT: Duration = Duration::from_secs(8);

fn build_cursor_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(CURSOR_HTTP_TIMEOUT)
        .build()
        .context("Failed to build Cursor HTTP client")
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("Could not determine home directory")
}

fn cursor_credentials_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".config/tokens/cursor-credentials.json")
}

fn old_cursor_credentials_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".tokens/cursor-credentials.json")
}

fn cursor_cache_dir(home_dir: &Path) -> PathBuf {
    home_dir.join(".config/tokens/cursor-cache")
}

fn old_cursor_cache_dir(home_dir: &Path) -> PathBuf {
    home_dir.join(".tokens/cursor-cache")
}

const USAGE_CSV_ENDPOINT: &str =
    "https://cursor.com/api/dashboard/export-usage-events-csv?strategy=tokens";
const USAGE_SUMMARY_ENDPOINT: &str = "https://cursor.com/api/usage-summary";

/// Marker file touched at the end of every `sync_cursor_cache` run (even when
/// some accounts fail). Its mtime gates secondary-account freshness checks so
/// a permanently-stale secondary (expired token, removed account, network
/// partition) does not force an implicit sync on every invocation.
const CURSOR_SYNC_ATTEMPT_MARKER: &str = "usage.last-sync-attempt";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorCredentials {
    #[serde(rename = "sessionToken")]
    pub session_token: String,
    #[serde(rename = "userId", skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CursorCredentialsStore {
    pub version: i32,
    #[serde(rename = "activeAccountId")]
    pub active_account_id: String,
    pub accounts: HashMap<String, CursorCredentials>,
}

#[derive(Debug, Serialize)]
pub struct AccountInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(rename = "userId", skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "isActive")]
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
pub struct SyncCursorResult {
    pub synced: bool,
    pub rows: usize,
    pub error: Option<String>,
}

pub fn get_cursor_credentials_path() -> Result<PathBuf> {
    Ok(cursor_credentials_path(&home_dir()?))
}

pub fn get_cursor_cache_dir() -> Result<PathBuf> {
    Ok(cursor_cache_dir(&home_dir()?))
}

fn migrate_cache_dir_from_old_path_in_home(home_dir: &Path) {
    let old_dir = old_cursor_cache_dir(home_dir);
    let new_dir = cursor_cache_dir(home_dir);
    if !new_dir.exists()
        && old_dir.exists()
        && fs::create_dir_all(&new_dir).is_ok()
        && copy_dir_recursive(&old_dir, &new_dir).is_ok()
    {
        let _ = fs::remove_dir_all(&old_dir);
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            fs::create_dir_all(&target)?;
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn build_cursor_headers(session_token: &str) -> reqwest::header::HeaderMap {
    use reqwest::header::HeaderValue;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Accept", HeaderValue::from_static("*/*"));
    headers.insert(
        "Accept-Language",
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    if let Ok(cookie) = format!("WorkosCursorSessionToken={}", session_token).parse() {
        headers.insert("Cookie", cookie);
    }
    headers.insert(
        "Referer",
        HeaderValue::from_static("https://www.cursor.com/settings"),
    );
    headers.insert(
        "User-Agent",
        HeaderValue::from_static("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
    );
    headers
}

fn count_cursor_csv_rows(csv_text: &str) -> usize {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_text.as_bytes());
    reader.records().filter_map(|r| r.ok()).count()
}

fn atomic_write_file(path: &std::path::Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path"))?;
    if !parent.exists() {
        fs::create_dir_all(parent)?;
    }

    let temp_name = format!(
        ".tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("cursor"),
        std::process::id()
    );
    let temp_path = parent.join(temp_name);

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp_path)?;
        file.write_all(contents.as_bytes())?;
    }

    #[cfg(not(unix))]
    {
        fs::write(&temp_path, contents)?;
    }

    if let Err(err) = fs::rename(&temp_path, path) {
        if path.exists() {
            match fs::copy(&temp_path, path) {
                Ok(_) => {
                    let _ = fs::remove_file(&temp_path);
                }
                Err(copy_err) => {
                    let _ = fs::remove_file(&temp_path);
                    return Err(anyhow::anyhow!(
                        "Failed to persist file with rename ({}) and copy fallback ({})",
                        err,
                        copy_err
                    ));
                }
            }
        } else {
            let _ = fs::remove_file(&temp_path);
            return Err(err.into());
        }
    }
    Ok(())
}

fn ensure_config_dir_in_home(home_dir: &Path) -> Result<()> {
    let config_dir = home_dir.join(".config/tokens");

    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

fn extract_user_id_from_session_token(token: &str) -> Option<String> {
    let token = token.trim();
    if token.contains("%3A%3A") {
        let user_id = token.split("%3A%3A").next()?.trim();
        if user_id.is_empty() {
            return None;
        }
        return Some(user_id.to_string());
    }
    if token.contains("::") {
        let user_id = token.split("::").next()?.trim();
        if user_id.is_empty() {
            return None;
        }
        return Some(user_id.to_string());
    }
    None
}

fn derive_account_id(session_token: &str) -> String {
    if let Some(user_id) = extract_user_id_from_session_token(session_token) {
        return user_id;
    }
    let mut hasher = Sha256::new();
    hasher.update(session_token.as_bytes());
    let hash = hasher.finalize();
    let hex = format!("{:x}", hash);
    format!("anon-{}", &hex[..12])
}

fn sanitize_account_id_for_filename(account_id: &str) -> String {
    let sanitized: String = account_id
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('-');
    let result = if trimmed.len() > 80 {
        &trimmed[..80]
    } else {
        trimmed
    };
    if result.is_empty() {
        "account".to_string()
    } else {
        result.to_string()
    }
}

pub fn load_credentials_store() -> Option<CursorCredentialsStore> {
    let home_dir = home_dir().ok()?;
    load_credentials_store_from_home(&home_dir)
}

fn load_credentials_store_from_home(home_dir: &Path) -> Option<CursorCredentialsStore> {
    let path = cursor_credentials_path(home_dir);
    let old_path = old_cursor_credentials_path(home_dir);
    let read_path = if path.exists() {
        path.clone()
    } else if old_path.exists() {
        old_path
    } else {
        return None;
    };

    let content = fs::read_to_string(&read_path).ok()?;

    if let Ok(mut store) = serde_json::from_str::<CursorCredentialsStore>(&content) {
        if store.version == 1 && !store.accounts.is_empty() {
            let mut changed = false;
            if !store.accounts.contains_key(&store.active_account_id) {
                if let Some(first_id) = store.accounts.keys().next().cloned() {
                    store.active_account_id = first_id;
                    changed = true;
                }
            }
            if changed || read_path != path {
                let _ = save_credentials_store_in_home(home_dir, &store);
            }
            if read_path != path {
                let _ = fs::remove_file(old_cursor_credentials_path(home_dir));
            }
            return Some(store);
        }
    }

    if let Ok(single) = serde_json::from_str::<CursorCredentials>(&content) {
        let account_id = derive_account_id(&single.session_token);
        let mut accounts = HashMap::new();
        accounts.insert(account_id.clone(), single);
        let migrated = CursorCredentialsStore {
            version: 1,
            active_account_id: account_id,
            accounts,
        };

        let _ = save_credentials_store_in_home(home_dir, &migrated);
        if read_path != path {
            let _ = fs::remove_file(old_cursor_credentials_path(home_dir));
        }
        return Some(migrated);
    }

    None
}

pub fn save_credentials_store(store: &CursorCredentialsStore) -> Result<()> {
    save_credentials_store_in_home(&home_dir()?, store)
}

fn save_credentials_store_in_home(home_dir: &Path, store: &CursorCredentialsStore) -> Result<()> {
    ensure_config_dir_in_home(home_dir)?;
    let path = cursor_credentials_path(home_dir);
    let json = serde_json::to_string_pretty(store)?;
    atomic_write_file(&path, &json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

fn resolve_account_id(store: &CursorCredentialsStore, name_or_id: &str) -> Option<String> {
    let needle = name_or_id.trim();
    if needle.is_empty() {
        return None;
    }

    if store.accounts.contains_key(needle) {
        return Some(needle.to_string());
    }

    let needle_lower = needle.to_lowercase();
    for (id, acct) in &store.accounts {
        if let Some(label) = &acct.label {
            if label.to_lowercase() == needle_lower {
                return Some(id.clone());
            }
        }
    }

    None
}

pub fn list_accounts() -> Vec<AccountInfo> {
    let store = match load_credentials_store() {
        Some(s) => s,
        None => return vec![],
    };

    let mut accounts: Vec<AccountInfo> = store
        .accounts
        .iter()
        .map(|(id, acct)| AccountInfo {
            id: id.clone(),
            label: acct.label.clone(),
            user_id: acct.user_id.clone(),
            created_at: acct.created_at.clone(),
            is_active: id == &store.active_account_id,
        })
        .collect();

    accounts.sort_by(|a, b| {
        if a.is_active != b.is_active {
            return if a.is_active {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let la = a.label.as_deref().unwrap_or(&a.id).to_lowercase();
        let lb = b.label.as_deref().unwrap_or(&b.id).to_lowercase();
        la.cmp(&lb)
    });

    accounts
}

pub fn find_account(name_or_id: &str) -> Option<AccountInfo> {
    let store = load_credentials_store()?;
    let resolved = resolve_account_id(&store, name_or_id)?;
    let acct = store.accounts.get(&resolved)?;

    Some(AccountInfo {
        id: resolved.clone(),
        label: acct.label.clone(),
        user_id: acct.user_id.clone(),
        created_at: acct.created_at.clone(),
        is_active: resolved == store.active_account_id,
    })
}

pub fn save_credentials(token: &str, label: Option<&str>) -> Result<String> {
    let account_id = derive_account_id(token);
    let user_id = extract_user_id_from_session_token(token);

    let mut store = load_credentials_store().unwrap_or_else(|| CursorCredentialsStore {
        version: 1,
        active_account_id: account_id.clone(),
        accounts: HashMap::new(),
    });

    if let Some(lbl) = label {
        let needle = lbl.trim().to_lowercase();
        if !needle.is_empty() {
            for (id, acct) in &store.accounts {
                if id == &account_id {
                    continue;
                }
                if let Some(existing_label) = &acct.label {
                    if existing_label.trim().to_lowercase() == needle {
                        anyhow::bail!("Cursor account label already exists: {}", lbl);
                    }
                }
            }
        }
    }

    let credentials = CursorCredentials {
        session_token: token.to_string(),
        user_id,
        created_at: chrono::Utc::now().to_rfc3339(),
        expires_at: None,
        label: label.map(|s| s.to_string()),
    };

    store.accounts.insert(account_id.clone(), credentials);
    store.active_account_id = account_id.clone();

    save_credentials_store(&store)?;

    Ok(account_id)
}

pub fn remove_account(name_or_id: &str, purge_cache: bool) -> Result<()> {
    let mut store =
        load_credentials_store().ok_or_else(|| anyhow::anyhow!("No saved Cursor accounts"))?;

    let resolved = resolve_account_id(&store, name_or_id)
        .ok_or_else(|| anyhow::anyhow!("Account not found: {}", name_or_id))?;

    let was_active = resolved == store.active_account_id;

    let cache_dir = get_cursor_cache_dir()?;
    if cache_dir.exists() {
        let per_account = cache_dir.join(format!(
            "usage.{}.csv",
            sanitize_account_id_for_filename(&resolved)
        ));
        if per_account.exists() {
            if purge_cache {
                let _ = fs::remove_file(&per_account);
            } else {
                let _ = archive_cache_file(&per_account, &format!("usage.{}", resolved));
            }
        }
        if was_active {
            let active_file = cache_dir.join("usage.csv");
            if active_file.exists() {
                if purge_cache {
                    let _ = fs::remove_file(&active_file);
                } else {
                    let _ = archive_cache_file(&active_file, &format!("usage.active.{}", resolved));
                }
            }
        }
    }

    store.accounts.remove(&resolved);

    if store.accounts.is_empty() {
        let path = get_cursor_credentials_path()?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }

    if was_active {
        if let Some(first_id) = store.accounts.keys().next().cloned() {
            let new_account_file = cache_dir.join(format!(
                "usage.{}.csv",
                sanitize_account_id_for_filename(&first_id)
            ));
            let active_file = cache_dir.join("usage.csv");
            if new_account_file.exists() {
                let _ = fs::rename(&new_account_file, &active_file);
            }
            store.active_account_id = first_id;
        }
    }

    save_credentials_store(&store)?;
    Ok(())
}

pub fn remove_all_accounts(purge_cache: bool) -> Result<()> {
    let cache_dir = get_cursor_cache_dir()?;
    if cache_dir.exists() {
        if let Ok(entries) = fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("usage") && name.ends_with(".csv") {
                    if purge_cache {
                        let _ = fs::remove_file(entry.path());
                    } else {
                        let _ = archive_cache_file(&entry.path(), &format!("usage.all.{}", name));
                    }
                }
            }
        }
    }

    let path = get_cursor_credentials_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn set_active_account(name_or_id: &str) -> Result<()> {
    let mut store =
        load_credentials_store().ok_or_else(|| anyhow::anyhow!("No saved Cursor accounts"))?;

    let resolved = resolve_account_id(&store, name_or_id)
        .ok_or_else(|| anyhow::anyhow!("Account not found: {}", name_or_id))?;

    let old_active_id = store.active_account_id.clone();

    if resolved != old_active_id {
        let _ = reconcile_cache_files(&old_active_id, &resolved);
    }

    store.active_account_id = resolved;
    save_credentials_store(&store)?;

    Ok(())
}

fn reconcile_cache_files(old_account_id: &str, new_account_id: &str) -> Result<()> {
    let cache_dir = get_cursor_cache_dir()?;
    if !cache_dir.exists() {
        return Ok(());
    }

    let active_file = cache_dir.join("usage.csv");
    let old_account_file = cache_dir.join(format!(
        "usage.{}.csv",
        sanitize_account_id_for_filename(old_account_id)
    ));
    let new_account_file = cache_dir.join(format!(
        "usage.{}.csv",
        sanitize_account_id_for_filename(new_account_id)
    ));

    if active_file.exists() {
        if old_account_file.exists() {
            let _ = archive_cache_file(&old_account_file, old_account_id);
        }
        fs::rename(&active_file, &old_account_file)?;
    }

    if new_account_file.exists() {
        if active_file.exists() {
            let _ = archive_cache_file(&active_file, "usage.active");
        }
        fs::rename(&new_account_file, &active_file)?;
    }

    Ok(())
}

pub fn load_active_credentials() -> Option<CursorCredentials> {
    let store = load_credentials_store()?;
    store.accounts.get(&store.active_account_id).cloned()
}


fn is_cursor_usage_csv_filename(name: &str) -> bool {
    if name == "usage.csv" {
        return true;
    }
    if !name.starts_with("usage.") || !name.ends_with(".csv") {
        return false;
    }
    if name.starts_with("usage.backup") {
        return false;
    }
    let stem = name.trim_start_matches("usage.").trim_end_matches(".csv");
    !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

pub fn has_cursor_usage_cache_in_home(home_dir: &Path) -> bool {
    migrate_cache_dir_from_old_path_in_home(home_dir);
    let cache_dir = cursor_cache_dir(home_dir);
    if !cache_dir.exists() {
        return false;
    }

    match fs::read_dir(cache_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .any(|name| is_cursor_usage_csv_filename(&name)),
        Err(_) => false,
    }
}

pub fn is_cursor_logged_in() -> bool {
    load_active_credentials().is_some()
}

pub fn load_credentials_for(name_or_id: &str) -> Option<CursorCredentials> {
    let store = load_credentials_store()?;
    let resolved = resolve_account_id(&store, name_or_id)?;
    store.accounts.get(&resolved).cloned()
}

#[derive(Debug)]
pub struct ValidateSessionResult {
    pub valid: bool,
    pub membership_type: Option<String>,
    pub error: Option<String>,
}

pub async fn validate_cursor_session(token: &str) -> ValidateSessionResult {
    let client = match build_cursor_http_client() {
        Ok(client) => client,
        Err(e) => {
            return ValidateSessionResult {
                valid: false,
                membership_type: None,
                error: Some(format!("Failed to build HTTP client: {}", e)),
            };
        }
    };
    let response = match client
        .get(USAGE_SUMMARY_ENDPOINT)
        .headers(build_cursor_headers(token))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return ValidateSessionResult {
                valid: false,
                membership_type: None,
                error: Some(format!("Failed to connect: {}", e)),
            };
        }
    };

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        return ValidateSessionResult {
            valid: false,
            membership_type: None,
            error: Some("Session token expired or invalid".to_string()),
        };
    }

    if !response.status().is_success() {
        return ValidateSessionResult {
            valid: false,
            membership_type: None,
            error: Some(format!("API returned status {}", response.status())),
        };
    }

    let data: serde_json::Value = match response.json().await {
        Ok(d) => d,
        Err(e) => {
            return ValidateSessionResult {
                valid: false,
                membership_type: None,
                error: Some(format!("Failed to parse response: {}", e)),
            };
        }
    };

    let has_billing_start = data
        .get("billingCycleStart")
        .and_then(|v| v.as_str())
        .is_some();
    let has_billing_end = data
        .get("billingCycleEnd")
        .and_then(|v| v.as_str())
        .is_some();

    if has_billing_start && has_billing_end {
        let membership_type = data
            .get("membershipType")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        ValidateSessionResult {
            valid: true,
            membership_type,
            error: None,
        }
    } else {
        ValidateSessionResult {
            valid: false,
            membership_type: None,
            error: Some("Invalid response format".to_string()),
        }
    }
}

pub async fn fetch_cursor_usage_csv(session_token: &str) -> Result<String> {
    let client = build_cursor_http_client()?;
    let response = client
        .get(USAGE_CSV_ENDPOINT)
        .headers(build_cursor_headers(session_token))
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        anyhow::bail!(
            "Cursor session expired. Please run 'tokens cursor login' to re-authenticate."
        );
    }

    if !response.status().is_success() {
        anyhow::bail!("Cursor API returned status {}", response.status());
    }

    let text = response.text().await?;

    if !text.starts_with("Date,") {
        anyhow::bail!("Invalid response from Cursor API - expected CSV format");
    }

    Ok(text)
}

async fn sync_cursor_cache_with_fetcher<F, Fut>(fetch_usage_csv: F) -> SyncCursorResult
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    let home_dir = match home_dir() {
        Ok(home_dir) => home_dir,
        Err(e) => {
            return SyncCursorResult {
                synced: false,
                rows: 0,
                error: Some(format!("Failed to get home dir: {}", e)),
            };
        }
    };

    sync_cursor_cache_with_fetcher_in_home(&home_dir, fetch_usage_csv).await
}

async fn sync_cursor_cache_with_fetcher_in_home<F, Fut>(
    home_dir: &Path,
    fetch_usage_csv: F,
) -> SyncCursorResult
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    migrate_cache_dir_from_old_path_in_home(home_dir);

    let store = match load_credentials_store_from_home(home_dir) {
        Some(s) => s,
        None => {
            return SyncCursorResult {
                synced: false,
                rows: 0,
                error: Some("Not authenticated".to_string()),
            };
        }
    };

    if store.accounts.is_empty() {
        return SyncCursorResult {
            synced: false,
            rows: 0,
            error: Some("Not authenticated".to_string()),
        };
    }

    let cache_dir = cursor_cache_dir(home_dir);
    if let Err(e) = fs::create_dir_all(&cache_dir) {
        return SyncCursorResult {
            synced: false,
            rows: 0,
            error: Some(format!("Failed to create cache dir: {}", e)),
        };
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&cache_dir, fs::Permissions::from_mode(0o700));
    }

    let active_dup = cache_dir.join(format!(
        "usage.{}.csv",
        sanitize_account_id_for_filename(&store.active_account_id)
    ));
    if active_dup.exists() {
        let _ = fs::remove_file(&active_dup);
    }

    let mut total_rows = 0;
    let mut success_count = 0;
    let mut errors: Vec<String> = Vec::new();

    for (account_id, credentials) in &store.accounts {
        let is_active = account_id == &store.active_account_id;

        match fetch_usage_csv(credentials.session_token.clone()).await {
            Ok(csv_text) => {
                let file_path = if is_active {
                    cache_dir.join("usage.csv")
                } else {
                    cache_dir.join(format!(
                        "usage.{}.csv",
                        sanitize_account_id_for_filename(account_id)
                    ))
                };

                let row_count = count_cursor_csv_rows(&csv_text);

                if let Err(e) = atomic_write_file(&file_path, &csv_text) {
                    errors.push(format!("{}: {}", account_id, e));
                } else {
                    total_rows += row_count;
                    success_count += 1;
                }
            }
            Err(e) => {
                errors.push(format!("{}: {}", account_id, e));
            }
        }
    }

    // Touch the sync-attempt marker unconditionally after the per-account loop
    // (regardless of partial failures). The marker's mtime short-circuits the
    // secondary-account freshness check so a permanently-stale secondary
    // doesn't force an implicit sync on every invocation. We ignore errors
    // here — if the marker can't be written (e.g. disk full) the gate simply
    // falls through to the CSV-freshness check as before.
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(cache_dir.join(CURSOR_SYNC_ATTEMPT_MARKER));

    if success_count == 0 {
        return SyncCursorResult {
            synced: false,
            rows: 0,
            error: Some(
                errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Cursor sync failed".to_string()),
            ),
        };
    }

    SyncCursorResult {
        synced: true,
        rows: total_rows,
        error: if errors.is_empty() {
            None
        } else {
            Some(format!(
                "Some accounts failed to sync ({}/{})",
                errors.len(),
                store.accounts.len()
            ))
        },
    }
}

pub async fn sync_cursor_cache() -> SyncCursorResult {
    sync_cursor_cache_with_fetcher(|session_token| async move {
        fetch_cursor_usage_csv(&session_token).await
    })
    .await
}

fn archive_cache_file(file_path: &std::path::Path, label: &str) -> Result<()> {
    let cache_dir = get_cursor_cache_dir()?;
    let archive_dir = cache_dir.join("archive");
    if !archive_dir.exists() {
        fs::create_dir_all(&archive_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&archive_dir, fs::Permissions::from_mode(0o700))?;
        }
    }

    let safe_label = sanitize_account_id_for_filename(label);
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let dest = archive_dir.join(format!("{}-{}.csv", safe_label, ts));
    fs::rename(file_path, dest)?;
    Ok(())
}

pub fn run_cursor_login(name: Option<String>) -> Result<()> {
    use colored::Colorize;
    use tokio::runtime::Runtime;

    let rt = Runtime::new()?;

    println!("\n  {}\n", "Cursor IDE - Login".cyan());

    if let Some(ref label) = name {
        if find_account(label).is_some() {
            println!(
                "  {}",
                format!(
                    "Account '{}' already exists. Use 'tokens cursor logout --name {}' first.",
                    label, label
                )
                .yellow()
            );
            println!();
            return Ok(());
        }
    }

    print!("  Enter Cursor WorkosCursorSessionToken value: ");
    std::io::stdout().flush()?;
    let token = rpassword::read_password().context("Failed to read session token")?;
    let token = token.trim().to_string();

    if token.is_empty() {
        println!("\n  {}\n", "No token provided.".yellow());
        return Ok(());
    }

    println!();
    println!("{}", "  Validating session token...".bright_black());

    let result = rt.block_on(async { validate_cursor_session(&token).await });

    if !result.valid {
        let msg = result
            .error
            .unwrap_or_else(|| "Invalid session token".to_string());
        println!(
            "\n  {}\n",
            format!("{}. Please check and try again.", msg).red()
        );
        std::process::exit(1);
    }

    let account_id = save_credentials(&token, name.as_deref())?;

    let display_name = name.as_deref().unwrap_or(&account_id);
    println!(
        "\n  {}",
        format!(
            "Successfully logged in to Cursor as {}",
            display_name.bold()
        )
        .green()
    );
    println!("{}", format!("  Account ID: {}", account_id).bright_black());
    println!();

    Ok(())
}

pub fn run_cursor_logout(name: Option<String>, all: bool, purge_cache: bool) -> Result<()> {
    use colored::Colorize;

    if all {
        let accounts = list_accounts();
        if accounts.is_empty() {
            println!("\n  {}\n", "No saved Cursor accounts.".yellow());
            return Ok(());
        }

        remove_all_accounts(purge_cache)?;
        println!("\n  {}\n", "Logged out from all Cursor accounts.".green());
        return Ok(());
    }

    if let Some(ref account_name) = name {
        remove_account(account_name, purge_cache)?;
        println!(
            "\n  {}\n",
            format!("Logged out from Cursor account '{}'.", account_name).green()
        );
        return Ok(());
    }

    let Some(store) = load_credentials_store() else {
        println!("\n  {}\n", "No saved Cursor accounts.".yellow());
        return Ok(());
    };
    let active_id = store.active_account_id.clone();
    let display = store
        .accounts
        .get(&active_id)
        .and_then(|a| a.label.clone())
        .unwrap_or_else(|| active_id.clone());

    remove_account(&active_id, purge_cache)?;
    println!(
        "\n  {}\n",
        format!("Logged out from Cursor account '{}'.", display).green()
    );

    Ok(())
}

pub fn run_cursor_status(name: Option<String>) -> Result<()> {
    use colored::Colorize;
    use tokio::runtime::Runtime;

    let rt = Runtime::new()?;

    let credentials = if let Some(ref account_name) = name {
        load_credentials_for(account_name)
    } else {
        load_active_credentials()
    };

    let credentials = match credentials {
        Some(c) => c,
        None => {
            if let Some(ref account_name) = name {
                println!(
                    "\n  {}\n",
                    format!("Account not found: {}", account_name).red()
                );
            } else {
                println!("\n  {}", "No saved Cursor accounts.".yellow());
                println!(
                    "{}",
                    "  Run 'tokens cursor login' to authenticate.\n".bright_black()
                );
            }
            return Ok(());
        }
    };

    println!("\n  {}\n", "Cursor IDE - Status".cyan());

    let display_name = credentials.label.as_deref().unwrap_or("(no label)");
    println!("{}", format!("  Account: {}", display_name).white());
    if let Some(ref uid) = credentials.user_id {
        println!("{}", format!("  User ID: {}", uid).bright_black());
    }

    println!("{}", "  Validating session...".bright_black());

    let result = rt.block_on(async { validate_cursor_session(&credentials.session_token).await });

    if result.valid {
        println!("  {}", "Session: Valid".green());
        if let Some(membership) = result.membership_type {
            println!("{}", format!("  Membership: {}", membership).bright_black());
        }
    } else {
        let msg = result
            .error
            .unwrap_or_else(|| "Invalid / Expired".to_string());
        println!("  {}", format!("Session: {}", msg).red());
    }
    println!();

    Ok(())
}

pub fn run_cursor_accounts(json: bool) -> Result<()> {
    use colored::Colorize;

    let accounts = list_accounts();

    if json {
        #[derive(Serialize)]
        struct Output {
            accounts: Vec<AccountInfo>,
        }
        let output = Output { accounts };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if accounts.is_empty() {
        println!("\n  {}\n", "No saved Cursor accounts.".yellow());
        return Ok(());
    }

    println!("{}", "\n  Cursor IDE - Accounts\n".cyan());
    for acct in &accounts {
        let name = if let Some(ref label) = acct.label {
            format!("{} ({})", label, acct.id)
        } else {
            acct.id.clone()
        };
        let marker = if acct.is_active { "*" } else { "-" };
        let marker_colored = if acct.is_active {
            marker.green().to_string()
        } else {
            marker.bright_black().to_string()
        };
        println!("  {} {}", marker_colored, name);
    }
    println!();

    Ok(())
}

pub fn run_cursor_sync(json: bool) -> Result<()> {
    use colored::Colorize;
    use tokio::runtime::Runtime;

    let rt = Runtime::new()?;
    let result = rt.block_on(sync_cursor_cache());

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("\n  {}\n", "Cursor IDE - Sync".cyan());
    if result.synced {
        println!(
            "{}",
            format!("  Synced {} Cursor usage event(s).", result.rows).green()
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

pub fn run_cursor_switch(name: &str) -> Result<()> {
    use colored::Colorize;

    set_active_account(name)?;
    println!(
        "\n  {}\n",
        format!("Active Cursor account set to {}", name.bold()).green()
    );

    Ok(())
}

