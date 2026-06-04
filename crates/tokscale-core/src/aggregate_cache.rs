use crate::{merge_daily_contributions, DailyContribution};
use std::collections::{BTreeMap, BTreeSet};

pub const CURRENT_AGGREGATE_CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAggregateEntry {
    pub source_key: String,
    pub fingerprint: String,
    pub contributions: Vec<DailyContribution>,
}

impl SourceAggregateEntry {
    pub fn new(
        source_key: impl Into<String>,
        fingerprint: impl Into<String>,
        contributions: Vec<DailyContribution>,
    ) -> Self {
        Self {
            source_key: source_key.into(),
            fingerprint: fingerprint.into(),
            contributions,
        }
    }

    fn dates(&self) -> BTreeSet<String> {
        self.contributions
            .iter()
            .map(|contribution| contribution.date.clone())
            .collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateCache {
    pub schema_version: u32,
    pub sources: BTreeMap<String, SourceAggregateEntry>,
}

impl Default for AggregateCache {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_AGGREGATE_CACHE_SCHEMA_VERSION,
            sources: BTreeMap::new(),
        }
    }
}

impl AggregateCache {
    pub fn upsert(&mut self, entry: SourceAggregateEntry) -> BTreeSet<String> {
        let source_key = entry.source_key.clone();
        if let Some(existing) = self.sources.get(&source_key) {
            if existing.fingerprint == entry.fingerprint {
                return BTreeSet::new();
            }
        }

        let mut affected_dates = self
            .sources
            .get(&source_key)
            .map(SourceAggregateEntry::dates)
            .unwrap_or_default();
        affected_dates.extend(entry.dates());
        self.sources.insert(source_key, entry);
        affected_dates
    }

    pub fn contributions_for_dates(&self, dates: &BTreeSet<String>) -> Vec<DailyContribution> {
        let contributions = self
            .sources
            .values()
            .flat_map(|entry| {
                entry
                    .contributions
                    .iter()
                    .filter(|contribution| dates.contains(&contribution.date))
                    .cloned()
            })
            .collect();

        merge_daily_contributions(contributions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientContribution, DailyContribution, DailyTotals, TokenBreakdown};
    use std::collections::BTreeSet;

    fn token_breakdown(total_tokens: i64) -> TokenBreakdown {
        TokenBreakdown {
            input: total_tokens,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        }
    }

    fn daily_contribution_for_test(
        date: &str,
        client: &str,
        total_tokens: i64,
    ) -> DailyContribution {
        DailyContribution {
            date: date.to_string(),
            totals: DailyTotals {
                tokens: total_tokens,
                cost: total_tokens as f64 / 1000.0,
                messages: 1,
            },
            intensity: 0,
            token_breakdown: token_breakdown(total_tokens),
            clients: vec![ClientContribution {
                client: client.to_string(),
                model_id: "model-a".to_string(),
                provider_id: "openai".to_string(),
                tokens: token_breakdown(total_tokens),
                cost: total_tokens as f64 / 1000.0,
                messages: 1,
            }],
            active_time_ms: None,
        }
    }

    #[test]
    fn aggregate_cache_replaces_changed_source_and_tracks_affected_dates() {
        let mut cache = AggregateCache::default();
        let first = SourceAggregateEntry::new(
            "codex:/tmp/a.jsonl",
            "fp1",
            vec![daily_contribution_for_test("2026-06-01", "codex", 100)],
        );
        cache.upsert(first);

        let changed = SourceAggregateEntry::new(
            "codex:/tmp/a.jsonl",
            "fp2",
            vec![daily_contribution_for_test("2026-06-02", "codex", 200)],
        );
        let affected = cache.upsert(changed);

        assert_eq!(
            affected,
            BTreeSet::from(["2026-06-01".to_string(), "2026-06-02".to_string()])
        );
        let contributions = cache.contributions_for_dates(&affected);
        assert_eq!(contributions.len(), 1);
        assert_eq!(contributions[0].date, "2026-06-02");
        assert_eq!(contributions[0].totals.tokens, 200);
    }

    #[test]
    fn aggregate_cache_merges_multiple_sources_for_requested_dates() {
        let mut cache = AggregateCache::default();
        cache.upsert(SourceAggregateEntry::new(
            "codex:/tmp/a.jsonl",
            "fp1",
            vec![daily_contribution_for_test("2026-06-02", "codex", 200)],
        ));
        cache.upsert(SourceAggregateEntry::new(
            "claude:/tmp/b.jsonl",
            "fp1",
            vec![daily_contribution_for_test("2026-06-02", "claude", 300)],
        ));

        let contributions =
            cache.contributions_for_dates(&BTreeSet::from(["2026-06-02".to_string()]));

        assert_eq!(contributions.len(), 1);
        assert_eq!(contributions[0].totals.tokens, 500);
        assert_eq!(contributions[0].clients.len(), 2);
    }
}
