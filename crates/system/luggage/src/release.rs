// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The release manifest: what a feed announces.
//!
//! Upstream's JSON shape, with one fork addition: an optional per-platform
//! `blake3` digest, verified before the signature and carried from day one
//! so the planned content-addressed distribution lane never needs a manifest
//! format break.

use std::{collections::HashMap, str::FromStr};

use semver::Version;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;
use url::Url;

use crate::error::{Error, Result};

/// The manifest file name a directory or GitHub feed serves.
pub const MANIFEST_NAME: &str = "luggage.json";

/// Supported update format.
#[derive(Debug, Serialize, Copy, Clone, PartialEq, Eq)]
pub enum UpdateFormat {
    /// The NSIS installer (.exe).
    Nsis,
    /// The Microsoft Software Installer (.msi) through WiX Toolset.
    Wix,
    /// The Linux AppImage package (.AppImage).
    AppImage,
    /// The macOS application bundle (.app).
    App,
}

impl std::fmt::Display for UpdateFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                UpdateFormat::Nsis => "nsis",
                UpdateFormat::Wix => "wix",
                UpdateFormat::AppImage => "appimage",
                UpdateFormat::App => "app",
            }
        )
    }
}

impl<'de> Deserialize<'de> for UpdateFormat {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let lower = String::deserialize(deserializer)?.to_lowercase();
        match lower.as_str() {
            "nsis" => Ok(UpdateFormat::Nsis),
            "wix" => Ok(UpdateFormat::Wix),
            "app" => Ok(UpdateFormat::App),
            "appimage" => Ok(UpdateFormat::AppImage),
            _ => Err(serde::de::Error::custom(
                "Unknown updater format, expected one of 'nsis', 'wix', 'app' or 'appimage'",
            )),
        }
    }
}

/// One platform's entry in a release.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReleaseManifestPlatform {
    /// Download URL for the platform.
    pub url: Url,
    /// Minisign signature of the artifact.
    pub signature: String,
    /// BLAKE3 hex digest of the artifact, verified before the signature
    /// when present. Fork addition; optional so upstream manifests parse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blake3: Option<String>,
    /// Update format.
    pub format: UpdateFormat,
}

/// Release data: one platform's (dynamic server response) or all platforms'.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum RemoteReleaseData {
    /// Dynamic release data based on the platform the update was requested
    /// from.
    Dynamic(ReleaseManifestPlatform),
    /// A map of release data per platform, keyed `<os>-<arch>`.
    Static {
        /// A map of release data per platform, keyed `<os>-<arch>`.
        platforms: HashMap<String, ReleaseManifestPlatform>,
    },
}

/// A release announced by a feed.
#[derive(Debug, Clone)]
pub struct RemoteRelease {
    /// Version to install.
    pub version: Version,
    /// Release notes.
    pub notes: Option<String>,
    /// Release date.
    pub pub_date: Option<OffsetDateTime>,
    /// Release data.
    pub data: RemoteReleaseData,
}

impl RemoteRelease {
    fn platform(&self, target: &str) -> Result<&ReleaseManifestPlatform> {
        match self.data {
            RemoteReleaseData::Dynamic(ref platform) => Ok(platform),
            RemoteReleaseData::Static { ref platforms } => platforms
                .get(target)
                .ok_or_else(|| Error::TargetNotFound(target.to_string())),
        }
    }

    /// The release's download URL for the given target.
    pub fn download_url(&self, target: &str) -> Result<&Url> {
        self.platform(target).map(|p| &p.url)
    }

    /// The release's signature for the given target.
    pub fn signature(&self, target: &str) -> Result<&String> {
        self.platform(target).map(|p| &p.signature)
    }

    /// The release's BLAKE3 digest for the given target, when the manifest
    /// carries one.
    pub fn blake3(&self, target: &str) -> Result<Option<String>> {
        self.platform(target).map(|p| p.blake3.clone())
    }

    /// The release's update format for the given target.
    pub fn format(&self, target: &str) -> Result<UpdateFormat> {
        self.platform(target).map(|p| p.format)
    }
}

fn parse_version<'de, D>(deserializer: D) -> std::result::Result<Version, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let str = String::deserialize(deserializer)?;
    Version::from_str(str.trim_start_matches('v')).map_err(D::Error::custom)
}

impl<'de> Deserialize<'de> for RemoteRelease {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct InnerRemoteRelease {
            #[serde(alias = "name", deserialize_with = "parse_version")]
            version: Version,
            notes: Option<String>,
            pub_date: Option<String>,
            platforms: Option<HashMap<String, ReleaseManifestPlatform>>,
            // Dynamic platform response.
            url: Option<Url>,
            signature: Option<String>,
            #[serde(default)]
            blake3: Option<String>,
            format: Option<UpdateFormat>,
        }

        let release = InnerRemoteRelease::deserialize(deserializer)?;

        let pub_date = release
            .pub_date
            .map(|date| {
                OffsetDateTime::parse(&date, &time::format_description::well_known::Rfc3339)
                    .map_err(|e| D::Error::custom(format!("invalid value for `pub_date`: {e}")))
            })
            .transpose()?;

        Ok(RemoteRelease {
            version: release.version,
            notes: release.notes,
            pub_date,
            data: if let Some(platforms) = release.platforms {
                RemoteReleaseData::Static { platforms }
            } else {
                RemoteReleaseData::Dynamic(ReleaseManifestPlatform {
                    url: release.url.ok_or_else(|| {
                        D::Error::custom("the `url` field was not set on the updater response")
                    })?,
                    signature: release.signature.ok_or_else(|| {
                        D::Error::custom(
                            "the `signature` field was not set on the updater response",
                        )
                    })?,
                    blake3: release.blake3,
                    format: release.format.ok_or_else(|| {
                        D::Error::custom("the `format` field was not set on the updater response")
                    })?,
                })
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_manifest_with_blake3_parses() {
        let json = r#"{
            "version": "v0.2.0",
            "notes": "test",
            "pub_date": "2026-07-24T12:00:00Z",
            "platforms": {
                "windows-x86_64": {
                    "url": "https://example.com/app-setup.exe",
                    "signature": "c2ln",
                    "blake3": "ab12",
                    "format": "nsis"
                }
            }
        }"#;
        let release: RemoteRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.version.to_string(), "0.2.0");
        assert_eq!(
            release.blake3("windows-x86_64").unwrap(),
            Some("ab12".to_string())
        );
        assert_eq!(
            release.format("windows-x86_64").unwrap(),
            UpdateFormat::Nsis
        );
        assert!(
            release.blake3("linux-x86_64").is_err(),
            "unknown target errors"
        );
    }

    #[test]
    fn upstream_manifest_without_blake3_still_parses() {
        // The digest is a fork addition; manifests written for upstream
        // must not break.
        let json = r#"{
            "version": "0.2.0",
            "url": "https://example.com/app.AppImage.tar.gz",
            "signature": "c2ln",
            "format": "appimage"
        }"#;
        let release: RemoteRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.blake3("anything").unwrap(), None);
    }

    #[test]
    fn leading_v_in_version_is_tolerated() {
        let json = r#"{"version":"v1.2.3","url":"https://e.com/a","signature":"s","format":"app"}"#;
        let release: RemoteRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.version.to_string(), "1.2.3");
    }

    #[test]
    fn file_urls_parse_in_manifests() {
        // Directory feeds announce artifacts as absolute file:// URLs.
        let json = r#"{
            "version": "0.2.0",
            "url": "file:///C:/feed/app-setup.exe",
            "signature": "c2ln",
            "format": "nsis"
        }"#;
        let release: RemoteRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.download_url("x").unwrap().scheme(), "file");
    }
}
