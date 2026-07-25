// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark AB (markik) — luggage fork
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! The updater: build one for the running app, then [`Updater::check`] the
//! feeds in order for a newer release.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use cargo_packager_utils::current_exe::current_exe;
use http::{
    HeaderName,
    header::{ACCEPT, USER_AGENT},
};
use percent_encoding::{AsciiSet, CONTROLS};
use reqwest::{
    StatusCode,
    blocking::Client,
    header::{HeaderMap, HeaderValue},
};
use semver::Version;
use url::Url;

use crate::config::{Config, Feed};
use crate::error::{Error, Result};
use crate::install::Update;
use crate::release::{MANIFEST_NAME, RemoteRelease};

/// An [`Updater`] builder.
pub struct UpdaterBuilder {
    current_version: Version,
    config: Config,
    version_comparator: Option<Box<dyn Fn(Version, RemoteRelease) -> bool + Send + Sync>>,
    executable_path: Option<PathBuf>,
    target: Option<String>,
    headers: HeaderMap,
    timeout: Option<Duration>,
}

impl UpdaterBuilder {
    /// Create a new updater builder.
    pub fn new(current_version: Version, config: Config) -> Self {
        Self {
            current_version,
            config,
            version_comparator: None,
            executable_path: None,
            target: None,
            headers: Default::default(),
            timeout: None,
        }
    }

    /// A custom function to decide whether a remote release counts as new.
    pub fn version_comparator<F: Fn(Version, RemoteRelease) -> bool + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.version_comparator = Some(Box::new(f));
        self
    }

    /// The minisign public key to verify updates with.
    pub fn pub_key(mut self, pub_key: impl Into<String>) -> Self {
        self.config.pubkey = pub_key.into();
        self
    }

    /// The target to request an update for.
    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target.replace(target.into());
        self
    }

    /// The feeds to check.
    pub fn feeds(mut self, feeds: Vec<Feed>) -> Self {
        self.config.feeds = feeds;
        self
    }

    /// The path to the current executable, when it is not the process's own.
    pub fn executable_path<P: AsRef<Path>>(mut self, p: P) -> Self {
        self.executable_path.replace(p.as_ref().into());
        self
    }

    /// Add a header to HTTP feed requests.
    pub fn header<K, V>(mut self, key: K, value: V) -> Result<Self>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        let key: std::result::Result<HeaderName, http::Error> = key.try_into().map_err(Into::into);
        let value: std::result::Result<HeaderValue, http::Error> =
            value.try_into().map_err(Into::into);
        self.headers.insert(key?, value?);
        Ok(self)
    }

    /// A timeout for HTTP feed requests.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Custom installer args on Windows.
    pub fn installer_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if self.config.windows.is_none() {
            self.config.windows.replace(Default::default());
        }
        self.config
            .windows
            .as_mut()
            .unwrap()
            .installer_args
            .replace(args.into_iter().map(Into::into).collect());
        self
    }

    /// Build the updater.
    pub fn build(self) -> Result<Updater> {
        if self.config.feeds.is_empty() {
            return Err(Error::EmptyFeeds);
        };

        let arch = get_updater_arch().ok_or(Error::UnsupportedArch)?;
        let (target, json_target) = if let Some(target) = self.target {
            (target.clone(), target)
        } else {
            let target = get_updater_target().ok_or(Error::UnsupportedOs)?;
            (target.to_string(), format!("{target}-{arch}"))
        };

        let executable_path = match self.executable_path {
            Some(p) => p,
            #[cfg(not(any(windows, target_os = "macos")))]
            None => {
                if let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
                    appimage
                } else {
                    current_exe()?
                }
            }
            #[cfg(any(windows, target_os = "macos"))]
            _ => current_exe()?,
        };

        // Get the extract_path from the provided executable_path.
        #[cfg(any(windows, target_os = "macos"))]
        let extract_path = extract_path_from_executable(&executable_path)?;
        #[cfg(not(any(windows, target_os = "macos")))]
        let extract_path = executable_path;

        Ok(Updater {
            config: self.config,
            current_version: self.current_version,
            version_comparator: self.version_comparator,
            timeout: self.timeout,
            arch,
            target,
            json_target,
            headers: self.headers,
            extract_path,
        })
    }
}

/// A built updater; see [`UpdaterBuilder`].
pub struct Updater {
    config: Config,
    current_version: Version,
    version_comparator: Option<Box<dyn Fn(Version, RemoteRelease) -> bool + Send + Sync>>,
    timeout: Option<Duration>,
    arch: &'static str,
    /// The `{{target}}` variable value for endpoint templating.
    target: String,
    /// The key looked up in a manifest's `platforms` object.
    json_target: String,
    headers: HeaderMap,
    extract_path: PathBuf,
}

impl Updater {
    /// Check the feeds in order. `Ok(None)` means no update is available.
    pub fn check(&self) -> Result<Option<Update>> {
        let mut remote_release: Option<RemoteRelease> = None;
        let mut last_error: Option<Error> = None;

        for feed in &self.config.feeds {
            let result = match feed {
                Feed::Http(url) => self.fetch_http(url.clone()),
                Feed::GitHub { owner, repo } => {
                    let url: Url = Feed::github_manifest_url(owner, repo).parse()?;
                    self.fetch_http(url)
                }
                Feed::Directory(dir) => self.fetch_directory(dir),
            };
            match result {
                // A feed can answer "definitively no update" (HTTP 204).
                Ok(None) => return Ok(None),
                Ok(Some(release)) => {
                    last_error = None;
                    remote_release = Some(release);
                    break;
                }
                Err(err) => {
                    log::error!("feed {feed} failed: {err}");
                    last_error = Some(err);
                }
            }
        }

        if let Some(error) = last_error {
            return Err(error);
        }
        let release = remote_release.ok_or(Error::ReleaseNotFound)?;

        let should_update = match self.version_comparator.as_ref() {
            Some(comparator) => comparator(self.current_version.clone(), release.clone()),
            None => release.version > self.current_version,
        };

        let update = if should_update {
            Some(Update {
                current_version: self.current_version.to_string(),
                config: self.config.clone(),
                target: self.target.clone(),
                extract_path: self.extract_path.clone(),
                version: release.version.to_string(),
                date: release.pub_date,
                download_url: release.download_url(&self.json_target)?.to_owned(),
                body: release.notes.clone(),
                signature: release.signature(&self.json_target)?.to_owned(),
                blake3: release.blake3(&self.json_target)?,
                timeout: self.timeout,
                headers: self.headers.clone(),
                format: release.format(&self.json_target)?,
            })
        } else {
            None
        };

        Ok(update)
    }

    /// Check a manifest's own signature before anything in it is believed.
    ///
    /// The artifact signature cannot cover the version and URL announced
    /// around it, so without this a feed controller can replay an old signed
    /// build under a new version number. `signature` is the contents of the
    /// feed's `luggage.json.sig`, or `None` when the feed had none.
    fn verify_manifest(&self, bytes: &[u8], signature: Option<&str>) -> Result<()> {
        match signature {
            Some(signature) => {
                crate::signing::verify_bytes(
                    bytes,
                    &crate::signing::wrap_signature_file(signature),
                    &self.config.pubkey,
                )
                .map_err(|_| Error::ManifestSignatureInvalid)
            }
            None if self.config.require_signed_manifest => Err(Error::ManifestUnsigned),
            // Explicitly opted out: the caller accepted the downgrade risk.
            None => {
                log::warn!("feed manifest is unsigned and verification is disabled");
                Ok(())
            }
        }
    }

    /// Fetch a release from an HTTP feed. `Ok(None)` is a definitive
    /// 204 No Content.
    fn fetch_http(&self, url: Url) -> Result<Option<RemoteRelease>> {
        let mut headers = self.headers.clone();
        if !headers.contains_key(ACCEPT) {
            headers.insert(ACCEPT, HeaderValue::from_str("application/json").unwrap());
        }
        if !headers.contains_key(USER_AGENT) {
            headers.insert(USER_AGENT, HeaderValue::from_str("luggage").unwrap());
        }

        let version = self.current_version.to_string();
        const CONTROLS_ADD: &AsciiSet = &CONTROLS.add(b'+');
        let encoded_version =
            percent_encoding::percent_encode(version.as_bytes(), CONTROLS_ADD).to_string();

        // Replace {{current_version}}, {{target}} and {{arch}}, in both the
        // raw and the url-encoded spellings (Url encodes path components).
        let url: Url = url
            .to_string()
            .replace("%7B%7Bcurrent_version%7D%7D", &encoded_version)
            .replace("%7B%7Btarget%7D%7D", &self.target)
            .replace("%7B%7Barch%7D%7D", self.arch)
            .replace("{{current_version}}", &encoded_version)
            .replace("{{target}}", &self.target)
            .replace("{{arch}}", self.arch)
            .parse()?;

        log::debug!("checking for updates at {url}");

        let signature_url: Url = format!("{url}.sig").parse()?;
        let mut request = Client::new().get(url).headers(headers);
        if let Some(timeout) = self.timeout {
            request = request.timeout(timeout);
        }
        let response = request.send()?;

        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "update feed did not respond with a successful status code: {}",
                response.status()
            )));
        }
        if response.status() == StatusCode::NO_CONTENT {
            log::debug!("update feed returned 204 No Content");
            return Ok(None);
        }

        // Take the bytes, not the parsed JSON: the signature is over exactly
        // what the feed served, and re-serializing would not reproduce it.
        let manifest_bytes = response.bytes()?;

        // Fetch the detached signature from `<url>.sig`. A feed that has none
        // is refused unless the caller opted out.
        let signature = self.fetch_manifest_signature(&signature_url);
        self.verify_manifest(&manifest_bytes, signature.as_deref())?;

        log::debug!("update response verified, {} bytes", manifest_bytes.len());
        Ok(Some(serde_json::from_slice::<RemoteRelease>(
            &manifest_bytes,
        )?))
    }

    /// Best-effort fetch of a detached manifest signature.
    ///
    /// A missing signature is `None` rather than an error here;
    /// [`Self::verify_manifest`] decides whether that is acceptable, so the
    /// policy lives in one place.
    fn fetch_manifest_signature(&self, url: &Url) -> Option<String> {
        let mut request = Client::new().get(url.clone());
        if let Some(timeout) = self.timeout {
            request = request.timeout(timeout);
        }
        match request.send() {
            Ok(response) if response.status().is_success() => response.text().ok(),
            Ok(response) => {
                log::debug!("no manifest signature at {url}: {}", response.status());
                None
            }
            Err(err) => {
                log::debug!("no manifest signature at {url}: {err}");
                None
            }
        }
    }

    /// Read a release from a directory feed's manifest.
    fn fetch_directory(&self, dir: &Path) -> Result<Option<RemoteRelease>> {
        let manifest = dir.join(MANIFEST_NAME);
        if !manifest.exists() {
            return Err(Error::ManifestNotFound {
                dir: dir.to_path_buf(),
            });
        }
        let bytes = std::fs::read(&manifest)?;
        let signature = std::fs::read_to_string(dir.join(format!("{MANIFEST_NAME}.sig"))).ok();
        self.verify_manifest(&bytes, signature.as_deref())?;
        log::debug!("read and verified manifest from {}", manifest.display());
        Ok(Some(serde_json::from_slice::<RemoteRelease>(&bytes)?))
    }
}

/// The `<os>-<arch>` updater target for the current platform.
#[doc(hidden)]
pub fn target() -> Option<String> {
    if let (Some(target), Some(arch)) = (get_updater_target(), get_updater_arch()) {
        Some(format!("{target}-{arch}"))
    } else {
        None
    }
}

pub(crate) fn get_updater_target() -> Option<&'static str> {
    if cfg!(target_os = "linux") {
        Some("linux")
    } else if cfg!(target_os = "macos") {
        Some("macos")
    } else if cfg!(target_os = "windows") {
        Some("windows")
    } else {
        None
    }
}

pub(crate) fn get_updater_arch() -> Option<&'static str> {
    if cfg!(target_arch = "x86") {
        Some("i686")
    } else if cfg!(target_arch = "x86_64") {
        Some("x86_64")
    } else if cfg!(target_arch = "arm") {
        Some("armv7")
    } else if cfg!(target_arch = "aarch64") {
        Some("aarch64")
    } else if cfg!(target_arch = "riscv64") {
        Some("riscv64")
    } else {
        None
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn extract_path_from_executable(executable_path: &Path) -> Result<PathBuf> {
    // The path of the current executable's directory by default,
    // e.g. C:\Program Files\My App\
    let extract_path = executable_path
        .parent()
        .map(PathBuf::from)
        .ok_or(Error::FailedToDetermineExtractPath)?;

    // On macOS the binary is at /Applications/App.app/Contents/MacOS/app;
    // the install target is /Applications/App.app.
    #[cfg(target_os = "macos")]
    if extract_path
        .display()
        .to_string()
        .contains("Contents/MacOS")
    {
        return extract_path
            .parent()
            .map(PathBuf::from)
            .ok_or(Error::FailedToDetermineExtractPath)?
            .parent()
            .map(PathBuf::from)
            .ok_or(Error::FailedToDetermineExtractPath);
    }

    Ok(extract_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::{ReleaseManifestPlatform, RemoteReleaseData, UpdateFormat};

    fn updater_for(feeds: Vec<Feed>, current: &str) -> Updater {
        UpdaterBuilder::new(
            current.parse().unwrap(),
            Config {
                feeds,
                pubkey: String::new(),
                windows: None,
            },
        )
        .target("test-target".to_string())
        .executable_path(std::env::temp_dir().join("luggage-test-exe"))
        .build()
        .unwrap()
    }

    fn write_manifest(dir: &Path, version: &str) {
        let manifest = serde_json::json!({
            "version": version,
            "platforms": {
                "test-target": {
                    "url": "file:///tmp/app-artifact",
                    "signature": "c2ln",
                    "blake3": "00",
                    "format": "nsis",
                }
            }
        });
        std::fs::write(
            dir.join(MANIFEST_NAME),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn no_feeds_is_a_build_error() {
        let result = UpdaterBuilder::new(
            "0.1.0".parse().unwrap(),
            Config {
                feeds: vec![],
                pubkey: String::new(),
                windows: None,
            },
        )
        .build();
        assert!(matches!(result, Err(Error::EmptyFeeds)));
    }

    #[test]
    fn directory_feed_finds_a_newer_release() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "0.2.0");
        let updater = updater_for(vec![Feed::Directory(dir.path().to_path_buf())], "0.1.0");
        let update = updater.check().unwrap().expect("update should be offered");
        assert_eq!(update.version, "0.2.0");
        assert_eq!(update.blake3.as_deref(), Some("00"));
        assert_eq!(update.format, UpdateFormat::Nsis);
    }

    #[test]
    fn directory_feed_with_the_same_version_offers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "0.1.0");
        let updater = updater_for(vec![Feed::Directory(dir.path().to_path_buf())], "0.1.0");
        assert!(updater.check().unwrap().is_none());
    }

    #[test]
    fn a_missing_manifest_is_a_named_error() {
        let dir = tempfile::tempdir().unwrap();
        let updater = updater_for(vec![Feed::Directory(dir.path().to_path_buf())], "0.1.0");
        let err = updater.check().unwrap_err();
        assert!(matches!(err, Error::ManifestNotFound { .. }), "got: {err}");
    }

    #[test]
    fn a_later_feed_serves_when_an_earlier_one_fails() {
        let empty = tempfile::tempdir().unwrap();
        let good = tempfile::tempdir().unwrap();
        write_manifest(good.path(), "0.3.0");
        let updater = updater_for(
            vec![
                Feed::Directory(empty.path().to_path_buf()),
                Feed::Directory(good.path().to_path_buf()),
            ],
            "0.1.0",
        );
        let update = updater.check().unwrap().expect("second feed should serve");
        assert_eq!(update.version, "0.3.0");
    }

    #[test]
    fn custom_version_comparator_wins() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "9.9.9");
        let updater = UpdaterBuilder::new(
            "0.1.0".parse().unwrap(),
            Config {
                feeds: vec![Feed::Directory(dir.path().to_path_buf())],
                pubkey: String::new(),
                windows: None,
            },
        )
        .target("test-target".to_string())
        .executable_path(std::env::temp_dir().join("luggage-test-exe"))
        .version_comparator(|_current, _release| false)
        .build()
        .unwrap();
        assert!(updater.check().unwrap().is_none(), "comparator said no");
    }

    #[test]
    fn release_accessors_respect_the_requested_target() {
        // Static data keyed by target must not leak across targets.
        let release = RemoteRelease {
            version: "1.0.0".parse().unwrap(),
            notes: None,
            pub_date: None,
            data: RemoteReleaseData::Static {
                platforms: [(
                    "linux-x86_64".to_string(),
                    ReleaseManifestPlatform {
                        url: "https://e.com/a".parse().unwrap(),
                        signature: "s".into(),
                        blake3: None,
                        format: UpdateFormat::AppImage,
                    },
                )]
                .into(),
            },
        };
        assert!(release.download_url("linux-x86_64").is_ok());
        assert!(release.download_url("windows-x86_64").is_err());
    }
}
