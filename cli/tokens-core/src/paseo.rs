//! Discover native provider transcripts configured by Paseo.
//!
//! Paseo's agent records/timelines are not additional billable sessions. Read
//! only provider directory overrides, then let the native parsers and scanner
//! deduplication handle usage. Configuration reference:
//! https://github.com/getpaseo/paseo/blob/main/docs/custom-providers.md

use crate::clients::ClientId;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) fn discover_scan_paths(
    home: &Path,
    enabled: &HashSet<ClientId>,
    use_env_roots: bool,
) -> Vec<(ClientId, PathBuf)> {
    if !enabled.contains(&ClientId::Claude) && !enabled.contains(&ClientId::Codex) {
        return Vec::new();
    }
    let paseo_home = if use_env_roots {
        std::env::var_os("PASEO_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|path| expand_home(&path, home))
            .unwrap_or_else(|| home.join(".paseo"))
    } else {
        home.join(".paseo")
    };
    let Ok(contents) = std::fs::read_to_string(paseo_home.join("config.json")) else {
        return Vec::new();
    };
    let Ok(config) = serde_json::from_str::<Value>(&contents) else {
        return Vec::new();
    };
    configured_scan_paths(&config, enabled)
}

fn expand_home(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(relative) => home.join(relative),
        Err(_) => path.to_path_buf(),
    }
}

fn configured_scan_paths(config: &Value, enabled: &HashSet<ClientId>) -> Vec<(ClientId, PathBuf)> {
    let Some(providers) = config
        .pointer("/agents/providers")
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for (id, provider) in providers {
        // Paseo profiles extend built-in providers and inherit their env,
        // with the profile's own entries taking precedence. Disabled profiles
        // can still have historical usage, so include their directories too.
        let base = if matches!(id.as_str(), "claude" | "codex") {
            id.as_str()
        } else {
            provider
                .get("extends")
                .and_then(Value::as_str)
                .unwrap_or("")
        };
        let (client, variable, subdirs): (_, _, &[&str]) = match base {
            "claude" => (
                ClientId::Claude,
                "CLAUDE_CONFIG_DIR",
                &["projects", "transcripts"],
            ),
            "codex" => (
                ClientId::Codex,
                "CODEX_HOME",
                &["sessions", "archived_sessions"],
            ),
            _ => continue,
        };
        if !enabled.contains(&client) {
            continue;
        }
        let raw = provider
            .get("env")
            .and_then(|env| env.get(variable))
            .or_else(|| providers.get(base)?.get("env")?.get(variable));
        let Some(raw) = raw
            .and_then(Value::as_str)
            .filter(|raw| !raw.trim().is_empty())
        else {
            continue;
        };
        let root = Path::new(raw);
        // Provider env values are passed literally to the subprocess. Relative
        // paths depend on its cwd, which need not be Tokens' cwd; guessing here
        // could scan an unrelated directory. Do not shell-expand these values.
        if !root.is_absolute() {
            continue;
        }
        paths.extend(subdirs.iter().map(|subdir| (client, root.join(subdir))));
    }
    paths.sort_unstable_by(|a, b| a.0.as_str().cmp(b.0.as_str()).then_with(|| a.1.cmp(&b.1)));
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_env::EnvGuard;
    use crate::scanner::scan_all_clients_with_env_strategy;
    use serde_json::json;
    use serial_test::serial;
    use tempfile::TempDir;

    fn write_config(home: &Path, config: &Value) {
        std::fs::create_dir_all(home).unwrap();
        std::fs::write(
            home.join("config.json"),
            serde_json::to_vec(config).unwrap(),
        )
        .unwrap();
    }

    fn session(path: &Path) -> PathBuf {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "{}\n").unwrap();
        path.to_path_buf()
    }

    #[test]
    fn paseo_scan_discovers_profiles_and_archives_with_native_clients() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let claude = home.join("custom-claude");
        let codex = home.join("custom-codex");
        write_config(
            &home.join(".paseo"),
            &json!({"agents": {"providers": {
                "claude": {"env": {"CLAUDE_CONFIG_DIR": claude}},
                "work-claude": {"extends": "claude", "label": "Work"},
                "work-codex": {"extends": "codex", "enabled": false, "env": {"CODEX_HOME": codex}},
                "unrelated": {"extends": "acp", "env": {"CODEX_HOME": home.join("ignore")}}
            }}}),
        );
        let project = session(&claude.join("projects/project/main.jsonl"));
        let subagent = session(&claude.join("projects/project/main/subagents/agent-one.jsonl"));
        let transcript = session(&claude.join("transcripts/other.jsonl"));
        let rollout = session(&codex.join("sessions/2026/10/20/rollout.jsonl"));
        let archived = session(&codex.join("archived_sessions/old.jsonl"));
        session(&home.join("ignore/sessions/ignored.jsonl"));
        // Paseo timelines/lastUsage must never become a second usage source.
        session(&home.join(".paseo/agents/project/agent/timeline.jsonl"));

        let result = scan_all_clients_with_env_strategy(home.to_str().unwrap(), &[], false);
        let claude_files: HashSet<_> = result.get(ClientId::Claude).iter().cloned().collect();
        assert_eq!(claude_files, HashSet::from([project, subagent, transcript]));
        let codex_files: HashSet<_> = result.get(ClientId::Codex).iter().cloned().collect();
        assert_eq!(codex_files, HashSet::from([rollout, archived]));

        let filtered = scan_all_clients_with_env_strategy(
            home.to_str().unwrap(),
            &["claude".to_string()],
            false,
        );
        assert!(filtered.get(ClientId::Codex).is_empty());
    }

    #[test]
    fn paseo_scan_deduplicates_default_and_overlapping_profile_roots() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let root = home.join(".codex");
        let rollout = session(&root.join("sessions/rollout.jsonl"));
        write_config(
            &home.join(".paseo"),
            &json!({"agents": {"providers": {
                "codex": {"env": {"CODEX_HOME": root}},
                "work": {"extends": "codex"},
                "alias": {"extends": "codex", "env": {"CODEX_HOME": root.join(".")}}
            }}}),
        );
        let result = scan_all_clients_with_env_strategy(
            home.to_str().unwrap(),
            &["codex".to_string()],
            false,
        );
        assert_eq!(result.get(ClientId::Codex), &[rollout]);
    }

    #[test]
    #[serial]
    fn paseo_home_override_is_ignored_for_explicit_home() {
        let home = TempDir::new().unwrap();
        let custom = TempDir::new().unwrap();
        let root = home.path().join("provider");
        write_config(
            custom.path(),
            &json!({"agents": {"providers": {
                "codex": {"env": {"CODEX_HOME": root}}
            }}}),
        );
        let mut env = EnvGuard::capture(&["PASEO_HOME"]);
        env.set("PASEO_HOME", custom.path());
        let enabled = HashSet::from([ClientId::Codex]);
        assert_eq!(discover_scan_paths(home.path(), &enabled, true).len(), 2);
        assert!(discover_scan_paths(home.path(), &enabled, false).is_empty());
        // PASEO_HOME itself follows Paseo's home expansion convention.
        assert_eq!(
            expand_home(Path::new("~/custom"), home.path()),
            home.path().join("custom")
        );
    }

    #[test]
    fn paseo_profile_overrides_base_env_and_skips_ambiguous_paths() {
        let home = TempDir::new().unwrap();
        let base = home.path().join("base");
        let profile = home.path().join("profile");
        let config = json!({"agents": {"providers": {
            "codex": {"env": {"CODEX_HOME": base}},
            "work": {"extends": "codex", "env": {"CODEX_HOME": profile}},
            "empty": {"extends": "codex", "env": {"CODEX_HOME": ""}},
            "relative": {"extends": "codex", "env": {"CODEX_HOME": "relative"}},
            "tilde": {"extends": "codex", "env": {"CODEX_HOME": "~/provider"}},
            "invalid": {"extends": "codex", "env": {"CODEX_HOME": 42}},
            "unknown": {"extends": "acp", "env": {"CODEX_HOME": home.path().join("ignored")}}
        }}});
        let enabled = HashSet::from([ClientId::Codex]);
        let paths = configured_scan_paths(&config, &enabled);
        assert_eq!(paths.len(), 4);
        assert!(paths.contains(&(ClientId::Codex, base.join("sessions"))));
        assert!(paths.contains(&(ClientId::Codex, profile.join("sessions"))));
        assert!(configured_scan_paths(&config, &HashSet::from([ClientId::Claude])).is_empty());
        assert!(configured_scan_paths(&json!({"agents": {"providers": []}}), &enabled).is_empty());
    }

    #[test]
    fn paseo_missing_or_invalid_config_is_nonfatal() {
        let home = TempDir::new().unwrap();
        let enabled = HashSet::from([ClientId::Claude]);
        assert!(discover_scan_paths(home.path(), &enabled, false).is_empty());
        std::fs::create_dir(home.path().join(".paseo")).unwrap();
        std::fs::write(home.path().join(".paseo/config.json"), "invalid json").unwrap();
        assert!(discover_scan_paths(home.path(), &enabled, false).is_empty());
    }
}
