//! Parallel aggregation of session data
//!
//! Uses rayon for parallel map-reduce operations.

use crate::sessions::UnifiedMessage;
use crate::{
    ClientContribution, DailyContribution, DailyTotals, DataSummary, GraphMeta, GraphResult,
    TokenBreakdown, YearSummary,
};
use rayon::prelude::*;
use std::collections::HashMap;

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
                "[tokens] Warning: Skipping contribution with invalid date '{}' ({} tokens, ${:.4} cost)",
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
}

impl Default for DayAccumulator {
    fn default() -> Self {
        Self {
            totals: DailyTotals::default(),
            token_breakdown: TokenBreakdown::default(),
            clients: HashMap::with_capacity(8),
        }
    }
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
        client_entry.cost += msg.cost;
        client_entry.messages = client_entry
            .messages
            .saturating_add(msg.message_count.max(0));

        // Normalize provider order for deterministic output
        let mut providers: Vec<&str> = client_entry.provider_id.split(", ").collect();
        providers.sort_unstable();
        providers.dedup();
        client_entry.provider_id = providers.join(", ");
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
            active_time_ms: None,
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

