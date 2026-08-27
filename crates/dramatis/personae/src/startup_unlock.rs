// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Startup unlock policy and local auto-unlock root storage.
//!
//! The sealed-record backend needs an already-unlocked 32-byte root key. This
//! module owns the host-startup side of that question: what mode is the app in
//! at launch, and when `AutoOs` is chosen, how do we obtain a local device-only
//! root key without inventing a second passphrase flow.
//!
//! Today only `AutoOs` has an implementation, and only on Windows. It stores a
//! random 32-byte local vault root wrapped with DPAPI in a small JSON file.
//! Other platforms, and the `Prompt` / `Locked` modes, stay declared but
//! unresolved so callers can surface an honest degraded startup state rather
//! than silently falling back to plaintext.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::IdentityError;

#[cfg(windows)]
const AUTO_UNLOCK_KEY_FILE_VERSION: u8 = 1;

/// Startup unlock policy for the identity vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StartupUnlockMode {
    /// Use OS-protected local storage to auto-unlock the local vault root.
    #[default]
    AutoOs,
    /// Require an explicit passphrase prompt during startup.
    Prompt,
    /// Start locked and bring sync/private lanes up after an explicit unlock.
    Locked,
}

/// Whether the current build has a real `AutoOs` backend for explicit local unlocks.
pub fn auto_unlock_backend_available() -> bool {
    cfg!(windows)
}

#[cfg(windows)]
#[derive(Debug, Serialize, Deserialize)]
struct AutoUnlockKeyFile {
    version: u8,
    backend: String,
    wrapped_key: Vec<u8>,
}

/// Load or create the local auto-unlock root key for `StartupUnlockMode::AutoOs`.
///
/// Returns `Ok(None)` on platforms where `AutoOs` is not implemented yet.
pub fn load_or_create_auto_unlock_root(
    path: impl Into<PathBuf>,
) -> Result<Option<[u8; 32]>, IdentityError> {
    let path = path.into();
    load_or_create_auto_unlock_root_impl(&path)
}

#[cfg(not(windows))]
fn load_or_create_auto_unlock_root_impl(_path: &Path) -> Result<Option<[u8; 32]>, IdentityError> {
    Ok(None)
}

#[cfg(windows)]
fn load_or_create_auto_unlock_root_impl(path: &Path) -> Result<Option<[u8; 32]>, IdentityError> {
    if path.exists() {
        let bytes = std::fs::read(path).map_err(|err| {
            IdentityError::Backend(format!("read auto-unlock root {:?}: {err}", path))
        })?;
        let file: AutoUnlockKeyFile = serde_json::from_slice(&bytes).map_err(|err| {
            IdentityError::Backend(format!("parse auto-unlock root {:?}: {err}", path))
        })?;
        if file.version != AUTO_UNLOCK_KEY_FILE_VERSION {
            return Err(IdentityError::Backend(format!(
                "unsupported auto-unlock root version {} at {:?}",
                file.version, path
            )));
        }
        let plaintext = dpapi_unprotect(&file.wrapped_key)?;
        let root = <[u8; 32]>::try_from(plaintext.as_slice()).map_err(|_| {
            IdentityError::Backend(format!(
                "auto-unlock root {:?} did not decrypt to 32 bytes",
                path
            ))
        })?;
        return Ok(Some(root));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            IdentityError::Backend(format!("create auto-unlock dir {:?}: {err}", parent))
        })?;
    }
    let mut root = [0u8; 32];
    getrandom::fill(&mut root).expect("OS randomness available");
    let wrapped_key = dpapi_protect(&root)?;
    let file = AutoUnlockKeyFile {
        version: AUTO_UNLOCK_KEY_FILE_VERSION,
        backend: "dpapi".to_string(),
        wrapped_key,
    };
    save_json_atomic(path, &file)?;
    Ok(Some(root))
}

#[cfg(windows)]
fn save_json_atomic<T>(path: &Path, value: &T) -> Result<(), IdentityError>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec(value)
        .map_err(|err| IdentityError::Backend(format!("serialize {:?}: {err}", path)))?;
    let parent = path.parent().ok_or_else(|| {
        IdentityError::Backend(format!("auto-unlock path has no parent: {:?}", path))
    })?;
    let tmp = tempfile_in_dir(parent)?;
    std::fs::write(&tmp, &bytes)
        .map_err(|err| IdentityError::Backend(format!("write tmp {:?}: {err}", tmp)))?;
    std::fs::rename(&tmp, path).map_err(|err| {
        IdentityError::Backend(format!("rename tmp {:?} -> {:?}: {err}", tmp, path))
    })?;
    Ok(())
}

#[cfg(windows)]
fn tempfile_in_dir(dir: &Path) -> Result<PathBuf, IdentityError> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| IdentityError::Backend(format!("time: {err}")))?
        .as_nanos();
    let mut rand = [0u8; 8];
    getrandom::fill(&mut rand).expect("OS randomness available");
    let mut hex = String::with_capacity(16);
    for byte in rand {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(dir.join(format!(".auto-unlock-{now}-{hex}.tmp")))
}

#[cfg(windows)]
fn dpapi_protect(plaintext: &[u8]) -> Result<Vec<u8>, IdentityError> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            null_mut(),
            null_mut(),
            null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(IdentityError::Backend("dpapi protect failed".to_string()));
    }
    let wrapped = unsafe {
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let bytes = slice.to_vec();
        LocalFree(output.pbData.cast());
        bytes
    };
    Ok(wrapped)
}

#[cfg(windows)]
fn dpapi_unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, IdentityError> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData};

    let input = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(IdentityError::Backend("dpapi unprotect failed".to_string()));
    }
    let plaintext = unsafe {
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let bytes = slice.to_vec();
        LocalFree(output.pbData.cast());
        bytes
    };
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(windows)]
    #[test]
    fn auto_unlock_root_round_trips_through_dpapi() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault-root.auto.json");
        let first = load_or_create_auto_unlock_root(&path).unwrap().unwrap();
        let second = load_or_create_auto_unlock_root(&path).unwrap().unwrap();
        assert_eq!(first, second);
    }

    #[cfg(not(windows))]
    #[test]
    fn auto_unlock_root_is_absent_without_platform_support() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vault-root.auto.json");
        assert_eq!(load_or_create_auto_unlock_root(&path).unwrap(), None);
    }
}
