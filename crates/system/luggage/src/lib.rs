// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark AB (markik) — luggage fork
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! # luggage
//!
//! Self-update for apps packaged by
//! [`cargo-packager`](https://docs.rs/cargo-packager): check a [`Feed`] for a
//! signed release manifest, download and verify (BLAKE3 when the manifest
//! carries it, then minisign always), and install per platform.
//!
//! Forked from `cargo-packager-updater` 0.2.3; see the crate README for the
//! provenance and the recorded divergences.
//!
//! ## Checking for an update
//!
//! ```no_run
//! use luggage::{check_update, Config, Feed};
//!
//! let config = Config {
//!     feeds: vec![Feed::parse("https://myserver.com/updates").unwrap()],
//!     pubkey: "<minisign public key>".into(),
//!     ..Default::default()
//! };
//! if let Some(update) =
//!     check_update("0.1.0".parse().unwrap(), config).expect("check failed")
//! {
//!     update.download_and_install().expect("update failed");
//! }
//! ```
//!
//! ## Feeds
//!
//! - **HTTP(S)** — upstream semantics: the endpoint may contain
//!   `{{target}}`, `{{arch}}` and `{{current_version}}`, and answers 204 (no
//!   update) or 200 with the release JSON.
//! - **Directory** — a local path holding `luggage.json`; artifact URLs are
//!   `file://`. No server: a LAN share or a mounted drive is a feed.
//! - **GitHub** — `github:owner/repo`; the latest release's `luggage.json`
//!   asset is the manifest and its other assets are the artifacts.
//!
//! ## Manifest
//!
//! The JSON shape upstream documents (`version`, `platforms.<target>.url` /
//! `signature` / `format`, optional `notes` / `pub_date`), plus an optional
//! per-platform `blake3` hex digest verified before the signature.

#![deny(missing_docs)]

mod config;
mod error;
mod install;
mod release;
mod updater;

pub use config::{Config, Feed, WindowsConfig, WindowsUpdateInstallMode};
pub use error::{Error, Result};
pub use install::Update;
pub use release::{
    MANIFEST_NAME, ReleaseManifestPlatform, RemoteRelease, RemoteReleaseData, UpdateFormat,
};
pub use updater::{Updater, UpdaterBuilder, target};

pub use http;
pub use reqwest;
pub use semver;
pub use url;

use semver::Version;

/// Check for an update against the configured feeds.
pub fn check_update(current_version: Version, config: Config) -> Result<Option<Update>> {
    UpdaterBuilder::new(current_version, config).build()?.check()
}
