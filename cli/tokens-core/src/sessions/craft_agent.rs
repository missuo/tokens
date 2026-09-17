//! Craft Agent session parser
//!
//! Craft Agent (https://www.craft.do) runs on the pi engine and keeps each
//! session's transcript in the pi record format, one file per session, under
//! `~/.craft-agent/workspaces/<workspace>/sessions/<session-id>/.pi-sessions/`.
//! Parsing therefore delegates to `super::pi::parse_pi_format_file`; only the
//! scan root, the client id and one provider rewrite differ.
//!
//! The scanner only admits `*.jsonl` files whose parent directory is exactly
//! `.pi-sessions` (the `craft-agent-pi-session` pattern). The rest of a Craft
//! workspace is not known to be pi-format, and a transcript in another shape
//! that happened to parse would be counted on top of the real one.
//!
//! Craft Agent lets users point it at their own endpoint, and such messages
//! record the literal provider `custom-endpoint`. That is not a vendor any
//! pricing source knows, so it is replaced by the provider the model name
//! implies (`claude-opus-5` -> `anthropic`) when there is one.
//!
//! Every session in a workspace records the workspace root as its `cwd`, so
//! workspace attribution is one bucket per Craft workspace. Token totals are
//! unaffected. See missuo/tokens#48.

use super::pi::parse_pi_format_file;
use super::UnifiedMessage;
use crate::provider_identity::inferred_provider_from_model;
use std::path::Path;

const CLIENT_ID: &str = "craft-agent";
const CUSTOM_ENDPOINT_PROVIDER: &str = "custom-endpoint";

/// Parse one Craft Agent pi-format session file.
pub fn parse_craft_agent_file(path: &Path) -> Vec<UnifiedMessage> {
    let mut messages = parse_pi_format_file(path, CLIENT_ID, CLIENT_ID);
    for message in &mut messages {
        if message.provider_id == CUSTOM_ENDPOINT_PROVIDER {
            if let Some(provider) = inferred_provider_from_model(&message.model_id) {
                message.provider_id = provider.to_string();
            }
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn session_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    // The record shape reported in missuo/tokens#48 from a real install.
    const SESSION: &str = r#"{"type":"session","version":3,"id":"019fc54d-8891-787c-80f4-c4d41c1d917e","timestamp":"2026-08-03T01:47:00.625Z","cwd":"/Users/me/.craft-agent/workspaces/my-workspace"}
{"type":"message","id":"bc7b8d5b","parentId":"bfed760b","timestamp":"2026-08-03T01:47:11.580Z","message":{"role":"assistant","model":"claude-opus-5","provider":"custom-endpoint","api":"anthropic-messages","stopReason":"toolUse","usage":{"input":2,"output":566,"cacheRead":0,"cacheWrite":29984,"totalTokens":30552,"reasoning":187,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}}}"#;

    #[test]
    fn parses_pi_format_sessions_as_craft_agent() {
        let file = session_file(SESSION);
        let messages = parse_craft_agent_file(file.path());

        assert_eq!(messages.len(), 1);
        let message = &messages[0];
        assert_eq!(message.client, "craft-agent");
        assert_eq!(message.session_id, "019fc54d-8891-787c-80f4-c4d41c1d917e");
        assert_eq!(message.model_id, "claude-opus-5");
        assert_eq!(message.tokens.input, 2);
        assert_eq!(message.tokens.output, 566);
        assert_eq!(message.tokens.cache_write, 29984);
    }

    #[test]
    fn custom_endpoint_provider_is_replaced_by_the_model_vendor() {
        let file = session_file(SESSION);
        let messages = parse_craft_agent_file(file.path());
        assert_eq!(messages[0].provider_id, "anthropic");
    }

    #[test]
    fn named_providers_are_kept() {
        let file = session_file(&SESSION.replace("custom-endpoint", "openrouter"));
        let messages = parse_craft_agent_file(file.path());
        assert_eq!(messages[0].provider_id, "openrouter");
    }
}
