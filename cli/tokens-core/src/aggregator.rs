//! Parallel aggregation of session data
//!
//! Uses rayon for parallel map-reduce operations.

use crate::sessions::UnifiedMessage;
use crate::{
    ClientContribution, DailyContribution, DailyTotals, DataSummary, GraphMeta, GraphResult,
    ProjectContribution, ProjectModelContribution, SessionContribution, TokenBreakdown,
    UnattributedModelDiagnostic, UnattributedSessionDiagnostic, YearSummary,
};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};

/// Aggregate messages into daily contributions
pub fn aggregate_by_date(messages: Vec<UnifiedMessage>) -> Vec<DailyContribution> {
    if messages.is_empty() {
        return Vec::new();
    }

    // Estimate unique days (typically 1-365) - use message count / 10 as heuristic
    let estimated_days = (messages.len() / 10).clamp(30, 400);

    // Parallel aggregation using fold/reduce pattern
    let daily_map: HashMap<String, DayAccumulator> = messages
        .into_par_iter()
        .fold(
            || HashMap::with_capacity(estimated_days),
            |mut acc: HashMap<String, DayAccumulator>, msg| {
                let entry = acc.entry(msg.date.clone()).or_default();
                entry.add_message(&msg);
                acc
            },
        )
        .reduce(
            || HashMap::with_capacity(estimated_days),
            |mut a, b| {
                for (date, acc) in b {
                    a.entry(date).or_default().merge(acc);
                }
                a
            },
        );

    // Convert to sorted vector with pre-allocated capacity
    let mut contributions: Vec<DailyContribution> = Vec::with_capacity(daily_map.len());
    contributions.extend(
        daily_map
            .into_iter()
            .map(|(date, acc)| acc.into_contribution(date)),
    );

    // Sort by date
    contributions.sort_by(|a, b| a.date.cmp(&b.date));

    // Calculate intensities based on max cost
    calculate_intensities(&mut contributions);

    contributions
}

/// Aggregate messages into per-session contributions, keyed on `session_id`.
///
/// Each returned [`SessionContribution`] sums all token buckets and cost for a
/// single session and exposes the same client/model breakdown shape as
/// [`aggregate_by_date`].  Sessions are sorted by `last_seen` descending so the
/// most recently active sessions appear first.
pub const UNATTRIBUTED_SOURCE_IDENTIFIER_LIMIT: usize = 20;

/// Aggregate workspace-less messages into session-level diagnostics without
/// retaining prompts, responses, or message bodies.
pub fn aggregate_unattributed_sessions(
    messages: &[UnifiedMessage],
) -> Vec<UnattributedSessionDiagnostic> {
    #[derive(Default)]
    struct DiagnosticAccumulator {
        first_seen: i64,
        last_seen: i64,
        tokens: i64,
        cost: f64,
        messages: i32,
        models: HashMap<(String, String), UnattributedModelDiagnostic>,
        source_identifiers: BTreeSet<String>,
        initialized: bool,
    }

    let mut sessions: HashMap<(String, String), DiagnosticAccumulator> = HashMap::new();
    for message in messages
        .iter()
        .filter(|message| message.workspace_key.is_none())
    {
        let key = (message.client.clone(), message.session_id.clone());
        let session = sessions.entry(key).or_default();
        let timestamp = timestamp_seconds(message.timestamp);
        if !session.initialized {
            session.first_seen = timestamp;
            session.last_seen = timestamp;
            session.initialized = true;
        } else {
            session.first_seen = session.first_seen.min(timestamp);
            session.last_seen = session.last_seen.max(timestamp);
        }

        let tokens = message.tokens.total();
        let cost = finite_nonnegative_cost(message.cost);
        session.tokens = session.tokens.saturating_add(tokens);
        session.cost += cost;
        session.messages = session
            .messages
            .saturating_add(message.message_count.max(0));

        let model_key = (
            crate::canonical_model_id(&message.model_id),
            message.provider_id.clone(),
        );
        let model = session.models.entry(model_key.clone()).or_insert_with(|| {
            UnattributedModelDiagnostic {
                model_id: model_key.0,
                provider_id: model_key.1,
                tokens: 0,
                cost: 0.0,
                messages: 0,
            }
        });
        model.tokens = model.tokens.saturating_add(tokens);
        model.cost += cost;
        model.messages = model.messages.saturating_add(message.message_count.max(0));

        if let Some(identifier) = message
            .dedup_key
            .as_ref()
            .map(|identifier| identifier.trim())
            .filter(|identifier| !identifier.is_empty())
        {
            let digest = Sha256::digest(identifier.as_bytes());
            session
                .source_identifiers
                .insert(format!("sha256:{digest:x}"));
        }
    }

    let mut output: Vec<UnattributedSessionDiagnostic> = sessions
        .into_iter()
        .map(|((client, session_id), session)| {
            let source_identifier_count = session.source_identifiers.len() as u64;
            let source_identifiers: Vec<String> = session
                .source_identifiers
                .into_iter()
                .take(UNATTRIBUTED_SOURCE_IDENTIFIER_LIMIT)
                .collect();
            let mut models: Vec<UnattributedModelDiagnostic> = session
                .models
                .into_values()
                .map(|mut model| {
                    model.cost = finite_nonnegative_cost(model.cost);
                    model
                })
                .collect();
            models.sort_by(|a, b| {
                b.cost
                    .total_cmp(&a.cost)
                    .then_with(|| b.tokens.cmp(&a.tokens))
                    .then_with(|| a.model_id.cmp(&b.model_id))
                    .then_with(|| a.provider_id.cmp(&b.provider_id))
            });
            UnattributedSessionDiagnostic {
                client,
                session_id,
                first_seen: session.first_seen,
                last_seen: session.last_seen,
                tokens: session.tokens.max(0),
                cost: finite_nonnegative_cost(session.cost),
                messages: session.messages.max(0),
                models,
                source_identifiers_truncated: source_identifier_count
                    > UNATTRIBUTED_SOURCE_IDENTIFIER_LIMIT as u64,
                source_identifier_count,
                source_identifiers,
            }
        })
        .collect();
    output.sort_by(|a, b| {
        a.client
            .cmp(&b.client)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    output
}

fn finite_nonnegative_cost(cost: f64) -> f64 {
    if cost.is_finite() {
        cost.max(0.0)
    } else {
        0.0
    }
}

fn timestamp_seconds(timestamp: i64) -> i64 {
    if timestamp.unsigned_abs() >= 1_000_000_000_000 {
        timestamp / 1000
    } else {
        timestamp
    }
}

pub fn aggregate_by_session(messages: Vec<UnifiedMessage>) -> Vec<SessionContribution> {
    if messages.is_empty() {
        return Vec::new();
    }

    let session_map: HashMap<String, SessionAccumulator> = messages
        .into_par_iter()
        .fold(
            HashMap::new,
            |mut acc: HashMap<String, SessionAccumulator>, msg| {
                let entry = acc.entry(msg.session_id.clone()).or_default();
                entry.add_message(&msg);
                acc
            },
        )
        .reduce(HashMap::new, |mut a, b| {
            for (id, acc) in b {
                a.entry(id).or_default().merge(acc);
            }
            a
        });

    let mut contributions: Vec<SessionContribution> = session_map
        .into_iter()
        .map(|(session_id, acc)| acc.into_contribution(session_id))
        .collect();

    // Most recently active first; stable sort by session_id when ties.
    contributions.sort_by(|a, b| {
        b.last_seen
            .cmp(&a.last_seen)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    contributions
}

/// Calculate summary statistics
pub fn calculate_summary(contributions: &[DailyContribution]) -> DataSummary {
    // Daily totals already saturate at i64::MAX (clamped extreme inputs), so
    // summing several such days must saturate too rather than overflow.
    let total_tokens: i64 = contributions
        .iter()
        .map(|c| c.totals.tokens)
        .fold(0i64, i64::saturating_add);
    let total_cost: f64 = contributions.iter().map(|c| c.totals.cost).sum();
    let active_days = contributions
        .iter()
        .filter(|c| c.totals.tokens > 0 || c.totals.cost > 0.0 || c.totals.messages > 0)
        .count() as i32;
    let max_cost = contributions
        .iter()
        .map(|c| c.totals.cost)
        .fold(0.0, f64::max);

    let mut clients_set = std::collections::HashSet::with_capacity(5);
    let mut models_set = std::collections::HashSet::with_capacity(20);

    for c in contributions {
        for s in &c.clients {
            clients_set.insert(s.client.clone());
            models_set.insert(s.model_id.clone());
        }
    }

    DataSummary {
        total_tokens,
        total_cost,
        total_days: contributions.len() as i32,
        active_days,
        average_per_day: if active_days > 0 {
            total_cost / active_days as f64
        } else {
            0.0
        },
        max_cost_in_single_day: max_cost,
        clients: {
            let mut v: Vec<_> = clients_set.into_iter().collect();
            v.sort();
            v
        },
        models: {
            let mut v: Vec<_> = models_set.into_iter().collect();
            v.sort();
            v
        },
    }
}

/// Calculate year summaries
pub fn calculate_years(contributions: &[DailyContribution]) -> Vec<YearSummary> {
    let mut years_map: HashMap<String, YearAccumulator> = HashMap::with_capacity(5);

    for c in contributions {
        // Guard against short/invalid date strings
        if c.date.len() < 4 {
            eprintln!(
                "Warning: Skipping contribution with invalid date '{}' ({} tokens, ${:.4} cost)",
                c.date, c.totals.tokens, c.totals.cost
            );
            continue;
        }
        let year = &c.date[0..4];
        let entry = years_map.entry(year.to_string()).or_default();
        entry.tokens = entry.tokens.saturating_add(c.totals.tokens);
        entry.cost += c.totals.cost;

        if entry.start.is_empty() || c.date < entry.start {
            entry.start = c.date.clone();
        }
        if entry.end.is_empty() || c.date > entry.end {
            entry.end = c.date.clone();
        }
    }

    let mut years: Vec<YearSummary> = Vec::with_capacity(years_map.len());
    years.extend(years_map.into_iter().map(|(year, acc)| YearSummary {
        year,
        total_tokens: acc.tokens,
        total_cost: acc.cost,
        range_start: acc.start,
        range_end: acc.end,
    }));

    years.sort_by(|a, b| a.year.cmp(&b.year));
    years
}

/// Generate complete graph result
pub fn generate_graph_result(
    contributions: Vec<DailyContribution>,
    processing_time_ms: u32,
) -> GraphResult {
    let summary = calculate_summary(&contributions);
    let years = calculate_years(&contributions);

    let date_range_start = contributions
        .first()
        .map(|c| c.date.clone())
        .unwrap_or_default();
    let date_range_end = contributions
        .last()
        .map(|c| c.date.clone())
        .unwrap_or_default();

    GraphResult {
        meta: GraphMeta {
            generated_at: chrono::Utc::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            date_range_start,
            date_range_end,
            processing_time_ms,
        },
        summary,
        years,
        contributions,
        time_metrics: None,
    }
}

// =============================================================================
// Internal helpers
// =============================================================================

struct DayAccumulator {
    totals: DailyTotals,
    token_breakdown: TokenBreakdown,
    clients: HashMap<String, ClientContribution>,
    projects: HashMap<Option<String>, ProjectAccumulator>,
}

impl Default for DayAccumulator {
    fn default() -> Self {
        Self {
            totals: DailyTotals::default(),
            token_breakdown: TokenBreakdown::default(),
            clients: HashMap::with_capacity(8),
            projects: HashMap::with_capacity(8),
        }
    }
}

#[derive(Default)]
struct ProjectAccumulator {
    /// Timestamp + label lets parallel reductions choose the latest label
    /// deterministically when a workspace was renamed.
    latest_label: Option<(i64, String)>,
    totals: DailyTotals,
    models: HashMap<(String, String), ProjectModelContribution>,
}

impl DayAccumulator {
    fn add_message(&mut self, msg: &UnifiedMessage) {
        let total_tokens = msg
            .tokens
            .input
            .saturating_add(msg.tokens.output)
            .saturating_add(msg.tokens.cache_read)
            .saturating_add(msg.tokens.cache_write)
            .saturating_add(msg.tokens.reasoning);
        let cost = finite_nonnegative_cost(msg.cost);

        self.totals.tokens = self.totals.tokens.saturating_add(total_tokens);
        self.totals.cost += cost;
        self.totals.messages = self
            .totals
            .messages
            .saturating_add(msg.message_count.max(0));

        self.token_breakdown.input = self.token_breakdown.input.saturating_add(msg.tokens.input);
        self.token_breakdown.output = self
            .token_breakdown
            .output
            .saturating_add(msg.tokens.output);
        self.token_breakdown.cache_read = self
            .token_breakdown
            .cache_read
            .saturating_add(msg.tokens.cache_read);
        self.token_breakdown.cache_write = self
            .token_breakdown
            .cache_write
            .saturating_add(msg.tokens.cache_write);
        self.token_breakdown.reasoning = self
            .token_breakdown
            .reasoning
            .saturating_add(msg.tokens.reasoning);

        // Update client contribution
        // Canonical (alias-free) id: this contribution is serialized into the
        // submit/upload/export payload, so a machine-local `modelAliases` config
        // must not rewrite the model identity that leaves the machine.
        let key = format!(
            "{}:{}",
            msg.client,
            crate::canonical_model_id(&msg.model_id)
        );
        let client_entry = self
            .clients
            .entry(key)
            .or_insert_with(|| ClientContribution {
                client: msg.client.clone(),
                model_id: crate::canonical_model_id(&msg.model_id),
                provider_id: msg.provider_id.clone(),
                tokens: TokenBreakdown::default(),
                cost: 0.0,
                messages: 0,
            });

        // Merge provider_id if different provider contributes to same client+model
        if !client_entry
            .provider_id
            .split(", ")
            .any(|p| p == msg.provider_id)
        {
            client_entry.provider_id = format!("{}, {}", client_entry.provider_id, msg.provider_id);
        }

        client_entry.tokens.input = client_entry.tokens.input.saturating_add(msg.tokens.input);
        client_entry.tokens.output = client_entry.tokens.output.saturating_add(msg.tokens.output);
        client_entry.tokens.cache_read = client_entry
            .tokens
            .cache_read
            .saturating_add(msg.tokens.cache_read);
        client_entry.tokens.cache_write = client_entry
            .tokens
            .cache_write
            .saturating_add(msg.tokens.cache_write);
        client_entry.tokens.reasoning = client_entry
            .tokens
            .reasoning
            .saturating_add(msg.tokens.reasoning);
        client_entry.cost += cost;
        client_entry.messages = client_entry
            .messages
            .saturating_add(msg.message_count.max(0));

        // Normalize provider order for deterministic output
        let mut providers: Vec<&str> = client_entry.provider_id.split(", ").collect();
        providers.sort_unstable();
        providers.dedup();
        client_entry.provider_id = providers.join(", ");

        let project_key = msg.workspace_key.clone();
        let project = self.projects.entry(project_key).or_default();
        if let Some(label) = msg
            .workspace_label
            .as_ref()
            .map(|label| label.trim())
            .filter(|label| !label.is_empty())
        {
            let candidate = (msg.timestamp, label.to_string());
            if project.latest_label.as_ref().is_none_or(|current| {
                candidate.0 > current.0 || (candidate.0 == current.0 && candidate.1 > current.1)
            }) {
                project.latest_label = Some(candidate);
            }
        }
        project.totals.tokens = project.totals.tokens.saturating_add(total_tokens);
        project.totals.cost += cost;
        project.totals.messages = project
            .totals
            .messages
            .saturating_add(msg.message_count.max(0));

        let project_model_key = (
            crate::canonical_model_id(&msg.model_id),
            msg.provider_id.clone(),
        );
        let project_model = project
            .models
            .entry(project_model_key.clone())
            .or_insert_with(|| ProjectModelContribution {
                model_id: project_model_key.0,
                provider_id: project_model_key.1,
                tokens: 0,
                cost: 0.0,
                messages: 0,
            });
        project_model.tokens = project_model.tokens.saturating_add(total_tokens);
        project_model.cost += cost;
        project_model.messages = project_model
            .messages
            .saturating_add(msg.message_count.max(0));
    }

    fn merge(&mut self, other: DayAccumulator) {
        self.totals.tokens = self.totals.tokens.saturating_add(other.totals.tokens);
        self.totals.cost += other.totals.cost;
        self.totals.messages = self.totals.messages.saturating_add(other.totals.messages);

        self.token_breakdown.input = self
            .token_breakdown
            .input
            .saturating_add(other.token_breakdown.input);
        self.token_breakdown.output = self
            .token_breakdown
            .output
            .saturating_add(other.token_breakdown.output);
        self.token_breakdown.cache_read = self
            .token_breakdown
            .cache_read
            .saturating_add(other.token_breakdown.cache_read);
        self.token_breakdown.cache_write = self
            .token_breakdown
            .cache_write
            .saturating_add(other.token_breakdown.cache_write);
        self.token_breakdown.reasoning = self
            .token_breakdown
            .reasoning
            .saturating_add(other.token_breakdown.reasoning);

        for (key, client_contrib) in other.clients {
            let entry = self
                .clients
                .entry(key)
                .or_insert_with(|| ClientContribution {
                    client: client_contrib.client.clone(),
                    model_id: client_contrib.model_id.clone(),
                    provider_id: client_contrib.provider_id.clone(),
                    tokens: TokenBreakdown::default(),
                    cost: 0.0,
                    messages: 0,
                });

            // Merge provider_ids from parallel reduction
            for provider in client_contrib.provider_id.split(", ") {
                if !entry.provider_id.split(", ").any(|p| p == provider) {
                    entry.provider_id = format!("{}, {}", entry.provider_id, provider);
                }
            }

            entry.tokens.input = entry
                .tokens
                .input
                .saturating_add(client_contrib.tokens.input);
            entry.tokens.output = entry
                .tokens
                .output
                .saturating_add(client_contrib.tokens.output);
            entry.tokens.cache_read = entry
                .tokens
                .cache_read
                .saturating_add(client_contrib.tokens.cache_read);
            entry.tokens.cache_write = entry
                .tokens
                .cache_write
                .saturating_add(client_contrib.tokens.cache_write);
            entry.tokens.reasoning = entry
                .tokens
                .reasoning
                .saturating_add(client_contrib.tokens.reasoning);
            entry.cost += client_contrib.cost;
            entry.messages = entry.messages.saturating_add(client_contrib.messages);
        }

        // Normalize provider order for deterministic output
        for entry in self.clients.values_mut() {
            let mut providers: Vec<&str> = entry.provider_id.split(", ").collect();
            providers.sort_unstable();
            providers.dedup();
            entry.provider_id = providers.join(", ");
        }

        for (project_key, other_project) in other.projects {
            let project = self.projects.entry(project_key).or_default();
            if let Some(candidate) = other_project.latest_label {
                if project.latest_label.as_ref().is_none_or(|current| {
                    candidate.0 > current.0 || (candidate.0 == current.0 && candidate.1 > current.1)
                }) {
                    project.latest_label = Some(candidate);
                }
            }
            project.totals.tokens = project
                .totals
                .tokens
                .saturating_add(other_project.totals.tokens);
            project.totals.cost += finite_nonnegative_cost(other_project.totals.cost);
            project.totals.messages = project
                .totals
                .messages
                .saturating_add(other_project.totals.messages);
            for (model_key, other_model) in other_project.models {
                let model =
                    project
                        .models
                        .entry(model_key)
                        .or_insert_with(|| ProjectModelContribution {
                            model_id: other_model.model_id.clone(),
                            provider_id: other_model.provider_id.clone(),
                            tokens: 0,
                            cost: 0.0,
                            messages: 0,
                        });
                model.tokens = model.tokens.saturating_add(other_model.tokens);
                model.cost += finite_nonnegative_cost(other_model.cost);
                model.messages = model.messages.saturating_add(other_model.messages);
            }
        }
    }

    fn into_contribution(self, date: String) -> DailyContribution {
        let token_breakdown = TokenBreakdown {
            input: self.token_breakdown.input.max(0),
            output: self.token_breakdown.output.max(0),
            cache_read: self.token_breakdown.cache_read.max(0),
            cache_write: self.token_breakdown.cache_write.max(0),
            reasoning: self.token_breakdown.reasoning.max(0),
        };

        let clients: Vec<ClientContribution> = self
            .clients
            .into_values()
            .map(|mut s| {
                s.tokens.input = s.tokens.input.max(0);
                s.tokens.output = s.tokens.output.max(0);
                s.tokens.cache_read = s.tokens.cache_read.max(0);
                s.tokens.cache_write = s.tokens.cache_write.max(0);
                s.tokens.reasoning = s.tokens.reasoning.max(0);
                s.cost = s.cost.max(0.0);
                s
            })
            .collect();

        let mut projects: Vec<ProjectContribution> = self
            .projects
            .into_iter()
            .map(|(project_key, project)| {
                let project_label = project
                    .latest_label
                    .map(|(_, label)| label)
                    .or_else(|| {
                        project_key
                            .as_deref()
                            .and_then(crate::sessions::workspace_label_from_key)
                    })
                    .unwrap_or_else(|| "Unattributed".to_string());
                let mut models: Vec<ProjectModelContribution> = project
                    .models
                    .into_values()
                    .map(|mut model| {
                        model.tokens = model.tokens.max(0);
                        model.cost = finite_nonnegative_cost(model.cost);
                        model.messages = model.messages.max(0);
                        model
                    })
                    .collect();
                models.sort_by(|a, b| {
                    b.cost
                        .total_cmp(&a.cost)
                        .then_with(|| b.tokens.cmp(&a.tokens))
                        .then_with(|| a.model_id.cmp(&b.model_id))
                        .then_with(|| a.provider_id.cmp(&b.provider_id))
                });
                ProjectContribution {
                    project_key,
                    project_label,
                    totals: DailyTotals {
                        tokens: project.totals.tokens.max(0),
                        cost: finite_nonnegative_cost(project.totals.cost),
                        messages: project.totals.messages.max(0),
                    },
                    models,
                }
            })
            .collect();
        projects.sort_by(|a, b| {
            b.totals
                .cost
                .total_cmp(&a.totals.cost)
                .then_with(|| b.totals.tokens.cmp(&a.totals.tokens))
                .then_with(|| a.project_label.cmp(&b.project_label))
                .then_with(|| a.project_key.cmp(&b.project_key))
        });

        DailyContribution {
            date,
            totals: DailyTotals {
                tokens: self.totals.tokens.max(0),
                cost: self.totals.cost.max(0.0),
                messages: self.totals.messages.max(0),
            },
            intensity: 0,
            token_breakdown,
            clients,
            projects,
            active_time_ms: None,
        }
    }
}

struct SessionAccumulator {
    totals: DailyTotals,
    token_breakdown: TokenBreakdown,
    clients: HashMap<String, ClientContribution>,
    /// Tracks the most-active (client, provider, model) for the session, used
    /// as the canonical top-level fields on `SessionContribution`.
    top_client: String,
    top_provider: String,
    top_model: String,
    top_cost: f64,
    first_seen: i64,
    last_seen: i64,
}

impl Default for SessionAccumulator {
    fn default() -> Self {
        Self {
            totals: DailyTotals::default(),
            token_breakdown: TokenBreakdown::default(),
            clients: HashMap::with_capacity(2),
            top_client: String::new(),
            top_provider: String::new(),
            top_model: String::new(),
            top_cost: f64::NEG_INFINITY,
            first_seen: i64::MAX,
            last_seen: i64::MIN,
        }
    }
}

impl SessionAccumulator {
    fn add_message(&mut self, msg: &UnifiedMessage) {
        let total_tokens = msg
            .tokens
            .input
            .saturating_add(msg.tokens.output)
            .saturating_add(msg.tokens.cache_read)
            .saturating_add(msg.tokens.cache_write)
            .saturating_add(msg.tokens.reasoning);

        self.totals.tokens = self.totals.tokens.saturating_add(total_tokens);
        self.totals.cost += msg.cost;
        self.totals.messages = self
            .totals
            .messages
            .saturating_add(msg.message_count.max(0));

        self.token_breakdown.input = self.token_breakdown.input.saturating_add(msg.tokens.input);
        self.token_breakdown.output = self
            .token_breakdown
            .output
            .saturating_add(msg.tokens.output);
        self.token_breakdown.cache_read = self
            .token_breakdown
            .cache_read
            .saturating_add(msg.tokens.cache_read);
        self.token_breakdown.cache_write = self
            .token_breakdown
            .cache_write
            .saturating_add(msg.tokens.cache_write);
        self.token_breakdown.reasoning = self
            .token_breakdown
            .reasoning
            .saturating_add(msg.tokens.reasoning);

        // Track tightest (client, provider, model) by cost contribution.
        // Canonical (alias-free) id — this feeds the submitted/exported payload,
        // so machine-local aliases must not rewrite it (see `add_message`).
        let normalized_model = crate::canonical_model_id(&msg.model_id);
        let key = format!("{}:{}:{}", msg.client, msg.provider_id, normalized_model);
        let client_entry = self
            .clients
            .entry(key)
            .or_insert_with(|| ClientContribution {
                client: msg.client.clone(),
                model_id: normalized_model.clone(),
                provider_id: msg.provider_id.clone(),
                tokens: TokenBreakdown::default(),
                cost: 0.0,
                messages: 0,
            });
        client_entry.tokens.input = client_entry.tokens.input.saturating_add(msg.tokens.input);
        client_entry.tokens.output = client_entry.tokens.output.saturating_add(msg.tokens.output);
        client_entry.tokens.cache_read = client_entry
            .tokens
            .cache_read
            .saturating_add(msg.tokens.cache_read);
        client_entry.tokens.cache_write = client_entry
            .tokens
            .cache_write
            .saturating_add(msg.tokens.cache_write);
        client_entry.tokens.reasoning = client_entry
            .tokens
            .reasoning
            .saturating_add(msg.tokens.reasoning);
        client_entry.cost += msg.cost;
        client_entry.messages = client_entry
            .messages
            .saturating_add(msg.message_count.max(0));

        if client_entry.cost > self.top_cost {
            self.top_cost = client_entry.cost;
            self.top_client = client_entry.client.clone();
            self.top_provider = client_entry.provider_id.clone();
            self.top_model = client_entry.model_id.clone();
        }

        // Timestamps in UnifiedMessage are stored in milliseconds in most
        // parsers; normalize to seconds for the contribution wire format.
        let secs = timestamp_seconds(msg.timestamp);
        if secs < self.first_seen {
            self.first_seen = secs;
        }
        if secs > self.last_seen {
            self.last_seen = secs;
        }
    }

    fn merge(&mut self, other: SessionAccumulator) {
        self.totals.tokens = self.totals.tokens.saturating_add(other.totals.tokens);
        self.totals.cost += other.totals.cost;
        self.totals.messages = self.totals.messages.saturating_add(other.totals.messages);

        self.token_breakdown.input = self
            .token_breakdown
            .input
            .saturating_add(other.token_breakdown.input);
        self.token_breakdown.output = self
            .token_breakdown
            .output
            .saturating_add(other.token_breakdown.output);
        self.token_breakdown.cache_read = self
            .token_breakdown
            .cache_read
            .saturating_add(other.token_breakdown.cache_read);
        self.token_breakdown.cache_write = self
            .token_breakdown
            .cache_write
            .saturating_add(other.token_breakdown.cache_write);
        self.token_breakdown.reasoning = self
            .token_breakdown
            .reasoning
            .saturating_add(other.token_breakdown.reasoning);

        for (key, contrib) in other.clients {
            let entry = self
                .clients
                .entry(key)
                .or_insert_with(|| ClientContribution {
                    client: contrib.client.clone(),
                    model_id: contrib.model_id.clone(),
                    provider_id: contrib.provider_id.clone(),
                    tokens: TokenBreakdown::default(),
                    cost: 0.0,
                    messages: 0,
                });
            entry.tokens.input = entry.tokens.input.saturating_add(contrib.tokens.input);
            entry.tokens.output = entry.tokens.output.saturating_add(contrib.tokens.output);
            entry.tokens.cache_read = entry
                .tokens
                .cache_read
                .saturating_add(contrib.tokens.cache_read);
            entry.tokens.cache_write = entry
                .tokens
                .cache_write
                .saturating_add(contrib.tokens.cache_write);
            entry.tokens.reasoning = entry
                .tokens
                .reasoning
                .saturating_add(contrib.tokens.reasoning);
            entry.cost += contrib.cost;
            entry.messages = entry.messages.saturating_add(contrib.messages);

            if entry.cost > self.top_cost {
                self.top_cost = entry.cost;
                self.top_client = entry.client.clone();
                self.top_provider = entry.provider_id.clone();
                self.top_model = entry.model_id.clone();
            }
        }

        if other.first_seen < self.first_seen {
            self.first_seen = other.first_seen;
        }
        if other.last_seen > self.last_seen {
            self.last_seen = other.last_seen;
        }
    }

    fn into_contribution(self, session_id: String) -> SessionContribution {
        let token_breakdown = TokenBreakdown {
            input: self.token_breakdown.input.max(0),
            output: self.token_breakdown.output.max(0),
            cache_read: self.token_breakdown.cache_read.max(0),
            cache_write: self.token_breakdown.cache_write.max(0),
            reasoning: self.token_breakdown.reasoning.max(0),
        };

        let mut clients: Vec<ClientContribution> = self
            .clients
            .into_values()
            .map(|mut c| {
                c.tokens.input = c.tokens.input.max(0);
                c.tokens.output = c.tokens.output.max(0);
                c.tokens.cache_read = c.tokens.cache_read.max(0);
                c.tokens.cache_write = c.tokens.cache_write.max(0);
                c.tokens.reasoning = c.tokens.reasoning.max(0);
                c.cost = c.cost.max(0.0);
                c
            })
            .collect();
        clients.sort_by(|a, b| {
            b.cost
                .partial_cmp(&a.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.client.cmp(&b.client))
                .then_with(|| a.model_id.cmp(&b.model_id))
        });

        let first_seen = if self.first_seen == i64::MAX {
            0
        } else {
            self.first_seen
        };
        let last_seen = if self.last_seen == i64::MIN {
            0
        } else {
            self.last_seen
        };

        SessionContribution {
            session_id,
            client: self.top_client,
            provider: self.top_provider,
            model: self.top_model,
            totals: DailyTotals {
                tokens: self.totals.tokens.max(0),
                cost: self.totals.cost.max(0.0),
                messages: self.totals.messages.max(0),
            },
            token_breakdown,
            clients,
            first_seen,
            last_seen,
        }
    }
}

#[derive(Default)]
struct YearAccumulator {
    tokens: i64,
    cost: f64,
    start: String,
    end: String,
}

/// Cost-relative intensity buckets (0-4): each day's intensity is a function
/// of its cost relative to the maximum cost across all `contributions`.
pub fn calculate_intensities(contributions: &mut [DailyContribution]) {
    let max_cost = contributions
        .iter()
        .map(|c| c.totals.cost)
        .fold(0.0, f64::max);

    if max_cost == 0.0 {
        return;
    }

    for c in contributions.iter_mut() {
        let ratio = c.totals.cost / max_cost;
        c.intensity = if ratio >= 0.75 {
            4
        } else if ratio >= 0.5 {
            3
        } else if ratio >= 0.25 {
            2
        } else if ratio > 0.0 {
            1
        } else {
            0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(
        client: &str,
        session: &str,
        model: &str,
        cost: f64,
        workspace_key: Option<&str>,
        workspace_label: Option<&str>,
    ) -> UnifiedMessage {
        let mut message = UnifiedMessage::new_with_dedup(
            client,
            model,
            "provider",
            session,
            1_700_000_000_000,
            TokenBreakdown {
                input: 10,
                output: 5,
                cache_read: 2,
                cache_write: 1,
                reasoning: 0,
            },
            cost,
            Some(format!("{client}:{session}:{model}")),
        );
        message.message_count = 2;
        message.set_workspace(
            workspace_key.map(str::to_string),
            workspace_label.map(str::to_string),
        );
        message
    }

    #[test]
    fn projects_conserve_daily_totals_and_keep_distinct_keys() {
        let messages = vec![
            message("a", "1", "expensive", 4.0, Some("/one/app"), Some("app")),
            message("b", "2", "cheap", 1.0, Some("/two/app"), Some("app")),
            message("c", "3", "unknown", 2.0, None, None),
        ];
        let days = aggregate_by_date(messages);
        assert_eq!(days.len(), 1);
        let day = &days[0];
        assert_eq!(day.projects.len(), 3);
        assert_eq!(day.projects[0].totals.cost, 4.0);
        assert_eq!(day.projects[1].project_key, None);
        assert_eq!(day.projects[1].project_label, "Unattributed");
        assert_eq!(day.projects[2].project_key.as_deref(), Some("/two/app"));
        assert_eq!(
            day.projects
                .iter()
                .map(|project| project.totals.tokens)
                .sum::<i64>(),
            day.totals.tokens
        );
        assert_eq!(
            day.projects
                .iter()
                .map(|project| project.totals.messages)
                .sum::<i32>(),
            day.totals.messages
        );
        assert!(
            (day.projects
                .iter()
                .map(|project| project.totals.cost)
                .sum::<f64>()
                - day.totals.cost)
                .abs()
                < f64::EPSILON
        );

        let graph_json = serde_json::to_value(generate_graph_result(days, 0)).unwrap();
        assert!(graph_json["contributions"][0].get("projects").is_none());
    }

    #[test]
    fn project_and_unattributed_costs_are_finite_and_nonnegative() {
        let messages = vec![
            message("client", "nan", "model-nan", f64::NAN, None, None),
            message(
                "client",
                "negative",
                "model-negative",
                -3.0,
                Some("/workspace"),
                Some("workspace"),
            ),
            message(
                "client",
                "infinite",
                "model-infinite",
                f64::INFINITY,
                Some("/workspace"),
                Some("workspace"),
            ),
            message(
                "client",
                "valid",
                "model-valid",
                2.5,
                Some("/workspace"),
                Some("workspace"),
            ),
        ];

        let days = aggregate_by_date(messages.clone());
        assert!(days[0].totals.cost.is_finite() && days[0].totals.cost >= 0.0);
        assert!(days[0]
            .clients
            .iter()
            .all(|client| client.cost.is_finite() && client.cost >= 0.0));
        for project in &days[0].projects {
            assert!(project.totals.cost.is_finite());
            assert!(project.totals.cost >= 0.0);
            assert!(project
                .models
                .iter()
                .all(|model| model.cost.is_finite() && model.cost >= 0.0));
        }
        let workspace = days[0]
            .projects
            .iter()
            .find(|project| project.project_key.as_deref() == Some("/workspace"))
            .unwrap();
        assert_eq!(workspace.totals.cost, 2.5);
        assert_eq!(workspace.models[0].model_id, "model-valid");
        let graph_json = serde_json::to_value(generate_graph_result(days, 0)).unwrap();
        assert!(graph_json["contributions"][0]["totals"]["cost"].is_number());

        let diagnostics = aggregate_unattributed_sessions(&messages);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].cost, 0.0);
        assert!(diagnostics[0]
            .models
            .iter()
            .all(|model| model.cost.is_finite() && model.cost >= 0.0));
        let encoded = serde_json::to_vec(&diagnostics).unwrap();
        let decoded: Vec<UnattributedSessionDiagnostic> = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, diagnostics);
    }

    #[test]
    fn timestamp_seconds_handles_extremes_and_millisecond_boundary() {
        assert_eq!(timestamp_seconds(999_999_999_999), 999_999_999_999);
        assert_eq!(timestamp_seconds(-999_999_999_999), -999_999_999_999);
        assert_eq!(timestamp_seconds(1_000_000_000_000), 1_000_000_000);
        assert_eq!(timestamp_seconds(-1_000_000_000_000), -1_000_000_000);
        assert_eq!(timestamp_seconds(i64::MIN), i64::MIN / 1000);
        assert_eq!(timestamp_seconds(i64::MAX), i64::MAX / 1000);
    }

    #[test]
    fn unattributed_diagnostics_preserve_models_and_bound_source_samples() {
        let mut messages = Vec::new();
        for index in 0..(UNATTRIBUTED_SOURCE_IDENTIFIER_LIMIT + 3) {
            let mut item = message(
                "client",
                "session",
                if index % 2 == 0 { "model-a" } else { "model-b" },
                1.0,
                None,
                None,
            );
            item.timestamp += index as i64 * 1_000;
            item.dedup_key = Some(format!("source-{index:02}"));
            messages.push(item);
        }
        messages.push(message(
            "client",
            "attributed",
            "model-c",
            10.0,
            Some("/workspace"),
            Some("workspace"),
        ));

        let diagnostics = aggregate_unattributed_sessions(&messages);
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.models.len(), 2);
        assert_eq!(
            diagnostic.source_identifiers.len(),
            UNATTRIBUTED_SOURCE_IDENTIFIER_LIMIT
        );
        assert_eq!(
            diagnostic.source_identifier_count,
            (UNATTRIBUTED_SOURCE_IDENTIFIER_LIMIT + 3) as u64
        );
        assert!(diagnostic.source_identifiers_truncated);
        assert_eq!(diagnostic.first_seen, 1_700_000_000);
        assert_eq!(
            diagnostic.last_seen,
            1_700_000_000 + (UNATTRIBUTED_SOURCE_IDENTIFIER_LIMIT + 2) as i64
        );
    }
}
