mod amp;
mod claude;
pub mod codex;
mod copilot;
mod gemini;
mod grok;
pub mod helpers;
mod kimi;
mod minimax;
mod warp;
mod zai;

use anyhow::Result;

// ── Shared types ──

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageMetric {
    pub label: String,
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub remaining_label: Option<String>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageOutput {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<UsageAccount>,
    pub plan: Option<String>,
    pub email: Option<String>,
    pub metrics: Vec<UsageMetric>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageAccount {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub is_active: bool,
}

impl UsageAccount {
    pub fn label_name(&self) -> Option<&str> {
        self.label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
    }

    pub fn short_id(&self) -> String {
        let id = self.id.trim();
        if id.is_empty() {
            return "unknown".to_string();
        }

        let char_count = id.chars().count();
        if char_count <= 12 {
            return id.to_string();
        }

        let head: String = id.chars().take(6).collect();
        let tail: String = id
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{head}...{tail}")
    }

    pub fn display_name(&self) -> String {
        self.label_name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("Account {}", self.short_id()))
    }
}

impl UsageOutput {
    pub fn account_display_name(&self) -> Option<String> {
        let account = self.account.as_ref()?;

        if let Some(label) = account.label_name() {
            return Some(label.to_string());
        }

        if let Some(email) = self
            .email
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(email.to_string());
        }

        Some(account.display_name())
    }

    pub fn display_name(&self) -> String {
        match &self.account {
            Some(_) => format!(
                "{} ({})",
                self.provider,
                self.account_display_name().unwrap_or_default()
            ),
            None => self.provider.clone(),
        }
    }
}

// ── Cache ──

fn cache_path() -> Option<std::path::PathBuf> {
    let dir = crate::paths::get_cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    Some(dir.join("subscription-usage-cache.json"))
}

pub fn save_cache(data: &[UsageOutput]) {
    let Some(path) = cache_path() else { return };
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let json = serde_json::json!({
        "timestamp": timestamp,
        "data": data,
    });
    let _ = std::fs::write(&path, serde_json::to_string(&json).unwrap_or_default());
}

pub fn clear_cache() {
    if let Some(path) = cache_path() {
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg_attr(test, allow(dead_code))]
pub fn load_cache() -> Option<Vec<UsageOutput>> {
    let path = cache_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&content).ok()?;
    let timestamp = doc.get("timestamp")?.as_u64()?;
    let age = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(timestamp);
    // Cache expires after 5 minutes
    if age > 300 {
        return None;
    }
    serde_json::from_value(doc.get("data")?.clone()).ok()
}

// ── Public API ──

type UsageProvider = (&'static str, fn() -> bool, fn() -> Result<Vec<UsageOutput>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum UsageFailureStatus {
    AuthExpired,
    NeedsAuth,
    RateLimited,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UsageFailure {
    provider: String,
    status: UsageFailureStatus,
}

#[derive(Debug, Default)]
struct UsageFetchReport {
    outputs: Vec<UsageOutput>,
    failures: Vec<UsageFailure>,
}

fn fetch_amp() -> Result<Vec<UsageOutput>> {
    amp::fetch().map(|output| vec![output])
}

fn fetch_claude() -> Result<Vec<UsageOutput>> {
    claude::fetch().map(|output| vec![output])
}

fn fetch_copilot() -> Result<Vec<UsageOutput>> {
    copilot::fetch().map(|output| vec![output])
}

fn fetch_gemini() -> Result<Vec<UsageOutput>> {
    gemini::fetch().map(|output| vec![output])
}

fn fetch_grok() -> Result<Vec<UsageOutput>> {
    grok::fetch().map(|output| vec![output])
}

fn fetch_kimi() -> Result<Vec<UsageOutput>> {
    kimi::fetch().map(|output| vec![output])
}

fn fetch_minimax() -> Result<Vec<UsageOutput>> {
    minimax::fetch().map(|output| vec![output])
}

fn fetch_warp() -> Result<Vec<UsageOutput>> {
    warp::fetch().map(|output| vec![output])
}

fn fetch_zai() -> Result<Vec<UsageOutput>> {
    zai::fetch().map(|output| vec![output])
}

fn usage_providers() -> Vec<UsageProvider> {
    vec![
        ("Claude", claude::has_credentials, fetch_claude),
        ("Codex", codex::has_credentials, codex::fetch_all),
        ("Gemini", gemini::has_credentials, fetch_gemini),
        ("Z.ai", zai::has_credentials, fetch_zai),
        ("Amp", amp::has_credentials, fetch_amp),
        ("Copilot", copilot::has_credentials, fetch_copilot),
        ("Grok Build", grok::has_credentials, fetch_grok),
        ("Kimi", kimi::has_credentials, fetch_kimi),
        ("MiniMax", minimax::has_credentials, fetch_minimax),
        ("Warp/Oz", warp::has_credentials, fetch_warp),
    ]
}

fn classify_provider_failure(_provider: &str, error: &str) -> UsageFailureStatus {
    if error.contains("AUTH_EXPIRED") {
        UsageFailureStatus::AuthExpired
    } else if error.contains("RATE_LIMITED") || error.contains("HTTP 429") {
        UsageFailureStatus::RateLimited
    } else if error.contains("NEEDS_AUTH") {
        UsageFailureStatus::NeedsAuth
    } else {
        UsageFailureStatus::Unavailable
    }
}

fn fetch_report() -> UsageFetchReport {
    let active: Vec<_> = usage_providers()
        .into_iter()
        .filter(|(_, has, _)| has())
        .collect();

    if active.is_empty() {
        return UsageFetchReport::default();
    }

    std::thread::scope(|s| {
        let handles = active
            .into_iter()
            .map(|(provider, _, fetch)| (provider, s.spawn(fetch)))
            .collect::<Vec<_>>();
        let mut report = UsageFetchReport::default();
        for (provider, handle) in handles {
            match handle.join() {
                Ok(Ok(outputs)) => report.outputs.extend(outputs),
                Ok(Err(error)) => report.failures.push(UsageFailure {
                    provider: provider.to_string(),
                    status: classify_provider_failure(provider, &error.to_string()),
                }),
                Err(_) => report.failures.push(UsageFailure {
                    provider: provider.to_string(),
                    status: UsageFailureStatus::Unavailable,
                }),
            }
        }
        report
    })
}

pub fn fetch_all() -> Vec<UsageOutput> {
    fetch_report().outputs
}

fn usage_json(report: &UsageFetchReport, include_status: bool) -> Result<String> {
    if !include_status {
        return Ok(serde_json::to_string_pretty(&report.outputs)?);
    }
    let mut rows = report
        .outputs
        .iter()
        .map(|output| {
            let mut value = serde_json::to_value(output)?;
            if let Some(object) = value.as_object_mut() {
                object.insert("status".to_string(), serde_json::json!("live"));
            }
            Ok::<serde_json::Value, serde_json::Error>(value)
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.extend(report.failures.iter().map(|failure| {
        serde_json::json!({
            "provider": failure.provider,
            "plan": null,
            "email": null,
            "metrics": [],
            "status": failure.status,
        })
    }));
    Ok(serde_json::to_string_pretty(&rows)?)
}

// ── Light-mode rendering ──

const BAR_WIDTH: usize = 12;
const CARD_WIDTH: usize = 62;

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_len - 1).collect();
    format!("{truncated}…")
}

fn render_light(output: &UsageOutput) {
    println!("╭{}╮", "─".repeat(CARD_WIDTH));
    // Provider header
    println!(
        "│ {:<width$}│",
        output.display_name(),
        width = CARD_WIDTH - 1
    );
    for m in &output.metrics {
        let rem = m
            .remaining_label
            .clone()
            .unwrap_or_else(|| format!("{:.0}% left", m.remaining_percent));
        let rem = truncate(&rem, 11);
        let bar = helpers::render_ascii_bar(m.remaining_percent, BAR_WIDTH);
        let reset = m
            .resets_at
            .as_ref()
            .map(|r| helpers::format_reset_time(r))
            .unwrap_or_default();
        let label = truncate(&m.label, 14);
        println!("│ {:<14}{:<11}{:<14}{:<22}│", label, rem, bar, reset);
    }
    if let Some(ref email) = output.email {
        let email = truncate(email, CARD_WIDTH - 11);
        println!(
            "│ {:<10}{:<width$}│",
            "Account",
            email,
            width = CARD_WIDTH - 11
        );
    }
    if let Some(ref plan) = output.plan {
        let plan = truncate(plan, CARD_WIDTH - 11);
        println!("│ {:<10}{:<width$}│", "Plan", plan, width = CARD_WIDTH - 11);
    }
    println!("╰{}╯", "─".repeat(CARD_WIDTH));
}

pub fn run(json: bool, _light: bool, include_status: bool) -> Result<()> {
    let report = if include_status {
        fetch_report()
    } else {
        UsageFetchReport {
            outputs: fetch_all(),
            failures: Vec::new(),
        }
    };
    if json {
        println!("{}", usage_json(&report, include_status)?);
    } else {
        for o in &report.outputs {
            render_light(o);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_output_display_name_includes_account_label() {
        let output = UsageOutput {
            provider: "Codex".to_string(),
            account: Some(UsageAccount {
                id: "acct_123".to_string(),
                label: Some("work".to_string()),
                is_active: true,
            }),
            plan: None,
            email: None,
            metrics: Vec::new(),
        };

        assert_eq!(output.display_name(), "Codex (work)");
    }

    #[test]
    fn usage_output_display_name_prefers_email_over_account_id() {
        let output = UsageOutput {
            provider: "Codex".to_string(),
            account: Some(UsageAccount {
                id: "acct_123".to_string(),
                label: Some("  ".to_string()),
                is_active: false,
            }),
            plan: None,
            email: Some("user@example.com".to_string()),
            metrics: Vec::new(),
        };

        assert_eq!(output.display_name(), "Codex (user@example.com)");
    }

    #[test]
    fn usage_output_display_name_masks_long_account_id() {
        let output = UsageOutput {
            provider: "Codex".to_string(),
            account: Some(UsageAccount {
                id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
                label: None,
                is_active: false,
            }),
            plan: None,
            email: None,
            metrics: Vec::new(),
        };

        assert_eq!(output.display_name(), "Codex (Account 123e45...4000)");
    }

    #[test]
    fn usage_output_deserializes_legacy_json_without_account() -> Result<()> {
        let output: UsageOutput = serde_json::from_str(
            r#"{
                "provider": "Codex",
                "plan": null,
                "email": null,
                "metrics": []
            }"#,
        )?;

        assert!(output.account.is_none());
        assert_eq!(output.display_name(), "Codex");
        Ok(())
    }

    #[test]
    fn provider_failure_statuses_are_classified_without_raw_errors() {
        assert_eq!(
            classify_provider_failure("Grok Build", "AUTH_EXPIRED token=secret"),
            UsageFailureStatus::AuthExpired
        );
        assert_eq!(
            classify_provider_failure("Claude", "RATE_LIMITED bearer=secret"),
            UsageFailureStatus::RateLimited
        );
        assert_eq!(
            classify_provider_failure("Claude", "NEEDS_AUTH refresh=secret"),
            UsageFailureStatus::NeedsAuth
        );
        assert_eq!(
            classify_provider_failure("Grok Build", "network token=secret"),
            UsageFailureStatus::Unavailable
        );
    }

    #[test]
    fn status_json_keeps_successes_and_safe_failure_placeholders() -> Result<()> {
        let report = UsageFetchReport {
            outputs: vec![UsageOutput {
                provider: "Codex".to_string(),
                account: None,
                plan: Some("Plus".to_string()),
                email: None,
                metrics: vec![],
            }],
            failures: vec![UsageFailure {
                provider: "Grok Build".to_string(),
                status: UsageFailureStatus::AuthExpired,
            }],
        };

        let json = usage_json(&report, true)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let rows = value.as_array().expect("top-level usage array");

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["provider"], "Codex");
        assert_eq!(rows[0]["status"], "live");
        assert_eq!(rows[1]["provider"], "Grok Build");
        assert_eq!(rows[1]["status"], "auth-expired");
        assert_eq!(rows[1]["metrics"], serde_json::json!([]));
        assert!(!json.contains("secret"));
        Ok(())
    }
}
