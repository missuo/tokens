//! Mux (coder/mux) session parser
//!
//! Parses session-usage.json files from ~/.mux/sessions/<workspaceId>/session-usage.json

use super::utils::{file_modified_timestamp_ms, read_file_or_none};
use super::UnifiedMessage;
use crate::{provider_identity, TokenBreakdown};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct MuxSessionUsage {
    #[allow(dead_code)]
    pub version: Option<u32>,
    #[serde(rename = "byModel")]
    pub by_model: Option<HashMap<String, MuxModelUsage>>,
    #[serde(rename = "lastRequest")]
    pub last_request: Option<MuxLastRequest>,
}

#[derive(Debug, Deserialize)]
pub struct MuxModelUsage {
    pub input: Option<MuxTokenBucket>,
    pub cached: Option<MuxTokenBucket>,
    #[serde(rename = "cacheCreate")]
    pub cache_create: Option<MuxTokenBucket>,
    pub output: Option<MuxTokenBucket>,
    pub reasoning: Option<MuxTokenBucket>,
}

#[derive(Debug, Deserialize)]
pub struct MuxTokenBucket {
    pub tokens: Option<i64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct MuxLastRequest {
    #[allow(dead_code)]
    pub model: Option<String>,
    pub timestamp: Option<i64>,
}

/// Parse a mux session-usage.json file.
/// Returns one UnifiedMessage per model entry in byModel.
pub fn parse_mux_file(path: &Path) -> Vec<UnifiedMessage> {
    let Some(data) = read_file_or_none(path) else {
        return vec![];
    };

    let usage: MuxSessionUsage = match serde_json::from_slice(&data) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let timestamp = usage
        .last_request
        .as_ref()
        .and_then(|lr| lr.timestamp)
        .unwrap_or_else(|| file_modified_timestamp_ms(path));

    let session_id = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let by_model = match usage.by_model {
        Some(m) => m,
        None => return vec![],
    };

    by_model
        .into_iter()
        .filter_map(|(model_key, model_usage)| {
            let tokens =
                |b: &Option<MuxTokenBucket>| b.as_ref().and_then(|b| b.tokens).unwrap_or(0).max(0);
            let cost =
                |b: &Option<MuxTokenBucket>| b.as_ref().and_then(|b| b.cost_usd).unwrap_or(0.0);
            let input = tokens(&model_usage.input);
            let cached = tokens(&model_usage.cached);
            let cache_create = tokens(&model_usage.cache_create);
            let output = tokens(&model_usage.output);
            let reasoning = tokens(&model_usage.reasoning);
            let source_cost = cost(&model_usage.input)
                + cost(&model_usage.cached)
                + cost(&model_usage.cache_create)
                + cost(&model_usage.output)
                + cost(&model_usage.reasoning);

            if input == 0 && cached == 0 && cache_create == 0 && output == 0 && reasoning == 0 {
                return None;
            }

            // Workspace-scoped, stable dedup key so an incremental re-parse
            // collapses the same model entry instead of double-counting it,
            // while two workspaces that used the same model stay distinct. The
            // previous positional index was the HashMap iteration position
            // (unstable across re-parses) and made the first model in every
            // workspace file `mux:<model>:0`, colliding across workspaces.
            let dedup_key = Some(format!("mux:{session_id}:{model_key}"));

            // Strip "provider:" prefix for model ID (e.g., "anthropic:claude-opus-4-6" -> "claude-opus-4-6")
            let (provider, model_id) = if model_key.contains(':') {
                let mut parts = model_key.splitn(2, ':');
                let p = parts.next().unwrap_or("").to_string();
                let m = parts.next().unwrap_or(&model_key).to_string();
                (p, m)
            } else {
                (String::new(), model_key)
            };
            let provider = provider_identity::canonical_provider(&provider).unwrap_or(provider);

            let mut message = UnifiedMessage::new_with_dedup(
                "mux",
                model_id,
                provider,
                session_id.clone(),
                timestamp,
                TokenBreakdown {
                    input,
                    output,
                    cache_read: cached,
                    cache_write: cache_create,
                    reasoning,
                },
                source_cost,
                dedup_key,
            );
            message.set_timestamp_provenance(crate::TimestampProvenance::Aggregate);
            Some(message)
        })
        .collect()
}
