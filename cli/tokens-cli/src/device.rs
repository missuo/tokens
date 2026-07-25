use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const DEVICE_FILE_NAME: &str = "device.json";
const DEVICE_ID_ENV: &str = "TOKENS_DEVICE_ID";
const DEVICE_NAME_ENV: &str = "TOKENS_DEVICE_NAME";
const MAX_DEVICE_ID_LEN: usize = 96;
const MAX_DEVICE_NAME_LEN: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubmitDevice {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitDeviceSource {
    Environment,
    ConfigFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitDeviceInspection {
    pub device: Option<SubmitDevice>,
    pub source: Option<SubmitDeviceSource>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredDevice {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    created_at: String,
}

pub fn resolve_submit_device() -> Result<SubmitDevice> {
    if let Some(id) = env_value(DEVICE_ID_ENV) {
        return Ok(SubmitDevice {
            id: validate_device_id(&id)?,
            name: env_value(DEVICE_NAME_ENV)
                .map(|name| validate_device_name(&name))
                .transpose()?,
        });
    }

    let path = device_file_path();
    let name_override = env_value(DEVICE_NAME_ENV)
        .map(|name| validate_device_name(&name))
        .transpose()?;

    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let stored: StoredDevice = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        return Ok(SubmitDevice {
            id: validate_device_id(&stored.id)?,
            name: name_override.or(stored.name),
        });
    }

    let stored = StoredDevice {
        id: format!("dev_{}", Uuid::new_v4().simple()),
        name: name_override,
        created_at: Utc::now().to_rfc3339(),
    };
    write_stored_device(&path, &stored)?;

    Ok(SubmitDevice {
        id: stored.id,
        name: stored.name,
    })
}

pub fn inspect_submit_device() -> Result<SubmitDeviceInspection> {
    let path = device_file_path();

    if let Some(id) = env_value(DEVICE_ID_ENV) {
        return Ok(SubmitDeviceInspection {
            device: Some(SubmitDevice {
                id: validate_device_id(&id)?,
                name: env_value(DEVICE_NAME_ENV)
                    .map(|name| validate_device_name(&name))
                    .transpose()?,
            }),
            source: Some(SubmitDeviceSource::Environment),
            path,
        });
    }

    if !path.exists() {
        return Ok(SubmitDeviceInspection {
            device: None,
            source: None,
            path,
        });
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let stored: StoredDevice = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let name_override = env_value(DEVICE_NAME_ENV)
        .map(|name| validate_device_name(&name))
        .transpose()?;

    Ok(SubmitDeviceInspection {
        device: Some(SubmitDevice {
            id: validate_device_id(&stored.id)?,
            name: name_override.or(stored.name),
        }),
        source: Some(SubmitDeviceSource::ConfigFile),
        path,
    })
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn device_file_path() -> PathBuf {
    crate::paths::get_config_dir().join(DEVICE_FILE_NAME)
}

fn validate_device_id(id: &str) -> Result<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("{} must not be empty", DEVICE_ID_ENV));
    }
    if trimmed.len() > MAX_DEVICE_ID_LEN {
        return Err(anyhow!(
            "{} must be at most {} characters",
            DEVICE_ID_ENV,
            MAX_DEVICE_ID_LEN
        ));
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
    {
        return Err(anyhow!(
            "{} may only contain ASCII letters, numbers, '.', '_', '-', or ':'",
            DEVICE_ID_ENV
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_device_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("{} must not be empty", DEVICE_NAME_ENV));
    }
    if trimmed.len() > MAX_DEVICE_NAME_LEN {
        return Err(anyhow!(
            "{} must be at most {} characters",
            DEVICE_NAME_ENV,
            MAX_DEVICE_NAME_LEN
        ));
    }
    Ok(trimmed.to_string())
}

fn write_stored_device(path: &Path, device: &StoredDevice) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(device)?;
    std::fs::write(&tmp_path, content)
        .with_context(|| format!("failed to write {}", tmp_path.display()))?;
    tokens_core::fs_atomic::replace_file(&tmp_path, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

