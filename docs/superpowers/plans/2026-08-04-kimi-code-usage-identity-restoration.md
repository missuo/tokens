# Kimi Code Usage Identity Restoration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore concrete Kimi Code model and provider identity from ordered request/usage events so synthesized or arbitrary aliases such as `__secondary__` and `cheap` become the real identity, including `grok-4.5 / xai`, without changing token accounting.

**Architecture:** Extend the Kimi Code parser into a single ordered pass over each physical `wire.jsonl`. Keep unmatched requests in per-file state, consume the nearest preceding same-alias request before filtering each usage record, and infer commercial provider ownership from the resolved model before considering the logged protocol. Increment only the Kimi parser cache version so historical Kimi files are reparsed.

**Tech Stack:** Rust 2021, `serde`, `simd-json`, `tempfile`, Cargo workspace, Swift 5.9/XCTest, Swift Package Manager, GitHub CLI.

## Global Constraints

- Use `origin/main` as the baseline.
- Keep PR #7 standalone on `fix/kimi-code-usage-identity`; all commits must remain scoped to the Kimi restoration or its review corrections.
- Implement restoration in the Kimi parser, not aggregation or presentation.
- Process each physical `wire.jsonl` independently; never share pending requests across files.
- Use JSONL line order, not timestamps, for correlation.
- Correlate and consume request state before scope and zero-token filtering.
- Keep top-level `usage.record` authoritative and continue ignoring nested `step.end` usage.
- Preserve token totals, timestamps, message counts, scope filtering, and legacy deduplication.
- Infer provider ownership from the resolved model first.
- Do not infer OpenAI ownership from an OpenAI-compatible protocol alone.
- Use canonical `moonshotai` for Kimi/Moonshot usage.
- Do not change pricing, aggregation, scanners, public report serialization, or Swift presentation for this restoration.
- Do not add Kimi workspace/project attribution.
- Keep `CACHE_FORMAT_VERSION` unchanged at `5`.
- Increment only `ClientId::Kimi` parser version from `2` to `3`.
- Do not modify Cargo manifests; `tempfile = "3"` already exists in `tokens-core` dev-dependencies.

## File Structure

### Files to modify

- `cli/tokens-core/src/sessions/kimi.rs`
  - Deserialize `llm.request` identity fields.
  - Correlate request and usage events in one ordered per-file pass.
  - Resolve real model and canonical provider.
  - Add inline parser-boundary tests.
- `cli/tokens-core/src/message_cache.rs`
  - Increment only the Kimi parser version.
  - Add an inline version-scope test.
- `cli/tokens-core/src/provider_identity.rs`
  - Infer Moonshot-family model names as canonical provider `moonshotai`.
  - Add focused shared-helper coverage.

### Documentation included in the Kimi commit

- `docs/research/2026-08-04-kimi-code-usage-identity.md`
- `docs/superpowers/specs/2026-08-04-kimi-code-usage-identity-restoration-design.md`
- `docs/superpowers/plans/2026-08-04-kimi-code-usage-identity-restoration.md`

### Shared dependency

- `cli/tokens-core/src/provider_identity.rs`
  - Reuse `canonical_provider(raw: &str) -> Option<String>`.
  - Extend `inferred_provider_from_model(model: &str) -> Option<&'static str>` for Moonshot-family models.

---

### Task 1: Protect the standalone branch state and capture a live baseline

**Files:** None.

**Interfaces:**
- Consumes: `origin/main`, the standalone Kimi branch, and installed Kimi history.
- Produces: `/tmp/kimi-identity-before.json` for final accounting comparison.

- [ ] **Step 1: Confirm branch, baseline, and working-tree state**

Run:

```bash
git fetch origin
git status --short --branch
git branch --show-current
git log --oneline --decorate -3
git diff origin/main...HEAD --stat
```

Expected before implementation:

- Branch is `fix/kimi-code-usage-identity`.
- `origin/main` is the baseline, and `HEAD` has no committed Kimi changes beyond that baseline yet.
- Kimi research/spec/plan files may be untracked; no tracked source file is modified.

If the branch is not `fix/kimi-code-usage-identity` or it contains unrelated committed changes relative to `origin/main`, stop before editing.

- [ ] **Step 2: Build the current release CLI**

Run:

```bash
cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli
```

Expected: release build succeeds.

- [ ] **Step 3: Capture the pre-restoration report**

Run:

```bash
cli/target/release/tokens usage --json --period all --force-rescan > /tmp/kimi-identity-before.json
jq '.byModel[] | select(.modelId == "__secondary__" or .modelId == "grok-4.5") | {modelId, providerId, tokens, cost, messages, clients}' /tmp/kimi-identity-before.json
```

Expected on the affected local history:

- A `__secondary__` model bucket is present.
- The affected usage is not yet represented as `grok-4.5 / xai`.

This is a live baseline, not an automated RED test.

---

### Task 2: Add Kimi parser-boundary tests

**Files:**
- Modify: `cli/tokens-core/src/sessions/kimi.rs` after the existing parser helpers.

**Interfaces:**
- Consumes: `parse_kimi_code_file(path: &Path) -> Vec<UnifiedMessage>` and `parse_kimi_file(path: &Path) -> Vec<UnifiedMessage>`.
- Produces: failing coverage for alias restoration, LIFO retries, pair retirement, filter ordering, file isolation, provider policy, and legacy behavior.

- [ ] **Step 1: Add reusable temporary-file fixture helpers**

Append an inline test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn write_kimi_code_wire(
        temp_dir: &TempDir,
        agent: &str,
        lines: &[String],
    ) -> PathBuf {
        let path = temp_dir
            .path()
            .join("sessions")
            .join("workspace_123")
            .join("session_abc")
            .join("agents")
            .join(agent)
            .join("wire.jsonl");

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut contents = lines.join("\n");
        contents.push('\n');
        fs::write(&path, contents).unwrap();
        path
    }

    fn request(alias: &str, model: &str, provider: &str, time: i64) -> String {
        json!({
            "type": "llm.request",
            "provider": provider,
            "model": model,
            "modelAlias": alias,
            "time": time
        })
        .to_string()
    }

    fn usage(
        model: &str,
        scope: &str,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_write: i64,
        time: i64,
    ) -> String {
        json!({
            "type": "usage.record",
            "model": model,
            "usage": {
                "inputOther": input,
                "output": output,
                "inputCacheRead": cache_read,
                "inputCacheCreation": cache_write
            },
            "usageScope": scope,
            "time": time
        })
        .to_string()
    }

    fn step_end_with_usage(time: i64) -> String {
        json!({
            "type": "context.append_loop_event",
            "event": {
                "type": "step.end",
                "usage": {
                    "inputOther": 10,
                    "output": 5,
                    "inputCacheRead": 2,
                    "inputCacheCreation": 1
                }
            },
            "time": time
        })
        .to_string()
    }

    fn assert_identity(message: &UnifiedMessage, model: &str, provider: &str) {
        assert_eq!(message.model_id, model);
        assert_eq!(message.provider_id, provider);
    }
```

Keep the module open for the tests below.

- [ ] **Step 2: Add the required Grok restoration test**

```rust
    #[test]
    fn kimi_code_secondary_alias_restores_grok_xai() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "grok-4.5", "openai", 1_000),
                usage("__secondary__", "turn", 10, 5, 2, 1, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "grok-4.5", "xai");
        assert_eq!(messages[0].session_id, "session_abc");
        assert_eq!(messages[0].timestamp, 2_000);
        assert_eq!(messages[0].tokens.input, 10);
        assert_eq!(messages[0].tokens.output, 5);
        assert_eq!(messages[0].tokens.cache_read, 2);
        assert_eq!(messages[0].tokens.cache_write, 1);
    }
```

Run RED:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core sessions::kimi::tests::kimi_code_secondary_alias_restores_grok_xai -- --exact --nocapture
```

Expected: test fails because current output remains `__secondary__ / moonshot`.

Add coverage proving correlation is not limited to a reserved alias:

```rust
    #[test]
    fn kimi_code_arbitrary_alias_restores_differing_concrete_model() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("cheap", "grok-4.5", "openai", 1_000),
                usage("cheap", "turn", 6, 3, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "grok-4.5", "xai");
    }

    #[test]
    fn kimi_code_unmatched_arbitrary_alias_is_retained() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[usage("cheap", "turn", 6, 3, 0, 0, 2_000)],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "cheap", "unknown");
    }
```

- [ ] **Step 3: Add retry and pair-boundary tests**

```rust
    #[test]
    fn kimi_code_retry_uses_latest_matching_request() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "claude-sonnet-4", "anthropic", 1_000),
                request("__secondary__", "grok-4.5", "openai", 1_100),
                usage("__secondary__", "turn", 8, 3, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "grok-4.5", "xai");
    }

    #[test]
    fn kimi_code_completed_pair_retires_older_requests() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "claude-sonnet-4", "anthropic", 1_000),
                request("__secondary__", "grok-4.5", "openai", 1_100),
                usage("__secondary__", "turn", 8, 3, 0, 0, 2_000),
                usage("__secondary__", "turn", 7, 2, 0, 0, 3_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 2);
        assert_identity(&messages[0], "grok-4.5", "xai");
        assert_identity(&messages[1], "__secondary__", "unknown");
    }
```

- [ ] **Step 4: Add filter-order and duplicate-usage tests**

```rust
    #[test]
    fn kimi_code_zero_usage_consumes_request_before_omission() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "grok-4.5", "openai", 1_000),
                usage("__secondary__", "turn", 0, 0, 0, 0, 2_000),
                usage("__secondary__", "turn", 9, 4, 0, 0, 3_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_session_usage_consumes_request_before_scope_filter() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "grok-4.5", "openai", 1_000),
                usage("__secondary__", "session", 10, 5, 0, 0, 2_000),
                usage("__secondary__", "turn", 9, 4, 0, 0, 3_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_ignores_duplicate_step_end_usage() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "grok-4.5", "openai", 1_000),
                step_end_with_usage(1_900),
                usage("__secondary__", "turn", 10, 5, 2, 1, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "grok-4.5", "xai");
        assert_eq!(messages[0].tokens.total(), 18);
    }
```

- [ ] **Step 5: Add file-isolation and provider-policy tests**

```rust
    #[test]
    fn kimi_code_files_do_not_share_request_state() {
        let temp_dir = TempDir::new().unwrap();
        let main_path = write_kimi_code_wire(
            &temp_dir,
            "main",
            &[request("__secondary__", "kimi-k2.5", "openai", 1_000)],
        );
        let child_path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[usage("__secondary__", "turn", 7, 3, 0, 0, 2_000)],
        );

        assert!(parse_kimi_code_file(&main_path).is_empty());
        let child_messages = parse_kimi_code_file(&child_path);

        assert_eq!(child_messages.len(), 1);
        assert_identity(&child_messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_unknown_custom_model_over_openai_protocol_stays_unknown() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "private-model", "openai", 1_000),
                usage("__secondary__", "turn", 4, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "private-model", "unknown");
    }

    #[test]
    fn kimi_code_provider_resolution_prefers_model_ownership() {
        assert_eq!(resolve_kimi_code_provider("grok-4.5", Some("openai")), "xai");
        assert_eq!(resolve_kimi_code_provider("gpt-5.6", Some("openai")), "openai");
        assert_eq!(resolve_kimi_code_provider("claude-sonnet-4", Some("openai")), "anthropic");
        assert_eq!(resolve_kimi_code_provider("gemini-2.5-pro", Some("openai")), "google");
        assert_eq!(resolve_kimi_code_provider("kimi-k2.5", Some("openai")), "moonshotai");
        assert_eq!(resolve_kimi_code_provider("private-model", Some("openai")), "unknown");
    }
```

- [ ] **Step 6: Add malformed-request, concrete Kimi, and legacy regression tests**

```rust
    #[test]
    fn kimi_code_concrete_kimi_model_uses_canonical_moonshot_provider() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "main",
            &[
                "{malformed json".to_string(),
                usage("kimi-code/kimi-k2.5", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "kimi-k2.5", "moonshotai");
    }

    #[test]
    fn kimi_code_moonshot_model_without_provider_hint_is_canonical() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "main",
            &[
                request("fast", "moonshot-v1", "", 1_000),
                usage("fast", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "moonshot-v1", "moonshotai");
    }

    #[test]
    fn kimi_code_logged_kimi_provider_is_canonical_moonshotai() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "main",
            &[
                request("k3", "k3", "kimi", 1_000),
                usage("k3", "turn", 10, 5, 2, 1, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "k3", "moonshotai");
    }

    #[test]
    fn kimi_code_request_without_nonempty_alias_is_not_a_candidate() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                json!({
                    "type": "llm.request",
                    "provider": "openai",
                    "model": "grok-4.5",
                    "modelAlias": "",
                    "time": 1_000
                })
                .to_string(),
                usage("__secondary__", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_request_without_nonempty_normalized_model_is_not_a_candidate() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "kimi-code/", "openai", 1_000),
                usage("__secondary__", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn kimi_code_invalid_newer_same_alias_retires_older_request() {
        let temp_dir = TempDir::new().unwrap();
        let path = write_kimi_code_wire(
            &temp_dir,
            "agent-1",
            &[
                request("__secondary__", "claude-sonnet-4", "anthropic", 1_000),
                request("__secondary__", "kimi-code/", "openai", 1_100),
                usage("__secondary__", "turn", 5, 2, 0, 0, 2_000),
            ],
        );

        let messages = parse_kimi_code_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "__secondary__", "unknown");
    }

    #[test]
    fn legacy_kimi_parsing_keeps_accounting_and_canonical_provider() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let path = root
            .join("sessions")
            .join("group-1")
            .join("session-legacy")
            .join("wire.jsonl");

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(root.join("config.json"), r#"{"model":"kimi-for-coding"}"#).unwrap();
        fs::write(
            &path,
            concat!(
                r#"{"timestamp":1700000000.0,"message":{"type":"StatusUpdate","payload":{"token_usage":{"input_other":10,"output":5,"input_cache_read":2,"input_cache_creation":1},"message_id":"msg-1"}}}"#,
                "\n"
            ),
        )
        .unwrap();

        let messages = parse_kimi_file(&path);

        assert_eq!(messages.len(), 1);
        assert_identity(&messages[0], "kimi-for-coding", "moonshotai");
        assert_eq!(messages[0].session_id, "session-legacy");
        assert_eq!(messages[0].timestamp, 1_700_000_000_000);
        assert_eq!(messages[0].tokens.total(), 18);
    }
}
```

- [ ] **Step 7: Run the complete Kimi test module in RED**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core sessions::kimi::tests -- --nocapture
```

Expected before implementation:

- Compilation fails because `resolve_kimi_code_provider` does not exist.
- After production symbols are introduced, identity tests must fail until ordered correlation is implemented.
- Characterization tests for existing scope/duplicate behavior may already pass.

Do not weaken or remove a test to obtain GREEN.

---

### Task 3: Implement ordered correlation and identity restoration

**Files:**
- Modify: `cli/tokens-core/src/provider_identity.rs` Moonshot-family inference and focused test.
- Modify: `cli/tokens-core/src/sessions/kimi.rs` imports, constants, Kimi Code wire types, helpers, and parser loop.

**Interfaces:**
- Consumes: `provider_identity::canonical_provider` and `provider_identity::inferred_provider_from_model`.
- Produces:
  - `PendingKimiRequest`.
  - `consume_matching_kimi_request`.
  - `resolve_kimi_code_provider`.
  - `resolve_kimi_code_usage_identity`.
  - Correct `UnifiedMessage` identity before aggregation/pricing.

- [ ] **Step 1: Extend shared Moonshot-family inference**

Update the existing Kimi/Moonshot branch in `inferred_provider_from_model` and add focused helper coverage:

```rust
    // Kimi / Moonshot AI — `kimi-k2.5`, `kimi-code`, `moonshot-v1`, etc.
    if contains_delimited(&lower, "kimi") || lower.contains("moonshot") {
        return Some("moonshotai");
    }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_moonshot_provider_from_model_family() {
        assert_eq!(
            inferred_provider_from_model("moonshot-v1"),
            Some("moonshotai")
        );
        assert_eq!(
            inferred_provider_from_model("MoonshotAI/moonshot-v1-128k"),
            Some("moonshotai")
        );
    }
}
```

- [ ] **Step 2: Import provider identity helpers and canonicalize constants**

Add:

```rust
use crate::provider_identity;
```

Replace the provider constant with:

```rust
const DEFAULT_PROVIDER: &str = "moonshotai";
const UNKNOWN_PROVIDER: &str = "unknown";
```

The canonical `DEFAULT_PROVIDER` also fixes legacy Kimi provider identity.

- [ ] **Step 3: Extend Kimi Code deserialization and define pending request state**

Replace the Kimi Code wire type with:

```rust
/// Kimi Code wire.jsonl line structure.
///
/// `llm.request` supplies protocol, concrete model, and runtime alias.
/// `usage.record` supplies the alias/model, token usage, scope, and time.
#[derive(Debug, Deserialize)]
struct KimiCodeWireLine {
    #[serde(rename = "type")]
    line_type: String,
    model: Option<String>,
    #[serde(rename = "modelAlias")]
    model_alias: Option<String>,
    provider: Option<String>,
    usage: Option<TokenUsage>,
    #[serde(rename = "usageScope")]
    usage_scope: Option<String>,
    time: Option<i64>,
}

#[derive(Debug)]
struct PendingKimiRequest {
    model_alias: String,
    model: String,
    provider: Option<String>,
}

impl PendingKimiRequest {
    fn from_wire_line(wire_line: &KimiCodeWireLine) -> Option<Self> {
        let model_alias = wire_line.model_alias.as_deref()?.trim();
        let model = normalize_kimi_code_model(wire_line.model.as_deref()?.trim());
        if model_alias.is_empty() || model.is_empty() {
            return None;
        }

        let provider = wire_line
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        Some(Self {
            model_alias: model_alias.to_string(),
            model,
            provider,
        })
    }
}
```

Do not add request IDs, timestamps, current configuration, or cross-file state.

- [ ] **Step 4: Add matching and identity helpers**

Insert before `parse_kimi_code_file`:

```rust
/// Select the nearest preceding same-alias request and retire the completed
/// request together with every older pending request. Newer requests remain.
fn consume_matching_kimi_request(
    pending_requests: &mut Vec<PendingKimiRequest>,
    usage_model: &str,
) -> Option<PendingKimiRequest> {
    let matched_index = pending_requests
        .iter()
        .rposition(|request| request.model_alias == usage_model)?;

    pending_requests.drain(..=matched_index).next_back()
}

fn resolve_kimi_code_provider(model_id: &str, provider_hint: Option<&str>) -> String {
    if let Some(provider) = provider_identity::inferred_provider_from_model(model_id) {
        return provider.to_string();
    }

    provider_hint
        .and_then(provider_identity::canonical_provider)
        // Kimi can log `openai` as a compatibility protocol for other owners.
        .filter(|provider| provider != "openai")
        .map(|provider| {
            if provider == "kimi" {
                DEFAULT_PROVIDER.to_string()
            } else {
                provider
            }
        })
        .unwrap_or_else(|| UNKNOWN_PROVIDER.to_string())
}

fn resolve_kimi_code_usage_identity(
    recorded_model: &str,
    matched_request: Option<&PendingKimiRequest>,
) -> (String, String) {
    let normalized_recorded_model = normalize_kimi_code_model(recorded_model);
    let model_id = matched_request
        .map(|request| request.model.clone())
        .unwrap_or(normalized_recorded_model);
    let provider_hint = matched_request.and_then(|request| request.provider.as_deref());
    let provider_id = resolve_kimi_code_provider(&model_id, provider_hint);

    (model_id, provider_id)
}
```

Behavior locked by these helpers:

- `rposition` implements nearest-preceding same-alias LIFO matching.
- Draining through the selected index retires the selected request and older failed attempts.
- Requests newer than the selected match remain pending.
- Any exact same-alias match supplies the request's concrete model, including arbitrary aliases such as `cheap`.
- Already-concrete usage remains unchanged when the matched request carries the same model, and unmatched recorded names remain normalized but otherwise intact.
- A newer unusable request is a barrier that retires older pending requests with its same nonempty alias.
- Model-family ownership outranks protocol hints; logged `kimi` canonicalizes to `moonshotai`.
- An unmatched routing alias remains visible and resolves to `unknown` when its name supplies no reliable ownership evidence.

- [ ] **Step 5: Refactor the parser into one ordered pass**

After creating `messages`, add:

```rust
let mut pending_requests: Vec<PendingKimiRequest> = Vec::new();
```

Replace the current usage-only handling with:

```rust
        if wire_line.line_type == "llm.request" {
            let model_alias = wire_line
                .model_alias
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(request) = PendingKimiRequest::from_wire_line(&wire_line) {
                pending_requests.push(request);
            } else if let Some(model_alias) = model_alias {
                // A newer unusable request supersedes older candidates for its
                // alias so later usage cannot revive a failed request.
                pending_requests.retain(|request| request.model_alias != model_alias);
            }
            continue;
        }

        // Top-level usage.record remains authoritative. Nested step.end usage
        // is intentionally ignored to avoid double counting.
        if wire_line.line_type != "usage.record" {
            continue;
        }

        // Correlation and retirement occur before scope and zero-token filters.
        let recorded_model = wire_line.model.as_deref().unwrap_or(DEFAULT_MODEL);
        let matched_request =
            consume_matching_kimi_request(&mut pending_requests, recorded_model);
        let (model_id, provider_id) = resolve_kimi_code_usage_identity(
            recorded_model,
            matched_request.as_ref(),
        );

        if wire_line.usage_scope.as_deref() != Some("turn") {
            continue;
        }

        let Some(tokens) = wire_line
            .usage
            .as_ref()
            .and_then(TokenUsage::to_breakdown)
        else {
            continue;
        };

        let timestamp_ms = wire_line.time.unwrap_or(fallback_timestamp);

        messages.push(UnifiedMessage::new(
            "kimi",
            model_id,
            provider_id,
            session_id.clone(),
            timestamp_ms,
            tokens,
            0.0,
        ));
```

Remove the old fixed-provider Kimi Code construction. Do not change legacy deduplication.

- [ ] **Step 6: Run focused GREEN verification**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core provider_identity::tests -- --nocapture
cargo test --manifest-path cli/Cargo.toml -p tokens-core sessions::kimi::tests::kimi_code_secondary_alias_restores_grok_xai -- --exact --nocapture
cargo test --manifest-path cli/Cargo.toml -p tokens-core sessions::kimi::tests -- --nocapture
```

Expected: all Kimi parser tests pass.

---

### Task 4: Add isolated Kimi cache invalidation

**Files:**
- Modify: `cli/tokens-core/src/message_cache.rs` in `parser_version` and at file end.

**Interfaces:**
- Consumes: `parser_version(ClientId) -> u32` and `CACHE_FORMAT_VERSION`.
- Produces: Kimi parser version `3` while the global cache format remains unchanged; unrelated parser versions are outside this focused invariant test.

- [ ] **Step 1: Add the failing cache-version test**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kimi_parser_version_bump_is_client_scoped() {
        assert_eq!(CACHE_FORMAT_VERSION, 5);
        assert_eq!(parser_version(ClientId::Kimi), 3);
    }
}
```

- [ ] **Step 2: Run cache RED**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core message_cache::tests::kimi_parser_version_bump_is_client_scoped -- --exact --nocapture
```

Expected: failure because Kimi is still version `2` rather than `3`.

- [ ] **Step 3: Increment only the Kimi parser version**

Replace the Kimi arm with:

```rust
// v1->v2: check each token bucket independently when deciding whether
// a usage record is empty, avoiding an overflowing sum.
// v2->v3: correlate Kimi Code usage aliases with preceding request
// records to restore concrete model and provider identity.
ClientId::Kimi => 3,
```

Do not alter `CACHE_FORMAT_VERSION` or another client arm. The focused test intentionally avoids pinning unrelated client versions, which have independent parser histories.

- [ ] **Step 4: Run cache GREEN**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core message_cache::tests::kimi_parser_version_bump_is_client_scoped -- --exact --nocapture
```

Expected: PASS.

---

### Task 5: Format and run complete automated verification

**Files:** No additional source changes unless verification exposes a defect.

**Interfaces:**
- Consumes: completed parser and cache changes.
- Produces: fresh evidence for Rust tests, lint, Swift tests, and release builds.

- [ ] **Step 1: Format Rust and inspect intended source files**

Run:

```bash
cargo fmt --manifest-path cli/Cargo.toml --all
git diff -- cli/tokens-core/src/sessions/kimi.rs cli/tokens-core/src/message_cache.rs
```

Expected: no unrelated formatting changes.

- [ ] **Step 2: Re-run focused tests**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml -p tokens-core sessions::kimi::tests -- --nocapture
cargo test --manifest-path cli/Cargo.toml -p tokens-core message_cache::tests::kimi_parser_version_bump_is_client_scoped -- --exact --nocapture
```

Expected: all focused tests pass.

- [ ] **Step 3: Run the complete Rust workspace tests**

Run:

```bash
cargo test --manifest-path cli/Cargo.toml --workspace
```

Expected: all Rust tests pass.

- [ ] **Step 4: Run formatting and lint checks**

Run:

```bash
cargo fmt --manifest-path cli/Cargo.toml --all -- --check
cargo clippy --manifest-path cli/Cargo.toml --workspace --all-targets -- -D warnings
```

Expected: both commands exit successfully with no warnings.

- [ ] **Step 5: Run complete Swift tests**

Run:

```bash
swift test
```

Expected: all Swift tests pass.

- [ ] **Step 6: Build both release products**

Run:

```bash
cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli
swift build -c release --product TokensMenuBar
```

Expected: both release builds succeed.

---

### Task 6: Verify cache rebuilding and live restored identity

**Files:** None unless live verification exposes a parser defect.

**Interfaces:**
- Consumes: release CLI and installed Kimi history.
- Produces: refreshed and forced reports proving restored identity and unchanged accounting.

- [ ] **Step 1: Exercise ordinary refresh and parser-version invalidation**

Run:

```bash
cli/target/release/tokens usage --json --period all --refresh > /tmp/kimi-identity-refresh.json
jq -e '[.byModel[] | select(.modelId == "__secondary__")] | length == 0' /tmp/kimi-identity-refresh.json
jq -e 'any(.byModel[]; .modelId == "grok-4.5" and .providerId == "xai" and .tokens > 0 and .messages > 0)' /tmp/kimi-identity-refresh.json
jq '.byModel[] | select(.modelId == "grok-4.5" and .providerId == "xai") | {modelId, providerId, tokens, cost, messages, clients}' /tmp/kimi-identity-refresh.json
```

Expected:

- No `__secondary__` model bucket.
- A `grok-4.5 / xai` bucket with positive tokens and messages.
- Cost is positive if the existing pricing dataset includes the applicable Grok rate.

If identity is correct but cost remains zero, investigate existing pricing lookup before changing pricing code; pricing implementation is outside the approved scope.

- [ ] **Step 2: Exercise full cache replacement**

Run:

```bash
cli/target/release/tokens usage --json --period all --force-rescan > /tmp/kimi-identity-after.json
jq -e '[.byModel[] | select(.modelId == "__secondary__")] | length == 0' /tmp/kimi-identity-after.json
jq -e 'any(.byModel[]; .modelId == "grok-4.5" and .providerId == "xai" and .tokens > 0 and .messages > 0)' /tmp/kimi-identity-after.json
```

Expected: both assertions pass.

- [ ] **Step 3: Compare accounting before and after**

Run:

```bash
jq -s -e '
  .[0].summary.totalTokens == .[1].summary.totalTokens
  and .[0].summary.messages == .[1].summary.messages
  and .[0].tokenBreakdown == .[1].tokenBreakdown
' /tmp/kimi-identity-before.json /tmp/kimi-identity-after.json
```

Expected: output `true` and exit status `0`.

Only model/provider grouping and resulting pricing may change. If live Kimi files changed between scans, rerun while Kimi Code is idle and use the fixed automated fixtures as the authoritative regression proof.

- [ ] **Step 4: Restart and visually verify the release Menu Bar**

Run:

```bash
make restart-release
```

Verify in the Model section:

- `__secondary__` is absent.
- `grok-4.5 / xai` is present.
- Counts and cost agree with `/tmp/kimi-identity-after.json`.
- Kimi/Moonshot models use `moonshotai`.

---

### Task 7: Review the complete Kimi change

**Files:**
- Review all files intended for the standalone Kimi commit.

**Interfaces:**
- Consumes: implementation, tests, and documentation.
- Produces: an approved, scope-limited diff ready to commit.

- [ ] **Step 1: Inspect the exact intended change set**

Run:

```bash
git diff --check
git status --short
git diff -- cli/tokens-core/src/sessions/kimi.rs cli/tokens-core/src/message_cache.rs
```

Expected intended Kimi commit files:

```text
cli/tokens-core/src/sessions/kimi.rs
cli/tokens-core/src/message_cache.rs
docs/research/2026-08-04-kimi-code-usage-identity.md
docs/superpowers/specs/2026-08-04-kimi-code-usage-identity-restoration-design.md
docs/superpowers/plans/2026-08-04-kimi-code-usage-identity-restoration.md
```

Do not use `git add -A` or stage an entire documentation directory.

- [ ] **Step 2: Request task-scoped and whole-change code review**

Review against the approved design, with special attention to:

- same-file LIFO correlation;
- pair-boundary retirement;
- correlation before filters;
- provider ownership versus protocol;
- cache-version isolation;
- unchanged token accounting;
- no Kimi-specific aggregation/UI logic.

Fix all Critical and Important findings, rerun affected tests, and obtain a clean re-review before committing.

---

### Task 8: Commit and push the Kimi restoration

**Files:** Exactly the five intended Kimi source/documentation files.

**Interfaces:**
- Consumes: reviewed and verified working tree based on `origin/main`.
- Produces: a standalone Kimi restoration history on `fix/kimi-code-usage-identity` for PR #7; scoped review corrections may follow the implementation commit.

- [ ] **Step 1: Stage only approved files**

Run:

```bash
git add \
  cli/tokens-core/src/sessions/kimi.rs \
  cli/tokens-core/src/message_cache.rs \
  docs/research/2026-08-04-kimi-code-usage-identity.md \
  docs/superpowers/specs/2026-08-04-kimi-code-usage-identity-restoration-design.md \
  docs/superpowers/plans/2026-08-04-kimi-code-usage-identity-restoration.md
```

- [ ] **Step 2: Inspect the staged diff**

Run:

```bash
git diff --cached --check
git diff --cached --stat
git diff --cached
```

Expected:

- Exactly five staged files.
- No Cargo manifest, Swift, pricing, aggregation, scanner, or report-schema changes.
- `CACHE_FORMAT_VERSION` remains `5`.
- Kimi parser version is `3`.

- [ ] **Step 3: Commit**

Run:

```bash
git commit \
  -m "fix: restore Kimi Code usage identity" \
  -m "Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 4: Verify the standalone commit history**

Run:

```bash
git log --oneline origin/main..HEAD
```

Expected: history contains the Kimi implementation commit plus only scoped review or preparation follow-ups; no unrelated commits.

- [ ] **Step 5: Push without force**

Run:

```bash
git push origin HEAD:fix/kimi-code-usage-identity
```

Do not use force push.

---

### Task 9: Update and verify PR #7

**Files:**
- Create temporary `/tmp/pr-7-body.md`; do not commit it.

**Interfaces:**
- Consumes: pushed standalone Kimi branch and fresh verification evidence.
- Produces: a Kimi-only PR #7 description.

- [ ] **Step 1: Confirm the standalone PR**

Run:

```bash
gh pr view 7 --repo HuaileiW/tokens --json number,title,url,headRefName,baseRefName,commits,state
```

Expected: PR #7 is open from `fix/kimi-code-usage-identity` against `main`, and every commit is scoped to the Kimi restoration or its review corrections.

- [ ] **Step 2: Write the updated PR body**

Write `/tmp/pr-7-body.md` with:

```markdown
## Summary

- restore Kimi Code `usage.record` routing aliases from the nearest preceding same-file `llm.request`
- report Grok usage as `grok-4.5 / xai` even when Kimi logged the OpenAI-compatible `openai` protocol
- consume request state before turn-scope and zero-token filtering
- canonicalize Kimi/Moonshot usage as `moonshotai`
- bump only the Kimi source-message parser version so unchanged historical files reparse

## Implementation

Kimi Code files are parsed in physical JSONL order with request state local to one `wire.jsonl`. A usage record consumes the nearest unmatched request with the same alias using LIFO matching, and any exact match can restore a differing concrete model regardless of alias spelling. Completed pairs retire older failed attempts; newer unusable same-alias requests act as barriers that retire stale same-alias candidates; unmatched aliases remain visible and use provider `unknown` when no ownership evidence exists.

Provider ownership is inferred from the resolved concrete model before considering the logged protocol. This prevents Grok and unknown custom OpenAI-compatible models from being attributed to OpenAI. Shared model-family inference recognizes both Kimi and Moonshot names as `moonshotai`, and a logged `kimi` provider hint is canonicalized to the same identity.

No Kimi-specific behavior was added to aggregation, pricing, report serialization, or Swift presentation.

## Verification

- [x] focused Kimi parser tests
- [x] isolated Kimi cache-version test
- [x] complete Rust workspace tests
- [x] Rust formatting and Clippy checks
- [x] complete Swift tests
- [x] release CLI build
- [x] release Menu Bar build
- [x] refreshed and forced live reports contain no `__secondary__` model bucket
- [x] live report contains `grok-4.5 / xai`
- [x] live token totals, message totals, and token breakdown are unchanged
- [x] release Menu Bar restarted and visually verified

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

Mark an item complete only if its corresponding command/check passed.

- [ ] **Step 3: Update the PR description**

Run:

```bash
gh pr edit 7 --repo HuaileiW/tokens --body-file /tmp/pr-7-body.md
```

If GraphQL rate limits block `gh pr edit`, use the GitHub REST API to update PR #7 without changing title, base, or head.

- [ ] **Step 4: Verify the remote PR and final local state**

Run:

```bash
gh pr view 7 --repo HuaileiW/tokens --json title,body,commits,url,state
gh pr checks 7 --repo HuaileiW/tokens
git status --short --branch
```

Expected:

- PR body contains only the Kimi restoration summary and verification and ends with the required Claude Code footer.
- The standalone Kimi commit is present.
- Checks are passing or clearly reported as still in progress.
- Branch is no longer ahead of its remote.
- No Kimi implementation file remains modified or untracked.
- Only unrelated pre-existing untracked documents may remain.
