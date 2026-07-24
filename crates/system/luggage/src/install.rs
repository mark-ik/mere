// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark AB (markik) — luggage fork
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! Downloading, verifying, and installing an update.
//!
//! Verification order: BLAKE3 (when the manifest carries a digest), then the
//! minisign signature, always. Install mechanics per platform are
//! upstream's: NSIS/MSI on Windows, AppImage swap on Linux, `.app` swap on
//! macOS. The staged-swap improvement is a planned divergence, not yet here.

use std::{
    io::{Cursor, Read},
    path::PathBuf,
    time::Duration,
};

use base64::Engine;
use http::header::{ACCEPT, USER_AGENT};
use minisign_verify::{PublicKey, Signature};
use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderValue},
};
use time::OffsetDateTime;
use url::Url;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::release::UpdateFormat;

/// An update found by [`crate::Updater::check`], ready to download and
/// install.
#[derive(Debug, Clone)]
pub struct Update {
    /// Config used to check for this update.
    pub config: Config,
    /// Update description.
    pub body: Option<String>,
    /// Version used to check for the update.
    pub current_version: String,
    /// Version announced.
    pub version: String,
    /// Update publish date.
    pub date: Option<OffsetDateTime>,
    /// Target.
    pub target: String,
    /// Extract path.
    pub extract_path: PathBuf,
    /// Download URL announced. `file://` URLs (directory feeds) are read
    /// from the filesystem; everything else is fetched over HTTP.
    pub download_url: Url,
    /// Minisign signature announced.
    pub signature: String,
    /// BLAKE3 hex digest announced, verified before the signature when
    /// present.
    pub blake3: Option<String>,
    /// Request timeout.
    pub timeout: Option<Duration>,
    /// Request headers.
    pub headers: HeaderMap,
    /// Update format.
    pub format: UpdateFormat,
}

impl Update {
    /// Download the update, verify it, and return its bytes.
    ///
    /// Use [`Update::install`] to install it.
    pub fn download(&self) -> Result<Vec<u8>> {
        self.download_inner(None::<Box<dyn Fn(usize, Option<u64>)>>, None::<Box<dyn FnOnce()>>)
    }

    /// [`Update::download`] with progress callbacks: the first runs per
    /// received chunk, the second once when the download finishes.
    pub fn download_extended<C: Fn(usize, Option<u64>), D: FnOnce()>(
        &self,
        on_chunk: C,
        on_download_finish: D,
    ) -> Result<Vec<u8>> {
        self.download_inner(Some(on_chunk), Some(on_download_finish))
    }

    fn download_inner<C: Fn(usize, Option<u64>), D: FnOnce()>(
        &self,
        on_chunk: Option<C>,
        on_download_finish: Option<D>,
    ) -> Result<Vec<u8>> {
        let buffer = if self.download_url.scheme() == "file" {
            // A directory feed's artifact: read it straight off the
            // filesystem. The same verification below applies unchanged.
            let path = self
                .download_url
                .to_file_path()
                .map_err(|_| Error::Network(format!("bad file url: {}", self.download_url)))?;
            let bytes = std::fs::read(&path)?;
            if let Some(on_chunk) = &on_chunk {
                on_chunk(bytes.len(), Some(bytes.len() as u64));
            }
            bytes
        } else {
            self.download_http(on_chunk)?
        };
        if let Some(on_download_finish) = on_download_finish {
            on_download_finish();
        }

        // BLAKE3 first when the manifest promises one: cheap, and a mismatch
        // is a better error than a signature failure.
        if let Some(expected) = &self.blake3 {
            let actual = blake3::hash(&buffer).to_hex().to_string();
            if !expected.eq_ignore_ascii_case(&actual) {
                return Err(Error::Blake3Mismatch {
                    expected: expected.clone(),
                    actual,
                });
            }
        }

        let mut update_buffer = Cursor::new(&buffer);
        verify_signature(&mut update_buffer, &self.signature, &self.config.pubkey)?;

        Ok(buffer)
    }

    fn download_http<C: Fn(usize, Option<u64>)>(&self, on_chunk: Option<C>) -> Result<Vec<u8>> {
        let mut headers = self.headers.clone();
        if !headers.contains_key(ACCEPT) {
            headers.insert(
                ACCEPT,
                HeaderValue::from_str("application/octet-stream").unwrap(),
            );
        }
        if !headers.contains_key(USER_AGENT) {
            headers.insert(USER_AGENT, HeaderValue::from_str("luggage").unwrap());
        }

        let mut request = Client::new()
            .get(self.download_url.clone())
            .headers(headers);
        if let Some(timeout) = self.timeout {
            request = request.timeout(timeout);
        }

        struct DownloadProgress<R, C: Fn(usize, Option<u64>)> {
            content_length: Option<u64>,
            inner: R,
            on_chunk: Option<C>,
        }

        impl<R: Read, C: Fn(usize, Option<u64>)> Read for DownloadProgress<R, C> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.inner.read(buf).inspect(|&n| {
                    if let Some(on_chunk) = &self.on_chunk {
                        (on_chunk)(n, self.content_length);
                    }
                })
            }
        }

        let response = request.send()?;
        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "Download request failed with status: {}",
                response.status()
            )));
        }

        let mut source = DownloadProgress {
            content_length: response
                .headers()
                .get("Content-Length")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok()),
            inner: response,
            on_chunk,
        };

        let mut buffer = Vec::new();
        std::io::copy(&mut source, &mut buffer)?;
        Ok(buffer)
    }

    /// Install the update package downloaded by [`Update::download`].
    pub fn install(&self, bytes: Vec<u8>) -> Result<()> {
        self.install_inner(bytes)
    }

    /// Download, verify, and install.
    pub fn download_and_install(&self) -> Result<()> {
        let bytes = self.download()?;
        self.install(bytes)
    }

    /// [`Update::download_and_install`] with progress callbacks.
    pub fn download_and_install_extended<C: Fn(usize, Option<u64>), D: FnOnce()>(
        &self,
        on_chunk: C,
        on_download_finish: D,
    ) -> Result<()> {
        let bytes = self.download_extended(on_chunk, on_download_finish)?;
        self.install(bytes)
    }

    // Windows
    //
    // ### Expected installers:
    // │── [AppName]_[version]_x64.msi        # Application MSI
    // │── [AppName]_[version]_x64-setup.exe  # NSIS installer
    // └── ...
    #[cfg(windows)]
    fn install_inner(&self, bytes: Vec<u8>) -> Result<()> {
        use std::{io::Write, os::windows::process::CommandExt, process::Command};

        use cargo_packager_utils::current_exe::current_exe;

        let extension = match self.format {
            UpdateFormat::Nsis => ".exe",
            UpdateFormat::Wix => ".msi",
            _ => return Err(Error::UnsupportedUpdateFormat),
        };

        let mut temp_file = tempfile::Builder::new().suffix(extension).tempfile()?;
        temp_file.write_all(&bytes)?;
        let (f, path) = temp_file.keep()?;
        drop(f);

        let system_root = std::env::var("SYSTEMROOT");
        let powershell_path = system_root.as_ref().map_or_else(
            |_| "powershell.exe".to_string(),
            |p| format!("{p}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"),
        );

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        // Two file types are supported, exe and msi; an exe is expected to
        // be an installer, not a runtime.
        match self.format {
            UpdateFormat::Nsis => {
                // Wrap the installer path in quotes for Start-Process.
                let mut installer_path = std::ffi::OsString::new();
                installer_path.push("\"");
                installer_path.push(&path);
                installer_path.push("\"");

                let installer_args = self
                    .config
                    .windows
                    .as_ref()
                    .and_then(|w| w.installer_args.clone())
                    .unwrap_or_default();
                let installer_args = [
                    self.config
                        .windows
                        .as_ref()
                        .and_then(|w| w.install_mode.clone())
                        .unwrap_or_default()
                        .nsis_args(),
                    installer_args
                        .iter()
                        .map(AsRef::as_ref)
                        .collect::<Vec<_>>()
                        .as_slice(),
                ]
                .concat();

                let mut cmd = Command::new(powershell_path);
                cmd.creation_flags(CREATE_NO_WINDOW)
                    .args(["-NoProfile", "-WindowStyle", "Hidden"])
                    .args(["Start-Process"])
                    .arg(installer_path);
                if !installer_args.is_empty() {
                    cmd.arg("-ArgumentList").arg(installer_args.join(", "));
                }
                cmd.spawn().expect("installer failed to start");

                std::process::exit(0);
            }
            UpdateFormat::Wix => {
                // Wrap the current exe path in quotes for Start-Process.
                let mut current_exe_arg = std::ffi::OsString::new();
                current_exe_arg.push("\"");
                current_exe_arg.push(current_exe()?);
                current_exe_arg.push("\"");

                let mut msi_path = std::ffi::OsString::new();
                msi_path.push("\"\"\"");
                msi_path.push(&path);
                msi_path.push("\"\"\"");

                let installer_args = self
                    .config
                    .windows
                    .as_ref()
                    .and_then(|w| w.installer_args.clone())
                    .unwrap_or_default();
                let installer_args = [
                    self.config
                        .windows
                        .as_ref()
                        .and_then(|w| w.install_mode.clone())
                        .unwrap_or_default()
                        .msiexec_args(),
                    installer_args
                        .iter()
                        .map(AsRef::as_ref)
                        .collect::<Vec<_>>()
                        .as_slice(),
                ]
                .concat();

                // Run the installer and relaunch the application.
                let powershell_install_res = Command::new(powershell_path)
                    .creation_flags(CREATE_NO_WINDOW)
                    .args(["-NoProfile", "-WindowStyle", "Hidden"])
                    .args([
                        "Start-Process",
                        "-Wait",
                        "-FilePath",
                        "$env:SYSTEMROOT\\System32\\msiexec.exe",
                        "-ArgumentList",
                    ])
                    .arg("/i,")
                    .arg(&msi_path)
                    .arg(format!(", {}, /promptrestart;", installer_args.join(", ")))
                    .arg("Start-Process")
                    .arg(current_exe_arg)
                    .spawn();
                if powershell_install_res.is_err() {
                    // Fallback to running msiexec directly (no relaunch), in
                    // case powershell is unavailable.
                    let msiexec_path = system_root.as_ref().map_or_else(
                        |_| "msiexec.exe".to_string(),
                        |p| format!("{p}\\System32\\msiexec.exe"),
                    );
                    let _ = Command::new(msiexec_path)
                        .arg("/i")
                        .arg(msi_path)
                        .args(installer_args)
                        .arg("/promptrestart")
                        .spawn();
                }

                std::process::exit(0);
            }
            _ => unreachable!(),
        }
    }

    // Linux (AppImage)
    //
    // ### Expected structure:
    // ├── [AppName]_[version]_amd64.AppImage.tar.gz   # GZ generated by cargo-packager
    // │   └──[AppName]_[version]_amd64.AppImage       # Application AppImage
    // └── ...
    //
    // An AppImage is already installed; the new one swaps in via rename, so
    // the temp dir must share the AppImage's mount point.
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    fn install_inner(&self, bytes: Vec<u8>) -> Result<()> {
        use std::fs;

        match self.format {
            UpdateFormat::AppImage => {}
            _ => return Err(Error::UnsupportedUpdateFormat),
        };

        let extract_path_metadata = self.extract_path.metadata()?;
        let tmp_dir_locations = vec![
            Box::new(|| Some(std::env::temp_dir())) as Box<dyn FnOnce() -> Option<PathBuf>>,
            Box::new(dirs::cache_dir),
            Box::new(|| Some(self.extract_path.parent().unwrap().to_path_buf())),
        ];

        for tmp_dir_location in tmp_dir_locations {
            if let Some(tmp_dir_root) = tmp_dir_location() {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};

                let tmp_dir = tempfile::Builder::new()
                    .prefix("current_app")
                    .tempdir_in(tmp_dir_root)?;
                let tmp_dir_metadata = tmp_dir.path().metadata()?;

                if extract_path_metadata.dev() == tmp_dir_metadata.dev() {
                    let mut perms = tmp_dir_metadata.permissions();
                    perms.set_mode(0o700);
                    fs::set_permissions(&tmp_dir, perms)?;

                    let tmp_app_image = tmp_dir.path().join("current_app.AppImage");

                    // Metadata to restore afterward.
                    let metadata = self.extract_path.metadata()?;

                    // Back up the current AppImage.
                    fs::rename(&self.extract_path, &tmp_app_image)?;

                    // If writing fails, restore the previous AppImage.
                    if let Err(err) = fs::write(&self.extract_path, bytes).and_then(|_| {
                        fs::set_permissions(&self.extract_path, metadata.permissions())
                    }) {
                        fs::rename(tmp_app_image, &self.extract_path)?;
                        return Err(err.into());
                    }

                    return Ok(());
                }
            }
        }

        Err(Error::TempDirNotOnSameMountPoint)
    }

    // MacOS
    //
    // ### Expected structure:
    // ├── [AppName]_[version]_x64.app.tar.gz   # GZ generated by cargo-packager
    // │   └──[AppName].app                     # Main application
    // │      └── Contents                      # Application contents...
    // └── ...
    #[cfg(target_os = "macos")]
    fn install_inner(&self, bytes: Vec<u8>) -> Result<()> {
        use flate2::read::GzDecoder;

        let cursor = Cursor::new(bytes);

        // Temp directories for backup and extraction.
        let tmp_backup_dir = tempfile::Builder::new()
            .prefix("luggage_current_app")
            .tempdir()?;
        let tmp_extract_dir = tempfile::Builder::new()
            .prefix("luggage_updated_app")
            .tempdir()?;

        let decoder = GzDecoder::new(cursor);
        let mut archive = tar::Archive::new(decoder);

        // Extract to the temporary directory.
        for entry in archive.entries()? {
            let mut entry = entry?;
            let collected_path: PathBuf = entry.path()?.iter().skip(1).collect();
            let extraction_path = tmp_extract_dir.path().join(&collected_path);

            if let Some(parent) = extraction_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if let Err(err) = entry.unpack(&extraction_path) {
                std::fs::remove_dir_all(tmp_extract_dir.path()).ok();
                return Err(err.into());
            }
        }

        // Move the current app aside.
        let move_result = std::fs::rename(
            &self.extract_path,
            tmp_backup_dir.path().join("current_app"),
        );
        let need_authorization = if let Err(err) = move_result {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                true
            } else {
                std::fs::remove_dir_all(tmp_extract_dir.path()).ok();
                return Err(err.into());
            }
        } else {
            false
        };

        if need_authorization {
            log::debug!("app installation needs admin privileges");
            // AppleScript performs the moves with admin privileges.
            let apple_script = format!(
                "do shell script \"rm -rf '{src}' && mv -f '{new}' '{src}'\" with administrator privileges",
                src = self.extract_path.display(),
                new = tmp_extract_dir.path().display()
            );

            let res = std::process::Command::new("osascript")
                .arg("-e")
                .arg(apple_script)
                .status();

            if res.is_err() || !res.unwrap().success() {
                log::error!("failed to install update using AppleScript");
                std::fs::remove_dir_all(tmp_extract_dir.path()).ok();
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Failed to move the new app into place",
                )));
            }
        } else {
            if self.extract_path.exists() {
                std::fs::remove_dir_all(&self.extract_path)?;
            }
            std::fs::rename(tmp_extract_dir.path(), &self.extract_path)?;
        }

        let _ = std::process::Command::new("touch")
            .arg(&self.extract_path)
            .status();

        Ok(())
    }
}

// Validate the minisign signature.
//
// NOTE: the buffer position is not reset.
fn verify_signature<R>(archive_reader: &mut R, release_signature: &str, pub_key: &str) -> Result<bool>
where
    R: Read,
{
    let pub_key_decoded = base64_to_string(pub_key)?;
    let public_key = PublicKey::decode(&pub_key_decoded)?;
    let signature_base64_decoded = base64_to_string(release_signature)?;
    let signature = Signature::decode(&signature_base64_decoded)?;

    let mut data = Vec::new();
    archive_reader.read_to_end(&mut data)?;

    public_key.verify(&data, &signature, true)?;
    Ok(true)
}

fn base64_to_string(base64_string: &str) -> Result<String> {
    let decoded_string = &base64::engine::general_purpose::STANDARD.decode(base64_string)?;
    let result = std::str::from_utf8(decoded_string)
        .map_err(|_| Error::SignatureUtf8(base64_string.into()))?
        .to_string();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Feed;
    use crate::release::MANIFEST_NAME;
    use crate::{Config, UpdaterBuilder};
    use std::io::Write as _;

    /// Sign bytes with a fresh in-process minisign key, returning the
    /// base64-of-boxed forms download() expects for (pubkey, signature).
    fn sign(data: &[u8]) -> (String, String) {
        let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let signature = minisign::sign(
            None,
            &keypair.sk,
            std::io::Cursor::new(data),
            None,
            None,
        )
        .unwrap();
        let engine = base64::engine::general_purpose::STANDARD;
        (
            engine.encode(keypair.pk.to_box().unwrap().to_string()),
            engine.encode(signature.to_string()),
        )
    }

    fn file_url(path: &std::path::Path) -> String {
        Url::from_file_path(path).unwrap().to_string()
    }

    /// Build an Update via a real directory feed + manifest.
    fn update_for(dir: &std::path::Path, artifact: &[u8], blake3: Option<String>) -> Update {
        let artifact_path = dir.join("artifact.exe");
        std::fs::File::create(&artifact_path)
            .unwrap()
            .write_all(artifact)
            .unwrap();
        let (pubkey, signature) = sign(artifact);
        let manifest = serde_json::json!({
            "version": "0.2.0",
            "platforms": {
                "test-target": {
                    "url": file_url(&artifact_path),
                    "signature": signature,
                    "blake3": blake3,
                    "format": "nsis",
                }
            }
        });
        std::fs::write(
            dir.join(MANIFEST_NAME),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        UpdaterBuilder::new(
            "0.1.0".parse().unwrap(),
            Config {
                feeds: vec![Feed::Directory(dir.to_path_buf())],
                pubkey,
                windows: None,
            },
        )
        .target("test-target".to_string())
        .executable_path(dir.join("fake-exe"))
        .build()
        .unwrap()
        .check()
        .unwrap()
        .expect("update should be offered")
    }

    #[test]
    fn download_verifies_blake3_and_signature_from_a_directory_feed() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = b"pretend installer bytes";
        let digest = blake3::hash(artifact).to_hex().to_string();
        let update = update_for(dir.path(), artifact, Some(digest));
        let bytes = update.download().expect("verified download");
        assert_eq!(bytes, artifact);
    }

    #[test]
    fn a_wrong_blake3_fails_before_the_signature_check() {
        let dir = tempfile::tempdir().unwrap();
        let update = update_for(dir.path(), b"real bytes", Some("00".repeat(32)));
        let err = update.download().unwrap_err();
        assert!(matches!(err, Error::Blake3Mismatch { .. }), "got: {err}");
    }

    #[test]
    fn a_manifest_without_blake3_still_verifies_the_signature() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = b"unsummed but signed";
        let update = update_for(dir.path(), artifact, None);
        assert_eq!(update.download().unwrap(), artifact);
    }

    #[test]
    fn tampered_bytes_fail_the_signature() {
        let dir = tempfile::tempdir().unwrap();
        let update = update_for(dir.path(), b"original bytes", None);
        // Overwrite the artifact after signing.
        let artifact_path = dir.path().join("artifact.exe");
        std::fs::write(&artifact_path, b"tampered bytes!").unwrap();
        let err = update.download().unwrap_err();
        assert!(matches!(err, Error::Minisign(_)), "got: {err}");
    }
}
