use rusqlite::{params, Connection};
use std::collections::HashSet;
use tempfile::TempDir;
use tokens_core::sessions::hermes::{parse_hermes_databases, parse_hermes_sqlite};

#[test]
fn test_parse_hermes_sqlite_returns_empty_for_missing_database() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.db");
    assert!(parse_hermes_sqlite(&missing).is_empty());
}

fn create_test_db(dir: &TempDir) -> std::path::PathBuf {
    let db_path = dir.path().join("state.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            model TEXT,
            started_at REAL NOT NULL,
            message_count INTEGER DEFAULT 0,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            cache_read_tokens INTEGER DEFAULT 0,
            cache_write_tokens INTEGER DEFAULT 0,
            reasoning_tokens INTEGER DEFAULT 0,
            billing_provider TEXT,
            estimated_cost_usd REAL,
            actual_cost_usd REAL
        );
        CREATE TABLE session_model_usage (
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            model TEXT NOT NULL,
            billing_provider TEXT NOT NULL DEFAULT '',
            billing_base_url TEXT NOT NULL DEFAULT '',
            billing_mode TEXT NOT NULL DEFAULT '',
            task TEXT NOT NULL DEFAULT '',
            api_call_count INTEGER NOT NULL DEFAULT 0,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0,
            cache_write_tokens INTEGER NOT NULL DEFAULT 0,
            reasoning_tokens INTEGER NOT NULL DEFAULT 0,
            estimated_cost_usd REAL NOT NULL DEFAULT 0,
            actual_cost_usd REAL NOT NULL DEFAULT 0,
            cost_status TEXT,
            cost_source TEXT,
            first_seen REAL,
            last_seen REAL,
            PRIMARY KEY (session_id, model, billing_provider, billing_base_url, billing_mode, task)
        );
        "#,
    )
    .unwrap();
    db_path
}

/// Schema of a Hermes build whose `session_model_usage` predates the
/// `reasoning_tokens` column, so `PER_MODEL_QUERY` cannot even be prepared.
fn create_drifted_test_db(dir: &TempDir) -> std::path::PathBuf {
    let db_path = dir.path().join("drifted-state.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            model TEXT,
            started_at REAL NOT NULL,
            message_count INTEGER DEFAULT 0,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            cache_read_tokens INTEGER DEFAULT 0,
            cache_write_tokens INTEGER DEFAULT 0,
            reasoning_tokens INTEGER DEFAULT 0,
            billing_provider TEXT,
            estimated_cost_usd REAL,
            actual_cost_usd REAL
        );
        CREATE TABLE session_model_usage (
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            model TEXT NOT NULL,
            billing_provider TEXT NOT NULL DEFAULT '',
            billing_base_url TEXT NOT NULL DEFAULT '',
            billing_mode TEXT NOT NULL DEFAULT '',
            task TEXT NOT NULL DEFAULT '',
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0,
            cache_write_tokens INTEGER NOT NULL DEFAULT 0,
            estimated_cost_usd REAL NOT NULL DEFAULT 0,
            actual_cost_usd REAL NOT NULL DEFAULT 0,
            PRIMARY KEY (session_id, model, billing_provider, billing_base_url, billing_mode, task)
        );
        "#,
    )
    .unwrap();
    db_path
}

/// Schema of a Hermes build that predates `session_model_usage`.
fn create_legacy_test_db(dir: &TempDir) -> std::path::PathBuf {
    let db_path = dir.path().join("legacy-state.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            model TEXT,
            started_at REAL NOT NULL,
            message_count INTEGER DEFAULT 0,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            cache_read_tokens INTEGER DEFAULT 0,
            cache_write_tokens INTEGER DEFAULT 0,
            reasoning_tokens INTEGER DEFAULT 0,
            billing_provider TEXT,
            estimated_cost_usd REAL,
            actual_cost_usd REAL
        );
        "#,
    )
    .unwrap();
    db_path
}

#[test]
fn test_parse_hermes_sqlite_reads_session_rows_and_preserves_message_count() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    // Session row (needed for JOIN: started_at, message_count, model)
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-1",
            "cli",
            "claude-sonnet-4",
            1_750_000_000.25_f64,
            42_i64,
        ],
    )
    .unwrap();

    // Per-model token data
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-1",
            "claude-sonnet-4",
            "anthropic",
            "",
            "",
            "",
            1200_i64,
            300_i64,
            50_i64,
            20_i64,
            10_i64,
            0.12_f64,
            0.34_f64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.client, "hermes");
    assert_eq!(msg.agent.as_deref(), Some("Hermes Agent"));
    assert_eq!(msg.session_id, "session-1");
    assert_eq!(msg.model_id, "claude-sonnet-4");
    assert_eq!(msg.provider_id, "anthropic");
    assert_eq!(msg.timestamp, 1_750_000_000_250_i64);
    assert_eq!(msg.message_count, 42);
    assert_eq!(msg.tokens.input, 1200);
    assert_eq!(msg.tokens.output, 300);
    assert_eq!(msg.tokens.cache_read, 50);
    assert_eq!(msg.tokens.cache_write, 20);
    assert_eq!(msg.tokens.reasoning, 10);
    assert_eq!(msg.cost, 0.34);
    assert_eq!(
        msg.dedup_key.as_deref(),
        Some("hermes:session-1:claude-sonnet-4:anthropic#0")
    );
}

#[test]
fn test_parse_hermes_sqlite_skips_empty_sessions_and_falls_back_to_estimated_cost_and_provider_inference(
) {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    // Valid session with tokens
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-valid",
            "telegram",
            "gpt-5.4",
            1_775_001_102.0_f64,
            3_i64,
        ],
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-valid",
            "gpt-5.4",
            "",
            "",
            "",
            "",
            100_i64,
            20_i64,
            0_i64,
            0_i64,
            5_i64,
            1.25_f64,
            0.0_f64,
        ],
    )
    .unwrap();

    // Empty session: no smu row, and no session totals either, so neither the
    // per-model pass nor the session-totals reconciliation emits it.
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-empty",
            "telegram",
            "gpt-5.4",
            1_775_001_103.0_f64,
            9_i64,
        ],
    )
    .unwrap();

    // Session with no model — should be excluded by WHERE
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-no-model",
            "cli",
            Option::<String>::None,
            1_775_001_104.0_f64,
            1_i64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.session_id, "session-valid");
    assert_eq!(msg.provider_id, "openai");
    assert_eq!(msg.cost, 1.25);
    assert_eq!(msg.message_count, 3);
}

#[test]
fn test_parse_hermes_sqlite_ignores_unknown_billing_provider_and_falls_back_to_model_inference() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-unknown-provider",
            "cli",
            "gpt-5.4",
            1_775_001_105.0_f64,
            2_i64,
        ],
    )
    .unwrap();

    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-unknown-provider",
            "gpt-5.4",
            "unknown",
            "",
            "",
            "",
            100_i64,
            20_i64,
            0_i64,
            0_i64,
            0_i64,
            0.5_f64,
            0.0_f64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "openai");
}

#[test]
fn test_parse_hermes_sqlite_emits_per_model_rows_for_multi_model_session() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    // Single session with primary model "glm-5.2" and two smu models
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-multi",
            "desktop",
            "glm-5.2",
            1_775_002_000.0_f64,
            15_i64,
        ],
    )
    .unwrap();

    // Primary model (matches sessions.model) — should get message_count
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-multi",
            "glm-5.2",
            "kilocode",
            "",
            "",
            "",
            5000_i64,
            800_i64,
            100_i64,
            0_i64,
            200_i64,
            0.0_f64,
            0.0_f64,
        ],
    )
    .unwrap();

    // Secondary model — should get message_count = 0
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-multi",
            "mimo-v2.5-free",
            "opencode-zen",
            "",
            "",
            "",
            3000_i64,
            500_i64,
            0_i64,
            0_i64,
            0_i64,
            0.0_f64,
            0.0_f64,
        ],
    )
    .unwrap();

    // Third model with two task-variant rows — should be SUMmed
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-multi",
            "cohere/north-mini-code:free",
            "bifrost",
            "",
            "",
            "",
            900_i64,
            100_i64,
            0_i64,
            0_i64,
            0_i64,
            0.0_f64,
            0.0_f64,
        ],
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-multi",
            "cohere/north-mini-code:free",
            "bifrost",
            "",
            "",
            "title_generation",
            100_i64,
            50_i64,
            0_i64,
            0_i64,
            0_i64,
            0.0_f64,
            0.0_f64,
        ],
    )
    .unwrap();

    let mut messages = parse_hermes_sqlite(&db_path);
    messages.sort_by(|a, b| a.model_id.cmp(&b.model_id));

    assert_eq!(messages.len(), 3);

    // cohere/north-mini-code:free: SUM of 2 rows (900+100=1000 input, 100+50=150 output)
    let cohere = messages
        .iter()
        .find(|m| m.model_id == "cohere/north-mini-code:free")
        .unwrap();
    assert_eq!(cohere.tokens.input, 1000);
    assert_eq!(cohere.tokens.output, 150);
    assert_eq!(cohere.message_count, 0); // secondary model
    assert_eq!(
        cohere.dedup_key.as_deref(),
        Some("hermes:session-multi:cohere/north-mini-code:free:bifrost#0")
    );

    // glm-5.2: primary model, gets message_count
    let glm = messages.iter().find(|m| m.model_id == "glm-5.2").unwrap();
    assert_eq!(glm.tokens.input, 5000);
    assert_eq!(glm.tokens.output, 800);
    assert_eq!(glm.message_count, 15); // primary model
    assert_eq!(
        glm.dedup_key.as_deref(),
        Some("hermes:session-multi:glm-5.2:kilocode#0")
    );

    // mimo-v2.5-free: secondary model
    let mimo = messages
        .iter()
        .find(|m| m.model_id == "mimo-v2.5-free")
        .unwrap();
    assert_eq!(mimo.tokens.input, 3000);
    assert_eq!(mimo.tokens.output, 500);
    assert_eq!(mimo.message_count, 0); // secondary model
    assert_eq!(
        mimo.dedup_key.as_deref(),
        Some("hermes:session-multi:mimo-v2.5-free:opencode-zen#0")
    );
}

#[test]
fn test_parse_hermes_sqlite_falls_back_to_session_totals_without_session_model_usage() {
    let dir = TempDir::new().unwrap();
    let db_path = create_legacy_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            billing_provider, estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "legacy-session",
            "cli",
            "claude-sonnet-4",
            1_750_000_000.25_f64,
            42_i64,
            1200_i64,
            300_i64,
            50_i64,
            20_i64,
            10_i64,
            "anthropic",
            0.12_f64,
            0.34_f64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.client, "hermes");
    assert_eq!(msg.agent.as_deref(), Some("Hermes Agent"));
    assert_eq!(msg.session_id, "legacy-session");
    assert_eq!(msg.model_id, "claude-sonnet-4");
    assert_eq!(msg.provider_id, "anthropic");
    assert_eq!(msg.timestamp, 1_750_000_000_250_i64);
    assert_eq!(msg.message_count, 42);
    assert_eq!(msg.tokens.input, 1200);
    assert_eq!(msg.tokens.output, 300);
    assert_eq!(msg.tokens.cache_read, 50);
    assert_eq!(msg.tokens.cache_write, 20);
    assert_eq!(msg.tokens.reasoning, 10);
    assert_eq!(msg.cost, 0.34);
    // The session-totals path emits one message per session, so it keeps the
    // bare session id as the dedup key.
    assert_eq!(msg.dedup_key.as_deref(), Some("legacy-session#0"));
}

#[test]
fn test_parse_hermes_sqlite_legacy_skips_empty_sessions_and_falls_back_to_estimated_cost() {
    let dir = TempDir::new().unwrap();
    let db_path = create_legacy_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    // Valid session with tokens but no actual cost and no billing provider
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count,
            input_tokens, output_tokens, reasoning_tokens,
            billing_provider, estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            "legacy-valid",
            "telegram",
            "gpt-5.4",
            1_775_001_102.0_f64,
            3_i64,
            100_i64,
            20_i64,
            5_i64,
            Option::<String>::None,
            1.25_f64,
            Option::<f64>::None,
        ],
    )
    .unwrap();

    // Session with no usage at all — should be excluded by WHERE
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "legacy-empty",
            "telegram",
            "gpt-5.4",
            1_775_001_103.0_f64,
            9_i64,
        ],
    )
    .unwrap();

    // Session with no model — should be excluded by WHERE
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count, input_tokens
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            "legacy-no-model",
            "cli",
            Option::<String>::None,
            1_775_001_104.0_f64,
            1_i64,
            500_i64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.session_id, "legacy-valid");
    assert_eq!(msg.provider_id, "openai");
    assert_eq!(msg.cost, 1.25);
    assert_eq!(msg.message_count, 3);
    assert_eq!(msg.dedup_key.as_deref(), Some("legacy-valid#0"));
}

/// A Hermes upgrade can create `session_model_usage` without backfilling
/// existing sessions. Those sessions have real usage on `sessions` and no smu
/// child row, so a table-level probe plus an inner JOIN drops them entirely.
#[test]
fn test_parse_hermes_sqlite_emits_session_totals_for_sessions_without_smu_rows() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    // Backfilled session: has smu rows, so smu is authoritative for it.
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count,
            input_tokens, output_tokens, billing_provider, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            "session-backfilled",
            "cli",
            "claude-sonnet-4",
            1_775_003_000.0_f64,
            5_i64,
            1200_i64,
            300_i64,
            "anthropic",
            0.10_f64,
        ],
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-backfilled",
            "claude-sonnet-4",
            "anthropic",
            "",
            "",
            "",
            1200_i64,
            300_i64,
            0_i64,
            0_i64,
            0_i64,
            0.0_f64,
            0.10_f64,
        ],
    )
    .unwrap();

    // Pre-upgrade session: substantial usage, no smu child row at all.
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count,
            input_tokens, output_tokens, billing_provider, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            "session-not-backfilled",
            "desktop",
            "claude-opus-4-6",
            1_775_003_001.0_f64,
            42_i64,
            999_000_i64,
            12_000_i64,
            "anthropic",
            2.50_f64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 2);

    // The backfilled session is emitted exactly once — the two passes are
    // disjoint, so it is not counted by both.
    let backfilled: Vec<_> = messages
        .iter()
        .filter(|m| m.session_id == "session-backfilled")
        .collect();
    assert_eq!(backfilled.len(), 1);
    assert_eq!(backfilled[0].tokens.input, 1200);
    assert_eq!(
        backfilled[0].dedup_key.as_deref(),
        Some("hermes:session-backfilled:claude-sonnet-4:anthropic#0")
    );

    let recovered = messages
        .iter()
        .find(|m| m.session_id == "session-not-backfilled")
        .expect("session without smu rows must still be reported");
    assert_eq!(recovered.model_id, "claude-opus-4-6");
    assert_eq!(recovered.provider_id, "anthropic");
    assert_eq!(recovered.tokens.input, 999_000);
    assert_eq!(recovered.tokens.output, 12_000);
    assert_eq!(recovered.cost, 2.50);
    assert_eq!(recovered.message_count, 42);
    assert_eq!(
        recovered.dedup_key.as_deref(),
        Some("session-not-backfilled#0")
    );
}

/// A session with *some* smu coverage is reported from smu alone. Topping it up
/// from the session totals would double count whatever smu already recorded.
#[test]
fn test_parse_hermes_sqlite_does_not_top_up_partially_covered_sessions() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count, input_tokens, output_tokens
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            "session-partial",
            "cli",
            "glm-5.2",
            1_775_003_100.0_f64,
            9_i64,
            5000_i64,
            900_i64,
        ],
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-partial",
            "glm-5.2",
            "kilocode",
            "",
            "",
            "",
            3000_i64,
            400_i64,
            0_i64,
            0_i64,
            0_i64,
            0.0_f64,
            0.0_f64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 3000);
    assert_eq!(messages[0].tokens.output, 400);
}

/// Schema drift (here: a `session_model_usage` without `reasoning_tokens`)
/// makes the per-model query fail to prepare. That must degrade to the session
/// totals, not to reporting nothing at all.
#[test]
fn test_parse_hermes_sqlite_falls_back_to_session_totals_when_per_model_query_cannot_prepare() {
    let dir = TempDir::new().unwrap();
    let db_path = create_drifted_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            billing_provider, estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "drifted-session",
            "cli",
            "claude-sonnet-4",
            1_750_000_000.25_f64,
            42_i64,
            1200_i64,
            300_i64,
            50_i64,
            20_i64,
            10_i64,
            "anthropic",
            0.12_f64,
            0.34_f64,
        ],
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
        params![
            "drifted-session",
            "claude-sonnet-4",
            "anthropic",
            "",
            "",
            "",
            1200_i64,
            300_i64,
            50_i64,
            20_i64,
            0.12_f64,
            0.34_f64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.session_id, "drifted-session");
    assert_eq!(msg.tokens.input, 1200);
    assert_eq!(msg.tokens.output, 300);
    assert_eq!(msg.tokens.reasoning, 10);
    assert_eq!(msg.cost, 0.34);
    assert_eq!(msg.message_count, 42);
    assert_eq!(msg.dedup_key.as_deref(), Some("drifted-session#0"));
}

/// The composite primary key lets one (session, model) span several billing
/// providers. Collapsing them would credit every token to one provider and hide
/// the other from the provider breakdown.
#[test]
fn test_parse_hermes_sqlite_keeps_billing_providers_separate_for_same_model() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-split",
            "desktop",
            "glm-5.2",
            1_775_004_000.0_f64,
            7_i64,
        ],
    )
    .unwrap();

    for (provider, input) in [("kilocode", 1000_i64), ("opencode-zen", 2000_i64)] {
        conn.execute(
            r#"
            INSERT INTO session_model_usage (
                session_id, model, billing_provider, billing_base_url, billing_mode, task,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
                estimated_cost_usd, actual_cost_usd
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                "session-split",
                "glm-5.2",
                provider,
                "",
                "",
                "",
                input,
                0_i64,
                0_i64,
                0_i64,
                0_i64,
                0.0_f64,
                0.0_f64,
            ],
        )
        .unwrap();
    }

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 2);

    let kilocode = messages
        .iter()
        .find(|m| m.provider_id == "kilocode")
        .expect("kilocode must survive the grouping");
    let opencode = messages
        .iter()
        .find(|m| m.provider_id == "opencode_zen")
        .expect("opencode-zen must survive the grouping");

    assert_eq!(kilocode.tokens.input, 1000);
    assert_eq!(opencode.tokens.input, 2000);
    assert_eq!(
        kilocode.dedup_key.as_deref(),
        Some("hermes:session-split:glm-5.2:kilocode#0")
    );
    assert_eq!(
        opencode.dedup_key.as_deref(),
        Some("hermes:session-split:glm-5.2:opencode-zen#0")
    );

    // `sessions.message_count` is a per-session total, so splitting the primary
    // model across providers must not credit it twice.
    assert_eq!(
        kilocode.message_count + opencode.message_count,
        7,
        "session message_count must be credited exactly once"
    );
}

/// Cost is resolved per row before summing: a row with a reconciled actual cost
/// must not discard the estimated cost of its task-variant siblings.
#[test]
fn test_parse_hermes_sqlite_sums_actual_and_estimated_costs_per_row() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params!["session-cost", "cli", "gpt-5.4", 1_775_005_000.0_f64, 4_i64,],
    )
    .unwrap();

    // Reconciled row: actual cost known.
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-cost",
            "gpt-5.4",
            "openai",
            "",
            "",
            "",
            1000_i64,
            200_i64,
            0_i64,
            0_i64,
            0_i64,
            0.0_f64,
            0.50_f64,
        ],
    )
    .unwrap();

    // Task-variant sibling: only an estimate so far.
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-cost",
            "gpt-5.4",
            "openai",
            "",
            "",
            "title_generation",
            100_i64,
            20_i64,
            0_i64,
            0_i64,
            0_i64,
            0.05_f64,
            0.0_f64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert!(
        (messages[0].cost - 0.55).abs() < 1e-9,
        "expected 0.50 actual + 0.05 estimated, got {}",
        messages[0].cost
    );
    assert_eq!(messages[0].tokens.input, 1100);
}

/// A group whose only signal is cost still has to be emitted: the HAVING clause
/// and the projection must agree on how cost is computed.
#[test]
fn test_parse_hermes_sqlite_emits_cost_only_rows() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-cost-only",
            "cli",
            "gpt-5.4",
            1_775_006_000.0_f64,
            1_i64,
        ],
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, billing_base_url, billing_mode, task,
            input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            estimated_cost_usd, actual_cost_usd
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            "session-cost-only",
            "gpt-5.4",
            "openai",
            "",
            "",
            "",
            0_i64,
            0_i64,
            0_i64,
            0_i64,
            0_i64,
            0.07_f64,
            0.0_f64,
        ],
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    assert!((messages[0].cost - 0.07).abs() < 1e-9);
}

/// `sessions.message_count` is credited to the row matching `sessions.model`,
/// but nothing guarantees such a row exists — the session can have no model, or
/// name one that `session_model_usage` never recorded. The count still belongs
/// to the session, so a deterministic stand-in row has to carry it.
#[test]
fn test_parse_hermes_sqlite_keeps_message_count_when_no_row_matches_session_model() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            "session-orphan-count",
            "cli",
            "glm-5.2",
            1_775_007_000.0_f64,
            11_i64,
        ],
    )
    .unwrap();

    for (model, input) in [("gpt-5.4", 2000_i64), ("mimo-v2.5-free", 1000_i64)] {
        conn.execute(
            r#"
            INSERT INTO session_model_usage (
                session_id, model, billing_provider, billing_base_url, billing_mode, task,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
                estimated_cost_usd, actual_cost_usd
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                "session-orphan-count",
                model,
                "openai",
                "",
                "",
                "",
                input,
                0_i64,
                0_i64,
                0_i64,
                0_i64,
                0.0_f64,
                0.0_f64,
            ],
        )
        .unwrap();
    }

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages.iter().map(|m| m.message_count).sum::<i32>(),
        11,
        "session message_count must survive, and exactly once"
    );
}

/// A nullable `billing_provider` lets one (session, model) group twice — once
/// under NULL, once under ''. Both rows carry real tokens, so their dedup keys
/// must differ or the caller's dedup pass drops one of them.
#[test]
fn test_parse_hermes_sqlite_separates_null_and_empty_billing_providers() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("nullable-provider.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            model TEXT,
            started_at REAL NOT NULL,
            message_count INTEGER DEFAULT 0,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            cache_read_tokens INTEGER DEFAULT 0,
            cache_write_tokens INTEGER DEFAULT 0,
            reasoning_tokens INTEGER DEFAULT 0,
            billing_provider TEXT,
            estimated_cost_usd REAL,
            actual_cost_usd REAL
        );
        CREATE TABLE session_model_usage (
            session_id TEXT NOT NULL,
            model TEXT NOT NULL,
            billing_provider TEXT,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0,
            cache_write_tokens INTEGER NOT NULL DEFAULT 0,
            reasoning_tokens INTEGER NOT NULL DEFAULT 0,
            estimated_cost_usd REAL NOT NULL DEFAULT 0,
            actual_cost_usd REAL NOT NULL DEFAULT 0,
            -- Part of session_model_usage since its first release.
            first_seen REAL,
            last_seen REAL
        );
        INSERT INTO sessions (id, source, model, started_at, message_count)
        VALUES ('session-nullable', 'cli', 'glm-5.2', 1775008000.0, 3);
        INSERT INTO session_model_usage (session_id, model, billing_provider, input_tokens)
        VALUES ('session-nullable', 'glm-5.2', NULL, 2000),
               ('session-nullable', 'glm-5.2', '',   1000);
        "#,
    )
    .unwrap();

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 2);

    let keys: HashSet<&str> = messages
        .iter()
        .filter_map(|m| m.dedup_key.as_deref())
        .collect();
    assert_eq!(keys.len(), 2, "NULL and '' must not share a dedup key");
    assert_eq!(messages.iter().map(|m| m.tokens.input).sum::<i64>(), 3000);
}

// missuo/tokens#129: lifetime usage rows are spread across the assistant
// messages in their window instead of all landing on `sessions.started_at`.

/// 2026-01-01T00:00:00Z.
const DAY0: f64 = 1_767_225_600.0;
const DAY: f64 = 86_400.0;

/// The columns of Hermes's `messages` table the parser reads, plus the
/// compaction flags whose rows it must still count.
fn add_messages_table(conn: &Connection) {
    conn.execute_batch(
        r#"
        CREATE TABLE messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(id),
            role TEXT NOT NULL,
            content TEXT,
            tool_call_id TEXT,
            tool_calls TEXT,
            tool_name TEXT,
            timestamp REAL NOT NULL,
            token_count INTEGER,
            finish_reason TEXT,
            active INTEGER NOT NULL DEFAULT 1,
            compacted INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .unwrap();
}

fn insert_message(conn: &Connection, session_id: &str, role: &str, timestamp: f64) {
    conn.execute(
        "INSERT INTO messages (session_id, role, content, timestamp) VALUES (?1, ?2, 'x', ?3)",
        params![session_id, role, timestamp],
    )
    .unwrap();
}

fn insert_session(conn: &Connection, id: &str, model: &str, started_at: f64, message_count: i64) {
    conn.execute(
        "INSERT INTO sessions (id, source, model, started_at, message_count) VALUES (?1, 'cli', ?2, ?3, ?4)",
        params![id, model, started_at, message_count],
    )
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn insert_usage(
    conn: &Connection,
    session_id: &str,
    model: &str,
    input: i64,
    output: i64,
    cost: f64,
    first_seen: Option<f64>,
    last_seen: Option<f64>,
) {
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, input_tokens, output_tokens,
            actual_cost_usd, first_seen, last_seen
        ) VALUES (?1, ?2, 'anthropic', ?3, ?4, ?5, ?6, ?7)
        "#,
        params![session_id, model, input, output, cost, first_seen, last_seen],
    )
    .unwrap();
}

fn ms(seconds: f64) -> i64 {
    (seconds * 1000.0) as i64
}

#[test]
fn test_parse_hermes_sqlite_spreads_usage_across_assistant_messages_in_window() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    add_messages_table(&conn);

    let start = DAY0 + 3_600.0;
    insert_session(&conn, "long", "claude-sonnet-4", start, 12);
    conn.execute(
        r#"
        INSERT INTO session_model_usage (
            session_id, model, billing_provider, input_tokens, output_tokens,
            cache_read_tokens, reasoning_tokens, actual_cost_usd, first_seen, last_seen
        ) VALUES ('long', 'claude-sonnet-4', 'anthropic', 1000, 101, 7, 3, 1.0, ?1, ?2)
        "#,
        params![start + 10.0, DAY0 + 2.0 * DAY + 100.0],
    )
    .unwrap();

    let in_window = [
        start + 20.0,
        start + 40.0,
        DAY0 + DAY + 50.0,
        DAY0 + 2.0 * DAY + 10.0,
        DAY0 + 2.0 * DAY + 20.0,
        DAY0 + 2.0 * DAY + 30.0,
    ];
    for ts in in_window {
        insert_message(&conn, "long", "assistant", ts);
    }
    // Not usage signals: user turns, a message carried in before usage began,
    // and one written after the last recorded usage.
    insert_message(&conn, "long", "user", start + 5.0);
    insert_message(&conn, "long", "assistant", start + 1.0);
    insert_message(&conn, "long", "assistant", DAY0 + 2.0 * DAY + 500.0);
    // A compaction clone keeps the original timestamp; it is one API call.
    conn.execute(
        "INSERT INTO messages (session_id, role, content, timestamp, active, compacted)
         VALUES ('long', 'assistant', 'x', ?1, 0, 1)",
        params![DAY0 + DAY + 50.0],
    )
    .unwrap();

    let mut messages = parse_hermes_sqlite(&db_path);
    messages.sort_by_key(|m| m.timestamp);

    let timestamps: Vec<i64> = messages.iter().map(|m| m.timestamp).collect();
    assert_eq!(timestamps, in_window.map(ms).to_vec());

    // Cumulative floors keep every bucket exact.
    let inputs: Vec<i64> = messages.iter().map(|m| m.tokens.input).collect();
    assert_eq!(inputs, vec![166, 167, 167, 166, 167, 167]);
    assert_eq!(messages.iter().map(|m| m.tokens.output).sum::<i64>(), 101);
    assert_eq!(messages.iter().map(|m| m.tokens.cache_read).sum::<i64>(), 7);
    assert_eq!(messages.iter().map(|m| m.tokens.reasoning).sum::<i64>(), 3);
    let cost: f64 = messages.iter().map(|m| m.cost).sum();
    assert!(
        (cost - 1.0).abs() < 1e-12,
        "cost should sum to 1.0, got {cost}"
    );

    // The start day no longer carries the whole session.
    let day_of = |m: &&tokens_core::UnifiedMessage| ((m.timestamp / 1000) as f64 - DAY0) / DAY;
    let input_on = |day: f64| -> i64 {
        messages
            .iter()
            .filter(|m| day_of(m).floor() == day)
            .map(|m| m.tokens.input)
            .sum()
    };
    assert_eq!(
        (input_on(0.0), input_on(1.0), input_on(2.0)),
        (333, 167, 500)
    );

    // sessions.message_count stays on one piece.
    let counts: Vec<i32> = messages.iter().map(|m| m.message_count).collect();
    assert_eq!(counts, vec![12, 0, 0, 0, 0, 0]);

    let keys: HashSet<String> = messages
        .iter()
        .filter_map(|m| m.dedup_key.clone())
        .collect();
    assert_eq!(keys.len(), 6);
    assert!(keys.contains("hermes:long:claude-sonnet-4:anthropic#5"));
}

#[test]
fn test_parse_hermes_sqlite_spreads_each_model_over_its_own_window() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    add_messages_table(&conn);

    insert_session(&conn, "switch", "claude-sonnet-4", DAY0, 4);
    insert_usage(
        &conn,
        "switch",
        "claude-sonnet-4",
        400,
        40,
        0.4,
        Some(DAY0),
        Some(DAY0 + 200.0),
    );
    insert_usage(
        &conn,
        "switch",
        "glm-5.2",
        900,
        90,
        0.0,
        Some(DAY0 + DAY),
        Some(DAY0 + DAY + 200.0),
    );
    for ts in [
        DAY0 + 100.0,
        DAY0 + 150.0,
        DAY0 + DAY + 100.0,
        DAY0 + DAY + 150.0,
        DAY0 + DAY + 190.0,
    ] {
        insert_message(&conn, "switch", "assistant", ts);
    }

    let messages = parse_hermes_sqlite(&db_path);
    let times_for = |model: &str| -> Vec<i64> {
        let mut times: Vec<i64> = messages
            .iter()
            .filter(|m| m.model_id == model)
            .map(|m| m.timestamp)
            .collect();
        times.sort();
        times
    };
    assert_eq!(
        times_for("claude-sonnet-4"),
        vec![ms(DAY0 + 100.0), ms(DAY0 + 150.0)]
    );
    assert_eq!(
        times_for("glm-5.2"),
        vec![
            ms(DAY0 + DAY + 100.0),
            ms(DAY0 + DAY + 150.0),
            ms(DAY0 + DAY + 190.0)
        ]
    );
    let glm_input: i64 = messages
        .iter()
        .filter(|m| m.model_id == "glm-5.2")
        .map(|m| m.tokens.input)
        .sum();
    assert_eq!(glm_input, 900);
    assert_eq!(messages.iter().map(|m| m.message_count).sum::<i32>(), 4);
}

#[test]
fn test_parse_hermes_sqlite_keeps_rows_without_window_messages_whole() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    add_messages_table(&conn);

    // Messages exist, but none inside this row's window: it stays whole at
    // the time its usage was first recorded, not at the session start.
    insert_session(&conn, "aux", "claude-sonnet-4", DAY0, 3);
    insert_usage(
        &conn,
        "aux",
        "claude-sonnet-4",
        500,
        50,
        0.5,
        Some(DAY0 + DAY),
        Some(DAY0 + DAY + 5.0),
    );
    insert_message(&conn, "aux", "assistant", DAY0 + 10.0);
    // No window and no messages at all: the session start is all there is.
    insert_session(&conn, "bare", "claude-sonnet-4", DAY0 + 7.0, 1);
    insert_usage(&conn, "bare", "claude-sonnet-4", 10, 1, 0.0, None, None);

    let messages = parse_hermes_sqlite(&db_path);
    assert_eq!(messages.len(), 2);
    let aux = messages.iter().find(|m| m.session_id == "aux").unwrap();
    assert_eq!(aux.timestamp, ms(DAY0 + DAY));
    assert_eq!((aux.tokens.input, aux.message_count), (500, 3));
    let bare = messages.iter().find(|m| m.session_id == "bare").unwrap();
    assert_eq!(bare.timestamp, ms(DAY0 + 7.0));
}

#[test]
fn test_parse_hermes_sqlite_drops_empty_pieces_but_keeps_the_count() {
    let dir = TempDir::new().unwrap();
    let db_path = create_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    add_messages_table(&conn);

    insert_session(&conn, "tiny", "claude-sonnet-4", DAY0, 9);
    insert_usage(
        &conn,
        "tiny",
        "claude-sonnet-4",
        2,
        0,
        0.0,
        Some(DAY0),
        Some(DAY0 + 100.0),
    );
    for i in 1..=5 {
        insert_message(&conn, "tiny", "assistant", DAY0 + f64::from(i));
    }

    let mut messages = parse_hermes_sqlite(&db_path);
    messages.sort_by_key(|m| m.timestamp);
    // Shares 0, 0, 1, 0, 1: the first piece survives for its message count,
    // the other zero pieces carry nothing and are dropped.
    let pieces: Vec<(i64, i64, i32)> = messages
        .iter()
        .map(|m| (m.timestamp, m.tokens.input, m.message_count))
        .collect();
    assert_eq!(
        pieces,
        vec![
            (ms(DAY0 + 1.0), 0, 9),
            (ms(DAY0 + 3.0), 1, 0),
            (ms(DAY0 + 5.0), 1, 0),
        ]
    );
}

#[test]
fn test_parse_hermes_sqlite_spreads_session_totals_from_their_start() {
    let dir = TempDir::new().unwrap();
    let db_path = create_legacy_test_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    add_messages_table(&conn);

    conn.execute(
        r#"
        INSERT INTO sessions (
            id, source, model, started_at, message_count, input_tokens, output_tokens,
            billing_provider, actual_cost_usd
        ) VALUES ('legacy-long', 'cli', 'claude-sonnet-4', ?1, 6, 300, 30, 'anthropic', 0.3)
        "#,
        params![DAY0 + 100.0],
    )
    .unwrap();
    // A parent message carried into a rotated session keeps its older time.
    insert_message(&conn, "legacy-long", "assistant", DAY0);
    for ts in [DAY0 + 200.0, DAY0 + DAY + 200.0, DAY0 + 3.0 * DAY] {
        insert_message(&conn, "legacy-long", "assistant", ts);
    }

    let mut messages = parse_hermes_sqlite(&db_path);
    messages.sort_by_key(|m| m.timestamp);
    let pieces: Vec<(i64, i64)> = messages
        .iter()
        .map(|m| (m.timestamp, m.tokens.input))
        .collect();
    assert_eq!(
        pieces,
        vec![
            (ms(DAY0 + 200.0), 100),
            (ms(DAY0 + DAY + 200.0), 100),
            (ms(DAY0 + 3.0 * DAY), 100),
        ]
    );
    assert_eq!(messages[0].dedup_key.as_deref(), Some("legacy-long#0"));
}

#[test]
fn test_parse_hermes_databases_dedups_rows_before_spreading() {
    let dir = TempDir::new().unwrap();
    let first_dir = dir.path().join("first");
    let second_dir = dir.path().join("second");
    std::fs::create_dir_all(&first_dir).unwrap();
    std::fs::create_dir_all(&second_dir).unwrap();
    let first = TempDir::new_in(&first_dir).unwrap();
    let second = TempDir::new_in(&second_dir).unwrap();

    // The first copy of the session has no messages and stays one piece.
    let first_db = create_test_db(&first);
    let conn = Connection::open(&first_db).unwrap();
    insert_session(&conn, "shared", "claude-sonnet-4", DAY0, 2);
    insert_usage(
        &conn,
        "shared",
        "claude-sonnet-4",
        100,
        10,
        0.1,
        Some(DAY0),
        Some(DAY0 + DAY),
    );
    drop(conn);

    // The second copy would spread the same row over three messages; none of
    // its pieces may be added on top of the first copy's row.
    let second_db = create_test_db(&second);
    let conn = Connection::open(&second_db).unwrap();
    add_messages_table(&conn);
    insert_session(&conn, "shared", "claude-sonnet-4", DAY0, 9);
    insert_usage(
        &conn,
        "shared",
        "claude-sonnet-4",
        999,
        99,
        9.9,
        Some(DAY0),
        Some(DAY0 + DAY),
    );
    insert_session(&conn, "only-second", "claude-sonnet-4", DAY0, 1);
    insert_usage(
        &conn,
        "only-second",
        "claude-sonnet-4",
        50,
        5,
        0.0,
        Some(DAY0),
        Some(DAY0 + DAY),
    );
    for ts in [DAY0 + 1.0, DAY0 + 2.0, DAY0 + 3.0] {
        insert_message(&conn, "shared", "assistant", ts);
    }
    drop(conn);

    let messages = parse_hermes_databases(&[first_db, second_db]);
    let shared: Vec<_> = messages
        .iter()
        .filter(|m| m.session_id == "shared")
        .collect();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].tokens.input, 100);
    assert!(messages.iter().any(|m| m.session_id == "only-second"));
}
