// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// Copyright 2023-2023 CrabNebula Ltd.
// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! # luggage
//!
//! Self-update for apps packaged by
//! [`cargo-packager`](https://docs.rs/cargo-packager): check a `Feed` for a
//! signed release manifest, download and verify (BLAKE3 when the manifest
//! carries it, then minisign always), and install per platform.
//!
//! Forked from `cargo-packager-updater` 0.2.3; see the crate README for the
//! provenance and the recorded divergences.
//!
//! Start at `check_update` (the `native` feature), which carries the worked
//! example.
//!
//! ## The `native` feature
//!
//! On by default, and it carries the entire update pipeline: checking feeds,
//! downloading, staging, installing — everything that touches the network,
//! the filesystem, or the host process.
//!
//! With `default-features = false` what remains is the **release-identity
//! core**: [`ReleaseRefV1`] — a reference naming one signed release — plus
//! [`RemoteRelease`], [`ReleaseManifestPlatform`], [`UpdateFormat`] and
//! [`MANIFEST_NAME`], which are what a release *is* and how it is spelled in
//! a manifest. No I/O, no feed configuration, and no dependency that fails to
//! build for `wasm32`.
//!
//! Verification is deliberately *not* part of that surface. The minisign and
//! BLAKE3 checks are `pub(crate)` helpers driven by the native pipeline, so
//! they went with it. A core consumer can name and compare a release;
//! verifying one would need an entry point this crate does not yet export,
//! and adding one is a public-API decision rather than a mechanical split.
//!
//! That split exists so a browser can *name and verify* a release without
//! carrying an updater it could never run. The alternative was a second,
//! parallel release type maintained by hand on the browser side, which is the
//! kind of duplicate that drifts silently and is wrong exactly when it
//! matters. Release identity has one definition, and this is it.
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
//!
//! **The manifest itself is signed too**, as a detached `luggage.json.sig`
//! served beside it, and is verified before anything in it is believed. A
//! per-artifact signature cannot cover the version and URL announced around
//! it, so without this a feed can advertise an old signed build as a new
//! version and roll a client backwards. See
//! `Config::require_signed_manifest` (the `native` feature), which defaults
//! to `true`.
//!
//! ## Staging
//!
//! `Update::stage` writes a verified artifact into an app-owned directory so
//! "ready to restart" survives the app closing, and
//! `StagedUpdate::take_verified` re-checks its digest at apply time. Both are
//! part of the `native` feature.

#![deny(missing_docs)]

mod error;
mod release;

#[cfg(feature = "native")]
mod config;
#[cfg(feature = "native")]
mod signing;
#[cfg(feature = "native")]
mod install;
#[cfg(feature = "native")]
mod staging;
#[cfg(feature = "native")]
mod updater;

pub use error::{Error, Result};
pub use release::{
    ReleaseManifestPlatform, ReleaseRefV1, RemoteRelease, RemoteReleaseData, UpdateFormat,
    MANIFEST_NAME,
};

#[cfg(feature = "native")]
pub use config::{Config, Feed, WindowsConfig, WindowsUpdateInstallMode};
#[cfg(feature = "native")]
pub use install::Update;
#[cfg(feature = "native")]
pub use staging::StagedUpdate;
#[cfg(feature = "native")]
pub use updater::{target, Updater, UpdaterBuilder};

pub use semver;
pub use url;

#[cfg(feature = "native")]
pub use http;
#[cfg(feature = "native")]
pub use reqwest;

/// Check for an update against the configured feeds.
///
/// ```no_run
/// use luggage::{check_update, Config, Feed};
///
/// let config = Config {
///     feeds: vec![Feed::parse("https://myserver.com/updates").unwrap()],
///     pubkey: "<minisign public key>".into(),
///     ..Default::default()
/// };
/// if let Some(update) =
///     check_update("0.1.0".parse().unwrap(), config).expect("check failed")
/// {
///     update.download_and_install().expect("update failed");
/// }
/// ```
#[cfg(feature = "native")]
pub fn check_update(
    current_version: semver::Version,
    config: Config,
) -> Result<Option<Update>> {
    UpdaterBuilder::new(current_version, config)
        .build()?
        .check()
}
