use crate::{auth, device, paths, t, tr};
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusProcessKind {
    Serve,
}

impl StatusProcessKind {
    fn as_str(self) -> &'static str {
        match self {
            StatusProcessKind::Serve => "serve",
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
struct StatusService {
    serve_running: bool,
    serve_pids: Vec<u32>,
    interval_minutes: u64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusReport {
    api_url: String,
    auth: StatusAuth,
    device: StatusDevice,
    paths: StatusPaths,
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
        || lower.starts_with("tokens ")
        || lower.contains("/tokens ")
        || lower.contains("/tokens ")
        || lower.contains("/tokens/bin/tokens ")
        || lower.contains("/target/debug/tokens ")
        || lower.contains("/target/release/tokens ")
        || lower.contains("/target/debug/tokens ")
        || lower.contains("/target/release/tokens ");

    if !has_tokens_binary {
        return None;
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

fn build_status_report() -> Result<StatusReport> {
    let auth_token = auth::resolve_api_token();
    let auth_source = auth_token.as_ref().map(|token| match token.source {
        auth::ApiTokenSource::Environment => "env",
        auth::ApiTokenSource::StoredCredentials => "stored",
    });
    let username = auth_token.as_ref().and_then(|token| token.username.clone());
    let device_inspection = device::inspect_submit_device()?;
    let device_source = device_inspection
        .source
        .as_ref()
        .map(|source| match source {
            device::SubmitDeviceSource::Environment => "env",
            device::SubmitDeviceSource::ConfigFile => "config",
        });
    let processes = detect_status_processes();
    let serve_pids: Vec<u32> = processes
        .iter()
        .filter(|process| process.kind == StatusProcessKind::Serve.as_str())
        .map(|process| process.pid)
        .collect();

    let mut notes = Vec::new();
    if auth_token.is_none() {
        notes.push(t!("commands.status.noteLogin").to_string());
    }
    if serve_pids.is_empty() {
        notes.push(t!("commands.status.noteStartService").to_string());
    }
    if device_inspection.device.is_none() {
        notes.push(t!("commands.status.noteDeviceMetadata").to_string());
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
        service: StatusService {
            serve_running: !serve_pids.is_empty(),
            serve_pids,
            interval_minutes: resolve_serve_interval_minutes(None),
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

    println!("\n  {}\n", t!("commands.status.title").cyan());

    if report.auth.logged_in {
        let source = report.auth.source.unwrap_or(t!("commands.status.unknown"));
        let user = report
            .auth
            .username
            .as_deref()
            .unwrap_or(t!("commands.status.unknownUser"));
        println!(
            "{}",
            tr!("commands.status.authLoggedIn", user = user, source = source).green()
        );
    } else {
        println!("{}", t!("commands.status.authLoggedOut").yellow());
    }
    println!(
        "{}",
        tr!("commands.status.api", url = report.api_url).bright_black()
    );

    if report.device.configured {
        let id = report
            .device
            .id
            .as_deref()
            .unwrap_or(t!("commands.status.unknown"));
        let source = report
            .device
            .source
            .unwrap_or(t!("commands.status.unknown"));
        match report.device.name.as_deref() {
            Some(name) => println!(
                "{}",
                tr!(
                    "commands.status.deviceWithName",
                    name = name,
                    id = id,
                    source = source
                )
                .green()
            ),
            None => println!(
                "{}",
                tr!("commands.status.device", id = id, source = source).green()
            ),
        }
    } else {
        println!("{}", t!("commands.status.deviceNotInitialized").yellow());
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
            tr!(
                "commands.status.serviceRunning",
                pids = pids,
                interval = report.service.interval_minutes
            )
            .green()
        );
    } else {
        println!(
            "{}",
            tr!(
                "commands.status.serviceNotDetected",
                interval = report.service.interval_minutes
            )
            .yellow()
        );
    }

    println!(
        "{}",
        tr!("commands.status.config", path = report.paths.config_dir).bright_black()
    );
    println!(
        "{}",
        tr!("commands.status.cache", path = report.paths.cache_dir).bright_black()
    );
    println!(
        "{}",
        tr!(
            "commands.status.credentials",
            path = report.auth.credentials_path
        )
        .bright_black()
    );
    println!(
        "{}",
        tr!("commands.status.deviceFile", path = report.device.path).bright_black()
    );

    if !report.notes.is_empty() {
        println!();
        println!("{}", t!("commands.status.notes").white());
        for note in report.notes {
            println!(
                "{}",
                tr!("commands.status.noteItem", note = note).bright_black()
            );
        }
    }
    println!();

    Ok(())
}
