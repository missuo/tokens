//! Cursor Agent CLI credentials, independent of the Electron IDE database.
use anyhow::{bail, ensure, Context, Result};
use base64::Engine;
use std::path::{Path, PathBuf};

pub(super) fn auth_paths(home: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("CURSOR_AUTH_FILE").filter(|p| !p.is_empty()) {
        return vec![path.into()];
    }
    if let Some(root) = std::env::var_os("CURSOR_CONFIG_DIR").filter(|p| !p.is_empty()) {
        paths.push(PathBuf::from(root).join("auth.json"));
    }
    #[cfg(target_os = "windows")]
    paths.push(
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"))
            .join("Cursor/auth.json"),
    );
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        paths.push(home.join(".cursor/auth.json"));
        paths.push(
            std::env::var_os("XDG_CONFIG_HOME")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
                .join("cursor/auth.json"),
        );
        #[cfg(not(target_os = "macos"))]
        paths.push(home.join(".cursor/auth.json"));
    }
    paths
}

pub(super) fn normalize_token(token: &str) -> Result<String> {
    let token = token.trim();
    let jwt = token
        .split_once("%3A%3A")
        .or_else(|| token.split_once("%3a%3a"))
        .or_else(|| token.split_once("::"))
        .map_or(token, |(_, jwt)| jwt);
    let payload = jwt
        .split('.')
        .nth(1)
        .context("Invalid Cursor CLI access token")?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .context("Invalid Cursor CLI access token payload")?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded)?;
    ensure!(
        claims.get("type").and_then(|v| v.as_str()) != Some("api_key_token"),
        "Cursor CLI API-key tokens cannot read dashboard usage; sign in with `agent login`"
    );
    if let Some(exp) = claims.get("exp").and_then(|v| v.as_i64()) {
        ensure!(
            exp > chrono::Utc::now().timestamp(),
            "Cursor CLI session expired; run `agent login`"
        );
    }
    super::session_token_from_access_token(jwt)
}

fn token_from_file(path: &Path) -> Result<String> {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let token = value
        .get("accessToken")
        .and_then(|v| v.as_str())
        .filter(|token| !token.trim().is_empty())
        .context("Cursor CLI auth.json has no accessToken")?;
    normalize_token(token)
}

// Injected lookup keeps tests independent of the real login Keychain and home.
fn resolve(paths: &[PathBuf], keychain: impl FnOnce() -> Result<String>) -> Result<String> {
    if let Ok(token) = keychain().and_then(|token| normalize_token(&token)) {
        return Ok(token);
    }
    for path in paths {
        if let Ok(token) = token_from_file(path) {
            return Ok(token);
        }
    }
    bail!("No usable Cursor CLI login found in its credential store; run `agent login` (or `cursor-agent login`)")
}

pub(super) fn read(home: &Path) -> Result<String> {
    let paths = auth_paths(home);
    resolve(&paths, || {
        #[cfg(target_os = "macos")]
        {
            // The CLI uses this exact service/account, via Apple's security CLI.
            // An explicit file backend must not accidentally select a stale keychain login.
            if std::env::var_os("CURSOR_AUTH_FILE").is_some()
                || std::env::var("AGENT_CLI_CREDENTIAL_STORE").as_deref() == Ok("file")
            {
                bail!("Cursor CLI uses file credentials");
            }
            let output = std::process::Command::new("/usr/bin/security")
                .args([
                    "find-generic-password",
                    "-s",
                    "cursor-access-token",
                    "-a",
                    "cursor-user",
                    "-w",
                ])
                .output()?;
            ensure!(
                output.status.success(),
                "Cursor CLI Keychain login not found"
            );
            Ok(String::from_utf8(output.stdout)?)
        }
        #[cfg(not(target_os = "macos"))]
        bail!("Cursor CLI uses file credentials")
    })
}

pub(super) fn available(home: &Path) -> bool {
    if auth_paths(home)
        .iter()
        .any(|path| token_from_file(path).is_ok())
    {
        return true;
    }
    #[cfg(target_os = "macos")]
    if std::env::var_os("CURSOR_AUTH_FILE").is_none()
        && std::env::var("AGENT_CLI_CREDENTIAL_STORE").as_deref() != Ok("file")
    {
        // Probe attributes only; reading the password happens during sync/login.
        return std::process::Command::new("/usr/bin/security")
            .args([
                "find-generic-password",
                "-s",
                "cursor-access-token",
                "-a",
                "cursor-user",
            ])
            .output()
            .is_ok_and(|output| output.status.success());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    fn jwt(claims: serde_json::Value) -> String {
        format!(
            "header.{}.signature",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string())
        )
    }

    #[test]
    fn reads_cli_file_without_an_ide_and_skips_invalid_candidates() {
        let root = tempfile::tempdir().unwrap();
        let bad = root.path().join("bad.json");
        let good = root.path().join("auth.json");
        std::fs::write(&bad, r#"{"apiKey":"crsr_fixture"}"#).unwrap();
        let token = jwt(serde_json::json!({"sub":"auth0|user_cli"}));
        std::fs::write(
            &good,
            serde_json::json!({"accessToken":token,"refreshToken":"unused"}).to_string(),
        )
        .unwrap();
        let result = resolve(&[bad, good], || bail!("no keychain")).unwrap();
        assert_eq!(result, format!("user_cli%3A%3A{token}"));
        assert!(!root.path().join("Library").exists());
    }

    #[test]
    fn reads_cli_keychain_and_normalizes_cookie_forms() {
        let token = jwt(serde_json::json!({"sub":"user_cli"}));
        let expected = format!("user_cli%3A%3A{token}");
        assert_eq!(resolve(&[], || Ok(token.clone())).unwrap(), expected);
        assert_eq!(
            normalize_token(&format!("user_cli::{token}")).unwrap(),
            expected
        );
        assert_eq!(normalize_token(&expected).unwrap(), expected);
    }

    #[test]
    fn rejects_api_keys_expired_or_missing_tokens() {
        for token in [
            "crsr_key".to_owned(),
            jwt(serde_json::json!({"sub":"user_cli","type":"api_key_token"})),
            jwt(serde_json::json!({"sub":"user_cli","exp":1})),
        ] {
            assert!(normalize_token(&token).is_err());
        }
        assert!(resolve(&[], || bail!("missing")).is_err());
    }
}
