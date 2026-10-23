use serde_json::{json, Value};
use std::io::Write;
use tokens_core::sessions::{claudecode::parse_claude_file_with_home, codex::parse_codex_file};
use tokens_core::speed::{InferenceSpeed, Speed, SpeedSource};
use tokens_core::{aggregate_by_date, aggregate_by_session, UnifiedMessage};

fn transcript(rows: &[Value]) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    for row in rows {
        writeln!(file, "{row}").unwrap();
    }
    file
}

fn speeds(messages: &[UnifiedMessage]) -> Vec<Option<(Speed, SpeedSource)>> {
    messages
        .iter()
        .map(|message| {
            message
                .inference_speed
                .map(|value| (value.speed, value.source))
        })
        .collect()
}

#[test]
fn claude_uses_reported_speed_without_losing_streamed_or_unknown_usage() {
    let assistant = |id: &str, usage: Value| {
        json!({
            "type": "assistant", "timestamp": "2026-10-10T12:00:00Z",
            "requestId": id,
            "message": {"id": id, "model": "claude-opus-4-6", "usage": usage}
        })
    };
    let file = transcript(&[
        assistant(
            "a",
            json!({"input_tokens":11,"output_tokens":1,"cache_read_input_tokens":7,"cache_creation_input_tokens":3}),
        ),
        assistant(
            "a",
            json!({"input_tokens":11,"output_tokens":5,"speed":"fast"}),
        ),
        assistant("a", json!({"output_tokens":9})),
        assistant(
            "b",
            json!({"input_tokens":13,"output_tokens":2,"speed":"standard","service_tier":"priority"}),
        ),
        assistant(
            "c",
            json!({"input_tokens":17,"output_tokens":4,"service_tier":"priority"}),
        ),
        assistant(
            "d",
            json!({"input_tokens":19,"output_tokens":6,"speed":123}),
        ),
        assistant(
            "e",
            json!({"input_tokens":23,"output_tokens":8,"speed":"future-speed"}),
        ),
    ]);
    let messages = parse_claude_file_with_home(file.path(), None);
    assert_eq!(
        speeds(&messages),
        vec![
            Some((Speed::Fast, SpeedSource::Reported)),
            Some((Speed::Standard, SpeedSource::Reported)),
            None,
            None,
            None,
        ]
    );
    let days = aggregate_by_date(messages);
    assert_eq!(days.len(), 1);
    assert_eq!(days[0].totals.tokens, 122);
    assert_eq!(days[0].totals.messages, 5);
    assert_eq!(days[0].totals.cost, 0.0);
    assert_eq!(days[0].clients.len(), 1);
    assert_eq!(days[0].clients[0].model_id, "claude-opus-4-6");
    assert_eq!(
        serde_json::to_value(&days[0].clients[0].speed_usage).unwrap(),
        json!([
            {"speed":"standard","source":"reported","tokens":15,"messages":1},
            {"speed":"fast","source":"reported","tokens":30,"messages":1},
        ])
    );
}

#[test]
fn claude_headless_stream_resets_speed_between_responses() {
    let file = transcript(&[
        json!({"type":"message_start","message":{"model":"claude-opus-4-6","usage":{"input_tokens":31,"speed":"fast"}}}),
        json!({"type":"message_delta","usage":{"output_tokens":7}}),
        json!({"type":"message_stop"}),
        json!({"type":"message_start","message":{"model":"claude-opus-4-6","usage":{"input_tokens":13}}}),
        json!({"type":"message_delta","usage":{"output_tokens":3}}),
        json!({"type":"message_stop"}),
        json!({"model":"claude-opus-4-6","usage":{"input_tokens":5,"output_tokens":2,"speed":"standard"}}),
    ]);
    let messages = parse_claude_file_with_home(file.path(), None);
    assert_eq!(
        speeds(&messages),
        vec![
            Some((Speed::Fast, SpeedSource::Reported)),
            None,
            Some((Speed::Standard, SpeedSource::Reported)),
        ]
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| message.tokens.total())
            .collect::<Vec<_>>(),
        vec![38, 16, 7]
    );
}

#[test]
fn codex_selection_is_scoped_to_the_turn_model_and_snapshot_owner() {
    let settings = |owner: &str, tier: Value| {
        json!({
            "type":"event_msg", "payload":{
                "type":"thread_settings_applied", "thread_id":owner,
                "thread_settings":{"model":"gpt-6-astra","service_tier":tier}
            }
        })
    };
    let context = |model: &str| json!({"type":"turn_context","payload":{"model":model}});
    let usage = |input: i64| {
        json!({
            "type":"event_msg","payload":{"type":"token_count","info":{
                "last_token_usage":{"input_tokens":input,"cached_input_tokens":3,"output_tokens":5}
            }}
        })
    };
    let file = transcript(&[
        json!({"type":"session_meta","payload":{"id":"own-thread"}}),
        context("gpt-6-astra"),
        usage(11),
        settings("own-thread", json!("priority")),
        context("gpt-6-astra"),
        usage(17),
        settings("own-thread", json!("ultrafast")),
        usage(23), // running turn still Fast
        context("gpt-6-astra"),
        usage(29),
        settings("parent-thread", json!("default")), // copied snapshot must not leak
        context("gpt-6-astra"),
        usage(31),
        context("gpt-6-sol"),
        usage(37), // different model, no evidence
        settings("own-thread", json!("default")),
        context("gpt-6-astra"),
        usage(41),
        settings("own-thread", Value::Null),
        context("gpt-6-astra"),
        usage(43),
        settings("own-thread", json!("fast")),
        context("gpt-6-astra"),
        usage(47),
        settings("own-thread", json!("flex")),
        context("gpt-6-astra"),
        usage(53),
    ]);
    let messages = parse_codex_file(file.path());
    assert_eq!(
        speeds(&messages),
        vec![
            None,
            Some((Speed::Fast, SpeedSource::Selected)),
            Some((Speed::Fast, SpeedSource::Selected)),
            Some((Speed::Ultrafast, SpeedSource::Selected)),
            Some((Speed::Ultrafast, SpeedSource::Selected)),
            None,
            Some((Speed::Standard, SpeedSource::Selected)),
            None,
            Some((Speed::Fast, SpeedSource::Selected)),
            None,
        ]
    );
    // Cached input is part of input, not an extra token charge.
    assert_eq!(
        messages
            .iter()
            .map(|message| message.tokens.total())
            .sum::<i64>(),
        382
    );
}

#[test]
fn aggregation_keeps_speed_evidence_separate_without_splitting_the_model() {
    let messages: Vec<_> = (0..1000)
        .map(|index| {
            let mut message = UnifiedMessage::new(
                "codex",
                "gpt-6-astra",
                "openai",
                "thread",
                1_791_633_600_000,
                tokens_core::TokenBreakdown {
                    input: 7,
                    output: 3,
                    ..Default::default()
                },
                0.25,
            );
            message.inference_speed = match index % 4 {
                0 => Some(InferenceSpeed {
                    speed: Speed::Fast,
                    source: SpeedSource::Reported,
                }),
                1 => Some(InferenceSpeed {
                    speed: Speed::Fast,
                    source: SpeedSource::Selected,
                }),
                2 => Some(InferenceSpeed {
                    speed: Speed::Ultrafast,
                    source: SpeedSource::Selected,
                }),
                _ => None,
            };
            message
        })
        .collect();
    let sessions = aggregate_by_session(messages.clone());
    let days = aggregate_by_date(messages);
    assert_eq!(days[0].totals.tokens, 10_000);
    assert_eq!(days[0].totals.messages, 1000);
    assert_eq!(days[0].totals.cost, 250.0);
    assert_eq!(days[0].clients.len(), 1);
    assert_eq!(
        serde_json::to_value(&days[0].clients[0].speed_usage).unwrap(),
        json!([
            {"speed":"fast","source":"reported","tokens":2500,"messages":250},
            {"speed":"fast","source":"selected","tokens":2500,"messages":250},
            {"speed":"ultrafast","source":"selected","tokens":2500,"messages":250},
        ])
    );
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].totals.tokens, 10_000);
    assert_eq!(sessions[0].totals.messages, 1000);
    assert_eq!(sessions[0].totals.cost, 250.0);
    assert_eq!(sessions[0].clients.len(), 1);
    assert_eq!(
        serde_json::to_value(&sessions[0].clients[0].speed_usage).unwrap(),
        json!([
            {"speed":"fast","source":"reported","tokens":2500,"messages":250},
            {"speed":"fast","source":"selected","tokens":2500,"messages":250},
            {"speed":"ultrafast","source":"selected","tokens":2500,"messages":250},
        ])
    );
}
