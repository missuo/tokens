use anyhow::Result;

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s.to_string(),
    }
}

/// Reads a secret from the platform credential store.
///
/// `service` means different things per platform, because the two stores have
/// different lookup semantics and the same string cannot serve both:
///
/// * macOS: `security find-generic-password -s <service>` matches on the
///   *service attribute alone* and ignores the account, so a bare service name
///   resolves whatever account the item was stored under.
/// * Windows: the Credential Manager has no service attribute. `CredReadW` is
///   an exact match on the full `TargetName` and supports no wildcards, so the
///   caller must pass a complete target name. For anything written by Go's
///   `go-keyring` (which is what `gh` uses) that is `"<service>:<username>"`,
///   never the bare `"<service>"` — see `copilot::gh_wincred_targets`.
pub fn read_keychain(service: &str) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("security")
            .args(["find-generic-password", "-s", service, "-w"])
            .output()?;
        if !out.status.success() {
            anyhow::bail!("Keychain lookup failed for service '{service}'");
        }
        Ok(String::from_utf8(out.stdout)?.trim_end().to_string())
    }

    #[cfg(target_os = "windows")]
    {
        // The caller supplies a full Windows target name here; this helper
        // does not compose one, because the composition rule belongs to
        // whichever library wrote the credential.
        read_wincred(service)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = service;
        anyhow::bail!("Keychain lookup is only available on macOS and Windows");
    }
}

/// Reads a credential blob from the Windows Credential Manager.
///
/// `target` must be the *full* `TargetName` exactly as stored: `CredReadW`
/// with `CRED_TYPE_GENERIC` is an exact lookup with no prefix or wildcard
/// matching, so a target that is one character short simply reports
/// `ERROR_NOT_FOUND`.
#[cfg(target_os = "windows")]
pub(super) fn read_wincred(target: &str) -> Result<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Credentials::{CredFree, CredReadW, CRED_TYPE_GENERIC};

    // CredReadW expects a null-terminated UTF-16 target name.
    let wide: Vec<u16> = OsStr::new(target)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut cred_ptr: *mut windows_sys::Win32::Security::Credentials::CREDENTIALW =
        std::ptr::null_mut();

    // On windows-sys CredReadW returns BOOL (i32); 0 means failure and the
    // error code is in GetLastError (e.g. ERROR_NOT_FOUND when the user never
    // ran `gh auth login`).
    unsafe {
        let ok = CredReadW(wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred_ptr);
        if ok == 0 {
            anyhow::bail!(
                "CredReadW failed for target '{target}': {}",
                std::io::Error::last_os_error()
            );
        }
        if cred_ptr.is_null() {
            anyhow::bail!("CredReadW returned null credential for target '{target}'");
        }
        let cred = &*cred_ptr;
        let blob_size = cred.CredentialBlobSize as usize;
        let blob_ptr = cred.CredentialBlob;
        if blob_ptr.is_null() || blob_size == 0 {
            CredFree(cred_ptr as *const std::ffi::c_void);
            anyhow::bail!("Empty credential blob for target '{target}'");
        }
        let slice = std::slice::from_raw_parts(blob_ptr, blob_size);
        // Decode before freeing, but bind the result so the allocation is
        // released on the error path too.
        let decoded = decode_wincred_blob(slice);
        CredFree(cred_ptr as *const std::ffi::c_void);
        decoded.map_err(|e| anyhow::anyhow!("Credential blob for target '{target}': {e}"))
    }
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
/// Exercises the real Credential Manager. Compiled and run only on Windows,
/// where `.github/workflows/test_coverage.yml` runs the suite as a hard gate —
/// that CI leg is the only place the Windows lookup can actually be executed.
#[cfg(all(test, target_os = "windows"))]
mod windows_wincred_tests {
    use super::read_wincred;
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredWriteW, CREDENTIALW, CRED_PERSIST_SESSION, CRED_TYPE_GENERIC,
    };

    fn wide(value: &str) -> Vec<u16> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Removes the credential on drop, so a failing assertion cannot leave an
    /// entry behind in the runner's (or a developer's) Credential Manager.
    struct TestCredential {
        target: String,
    }

    impl TestCredential {
        /// Returns `None` when the write is refused, so a locked-down runner
        /// skips instead of failing for an unrelated reason.
        fn write(target: &str, secret: &[u8]) -> Option<Self> {
            let mut target_wide = wide(target);
            let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
            cred.Type = CRED_TYPE_GENERIC;
            cred.TargetName = target_wide.as_mut_ptr();
            cred.CredentialBlobSize = secret.len() as u32;
            cred.CredentialBlob = secret.as_ptr() as *mut u8;
            // Session persistence: the entry disappears at logoff even if the
            // Drop below never runs.
            cred.Persist = CRED_PERSIST_SESSION;
            let ok = unsafe { CredWriteW(&cred, 0) };
            if ok == 0 {
                return None;
            }
            Some(Self {
                target: target.to_string(),
            })
        }
    }

    impl Drop for TestCredential {
        fn drop(&mut self) {
            let target_wide = wide(&self.target);
            unsafe {
                CredDeleteW(target_wide.as_ptr(), CRED_TYPE_GENERIC, 0);
            }
        }
    }

    #[test]
    fn credreadw_matches_the_full_target_name_and_not_a_prefix() {
        // Deliberately shaped like the go-keyring composition: `base` stands
        // in for the service name and `composed` for what go-keyring actually
        // writes. Reading `base` must fail — that is #1194 in one assertion.
        let base = format!("tokens-test:{}:gh", std::process::id());
        let composed = format!("{base}:");
        let Some(_guard) = TestCredential::write(&composed, b"gho_wincred_roundtrip") else {
            eprintln!("skipping: CredWriteW was refused on this runner");
            return;
        };

        assert_eq!(
            read_wincred(&composed).unwrap(),
            "gho_wincred_roundtrip",
            "the composed target must read back verbatim"
        );
        assert!(
            read_wincred(&base).is_err(),
            "CredReadW must not resolve a prefix of the stored target"
        );
    }

    #[test]
    fn read_wincred_trims_nul_padding_written_by_other_tools() {
        let target = format!("tokens-test:{}:padded:", std::process::id());
        let Some(_guard) = TestCredential::write(&target, b"gho_padded\0") else {
            eprintln!("skipping: CredWriteW was refused on this runner");
            return;
        };
        assert_eq!(read_wincred(&target).unwrap(), "gho_padded");
    }

    #[test]
    fn read_wincred_reports_a_missing_target_as_an_error() {
        let target = format!("tokens-test:{}:absent:", std::process::id());
        assert!(read_wincred(&target).is_err());
    }
}
