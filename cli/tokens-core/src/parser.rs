//! SIMD-accelerated JSON parser
//!
//! Uses simd-json for fast JSON parsing with SIMD instructions.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Parse a JSON file using SIMD-accelerated parsing
pub fn parse_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ParseError> {
    let mut data = fs::read(path).map_err(|e| ParseError::IoError(e.to_string()))?;

    simd_json::from_slice(&mut data).map_err(|e| ParseError::JsonError(e.to_string()))
}

/// Parse a JSONL file (one JSON object per line)
pub fn parse_jsonl_file<T, F>(path: &Path, mut process: F) -> Result<(), ParseError>
where
    T: serde::de::DeserializeOwned,
    F: FnMut(T),
{
    let file = fs::File::open(path).map_err(|e| ParseError::IoError(e.to_string()))?;
    let reader = BufReader::new(file);
    let mut buffer = Vec::with_capacity(4096);

    for line in reader.lines() {
        let line = line.map_err(|e| ParseError::IoError(e.to_string()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        buffer.clear();
        buffer.extend_from_slice(trimmed.as_bytes());

        match simd_json::from_slice::<T>(&mut buffer) {
            Ok(value) => process(value),
            Err(_) => continue,
        }
    }

    Ok(())
}

/// Parse error types
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("IO error: {0}")]
    IoError(String),

    #[error("JSON parse error: {0}")]
    JsonError(String),
}

