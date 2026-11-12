//! `tokens mcp-names` — the opt-in for uploading configured MCP server names.
//!
//! Off by default. When on, `tokens submit` reads the `mcpServers` keys (names
//! only) from the MCP configs of the clients that submission selects, and the
//! server shows them on the public profile. A submit with it off sends no
//! names, which clears whatever list an earlier submit stored.

use crate::{t, tr};
use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use std::path::Path;
use tokens_core::mcp::{mcp_config_locations, McpConfigLocation};

#[derive(Subcommand)]
pub enum McpNamesSubcommand {
    #[command(about = "Include configured MCP server names in future submissions")]
    Enable,
    #[command(about = "Stop including MCP server names; the next submit clears the stored list")]
    Disable,
    #[command(about = "Show whether MCP server names are shared and which configs are read")]
    Status {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusOutput {
    enabled: bool,
    locations: Vec<LocationOutput>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LocationOutput {
    client: &'static str,
    path: String,
    exists: bool,
}

pub fn run(subcommand: McpNamesSubcommand) -> Result<()> {
    match subcommand {
        McpNamesSubcommand::Enable => set_enabled(true),
        McpNamesSubcommand::Disable => set_enabled(false),
        McpNamesSubcommand::Status { json } => status(json),
    }
}

fn known_locations() -> Vec<McpConfigLocation> {
    tokens_core::paths::home_dir()
        .map(|home| mcp_config_locations(&home))
        .unwrap_or_default()
}

fn set_enabled(enabled: bool) -> Result<()> {
    let mut settings = crate::settings::Settings::load();
    settings.share_mcp_server_names = enabled;
    settings.save()?;

    println!();
    if enabled {
        println!("{}", t!("commands.mcpNames.enabled").green());
        println!();
        print_explanation(&known_locations());
        println!("{}", t!("commands.mcpNames.disableHint").bright_black());
    } else {
        println!("{}", t!("commands.mcpNames.disabled").green());
        println!("{}", t!("commands.mcpNames.disabledClears").bright_black());
    }
    println!();
    Ok(())
}

fn status(json: bool) -> Result<()> {
    let enabled = crate::settings::Settings::load().share_mcp_server_names;
    let locations = known_locations();

    if json {
        let output = StatusOutput {
            enabled,
            locations: locations
                .iter()
                .map(|location| LocationOutput {
                    client: location.client,
                    path: location.path.display().to_string(),
                    exists: location.path.exists(),
                })
                .collect(),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!();
    if enabled {
        println!("{}", t!("commands.mcpNames.statusOn"));
    } else {
        println!("{}", t!("commands.mcpNames.statusOff"));
    }
    println!();
    print_explanation(&locations);
    println!();
    Ok(())
}

/// What is read, from where, and who sees it — shown on enable and in status.
fn print_explanation(locations: &[McpConfigLocation]) {
    println!("{}", t!("commands.mcpNames.whatIsRead"));
    for location in locations {
        println!(
            "{}",
            location_line(location.client, &location.path).bright_black()
        );
    }
    println!("{}", t!("commands.mcpNames.public"));
    println!("{}", t!("commands.mcpNames.preview").bright_black());
}

fn location_line(client: &str, path: &Path) -> String {
    if path.exists() {
        tr!(
            "commands.mcpNames.location",
            client = client,
            path = path.display()
        )
    } else {
        tr!(
            "commands.mcpNames.locationMissing",
            client = client,
            path = path.display()
        )
    }
}
