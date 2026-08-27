// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Updater errors.

use thiserror::Error;

/// All errors that can occur while running the updater.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// No feeds were configured.
    #[error("Updater does not have any feeds set.")]
    EmptyFeeds,
    /// A feed string could not be parsed.
    #[error("invalid feed {0}")]
    InvalidFeed(String),
    /// The feed served no manifest signature and one is required.
    #[error(
        "the feed's manifest is unsigned: sign {manifest} and serve {manifest}.sig beside it, \
         or set require_signed_manifest = false to accept the downgrade risk",
        manifest = crate::MANIFEST_NAME
    )]
    ManifestUnsigned,
    /// The manifest signature did not verify against the configured key.
    #[error("the feed's manifest signature does not verify against the configured public key")]
    ManifestSignatureInvalid,
    /// A directory feed has no manifest.
    #[error("no {manifest} in feed directory {dir}", manifest = crate::MANIFEST_NAME)]
    ManifestNotFound {
        /// The directory checked.
        dir: std::path::PathBuf,
    },
    /// The artifact's BLAKE3 digest did not match the manifest.
    #[error("artifact BLAKE3 mismatch: manifest says {expected}, downloaded bytes are {actual}")]
    Blake3Mismatch {
        /// The digest the manifest promised.
        expected: String,
        /// The digest of what was actually downloaded.
        actual: String,
    },
    /// IO errors.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Semver errors.
    #[error(transparent)]
    Semver(#[from] semver::Error),
    /// Serialization errors.
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    /// Could not fetch a valid release JSON from any feed.
    #[error("Could not fetch a valid release JSON from any feed")]
    ReleaseNotFound,
    /// Unsupported app architecture.
    #[error(
        "Unsupported application architecture, expected one of `x86`, `x86_64`, `arm` or `aarch64`."
    )]
    UnsupportedArch,
    /// Unsupported update format.
    #[error("Unsupported update format for the current target")]
    UnsupportedUpdateFormat,
    /// Operating system is not supported.
    #[error("Unsupported OS, expected one of `linux`, `macos` or `windows`.")]
    UnsupportedOs,
    /// Failed to determine the update extract path.
    #[error("Failed to determine updater package extract path.")]
    FailedToDetermineExtractPath,
    /// Url parsing errors.
    #[error(transparent)]
    UrlParse(#[from] url::ParseError),
    /// The platform was not found in the manifest's `platforms` object.
    #[error("the platform `{0}` was not found on the response `platforms` object")]
    TargetNotFound(String),
    /// Download failed.
    #[error("`{0}`")]
    Network(String),
    /// `minisign_verify` errors.
    #[error(transparent)]
    Minisign(#[from] minisign_verify::Error),
    /// `base64` errors.
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
    /// UTF8 errors in the signature.
    #[error(
        "The signature {0} could not be decoded, please check if it is a valid base64 string. \
         The signature must be the contents of the `.sig` file generated when packaging, as a string."
    )]
    SignatureUtf8(String),
    /// The temp dir is not on the same mount point as the AppImage, which
    /// prevents the rename-based swap.
    #[error("temp directory is not on the same mount point as the AppImage")]
    TempDirNotOnSameMountPoint,
    /// The `reqwest` crate errors.
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    /// The `http` crate errors.
    #[error(transparent)]
    Http(#[from] http::Error),
    /// Persisting a temporary file failed.
    #[error(transparent)]
    PersistError(#[from] tempfile::PersistError),
}

/// Convenience alias for the luggage crate's Result type.
pub type Result<T> = std::result::Result<T, Error>;
