use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

const CACHE_TTL_SECS: u64 = 3600;

pub fn get_cache_dir() -> PathBuf {
    crate::paths::get_cache_dir()
}

pub fn get_cache_path(filename: &str) -> PathBuf {
    get_cache_dir().join(filename)
}

#[derive(Serialize, Deserialize)]
pub struct CachedData<T> {
    pub timestamp: u64,
    pub data: T,
}

fn load_cache_with_policy<T: for<'de> Deserialize<'de>>(
    filename: &str,
    allow_stale: bool,
) -> Option<T> {
    let canonical_path = get_cache_path(filename);
    let cached: CachedData<T> = match fs::read_to_string(&canonical_path) {
        Ok(content) => serde_json::from_str(&content).ok()?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            legacy_cache_paths(filename).into_iter().find_map(|path| {
                let content = fs::read_to_string(&path).ok()?;
                serde_json::from_str(&content).ok()
            })?
        }
        Err(_) => return None,
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();

    if cached.timestamp > now {
        return None;
    }

    if !allow_stale && now.saturating_sub(cached.timestamp) > CACHE_TTL_SECS {
        return None;
    }

    Some(cached.data)
}

pub fn load_cache<T: for<'de> Deserialize<'de>>(filename: &str) -> Option<T> {
    load_cache_with_policy(filename, false)
}

pub fn load_cache_any_age<T: for<'de> Deserialize<'de>>(filename: &str) -> Option<T> {
    load_cache_with_policy(filename, true)
}

pub fn save_cache<T: Serialize>(filename: &str, data: &T) -> Result<(), std::io::Error> {
    let dir = get_cache_dir();
    fs::create_dir_all(&dir)?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_secs();

    let cached = CachedData {
        timestamp: now,
        data,
    };
    let content = serde_json::to_string(&cached)?;

    let final_path = get_cache_path(filename);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let tmp_filename = format!(".{}.{}.{:x}.tmp", filename, std::process::id(), nanos);
    let tmp_path = dir.join(&tmp_filename);

    use std::io::Write;
    // INVARIANT: All cache writes use atomic temp-file rename. NEVER delete
    // the canonical cache file before writing — a partial save or process
    // crash between delete and rename would lose the cache. The temp-file
    // pattern makes corruption-on-crash impossible.
    let write_result = (|| {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        crate::fs_atomic::replace_file(&tmp_path, &final_path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }

    write_result
}

fn legacy_cache_paths(filename: &str) -> Vec<PathBuf> {
    if crate::paths::is_config_dir_overridden() {
        return Vec::new();
    }

    [
        crate::paths::legacy_dirs_cache_dir().map(|d| d.join(filename)),
        crate::paths::legacy_dot_cache_dir().map(|d| d.join(filename)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

