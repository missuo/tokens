use serde_json::{json, Value};
use std::path::Path;

fn run_usage_json(profile: &Path, args: &[&str]) -> Value {
    let output = assert_cmd::cargo::cargo_bin_cmd!("tokens")
        .env("HOME", profile)
        .env("TOKENS_CONFIG_DIR", profile.join("tokens-config"))
        .args(["usage", "--json"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn object_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("expected JSON object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn normalized_duration(mut report: Value) -> Value {
    report["scan"]["durationMs"] = json!(0);
    report
}

#[test]
fn usage_binary_emits_v3_for_preset_and_custom_requests() {
    let profile = tempfile::tempdir().unwrap();
    for (range_args, expected_selection, expected_mode, expected_rebuilt) in [
        (
            vec!["--period", "today"],
            json!({"kind": "preset", "preset": "today"}),
            "incremental",
            true,
        ),
        (
            vec!["--since", "2000-01-01", "--until", "2000-01-02"],
            json!({
                "kind": "custom",
                "startDate": "2000-01-01",
                "endDate": "2000-01-02"
            }),
            "snapshot",
            false,
        ),
    ] {
        let mut args = vec!["--contract", "v3"];
        args.extend(range_args);
        let report = run_usage_json(profile.path(), &args);
        assert_eq!(report["schemaVersion"], 3);
        assert_eq!(report["selection"], expected_selection);
        assert_eq!(report["scan"]["mode"], expected_mode);
        assert_eq!(report["scan"]["cache"]["snapshotRebuilt"], expected_rebuilt);
        assert_eq!(report["scan"]["cache"]["snapshotSchemaVersion"], 3);
        assert_eq!(report["meta"]["reportContract"], "v3");
    }
}

#[test]
fn omitted_and_explicit_v2_today_are_the_same_v2_shape() {
    let profile = tempfile::tempdir().unwrap();
    let omitted = run_usage_json(profile.path(), &[]);
    let explicit = run_usage_json(profile.path(), &["--contract", "v2", "--period", "today"]);

    for report in [&omitted, &explicit] {
        assert_eq!(report["schemaVersion"], 2);
        assert_eq!(report["period"], "today");
        assert_eq!(
            object_keys(report),
            [
                "byClient",
                "byDay",
                "byModel",
                "byProject",
                "dateRange",
                "generatedAt",
                "meta",
                "period",
                "scan",
                "schemaVersion",
                "summary",
                "tokenBreakdown",
            ]
        );
        assert_eq!(object_keys(&report["dateRange"]), ["end", "start"]);
        assert_eq!(
            object_keys(&report["scan"]),
            ["cache", "durationMs", "forceRescan", "mode"]
        );
        assert_eq!(
            object_keys(&report["scan"]["cache"]),
            ["snapshotRebuilt", "sourceHits", "sourceMisses"]
        );
        assert_eq!(
            object_keys(&report["summary"]),
            [
                "activeDays",
                "clients",
                "messages",
                "models",
                "totalCost",
                "totalTokens",
            ]
        );
        assert_eq!(
            object_keys(&report["tokenBreakdown"]),
            ["cacheRead", "cacheWrite", "input", "output", "reasoning"]
        );
        assert_eq!(object_keys(&report["meta"]), ["cliVersion", "timezone"]);
        assert!(report.get("selection").is_none());
        assert!(report.get("timeSeries").is_none());
        assert!(report["scan"]["cache"]
            .get("snapshotSchemaVersion")
            .is_none());
        assert!(report["meta"].get("reportContract").is_none());
    }

    assert_eq!(omitted["scan"]["mode"], "incremental");
    assert_eq!(omitted["scan"]["cache"]["snapshotRebuilt"], true);
    assert!(omitted["generatedAt"]
        .as_str()
        .is_some_and(|generated_at| !generated_at.is_empty()));

    assert_eq!(explicit["scan"]["mode"], "snapshot");
    assert_eq!(explicit["scan"]["cache"]["snapshotRebuilt"], false);
    assert_eq!(explicit["generatedAt"], omitted["generatedAt"]);

    let mut expected_reuse = normalized_duration(omitted);
    expected_reuse["scan"]["mode"] = json!("snapshot");
    expected_reuse["scan"]["cache"]["snapshotRebuilt"] = json!(false);
    assert_eq!(normalized_duration(explicit), expected_reuse);
}
