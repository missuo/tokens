use crate::settings::{
    AutosubmitSettings, DEFAULT_AUTOSUBMIT_INTERVAL_MINUTES, MAX_AUTOSUBMIT_INTERVAL_MINUTES,
    MIN_AUTOSUBMIT_INTERVAL_MINUTES,
};
use crate::{t, tr, ClientFlags, DateRangeFlags};
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use fs2::FileExt;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

const JOB_ID: &str = "ci.tokens.autosubmit";
const CRON_MARKER_BEGIN: &str = "# BEGIN TOKENS AUTOSUBMIT";
const CRON_MARKER_END: &str = "# END TOKENS AUTOSUBMIT";
const SKIP_SCHEDULER_ENV: &str = "TOKENS_AUTOSUBMIT_SKIP_SCHEDULER";

#[derive(Subcommand)]
pub enum AutosubmitSubcommand {
    #[command(about = "Enable periodic submit using the OS scheduler")]
    Enable(AutosubmitEnableArgs),
    #[command(about = "Show autosubmit status")]
    Status {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Disable autosubmit and remove its scheduler entry")]
    Disable,
    #[command(about = "Run autosubmit once if it is due")]
    Run {
        #[arg(long, help = "Run even when the configured interval has not elapsed")]
        force: bool,
    },
}

#[derive(Args)]
pub struct AutosubmitEnableArgs {
    #[arg(
        long,
        value_name = "DURATION",
        default_value = "24h",
        help = "Submit interval, e.g. 30m, 2h, or 1d"
    )]
    interval: String,
    #[command(flatten)]
    clients: ClientFlags,
    #[command(flatten)]
    date: DateRangeFlags,
    #[arg(long, value_enum, help = "Override the detected scheduler backend")]
    scheduler: Option<SchedulerKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum SchedulerKind {
    Launchd,
    Systemd,
    Cron,
    WindowsTaskScheduler,
}

impl SchedulerKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Launchd => "launchd",
            Self::Systemd => "systemd",
            Self::Cron => "cron",
            Self::WindowsTaskScheduler => "windows-task-scheduler",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "launchd" => Some(Self::Launchd),
            "systemd" => Some(Self::Systemd),
            "cron" => Some(Self::Cron),
            "windows-task-scheduler" => Some(Self::WindowsTaskScheduler),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutosubmitRunDecision {
    Disabled,
    NotDue { next_run_at_ms: i64 },
    Due,
}

pub struct AutosubmitRunLock {
    _file: std::fs::File,
}

#[derive(Debug, Clone)]
struct SchedulerSpec {
    files: Vec<(PathBuf, String)>,
    install_commands: Vec<(String, Vec<String>)>,
    uninstall_commands: Vec<(String, Vec<String>)>,
    cron_block: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusOutput {
    enabled: bool,
    interval_minutes: u64,
    scheduler: Option<String>,
    clients: Vec<String>,
    since: Option<String>,
    until: Option<String>,
    year: Option<String>,
    today: bool,
    yesterday: bool,
    week: bool,
    month: bool,
    last_run_at_ms: Option<i64>,
    last_error: Option<String>,
}

pub fn enable(args: AutosubmitEnableArgs) -> Result<()> {
    let interval_minutes = parse_interval_minutes(&args.interval)?;
    let scheduler = args.scheduler.unwrap_or_else(default_scheduler_kind);
    let exe = std::env::current_exe().context(t!("commands.autosubmit.resolveExeFailed"))?;
    validate_scheduler_executable(&exe)?;

    let mut settings = crate::settings::Settings::load();
    settings.autosubmit = AutosubmitSettings {
        enabled: true,
        interval_minutes,
        clients: clients_for_settings(args.clients),
        since: args.date.since,
        until: args.date.until,
        year: args.date.year,
        today: args.date.today,
        yesterday: args.date.yesterday,
        week: args.date.week,
        month: args.date.month,
        scheduler: Some(scheduler.as_str().to_string()),
        last_run_at_ms: settings.autosubmit.last_run_at_ms,
        last_error: None,
    };

    if !skip_scheduler_install() {
        install_scheduler(scheduler, &exe, &settings.autosubmit)?;
    }
    settings.save()?;

    println!(
        "{}",
        tr!(
            "commands.autosubmit.enabled",
            interval = interval_minutes,
            scheduler = scheduler.as_str()
        )
    );
    Ok(())
}

pub fn status(json: bool) -> Result<()> {
    let autosubmit = crate::settings::Settings::load().autosubmit;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&status_output(&autosubmit))?
        );
        return Ok(());
    }

    if autosubmit.enabled {
        println!("{}", t!("commands.autosubmit.isEnabled"));
        println!(
            "{}",
            tr!(
                "commands.autosubmit.interval",
                interval = autosubmit.interval_minutes
            )
        );
        println!(
            "{}",
            tr!(
                "commands.autosubmit.scheduler",
                scheduler = autosubmit
                    .scheduler
                    .as_deref()
                    .unwrap_or(t!("commands.autosubmit.unknown"))
            )
        );
        if !autosubmit.clients.is_empty() {
            println!(
                "{}",
                tr!(
                    "commands.autosubmit.clients",
                    clients = autosubmit.clients.join(", ")
                )
            );
        } else {
            println!("{}", t!("commands.autosubmit.defaultClients"));
        }
    } else {
        println!("{}", t!("commands.autosubmit.isDisabled"));
    }
    if let Some(last_run_at_ms) = autosubmit.last_run_at_ms {
        println!(
            "{}",
            tr!(
                "commands.autosubmit.lastRun",
                timestamp = format_timestamp_ms(last_run_at_ms)
            )
        );
    }
    if let Some(error) = autosubmit.last_error {
        println!("{}", tr!("commands.autosubmit.lastError", error = error));
    }
    Ok(())
}

pub fn disable() -> Result<()> {
    let mut settings = crate::settings::Settings::load();
    let scheduler = settings
        .autosubmit
        .scheduler
        .as_deref()
        .and_then(SchedulerKind::from_str)
        .unwrap_or_else(default_scheduler_kind);

    if settings.autosubmit.enabled && !skip_scheduler_install() {
        uninstall_scheduler(scheduler)?;
    }

    settings.autosubmit.enabled = false;
    settings.autosubmit.last_error = None;
    settings.save()?;
    println!("{}", t!("commands.autosubmit.disabled"));
    Ok(())
}

pub fn load_run_config(
    force: bool,
    now_ms: i64,
) -> Result<(AutosubmitSettings, AutosubmitRunDecision)> {
    let settings = crate::settings::Settings::load().autosubmit;
    let decision = run_decision(&settings, now_ms, force);
    Ok((settings, decision))
}

pub fn record_run_success(now_ms: i64) -> Result<()> {
    let mut settings = crate::settings::Settings::load();
    settings.autosubmit.last_run_at_ms = Some(now_ms);
    settings.autosubmit.last_error = None;
    settings.save()
}

pub fn record_run_error(error: &str) -> Result<()> {
    let mut settings = crate::settings::Settings::load();
    settings.autosubmit.last_error = Some(error.to_string());
    settings.save()
}

pub fn submit_filters(
    settings: &AutosubmitSettings,
) -> (
    Option<Vec<String>>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let clients = if settings.clients.is_empty() {
        Some(default_submit_clients())
    } else {
        Some(settings.clients.clone())
    };
    let date = DateRangeFlags {
        today: settings.today,
        yesterday: settings.yesterday,
        week: settings.week,
        month: settings.month,
        since: settings.since.clone(),
        until: settings.until.clone(),
        year: settings.year.clone(),
    };
    let (since, until) = build_date_filter_for_date(&date, chrono::Local::now().date_naive());
    let year = if date.today || date.yesterday || date.week || date.month {
        None
    } else {
        date.year
    };
    (clients, since, until, year)
}

/// `ERROR_LOCK_VIOLATION`: what `LockFileEx` reports on Windows when another
/// handle already holds the range. No Unix `flock` failure uses errno 33, so
/// matching it everywhere cannot disguise a genuine error as contention.
const WINDOWS_ERROR_LOCK_VIOLATION: i32 = 33;

/// Whether `err` means "another process already holds this lock" rather than a
/// real I/O failure. Only `WouldBlock` used to count, so on Windows a second
/// run hit the error arm instead of stepping aside.
pub(crate) fn is_lock_contention(err: &std::io::Error) -> bool {
    err.kind() == ErrorKind::WouldBlock || err.raw_os_error() == Some(WINDOWS_ERROR_LOCK_VIOLATION)
}

pub fn try_acquire_run_lock() -> Result<Option<AutosubmitRunLock>> {
    let path = autosubmit_lock_path()?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .with_context(|| tr!("commands.autosubmit.openLockFailed", path = path.display()))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(AutosubmitRunLock { _file: file })),
        Err(err) if is_lock_contention(&err) => Ok(None),
        Err(err) => Err(err)
            .with_context(|| tr!("commands.autosubmit.lockStateFailed", path = path.display())),
    }
}

fn status_output(settings: &AutosubmitSettings) -> StatusOutput {
    StatusOutput {
        enabled: settings.enabled,
        interval_minutes: settings.interval_minutes,
        scheduler: settings.scheduler.clone(),
        clients: settings.clients.clone(),
        since: settings.since.clone(),
        until: settings.until.clone(),
        year: settings.year.clone(),
        today: settings.today,
        yesterday: settings.yesterday,
        week: settings.week,
        month: settings.month,
        last_run_at_ms: settings.last_run_at_ms,
        last_error: settings.last_error.clone(),
    }
}

pub fn run_decision(
    settings: &AutosubmitSettings,
    now_ms: i64,
    force: bool,
) -> AutosubmitRunDecision {
    if !settings.enabled {
        return AutosubmitRunDecision::Disabled;
    }
    if force {
        return AutosubmitRunDecision::Due;
    }
    let interval_ms = (settings.interval_minutes as i64).saturating_mul(60_000);
    match settings.last_run_at_ms {
        Some(last) if now_ms < last.saturating_add(interval_ms) => AutosubmitRunDecision::NotDue {
            next_run_at_ms: last.saturating_add(interval_ms),
        },
        _ => AutosubmitRunDecision::Due,
    }
}

pub fn parse_interval_minutes(input: &str) -> Result<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("{}", t!("commands.autosubmit.intervalEmpty"));
    }
    let split = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let amount: u64 = number
        .parse()
        .with_context(|| tr!("commands.autosubmit.intervalInvalid", input = input))?;
    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "m" | "min" | "mins" | "minute" | "minutes" => 1,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60,
        "d" | "day" | "days" => 24 * 60,
        _ => bail!("{}", tr!("commands.autosubmit.intervalUnit", unit = unit)),
    };
    let minutes = amount
        .checked_mul(multiplier)
        .ok_or_else(|| anyhow::anyhow!(t!("commands.autosubmit.intervalTooLarge")))?;
    if !(MIN_AUTOSUBMIT_INTERVAL_MINUTES..=MAX_AUTOSUBMIT_INTERVAL_MINUTES).contains(&minutes) {
        bail!(
            "{}",
            tr!(
                "commands.autosubmit.intervalRange",
                min = MIN_AUTOSUBMIT_INTERVAL_MINUTES,
                max = MAX_AUTOSUBMIT_INTERVAL_MINUTES
            )
        );
    }
    Ok(minutes)
}

fn clients_for_settings(flags: ClientFlags) -> Vec<String> {
    if flags.clients.is_empty() {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    flags
        .clients
        .into_iter()
        .map(|client| client.as_filter_str().to_string())
        .filter(|client| seen.insert(client.clone()))
        .collect()
}

fn default_submit_clients() -> Vec<String> {
    let mut clients: Vec<String> = tokens_core::ClientId::iter()
        .filter(|client| client.submit_default())
        .map(|client| client.as_str().to_string())
        .collect();
    clients.push("synthetic".to_string());
    clients
}

fn skip_scheduler_install() -> bool {
    std::env::var(SKIP_SCHEDULER_ENV)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn default_scheduler_kind() -> SchedulerKind {
    if cfg!(target_os = "macos") {
        SchedulerKind::Launchd
    } else if cfg!(target_os = "windows") {
        SchedulerKind::WindowsTaskScheduler
    } else if Command::new("systemctl")
        .args(["--user", "--version"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        SchedulerKind::Systemd
    } else {
        SchedulerKind::Cron
    }
}

fn install_scheduler(
    scheduler: SchedulerKind,
    exe: &Path,
    settings: &AutosubmitSettings,
) -> Result<()> {
    let spec = render_scheduler_spec(scheduler, exe, settings)?;
    for (path, content) in spec.files {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
    }
    for (program, args) in spec.install_commands {
        run_status_command(&program, &args)?;
    }
    if let Some(block) = spec.cron_block {
        install_cron_block(&block)?;
    }
    Ok(())
}

fn uninstall_scheduler(scheduler: SchedulerKind) -> Result<()> {
    let dummy = AutosubmitSettings {
        interval_minutes: DEFAULT_AUTOSUBMIT_INTERVAL_MINUTES,
        ..AutosubmitSettings::default()
    };
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tokens"));
    let spec = render_scheduler_spec(scheduler, &exe, &dummy)?;
    for (program, args) in spec.uninstall_commands {
        let _ = Command::new(&program).args(&args).status();
    }
    if scheduler == SchedulerKind::Cron {
        let _ = uninstall_cron_block();
    }
    for (path, _) in spec.files {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn render_scheduler_spec(
    scheduler: SchedulerKind,
    exe: &Path,
    settings: &AutosubmitSettings,
) -> Result<SchedulerSpec> {
    validate_scheduler_executable(exe)?;
    match scheduler {
        SchedulerKind::Launchd => render_launchd_spec(exe, settings),
        SchedulerKind::Systemd => render_systemd_spec(exe, settings),
        SchedulerKind::Cron => render_cron_spec(exe, settings),
        SchedulerKind::WindowsTaskScheduler => render_windows_task_spec(exe, settings),
    }
}

fn render_launchd_spec(exe: &Path, settings: &AutosubmitSettings) -> Result<SchedulerSpec> {
    let home = dirs::home_dir().context(t!("commands.autosubmit.homeDirUnknown"))?;
    let plist_path = home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{JOB_ID}.plist"));
    let log_path = autosubmit_log_path()?;
    let interval_seconds = settings.interval_minutes.saturating_mul(60).max(60);
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{job}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>autosubmit</string>
    <string>run</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>StartInterval</key><integer>{interval}</integer>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
        job = xml_escape(JOB_ID),
        exe = xml_escape(&exe.to_string_lossy()),
        interval = interval_seconds,
        log = xml_escape(&log_path.to_string_lossy())
    );
    Ok(SchedulerSpec {
        files: vec![(plist_path.clone(), content)],
        cron_block: None,
        install_commands: vec![(
            "launchctl".to_string(),
            vec![
                "load".to_string(),
                plist_path.to_string_lossy().into_owned(),
            ],
        )],
        uninstall_commands: vec![(
            "launchctl".to_string(),
            vec![
                "unload".to_string(),
                plist_path.to_string_lossy().into_owned(),
            ],
        )],
    })
}

fn render_systemd_spec(exe: &Path, settings: &AutosubmitSettings) -> Result<SchedulerSpec> {
    let user_dir = systemd_user_dir()?;
    let service_path = user_dir.join("tokens-autosubmit.service");
    let timer_path = user_dir.join("tokens-autosubmit.timer");
    let log_path = autosubmit_log_path()?;
    let service = format!(
        "[Unit]\nDescription=Tokens autosubmit\n\n[Service]\nType=oneshot\nExecStart={} autosubmit run\nStandardOutput=append:{}\nStandardError=append:{}\n",
        systemd_escape_path(exe),
        systemd_escape_path(&log_path),
        systemd_escape_path(&log_path)
    );
    let timer = format!(
        "[Unit]\nDescription=Run Tokens autosubmit periodically\n\n[Timer]\nOnBootSec=5m\nOnUnitActiveSec={}min\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
        settings.interval_minutes
    );
    Ok(SchedulerSpec {
        files: vec![(service_path, service), (timer_path, timer)],
        cron_block: None,
        install_commands: vec![
            (
                "systemctl".to_string(),
                vec!["--user".to_string(), "daemon-reload".to_string()],
            ),
            (
                "systemctl".to_string(),
                vec![
                    "--user".to_string(),
                    "enable".to_string(),
                    "--now".to_string(),
                    "tokens-autosubmit.timer".to_string(),
                ],
            ),
        ],
        uninstall_commands: vec![
            (
                "systemctl".to_string(),
                vec![
                    "--user".to_string(),
                    "disable".to_string(),
                    "--now".to_string(),
                    "tokens-autosubmit.timer".to_string(),
                ],
            ),
            (
                "systemctl".to_string(),
                vec!["--user".to_string(), "daemon-reload".to_string()],
            ),
        ],
    })
}

fn systemd_user_dir() -> Result<PathBuf> {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")));
    Ok(config_dir
        .context(t!("commands.autosubmit.configDirUnknown"))?
        .join("systemd")
        .join("user"))
}

fn render_cron_spec(exe: &Path, settings: &AutosubmitSettings) -> Result<SchedulerSpec> {
    let log_path = autosubmit_log_path()?;
    let interval = settings.interval_minutes.max(1);
    let schedule = if interval < 60 {
        format!("*/{interval} * * * *")
    } else {
        "0 * * * *".to_string()
    };
    let line = format!(
        "{schedule} {} autosubmit run >> {} 2>&1",
        shell_quote(&exe.to_string_lossy()),
        shell_quote(&log_path.to_string_lossy())
    );
    let block = format!("{CRON_MARKER_BEGIN}\n{line}\n{CRON_MARKER_END}");
    Ok(SchedulerSpec {
        files: Vec::new(),
        install_commands: Vec::new(),
        uninstall_commands: Vec::new(),
        cron_block: Some(block),
    })
}

fn render_windows_task_spec(exe: &Path, settings: &AutosubmitSettings) -> Result<SchedulerSpec> {
    let (schedule, modifier) = windows_schedule(settings.interval_minutes)?;
    let task = format!(
        r#""{}" autosubmit run"#,
        windows_task_executable(exe).display()
    );
    Ok(SchedulerSpec {
        files: Vec::new(),
        cron_block: None,
        install_commands: vec![(
            "schtasks".to_string(),
            vec![
                "/Create".to_string(),
                "/F".to_string(),
                "/SC".to_string(),
                schedule,
                "/MO".to_string(),
                modifier,
                "/TN".to_string(),
                JOB_ID.to_string(),
                "/TR".to_string(),
                task,
            ],
        )],
        uninstall_commands: vec![(
            "schtasks".to_string(),
            vec![
                "/Delete".to_string(),
                "/F".to_string(),
                "/TN".to_string(),
                JOB_ID.to_string(),
            ],
        )],
    })
}

/// `tokens.exe` is a console program, so Task Scheduler opens a console window
/// for it on every run. Point the task at the windowless `tokensw.exe` shipped
/// next to it instead; an install that predates it keeps the console binary.
fn windows_task_executable(exe: &Path) -> PathBuf {
    let launcher = exe.with_file_name(WINDOWS_LAUNCHER_EXE);
    if launcher.is_file() {
        launcher
    } else {
        exe.to_path_buf()
    }
}

const WINDOWS_LAUNCHER_EXE: &str = "tokensw.exe";
const WINDOWS_CONSOLE_EXE: &str = "tokens.exe";

/// Re-point a scheduled task registered before `tokensw.exe` existed.
///
/// An npm/bun upgrade drops `tokensw.exe` next to `tokens.exe`, but the task
/// that `autosubmit enable` registered earlier still runs `tokens.exe`, so a
/// console window keeps flashing up until the user re-runs `enable`. The task
/// itself calls `autosubmit run`, so doing the check there heals it on the next
/// tick. Best-effort: any failure is reported on stderr and otherwise ignored.
pub fn self_heal_windows_task(settings: &AutosubmitSettings) {
    #[cfg(windows)]
    {
        match migrate_windows_task(settings) {
            Ok(true) => {
                eprintln!("Updated the autosubmit scheduled task to run without a console window.")
            }
            Ok(false) => {}
            Err(err) => eprintln!("Could not update the autosubmit scheduled task: {err:#}"),
        }
    }
    #[cfg(not(windows))]
    let _ = settings;
}

#[cfg(windows)]
fn migrate_windows_task(settings: &AutosubmitSettings) -> Result<bool> {
    if skip_scheduler_install() {
        return Ok(false);
    }
    let exe = std::env::current_exe().context("Could not resolve current tokens executable")?;
    let launcher_present = exe.with_file_name(WINDOWS_LAUNCHER_EXE).is_file();
    if !windows_task_needs_migration(settings, launcher_present, registered_windows_task_command) {
        return Ok(false);
    }
    install_scheduler(SchedulerKind::WindowsTaskScheduler, &exe, settings)?;
    Ok(true)
}

/// The executable the registered task runs, read from `schtasks /Query /XML`
/// (element names are not localized, unlike the `/FO LIST` labels).
#[cfg(windows)]
fn registered_windows_task_command() -> Option<String> {
    let output = Command::new("schtasks")
        .args(["/Query", "/TN", JOB_ID, "/XML"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    windows_task_command_from_xml(&decode_schtasks_output(&output.stdout))
}

/// Whether the registered autosubmit task should be re-created to run the
/// windowless launcher. `registered_command` is only called (it spawns
/// `schtasks`) once the cheap checks pass.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_task_needs_migration(
    settings: &AutosubmitSettings,
    launcher_present: bool,
    registered_command: impl FnOnce() -> Option<String>,
) -> bool {
    if !settings.enabled || !launcher_present {
        return false;
    }
    let scheduler = settings
        .scheduler
        .as_deref()
        .and_then(SchedulerKind::from_str);
    if scheduler != Some(SchedulerKind::WindowsTaskScheduler) {
        return false;
    }
    let Some(command) = registered_command() else {
        return false;
    };
    // Only the file name matters, and it is ASCII, so this holds even if a
    // non-ASCII directory came back mangled by the console code page.
    let file_name = command
        .trim()
        .trim_matches('"')
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default();
    file_name.eq_ignore_ascii_case(WINDOWS_CONSOLE_EXE)
}

/// `<Command>` of the task's first exec action.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_task_command_from_xml(xml: &str) -> Option<String> {
    let start = xml.find("<Command>")? + "<Command>".len();
    let end = start + xml[start..].find("</Command>")?;
    let command = xml[start..end].trim();
    (!command.is_empty()).then(|| command.to_string())
}

/// `schtasks /XML` output may be UTF-16LE (with or without a BOM) or the
/// console code page; decode either well enough to find `<Command>`.
#[cfg_attr(not(windows), allow(dead_code))]
fn decode_schtasks_output(bytes: &[u8]) -> String {
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || (bytes.len() >= 2 && bytes[1] == 0);
    if utf16 {
        let body = bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes);
        // A trailing odd byte is dropped, as `chunks_exact(2)` would.
        let (pairs, _) = body.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn windows_schedule(interval_minutes: u64) -> Result<(String, String)> {
    if interval_minutes < 24 * 60 {
        return Ok(("MINUTE".to_string(), interval_minutes.max(1).to_string()));
    }

    if interval_minutes.is_multiple_of(24 * 60) {
        return Ok((
            "DAILY".to_string(),
            (interval_minutes / (24 * 60)).max(1).to_string(),
        ));
    }

    bail!("{}", t!("commands.autosubmit.windowsIntervalUnsupported"))
}

pub fn replace_cron_block(existing: &str, block: &str) -> String {
    let mut output = Vec::new();
    let mut inside = false;
    for line in existing.lines() {
        if line.trim() == CRON_MARKER_BEGIN {
            inside = true;
            continue;
        }
        if line.trim() == CRON_MARKER_END {
            inside = false;
            continue;
        }
        if !inside {
            output.push(line.to_string());
        }
    }
    output.push(block.to_string());
    output.join("\n") + "\n"
}

fn install_cron_block(block: &str) -> Result<()> {
    let existing = read_crontab().unwrap_or_default();
    let updated = replace_cron_block(&existing, block);
    write_crontab(&updated)
}

fn uninstall_cron_block() -> Result<()> {
    let existing = read_crontab().unwrap_or_default();
    let updated = remove_cron_block(&existing);
    write_crontab(&updated)
}

fn remove_cron_block(existing: &str) -> String {
    let mut output = Vec::new();
    let mut inside = false;
    for line in existing.lines() {
        if line.trim() == CRON_MARKER_BEGIN {
            inside = true;
            continue;
        }
        if line.trim() == CRON_MARKER_END {
            inside = false;
            continue;
        }
        if !inside {
            output.push(line.to_string());
        }
    }
    if output.is_empty() {
        String::new()
    } else {
        output.join("\n") + "\n"
    }
}

fn read_crontab() -> Result<String> {
    let output = Command::new("crontab").arg("-l").output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Ok(String::new())
    }
}

fn write_crontab(content: &str) -> Result<()> {
    use std::io::Write;
    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        bail!(
            "{}",
            tr!("commands.autosubmit.crontabFailed", status = status)
        );
    }
    Ok(())
}

fn run_status_command(program: &str, args: &[String]) -> Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        bail!(
            "{}",
            tr!(
                "commands.autosubmit.commandFailed",
                program = program,
                status = status
            )
        );
    }
    Ok(())
}

fn autosubmit_log_path() -> Result<PathBuf> {
    let dir = crate::paths::get_config_dir().join("autosubmit");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("autosubmit.log"))
}

fn autosubmit_lock_path() -> Result<PathBuf> {
    let dir = crate::paths::get_config_dir().join("autosubmit");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("autosubmit.lock"))
}

fn validate_scheduler_executable(path: &Path) -> Result<()> {
    let rendered = path.to_string_lossy();
    if rendered.contains('\n') || rendered.contains('\r') || rendered.contains('\0') {
        bail!("{}", t!("commands.autosubmit.exeControlChars"));
    }
    Ok(())
}

pub fn format_timestamp_ms(timestamp_ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp_ms)
        .map(|timestamp| timestamp.to_rfc3339())
        .unwrap_or_else(|| timestamp_ms.to_string())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn systemd_escape_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(' ', "\\x20")
}

fn build_date_filter_for_date(
    date: &DateRangeFlags,
    current_date: chrono::NaiveDate,
) -> (Option<String>, Option<String>) {
    use chrono::{Datelike, Duration};

    if date.today {
        let day = current_date.format("%Y-%m-%d").to_string();
        return (Some(day.clone()), Some(day));
    }
    if date.yesterday {
        let day = (current_date - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        return (Some(day.clone()), Some(day));
    }
    if date.week {
        let start = current_date - Duration::days(6);
        return (
            Some(start.format("%Y-%m-%d").to_string()),
            Some(current_date.format("%Y-%m-%d").to_string()),
        );
    }
    if date.month {
        let start = current_date.with_day(1).unwrap_or(current_date);
        return (
            Some(start.format("%Y-%m-%d").to_string()),
            Some(current_date.format("%Y-%m-%d").to_string()),
        );
    }
    (date.since.clone(), date.until.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe_in(dir: &Path, with_launcher: bool) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let exe = dir.join(WINDOWS_CONSOLE_EXE);
        fs::write(&exe, b"").unwrap();
        if with_launcher {
            fs::write(dir.join(WINDOWS_LAUNCHER_EXE), b"").unwrap();
        }
        exe
    }

    fn windows_settings() -> AutosubmitSettings {
        AutosubmitSettings {
            enabled: true,
            interval_minutes: 30,
            scheduler: Some(SchedulerKind::WindowsTaskScheduler.as_str().to_string()),
            ..AutosubmitSettings::default()
        }
    }

    fn task_run_arg(spec: &SchedulerSpec) -> String {
        let (program, args) = &spec.install_commands[0];
        assert_eq!(program, "schtasks");
        let tr = args.iter().position(|arg| arg == "/TR").unwrap();
        args[tr + 1].clone()
    }

    #[test]
    fn windows_task_executable_prefers_launcher_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = exe_in(tmp.path(), true);
        assert_eq!(
            windows_task_executable(&exe),
            tmp.path().join(WINDOWS_LAUNCHER_EXE)
        );
    }

    #[test]
    fn windows_task_executable_falls_back_to_console_exe() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = exe_in(tmp.path(), false);
        assert_eq!(windows_task_executable(&exe), exe);
    }

    #[test]
    fn windows_task_executable_ignores_launcher_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = exe_in(tmp.path(), false);
        fs::create_dir(tmp.path().join(WINDOWS_LAUNCHER_EXE)).unwrap();
        assert_eq!(windows_task_executable(&exe), exe);
    }

    #[test]
    fn windows_task_run_quotes_paths_with_spaces_and_non_ascii() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Program Files").join("ノード 工具");
        let settings = windows_settings();

        let exe = exe_in(&dir, false);
        let spec = render_windows_task_spec(&exe, &settings).unwrap();
        assert_eq!(
            task_run_arg(&spec),
            format!(r#""{}" autosubmit run"#, exe.display())
        );

        let exe = exe_in(&dir, true);
        let spec = render_windows_task_spec(&exe, &settings).unwrap();
        assert_eq!(
            task_run_arg(&spec),
            format!(
                r#""{}" autosubmit run"#,
                dir.join(WINDOWS_LAUNCHER_EXE).display()
            )
        );
    }

    #[test]
    fn migrates_task_that_still_runs_console_exe() {
        let settings = windows_settings();
        for command in [
            r#"C:\Users\me\AppData\Roaming\npm\node_modules\tokens-cli\bin\tokens.exe"#,
            r#""C:\Program Files\nodejs\TOKENS.EXE""#,
            r#"C:\Users\ユーザー\tokens.exe"#,
        ] {
            assert!(
                windows_task_needs_migration(&settings, true, || Some(command.to_string())),
                "{command}"
            );
        }
    }

    #[test]
    fn leaves_task_alone_when_not_applicable() {
        let settings = windows_settings();
        let old = || Some(r#"C:\npm\tokens.exe"#.to_string());

        // Already on the launcher, or nothing registered / unreadable.
        assert!(!windows_task_needs_migration(&settings, true, || Some(
            r#""C:\npm\tokensw.exe""#.to_string()
        )));
        assert!(!windows_task_needs_migration(&settings, true, || None));
        // No launcher shipped next to us.
        assert!(!windows_task_needs_migration(&settings, false, old));
        // Disabled, or a different scheduler.
        let disabled = AutosubmitSettings {
            enabled: false,
            ..windows_settings()
        };
        assert!(!windows_task_needs_migration(&disabled, true, old));
        let cron = AutosubmitSettings {
            scheduler: Some("cron".to_string()),
            ..windows_settings()
        };
        assert!(!windows_task_needs_migration(&cron, true, old));
        let unknown = AutosubmitSettings {
            scheduler: None,
            ..windows_settings()
        };
        assert!(!windows_task_needs_migration(&unknown, true, old));
    }

    #[test]
    fn skips_schtasks_query_when_cheap_checks_fail() {
        let disabled = AutosubmitSettings {
            enabled: false,
            ..windows_settings()
        };
        let mut queried = false;
        windows_task_needs_migration(&disabled, true, || {
            queried = true;
            None
        });
        windows_task_needs_migration(&windows_settings(), false, || {
            queried = true;
            None
        });
        assert!(!queried);
    }

    #[test]
    fn reads_command_from_task_xml_in_utf16_and_utf8() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n<Task><Actions Context=\"Author\"><Exec>\r\n  <Command>\"C:\\Program Files\\nodejs\\tokens.exe\"</Command>\r\n  <Arguments>autosubmit run</Arguments>\r\n</Exec></Actions></Task>";
        let expected = "\"C:\\Program Files\\nodejs\\tokens.exe\"";

        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
        let decoded = decode_schtasks_output(&utf16);
        assert_eq!(
            windows_task_command_from_xml(&decoded).as_deref(),
            Some(expected)
        );

        let no_bom: Vec<u8> = xml.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(
            windows_task_command_from_xml(&decode_schtasks_output(&no_bom)).as_deref(),
            Some(expected)
        );

        assert_eq!(
            windows_task_command_from_xml(&decode_schtasks_output(xml.as_bytes())).as_deref(),
            Some(expected)
        );
        assert_eq!(windows_task_command_from_xml("<Task></Task>"), None);
    }
}
