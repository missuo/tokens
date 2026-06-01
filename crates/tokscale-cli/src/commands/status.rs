use crate::{auth, commands::submit_history, device, paths};
use anyhow::Result;
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusProcessKind {
    Serve,
    WarmTuiCache,
}

impl StatusProcessKind {
    fn as_str(self) -> &'static str {
        match self {
            StatusProcessKind::Serve => "serve",
            StatusProcessKind::WarmTuiCache => "warm-tui-cache",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusProcess {
    pid: u32,
    kind: &'static str,
    command: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusAuth {
    logged_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    credentials_path: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusDevice {
    configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'static str>,
    path: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusPaths {
    config_dir: String,
    cache_dir: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusSubmit {
    history_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<submit_history::SubmitHistoryEntry>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusService {
    health: &'static str,
    serve_running: bool,
    serve_pids: Vec<u32>,
    warm_tui_cache_pids: Vec<u32>,
    interval_minutes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler: Option<StatusScheduler>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusScheduler {
    kind: &'static str,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_run_at: Option<String>,
    scheduled_submit: bool,
    legacy_keep_alive: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusReport {
    api_url: String,
    auth: StatusAuth,
    device: StatusDevice,
    paths: StatusPaths,
    submit: StatusSubmit,
    service: StatusService,
    processes: Vec<StatusProcess>,
    notes: Vec<String>,
}

pub(crate) fn resolve_serve_interval_minutes(interval_min: Option<u64>) -> u64 {
    resolve_serve_interval_minutes_with_env(
        interval_min,
        std::env::var("TOKENS_SUBMIT_INTERVAL").ok().as_deref(),
    )
}

fn resolve_serve_interval_minutes_with_env(
    interval_min: Option<u64>,
    env_value: Option<&str>,
) -> u64 {
    interval_min
        .or_else(|| env_value.and_then(|v| v.trim().parse::<u64>().ok()))
        .filter(|m| *m >= 1)
        .unwrap_or(30)
}

fn classify_status_process_command(command: &str) -> Option<StatusProcessKind> {
    let lower = command.to_ascii_lowercase();
    let has_tokens_binary = lower.starts_with("tokens ")
        || lower.starts_with("tokscale ")
        || lower.contains("/tokens ")
        || lower.contains("/tokscale ")
        || lower.contains("/tokens/bin/tokens ")
        || lower.contains("/target/debug/tokens ")
        || lower.contains("/target/release/tokens ")
        || lower.contains("/target/debug/tokscale ")
        || lower.contains("/target/release/tokscale ");

    if !has_tokens_binary {
        return None;
    }

    if lower
        .split_whitespace()
        .any(|part| part == "warm-tui-cache")
    {
        return Some(StatusProcessKind::WarmTuiCache);
    }

    if lower.split_whitespace().any(|part| part == "serve") {
        return Some(StatusProcessKind::Serve);
    }

    None
}

fn parse_status_process_line(line: &str) -> Option<StatusProcess> {
    let trimmed = line.trim_start();
    let (pid, command) = trimmed.split_once(char::is_whitespace)?;
    let pid = pid.parse::<u32>().ok()?;
    let command = command.trim().to_string();
    let kind = classify_status_process_command(&command)?;
    Some(StatusProcess {
        pid,
        kind: kind.as_str(),
        command,
    })
}

fn parse_status_processes(output: &str) -> Vec<StatusProcess> {
    output
        .lines()
        .filter_map(parse_status_process_line)
        .collect()
}

fn detect_status_processes() -> Vec<StatusProcess> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,command="])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    parse_status_processes(&String::from_utf8_lossy(&output.stdout))
}

fn command_has_token(command: &str, token: &str) -> bool {
    command
        .split_whitespace()
        .any(|part| part.trim_matches('"') == token)
}

fn command_references_tokens_binary(command: &str) -> bool {
    command.split_whitespace().any(|part| {
        let part = part.trim_matches('"');
        part == "tokens"
            || part == "tokscale"
            || part.ends_with("/tokens")
            || part.ends_with("/tokscale")
    })
}

fn command_is_tokens_submit(command: &str) -> bool {
    command_references_tokens_binary(command) && command_has_token(command, "submit")
}

fn command_is_tokens_serve(command: &str) -> bool {
    command_references_tokens_binary(command) && command_has_token(command, "serve")
}

fn extract_xml_strings(content: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("<string>") {
        rest = &rest[start + "<string>".len()..];
        let Some(end) = rest.find("</string>") else {
            break;
        };
        values.push(rest[..end].trim().to_string());
        rest = &rest[end + "</string>".len()..];
    }
    values
}

fn extract_launchd_program_arguments(content: &str) -> Option<String> {
    let (_, after_key) = content.split_once("<key>ProgramArguments</key>")?;
    let (_, after_array) = after_key.split_once("<array>")?;
    let (array, _) = after_array.split_once("</array>")?;
    let args = extract_xml_strings(array);
    (!args.is_empty()).then(|| args.join(" "))
}

fn extract_launchd_start_interval(content: &str) -> Option<u64> {
    let (_, after_key) = content.split_once("<key>StartInterval</key>")?;
    let (_, after_integer) = after_key.split_once("<integer>")?;
    let (value, _) = after_integer.split_once("</integer>")?;
    value.trim().parse().ok()
}

fn extract_launchd_string_value(content: &str, key: &str) -> Option<String> {
    let needle = format!("<key>{key}</key>");
    let (_, after_key) = content.split_once(needle.as_str())?;
    let (_, after_string) = after_key.split_once("<string>")?;
    let (value, _) = after_string.split_once("</string>")?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn infer_homebrew_log_path_from_command(command: &str) -> Option<String> {
    command.split_whitespace().find_map(|part| {
        let part = part.trim_matches('"');
        let prefix = part.strip_suffix("/opt/tokens/bin/tokens")?;
        Some(format!("{prefix}/var/log/tokens.log"))
    })
}

fn parse_systemd_duration_seconds(raw: &str) -> Option<u64> {
    let value = raw.trim();
    if let Some(minutes) = value.strip_suffix("min") {
        return minutes.trim().parse::<u64>().ok().map(|m| m * 60);
    }
    if let Some(seconds) = value.strip_suffix('s') {
        return seconds.trim().parse::<u64>().ok();
    }
    if let Some(hours) = value.strip_suffix('h') {
        return hours.trim().parse::<u64>().ok().map(|h| h * 60 * 60);
    }
    value.parse().ok()
}

fn parse_systemd_key_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    content.lines().find_map(|line| {
        let line = line.trim();
        let (line_key, value) = line.split_once('=')?;
        (line_key.trim() == key).then_some(value.trim())
    })
}

fn parse_homebrew_launchd_scheduler(path: &str, content: &str) -> Option<StatusScheduler> {
    let command = extract_launchd_program_arguments(content)?;
    if !command_references_tokens_binary(&command) {
        return None;
    }
    let interval_seconds = extract_launchd_start_interval(content);
    let log_path = extract_launchd_string_value(content, "StandardOutPath")
        .or_else(|| infer_homebrew_log_path_from_command(&command));
    let error_log_path =
        extract_launchd_string_value(content, "StandardErrorPath").or_else(|| log_path.clone());
    let legacy_keep_alive =
        content.contains("<key>KeepAlive</key>") || command_is_tokens_serve(&command);
    let scheduled_submit = interval_seconds.is_some() && command_is_tokens_submit(&command);

    Some(StatusScheduler {
        kind: "homebrew-launchd",
        path: path.to_string(),
        interval_seconds,
        command: Some(command),
        log_path,
        error_log_path,
        log_command: None,
        next_run_at: None,
        scheduled_submit,
        legacy_keep_alive,
    })
}

fn systemd_service_unit_from_timer_path(path: &str) -> Option<String> {
    let file_name = Path::new(path).file_name()?.to_str()?;
    file_name
        .strip_suffix(".timer")
        .map(|stem| format!("{stem}.service"))
}

fn normalize_systemd_next_run_value(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("n/a") || value == "0" {
        None
    } else {
        Some(value.to_string())
    }
}

fn detect_systemd_next_run_at(timer_name: &str) -> Option<String> {
    let output = std::process::Command::new("systemctl")
        .args([
            "--user",
            "show",
            timer_name,
            "--property=NextElapseUSecRealtime",
            "--value",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    normalize_systemd_next_run_value(&String::from_utf8_lossy(&output.stdout))
}

fn parse_systemd_scheduler(
    kind: &'static str,
    path: &str,
    timer_content: &str,
    service_content: &str,
) -> Option<StatusScheduler> {
    let interval_seconds = parse_systemd_key_value(timer_content, "OnUnitActiveSec")
        .and_then(parse_systemd_duration_seconds);
    let command = parse_systemd_key_value(service_content, "ExecStart").map(str::to_string);
    if command
        .as_deref()
        .is_some_and(|command| !command_references_tokens_binary(command))
    {
        return None;
    }
    let legacy_keep_alive = service_content.lines().any(|line| {
        let line = line.trim();
        line.starts_with("Restart=") && !line.ends_with("no")
    }) || command.as_deref().is_some_and(command_is_tokens_serve);
    let scheduled_submit =
        interval_seconds.is_some() && command.as_deref().is_some_and(command_is_tokens_submit);
    let log_command = systemd_service_unit_from_timer_path(path)
        .map(|unit| format!("journalctl --user -u {unit} -f"));

    Some(StatusScheduler {
        kind,
        path: path.to_string(),
        interval_seconds,
        command,
        log_path: None,
        error_log_path: None,
        log_command,
        next_run_at: None,
        scheduled_submit,
        legacy_keep_alive,
    })
}

fn read_to_string_if_exists(path: &Path) -> Option<String> {
    path.exists()
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}

fn detect_status_scheduler() -> Option<StatusScheduler> {
    let home_dir = dirs::home_dir()?;

    let launchd_path = home_dir.join("Library/LaunchAgents/homebrew.mxcl.tokens.plist");
    if let Some(content) = read_to_string_if_exists(&launchd_path) {
        if let Some(scheduler) =
            parse_homebrew_launchd_scheduler(&launchd_path.display().to_string(), &content)
        {
            return Some(scheduler);
        }
    }

    let systemd_user_dir = home_dir.join(".config/systemd/user");
    for (kind, timer_name, service_name) in [
        (
            "homebrew-systemd",
            "homebrew.tokens.timer",
            "homebrew.tokens.service",
        ),
        ("systemd-user", "tokens.timer", "tokens.service"),
    ] {
        let timer_path = systemd_user_dir.join(timer_name);
        let Some(timer_content) = read_to_string_if_exists(&timer_path) else {
            continue;
        };
        let service_content =
            fs::read_to_string(systemd_user_dir.join(service_name)).unwrap_or_default();
        if let Some(scheduler) = parse_systemd_scheduler(
            kind,
            &timer_path.display().to_string(),
            &timer_content,
            &service_content,
        ) {
            let mut scheduler = scheduler;
            scheduler.next_run_at = detect_systemd_next_run_at(timer_name);
            return Some(scheduler);
        }
    }

    None
}

fn format_interval_seconds(seconds: u64) -> String {
    if seconds % 60 == 0 {
        let minutes = seconds / 60;
        if minutes == 1 {
            "1 min".to_string()
        } else {
            format!("{minutes} min")
        }
    } else {
        format!("{seconds} sec")
    }
}

fn status_service_health(serve_running: bool, scheduler: Option<&StatusScheduler>) -> &'static str {
    if scheduler.is_some_and(|scheduler| scheduler.scheduled_submit) {
        "scheduled-submit"
    } else if scheduler.is_some_and(|scheduler| scheduler.legacy_keep_alive) {
        "legacy-keep-alive"
    } else if scheduler.is_some() {
        "service-configured"
    } else if serve_running {
        "serve-running"
    } else {
        "not-configured"
    }
}

fn status_service_recommendations(
    serve_running: bool,
    scheduler: Option<&StatusScheduler>,
) -> Vec<String> {
    match status_service_health(serve_running, scheduler) {
        "legacy-keep-alive" => match scheduler.map(|scheduler| scheduler.kind) {
            Some("homebrew-launchd") | Some("homebrew-systemd") => vec![
                "Upgrade tokens, then run `brew services restart tokens` to replace the old keep-alive plist."
                    .to_string(),
            ],
            Some("systemd-user") => vec![
                "Install the updated tokens.service and tokens.timer, then run `systemctl --user disable --now tokens` and `systemctl --user enable --now tokens.timer`."
                    .to_string(),
            ],
            _ => vec![
                "Replace the old keep-alive service with scheduled `tokens --no-spinner submit` runs."
                    .to_string(),
            ],
        },
        "not-configured" => vec![
            "Start scheduled background submission with `brew services start tokens` on macOS/Linuxbrew or `systemctl --user enable --now tokens.timer` on Linux."
                .to_string(),
        ],
        _ => Vec::new(),
    }
}

fn format_status_cost(cost: f64) -> String {
    format!("${cost:.2}")
}

fn format_latest_submit_line(entry: &submit_history::SubmitHistoryEntry) -> String {
    let status = match entry.status {
        submit_history::SubmitHistoryStatus::Success => "success",
        submit_history::SubmitHistoryStatus::Failed => "failed",
        submit_history::SubmitHistoryStatus::Partial => "partial",
    };

    if matches!(entry.status, submit_history::SubmitHistoryStatus::Failed) {
        if let Some(error) = entry.error_summary.as_deref() {
            return format!("  Last submit: {status} at {}: {error}", entry.finished_at);
        }
        return format!("  Last submit: {status} at {}", entry.finished_at);
    }

    format!(
        "  Last submit: {status} at {} ({} tokens, {})",
        entry.finished_at,
        entry.tokens_submitted,
        format_status_cost(entry.cost_submitted)
    )
}

fn build_status_report() -> Result<StatusReport> {
    let auth_token = auth::resolve_api_token();
    let auth_source = auth_token.as_ref().map(|token| match token.source {
        auth::ApiTokenSource::Environment => "env",
        auth::ApiTokenSource::StoredCredentials => "stored",
    });
    let username = auth_token.as_ref().and_then(|token| token.username.clone());
    let device_inspection = device::inspect_submit_device()?;
    let submit_history_path = submit_history::history_path();
    let latest_submit = submit_history::latest_entry().unwrap_or(None);
    let device_source = device_inspection
        .source
        .as_ref()
        .map(|source| match source {
            device::SubmitDeviceSource::Environment => "env",
            device::SubmitDeviceSource::ConfigFile => "config",
        });
    let processes = detect_status_processes();
    let scheduler = detect_status_scheduler();
    let serve_pids: Vec<u32> = processes
        .iter()
        .filter(|process| process.kind == StatusProcessKind::Serve.as_str())
        .map(|process| process.pid)
        .collect();
    let warm_tui_cache_pids: Vec<u32> = processes
        .iter()
        .filter(|process| process.kind == StatusProcessKind::WarmTuiCache.as_str())
        .map(|process| process.pid)
        .collect();
    let serve_running = !serve_pids.is_empty();
    let health = status_service_health(serve_running, scheduler.as_ref());
    let recommendations = status_service_recommendations(serve_running, scheduler.as_ref());

    let mut notes = Vec::new();
    if auth_token.is_none() {
        notes.push("Run `tokens login` before starting background submission.".to_string());
    }
    if serve_pids.is_empty() && scheduler.is_none() {
        notes.push(
            "No long-running `tokens serve` process or scheduled submit service detected. Start background submission with `brew services start tokens` or `systemctl --user enable --now tokens.timer`."
                .to_string(),
        );
    }
    if scheduler
        .as_ref()
        .is_some_and(|scheduler| scheduler.legacy_keep_alive)
    {
        notes.push(
            "A legacy keep-alive service is configured. Reinstall or restart the service after upgrading to switch to scheduled submits."
                .to_string(),
        );
    }
    if !warm_tui_cache_pids.is_empty() {
        notes.push("`warm-tui-cache` is a short-lived helper spawned after submit.".to_string());
    }
    if device_inspection.device.is_none() {
        notes.push("Device metadata is created on the next non-dry-run submit.".to_string());
    }

    Ok(StatusReport {
        api_url: auth::get_api_base_url(),
        auth: StatusAuth {
            logged_in: auth_token.is_some(),
            source: auth_source,
            username,
            credentials_path: auth::credentials_path()?.display().to_string(),
        },
        device: StatusDevice {
            configured: device_inspection.device.is_some(),
            id: device_inspection.device.as_ref().map(|d| d.id.clone()),
            name: device_inspection
                .device
                .as_ref()
                .and_then(|d| d.name.clone()),
            source: device_source,
            path: device_inspection.path.display().to_string(),
        },
        paths: StatusPaths {
            config_dir: paths::get_config_dir().display().to_string(),
            cache_dir: paths::get_cache_dir().display().to_string(),
        },
        submit: StatusSubmit {
            history_path: submit_history_path.display().to_string(),
            latest: latest_submit,
        },
        service: StatusService {
            health,
            serve_running,
            serve_pids,
            warm_tui_cache_pids,
            interval_minutes: resolve_serve_interval_minutes(None),
            scheduler,
            recommendations,
        },
        processes,
        notes,
    })
}

pub fn run(json: bool) -> Result<()> {
    use colored::Colorize;

    let report = build_status_report()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("\n  {}\n", "Tokens - Local Status".cyan());

    if report.auth.logged_in {
        let source = report.auth.source.unwrap_or("unknown");
        let user = report.auth.username.as_deref().unwrap_or("unknown user");
        println!(
            "{}",
            format!("  Auth: logged in as {user} ({source})").green()
        );
    } else {
        println!("{}", "  Auth: not logged in".yellow());
    }
    println!("{}", format!("  API: {}", report.api_url).bright_black());

    if report.device.configured {
        let id = report.device.id.as_deref().unwrap_or("unknown");
        let source = report.device.source.unwrap_or("unknown");
        match report.device.name.as_deref() {
            Some(name) => println!("{}", format!("  Device: {name} ({id}, {source})").green()),
            None => println!("{}", format!("  Device: {id} ({source})").green()),
        }
    } else {
        println!("{}", "  Device: not initialized".yellow());
    }

    if report.service.serve_running {
        let pids = report
            .service
            .serve_pids
            .iter()
            .map(|pid| pid.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{}",
            format!(
                "  Background: tokens serve running (pid {pids}, current-env interval {} min)",
                report.service.interval_minutes
            )
            .green()
        );
    } else {
        println!(
            "{}",
            "  Background: no long-running tokens serve process detected".yellow()
        );
    }

    if !report.service.warm_tui_cache_pids.is_empty() {
        let pids = report
            .service
            .warm_tui_cache_pids
            .iter()
            .map(|pid| pid.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{}",
            format!("  Helper: warm-tui-cache running (pid {pids})").bright_black()
        );
    }

    if let Some(scheduler) = &report.service.scheduler {
        let interval = scheduler
            .interval_seconds
            .map(format_interval_seconds)
            .unwrap_or_else(|| "unknown interval".to_string());
        if scheduler.scheduled_submit {
            println!(
                "{}",
                format!(
                    "  Scheduler: {} scheduled submit configured ({interval})",
                    scheduler.kind
                )
                .green()
            );
        } else if scheduler.legacy_keep_alive {
            println!(
                "{}",
                format!(
                    "  Scheduler: {} legacy keep-alive service configured",
                    scheduler.kind
                )
                .yellow()
            );
        } else {
            println!(
                "{}",
                format!(
                    "  Scheduler: {} service configured ({interval})",
                    scheduler.kind
                )
                .yellow()
            );
        }
    }

    if let Some(scheduler) = &report.service.scheduler {
        if let Some(next_run_at) = &scheduler.next_run_at {
            println!("{}", format!("  Next run: {next_run_at}").bright_black());
        }
        if let Some(log_path) = &scheduler.log_path {
            match scheduler.error_log_path.as_deref() {
                Some(error_log_path) if error_log_path != log_path => println!(
                    "{}",
                    format!("  Logs: {log_path} (stderr: {error_log_path})").bright_black()
                ),
                _ => println!("{}", format!("  Logs: {log_path}").bright_black()),
            }
        } else if let Some(log_command) = &scheduler.log_command {
            println!("{}", format!("  Logs: {log_command}").bright_black());
        }
    }

    if let Some(latest_submit) = &report.submit.latest {
        let line = format_latest_submit_line(latest_submit);
        match latest_submit.status {
            submit_history::SubmitHistoryStatus::Success => println!("{}", line.green()),
            submit_history::SubmitHistoryStatus::Failed
            | submit_history::SubmitHistoryStatus::Partial => println!("{}", line.yellow()),
        }
    }

    println!(
        "{}",
        format!("  Config: {}", report.paths.config_dir).bright_black()
    );
    println!(
        "{}",
        format!("  Cache: {}", report.paths.cache_dir).bright_black()
    );
    println!(
        "{}",
        format!("  Credentials: {}", report.auth.credentials_path).bright_black()
    );
    println!(
        "{}",
        format!("  Device file: {}", report.device.path).bright_black()
    );

    if !report.notes.is_empty() {
        println!();
        println!("{}", "  Notes:".white());
        for note in report.notes {
            println!("{}", format!("    - {note}").bright_black());
        }
    }
    if !report.service.recommendations.is_empty() {
        println!();
        println!("{}", "  Suggested fixes:".white());
        for recommendation in report.service.recommendations {
            println!("{}", format!("    - {recommendation}").bright_black());
        }
    }
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_serve_interval_minutes_precedence_and_fallback() {
        assert_eq!(
            resolve_serve_interval_minutes_with_env(Some(7), Some("12")),
            7
        );
        assert_eq!(
            resolve_serve_interval_minutes_with_env(None, Some("12")),
            12
        );
        assert_eq!(resolve_serve_interval_minutes_with_env(None, Some("0")), 30);
        assert_eq!(
            resolve_serve_interval_minutes_with_env(None, Some("abc")),
            30
        );
        assert_eq!(resolve_serve_interval_minutes_with_env(None, None), 30);
    }

    #[test]
    fn parse_status_processes_detects_serve_and_warm_cache() {
        let processes = parse_status_processes(
            r#"
  123 /opt/homebrew/opt/tokens/bin/tokens serve
  124 /opt/homebrew/opt/tokens/bin/tokens warm-tui-cache
  125 /opt/homebrew/opt/tokens/bin/tokens status
  126 /bin/zsh -lc cargo test
"#,
        );

        assert_eq!(processes.len(), 2);
        assert_eq!(processes[0].pid, 123);
        assert_eq!(processes[0].kind, "serve");
        assert_eq!(processes[1].pid, 124);
        assert_eq!(processes[1].kind, "warm-tui-cache");
    }

    #[test]
    fn parse_homebrew_launchd_scheduler_detects_interval_submit() {
        let scheduler = parse_homebrew_launchd_scheduler(
            "/Users/example/Library/LaunchAgents/homebrew.mxcl.tokens.plist",
            r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>homebrew.mxcl.tokens</string>
  <key>ProgramArguments</key>
  <array>
    <string>/opt/homebrew/opt/tokens/bin/tokens</string>
    <string>--no-spinner</string>
    <string>submit</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StartInterval</key>
  <integer>1800</integer>
  <key>StandardOutPath</key>
  <string>/opt/homebrew/var/log/tokens.log</string>
  <key>StandardErrorPath</key>
  <string>/opt/homebrew/var/log/tokens.err.log</string>
</dict>
</plist>
"#,
        )
        .expect("scheduled launchd plist should parse");

        assert_eq!(scheduler.kind, "homebrew-launchd");
        assert_eq!(scheduler.interval_seconds, Some(1800));
        assert_eq!(
            scheduler.log_path.as_deref(),
            Some("/opt/homebrew/var/log/tokens.log")
        );
        assert_eq!(
            scheduler.error_log_path.as_deref(),
            Some("/opt/homebrew/var/log/tokens.err.log")
        );
        assert_eq!(scheduler.log_command, None);
        assert_eq!(scheduler.next_run_at, None);
        assert!(scheduler.scheduled_submit);
        assert!(!scheduler.legacy_keep_alive);
    }

    #[test]
    fn parse_homebrew_launchd_scheduler_infers_brew_log_path_from_command() {
        let scheduler = parse_homebrew_launchd_scheduler(
            "/Users/example/Library/LaunchAgents/homebrew.mxcl.tokens.plist",
            r#"
<plist version="1.0">
<dict>
  <key>ProgramArguments</key>
  <array>
    <string>/opt/homebrew/opt/tokens/bin/tokens</string>
    <string>--no-spinner</string>
    <string>submit</string>
  </array>
  <key>StartInterval</key>
  <integer>1800</integer>
</dict>
</plist>
"#,
        )
        .expect("homebrew launchd plist should parse");

        assert_eq!(
            scheduler.log_path.as_deref(),
            Some("/opt/homebrew/var/log/tokens.log")
        );
        assert_eq!(
            scheduler.error_log_path.as_deref(),
            Some("/opt/homebrew/var/log/tokens.log")
        );
    }

    #[test]
    fn parse_homebrew_launchd_scheduler_flags_legacy_keep_alive_serve() {
        let scheduler = parse_homebrew_launchd_scheduler(
            "/Users/example/Library/LaunchAgents/homebrew.mxcl.tokens.plist",
            r#"
<plist version="1.0">
<dict>
  <key>ProgramArguments</key>
  <array>
    <string>/opt/homebrew/opt/tokens/bin/tokens</string>
    <string>serve</string>
  </array>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#,
        )
        .expect("legacy launchd plist should parse");

        assert_eq!(scheduler.kind, "homebrew-launchd");
        assert_eq!(scheduler.interval_seconds, None);
        assert_eq!(
            scheduler.log_path.as_deref(),
            Some("/opt/homebrew/var/log/tokens.log")
        );
        assert_eq!(
            scheduler.error_log_path.as_deref(),
            Some("/opt/homebrew/var/log/tokens.log")
        );
        assert_eq!(scheduler.log_command, None);
        assert_eq!(scheduler.next_run_at, None);
        assert!(!scheduler.scheduled_submit);
        assert!(scheduler.legacy_keep_alive);
    }

    #[test]
    fn parse_systemd_scheduler_detects_timer_submit() {
        let scheduler = parse_systemd_scheduler(
            "systemd-user",
            "/home/example/.config/systemd/user/tokens.timer",
            r#"
[Timer]
OnActiveSec=2min
OnUnitActiveSec=30min
Persistent=true
Unit=tokens.service
"#,
            r#"
[Service]
Type=oneshot
ExecStart=tokens --no-spinner submit
"#,
        )
        .expect("systemd timer should parse");

        assert_eq!(scheduler.kind, "systemd-user");
        assert_eq!(scheduler.interval_seconds, Some(1800));
        assert_eq!(scheduler.log_path, None);
        assert_eq!(scheduler.error_log_path, None);
        assert_eq!(
            scheduler.log_command.as_deref(),
            Some("journalctl --user -u tokens.service -f")
        );
        assert_eq!(scheduler.next_run_at, None);
        assert!(scheduler.scheduled_submit);
        assert!(!scheduler.legacy_keep_alive);
    }

    #[test]
    fn normalize_systemd_next_run_value_ignores_empty_or_unset_values() {
        assert_eq!(normalize_systemd_next_run_value(""), None);
        assert_eq!(normalize_systemd_next_run_value("n/a"), None);
        assert_eq!(normalize_systemd_next_run_value("0"), None);
        assert_eq!(
            normalize_systemd_next_run_value("Mon 2026-06-01 10:30:00 JST"),
            Some("Mon 2026-06-01 10:30:00 JST".to_string())
        );
    }

    #[test]
    fn service_health_prefers_scheduled_submit() {
        let scheduler = StatusScheduler {
            kind: "homebrew-launchd",
            path: "/Users/example/Library/LaunchAgents/homebrew.mxcl.tokens.plist".to_string(),
            interval_seconds: Some(1800),
            command: Some("/opt/homebrew/opt/tokens/bin/tokens --no-spinner submit".to_string()),
            log_path: Some("/opt/homebrew/var/log/tokens.log".to_string()),
            error_log_path: Some("/opt/homebrew/var/log/tokens.log".to_string()),
            log_command: None,
            next_run_at: None,
            scheduled_submit: true,
            legacy_keep_alive: false,
        };

        assert_eq!(
            status_service_health(false, Some(&scheduler)),
            "scheduled-submit"
        );
        assert!(status_service_recommendations(false, Some(&scheduler)).is_empty());
    }

    #[test]
    fn service_recommendations_include_homebrew_restart_for_legacy_keep_alive() {
        let scheduler = StatusScheduler {
            kind: "homebrew-launchd",
            path: "/Users/example/Library/LaunchAgents/homebrew.mxcl.tokens.plist".to_string(),
            interval_seconds: None,
            command: Some("/opt/homebrew/opt/tokens/bin/tokens serve".to_string()),
            log_path: Some("/opt/homebrew/var/log/tokens.log".to_string()),
            error_log_path: Some("/opt/homebrew/var/log/tokens.log".to_string()),
            log_command: None,
            next_run_at: None,
            scheduled_submit: false,
            legacy_keep_alive: true,
        };

        assert_eq!(
            status_service_health(false, Some(&scheduler)),
            "legacy-keep-alive"
        );
        assert_eq!(
            status_service_recommendations(false, Some(&scheduler)),
            vec![
                "Upgrade tokens, then run `brew services restart tokens` to replace the old keep-alive plist.".to_string()
            ]
        );
    }

    #[test]
    fn service_recommendations_include_start_commands_when_no_background_service_exists() {
        assert_eq!(status_service_health(false, None), "not-configured");
        assert_eq!(
            status_service_recommendations(false, None),
            vec![
                "Start scheduled background submission with `brew services start tokens` on macOS/Linuxbrew or `systemctl --user enable --now tokens.timer` on Linux.".to_string()
            ]
        );
    }

    fn sample_submit_history_entry(
        status: submit_history::SubmitHistoryStatus,
        error_summary: Option<&str>,
    ) -> submit_history::SubmitHistoryEntry {
        submit_history::SubmitHistoryEntry {
            id: "entry_1".to_string(),
            started_at: "2026-06-01T00:00:00Z".to_string(),
            finished_at: "2026-06-01T00:00:05Z".to_string(),
            status,
            clients: vec!["claude".to_string(), "codex".to_string()],
            rows_submitted: 2,
            tokens_submitted: 300,
            cost_submitted: 1.75,
            active_days: 2,
            device_id: Some("dev_test".to_string()),
            submission_id: Some("sub_test".to_string()),
            error_summary: error_summary.map(str::to_string),
            source_version: "3.0.0-test".to_string(),
        }
    }

    #[test]
    fn latest_submit_line_formats_success() {
        let line = format_latest_submit_line(&sample_submit_history_entry(
            submit_history::SubmitHistoryStatus::Success,
            None,
        ));

        assert_eq!(
            line,
            "  Last submit: success at 2026-06-01T00:00:05Z (300 tokens, $1.75)"
        );
    }

    #[test]
    fn latest_submit_line_formats_failure_with_error_summary() {
        let line = format_latest_submit_line(&sample_submit_history_entry(
            submit_history::SubmitHistoryStatus::Failed,
            Some("Server returned 500"),
        ));

        assert_eq!(
            line,
            "  Last submit: failed at 2026-06-01T00:00:05Z: Server returned 500"
        );
    }
}
