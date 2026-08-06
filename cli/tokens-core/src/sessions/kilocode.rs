//! KiloCode task parser
//!
//! Shares the same task-log format as Roo Code and reuses the same parser helper.

use super::roocode::parse_roo_kilo_file;
use super::UnifiedMessage;
use std::path::Path;

pub fn parse_kilocode_file(path: &Path) -> Vec<UnifiedMessage> {
    parse_roo_kilo_file(path, "kilocode")
}
