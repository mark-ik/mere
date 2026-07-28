// Copyright 2026 Mark AB (markik) — luggage
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! Durable staging: hold a verified update on disk until it is applied.
//!
//! T2 of the auto-update plan. Without this, "downloaded, ready to restart"
//! is a claim about bytes held in memory: close the app and the download is
//! gone, and the next launch starts over. Staging writes the verified
//! artifact into an app-owned directory with a small record beside it, so
//! the offer survives a restart — which is the whole point of offering a
//! restart rather than forcing one.
//!
//! ## Re-verification
//!
//! A staged file sits on disk, possibly for days, somewhere another process
//! can reach. [`StagedUpdate::take_verified`] re-hashes it against the
//! digest recorded at download time and refuses on mismatch, so time-of-
//! check/time-of-use is not a hole. Staging therefore requires a manifest
//! that carried a `blake3`; without one there is nothing to re-check
//! against and [`Update::stage`] says so rather than staging unverifiable
//! bytes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::install::Update;
use crate::release::UpdateFormat;

/// File name of the record describing what is staged.
const RECORD_NAME: &str = "staged.json";
/// File name of the staged artifact itself.
const ARTIFACT_NAME: &str = "staged-artifact";

#[derive(Debug, Serialize, Deserialize)]
struct StagedRecord {
    version: String,
    format: UpdateFormat,
    blake3: String,
    /// Where the running app lives, captured at download time: the app that
    /// applies this may be a fresh process that has not checked a feed.
    extract_path: PathBuf,
}

/// An update sitting on disk, waiting to be applied.
#[derive(Debug)]
pub struct StagedUpdate {
    dir: PathBuf,
    record: StagedRecord,
}

impl StagedUpdate {
    /// The version staged.
    pub fn version(&self) -> &str {
        &self.record.version
    }

    /// The format staged.
    pub fn format(&self) -> UpdateFormat {
        self.record.format
    }

    /// Load whatever is staged in `dir`, if anything.
    ///
    /// A partial or unreadable staging directory reports `Ok(None)` rather
    /// than an error: a half-written stage is not a failure of the app, and
    /// the next check simply offers the update again.
    pub fn load(dir: impl AsRef<Path>) -> Result<Option<Self>> {
        let dir = dir.as_ref().to_path_buf();
        let record_path = dir.join(RECORD_NAME);
        if !record_path.exists() || !dir.join(ARTIFACT_NAME).exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&record_path)?;
        match serde_json::from_slice::<StagedRecord>(&bytes) {
            Ok(record) => Ok(Some(Self { dir, record })),
            Err(err) => {
                log::warn!("ignoring unreadable staging record {record_path:?}: {err}");
                Ok(None)
            }
        }
    }

    /// Read the staged artifact back, re-checking its digest.
    ///
    /// The bytes were verified (BLAKE3 *and* signature) before staging;
    /// this guards the gap between then and now.
    pub fn take_verified(&self) -> Result<Vec<u8>> {
        let bytes = std::fs::read(self.dir.join(ARTIFACT_NAME))?;
        let actual = blake3::hash(&bytes).to_hex().to_string();
        if !actual.eq_ignore_ascii_case(&self.record.blake3) {
            return Err(Error::Blake3Mismatch {
                expected: self.record.blake3.clone(),
                actual,
            });
        }
        Ok(bytes)
    }

    /// Where the app being replaced lives.
    pub fn extract_path(&self) -> &Path {
        &self.record.extract_path
    }

    /// Delete the staged update.
    ///
    /// Call after a successful apply, or when the user declines: a stale
    /// stage would otherwise be offered forever.
    pub fn discard(&self) -> Result<()> {
        let _ = std::fs::remove_file(self.dir.join(ARTIFACT_NAME));
        let _ = std::fs::remove_file(self.dir.join(RECORD_NAME));
        Ok(())
    }
}

impl Update {
    /// Write verified bytes into `dir` so the offer survives a restart.
    ///
    /// Requires the manifest to have carried a `blake3` digest; there is
    /// nothing to re-verify against otherwise, and staging bytes that
    /// cannot be re-checked would be worse than not staging at all.
    pub fn stage(&self, dir: impl AsRef<Path>, bytes: &[u8]) -> Result<StagedUpdate> {
        let digest = self.blake3.clone().ok_or_else(|| {
            Error::Network(
                "cannot stage an update whose manifest carries no blake3 digest".to_string(),
            )
        })?;
        // Trust nothing that was not checked here: the caller may have got
        // these bytes from anywhere.
        let actual = blake3::hash(bytes).to_hex().to_string();
        if !actual.eq_ignore_ascii_case(&digest) {
            return Err(Error::Blake3Mismatch {
                expected: digest,
                actual,
            });
        }

        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;

        // Artifact first, then the record: the record is what `load` keys
        // on, so it must never point at a file that is not fully written.
        let artifact = dir.join(ARTIFACT_NAME);
        let tmp = dir.join("staged-artifact.partial");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &artifact)?;

        let record = StagedRecord {
            version: self.version.clone(),
            format: self.format,
            blake3: digest,
            extract_path: self.extract_path.clone(),
        };
        std::fs::write(dir.join(RECORD_NAME), serde_json::to_vec_pretty(&record)?)?;

        Ok(StagedUpdate { dir, record })
    }

    /// Install from a staged update, re-verifying it first.
    pub fn install_staged(&self, staged: &StagedUpdate) -> Result<()> {
        let bytes = staged.take_verified()?;
        self.install(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Feed};
    use crate::release::MANIFEST_NAME;
    use crate::UpdaterBuilder;
    use std::io::Write as _;

    /// Build a real `Update` (via a directory feed) for an artifact.
    fn update_for(dir: &Path, artifact: &[u8], with_digest: bool) -> Update {
        let artifact_path = dir.join("artifact.bin");
        std::fs::File::create(&artifact_path)
            .unwrap()
            .write_all(artifact)
            .unwrap();
        let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let signature = minisign::sign(
            None,
            &keypair.sk,
            std::io::Cursor::new(artifact),
            None,
            None,
        )
        .unwrap();
        use base64::Engine as _;
        let engine = base64::engine::general_purpose::STANDARD;
        let digest = blake3::hash(artifact).to_hex().to_string();
        let manifest = serde_json::json!({
            "version": "0.2.0",
            "platforms": {
                "t": {
                    "url": url::Url::from_file_path(&artifact_path).unwrap().to_string(),
                    "signature": engine.encode(signature.to_string()),
                    "blake3": if with_digest { Some(digest) } else { None },
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
                pubkey: engine.encode(keypair.pk.to_box().unwrap().to_string()),
                // Staging is what is under test here, not manifest signing.
                require_signed_manifest: false,
                ..Default::default()
            },
        )
        .target("t".to_string())
        .executable_path(dir.join("fake-exe"))
        .build()
        .unwrap()
        .check()
        .unwrap()
        .expect("an update")
    }

    #[test]
    fn staging_survives_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("stage");
        let artifact = b"installer bytes";
        let update = update_for(dir.path(), artifact, true);

        let staged = update.stage(&stage, artifact).unwrap();
        assert_eq!(staged.version(), "0.2.0");

        // A fresh process finds it without having checked a feed.
        let found = StagedUpdate::load(&stage).unwrap().expect("staged update");
        assert_eq!(found.version(), "0.2.0");
        assert_eq!(found.take_verified().unwrap(), artifact);
    }

    #[test]
    fn nothing_staged_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(StagedUpdate::load(dir.path()).unwrap().is_none());
    }

    #[test]
    fn tampering_with_a_staged_artifact_is_caught_at_apply_time() {
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("stage");
        let artifact = b"installer bytes";
        let update = update_for(dir.path(), artifact, true);
        update.stage(&stage, artifact).unwrap();

        // Something rewrites the staged file while it waits.
        std::fs::write(stage.join(ARTIFACT_NAME), b"malicious bytes").unwrap();

        let found = StagedUpdate::load(&stage).unwrap().unwrap();
        let err = found.take_verified().unwrap_err();
        assert!(
            matches!(err, Error::Blake3Mismatch { .. }),
            "a staged file must be re-checked, got: {err}"
        );
    }

    #[test]
    fn staging_refuses_bytes_that_do_not_match_the_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("stage");
        let update = update_for(dir.path(), b"real bytes", true);
        let err = update.stage(&stage, b"different bytes").unwrap_err();
        assert!(matches!(err, Error::Blake3Mismatch { .. }), "got: {err}");
    }

    #[test]
    fn a_manifest_without_a_digest_cannot_be_staged() {
        // Nothing to re-verify against later, so refuse rather than stage
        // bytes that would be trusted blindly at apply time.
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("stage");
        let update = update_for(dir.path(), b"unsummed", false);
        assert!(update.stage(&stage, b"unsummed").is_err());
    }

    #[test]
    fn discard_clears_the_offer() {
        let dir = tempfile::tempdir().unwrap();
        let stage = dir.path().join("stage");
        let artifact = b"installer bytes";
        let update = update_for(dir.path(), artifact, true);
        update.stage(&stage, artifact).unwrap().discard().unwrap();
        assert!(StagedUpdate::load(&stage).unwrap().is_none());
    }
}
