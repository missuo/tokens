mod antigravity;
mod auth;
mod cloud;
mod commands;
mod cursor;
mod device;
mod hindsight;
mod i18n;
mod paths;
mod process_liveness;
mod settings;
mod timezone;
mod trae;
mod warp;

use anyhow::Result;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
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

    /// Terminal language: `en`, `zh-CN`, `ja`, or `auto` (the default, which
    /// follows `TOKENS_LANG`, then settings.json, then English). `--json`
    /// output is always English.
    #[arg(long, global = true, value_name = "LANG")]
    lang: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Login to Tokens (browser auth, or --token to save an existing token)")]
    Login {
        #[arg(long, help = "Save an existing Tokens API token without browser auth")]
        token: Option<String>,
        #[arg(
            long,
            help = "Make your profile private and print a read token for accessing it"
        )]
        private: bool,
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
        #[arg(help = "Source CLI ('codex' or 'mcode')")]
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
    #[command(about = "Hindsight memory backend integration commands")]
    Hindsight {
        #[command(subcommand)]
        subcommand: HindsightSubcommand,
    },
    #[command(about = "Delete all submitted usage data from the server")]
    DeleteSubmittedData,
    #[command(about = "Report usage from a cloud agent session (Claude Code cloud, Codex cloud)")]
    Cloud {
        #[command(subcommand)]
        subcommand: CloudSubcommand,
    },
}

#[derive(Subcommand)]
enum CloudSubcommand {
    #[command(
        about = "Submit this cloud session's usage; safe to run from a Stop/notify hook (never fails the hook)"
    )]
    Submit {
        #[arg(
            long,
            value_enum,
            help = "Cloud provider (auto-detected for Claude Code; or $TOKENS_CLOUD_PROVIDER)"
        )]
        provider: Option<cloud::CloudProvider>,
        #[arg(
            long,
            help = "Session id to key this session's device on (or $TOKENS_CLOUD_SESSION_ID); only its hash is sent"
        )]
        session_id: Option<String>,
        #[arg(
            long,
            value_name = "MINUTES",
            default_value_t = 0,
            help = "Optional throttle: skip if this session submitted less than MINUTES ago (default 0 = submit every time, so a session's last turns are never dropped)"
        )]
        min_interval: u64,
        #[arg(long, help = "Submit even if the last submit was recent")]
        force: bool,
        #[arg(long, help = "Exit non-zero on failure instead of always exiting 0")]
        strict: bool,
        #[arg(long, help = "Show what would be submitted without submitting")]
        dry_run: bool,
        #[arg(
            hide = true,
            help = "Hook payload JSON (Codex passes it as the last argument); only the session id is read"
        )]
        payload: Option<String>,
    },
    #[command(about = "Show the detected cloud provider and the device this session submits as")]
    Status {
        #[arg(long, value_enum)]
        provider: Option<cloud::CloudProvider>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
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
        #[arg(long, help = "Prompt for an access token instead of auto-detecting")]
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
enum HindsightSubcommand {
    #[command(about = "Sync Hindsight LLM request logs into the local ledger cache")]
    Sync {
        #[arg(
            long,
            default_value = "http://127.0.0.1:8888",
            help = "Hindsight API base URL"
        )]
        api: String,
        #[arg(long, default_value = "default", help = "Tenant identifier")]
        tenant: String,
        #[arg(
            long,
            help = "Bearer authentication token (or set HINDSIGHT_API_API_TOKEN)"
        )]
        token: Option<String>,
        #[arg(long, help = "Output as JSON")]
        json: bool,
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

/// Install the `tracing` subscriber so `tokens-core`'s `tracing::warn!` calls
/// are actually visible — notably `scanner.rs`'s heads-up that a scan path
/// escaped `$HOME`. Off by default (a report must stay quiet); opt in with
/// `TOKENS_LOG=warn`, `TOKENS_LOG=debug`, or any `EnvFilter` directive.
/// Logs go to stderr so `--json` output on stdout stays parseable.
fn init_tracing() {
    let Ok(filter) = std::env::var("TOKENS_LOG") else {
        return;
    };
    if filter.trim().is_empty() {
        return;
    }

    let Ok(env_filter) = tracing_subscriber::EnvFilter::try_new(&filter) else {
        eprintln!("{}", tr!("cli.tracing.invalidFilter", filter = filter));
        return;
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Read `--lang <value>` / `--lang=<value>` straight out of the process
/// arguments.
///
/// The language has to be known before clap parses, because `get_matches`
/// prints `--help` itself and that text only becomes translatable once
/// [`apply_localized_help`] has replaced it. Scanning is deliberately shallow:
/// the flag is global so its value is simply the next argument, and `--` ends
/// the part clap will still interpret as flags.
fn lang_flag_from_args() -> Option<String> {
    lang_flag_in(std::env::args_os().skip(1))
}

fn lang_flag_in(args: impl IntoIterator<Item = std::ffi::OsString>) -> Option<String> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let Some(arg) = arg.to_str() else {
            continue;
        };
        if arg == "--" {
            return None;
        }
        if arg == "--lang" {
            return args
                .next()
                .map(|value| value.to_string_lossy().into_owned());
        }
        if let Some(value) = arg.strip_prefix("--lang=") {
            return Some(value.to_string());
        }
    }
    None
}

/// Whether this run asked for `--json` anywhere before `--`.
///
/// JSON output is read by programs, so a run that asks for it is English from
/// the first byte: the document itself and every error or warning around it.
/// Checked on the raw arguments for the same reason as `--lang` — the language
/// has to be settled before anything prints.
fn json_requested_in(args: impl IntoIterator<Item = std::ffi::OsString>) -> bool {
    args.into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .take_while(|arg| arg != "--")
        .any(|arg| arg == "--json")
}

/// Replace clap's compile-time help text with the catalogue's, in place.
///
/// `#[command(about = ...)]` and `#[arg(help = ...)]` are frozen when the
/// derive runs, so the only way to make them follow `--lang` is to rewrite the
/// built [`clap::Command`] before it parses or prints. Keys are derived from
/// the tree position (`cli.<command>.<element>`), using `Command::get_name`
/// for subcommands and `Arg::get_id` for arguments; `value_name`, `hide` and
/// everything else are left alone.
///
/// clap hardcodes its section titles except for subcommands, so the usage line
/// is re-templated and every argument is pinned to a localized heading. The
/// templates reproduce clap's defaults exactly, which keeps an English run
/// byte-identical.
fn apply_localized_help(command: &mut clap::Command) {
    let mut localized = localized_texts(command.clone(), "");

    // clap only creates `--help`/`--version` while building, and they need a
    // heading too — otherwise they form their own untranslated "Options" group
    // ahead of the translated one. Text lookups run first so the generated
    // `help` subcommand never has to be keyed.
    localized.build();
    localized = localized_headings(localized);

    *command = localized;
}

/// Rewrite the authored text and the section titles on the declared tree.
fn localized_texts(mut command: clap::Command, path: &str) -> clap::Command {
    let about_key = if path.is_empty() {
        "cli.root.about".to_string()
    } else {
        format!("cli.{path}.about")
    };
    if let Some(about) = localized_text(&about_key) {
        command = command.about(about);
    }

    // This mirrors clap's `DEFAULT_TEMPLATE` byte for byte apart from the usage
    // title, so an English run renders exactly as it did before.
    command = command
        .help_template(usage_template())
        .subcommand_help_heading(t!("cli.heading.commands"));

    command = command.mut_args(|arg| {
        let help_key = if path.is_empty() {
            format!("cli.root.{}.help", arg.get_id())
        } else {
            format!("cli.{path}.{}.help", arg.get_id())
        };
        match localized_text(&help_key) {
            Some(help) => arg.help(help),
            None => arg,
        }
    });

    command.mut_subcommands(|subcommand| {
        let child_path = if path.is_empty() {
            subcommand.get_name().to_string()
        } else {
            format!("{path}.{}", subcommand.get_name())
        };
        localized_texts(subcommand, &child_path)
    })
}

/// clap's default help template with only the usage title swapped. Kept in one
/// place so the generated `help` subcommand gets the same treatment.
fn usage_template() -> String {
    format!(
        "{{before-help}}{{about-with-newline}}\n{} {{usage}}\n\n{{all-args}}{{after-help}}",
        t!("cli.heading.usage")
    )
}

/// Pin every built argument (the generated flags included) to a localized
/// heading. `mut_args` rewrites in place, unlike `mut_arg`, which appends and
/// would reorder positionals out from under `trailing_var_arg`.
fn localized_headings(mut command: clap::Command) -> clap::Command {
    let arguments_heading = t!("cli.heading.arguments");
    let options_heading = t!("cli.heading.options");
    let help_text = t!("cli.heading.helpAction");
    let version_text = t!("cli.heading.versionAction");
    let help_subcommand_about = t!("cli.heading.helpSubcommand");
    let mut positional_order = 0;

    command = command.mut_args(|arg| {
        let arg = if arg.get_index().is_some() {
            // clap's heading groups sort options by flag but flagless args by
            // id, so the declaration order is restored as a display order.
            let ordinal = positional_order;
            positional_order += 1;
            arg.help_heading(arguments_heading).display_order(ordinal)
        } else {
            arg.help_heading(options_heading)
        };

        match arg.get_id().as_str() {
            "help" => arg.help(help_text),
            "version" => arg.help(version_text),
            _ => arg,
        }
    });

    command.mut_subcommands(|subcommand| {
        // The `help` subcommand is generated during `build`, after the text
        // pass, so its own page is finished here.
        let subcommand = if subcommand.get_name() == "help" {
            subcommand
                .about(help_subcommand_about)
                .help_template(usage_template())
                .subcommand_help_heading(t!("cli.heading.commands"))
        } else {
            subcommand
        };
        localized_headings(subcommand)
    })
}

/// The catalogue entry for `key`, or `None` when it is absent so the clap
/// default stays in place.
fn localized_text(key: &str) -> Option<&'static str> {
    // `t` answers an absent key with a `debug_assert`, so a help-less argument
    // needs the miss caught rather than aborting the whole help page.
    std::panic::catch_unwind(|| crate::i18n::t(key))
        .ok()
        .filter(|text| *text != key)
}

fn main() -> Result<()> {
    init_tracing();

    // The language is resolved before clap runs so its generated help is
    // already translated, and from the raw arguments rather than the parsed
    // ones because parsing is exactly what prints.
    let settings = settings::Settings::load();
    let lang_flag = lang_flag_from_args();

    // Before anything prints. clap's own usage errors stay English; every
    // message this program writes from here on is in the resolved language.
    // An unparseable `--lang` falls through the chain so the error itself can
    // be printed in whichever language that chain lands on.
    // `--json` pins English regardless of `--lang`, `TOKENS_LANG` or the
    // setting: machine-read output (errors and warnings included) never moves.
    i18n::set_lang(if json_requested_in(std::env::args_os().skip(1)) {
        i18n::Lang::En
    } else {
        i18n::resolve(lang_flag.as_deref(), settings.language.as_deref())
    });

    if let Some(raw) = lang_flag.as_deref().filter(|raw| *raw != "auto") {
        if i18n::Lang::parse(raw).is_none() {
            use i18n::Lang;
            let options = [Lang::En, Lang::ZhCn, Lang::Ja].map(Lang::tag).join(", ");
            return Err(anyhow::anyhow!(tr!(
                "cli.root.unknownLang",
                value = raw,
                options = options
            )));
        }
    }

    // Build the command by hand instead of `Cli::parse()` so the help text can
    // be localized first: `get_matches` prints `--help`/usage errors itself.
    let mut command = Cli::command();
    apply_localized_help(&mut command);
    let cli = Cli::from_arg_matches(&command.clone().get_matches())?;

    // Normalize configured alias keys with the same prefix rules as scanned IDs.
    tokens_core::model_normalization::set_prefixes_to_strip(&settings.model_id_prefixes_to_strip);
    tokens_core::model_alias::set_global(&settings.model_aliases);

    // Install OpenCode's configured model display names alongside the aliases.
    // Presentation only — the model id used for pricing, caching, and submit is
    // untouched. An absent or unreadable OpenCode config is a strict no-op.
    tokens_core::opencode_model_name::set_global(tokens_core::opencode_model_name::load_for_home(
        None,
    ));

    match cli.command {
        Some(Commands::Login { token, private }) => run_login_command(token, private),
        Some(Commands::Logout) => run_logout_command(),
        Some(Commands::Whoami) => run_whoami_command(),
        Some(Commands::Status { json }) => commands::status::run(json),
        Some(Commands::Import {
            file,
            format,
            output,
            dry_run,
        }) => run_import_command(file, format, output, dry_run),
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
                None,
            )
        }
        Some(Commands::Serve { clients, interval }) => {
            let clients = build_client_filter_with_defaults(clients, &[]);
            run_serve(interval, clients)
        }
        Some(Commands::Autosubmit { subcommand }) => run_autosubmit_command(subcommand),
        Some(Commands::Headless {
            source,
            args,
            format,
            output,
            no_auto_flags,
        }) => run_headless_command(&source, args, format, output, no_auto_flags),
        Some(Commands::Cursor { subcommand }) => run_cursor_command(subcommand),
        Some(Commands::Antigravity { subcommand }) => run_antigravity_command(subcommand),
        Some(Commands::Codex { subcommand }) => run_codex_command(subcommand),
        Some(Commands::Trae { subcommand }) => run_trae_command(subcommand),
        Some(Commands::Warp { subcommand }) => run_warp_command(subcommand),
        Some(Commands::Hindsight { subcommand }) => run_hindsight_command(subcommand),
        Some(Commands::DeleteSubmittedData) => run_delete_data_command(),
        Some(Commands::Cloud { subcommand }) => run_cloud_command(subcommand),
        None => {
            // The dashboard and the report commands are gone, so a bare
            // `tokens` prints usage rather than rendering anything.
            command.print_help()?;
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
    Reasonix,
    Freebuff,
    Fx,
    Senpi,
    #[value(alias = "auggie")]
    Augment,
    Kimchi,
    #[value(name = "prime-agent")]
    PrimeAgent,
    #[value(name = "cherrystudio")]
    CherryStudio,
    Dsh,
    Mcode,
    Omp,
    #[value(name = "lmstudio")]
    LmStudio,
    Unsloth,
    Hindsight,
    #[value(name = "craft-agent")]
    CraftAgent,
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
            Self::Reasonix => "reasonix",
            Self::Freebuff => "freebuff",
            Self::Fx => "fx",
            Self::Senpi => "senpi",
            Self::Augment => "augment",
            Self::Kimchi => "kimchi",
            Self::PrimeAgent => "prime-agent",
            Self::CherryStudio => "cherrystudio",
            Self::Dsh => "dsh",
            Self::Mcode => "mcode",
            Self::Omp => "omp",
            Self::LmStudio => "lmstudio",
            Self::Unsloth => "unsloth",
            Self::Hindsight => "hindsight",
            Self::CraftAgent => "craft-agent",
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
            Self::Reasonix => Some(ClientId::Reasonix),
            Self::Freebuff => Some(ClientId::Freebuff),
            Self::Fx => Some(ClientId::Fx),
            Self::Senpi => Some(ClientId::Senpi),
            Self::Augment => Some(ClientId::Augment),
            Self::Kimchi => Some(ClientId::Kimchi),
            Self::PrimeAgent => Some(ClientId::PrimeAgent),
            Self::CherryStudio => Some(ClientId::CherryStudio),
            Self::Dsh => Some(ClientId::Dsh),
            Self::Mcode => Some(ClientId::Mcode),
            Self::Omp => Some(ClientId::Omp),
            Self::LmStudio => Some(ClientId::LmStudio),
            Self::Unsloth => Some(ClientId::Unsloth),
            Self::Hindsight => Some(ClientId::Hindsight),
            Self::CraftAgent => Some(ClientId::CraftAgent),
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
            ClientId::Reasonix => Self::Reasonix,
            ClientId::Freebuff => Self::Freebuff,
            ClientId::Fx => Self::Fx,
            ClientId::Senpi => Self::Senpi,
            ClientId::Augment => Self::Augment,
            ClientId::Kimchi => Self::Kimchi,
            ClientId::PrimeAgent => Self::PrimeAgent,
            ClientId::CherryStudio => Self::CherryStudio,
            ClientId::Dsh => Self::Dsh,
            ClientId::Mcode => Self::Mcode,
            ClientId::Omp => Self::Omp,
            ClientId::LmStudio => Self::LmStudio,
            ClientId::Unsloth => Self::Unsloth,
            ClientId::Hindsight => Self::Hindsight,
            ClientId::CraftAgent => Self::CraftAgent,
        }
    }

    /// Parse a canonical lowercase identifier (the same form
    /// `as_filter_str` returns) into a `ClientFilter`. Returns `None` for
    /// any unknown id so callers can drop unrecognized settings entries
    /// without erroring.
    pub fn from_filter_str(s: &str) -> Option<Self> {
        // Canonical ids match as_filter_str. A few product aliases map onto
        // the same ClientFilter (e.g. "auggie" -> Augment).
        if s == "auggie" {
            return Some(Self::Augment);
        }
        Self::value_variants()
            .iter()
            .copied()
            .find(|f| f.as_filter_str() == s)
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

fn client_filter_explicitly_requests_hindsight(clients: &Option<Vec<String>>) -> bool {
    clients
        .as_ref()
        .is_some_and(|sources| sources.iter().any(|source| source == "hindsight"))
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
        return vec![t!("cli.warning.cursorNoHome").to_string()];
    };
    if state.has_cache {
        return Vec::new();
    }

    let action = if state.has_credentials {
        t!("cli.warning.actionCursorSync")
    } else {
        t!("cli.warning.actionCursorLoginSync")
    };

    vec![tr!(
        "cli.warning.cursorNeedsCache",
        path = state.cache_glob,
        action = action
    )]
}

fn emit_cursor_setup_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }

    use colored::Colorize;
    for warning in warnings {
        eprintln!("{}", tr!("cli.warning", warning = warning).yellow());
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
        t!("cli.warning.actionWarpSync")
    } else {
        t!("cli.warning.actionWarpLoginSync")
    };

    vec![tr!(
        "cli.warning.warpNeedsCache",
        path = cache_glob,
        action = action
    )]
}

fn hindsight_setup_warnings(clients: &Option<Vec<String>>) -> Vec<String> {
    if !client_filter_explicitly_requests_hindsight(clients)
        || hindsight::has_hindsight_usage_cache_in_home(None)
    {
        return Vec::new();
    }

    let cache_glob = match std::env::var("HINDSIGHT_HOME") {
        Ok(val) if !val.trim().is_empty() => format!("{}/usage/*.jsonl", val.trim()),
        _ => "~/.hindsight/usage/*.jsonl".to_string(),
    };

    vec![tr!("cli.warning.hindsightNeedsCache", path = cache_glob)]
}

fn setup_warnings(clients: &Option<Vec<String>>) -> Vec<String> {
    let mut warnings = cursor_setup_warnings(clients);
    warnings.extend(warp_setup_warnings(clients));
    warnings.extend(hindsight_setup_warnings(clients));
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
    build_date_filter_for_date(date, timezone::current().today())
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
    // Empty explicitly means unknown; an omitted field identifies a legacy CLI.
    speed_usage: Vec<tokens_core::speed::SpeedUsage>,
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
        return Err(anyhow::anyhow!(t!("cli.submit.replaceOnlyExplicitBounds")));
    }

    let replacement_clients = clients
        .filter(|clients| !clients.is_empty())
        .map(<[String]>::to_vec)
        .ok_or_else(|| anyhow::anyhow!(t!("cli.submit.replaceNeedsClient")))?;
    let start = date
        .since
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!(t!("cli.submit.replaceNeedsSince")))?;
    let end = date
        .until
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!(t!("cli.submit.replaceNeedsUntil")))?;
    let start_date = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!(t!("cli.submit.replaceSinceFormat")))?;
    let end_date = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!(t!("cli.submit.replaceUntilFormat")))?;
    if start_date > end_date {
        return Err(anyhow::anyhow!(t!("cli.submit.replaceUntilAfterSince")));
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

/// Per-client parser revision sent with every submission. The server keeps a
/// day's stored value when a resubmit at the same revision reports fewer
/// tokens, so a revision is bumped only when a parser fix deliberately lowers
/// totals and the corrected figure should replace what is stored.
///
/// Codex 3: reasoning tokens are no longer counted on top of output tokens,
/// which already include them. Codex 4: a `codex exec --json` capture no
/// longer counts beside the run's rollout or once per copy. Codex rollouts are not cleaned up locally, so
/// the recount covers the whole history. Claude's and Grok's smaller
/// corrections are deliberately not bumped: both clients delete old session
/// files, and a bump would let a total that shrank because history vanished
/// overwrite the stored one.
fn submit_parser_revision(client: &str) -> u32 {
    if client == "codex" {
        4
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
                        speed_usage: s.speed_usage.clone(),
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

fn run_login_command(token: Option<String>, private: bool) -> Result<()> {
    use tokio::runtime::Runtime;

    let rt = Runtime::new()?;
    rt.block_on(async {
        match token {
            Some(token) => auth::login_with_token(&token, private).await,
            None => auth::login(private).await,
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

    let auth_token = auth::resolve_api_token()
        .ok_or_else(|| anyhow::anyhow!(t!("cli.delete-submitted-data.notLoggedIn")))?;

    println!("\n{}", t!("cli.delete-submitted-data.title").red().bold());
    println!("{}", t!("cli.delete-submitted-data.intro").bright_black());
    println!(
        "{}",
        t!("cli.delete-submitted-data.leaderboard").bright_black()
    );
    println!("{}", t!("cli.delete-submitted-data.profile").bright_black());
    println!("{}", t!("cli.delete-submitted-data.history").bright_black());
    println!(
        "{}",
        t!("cli.delete-submitted-data.accountStays").bright_black()
    );

    print!("{}", t!("cli.delete-submitted-data.confirmFirst").white());
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" {
        println!(
            "{}",
            t!("cli.delete-submitted-data.cancelled").bright_black()
        );
        return Ok(());
    }

    print!("{}", t!("cli.delete-submitted-data.confirmSecond").white());
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" {
        println!(
            "{}",
            t!("cli.delete-submitted-data.cancelled").bright_black()
        );
        return Ok(());
    }

    print!("{}", t!("cli.delete-submitted-data.typeToConfirm").white());
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "delete my data" {
        println!(
            "{}",
            t!("cli.delete-submitted-data.confirmFailed").bright_black()
        );
        return Ok(());
    }

    println!(
        "\n{}",
        t!("cli.delete-submitted-data.deleting").bright_black()
    );

    let api_url = auth::get_api_base_url();
    let rt = Runtime::new()?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = rt.block_on(async {
        client
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
                        tr!("cli.delete-submitted-data.deleted", count = count).green()
                    );
                }
                DeleteSubmittedDataOutcome::NotFound => {
                    println!("{}", t!("cli.delete-submitted-data.none").yellow());
                }
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!(tr!(
                "cli.delete-submitted-data.requestFailed",
                error = e
            )));
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
        Err(anyhow::anyhow!(tr!(
            "cli.delete-submitted-data.serverFailed",
            status = status,
            error = err
        )))
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
    println!("{}", t!("cli.star.heading").cyan());
    println!("{}", t!("cli.star.body").bright_black());
    println!(
        "  {}\n",
        osc8_link("https://github.com/missuo/tokens").bright_black()
    );
    print!("{}", t!("cli.star.prompt").white());
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
            println!("{}", t!("cli.star.starred").green());
            save_star_cache(username, true);
        }
        _ => {
            println!("{}", t!("cli.star.failed").yellow());
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
        return Err(anyhow::anyhow!(tr!(
            "cli.import.unsupportedFormat",
            format = format,
            supported = commands::import::SUPPORTED_FORMATS.join(", ")
        )));
    }

    // All human-readable banners/summaries/warnings go to stderr so stdout
    // stays pure JSON when no --output path is given (matching `tokens
    // graph`'s behavior) — e.g. `tokens import export.json > out.json`
    // must produce a valid JSON file.
    eprintln!("\n  {}\n", t!("cli.import.title").cyan());

    let contents = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!(tr!("cli.import.readFailed", file = file, error = e)))?;
    let outcome = commands::import::parse_export(&fmt, &contents)?;
    let graph = &outcome.graph;

    eprintln!("{}", t!("cli.import.importedData").white());
    eprintln!(
        "{}",
        tr!(
            "cli.import.dateRange",
            start = graph.meta.date_range_start,
            end = graph.meta.date_range_end
        )
        .bright_black()
    );
    eprintln!(
        "{}",
        tr!("cli.import.activeDays", days = graph.summary.active_days).bright_black()
    );
    eprintln!(
        "{}",
        tr!(
            "cli.import.totalTokens",
            tokens = format_tokens_with_commas(graph.summary.total_tokens)
        )
        .bright_black()
    );
    eprintln!(
        "{}",
        tr!(
            "cli.import.totalCost",
            cost = format_currency(graph.summary.total_cost)
        )
        .bright_black()
    );
    if !graph.summary.clients.is_empty() {
        eprintln!(
            "{}",
            tr!(
                "cli.import.clients",
                clients = graph.summary.clients.join(", ")
            )
            .bright_black()
        );
    }
    eprintln!(
        "{}",
        tr!("cli.import.models", count = graph.summary.models.len()).bright_black()
    );

    if outcome.agent_attributed_rows > 0 {
        eprintln!(
            "{}",
            tr!(
                "cli.import.clientAttribution",
                rows = outcome.agent_attributed_rows
            )
            .bright_black()
        );
    }

    if !outcome.unknown_clients.is_empty() {
        eprintln!(
            "\n  {}",
            tr!(
                "cli.import.unknownClients",
                clients = outcome.unknown_clients.join(", ")
            )
            .yellow()
        );
    }

    if outcome.negative_values_clamped > 0 {
        eprintln!(
            "{}",
            tr!(
                "cli.import.negativeValues",
                count = outcome.negative_values_clamped
            )
            .yellow()
        );
    }

    if outcome.suspect_cost_rows > 0 {
        eprintln!(
            "{}",
            tr!("cli.import.suspectCost", count = outcome.suspect_cost_rows).yellow()
        );
    }

    if outcome.future_dated_rows > 0 {
        eprintln!(
            "{}",
            tr!("cli.import.futureDated", count = outcome.future_dated_rows).yellow()
        );
    }

    if outcome.unparseable_cost_rows > 0 {
        eprintln!(
            "{}",
            tr!(
                "cli.import.unparseableCost",
                count = outcome.unparseable_cost_rows
            )
            .yellow()
        );
    }

    if outcome.non_finite_cost_rows > 0 {
        eprintln!(
            "{}",
            tr!(
                "cli.import.nonFiniteCost",
                count = outcome.non_finite_cost_rows
            )
            .yellow()
        );
    }

    if outcome.multi_model_fallback_rows > 0 {
        eprintln!(
            "{}",
            tr!(
                "cli.import.multiModelFallback",
                count = outcome.multi_model_fallback_rows
            )
            .yellow()
        );
    }

    for warning in &outcome.breakdown_reconciliation_warnings {
        eprintln!("{}", tr!("cli.warning", warning = warning).yellow());
    }

    if dry_run {
        eprintln!("{}", t!("cli.import.dryRun").yellow());
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
        eprintln!("{}", tr!("cli.import.written", path = output_path).green());
    } else {
        println!("{}", json_output);
    }

    // Be explicit about the upload boundary so nobody assumes `import` puts
    // data on the leaderboard.
    eprintln!("{}", t!("cli.import.note").bright_black());

    Ok(())
}

#[derive(serde::Deserialize)]
struct SubmitResponse {
    #[serde(rename = "submissionId")]
    submission_id: Option<String>,
    #[allow(dead_code)]
    username: Option<String>,
    #[serde(default)]
    metrics: Option<SubmitMetrics>,
    warnings: Option<Vec<String>>,
    error: Option<String>,
    details: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
struct SubmitMetrics {
    #[serde(rename = "totalTokens", default, deserialize_with = "de_lenient_i64")]
    total_tokens: Option<i64>,
    #[serde(rename = "totalCost", default, deserialize_with = "de_lenient_f64")]
    total_cost: Option<f64>,
    #[serde(rename = "activeDays")]
    active_days: Option<i32>,
    #[allow(dead_code)]
    sources: Option<Vec<String>>,
}

/// Accept a JSON number *or* a numeric string.
///
/// Postgres `bigint` reaches the API's JSON as a string (`"22301189081"`),
/// because the driver will not risk precision loss by coercing int8 to a JS
/// number. A strict `Option<i64>` therefore failed on that one field, which
/// failed the whole `SubmitResponse`, which left every field `None` — so the
/// summary printed empty *and* the server's warnings about preserved data were
/// silently discarded. Being lenient here keeps one field's wire type from
/// costing us the entire response.
fn de_lenient_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrString {
        Number(i64),
        String(String),
    }
    Ok(match Option::<NumberOrString>::deserialize(deserializer)? {
        Some(NumberOrString::Number(n)) => Some(n),
        Some(NumberOrString::String(s)) => s.trim().parse().ok(),
        None => None,
    })
}

/// Same leniency for `numeric`/`decimal` columns, which arrive as strings for
/// the same reason.
fn de_lenient_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrString {
        Number(f64),
        String(String),
    }
    Ok(match Option::<NumberOrString>::deserialize(deserializer)? {
        Some(NumberOrString::Number(n)) => Some(n),
        Some(NumberOrString::String(s)) => s.trim().parse().ok(),
        None => None,
    })
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
        tr!("cli.submit.excludedRows", count = excluded.len()).yellow()
    );

    for row in excluded.iter().take(MAX_DETAIL_ROWS) {
        let provider = if row.provider_id.is_empty() {
            String::new()
        } else {
            format!(" (provider={})", row.provider_id)
        };
        println!(
            "{}",
            tr!(
                "cli.submit.excludedRow",
                client = row.client,
                model = row.model_id,
                provider = provider,
                date = row.date,
                cost = format!("{:.4}", row.cost)
            )
            .bright_black()
        );
    }

    if excluded.len() > MAX_DETAIL_ROWS {
        println!(
            "{}",
            tr!(
                "cli.submit.excludedMore",
                count = excluded.len() - MAX_DETAIL_ROWS
            )
            .bright_black()
        );
    }

    println!(
        "{}",
        tr!(
            "cli.submit.excludedTotal",
            cost = format_currency(total_cost)
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
        eprintln!("\n  {}", t!("cli.serve.notLoggedIn").yellow());
        eprintln!("{}", t!("cli.serve.loginFirst").bright_black());
        std::process::exit(1);
    }

    let jitter = serve_startup_jitter(interval);
    println!(
        "{}",
        tr!(
            "cli.serve.started",
            label = "tokens serve:".cyan(),
            minutes = interval_min
        )
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
            Ok(true) => println!(
                "{}",
                tr!("cli.serve.submitComplete", mark = "•".green(), ts = ts)
            ),
            Ok(false) => {
                eprintln!(
                    "{}",
                    tr!("cli.serve.submitNonZero", mark = "✗".yellow(), ts = ts)
                )
            }
            // Never crash the daemon on a transient failure — log and retry next cycle.
            Err(error) => eprintln!(
                "{}",
                tr!(
                    "cli.serve.submitFailed",
                    mark = "✗".yellow(),
                    ts = ts,
                    error = error
                )
            ),
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
            // No-op unless Windows, enabled, and the task predates tokensw.exe.
            commands::autosubmit::self_heal_windows_task(&settings);
            match decision {
                AutosubmitRunDecision::Disabled => {
                    println!("{}", t!("cli.autosubmit.disabled"));
                    return Ok(());
                }
                AutosubmitRunDecision::NotDue { next_run_at_ms } => {
                    println!(
                        "{}",
                        tr!(
                            "cli.autosubmit.notDue",
                            next = commands::autosubmit::format_timestamp_ms(next_run_at_ms)
                        )
                    );
                    return Ok(());
                }
                AutosubmitRunDecision::Due => {}
            }

            let Some(_lock) = commands::autosubmit::try_acquire_run_lock()? else {
                println!("{}", t!("cli.autosubmit.alreadyRunning"));
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

fn run_cloud_command(subcommand: CloudSubcommand) -> Result<()> {
    match subcommand {
        CloudSubcommand::Status {
            provider,
            session_id,
            json,
        } => {
            let inputs = cloud::DetectInputs::from_process(provider, session_id, None);
            let detected = cloud::detect(&inputs);
            if json {
                let value = match &detected {
                    Ok(session) => serde_json::json!({
                        "cloud": true,
                        "provider": session.provider.as_str(),
                        "client": session.provider.client_id(),
                        "sessionIdSource": session.session_source.describe(),
                        "device": session.submit_device(),
                        "loggedIn": auth::resolve_api_token().is_some(),
                    }),
                    Err(err) => serde_json::json!({ "cloud": false, "reason": err.to_string() }),
                };
                println!("{}", serde_json::to_string_pretty(&value)?);
                return Ok(());
            }
            match detected {
                Ok(session) => {
                    let device = session.submit_device();
                    println!(
                        "{}",
                        tr!("cli.cloud.session", provider = session.provider.as_str())
                    );
                    println!(
                        "{}",
                        tr!("cli.cloud.client", client = session.provider.client_id())
                    );
                    println!(
                        "{}",
                        tr!(
                            "cli.cloud.sessionIdFrom",
                            source = session.session_source.describe()
                        )
                    );
                    println!("{}", tr!("cli.cloud.deviceId", id = device.id));
                    println!(
                        "{}",
                        tr!(
                            "cli.cloud.deviceName",
                            name = device.name.unwrap_or_default()
                        )
                    );
                    println!(
                        "{}",
                        tr!(
                            "cli.cloud.loggedIn",
                            state = if auth::resolve_api_token().is_some() {
                                t!("cli.cloud.yes")
                            } else {
                                t!("cli.cloud.no")
                            }
                        )
                    );
                }
                Err(err) => println!("{}", tr!("cli.cloud.notCloud", error = err)),
            }
            Ok(())
        }
        CloudSubcommand::Submit {
            provider,
            session_id,
            min_interval,
            force,
            strict,
            dry_run,
            payload,
        } => {
            let result =
                run_cloud_submit(provider, session_id, min_interval, force, dry_run, payload);
            match result {
                Ok(()) => Ok(()),
                // A hook that exits non-zero can block or clutter the agent's
                // turn, and a missed submit is caught up by the next one, so
                // failures are reported but not propagated unless asked.
                Err(err) if !strict => {
                    eprintln!("{}", tr!("cli.cloud.submitFailed", error = err));
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }
    }
}

fn run_cloud_submit(
    provider: Option<cloud::CloudProvider>,
    session_id: Option<String>,
    min_interval: u64,
    force: bool,
    dry_run: bool,
    payload: Option<String>,
) -> Result<()> {
    let payload = payload.or_else(cloud::read_stdin_payload);
    let inputs = cloud::DetectInputs::from_process(provider, session_id, payload);
    let session = cloud::detect(&inputs)?;
    let device = session.submit_device();

    let now_ms = chrono::Utc::now().timestamp_millis();
    if !force && !dry_run && cloud::throttled(&device.id, min_interval, now_ms) {
        println!("{}", tr!("cli.cloud.throttled", minutes = min_interval));
        return Ok(());
    }

    run_submit_command(
        Some(vec![session.provider.client_id().to_string()]),
        None,
        None,
        None,
        dry_run,
        SubmitMode::Autosubmit,
        None,
        Some(&session),
    )?;

    if !dry_run {
        cloud::record_submit(&device.id, now_ms)?;
    }
    Ok(())
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

#[allow(clippy::too_many_arguments)]
fn run_submit_command(
    clients: Option<Vec<String>>,
    since: Option<String>,
    until: Option<String>,
    year: Option<String>,
    dry_run: bool,
    mode: SubmitMode,
    replacement: Option<SubmitReplacementCoverage>,
    cloud_session: Option<&cloud::CloudSession>,
) -> Result<()> {
    use colored::Colorize;
    use std::io::IsTerminal;
    use tokens_core::{generate_graph, GroupBy, ReportOptions};
    use tokio::runtime::Runtime;

    let auth_token = match auth::resolve_api_token() {
        Some(token) => token,
        None => {
            if mode == SubmitMode::Autosubmit {
                return Err(anyhow::anyhow!(t!("cli.submit.autosubmitRequiresLogin")));
            }
            eprintln!("\n  {}", t!("cli.submit.notLoggedIn").yellow());
            eprintln!("{}", t!("cli.submit.loginOrEnv").bright_black());
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

    println!("\n  {}\n", t!("cli.submit.title").cyan());

    // Persist the detected timezone on first submit so every later submission —
    // including ones made from another timezone while traveling — buckets usage
    // into the same calendar dates. See https://github.com/missuo/tokens/issues/15.
    timezone::ensure_pinned();

    let explicit_cursor_filter = client_filter_explicitly_requests_cursor(&clients);
    let explicit_warp_filter = client_filter_explicitly_requests_warp(&clients);
    let explicit_hindsight_filter = client_filter_explicitly_requests_hindsight(&clients);
    let clients = clients.or_else(|| Some(default_submit_clients()));

    let include_cursor = clients
        .as_ref()
        .is_none_or(|s| s.iter().any(|src| src == "cursor"));
    let has_cursor_cache = has_cursor_usage_cache();
    if include_cursor && cursor::is_cursor_logged_in() {
        println!("{}", t!("cli.submit.syncingCursor").bright_black());
        let rt_sync = Runtime::new()?;
        let sync_result = rt_sync.block_on(async { cursor::sync_cursor_cache(false).await });
        if sync_result.synced {
            println!(
                "{}",
                tr!("cli.submit.cursorSynced", rows = sync_result.rows).bright_black()
            );
        } else if let Some(err) = sync_result.error {
            if has_cursor_cache {
                println!(
                    "{}",
                    tr!("cli.submit.cursorSyncFailedCached", error = err).yellow()
                );
            }
        }
    }

    let include_antigravity = clients
        .as_ref()
        .is_none_or(|s| s.iter().any(|src| src == "antigravity"));
    let has_antigravity_cache = antigravity::has_antigravity_cache();
    if include_antigravity {
        match antigravity::detect_antigravity_connections() {
            Ok(active_connections) if !active_connections.is_empty() => {
                println!("{}", t!("cli.submit.syncingAntigravity").bright_black());
                let sync_result = antigravity::sync_antigravity_cache_with_connections(
                    &active_connections,
                    false,
                );
                if sync_result.synced {
                    println!(
                        "{}",
                        tr!(
                            "cli.submit.antigravitySynced",
                            sessions = sync_result.sessions,
                            session_suffix = if sync_result.sessions == 1 { "" } else { "s" },
                            connections = sync_result.connections,
                            connection_suffix = if sync_result.connections == 1 {
                                ""
                            } else {
                                "s"
                            }
                        )
                        .bright_black()
                    );
                } else if sync_result.partial {
                    println!(
                        "{}",
                        tr!(
                            "cli.submit.antigravityPartial",
                            sessions = sync_result.sessions,
                            session_suffix = if sync_result.sessions == 1 { "" } else { "s" },
                            retained = sync_result.retained_sessions,
                            failed = sync_result.failed_sessions,
                            error = sync_result
                                .error
                                .as_deref()
                                .unwrap_or(t!("cli.submit.unknownSyncError"))
                        )
                        .yellow()
                    );
                } else if let Some(err) = sync_result.error {
                    if has_antigravity_cache {
                        println!(
                            "{}",
                            tr!("cli.submit.antigravitySyncFailedCached", error = err).yellow()
                        );
                    } else {
                        println!(
                            "{}",
                            tr!("cli.submit.antigravitySyncFailed", error = err).yellow()
                        );
                    }
                }
            }
            Ok(_) => {}
            Err(err) => {
                if has_antigravity_cache {
                    println!(
                        "{}",
                        tr!("cli.submit.antigravityDiscoveryFailedCached", error = err).yellow()
                    );
                } else {
                    println!(
                        "{}",
                        tr!("cli.submit.antigravityDiscoveryFailed", error = err).yellow()
                    );
                }
            }
        }
    }
    if explicit_cursor_filter || explicit_warp_filter || explicit_hindsight_filter {
        let cursor_setup_warnings = setup_warnings(&clients);
        emit_cursor_setup_warnings(&cursor_setup_warnings);
    }

    println!("{}", t!("cli.submit.scanning").bright_black());

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
                worktree_rollup: tokens_core::WorktreeRollup::default(),
                scanner_settings: settings::load_scanner_settings(),
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
    report_unpriced_submission_usage(&graph_result.unpriced_submission_usage);

    if let Some(replacement) = replacement.as_ref() {
        for client in &replacement.clients {
            let has_contribution = graph_result
                .contributions
                .iter()
                .any(|day| day.clients.iter().any(|entry| &entry.client == client));
            if !has_contribution {
                return Err(anyhow::anyhow!(tr!(
                    "cli.submit.replaceNoContributions",
                    client = client,
                    start = replacement.start,
                    end = replacement.end
                )));
            }
        }
    }

    println!("{}", t!("cli.submit.dataToSubmit").white());
    println!(
        "{}",
        tr!(
            "cli.submit.dateRange",
            start = graph_result.meta.date_range_start,
            end = graph_result.meta.date_range_end,
        )
        .bright_black()
    );
    println!(
        "{}",
        tr!(
            "cli.submit.activeDays",
            days = graph_result.summary.active_days
        )
        .bright_black()
    );
    println!(
        "{}",
        tr!(
            "cli.submit.totalTokens",
            tokens = format_tokens_with_commas(graph_result.summary.total_tokens)
        )
        .bright_black()
    );
    println!(
        "{}",
        tr!(
            "cli.submit.totalCost",
            cost = format_currency(graph_result.summary.total_cost)
        )
        .bright_black()
    );
    println!(
        "{}",
        tr!(
            "cli.submit.clients",
            clients = graph_result.summary.clients.join(", ")
        )
        .bright_black()
    );
    println!(
        "{}",
        tr!(
            "cli.submit.models",
            count = graph_result.summary.models.len()
        )
        .bright_black()
    );
    println!();

    if graph_result.summary.total_tokens == 0 {
        println!("{}", t!("cli.submit.noData").yellow());
        return Ok(());
    }

    if dry_run {
        println!("{}", t!("cli.submit.dryRun").yellow());
        return Ok(());
    }

    println!("{}", t!("cli.submit.submitting").bright_black());

    let api_url = auth::get_api_base_url();

    // A cloud session submits as a device keyed to the session, never the
    // container's device.json (see cloud.rs).
    let submit_device = match cloud_session {
        Some(session) => session.submit_device(),
        None => device::resolve_submit_device()?,
    };
    let mut submit_payload =
        to_ts_token_contribution_data(&graph_result, Some(&submit_device), replacement.as_ref());
    if cloud_session.is_some() {
        // The container's MCP config is the repository's, not the user's, and
        // the server would otherwise replace the account's list with it.
        submit_payload.mcp_servers = None;
    }

    // `tokens serve` drives this unattended, so the request must be bounded.
    // The overall budget is more generous than the 30s used elsewhere because a
    // submission body carries a full contribution history, not just metadata.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = rt.block_on(async {
        client
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
            let raw = rt.block_on(async { resp.text().await }).unwrap_or_default();
            // Keep the parse failure rather than swallowing it: a 2xx whose body
            // we cannot read still means we are showing the user nothing the
            // server said, including its warnings. That must be loud.
            let parse_error = match serde_json::from_str::<SubmitResponse>(&raw) {
                Ok(_) => None,
                Err(err) => Some(err.to_string()),
            };
            let body: SubmitResponse = serde_json::from_str(&raw).unwrap_or(SubmitResponse {
                submission_id: None,
                username: None,
                metrics: None,
                warnings: None,
                error: Some(tr!("cli.submit.unparseableResponse", status = status)),
                details: None,
            });

            if !status.is_success() {
                let error = body
                    .error
                    .clone()
                    .unwrap_or_else(|| t!("cli.submit.failed").to_string());
                eprintln!("\n  {}", tr!("cli.submit.error", error = error).red());
                if let Some(details) = body.details {
                    for detail in details {
                        eprintln!(
                            "{}",
                            tr!("cli.submit.detail", detail = detail).bright_black()
                        );
                    }
                }
                eprintln!();
                if mode == SubmitMode::Autosubmit {
                    return Err(anyhow::anyhow!(error));
                }
                std::process::exit(1);
            }

            println!("\n  {}", t!("cli.submit.success").green());
            println!();
            if let Some(err) = &parse_error {
                eprintln!("{}", tr!("cli.submit.unreadableNote", error = err).yellow());
                println!();
            }
            println!("{}", t!("cli.submit.summary").white());
            if let Some(id) = body.submission_id {
                println!("{}", tr!("cli.submit.submissionId", id = id).bright_black());
            }
            if let Some(metrics) = &body.metrics {
                if let Some(tokens) = metrics.total_tokens {
                    println!(
                        "{}",
                        tr!(
                            "cli.submit.totalTokens",
                            tokens = format_tokens_with_commas(tokens)
                        )
                        .bright_black()
                    );
                }
                if let Some(cost) = metrics.total_cost {
                    println!(
                        "{}",
                        tr!("cli.submit.totalCost", cost = format_currency(cost)).bright_black()
                    );
                }
                if let Some(days) = metrics.active_days {
                    println!(
                        "{}",
                        tr!("cli.submit.activeDays", days = days).bright_black()
                    );
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
                        &tr!(
                            "cli.submit.profileLink",
                            base_url = api_url,
                            username = username
                        ),
                    )
                    .cyan()
                );
                println!();
            }

            if let Some(warnings) = body.warnings {
                if !warnings.is_empty() {
                    println!("{}", t!("cli.submit.warningsHeading").yellow());
                    for warning in warnings {
                        println!(
                            "{}",
                            tr!("cli.submit.warning", warning = warning).bright_black()
                        );
                    }
                    println!();
                }
            }
        }
        Err(err) => {
            eprintln!("\n  {}", t!("cli.submit.connectFailed").red());
            eprintln!("{}\n", format!("  {}", err).bright_black());
            if mode == SubmitMode::Autosubmit {
                return Err(anyhow::anyhow!(tr!("cli.submit.connectError", error = err)));
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
        Some(other) => anyhow::bail!(tr!("cli.trae.unknownVariant", variant = other)),
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
                println!("  {}", t!("cli.trae.manualTitle").cyan());
                println!("  {}", t!("cli.trae.pastePrompt").bright_black());
                println!("  {}", t!("cli.trae.step1").bright_black());
                println!("  {}", t!("cli.trae.step2").bright_black());
                print!("{}", t!("cli.trae.tokenPrompt"));
                io::stdout().flush()?;
                let mut token = String::new();
                io::stdin().read_line(&mut token)?;
                let token = token.trim().to_string();
                if token.is_empty() {
                    anyhow::bail!(t!("cli.trae.emptyToken"));
                }
                trae::auth::save_manual_token(selected, token, None)?;
                println!(
                    "\n  {}",
                    tr!("cli.trae.tokenSaved", client = selected.client_str()).green()
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
                            println!(
                                "{}",
                                tr!("cli.trae.loggedIn", client = v.client_str().green())
                            );
                            any_success = true;
                        }
                        Err(e) => {
                            println!(
                                "{}",
                                tr!(
                                    "cli.trae.autoLoginFailed",
                                    client = v.client_str().yellow(),
                                    error = e
                                )
                            );
                        }
                    }
                }
                if !any_success {
                    println!("{}", t!("cli.trae.noCredentials").yellow());
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
                println!(
                    "{}",
                    tr!("cli.trae.loggedOut", client = v.client_str().green())
                );
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
                        "{}",
                        tr!(
                            "cli.trae.statusLine",
                            client = v.client_str(),
                            state = if has {
                                t!("cli.trae.authenticated").green()
                            } else {
                                t!("cli.trae.notAuthenticated").yellow()
                            }
                        )
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
                anyhow::bail!(tr!("cli.trae.sincePositive", days = days));
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

fn run_hindsight_command(subcommand: HindsightSubcommand) -> Result<()> {
    match subcommand {
        HindsightSubcommand::Sync {
            api,
            tenant,
            token,
            json,
        } => hindsight::run_hindsight_sync(hindsight::SyncHindsightOptions {
            api,
            tenant,
            token,
            json,
            home: None,
        }),
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

fn report_unpriced_submission_usage(unpriced: &[tokens_core::UnpricedSubmissionUsage]) {
    use colored::Colorize;

    if unpriced.is_empty() {
        return;
    }

    // One row per provider/model pair can run to dozens on a long proxy-model
    // history, so cap the detail and report the aggregate. Rank by tokens --
    // every row is $0.00, so tokens are what pricing a model would recover --
    // and still name the capped ids, because the hint below asks the user to
    // add pricing keyed by exactly these ids.
    const MAX_DETAIL_ROWS: usize = 20;
    const TAIL_IDS_PER_LINE: usize = 4;

    let mut ranked: Vec<&tokens_core::UnpricedSubmissionUsage> = unpriced.iter().collect();
    ranked.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| b.message_count.cmp(&a.message_count))
            .then_with(|| (&a.provider_id, &a.model_id).cmp(&(&b.provider_id, &b.model_id)))
    });

    for row in ranked.iter().take(MAX_DETAIL_ROWS) {
        println!(
            "{}",
            tr!(
                "cli.submit.unpricedRow",
                messages = row.message_count,
                provider = row.provider_id,
                model = row.model_id,
                tokens = format_tokens_with_commas(row.total_tokens),
                reason = row.reason,
            )
            .yellow()
        );
    }

    if ranked.len() > MAX_DETAIL_ROWS {
        let capped = &ranked[MAX_DETAIL_ROWS..];
        println!(
            "{}",
            tr!("cli.submit.unpricedMore", count = capped.len()).bright_black()
        );
        for chunk in capped.chunks(TAIL_IDS_PER_LINE) {
            let ids = chunk
                .iter()
                .map(|row| format!("{}/{}", row.provider_id, row.model_id))
                .collect::<Vec<_>>()
                .join(", ");
            println!("{}", format!("      {}", ids).bright_black());
        }
    }

    let total_messages: usize = unpriced
        .iter()
        .fold(0usize, |acc, row| acc.saturating_add(row.message_count));
    let total_tokens: i64 = unpriced
        .iter()
        .fold(0i64, |acc, row| acc.saturating_add(row.total_tokens));
    println!(
        "{}",
        tr!(
            "cli.submit.unpricedTotal",
            messages = total_messages,
            tokens = format_tokens_with_commas(total_tokens),
            providers = unpriced.len(),
        )
        .bright_black()
    );

    let pricing_path = crate::paths::get_config_dir().join("custom-pricing.json");
    println!(
        "{}",
        tr!("cli.submit.unpricedHint", path = pricing_path.display()).bright_black()
    );
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

/// How long the stdout pump gets to finish draining a killed child's pipe.
///
/// Bounded because a *descendant* of the child may still hold the write end, in
/// which case the pump never reaches EOF and an unbounded wait hangs the whole
/// timeout (#1049). Two seconds because the ordinary case -- the pipe closing
/// with the child -- only has to move at most one pipe buffer, so anything past
/// a few milliseconds already means a descendant is holding it open.
const STDOUT_DRAIN_GRACE: Duration = Duration::from_secs(2);

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
        .map_err(|e| {
            anyhow::anyhow!(tr!(
                "cli.headless.spawnFailed",
                command = command,
                error = e
            ))
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!(t!("cli.headless.captureStdoutFailed")))?;

    let mut output_file = std::fs::File::create(output_path).map_err(|e| {
        anyhow::anyhow!(tr!(
            "cli.headless.createOutputFailed",
            path = output_path.display(),
            error = e
        ))
    })?;

    // The pump reports completion over a channel rather than only through its
    // JoinHandle, so the timeout path below can wait for it with a bound.
    let (pump_done_tx, pump_done_rx) = std::sync::mpsc::channel::<Result<()>>();
    thread::spawn(move || {
        let result = (|| -> Result<()> {
            let mut reader = std::io::BufReader::new(stdout);
            let mut buffer = [0; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => return Ok(()),
                    Ok(n) => output_file.write_all(&buffer[..n]).map_err(|e| {
                        anyhow::anyhow!(tr!("cli.headless.writeOutputFailed", error = e))
                    })?,
                    Err(e) => {
                        return Err(anyhow::anyhow!(tr!(
                            "cli.headless.readStdoutFailed",
                            error = e
                        )));
                    }
                }
            }
        })();
        // A panic drops the sender instead, surfacing as RecvError below.
        let _ = pump_done_tx.send(result);
    });

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| anyhow::anyhow!(tr!("cli.headless.waitFailed", error = e)))?
        {
            break status;
        }

        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|e| anyhow::anyhow!(tr!("cli.headless.waitTimeoutFailed", error = e)))?;
        }

        thread::sleep(Duration::from_millis(25));
    };

    if timed_out {
        // Wait, but with a bound. An unbounded wait hangs whenever a descendant
        // holds the pipe open (#1049); no wait at all loses output, because the
        // caller prints "Partial output saved" and then calls process::exit,
        // which does not wait for threads -- so anything the child had already
        // written but the pump had not yet copied would be dropped.
        //
        // A drain error is deliberately ignored: the run already failed on the
        // timeout, and the partial file is best-effort by definition.
        let _ = pump_done_rx.recv_timeout(STDOUT_DRAIN_GRACE);
    } else {
        // The child exited on its own, but that does NOT mean the pipe is closed:
        // a descendant it spawned can still hold the write end, and then the pump
        // never reaches EOF. This branch has no deadline behind it -- `timed_out`
        // is false precisely because the deadline was never reached -- so an
        // unbounded wait here hangs forever with nothing to rescue it. That was
        // true of the original unconditional join too, and #1166 only bounded the
        // timeout branch, so it survived both.
        //
        // Bound it by whatever is left of the caller's own deadline, plus the same
        // drain grace. Total wall time therefore stays within the configured
        // timeout plus the grace, whichever path is taken.
        let remaining = deadline.saturating_duration_since(Instant::now());
        match pump_done_rx.recv_timeout(remaining + STDOUT_DRAIN_GRACE) {
            Ok(pump_result) => pump_result?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Report rather than silently truncate: the child succeeded, so
                // returning Ok here would present a capture file we cannot show
                // is complete.
                return Err(anyhow::anyhow!(tr!(
                    "cli.headless.stdoutHeldOpen",
                    command = command
                )));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(anyhow::anyhow!(t!("cli.headless.readerPanicked")));
            }
        }
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
    if source_lower != "codex" && source_lower != "mcode" {
        eprintln!("\n  {}", tr!("cli.headless.unknownSource", source = source));
        eprintln!("{}", t!("cli.headless.supportedSources"));
        std::process::exit(1);
    }

    let resolved_format = match format {
        Some(f) if f == "json" || f == "jsonl" => f,
        Some(f) => {
            eprintln!("\n  {}", tr!("cli.headless.invalidFormat", format = f));
            std::process::exit(1);
        }
        None => "jsonl".to_string(),
    };

    if source_lower == "mcode" && resolved_format != "jsonl" {
        eprintln!("\n  {}", t!("cli.headless.mcodeRequiresJsonl"));
        std::process::exit(1);
    }

    let final_args = prepare_headless_args(&source_lower, args, no_auto_flags)?;

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!(t!("cli.headless.homeFailed")))?;
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
    println!("\n  {}", t!("cli.headless.title").cyan());
    println!(
        "  {}",
        tr!("cli.headless.source", source = source_lower).bright_black()
    );
    println!(
        "  {}",
        tr!("cli.headless.output", path = output_path).bright_black()
    );
    println!(
        "  {}",
        tr!("cli.headless.timeout", seconds = timeout.as_secs()).bright_black()
    );
    println!();

    let outcome =
        run_capture_command(&source_lower, &final_args, Path::new(&output_path), timeout)?;

    if outcome.timed_out {
        eprintln!(
            "{}",
            tr!("cli.headless.timedOut", seconds = timeout.as_secs()).red()
        );
        eprintln!("{}", t!("cli.headless.partialSaved").bright_black());
        println!();
        std::process::exit(124);
    }

    println!("{}", tr!("cli.headless.saved", path = output_path).green());
    println!();

    if outcome.exit_code != 0 {
        std::process::exit(outcome.exit_code);
    }

    Ok(())
}

fn prepare_headless_args(
    source: &str,
    mut args: Vec<String>,
    no_auto_flags: bool,
) -> Result<Vec<String>> {
    if no_auto_flags {
        return Ok(args);
    }

    if source == "codex" {
        if !args.iter().any(|arg| arg == "--json") {
            args.push("--json".to_string());
        }
        return Ok(args);
    }

    let exec_index = args
        .iter()
        .position(|arg| arg == "exec")
        .ok_or_else(|| anyhow::anyhow!(t!("cli.headless.execRequired")))?;
    let has_output_format = args.iter().any(|arg| {
        arg == "--output-format"
            || arg.starts_with("--output-format=")
            || arg == "--format"
            || arg.starts_with("--format=")
    });
    if !has_output_format {
        args.splice(
            exec_index + 1..exec_index + 1,
            ["--output-format".to_string(), "stream-json".to_string()],
        );
    }
    Ok(args)
}

#[cfg(test)]
mod help_catalogues {
    use super::*;
    use serial_test::serial;
    use std::collections::HashMap;

    fn os_args(args: &[&str]) -> Vec<std::ffi::OsString> {
        args.iter().map(std::ffi::OsString::from).collect()
    }

    #[test]
    fn json_runs_are_detected_before_parsing() {
        assert!(json_requested_in(os_args(&["status", "--json"])));
        assert!(json_requested_in(os_args(&["--lang", "ja", "status", "--json"])));
        assert!(!json_requested_in(os_args(&["--lang", "ja", "status"])));
        // After `--` it belongs to a wrapped command, not to us.
        assert!(!json_requested_in(os_args(&["headless", "codex", "--", "--json"])));
    }

    #[test]
    fn lang_flag_is_read_from_raw_arguments() {
        assert_eq!(lang_flag_in(os_args(&["--lang", "ja", "status"])).as_deref(), Some("ja"));
        assert_eq!(lang_flag_in(os_args(&["status", "--lang=zh-CN"])).as_deref(), Some("zh-CN"));
        assert_eq!(lang_flag_in(os_args(&["status", "--", "--lang", "ja"])), None);
        assert_eq!(lang_flag_in(os_args(&["status"])), None);
    }

    fn catalogue(lang: &str) -> HashMap<String, String> {
        let text = match lang {
            "zh-CN" => include_str!("../locales/zh-CN/cli.json"),
            "ja" => include_str!("../locales/ja/cli.json"),
            _ => include_str!("../locales/en/cli.json"),
        };
        serde_json::from_str(text).expect("help catalogue is JSON")
    }

    /// `(key, original text)` for every clap element that carries help text.
    fn help_texts(command: &clap::Command, path: &str, out: &mut Vec<(String, String)>) {
        if let Some(about) = command.get_about() {
            let about_key = if path.is_empty() {
                "cli.root.about".to_string()
            } else {
                format!("cli.{path}.about")
            };
            out.push((about_key, about.to_string()));
        }
        for arg in command.get_arguments() {
            if let Some(help) = arg.get_help() {
                let help_key = if path.is_empty() {
                    format!("cli.root.{}.help", arg.get_id())
                } else {
                    format!("cli.{path}.{}.help", arg.get_id())
                };
                out.push((help_key, help.to_string()));
            }
        }

        for subcommand in command.get_subcommands() {
            let child_path = if path.is_empty() {
                subcommand.get_name().to_string()
            } else {
                format!("{path}.{}", subcommand.get_name())
            };
            help_texts(subcommand, &child_path, out);
        }
    }

    /// Every key the localizer probes, whether or not the element has text.
    fn probe_keys(command: &clap::Command, path: &str, out: &mut Vec<String>) {
        out.push(if path.is_empty() {
            "cli.root.about".to_string()
        } else {
            format!("cli.{path}.about")
        });
        for arg in command.get_arguments() {
            out.push(if path.is_empty() {
                format!("cli.root.{}.help", arg.get_id())
            } else {
                format!("cli.{path}.{}.help", arg.get_id())
            });
        }
        for subcommand in command.get_subcommands() {
            let child_path = if path.is_empty() {
                subcommand.get_name().to_string()
            } else {
                format!("{path}.{}", subcommand.get_name())
            };
            probe_keys(subcommand, &child_path, out);
        }
    }

    fn all_probe_keys() -> Vec<String> {
        let mut out = Vec::new();
        probe_keys(&Cli::command(), "", &mut out);
        out
    }

    fn all_texts() -> Vec<(String, String)> {
        let mut out = Vec::new();
        help_texts(&Cli::command(), "", &mut out);
        out
    }

    #[test]
    fn english_catalogue_matches_the_clap_tree() {
        let english = catalogue("en");
        let texts = all_texts();

        for (key, original) in &texts {
            assert_eq!(
                english.get(key).map(String::as_str),
                Some(original.as_str()),
                "`{key}` must keep today's English wording"
            );
        }

        // Every probed element must have an entry, even one whose English text
        // comes from the catalogue rather than the (unset) attribute.
        let probed = all_probe_keys();
        let mut missing: Vec<&String> = probed
            .iter()
            .filter(|key| !english.contains_key(*key))
            .collect();
        missing.sort();
        assert!(missing.is_empty(), "unmapped elements: {missing:?}");

        // Only clap-shaped entries (`*.about` / `*.help`) must still exist in
        // the tree; the catalogue also carries the terminal messages.
        let mut unmapped: Vec<&String> = english
            .keys()
            .filter(|key| key.ends_with(".about") || key.ends_with(".help"))
            .filter(|key| !probed.iter().any(|probed_key| probed_key == *key))
            .collect();
        unmapped.sort();
        assert!(unmapped.is_empty(), "stale entries: {unmapped:?}");
    }

    /// `cargo test dump_help_keys -- --nocapture` prints every key and its
    /// current English text, newest line last — the source for the catalogue.
    #[test]
    fn dump_help_keys() {
        for (key, text) in all_texts() {
            println!("{key}\t{text}");
        }
    }

    #[test]
    fn every_catalogue_carries_the_same_keys() {
        let english = catalogue("en");

        for lang in ["zh-CN", "ja"] {
            let other = catalogue(lang);
            let mut missing: Vec<&String> = english
                .keys()
                .filter(|key| !other.contains_key(*key))
                .collect();
            missing.sort();
            let mut extra: Vec<&String> = other
                .keys()
                .filter(|key| !english.contains_key(*key))
                .collect();
            extra.sort();
            assert!(missing.is_empty(), "{lang} is missing {missing:?}");
            assert!(extra.is_empty(), "{lang} has stray {extra:?}");
        }
    }

    #[test]
    fn translations_keep_the_english_placeholders() {
        fn placeholder(text: &str) -> Vec<&str> {
            let mut names: Vec<&str> = text
                .split('{')
                .skip(1)
                .filter_map(|rest| rest.split('}').next())
                .filter(|name| {
                    !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                })
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        }

        let english = catalogue("en");
        for lang in ["zh-CN", "ja"] {
            let other = catalogue(lang);
            for (key, text) in &other {
                for name in placeholder(text) {
                    assert!(
                        english[key].contains(&format!("{{{name}}}")),
                        "{lang} `{key}` uses {{{name}}}, absent from English"
                    );
                }
            }
        }
    }

    // Switching the global language races parallel tests that print output, so
    // this shares the serial lock the output-asserting tests already take.
    #[test]
    #[serial]
    fn apply_localized_help_replaces_every_text() {
        i18n::set_lang(i18n::Lang::Ja);

        let mut command = Cli::command();
        apply_localized_help(&mut command);

        let japanese = catalogue("ja");
        let mut localized = Vec::new();
        help_texts(&command, "", &mut localized);

        // The generated `help`/`version` flags and the `help` subcommand are
        // not in the catalogue, so only authored keys are compared one by one.
        for (key, text) in &localized {
            if let Some(expected) = japanese.get(key) {
                assert_eq!(text, expected, "`{key}` was not localized");
            }
        }

        assert_eq!(
            command.get_subcommand_help_heading(),
            Some(japanese["cli.heading.commands"].as_str())
        );
        let rendered = command.render_help().to_string();
        assert!(rendered.contains(&japanese["cli.heading.usage"]));
        assert!(rendered.contains(&japanese["cli.heading.helpAction"]));
        assert!(!rendered.contains("Usage:"));
        assert!(!rendered.contains("Print help"));

        let login = command
            .find_subcommand_mut("login")
            .expect("login subcommand");
        let token = login
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == "token")
            .expect("login --token");
        assert_eq!(
            token.get_help_heading(),
            Some(japanese["cli.heading.options"].as_str())
        );

        i18n::set_lang(i18n::Lang::En);
    }
}
