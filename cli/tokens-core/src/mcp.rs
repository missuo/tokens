//! MCP server discovery — collects configured server *names* only (no secrets/paths).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub fn discover_mcp_server_names(home_dir: Option<&Path>) -> Vec<String> {
    let home = match home_dir.map(PathBuf::from).or_else(dirs::home_dir) {
        Some(h) => h,
        None => return Vec::new(),
    };

    let mut names: BTreeSet<String> = BTreeSet::new();

    collect_mcp_server_keys(&home.join(".claude").join(".mcp.json"), &mut names);

    #[cfg(target_os = "macos")]
    {
        collect_mcp_server_keys(
            &home
                .join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json"),
            &mut names,
        );
    }

    collect_mcp_server_keys(&home.join(".cursor").join("mcp.json"), &mut names);

    collect_mcp_server_keys(
        &home.join(".kiro").join("settings").join("mcp.json"),
        &mut names,
    );

    collect_skill_mcp_names(
        &home.join(".config").join("opencode").join("skills"),
        &mut names,
    );
    collect_skill_mcp_names(&home.join(".opencode").join("skills"), &mut names);

    names.into_iter().collect()
}

fn collect_mcp_server_keys(path: &Path, names: &mut BTreeSet<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    if let Some(servers) = value.get("mcpServers").and_then(|v| v.as_object()) {
        for key in servers.keys() {
            if !key.is_empty() {
                names.insert(key.clone());
            }
        }
    }
}

fn collect_skill_mcp_names(skills_dir: &Path, names: &mut BTreeSet<String>) {
    let dir = match std::fs::read_dir(skills_dir) {
        Ok(d) => d,
        Err(_) => return,
    };

    for entry in dir.flatten() {
        let path = entry.path();

        if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.is_file() {
                extract_mcp_names_from_skill_md(&skill_file, names);
            }
        } else if path.extension().is_some_and(|ext| ext == "md") {
            extract_mcp_names_from_skill_md(&path, names);
        }
    }
}

/// Line-based YAML frontmatter `mcp:` key extractor (avoids a full YAML dependency).
fn extract_mcp_names_from_skill_md(path: &Path, names: &mut BTreeSet<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let frontmatter = match extract_yaml_frontmatter(&content) {
        Some(fm) => fm,
        None => return,
    };

    let mut in_mcp_section = false;
    let mut mcp_indent: usize = 0;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        if !in_mcp_section {
            if trimmed == "mcp:" || trimmed.starts_with("mcp:") {
                in_mcp_section = true;
                mcp_indent = indent;
            }
        } else {
            if indent <= mcp_indent && !trimmed.is_empty() {
                break;
            }

            if indent == mcp_indent + 2 || (mcp_indent == 0 && indent == 2) {
                if let Some(key) = trimmed
                    .strip_suffix(':')
                    .or_else(|| trimmed.split(':').next())
                {
                    let key = key.trim();
                    if !key.is_empty() && !key.starts_with('-') {
                        names.insert(key.to_string());
                    }
                }
            }
        }
    }
}

fn extract_yaml_frontmatter(content: &str) -> Option<&str> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }

    let after_first = &trimmed[3..];
    let after_first = after_first.trim_start_matches(['\r', '\n']);

    let end = after_first.find("\n---")?;
    Some(&after_first[..end])
}

