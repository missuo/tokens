use tokens_core::{
    aggregate_by_date, canonical_model_id, normalize_model_for_grouping, TokenBreakdown,
    UnifiedMessage,
};

fn configure_prefixes() {
    tokens_core::model_normalization::set_prefixes_to_strip(&[
        "GATEWAY-A/".into(),
        "gateway-b/".into(),
        "gateway-a/team/".into(),
    ]);
}

#[test]
fn configured_prefixes_apply_to_any_model_family() {
    configure_prefixes();

    assert_eq!(canonical_model_id("gateway-a/gpt-5.4"), "gpt-5.4");
    assert_eq!(
        canonical_model_id("gateway-b/claude-opus-4-6"),
        "claude-opus-4-6"
    );
    assert_eq!(normalize_model_for_grouping("gateway-a/gpt-5.4"), "gpt-5.4");
}

#[test]
fn prefix_removal_keeps_existing_case_and_suffix_normalization() {
    configure_prefixes();

    assert_eq!(
        canonical_model_id("GATEWAY-A/GPT-5.4-20261001(HIGH)"),
        "gpt-5.4"
    );
    assert_eq!(
        canonical_model_id("gateway-b/anthropic/claude-4-6-opus"),
        "claude-opus-4-6"
    );
}

#[test]
fn unconfigured_namespaces_and_partial_matches_remain_distinct() {
    configure_prefixes();

    assert_eq!(canonical_model_id("Qwen/Qwen3"), "qwen/qwen3");
    assert_eq!(
        canonical_model_id("meta-llama/Llama-4"),
        "meta-llama/llama-4"
    );
    assert_eq!(
        canonical_model_id("gateway-a-other/gpt-5.4"),
        "gateway-a-other/gpt-5.4"
    );
    assert_eq!(
        canonical_model_id("other/gateway-a/gpt-5.4"),
        "other/gateway-a/gpt-5.4"
    );
    assert_eq!(canonical_model_id("gateway-a/"), "gateway-a/");
}

#[test]
fn the_longest_prefix_is_removed_once() {
    configure_prefixes();

    assert_eq!(canonical_model_id("gateway-a/team/gpt-5.4"), "gpt-5.4");
    assert_eq!(
        canonical_model_id("gateway-a/gateway-b/gpt-5.4"),
        "gateway-b/gpt-5.4"
    );
}

#[test]
fn daily_submission_merges_routes_without_losing_usage_or_provider_identity() {
    configure_prefixes();
    let messages = vec![
        UnifiedMessage::new(
            "codex",
            "gpt-5.4",
            "native",
            "native-session",
            1_791_072_000_000,
            TokenBreakdown {
                input: 11,
                output: 3,
                cache_read: 5,
                cache_write: 7,
                reasoning: 2,
            },
            0.125,
        ),
        UnifiedMessage::new(
            "codex",
            "gateway-a/gpt-5.4",
            "provider-a",
            "session-a",
            1_791_072_000_000,
            TokenBreakdown {
                input: 19,
                output: 17,
                cache_read: 13,
                cache_write: 23,
                reasoning: 29,
            },
            0.375,
        ),
        UnifiedMessage::new(
            "codex",
            "gateway-b/gpt-5.4",
            "provider-b",
            "session-b",
            1_791_072_000_000,
            TokenBreakdown {
                input: 31,
                output: 37,
                cache_read: 41,
                cache_write: 43,
                reasoning: 47,
            },
            0.25,
        ),
    ];

    let days = aggregate_by_date(messages);

    assert_eq!(days.len(), 1);
    assert_eq!(days[0].totals.tokens, 328);
    assert_eq!(days[0].totals.cost, 0.75);
    assert_eq!(days[0].totals.messages, 3);
    assert_eq!(days[0].clients.len(), 1);
    let model = &days[0].clients[0];
    assert_eq!(model.model_id, "gpt-5.4");
    assert_eq!(model.provider_id, "native, provider-a, provider-b");
    assert_eq!(model.cost, 0.75);
    assert_eq!(model.messages, 3);
    assert_eq!(
        model.tokens,
        TokenBreakdown {
            input: 61,
            output: 57,
            cache_read: 59,
            cache_write: 73,
            reasoning: 78,
        }
    );
}
