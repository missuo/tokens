use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::IsTerminal;
// Only the unix branch of `save_credentials` writes through the trait; the
// other one calls `fs::write`, which does not need it in scope.
#[cfg(unix)]
use std::io::Write;
use std::path::PathBuf;

const API_TOKEN_ENV_VAR: &str = "TOKENS_API_TOKEN";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub token: String,
    pub username: String,
    #[serde(rename = "avatarUrl", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiTokenSource {
    Environment,
    StoredCredentials,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiTokenAuth {
    pub token: String,
    pub username: Option<String>,
    pub source: ApiTokenSource,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    #[serde(rename = "deviceCode")]
    device_code: String,
    #[serde(rename = "userCode")]
    user_code: String,
    #[serde(rename = "verificationUrl")]
    verification_url: String,
    #[serde(rename = "expiresIn")]
    expires_in: u64,
    interval: u64,
}

/// Bounds on the server-supplied device-flow poll interval. `interval: 0` would
/// otherwise spin an unthrottled poll loop, and an absurdly large value would
/// stall the login with no output.
const MIN_POLL_INTERVAL_SECS: u64 = 1;
const MAX_POLL_INTERVAL_SECS: u64 = 60;

/// Bounds on the server-supplied code lifetime, which drives the poll deadline.
const MIN_CODE_LIFETIME_SECS: u64 = 60;
const MAX_CODE_LIFETIME_SECS: u64 = 30 * 60;

#[derive(Debug, Deserialize)]
struct PollResponse {
    // Optional because the server omits it entirely on its error paths:
    // web/src/app/api/auth/device/poll/route.ts returns a bare `{"error": …}`
    // with no `status` key. A required field made the whole body fail to
    // deserialize, so the error was silently dropped and login polled on to a
    // generic timeout — the exact symptom surfacing `error` was meant to fix.
    #[serde(default)]
    status: Option<String>,
    token: Option<String>,
    user: Option<UserInfo>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    username: String,
    #[serde(rename = "avatarUrl")]
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenValidationResponse {
    user: UserInfo,
}

fn get_credentials_path() -> Result<PathBuf> {
    Ok(crate::paths::get_config_dir().join("credentials.json"))
}

pub fn credentials_path() -> Result<PathBuf> {
    get_credentials_path()
}

fn ensure_config_dir() -> Result<()> {
    let config_dir = crate::paths::get_config_dir();

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

pub fn save_credentials(credentials: &Credentials) -> Result<()> {
    ensure_config_dir()?;
    let path = get_credentials_path()?;
    let json = serde_json::to_string_pretty(credentials)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::fs::PermissionsExt;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        // `.mode()` only applies when the file is created, so a credentials
        // file left behind by an older release (or the non-unix branch below)
        // would keep its original mode. Repair it on every write.
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(json.as_bytes())?;
    }

    #[cfg(not(unix))]
    {
        fs::write(&path, json)?;
    }

    Ok(())
}

/// Pre-`get_config_dir()` credential location: a hardcoded
/// `$HOME/.config/tokens/credentials.json` on every platform.
///
/// macOS resolves to the same path either way, but on Windows the config dir
/// is now `%APPDATA%\tokens` and on Linux it honours `XDG_CONFIG_HOME`, so
/// switching to the shared helper moved the file out from under everyone who
/// was already logged in on those platforms — silently, since a missing file
/// is indistinguishable from never having logged in. Gated on the override for
/// the same hermeticity reason as the cursor probe.
fn legacy_credentials_path() -> Option<PathBuf> {
    if crate::paths::is_config_dir_overridden() {
        return None;
    }
    Some(dirs::home_dir()?.join(".config/tokens/credentials.json"))
}

pub fn load_credentials() -> Option<Credentials> {
    let path = get_credentials_path().ok()?;

    let read_path = if path.exists() {
        path
    } else {
        legacy_credentials_path().filter(|legacy| legacy.exists() && *legacy != path)?
    };

    let content = fs::read_to_string(read_path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn load_api_token_from_env() -> Option<String> {
    let token = std::env::var(API_TOKEN_ENV_VAR).ok()?;
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub fn resolve_api_token() -> Option<ApiTokenAuth> {
    if let Some(token) = load_api_token_from_env() {
        return Some(ApiTokenAuth {
            token,
            username: None,
            source: ApiTokenSource::Environment,
        });
    }

    load_credentials().map(|credentials| ApiTokenAuth {
        token: credentials.token,
        username: Some(credentials.username),
        source: ApiTokenSource::StoredCredentials,
    })
}

pub fn clear_credentials() -> Result<bool> {
    let path = get_credentials_path()?;
    let mut cleared = false;

    if path.exists() {
        fs::remove_file(&path)?;
        cleared = true;
    }

    // `load_credentials` will still read the pre-migration file, so leaving it
    // behind means `tokens logout` reports success while a working token stays
    // on disk and the next command signs straight back in.
    if let Some(legacy) = legacy_credentials_path() {
        if legacy != path && legacy.exists() {
            fs::remove_file(&legacy)?;
            cleared = true;
        }
    }

    Ok(cleared)
}

pub fn get_api_base_url() -> String {
    std::env::var("TOKENS_API_URL").unwrap_or_else(|_| "https://tokens.ci".to_string())
}

fn get_device_name() -> String {
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    format!("CLI on {}", hostname)
}

#[cfg(target_os = "linux")]
fn has_non_empty_env_var(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn should_auto_open_browser() -> bool {
    has_non_empty_env_var("DISPLAY") || has_non_empty_env_var("WAYLAND_DISPLAY")
}

#[cfg(not(target_os = "linux"))]
fn should_auto_open_browser() -> bool {
    true
}

/// Validate a server-supplied verification URL before it reaches a terminal
/// escape sequence or the platform URL opener.
///
/// Control characters are rejected because `\x1b`/`\x07` would close the OSC-8
/// hyperlink early and inject terminal output. The scheme is restricted to
/// `https`, plus plain `http` on loopback so a locally hosted `TOKENS_API_URL`
/// still works — otherwise a compromised server could hand `open`/`xdg-open`/
/// `cmd /C start` a `file://` path or a custom URI-handler scheme.
/// Server-supplied text on its way to the terminal.
///
/// The verification URL is validated before it is printed, but the poll
/// response's free-form `error` string was going straight into `bail!`. That is
/// a wider hole than the one URL validation closed: an OSC/CSI sequence in it
/// can retitle the window, clear the screen, or paint a convincing fake prompt.
/// Drop control characters and bound the length — a legitimate server message
/// needs neither.
fn sanitize_server_text(text: &str) -> String {
    const MAX: usize = 300;
    let cleaned: String = text.chars().filter(|c| !c.is_control()).collect();
    if cleaned.chars().count() > MAX {
        let head: String = cleaned.chars().take(MAX).collect();
        format!("{head}…")
    } else {
        cleaned
    }
}

/// Characters `cmd.exe /C` treats as syntax. Rust's argument escaping does not
/// neutralise them when cmd is the program being invoked, so `start "" <url>`
/// would run whatever follows an `&`.
#[cfg(target_os = "windows")]
const CMD_METACHARACTERS: &[char] = &['&', '|', '<', '>', '^', '"', '%', '!'];

fn validate_verification_url(url: &str) -> Result<()> {
    if url.chars().any(|c| c.is_control()) {
        anyhow::bail!("Server returned a verification URL containing control characters.");
    }

    let parsed = reqwest::Url::parse(url)
        .map_err(|_| anyhow::anyhow!("Server returned an invalid verification URL."))?;

    let is_loopback_http = parsed.scheme() == "http"
        && matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));

    if parsed.scheme() != "https" && !is_loopback_http {
        anyhow::bail!(
            "Server returned a verification URL with an unsupported scheme: {}",
            parsed.scheme()
        );
    }

    // Userinfo lets a URL read as one host while resolving to another
    // (`https://accounts.google.com@evil.example/`), and nothing legitimate
    // puts credentials in a verification link.
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("Server returned a verification URL containing credentials.");
    }

    #[cfg(target_os = "windows")]
    if url.contains(CMD_METACHARACTERS) {
        anyhow::bail!("Server returned a verification URL containing shell metacharacters.");
    }

    Ok(())
}

fn open_browser(url: &str) -> bool {
    if !should_auto_open_browser() {
        return false;
    }

    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("open").arg(url).spawn().is_ok();
    }

    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .is_ok();
    }

    #[cfg(target_os = "linux")]
    {
        return std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok();
    }

    #[allow(unreachable_code)]
    false
}

pub async fn login() -> Result<()> {
    use colored::Colorize;

    if let Some(creds) = load_credentials() {
        println!(
            "\n  {}",
            format!("Already logged in as {}", creds.username.bold()).yellow()
        );
        println!(
            "{}",
            "  Run 'tokens logout' to sign out first.\n".bright_black()
        );
        return Ok(());
    }

    let base_url = get_api_base_url();

    println!("\n  {}\n", "Tokens - Login".cyan());
    println!("{}", "  Requesting authorization code...".bright_black());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let device_code_response = client
        .post(format!("{}/api/auth/device", base_url))
        .json(&serde_json::json!({
            "deviceName": get_device_name()
        }))
        .send()
        .await?;

    if !device_code_response.status().is_success() {
        anyhow::bail!("Server returned {}", device_code_response.status());
    }

    let device_data: DeviceCodeResponse = device_code_response.json().await?;

    validate_verification_url(&device_data.verification_url)?;

    println!();
    println!("{}", "  Open this URL in your browser:".white());
    let url_display = if std::io::stdout().is_terminal() {
        format!(
            "\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\",
            device_data.verification_url, device_data.verification_url
        )
    } else {
        device_data.verification_url.clone()
    };
    println!("{}", format!("  {}\n", url_display).cyan());
    println!("{}", "  Enter this code:".white());
    println!(
        "{}\n",
        format!("  {}", device_data.user_code).green().bold()
    );

    if !open_browser(&device_data.verification_url) {
        println!(
            "{}",
            "  Browser auto-open unavailable in this environment. Continue with the URL above.\n"
                .bright_black()
        );
    }

    println!("{}", "  Waiting for authorization...".bright_black());

    let poll_interval = std::time::Duration::from_secs(
        device_data
            .interval
            .clamp(MIN_POLL_INTERVAL_SECS, MAX_POLL_INTERVAL_SECS),
    );
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(
            device_data
                .expires_in
                .clamp(MIN_CODE_LIFETIME_SECS, MAX_CODE_LIFETIME_SECS),
        );

    loop {
        tokio::time::sleep(poll_interval).await;

        let poll_response = client
            .post(format!("{}/api/auth/device/poll", base_url))
            .json(&serde_json::json!({
                "deviceCode": device_data.device_code
            }))
            .send()
            .await;

        match poll_response {
            Ok(response) => {
                if let Ok(data) = response.json::<PollResponse>().await {
                    if data.status.as_deref() == Some("complete") {
                        if let (Some(token), Some(user)) = (data.token, data.user) {
                            let credentials = Credentials {
                                token,
                                username: user.username.clone(),
                                avatar_url: user.avatar_url,
                                created_at: chrono::Utc::now().to_rfc3339(),
                            };

                            save_credentials(&credentials)?;

                            println!(
                                "\n  {}",
                                format!("Success! Logged in as {}", user.username.bold()).green()
                            );
                            println!(
                                "{}",
                                "  You can now use 'tokens submit' to share your usage.\n"
                                    .bright_black()
                            );
                            return Ok(());
                        }
                    }

                    if data.status.as_deref() == Some("expired") {
                        anyhow::bail!("Authorization code expired. Please try again.");
                    }

                    // Any other rejection (revoked code, banned account, rate
                    // limit) is terminal, so show it instead of polling on to a
                    // generic timeout.
                    if let Some(error) = data
                        .error
                        .as_deref()
                        .map(str::trim)
                        .filter(|error| !error.is_empty())
                    {
                        println!();
                        anyhow::bail!("{}", sanitize_server_text(error));
                    }

                    print!("{}", ".".bright_black());
                    use std::io::Write;
                    std::io::stdout().flush()?;
                }
            }
            Err(_) => {
                print!("{}", "!".red());
                use std::io::Write;
                std::io::stdout().flush()?;
            }
        }

        if std::time::Instant::now() >= deadline {
            anyhow::bail!("Timeout: Authorization took too long. Please try again.");
        }
    }
}

pub async fn login_with_token(token: &str) -> Result<()> {
    use colored::Colorize;

    let token = token.trim();
    if token.is_empty() {
        anyhow::bail!("API token cannot be empty.");
    }
    if !token.starts_with("tt_") {
        anyhow::bail!("Tokens API tokens must start with `tt_`.");
    }

    let base_url = get_api_base_url();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = client
        .get(format!("{}/api/auth/token", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let error = body
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("API token validation failed");
        anyhow::bail!("{} ({})", error, status);
    }

    let data: TokenValidationResponse = response.json().await?;
    let credentials = Credentials {
        token: token.to_string(),
        username: data.user.username.clone(),
        avatar_url: data.user.avatar_url,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    save_credentials(&credentials)?;

    println!(
        "\n  {}",
        format!("Success! Logged in as {}", credentials.username.bold()).green()
    );
    println!(
        "{}",
        "  You can now use 'tokens submit' to share your usage.\n".bright_black()
    );

    Ok(())
}

pub fn logout() -> Result<()> {
    use colored::Colorize;

    let credentials = load_credentials();

    let Some(creds) = credentials else {
        println!("\n  {}\n", "Not logged in.".yellow());
        return Ok(());
    };

    let username = creds.username;
    let cleared = clear_credentials()?;

    if cleared {
        println!(
            "\n  {}\n",
            format!("Logged out from {}", username.bold()).green()
        );
    } else {
        anyhow::bail!("Failed to clear credentials.");
    }

    Ok(())
}

pub fn whoami() -> Result<()> {
    use colored::Colorize;

    let Some(creds) = load_credentials() else {
        println!("\n  {}", "Not logged in.".yellow());
        println!(
            "{}",
            "  Run 'tokens login' to authenticate.\n".bright_black()
        );
        return Ok(());
    };

    println!("\n  {}\n", "Tokens - Account Info".cyan());
    println!(
        "{}",
        format!("  Username:  {}", creds.username.bold()).white()
    );

    if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&creds.created_at) {
        println!(
            "{}",
            format!("  Logged in: {}", created.format("%Y-%m-%d")).bright_black()
        );
    }

    println!();

    Ok(())
}

