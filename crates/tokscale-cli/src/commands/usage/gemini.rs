use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

use super::{UsageMetric, UsageOutput};

const QUOTA_ENDPOINT: &str = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";

#[derive(Debug, Deserialize)]
struct OAuthCreds {
    access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuotaResponse {
    buckets: Option<Vec<QuotaBucket>>,
}

#[derive(Debug, Deserialize)]
struct QuotaBucket {
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    #[serde(rename = "remainingFraction")]
    remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

fn creds_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".gemini")
        .join("oauth_creds.json")
}

pub fn has_credentials() -> bool {
    creds_path().exists()
}

fn read_credentials() -> Result<OAuthCreds> {
    let content = std::fs::read_to_string(creds_path())?;
    Ok(serde_json::from_str(&content)?)
}

fn locate_gemini() -> Option<std::path::PathBuf> {
    for candidate in ["/opt/homebrew/bin/gemini", "/usr/local/bin/gemini"] {
        let path = std::path::PathBuf::from(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(home) = dirs::home_dir() {
        let path = home.join(".local/bin/gemini");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Refresh the OAuth token by briefly running the Gemini CLI, which refreshes its
/// own token (writing back to oauth_creds.json) on startup. This avoids embedding
/// gemini-cli's OAuth client secret in our source — the CLI owns that. Mirrors how
/// the CLI is the system of record for Gemini auth.
fn refresh_via_cli() -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let gemini = locate_gemini().ok_or_else(|| anyhow::anyhow!("gemini CLI not found"))?;
    let mut child = Command::new(gemini)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"/quit\n");
    }
    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None => {
                if start.elapsed() > Duration::from_secs(20) {
                    let _ = child.kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Ok(())
}

async fn fetch_quota(client: &reqwest::Client, token: &str) -> Result<QuotaResponse> {
    let resp = client
        .post(QUOTA_ENDPOINT)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("NEEDS_AUTH");
    }
    if !status.is_success() {
        anyhow::bail!("Gemini quota request failed (HTTP {status})");
    }
    Ok(resp.json().await?)
}

/// Code Assist gates daily request quota by tier (Pro / Flash / Flash-Lite) but
/// exposes the same bucket under every model alias the user may call. Collapse
/// aliases to their tier, keeping the lowest remaining fraction, so the menu
/// shows one row per tier instead of the same number several times.
/// Flash-Lite must be checked before Flash ("flash" is a substring of both).
fn tier_for(model_id: &str) -> Option<&'static str> {
    let lower = model_id.to_lowercase();
    if lower.contains("flash-lite") {
        Some("flash-lite")
    } else if lower.contains("flash") {
        Some("flash")
    } else if lower.contains("pro") {
        Some("pro")
    } else {
        None
    }
}

fn tier_label(tier: &str) -> String {
    match tier {
        "pro" => "Pro".to_string(),
        "flash" => "Flash".to_string(),
        "flash-lite" => "Flash Lite".to_string(),
        other => other.to_string(),
    }
}

fn tier_order(tier: &str) -> i32 {
    match tier {
        "pro" => 0,
        "flash" => 1,
        "flash-lite" => 2,
        _ => 99,
    }
}

fn build_metrics(buckets: &[QuotaBucket]) -> Vec<UsageMetric> {
    struct Entry {
        label: String,
        fraction: f64,
        reset: Option<String>,
        order: i32,
    }
    let mut grouped: HashMap<String, Entry> = HashMap::new();
    for bucket in buckets {
        let (Some(model_id), Some(fraction)) =
            (bucket.model_id.as_ref(), bucket.remaining_fraction)
        else {
            continue;
        };
        let (key, label, order) = match tier_for(model_id) {
            Some(tier) => (tier.to_string(), tier_label(tier), tier_order(tier)),
            None => (model_id.clone(), model_id.clone(), 99),
        };
        grouped
            .entry(key)
            .and_modify(|existing| {
                if fraction < existing.fraction {
                    existing.fraction = fraction;
                    existing.reset = bucket.reset_time.clone();
                }
            })
            .or_insert(Entry {
                label,
                fraction,
                reset: bucket.reset_time.clone(),
                order,
            });
    }
    let mut entries: Vec<Entry> = grouped.into_values().collect();
    entries.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.label.cmp(&b.label)));
    entries
        .into_iter()
        .map(|entry| {
            let remaining = (entry.fraction * 100.0).clamp(0.0, 100.0);
            UsageMetric {
                label: entry.label,
                used_percent: 100.0 - remaining,
                remaining_percent: remaining,
                remaining_label: None,
                resets_at: entry.reset,
            }
        })
        .collect()
}

pub fn fetch() -> Result<UsageOutput> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let creds = read_credentials()?;
        let client = reqwest::Client::new();

        // Access tokens are short-lived (~1h). Try the cached one; on 401 ask the
        // Gemini CLI to refresh (it writes a fresh token back to oauth_creds.json),
        // then re-read and retry once.
        let token = creds
            .access_token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No Gemini access token"))?;
        let resp = match fetch_quota(&client, &token).await {
            Ok(resp) => resp,
            Err(e) if e.to_string().contains("NEEDS_AUTH") => {
                refresh_via_cli()?;
                let refreshed = read_credentials()?;
                let fresh = refreshed
                    .access_token
                    .ok_or_else(|| anyhow::anyhow!("No Gemini access token after refresh"))?;
                fetch_quota(&client, &fresh).await?
            }
            Err(e) => return Err(e),
        };

        let buckets = resp.buckets.unwrap_or_default();
        Ok(UsageOutput {
            provider: "Gemini".into(),
            plan: None,
            email: None,
            metrics: build_metrics(&buckets),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(model: &str, fraction: f64) -> QuotaBucket {
        QuotaBucket {
            model_id: Some(model.to_string()),
            remaining_fraction: Some(fraction),
            reset_time: Some("2026-06-08T04:51:11Z".to_string()),
        }
    }

    #[test]
    fn collapses_model_aliases_to_tiers() {
        let buckets = vec![
            bucket("gemini-2.5-pro", 0.9),
            bucket("gemini-3.1-pro-preview", 0.8), // same Pro tier, lower → wins
            bucket("gemini-2.5-flash", 1.0),
            bucket("gemini-2.5-flash-lite", 1.0),
            bucket("gemini-3.1-flash-lite", 0.5), // same Flash-Lite tier, lower → wins
        ];
        let metrics = build_metrics(&buckets);
        // One row per tier, ordered Pro / Flash / Flash Lite.
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].label, "Pro");
        assert_eq!(metrics[0].remaining_percent, 80.0);
        assert_eq!(metrics[1].label, "Flash");
        assert_eq!(metrics[1].remaining_percent, 100.0);
        assert_eq!(metrics[2].label, "Flash Lite");
        assert_eq!(metrics[2].remaining_percent, 50.0);
        assert_eq!(metrics[2].used_percent, 50.0);
    }

    #[test]
    fn unknown_models_keep_their_own_row() {
        let metrics = build_metrics(&[bucket("some-experimental-model", 0.7)]);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].label, "some-experimental-model");
        assert_eq!(metrics[0].remaining_percent, 70.0);
    }
}
