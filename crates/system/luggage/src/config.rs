// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark AB (markik) — luggage fork
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! Updater configuration: feeds, the signing key, and Windows install modes.
//!
//! [`Feed`] is the fork's main divergence: upstream took HTTP endpoints
//! only; luggage adds local directories and GitHub repos as first-class
//! sources, parsed from strings so settings files stay plain.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};

/// Where releases are announced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    /// An HTTP(S) endpoint, with upstream's `{{target}}` / `{{arch}}` /
    /// `{{current_version}}` templating. Answers 204 for no update, or 200
    /// with the release JSON.
    Http(Url),
    /// A local directory containing [`crate::MANIFEST_NAME`]. Artifact URLs
    /// in such a manifest are absolute `file://` URLs.
    Directory(PathBuf),
    /// A GitHub repository; the latest release's [`crate::MANIFEST_NAME`]
    /// asset is the manifest.
    GitHub {
        /// Repository owner.
        owner: String,
        /// Repository name.
        repo: String,
    },
}

impl Feed {
    /// Parse a feed from its settings form.
    ///
    /// - `github:owner/repo` is a GitHub feed;
    /// - an `http://` / `https://` URL is an HTTP feed;
    /// - a `file://` URL or a bare path is a directory feed.
    pub fn parse(s: &str) -> Result<Self> {
        if let Some(rest) = s.strip_prefix("github:") {
            let (owner, repo) = rest
                .split_once('/')
                .ok_or_else(|| Error::InvalidFeed(format!("{s:?}: expected github:owner/repo")))?;
            if owner.is_empty() || repo.is_empty() || repo.contains('/') {
                return Err(Error::InvalidFeed(format!(
                    "{s:?}: expected github:owner/repo"
                )));
            }
            return Ok(Self::GitHub {
                owner: owner.to_string(),
                repo: repo.to_string(),
            });
        }
        if let Ok(url) = Url::parse(s) {
            // A Windows drive path ("C:/feed") parses as a URL whose scheme
            // is the drive letter; that is a path, not a scheme.
            if url.scheme().len() == 1 {
                return Ok(Self::Directory(PathBuf::from(s)));
            }
            match url.scheme() {
                "http" | "https" => return Ok(Self::Http(url)),
                "file" => {
                    let path = url.to_file_path().map_err(|_| {
                        Error::InvalidFeed(format!("{s:?}: file URL has no local path"))
                    })?;
                    return Ok(Self::Directory(path));
                }
                _ => {
                    return Err(Error::InvalidFeed(format!(
                        "{s:?}: unsupported scheme {:?}",
                        url.scheme()
                    )));
                }
            }
        }
        // Not a URL: a plain filesystem path.
        Ok(Self::Directory(PathBuf::from(s)))
    }

    /// The GitHub feed's manifest URL.
    pub(crate) fn github_manifest_url(owner: &str, repo: &str) -> String {
        format!(
            "https://github.com/{owner}/{repo}/releases/latest/download/{}",
            crate::MANIFEST_NAME
        )
    }
}

impl std::fmt::Display for Feed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(url) => write!(f, "{url}"),
            Self::Directory(path) => write!(f, "{}", path.display()),
            Self::GitHub { owner, repo } => write!(f, "github:{owner}/{repo}"),
        }
    }
}

/// Install modes for the Windows update.
#[derive(Debug, PartialEq, Eq, Clone, Default, Deserialize, Serialize)]
pub enum WindowsUpdateInstallMode {
    /// A basic UI, including a final dialog box at the end.
    BasicUi,
    /// No user interaction required. Requires admin privileges if the
    /// installer does.
    Quiet,
    /// Unattended: only a progress bar. The default.
    #[default]
    Passive,
}

impl WindowsUpdateInstallMode {
    /// The associated `msiexec.exe` arguments.
    pub fn msiexec_args(&self) -> &'static [&'static str] {
        match self {
            Self::BasicUi => &["/qb+"],
            Self::Quiet => &["/quiet"],
            Self::Passive => &["/passive"],
        }
    }

    /// The associated NSIS arguments.
    pub fn nsis_args(&self) -> &'static [&'static str] {
        match self {
            Self::Passive => &["/P", "/R"],
            Self::Quiet => &["/S", "/R"],
            _ => &[],
        }
    }
}

/// The updater configuration for Windows.
#[derive(Debug, Default, PartialEq, Eq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsConfig {
    /// Additional arguments given to the NSIS or WiX installer.
    pub installer_args: Option<Vec<String>>,
    /// The installation mode for the update on Windows. Defaults to
    /// `Passive`.
    pub install_mode: Option<WindowsUpdateInstallMode>,
}

/// Updater configuration.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// The feeds to check, in order; the first that yields a release wins.
    pub feeds: Vec<Feed>,
    /// The minisign public key releases are signed with.
    pub pubkey: String,
    /// The Windows configuration for the updater.
    pub windows: Option<WindowsConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_feed_forms() {
        assert!(matches!(
            Feed::parse("https://releases.example.com/{{target}}").unwrap(),
            Feed::Http(_)
        ));
        assert_eq!(
            Feed::parse("github:mark-ik/hocket").unwrap(),
            Feed::GitHub {
                owner: "mark-ik".into(),
                repo: "hocket".into()
            }
        );
        assert!(matches!(
            Feed::parse("C:/some/feed/dir").unwrap(),
            Feed::Directory(_)
        ));
        assert!(matches!(
            Feed::parse("/srv/updates").unwrap(),
            Feed::Directory(_)
        ));
    }

    #[test]
    fn rejects_malformed_github_feeds() {
        for bad in ["github:", "github:justowner", "github:o/r/extra", "github:/r"] {
            assert!(Feed::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn rejects_unsupported_url_schemes() {
        assert!(Feed::parse("ftp://example.com/feed").is_err());
    }

    #[test]
    fn github_manifest_url_targets_the_latest_release_asset() {
        assert_eq!(
            Feed::github_manifest_url("mark-ik", "hocket"),
            "https://github.com/mark-ik/hocket/releases/latest/download/luggage.json"
        );
    }
}
