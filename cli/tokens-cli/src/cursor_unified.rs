//! Offline import of the unified dashboard's billing export. This is deliberately
//! separate from legacy Cursor authentication and API sync.
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

const SOURCE: &str = "grok-unified-billing-csv-v1";

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "Billing Item ID")]
    id: String,
    #[serde(rename = "Event Time")]
    time: String,
    #[serde(rename = "Product")]
    product: String,
    #[serde(rename = "Charge Category")]
    category: String,
    #[serde(rename = "Principal Type")]
    principal_type: String,
    #[serde(rename = "Principal ID")]
    principal: String,
    #[serde(rename = "Usage")]
    usage: String,
    #[serde(rename = "Pricing Currency")]
    currency: String,
    #[serde(rename = "Pricing Contracted")]
    price: String,
    #[serde(rename = "Model")]
    model: String,
    #[serde(rename = "User Email")]
    email: String,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Tokens {
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    billing_item_id: String,
    timestamp: i64,
    model: String,
    token_usage: Tokens,
    // Actual billed USD, not the legacy API's metered tokenUsage.totalCents.
    charged_cents: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cache {
    source: String,
    principal_id: String,
    after_ms: i64,
    cost_basis: String,
    usage_events_display: Vec<Event>,
}

fn timestamp(value: &str) -> Result<i64> {
    let ms = chrono::DateTime::parse_from_rfc3339(value)?.timestamp_millis();
    ensure!(ms > 0, "Timestamp must be after the Unix epoch");
    Ok(ms)
}

fn parse_tokens(value: &str) -> Result<Tokens> {
    let values: BTreeMap<String, serde_json::Value> = serde_json::from_str(value)?;
    ensure!(!values.is_empty(), "Empty token breakdown");
    let mut tokens = Tokens::default();
    let mut total = 0i64;
    for (key, value) in values {
        let count = match &value {
            serde_json::Value::String(s) => s.parse::<i64>().ok(),
            _ => value.as_i64(),
        }
        .filter(|n| *n >= 0)
        .with_context(|| format!("Invalid token count for {key}"))?;
        total = total.checked_add(count).context("Token total overflow")?;
        match key.as_str() {
            "prompt-text-tokens" => tokens.input_tokens = count,
            "completion-text-tokens" => tokens.output_tokens = count,
            "cached-prompt-text-tokens" => tokens.cache_read_tokens = count,
            "cache-write-text-tokens" => tokens.cache_write_tokens = count,
            _ => bail!("Unsupported Cursor usage dimension: {key}"),
        }
    }
    Ok(tokens)
}

fn parse_csv(file: &Path, email: &str, after_ms: i64) -> Result<Cache> {
    ensure!(!email.trim().is_empty(), "Expected email must not be empty");
    let mut reader = csv::Reader::from_path(file)?;
    let mut principal = None;
    let mut events = BTreeMap::new();
    for (index, row) in reader.deserialize::<Row>().enumerate() {
        let row = row.with_context(|| format!("Invalid CSV row {}", index + 2))?;
        if row.product != "cursor" || row.category != "usage" {
            continue;
        }
        // Check identity even for rows outside the selected time range.
        ensure!(
            row.email.eq_ignore_ascii_case(email.trim()),
            "Cursor export contains a different user email (row {})",
            index + 2
        );
        ensure!(
            row.principal_type == "user" && !row.principal.is_empty(),
            "Expected a user principal (row {})",
            index + 2
        );
        if let Some(id) = &principal {
            ensure!(
                id == &row.principal,
                "Export contains multiple Cursor principals"
            );
        } else {
            principal = Some(row.principal.clone());
        }
        let time = timestamp(&row.time)
            .with_context(|| format!("Invalid event time (row {})", index + 2))?;
        if time <= after_ms {
            continue;
        }
        ensure!(
            !row.id.is_empty() && !row.model.trim().is_empty(),
            "Missing billing item ID or model (row {})",
            index + 2
        );
        ensure!(
            row.currency == "USD",
            "Only USD billing exports are supported"
        );
        let charged_cents = row
            .price
            .parse::<f64>()
            .context("Invalid Pricing Contracted")?
            * 100.0;
        ensure!(
            charged_cents.is_finite() && charged_cents >= 0.0,
            "Invalid billed cost"
        );
        let event = Event {
            billing_item_id: row.id.clone(),
            timestamp: time,
            model: row.model,
            token_usage: parse_tokens(&row.usage)
                .with_context(|| format!("Invalid usage (row {})", index + 2))?,
            charged_cents,
        };
        if let Some(previous) = events.insert(row.id, event) {
            ensure!(
                events.get(&previous.billing_item_id) == Some(&previous),
                "Conflicting duplicate billing item ID"
            );
        }
    }
    ensure!(
        !events.is_empty(),
        "No Cursor usage after the cutoff in this export"
    );
    Ok(Cache {
        source: SOURCE.into(),
        principal_id: principal.context("No Cursor user principal")?,
        after_ms,
        cost_basis: "actual-billed-usd".into(),
        usage_events_display: events.into_values().collect(),
    })
}

fn merge(existing: Cache, incoming: &mut Cache) -> Result<()> {
    ensure!(
        existing.source == SOURCE && existing.cost_basis == incoming.cost_basis,
        "Refusing to replace an unrelated cache"
    );
    ensure!(
        existing.principal_id == incoming.principal_id && existing.after_ms == incoming.after_ms,
        "Existing import has a different principal or cutoff"
    );
    let mut events = BTreeMap::new();
    for event in existing
        .usage_events_display
        .into_iter()
        .chain(std::mem::take(&mut incoming.usage_events_display))
    {
        ensure!(
            event.timestamp > incoming.after_ms,
            "Existing import contains usage before the cutoff"
        );
        if let Some(previous) = events.insert(event.billing_item_id.clone(), event) {
            ensure!(
                events.get(&previous.billing_item_id) == Some(&previous),
                "Conflicting billing item ID; original cache has not been changed"
            );
        }
    }
    incoming.usage_events_display = events.into_values().collect();
    Ok(())
}

/// A conservative overlap check: legacy events must all precede the explicit
/// cutover. Billing IDs cannot be matched to legacy conversation IDs.
fn check_legacy_cutoff(dir: &Path, after_ms: i64) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !(name == "usage.json" || name == "usage.csv" || name.starts_with("usage."))
            || name.starts_with("usage.unified-")
            || name.starts_with("usage.backup")
            || !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("json" | "csv")
            )
        {
            continue;
        }
        let overlap = tokens_core::sessions::cursor::parse_cursor_file(&path)
            .iter()
            .any(|event| event.timestamp > after_ms);
        ensure!(
            !overlap,
            "Legacy cache {} contains events after --after; choose a non-overlapping cutoff",
            path.display()
        );
    }
    Ok(())
}

pub fn run_import(
    file: &Path,
    email: &str,
    after: &str,
    output_dir: &Path,
    dry_run: bool,
) -> Result<()> {
    let mut cache = parse_csv(file, email, timestamp(after)?)?;
    let digest = format!("{:x}", Sha256::digest(cache.principal_id.as_bytes()));
    let output = output_dir.join(format!("usage.unified-{digest}.json"));
    // Serialize import writers so two overlapping exports cannot lose events.
    // Dry runs never create a directory or lock file.
    let _lock = if dry_run {
        None
    } else {
        fs::create_dir_all(output_dir)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(output_dir.join(".unified-import.lock"))?;
        lock.lock_exclusive()?;
        Some(lock)
    };
    check_legacy_cutoff(output_dir, cache.after_ms)?;
    if output.exists() {
        let existing = serde_json::from_slice(&fs::read(&output)?)
            .context("Cannot read existing unified import; refusing to overwrite it")?;
        merge(existing, &mut cache)?;
    }
    let count = cache.usage_events_display.len();
    let total: i128 = cache
        .usage_events_display
        .iter()
        .map(|e| {
            let t = &e.token_usage;
            i128::from(t.input_tokens)
                + i128::from(t.output_tokens)
                + i128::from(t.cache_read_tokens)
                + i128::from(t.cache_write_tokens)
        })
        .sum();
    if !dry_run {
        crate::cursor::atomic_write_file(&output, &serde_json::to_string(&cache)?)?;
    }
    println!(
        "{} {count} Cursor billing items ({total} tokens) at {}",
        if dry_run { "Would store" } else { "Stored" },
        output.display()
    );
    println!("Cost basis: actual billed USD (included usage may be $0); conversation IDs are unavailable.");
    println!("Offline import only. Review with tokens submit --client cursor --dry-run before submitting.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const AFTER: &str = "2026-10-14T12:00:00Z";
    const EMAIL: &str = "sample@example.test";

    fn row(id: &str) -> Vec<String> {
        [id, "2026-10-20T10:00:00Z", "cursor", "usage", "included", "user", "user_sample", r#"{"prompt-text-tokens":"10","completion-text-tokens":"5","cached-prompt-text-tokens":"20","cache-write-text-tokens":"3"}"#, "USD", "0", "example-model", EMAIL]
            .into_iter().map(str::to_string).collect()
    }

    fn write_csv(path: &Path, rows: &[Vec<String>]) {
        let mut writer = csv::Writer::from_path(path).unwrap();
        writer
            .write_record([
                "Billing Item ID",
                "Event Time",
                "Product",
                "Charge Category",
                "Usage Type",
                "Principal Type",
                "Principal ID",
                "Usage",
                "Pricing Currency",
                "Pricing Contracted",
                "Model",
                "User Email",
            ])
            .unwrap();
        for row in rows {
            writer.write_record(row).unwrap();
        }
        writer.flush().unwrap();
    }

    fn output(dir: &Path) -> std::path::PathBuf {
        dir.join(format!(
            "usage.unified-{:x}.json",
            Sha256::digest(b"user_sample")
        ))
    }

    #[test]
    fn import_is_idempotent_and_preserves_legacy_and_previous_exports() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("cache");
        fs::create_dir(&dir).unwrap();
        let legacy = json!({"usageEventsDisplay": [{"timestamp": timestamp(AFTER).unwrap(), "model": "legacy", "tokenUsage": {"inputTokens": 99}}]}).to_string();
        fs::write(dir.join("usage.json"), &legacy).unwrap();
        let csv = temp.path().join("export.csv");
        let included = row("a");
        let mut paid = row("b");
        paid[4] = "on_demand".into();
        paid[9] = "0.1234".into();
        let mut other_product = row("c");
        other_product[2] = "grok-chat".into();
        other_product[7] = "not a Cursor token breakdown".into();
        let mut old = row("d");
        old[1] = AFTER.into();
        write_csv(
            &csv,
            &[included.clone(), included, paid, other_product, old],
        );
        run_import(&csv, EMAIL, AFTER, &dir, false).unwrap();
        let path = output(&dir);
        let first = fs::read(&path).unwrap();
        run_import(&csv, EMAIL, AFTER, &dir, false).unwrap();
        assert_eq!(fs::read(&path).unwrap(), first);
        assert_eq!(fs::read_to_string(dir.join("usage.json")).unwrap(), legacy);
        // Exercise the actual core parser, not just the serialized shape.
        let messages = tokens_core::sessions::cursor::parse_cursor_file(&path);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].tokens.input, 10);
        assert_eq!(messages[0].tokens.output, 5);
        assert_eq!(messages[0].tokens.cache_read, 20);
        assert_eq!(messages[0].tokens.cache_write, 3);
        assert_eq!(messages[0].cost, 0.0);
        assert_eq!(
            messages[0].cost_source,
            tokens_core::sessions::CostSource::ProviderReported
        );
        assert!((messages[1].cost - 0.1234).abs() < 1e-12);
        assert!(messages[0].session_id.starts_with("cursor-unified-"));
        write_csv(&csv, &[row("e")]);
        run_import(&csv, EMAIL, AFTER, &dir, false).unwrap();
        assert_eq!(
            tokens_core::sessions::cursor::parse_cursor_file(&path).len(),
            3
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn invalid_exports_and_conflicts_leave_existing_cache_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let csv = temp.path().join("export.csv");
        let dir = temp.path().join("cache");
        write_csv(&csv, &[row("a")]);
        run_import(&csv, EMAIL, AFTER, &dir, false).unwrap();
        let before = fs::read(output(&dir)).unwrap();
        for (column, invalid) in [
            (11, "other@example.test"),
            (5, "team"),
            (8, "EUR"),
            (9, "NaN"),
            (9, "-1"),
            (10, ""),
            (0, ""),
            (1, "yesterday"),
            (7, r#"{"prompt-text-tokens":"-1"}"#),
            (7, r#"{"new-token-dimension":"10"}"#),
            (7, r#"{"prompt-text-tokens":"1.5"}"#),
            (
                7,
                r#"{"prompt-text-tokens":9223372036854775807,"completion-text-tokens":1}"#,
            ),
            (9, "1.00"),
        ] {
            let mut invalid_row = row("a");
            invalid_row[column] = invalid.into();
            write_csv(&csv, &[invalid_row]);
            assert!(
                run_import(&csv, EMAIL, AFTER, &dir, false).is_err(),
                "column {column}, value {invalid}"
            );
            assert_eq!(fs::read(output(&dir)).unwrap(), before);
        }
        write_csv(&csv, &[row("a")]);
        assert!(run_import(&csv, EMAIL, "2026-10-15T00:00:00Z", &dir, false).is_err());
        assert_eq!(fs::read(output(&dir)).unwrap(), before);
    }

    #[test]
    fn rejects_legacy_overlap_and_mixed_principals() {
        let temp = tempfile::tempdir().unwrap();
        let csv = temp.path().join("export.csv");
        let dir = temp.path().join("cache");
        fs::create_dir(&dir).unwrap();
        write_csv(&csv, &[row("a")]);
        fs::write(dir.join("usage.json"), json!({"usageEventsDisplay": [{"timestamp": "2026-10-20T00:00:00Z", "model": "legacy"}]}).to_string()).unwrap();
        assert!(run_import(&csv, EMAIL, AFTER, &dir, false)
            .unwrap_err()
            .to_string()
            .contains("non-overlapping"));
        assert!(!output(&dir).exists());
        let mut second = row("b");
        second[6] = "another-principal".into();
        write_csv(&csv, &[row("a"), second]);
        assert!(parse_csv(&csv, EMAIL, timestamp(AFTER).unwrap()).is_err());
    }

    #[test]
    fn dry_run_writes_nothing_and_rejects_conflicting_duplicate_ids() {
        let temp = tempfile::tempdir().unwrap();
        let csv = temp.path().join("export.csv");
        let dir = temp.path().join("not-created");
        write_csv(&csv, &[row("a")]);
        run_import(&csv, EMAIL, AFTER, &dir, true).unwrap();
        assert!(!dir.exists());
        let mut duplicate = row("a");
        duplicate[7] = r#"{"prompt-text-tokens":"15"}"#.into();
        write_csv(&csv, &[row("a"), duplicate]);
        assert!(run_import(&csv, EMAIL, AFTER, &dir, true).is_err());
        assert!(!dir.exists());
    }
}
