//! CodeBuddy session parser.
//!
//! CodeBuddy persists CLI/WebUI usage as JSONL transcripts under
//! `~/.codebuddy/projects/<project-key>/*.jsonl`, and the IDE / VS Code
//! extension writes final agent usage into extension logs.

use super::UnifiedMessage;
use std::path::Path;

const DEFAULT_MODEL: &str = "codebuddy";

pub fn parse_codebuddy_file(path: &Path) -> Vec<UnifiedMessage> {
    if super::tencent_buddy::is_extension_log_source(path) {
        return super::tencent_buddy::parse_extension_log_file("codebuddy", DEFAULT_MODEL, path);
    }

    super::tencent_buddy::parse_jsonl_file("codebuddy", DEFAULT_MODEL, path)
}
