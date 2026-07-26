#![deny(clippy::all)]

mod aggregator;
pub mod bucket_tz;
mod cc_mirror;
pub mod clients;
pub mod content_extractor;
pub mod fs_atomic;
pub mod mcp;
mod message_cache;
pub use message_cache::clear_source_message_cache;
pub mod model_alias;
pub mod opencode_model_name;
mod parser;
pub mod paths;
pub mod pricing;
mod provider_identity;
pub mod scanner;
pub mod sessionize;
pub mod sessions;
pub mod wiki;

pub use aggregator::*;
pub use bucket_tz::{bucket_timezone, parse_bucket_timezone, set_bucket_timezone, BucketTimezone};
pub use clients::{ClientCounts, ClientDef, ClientId, PathRoot};
pub use model_alias::ModelAliasMap;
pub use parser::*;
pub use scanner::*;
pub use sessionize::{
    compute_daily_active_time, compute_time_metrics, sessionize, SessionInterval, TimeMetrics,
    DEFAULT_IDLE_GAP_MS,
};
pub use sessions::{CostSource, UnifiedMessage};

use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// Strip a CLIProxyAPI-style `(level)` reasoning-effort suffix from a model id.
///
/// Mirrors <https://help.router-for.me/configuration/thinking>: the proxy
/// strips the parentheses before routing, so for pricing lookups we treat the
/// suffix as cosmetic and resolve to the base model. Accepts the level set the
/// proxy documents (case-insensitive — callers pass the lowercased id):
/// `minimal`, `low`, `medium`, `high`, `xhigh`, `auto`, `none`. Numeric
/// thinking budgets are intentionally not handled here.
pub(crate) fn strip_parenthesized_reasoning_tier(model_id: &str) -> Option<&str> {
    let without_closing_paren = model_id.strip_suffix(')')?;
    let (base_model, tier) = without_closing_paren.rsplit_once('(')?;

    if base_model.is_empty() || base_model.trim() != base_model {
        return None;
    }

    if !matches!(
        tier,
        "minimal" | "low" | "medium" | "high" | "xhigh" | "auto" | "none"
    ) {
        return None;
    }

    Some(base_model)
}

/// Canonical model identity — the model id that leaves the machine.
///
/// This is [`normalize_syntactic`] with **no alias folding**: purely structural
/// canonicalization (lowercase, strip a `(reasoning-tier)` suffix, strip a
/// trailing `-YYYYMMDD` date, rewrite `.`→`-` inside claude version numbers, and
/// fold an `anthropic/claude-…` prefix). It never consults the user's
/// machine-local `modelAliases`.
///
/// Every path that submits, uploads, exports as raw data, or persists a model id
/// MUST use this, not [`normalize_model_for_grouping`]. A machine-local alias
/// config must never rewrite the model identity persisted server-side, or usage
/// history would fragment and fork across a user's devices.
pub fn canonical_model_id(model_id: &str) -> String {
    normalize_syntactic(model_id)
}

/// Local display/grouping model name: [`canonical_model_id`] plus the user's
/// configured `modelAliases` fold. Every local report-grouping surface — the
/// models report, every `--group-by`, monthly, hourly, and the TUI — routes
/// through this so name variants fold uniformly for presentation.
///
/// The alias fold is **presentation only** and must never reach the
/// submit/upload/export/persist path (those use [`canonical_model_id`]), or a
/// machine-local alias config would rewrite the uploaded model identity. An
/// empty/unset alias config makes this identical to [`canonical_model_id`].
pub fn normalize_model_for_grouping(model_id: &str) -> String {
    model_alias::global().apply(normalize_syntactic(model_id))
}

/// Local display/grouping name with OpenCode's configured model label applied
/// when one exists. The configured label is scoped to OpenCode and matched by
/// provider plus raw model key; all other messages use the normal grouping
/// name.
pub fn model_name_for_grouping(client: &str, provider_id: &str, model_id: &str) -> String {
    let fallback = normalize_model_for_grouping(model_id);
    if client == "opencode" {
        opencode_model_name::global()
            .display_name(provider_id, model_id)
            .map(str::to_string)
            .unwrap_or(fallback)
    } else {
        fallback
    }
}

/// Structural-only model-name normalization: lowercase, strip a
/// `(reasoning-tier)` suffix, strip a trailing `-YYYYMMDD` date, rewrite `.`→`-`
/// inside claude version numbers, and fold an `anthropic/claude-…` prefix.
///
/// This is the syntactic half of [`normalize_model_for_grouping`] /
/// [`canonical_model_id`]. It is also used by [`model_alias::ModelAliasResolver`]
/// to normalize configured alias keys and values into the same space, so a
/// configured alias matches its model regardless of case, dated suffix, or
/// `.`-vs-`-` spelling.
pub(crate) fn normalize_syntactic(model_id: &str) -> String {
    let mut name = model_id.to_lowercase();

    if let Some(base_model) = strip_parenthesized_reasoning_tier(&name) {
        name = base_model.to_string();
    }
    if name.len() > 9 {
        let potential_date = &name[name.len() - 8..];
        if potential_date.chars().all(|c| c.is_ascii_digit())
            && name.as_bytes()[name.len() - 9] == b'-'
        {
            name = name[..name.len() - 9].to_string();
        }
    }

    if name.contains("claude") {
        let chars: Vec<char> = name.chars().collect();
        let mut result = String::with_capacity(name.len());
        for i in 0..chars.len() {
            if chars[i] == '.'
                && i > 0
                && i < chars.len() - 1
                && chars[i - 1].is_ascii_digit()
                && chars[i + 1].is_ascii_digit()
            {
                result.push('-');
            } else {
                result.push(chars[i]);
            }
        }
        name = result;
    }

    if let Some(canonical) = normalize_anthropic_prefixed_claude_model(&name) {
        name = canonical;
    }

    name
}

fn normalize_anthropic_prefixed_claude_model(model_id: &str) -> Option<String> {
    let rest = model_id.strip_prefix("anthropic/claude-")?;
    let mut parts = rest.split('-');
    let major = parts.next()?;
    let minor = parts.next()?;
    let family = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    if !matches!(family, "opus" | "sonnet" | "haiku") {
        return None;
    }

    Some(format!("claude-{family}-{major}-{minor}"))
}

fn retain_for_requested_clients(
    client: &str,
    model_id: &str,
    provider_id: &str,
    requested: &HashSet<&str>,
) -> bool {
    requested.contains(client)
        || (requested.contains("claude") && client.starts_with("cc-mirror/"))
        // "gjc" is a superset request: 9Router bridge data IS gjc-format, so
        // requesting gjc retains 9router-stamped messages too. The reverse is
        // intentionally NOT true — `--client 9router` must retain only
        // 9router-stamped messages, not native gjc ones.
        || (requested.contains("gjc") && client.eq_ignore_ascii_case("9router"))
        || (requested.contains("synthetic")
            && sessions::synthetic::matches_synthetic_filter(client, model_id, provider_id))
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub enum GroupBy {
    Model,
    #[default]
    ClientModel,
    ClientProviderModel,
    WorkspaceModel,
    Session,
    ClientSession,
}

impl std::fmt::Display for GroupBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupBy::Model => write!(f, "model"),
            GroupBy::ClientModel => write!(f, "client,model"),
            GroupBy::ClientProviderModel => write!(f, "client,provider,model"),
            GroupBy::WorkspaceModel => write!(f, "workspace,model"),
            GroupBy::Session => write!(f, "session,model"),
            GroupBy::ClientSession => write!(f, "client,session,model"),
        }
    }
}

impl std::str::FromStr for GroupBy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized: String = s.split(',').map(|p| p.trim()).collect::<Vec<_>>().join(",");
        match normalized.to_lowercase().as_str() {
            "model" => Ok(GroupBy::Model),
            "client,model" | "client-model" => Ok(GroupBy::ClientModel),
            "client,provider,model" | "client-provider-model" => Ok(GroupBy::ClientProviderModel),
            "workspace,model" | "workspace-model" => Ok(GroupBy::WorkspaceModel),
            "session" | "session,model" | "session-model" => Ok(GroupBy::Session),
            "client,session" | "client-session" | "client,session,model" | "client-session-model" => {
                Ok(GroupBy::ClientSession)
            }
            _ => Err(format!(
                "Invalid group-by value: '{}'. Valid options: model, client,model, client,provider,model, workspace,model, session,model, client,session,model",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TokenBreakdown {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
}

impl TokenBreakdown {
    pub fn total(&self) -> i64 {
        // saturating so clamped (i64::MAX) buckets from a corrupt source can't
        // overflow the sum.
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
            .saturating_add(self.reasoning)
    }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPerformance {
    #[serde(rename = "msPer1KTokens")]
    pub ms_per_1k_tokens: Option<f64>,
    pub total_duration_ms: i64,
    pub timed_tokens: i64,
    pub sample_count: i32,
    pub token_coverage: f64,
}

impl ModelPerformance {
    pub fn record_message(&mut self, token_total: i64, duration_ms: Option<i64>) {
        let Some(duration_ms) = duration_ms else {
            return;
        };
        if duration_ms <= 0 || token_total <= 0 {
            return;
        }

        self.total_duration_ms = self.total_duration_ms.saturating_add(duration_ms);
        self.timed_tokens = self.timed_tokens.saturating_add(token_total);
        self.sample_count = self.sample_count.saturating_add(1);
    }

    pub fn finalize(&mut self, total_tokens: i64) {
        self.ms_per_1k_tokens = if self.timed_tokens > 0 && self.total_duration_ms > 0 {
            Some(self.total_duration_ms as f64 * 1000.0 / self.timed_tokens as f64)
        } else {
            None
        };

        self.token_coverage = if total_tokens > 0 {
            (self.timed_tokens as f64 / total_tokens as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    pub fn from_totals(total_duration_ms: i64, timed_tokens: i64, sample_count: i32) -> Self {
        let mut performance = Self {
            total_duration_ms,
            timed_tokens,
            sample_count,
            ..Self::default()
        };
        performance.finalize(timed_tokens);
        performance
    }
}

#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub client: String,
    pub model_id: String,
    pub provider_id: String,
    pub session_id: String,
    pub workspace_key: Option<String>,
    pub workspace_label: Option<String>,
    pub timestamp: i64,
    pub date: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub duration_ms: Option<i64>,
    pub message_count: i32,
    pub agent: Option<String>,
}

pub struct ParsedMessages {
    pub messages: Vec<ParsedMessage>,
    pub counts: ClientCounts,
    pub processing_time_ms: u32,
}

impl Clone for ParsedMessages {
    fn clone(&self) -> Self {
        let mut counts = ClientCounts::new();
        for client in ClientId::iter() {
            counts.set(client, self.counts.get(client));
        }

        Self {
            messages: self.messages.clone(),
            counts,
            processing_time_ms: self.processing_time_ms,
        }
    }
}

impl std::fmt::Debug for ParsedMessages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("ParsedMessages");
        debug.field("messages", &self.messages);
        for client in ClientId::iter() {
            debug.field(client.as_str(), &self.counts.get(client));
        }
        debug.field("processing_time_ms", &self.processing_time_ms);
        debug.finish()
    }
}

/// Database state used to resolve Devin Desktop ACP titles. The source stream
/// is deliberately absent: one lookup is valid for every Desktop file that
/// observed the same CLI database/WAL snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DevinDesktopLookupSnapshot {
    db_paths: Vec<PathBuf>,
    related_files: Vec<message_cache::RelatedFileFingerprint>,
}

type DevinDesktopLookupCache = Mutex<
    HashMap<DevinDesktopLookupSnapshot, Arc<OnceLock<sessions::devin::DevinDesktopSessionLookup>>>,
>;

/// Return the shared title lookup cell for one post-validation database
/// snapshot. The cell is placed in the map before it is initialized, allowing
/// parallel Desktop files from one snapshot to share one SQLite scan without
/// holding the map lock during that scan.
fn devin_desktop_lookup_cell_for_snapshot(
    lookup_cache: &DevinDesktopLookupCache,
    db_paths: &[PathBuf],
    fingerprint: &message_cache::SourceFingerprint,
) -> Arc<OnceLock<sessions::devin::DevinDesktopSessionLookup>> {
    let snapshot = DevinDesktopLookupSnapshot {
        db_paths: db_paths.to_vec(),
        related_files: fingerprint.related_files.clone(),
    };
    let mut lookups = lookup_cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(
        lookups
            .entry(snapshot)
            .or_insert_with(|| Arc::new(OnceLock::new())),
    )
}

#[derive(Debug, Clone, Default)]
pub struct LocalParseOptions {
    pub home_dir: Option<String>,
    pub use_env_roots: bool,
    pub clients: Option<Vec<String>>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub year: Option<String>,
    /// Persistent scanner config loaded from `~/.config/tokens/settings.json`.
    /// Defaults to empty when callers don't care about user-configured paths.
    pub scanner_settings: scanner::ScannerSettings,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DailyTotals {
    pub tokens: i64,
    pub cost: f64,
    pub messages: i32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClientContribution {
    pub client: String,
    pub model_id: String,
    pub provider_id: String,
    pub tokens: TokenBreakdown,
    pub cost: f64,
    pub messages: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DailyContribution {
    pub date: String,
    pub totals: DailyTotals,
    pub intensity: u8,
    pub token_breakdown: TokenBreakdown,
    pub clients: Vec<ClientContribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_time_ms: Option<i64>,
}

/// Per-session aggregate of token usage, cost, and timing — keyed on
/// `session_id` so downstream consumers can attribute cost to a specific
/// agent-CLI session rather than just a date or model rollup.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SessionContribution {
    pub session_id: String,
    pub client: String,
    pub provider: String,
    pub model: String,
    pub totals: DailyTotals,
    pub token_breakdown: TokenBreakdown,
    pub clients: Vec<ClientContribution>,
    /// Earliest message timestamp (unix seconds) in the session.
    pub first_seen: i64,
    /// Latest message timestamp (unix seconds) in the session.
    pub last_seen: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct YearSummary {
    pub year: String,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub range_start: String,
    pub range_end: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DataSummary {
    pub total_tokens: i64,
    pub total_cost: f64,
    pub total_days: i32,
    pub active_days: i32,
    pub average_per_day: f64,
    pub max_cost_in_single_day: f64,
    pub clients: Vec<String>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphMeta {
    pub generated_at: String,
    pub version: String,
    pub date_range_start: String,
    pub date_range_end: String,
    pub processing_time_ms: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphResult {
    pub meta: GraphMeta,
    pub summary: DataSummary,
    pub years: Vec<YearSummary>,
    pub contributions: Vec<DailyContribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_metrics: Option<sessionize::TimeMetrics>,
}

#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    pub home_dir: Option<String>,
    pub use_env_roots: bool,
    pub clients: Option<Vec<String>>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub year: Option<String>,
    pub group_by: GroupBy,
    /// Persistent scanner config loaded from `~/.config/tokens/settings.json`.
    /// Defaults to empty when callers don't care about user-configured paths.
    pub scanner_settings: scanner::ScannerSettings,
    /// When true, only scan transcript files modified today (skips historical
    /// files at the filesystem layer) and keep only today's messages. Powers the
    /// menu bar's fast "today" refresh. Opt-in; default false scans everything.
    pub today_only: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelUsage {
    pub client: String,
    pub merged_clients: Option<String>,
    pub workspace_key: Option<String>,
    pub workspace_label: Option<String>,
    pub session_id: Option<String>,
    pub model: String,
    pub provider: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub message_count: i32,
    pub cost: f64,
    pub performance: ModelPerformance,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MonthlyUsage {
    pub month: String,
    pub models: Vec<String>,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub message_count: i32,
    pub cost: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelReport {
    pub entries: Vec<ModelUsage>,
    pub total_input: i64,
    pub total_output: i64,
    pub total_cache_read: i64,
    pub total_cache_write: i64,
    pub total_messages: i32,
    pub total_cost: f64,
    pub processing_time_ms: u32,
}

const UNKNOWN_WORKSPACE_LABEL: &str = "Unknown workspace";
const UNKNOWN_WORKSPACE_GROUP_KEY: &str = "\0unknown-workspace";

#[derive(Debug, Clone, serde::Serialize)]
pub struct MonthlyReport {
    pub entries: Vec<MonthlyUsage>,
    pub total_cost: f64,
    pub processing_time_ms: u32,
}

/// Hourly usage entry for a single hour slot (e.g. "2026-03-23 14:00")
#[derive(Debug, Clone, serde::Serialize)]
pub struct HourlyUsage {
    pub hour: String,
    pub clients: Vec<String>,
    pub models: Vec<String>,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub message_count: i32,
    /// Number of user interaction turns (user→assistant boundaries).
    pub turn_count: i32,
    pub reasoning: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HourlyReport {
    pub entries: Vec<HourlyUsage>,
    pub total_cost: f64,
    pub processing_time_ms: u32,
}

pub fn get_home_dir_string(home_dir_option: &Option<String>) -> Result<String, String> {
    home_dir_option
        .clone()
        .or_else(|| std::env::var("HOME").ok())
        .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().into_owned()))
        .ok_or_else(|| {
            "HOME directory not specified and could not determine home directory".to_string()
        })
}

#[allow(dead_code)]
fn parse_all_messages_with_pricing(
    home_dir: &str,
    clients: &[String],
    pricing: Option<&pricing::PricingService>,
) -> Vec<UnifiedMessage> {
    parse_all_messages_with_pricing_with_env_strategy(
        home_dir,
        clients,
        pricing,
        true,
        &scanner::ScannerSettings::default(),
        false,
    )
}

/// Midnight (today, 00:00) as Unix ms in the configured bucketing timezone.
/// Used by today-only scans to drop files older than today. Returns None only
/// on a DST midnight gap.
fn local_today_start_ms() -> Option<i64> {
    crate::bucket_tz::bucket_timezone().midnight_today_ms()
}

fn parse_all_messages_with_pricing_with_env_strategy(
    home_dir: &str,
    clients: &[String],
    pricing: Option<&pricing::PricingService>,
    use_env_roots: bool,
    scanner_settings: &scanner::ScannerSettings,
    today_only: bool,
) -> Vec<UnifiedMessage> {
    #[derive(Debug)]
    struct CachedParseOutcome {
        messages: Vec<UnifiedMessage>,
        cache_entry: Option<message_cache::CachedSourceEntry>,
        invalidate_cache: bool,
    }

    fn apply_pricing_to_messages(
        messages: &mut [UnifiedMessage],
        pricing: Option<&pricing::PricingService>,
    ) {
        for message in messages {
            message.refresh_derived_fields();
            apply_pricing_if_available(message, pricing);
        }
    }

    fn cached_messages(
        cached: &message_cache::CachedSourceEntry,
        pricing: Option<&pricing::PricingService>,
    ) -> Vec<UnifiedMessage> {
        let mut messages = cached.messages.clone();
        apply_pricing_to_messages(&mut messages, pricing);
        messages
    }

    fn parse_full_log_source(
        path: &Path,
        pricing: Option<&pricing::PricingService>,
        is_headless: bool,
    ) -> CachedParseOutcome {
        let fallback_timestamp = sessions::utils::file_modified_timestamp_ms(path);
        let parsed = sessions::codex::parse_codex_file_incremental(
            path,
            0,
            sessions::codex::CodexParseState::default(),
        );
        let messages = finalize_codex_messages(
            parsed.messages.clone(),
            pricing,
            is_headless,
            &parsed.fallback_timestamp_indices,
            fallback_timestamp,
        );
        if !parsed.parse_succeeded {
            return CachedParseOutcome {
                messages,
                cache_entry: None,
                invalidate_cache: false,
            };
        }

        if parsed.unresolved_model_events {
            return CachedParseOutcome {
                messages,
                cache_entry: None,
                invalidate_cache: false,
            };
        }

        let cache_entry = build_codex_cache_entry(
            path,
            parsed.messages,
            parsed.consumed_offset,
            parsed.state,
            parsed.fallback_timestamp_indices,
        );

        CachedParseOutcome {
            messages,
            cache_entry,
            invalidate_cache: false,
        }
    }

    fn finalize_codex_messages(
        mut messages: Vec<UnifiedMessage>,
        pricing: Option<&pricing::PricingService>,
        is_headless: bool,
        fallback_timestamp_indices: &[usize],
        fallback_timestamp: i64,
    ) -> Vec<UnifiedMessage> {
        for index in fallback_timestamp_indices {
            if let Some(message) = messages.get_mut(*index) {
                message.set_timestamp(fallback_timestamp);
            }
        }
        apply_pricing_to_messages(&mut messages, pricing);
        for message in &mut messages {
            apply_headless_agent(message, is_headless);
        }
        messages
    }

    fn build_codex_cache_entry(
        path: &Path,
        raw_messages: Vec<UnifiedMessage>,
        consumed_offset: u64,
        state: sessions::codex::CodexParseState,
        fallback_timestamp_indices: Vec<usize>,
    ) -> Option<message_cache::CachedSourceEntry> {
        let fingerprint = message_cache::SourceFingerprint::from_path(path)?;
        if fingerprint.size != consumed_offset {
            return None;
        }

        let codex_incremental = message_cache::build_codex_incremental_cache_with_prefix_hash(
            path,
            consumed_offset,
            state,
            fingerprint.content_hash,
        )?;

        Some(message_cache::CachedSourceEntry::new(
            message_cache::CacheIdentity::for_client(ClientId::Codex),
            path,
            fingerprint,
            raw_messages,
            fallback_timestamp_indices,
            Some(codex_incremental),
        ))
    }

    fn load_or_parse_source_with_fingerprint_and_policy<F, FingerprintFn>(
        identity: message_cache::CacheIdentity,
        path: &Path,
        source_cache: &message_cache::SourceMessageCache,
        pricing: Option<&pricing::PricingService>,
        fingerprint_from_path: FingerprintFn,
        parse: F,
    ) -> CachedParseOutcome
    where
        F: Fn(&Path, Option<&message_cache::SourceFingerprint>) -> (Vec<UnifiedMessage>, bool),
        FingerprintFn: Fn(
            &Path,
            Option<&message_cache::SourceFingerprint>,
        ) -> Option<message_cache::FingerprintStatus>,
    {
        let cached = source_cache.get(identity, path);
        let Some(fingerprint_status) =
            fingerprint_from_path(path, cached.map(|entry| &entry.fingerprint))
        else {
            let (mut messages, _) = parse(path, None);
            apply_pricing_to_messages(&mut messages, pricing);
            return CachedParseOutcome {
                messages,
                cache_entry: None,
                invalidate_cache: false,
            };
        };

        let fingerprint = match fingerprint_status {
            message_cache::FingerprintStatus::Unchanged => {
                let Some(cached) = cached else {
                    unreachable!("an uncached source always builds a complete fingerprint")
                };
                if !cached.messages.is_empty() {
                    return CachedParseOutcome {
                        messages: cached_messages(cached, pricing),
                        cache_entry: None,
                        invalidate_cache: false,
                    };
                }
                cached.fingerprint.clone()
            }
            message_cache::FingerprintStatus::Changed(fingerprint) => fingerprint,
        };

        if let Some(cached) = cached {
            if cached.fingerprint == fingerprint && !cached.messages.is_empty() {
                return CachedParseOutcome {
                    messages: cached_messages(cached, pricing),
                    cache_entry: None,
                    invalidate_cache: false,
                };
            }
        }

        let (mut messages, cacheable) = parse(path, Some(&fingerprint));
        let cache_entry = if messages.is_empty() || !cacheable {
            None
        } else {
            Some(message_cache::CachedSourceEntry::new(
                identity,
                path,
                fingerprint,
                messages.clone(),
                Vec::new(),
                None,
            ))
        };
        apply_pricing_to_messages(&mut messages, pricing);

        CachedParseOutcome {
            messages,
            cache_entry,
            invalidate_cache: !cacheable,
        }
    }

    fn load_or_parse_source_with_fingerprint<F, FingerprintFn>(
        identity: message_cache::CacheIdentity,
        path: &Path,
        source_cache: &message_cache::SourceMessageCache,
        pricing: Option<&pricing::PricingService>,
        fingerprint_from_path: FingerprintFn,
        parse: F,
    ) -> CachedParseOutcome
    where
        F: Fn(&Path) -> Vec<UnifiedMessage>,
        FingerprintFn: Fn(
            &Path,
            Option<&message_cache::SourceFingerprint>,
        ) -> Option<message_cache::FingerprintStatus>,
    {
        load_or_parse_source_with_fingerprint_and_policy(
            identity,
            path,
            source_cache,
            pricing,
            fingerprint_from_path,
            |path, _| (parse(path), true),
        )
    }

    fn load_or_parse_source_with_fingerprint_context<F, FingerprintFn>(
        identity: message_cache::CacheIdentity,
        path: &Path,
        source_cache: &message_cache::SourceMessageCache,
        pricing: Option<&pricing::PricingService>,
        fingerprint_from_path: FingerprintFn,
        parse: F,
    ) -> CachedParseOutcome
    where
        F: Fn(&Path, Option<&message_cache::SourceFingerprint>) -> Vec<UnifiedMessage>,
        FingerprintFn: Fn(
            &Path,
            Option<&message_cache::SourceFingerprint>,
        ) -> Option<message_cache::FingerprintStatus>,
    {
        load_or_parse_source_with_fingerprint_and_policy(
            identity,
            path,
            source_cache,
            pricing,
            fingerprint_from_path,
            |path, fingerprint| (parse(path, fingerprint), true),
        )
    }

    fn load_or_parse_source<F>(
        identity: message_cache::CacheIdentity,
        path: &Path,
        source_cache: &message_cache::SourceMessageCache,
        pricing: Option<&pricing::PricingService>,
        parse: F,
    ) -> CachedParseOutcome
    where
        F: Fn(&Path) -> Vec<UnifiedMessage>,
    {
        load_or_parse_source_with_fingerprint(
            identity,
            path,
            source_cache,
            pricing,
            message_cache::SourceFingerprint::check_path_samples_only,
            parse,
        )
    }

    fn load_or_parse_sqlite_source<F>(
        identity: message_cache::CacheIdentity,
        path: &Path,
        source_cache: &message_cache::SourceMessageCache,
        pricing: Option<&pricing::PricingService>,
        parse: F,
    ) -> CachedParseOutcome
    where
        F: Fn(&Path) -> Vec<UnifiedMessage>,
    {
        load_or_parse_source_with_fingerprint(
            identity,
            path,
            source_cache,
            pricing,
            message_cache::SourceFingerprint::check_sqlite_path,
            parse,
        )
    }

    fn load_or_parse_codex_source(
        path: &Path,
        source_cache: &message_cache::SourceMessageCache,
        pricing: Option<&pricing::PricingService>,
        headless_roots: &[PathBuf],
    ) -> CachedParseOutcome {
        let identity = message_cache::CacheIdentity::for_client(ClientId::Codex);
        let is_headless = is_headless_path(path, headless_roots);
        let cached = source_cache.get(identity, path);
        if cached.is_none() {
            // The post-parse cache build computes the authoritative fingerprint
            // after reading the file. Avoid hashing an uncached source here
            // only to discard that digest before parsing it.
            return parse_full_log_source(path, pricing, is_headless);
        }
        let Some(fingerprint_status) = message_cache::SourceFingerprint::check_path(
            path,
            cached.map(|entry| &entry.fingerprint),
        ) else {
            return parse_full_log_source(path, pricing, is_headless);
        };
        let fingerprint = match fingerprint_status {
            message_cache::FingerprintStatus::Unchanged => cached
                .expect("an uncached source always builds a complete fingerprint")
                .fingerprint
                .clone(),
            message_cache::FingerprintStatus::Changed(fingerprint) => fingerprint,
        };
        let fallback_timestamp = sessions::utils::file_modified_timestamp_ms(path);

        if let Some(cached) = cached {
            let reparse_from_start = |invalidate_cache: bool| {
                let mut outcome = parse_full_log_source(path, pricing, is_headless);
                outcome.invalidate_cache = invalidate_cache && outcome.cache_entry.is_none();
                outcome
            };

            if cached.fingerprint == fingerprint {
                if message_cache::codex_cache_entry_matches_fingerprint(cached, &fingerprint) {
                    return CachedParseOutcome {
                        messages: finalize_codex_messages(
                            cached.messages.clone(),
                            pricing,
                            is_headless,
                            &cached.fallback_timestamp_indices,
                            fallback_timestamp,
                        ),
                        cache_entry: None,
                        invalidate_cache: false,
                    };
                }

                return reparse_from_start(true);
            }

            if let Some(codex_incremental) = cached.codex_incremental.as_ref() {
                if fingerprint.size > codex_incremental.consumed_offset
                    && message_cache::codex_prefix_matches(path, codex_incremental)
                {
                    let parsed = sessions::codex::parse_codex_file_incremental(
                        path,
                        codex_incremental.consumed_offset,
                        codex_incremental.state.clone(),
                    );
                    if parsed.parse_succeeded && !parsed.unresolved_model_events {
                        let mut raw_messages = cached.messages.clone();
                        let mut fallback_timestamp_indices =
                            cached.fallback_timestamp_indices.clone();
                        let existing_len = raw_messages.len();
                        fallback_timestamp_indices.extend(
                            parsed
                                .fallback_timestamp_indices
                                .iter()
                                .map(|index| existing_len + index),
                        );
                        raw_messages.extend(parsed.messages.clone());
                        let cache_entry = build_codex_cache_entry(
                            path,
                            raw_messages.clone(),
                            parsed.consumed_offset,
                            parsed.state,
                            fallback_timestamp_indices.clone(),
                        );
                        if let Some(cache_entry) = cache_entry {
                            let messages = finalize_codex_messages(
                                raw_messages,
                                pricing,
                                is_headless,
                                &fallback_timestamp_indices,
                                fallback_timestamp,
                            );

                            return CachedParseOutcome {
                                messages,
                                cache_entry: Some(cache_entry),
                                invalidate_cache: false,
                            };
                        }
                    }
                }
            }

            return reparse_from_start(true);
        }

        unreachable!("uncached Codex sources return before fingerprint validation")
    }

    let mut scan_result = scanner::scan_all_clients_with_scanner_settings(
        home_dir,
        clients,
        use_env_roots,
        scanner_settings,
    );
    // today-only: drop every transcript not touched today before we read any of
    // them, so the scan only pays for today's files (the menu bar's light scan).
    if today_only {
        if let Some(today_start) = local_today_start_ms() {
            scan_result.retain_files_modified_since(today_start);
        }
    }
    let headless_roots = scanner::headless_roots_with_env_strategy(home_dir, use_env_roots);
    let mut source_cache = message_cache::SourceMessageCache::load();
    source_cache.prune_missing_files();
    let mut all_messages: Vec<UnifiedMessage> = Vec::new();
    let include_all = clients.is_empty();
    let include_synthetic = include_all || clients.iter().any(|c| c == "synthetic");
    let include_devin_cli = include_synthetic || clients.iter().any(|c| c == "devin-cli");
    let include_devin_desktop = include_synthetic || clients.iter().any(|c| c == "devin-desktop");

    // Parse OpenCode: prefer SQLite, collapse forked SQLite history there, then
    // suppress legacy JSON overlap by message identity.
    let mut opencode_seen: HashSet<String> = HashSet::new();

    for db_path in &scan_result.opencode_dbs {
        let CachedParseOutcome {
            messages,
            cache_entry,
            ..
        } = load_or_parse_sqlite_source(
            message_cache::CacheIdentity::for_client(ClientId::OpenCode),
            db_path,
            &source_cache,
            pricing,
            sessions::opencode::parse_opencode_sqlite,
        );

        // Dedup across channel-suffixed dbs: the same session can end up in
        // both `opencode.db` and `opencode-<channel>.db` if the user
        // switches channels mid-session. `discover_opencode_dbs` returns
        // paths in sorted order, so the first-seen copy is deterministic.
        all_messages.extend(messages.into_iter().filter(|message| {
            message
                .dedup_key
                .as_ref()
                .is_none_or(|key| opencode_seen.insert(key.clone()))
        }));

        if let Some(entry) = cache_entry {
            source_cache.insert(entry);
        }
    }

    let opencode_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::OpenCode)
        .par_iter()
        .filter_map(|path| {
            Some(load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::OpenCode),
                path,
                &source_cache,
                pricing,
                |path| {
                    sessions::opencode::parse_opencode_file(path)
                        .into_iter()
                        .collect()
                },
            ))
        })
        .collect();
    for outcome in opencode_outcomes {
        all_messages.extend(outcome.messages.into_iter().filter(|message| {
            message
                .dedup_key
                .as_ref()
                .is_none_or(|key| opencode_seen.insert(key.clone()))
        }));
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    // Parse MiMo Code: SQLite database(s)
    let mut micode_seen: HashSet<String> = HashSet::new();

    for db_path in &scan_result.micode_dbs {
        // Pass `None` so the loader does not reprice: MiMo Code carries an
        // authoritative per-message cost that unconditional repricing would
        // overwrite (and persist to the cache). Reprice only messages that had
        // no embedded cost, mirroring the gjc lane's guard.
        let CachedParseOutcome {
            messages,
            cache_entry,
            ..
        } = load_or_parse_sqlite_source(
            message_cache::CacheIdentity::for_client(ClientId::MiMoCode),
            db_path,
            &source_cache,
            None,
            sessions::micode::parse_micode_sqlite,
        );

        all_messages.extend(
            messages
                .into_iter()
                .map(|mut message| {
                    if message.cost <= 0.0 {
                        apply_pricing_if_available(&mut message, pricing);
                    }
                    message
                })
                .filter(|message| {
                    message
                        .dedup_key
                        .as_ref()
                        .is_none_or(|key| micode_seen.insert(key.clone()))
                }),
        );

        if let Some(entry) = cache_entry {
            source_cache.insert(entry);
        }
    }

    let claude_home = PathBuf::from(home_dir);
    let claude_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Claude)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::Claude),
                path,
                &source_cache,
                pricing,
                |path, cached| {
                    message_cache::SourceFingerprint::check_claude_code_path_with_home_samples_only(
                        path,
                        cached,
                        Some(&claude_home),
                    )
                },
                |path| sessions::claudecode::parse_claude_file_with_home(path, Some(&claude_home)),
            )
        })
        .collect();
    let mut claude_messages_raw: Vec<(String, UnifiedMessage)> = Vec::new();
    for outcome in claude_outcomes {
        claude_messages_raw.extend(outcome.messages.into_iter().map(|msg| {
            let dedup_key = msg.dedup_key.clone().unwrap_or_default();
            (dedup_key, msg)
        }));
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let mut seen_keys: HashSet<String> = HashSet::new();
    let claude_messages: Vec<UnifiedMessage> = claude_messages_raw
        .into_iter()
        .filter(|(key, _)| key.is_empty() || seen_keys.insert(key.clone()))
        .map(|(_, msg)| msg)
        .collect();
    all_messages.extend(claude_messages);

    let codex_outcomes: Vec<(PathBuf, CachedParseOutcome)> = scan_result
        .get(ClientId::Codex)
        .par_iter()
        .map(|path| {
            (
                path.clone(),
                load_or_parse_codex_source(path, &source_cache, pricing, &headless_roots),
            )
        })
        .collect();
    let mut codex_seen: HashSet<String> = HashSet::new();
    for (path, outcome) in codex_outcomes {
        all_messages.extend(
            outcome
                .messages
                .into_iter()
                .filter(|message| should_keep_deduped_message(&mut codex_seen, message)),
        );
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        } else if outcome.invalidate_cache {
            source_cache.remove(
                message_cache::CacheIdentity::for_client(ClientId::Codex),
                &path,
            );
        }
    }

    let copilot_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Copilot)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::Copilot),
                path,
                &source_cache,
                pricing,
                sessions::copilot::parse_copilot_file,
            )
        })
        .collect();
    for outcome in copilot_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }
    if let Some(db_path) = &scan_result.copilot_desktop_db {
        let otel_sessions: HashSet<String> = all_messages
            .iter()
            .filter(|message| message.client == "copilot")
            .map(|message| message.session_id.clone())
            .collect();
        let desktop_msgs = sessions::copilot_desktop::parse_copilot_desktop_db(db_path);
        all_messages.extend(
            desktop_msgs
                .into_iter()
                .filter(|message| !otel_sessions.contains(&message.session_id))
                .map(|mut message| {
                    apply_pricing_if_available(&mut message, pricing);
                    message
                }),
        );
    }
    {
        let existing_dedup_keys: HashSet<String> = all_messages
            .iter()
            .filter(|m| m.client == "copilot")
            .filter_map(|m| m.dedup_key.clone())
            .collect();
        let existing_copilot_session_timestamps: HashSet<(String, i64)> = all_messages
            .iter()
            .filter(|m| m.client == "copilot")
            .map(|m| (m.session_id.clone(), m.timestamp))
            .collect();
        let vscode_msgs = sessions::copilot_vscode::parse_copilot_vscode_sessions(
            &scan_result.copilot_vscode_sessions,
        );
        all_messages.extend(
            vscode_msgs
                .into_iter()
                .filter(|m| {
                    let key_unique = m
                        .dedup_key
                        .as_deref()
                        .map(|k| !existing_dedup_keys.contains(k))
                        .unwrap_or(true);
                    let session_ts_unique = !existing_copilot_session_timestamps
                        .contains(&(m.session_id.clone(), m.timestamp));
                    key_unique && session_ts_unique
                })
                .map(|mut message| {
                    apply_pricing_if_available(&mut message, pricing);
                    message
                }),
        );
    }

    let gemini_outcomes: Vec<(PathBuf, CachedParseOutcome)> = scan_result
        .get(ClientId::Gemini)
        .par_iter()
        .map(|path| {
            let outcome = load_or_parse_source_with_fingerprint_and_policy(
                message_cache::CacheIdentity::for_client(ClientId::Gemini),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_path_samples_only,
                |path, _| {
                    let parsed = sessions::gemini::parse_gemini_file_with_cache_status(path);
                    (parsed.messages, parsed.cacheable)
                },
            );
            (path.clone(), outcome)
        })
        .collect();
    for (path, outcome) in gemini_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        } else if outcome.invalidate_cache {
            source_cache.remove(
                message_cache::CacheIdentity::for_client(ClientId::Gemini),
                &path,
            );
        }
    }

    let cursor_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Cursor)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::Cursor),
                path,
                &source_cache,
                pricing,
                sessions::cursor::parse_cursor_file,
            )
        })
        .collect();
    for outcome in cursor_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let warp_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Warp)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::Warp),
                path,
                &source_cache,
                pricing,
                sessions::warp::parse_warp_file,
            )
        })
        .collect();
    for outcome in warp_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let grok_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Grok)
        .par_iter()
        .map(|path| {
            // Use a Grok-aware fingerprint: parse output depends on the sibling
            // signals.json rollup, so that file must participate in the cache key
            // or a late/updated rollup is ignored forever for cached sessions.
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::Grok),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_grok_path_samples_only,
                sessions::grok::parse_grok_updates_file,
            )
        })
        .collect();
    for outcome in grok_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let jcode_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Jcode)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::Jcode),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_jcode_path_samples_only,
                sessions::jcode::parse_jcode_file,
            )
        })
        .collect();
    let mut jcode_seen: HashSet<String> = HashSet::new();
    for outcome in jcode_outcomes {
        all_messages.extend(
            outcome
                .messages
                .into_iter()
                .filter(|message| should_keep_deduped_message(&mut jcode_seen, message)),
        );
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let amp_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Amp)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::Amp),
                path,
                &source_cache,
                pricing,
                sessions::amp::parse_amp_file,
            )
        })
        .collect();
    for outcome in amp_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let codebuff_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Codebuff)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::Codebuff),
                path,
                &source_cache,
                pricing,
                sessions::codebuff::parse_codebuff_file,
            )
        })
        .collect();
    for outcome in codebuff_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let droid_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Droid)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::Droid),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_droid_path_samples_only,
                sessions::droid::parse_droid_file,
            )
        })
        .collect();
    for outcome in droid_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let openclaw_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::OpenClaw)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::OpenClaw),
                path,
                &source_cache,
                pricing,
                sessions::openclaw::parse_openclaw_transcript,
            )
        })
        .collect();
    for outcome in openclaw_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let pi_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Pi)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::Pi),
                path,
                &source_cache,
                pricing,
                sessions::pi::parse_pi_file,
            )
        })
        .collect();
    for outcome in pi_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    // Command Code does not persist token usage or cost locally, so tokens are
    // estimated and priced. The model id comes from ~/.commandcode/config.json
    // (canonicalized, e.g. "MiniMaxAI/MiniMax-M3-Free" -> "MiniMax-M3"), not the
    // transcript, so the source cache — which fingerprints only the transcript
    // file — is bypassed: otherwise a config.json model change would leave stale
    // cached pricing until the transcript itself changed.
    let commandcode_messages: Vec<UnifiedMessage> = scan_result
        .get(ClientId::CommandCode)
        .par_iter()
        .flat_map(|path| {
            sessions::commandcode::parse_commandcode_file(path)
                .into_iter()
                .map(|mut msg| {
                    apply_pricing_if_available(&mut msg, pricing);
                    msg
                })
                .collect::<Vec<_>>()
        })
        .collect();
    all_messages.extend(commandcode_messages);

    // gjc (gajae-code) JSONL sessions. Binding note N1: this cached cluster
    // MUST obtain messages via the non-repricing parser and apply the A1
    // Hermes guard explicitly (reprice only when the embedded usage.cost.total
    // was absent, i.e. cost <= 0.0). Routing through load_or_parse_source /
    // apply_pricing_to_messages / cached_messages would reprice unconditionally
    // and overwrite gjc's authoritative embedded cost, silently downgrading to
    // A2 on the dominant cached path. Message-level dedup via
    // should_keep_deduped_message collapses depth-1/depth-2 replays.
    let mut gjc_seen: HashSet<String> = HashSet::new();
    let gjc_messages: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Gjc)
        .par_iter()
        .flat_map(|path| {
            sessions::gjc::parse_gjc_file(path)
                .into_iter()
                .map(|mut msg| {
                    if msg.cost <= 0.0 {
                        apply_pricing_if_available(&mut msg, pricing);
                    }
                    msg
                })
                .collect::<Vec<_>>()
        })
        .collect();
    all_messages.extend(
        gjc_messages
            .into_iter()
            .filter(|message| should_keep_deduped_message(&mut gjc_seen, message)),
    );

    // Junie events carry authoritative per-call `modelUsage.cost` values.
    // Keep this off the generic source cache because cached_messages()
    // reprices every message unconditionally; only fill cost from pricing
    // when Junie emitted no usable cost.
    let mut junie_seen: HashSet<String> = HashSet::new();
    let junie_messages: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Junie)
        .par_iter()
        .flat_map(|path| {
            sessions::junie::parse_junie_file(path)
                .into_iter()
                .map(|mut msg| {
                    if msg.cost <= 0.0 {
                        apply_pricing_if_available(&mut msg, pricing);
                    }
                    msg
                })
                .collect::<Vec<_>>()
        })
        .collect();
    all_messages.extend(
        junie_messages
            .into_iter()
            .filter(|message| should_keep_deduped_message(&mut junie_seen, message)),
    );

    // ZCode v2 CLI stores authoritative model usage in SQLite.
    if let Some(db_path) = &scan_result.zcode_db {
        let CachedParseOutcome {
            messages,
            cache_entry,
            ..
        } = load_or_parse_sqlite_source(
            message_cache::CacheIdentity::for_client(ClientId::Zcode),
            db_path,
            &source_cache,
            pricing,
            sessions::zcode::parse_zcode_sqlite,
        );
        all_messages.extend(messages);
        if let Some(entry) = cache_entry {
            source_cache.insert(entry);
        }
    }

    // ZCode (Z.ai GLM-5.2 ADE) JSONL sessions. Token usage may be embedded
    // from the API response; otherwise estimated from content.
    let zcode_messages: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Zcode)
        .par_iter()
        .flat_map(|path| {
            sessions::zcode::parse_zcode_file(path)
                .into_iter()
                .map(|mut msg| {
                    apply_pricing_if_available(&mut msg, pricing);
                    msg
                })
                .collect::<Vec<_>>()
        })
        .collect();
    all_messages.extend(zcode_messages);

    let kimi_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Kimi)
        .par_iter()
        .map(|path| {
            let parse: fn(&Path) -> Vec<UnifiedMessage> = if sessions::kimi::is_kimi_code_path(path)
            {
                sessions::kimi::parse_kimi_code_file
            } else {
                sessions::kimi::parse_kimi_file
            };
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::Kimi),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_kimi_path_samples_only,
                parse,
            )
        })
        .collect();
    for outcome in kimi_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    // Parse Qwen files
    let qwen_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Qwen)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::Qwen),
                path,
                &source_cache,
                pricing,
                sessions::qwen::parse_qwen_file,
            )
        })
        .collect();
    for outcome in qwen_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let roocode_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::RooCode)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::RooCode),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_roo_path_samples_only,
                sessions::roocode::parse_roocode_file,
            )
        })
        .collect();
    for outcome in roocode_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let kilocode_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::KiloCode)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::KiloCode),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_roo_path_samples_only,
                sessions::kilocode::parse_kilocode_file,
            )
        })
        .collect();
    for outcome in kilocode_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let cline_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Cline)
        .par_iter()
        .map(|path| {
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::Cline),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_roo_path_samples_only,
                sessions::cline::parse_cline_file,
            )
        })
        .collect();
    for outcome in cline_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let mux_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Mux)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::Mux),
                path,
                &source_cache,
                pricing,
                sessions::mux::parse_mux_file,
            )
        })
        .collect();
    for outcome in mux_outcomes {
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    // Kilo CLI: SQLite database
    if let Some(db_path) = &scan_result.kilo_db {
        let kilo_messages: Vec<UnifiedMessage> = sessions::kilo::parse_kilo_sqlite(db_path)
            .into_iter()
            .map(|mut msg| {
                apply_pricing_if_available(&mut msg, pricing);
                msg
            })
            .collect();
        all_messages.extend(kilo_messages);
    }

    let mut hermes_seen: HashSet<String> = HashSet::new();
    for db_path in scan_result.hermes_db_paths() {
        let hermes_messages = parse_hermes_sqlite_with_pricing(&db_path, pricing);
        all_messages.extend(
            hermes_messages
                .into_iter()
                .filter(|message| should_keep_deduped_message(&mut hermes_seen, message)),
        );
    }

    if let Some(db_path) = &scan_result.goose_db {
        let goose_messages: Vec<UnifiedMessage> = sessions::goose::parse_goose_sqlite(db_path)
            .into_iter()
            .map(|mut msg| {
                apply_pricing_if_available(&mut msg, pricing);
                msg
            })
            .collect();
        all_messages.extend(goose_messages);
    }

    // Devin CLI stores authoritative model usage in SQLite. Multiple paths can
    // be configured through scanner extra roots, so parse and dedupe all of
    // them instead of silently ignoring non-default databases.
    let mut devin_cli_session_ids: HashSet<String> = HashSet::new();
    if include_devin_cli {
        let devin_cli_outcomes: Vec<CachedParseOutcome> = scan_result
            .devin_dbs
            .par_iter()
            .map(|db_path| {
                load_or_parse_sqlite_source(
                    message_cache::CacheIdentity::for_client(ClientId::DevinCli),
                    db_path,
                    &source_cache,
                    pricing,
                    sessions::devin::parse_devin_cli_sqlite,
                )
            })
            .collect();
        let mut devin_cli_seen = HashSet::new();
        for outcome in devin_cli_outcomes {
            for message in outcome
                .messages
                .into_iter()
                .filter(|message| should_keep_deduped_message(&mut devin_cli_seen, message))
            {
                devin_cli_session_ids.insert(message.session_id.clone());
                all_messages.push(message);
            }
            if let Some(entry) = outcome.cache_entry {
                source_cache.insert(entry);
            }
        }
    }

    for db_path in scan_result.zed_db_paths() {
        let outcome = load_or_parse_sqlite_source(
            message_cache::CacheIdentity::for_client(ClientId::Zed),
            &db_path,
            &source_cache,
            pricing,
            sessions::zed::parse_zed_sqlite,
        );
        all_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    let kiro_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::Kiro)
        .par_iter()
        .map(|path| {
            // Kiro-aware fingerprint: IDE `sess_*/session.json` sources derive
            // their token counts from the sibling `messages.jsonl`, so that
            // file must participate in the cache key or an append landing
            // after the last `session.json` write is ignored forever.
            load_or_parse_source_with_fingerprint(
                message_cache::CacheIdentity::for_client(ClientId::Kiro),
                path,
                &source_cache,
                pricing,
                message_cache::SourceFingerprint::check_kiro_path_samples_only,
                sessions::kiro::parse_kiro_file,
            )
        })
        .collect();
    // Collect Kiro file messages before extending so snapshot suppression can
    // see execution coverage across files (it is a cross-file merge concern,
    // like merge_workbuddy_messages, and must run after cache loads).
    let mut kiro_file_messages: Vec<UnifiedMessage> = Vec::new();
    for outcome in kiro_outcomes {
        kiro_file_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }
    all_messages.extend(sessions::kiro::suppress_snapshots_covered_by_executions(
        kiro_file_messages,
    ));

    if let Some(db_path) = &scan_result.kiro_db {
        let kiro_db_messages: Vec<UnifiedMessage> = sessions::kiro::parse_kiro_sqlite(db_path)
            .into_iter()
            .map(|mut msg| {
                apply_pricing_if_available(&mut msg, pricing);
                msg
            })
            .collect();
        all_messages.extend(kiro_db_messages);
    }

    for source in &scan_result.crush_dbs {
        let crush_messages: Vec<UnifiedMessage> =
            sessions::crush::parse_crush_sqlite(&source.db_path)
                .into_iter()
                .map(|mut msg| {
                    msg.set_workspace(source.workspace_key.clone(), source.workspace_label.clone());
                    apply_pricing_if_available(&mut msg, pricing);
                    msg
                })
                .collect();
        all_messages.extend(crush_messages);
    }

    let antigravity_messages: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Antigravity)
        .par_iter()
        .flat_map(|path| {
            sessions::antigravity::parse_antigravity_file(path)
                .into_iter()
                .map(|mut msg| {
                    apply_pricing_if_available(&mut msg, pricing);
                    msg
                })
                .collect::<Vec<_>>()
        })
        .collect();
    all_messages.extend(antigravity_messages);

    let antigravity_cli_messages: Vec<UnifiedMessage> = scan_result
        .get(ClientId::AntigravityCli)
        .par_iter()
        .flat_map(|path| {
            sessions::antigravity_cli::parse_antigravity_cli_file(path)
                .into_iter()
                .map(|mut msg| {
                    apply_pricing_if_available(&mut msg, pricing);
                    msg
                })
                .collect::<Vec<_>>()
        })
        .collect();
    all_messages.extend(antigravity_cli_messages);

    // Trae API dump uses exact dollar_float totals, so pricing lookup is not needed.
    let trae_messages: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Trae)
        .par_iter()
        .flat_map(|path| sessions::trae::parse_trae_file("trae", path))
        .collect();
    let deduped_trae_messages = dedupe_latest_trae_messages(trae_messages);
    all_messages.extend(deduped_trae_messages);

    let codebuddy_outcomes: Vec<CachedParseOutcome> = scan_result
        .get(ClientId::CodeBuddy)
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::CodeBuddy),
                path,
                &source_cache,
                pricing,
                sessions::codebuddy::parse_codebuddy_file,
            )
        })
        .collect();
    let mut codebuddy_seen: HashSet<String> = HashSet::new();
    for outcome in codebuddy_outcomes {
        all_messages.extend(outcome.messages.into_iter().filter(|message| {
            message
                .dedup_key
                .as_ref()
                .is_none_or(|key| codebuddy_seen.insert(key.clone()))
        }));
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }

    // Devin Desktop ACP file names are unrelated to the CLI database session
    // ids. Resolve their session titles through the database so the CLI can
    // take precedence only when both sources really describe one session.
    if include_devin_desktop {
        // Lookups are constructed only for cache misses. Key them by the
        // post-validation database snapshot so parallel misses that observe
        // different SQLite states never share stale metadata; identical
        // snapshots still share one query on a cold scan.
        let devin_desktop_lookups = DevinDesktopLookupCache::default();
        let devin_desktop_outcomes: Vec<CachedParseOutcome> = scan_result
            .get(ClientId::DevinDesktop)
            .par_iter()
            .map(|path| {
                load_or_parse_source_with_fingerprint_context(
                    message_cache::CacheIdentity::for_client(ClientId::DevinDesktop),
                    path,
                    &source_cache,
                    pricing,
                    |path, cached| {
                        message_cache::SourceFingerprint::check_devin_desktop_path_samples_only(
                            path,
                            &scan_result.devin_dbs,
                            cached,
                        )
                    },
                    |path, fingerprint| {
                        if let Some(fingerprint) = fingerprint {
                            let lookup_cell = devin_desktop_lookup_cell_for_snapshot(
                                &devin_desktop_lookups,
                                &scan_result.devin_dbs,
                                fingerprint,
                            );
                            let lookup = lookup_cell.get_or_init(|| {
                                sessions::devin::load_devin_desktop_session_lookup(
                                    &scan_result.devin_dbs,
                                )
                            });
                            sessions::devin::parse_devin_desktop_ndjson_with_lookup(path, lookup)
                        } else {
                            // Unreadable sources cannot produce a cache entry,
                            // so they do not need a snapshot-keyed lookup.
                            sessions::devin::parse_devin_desktop_ndjson_with_lookup(
                                path,
                                &sessions::devin::load_devin_desktop_session_lookup(
                                    &scan_result.devin_dbs,
                                ),
                            )
                        }
                    },
                )
            })
            .collect();
        for outcome in devin_desktop_outcomes {
            all_messages.extend(
                outcome
                    .messages
                    .into_iter()
                    .filter(|message| !devin_cli_session_ids.contains(&message.session_id)),
            );
            if let Some(entry) = outcome.cache_entry {
                source_cache.insert(entry);
            }
        }
    }

    let (workbuddy_detailed_paths, workbuddy_fallback_paths) =
        partition_workbuddy_paths(scan_result.get(ClientId::WorkBuddy));
    let workbuddy_detailed_outcomes: Vec<CachedParseOutcome> = workbuddy_detailed_paths
        .par_iter()
        .map(|path| {
            load_or_parse_source(
                message_cache::CacheIdentity::for_client(ClientId::WorkBuddy),
                path,
                &source_cache,
                pricing,
                sessions::workbuddy::parse_workbuddy_file,
            )
        })
        .collect();
    let workbuddy_fallback_outcomes: Vec<CachedParseOutcome> = workbuddy_fallback_paths
        .par_iter()
        .map(|path| {
            load_or_parse_sqlite_source(
                message_cache::CacheIdentity::for_client(ClientId::WorkBuddy),
                path,
                &source_cache,
                pricing,
                sessions::workbuddy::parse_workbuddy_file,
            )
        })
        .collect();
    let mut workbuddy_detailed_messages = Vec::new();
    for outcome in workbuddy_detailed_outcomes {
        workbuddy_detailed_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }
    let mut workbuddy_fallback_messages = Vec::new();
    for outcome in workbuddy_fallback_outcomes {
        workbuddy_fallback_messages.extend(outcome.messages);
        if let Some(entry) = outcome.cache_entry {
            source_cache.insert(entry);
        }
    }
    all_messages.extend(merge_workbuddy_messages(
        workbuddy_detailed_messages,
        workbuddy_fallback_messages,
    ));

    if include_synthetic {
        if let Some(db_path) = &scan_result.synthetic_db {
            let outcome = load_or_parse_sqlite_source(
                message_cache::CacheIdentity::synthetic(),
                db_path,
                &source_cache,
                pricing,
                sessions::synthetic::parse_octofriend_sqlite,
            );
            all_messages.extend(outcome.messages);
            if let Some(entry) = outcome.cache_entry {
                source_cache.insert(entry);
            }
        }
    }

    // Filter BEFORE normalization so retain_for_requested_clients can see
    // original model/provider prefixes (e.g. "accounts/fireworks/models/…")
    // that is_synthetic_gateway relies on for gateway detection.
    if !include_all {
        let requested: HashSet<&str> = clients.iter().map(String::as_str).collect();
        all_messages.retain(|msg| {
            retain_for_requested_clients(&msg.client, &msg.model_id, &msg.provider_id, &requested)
        });
    }

    if include_synthetic {
        for msg in &mut all_messages {
            sessions::synthetic::normalize_synthetic_gateway_fields(
                &mut msg.model_id,
                &mut msg.provider_id,
            );
        }
    }

    source_cache.save_if_dirty();

    all_messages
}

fn dedupe_latest_trae_messages(mut messages: Vec<UnifiedMessage>) -> Vec<UnifiedMessage> {
    let mut latest_by_session: HashMap<String, UnifiedMessage> = HashMap::new();

    for message in messages.drain(..) {
        let session_id = message.session_id.clone();
        match latest_by_session.get_mut(&session_id) {
            Some(existing) => {
                let should_replace = message.timestamp > existing.timestamp
                    || (message.timestamp == existing.timestamp
                        && message.dedup_key.as_ref().is_some_and(|key| {
                            existing
                                .dedup_key
                                .as_ref()
                                .is_none_or(|existing_key| key > existing_key)
                        }));
                if should_replace {
                    *existing = message;
                }
            }
            None => {
                let _ = latest_by_session.insert(session_id, message);
            }
        }
    }

    let mut deduped: Vec<UnifiedMessage> = latest_by_session.into_values().collect();
    deduped.sort_unstable_by(|a, b| {
        a.session_id
            .cmp(&b.session_id)
            .then_with(|| a.timestamp.cmp(&b.timestamp))
    });
    deduped
}

fn partition_workbuddy_paths(paths: &[PathBuf]) -> (Vec<&PathBuf>, Vec<&PathBuf>) {
    paths
        .iter()
        .partition(|path| sessions::workbuddy::is_detailed_workbuddy_source(path))
}

fn merge_workbuddy_messages(
    detailed_messages: Vec<UnifiedMessage>,
    fallback_messages: Vec<UnifiedMessage>,
) -> Vec<UnifiedMessage> {
    // The SQLite fallback carries ONE cumulative row per session (dated solely by
    // `updated_at`), while the detailed JSONL carries accurate per-message rows.
    // A fallback row is redundant exactly when its session already has detailed
    // coverage — independent of which calendar day `updated_at` lands on. Keying
    // this on the session (not the date) fixes two failures of the old
    // date-overlap check: it no longer double-counts a session whose aggregate
    // lands on a day with no detailed rows, and no longer drops a fallback-only
    // session that merely shares a day with unrelated detailed activity. Both
    // parsers derive `session_id` from the same WorkBuddy session identifier, so
    // the keys are directly comparable.
    let detailed_sessions: HashSet<String> = detailed_messages
        .iter()
        .filter(|message| !message.session_id.is_empty())
        .map(|message| message.session_id.clone())
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged: Vec<UnifiedMessage> = detailed_messages
        .into_iter()
        .filter(|message| should_keep_deduped_message(&mut seen, message))
        .collect();

    merged.extend(fallback_messages.into_iter().filter(|message| {
        !detailed_sessions.contains(&message.session_id)
            && should_keep_deduped_message(&mut seen, message)
    }));
    merged
}

fn filter_unified_messages(
    messages: Vec<UnifiedMessage>,
    options: &LocalParseOptions,
) -> Vec<UnifiedMessage> {
    let mut filtered = messages;

    if let Some(year) = &options.year {
        let year_prefix = format!("{}-", year);
        filtered.retain(|m| m.date.starts_with(&year_prefix));
    }

    if let Some(since) = &options.since {
        filtered.retain(|m| m.date.as_str() >= since.as_str());
    }

    if let Some(until) = &options.until {
        filtered.retain(|m| m.date.as_str() <= until.as_str());
    }

    filtered
}

fn workspace_bucket(msg: &UnifiedMessage) -> (String, Option<String>, String) {
    match (&msg.workspace_key, &msg.workspace_label) {
        (Some(key), Some(label)) => (key.clone(), Some(key.clone()), label.clone()),
        (Some(key), None) => (
            key.clone(),
            Some(key.clone()),
            sessions::workspace_label_from_key(key)
                .unwrap_or_else(|| UNKNOWN_WORKSPACE_LABEL.to_string()),
        ),
        _ => (
            UNKNOWN_WORKSPACE_GROUP_KEY.to_string(),
            None,
            UNKNOWN_WORKSPACE_LABEL.to_string(),
        ),
    }
}

fn aggregate_model_usage_entries(
    messages: Vec<UnifiedMessage>,
    group_by: &GroupBy,
) -> Vec<ModelUsage> {
    let mut model_map: HashMap<String, ModelUsage> = HashMap::new();

    for msg in messages {
        let normalized = model_name_for_grouping(&msg.client, &msg.provider_id, &msg.model_id);
        let (workspace_group_key, workspace_key, workspace_label) = workspace_bucket(&msg);
        let key = match group_by {
            GroupBy::Model => normalized.clone(),
            GroupBy::ClientModel => format!("{}:{}", msg.client, normalized),
            GroupBy::ClientProviderModel => {
                format!("{}:{}:{}", msg.client, msg.provider_id, normalized)
            }
            GroupBy::WorkspaceModel => format!("{}:{}", workspace_group_key, normalized),
            GroupBy::Session => format!("{}:{}", msg.session_id, normalized),
            GroupBy::ClientSession => {
                format!("{}:{}:{}", msg.client, msg.session_id, normalized)
            }
        };
        let merge_clients = matches!(group_by, GroupBy::Model | GroupBy::WorkspaceModel);
        let session_grouped = matches!(group_by, GroupBy::Session | GroupBy::ClientSession);
        let entry = model_map.entry(key).or_insert_with(|| ModelUsage {
            client: msg.client.clone(),
            merged_clients: if merge_clients {
                Some(msg.client.clone())
            } else {
                None
            },
            workspace_key: if matches!(group_by, GroupBy::WorkspaceModel) {
                workspace_key.clone()
            } else {
                None
            },
            workspace_label: if matches!(group_by, GroupBy::WorkspaceModel) {
                Some(workspace_label.clone())
            } else {
                None
            },
            session_id: if session_grouped {
                Some(msg.session_id.clone())
            } else {
                None
            },
            model: normalized.clone(),
            provider: msg.provider_id.clone(),
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
            message_count: 0,
            cost: 0.0,
            performance: ModelPerformance::default(),
        });

        if merge_clients {
            if !entry.client.split(", ").any(|s| s == msg.client) {
                entry.client = format!("{}, {}", entry.client, msg.client);
            }

            if let Some(merged_clients) = &mut entry.merged_clients {
                if !merged_clients.split(", ").any(|s| s == msg.client) {
                    *merged_clients = format!("{}, {}", merged_clients, msg.client);
                }
            }
        }

        if *group_by != GroupBy::ClientProviderModel
            && !entry.provider.split(", ").any(|p| p == msg.provider_id)
        {
            entry.provider = format!("{}, {}", entry.provider, msg.provider_id);
        }

        // saturating_add so clamped (i64::MAX) buckets from a corrupt source
        // can't overflow the fold (matches the grand-total sum below).
        entry.input = entry.input.saturating_add(msg.tokens.input);
        entry.output = entry.output.saturating_add(msg.tokens.output);
        entry.cache_read = entry.cache_read.saturating_add(msg.tokens.cache_read);
        entry.cache_write = entry.cache_write.saturating_add(msg.tokens.cache_write);
        entry.reasoning = entry.reasoning.saturating_add(msg.tokens.reasoning);
        entry.message_count += msg.message_count.max(0);
        entry.cost += msg.cost;
        entry
            .performance
            .record_message(positive_token_total(&msg.tokens), msg.duration_ms);
    }

    let mut entries: Vec<ModelUsage> = model_map
        .into_values()
        .map(|mut entry| {
            let total_tokens = entry
                .input
                .max(0)
                .saturating_add(entry.output.max(0))
                .saturating_add(entry.cache_read.max(0))
                .saturating_add(entry.cache_write.max(0))
                .saturating_add(entry.reasoning.max(0));
            entry.performance.finalize(total_tokens);
            let mut providers: Vec<&str> = entry.provider.split(", ").collect();
            providers.sort_unstable();
            providers.dedup();
            entry.provider = providers.join(", ");
            entry
        })
        .collect();
    entries.sort_by(|a, b| match (a.cost.is_nan(), b.cost.is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => b
            .cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal),
    });

    entries
}

fn positive_token_total(tokens: &TokenBreakdown) -> i64 {
    // saturating so multiple clamped (i64::MAX) buckets can't overflow the sum.
    tokens
        .input
        .max(0)
        .saturating_add(tokens.output.max(0))
        .saturating_add(tokens.cache_read.max(0))
        .saturating_add(tokens.cache_write.max(0))
        .saturating_add(tokens.reasoning.max(0))
}

/// Sum the (input, output, cache_read, cache_write) token fields across model
/// usage entries with saturating_add, so clamped (i64::MAX) entry buckets from a
/// corrupt source can't overflow the report-level totals (the entries are
/// already saturated per-field by aggregate_model_usage_entries).
fn model_report_token_totals(entries: &[ModelUsage]) -> (i64, i64, i64, i64) {
    entries.iter().fold(
        (0, 0, 0, 0),
        |(input, output, cache_read, cache_write), entry| {
            (
                input.saturating_add(entry.input),
                output.saturating_add(entry.output),
                cache_read.saturating_add(entry.cache_read),
                cache_write.saturating_add(entry.cache_write),
            )
        },
    )
}

pub async fn get_model_report(options: ReportOptions) -> Result<ModelReport, String> {
    let start = Instant::now();

    let home_dir = get_home_dir_string(&options.home_dir)?;

    let clients: Vec<String> = options.clients.clone().unwrap_or_else(|| {
        let mut clients: Vec<String> = ClientId::ALL
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        clients.push("synthetic".to_string());
        clients
    });

    let pricing = load_pricing_for_local_parse().await;
    let all_messages = parse_all_messages_with_pricing_with_env_strategy(
        &home_dir,
        &clients,
        pricing.as_deref(),
        options.use_env_roots,
        &options.scanner_settings,
        false,
    );

    let filtered = filter_messages_for_report(all_messages, &options);
    let entries = aggregate_model_usage_entries(filtered, &options.group_by);

    let (total_input, total_output, total_cache_read, total_cache_write) =
        model_report_token_totals(&entries);
    let total_messages: i32 = entries.iter().map(|e| e.message_count).sum();
    // f64's Sum identity is -0.0, so an empty report would serialize as
    // "totalCost": -0.0; adding +0.0 normalizes the sign without changing
    // any non-zero total.
    let total_cost: f64 = entries.iter().map(|e| e.cost).sum::<f64>() + 0.0;

    Ok(ModelReport {
        entries,
        total_input,
        total_output,
        total_cache_read,
        total_cache_write,
        total_messages,
        total_cost,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}

#[derive(Default)]
struct MonthAggregator {
    models: HashSet<String>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    message_count: i32,
    cost: f64,
}

pub async fn get_monthly_report(options: ReportOptions) -> Result<MonthlyReport, String> {
    let start = Instant::now();

    let home_dir = get_home_dir_string(&options.home_dir)?;

    let clients: Vec<String> = options.clients.clone().unwrap_or_else(|| {
        let mut clients: Vec<String> = ClientId::ALL
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        clients.push("synthetic".to_string());
        clients
    });

    let pricing = load_pricing_for_local_parse().await;
    let all_messages = parse_all_messages_with_pricing_with_env_strategy(
        &home_dir,
        &clients,
        pricing.as_deref(),
        options.use_env_roots,
        &options.scanner_settings,
        false,
    );

    let filtered = filter_messages_for_report(all_messages, &options);

    let mut month_map: HashMap<String, MonthAggregator> = HashMap::new();

    for msg in filtered {
        let month = if msg.date.len() >= 7 {
            msg.date[..7].to_string()
        } else {
            continue;
        };

        let entry = month_map.entry(month).or_default();

        entry.models.insert(model_name_for_grouping(
            &msg.client,
            &msg.provider_id,
            &msg.model_id,
        ));
        // saturating_add so clamped (i64::MAX) buckets from a corrupt source
        // can't overflow the fold.
        entry.input = entry.input.saturating_add(msg.tokens.input);
        entry.output = entry.output.saturating_add(msg.tokens.output);
        entry.cache_read = entry.cache_read.saturating_add(msg.tokens.cache_read);
        entry.cache_write = entry.cache_write.saturating_add(msg.tokens.cache_write);
        entry.message_count += msg.message_count.max(0);
        entry.cost += msg.cost;
    }

    let mut entries: Vec<MonthlyUsage> = month_map
        .into_iter()
        .map(|(month, agg)| MonthlyUsage {
            month,
            models: agg.models.into_iter().collect(),
            input: agg.input,
            output: agg.output,
            cache_read: agg.cache_read,
            cache_write: agg.cache_write,
            message_count: agg.message_count,
            cost: agg.cost,
        })
        .collect();

    entries.sort_by(|a, b| a.month.cmp(&b.month));

    // f64's Sum identity is -0.0, so an empty report would serialize as
    // "totalCost": -0.0; adding +0.0 normalizes the sign without changing
    // any non-zero total.
    let total_cost: f64 = entries.iter().map(|e| e.cost).sum::<f64>() + 0.0;

    Ok(MonthlyReport {
        entries,
        total_cost,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}

#[derive(Default)]
struct HourAggregator {
    clients: HashSet<String>,
    models: HashSet<String>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    message_count: i32,
    turn_count: i32,
    cost: f64,
}

/// Generate hourly usage report, keyed by "YYYY-MM-DD HH:00".
///
/// Derives the hour slot from `UnifiedMessage.timestamp` (Unix ms).
/// Falls back to date + "00:00" when timestamp is zero or missing.
pub async fn get_hourly_report(options: ReportOptions) -> Result<HourlyReport, String> {
    let start = Instant::now();

    let home_dir = get_home_dir_string(&options.home_dir)?;

    let clients: Vec<String> = options.clients.clone().unwrap_or_else(|| {
        let mut clients: Vec<String> = ClientId::ALL
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        clients.push("synthetic".to_string());
        clients
    });

    let pricing = load_pricing_for_local_parse().await;
    let all_messages = parse_all_messages_with_pricing_with_env_strategy(
        &home_dir,
        &clients,
        pricing.as_deref(),
        options.use_env_roots,
        &options.scanner_settings,
        false,
    );

    let filtered = filter_messages_for_report(all_messages, &options);

    let mut hour_map: HashMap<String, HourAggregator> = HashMap::new();

    for msg in filtered {
        let hour_key = if msg.timestamp > 0 {
            crate::bucket_tz::bucket_timezone()
                .date_hour_of_ms(msg.timestamp)
                .unwrap_or_else(|| format!("{} 00:00", msg.date))
        } else {
            format!("{} 00:00", msg.date)
        };

        let entry = hour_map.entry(hour_key).or_default();

        entry.clients.insert(msg.client.clone());
        entry.models.insert(model_name_for_grouping(
            &msg.client,
            &msg.provider_id,
            &msg.model_id,
        ));
        // saturating_add so clamped (i64::MAX) buckets from a corrupt source
        // can't overflow the fold.
        entry.input = entry.input.saturating_add(msg.tokens.input);
        entry.output = entry.output.saturating_add(msg.tokens.output);
        entry.cache_read = entry.cache_read.saturating_add(msg.tokens.cache_read);
        entry.cache_write = entry.cache_write.saturating_add(msg.tokens.cache_write);
        entry.reasoning = entry.reasoning.saturating_add(msg.tokens.reasoning);
        entry.message_count += msg.message_count.max(0);
        if msg.is_turn_start {
            entry.turn_count += 1;
        }
        entry.cost += msg.cost;
    }

    let mut entries: Vec<HourlyUsage> = hour_map
        .into_iter()
        .map(|(hour, agg)| HourlyUsage {
            hour,
            clients: {
                let mut v: Vec<String> = agg.clients.into_iter().collect();
                v.sort();
                v
            },
            models: {
                let mut v: Vec<String> = agg.models.into_iter().collect();
                v.sort();
                v
            },
            input: agg.input,
            output: agg.output,
            cache_read: agg.cache_read,
            cache_write: agg.cache_write,
            message_count: agg.message_count,
            turn_count: agg.turn_count,
            reasoning: agg.reasoning,
            cost: agg.cost,
        })
        .collect();

    entries.sort_by(|a, b| a.hour.cmp(&b.hour));

    // f64's Sum identity is -0.0, so an empty report would serialize as
    // "totalCost": -0.0; adding +0.0 normalizes the sign without changing
    // any non-zero total.
    let total_cost: f64 = entries.iter().map(|e| e.cost).sum::<f64>() + 0.0;

    Ok(HourlyReport {
        entries,
        total_cost,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}

async fn generate_graph_with_loaded_pricing(
    options: ReportOptions,
    pricing: Option<&pricing::PricingService>,
) -> Result<GraphResult, String> {
    let start = Instant::now();

    let home_dir = get_home_dir_string(&options.home_dir)?;

    let clients: Vec<String> = options.clients.clone().unwrap_or_else(|| {
        let mut clients: Vec<String> = ClientId::ALL
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        clients.push("synthetic".to_string());
        clients
    });

    let all_messages = parse_all_messages_with_pricing_with_env_strategy(
        &home_dir,
        &clients,
        pricing,
        options.use_env_roots,
        &options.scanner_settings,
        options.today_only,
    );

    let filtered = filter_messages_for_report(all_messages, &options);

    let intervals = sessionize::sessionize(&filtered, sessionize::DEFAULT_IDLE_GAP_MS);
    let time_metrics =
        sessionize::compute_time_metrics(&intervals, sessionize::DEFAULT_IDLE_GAP_MS);

    let daily_active_time = sessionize::compute_daily_active_time(&intervals);
    let contributions = aggregator::aggregate_by_date(filtered);

    let processing_time_ms = start.elapsed().as_millis() as u32;
    let mut result = aggregator::generate_graph_result(contributions, processing_time_ms);
    result.time_metrics = Some(time_metrics);

    for contribution in &mut result.contributions {
        if let Some(&ms) = daily_active_time.get(&contribution.date) {
            contribution.active_time_ms = Some(ms);
        }
    }

    Ok(result)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TimeMetricsReport {
    pub metrics: sessionize::TimeMetrics,
    pub processing_time_ms: u32,
}

pub async fn get_time_metrics_report(options: ReportOptions) -> Result<TimeMetricsReport, String> {
    let start = Instant::now();

    let home_dir = get_home_dir_string(&options.home_dir)?;

    let clients: Vec<String> = options.clients.clone().unwrap_or_else(|| {
        let mut clients: Vec<String> = ClientId::ALL
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        clients.push("synthetic".to_string());
        clients
    });

    let all_messages = parse_all_messages_with_pricing_with_env_strategy(
        &home_dir,
        &clients,
        None,
        options.use_env_roots,
        &options.scanner_settings,
        false,
    );

    let filtered = filter_messages_for_report(all_messages, &options);

    let intervals = sessionize::sessionize(&filtered, sessionize::DEFAULT_IDLE_GAP_MS);
    let metrics = sessionize::compute_time_metrics(&intervals, sessionize::DEFAULT_IDLE_GAP_MS);

    Ok(TimeMetricsReport {
        metrics,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}

pub async fn generate_graph(options: ReportOptions) -> Result<GraphResult, String> {
    let pricing = pricing::PricingService::get_or_init().await?;
    generate_graph_with_loaded_pricing(options, Some(&pricing)).await
}

pub async fn generate_local_graph_report(options: ReportOptions) -> Result<GraphResult, String> {
    let pricing = load_pricing_for_local_parse().await;
    generate_graph_with_loaded_pricing(options, pricing.as_deref()).await
}

fn filter_messages_for_report(
    messages: Vec<UnifiedMessage>,
    options: &ReportOptions,
) -> Vec<UnifiedMessage> {
    let mut filtered = messages;

    if let Some(year) = &options.year {
        let year_prefix = format!("{}-", year);
        filtered.retain(|m| m.date.starts_with(&year_prefix));
    }

    if let Some(since) = &options.since {
        filtered.retain(|m| m.date.as_str() >= since.as_str());
    }

    if let Some(until) = &options.until {
        filtered.retain(|m| m.date.as_str() <= until.as_str());
    }

    if options.today_only {
        // File-level pruning leaves today's files, but a file touched today can
        // still hold yesterday's tail (a session crossing midnight). Pin to today
        // so a today-only report is exactly today.
        let today = crate::bucket_tz::bucket_timezone()
            .today()
            .format("%Y-%m-%d")
            .to_string();
        filtered.retain(|m| m.date == today);
    }

    filtered
}

fn is_headless_path(path: &Path, headless_roots: &[PathBuf]) -> bool {
    headless_roots.iter().any(|root| path.starts_with(root))
}

fn apply_headless_agent(message: &mut UnifiedMessage, is_headless: bool) {
    if is_headless && message.agent.is_none() {
        message.agent = Some("headless".to_string());
    }
}

fn pricing_multiplier(message: &UnifiedMessage) -> f64 {
    // Zed bills hosted models at provider list price + 10%.
    // Source: https://zed.dev/docs/ai/plans-and-usage and https://zed.dev/docs/ai/models
    //
    // The multiplier is keyed on the message's `provider_id`, not on the
    // provenance of the matched LiteLLM pricing row. Today this is safe because
    // tokens's bundled LiteLLM dataset only carries upstream-provider rows
    // (anthropic, openai, google) for the underlying models. If a future
    // LiteLLM update adds rows under provider `zed.dev` that already include
    // Zed's markup, this function would double-bill — revisit by threading
    // the matched-price provenance through `apply_pricing_if_available`.
    if message.client == "zed"
        && message
            .provider_id
            .eq_ignore_ascii_case(sessions::zed::ZED_HOSTED_PROVIDER)
    {
        1.1
    } else {
        1.0
    }
}

fn apply_pricing_if_available(
    message: &mut UnifiedMessage,
    pricing: Option<&pricing::PricingService>,
) {
    if message.has_authoritative_cost() {
        return;
    }

    let Some(pricing) = pricing else {
        return;
    };

    let calculated_cost = pricing.calculate_cost_with_provider(
        &message.model_id,
        Some(&message.provider_id),
        &message.tokens,
    ) * pricing_multiplier(message);

    if calculated_cost > 0.0 {
        message.cost = calculated_cost;
        message.mark_estimated_cost();
    }
}

fn parse_hermes_sqlite_with_pricing(
    db_path: &Path,
    pricing: Option<&pricing::PricingService>,
) -> Vec<UnifiedMessage> {
    sessions::hermes::parse_hermes_sqlite(db_path)
        .into_iter()
        .map(|mut msg| {
            if msg.cost <= 0.0 {
                apply_pricing_if_available(&mut msg, pricing);
            }
            msg
        })
        .collect()
}

fn select_local_parse_pricing<F>(
    fresh: Result<Arc<pricing::PricingService>, String>,
    stale: F,
) -> Option<Arc<pricing::PricingService>>
where
    F: FnOnce() -> Option<pricing::PricingService>,
{
    fresh.ok().or_else(|| stale().map(Arc::new))
}

async fn load_pricing_for_local_parse() -> Option<Arc<pricing::PricingService>> {
    if std::env::var("TOKENS_PRICING_CACHE_ONLY")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
    {
        return pricing::PricingService::load_cached_any_age().map(Arc::new);
    }

    // Interactive/local views should pick up newly released model pricing as soon
    // as a fresh fetch succeeds, but still remain usable offline by falling back
    // to any cached dataset when the network path fails.
    select_local_parse_pricing(
        pricing::PricingService::get_or_init().await,
        pricing::PricingService::load_cached_any_age,
    )
}

fn resolve_local_parse_request(
    options: &LocalParseOptions,
) -> Result<(String, Vec<String>), String> {
    let home_dir = get_home_dir_string(&options.home_dir)?;
    let clients = options.clients.clone().unwrap_or_else(|| {
        let mut clients: Vec<String> = ClientId::iter()
            .filter(|c| c.parse_local())
            .map(|c| c.as_str().to_string())
            .collect();
        clients.push("synthetic".to_string());
        clients
    });
    Ok((home_dir, clients))
}

fn parse_local_unified_messages_resolved(
    options: LocalParseOptions,
    home_dir: &str,
    clients: &[String],
    pricing: Option<&pricing::PricingService>,
) -> Result<Vec<UnifiedMessage>, String> {
    let messages = parse_all_messages_with_pricing_with_env_strategy(
        home_dir,
        clients,
        pricing,
        options.use_env_roots,
        &options.scanner_settings,
        false,
    );
    Ok(filter_unified_messages(messages, &options))
}
pub fn parse_local_clients(options: LocalParseOptions) -> Result<ParsedMessages, String> {
    let start = Instant::now();

    let home_dir = get_home_dir_string(&options.home_dir)?;

    let clients: Vec<String> = options.clients.clone().unwrap_or_else(|| {
        let mut clients: Vec<String> = ClientId::iter()
            .filter(|c| c.parse_local())
            .map(|c| c.as_str().to_string())
            .collect();
        clients.push("synthetic".to_string());
        clients
    });
    let include_all = clients.is_empty();
    let include_synthetic = include_all || clients.iter().any(|c| c == "synthetic");
    let include_devin_cli = include_synthetic || clients.iter().any(|c| c == "devin-cli");
    let include_devin_desktop = include_synthetic || clients.iter().any(|c| c == "devin-desktop");

    let scan_result = scanner::scan_all_clients_with_scanner_settings(
        &home_dir,
        &clients,
        options.use_env_roots,
        &options.scanner_settings,
    );
    let headless_roots =
        scanner::headless_roots_with_env_strategy(&home_dir, options.use_env_roots);

    let mut messages: Vec<ParsedMessage> = Vec::new();

    // Parse OpenCode: prefer SQLite, collapse forked SQLite history there, then
    // suppress legacy JSON overlap by message identity.
    let mut counts = ClientCounts::new();

    let opencode_count: i32 = {
        let mut seen: HashSet<String> = HashSet::new();
        let mut count: i32 = 0;

        for db_path in &scan_result.opencode_dbs {
            let sqlite_msgs: Vec<(String, ParsedMessage)> =
                sessions::opencode::parse_opencode_sqlite(db_path)
                    .into_iter()
                    .filter_map(|msg| {
                        let key = msg.dedup_key.clone().unwrap_or_default();
                        // Dedup across multiple channel-suffixed dbs: the
                        // same session can end up in both `opencode.db` and
                        // `opencode-<channel>.db` if the user switches
                        // channels mid-session.
                        if !key.is_empty() && !seen.insert(key.clone()) {
                            return None;
                        }
                        Some((key, unified_to_parsed(&msg)))
                    })
                    .collect();
            count += sqlite_msgs.len() as i32;
            for (_key, parsed) in sqlite_msgs {
                messages.push(parsed);
            }
        }

        let json_msgs: Vec<(String, ParsedMessage)> = scan_result
            .get(ClientId::OpenCode)
            .par_iter()
            .filter_map(|path| {
                let msg = sessions::opencode::parse_opencode_file(path)?;
                let key = msg.dedup_key.clone().unwrap_or_default();
                Some((key, unified_to_parsed(&msg)))
            })
            .collect();
        let deduped: Vec<ParsedMessage> = json_msgs
            .into_iter()
            .filter(|(key, _)| key.is_empty() || seen.insert(key.clone()))
            .map(|(_, msg)| msg)
            .collect();
        count += deduped.len() as i32;
        messages.extend(deduped);

        count
    };
    counts.set(ClientId::OpenCode, opencode_count);

    let claude_home = PathBuf::from(&home_dir);
    let claude_msgs_raw: Vec<(String, ParsedMessage)> = scan_result
        .get(ClientId::Claude)
        .par_iter()
        .map_init(std::collections::HashMap::new, |parent_cache, path| {
            sessions::claudecode::parse_claude_file_with_cache_and_home(
                path,
                parent_cache,
                Some(&claude_home),
            )
            .into_iter()
            .map(|msg| {
                let dedup_key = msg.dedup_key.clone().unwrap_or_default();
                (dedup_key, unified_to_parsed(&msg))
            })
            .collect::<Vec<_>>()
        })
        .flatten()
        .collect();

    let mut seen_keys: HashSet<String> = HashSet::new();
    let claude_msgs: Vec<ParsedMessage> = claude_msgs_raw
        .into_iter()
        .filter(|(key, _)| key.is_empty() || seen_keys.insert(key.clone()))
        .map(|(_, msg)| msg)
        .collect();
    let claude_count = claude_msgs.len() as i32;
    counts.set(ClientId::Claude, claude_count);
    messages.extend(claude_msgs);

    let codex_msgs_raw: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Codex)
        .par_iter()
        .flat_map(|path| {
            let is_headless = is_headless_path(path, &headless_roots);
            sessions::codex::parse_codex_file(path)
                .into_iter()
                .map(|mut msg| {
                    apply_headless_agent(&mut msg, is_headless);
                    msg
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let mut codex_seen: HashSet<String> = HashSet::new();
    let codex_msgs: Vec<ParsedMessage> = codex_msgs_raw
        .into_iter()
        .filter(|message| should_keep_deduped_message(&mut codex_seen, message))
        .map(|message| unified_to_parsed(&message))
        .collect();
    let codex_count = codex_msgs.len() as i32;
    counts.set(ClientId::Codex, codex_count);
    messages.extend(codex_msgs);

    let mut copilot_unified_msgs: Vec<_> = scan_result
        .get(ClientId::Copilot)
        .par_iter()
        .flat_map(|path| {
            sessions::copilot::parse_copilot_file(path)
                .into_iter()
                .collect::<Vec<_>>()
        })
        .collect();
    if let Some(db_path) = &scan_result.copilot_desktop_db {
        let otel_sessions: HashSet<String> = copilot_unified_msgs
            .iter()
            .map(|message| message.session_id.clone())
            .collect();
        copilot_unified_msgs.extend(
            sessions::copilot_desktop::parse_copilot_desktop_db(db_path)
                .into_iter()
                .filter(|message| !otel_sessions.contains(&message.session_id)),
        );
    }
    {
        let existing_dedup_keys: HashSet<String> = copilot_unified_msgs
            .iter()
            .filter_map(|m| m.dedup_key.clone())
            .collect();
        let existing_copilot_session_timestamps: HashSet<(String, i64)> = copilot_unified_msgs
            .iter()
            .map(|m| (m.session_id.clone(), m.timestamp))
            .collect();
        copilot_unified_msgs.extend(
            sessions::copilot_vscode::parse_copilot_vscode_sessions(
                &scan_result.copilot_vscode_sessions,
            )
            .into_iter()
            .filter(|m| {
                let key_unique = m
                    .dedup_key
                    .as_deref()
                    .map(|k| !existing_dedup_keys.contains(k))
                    .unwrap_or(true);
                let session_ts_unique = !existing_copilot_session_timestamps
                    .contains(&(m.session_id.clone(), m.timestamp));
                key_unique && session_ts_unique
            }),
        );
    }
    let copilot_msgs: Vec<ParsedMessage> =
        copilot_unified_msgs.iter().map(unified_to_parsed).collect();
    let copilot_count = copilot_msgs.len() as i32;
    counts.set(ClientId::Copilot, copilot_count);
    messages.extend(copilot_msgs);

    let gemini_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Gemini)
        .par_iter()
        .flat_map(|path| {
            sessions::gemini::parse_gemini_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let gemini_count = gemini_msgs.len() as i32;
    counts.set(ClientId::Gemini, gemini_count);
    messages.extend(gemini_msgs);

    let amp_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Amp)
        .par_iter()
        .flat_map(|path| {
            sessions::amp::parse_amp_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let amp_count = amp_msgs.len() as i32;
    counts.set(ClientId::Amp, amp_count);
    messages.extend(amp_msgs);

    let codebuff_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Codebuff)
        .par_iter()
        .flat_map(|path| {
            sessions::codebuff::parse_codebuff_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let codebuff_count = codebuff_msgs.len() as i32;
    counts.set(ClientId::Codebuff, codebuff_count);
    messages.extend(codebuff_msgs);

    let droid_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Droid)
        .par_iter()
        .flat_map(|path| {
            sessions::droid::parse_droid_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let droid_count = droid_msgs.len() as i32;
    counts.set(ClientId::Droid, droid_count);
    messages.extend(droid_msgs);

    let openclaw_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::OpenClaw)
        .par_iter()
        .flat_map(|path| {
            sessions::openclaw::parse_openclaw_transcript(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let openclaw_count = openclaw_msgs.len() as i32;
    counts.set(ClientId::OpenClaw, openclaw_count);
    messages.extend(openclaw_msgs);

    let pi_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Pi)
        .par_iter()
        .flat_map(|path| {
            sessions::pi::parse_pi_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let pi_count = pi_msgs.len() as i32;
    counts.set(ClientId::Pi, pi_count);
    messages.extend(pi_msgs);

    let commandcode_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::CommandCode)
        .par_iter()
        .flat_map(|path| {
            sessions::commandcode::parse_commandcode_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let commandcode_count = commandcode_msgs.len() as i32;
    counts.set(ClientId::CommandCode, commandcode_count);
    messages.extend(commandcode_msgs);

    // gjc (gajae-code) JSONL sessions. This non-cached path produces
    // ParsedMessage (no cost field) and has no pricing service in scope, so
    // the A1 cost guard is a no-op here — cost correctness is enforced on the
    // cached pricing path (see the gjc_outcomes block). What matters here is
    // message-level dedup (codebuff-style key via should_keep_deduped_message)
    // to collapse depth-1/depth-2 replays, mirroring the codex cluster.
    let gjc_msgs_raw: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Gjc)
        .par_iter()
        .flat_map(|path| sessions::gjc::parse_gjc_file(path))
        .collect();
    let mut gjc_seen: HashSet<String> = HashSet::new();
    let gjc_msgs: Vec<ParsedMessage> = gjc_msgs_raw
        .into_iter()
        .filter(|message| should_keep_deduped_message(&mut gjc_seen, message))
        .map(|message| unified_to_parsed(&message))
        .collect();
    let gjc_count = gjc_msgs.len() as i32;
    counts.set(ClientId::Gjc, gjc_count);
    messages.extend(gjc_msgs);

    // ParsedMessage has no pricing service in scope, but Junie parser already
    // preserves the embedded session costs for callers that need UnifiedMessage.
    // Dedup still matters here because Junie can replay metadata events.
    let junie_msgs_raw: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Junie)
        .par_iter()
        .flat_map(|path| sessions::junie::parse_junie_file(path))
        .collect();
    let mut junie_seen: HashSet<String> = HashSet::new();
    let junie_msgs: Vec<ParsedMessage> = junie_msgs_raw
        .into_iter()
        .filter(|message| should_keep_deduped_message(&mut junie_seen, message))
        .map(|message| unified_to_parsed(&message))
        .collect();
    let junie_count = summed_parsed_message_count(&junie_msgs);
    counts.set(ClientId::Junie, junie_count);
    messages.extend(junie_msgs);

    // ZCode v2 CLI SQLite usage plus legacy JSONL session transcripts.
    let mut zcode_msgs: Vec<ParsedMessage> = scan_result
        .zcode_db
        .as_ref()
        .map(|db_path| {
            sessions::zcode::parse_zcode_sqlite(db_path)
                .into_iter()
                .map(|message| unified_to_parsed(&message))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    zcode_msgs.extend(
        scan_result
            .get(ClientId::Zcode)
            .par_iter()
            .flat_map(|path| sessions::zcode::parse_zcode_file(path))
            .map(|message| unified_to_parsed(&message))
            .collect::<Vec<_>>(),
    );
    let zcode_count = summed_parsed_message_count(&zcode_msgs);
    counts.set(ClientId::Zcode, zcode_count);
    messages.extend(zcode_msgs);

    let opencodereview_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::OpenCodeReview)
        .par_iter()
        .flat_map(|path| {
            sessions::opencodereview::parse_opencodereview_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let opencodereview_count = summed_parsed_message_count(&opencodereview_msgs);
    counts.set(ClientId::OpenCodeReview, opencodereview_count);
    messages.extend(opencodereview_msgs);

    // Parse Kimi wire.jsonl files in parallel
    let kimi_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Kimi)
        .par_iter()
        .flat_map(|path| {
            let msgs = if sessions::kimi::is_kimi_code_path(path) {
                sessions::kimi::parse_kimi_code_file(path)
            } else {
                sessions::kimi::parse_kimi_file(path)
            };
            msgs.into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let kimi_count = kimi_msgs.len() as i32;
    counts.set(ClientId::Kimi, kimi_count);
    messages.extend(kimi_msgs);

    // Parse Qwen JSONL files in parallel
    let qwen_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Qwen)
        .par_iter()
        .flat_map(|path| {
            sessions::qwen::parse_qwen_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let qwen_count = qwen_msgs.len() as i32;
    counts.set(ClientId::Qwen, qwen_count);
    messages.extend(qwen_msgs);

    let roocode_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::RooCode)
        .par_iter()
        .flat_map(|path| {
            sessions::roocode::parse_roocode_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let roocode_count = roocode_msgs.len() as i32;
    counts.set(ClientId::RooCode, roocode_count);
    messages.extend(roocode_msgs);

    let kilocode_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::KiloCode)
        .par_iter()
        .flat_map(|path| {
            sessions::kilocode::parse_kilocode_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let kilocode_count = summed_parsed_message_count(&kilocode_msgs);
    counts.set(ClientId::KiloCode, kilocode_count);
    messages.extend(kilocode_msgs);

    let cline_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Cline)
        .par_iter()
        .flat_map(|path| {
            sessions::cline::parse_cline_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let cline_count = summed_parsed_message_count(&cline_msgs);
    counts.set(ClientId::Cline, cline_count);
    messages.extend(cline_msgs);

    let mux_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Mux)
        .par_iter()
        .flat_map(|path| {
            sessions::mux::parse_mux_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let mux_count = summed_parsed_message_count(&mux_msgs);
    counts.set(ClientId::Mux, mux_count);
    messages.extend(mux_msgs);

    // Kilo CLI: SQLite database
    let _kilo_count: i32 = if let Some(db_path) = &scan_result.kilo_db {
        let kilo_msgs: Vec<ParsedMessage> = sessions::kilo::parse_kilo_sqlite(db_path)
            .into_iter()
            .map(|msg| unified_to_parsed(&msg))
            .collect();
        let count = summed_parsed_message_count(&kilo_msgs);
        counts.set(ClientId::Kilo, count);
        messages.extend(kilo_msgs);
        count
    } else {
        0
    };

    let hermes_db_paths = scan_result.hermes_db_paths();
    if !hermes_db_paths.is_empty() {
        let mut hermes_seen: HashSet<String> = HashSet::new();
        let hermes_msgs: Vec<ParsedMessage> = hermes_db_paths
            .iter()
            .flat_map(|db_path| sessions::hermes::parse_hermes_sqlite(db_path))
            .filter(|msg| should_keep_deduped_message(&mut hermes_seen, msg))
            .map(|msg| unified_to_parsed(&msg))
            .collect();
        let count = summed_parsed_message_count(&hermes_msgs);
        counts.set(ClientId::Hermes, count);
        messages.extend(hermes_msgs);
    }

    if let Some(db_path) = &scan_result.goose_db {
        let goose_msgs: Vec<ParsedMessage> = sessions::goose::parse_goose_sqlite(db_path)
            .into_iter()
            .map(|msg| unified_to_parsed(&msg))
            .collect();
        let count = summed_parsed_message_count(&goose_msgs);
        counts.set(ClientId::Goose, count);
        messages.extend(goose_msgs);
    }

    let zed_db_paths = scan_result.zed_db_paths();
    if !zed_db_paths.is_empty() {
        let zed_msgs: Vec<ParsedMessage> = zed_db_paths
            .iter()
            .flat_map(|db_path| sessions::zed::parse_zed_sqlite(db_path))
            .map(|msg| unified_to_parsed(&msg))
            .collect();
        let count = summed_parsed_message_count(&zed_msgs);
        counts.set(ClientId::Zed, count);
        messages.extend(zed_msgs);
    }

    let kiro_unified: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Kiro)
        .par_iter()
        .flat_map(|path| sessions::kiro::parse_kiro_file(path))
        .collect();
    let kiro_msgs: Vec<ParsedMessage> =
        sessions::kiro::suppress_snapshots_covered_by_executions(kiro_unified)
            .iter()
            .map(unified_to_parsed)
            .collect();
    let kiro_count = summed_parsed_message_count(&kiro_msgs);
    counts.set(ClientId::Kiro, kiro_count);
    messages.extend(kiro_msgs);

    if let Some(db_path) = &scan_result.kiro_db {
        let kiro_db_msgs: Vec<ParsedMessage> = sessions::kiro::parse_kiro_sqlite(db_path)
            .into_iter()
            .map(|msg| unified_to_parsed(&msg))
            .collect();
        let kiro_db_count = summed_parsed_message_count(&kiro_db_msgs);
        counts.add(ClientId::Kiro, kiro_db_count);
        messages.extend(kiro_db_msgs);
    }

    let crush_msgs: Vec<ParsedMessage> = scan_result
        .crush_dbs
        .par_iter()
        .flat_map(|source| {
            sessions::crush::parse_crush_sqlite(&source.db_path)
                .into_iter()
                .map(|mut msg| {
                    msg.set_workspace(source.workspace_key.clone(), source.workspace_label.clone());
                    unified_to_parsed(&msg)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let crush_count = summed_parsed_message_count(&crush_msgs);
    counts.set(ClientId::Crush, crush_count);
    messages.extend(crush_msgs);

    let antigravity_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Antigravity)
        .par_iter()
        .flat_map(|path| {
            sessions::antigravity::parse_antigravity_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let antigravity_count = antigravity_msgs.len() as i32;
    counts.set(ClientId::Antigravity, antigravity_count);
    messages.extend(antigravity_msgs);

    let antigravity_cli_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::AntigravityCli)
        .par_iter()
        .flat_map(|path| {
            sessions::antigravity_cli::parse_antigravity_cli_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let antigravity_cli_count = antigravity_cli_msgs.len() as i32;
    counts.set(ClientId::AntigravityCli, antigravity_cli_count);
    messages.extend(antigravity_cli_msgs);

    let trae_msgs: Vec<ParsedMessage> = {
        let unique_trae_messages = dedupe_latest_trae_messages(
            scan_result
                .get(ClientId::Trae)
                .par_iter()
                .flat_map(|path| sessions::trae::parse_trae_file("trae", path))
                .collect(),
        );
        unique_trae_messages
            .into_iter()
            .map(|msg| unified_to_parsed(&msg))
            .collect()
    };
    let trae_count = trae_msgs.len() as i32;
    counts.set(ClientId::Trae, trae_count);
    messages.extend(trae_msgs);

    let warp_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Warp)
        .par_iter()
        .flat_map(|path| {
            sessions::warp::parse_warp_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let warp_count = summed_parsed_message_count(&warp_msgs);
    counts.set(ClientId::Warp, warp_count);
    messages.extend(warp_msgs);

    // Devin CLI SQLite usage plus Desktop NDJSON event streams. The CLI
    // database is authoritative only when the CLI client itself is selected;
    // Desktop-only reports still use the database for title/model metadata but
    // must not leak CLI usage into their result.
    let mut devin_cli_seen = HashSet::new();
    let devin_cli_messages: Vec<UnifiedMessage> = if include_devin_cli {
        scan_result
            .devin_dbs
            .iter()
            .flat_map(|db_path| sessions::devin::parse_devin_cli_sqlite(db_path))
            .filter(|message| should_keep_deduped_message(&mut devin_cli_seen, message))
            .collect()
    } else {
        Vec::new()
    };
    let cli_session_ids: HashSet<String> = devin_cli_messages
        .iter()
        .map(|message| message.session_id.clone())
        .collect();
    let devin_desktop_messages_raw: Vec<UnifiedMessage> = if include_devin_desktop {
        let devin_desktop_lookup =
            sessions::devin::load_devin_desktop_session_lookup(&scan_result.devin_dbs);
        scan_result
            .get(ClientId::DevinDesktop)
            .par_iter()
            .flat_map(|path| {
                sessions::devin::parse_devin_desktop_ndjson_with_lookup(path, &devin_desktop_lookup)
            })
            .collect()
    } else {
        Vec::new()
    };
    // Count before dedup so the `clients` command reflects how many Desktop
    // sessions were actually found, even when they overlap with the CLI DB.
    let devin_desktop_raw_count: i32 = devin_desktop_messages_raw
        .iter()
        .map(|msg| msg.message_count.max(0))
        .sum();
    let devin_desktop_messages: Vec<UnifiedMessage> = devin_desktop_messages_raw
        .into_iter()
        .filter(|message| !cli_session_ids.contains(&message.session_id))
        .collect();

    let devin_cli_parsed: Vec<ParsedMessage> = devin_cli_messages
        .into_iter()
        .map(|msg| unified_to_parsed(&msg))
        .collect();
    let devin_desktop_parsed: Vec<ParsedMessage> = devin_desktop_messages
        .into_iter()
        .map(|msg| unified_to_parsed(&msg))
        .collect();
    let devin_cli_count = summed_parsed_message_count(&devin_cli_parsed);
    // Use the pre-dedup count for the `clients` command display so users see
    // all discovered Desktop sessions. The dedup-filtered messages are still
    // what gets added to the combined `messages` vector.
    let devin_desktop_count = devin_desktop_raw_count;
    counts.set(ClientId::DevinCli, devin_cli_count);
    counts.set(ClientId::DevinDesktop, devin_desktop_count);
    messages.extend(devin_cli_parsed);
    messages.extend(devin_desktop_parsed);

    let codebuddy_msgs_raw: Vec<UnifiedMessage> = scan_result
        .get(ClientId::CodeBuddy)
        .par_iter()
        .flat_map(|path| sessions::codebuddy::parse_codebuddy_file(path))
        .collect();
    let mut codebuddy_seen: HashSet<String> = HashSet::new();
    let codebuddy_msgs: Vec<ParsedMessage> = codebuddy_msgs_raw
        .into_iter()
        .filter(|message| {
            message
                .dedup_key
                .as_ref()
                .is_none_or(|key| codebuddy_seen.insert(key.clone()))
        })
        .map(|msg| unified_to_parsed(&msg))
        .collect();
    let codebuddy_count = summed_parsed_message_count(&codebuddy_msgs);
    counts.set(ClientId::CodeBuddy, codebuddy_count);
    messages.extend(codebuddy_msgs);
    let (workbuddy_detailed_paths, workbuddy_fallback_paths) =
        partition_workbuddy_paths(scan_result.get(ClientId::WorkBuddy));
    let workbuddy_detailed_messages: Vec<UnifiedMessage> = workbuddy_detailed_paths
        .par_iter()
        .flat_map(|path| sessions::workbuddy::parse_workbuddy_file(path))
        .collect();
    let workbuddy_fallback_messages: Vec<UnifiedMessage> = workbuddy_fallback_paths
        .par_iter()
        .flat_map(|path| sessions::workbuddy::parse_workbuddy_file(path))
        .collect();
    let workbuddy_msgs: Vec<ParsedMessage> =
        merge_workbuddy_messages(workbuddy_detailed_messages, workbuddy_fallback_messages)
            .into_iter()
            .map(|msg| unified_to_parsed(&msg))
            .collect();
    let workbuddy_count = summed_parsed_message_count(&workbuddy_msgs);
    counts.set(ClientId::WorkBuddy, workbuddy_count);
    messages.extend(workbuddy_msgs);

    let grok_msgs: Vec<ParsedMessage> = scan_result
        .get(ClientId::Grok)
        .par_iter()
        .flat_map(|path| {
            sessions::grok::parse_grok_updates_file(path)
                .into_iter()
                .map(|msg| unified_to_parsed(&msg))
                .collect::<Vec<_>>()
        })
        .collect();
    let grok_count = summed_parsed_message_count(&grok_msgs);
    counts.set(ClientId::Grok, grok_count);
    messages.extend(grok_msgs);

    let jcode_msgs_raw: Vec<UnifiedMessage> = scan_result
        .get(ClientId::Jcode)
        .par_iter()
        .flat_map(|path| sessions::jcode::parse_jcode_file(path))
        .collect();
    let mut jcode_seen: HashSet<String> = HashSet::new();
    let jcode_msgs: Vec<ParsedMessage> = jcode_msgs_raw
        .into_iter()
        .filter(|message| should_keep_deduped_message(&mut jcode_seen, message))
        .map(|msg| unified_to_parsed(&msg))
        .collect();
    let jcode_count = summed_parsed_message_count(&jcode_msgs);
    counts.set(ClientId::Jcode, jcode_count);
    messages.extend(jcode_msgs);

    if include_synthetic {
        if let Some(db_path) = &scan_result.synthetic_db {
            let synthetic_msgs: Vec<ParsedMessage> =
                sessions::synthetic::parse_octofriend_sqlite(db_path)
                    .into_iter()
                    .map(|msg| unified_to_parsed(&msg))
                    .collect();
            messages.extend(synthetic_msgs);
        }
    }

    // Filter BEFORE normalization (see parse_all_messages_with_pricing).
    if !include_all {
        let requested: HashSet<&str> = clients.iter().map(String::as_str).collect();
        messages.retain(|msg| {
            retain_for_requested_clients(&msg.client, &msg.model_id, &msg.provider_id, &requested)
        });
    }

    if include_synthetic {
        for msg in &mut messages {
            sessions::synthetic::normalize_synthetic_gateway_fields(
                &mut msg.model_id,
                &mut msg.provider_id,
            );
        }
    }

    let filtered = filter_parsed_messages(messages, &options);

    Ok(ParsedMessages {
        messages: filtered,
        counts,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}

#[doc(hidden)]
pub async fn parse_local_unified_messages_with_pricing(
    options: LocalParseOptions,
    pricing: Option<&pricing::PricingService>,
) -> Result<Vec<UnifiedMessage>, String> {
    let (home_dir, clients) = resolve_local_parse_request(&options)?;
    parse_local_unified_messages_resolved(options, &home_dir, &clients, pricing)
}

pub async fn parse_local_unified_messages(
    options: LocalParseOptions,
) -> Result<Vec<UnifiedMessage>, String> {
    let (home_dir, clients) = resolve_local_parse_request(&options)?;
    let pricing = load_pricing_for_local_parse().await;
    parse_local_unified_messages_resolved(options, &home_dir, &clients, pricing.as_deref())
}

fn unified_to_parsed(msg: &UnifiedMessage) -> ParsedMessage {
    ParsedMessage {
        client: msg.client.clone(),
        model_id: msg.model_id.clone(),
        provider_id: msg.provider_id.clone(),
        session_id: msg.session_id.clone(),
        workspace_key: msg.workspace_key.clone(),
        workspace_label: msg.workspace_label.clone(),
        timestamp: msg.timestamp,
        date: msg.date.clone(),
        input: msg.tokens.input,
        output: msg.tokens.output,
        cache_read: msg.tokens.cache_read,
        cache_write: msg.tokens.cache_write,
        reasoning: msg.tokens.reasoning,
        duration_ms: msg.duration_ms,
        message_count: msg.message_count,
        agent: msg.agent.clone(),
    }
}

fn should_keep_deduped_message(seen_keys: &mut HashSet<String>, message: &UnifiedMessage) -> bool {
    message
        .dedup_key
        .as_ref()
        .is_none_or(|key| seen_keys.insert(key.clone()))
}

fn summed_parsed_message_count(messages: &[ParsedMessage]) -> i32 {
    messages
        .iter()
        .map(|msg| msg.message_count.max(0))
        .sum::<i32>()
}

fn filter_parsed_messages(
    messages: Vec<ParsedMessage>,
    options: &LocalParseOptions,
) -> Vec<ParsedMessage> {
    let mut filtered = messages;

    if let Some(year) = &options.year {
        let year_prefix = format!("{}-", year);
        filtered.retain(|m| m.date.starts_with(&year_prefix));
    }

    if let Some(since) = &options.since {
        filtered.retain(|m| m.date.as_str() >= since.as_str());
    }

    if let Some(until) = &options.until {
        filtered.retain(|m| m.date.as_str() <= until.as_str());
    }
    filtered
}

pub fn parsed_to_unified(msg: &ParsedMessage, cost: f64) -> UnifiedMessage {
    UnifiedMessage {
        client: msg.client.clone(),
        model_id: msg.model_id.clone(),
        provider_id: msg.provider_id.clone(),
        session_id: msg.session_id.clone(),
        workspace_key: msg.workspace_key.clone(),
        workspace_label: msg.workspace_label.clone(),
        timestamp: msg.timestamp,
        date: msg.date.clone(),
        tokens: TokenBreakdown {
            input: msg.input,
            output: msg.output,
            cache_read: msg.cache_read,
            cache_write: msg.cache_write,
            reasoning: msg.reasoning,
        },
        cost,
        cost_source: CostSource::Unknown,
        duration_ms: msg.duration_ms,
        message_count: msg.message_count,
        agent: msg.agent.clone(),
        dedup_key: None,
        session_title: None,
        is_turn_start: false,
    }
}

