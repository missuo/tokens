use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::provider_identity::inferred_provider_from_model;
use crate::TokenBreakdown;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub fn parse_copilot_vscode_sessions(paths: &[PathBuf]) -> Vec<UnifiedMessage> {
    paths.iter().flat_map(|path| parse_file(path)).collect()
}

fn parse_file(path: &Path) -> Vec<UnifiedMessage> {
    let session_id = match path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) => stem.to_string(),
        None => return Vec::new(),
    };

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let workspace = read_workspace_for_file(path);

    let mut requests: Vec<Value> = Vec::new();

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let kind = obj.get("kind").and_then(Value::as_i64).unwrap_or(-1);
        match kind {
            0 => {
                if let Some(arr) = obj.pointer("/v/requests").and_then(Value::as_array) {
                    requests.extend(arr.iter().cloned());
                }
            }
            2 => {
                if let Some(k) = obj.get("k").and_then(Value::as_array) {
                    let is_requests = k
                        .first()
                        .and_then(Value::as_str)
                        .map(|s| s == "requests")
                        .unwrap_or(false);
                    if is_requests {
                        if let Some(arr) = obj.get("v").and_then(Value::as_array) {
                            requests.extend(arr.iter().cloned());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    requests
        .iter()
        .filter_map(|req| request_to_message(req, &session_id, &workspace))
        .collect()
}

fn request_to_message(
    req: &Value,
    session_id: &str,
    workspace: &Option<(String, Option<String>)>,
) -> Option<UnifiedMessage> {
    let prompt_tokens = req
        .get("promptTokens")
        .and_then(Value::as_i64)
        .or_else(|| {
            req.pointer("/result/metadata/promptTokens")
                .and_then(Value::as_i64)
        })
        .unwrap_or(0);

    let completion_tokens = req
        .get("completionTokens")
        .and_then(Value::as_i64)
        .or_else(|| {
            req.pointer("/result/metadata/outputTokens")
                .and_then(Value::as_i64)
        })
        .unwrap_or(0);

    if prompt_tokens == 0 && completion_tokens == 0 {
        return None;
    }

    let timestamp_ms = req.get("timestamp").and_then(Value::as_i64).unwrap_or(0);

    let resolved_model = req
        .pointer("/result/metadata/resolvedModel")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let model_id_raw = req
        .get("modelId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let model_id = resolved_model
        .or_else(|| model_id_raw.map(|m| m.strip_prefix("copilot/").unwrap_or(m)))
        .unwrap_or("auto")
        .to_string();

    // Filter: only include requests that are copilot-originated
    // (modelId starts with "copilot/" or resolved model is present)
    let is_copilot = resolved_model.is_some()
        || model_id_raw
            .map(|m| m.starts_with("copilot/"))
            .unwrap_or(false);
    if !is_copilot {
        return None;
    }

    let provider_id = inferred_provider_from_model(&model_id)
        .unwrap_or("github-copilot")
        .to_string();

    let reasoning_tokens: i64 = req
        .pointer("/result/metadata/toolCallRounds")
        .and_then(Value::as_array)
        .map(|rounds| {
            rounds
                .iter()
                .filter_map(|r| r.pointer("/thinking/tokens").and_then(Value::as_i64))
                .sum()
        })
        .unwrap_or(0);

    let tokens = TokenBreakdown {
        input: prompt_tokens.max(0),
        output: completion_tokens.max(0),
        cache_read: 0,
        cache_write: 0,
        reasoning: reasoning_tokens.max(0),
    };

    let dedup_key = format!("copilot-vscode:{}:{}", session_id, timestamp_ms);

    let mut message = UnifiedMessage::new_with_dedup(
        "copilot",
        model_id,
        provider_id,
        session_id.to_string(),
        timestamp_ms,
        tokens,
        0.0,
        Some(dedup_key),
    );

    if let Some((key, label)) = workspace {
        message.set_workspace(Some(key.clone()), label.clone());
    }

    Some(message)
}

fn read_workspace_for_file(jsonl_path: &Path) -> Option<(String, Option<String>)> {
    // Path: workspaceStorage/{hash}/chatSessions/{uuid}.jsonl
    // workspace.json is at: workspaceStorage/{hash}/workspace.json
    let hash_dir = jsonl_path.parent()?.parent()?;
    let workspace_json = hash_dir.join("workspace.json");

    let contents = std::fs::read_to_string(&workspace_json).ok()?;
    let obj: Value = serde_json::from_str(&contents).ok()?;

    let folder = obj
        .get("folder")
        .and_then(Value::as_str)
        .or_else(|| obj.get("workspace").and_then(Value::as_str))?;

    // folder is a URI like "file:///Users/alice/project"
    let path_str = if let Some(stripped) = folder.strip_prefix("file://") {
        // On Windows "file:///C:/..." → strip "file://" leaving "/C:/..."
        // normalize_workspace_key handles slashes
        stripped
    } else {
        folder
    };

    let workspace_key = normalize_workspace_key(path_str)?;
    let workspace_label = workspace_label_from_key(&workspace_key);
    Some((workspace_key, workspace_label))
}

