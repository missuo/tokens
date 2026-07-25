//! Provider usage-API types retained for the client integration commands.
//!
//! The `usage` display command and its per-provider report modules were
//! removed. What remains is the shared type vocabulary plus the Codex usage
//! fetcher behind `tokens codex import/status/accounts`, and the helpers the
//! client integrations (warp, cursor, ...) call.

pub mod codex;
pub mod helpers;


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
pub struct UsageResetCredits {
    pub available_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credits: Vec<UsageResetCredit>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageResetCredit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageCreditStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_credits: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unlimited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overage_limit_reached: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageSpendControl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub individual_limit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reached: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageOutput {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<UsageAccount>,
    pub plan: Option<String>,
    pub email: Option<String>,
    pub metrics: Vec<UsageMetric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_credits: Option<UsageResetCredits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit_status: Option<UsageCreditStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spend_control: Option<UsageSpendControl>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageAccount {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub is_active: bool,
}

