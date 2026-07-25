mod antigravity;
mod auth;
mod commands;
mod cursor;
mod device;
mod paths;
mod settings;
mod timezone;
mod trae;
mod warp;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use std::io::{IsTerminal};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "tokens")]
#[command(author, version, about = "AI token usage analytics")]
struct Cli {
    // No global flags: the filters that used to live here (--json, --client,
    // --today/--week/--since/...) only ever fed the report commands, which are
    // gone. The subcommands that still need them declare their own.
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Login to Tokens (opens browser for GitHub auth)")]
    Login {
        #[arg(long, help = "Save an existing Tokens API token without browser auth")]
        token: Option<String>,
    },
    #[command(about = "Logout from Tokens")]
    Logout,
    #[command(about = "Show current logged in user")]
    Whoami,
    #[command(about = "Show local auth, device, and background service status")]
    Status {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(
        about = "Import historical usage from a third-party aggregate export (e.g. clawdboard) into tokens JSON"
    )]
    Import {
        #[arg(help = "Path to the export file to import")]
        file: String,
        #[arg(
            long,
            default_value = "clawdboard",
            help = "Export format (currently only 'clawdboard')"
        )]
        format: String,
        #[arg(
            long,
            help = "Write normalized tokens JSON to this file instead of stdout"
        )]
        output: Option<String>,
        #[arg(long, help = "Parse and summarize only; do not emit normalized JSON")]
        dry_run: bool,
    },
    #[command(about = "Submit usage data to the Tokens social platform")]
    Submit {
        #[command(flatten)]
        clients: ClientFlags,
        #[command(flatten)]
        date: DateRangeFlags,
        #[arg(
            long,
            help = "Show what would be submitted without actually submitting"
        )]
        dry_run: bool,
        #[arg(
            long,
            help = "Authoritatively replace explicitly selected clients within --since/--until"
        )]
        replace: bool,
    },
    #[command(about = "Run in the background and submit usage on a recurring interval")]
    Serve {
        #[command(flatten)]
        clients: ClientFlags,
        #[arg(
            long,
            value_name = "MINUTES",
            help = "Minutes between submissions (default 30, or $TOKENS_SUBMIT_INTERVAL)"
        )]
        interval: Option<u64>,
    },
    #[command(about = "Manage periodic usage submission")]
    Autosubmit {
        #[command(subcommand)]
        subcommand: commands::autosubmit::AutosubmitSubcommand,
    },
    #[command(about = "Capture subprocess output for token usage tracking")]
    Headless {
        #[arg(help = "Source CLI (currently only 'codex' supported)")]
        source: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
        #[arg(long, help = "Override output format (json or jsonl)")]
        format: Option<String>,
        #[arg(long, help = "Write captured output to file")]
        output: Option<String>,
        #[arg(long, help = "Do not auto-add JSON output flags")]
        no_auto_flags: bool,
    },
    #[command(about = "Codex account integration commands")]
    Codex {
        #[command(subcommand)]
        subcommand: CodexSubcommand,
    },
    #[command(about = "Cursor API cache integration commands")]
    Cursor {
        #[command(subcommand)]
        subcommand: CursorSubcommand,
    },
    #[command(about = "Antigravity integration commands")]
    Antigravity {
        #[command(subcommand)]
        subcommand: AntigravitySubcommand,
    },
    #[command(about = "Trae IDE integration commands")]
    Trae {
        #[command(subcommand)]
        subcommand: TraeSubcommand,
    },
    #[command(about = "Warp/Oz aggregate usage integration commands")]
    Warp {
        #[command(subcommand)]
        subcommand: WarpSubcommand,
    },
    #[command(about = "Delete all submitted usage data from the server")]
    DeleteSubmittedData,
}

#[derive(Subcommand)]
enum CursorSubcommand {
    #[command(about = "Login to Cursor with a browser session token")]
    Login {
        #[arg(long, help = "Label for this Cursor account (e.g., work, personal)")]
        name: Option<String>,
    },
    #[command(about = "Logout from a Cursor account")]
    Logout {
        #[arg(long, help = "Account label or id")]
        name: Option<String>,
        #[arg(long, help = "Logout from all Cursor accounts")]
        all: bool,
        #[arg(long, help = "Also delete cached Cursor usage")]
        purge_cache: bool,
    },
    #[command(about = "Check Cursor authentication status")]
    Status {
        #[arg(long, help = "Account label or id")]
        name: Option<String>,
    },
    #[command(about = "List saved Cursor accounts")]
    Accounts {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Sync Cursor API usage into cursor-cache/usage*.csv")]
    Sync {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Switch active Cursor account")]
    Switch {
        #[arg(help = "Account label or id")]
        name: String,
    },
}

#[derive(Subcommand)]
enum CodexSubcommand {
    #[command(about = "Import the current Codex OAuth credentials as a saved account")]
    Import {
        #[arg(long, help = "Label for this Codex account (e.g., work, personal)")]
        name: Option<String>,
    },
    #[command(about = "List saved Codex accounts")]
    Accounts {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Switch active Codex account and write Codex auth.json")]
    Switch {
        #[arg(help = "Account label or id")]
        name: String,
    },
    #[command(about = "Remove a saved Codex account")]
    Remove {
        #[arg(help = "Account label or id")]
        name: String,
    },
    #[command(about = "Check Codex subscription usage for an account")]
    Status {
        #[arg(long, help = "Account label or id")]
        name: Option<String>,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Show an opt-in Codex account-activity snapshot")]
    Activity {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
}

#[derive(Subcommand)]
enum AntigravitySubcommand {
    #[command(about = "Sync usage from running Antigravity language servers")]
    Sync,
    #[command(about = "Show Antigravity sync status")]
    Status {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Delete cached Antigravity usage artifacts")]
    PurgeCache,
}

#[derive(Subcommand)]
enum TraeSubcommand {
    #[command(about = "Authenticate Trae — auto-detect from desktop client or paste JWT")]
    Login {
        #[arg(long, help = "Paste access token directly (for manual fallback)")]
        manual: bool,
        #[arg(long, help = "Target Trae variant (solo, ide)")]
        variant: Option<String>,
    },
    #[command(about = "Remove cached Trae credentials")]
    Logout {
        #[arg(long, help = "Target Trae variant (solo, ide)")]
        variant: Option<String>,
    },
    #[command(about = "Show Trae authentication status")]
    Status {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Sync Trae usage data into local cache")]
    Sync {
        #[arg(long, help = "Number of days to sync (default: 30)")]
        since: Option<i64>,
        #[arg(long, help = "Include auxiliary usage types (not just main chat)")]
        include_aux: bool,
    },
}

#[derive(Subcommand)]
enum WarpSubcommand {
    #[command(about = "Save Warp GraphQL authentication for aggregate usage sync")]
    Login {
        #[arg(long, help = "Warp bearer token or cookie header value")]
        token: Option<String>,
        #[arg(
            long,
            help = "Treat token as a Cookie header instead of a bearer token"
        )]
        cookie: bool,
    },
    #[command(about = "Remove cached Warp credentials")]
    Logout {
        #[arg(long, help = "Also delete cached Warp aggregate usage")]
        purge_cache: bool,
    },
    #[command(about = "Show Warp aggregate sync status")]
    Status {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Sync Warp aggregate usage into local cache")]
    Sync {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Install user-configured model aliases once, before any scan runs, so
    // model-name variants fold consistently across every command. An empty or
    // absent config is a strict no-op.
    tokens_core::model_alias::set_global(&settings::load_model_aliases());

    // Pin the date-bucketing timezone before any scanning so usage is
    // attributed to stable calendar dates regardless of where `submit` runs.
    timezone::install();

    match cli.command {
        Some(Commands::Login { token }) => {
            run_login_command(token)
        }
        Some(Commands::Logout) => {
            run_logout_command()
        }
        Some(Commands::Whoami) => {
            run_whoami_command()
        }
        Some(Commands::Status { json }) => {
            commands::status::run(json)
        }
        Some(Commands::Import {
            file,
            format,
            output,
            dry_run,
        }) => {
            run_import_command(file, format, output, dry_run)
        }
        Some(Commands::Submit {
            clients,
            date,
            dry_run,
            replace,
        }) => {
            // Bypass settings.json defaultClients for the submit path: we want the
            // submit-specific default_submit_clients() fallback (in run_submit_command)
            // to fire when the user passes no client flags, not the user's general
            // defaultClients view filter (which may exclude clients they still want
            // to upload). Pass an explicit empty defaults slice.
            let clients = build_client_filter_with_defaults(clients, &[]);
            let replacement = resolve_submit_replacement(replace, clients.as_deref(), &date)?;
            let (since, until) = build_date_filter(&date);
            let year = normalize_year_filter(&date);
            run_submit_command(
                clients,
                since,
                until,
                year,
                dry_run,
                SubmitMode::Interactive,
                replacement,
            )
        }
        Some(Commands::Serve { clients, interval }) => {
            let clients = build_client_filter_with_defaults(clients, &[]);
            run_serve(interval, clients)
        }
        Some(Commands::Autosubmit { subcommand }) => {
            run_autosubmit_command(subcommand)
        }
        Some(Commands::Headless {
            source,
            args,
            format,
            output,
            no_auto_flags,
        }) => {
            run_headless_command(&source, args, format, output, no_auto_flags)
        }
        Some(Commands::Cursor { subcommand }) => {
            run_cursor_command(subcommand)
        }
        Some(Commands::Antigravity { subcommand }) => {
            run_antigravity_command(subcommand)
        }
        Some(Commands::Codex { subcommand }) => {
            run_codex_command(subcommand)
        }
        Some(Commands::Trae { subcommand }) => {
            run_trae_command(subcommand)
        }
        Some(Commands::Warp { subcommand }) => {
            run_warp_command(subcommand)
        }
        Some(Commands::DeleteSubmittedData) => {
            run_delete_data_command()
        }
        None => {
            // The dashboard and the report commands are gone, so a bare
            // `tokens` prints usage rather than rendering anything.
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

/// Client identifiers exposed via `--client`.
///
/// Mirrors `tokens_core::ClientId` plus the `Synthetic` meta-client. We
/// duplicate the variant set on the CLI side so `tokens-core` stays free of
/// CLI-parsing dependencies and so `Synthetic` (which has no scan path of its
/// own) can be treated as a first-class filter value without changing core
/// invariants.
///
/// Variant order intentionally mirrors `ClientId::ALL` declaration order so
/// the TUI source picker, `--help`'s `[possible values: ...]` listing, and
/// any future iteration over `ClientFilter::value_variants()` agree on a
/// single chronological ordering. `Synthetic` is appended at the end since
/// it has no `ClientId` counterpart.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[value(rename_all = "lowercase")]
pub enum ClientFilter {
    Opencode,
    Claude,
    Codex,
    Cursor,
    Gemini,
    Amp,
    Droid,
    Openclaw,
    Pi,
    Kimi,
    Qwen,
    Roocode,
    Kilocode,
    Mux,
    Kilo,
    Crush,
    Hermes,
    Copilot,
    Goose,
    Codebuff,
    Antigravity,
    Zed,
    Kiro,
    #[value(name = "trae")]
    Trae,
    Warp,
    Cline,
    #[value(name = "9router")]
    NineRouter,
    Gjc,
    Grok,
    Jcode,
    Commandcode,
    Micode,
    #[value(name = "antigravity-cli")]
    AntigravityCli,
    Junie,
    Zcode,
    Opencodereview,
    Codebuddy,
    Workbuddy,
    #[value(name = "devin-cli")]
    DevinCli,
    #[value(name = "devin-desktop")]
    DevinDesktop,
    Synthetic,
}

impl ClientFilter {
    /// Returns the canonical lowercase identifier consumed by
    /// `tokens_core` filter lists. Must match `ClientId::as_str` for every
    /// variant that has a corresponding `ClientId`.
    pub fn as_filter_str(&self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Gemini => "gemini",
            Self::Amp => "amp",
            Self::Droid => "droid",
            Self::Openclaw => "openclaw",
            Self::Pi => "pi",
            Self::Kimi => "kimi",
            Self::Qwen => "qwen",
            Self::Roocode => "roocode",
            Self::Kilocode => "kilocode",
            Self::Mux => "mux",
            Self::Kilo => "kilo",
            Self::Crush => "crush",
            Self::Hermes => "hermes",
            Self::Copilot => "copilot",
            Self::Goose => "goose",
            Self::Codebuff => "codebuff",
            Self::Antigravity => "antigravity",
            Self::Zed => "zed",
            Self::Kiro => "kiro",
            Self::Trae => "trae",
            Self::Warp => "warp",
            Self::Cline => "cline",
            Self::Gjc => "gjc",
            Self::NineRouter => "9router",
            Self::Grok => "grok",
            Self::Jcode => "jcode",
            Self::Commandcode => "commandcode",
            Self::Micode => "micode",
            Self::AntigravityCli => "antigravity-cli",
            Self::Junie => "junie",
            Self::Zcode => "zcode",
            Self::Opencodereview => "opencodereview",
            Self::Codebuddy => "codebuddy",
            Self::Workbuddy => "workbuddy",
            Self::DevinCli => "devin-cli",
            Self::DevinDesktop => "devin-desktop",
            Self::Synthetic => "synthetic",
        }
    }

    /// Convert to the corresponding `ClientId`, or `None` for the
    /// `Synthetic` meta-client which has no scan path of its own.
    ///
    /// Used at boundaries where TUI state (`HashSet<ClientFilter>`) needs
    /// to feed core APIs that still consume `Vec<ClientId>`.
    pub fn to_client_id(self) -> Option<tokens_core::ClientId> {
        use tokens_core::ClientId;
        match self {
            Self::Opencode => Some(ClientId::OpenCode),
            Self::Claude => Some(ClientId::Claude),
            Self::Codex => Some(ClientId::Codex),
            Self::Cursor => Some(ClientId::Cursor),
            Self::Gemini => Some(ClientId::Gemini),
            Self::Amp => Some(ClientId::Amp),
            Self::Droid => Some(ClientId::Droid),
            Self::Openclaw => Some(ClientId::OpenClaw),
            Self::Pi => Some(ClientId::Pi),
            Self::Kimi => Some(ClientId::Kimi),
            Self::Qwen => Some(ClientId::Qwen),
            Self::Roocode => Some(ClientId::RooCode),
            Self::Kilocode => Some(ClientId::KiloCode),
            Self::Mux => Some(ClientId::Mux),
            Self::Kilo => Some(ClientId::Kilo),
            Self::Crush => Some(ClientId::Crush),
            Self::Hermes => Some(ClientId::Hermes),
            Self::Copilot => Some(ClientId::Copilot),
            Self::Goose => Some(ClientId::Goose),
            Self::Codebuff => Some(ClientId::Codebuff),
            Self::Antigravity => Some(ClientId::Antigravity),
            Self::Zed => Some(ClientId::Zed),
            Self::Kiro => Some(ClientId::Kiro),
            Self::Trae => Some(ClientId::Trae),
            Self::Warp => Some(ClientId::Warp),
            Self::Cline => Some(ClientId::Cline),
            Self::Gjc => Some(ClientId::Gjc),
            Self::NineRouter => Some(ClientId::Gjc),
            Self::Grok => Some(ClientId::Grok),
            Self::Jcode => Some(ClientId::Jcode),
            Self::Commandcode => Some(ClientId::CommandCode),
            Self::Micode => Some(ClientId::MiMoCode),
            Self::AntigravityCli => Some(ClientId::AntigravityCli),
            Self::Junie => Some(ClientId::Junie),
            Self::Zcode => Some(ClientId::Zcode),
            Self::Opencodereview => Some(ClientId::OpenCodeReview),
            Self::Codebuddy => Some(ClientId::CodeBuddy),
            Self::Workbuddy => Some(ClientId::WorkBuddy),
            Self::DevinCli => Some(ClientId::DevinCli),
            Self::DevinDesktop => Some(ClientId::DevinDesktop),
            Self::Synthetic => None,
        }
    }

    /// Lift a `ClientId` back into a `ClientFilter`. Total inverse of
    /// `to_client_id` for non-`Synthetic` variants.
    pub fn from_client_id(client: tokens_core::ClientId) -> Self {
        use tokens_core::ClientId;
        match client {
            ClientId::OpenCode => Self::Opencode,
            ClientId::Claude => Self::Claude,
            ClientId::Codex => Self::Codex,
            ClientId::Cursor => Self::Cursor,
            ClientId::Gemini => Self::Gemini,
            ClientId::Amp => Self::Amp,
            ClientId::Droid => Self::Droid,
            ClientId::OpenClaw => Self::Openclaw,
            ClientId::Pi => Self::Pi,
            ClientId::Kimi => Self::Kimi,
            ClientId::Qwen => Self::Qwen,
            ClientId::RooCode => Self::Roocode,
            ClientId::KiloCode => Self::Kilocode,
            ClientId::Mux => Self::Mux,
            ClientId::Kilo => Self::Kilo,
            ClientId::Crush => Self::Crush,
            ClientId::Hermes => Self::Hermes,
            ClientId::Copilot => Self::Copilot,
            ClientId::Goose => Self::Goose,
            ClientId::Codebuff => Self::Codebuff,
            ClientId::Antigravity => Self::Antigravity,
            ClientId::Zed => Self::Zed,
            ClientId::Kiro => Self::Kiro,
            ClientId::Trae => Self::Trae,
            ClientId::Warp => Self::Warp,
            ClientId::Cline => Self::Cline,
            ClientId::Gjc => Self::Gjc,
            ClientId::Grok => Self::Grok,
            ClientId::Jcode => Self::Jcode,
            ClientId::CommandCode => Self::Commandcode,
            ClientId::MiMoCode => Self::Micode,
            ClientId::AntigravityCli => Self::AntigravityCli,
            ClientId::Junie => Self::Junie,
            ClientId::Zcode => Self::Zcode,
            ClientId::OpenCodeReview => Self::Opencodereview,
            ClientId::CodeBuddy => Self::Codebuddy,
            ClientId::WorkBuddy => Self::Workbuddy,
            ClientId::DevinCli => Self::DevinCli,
            ClientId::DevinDesktop => Self::DevinDesktop,
        }
    }

    /// Parse a canonical lowercase identifier (the same form
    /// `as_filter_str` returns) into a `ClientFilter`. Returns `None` for
    /// any unknown id so callers can drop unrecognized settings entries
    /// without erroring.
    pub fn from_filter_str(s: &str) -> Option<Self> {
        Self::value_variants()
            .iter()
            .copied()
            .find(|f| f.as_filter_str() == s)
    }

    /// The "no filter" default set: every real client, with `Synthetic`
    /// **excluded**. Matches the pre-refactor behavior where a missing
    /// filter scanned every `ClientId` but did NOT post-process synthetic
    /// (synthetic detection has always been opt-in because it
    /// re-attributes messages from other clients to a different bucket).
    ///
    /// Single source of truth: every code path that needs a default
    /// filter (TUI launch, `submit` warm cache, etc.) must consult this
    /// so the cache key, the in-app state, and the loader filter all
    /// agree. Drift between them produces stale-cache misses on every
    /// launch.
    pub fn default_set() -> std::collections::HashSet<Self> {
        Self::value_variants()
            .iter()
            .copied()
            .filter(|f| !matches!(f, Self::Synthetic | Self::NineRouter))
            .collect()
    }
}

#[derive(Args, Clone, Debug, Default)]
pub struct ClientFlags {
    /// Canonical client filter. Repeatable or comma-separated.
    /// Example: `--client opencode,claude` or `-c opencode -c claude`.
    #[arg(
        id = "client_filter",
        long = "client",
        short = 'c',
        value_name = "CLIENTS",
        value_enum,
        value_delimiter = ',',
        action = clap::ArgAction::Append,
        ignore_case = true,
        help = "Filter by client(s). Repeatable or comma-separated (e.g. -c opencode,claude)."
    )]
    pub clients: Vec<ClientFilter>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct DateRangeFlags {
    #[arg(
        long,
        help = "Show only today's usage",
        conflicts_with_all = ["yesterday", "week", "month", "since", "until", "year"]
    )]
    pub today: bool,
    #[arg(
        long,
        help = "Show only yesterday's usage",
        conflicts_with_all = ["week", "month", "since", "until", "year"]
    )]
    pub yesterday: bool,
    #[arg(
        long,
        help = "Show last 7 days",
        conflicts_with_all = ["month", "since", "until", "year"]
    )]
    pub week: bool,
    #[arg(
        long,
        help = "Show current month",
        conflicts_with_all = ["since", "until", "year"]
    )]
    pub month: bool,
    #[arg(long, help = "Start date (YYYY-MM-DD)")]
    pub since: Option<String>,
    #[arg(long, help = "End date (YYYY-MM-DD)")]
    pub until: Option<String>,
    #[arg(long, help = "Filter by year (YYYY)")]
    pub year: Option<String>,
}

/// Pure variant of [`build_client_filter`] for unit-testable resolution.
/// `defaults` is the (already-validated) list of canonical filter ids that
/// should apply when no CLI flag is present.
fn build_client_filter_with_defaults(
    flags: ClientFlags,
    defaults: &[String],
) -> Option<Vec<String>> {
    let mut ordered: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for client in &flags.clients {
        let id = client.as_filter_str().to_string();
        if seen.insert(id.clone()) {
            ordered.push(id);
        }
    }

    // Defaults only apply when the user passed no canonical `--client` flags.
    // CLI flags always win — predictable semantics over "merge". Unknown /
    // typo'd ids are dropped silently so a stale settings.json entry never
    // breaks tokens.
    if ordered.is_empty() {
        for raw in defaults {
            if let Some(client) = ClientFilter::from_filter_str(raw) {
                let id = client.as_filter_str().to_string();
                if seen.insert(id.clone()) {
                    ordered.push(id);
                }
            }
        }
    }

    if ordered.is_empty() {
        None
    } else {
        Some(ordered)
    }
}

fn client_filter_explicitly_requests_cursor(clients: &Option<Vec<String>>) -> bool {
    clients
        .as_ref()
        .is_some_and(|sources| sources.iter().any(|source| source == "cursor"))
}

fn client_filter_explicitly_requests_warp(clients: &Option<Vec<String>>) -> bool {
    clients
        .as_ref()
        .is_some_and(|sources| sources.iter().any(|source| source == "warp"))
}

#[derive(Debug)]
struct CursorSetupState {
    has_credentials: bool,
    has_cache: bool,
    cache_glob: String,
}

fn cursor_setup_state() -> Option<CursorSetupState> {
    let home_path = dirs::home_dir()?;
    let has_credentials = cursor::is_cursor_logged_in();
    let has_cache = cursor::has_cursor_usage_cache_in_home(&home_path);
    let cache_glob = "~/.config/tokens/cursor-cache/usage*.csv".to_string();

    Some(CursorSetupState {
        has_credentials,
        has_cache,
        cache_glob,
    })
}

fn has_cursor_usage_cache() -> bool {
    cursor_setup_state().is_some_and(|state| state.has_cache)
}

fn cursor_setup_warnings(clients: &Option<Vec<String>>) -> Vec<String> {
    if !client_filter_explicitly_requests_cursor(clients) {
        return Vec::new();
    }

    let Some(state) = cursor_setup_state() else {
        return vec![
            "Cursor usage requires the Tokens Cursor API cache, but the home directory could not be resolved. Run `tokens cursor login` and `tokens cursor sync --json`. Tokens does not parse local `~/.cursor` session data.".to_string(),
        ];
    };
    if state.has_cache {
        return Vec::new();
    }

    let action = if state.has_credentials {
        "run `tokens cursor sync --json`"
    } else {
        "run `tokens cursor login` and `tokens cursor sync --json`"
    };

    vec![format!(
        "Cursor usage requires the Tokens Cursor API cache at `{}`; {}. Tokens does not parse local `~/.cursor` session data.",
        state.cache_glob, action
    )]
}

fn emit_cursor_setup_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }

    use colored::Colorize;
    for warning in warnings {
        eprintln!("{}", format!("  Warning: {}", warning).yellow());
    }
}

fn warp_setup_warnings(clients: &Option<Vec<String>>) -> Vec<String> {
    if !client_filter_explicitly_requests_warp(clients) {
        return Vec::new();
    }

    if warp::load_usage_cache().is_some() {
        return Vec::new();
    }

    let cache_glob = "~/.config/tokens/warp-cache/usage*.json".to_string();
    let action = if warp::has_credentials() {
        "run `tokens warp sync`"
    } else {
        "run `tokens warp login` and `tokens warp sync`"
    };

    vec![format!(
        "Warp usage requires the Tokens aggregate API cache at `{}`; {}. Tokens does not parse local Warp/Oz session transcripts and does not infer tokens from request counts.",
        cache_glob, action
    )]
}

fn setup_warnings(clients: &Option<Vec<String>>) -> Vec<String> {
    let mut warnings = cursor_setup_warnings(clients);
    warnings.extend(warp_setup_warnings(clients));
    warnings
}

fn default_submit_clients() -> Vec<String> {
    let mut clients: Vec<String> = tokens_core::ClientId::iter()
        .filter(|client| client.submit_default())
        .map(|client| client.as_str().to_string())
        .collect();
    clients.push("synthetic".to_string());
    clients
}


fn build_date_filter(date: &DateRangeFlags) -> (Option<String>, Option<String>) {
    build_date_filter_for_date(date, tokens_core::bucket_timezone().today())
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

fn normalize_year_filter(date: &DateRangeFlags) -> Option<String> {
    if date.today || date.yesterday || date.week || date.month {
        None
    } else {
        date.year.clone()
    }
}

fn format_currency(n: f64) -> String {
    format!("${:.2}", n)
}

/// Format a URL as an OSC 8 clickable hyperlink for supported terminals.
/// Falls back to plain URL text when stdout is not a terminal.
fn osc8_link(url: &str) -> String {
    if std::io::stdout().is_terminal() {
        format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, url)
    } else {
        url.to_string()
    }
}
/// Format text as an OSC 8 clickable hyperlink with custom display text.
/// Falls back to plain display text when stdout is not a terminal.
fn osc8_link_with_text(url: &str, text: &str) -> String {
    if std::io::stdout().is_terminal() {
        format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, text)
    } else {
        text.to_string()
    }
}

fn get_headless_roots(home_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(env_dir) = std::env::var("TOKENS_HEADLESS_DIR") {
        roots.push(PathBuf::from(env_dir));
    } else {
        roots.push(home_dir.join(".config/tokens/headless"));

        #[cfg(target_os = "macos")]
        {
            roots.push(home_dir.join("Library/Application Support/tokens/headless"));
        }
    }

    roots
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsTokenBreakdown {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsSourceContribution {
    client: String,
    model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
    tokens: TsTokenBreakdown,
    cost: f64,
    messages: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<TsClientContributionProvenance>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsClientContributionProvenance {
    schema_version: u32,
    message_count: i32,
    model_count: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsDailyTotals {
    tokens: i64,
    cost: f64,
    messages: i32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsDailyContribution {
    date: String,
    totals: TsDailyTotals,
    intensity: u8,
    token_breakdown: TsTokenBreakdown,
    clients: Vec<TsSourceContribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_time_ms: Option<i64>,
}

#[derive(serde::Serialize)]
struct DateRange {
    start: String,
    end: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsYearSummary {
    year: String,
    total_tokens: i64,
    total_cost: f64,
    range: DateRange,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsDataSummary {
    total_tokens: i64,
    total_cost: f64,
    total_days: i32,
    active_days: i32,
    average_per_day: f64,
    max_cost_in_single_day: f64,
    clients: Vec<String>,
    models: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsExportMeta {
    generated_at: String,
    version: String,
    date_range: DateRange,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsSubmitDevice {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsTimeMetrics {
    total_active_time_ms: i64,
    longest_continuous_ms: i64,
    max_concurrent_sessions: u32,
    session_count: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsClientManifestCoverage {
    mode: &'static str,
    start: String,
    end: String,
    missing_data: &'static str,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsClientManifestEntry {
    client: String,
    parser_revision: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage: Option<TsClientManifestCoverage>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsClientManifest {
    schema_version: u32,
    clients: Vec<TsClientManifestEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SubmitReplacementCoverage {
    clients: Vec<String>,
    start: String,
    end: String,
}

fn resolve_submit_replacement(
    replace: bool,
    clients: Option<&[String]>,
    date: &DateRangeFlags,
) -> Result<Option<SubmitReplacementCoverage>> {
    if !replace {
        return Ok(None);
    }
    if date.today || date.yesterday || date.week || date.month || date.year.is_some() {
        return Err(anyhow::anyhow!(
            "--replace only accepts explicit --since and --until bounds; remove --today/--yesterday/--week/--month/--year"
        ));
    }

    let replacement_clients = clients
        .filter(|clients| !clients.is_empty())
        .map(<[String]>::to_vec)
        .ok_or_else(|| anyhow::anyhow!("--replace requires at least one explicit --client"))?;
    let start = date
        .since
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--replace requires a bounded --since date"))?;
    let end = date
        .until
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--replace requires a bounded --until date"))?;
    let start_date = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("--replace --since must use YYYY-MM-DD"))?;
    let end_date = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("--replace --until must use YYYY-MM-DD"))?;
    if start_date > end_date {
        return Err(anyhow::anyhow!(
            "--replace --until must be on or after --since"
        ));
    }

    Ok(Some(SubmitReplacementCoverage {
        clients: replacement_clients,
        start: start.to_string(),
        end: end.to_string(),
    }))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TsTokenContributionData {
    meta: TsExportMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<TsSubmitDevice>,
    summary: TsDataSummary,
    years: Vec<TsYearSummary>,
    contributions: Vec<TsDailyContribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_metrics: Option<TsTimeMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcp_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_manifest: Option<TsClientManifest>,
}

fn submit_parser_revision(client: &str) -> u32 {
    if client == "codex" {
        2
    } else {
        1
    }
}

fn build_submit_client_manifest(
    graph: &tokens_core::GraphResult,
    replacement: Option<&SubmitReplacementCoverage>,
) -> TsClientManifest {
    use std::collections::{BTreeMap, BTreeSet};

    let mut clients = BTreeSet::new();
    for day in &graph.contributions {
        for contribution in &day.clients {
            clients.insert(contribution.client.clone());
        }
    }
    if let Some(replacement) = replacement {
        clients.extend(replacement.clients.iter().cloned());
    }

    let replacement_clients: BTreeMap<&str, &SubmitReplacementCoverage> = replacement
        .into_iter()
        .flat_map(|coverage| {
            coverage
                .clients
                .iter()
                .map(move |client| (client.as_str(), coverage))
        })
        .collect();

    TsClientManifest {
        schema_version: 1,
        clients: clients
            .into_iter()
            .map(|client| {
                let coverage = replacement_clients.get(client.as_str()).map(|replacement| {
                    TsClientManifestCoverage {
                        mode: "full",
                        start: replacement.start.clone(),
                        end: replacement.end.clone(),
                        missing_data: "tombstone",
                    }
                });
                TsClientManifestEntry {
                    parser_revision: submit_parser_revision(&client),
                    client,
                    coverage,
                }
            })
            .collect(),
    }
}

fn to_ts_token_contribution_data(
    graph: &tokens_core::GraphResult,
    device: Option<&device::SubmitDevice>,
    replacement: Option<&SubmitReplacementCoverage>,
) -> TsTokenContributionData {
    let include_submit_provenance = device.is_some();

    TsTokenContributionData {
        meta: TsExportMeta {
            generated_at: graph.meta.generated_at.clone(),
            version: graph.meta.version.clone(),
            date_range: DateRange {
                start: graph.meta.date_range_start.clone(),
                end: graph.meta.date_range_end.clone(),
            },
        },
        device: device.map(|d| TsSubmitDevice {
            id: d.id.clone(),
            name: d.name.clone(),
        }),
        client_manifest: device.map(|_| build_submit_client_manifest(graph, replacement)),
        summary: TsDataSummary {
            total_tokens: graph.summary.total_tokens,
            total_cost: graph.summary.total_cost,
            total_days: graph.summary.total_days,
            active_days: graph.summary.active_days,
            average_per_day: graph.summary.average_per_day,
            max_cost_in_single_day: graph.summary.max_cost_in_single_day,
            clients: graph.summary.clients.clone(),
            models: graph.summary.models.clone(),
        },
        years: graph
            .years
            .iter()
            .map(|y| TsYearSummary {
                year: y.year.clone(),
                total_tokens: y.total_tokens,
                total_cost: y.total_cost,
                range: DateRange {
                    start: y.range_start.clone(),
                    end: y.range_end.clone(),
                },
            })
            .collect(),
        contributions: graph
            .contributions
            .iter()
            .map(|d| TsDailyContribution {
                date: d.date.clone(),
                totals: TsDailyTotals {
                    tokens: d.totals.tokens,
                    cost: d.totals.cost,
                    messages: d.totals.messages,
                },
                intensity: d.intensity,
                token_breakdown: TsTokenBreakdown {
                    input: d.token_breakdown.input,
                    output: d.token_breakdown.output,
                    cache_read: d.token_breakdown.cache_read,
                    cache_write: d.token_breakdown.cache_write,
                    reasoning: d.token_breakdown.reasoning,
                },
                clients: d
                    .clients
                    .iter()
                    .map(|s| TsSourceContribution {
                        client: s.client.clone(),
                        model_id: s.model_id.clone(),
                        provider_id: if s.provider_id.is_empty() {
                            None
                        } else {
                            Some(s.provider_id.clone())
                        },
                        tokens: TsTokenBreakdown {
                            input: s.tokens.input,
                            output: s.tokens.output,
                            cache_read: s.tokens.cache_read,
                            cache_write: s.tokens.cache_write,
                            reasoning: s.tokens.reasoning,
                        },
                        cost: s.cost,
                        messages: s.messages,
                        provenance: include_submit_provenance.then(|| {
                            TsClientContributionProvenance {
                                schema_version: submit_parser_revision(&s.client),
                                message_count: s.messages,
                                model_count: 1,
                            }
                        }),
                    })
                    .collect(),
                active_time_ms: d.active_time_ms,
            })
            .collect(),
        time_metrics: graph.time_metrics.as_ref().map(|tm| TsTimeMetrics {
            total_active_time_ms: tm.total_active_time_ms,
            longest_continuous_ms: tm.longest_continuous_ms,
            max_concurrent_sessions: tm.max_concurrent_sessions,
            session_count: tm.session_count,
        }),
        mcp_servers: {
            let servers = tokens_core::mcp::discover_mcp_server_names(None);
            if servers.is_empty() {
                None
            } else {
                Some(servers)
            }
        },
    }
}

fn run_login_command(token: Option<String>) -> Result<()> {
    use tokio::runtime::Runtime;

    let rt = Runtime::new()?;
    rt.block_on(async {
        match token {
            Some(token) => auth::login_with_token(&token).await,
            None => auth::login().await,
        }
    })
}

fn run_logout_command() -> Result<()> {
    auth::logout()
}

fn run_whoami_command() -> Result<()> {
    auth::whoami()
}

fn run_delete_data_command() -> Result<()> {
    use colored::Colorize;
    use std::io::{self, Write};
    use tokio::runtime::Runtime;

    let auth_token = auth::resolve_api_token().ok_or_else(|| {
        anyhow::anyhow!("Not logged in. Run `tokens login` or set TOKENS_API_TOKEN.")
    })?;

    println!("\n{}", "  ⚠ Delete all submitted usage data".red().bold());
    println!("{}", "  This will permanently remove:".bright_black());
    println!("{}", "    • Leaderboard entries".bright_black());
    println!("{}", "    • Public profile stats".bright_black());
    println!("{}", "    • Daily usage history".bright_black());
    println!(
        "{}",
        "  Your account and API tokens will stay active.\n".bright_black()
    );

    print!(
        "{}",
        "  Are you sure you want to delete all submitted data? (y/N): ".white()
    );
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" {
        println!("{}", "  Cancelled.".bright_black());
        return Ok(());
    }

    print!(
        "{}",
        "  This cannot be undone. You will lose all historical token/cost data. Continue? (y/N): "
            .white()
    );
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" {
        println!("{}", "  Cancelled.".bright_black());
        return Ok(());
    }

    print!("{}", "  Type \"delete my data\" to confirm: ".white());
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "delete my data" {
        println!("{}", "  Confirmation failed. Cancelled.".bright_black());
        return Ok(());
    }

    println!("\n{}", "  Deleting submitted data...".bright_black());

    let api_url = auth::get_api_base_url();
    let rt = Runtime::new()?;

    let response = rt.block_on(async {
        reqwest::Client::new()
            .delete(format!("{}/api/settings/submitted-data", api_url))
            .header("Authorization", format!("Bearer {}", auth_token.token))
            .send()
            .await
    });

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body: serde_json::Value =
                rt.block_on(async { resp.json().await }).unwrap_or_default();

            match interpret_delete_submitted_data_response(status, &body)? {
                DeleteSubmittedDataOutcome::Deleted(count) => {
                    println!(
                        "{}",
                        format!(
                            "  ✓ Deleted {} submission(s). Leaderboard and profile will refresh shortly.",
                            count
                        )
                        .green()
                    );
                }
                DeleteSubmittedDataOutcome::NotFound => {
                    println!("{}", "  No submitted data found for this account.".yellow());
                }
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Request failed: {}", e));
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum DeleteSubmittedDataOutcome {
    Deleted(i64),
    NotFound,
}

fn interpret_delete_submitted_data_response(
    status: reqwest::StatusCode,
    body: &serde_json::Value,
) -> Result<DeleteSubmittedDataOutcome> {
    if status.is_success() {
        let deleted = body
            .get("deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let count = body
            .get("deletedSubmissions")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if deleted {
            Ok(DeleteSubmittedDataOutcome::Deleted(count))
        } else {
            Ok(DeleteSubmittedDataOutcome::NotFound)
        }
    } else {
        let err = body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error");
        Err(anyhow::anyhow!("Failed ({}): {}", status, err))
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StarCache {
    #[serde(default)]
    username: String,
    #[serde(default)]
    has_starred: bool,
    #[serde(default)]
    checked_at: String,
}

fn star_cache_path() -> Option<PathBuf> {
    Some(crate::paths::get_config_dir().join("star-cache.json"))
}

fn legacy_macos_star_cache_path() -> Option<PathBuf> {
    crate::paths::legacy_macos_config_dir().map(|d| d.join("star-cache.json"))
}

fn load_star_cache(username: &str) -> Option<StarCache> {
    // Read the canonical path first; on macOS, fall back once to the
    // pre-#468 location under `~/Library/Application Support/tokens/`
    // so existing users don't get re-prompted to star the repo just
    // because their previous cache lives at the legacy path. The legacy
    // read is suppressed when `TOKENS_CONFIG_DIR` is set so isolated
    // profiles stay hermetic.
    let primary = star_cache_path().and_then(|path| std::fs::read_to_string(path).ok());
    let content = primary.or_else(|| {
        legacy_macos_star_cache_path().and_then(|legacy| std::fs::read_to_string(legacy).ok())
    })?;
    let cache: StarCache = serde_json::from_str(&content).ok()?;
    // Must match username and have hasStarred=true
    if cache.username != username || !cache.has_starred {
        return None;
    }
    Some(cache)
}

fn save_star_cache(username: &str, has_starred: bool) {
    // Only cache positive confirmations (matching v1 behavior)
    if !has_starred {
        return;
    }
    let Some(path) = star_cache_path() else {
        return;
    };
    let now = chrono::Utc::now().to_rfc3339();
    let cache = StarCache {
        username: username.to_string(),
        has_starred,
        checked_at: now,
    };
    if let Ok(content) = serde_json::to_string_pretty(&cache) {
        if let Some(dir) = path.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return;
            }
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let tmp_filename = format!(".star-cache.{}.{:x}.tmp", std::process::id(), nanos);
            let tmp_path = dir.join(tmp_filename);

            let write_result = (|| -> std::io::Result<()> {
                use std::io::Write;
                let mut file = std::fs::File::create(&tmp_path)?;
                file.write_all(content.as_bytes())?;
                file.sync_all()?;
                tokens_core::fs_atomic::replace_file(&tmp_path, &path)
            })();

            if write_result.is_err() {
                let _ = std::fs::remove_file(&tmp_path);
            }
        }
    }
}

fn prompt_star_repo(username: &str) -> Result<()> {
    use colored::Colorize;
    use std::io::{self, Write};
    use std::process::Command;

    // Check local cache first (avoids network call)
    if load_star_cache(username).is_some() {
        return Ok(());
    }

    // Check if gh CLI is available
    let gh_available = Command::new("gh")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);

    if !gh_available {
        return Ok(());
    }

    // Check if user has already starred via gh API
    // Returns exit 0 (HTTP 204) if starred, non-zero (HTTP 404) if not
    let already_starred = Command::new("gh")
        .args(["api", "/user/starred/missuo/tokens"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if already_starred {
        save_star_cache(username, true);
        return Ok(());
    }

    println!();
    println!("{}", "  Help us grow! \u{2b50}".cyan());
    println!(
        "{}",
        "  Starring tokens helps others discover the project.".bright_black()
    );
    println!(
        "  {}\n",
        osc8_link("https://github.com/missuo/tokens").bright_black()
    );
    print!(
        "{}",
        "  \u{2b50} Would you like to star Tokens? (Y/n): ".white()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_lowercase();
    if answer == "n" || answer == "no" {
        // Decline: don't cache (will re-prompt next time, matching v1)
        println!();
        return Ok(());
    }

    // Star via gh API (gh repo star is not a valid command)
    let status = Command::new("gh")
        .args([
            "api",
            "--silent",
            "--method",
            "PUT",
            "/user/starred/missuo/tokens",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => {
            println!(
                "{}",
                "  \u{2713} Starred! Thank you for your support.\n".green()
            );
            save_star_cache(username, true);
        }
        _ => {
            println!(
                "{}",
                "  Failed to star via gh CLI. Continuing to submit...\n".yellow()
            );
        }
    }

    Ok(())
}

/// Import a third-party aggregate export (currently clawdboard) and emit it as
/// standard tokens JSON — the same shape `tokens graph` produces.
///
/// This deliberately does NOT upload: backfilled aggregates cannot be verified
/// the way locally-scanned sessions are, so submitting them requires
/// server-side support for tagging backfilled data distinctly from live CLI
/// usage. See https://github.com/junhoyeo/tokscale/issues/888.
fn run_import_command(
    file: String,
    format: String,
    output: Option<String>,
    dry_run: bool,
) -> Result<()> {
    use colored::Colorize;

    let fmt = format.trim().to_lowercase();
    if !commands::import::SUPPORTED_FORMATS.contains(&fmt.as_str()) {
        return Err(anyhow::anyhow!(
            "Unsupported import format '{}'. Supported: {}",
            format,
            commands::import::SUPPORTED_FORMATS.join(", ")
        ));
    }

    // All human-readable banners/summaries/warnings go to stderr so stdout
    // stays pure JSON when no --output path is given (matching `tokens
    // graph`'s behavior) — e.g. `tokens import export.json > out.json`
    // must produce a valid JSON file.
    eprintln!("\n  {}\n", "Tokens - Import Usage Data".cyan());

    let contents = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", file, e))?;
    let outcome = commands::import::parse_export(&fmt, &contents)?;
    let graph = &outcome.graph;

    eprintln!("{}", "  Imported data:".white());
    eprintln!(
        "{}",
        format!(
            "    Date range: {} to {}",
            graph.meta.date_range_start, graph.meta.date_range_end
        )
        .bright_black()
    );
    eprintln!(
        "{}",
        format!("    Active days: {}", graph.summary.active_days).bright_black()
    );
    eprintln!(
        "{}",
        format!(
            "    Total tokens: {}",
            format_tokens_with_commas(graph.summary.total_tokens)
        )
        .bright_black()
    );
    eprintln!(
        "{}",
        format!(
            "    Total cost: {}",
            format_currency(graph.summary.total_cost)
        )
        .bright_black()
    );
    if !graph.summary.clients.is_empty() {
        eprintln!(
            "{}",
            format!("    Clients: {}", graph.summary.clients.join(", ")).bright_black()
        );
    }
    eprintln!(
        "{}",
        format!("    Models: {}", graph.summary.models.len()).bright_black()
    );

    if !outcome.unknown_clients.is_empty() {
        eprintln!(
            "\n  {}",
            format!(
                "Warning: unrecognized client id(s): {}. The leaderboard only \
                 accepts known clients, so these would be rejected on submit.",
                outcome.unknown_clients.join(", ")
            )
            .yellow()
        );
    }

    if outcome.negative_values_clamped > 0 {
        eprintln!(
            "{}",
            format!(
                "\n  Warning: {} negative token/cost value(s) in the export were clamped to \
                 zero.",
                outcome.negative_values_clamped
            )
            .yellow()
        );
    }

    if outcome.suspect_cost_rows > 0 {
        eprintln!(
            "{}",
            format!(
                "\n  Warning: {} modelBreakdown row(s) have cost > 0 but all token fields are \
                 0. The server rejects submissions shaped like this (\"Cost submitted without \
                 tokens\"), so these rows would be rejected if ever uploaded.",
                outcome.suspect_cost_rows
            )
            .yellow()
        );
    }

    if outcome.future_dated_rows > 0 {
        eprintln!(
            "{}",
            format!(
                "\n  Warning: {} row(s) are dated in the future. The submit endpoint rejects \
                 dates too far ahead, so these rows would be rejected if ever uploaded.",
                outcome.future_dated_rows
            )
            .yellow()
        );
    }

    if outcome.unparseable_cost_rows > 0 {
        eprintln!(
            "{}",
            format!(
                "\n  Warning: {} totalCost value(s) in the export could not be parsed and were \
                 treated as 0.",
                outcome.unparseable_cost_rows
            )
            .yellow()
        );
    }

    if outcome.non_finite_cost_rows > 0 {
        eprintln!(
            "{}",
            format!(
                "\n  Warning: {} cost value(s) in the export were non-finite (NaN/Infinity) \
                 and were sanitized to 0.",
                outcome.non_finite_cost_rows
            )
            .yellow()
        );
    }

    if outcome.multi_model_fallback_rows > 0 {
        eprintln!(
            "{}",
            format!(
                "\n  Warning: {} row(s) had no per-model breakdown and multiple models used; \
                 all usage in those rows was attributed to the first model only.",
                outcome.multi_model_fallback_rows
            )
            .yellow()
        );
    }

    for warning in &outcome.breakdown_reconciliation_warnings {
        eprintln!("{}", format!("\n  Warning: {}", warning).yellow());
    }

    if dry_run {
        eprintln!(
            "{}",
            "\n  Dry run - not emitting normalized JSON.\n".yellow()
        );
        return Ok(());
    }

    let mut payload = to_ts_token_contribution_data(graph, None, None);
    // The imported data has no MCP provenance of its own — it's derived
    // purely from a third-party clawdboard export. Reusing the graph/submit
    // converter would otherwise embed the *local* machine's configured MCP
    // server names, leaking unrelated metadata into a file that should only
    // reflect the export's contents.
    payload.mcp_servers = None;
    let json_output = serde_json::to_string_pretty(&payload)?;

    if let Some(output_path) = output {
        std::fs::write(&output_path, json_output)?;
        eprintln!(
            "{}",
            format!("\n  ✓ Normalized tokens data written to {}", output_path).green()
        );
    } else {
        println!("{}", json_output);
    }

    // Be explicit about the upload boundary so nobody assumes `import` puts
    // data on the leaderboard.
    eprintln!(
        "{}",
        "\n  Note: import only converts data to tokens's format; it does not \
         upload to the leaderboard.\n  Uploading backfilled history needs \
         server-side support for tagging it distinctly from live CLI usage.\n"
            .bright_black()
    );

    Ok(())
}

#[derive(serde::Deserialize)]
struct SubmitResponse {
    #[serde(rename = "submissionId")]
    submission_id: Option<String>,
    #[allow(dead_code)]
    username: Option<String>,
    metrics: Option<SubmitMetrics>,
    warnings: Option<Vec<String>>,
    error: Option<String>,
    details: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
struct SubmitMetrics {
    #[serde(rename = "totalTokens")]
    total_tokens: Option<i64>,
    #[serde(rename = "totalCost")]
    total_cost: Option<f64>,
    #[serde(rename = "activeDays")]
    active_days: Option<i32>,
    #[allow(dead_code)]
    sources: Option<Vec<String>>,
}

/// A client row dropped from a submission because it carried cost without any
/// token attribution. See [`exclude_tokenless_cost_contributions`].
#[derive(Debug, Clone, PartialEq)]
struct ExcludedTokenlessRow {
    date: String,
    client: String,
    model_id: String,
    provider_id: String,
    cost: f64,
}

fn client_token_total(tokens: &tokens_core::TokenBreakdown) -> i64 {
    // TokenBreakdown::total() already saturating_adds its fields so a clamped
    // (i64::MAX) bucket from a corrupt source can't overflow this display fold.
    tokens.total()
}

/// Cursor's pre-2025-05 exports include `premium-tool-call` rows billed per
/// tool invocation with no token attribution. The server grandfathers these
/// (cost > 0, tokens = 0) rather than rejecting them, so the client must not
/// drop them either — otherwise that legitimate cost silently disappears from
/// the submission. Keep in sync with `CURSOR_LEGACY_TOKENLESS_MODELS` in
/// packages/frontend/src/lib/validation/submission.ts.
fn is_legacy_tokenless_cursor_row(client: &tokens_core::ClientContribution) -> bool {
    client.client == "cursor"
        && client.model_id == "premium-tool-call"
        && client_token_total(&client.tokens) == 0
}

fn is_aggregate_only_warp_row(client: &tokens_core::ClientContribution) -> bool {
    client.client == "warp"
        && client.model_id == "aggregate-requests"
        && client_token_total(&client.tokens) == 0
}

/// A row the server's "Cost submitted without tokens" sanity check would
/// reject: real cost with every token bucket at zero, excluding the Cursor
/// `premium-tool-call` carve-out above.
fn is_tokenless_costed_row(client: &tokens_core::ClientContribution) -> bool {
    (is_aggregate_only_warp_row(client) || client.cost > 0.0)
        && client_token_total(&client.tokens) == 0
        && !is_legacy_tokenless_cursor_row(client)
}

/// Drop client rows that report cost without any tokens so the submission
/// passes the server's cost-without-tokens validation instead of being
/// rejected wholesale.
///
/// Cursor's usage export lists historical request/On-Demand charges (e.g.
/// `auto`, `claude-3.5-sonnet`, `o3`) with empty token columns, and Warp/Oz
/// only exposes aggregate request/spend counters. The server rejects cost with
/// no tokens, and request counts must not be submitted as fabricated tokens, so
/// we exclude the offending rows here and report them to the user.
///
/// Excluded rows always carry zero tokens, so only cost/messages change; token
/// totals, breakdowns, and intensities are untouched. Summary and year rollups
/// are recomputed from the trimmed contributions.
fn exclude_tokenless_cost_contributions(
    graph_result: &mut tokens_core::GraphResult,
) -> Vec<ExcludedTokenlessRow> {
    let mut excluded: Vec<ExcludedTokenlessRow> = Vec::new();

    for day in graph_result.contributions.iter_mut() {
        let date = day.date.clone();
        let mut removed_cost = 0.0;
        let mut removed_messages: i32 = 0;

        day.clients.retain(|client| {
            if is_tokenless_costed_row(client) {
                excluded.push(ExcludedTokenlessRow {
                    date: date.clone(),
                    client: client.client.clone(),
                    model_id: client.model_id.clone(),
                    provider_id: client.provider_id.clone(),
                    cost: client.cost,
                });
                removed_cost += client.cost;
                removed_messages = removed_messages.saturating_add(client.messages);
                false
            } else {
                true
            }
        });

        if removed_cost > 0.0 || removed_messages > 0 {
            day.totals.cost = (day.totals.cost - removed_cost).max(0.0);
            day.totals.messages = day.totals.messages.saturating_sub(removed_messages).max(0);
        }
    }

    if !excluded.is_empty() {
        graph_result.summary = tokens_core::calculate_summary(&graph_result.contributions);
        graph_result.years = tokens_core::calculate_years(&graph_result.contributions);
    }

    excluded
}

/// Print the rows dropped by [`exclude_tokenless_cost_contributions`] so the
/// user can see exactly what was left out, capping the per-row detail so a long
/// history of legacy Cursor charges doesn't flood the terminal.
fn report_excluded_tokenless_rows(excluded: &[ExcludedTokenlessRow]) {
    use colored::Colorize;

    if excluded.is_empty() {
        return;
    }

    const MAX_DETAIL_ROWS: usize = 20;
    let total_cost: f64 = excluded.iter().map(|row| row.cost).sum();

    println!(
        "{}",
        format!(
            "  Excluded {} aggregate/cost-only row(s) with no token data:",
            excluded.len()
        )
        .yellow()
    );

    for row in excluded.iter().take(MAX_DETAIL_ROWS) {
        let provider = if row.provider_id.is_empty() {
            String::new()
        } else {
            format!(" (provider={})", row.provider_id)
        };
        println!(
            "{}",
            format!(
                "    - {}/{}{} on {}: ${:.4}",
                row.client, row.model_id, provider, row.date, row.cost
            )
            .bright_black()
        );
    }

    if excluded.len() > MAX_DETAIL_ROWS {
        println!(
            "{}",
            format!("    ... and {} more", excluded.len() - MAX_DETAIL_ROWS).bright_black()
        );
    }

    println!(
        "{}",
        format!(
            "    Excluded {} total; the rest is submitted.",
            format_currency(total_cost)
        )
        .bright_black()
    );
    println!();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubmitMode {
    Interactive,
    Autosubmit,
}

/// Long-running service loop: submit immediately, then every `interval` minutes.
///
/// Designed to be supervised by launchd (`brew services`) / systemd, which keep
/// it alive and start it at login/boot. There is no durable state to flush, so
/// the default SIGTERM disposition (terminate) is a perfectly clean shutdown —
/// the next start just submits again.
fn run_serve(interval_min: Option<u64>, clients: Option<Vec<String>>) -> Result<()> {
    use colored::Colorize;
    use std::time::Duration;

    // Resolve cadence: --interval flag > $TOKENS_SUBMIT_INTERVAL > 30 min (min 1).
    let interval_min = commands::status::resolve_serve_interval_minutes(interval_min);
    let interval = Duration::from_secs(interval_min * 60);

    // Fail fast if not authenticated so the supervisor surfaces a clear error
    // instead of spinning forever on a login prompt.
    if auth::resolve_api_token().is_none() {
        eprintln!("\n  {}", "Not logged in.".yellow());
        eprintln!(
            "{}",
            "  Run 'tokens login' before starting the service.\n".bright_black()
        );
        std::process::exit(1);
    }

    let jitter = serve_startup_jitter(interval);
    println!(
        "  {} submitting every {interval_min} min (SIGTERM/Ctrl-C to stop)",
        "tokens serve:".cyan()
    );

    loop {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        // Each submission runs in a short-lived child process rather than
        // in-process. A full scan parses every session into memory, and the
        // system allocator keeps those freed pages inside this long-lived
        // process instead of returning them to the OS — so RSS would plateau at
        // the worst cycle's peak and creep up with heap fragmentation. Letting a
        // child do the work and exit hands all of that memory back every cycle,
        // keeping the resident daemon tiny regardless of how many sessions exist.
        match run_submit_subprocess(clients.as_deref()) {
            Ok(true) => println!("  {} [{ts}] submit complete", "•".green()),
            Ok(false) => {
                eprintln!(
                    "  {} [{ts}] submit exited with a non-zero status",
                    "✗".yellow()
                )
            }
            // Never crash the daemon on a transient failure — log and retry next cycle.
            Err(error) => eprintln!("  {} [{ts}] submit failed: {error}", "✗".yellow()),
        }
        std::thread::sleep(interval + jitter);
    }
}

fn run_autosubmit_command(subcommand: commands::autosubmit::AutosubmitSubcommand) -> Result<()> {
    use commands::autosubmit::{AutosubmitRunDecision, AutosubmitSubcommand};

    match subcommand {
        AutosubmitSubcommand::Enable(args) => commands::autosubmit::enable(args),
        AutosubmitSubcommand::Status { json } => commands::autosubmit::status(json),
        AutosubmitSubcommand::Disable => commands::autosubmit::disable(),
        AutosubmitSubcommand::Run { force } => {
            let now_ms = chrono::Utc::now().timestamp_millis();
            let (settings, decision) = commands::autosubmit::load_run_config(force, now_ms)?;
            match decision {
                AutosubmitRunDecision::Disabled => {
                    println!("Autosubmit is disabled.");
                    return Ok(());
                }
                AutosubmitRunDecision::NotDue { next_run_at_ms } => {
                    println!(
                        "Autosubmit is not due yet. Next run: {}.",
                        commands::autosubmit::format_timestamp_ms(next_run_at_ms)
                    );
                    return Ok(());
                }
                AutosubmitRunDecision::Due => {}
            }

            let Some(_lock) = commands::autosubmit::try_acquire_run_lock()? else {
                println!("Autosubmit is already running.");
                return Ok(());
            };

            let (clients, since, until, year) = commands::autosubmit::submit_filters(&settings);
            match run_submit_command(
                clients,
                since,
                until,
                year,
                false,
                SubmitMode::Autosubmit,
                None,
            ) {
                Ok(()) => {
                    commands::autosubmit::record_run_success(
                        chrono::Utc::now().timestamp_millis(),
                    )?;
                    Ok(())
                }
                Err(err) => {
                    let message = err.to_string();
                    let _ = commands::autosubmit::record_run_error(&message);
                    Err(err)
                }
            }
        }
    }
}

/// Run one submission as a child `tokens submit` process and wait for it.
///
/// Returns `Ok(true)` on a clean (zero-exit) submission. Spawning a child —
/// instead of calling [`run_submit_command`] in-process — is what keeps the
/// long-lived `serve` daemon's memory flat: the child holds the entire
/// parsed-session working set, then exits and releases every page back to the
/// OS. The client filter is forwarded verbatim as canonical `--client` ids.
fn run_submit_subprocess(clients: Option<&[String]>) -> Result<bool> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("submit");
    if let Some(clients) = clients {
        for client in clients {
            cmd.arg("--client").arg(client);
        }
    }
    Ok(cmd.status()?.success())
}

/// A stable per-process delay in `0..=min(interval/10, 60s)` to stagger a fleet.
fn serve_startup_jitter(interval: std::time::Duration) -> std::time::Duration {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let max = std::cmp::min(interval.as_secs() / 10, 60);
    if max == 0 {
        return Duration::ZERO;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::from(d.subsec_nanos()))
        .unwrap_or(0);
    Duration::from_secs(nanos % (max + 1))
}

fn run_submit_command(
    clients: Option<Vec<String>>,
    since: Option<String>,
    until: Option<String>,
    year: Option<String>,
    dry_run: bool,
    mode: SubmitMode,
    replacement: Option<SubmitReplacementCoverage>,
) -> Result<()> {
    use colored::Colorize;
    use std::io::IsTerminal;
    use tokio::runtime::Runtime;
    use tokens_core::{generate_graph, GroupBy, ReportOptions};

    let auth_token = match auth::resolve_api_token() {
        Some(token) => token,
        None => {
            if mode == SubmitMode::Autosubmit {
                return Err(anyhow::anyhow!(
                    "Autosubmit requires login. Run `tokens login` or set TOKENS_API_TOKEN."
                ));
            }
            eprintln!("\n  {}", "Not logged in.".yellow());
            eprintln!(
                "{}",
                "  Run 'bunx tokens-cli@latest login' or set TOKENS_API_TOKEN.\n".bright_black()
            );
            std::process::exit(1);
        }
    };

    if mode == SubmitMode::Interactive
        && auth_token.source == auth::ApiTokenSource::StoredCredentials
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
    {
        if let Some(username) = auth_token.username.as_deref() {
            let _ = prompt_star_repo(username);
        }
    }

    println!("\n  {}\n", "Tokens - Submit Usage Data".cyan());

    // Persist the detected timezone on first submit so every later submission —
    // including ones made from another timezone while traveling — buckets usage
    // into the same calendar dates. See https://github.com/missuo/tokens/issues/15.
    timezone::ensure_pinned();

    let explicit_cursor_filter = client_filter_explicitly_requests_cursor(&clients);
    let explicit_warp_filter = client_filter_explicitly_requests_warp(&clients);
    let clients = clients.or_else(|| Some(default_submit_clients()));

    let include_cursor = clients
        .as_ref()
        .is_none_or(|s| s.iter().any(|src| src == "cursor"));
    let has_cursor_cache = has_cursor_usage_cache();
    if include_cursor && cursor::is_cursor_logged_in() {
        println!("{}", "  Syncing Cursor usage data...".bright_black());
        let rt_sync = Runtime::new()?;
        let sync_result = rt_sync.block_on(async { cursor::sync_cursor_cache().await });
        if sync_result.synced {
            println!(
                "{}",
                format!("  Cursor: {} usage events synced", sync_result.rows).bright_black()
            );
        } else if let Some(err) = sync_result.error {
            if has_cursor_cache {
                println!(
                    "{}",
                    format!("  Cursor sync failed; using cached data: {}", err).yellow()
                );
            }
        }
    }
    if explicit_cursor_filter || explicit_warp_filter {
        let cursor_setup_warnings = setup_warnings(&clients);
        emit_cursor_setup_warnings(&cursor_setup_warnings);
    }

    println!("{}", "  Scanning local session data...".bright_black());

    let rt = Runtime::new()?;
    let mut graph_result = rt
        .block_on(async {
            generate_graph(ReportOptions {
                home_dir: None,
                use_env_roots: true,
                clients,
                since,
                until,
                year,
                group_by: GroupBy::default(),
                scanner_settings: settings::load_scanner_settings(),
                // Submit path: never compute subagents — the submit payload must
                // be identical with or without this feature.
                include_subagents: false,
                include_work_time: false,
                today_only: false,
            })
            .await
        })
        .map_err(|e| anyhow::anyhow!(e))?;

    // Preserve local-calendar contributions here. The API validator owns the
    // UTC+ timezone buffer; client-side UTC capping silently drops current-day
    // usage for users east of UTC. See #318 and #360.
    // Drop cost-only rows the server would reject (Cursor historical exports
    // record per-request cost with empty token columns) and report what was
    // left out, so a single legacy charge can't block the whole submission.
    let excluded_rows = exclude_tokenless_cost_contributions(&mut graph_result);
    report_excluded_tokenless_rows(&excluded_rows);

    if let Some(replacement) = replacement.as_ref() {
        for client in &replacement.clients {
            let has_contribution = graph_result
                .contributions
                .iter()
                .any(|day| day.clients.iter().any(|entry| &entry.client == client));
            if !has_contribution {
                return Err(anyhow::anyhow!(
                    "--replace found no {} contributions in {} to {}; refusing an unanchored replacement",
                    client,
                    replacement.start,
                    replacement.end
                ));
            }
        }
    }

    println!("{}", "  Data to submit:".white());
    println!(
        "{}",
        format!(
            "    Date range: {} to {}",
            graph_result.meta.date_range_start, graph_result.meta.date_range_end,
        )
        .bright_black()
    );
    println!(
        "{}",
        format!("    Active days: {}", graph_result.summary.active_days).bright_black()
    );
    println!(
        "{}",
        format!(
            "    Total tokens: {}",
            format_tokens_with_commas(graph_result.summary.total_tokens)
        )
        .bright_black()
    );
    println!(
        "{}",
        format!(
            "    Total cost: {}",
            format_currency(graph_result.summary.total_cost)
        )
        .bright_black()
    );
    println!(
        "{}",
        format!("    Clients: {}", graph_result.summary.clients.join(", ")).bright_black()
    );
    println!(
        "{}",
        format!("    Models: {} models", graph_result.summary.models.len()).bright_black()
    );
    println!();

    if graph_result.summary.total_tokens == 0 {
        println!("{}", "  No usage data found to submit.\n".yellow());
        return Ok(());
    }

    if dry_run {
        println!("{}", "  Dry run - not submitting data.\n".yellow());
        return Ok(());
    }

    println!("{}", "  Submitting to server...".bright_black());

    let api_url = auth::get_api_base_url();

    let submit_device = device::resolve_submit_device()?;
    let submit_payload =
        to_ts_token_contribution_data(&graph_result, Some(&submit_device), replacement.as_ref());

    let response = rt.block_on(async {
        reqwest::Client::new()
            .post(format!("{}/api/submit", api_url))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", auth_token.token))
            .json(&submit_payload)
            .send()
            .await
    });

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body: SubmitResponse =
                rt.block_on(async { resp.json().await })
                    .unwrap_or_else(|_| SubmitResponse {
                        submission_id: None,
                        username: None,
                        metrics: None,
                        warnings: None,
                        error: Some(format!(
                            "Server returned {} with unparseable response",
                            status
                        )),
                        details: None,
                    });

            if !status.is_success() {
                let error = body
                    .error
                    .clone()
                    .unwrap_or_else(|| "Submission failed".to_string());
                eprintln!("\n  {}", format!("Error: {}", error).red());
                if let Some(details) = body.details {
                    for detail in details {
                        eprintln!("{}", format!("    - {}", detail).bright_black());
                    }
                }
                println!();
                if mode == SubmitMode::Autosubmit {
                    return Err(anyhow::anyhow!(error));
                }
                std::process::exit(1);
            }

            println!("\n  {}", "Successfully submitted!".green());
            println!();
            println!("{}", "  Summary:".white());
            if let Some(id) = body.submission_id {
                println!("{}", format!("    Submission ID: {}", id).bright_black());
            }
            if let Some(metrics) = &body.metrics {
                if let Some(tokens) = metrics.total_tokens {
                    println!(
                        "{}",
                        format!("    Total tokens: {}", format_tokens_with_commas(tokens))
                            .bright_black()
                    );
                }
                if let Some(cost) = metrics.total_cost {
                    println!(
                        "{}",
                        format!("    Total cost: {}", format_currency(cost)).bright_black()
                    );
                }
                if let Some(days) = metrics.active_days {
                    println!("{}", format!("    Active days: {}", days).bright_black());
                }
            }
            if let Some(username) = body
                .username
                .clone()
                .or_else(|| auth_token.username.clone())
            {
                println!();
                println!(
                    "{}",
                    osc8_link_with_text(
                        &format!("{}/u/{}", api_url, username),
                        &format!("  View your profile: {}/u/{}", api_url, username),
                    )
                    .cyan()
                );
                println!();
            }

            if let Some(warnings) = body.warnings {
                if !warnings.is_empty() {
                    println!("{}", "  Warnings:".yellow());
                    for warning in warnings {
                        println!("{}", format!("    - {}", warning).bright_black());
                    }
                    println!();
                }
            }
        }
        Err(err) => {
            eprintln!("\n  {}", "Error: Failed to connect to server.".red());
            eprintln!("{}\n", format!("  {}", err).bright_black());
            if mode == SubmitMode::Autosubmit {
                return Err(anyhow::anyhow!("Failed to connect to server: {err}"));
            }
            std::process::exit(1);
        }
    }

    Ok(())
}


fn run_cursor_command(subcommand: CursorSubcommand) -> Result<()> {
    match subcommand {
        CursorSubcommand::Login { name } => cursor::run_cursor_login(name),
        CursorSubcommand::Logout {
            name,
            all,
            purge_cache,
        } => cursor::run_cursor_logout(name, all, purge_cache),
        CursorSubcommand::Status { name } => cursor::run_cursor_status(name),
        CursorSubcommand::Accounts { json } => cursor::run_cursor_accounts(json),
        CursorSubcommand::Sync { json } => cursor::run_cursor_sync(json),
        CursorSubcommand::Switch { name } => cursor::run_cursor_switch(&name),
    }
}

fn run_codex_command(subcommand: CodexSubcommand) -> Result<()> {
    match subcommand {
        CodexSubcommand::Import { name } => commands::usage::codex::run_codex_import(name),
        CodexSubcommand::Accounts { json } => commands::usage::codex::run_codex_accounts(json),
        CodexSubcommand::Switch { name } => commands::usage::codex::run_codex_switch(&name),
        CodexSubcommand::Remove { name } => commands::usage::codex::run_codex_remove(&name),
        CodexSubcommand::Status { name, json } => {
            commands::usage::codex::run_codex_status(name, json)
        }
        CodexSubcommand::Activity { json } => commands::codex_activity::run(json),
    }
}

fn run_antigravity_command(subcommand: AntigravitySubcommand) -> Result<()> {
    match subcommand {
        AntigravitySubcommand::Sync => antigravity::run_antigravity_sync(),
        AntigravitySubcommand::Status { json } => antigravity::run_antigravity_status(json),
        AntigravitySubcommand::PurgeCache => antigravity::run_antigravity_purge_cache(),
    }
}

/// Parse `--variant` into a typed value.
///
/// Returns:
/// - `Ok(Some(v))` when a recognized value was provided
/// - `Ok(None)` when the flag was omitted entirely
/// - `Err` when an unrecognized value was provided
///
/// The earlier version returned `Option<_>` and merged the "unrecognized" and
/// "omitted" cases, which let callers silently fall through to "all variants"
/// when the user typed something like `--variant slo` — they got every variant
/// touched instead of an error.
fn parse_variant_arg(arg: Option<&str>) -> Result<Option<trae::auth::TraeVariant>> {
    match arg {
        Some("solo") => Ok(Some(trae::auth::TraeVariant::Solo)),
        Some("ide") => Ok(Some(trae::auth::TraeVariant::Ide)),
        Some(other) => anyhow::bail!("unknown variant: {other}, valid values: solo, ide"),
        None => Ok(None),
    }
}

fn run_trae_command(subcommand: TraeSubcommand) -> Result<()> {
    use colored::Colorize;
    let rt = tokio::runtime::Runtime::new()?;

    match subcommand {
        TraeSubcommand::Login { manual, variant } => {
            if manual {
                use std::io::{self, Write};
                // Default to international Solo when `--variant` is omitted.
                let selected =
                    parse_variant_arg(variant.as_deref())?.unwrap_or(trae::auth::TraeVariant::Solo);
                println!();
                println!("  {}", "Trae Manual Token Login".cyan());
                println!(
                    "  {}",
                    "Paste your JWT access token from the browser DevTools:".bright_black()
                );
                println!(
                    "  {}",
                    "1. Open https://www.trae.ai/account-setting#usage".bright_black()
                );
                println!(
                    "  {}",
                    "2. F12 → Network → filter 'query_user_usage' → copy Authorization value"
                        .bright_black()
                );
                print!("  Token: ");
                io::stdout().flush()?;
                let mut token = String::new();
                io::stdin().read_line(&mut token)?;
                let token = token.trim().to_string();
                if token.is_empty() {
                    anyhow::bail!("token must not be empty");
                }
                trae::auth::save_manual_token(selected, token, None)?;
                println!(
                    "\n  {}",
                    format!("Token saved for {}", selected.client_str()).green()
                );
            } else {
                let variants: Vec<trae::auth::TraeVariant> =
                    match parse_variant_arg(variant.as_deref())? {
                        Some(v) => vec![v],
                        None => trae::auth::all_variants().to_vec(),
                    };

                let mut any_success = false;
                for v in variants {
                    match rt.block_on(trae::auth::resolve_token(v)) {
                        Ok(_) => {
                            println!("  {} logged in (auto-detected)", v.client_str().green());
                            any_success = true;
                        }
                        Err(e) => {
                            println!("  {} auto-login failed: {}", v.client_str().yellow(), e);
                        }
                    }
                }
                if !any_success {
                    println!(
                        "  {}",
                        "No Trae credentials found. Use --manual to paste a token by hand."
                            .yellow()
                    );
                }
            }
            Ok(())
        }
        TraeSubcommand::Logout { variant } => {
            let variants: Vec<trae::auth::TraeVariant> =
                match parse_variant_arg(variant.as_deref())? {
                    Some(v) => vec![v],
                    None => trae::auth::all_variants().to_vec(),
                };
            for v in variants {
                trae::auth::logout(v)?;
                println!("  {} logged out", v.client_str().green());
            }
            Ok(())
        }
        TraeSubcommand::Status { json } => {
            let mut status = serde_json::Map::new();
            for v in trae::auth::all_variants() {
                let has = trae::auth::has_credentials(v);
                if json {
                    status.insert(v.client_str().to_string(), serde_json::Value::Bool(has));
                } else {
                    println!(
                        "  {}: {}",
                        v.client_str(),
                        if has {
                            "authenticated".green()
                        } else {
                            "not authenticated".yellow()
                        }
                    );
                }
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            }
            Ok(())
        }
        TraeSubcommand::Sync { since, include_aux } => {
            let days = since.unwrap_or(30);
            // Negative `days` would compute `now - (negative * 86400)` → a
            // future `start_time`, and zero collapses the query window to an
            // empty range. Reject both at the CLI boundary instead of
            // forwarding garbage to the sync layer.
            if days <= 0 {
                anyhow::bail!("--since must be a positive number of days (got {days})");
            }
            // Trae IDE and Trae Solo share account-level usage data, so we
            // always sync once using whichever credential source is available.
            let variants: Vec<trae::auth::TraeVariant> = trae::auth::all_variants()
                .into_iter()
                .filter(|v| trae::auth::has_credentials(*v))
                .collect();
            rt.block_on(trae::sync::run_trae_sync(&variants, days, include_aux))
        }
    }
}

fn run_warp_command(subcommand: WarpSubcommand) -> Result<()> {
    match subcommand {
        WarpSubcommand::Login { token, cookie } => warp::run_warp_login(token, cookie),
        WarpSubcommand::Logout { purge_cache } => warp::run_warp_logout(purge_cache),
        WarpSubcommand::Status { json } => warp::run_warp_status(json),
        WarpSubcommand::Sync { json } => warp::run_warp_sync(json),
    }
}

fn format_tokens_with_commas(n: i64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len + len / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(b as char);
    }
    result
}

struct CaptureCommandOutcome {
    exit_code: i32,
    timed_out: bool,
}

fn run_capture_command(
    command: &str,
    args: &[String],
    output_path: &Path,
    timeout: Duration,
) -> Result<CaptureCommandOutcome> {
    use std::io::{Read, Write};
    use std::process::Command;
    use std::thread;
    use std::time::Instant;

    let mut child = Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn '{}': {}", command, e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout from command"))?;

    let mut output_file = std::fs::File::create(output_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to create output file '{}': {}",
            output_path.display(),
            e
        )
    })?;

    let output_handle = thread::spawn(move || -> Result<()> {
        let mut reader = std::io::BufReader::new(stdout);
        let mut buffer = [0; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(n) => output_file
                    .write_all(&buffer[..n])
                    .map_err(|e| anyhow::anyhow!("Failed to write to output file: {}", e))?,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to read from subprocess stdout: {}",
                        e
                    ));
                }
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| anyhow::anyhow!("Failed to wait for subprocess: {}", e))?
        {
            break status;
        }

        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|e| anyhow::anyhow!("Failed to wait for timed-out subprocess: {}", e))?;
        }

        thread::sleep(Duration::from_millis(25));
    };

    let output_result = output_handle
        .join()
        .map_err(|_| anyhow::anyhow!("Subprocess stdout reader thread panicked"))?;
    if !timed_out {
        output_result?;
    }

    Ok(CaptureCommandOutcome {
        exit_code: status.code().unwrap_or(1),
        timed_out,
    })
}

fn run_headless_command(
    source: &str,
    args: Vec<String>,
    format: Option<String>,
    output: Option<String>,
    no_auto_flags: bool,
) -> Result<()> {
    use chrono::Utc;
    use uuid::Uuid;

    let source_lower = source.to_lowercase();
    if source_lower != "codex" {
        eprintln!("\n  Error: Unknown headless source '{}'.", source);
        eprintln!("  Currently only 'codex' is supported.\n");
        std::process::exit(1);
    }

    let resolved_format = match format {
        Some(f) if f == "json" || f == "jsonl" => f,
        Some(f) => {
            eprintln!("\n  Error: Invalid format '{}'. Use json or jsonl.\n", f);
            std::process::exit(1);
        }
        None => "jsonl".to_string(),
    };

    let mut final_args = args.clone();
    if !no_auto_flags && source_lower == "codex" && !final_args.contains(&"--json".to_string()) {
        final_args.push("--json".to_string());
    }

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    let headless_roots = get_headless_roots(&home_dir);

    let output_path = if let Some(custom_output) = output {
        let parent = Path::new(&custom_output)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        custom_output
    } else {
        let root = headless_roots
            .first()
            .cloned()
            .unwrap_or_else(|| home_dir.join(".config/tokens/headless"));
        let dir = root.join(&source_lower);
        std::fs::create_dir_all(&dir)?;

        let now = Utc::now();
        let timestamp = now.format("%Y-%m-%dT%H-%M-%S-%3fZ").to_string();
        let uuid_short = Uuid::new_v4()
            .to_string()
            .replace("-", "")
            .chars()
            .take(8)
            .collect::<String>();
        let filename = format!(
            "{}-{}-{}.{}",
            source_lower, timestamp, uuid_short, resolved_format
        );

        dir.join(filename).to_string_lossy().to_string()
    };

    let settings = settings::Settings::load();
    let timeout = settings.get_native_timeout();

    use colored::Colorize;
    println!("\n  {}", "Headless capture".cyan());
    println!("  {}", format!("source: {}", source_lower).bright_black());
    println!("  {}", format!("output: {}", output_path).bright_black());
    println!(
        "  {}",
        format!("timeout: {}s", timeout.as_secs()).bright_black()
    );
    println!();

    let outcome =
        run_capture_command(&source_lower, &final_args, Path::new(&output_path), timeout)?;

    if outcome.timed_out {
        eprintln!(
            "{}",
            format!("\n  Subprocess timed out after {}s", timeout.as_secs()).red()
        );
        eprintln!("{}", "  Partial output saved. Increase timeout with TOKENS_NATIVE_TIMEOUT_MS or settings.json".bright_black());
        println!();
        std::process::exit(124);
    }

    println!(
        "{}",
        format!("✓ Saved headless output to {}", output_path).green()
    );
    println!();

    if outcome.exit_code != 0 {
        std::process::exit(outcome.exit_code);
    }

    Ok(())
}

