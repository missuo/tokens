//! TRAE CLI 2.0 rollouts end to end: discovered under `~/.trae/cli`, parsed
//! through the Codex parser, and reported as `trae-cli` rather than `codex`.

mod common;

use std::path::Path;

use tokens_core::scanner::{scan_all_clients_with_scanner_settings, ScannerSettings};
use tokens_core::{
    parse_local_clients, parse_local_unified_messages_with_pricing_uncached, ClientId,
    LocalParseOptions, UnifiedMessage,
};

// Shapes from the TRAE CLI 0.200.19 fixtures in kenn-io/agentsview
// (`internal/parser/testdata/traex/`): a `token_count` rollout and a newer
// `history_mutation` rollout that carries usage in `token_usage_record`.
const TOKEN_COUNT_ROLLOUT: &[&str] = &[
    r#"{"timestamp":"2026-08-01T18:07:04.550Z","type":"session_meta","payload":{"id":"019fbcca-9fd4-7d20-83dc-0762b2f839b3","timestamp":"2026-08-01T18:07:03.636Z","cwd":"/home/user/code/api","originator":"codex-tui","cli_version":"0.200.19","model_provider":"trae"}}"#,
    r#"{"timestamp":"2026-08-01T18:07:04.570Z","type":"turn_context","payload":{"turn_id":"turn-1","cwd":"/home/user/code/api","model":"gpt-5-codex","model_provider":"trae"}}"#,
    r#"{"timestamp":"2026-08-01T18:07:04.590Z","type":"event_msg","payload":{"type":"user_message","message":"Explore the parser package."}}"#,
    r#"{"timestamp":"2026-08-01T18:07:14.100Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":4200,"cached_input_tokens":3600,"output_tokens":180,"reasoning_output_tokens":64,"total_tokens":4380},"total_token_usage":{"input_tokens":4200,"cached_input_tokens":3600,"output_tokens":180,"reasoning_output_tokens":64,"total_tokens":4380},"model_context_window":272000}}}"#,
];

const HISTORY_MUTATION_ROLLOUT: &[&str] = &[
    r#"{"timestamp":"2026-09-01T10:00:00.000Z","type":"session_meta","payload":{"id":"019fcf70-1111-7000-8000-000000000001","timestamp":"2026-09-01T10:00:00.000Z","cwd":"/home/user/code/app","originator":"codex-tui","cli_version":"0.200.19","model_provider":"trae"}}"#,
    r#"{"timestamp":"2026-09-01T10:00:01.000Z","type":"turn_context","payload":{"turn_id":"turn-1","model":"gpt-5-codex","model_provider":"trae"}}"#,
    r#"{"timestamp":"2026-09-01T10:00:02.000Z","type":"history_mutation","payload":{"operation":"append","items":[{"type":"message","role":"user","content":[{"type":"input_text","text":"Inspect the parser."}]}]}}"#,
    r#"{"timestamp":"2026-09-01T10:00:03.000Z","type":"token_usage_record","payload":{"response_id":"resp_1","usage":{"input_tokens":100,"cached_input_tokens":40,"cache_creation_input_tokens":0,"output_tokens":12,"reasoning_output_tokens":2,"total_tokens":112}}}"#,
];

fn options(home: &Path, client: &str) -> LocalParseOptions {
    LocalParseOptions {
        home_dir: Some(home.to_string_lossy().into_owned()),
        use_env_roots: false,
        clients: Some(vec![client.to_string()]),
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

#[tokio::test]
async fn trae_cli_rollouts_are_counted_under_their_own_client() {
    let home = common::temp_home();
    write_jsonl(
        &home.path().join(
            ".trae/cli/sessions/2026/08/01/rollout-2026-08-01T18-07-04-019fbcca-9fd4-7d20-83dc-0762b2f839b3.jsonl",
        ),
        TOKEN_COUNT_ROLLOUT,
    );
    write_jsonl(
        &home.path().join(
            ".trae/cli/archived_sessions/rollout-2026-09-01T10-00-00-019fcf70-1111-7000-8000-000000000001.jsonl",
        ),
        HISTORY_MUTATION_ROLLOUT,
    );
    // Prompt history beside the rollouts carries no usage and is not scanned.
    write_jsonl(
        &home.path().join(".trae/cli/history.jsonl"),
        &[
            r#"{"session_id":"019fbcca-9fd4-7d20-83dc-0762b2f839b3","ts":1785561000,"text":"Explore the parser package."}"#,
        ],
    );

    let scan = scan_all_clients_with_scanner_settings(
        home.path().to_str().unwrap(),
        &["trae-cli".to_string()],
        false,
        &ScannerSettings::default(),
    );
    assert_eq!(scan.get(ClientId::TraeCli).len(), 2);

    let parsed = parse_local_clients(options(home.path(), "trae-cli")).unwrap();
    assert_eq!(parsed.counts.get(ClientId::TraeCli), 2);

    let messages =
        parse_local_unified_messages_with_pricing_uncached(options(home.path(), "trae-cli"), None)
            .await
            .expect("uncached local parse");
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().all(|message| message.client == "trae-cli"));
    assert!(messages
        .iter()
        .all(|message| message.model_id == "gpt-5-codex"));
    assert_eq!(total_tokens(&messages), 4380 + 112);

    // The rollouts live outside `~/.codex`, so they never count as Codex.
    let codex =
        parse_local_unified_messages_with_pricing_uncached(options(home.path(), "codex"), None)
            .await
            .expect("uncached codex parse");
    assert!(codex.is_empty());
}
