use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokens_core::{
    ClientContribution, DailyContribution, DailyHourlyUsageFacts, DailyTotals, LocalUsageScan,
    ProjectContribution, ProjectModelContribution, TokenBreakdown,
};

use super::cost_checks::{checked_cost_sum, cost_matches};

pub(crate) const SNAPSHOT_SCHEMA_VERSION: u32 = 3;
pub(crate) const SNAPSHOT_FILENAME: &str = "usage-snapshot-v3.json";
pub(crate) const V2_SNAPSHOT_FILENAME: &str = "usage-snapshot-v2.json";
pub(crate) const V1_SNAPSHOT_FILENAME: &str = "usage-snapshot-v1.json";
const OPERATION_LOCK_FILENAME: &str = "usage-snapshot-operation.lock";

static SNAPSHOT_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSnapshot {
    pub(crate) schema_version: u32,
    pub(crate) generated_at: String,
    pub(crate) bucket_date: String,
    pub(crate) timezone: String,
    pub(crate) days: Vec<UsageSnapshotDay>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSnapshotDay {
    pub(crate) date: String,
    pub(crate) totals: UsageSnapshotTotals,
    pub(crate) token_breakdown: UsageSnapshotTokenBreakdown,
    pub(crate) clients: Vec<UsageSnapshotClient>,
    pub(crate) projects: Vec<UsageSnapshotProject>,
    pub(crate) hours: Vec<UsageSnapshotHour>,
    pub(crate) unplaced_for_hourly: UsageSnapshotTotals,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSnapshotTotals {
    pub(crate) tokens: i64,
    pub(crate) cost: f64,
    pub(crate) messages: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSnapshotTokenBreakdown {
    pub(crate) input: i64,
    pub(crate) output: i64,
    pub(crate) cache_read: i64,
    pub(crate) cache_write: i64,
    pub(crate) reasoning: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSnapshotClient {
    pub(crate) client: String,
    pub(crate) model_id: String,
    pub(crate) provider_id: String,
    pub(crate) token_breakdown: UsageSnapshotTokenBreakdown,
    pub(crate) cost: f64,
    pub(crate) messages: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSnapshotProject {
    pub(crate) project_key: Option<String>,
    pub(crate) display_name: String,
    pub(crate) totals: UsageSnapshotTotals,
    pub(crate) models: Vec<UsageSnapshotProjectModel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSnapshotProjectModel {
    pub(crate) model_id: String,
    pub(crate) provider_id: String,
    pub(crate) totals: UsageSnapshotTotals,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSnapshotHour {
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
    pub(crate) totals: UsageSnapshotTotals,
}

pub(crate) fn build_snapshot(
    scan: &LocalUsageScan,
    bucket_date: &str,
    timezone: &str,
) -> Result<UsageSnapshot> {
    let mut hourly_by_date: BTreeMap<&str, &DailyHourlyUsageFacts> = scan
        .hourly_facts
        .iter()
        .map(|facts| (facts.date.as_str(), facts))
        .collect();
    if hourly_by_date.len() != scan.hourly_facts.len() {
        bail!("duplicate hourly usage date in local scan");
    }

    let days = scan
        .graph
        .contributions
        .iter()
        .map(|day| {
            let hourly = hourly_by_date
                .remove(day.date.as_str())
                .with_context(|| format!("missing hourly usage facts for {}", day.date))?;
            Ok(UsageSnapshotDay {
                date: day.date.clone(),
                totals: snapshot_totals(&day.totals),
                token_breakdown: snapshot_token_breakdown(&day.token_breakdown),
                clients: day
                    .clients
                    .iter()
                    .map(|client| UsageSnapshotClient {
                        client: client.client.clone(),
                        model_id: client.model_id.clone(),
                        provider_id: client.provider_id.clone(),
                        token_breakdown: snapshot_token_breakdown(&client.tokens),
                        cost: client.cost,
                        messages: client.messages,
                    })
                    .collect(),
                projects: day
                    .projects
                    .iter()
                    .map(|project| UsageSnapshotProject {
                        project_key: project.project_key.clone(),
                        display_name: project.project_label.clone(),
                        totals: snapshot_totals(&project.totals),
                        models: project
                            .models
                            .iter()
                            .map(|model| UsageSnapshotProjectModel {
                                model_id: model.model_id.clone(),
                                provider_id: model.provider_id.clone(),
                                totals: UsageSnapshotTotals {
                                    tokens: model.tokens,
                                    cost: model.cost,
                                    messages: model.messages,
                                },
                            })
                            .collect(),
                    })
                    .collect(),
                hours: hourly
                    .hours
                    .iter()
                    .map(|hour| UsageSnapshotHour {
                        start_ms: hour.start_ms,
                        end_ms: hour.end_ms,
                        totals: snapshot_totals(&hour.totals),
                    })
                    .collect(),
                unplaced_for_hourly: snapshot_totals(&hourly.unplaced_for_hourly),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if let Some(extra) = hourly_by_date.keys().next() {
        bail!("hourly usage facts have no daily contribution for {extra}");
    }

    let snapshot = UsageSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        generated_at: scan.graph.meta.generated_at.clone(),
        bucket_date: bucket_date.to_string(),
        timezone: timezone.to_string(),
        days,
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

pub(crate) fn snapshot_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(SNAPSHOT_FILENAME)
}

pub(crate) struct SnapshotOperationGuard {
    file: fs::File,
}

impl Drop for SnapshotOperationGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub(crate) fn acquire_shared_operation_lock(cache_dir: &Path) -> Result<SnapshotOperationGuard> {
    acquire_operation_lock(cache_dir, false)
}

pub(crate) fn acquire_exclusive_operation_lock(cache_dir: &Path) -> Result<SnapshotOperationGuard> {
    acquire_operation_lock(cache_dir, true)
}

fn acquire_operation_lock(cache_dir: &Path, exclusive: bool) -> Result<SnapshotOperationGuard> {
    fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join(OPERATION_LOCK_FILENAME);
    let file = open_private_snapshot_lock(&path)?;
    if exclusive {
        FileExt::lock_exclusive(&file)
            .with_context(|| format!("lock usage snapshot operation {}", path.display()))?;
    } else {
        FileExt::lock_shared(&file)
            .with_context(|| format!("lock usage snapshot operation {}", path.display()))?;
    }
    Ok(SnapshotOperationGuard { file })
}

pub(crate) fn load_reusable_snapshot(
    cache_dir: &Path,
    expected_bucket_date: &str,
    expected_timezone: &str,
) -> Option<UsageSnapshot> {
    let raw = fs::read(snapshot_path(cache_dir)).ok()?;
    let snapshot: UsageSnapshot = serde_json::from_slice(&raw).ok()?;
    validate_snapshot(&snapshot).ok()?;
    if snapshot.bucket_date != expected_bucket_date || snapshot.timezone != expected_timezone {
        return None;
    }
    Some(snapshot)
}

pub(crate) fn daily_contributions(snapshot: &UsageSnapshot) -> Vec<DailyContribution> {
    let mut contributions: Vec<DailyContribution> = snapshot
        .days
        .iter()
        .map(|day| DailyContribution {
            date: day.date.clone(),
            totals: DailyTotals {
                tokens: day.totals.tokens,
                cost: day.totals.cost,
                messages: day.totals.messages,
            },
            intensity: 0,
            token_breakdown: TokenBreakdown {
                input: day.token_breakdown.input,
                output: day.token_breakdown.output,
                cache_read: day.token_breakdown.cache_read,
                cache_write: day.token_breakdown.cache_write,
                reasoning: day.token_breakdown.reasoning,
            },
            clients: day
                .clients
                .iter()
                .map(|client| ClientContribution {
                    client: client.client.clone(),
                    model_id: client.model_id.clone(),
                    provider_id: client.provider_id.clone(),
                    tokens: TokenBreakdown {
                        input: client.token_breakdown.input,
                        output: client.token_breakdown.output,
                        cache_read: client.token_breakdown.cache_read,
                        cache_write: client.token_breakdown.cache_write,
                        reasoning: client.token_breakdown.reasoning,
                    },
                    cost: client.cost,
                    messages: client.messages,
                })
                .collect(),
            projects: day
                .projects
                .iter()
                .map(|project| ProjectContribution {
                    project_key: project.project_key.clone(),
                    project_label: project.display_name.clone(),
                    totals: DailyTotals {
                        tokens: project.totals.tokens,
                        cost: project.totals.cost,
                        messages: project.totals.messages,
                    },
                    models: project
                        .models
                        .iter()
                        .map(|model| ProjectModelContribution {
                            model_id: model.model_id.clone(),
                            provider_id: model.provider_id.clone(),
                            tokens: model.totals.tokens,
                            cost: model.totals.cost,
                            messages: model.totals.messages,
                        })
                        .collect(),
                })
                .collect(),
            active_time_ms: None,
        })
        .collect();
    tokens_core::calculate_intensities(&mut contributions);
    contributions
}

pub(crate) fn write_snapshot(cache_dir: &Path, snapshot: &UsageSnapshot) -> Result<()> {
    validate_snapshot(snapshot)?;
    fs::create_dir_all(cache_dir)?;
    let body = serde_json::to_vec_pretty(snapshot)?;
    write_private_snapshot(&snapshot_path(cache_dir), &body)?;
    let _ = fs::remove_file(cache_dir.join(V2_SNAPSHOT_FILENAME));
    let _ = fs::remove_file(cache_dir.join(V1_SNAPSHOT_FILENAME));
    Ok(())
}

pub(crate) fn clear_usage_snapshots(cache_dir: &Path) -> Result<()> {
    let mut failures = Vec::new();
    for filename in [
        SNAPSHOT_FILENAME,
        V2_SNAPSHOT_FILENAME,
        V1_SNAPSHOT_FILENAME,
    ] {
        let path = cache_dir.join(filename);
        if let Err(error) = fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                failures.push(format!("remove {}: {error}", path.display()));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!("failed to clear usage snapshots: {}", failures.join("; "))
    }
}

pub(crate) fn validate_snapshot(snapshot: &UsageSnapshot) -> Result<()> {
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        bail!("unsupported usage snapshot schema");
    }
    chrono::DateTime::parse_from_rfc3339(&snapshot.generated_at)
        .context("invalid snapshot generatedAt")?;
    parse_date(&snapshot.bucket_date).context("invalid snapshot bucket date")?;
    let timezone = snapshot_timezone(&snapshot.timezone).context("invalid snapshot timezone")?;

    let mut previous_day: Option<&str> = None;
    for day in &snapshot.days {
        parse_date(&day.date).with_context(|| format!("invalid snapshot day {}", day.date))?;
        if previous_day.is_some_and(|previous| previous >= day.date.as_str()) {
            bail!("snapshot days must be strictly sorted by date");
        }
        previous_day = Some(&day.date);
        validate_totals(&day.totals, "daily totals")?;
        let token_total = validate_token_breakdown(&day.token_breakdown, "daily token breakdown")?;
        if token_total != day.totals.tokens {
            bail!(
                "daily token breakdown does not conserve tokens for {}",
                day.date
            );
        }

        let mut client_totals = UsageSnapshotTotals::default();
        for client in &day.clients {
            let tokens =
                validate_token_breakdown(&client.token_breakdown, "client token breakdown")?;
            let totals = UsageSnapshotTotals {
                tokens,
                cost: client.cost,
                messages: client.messages,
            };
            validate_totals(&totals, "client totals")?;
            add_totals_checked(&mut client_totals, &totals, "client totals")?;
        }
        require_totals_match(
            &client_totals,
            &day.totals,
            &format!("clients do not conserve daily totals for {}", day.date),
        )?;

        let mut project_totals = UsageSnapshotTotals::default();
        for project in &day.projects {
            validate_totals(&project.totals, "project totals")?;
            let mut model_totals = UsageSnapshotTotals::default();
            for model in &project.models {
                validate_totals(&model.totals, "project model totals")?;
                add_totals_checked(&mut model_totals, &model.totals, "project model totals")?;
            }
            require_totals_match(
                &model_totals,
                &project.totals,
                &format!(
                    "project models do not conserve project totals for {}",
                    project.display_name
                ),
            )?;
            add_totals_checked(&mut project_totals, &project.totals, "project totals")?;
        }
        require_totals_match(
            &project_totals,
            &day.totals,
            &format!("projects do not conserve daily totals for {}", day.date),
        )?;

        validate_totals(&day.unplaced_for_hourly, "unplaced hourly totals")?;
        let mut placed = UsageSnapshotTotals::default();
        let mut previous_end = None;
        for hour in &day.hours {
            validate_totals(&hour.totals, "hour totals")?;
            if timezone.hour_bounds_of_ms(hour.start_ms) != Some((hour.start_ms, hour.end_ms)) {
                bail!("snapshot hour does not match canonical reporting-timezone bounds");
            }
            if previous_end.is_some_and(|end| hour.start_ms < end) {
                bail!("snapshot hours must be sorted and nonoverlapping");
            }
            if timezone.date_of_ms(hour.start_ms) != day.date {
                bail!("snapshot hour start does not match day {}", day.date);
            }
            previous_end = Some(hour.end_ms);
            add_totals_checked(&mut placed, &hour.totals, "hour totals")?;
        }
        add_totals_checked(
            &mut placed,
            &day.unplaced_for_hourly,
            "hourly conservation totals",
        )?;
        require_totals_match(
            &placed,
            &day.totals,
            &format!(
                "snapshot hourly facts do not conserve daily totals for {}",
                day.date
            ),
        )?;
    }
    Ok(())
}

fn write_private_snapshot(path: &Path, body: &[u8]) -> Result<()> {
    let lock_path = snapshot_lock_path(path);
    let lock = open_private_snapshot_lock(&lock_path)?;
    lock.lock_exclusive()
        .with_context(|| format!("lock usage snapshot {}", lock_path.display()))?;

    let sequence = SNAPSHOT_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("json.{}.{}.tmp", std::process::id(), sequence));
    let result = (|| -> Result<()> {
        #[cfg(unix)]
        let mut output = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)?
        };
        #[cfg(not(unix))]
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;

        output.write_all(body)?;
        output.sync_all()?;
        tokens_core::fs_atomic::replace_file(&temporary, path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("write usage snapshot {}", path.display()))
}

fn snapshot_lock_path(path: &Path) -> PathBuf {
    path.with_extension("json.lock")
}

fn open_private_snapshot_lock(path: &Path) -> Result<fs::File> {
    #[cfg(unix)]
    let lock = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)
    };
    #[cfg(not(unix))]
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path);

    let lock = lock.with_context(|| format!("open usage snapshot lock {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(lock)
}

fn snapshot_totals(totals: &DailyTotals) -> UsageSnapshotTotals {
    UsageSnapshotTotals {
        tokens: totals.tokens,
        cost: totals.cost,
        messages: totals.messages,
    }
}

fn snapshot_token_breakdown(tokens: &TokenBreakdown) -> UsageSnapshotTokenBreakdown {
    UsageSnapshotTokenBreakdown {
        input: tokens.input,
        output: tokens.output,
        cache_read: tokens.cache_read,
        cache_write: tokens.cache_write,
        reasoning: tokens.reasoning,
    }
}

fn validate_token_breakdown(breakdown: &UsageSnapshotTokenBreakdown, label: &str) -> Result<i64> {
    let values = [
        breakdown.input,
        breakdown.output,
        breakdown.cache_read,
        breakdown.cache_write,
        breakdown.reasoning,
    ];
    if values.iter().any(|value| *value < 0) {
        bail!("{label} must be nonnegative");
    }
    values.into_iter().try_fold(0i64, |total, value| {
        total
            .checked_add(value)
            .with_context(|| format!("{label} token total overflow"))
    })
}

fn add_totals_checked(
    target: &mut UsageSnapshotTotals,
    addition: &UsageSnapshotTotals,
    label: &str,
) -> Result<()> {
    target.tokens = target
        .tokens
        .checked_add(addition.tokens)
        .with_context(|| format!("{label} token accumulation overflow"))?;
    target.messages = target
        .messages
        .checked_add(addition.messages)
        .with_context(|| format!("{label} message accumulation overflow"))?;
    target.cost = checked_cost_sum(target.cost, addition.cost, label)?;
    Ok(())
}

fn require_totals_match(
    actual: &UsageSnapshotTotals,
    expected: &UsageSnapshotTotals,
    message: &str,
) -> Result<()> {
    if actual.tokens != expected.tokens
        || actual.messages != expected.messages
        || !cost_matches(actual.cost, expected.cost)
    {
        bail!("{message}");
    }
    Ok(())
}

fn validate_totals(totals: &UsageSnapshotTotals, label: &str) -> Result<()> {
    if totals.tokens < 0 || totals.messages < 0 || !totals.cost.is_finite() || totals.cost < 0.0 {
        bail!("{label} must be finite and nonnegative");
    }
    Ok(())
}

fn parse_date(value: &str) -> Result<NaiveDate> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
    if date.format("%Y-%m-%d").to_string() != value {
        bail!("date must use YYYY-MM-DD");
    }
    Ok(date)
}

fn snapshot_timezone(value: &str) -> Option<tokens_core::BucketTimezone> {
    if value == "local" {
        Some(tokens_core::BucketTimezone::Local)
    } else {
        tokens_core::parse_bucket_timezone(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use tokens_core::{
        ClientContribution, DailyContribution, DailyHourlyUsageFacts, DailyTotals,
        ExactHourlyUsageFact, GraphMeta, GraphResult, LocalUsageScan, ProjectContribution,
        ProjectModelContribution, TokenBreakdown,
    };

    fn totals(tokens: i64, cost: f64, messages: i32) -> DailyTotals {
        DailyTotals {
            tokens,
            cost,
            messages,
        }
    }

    fn sample_scan() -> LocalUsageScan {
        let date = "2026-08-04";
        let start_ms = Utc
            .with_ymd_and_hms(2026, 8, 4, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        LocalUsageScan {
            graph: GraphResult {
                meta: GraphMeta {
                    generated_at: "2026-08-04T08:30:00Z".into(),
                    version: "test".into(),
                    date_range_start: date.into(),
                    date_range_end: date.into(),
                    processing_time_ms: 1,
                },
                summary: tokens_core::DataSummary {
                    total_tokens: 100,
                    total_cost: 1.5,
                    total_days: 1,
                    active_days: 1,
                    average_per_day: 1.5,
                    max_cost_in_single_day: 1.5,
                    clients: vec!["claude-code".into()],
                    models: vec!["claude-sonnet".into()],
                },
                years: vec![],
                contributions: vec![DailyContribution {
                    date: date.into(),
                    totals: totals(100, 1.5, 3),
                    intensity: 4,
                    token_breakdown: TokenBreakdown {
                        input: 50,
                        output: 25,
                        cache_read: 15,
                        cache_write: 5,
                        reasoning: 5,
                    },
                    clients: vec![ClientContribution {
                        client: "claude-code".into(),
                        model_id: "claude-sonnet".into(),
                        provider_id: "anthropic".into(),
                        tokens: TokenBreakdown {
                            input: 50,
                            output: 25,
                            cache_read: 15,
                            cache_write: 5,
                            reasoning: 5,
                        },
                        cost: 1.5,
                        messages: 3,
                    }],
                    projects: vec![ProjectContribution {
                        project_key: Some("/workspace/tokens".into()),
                        project_label: "tokens".into(),
                        totals: totals(100, 1.5, 3),
                        models: vec![ProjectModelContribution {
                            model_id: "claude-sonnet".into(),
                            provider_id: "anthropic".into(),
                            tokens: 100,
                            cost: 1.5,
                            messages: 3,
                        }],
                    }],
                    active_time_ms: None,
                }],
                time_metrics: None,
            },
            unattributed_sessions: vec![],
            hourly_facts: vec![DailyHourlyUsageFacts {
                date: date.into(),
                hours: vec![ExactHourlyUsageFact {
                    start_ms,
                    end_ms: start_ms + 3_600_000,
                    totals: totals(60, 0.9, 2),
                }],
                unplaced_for_hourly: totals(40, 0.6, 1),
            }],
        }
    }

    #[test]
    fn snapshot_round_trip_matches_approved_shape() {
        let snapshot = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();
        let encoded = serde_json::to_vec_pretty(&snapshot).unwrap();
        let decoded: UsageSnapshot = serde_json::from_slice(&encoded).unwrap();

        assert_eq!(decoded, snapshot);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&encoded).unwrap(),
            json!({
                "schemaVersion": 3,
                "generatedAt": "2026-08-04T08:30:00Z",
                "bucketDate": "2026-08-04",
                "timezone": "UTC",
                "days": [{
                    "date": "2026-08-04",
                    "totals": {"tokens": 100, "cost": 1.5, "messages": 3},
                    "tokenBreakdown": {
                        "input": 50,
                        "output": 25,
                        "cacheRead": 15,
                        "cacheWrite": 5,
                        "reasoning": 5
                    },
                    "clients": [{
                        "client": "claude-code",
                        "modelId": "claude-sonnet",
                        "providerId": "anthropic",
                        "tokenBreakdown": {
                            "input": 50,
                            "output": 25,
                            "cacheRead": 15,
                            "cacheWrite": 5,
                            "reasoning": 5
                        },
                        "cost": 1.5,
                        "messages": 3
                    }],
                    "projects": [{
                        "projectKey": "/workspace/tokens",
                        "displayName": "tokens",
                        "totals": {"tokens": 100, "cost": 1.5, "messages": 3},
                        "models": [{
                            "modelId": "claude-sonnet",
                            "providerId": "anthropic",
                            "totals": {"tokens": 100, "cost": 1.5, "messages": 3}
                        }]
                    }],
                    "hours": [{
                        "startMs": 1785801600000_i64,
                        "endMs": 1785805200000_i64,
                        "totals": {"tokens": 60, "cost": 0.9, "messages": 2}
                    }],
                    "unplacedForHourly": {"tokens": 40, "cost": 0.6, "messages": 1}
                }]
            })
        );
    }

    #[test]
    fn daily_hours_and_unplaced_conserve_snapshot_totals() {
        let snapshot = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();
        let day = &snapshot.days[0];
        let hour = &day.hours[0].totals;

        assert_eq!(
            hour.tokens + day.unplaced_for_hourly.tokens,
            day.totals.tokens
        );
        assert_eq!(
            hour.messages + day.unplaced_for_hourly.messages,
            day.totals.messages
        );
        assert!((hour.cost + day.unplaced_for_hourly.cost - day.totals.cost).abs() <= 1e-9);
        validate_snapshot(&snapshot).unwrap();
    }

    #[test]
    fn lord_howe_final_absolute_hour_is_valid_for_its_start_date() {
        let timezone = chrono_tz::Australia::Lord_Howe;
        let instant = timezone
            .with_ymd_and_hms(2026, 10, 4, 23, 45, 0)
            .single()
            .unwrap();
        let mut message = tokens_core::UnifiedMessage::new_with_dedup(
            "client",
            "model",
            "provider",
            "session",
            instant.timestamp_millis(),
            TokenBreakdown {
                input: 10,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            1.0,
            Some("lord-howe-final-hour".into()),
        );
        message.date = "2026-10-04".into();
        message.message_count = 1;
        let hourly_facts = tokens_core::aggregate_hourly_usage_facts(
            std::slice::from_ref(&message),
            tokens_core::BucketTimezone::Named(timezone),
        );
        let graph =
            tokens_core::generate_graph_result(tokens_core::aggregate_by_date(vec![message]), 1);
        let scan = LocalUsageScan {
            graph,
            unattributed_sessions: vec![],
            hourly_facts,
        };
        let hour = &scan.hourly_facts[0].hours[0];
        assert_eq!(
            timezone
                .timestamp_millis_opt(hour.start_ms)
                .single()
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-04 23:30"
        );
        assert_eq!(
            timezone
                .timestamp_millis_opt(hour.end_ms)
                .single()
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-05 00:00"
        );

        build_snapshot(&scan, "2026-10-04", "Australia/Lord_Howe").unwrap();
    }

    #[test]
    fn cost_conservation_uses_a_small_floating_point_tolerance() {
        let mut within_tolerance = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();
        within_tolerance.days[0].totals.cost += 1e-10;
        validate_snapshot(&within_tolerance).unwrap();

        let mut outside_tolerance = within_tolerance;
        outside_tolerance.days[0].totals.cost += 1e-6;
        assert!(validate_snapshot(&outside_tolerance).is_err());
    }

    #[test]
    fn invalid_metadata_and_numeric_breakdown_facts_are_rejected() {
        let base = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();

        let mut invalid_generated_at = base.clone();
        invalid_generated_at.generated_at = "not-rfc3339".into();

        let mut negative_day_breakdown = base.clone();
        negative_day_breakdown.days[0].token_breakdown.input = -1;

        let mut negative_client_breakdown = base.clone();
        negative_client_breakdown.days[0].clients[0]
            .token_breakdown
            .output = -1;

        let mut nonfinite_client_cost = base.clone();
        nonfinite_client_cost.days[0].clients[0].cost = f64::NAN;

        let mut negative_project_messages = base.clone();
        negative_project_messages.days[0].projects[0]
            .totals
            .messages = -1;

        let mut negative_model_tokens = base;
        negative_model_tokens.days[0].projects[0].models[0]
            .totals
            .tokens = -1;

        for invalid in [
            invalid_generated_at,
            negative_day_breakdown,
            negative_client_breakdown,
            nonfinite_client_cost,
            negative_project_messages,
            negative_model_tokens,
        ] {
            assert!(validate_snapshot(&invalid).is_err());
        }
    }

    #[test]
    fn every_daily_breakdown_must_conserve_its_parent_totals() {
        let base = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();

        let mut day_breakdown = base.clone();
        day_breakdown.days[0].token_breakdown.input += 1;

        let mut clients = base.clone();
        clients.days[0].clients[0].messages -= 1;

        let mut projects = base.clone();
        projects.days[0].projects[0].totals.cost -= 0.1;

        let mut project_models = base;
        project_models.days[0].projects[0].models[0].totals.tokens -= 1;

        for invalid in [day_breakdown, clients, projects, project_models] {
            assert!(validate_snapshot(&invalid).is_err());
        }
    }

    #[test]
    fn floating_point_accumulation_overflow_is_rejected() {
        let mut snapshot = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();
        let day = &mut snapshot.days[0];
        day.totals.cost = f64::MAX;
        day.hours[0].totals.cost = f64::MAX;
        day.unplaced_for_hourly.cost = f64::MAX;
        day.clients[0].cost = f64::MAX;
        day.projects[0].totals.cost = f64::MAX;
        day.projects[0].models[0].totals.cost = f64::MAX;

        assert!(validate_snapshot(&snapshot).is_err());
    }

    #[test]
    fn invalid_daily_conservation_is_rejected() {
        let mut snapshot = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();
        snapshot.days[0].unplaced_for_hourly.tokens -= 1;

        let error = validate_snapshot(&snapshot).unwrap_err();

        assert!(error
            .to_string()
            .contains("hourly facts do not conserve daily totals"));
    }

    #[test]
    fn invalid_hour_bounds_order_and_day_are_rejected() {
        let base = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();

        let mut wrong_span = base.clone();
        wrong_span.days[0].hours[0].end_ms -= 1;

        let mut overlapping = base.clone();
        let mut zero_overlap = overlapping.days[0].hours[0].clone();
        zero_overlap.totals = UsageSnapshotTotals::default();
        overlapping.days[0].hours.push(zero_overlap);

        let mut misaligned = base.clone();
        misaligned.days[0].hours[0].start_ms += 900_000;
        misaligned.days[0].hours[0].end_ms += 900_000;

        let mut wrong_day = base;
        wrong_day.days[0].hours[0].start_ms += 86_400_000;
        wrong_day.days[0].hours[0].end_ms += 86_400_000;

        for invalid in [wrong_span, overlapping, misaligned, wrong_day] {
            assert!(validate_snapshot(&invalid).is_err());
        }
    }

    #[test]
    fn same_day_timezone_and_schema_are_required_for_reuse() {
        let dir = tempfile::tempdir().unwrap();
        let snapshot = build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap();
        write_snapshot(dir.path(), &snapshot).unwrap();

        assert_eq!(
            load_reusable_snapshot(dir.path(), "2026-08-04", "UTC"),
            Some(snapshot.clone())
        );
        assert_eq!(
            load_reusable_snapshot(dir.path(), "2026-08-03", "UTC"),
            None
        );
        assert_eq!(
            load_reusable_snapshot(dir.path(), "2026-08-04", "America/Los_Angeles"),
            None
        );

        let path = snapshot_path(dir.path());
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["schemaVersion"] = json!(SNAPSHOT_SCHEMA_VERSION + 1);
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            load_reusable_snapshot(dir.path(), "2026-08-04", "UTC"),
            None
        );
    }

    #[test]
    fn decode_invalid_snapshot_is_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(snapshot_path(dir.path()), b"{not-json").unwrap();

        assert_eq!(
            load_reusable_snapshot(dir.path(), "2026-08-04", "UTC"),
            None
        );
    }

    #[test]
    fn v2_snapshot_is_ignored_when_v3_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(V2_SNAPSHOT_FILENAME),
            br#"{"schemaVersion":2,"generatedAt":"2026-08-04T08:30:00Z","bucketDate":"2026-08-04","timezone":"UTC","contributions":[]}"#,
        )
        .unwrap();

        assert_eq!(
            load_reusable_snapshot(dir.path(), "2026-08-04", "UTC"),
            None
        );
    }

    #[test]
    fn force_clear_removes_v3_v2_and_v1_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        for filename in [
            SNAPSHOT_FILENAME,
            V2_SNAPSHOT_FILENAME,
            V1_SNAPSHOT_FILENAME,
        ] {
            std::fs::write(dir.path().join(filename), b"stale").unwrap();
        }

        clear_usage_snapshots(dir.path()).unwrap();

        for filename in [
            SNAPSHOT_FILENAME,
            V2_SNAPSHOT_FILENAME,
            V1_SNAPSHOT_FILENAME,
        ] {
            assert!(!dir.path().join(filename).exists(), "{filename}");
        }
    }

    #[test]
    fn clear_attempts_every_snapshot_removal_and_combines_failures() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(SNAPSHOT_FILENAME)).unwrap();
        std::fs::create_dir(dir.path().join(V2_SNAPSHOT_FILENAME)).unwrap();
        std::fs::write(dir.path().join(V1_SNAPSHOT_FILENAME), b"stale").unwrap();

        let error = clear_usage_snapshots(dir.path()).unwrap_err();
        let message = format!("{error:#}");

        assert!(message.contains(SNAPSHOT_FILENAME));
        assert!(message.contains(V2_SNAPSHOT_FILENAME));
        assert!(!dir.path().join(V1_SNAPSHOT_FILENAME).exists());
    }

    #[test]
    fn operation_lock_prevents_older_refresh_from_overwriting_completed_force() {
        use std::sync::{mpsc, Arc};
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let cache_dir = Arc::new(dir.path().to_path_buf());
        let old_snapshot = Arc::new(build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap());
        let mut new_snapshot = (*old_snapshot).clone();
        new_snapshot.generated_at = "2026-08-04T09:30:00Z".into();
        let new_snapshot = Arc::new(new_snapshot);
        let (refresh_locked_tx, refresh_locked_rx) = mpsc::channel();
        let (release_refresh_tx, release_refresh_rx) = mpsc::channel();

        let refresh = {
            let cache_dir = Arc::clone(&cache_dir);
            let snapshot = Arc::clone(&old_snapshot);
            std::thread::spawn(move || {
                let _operation = acquire_exclusive_operation_lock(&cache_dir).unwrap();
                refresh_locked_tx.send(()).unwrap();
                release_refresh_rx.recv().unwrap();
                write_snapshot(&cache_dir, &snapshot).unwrap();
            })
        };
        refresh_locked_rx.recv().unwrap();

        let (force_started_tx, force_started_rx) = mpsc::channel();
        let (force_done_tx, force_done_rx) = mpsc::channel();
        let force = {
            let cache_dir = Arc::clone(&cache_dir);
            let snapshot = Arc::clone(&new_snapshot);
            std::thread::spawn(move || {
                force_started_tx.send(()).unwrap();
                let _operation = acquire_exclusive_operation_lock(&cache_dir).unwrap();
                clear_usage_snapshots(&cache_dir).unwrap();
                write_snapshot(&cache_dir, &snapshot).unwrap();
                force_done_tx.send(()).unwrap();
            })
        };
        force_started_rx.recv().unwrap();
        assert!(force_done_rx
            .recv_timeout(Duration::from_millis(50))
            .is_err());

        release_refresh_tx.send(()).unwrap();
        refresh.join().unwrap();
        force_done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        force.join().unwrap();

        assert_eq!(
            load_reusable_snapshot(&cache_dir, "2026-08-04", "UTC"),
            Some((*new_snapshot).clone())
        );
    }

    #[test]
    fn snapshot_write_is_atomic_and_private() {
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let snapshot = Arc::new(build_snapshot(&sample_scan(), "2026-08-04", "UTC").unwrap());
        let writers: Vec<_> = (0..20)
            .map(|_| {
                let cache_dir = dir.path().to_path_buf();
                let snapshot = Arc::clone(&snapshot);
                std::thread::spawn(move || write_snapshot(&cache_dir, &snapshot))
            })
            .collect();

        for writer in writers {
            writer.join().unwrap().unwrap();
        }
        let published = std::fs::read(snapshot_path(dir.path())).unwrap();
        let decoded: UsageSnapshot = serde_json::from_slice(&published).unwrap();
        assert_eq!(decoded, *snapshot);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(snapshot_path(dir.path()))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(snapshot_lock_path(&snapshot_path(dir.path())))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
