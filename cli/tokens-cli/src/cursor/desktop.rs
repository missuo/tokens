use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use anyhow::{Context, Result, ensure};
use base64::Engine;
use std::ffi::{c_char, c_int, c_uint};

const PBKDF2: c_uint = 2;
const HMAC_SHA1: c_uint = 1;
const PREFIX: &str = "cursor-safe-storage:v1:";

unsafe extern "C" {
    fn CCKeyDerivationPBKDF(
        algorithm: c_uint,
        password: *const c_char,
        password_len: usize,
        salt: *const u8,
        salt_len: usize,
        prf: c_uint,
        rounds: c_uint,
        key: *mut u8,
        key_len: usize,
    ) -> c_int;
}

pub(super) fn decode(token: String) -> Result<String> {
    let Some(encoded) = token.strip_prefix(PREFIX) else {
        return Ok(token);
    };
    let password = crate::commands::usage::helpers::read_keychain("Cursor Safe Storage")
        .context("Could not unlock Cursor desktop credentials")?;
    decrypt(encoded, password.as_bytes())
}

fn decrypt(encoded: &str, password: &[u8]) -> Result<String> {
    let mut data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("Invalid Cursor Safe Storage encoding")?;
    ensure!(
        data.starts_with(b"v10"),
        "Unsupported Cursor Safe Storage format"
    );
    let salt = b"saltysalt";
    let mut key = [0; 16];
    // Electron's macOS Safe Storage uses PBKDF2-HMAC-SHA1 and AES-128-CBC.
    // SAFETY: The password, salt and output pointers have the supplied lengths.
    let status = unsafe {
        CCKeyDerivationPBKDF(
            PBKDF2,
            password.as_ptr().cast(),
            password.len(),
            salt.as_ptr(),
            salt.len(),
            HMAC_SHA1,
            1003,
            key.as_mut_ptr(),
            key.len(),
        )
    };
    ensure!(status == 0, "Could not derive Cursor Safe Storage key");
    let plain = cbc::Decryptor::<aes::Aes128>::new(&key.into(), (&[b' '; 16]).into())
        .decrypt_padded_mut::<Pkcs7>(&mut data[3..])
        .map_err(|_| anyhow::anyhow!("Could not decrypt Cursor desktop credentials"))?;
    Ok(std::str::from_utf8(plain)
        .context("Invalid Cursor desktop credential encoding")?
        .to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_macos_safe_storage() {
        assert_eq!(
            super::decrypt("djEwiSWYXaBIRVFFzU6zHsfmkA==", b"fixture-password").unwrap(),
            "fixture-token"
        );
        assert!(super::decrypt("djEwiSWYXaBIRVFFzU6zHsfmkA==", b"wrong-password").is_err());
    }
}
