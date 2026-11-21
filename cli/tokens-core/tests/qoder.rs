//! End-to-end Qoder / Qoder CN coverage.
//!
//! Every fixture here is synthetic: a temp home with JSONL transcripts, segment
//! logs and a real (tempfile) SQLite database. Nothing reads the developer's
//! `~/.qoder`, and no test depends on Qoder being installed.
//!
//! The token assertions are exact on purpose. The IDE case pins the documented
//! `prompt_tokens` / `cached_tokens` / `completion_tokens` relationship
//! (39005 / 37485 / 67 → 1520 uncached input, 37485 cache read, 67 output,
//! 39072 total) so "cached is already inside prompt" cannot regress silently.

mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokens_core::pricing::{litellm::ModelPricing, PricingService};
use tokens_core::scanner::{scan_all_clients_with_scanner_settings, ScannerSettings};
use tokens_core::{
    parse_local_clients, parse_local_unified_messages_with_pricing,
    parse_local_unified_messages_with_pricing_uncached, ClientId, LocalParseOptions, PathRoot,
    UnifiedMessage,
};

/// Parse a synthetic home with the uncached (in-memory) source cache.
async fn parse_uncached(home: &Path) -> Vec<UnifiedMessage> {
    parse_local_unified_messages_with_pricing_uncached(qoder_options(home), None)
        .await
        .expect("uncached local parse")
}

fn qoder_options(home: &Path) -> LocalParseOptions {
    LocalParseOptions {
        home_dir: Some(home.to_string_lossy().into_owned()),
        use_env_roots: false,
        clients: Some(vec!["qoder".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: ScannerSettings::default(),
    }
}

fn write_jsonl(path: &Path, lines: &[&str]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, lines.join("\n") + "\n").unwrap();
}

fn total_tokens(messages: &[UnifiedMessage]) -> i64 {
    messages.iter().map(|message| message.tokens.total()).sum()
}

fn buckets(messages: &[UnifiedMessage]) -> (i64, i64, i64, i64) {
    messages.iter().fold((0, 0, 0, 0), |accumulator, message| {
        (
            accumulator.0 + message.tokens.input,
            accumulator.1 + message.tokens.output,
            accumulator.2 + message.tokens.cache_read,
            accumulator.3 + message.tokens.cache_write,
        )
    })
}

const INTL_ONE_REQUEST: &str = concat!(
    r#"{"type":"runtime-config","sessionId":"sess-intl","model":"qmodel","timestamp":1794041573406}"#,
    "\n",
    r#"{"type":"user","uuid":"u1","sessionId":"sess-intl","cwd":"/work/intl-app","timestamp":"2026-11-07T10:00:00.000Z","message":{"role":"user","content":"hello"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a1","sessionId":"sess-intl","cwd":"/work/intl-app","timestamp":"2026-11-07T10:00:01.000Z","message":{"id":"msg_intl","role":"assistant","model":"qmodel","content":[{"type":"thinking","thinking":"plan"}]}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a2","sessionId":"sess-intl","cwd":"/work/intl-app","timestamp":"2026-11-07T10:00:05.000Z","message":{"id":"msg_intl","role":"assistant","model":"qmodel","content":[{"type":"text","text":"done"}],"usage":{"input_tokens":1200,"output_tokens":40,"cache_read_input_tokens":800,"cache_creation_input_tokens":100,"cache_creation":{"ephemeral_1h_input_tokens":50},"request_id":"req-intl-1"}}}"#,
    "\n",
);

const CN_ONE_REQUEST: &str = concat!(
    r#"{"type":"runtime-config","sessionId":"sess-cn","model":"dfmodel","timestamp":1794041573406}"#,
    "\n",
    r#"{"type":"user","uuid":"u1","sessionId":"sess-cn","cwd":"C:\\work\\cn-app","timestamp":"2026-11-07T11:00:00.000Z","message":{"role":"user","content":"hello"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a1","sessionId":"sess-cn","cwd":"C:\\work\\cn-app","timestamp":"2026-11-07T11:00:03.000Z","message":{"id":"msg_cn","role":"assistant","model":"dfmodel","content":[{"type":"text","text":"done"}],"usage":{"input_tokens":1000,"output_tokens":20,"cache_read_input_tokens":600,"cache_creation_input_tokens":0,"request_id":"req-cn-1"}}}"#,
    "\n",
);

/// T01 / T02 / T03: both editions are discovered under one client, counted
/// separately, and never deduplicated against each other even when their
/// request ids collide.
#[tokio::test]
async fn test_qoder_both_editions_are_merged_without_cross_deduplication() {
    let home = common::temp_home();
    write_jsonl(
        &home
            .path()
            .join(".qoder/projects/-work-intl-app/sess-intl.jsonl"),
        &[INTL_ONE_REQUEST],
    );
    write_jsonl(
        &home
            .path()
            .join(".qoder-cn/projects/-C-work-cn-app/sess-cn.jsonl"),
        &[CN_ONE_REQUEST],
    );
    // Same request id in the other edition: a different account's request.
    write_jsonl(
        &home
            .path()
            .join(".qoder-cn/projects/-C-work-cn-app/sess-cn-dup.jsonl"),
        &[
            r#"{"type":"assistant","sessionId":"sess-cn-dup","cwd":"C:\\work\\cn-app","timestamp":"2026-11-07T11:05:00.000Z","message":{"id":"msg_cn_dup","role":"assistant","model":"dfmodel","usage":{"input_tokens":1000,"output_tokens":20,"cache_read_input_tokens":600,"request_id":"req-intl-1"}}}"#,
        ],
    );

    let scan = scan_all_clients_with_scanner_settings(
        home.path().to_str().unwrap(),
        &["qoder".to_string()],
        false,
        &ScannerSettings::default(),
    );
    assert_eq!(scan.get(ClientId::Qoder).len(), 3);

    let parsed = parse_local_clients(qoder_options(home.path())).unwrap();
    assert_eq!(parsed.counts.get(ClientId::Qoder), 3);

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 3);
    assert!(messages.iter().all(|message| message.client == "qoder"));

    // (input, output, cache_read, cache_write)
    assert_eq!(
        buckets(&messages),
        (400 + 400 + 400, 40 + 20 + 20, 800 + 600 + 600, 100)
    );
    assert_eq!(total_tokens(&messages), 1200 + 80 + 2000 + 100);

    let models: Vec<&str> = messages.iter().map(|m| m.model_id.as_str()).collect();
    assert!(models.contains(&"qmodel"));
    assert!(models.contains(&"dfmodel"));
}

/// T18: the model a request ran on follows `runtime-config` switches inside one
/// session, and `auto` resolves through the preceding `businessModelKey`.
#[tokio::test]
async fn test_qoder_model_switch_and_auto_resolution() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(".qoder/projects/-repo/sess-switch.jsonl"),
        &[concat!(
            r#"{"type":"runtime-config","sessionId":"sess-switch","model":"qfmodel","timestamp":1794041573406}"#,
            "\n",
            r#"{"type":"user","uuid":"u1","sessionId":"sess-switch","businessModelKey":"qfmodel","timestamp":"2026-11-07T10:00:00.000Z","message":{"role":"user","content":"one"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-switch","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_1","role":"assistant","model":"auto","usage":{"input_tokens":100,"output_tokens":10,"request_id":"req-1"}}}"#,
            "\n",
            r#"{"type":"user","uuid":"u2","sessionId":"sess-switch","timestamp":"2026-11-07T10:01:00.000Z","message":{"role":"user","content":"two"}}"#,
            "\n",
            r#"{"type":"runtime-config","sessionId":"sess-switch","model":"gfmodel","timestamp":1794041600000}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a2","sessionId":"sess-switch","timestamp":"2026-11-07T10:02:00.000Z","message":{"id":"msg_2","role":"assistant","model":"auto","usage":{"input_tokens":200,"output_tokens":20,"request_id":"req-2"}}}"#,
            "\n",
        )],
    );

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].model_id, "qfmodel");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[1].model_id, "gfmodel");
    assert_eq!(messages[1].tokens.input, 200);
    assert!(messages.iter().all(|m| m.provider_id == "qoder"));
    // A switch does not move the request to the other turn's day.
    assert!(messages.iter().all(|m| m.date == "2026-11-07"));
}

/// T04 / T05: fragments of one reply collapse, distinct replies in one session
/// stay separate.
#[tokio::test]
async fn test_qoder_multi_fragment_and_multi_request_session() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(".qoder/projects/-repo/sess-multi.jsonl"),
        &[concat!(
            r#"{"type":"user","uuid":"u1","sessionId":"sess-multi","timestamp":"2026-11-07T10:00:00.000Z","message":{"role":"user","content":"one"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-multi","timestamp":"2026-11-07T10:00:01.000Z","message":{"id":"msg_a","role":"assistant","model":"qmodel","usage":{"input_tokens":1000,"output_tokens":10,"request_id":"req-a"}}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a2","sessionId":"sess-multi","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_a","role":"assistant","model":"qmodel","usage":{"input_tokens":1000,"output_tokens":90,"request_id":"req-a"}}}"#,
            "\n",
            r#"{"type":"user","uuid":"u2","sessionId":"sess-multi","timestamp":"2026-11-07T10:01:00.000Z","message":{"role":"user","content":"two"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a3","sessionId":"sess-multi","timestamp":"2026-11-07T10:01:05.000Z","message":{"id":"msg_b","role":"assistant","model":"qmodel","usage":{"input_tokens":2000,"output_tokens":20,"request_id":"req-b"}}}"#,
            "\n",
        )],
    );

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 1000);
    assert_eq!(messages[0].tokens.output, 90);
    // Measured from the prompt (10:00:00) to the reply's last fragment (10:00:02).
    assert_eq!(messages[0].duration_ms, Some(2000));
    assert_eq!(messages[1].tokens.input, 2000);
    assert_eq!(messages[1].tokens.output, 20);
    assert_eq!(total_tokens(&messages), 3110);
}

/// T06: a forked session replaying the parent's turn counts it once, wherever
/// the two files land in scan order.
#[tokio::test]
async fn test_qoder_forked_session_replay_counts_once() {
    let home = common::temp_home();
    let parent = concat!(
        r#"{"type":"assistant","uuid":"a1","sessionId":"sess-parent","cwd":"/work/app","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_parent","role":"assistant","model":"qmodel","usage":{"input_tokens":500,"output_tokens":120,"cache_read_input_tokens":300,"cache_creation_input_tokens":50,"request_id":"req-parent"}}}"#,
        "\n",
    );
    write_jsonl(
        &home
            .path()
            .join(".qoder/projects/-work-app/sess-parent.jsonl"),
        &[parent],
    );
    write_jsonl(
        &home
            .path()
            .join(".qoder/projects/-work-app/sess-fork.jsonl"),
        &[concat!(
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-fork","cwd":"/work/app","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_parent","role":"assistant","model":"qmodel","usage":{"input_tokens":500,"output_tokens":120,"cache_read_input_tokens":300,"cache_creation_input_tokens":50,"request_id":"req-parent"}}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a2","sessionId":"sess-fork","cwd":"/work/app","timestamp":"2026-11-07T10:01:00.000Z","message":{"id":"msg_fork","role":"assistant","model":"qmodel","usage":{"input_tokens":600,"output_tokens":80,"request_id":"req-fork"}}}"#,
            "\n",
        )],
    );

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 2);
    assert_eq!(buckets(&messages), (200 + 600, 120 + 80, 300, 50));
    assert_eq!(total_tokens(&messages), 1350);
}

/// T07: subagent transcripts are counted, attributed to the agent, and never
/// confused with the parent session's own requests.
#[tokio::test]
async fn test_qoder_subagent_is_counted_once_with_attribution() {
    let home = common::temp_home();
    write_jsonl(
        &home
            .path()
            .join(".qoder/projects/-repo/sess-main/subagents/agent-xyz.meta.json"),
        &[r#"{"agentType":"code-reviewer"}"#],
    );
    write_jsonl(
        &home.path().join(".qoder/projects/-repo/sess-main.jsonl"),
        &[concat!(
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-main","cwd":"/work/repo","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_main","role":"assistant","model":"qmodel","usage":{"input_tokens":500,"output_tokens":50,"request_id":"req-main"}}}"#,
            "\n",
        )],
    );
    write_jsonl(
        &home
            .path()
            .join(".qoder/projects/-repo/sess-main/subagents/agent-xyz.jsonl"),
        &[concat!(
            r#"{"type":"assistant","isSidechain":true,"sessionId":"sess-main","agentId":"xyz","cwd":"/work/repo","timestamp":"2026-11-07T10:00:10.000Z","message":{"id":"msg_sub","role":"assistant","model":"qmodel","usage":{"input_tokens":300,"output_tokens":30,"request_id":"req-sub"}}}"#,
            "\n",
        )],
    );
    // The journal shares the `.jsonl` suffix but is not a transcript.
    write_jsonl(
        &home
            .path()
            .join(".qoder/projects/-repo/sess-main/subagents/journal.jsonl"),
        &[
            r#"{"type":"assistant","message":{"id":"journal","model":"qmodel","usage":{"input_tokens":9999,"output_tokens":9999}}}"#,
        ],
    );

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 2);
    assert_eq!(total_tokens(&messages), 880);
    let subagent = messages
        .iter()
        .find(|message| message.session_id.contains(":agent:xyz"))
        .expect("subagent message");
    assert_eq!(subagent.agent.as_deref(), Some("Code Reviewer"));
    assert_eq!(subagent.tokens.input, 300);
    let parent = messages
        .iter()
        .find(|message| message.session_id == "sess-main")
        .expect("parent message");
    assert_eq!(parent.agent, None);
}

/// T08 / T09: credits-only and usage-less rows are not turned into token or
/// dollar estimates.
#[tokio::test]
async fn test_qoder_credit_only_rows_are_never_estimated() {
    let home = common::temp_home();
    write_jsonl(
        &home
            .path()
            .join(".qoder-cn/projects/-repo/sess-credits.jsonl"),
        &[concat!(
            r#"{"type":"user","uuid":"u1","sessionId":"sess-credits","timestamp":"2026-08-22T06:35:31.000Z","message":{"role":"user","content":"hello"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-credits","timestamp":"2026-08-22T06:36:32.000Z","message":{"id":"msg_credits","role":"assistant","model":"qmodel_38max","content":[{"type":"text","text":"a long reply that must not be estimated"}],"usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"credits":8.229,"original_credits":8.229,"billable":true,"context_usage_ratio":0.2}}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a2","sessionId":"sess-credits","timestamp":"2026-08-22T06:40:00.000Z","message":{"id":"msg_no_usage","role":"assistant","model":"qmodel_38max","content":[{"type":"text","text":"another reply with no usage field at all"}]}}"#,
            "\n",
        )],
    );

    let messages = parse_uncached(home.path()).await;
    assert!(
        messages.is_empty(),
        "credit-only rows carry no measured tokens"
    );

    let parsed = parse_local_clients(qoder_options(home.path())).unwrap();
    assert_eq!(parsed.counts.get(ClientId::Qoder), 0);
    assert!(parsed.messages.is_empty());
}

/// T10: a BYOK row keeps its vendor and every cache bucket, including the 1h
/// cache-write subset.
#[tokio::test]
async fn test_qoder_byok_cache_buckets() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(".qoder/projects/-repo/sess-byok.jsonl"),
        &[concat!(
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-byok","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_byok","role":"assistant","model":"deepseek/chat/deepseek-v3","usage":{"input_tokens":2000,"output_tokens":300,"cache_read_input_tokens":1500,"cache_creation_input_tokens":400,"cache_creation":{"ephemeral_1h_input_tokens":250,"ephemeral_5m_input_tokens":150},"request_id":"req-byok"}}}"#,
            "\n",
        )],
    );

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.model_id, "deepseek-v3");
    assert_eq!(message.provider_id, "deepseek");
    assert_eq!(message.tokens.input, 500);
    assert_eq!(message.tokens.cache_read, 1500);
    assert_eq!(message.tokens.cache_write, 400);
    assert_eq!(message.cache_write_1h, 250);
    assert_eq!(message.tokens.output, 300);
    assert_eq!(message.tokens.total(), 2700);
}

/// T19: `qoder/` and `qoder-cn/` qualify the client, not the vendor.
#[tokio::test]
async fn test_qoder_model_namespaces_are_not_providers() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(".qoder/projects/-repo/sess-ns.jsonl"),
        &[concat!(
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-ns","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_ns_1","role":"assistant","model":"qoder/claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":10,"request_id":"req-ns-1"}}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a2","sessionId":"sess-ns","timestamp":"2026-11-07T10:01:02.000Z","message":{"id":"msg_ns_2","role":"assistant","model":"qoder-cn/qwen3-coder-plus","usage":{"input_tokens":200,"output_tokens":20,"request_id":"req-ns-2"}}}"#,
            "\n",
        )],
    );

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[1].model_id, "qwen3-coder-plus");
    assert_eq!(messages[1].provider_id, "qwen");
}

/// T17: a routing slot never inherits another client's `auto` price, while a
/// genuinely identified BYOK model is still priced.
#[tokio::test]
async fn test_qoder_routing_slots_stay_unpriced() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(".qoder/projects/-repo/sess-pricing.jsonl"),
        &[concat!(
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-pricing","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_auto","role":"assistant","model":"auto","usage":{"input_tokens":1000,"output_tokens":100,"request_id":"req-auto"}}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"a2","sessionId":"sess-pricing","timestamp":"2026-11-07T10:01:02.000Z","message":{"id":"msg_byok","role":"assistant","model":"claude-sonnet-4-6","usage":{"input_tokens":1000,"output_tokens":100,"request_id":"req-byok"}}}"#,
            "\n",
        )],
    );

    let mut dataset = HashMap::new();
    dataset.insert(
        "auto".to_string(),
        ModelPricing {
            input_cost_per_token: Some(1.0),
            output_cost_per_token: Some(1.0),
            ..Default::default()
        },
    );
    dataset.insert(
        "claude-sonnet-4-6".to_string(),
        ModelPricing {
            input_cost_per_token: Some(0.001),
            output_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );
    let pricing = PricingService::new(dataset, HashMap::new());

    let messages = parse_local_unified_messages_with_pricing_uncached(
        qoder_options(home.path()),
        Some(&pricing),
    )
    .await
    .unwrap();

    let auto = messages
        .iter()
        .find(|message| message.model_id == "auto")
        .unwrap();
    assert_eq!(auto.provider_id, "qoder");
    assert_eq!(
        auto.cost, 0.0,
        "a routing slot must not borrow another client's price"
    );
    assert_eq!(auto.cost_source, tokens_core::CostSource::Unknown);

    let byok = messages
        .iter()
        .find(|message| message.model_id == "claude-sonnet-4-6")
        .unwrap();
    assert_eq!(byok.provider_id, "anthropic");
    assert!((byok.cost - (1000.0 * 0.001 + 100.0 * 0.002)).abs() < 1e-9);
    assert_eq!(byok.cost_source, tokens_core::CostSource::Estimated);
}

/// T14 / T15 / T16: segment logs are counted per completed request, a request
/// that also exists in a transcript is counted once, and `turn.finished` never
/// adds a second copy.
#[tokio::test]
async fn test_qoder_segments_and_transcript_dedupe() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(".qoder/projects/-work-app/sess-1.jsonl"),
        &[concat!(
            r#"{"type":"assistant","uuid":"a1","sessionId":"sess-1","cwd":"/work/app","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"msg_1","role":"assistant","model":"qmodel","usage":{"input_tokens":1200,"output_tokens":40,"request_id":"req-shared"}}}"#,
            "\n",
        )],
    );
    write_jsonl(
        &home
            .path()
            .join(".qoder/logs/sessions/-work-app/sess-1/segments/run-1.jsonl"),
        &[concat!(
            r#"{"ts":"2026-11-07T10:00:01.000Z","type":"session.config.loaded","data":{"project_root":"/work/app"}}"#,
            "\n",
            r#"{"ts":"2026-11-07T10:00:02.000Z","type":"model.response.completed","request_id":"req-shared","data":{"model":"qmodel","input_tokens":1200,"output_tokens":40,"cache_read_input_tokens":0}}"#,
            "\n",
            r#"{"ts":"2026-11-07T10:05:00.000Z","type":"model.response.completed","request_id":"req-segment-only","data":{"model":"qmodel","input_tokens":600,"output_tokens":20,"cache_read_input_tokens":100}}"#,
            "\n",
            r#"{"ts":"2026-11-07T10:05:01.000Z","type":"turn.finished","data":{"model":"qmodel","input_tokens":1800,"output_tokens":60,"cache_read_input_tokens":100}}"#,
            "\n",
        )],
    );
    // The same completed event replayed into a second segment file (a restarted
    // run) must not add a second copy.
    write_jsonl(
        &home
            .path()
            .join(".qoder/logs/sessions/-work-app/sess-1/segments/run-2.jsonl"),
        &[concat!(
            r#"{"ts":"2026-11-07T10:05:00.000Z","type":"model.response.completed","request_id":"req-segment-only","data":{"model":"qmodel","input_tokens":600,"output_tokens":20,"cache_read_input_tokens":100}}"#,
            "\n",
        )],
    );

    // Diagnostics beside the segment files are not usage: `runs/` holds the
    // same completion event for its own purposes.
    write_jsonl(
        &home
            .path()
            .join(".qoder/logs/sessions/-work-app/sess-1/runs/run-1.jsonl"),
        &[
            r#"{"ts":"2026-11-07T10:05:00.000Z","type":"model.response.completed","request_id":"req-run-dir","data":{"model":"qmodel","input_tokens":9999,"output_tokens":9999}}"#,
        ],
    );

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 2);
    assert_eq!(total_tokens(&messages), 1240 + 620);
    let (input, output, cache_read, cache_write) = buckets(&messages);
    assert_eq!(
        (input, output, cache_read, cache_write),
        (1200 + 500, 40 + 20, 100, 0)
    );

    let segment_only = messages
        .iter()
        .find(|message| message.tokens.input == 500)
        .expect("the --no-session-persistence request is counted");
    assert_eq!(segment_only.session_id, "sess-1");
    assert_eq!(segment_only.workspace_key.as_deref(), Some("/work/app"));

    // Swapping which source is discovered first cannot change the result.
    let mut reversed = messages.clone();
    reversed.reverse();
    let collapsed = tokens_core::sessions::qoder::collapse_qoder_messages(reversed);
    assert_eq!(total_tokens(&collapsed), total_tokens(&messages));
}

/// T11 / T12: the IDE database's `prompt_tokens` already includes
/// `cached_tokens`; a missing `chat_session` table only drops attribution.
#[tokio::test]
async fn test_qoder_ide_database_exact_body_and_missing_session_table() {
    let home = common::temp_home();
    let app_data =
        PathRoot::AppData.resolve_with_env_strategy(home.path().to_str().unwrap(), false);
    let db = PathBuf::from(app_data)
        .join("Qoder")
        .join("SharedClientCache/cache/db/local.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_message (
                 id TEXT PRIMARY KEY,
                 session_id TEXT,
                 request_id TEXT,
                 role TEXT,
                 token_info TEXT,
                 model_info TEXT,
                 gmt_create INTEGER
             );
             CREATE TABLE chat_session (
                 session_id TEXT PRIMARY KEY,
                 project_uri TEXT,
                 project_name TEXT,
                 preferred_model_info TEXT
             );
             INSERT INTO chat_session VALUES
                 ('sess-ide', 'file:///work/ide-app', 'ide-app', '{\"model_key\":\"qmodel\"}');
             INSERT INTO chat_message VALUES
                 ('m1','sess-ide','req-ide-1','assistant',
                  '{\"prompt_tokens\":39005,\"cached_tokens\":37485,\"completion_tokens\":67,\"max_input_tokens\":200000}',
                  '{\"model_key\":\"qwen3.8-max\"}', 1762509600000),
                 ('u1','sess-ide',NULL,'user',
                  '{\"prompt_tokens\":39005,\"cached_tokens\":37485,\"completion_tokens\":0}',
                  NULL, 1762509599000);",
        )
        .unwrap();
    }
    // A second edition with no chat_session table at all.
    let cn_db = PathBuf::from(
        PathRoot::AppData.resolve_with_env_strategy(home.path().to_str().unwrap(), false),
    )
    .join("QoderCN")
    .join("SharedClientCache/cache/db/local.db");
    std::fs::create_dir_all(cn_db.parent().unwrap()).unwrap();
    {
        let conn = rusqlite::Connection::open(&cn_db).unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_message (
                 id TEXT PRIMARY KEY,
                 session_id TEXT,
                 request_id TEXT,
                 role TEXT,
                 token_info TEXT,
                 model_info TEXT,
                 gmt_create INTEGER
             );
             INSERT INTO chat_message VALUES
                 ('c1','sess-cn',NULL,'assistant',
                  '{\"prompt_tokens\":1000,\"cached_tokens\":250,\"completion_tokens\":100}',
                  '{\"model_key\":\"auto\"}', 1762509700000);",
        )
        .unwrap();
    }

    let scan = scan_all_clients_with_scanner_settings(
        home.path().to_str().unwrap(),
        &["qoder".to_string()],
        false,
        &ScannerSettings::default(),
    );
    assert_eq!(scan.qoder_ide_dbs.len(), 2);

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 2);

    let ide = messages
        .iter()
        .find(|message| message.session_id == "sess-ide")
        .expect("IDE row");
    assert_eq!(ide.tokens.input, 1520);
    assert_eq!(ide.tokens.cache_read, 37485);
    assert_eq!(ide.tokens.output, 67);
    assert_eq!(ide.tokens.total(), 39072);
    assert_eq!(ide.model_id, "qwen3.8-max");
    assert_eq!(ide.provider_id, "qwen");
    assert_eq!(ide.workspace_key.as_deref(), Some("/work/ide-app"));

    let cn = messages
        .iter()
        .find(|message| message.session_id == "sess-cn")
        .expect("CN IDE row without chat_session");
    assert_eq!(cn.tokens.input, 750);
    assert_eq!(cn.tokens.cache_read, 250);
    assert_eq!(cn.tokens.output, 100);
    assert_eq!(cn.model_id, "auto");
    assert_eq!(cn.provider_id, "qoder");
    assert_eq!(cn.workspace_key, None);
}

/// T13: rows the IDE writes while its own connection is open live in the
/// `-wal` sidecar. A warm cache keyed on the database file alone would never
/// see them.
#[tokio::test]
#[serial_test::serial]
async fn test_qoder_ide_wal_writes_invalidate_the_source_cache() {
    let home = common::temp_home();
    let cache_home = common::temp_home();
    let mut env = EnvGuard::capture(&["TOKENS_CONFIG_DIR", "HOME", "XDG_CONFIG_HOME"]);
    env.set("TOKENS_CONFIG_DIR", cache_home.path());
    env.set("HOME", home.path());
    env.set("XDG_CONFIG_HOME", home.path().join(".config"));

    let db = PathBuf::from(
        PathRoot::AppData.resolve_with_env_strategy(home.path().to_str().unwrap(), false),
    )
    .join("Qoder/SharedClientCache/cache/db/local.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let writer = rusqlite::Connection::open(&db).unwrap();
    writer
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE chat_message (
                 id TEXT PRIMARY KEY,
                 session_id TEXT,
                 request_id TEXT,
                 role TEXT,
                 token_info TEXT,
                 model_info TEXT,
                 gmt_create INTEGER
             );",
        )
        .unwrap();
    writer
        .execute(
            "INSERT INTO chat_message VALUES
                 ('m1','sess-wal','req-1','assistant', ?1, '{\"model_key\":\"auto\"}', 1762509600000)",
            rusqlite::params![r#"{"prompt_tokens":100,"cached_tokens":0,"completion_tokens":10}"#],
        )
        .unwrap();

    let first = parse_local_unified_messages_with_pricing(qoder_options(home.path()), None)
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].tokens.input, 100);

    // The IDE is still running: the new row is only in the -wal sidecar.
    writer
        .execute(
            "INSERT INTO chat_message VALUES
                 ('m2','sess-wal','req-2','assistant', ?1, '{\"model_key\":\"auto\"}', 1762509601000)",
            rusqlite::params![r#"{"prompt_tokens":300,"cached_tokens":100,"completion_tokens":30}"#],
        )
        .unwrap();
    assert!(PathBuf::from(format!("{}-wal", db.display())).exists());

    let second = parse_local_unified_messages_with_pricing(qoder_options(home.path()), None)
        .await
        .unwrap();
    assert_eq!(
        second.len(),
        2,
        "the -wal write must invalidate the cached read"
    );
    assert_eq!(buckets(&second), (100 + 200, 10 + 30, 100, 0));
    drop(writer);
}

/// T22: rescanning the same home twice — warm cache and cold parse — agrees.
#[tokio::test]
#[serial_test::serial]
async fn test_qoder_repeated_scans_are_stable() {
    let home = common::temp_home();
    let cache_home = common::temp_home();
    let mut env = EnvGuard::capture(&["TOKENS_CONFIG_DIR", "HOME", "XDG_CONFIG_HOME"]);
    env.set("TOKENS_CONFIG_DIR", cache_home.path());
    env.set("HOME", home.path());
    env.set("XDG_CONFIG_HOME", home.path().join(".config"));

    write_jsonl(
        &home.path().join(".qoder/projects/-work-app/sess-1.jsonl"),
        &[INTL_ONE_REQUEST],
    );
    write_jsonl(
        &home
            .path()
            .join(".qoder/logs/sessions/-work-app/sess-2/segments/run.jsonl"),
        &[
            r#"{"ts":"2026-11-07T12:00:00.000Z","type":"model.response.completed","request_id":"req-seg","data":{"model":"qmodel","input_tokens":700,"output_tokens":30,"cache_read_input_tokens":200}}"#,
        ],
    );

    let cold = parse_local_unified_messages_with_pricing(qoder_options(home.path()), None)
        .await
        .unwrap();
    let warm = parse_local_unified_messages_with_pricing(qoder_options(home.path()), None)
        .await
        .unwrap();
    let uncached =
        parse_local_unified_messages_with_pricing_uncached(qoder_options(home.path()), None)
            .await
            .unwrap();

    assert_eq!(cold.len(), 2);
    assert_eq!(total_tokens(&cold), 1340 + 730);
    assert_eq!(total_tokens(&warm), total_tokens(&cold));
    assert_eq!(total_tokens(&uncached), total_tokens(&cold));
    assert_eq!(buckets(&warm), buckets(&cold));

    // Appending a new request grows the total by exactly that request.
    write_jsonl(
        &home.path().join(".qoder/projects/-work-app/sess-3.jsonl"),
        &[
            r#"{"type":"assistant","sessionId":"sess-3","cwd":"/work/app","timestamp":"2026-11-07T13:00:00.000Z","message":{"id":"msg_3","role":"assistant","model":"qmodel","usage":{"input_tokens":50,"output_tokens":5,"request_id":"req-3"}}}"#,
        ],
    );
    let grown = parse_local_unified_messages_with_pricing(qoder_options(home.path()), None)
        .await
        .unwrap();
    assert_eq!(grown.len(), 3);
    assert_eq!(total_tokens(&grown), total_tokens(&cold) + 55);
}

/// T20 / T21: damaged lines, absurd counters and nonsense timestamps neither
/// crash the parse nor inflate a day.
#[tokio::test]
async fn test_qoder_damaged_input_never_inflates_totals() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(".qoder/projects/-repo/sess-bad.jsonl"),
        &[
            r#"{"type":"assistant","sessionId":"sess-bad","timestamp":"2026-11-07T10:00:00.000Z","message":{"id":"m1","model":"qmodel","usage":{"input_tokens":100,"output_tokens":10,"request_id":"req-1"}}}"#,
            r#"{"type":"assistant","sessionId":"sess-bad","timestamp":"2026-11-07T10:00:01.000Z","message":{"id":"m2","model":"qm"#,
            r#"{"type":"assistant","sessionId":"sess-bad","timestamp":"2026-11-07T10:00:02.000Z","message":{"id":"m3","model":"qmodel","usage":{"input_tokens":100,"output_tokens":10,"request_id":"req-1"}}}"#,
            r#"{"type":"assistant","sessionId":"sess-bad","timestamp":"2026-11-07T10:00:03.000Z","message":{"id":"m4","model":"qmodel","usage":{"input_tokens":-100,"output_tokens":-10,"cache_read_input_tokens":-5}}}"#,
            r#"{"type":"assistant","sessionId":"sess-bad","timestamp":"99999999999999999999999","message":{"id":"m5","model":"qmodel","usage":{"input_tokens":7,"output_tokens":1,"request_id":"req-5"}}}"#,
            "",
        ],
    );

    let messages = parse_uncached(home.path()).await;
    assert_eq!(messages.len(), 2);
    assert_eq!(total_tokens(&messages), 110 + 8);
}

/// T23: `QODER_CONFIG_DIR` / `QODERCN_CONFIG_DIR` are honoured, and a scan of a
/// redirected root never reaches into the developer's real Qoder directories.
#[tokio::test]
#[serial_test::serial]
async fn test_qoder_config_dir_overrides_are_honoured() {
    let home = common::temp_home();
    let intl_root = common::temp_home();
    let cn_root = common::temp_home();
    let mut env = EnvGuard::capture(&[
        "QODER_CONFIG_DIR",
        "QODERCN_CONFIG_DIR",
        "QODER_HOME",
        "QODER_CN_HOME",
    ]);
    env.set("QODER_CONFIG_DIR", intl_root.path());
    env.set("QODERCN_CONFIG_DIR", cn_root.path());
    env.remove("QODER_HOME");
    env.remove("QODER_CN_HOME");

    write_jsonl(
        &intl_root.path().join("projects/-repo/sess-intl.jsonl"),
        &[INTL_ONE_REQUEST],
    );
    write_jsonl(
        &cn_root.path().join("projects/-repo/sess-cn.jsonl"),
        &[CN_ONE_REQUEST],
    );

    let options = LocalParseOptions {
        home_dir: Some(home.path().to_string_lossy().into_owned()),
        use_env_roots: true,
        clients: Some(vec!["qoder".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: ScannerSettings::default(),
    };
    let messages = parse_local_unified_messages_with_pricing_uncached(options, None)
        .await
        .unwrap();
    assert_eq!(messages.len(), 2);

    let scan = scan_all_clients_with_scanner_settings(
        home.path().to_str().unwrap(),
        &["qoder".to_string()],
        true,
        &ScannerSettings::default(),
    );
    let scanned: Vec<String> = scan
        .get(ClientId::Qoder)
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(scanned.len(), 2);
    assert!(scanned.iter().any(|path| path.contains("sess-intl")));
    assert!(scanned.iter().any(|path| path.contains("sess-cn")));
}

/// A Qoder-only filter must not pick up other clients, and other clients must
/// not pick up Qoder (T24 at the core level; the CLI enum is tested in
/// `tokens-cli`).
#[tokio::test]
async fn test_qoder_filter_is_isolated() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(".qoder/projects/-repo/sess-1.jsonl"),
        &[INTL_ONE_REQUEST],
    );
    write_jsonl(
        &home
            .path()
            .join(".qwen/projects/-repo/chats/sess-qwen.jsonl"),
        &[
            r#"{"type":"assistant","model":"qwen3.5-plus","timestamp":"2026-11-07T14:24:56.857Z","sessionId":"sess-qwen","usageMetadata":{"promptTokenCount":100,"candidatesTokenCount":10}}"#,
        ],
    );

    let qoder_only = parse_uncached(home.path()).await;
    assert_eq!(qoder_only.len(), 1);
    assert_eq!(qoder_only[0].client, "qoder");

    let qwen_only = parse_local_unified_messages_with_pricing_uncached(
        LocalParseOptions {
            clients: Some(vec!["qwen".to_string()]),
            ..qoder_options(home.path())
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(qwen_only.len(), 1);
    assert_eq!(qwen_only[0].client, "qwen");
}

/// A home with no Qoder data at all is an empty source, not an error.
#[tokio::test]
async fn test_qoder_absent_installation_degrades_to_empty() {
    let home = common::temp_home();
    let messages = parse_uncached(home.path()).await;
    assert!(messages.is_empty());
    let parsed = parse_local_clients(qoder_options(home.path())).unwrap();
    assert_eq!(parsed.counts.get(ClientId::Qoder), 0);
}

/// Minimal env guard: `paths::test_env` is crate-private, and these tests are
/// serialized so a process-wide environment change is safe.
struct EnvGuard(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl EnvGuard {
    fn capture(keys: &[&'static str]) -> Self {
        Self(
            keys.iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        )
    }

    fn set(&mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) {
        // SAFETY: every test touching these variables holds `serial_test::serial`.
        unsafe { std::env::set_var(key, value) };
    }

    fn remove(&mut self, key: &str) {
        unsafe { std::env::remove_var(key) };
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, previous) in self.0.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}
