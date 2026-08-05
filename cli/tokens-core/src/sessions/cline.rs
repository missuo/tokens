//! Cline task parser
//!
//! Cline is the upstream project that Roo Code and Kilo forked from, so it
//! shares the same VS Code globalStorage task-log format and reuses the same
//! parser helper.

use super::roocode::parse_roo_kilo_file;
use super::UnifiedMessage;
use std::path::Path;

pub fn parse_cline_file(path: &Path) -> Vec<UnifiedMessage> {
    parse_roo_kilo_file(path, "cline")
}
