//! Session parsers for different AI coding assistant formats
//!
//! Each client has its own parser that converts to a unified message format.

pub mod amp;
pub mod antigravity;
pub mod antigravity_cli;
pub mod claudecode;
pub mod cline;
pub mod codebuddy;
pub mod codebuff;
pub mod codex;
pub mod commandcode;
pub mod copilot;
pub mod copilot_desktop;
pub mod copilot_vscode;
pub mod crush;
pub mod cursor;
pub mod devin;
pub mod droid;
pub mod gemini;
pub mod gjc;
pub mod goose;
pub mod grok;
pub mod hermes;
pub mod jcode;
pub mod junie;
pub mod kilo;
pub mod kilocode;
pub mod kimi;
pub mod kiro;
pub mod micode;
pub mod mux;
pub mod openclaw;
pub mod opencode;
pub mod opencodereview;
pub mod pi;
pub mod qwen;
pub mod roocode;
pub mod synthetic;
pub(crate) mod tencent_buddy;
pub mod trae;
pub(crate) mod utils;
pub mod warp;
pub mod workbuddy;
pub mod zcode;
pub mod zed;

use crate::TokenBreakdown;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CostSource {
    #[default]
    Unknown,
    ProviderReported,
    Estimated,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimestampProvenance {
    #[default]
    Exact,
    DateOnly,
    Aggregate,
    Fallback,
}

impl TimestampProvenance {
    pub const fn is_trustworthy_for_hourly(self) -> bool {
        matches!(self, Self::Exact)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnifiedMessage {
    pub client: String,
    pub model_id: String,
    pub provider_id: String,
    pub session_id: String,
    pub workspace_key: Option<String>,
    pub workspace_label: Option<String>,
    pub timestamp: i64,
    #[serde(default)]
    pub timestamp_provenance: TimestampProvenance,
    pub date: String,
    pub tokens: TokenBreakdown,
    pub cost: f64,
    #[serde(default)]
    pub cost_source: CostSource,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default = "default_message_count")]
    pub message_count: i32,
    pub agent: Option<String>,
    pub dedup_key: Option<String>,
    /// Human-readable session title/name when the source client stores one
    /// (e.g. OpenCode's `session.title` column). `None` for clients that
    /// don't record a title; the Sessions tab falls back to showing just
    /// the session ID in that case.
    #[serde(default)]
    pub session_title: Option<String>,
    /// True if this message is the first assistant response after a user turn.
    /// Used to count user interaction turns (as opposed to API message count).
    #[serde(default)]
    pub is_turn_start: bool,
}

const fn default_message_count() -> i32 {
    1
}

pub fn normalize_agent_name(agent: &str) -> String {
    let cleaned = strip_zero_width_chars(agent);
    let trimmed = cleaned.trim();
    let stripped = strip_agent_prefix(trimmed);
    let canonical = canonicalize_agent_name(stripped);
    let agent_lower = canonical.to_lowercase();

    if agent_lower.contains("plan") {
        if agent_lower.contains("omo") || agent_lower.contains("sisyphus") {
            return "Planner-Sisyphus".to_string();
        }
        return titlecase_agent(&canonical);
    }

    if agent_lower == "omo" || agent_lower == "sisyphus" {
        return "Sisyphus".to_string();
    }

    if agent_lower == "orchestrator-sisyphus" {
        return "Atlas".to_string();
    }

    titlecase_agent(&canonical)
}

pub fn normalize_opencode_agent_name(agent: &str) -> String {
    let cleaned = strip_zero_width_chars(agent);
    let trimmed = cleaned.trim();
    let stripped = strip_agent_prefix(trimmed);
    let canonical = canonicalize_agent_name(stripped);
    let agent_lower = canonical.to_lowercase();

    if let Some(normalized) = normalize_oh_my_opencode_agent_name(&agent_lower) {
        return normalized;
    }

    normalize_agent_name(&canonical)
}

pub fn normalize_copilot_agent_name(agent: &str) -> String {
    // Hardcoded brand name for the default native agent
    if agent.eq_ignore_ascii_case("github.copilot.default") {
        return "GitHub Copilot".to_string();
    }

    // Native github.copilot.* agents: strip prefix, titlecase remainder
    const GITHUB_COPILOT_PREFIX: &str = "github.copilot.";
    if agent
        .get(..GITHUB_COPILOT_PREFIX.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(GITHUB_COPILOT_PREFIX))
    {
        let remainder = &agent[GITHUB_COPILOT_PREFIX.len()..];
        let hyphenated = remainder.replace('.', "-");
        return titlecase_agent(&hyphenated);
    }

    // Plugin:team:slug format — titlecase each colon-separated part, join with ": "
    const PLUGIN_PREFIX: &str = "Plugin:";
    if agent
        .get(..PLUGIN_PREFIX.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(PLUGIN_PREFIX))
    {
        let rest = &agent[PLUGIN_PREFIX.len()..];
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() == 2 {
            let team = titlecase_agent(parts[0]);
            let slug = titlecase_agent(parts[1]);
            return format!("{}: {}", team, slug);
        }
        return titlecase_agent(rest);
    }

    normalize_agent_name(agent)
}

fn normalize_oh_my_opencode_agent_name(agent_lower: &str) -> Option<String> {
    let normalized = match agent_lower {
        // Parenthesized format and dash format
        "sisyphus (ultraworker)"
        | "sisyphus - ultraworker"
        | "sisyphus ultraworker"
        | "sisyphus" => "Sisyphus",
        "hephaestus (deep agent)"
        | "hephaestus - deep agent"
        | "hephaestus deep agent"
        | "hephaestus" => "Hephaestus",
        "prometheus (plan builder)"
        | "prometheus - plan builder"
        | "prometheus plan builder"
        | "prometheus (planner)"
        | "prometheus" => "Prometheus",
        "atlas (plan executor)" | "atlas - plan executor" | "atlas plan executor" | "atlas" => {
            "Atlas"
        }
        "metis (plan consultant)"
        | "metis - plan consultant"
        | "metis plan consultant"
        | "metis" => "Metis",
        "momus (plan critic)"
        | "momus - plan critic"
        | "momus plan critic"
        | "momus (plan reviewer)"
        | "momus" => "Momus",
        "orchestrator-sisyphus" => "Atlas",
        "sisyphus-junior" => "Sisyphus-Junior",
        "planner-sisyphus" => "Planner-Sisyphus",
        _ => return None,
    };

    Some(normalized.to_string())
}

/// Strip zero-width Unicode characters that oh-my-openagent uses as
/// invisible sort-order prefixes (U+200B ZERO WIDTH SPACE, U+200C ZERO
/// WIDTH NON-JOINER, U+200D ZERO WIDTH JOINER, U+FEFF BOM/ZWNBSP).
fn strip_zero_width_chars(s: &str) -> String {
    if !s.contains(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}']) {
        return s.to_string();
    }
    s.chars()
        .filter(|c| !matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}'))
        .collect()
}

fn strip_agent_prefix(name: &str) -> &str {
    for prefix in &["astrape:", "oh-my-claudecode:", "oh-my-codex:"] {
        if name
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return &name[prefix.len()..];
        }
    }
    name
}

fn canonicalize_agent_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn titlecase_word(word: &str) -> String {
    match word.to_lowercase().as_str() {
        "ui" => "UI".to_string(),
        "ux" => "UX".to_string(),
        "api" => "API".to_string(),
        _ => {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.collect::<String>()
                }
            }
        }
    }
}

fn titlecase_agent(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    name.split('-')
        .flat_map(|part| part.split_whitespace())
        .map(titlecase_word)
        .collect::<Vec<_>>()
        .join(" ")
}

impl UnifiedMessage {
    pub fn new(
        client: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: i64,
        tokens: TokenBreakdown,
        cost: f64,
    ) -> Self {
        Self::new_full(
            client,
            model_id,
            provider_id,
            session_id,
            timestamp,
            tokens,
            cost,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_agent(
        client: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: i64,
        tokens: TokenBreakdown,
        cost: f64,
        agent: Option<String>,
    ) -> Self {
        Self::new_full(
            client,
            model_id,
            provider_id,
            session_id,
            timestamp,
            tokens,
            cost,
            agent,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dedup(
        client: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: i64,
        tokens: TokenBreakdown,
        cost: f64,
        dedup_key: Option<String>,
    ) -> Self {
        Self::new_full(
            client,
            model_id,
            provider_id,
            session_id,
            timestamp,
            tokens,
            cost,
            None,
            dedup_key,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_full(
        client: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: i64,
        tokens: TokenBreakdown,
        cost: f64,
        agent: Option<String>,
        dedup_key: Option<String>,
    ) -> Self {
        let date = timestamp_to_date(timestamp);
        Self {
            client: client.into(),
            model_id: model_id.into(),
            provider_id: provider_id.into(),
            session_id: session_id.into(),
            workspace_key: None,
            workspace_label: None,
            timestamp,
            timestamp_provenance: TimestampProvenance::Exact,
            date,
            tokens,
            cost,
            cost_source: CostSource::Unknown,
            duration_ms: None,
            message_count: default_message_count(),
            agent,
            dedup_key,
            session_title: None,
            is_turn_start: false,
        }
    }

    pub fn set_workspace(
        &mut self,
        workspace_key: Option<String>,
        workspace_label: Option<String>,
    ) {
        self.workspace_key = workspace_key;
        self.workspace_label = workspace_label;
    }

    pub(crate) fn refresh_derived_fields(&mut self) {
        self.date = timestamp_to_date(self.timestamp);
    }

    pub(crate) fn set_timestamp(&mut self, timestamp: i64) {
        self.timestamp = timestamp;
        self.refresh_derived_fields();
    }

    pub fn set_timestamp_provenance(&mut self, provenance: TimestampProvenance) {
        self.timestamp_provenance = provenance;
    }

    pub const fn is_trustworthy_for_hourly(&self) -> bool {
        self.timestamp_provenance.is_trustworthy_for_hourly()
    }

    pub(crate) fn retain_best_timestamp_from(&mut self, other: &Self) {
        if !self.is_trustworthy_for_hourly() && other.is_trustworthy_for_hourly() {
            self.set_timestamp(other.timestamp);
            self.set_timestamp_provenance(other.timestamp_provenance);
        }
    }

    pub fn mark_provider_reported_cost(&mut self) {
        self.cost_source = CostSource::ProviderReported;
    }

    pub(crate) fn mark_estimated_cost(&mut self) {
        self.cost_source = CostSource::Estimated;
    }

    pub(crate) fn has_authoritative_cost(&self) -> bool {
        self.cost_source == CostSource::ProviderReported
    }
}

pub fn normalize_workspace_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let preserve_unc_prefix = trimmed.starts_with("\\\\") || trimmed.starts_with("//");
    let mut normalized = trimmed.replace('\\', "/");

    if preserve_unc_prefix {
        let body = normalized.trim_start_matches('/');
        let mut collapsed = body.to_string();
        while collapsed.contains("//") {
            collapsed = collapsed.replace("//", "/");
        }
        normalized = format!("//{}", collapsed);
    } else {
        while normalized.contains("//") {
            normalized = normalized.replace("//", "/");
        }
    }

    let minimum_len = if preserve_unc_prefix { 2 } else { 1 };
    if normalized.len() > minimum_len {
        normalized = normalized.trim_end_matches('/').to_string();
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub fn workspace_label_from_key(key: &str) -> Option<String> {
    key.rsplit('/')
        .find(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
}

/// Convert Unix milliseconds to a YYYY-MM-DD date string in the process-wide
/// bucketing timezone (see [`crate::bucket_tz`]). Defaults to the machine-local
/// timezone when nothing has been pinned.
fn timestamp_to_date(timestamp_ms: i64) -> String {
    crate::bucket_tz::bucket_timezone().date_of_ms(timestamp_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_message() -> UnifiedMessage {
        UnifiedMessage::new(
            "client",
            "model",
            "provider",
            "session",
            1_700_000_000_000,
            TokenBreakdown::default(),
            0.0,
        )
    }

    #[test]
    fn standard_constructors_default_to_exact_hourly_trust() {
        let message = sample_message();

        assert_eq!(message.timestamp_provenance, TimestampProvenance::Exact);
        assert!(message.is_trustworthy_for_hourly());
    }

    #[test]
    fn timestamp_provenance_marks_date_only_aggregate_and_fallback_as_untrustworthy() {
        for provenance in [
            TimestampProvenance::DateOnly,
            TimestampProvenance::Aggregate,
            TimestampProvenance::Fallback,
        ] {
            let mut message = sample_message();
            message.set_timestamp_provenance(provenance);

            assert_eq!(message.timestamp_provenance, provenance);
            assert!(!message.is_trustworthy_for_hourly());
        }
    }

    #[test]
    fn timestamp_provenance_has_stable_serializable_variants() {
        for (provenance, encoded) in [
            (TimestampProvenance::Exact, "\"exact\""),
            (TimestampProvenance::DateOnly, "\"dateOnly\""),
            (TimestampProvenance::Aggregate, "\"aggregate\""),
            (TimestampProvenance::Fallback, "\"fallback\""),
        ] {
            assert_eq!(serde_json::to_string(&provenance).unwrap(), encoded);
            assert_eq!(
                serde_json::from_str::<TimestampProvenance>(encoded).unwrap(),
                provenance
            );
        }
    }
}
