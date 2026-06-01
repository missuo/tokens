use crate::GraphResult;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccuracyConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccuracySourceKind {
    LocalScan,
    ProviderOfficial,
    EstimatedPricing,
    CustomPricing,
    SubmittedServer,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccuracySource {
    pub kind: AccuracySourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    pub label: String,
    pub confidence: AccuracyConfidence,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingAccuracy {
    pub kind: AccuracySourceKind,
    pub confidence: AccuracyConfidence,
    pub source: String,
    pub matched_models: usize,
    pub unpriced_models: usize,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccuracyReport {
    pub confidence: AccuracyConfidence,
    pub sources: Vec<AccuracySource>,
    pub pricing: PricingAccuracy,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccuracyModelEvidence {
    pub client: String,
    pub model: String,
    pub provider: String,
    pub tokens: i64,
    pub cost: f64,
}

pub fn accuracy_report_for_models(
    clients: &[String],
    models: &[AccuracyModelEvidence],
) -> AccuracyReport {
    let mut client_ids = BTreeSet::new();
    for client in clients {
        insert_client_ids(&mut client_ids, client);
    }

    if client_ids.is_empty() {
        for model in models {
            insert_client_ids(&mut client_ids, &model.client);
        }
    }

    let sources = client_ids
        .into_iter()
        .map(|client| source_for_client(&client))
        .collect::<Vec<_>>();
    let source_confidence = sources
        .iter()
        .map(|source| source.confidence)
        .fold(AccuracyConfidence::High, lower_confidence);
    let pricing = pricing_accuracy(models);
    let warnings = pricing_warnings(&pricing);
    let confidence = lower_confidence(source_confidence, pricing.confidence);

    AccuracyReport {
        confidence,
        sources,
        pricing,
        warnings,
    }
}

pub fn accuracy_report_for_graph(graph: &GraphResult) -> AccuracyReport {
    let evidence = graph
        .contributions
        .iter()
        .flat_map(|day| day.clients.iter())
        .map(|client| AccuracyModelEvidence {
            client: client.client.clone(),
            model: client.model_id.clone(),
            provider: client.provider_id.clone(),
            tokens: client.tokens.total(),
            cost: client.cost,
        })
        .collect::<Vec<_>>();

    accuracy_report_for_models(&graph.summary.clients, &evidence)
}

fn insert_client_ids(client_ids: &mut BTreeSet<String>, client: &str) {
    for part in client.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            client_ids.insert(trimmed.to_string());
        }
    }
}

fn source_for_client(client: &str) -> AccuracySource {
    if is_provider_official_client(client) {
        AccuracySource {
            kind: AccuracySourceKind::ProviderOfficial,
            client: Some(client.to_string()),
            label: format!("{client} provider usage cache"),
            confidence: AccuracyConfidence::High,
            reason: "Usage comes from provider API cache.".to_string(),
        }
    } else {
        AccuracySource {
            kind: AccuracySourceKind::LocalScan,
            client: Some(client.to_string()),
            label: format!("{client} local usage data"),
            confidence: AccuracyConfidence::Medium,
            reason: "Usage is parsed from local client data; cost may be estimated.".to_string(),
        }
    }
}

fn is_provider_official_client(client: &str) -> bool {
    let normalized = client.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "cursor" | "trae")
        || normalized.starts_with("cursor-")
        || normalized.starts_with("trae-")
}

fn pricing_accuracy(models: &[AccuracyModelEvidence]) -> PricingAccuracy {
    let mut per_model = BTreeMap::<(String, String), (i64, f64)>::new();
    for model in models {
        if model.tokens <= 0 {
            continue;
        }

        let key = (model.provider.clone(), model.model.clone());
        let entry = per_model.entry(key).or_insert((0, 0.0));
        entry.0 = entry.0.saturating_add(model.tokens);
        entry.1 += model.cost;
    }

    let mut matched_models = 0;
    let mut unpriced_models = 0;
    for (_tokens, cost) in per_model.values() {
        if *cost > 0.0 {
            matched_models += 1;
        } else {
            unpriced_models += 1;
        }
    }

    let confidence = if unpriced_models > 0 {
        AccuracyConfidence::Low
    } else if matched_models > 0 {
        AccuracyConfidence::Medium
    } else {
        AccuracyConfidence::Medium
    };

    PricingAccuracy {
        kind: if per_model.is_empty() {
            AccuracySourceKind::Unknown
        } else {
            AccuracySourceKind::EstimatedPricing
        },
        confidence,
        source: if per_model.is_empty() {
            "no-token-usage".to_string()
        } else {
            "local-cost-or-pricing-table".to_string()
        },
        matched_models,
        unpriced_models,
        stale: false,
    }
}

fn pricing_warnings(pricing: &PricingAccuracy) -> Vec<String> {
    match pricing.unpriced_models {
        0 => Vec::new(),
        1 => vec![
            "1 model has token usage but no priced cost; provider billing may differ.".to_string(),
        ],
        count => vec![format!(
            "{count} models have token usage but no priced cost; provider billing may differ."
        )],
    }
}

fn lower_confidence(left: AccuracyConfidence, right: AccuracyConfidence) -> AccuracyConfidence {
    use AccuracyConfidence::{High, Low, Medium};

    match (left, right) {
        (Low, _) | (_, Low) => Low,
        (Medium, _) | (_, Medium) => Medium,
        (High, High) => High,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DailyTotals, GraphMeta, GraphResult, TokenBreakdown};

    #[test]
    fn accuracy_report_for_models_groups_local_client_sources() {
        let report = accuracy_report_for_models(
            &["claude".to_string(), "codex".to_string()],
            &[AccuracyModelEvidence {
                client: "claude".to_string(),
                model: "claude-sonnet-4.5".to_string(),
                provider: "anthropic".to_string(),
                tokens: 120,
                cost: 0.25,
            }],
        );

        assert_eq!(report.confidence, AccuracyConfidence::Medium);
        assert_eq!(report.sources.len(), 2);
        assert!(report
            .sources
            .iter()
            .all(|source| source.kind == AccuracySourceKind::LocalScan));
        assert_eq!(report.pricing.matched_models, 1);
        assert_eq!(report.pricing.unpriced_models, 0);
    }

    #[test]
    fn accuracy_report_for_models_prefers_explicit_clients_over_merged_evidence_client() {
        let report = accuracy_report_for_models(
            &["claude".to_string(), "codex".to_string()],
            &[AccuracyModelEvidence {
                client: "claude, codex".to_string(),
                model: "gpt-5".to_string(),
                provider: "openai".to_string(),
                tokens: 120,
                cost: 0.25,
            }],
        );

        let clients = report
            .sources
            .iter()
            .filter_map(|source| source.client.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(clients, vec!["claude", "codex"]);
    }

    #[test]
    fn accuracy_report_for_models_lowers_confidence_for_unpriced_models() {
        let report = accuracy_report_for_models(
            &["codex".to_string()],
            &[AccuracyModelEvidence {
                client: "codex".to_string(),
                model: "unknown-model".to_string(),
                provider: "openai".to_string(),
                tokens: 200,
                cost: 0.0,
            }],
        );

        assert_eq!(report.confidence, AccuracyConfidence::Low);
        assert_eq!(report.pricing.confidence, AccuracyConfidence::Low);
        assert_eq!(report.pricing.unpriced_models, 1);
        assert_eq!(
            report.warnings,
            vec!["1 model has token usage but no priced cost; provider billing may differ."]
        );
    }

    #[test]
    fn accuracy_report_for_models_marks_provider_official_sources_high_confidence() {
        let report = accuracy_report_for_models(
            &["cursor".to_string()],
            &[AccuracyModelEvidence {
                client: "cursor".to_string(),
                model: "gpt-5.2".to_string(),
                provider: "openai".to_string(),
                tokens: 300,
                cost: 0.42,
            }],
        );

        assert_eq!(report.sources.len(), 1);
        assert_eq!(report.sources[0].kind, AccuracySourceKind::ProviderOfficial);
        assert_eq!(report.sources[0].confidence, AccuracyConfidence::High);
    }

    #[test]
    fn accuracy_report_for_graph_uses_summary_clients_and_daily_models() {
        let graph = GraphResult {
            meta: GraphMeta {
                generated_at: "2026-06-01T00:00:00Z".to_string(),
                version: "3.0.0-test".to_string(),
                date_range_start: "2026-06-01".to_string(),
                date_range_end: "2026-06-01".to_string(),
                processing_time_ms: 1,
            },
            summary: crate::DataSummary {
                total_tokens: 150,
                total_cost: 0.12,
                total_days: 1,
                active_days: 1,
                average_per_day: 150.0,
                max_cost_in_single_day: 0.12,
                clients: vec!["claude".to_string()],
                models: vec!["claude-sonnet-4.5".to_string()],
            },
            years: Vec::new(),
            contributions: vec![crate::DailyContribution {
                date: "2026-06-01".to_string(),
                totals: DailyTotals {
                    tokens: 150,
                    cost: 0.12,
                    messages: 1,
                },
                intensity: 1,
                token_breakdown: TokenBreakdown {
                    input: 100,
                    output: 50,
                    cache_read: 0,
                    cache_write: 0,
                    reasoning: 0,
                },
                clients: vec![crate::ClientContribution {
                    client: "claude".to_string(),
                    model_id: "claude-sonnet-4.5".to_string(),
                    provider_id: "anthropic".to_string(),
                    tokens: TokenBreakdown {
                        input: 100,
                        output: 50,
                        cache_read: 0,
                        cache_write: 0,
                        reasoning: 0,
                    },
                    cost: 0.12,
                    messages: 1,
                }],
                active_time_ms: None,
            }],
            time_metrics: None,
        };

        let report = accuracy_report_for_graph(&graph);

        assert_eq!(report.confidence, AccuracyConfidence::Medium);
        assert_eq!(report.pricing.matched_models, 1);
        assert_eq!(report.sources[0].client.as_deref(), Some("claude"));
    }
}
