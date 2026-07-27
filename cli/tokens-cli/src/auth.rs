use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::IsTerminal;
// Only the unix branch of `save_credentials` writes through the trait; the
// other one calls `fs::write`, which does not need it in scope.
#[cfg(unix)]
use std::io::Write;
use std::path::PathBuf;

const API_TOKEN_ENV_VAR: &str = "TOKENS_API_TOKEN";

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("Could not determine home directory")
}

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
    #[allow(dead_code)]
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct PollResponse {
    status: String,
    token: Option<String>,
    user: Option<UserInfo>,
    #[allow(dead_code)]
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
    Ok(home_dir()?.join(".config/tokens/credentials.json"))
}

pub fn credentials_path() -> Result<PathBuf> {
    get_credentials_path()
}

fn ensure_config_dir() -> Result<()> {
    let config_dir = home_dir()?.join(".config/tokens");

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

        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(json.as_bytes())?;
    }

    #[cfg(not(unix))]
    {
        fs::write(&path, json)?;
    }

    Ok(())
}

pub fn load_credentials() -> Option<Credentials> {
    let path = get_credentials_path().ok()?;
    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(path).ok()?;
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
    if path.exists() {
        fs::remove_file(path)?;
        Ok(true)
    } else {
        Ok(false)
    }
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

    let poll_interval = std::time::Duration::from_secs(device_data.interval);
    let max_attempts = 180;

    for attempt in 0..max_attempts {
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
                    if data.status == "complete" {
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

                    if data.status == "expired" {
                        anyhow::bail!("Authorization code expired. Please try again.");
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

        if attempt >= max_attempts - 1 {
            anyhow::bail!("Timeout: Authorization took too long. Please try again.");
        }
    }

    Ok(())
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

