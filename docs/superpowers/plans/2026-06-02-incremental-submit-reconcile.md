# Incremental Submit Reconcile Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce background submit memory by avoiding high-frequency full-history scans while preserving server-side totals.

**Architecture:** Keep manual `tokens submit` as a full-history reconciliation path. Add a background submit path that only submits affected day/client rows after local fingerprint checks, and run a full reconciliation on a weekly cadence or when the local cache schema changes. The server already merges incoming rows by same user/device/day and recalculates account totals from `daily_breakdown`, so the CLI must submit complete day/client snapshots, not token deltas.

**Tech Stack:** Rust CLI/core, serde/bincode local cache, existing Next.js `/api/submit` merge contract, existing Rust unit tests.

---

### Task 1: Pin The Submit Contract

**Files:**
- Modify: `crates/tokscale-cli/src/main.rs`
- Test: `crates/tokscale-cli/src/main.rs`

- [ ] **Step 1: Write the failing test**

Add a unit test near the submit payload tests that proves a partial background payload can omit global `time_metrics` and day-level `activeTimeMs` while still carrying day/client snapshots.

```rust
#[test]
fn partial_submit_payload_omits_time_metrics() {
    let mut graph = graph_result_with_contributions(vec![daily_contribution(
        "2026-06-02",
        42,
        0.12,
        "codex",
        "model-b",
    )]);
    graph.contributions[0].active_time_ms = Some(60_000);
    graph.time_metrics = Some(tokscale_core::TimeMetrics {
        total_active_time_ms: 60_000,
        total_wall_time_ms: 60_000,
        longest_continuous_ms: 60_000,
        max_concurrent_sessions: 1,
        session_count: 1,
    });

    let payload = to_ts_token_contribution_data_for_submit(&graph, None, SubmitPayloadMode::Partial);

    assert_eq!(payload.contributions.len(), 1);
    assert!(payload.time_metrics.is_none());
    assert!(payload.contributions[0].active_time_ms.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p tokscale-cli partial_submit_payload_omits_time_metrics --locked
```

Expected: fail because `SubmitPayloadMode` and `to_ts_token_contribution_data_for_submit` do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Add this enum and wrapper near `to_ts_token_contribution_data`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubmitPayloadMode {
    Full,
    Partial,
}

fn to_ts_token_contribution_data_for_submit(
    graph: &tokscale_core::GraphResult,
    device: Option<&device::SubmitDevice>,
    mode: SubmitPayloadMode,
) -> TsTokenContributionData {
    let mut payload = to_ts_token_contribution_data(graph, device);
    if mode == SubmitPayloadMode::Partial {
        payload.time_metrics = None;
        for contribution in &mut payload.contributions {
            contribution.active_time_ms = None;
        }
    }
    payload
}
```

Update the existing full submit call to use `SubmitPayloadMode::Full`.

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test -p tokscale-cli partial_submit_payload_omits_time_metrics --locked
```

Expected: pass.

### Task 2: Add Reconcile State

**Files:**
- Create: `crates/tokscale-cli/src/commands/reconcile_state.rs`
- Modify: `crates/tokscale-cli/src/commands/mod.rs`
- Test: `crates/tokscale-cli/src/commands/reconcile_state.rs`

- [ ] **Step 1: Write the failing tests**

Create tests for loading missing state and deciding weekly full reconciliation.

```rust
#[test]
fn load_missing_reconcile_state_returns_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reconcile-state.json");

    let state = ReconcileState::load_from_path(&path).unwrap();

    assert!(state.last_full_reconcile_at.is_none());
    assert_eq!(state.cache_schema_version, CURRENT_RECONCILE_CACHE_SCHEMA_VERSION);
}

#[test]
fn should_full_reconcile_after_seven_days() {
    let state = ReconcileState {
        last_full_reconcile_at: Some("2026-06-01T00:00:00Z".to_string()),
        cache_schema_version: CURRENT_RECONCILE_CACHE_SCHEMA_VERSION,
    };
    let now = chrono::DateTime::parse_from_rfc3339("2026-06-08T00:00:01Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    assert!(state.should_full_reconcile(now));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p tokscale-cli reconcile_state --locked
```

Expected: fail because the module does not exist.

- [ ] **Step 3: Write minimal implementation**

Implement JSON state under `paths::get_cache_dir().join("reconcile-state.json")`:

```rust
pub const CURRENT_RECONCILE_CACHE_SCHEMA_VERSION: u32 = 1;
const FULL_RECONCILE_INTERVAL_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileState {
    pub last_full_reconcile_at: Option<String>,
    pub cache_schema_version: u32,
}

impl Default for ReconcileState {
    fn default() -> Self {
        Self {
            last_full_reconcile_at: None,
            cache_schema_version: CURRENT_RECONCILE_CACHE_SCHEMA_VERSION,
        }
    }
}

impl ReconcileState {
    pub fn load_from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn should_full_reconcile(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        if self.cache_schema_version != CURRENT_RECONCILE_CACHE_SCHEMA_VERSION {
            return true;
        }
        let Some(last) = &self.last_full_reconcile_at else {
            return true;
        };
        let Ok(last) = chrono::DateTime::parse_from_rfc3339(last) else {
            return true;
        };
        now.signed_duration_since(last.with_timezone(&chrono::Utc)).num_days()
            >= FULL_RECONCILE_INTERVAL_DAYS
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test -p tokscale-cli reconcile_state --locked
```

Expected: pass.

### Task 3: Build The Incremental Aggregate Cache

**Files:**
- Create: `crates/tokscale-core/src/aggregate_cache.rs`
- Modify: `crates/tokscale-core/src/lib.rs`
- Modify: `crates/tokscale-core/src/aggregator.rs`
- Test: `crates/tokscale-core/src/aggregate_cache.rs`

- [ ] **Step 1: Write the failing tests**

Add tests proving a changed source replaces only that source's contribution and returns affected dates.

```rust
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

    assert_eq!(affected, ["2026-06-01".to_string(), "2026-06-02".to_string()].into());
    assert_eq!(cache.contributions_for_dates(&affected).len(), 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p tokscale-core aggregate_cache --locked
```

Expected: fail because the module does not exist.

- [ ] **Step 3: Write minimal implementation**

Implement a schema-versioned cache that stores per-source `Vec<DailyContribution>` plus a source fingerprint string. Add an aggregator helper:

```rust
pub fn merge_daily_contributions(contributions: Vec<DailyContribution>) -> Vec<DailyContribution>
```

This helper should merge days and client/model rows with the same keys, then recalculate totals and intensity. Reuse the existing accumulator behavior instead of duplicating arithmetic.

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test -p tokscale-core aggregate_cache --locked
cargo test -p tokscale-core aggregate_by_date --locked
```

Expected: pass.

### Task 4: Route Background Submit Through Incremental Mode

**Files:**
- Modify: `crates/tokscale-cli/src/main.rs`
- Modify: `crates/tokscale-core/src/lib.rs`
- Test: `crates/tokscale-cli/src/main.rs`

- [ ] **Step 1: Write the failing tests**

Add a test that the serve path chooses partial mode when reconciliation is not due and full mode when it is due.

```rust
#[test]
fn serve_submit_mode_uses_weekly_full_reconcile_gate() {
    let fresh = commands::reconcile_state::ReconcileState {
        last_full_reconcile_at: Some("2026-06-01T00:00:00Z".to_string()),
        cache_schema_version: commands::reconcile_state::CURRENT_RECONCILE_CACHE_SCHEMA_VERSION,
    };
    let soon = chrono::DateTime::parse_from_rfc3339("2026-06-02T00:00:00Z").unwrap().with_timezone(&chrono::Utc);
    let late = chrono::DateTime::parse_from_rfc3339("2026-06-08T00:00:01Z").unwrap().with_timezone(&chrono::Utc);

    assert_eq!(background_submit_mode(&fresh, soon), SubmitPayloadMode::Partial);
    assert_eq!(background_submit_mode(&fresh, late), SubmitPayloadMode::Full);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p tokscale-cli serve_submit_mode_uses_weekly_full_reconcile_gate --locked
```

Expected: fail because `background_submit_mode` does not exist.

- [ ] **Step 3: Write minimal implementation**

Add a `run_background_submit_command` wrapper used by `run_serve`. It should:

1. Load reconcile state.
2. Choose `Full` if weekly reconcile is due.
3. Choose `Partial` otherwise.
4. For `Partial`, call a new core function that returns a `GraphResult` built from affected dates in the aggregate cache.
5. For `Full`, call existing `generate_graph`, then rebuild aggregate cache and update `last_full_reconcile_at`.

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
cargo test -p tokscale-cli serve_submit_mode_uses_weekly_full_reconcile_gate --locked
cargo test -p tokscale-cli submit --locked
```

Expected: pass.

### Task 5: Verify Memory Behavior On Bonny's Machine

**Files:**
- No source edits.

- [ ] **Step 1: Build the CLI**

Run:

```bash
cargo build -p tokscale-cli --locked
```

Expected: build completes with exit code 0.

- [ ] **Step 2: Establish full-scan baseline**

Run:

```bash
/usr/bin/time -l target/debug/tokens submit --dry-run
```

Expected: full scan still reports the known high peak memory on a large history machine.

- [ ] **Step 3: Warm the aggregate cache**

Run:

```bash
target/debug/tokens submit --dry-run
```

Expected: cache file exists and contains source aggregate entries.

- [ ] **Step 4: Measure partial background submit**

Run:

```bash
/usr/bin/time -l target/debug/tokens serve --interval 1
```

Stop after one submit cycle.

Expected: the partial path avoids full-history message materialization and peak RSS is materially lower than the full-scan baseline.

---

## Self-Review

- Spec coverage: covers high-frequency incremental submit, weekly full reconciliation, and the no-delta rule needed for server correctness.
- Placeholder scan: no TODO/TBD placeholders.
- Scope check: this is intentionally split into contract, state, aggregate cache, background routing, and machine verification so each task can be tested independently.
