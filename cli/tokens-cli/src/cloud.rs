//! Usage reporting from cloud agent sessions (Claude Code on the web, Codex
//! cloud, and anything else that runs an agent in a throwaway container).
//!
//! A cloud container is not a device in the usual sense. It is created for one
//! session, may be torn down and resumed in a fresh container with the same
//! transcript restored, and never runs `tokens serve`. Treating it like a laptop
//! breaks in two ways:
//!
//! * a random `device.json` id is minted per container, so a resumed session
//!   resubmits its whole transcript under a *new* device and the server adds it
//!   on top of the first copy;
//! * the device name would default to the container hostname.
//!
//! So a cloud submission is keyed to the **session**, not the container: the
//! device id is a domain-separated SHA-256 of the provider's session id. The same
//! session always lands on the same device row, where the server's per-day
//! regression guard keeps the larger copy instead of summing them. Different
//! sessions are different devices, so their usage adds up as it should.
//!
//! Privacy: the raw session id never leaves the container (only the hash does),
//! the device name is a fixed label rather than anything read from the machine,
//! and a hook payload on stdin/argv is read for its session id alone — prompts,
//! transcript paths and messages in it are dropped unread.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;

use crate::device::SubmitDevice;

/// Bumping this re-keys every cloud device, so it must stay fixed.
const DEVICE_HASH_DOMAIN: &str = "tokens-cloud-device/v1";
/// Hex characters of the hash kept in the device id (128 bits).
const DEVICE_HASH_HEX_LEN: usize = 32;
const STATE_FILE_NAME: &str = "cloud-submit.json";
/// Hook payloads are small JSON objects; anything larger is not one.
const MAX_HOOK_PAYLOAD_BYTES: usize = 1024 * 1024;
const MAX_SESSION_ID_LEN: usize = 256;

pub const PROVIDER_ENV: &str = "TOKENS_CLOUD_PROVIDER";
pub const SESSION_ID_ENV: &str = "TOKENS_CLOUD_SESSION_ID";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CloudProvider {
    /// Claude Code on the web / Claude Code cloud sessions.
    #[value(name = "claude-code")]
    ClaudeCode,
    /// OpenAI Codex cloud tasks.
    #[value(name = "codex")]
    Codex,
}

impl CloudProvider {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude-code" | "claude_code" | "claude" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
        }
    }

    /// Segment used in the device id. Mirrors `CLOUD_DEVICE_PROVIDERS` in
    /// web/src/lib/devices/cloud.ts, which enforces the client below.
    fn device_segment(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }

    /// The only client a cloud device of this provider may submit. The agent
    /// that ran in the container is the only thing that container can vouch
    /// for; anything else found there was put there.
    pub fn client_id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn device_label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code cloud",
            Self::Codex => "Codex cloud",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIdSource {
    Flag,
    Environment,
    HookPayload,
    ClaudeCodeRemoteEnv,
    ClaudeCodeSessionEnv,
}

impl SessionIdSource {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Flag => "--session-id",
            Self::Environment => SESSION_ID_ENV,
            Self::HookPayload => "hook payload",
            Self::ClaudeCodeRemoteEnv => "CLAUDE_CODE_REMOTE_SESSION_ID",
            Self::ClaudeCodeSessionEnv => "CLAUDE_CODE_SESSION_ID",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudSession {
    pub provider: CloudProvider,
    pub session_source: SessionIdSource,
    session_id: String,
}

impl CloudSession {
    /// The device this session submits as: a hash of the session id, never the
    /// id itself, with a fixed name.
    pub fn submit_device(&self) -> SubmitDevice {
        SubmitDevice {
            id: device_id_for(self.provider, &self.session_id),
            name: Some(self.provider.device_label().to_string()),
        }
    }
}

/// Inputs for [`detect`], separated from the process environment so tests can
/// drive it without touching global state.
#[derive(Debug, Default, Clone)]
pub struct DetectInputs {
    pub provider_flag: Option<CloudProvider>,
    pub session_id_flag: Option<String>,
    pub hook_payload: Option<String>,
    pub env: Vec<(String, String)>,
}

impl DetectInputs {
    pub fn from_process(
        provider_flag: Option<CloudProvider>,
        session_id_flag: Option<String>,
        hook_payload: Option<String>,
    ) -> Self {
        let keys = [
            PROVIDER_ENV,
            SESSION_ID_ENV,
            "CLAUDE_CODE_REMOTE",
            "CLAUDE_CODE_REMOTE_SESSION_ID",
            "CLAUDE_CODE_SESSION_ID",
        ];
        Self {
            provider_flag,
            session_id_flag,
            hook_payload,
            env: keys
                .iter()
                .filter_map(|key| std::env::var(key).ok().map(|v| (key.to_string(), v)))
                .collect(),
        }
    }

    fn env(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim())
            .filter(|v| !v.is_empty())
    }
}

fn is_claude_code_remote(inputs: &DetectInputs) -> bool {
    inputs
        .env("CLAUDE_CODE_REMOTE")
        .is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1")
}

/// Work out which provider and session this process is running in.
///
/// Provider: `--provider`, else `$TOKENS_CLOUD_PROVIDER`, else Claude Code when
/// `CLAUDE_CODE_REMOTE=true`. Session id: `--session-id`, else
/// `$TOKENS_CLOUD_SESSION_ID`, else the hook payload, else (Claude Code only)
/// the session env vars Claude Code sets in its cloud containers.
///
/// There is deliberately no random fallback: a session we cannot name cannot be
/// deduplicated when it resumes in another container, so it is refused.
pub fn detect(inputs: &DetectInputs) -> Result<CloudSession> {
    let provider = match inputs.provider_flag {
        Some(provider) => provider,
        None => match inputs.env(PROVIDER_ENV) {
            Some(value) => CloudProvider::parse(value).ok_or_else(|| {
                anyhow!("{PROVIDER_ENV}={value} is not a supported provider (claude-code, codex)")
            })?,
            None if is_claude_code_remote(inputs) => CloudProvider::ClaudeCode,
            None => {
                return Err(anyhow!(
                    "not a recognised cloud session: pass --provider or set {PROVIDER_ENV}"
                ))
            }
        },
    };

    let hook_session = inputs
        .hook_payload
        .as_deref()
        .and_then(session_id_from_hook_payload);

    let (session_id, session_source) = if let Some(id) = inputs.session_id_flag.clone() {
        (id, SessionIdSource::Flag)
    } else if let Some(id) = inputs.env(SESSION_ID_ENV) {
        (id.to_string(), SessionIdSource::Environment)
    } else if provider == CloudProvider::ClaudeCode
        && inputs.env("CLAUDE_CODE_REMOTE_SESSION_ID").is_some()
    {
        // Preferred over the hook's local session id: it names the cloud
        // session itself, which outlives any one container and any `/clear`.
        (
            inputs.env("CLAUDE_CODE_REMOTE_SESSION_ID").unwrap().to_string(),
            SessionIdSource::ClaudeCodeRemoteEnv,
        )
    } else if let Some(id) = hook_session {
        (id, SessionIdSource::HookPayload)
    } else if provider == CloudProvider::ClaudeCode
        && is_claude_code_remote(inputs)
        && inputs.env("CLAUDE_CODE_SESSION_ID").is_some()
    {
        (
            inputs.env("CLAUDE_CODE_SESSION_ID").unwrap().to_string(),
            SessionIdSource::ClaudeCodeSessionEnv,
        )
    } else {
        return Err(anyhow!(
            "could not determine the {} session id: pass --session-id, set {SESSION_ID_ENV}, \
             or run from a hook that provides one",
            provider.as_str()
        ));
    };

    let session_id = validate_session_id(&session_id)?;
    Ok(CloudSession {
        provider,
        session_source,
        session_id,
    })
}

fn validate_session_id(id: &str) -> Result<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("cloud session id must not be empty"));
    }
    if trimmed.len() > MAX_SESSION_ID_LEN {
        return Err(anyhow!(
            "cloud session id must be at most {MAX_SESSION_ID_LEN} characters"
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(anyhow!("cloud session id must not contain control characters"));
    }
    Ok(trimmed.to_string())
}

/// `cloud_<provider>_<32 hex>`. The server checks this exact shape
/// (web/src/lib/devices/cloud.ts) before applying cloud-device rules.
pub fn device_id_for(provider: CloudProvider, session_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DEVICE_HASH_DOMAIN.as_bytes());
    hasher.update([0u8]);
    hasher.update(provider.as_str().as_bytes());
    hasher.update([0u8]);
    hasher.update(session_id.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "cloud_{}_{}",
        provider.device_segment(),
        &hex[..DEVICE_HASH_HEX_LEN]
    )
}

/// Pull the session id out of a hook payload and nothing else.
///
/// Claude Code hooks send `{"session_id": …}` on stdin; Codex's `notify`
/// program receives `{"thread-id": …}` as its last argument. Both payloads also
/// carry prompts or transcript paths, which are never read.
pub fn session_id_from_hook_payload(raw: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct HookPayload {
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default, rename = "thread-id")]
        thread_id: Option<String>,
        #[serde(default, rename = "thread_id")]
        thread_id_snake: Option<String>,
    }

    let payload: HookPayload = serde_json::from_str(raw.trim()).ok()?;
    payload
        .session_id
        .or(payload.thread_id)
        .or(payload.thread_id_snake)
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// Read a hook payload from stdin when stdin is a pipe (Claude Code hooks).
/// A terminal or an empty pipe yields `None`.
pub fn read_stdin_payload() -> Option<String> {
    use std::io::IsTerminal;
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let mut buf = String::new();
    stdin
        .lock()
        .take(MAX_HOOK_PAYLOAD_BYTES as u64)
        .read_to_string(&mut buf)
        .ok()?;
    let trimmed = buf.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudSubmitState {
    device_id: String,
    last_submit_at_ms: i64,
}

fn state_path() -> PathBuf {
    crate::paths::get_config_dir().join(STATE_FILE_NAME)
}

/// Whether the last successful submit for this device is recent enough to skip
/// this one. Stop hooks fire after every turn; scanning on each would be waste.
pub fn throttled(device_id: &str, min_interval_minutes: u64, now_ms: i64) -> bool {
    if min_interval_minutes == 0 {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(state_path()) else {
        return false;
    };
    let Ok(state) = serde_json::from_str::<CloudSubmitState>(&content) else {
        return false;
    };
    state.device_id == device_id
        && now_ms.saturating_sub(state.last_submit_at_ms)
            < (min_interval_minutes as i64).saturating_mul(60_000)
}

pub fn record_submit(device_id: &str, now_ms: i64) -> Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let state = CloudSubmitState {
        device_id: device_id.to_string(),
        last_submit_at_ms: now_ms,
    };
    std::fs::write(&path, serde_json::to_string(&state)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(env: &[(&str, &str)]) -> DetectInputs {
        DetectInputs {
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn claude_code_remote_is_detected_from_its_environment() {
        let session = detect(&inputs(&[
            ("CLAUDE_CODE_REMOTE", "true"),
            ("CLAUDE_CODE_REMOTE_SESSION_ID", "cse_abc"),
            ("CLAUDE_CODE_SESSION_ID", "local-uuid"),
        ]))
        .unwrap();
        assert_eq!(session.provider, CloudProvider::ClaudeCode);
        assert_eq!(session.session_source, SessionIdSource::ClaudeCodeRemoteEnv);
        assert_eq!(
            session.submit_device().id,
            device_id_for(CloudProvider::ClaudeCode, "cse_abc")
        );
    }

    #[test]
    fn remote_session_env_wins_over_the_hook_session_id() {
        let mut detect_inputs = inputs(&[
            ("CLAUDE_CODE_REMOTE", "true"),
            ("CLAUDE_CODE_REMOTE_SESSION_ID", "cse_abc"),
        ]);
        detect_inputs.hook_payload = Some(r#"{"session_id":"local-uuid"}"#.to_string());
        let session = detect(&detect_inputs).unwrap();
        assert_eq!(session.session_source, SessionIdSource::ClaudeCodeRemoteEnv);
    }

    #[test]
    fn a_local_machine_is_not_a_cloud_session() {
        let err = detect(&inputs(&[("CLAUDE_CODE_SESSION_ID", "local-uuid")])).unwrap_err();
        assert!(err.to_string().contains("not a recognised cloud session"));
    }

    #[test]
    fn codex_reads_the_thread_id_from_the_notify_payload_only() {
        let detect_inputs = DetectInputs {
            provider_flag: Some(CloudProvider::Codex),
            hook_payload: Some(
                r#"{"type":"agent-turn-complete","thread-id":"th_1","turn-id":"t1",
                    "input-messages":["secret prompt"],"last-assistant-message":"secret"}"#
                    .to_string(),
            ),
            ..Default::default()
        };
        let session = detect(&detect_inputs).unwrap();
        assert_eq!(session.provider, CloudProvider::Codex);
        assert_eq!(session.session_source, SessionIdSource::HookPayload);
        let device = session.submit_device();
        assert_eq!(device.id, device_id_for(CloudProvider::Codex, "th_1"));
        assert_eq!(device.name.as_deref(), Some("Codex cloud"));
        assert!(!device.id.contains("th_1"));
    }

    #[test]
    fn a_session_without_an_id_is_refused_rather_than_randomised() {
        let detect_inputs = DetectInputs {
            provider_flag: Some(CloudProvider::Codex),
            ..Default::default()
        };
        assert!(detect(&detect_inputs).is_err());
    }

    #[test]
    fn device_ids_are_stable_hashes_in_the_shape_the_server_expects() {
        let a = device_id_for(CloudProvider::ClaudeCode, "cse_abc");
        let b = device_id_for(CloudProvider::ClaudeCode, "cse_abc");
        let c = device_id_for(CloudProvider::ClaudeCode, "cse_abd");
        let d = device_id_for(CloudProvider::Codex, "cse_abc");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert!(a.starts_with("cloud_claude_"));
        assert!(d.starts_with("cloud_codex_"));
        let hash = a.trim_start_matches("cloud_claude_");
        assert_eq!(hash.len(), DEVICE_HASH_HEX_LEN);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn provider_env_must_name_a_supported_provider() {
        assert!(detect(&inputs(&[(PROVIDER_ENV, "devin")])).is_err());
        let session = detect(&inputs(&[(PROVIDER_ENV, "codex"), (SESSION_ID_ENV, "x")])).unwrap();
        assert_eq!(session.provider, CloudProvider::Codex);
        assert_eq!(session.session_source, SessionIdSource::Environment);
    }

    #[test]
    fn hook_payload_that_is_not_json_is_ignored() {
        assert_eq!(session_id_from_hook_payload("not json"), None);
        assert_eq!(session_id_from_hook_payload(r#"{"session_id":"  "}"#), None);
    }
}
