# Companion Summary Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a compact local JSON summary that future menu bar, Raycast, local cockpit, and mobile-sync surfaces can read without scanning raw session history.

**Architecture:** Add a CLI-side `companion_summary` module that owns the file path, schema, read/write helpers, compact label formatting, and graph-to-summary conversion. Background submit writes the summary after it already scanned local sessions; `tokens status --json` and a lightweight `tokens companion-summary --json` reader expose the cached summary without triggering another scan.

**Tech Stack:** Rust CLI, serde JSON, existing `tokscale_core::GraphResult`, existing `submit-history.jsonl`, existing cache directory helpers, existing Rust unit tests.

---

## Scope

This plan implements Phase 1 from `docs/superpowers/specs/2026-06-04-menu-bar-companion-design.md`: the local summary contract. It does not build the native macOS menu bar UI, Raycast extension, local cockpit, or mobile app.

The native menu bar app should not be started until this summary exists, because otherwise the UI will be tempted to rescan sessions on open.

## File Structure

- Create: `crates/tokscale-cli/src/commands/companion_summary.rs`
  - Owns `companion-summary.json`, schema structs, stale calculation, compact labels, graph conversion, atomic writes, and JSON reader output.
- Modify: `crates/tokscale-cli/src/commands/mod.rs`
  - Exports the new module.
- Modify: `crates/tokscale-cli/src/commands/status.rs`
  - Adds a `companion` object to `tokens status --json` and a short text line when a summary exists.
- Modify: `crates/tokscale-cli/src/main.rs`
  - Adds `tokens companion-summary --json`.
  - Writes the companion summary from the existing submit scan path.

Do not create a native app or web UI in this plan. Do not add a second local scan just to populate the summary.

## Data Contract

The first schema is intentionally compact:

```json
{
  "version": 1,
  "generatedAt": "2026-06-04T00:00:00Z",
  "stale": false,
  "staleReason": null,
  "collapsed": {
    "metric": "todayCost",
    "label": "$1.24",
    "state": "normal"
  },
  "today": {
    "date": "2026-06-04",
    "costUsd": 1.24,
    "tokens": 18000000,
    "messages": 42
  },
  "totals": {
    "costUsd": 23.91,
    "tokens": 35202912831,
    "activeDays": 120,
    "clients": ["claude", "codex"],
    "models": 18
  },
  "top": {
    "client": "codex",
    "model": "gpt-5"
  },
  "latestSubmit": {
    "status": "success",
    "finishedAt": "2026-06-04T00:00:05Z",
    "submissionId": "sub_123"
  },
  "health": {
    "summaryPath": "/Users/example/Library/Caches/tokens/companion-summary.json",
    "lastScanDurationMs": 1800,
    "warnings": []
  },
  "accuracy": {
    "confidence": "medium",
    "sourceKinds": ["local-scan", "estimated-pricing"],
    "warnings": []
  }
}
```

Rules:

- `collapsed.label` must stay short enough for a menu bar: usually 4 to 8 visible characters.
- `tokens status --json` must read this file only; it must not generate the summary by scanning sessions.
- `tokens companion-summary --json` must read this file only; it must not generate the summary by scanning sessions.
- Raw prompts, completions, credentials, and chat content must not be serialized.
- If the file is missing, reader commands report `latest: null` instead of failing.
- If `generatedAt` is older than 2 hours, readers mark the summary as stale and keep the last label displayable.

---

### Task 1: Add Companion Summary Module

**Files:**
- Create: `crates/tokscale-cli/src/commands/companion_summary.rs`
- Modify: `crates/tokscale-cli/src/commands/mod.rs`

- [ ] **Step 1: Write failing tests for path, missing read, stale read, and compact labels**

Create `crates/tokscale-cli/src/commands/companion_summary.rs` with the tests first:

```rust
use crate::paths;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const COMPANION_SUMMARY_SCHEMA_VERSION: u32 = 1;
const COMPANION_SUMMARY_FILE_NAME: &str = "companion-summary.json";
const STALE_AFTER_SECONDS: i64 = 2 * 60 * 60;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_missing_companion_summary_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("companion-summary.json");

        let summary = read_from_path(&path).unwrap();

        assert!(summary.is_none());
    }

    #[test]
    fn mark_stale_flags_old_summary_without_changing_label() {
        let mut summary = sample_summary("2026-06-04T00:00:00Z");
        mark_stale(
            &mut summary,
            chrono::DateTime::parse_from_rfc3339("2026-06-04T02:00:01Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );

        assert!(summary.stale);
        assert_eq!(summary.stale_reason.as_deref(), Some("summary-older-than-2h"));
        assert_eq!(summary.collapsed.label, "$1.24");
    }

    #[test]
    fn format_compact_cost_keeps_menu_bar_label_short() {
        assert_eq!(format_compact_cost(0.0), "$0.00");
        assert_eq!(format_compact_cost(1.236), "$1.24");
        assert_eq!(format_compact_cost(123.8), "$124");
        assert_eq!(format_compact_cost(1200.0), "$1.2K");
    }

    #[test]
    fn format_compact_tokens_keeps_menu_bar_label_short() {
        assert_eq!(format_compact_tokens(42), "42");
        assert_eq!(format_compact_tokens(12_340), "12K");
        assert_eq!(format_compact_tokens(18_000_000), "18M");
        assert_eq!(format_compact_tokens(3_520_000_000), "3.5B");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p tokscale-cli companion_summary --locked
```

Expected: fail because `read_from_path`, `mark_stale`, `sample_summary`, `format_compact_cost`, and `format_compact_tokens` do not exist.

- [ ] **Step 3: Add the data model and reader/writer helpers**

Add this implementation above the test module:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSummary {
    pub version: u32,
    pub generated_at: String,
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
    pub collapsed: CompanionCollapsed,
    pub today: CompanionToday,
    pub totals: CompanionTotals,
    pub top: CompanionTop,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_submit: Option<CompanionLatestSubmit>,
    pub health: CompanionHealth,
    pub accuracy: CompanionAccuracy,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionCollapsed {
    pub metric: String,
    pub label: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionToday {
    pub date: String,
    pub cost_usd: f64,
    pub tokens: i64,
    pub messages: i32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionTotals {
    pub cost_usd: f64,
    pub tokens: i64,
    pub active_days: i32,
    pub clients: Vec<String>,
    pub models: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionTop {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionLatestSubmit {
    pub status: String,
    pub finished_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submission_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionHealth {
    pub summary_path: String,
    pub last_scan_duration_ms: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionAccuracy {
    pub confidence: String,
    pub source_kinds: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn summary_path() -> PathBuf {
    paths::get_cache_dir().join(COMPANION_SUMMARY_FILE_NAME)
}

pub fn read_latest() -> Result<Option<CompanionSummary>> {
    let mut summary = read_from_path(&summary_path())?;
    if let Some(summary) = &mut summary {
        mark_stale(summary, chrono::Utc::now());
    }
    Ok(summary)
}

pub(crate) fn read_from_path(path: &Path) -> Result<Option<CompanionSummary>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read companion summary at {}", path.display()))?;
    let summary = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse companion summary at {}", path.display()))?;
    Ok(Some(summary))
}

pub fn write_latest(summary: &CompanionSummary) -> Result<()> {
    write_to_path(&summary_path(), summary)
}

pub(crate) fn write_to_path(path: &Path, summary: &CompanionSummary) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create companion summary dir {}", parent.display()))?;
    }
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, serde_json::to_vec_pretty(summary)?)
        .with_context(|| format!("failed to write companion summary at {}", tmp_path.display()))?;
    tokscale_core::fs_atomic::replace_file(&tmp_path, path).with_context(|| {
        format!(
            "failed to replace companion summary {} with {}",
            path.display(),
            tmp_path.display()
        )
    })
}

pub(crate) fn mark_stale(summary: &mut CompanionSummary, now: chrono::DateTime<chrono::Utc>) {
    let Ok(generated_at) = chrono::DateTime::parse_from_rfc3339(&summary.generated_at) else {
        summary.stale = true;
        summary.stale_reason = Some("invalid-generated-at".to_string());
        summary.collapsed.state = "stale".to_string();
        return;
    };

    if now.signed_duration_since(generated_at.with_timezone(&chrono::Utc)).num_seconds()
        > STALE_AFTER_SECONDS
    {
        summary.stale = true;
        summary.stale_reason = Some("summary-older-than-2h".to_string());
        summary.collapsed.state = "stale".to_string();
    }
}

pub(crate) fn format_compact_cost(cost: f64) -> String {
    if cost >= 1000.0 {
        format!("${:.1}K", cost / 1000.0)
    } else if cost >= 100.0 {
        format!("${:.0}", cost)
    } else {
        format!("${cost:.2}")
    }
}

pub(crate) fn format_compact_tokens(tokens: i64) -> String {
    if tokens >= 1_000_000_000 {
        format!("{:.1}B", tokens as f64 / 1_000_000_000.0)
    } else if tokens >= 1_000_000 {
        format!("{:.0}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.0}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}
```

Add this test-only helper inside the test module:

```rust
fn sample_summary(generated_at: &str) -> CompanionSummary {
    CompanionSummary {
        version: COMPANION_SUMMARY_SCHEMA_VERSION,
        generated_at: generated_at.to_string(),
        stale: false,
        stale_reason: None,
        collapsed: CompanionCollapsed {
            metric: "todayCost".to_string(),
            label: "$1.24".to_string(),
            state: "normal".to_string(),
        },
        today: CompanionToday {
            date: "2026-06-04".to_string(),
            cost_usd: 1.24,
            tokens: 18_000_000,
            messages: 42,
        },
        totals: CompanionTotals {
            cost_usd: 23.91,
            tokens: 35_202_912_831,
            active_days: 120,
            clients: vec!["codex".to_string()],
            models: 18,
        },
        top: CompanionTop {
            client: Some("codex".to_string()),
            model: Some("gpt-5".to_string()),
        },
        latest_submit: None,
        health: CompanionHealth {
            summary_path: "/tmp/companion-summary.json".to_string(),
            last_scan_duration_ms: 1800,
            warnings: Vec::new(),
        },
        accuracy: CompanionAccuracy {
            confidence: "medium".to_string(),
            source_kinds: vec!["local-scan".to_string(), "estimated-pricing".to_string()],
            warnings: Vec::new(),
        },
    }
}
```

Export the module in `crates/tokscale-cli/src/commands/mod.rs`:

```rust
pub mod companion_summary;
pub mod reconcile_state;
pub mod status;
pub mod submit_history;
pub mod usage;
pub mod wrapped;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
cargo test -p tokscale-cli companion_summary --locked
```

Expected: pass.

---

### Task 2: Convert Existing Graph Output Into Companion Summary

**Files:**
- Modify: `crates/tokscale-cli/src/commands/companion_summary.rs`

- [ ] **Step 1: Write failing graph conversion tests**

Add tests inside `companion_summary.rs`:

```rust
#[test]
fn summary_from_graph_uses_today_for_collapsed_cost() {
    let graph = graph_result_for_test(vec![
        daily_contribution_for_test("2026-06-03", "claude", "claude-sonnet", 1000, 0.50, 3),
        daily_contribution_for_test("2026-06-04", "codex", "gpt-5", 18_000_000, 1.236, 42),
    ]);
    let latest_submit = CompanionLatestSubmit {
        status: "success".to_string(),
        finished_at: "2026-06-04T00:00:05Z".to_string(),
        submission_id: Some("sub_test".to_string()),
    };

    let summary = from_graph(
        &graph,
        Some(latest_submit),
        "2026-06-04",
        "/tmp/companion-summary.json",
    );

    assert_eq!(summary.collapsed.metric, "todayCost");
    assert_eq!(summary.collapsed.label, "$1.24");
    assert_eq!(summary.today.date, "2026-06-04");
    assert_eq!(summary.today.tokens, 18_000_000);
    assert_eq!(summary.today.messages, 42);
    assert_eq!(summary.top.client.as_deref(), Some("codex"));
    assert_eq!(summary.top.model.as_deref(), Some("gpt-5"));
    assert_eq!(summary.latest_submit.as_ref().unwrap().submission_id.as_deref(), Some("sub_test"));
}

#[test]
fn summary_from_graph_uses_zero_today_when_today_is_absent() {
    let graph = graph_result_for_test(vec![daily_contribution_for_test(
        "2026-06-03",
        "claude",
        "claude-sonnet",
        1000,
        0.50,
        3,
    )]);

    let summary = from_graph(&graph, None, "2026-06-04", "/tmp/companion-summary.json");

    assert_eq!(summary.collapsed.label, "$0.00");
    assert_eq!(summary.today.date, "2026-06-04");
    assert_eq!(summary.today.tokens, 0);
    assert_eq!(summary.today.messages, 0);
    assert_eq!(summary.top.client.as_deref(), Some("claude"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p tokscale-cli companion_summary --locked
```

Expected: fail because `from_graph`, `graph_result_for_test`, and `daily_contribution_for_test` do not exist.

- [ ] **Step 3: Add graph conversion implementation**

Add this function:

```rust
pub fn from_graph(
    graph: &tokscale_core::GraphResult,
    latest_submit: Option<CompanionLatestSubmit>,
    today_date: &str,
    summary_path: &str,
) -> CompanionSummary {
    let today = graph
        .contributions
        .iter()
        .find(|day| day.date == today_date);
    let today_cost = today.map(|day| day.totals.cost).unwrap_or(0.0);
    let today_tokens = today.map(|day| day.totals.tokens).unwrap_or(0);
    let today_messages = today.map(|day| day.totals.messages).unwrap_or(0);
    let top_client = top_client(graph);
    let top_model = top_model(graph);
    let accuracy = tokscale_core::accuracy_report_for_graph(graph);

    CompanionSummary {
        version: COMPANION_SUMMARY_SCHEMA_VERSION,
        generated_at: graph.meta.generated_at.clone(),
        stale: false,
        stale_reason: None,
        collapsed: CompanionCollapsed {
            metric: "todayCost".to_string(),
            label: format_compact_cost(today_cost),
            state: if today_tokens > 0 { "normal" } else { "idle" }.to_string(),
        },
        today: CompanionToday {
            date: today_date.to_string(),
            cost_usd: today_cost,
            tokens: today_tokens,
            messages: today_messages,
        },
        totals: CompanionTotals {
            cost_usd: graph.summary.total_cost,
            tokens: graph.summary.total_tokens,
            active_days: graph.summary.active_days,
            clients: graph.summary.clients.clone(),
            models: graph.summary.models.len(),
        },
        top: CompanionTop {
            client: top_client,
            model: top_model,
        },
        latest_submit,
        health: CompanionHealth {
            summary_path: summary_path.to_string(),
            last_scan_duration_ms: graph.meta.processing_time_ms,
            warnings: Vec::new(),
        },
        accuracy: CompanionAccuracy {
            confidence: accuracy_confidence_label(accuracy.confidence).to_string(),
            source_kinds: accuracy
                .sources
                .iter()
                .map(|source| accuracy_source_kind_label(source.kind).to_string())
                .collect(),
            warnings: accuracy.warnings.clone(),
        },
    }
}

fn accuracy_confidence_label(confidence: tokscale_core::AccuracyConfidence) -> &'static str {
    match confidence {
        tokscale_core::AccuracyConfidence::High => "high",
        tokscale_core::AccuracyConfidence::Medium => "medium",
        tokscale_core::AccuracyConfidence::Low => "low",
    }
}

fn accuracy_source_kind_label(kind: tokscale_core::AccuracySourceKind) -> &'static str {
    match kind {
        tokscale_core::AccuracySourceKind::LocalScan => "local-scan",
        tokscale_core::AccuracySourceKind::ProviderOfficial => "provider-official",
        tokscale_core::AccuracySourceKind::EstimatedPricing => "estimated-pricing",
        tokscale_core::AccuracySourceKind::CustomPricing => "custom-pricing",
        tokscale_core::AccuracySourceKind::SubmittedServer => "submitted-server",
        tokscale_core::AccuracySourceKind::Unknown => "unknown",
    }
}

fn top_client(graph: &tokscale_core::GraphResult) -> Option<String> {
    let mut totals = std::collections::HashMap::<String, f64>::new();
    for day in &graph.contributions {
        for client in &day.clients {
            *totals.entry(client.client.clone()).or_default() += client.cost;
        }
    }
    totals
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(client, _)| client)
}

fn top_model(graph: &tokscale_core::GraphResult) -> Option<String> {
    let mut totals = std::collections::HashMap::<String, f64>::new();
    for day in &graph.contributions {
        for client in &day.clients {
            *totals.entry(client.model_id.clone()).or_default() += client.cost;
        }
    }
    totals
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(model, _)| model)
}
```

Add these test helpers inside the test module:

```rust
fn daily_contribution_for_test(
    date: &str,
    client: &str,
    model: &str,
    tokens: i64,
    cost: f64,
    messages: i32,
) -> tokscale_core::DailyContribution {
    tokscale_core::DailyContribution {
        date: date.to_string(),
        totals: tokscale_core::DailyTotals {
            tokens,
            cost,
            messages,
        },
        intensity: 1,
        token_breakdown: tokscale_core::TokenBreakdown {
            input: tokens,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        clients: vec![tokscale_core::ClientContribution {
            client: client.to_string(),
            model_id: model.to_string(),
            provider_id: "openai".to_string(),
            tokens: tokscale_core::TokenBreakdown {
                input: tokens,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            cost,
            messages,
        }],
        active_time_ms: None,
    }
}

fn graph_result_for_test(
    contributions: Vec<tokscale_core::DailyContribution>,
) -> tokscale_core::GraphResult {
    let summary = tokscale_core::calculate_summary(&contributions);
    let years = tokscale_core::calculate_years(&contributions);
    tokscale_core::GraphResult {
        meta: tokscale_core::GraphMeta {
            generated_at: "2026-06-04T00:00:00Z".to_string(),
            version: "3.0.3-test".to_string(),
            date_range_start: contributions
                .first()
                .map(|day| day.date.clone())
                .unwrap_or_default(),
            date_range_end: contributions
                .last()
                .map(|day| day.date.clone())
                .unwrap_or_default(),
            processing_time_ms: 1800,
        },
        summary,
        years,
        contributions,
        time_metrics: None,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:

```bash
cargo test -p tokscale-cli companion_summary --locked
```

Expected: pass.

---

### Task 3: Write Summary From Existing Submit Flow

**Files:**
- Modify: `crates/tokscale-cli/src/main.rs`

- [ ] **Step 1: Write a focused unit test for latest-submit conversion**

Add this test near the existing submit history tests in `main.rs`:

```rust
#[test]
fn companion_latest_submit_from_history_entry_keeps_public_fields_only() {
    let entry = commands::submit_history::SubmitHistoryEntry {
        id: "entry_1".to_string(),
        started_at: "2026-06-04T00:00:00Z".to_string(),
        finished_at: "2026-06-04T00:00:05Z".to_string(),
        status: commands::submit_history::SubmitHistoryStatus::Success,
        clients: vec!["codex".to_string()],
        rows_submitted: 1,
        tokens_submitted: 100,
        cost_submitted: 1.24,
        active_days: 1,
        device_id: Some("dev_test".to_string()),
        submission_id: Some("sub_test".to_string()),
        error_summary: None,
        source_version: "3.0.3-test".to_string(),
    };

    let latest = companion_latest_submit_from_history_entry(&entry);

    assert_eq!(latest.status, "success");
    assert_eq!(latest.finished_at, "2026-06-04T00:00:05Z");
    assert_eq!(latest.submission_id.as_deref(), Some("sub_test"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p tokscale-cli companion_latest_submit_from_history_entry --locked
```

Expected: fail because `companion_latest_submit_from_history_entry` does not exist.

- [ ] **Step 3: Add submit-history to companion conversion helpers**

Add near the submit history helper functions in `main.rs`:

```rust
fn companion_latest_submit_from_history_entry(
    entry: &commands::submit_history::SubmitHistoryEntry,
) -> commands::companion_summary::CompanionLatestSubmit {
    let status = match entry.status {
        commands::submit_history::SubmitHistoryStatus::Success => "success",
        commands::submit_history::SubmitHistoryStatus::Failed => "failed",
        commands::submit_history::SubmitHistoryStatus::Partial => "partial",
    };

    commands::companion_summary::CompanionLatestSubmit {
        status: status.to_string(),
        finished_at: entry.finished_at.clone(),
        submission_id: entry.submission_id.clone(),
    }
}

fn record_companion_summary_from_graph(
    graph: &tokscale_core::GraphResult,
    latest_submit: Option<&commands::submit_history::SubmitHistoryEntry>,
) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let path = commands::companion_summary::summary_path();
    let latest_submit = latest_submit.map(companion_latest_submit_from_history_entry);
    let summary = commands::companion_summary::from_graph(
        graph,
        latest_submit,
        &today,
        &path.display().to_string(),
    );
    let _ = commands::companion_summary::write_latest(&summary);
}
```

- [ ] **Step 4: Write the summary after each non-dry-run submit scan**

In `run_submit_command`, call `record_companion_summary_from_graph` after recording the submit history entry.

For failed HTTP status, place this before `std::process::exit(1)`:

```rust
record_submit_history_entry(&entry);
record_companion_summary_from_graph(&graph_result, Some(&entry));
println!();
std::process::exit(1);
```

For successful submit, place this after `record_submit_history_entry(&entry);`:

```rust
record_submit_history_entry(&entry);
record_companion_summary_from_graph(&graph_result, Some(&entry));
```

For network error, place this before `std::process::exit(1)`:

```rust
record_submit_history_entry(&entry);
record_companion_summary_from_graph(&graph_result, Some(&entry));
std::process::exit(1);
```

Do not write a summary in `dry_run`, because dry-run is often used for manual diagnostics and should not update the menu bar state.

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo test -p tokscale-cli companion_latest_submit_from_history_entry --locked
cargo test -p tokscale-cli companion_summary --locked
```

Expected: both pass.

---

### Task 4: Expose Summary Through Status And A Lightweight Reader Command

**Files:**
- Modify: `crates/tokscale-cli/src/commands/status.rs`
- Modify: `crates/tokscale-cli/src/main.rs`

- [ ] **Step 1: Write status JSON helper tests**

Add this test to `status.rs`:

```rust
#[test]
fn status_companion_report_keeps_missing_summary_non_fatal() {
    let companion = build_status_companion("/tmp/companion-summary.json".to_string(), None);

    assert_eq!(companion.summary_path, "/tmp/companion-summary.json");
    assert!(companion.latest.is_none());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p tokscale-cli status_companion --locked
```

Expected: fail because `StatusCompanion` and `build_status_companion` do not exist.

- [ ] **Step 3: Add companion field to status report**

In `status.rs`, update the import:

```rust
use crate::{auth, commands::companion_summary, commands::submit_history, device, paths};
```

Add the status struct:

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusCompanion {
    summary_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<companion_summary::CompanionSummary>,
}
```

Add it to `StatusReport`:

```rust
companion: StatusCompanion,
```

Add the helper:

```rust
fn build_status_companion(
    summary_path: String,
    latest: Option<companion_summary::CompanionSummary>,
) -> StatusCompanion {
    StatusCompanion {
        summary_path,
        latest,
    }
}
```

In `build_status_report`, read the summary:

```rust
let companion_summary_path = companion_summary::summary_path();
let companion_latest = companion_summary::read_latest().unwrap_or(None);
```

Set the report field:

```rust
companion: build_status_companion(
    companion_summary_path.display().to_string(),
    companion_latest,
),
```

In text output, after latest submit, add:

```rust
if let Some(companion) = &report.companion.latest {
    let stale = if companion.stale { " stale" } else { "" };
    println!(
        "{}",
        format!(
            "  Companion: {} today ({}, generated {}{})",
            companion.collapsed.label,
            companion.collapsed.metric,
            companion.generated_at,
            stale
        )
        .bright_black()
    );
}
```

- [ ] **Step 4: Add `tokens companion-summary --json`**

In `Commands`, add:

```rust
#[command(about = "Read compact local summary for menu bar and companion apps")]
CompanionSummary {
    #[arg(long, help = "Output as JSON")]
    json: bool,
},
```

In the match arm:

```rust
Some(Commands::CompanionSummary { json }) => run_companion_summary_command(json),
```

Add the command function near other command runners:

```rust
fn run_companion_summary_command(json: bool) -> Result<()> {
    let summary = commands::companion_summary::read_latest()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    match summary {
        Some(summary) => {
            println!("{}", summary.collapsed.label);
            if summary.stale {
                eprintln!(
                    "stale: {}",
                    summary
                        .stale_reason
                        .as_deref()
                        .unwrap_or("summary-stale")
                );
            }
        }
        None => {
            println!("No companion summary yet. Run `tokens submit` once to generate it.");
        }
    }

    Ok(())
}
```

This command must only read `companion-summary.json`. It must not call `generate_graph`, `generate_local_graph_report`, scanner code, parser code, or session code.

- [ ] **Step 5: Run focused tests and command help smoke**

Run:

```bash
cargo test -p tokscale-cli status_companion --locked
cargo test -p tokscale-cli companion_summary --locked
cargo run -p tokscale-cli --bin tokens -- --no-spinner companion-summary --json
```

Expected:

- Tests pass.
- The command prints `null` when the summary file does not exist, or a compact JSON summary when it does.
- The command returns quickly and does not print `Scanning local session data...`.

---

### Task 5: Final Verification

**Files:**
- No additional files.

- [ ] **Step 1: Run the focused test matrix**

Run:

```bash
cargo test -p tokscale-cli companion_summary --locked
cargo test -p tokscale-cli status_companion --locked
cargo test -p tokscale-cli submit_history --locked
```

Expected: all pass.

- [ ] **Step 2: Run the broader affected CLI tests**

Run:

```bash
cargo test -p tokscale-cli --locked
```

Expected: pass.

- [ ] **Step 3: Verify the reader does not scan sessions**

Run:

```bash
cargo run -p tokscale-cli --bin tokens -- --no-spinner companion-summary --json
```

Expected:

- Output is `null` or a JSON summary.
- Output does not include `Scanning local session data...`.
- Runtime should be near-instant because it reads one cache file.

- [ ] **Step 4: Verify status includes companion without scanning**

Run:

```bash
cargo run -p tokscale-cli --bin tokens -- --no-spinner status --json
```

Expected:

- JSON includes `companion.summaryPath`.
- JSON includes `companion.latest` when `companion-summary.json` exists.
- Output does not include `Scanning local session data...`.

- [ ] **Step 5: Manual scan/write smoke**

Only run this if it is acceptable to update the local companion summary:

```bash
tokens --no-spinner submit --dry-run
```

Expected:

- Dry-run does not update `companion-summary.json`.

Then run a real submit only with Bonny's explicit approval:

```bash
tokens --no-spinner submit
tokens --no-spinner companion-summary --json
```

Expected:

- Real submit updates `companion-summary.json`.
- `collapsed.label` is short.
- `health.lastScanDurationMs` matches the scan that already happened during submit.

## Harsh Review

- Most likely bug: `today` uses local date while submit caps future UTC dates. This is acceptable for a local menu bar, but server/profile comparisons must label their time window separately.
- Most likely performance regression: accidentally calling `generate_graph` from `status` or `companion-summary`. The verification explicitly checks that reader commands do not print scan text.
- Most likely schema issue: future `AccuracyConfidence` or `AccuracySourceKind` variants require updating the explicit label matchers in `companion_summary.rs`.
- Most likely product gap: project/session tabs still need richer cached data. Do not fake those in Phase 1; add them in a later summary schema version after the incremental aggregate cache is wired.

## Commit Guidance

Do not commit from the current dirty workspace unless Bonny explicitly asks. If committing is approved later, use one conventional commit:

```bash
git add crates/tokscale-cli/src/commands/companion_summary.rs crates/tokscale-cli/src/commands/mod.rs crates/tokscale-cli/src/commands/status.rs crates/tokscale-cli/src/main.rs
git commit -m "feat(status): add companion summary cache"
```
