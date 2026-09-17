use anyhow::Result;

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s.to_string(),
    }
}

pub fn read_keychain(service: &str) -> Result<String> {
    if cfg!(not(target_os = "macos")) {
        anyhow::bail!("Keychain lookup is only available on macOS");
    }
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", service, "-w"])
        .output()?;
    if !out.status.success() {
        anyhow::bail!("Keychain lookup failed for service '{service}'");
    }
    Ok(String::from_utf8(out.stdout)?.trim_end().to_string())
}

pub fn atomic_write_secret(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent directory",
        )
    })?;
    std::fs::create_dir_all(dir)?;
    // Set 0700 unconditionally: this can be the first writer to create the
    // config/cache root, and the `ensure_cache_dir` helpers elsewhere only
    // chmod when they create the directory themselves.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let temp_path = path.with_extension(format!("{}.tmp", std::process::id()));
    {
        #[cfg(unix)]
        let mut opts = {
            use std::os::unix::fs::OpenOptionsExt;
            let mut o = std::fs::OpenOptions::new();
            o.mode(0o600);
            o
        };
        #[cfg(not(unix))]
        let mut opts = std::fs::OpenOptions::new();
        let mut f = match opts.write(true).create_new(true).open(&temp_path) {
            Ok(f) => f,
            Err(e) => {
                let _ = std::fs::remove_file(&temp_path);
                return Err(e);
            }
        };
        if let Err(e) = std::io::Write::write_all(&mut f, data) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e);
        }
    }
    if let Err(e) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }
    Ok(())
}

