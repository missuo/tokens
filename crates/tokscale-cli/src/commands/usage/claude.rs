use anyhow::Result;
use serde::Deserialize;

use super::helpers::capitalize;
use super::{UsageMetric, UsageOutput};

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const BETA_HEADER: &str = "oauth-2025-04-20";

#[derive(Debug, Clone, Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<Oauth>,
}

#[derive(Debug, Clone, Deserialize)]
struct Oauth {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "refreshToken")]
    refresh_token: Option<String>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier")]
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
    seven_day_opus: Option<Window>,
}

#[derive(Debug, Deserialize)]
struct Window {
    utilization: f64,
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenRefresh {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CredentialSource {
    File,
    Keychain,
}

#[derive(Debug, Clone)]
struct CredentialCandidate {
    source: CredentialSource,
    oauth: Oauth,
}

fn read_keychain() -> Result<String> {
    super::helpers::read_keychain("Claude Code-credentials")
}

pub fn has_credentials() -> bool {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".claude").join(".credentials.json").exists()
        || super::helpers::read_keychain("Claude Code-credentials").is_ok()
}

fn credential_candidates_from_raw(
    keychain: Option<&str>,
    file: Option<&str>,
) -> Vec<CredentialCandidate> {
    let mut seen = std::collections::HashSet::new();
    [
        (CredentialSource::Keychain, keychain),
        (CredentialSource::File, file),
    ]
    .into_iter()
    .filter_map(|(source, raw)| {
        let creds = serde_json::from_str::<Credentials>(raw?).ok()?;
        let oauth = creds.claude_ai_oauth?;
        let access_token = oauth.access_token.as_deref()?.trim();
        let refresh_token = oauth.refresh_token.as_deref().map(str::trim);
        if access_token.is_empty()
            || !seen.insert((access_token.to_string(), refresh_token.map(str::to_string)))
        {
            return None;
        }
        Some(CredentialCandidate { source, oauth })
    })
    .collect()
}

fn read_credential_candidates() -> Result<Vec<CredentialCandidate>> {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = home.join(".claude").join(".credentials.json");
    let keychain = read_keychain().ok();
    let file = std::fs::read_to_string(path).ok();
    let candidates = credential_candidates_from_raw(keychain.as_deref(), file.as_deref());
    if candidates.is_empty() {
        anyhow::bail!("NEEDS_AUTH");
    }
    Ok(candidates)
}

fn save_credentials(
    access_token: &str,
    refresh_token: &str,
    subscription_type: Option<&str>,
    rate_limit_tier: Option<&str>,
) {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let path = home.join(".claude").join(".credentials.json");
    let mut oauth = serde_json::json!({
        "accessToken": access_token,
        "refreshToken": refresh_token,
    });
    if let Some(st) = subscription_type {
        oauth["subscriptionType"] = serde_json::Value::String(st.to_string());
    }
    if let Some(rlt) = rate_limit_tier {
        oauth["rateLimitTier"] = serde_json::Value::String(rlt.to_string());
    }
    let json = serde_json::json!({
        "claudeAiOauth": oauth
    });
    let content = match serde_json::to_string_pretty(&json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: failed to serialize Claude credentials: {e}");
            return;
        }
    };
    if let Err(e) = super::helpers::atomic_write_secret(&path, content.as_bytes()) {
        eprintln!("warning: failed to save Claude credentials: {e}");
    }
}

async fn refresh_token(client: &reqwest::Client, rt: &str) -> Result<TokenRefresh> {
    let resp = client
        .post("https://platform.claude.com/v1/oauth/token")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": rt,
            "client_id": CLIENT_ID,
            "scope": "user:profile user:inference user:sessions:claude_code user:mcp_servers"
        }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("Claude token refresh failed (HTTP {})", resp.status());
    }
    Ok(resp.json().await?)
}

async fn fetch_usage(client: &reqwest::Client, token: &str) -> Result<UsageResponse> {
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("anthropic-beta", BETA_HEADER)
        .send()
        .await?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("NEEDS_AUTH");
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        anyhow::bail!("RATE_LIMITED");
    }
    if !status.is_success() {
        anyhow::bail!("UNAVAILABLE");
    }
    Ok(resp.json().await?)
}

fn window_metric(label: &str, w: &Window) -> UsageMetric {
    let used = w.utilization.clamp(0.0, 100.0);
    UsageMetric {
        label: label.into(),
        used_percent: used,
        remaining_percent: 100.0 - used,
        remaining_label: None,
        resets_at: w.resets_at.clone(),
    }
}

pub fn fetch() -> Result<UsageOutput> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let client = reqwest::Client::new();
        let mut saw_rate_limit = false;
        let mut saw_auth_failure = false;
        for candidate in read_credential_candidates()? {
            let Some(access_token) = candidate.oauth.access_token.as_deref() else {
                continue;
            };
            let mut response = fetch_usage(&client, access_token).await;
            if response
                .as_ref()
                .is_err_and(|error| error.to_string().contains("NEEDS_AUTH"))
            {
                saw_auth_failure = true;
                if let Some(refresh) = candidate.oauth.refresh_token.as_deref() {
                    if let Ok(refreshed) = refresh_token(&client, refresh).await {
                        if let Some(new_access) = refreshed.access_token.as_deref() {
                            if candidate.source == CredentialSource::File {
                                save_credentials(
                                    new_access,
                                    refreshed.refresh_token.as_deref().unwrap_or(refresh),
                                    candidate.oauth.subscription_type.as_deref(),
                                    candidate.oauth.rate_limit_tier.as_deref(),
                                );
                            }
                            response = fetch_usage(&client, new_access).await;
                        }
                    }
                }
            }
            let resp = match response {
                Ok(response) => response,
                Err(error) => {
                    let marker = error.to_string();
                    saw_rate_limit |= marker.contains("RATE_LIMITED");
                    saw_auth_failure |= marker.contains("NEEDS_AUTH");
                    continue;
                }
            };
            let plan = candidate
                .oauth
                .subscription_type
                .as_ref()
                .map(|subscription| {
                    let tier = candidate
                        .oauth
                        .rate_limit_tier
                        .as_deref()
                        .and_then(|value| value.rsplit('_').next());
                    match tier {
                        Some(multiplier) => format!("{} {}", capitalize(subscription), multiplier),
                        None => capitalize(subscription),
                    }
                });
            let mut metrics = Vec::new();
            if let Some(ref window) = resp.five_hour {
                metrics.push(window_metric("Session", window));
            }
            if let Some(ref window) = resp.seven_day {
                metrics.push(window_metric("Weekly", window));
            }
            if let Some(ref window) = resp.seven_day_opus {
                metrics.push(window_metric("Opus", window));
            }
            return Ok(UsageOutput {
                provider: "Claude".into(),
                account: None,
                plan,
                email: None,
                metrics,
                reset_credits: None,
                credit_status: None,
                spend_control: None,
            });
        }
        if saw_rate_limit {
            anyhow::bail!("RATE_LIMITED");
        }
        if saw_auth_failure {
            anyhow::bail!("NEEDS_AUTH");
        }
        anyhow::bail!("UNAVAILABLE")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYCHAIN: &str = r#"{
        "claudeAiOauth": {
            "accessToken": "keychain-token",
            "refreshToken": "keychain-refresh",
            "subscriptionType": "max"
        }
    }"#;
    const FILE: &str = r#"{
        "claudeAiOauth": {
            "accessToken": "file-token",
            "refreshToken": "file-refresh",
            "subscriptionType": "pro"
        }
    }"#;

    #[test]
    fn credential_candidates_prefer_keychain_then_file() {
        let candidates = credential_candidates_from_raw(Some(KEYCHAIN), Some(FILE));

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].source, CredentialSource::Keychain);
        assert_eq!(candidates[1].source, CredentialSource::File);
        assert_eq!(
            candidates[0].oauth.access_token.as_deref(),
            Some("keychain-token")
        );
        assert_eq!(
            candidates[1].oauth.access_token.as_deref(),
            Some("file-token")
        );
    }

    #[test]
    fn credential_candidates_use_file_when_keychain_is_invalid() {
        let candidates = credential_candidates_from_raw(Some("not-json"), Some(FILE));

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].source, CredentialSource::File);
    }

    #[test]
    fn credential_candidates_dedupe_identical_tokens_preferring_keychain() {
        let candidates = credential_candidates_from_raw(Some(KEYCHAIN), Some(KEYCHAIN));

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].source, CredentialSource::Keychain);
    }

    #[test]
    fn credential_candidates_keep_distinct_refresh_fallbacks() {
        let file = KEYCHAIN.replace("keychain-refresh", "file-refresh");
        let candidates = credential_candidates_from_raw(Some(KEYCHAIN), Some(&file));

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].source, CredentialSource::Keychain);
        assert_eq!(candidates[1].source, CredentialSource::File);
    }

    #[test]
    fn credential_candidates_skip_missing_access_token() {
        let missing = r#"{"claudeAiOauth":{"refreshToken":"refresh-only"}}"#;
        let candidates = credential_candidates_from_raw(Some(missing), Some(FILE));

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].source, CredentialSource::File);
    }
}
