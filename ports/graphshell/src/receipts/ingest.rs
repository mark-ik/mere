//! Receipt ingest: a scenario-receipt directory becomes graph facts.
//!
//! A capture without provenance is an image nobody can place six months
//! later. The remote-receipt lane (`genet/scripts/remote-receipt.ps1`) already
//! binds every artifact to a commit, a machine, a session type and a scenario
//! in a `manifest.json`; this module turns that directory into the personal
//! graph's own vocabulary so the binding is a replicated fact rather than a
//! JSON file in an unversioned directory.
//!
//! The whole of R1 (replication) falls out of one observation: the events this
//! emits — [`PersonalGraphEvent::AddNode`], `SetFacet`, `AddTag`,
//! `ObserveBlobAvailability` — are already what the resident host replicates.
//! There is no second sync system, and this module deliberately does not
//! contain one.
//!
//! **Ingest reads no clock.** Every timestamp comes from the manifest, because
//! the facts are about when the *run* happened, not when it was filed. That
//! also makes ingest deterministic: the same directory yields byte-identical
//! events, which is what makes re-ingest a no-op rather than a duplicate.
//!
//! **Ingest verifies rather than trusts.** The manifest carries a SHA-256 per
//! artifact, computed on the machine that produced it; the bytes that arrived
//! are re-hashed here and a mismatch is a hard error. Blobs are then stored by
//! their blake3 content hash (muniment's addressing), so identical captures
//! from three machines dedupe to one blob with no work.

use std::path::Path;

use muniment::{Backend, BlobStore, Hash};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::personal_sync::{BlobAvailabilityObservation, PersonalGraphEvent};
use crate::receipts::manifest::{
    FACET_ARTIFACTS, FACET_RUN, RECEIPT_NAMESPACE, ReceiptError, ReceiptManifest,
};

/// What one ingest produced.
#[derive(Clone, Debug)]
pub struct IngestedReceipt {
    /// The receipt's node id.
    pub node: Uuid,
    /// Its stable address.
    pub address: String,
    /// The graph events to author. Already in dependency order (the node
    /// before anything that references it).
    pub events: Vec<PersonalGraphEvent>,
    /// Each artifact's file name and the blake3 hash it was stored under.
    pub blobs: Vec<(String, Hash)>,
}

/// Read a receipt directory, store its artifacts, and produce the graph events
/// that record it.
///
/// `device` names the machine holding the bytes — the blob-availability facts
/// are per device, because a blob present here is not present everywhere.
///
/// Idempotent: the node id, every event, and every blob address derive from
/// content and the manifest, so a second ingest of the same directory produces
/// the same events over the same blobs and changes nothing.
pub async fn ingest_directory<B: Backend>(
    dir: &Path,
    store: &BlobStore<B>,
    device: &str,
) -> Result<IngestedReceipt, ReceiptError> {
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(ReceiptError::NoManifest(dir.to_path_buf()));
    }
    let manifest = ReceiptManifest::parse(&std::fs::read_to_string(&manifest_path)?)?;
    ingest_manifest(&manifest, dir, store, device).await
}

/// [`ingest_directory`] with the manifest already parsed — the seam a test or
/// a caller holding the manifest in memory uses.
pub async fn ingest_manifest<B: Backend>(
    manifest: &ReceiptManifest,
    dir: &Path,
    store: &BlobStore<B>,
    device: &str,
) -> Result<IngestedReceipt, ReceiptError> {
    let node = manifest.node_id();
    let address = manifest.address();
    let at_ms = manifest.ran_at_ms();

    let mut events = vec![PersonalGraphEvent::AddNode {
        id: node,
        address: address.clone(),
        title: manifest.title(),
    }];

    // Tags are the cheap axis a list filters on: which product, which machine
    // kind, and whether it passed.
    for tag in [
        format!("repo:{}", manifest.repo),
        format!("platform:{}", manifest.platform),
        format!("host:{}", manifest.host()),
        if manifest.passed() {
            "receipt:ok".into()
        } else {
            "receipt:failed".into()
        },
    ] {
        events.push(PersonalGraphEvent::AddTag { node, tag });
    }

    let mut blobs = Vec::new();
    let mut artifact_facts = Vec::new();
    for artifact in &manifest.artifacts {
        let path = dir.join(&artifact.name);
        if !path.exists() {
            return Err(ReceiptError::MissingArtifact(artifact.name.clone()));
        }
        let bytes = std::fs::read(&path)?;

        // Verify before storing. The manifest is the claim; these bytes are
        // the evidence, and a receipt whose evidence was corrupted in transit
        // must not enter the graph looking sound.
        let found = hex_sha256(&bytes);
        if !artifact.sha256.is_empty() && !found.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(ReceiptError::DigestMismatch {
                name: artifact.name.clone(),
                expected: artifact.sha256.clone(),
                found,
            });
        }

        let hash = store.put(&bytes).await?;
        artifact_facts.push(json!({
            "name": artifact.name,
            "bytes": bytes.len(),
            "blake3": hash.to_hex(),
            "sha256": found,
        }));

        // One availability fact per artifact per device, with a derived id so
        // a re-ingest restates rather than appends.
        events.push(PersonalGraphEvent::ObserveBlobAvailability {
            observation: BlobAvailabilityObservation {
                record_id: observation_id(node, &hash, device),
                container_id: node,
                blob: *hash.as_bytes(),
                device: device.to_string(),
                available: true,
                at_ms,
            },
        });
        blobs.push((artifact.name.clone(), hash));
    }

    events.push(PersonalGraphEvent::SetFacet {
        node,
        facet: FACET_RUN.to_string(),
        value: run_facet(manifest),
    });
    events.push(PersonalGraphEvent::SetFacet {
        node,
        facet: FACET_ARTIFACTS.to_string(),
        value: Value::Array(artifact_facts),
    });

    Ok(IngestedReceipt {
        node,
        address,
        events,
        blobs,
    })
}

/// The provenance facet: everything needed to place this receipt later.
fn run_facet(manifest: &ReceiptManifest) -> Value {
    json!({
        "repo": manifest.repo,
        "package": manifest.package,
        "scenario": manifest.scenario,
        "target": manifest.target,
        "platform": manifest.platform,
        "os": manifest.remote_os,
        "commit": manifest.remote_commit,
        "dirty": manifest.remote_dirty,
        "session": manifest.session,
        "ran_at": manifest.ran_at_utc,
        "exit_code": manifest.exit_code,
    })
}

/// A blob-availability record id that is a function of what it asserts.
fn observation_id(node: Uuid, blob: &Hash, device: &str) -> Uuid {
    let mut name = Vec::new();
    name.extend_from_slice(node.as_bytes());
    name.extend_from_slice(blob.as_bytes());
    name.extend_from_slice(device.as_bytes());
    Uuid::new_v5(&RECEIPT_NAMESPACE, &name)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipts::{sync_address_rule, sync_facets};
    use muniment::MemoryBackend;

    fn manifest_json(artifacts: &str) -> String {
        format!(
            r#"{{
              "repo": "woodshed",
              "package": "woodshed-genet",
              "scenario": "design_docs/scenarios/frame.scn",
              "target": "mark@thinkpad",
              "platform": "linux",
              "remote_os": "Linux 6.9.4",
              "remote_commit": "abc123def456",
              "remote_dirty": 0,
              "session": "wayland (wayland-0)",
              "ran_at_utc": "2026-08-10T14:31:05.1234567Z",
              "exit_code": 0,
              "artifacts": [{artifacts}]
            }}"#
        )
    }

    fn write_receipt(dir: &Path, body: &[u8]) -> ReceiptManifest {
        std::fs::write(dir.join("frame.png"), body).unwrap();
        let json = manifest_json(&format!(
            r#"{{ "name": "frame.png", "bytes": {}, "sha256": "{}" }}"#,
            body.len(),
            hex_sha256(body)
        ));
        std::fs::write(dir.join("manifest.json"), &json).unwrap();
        ReceiptManifest::parse(&json).unwrap()
    }

    #[test]
    fn the_address_and_node_id_are_functions_of_the_run() {
        let a = ReceiptManifest::parse(&manifest_json("")).unwrap();
        let b = ReceiptManifest::parse(&manifest_json("")).unwrap();
        assert_eq!(a.node_id(), b.node_id());
        assert!(a.address().starts_with("receipt:woodshed/thinkpad/"));

        // A different run is a different receipt.
        let other =
            ReceiptManifest::parse(&manifest_json("").replace("mark@thinkpad", "mark@imac"))
                .unwrap();
        assert_ne!(a.node_id(), other.node_id());
    }

    #[test]
    fn unknown_manifest_fields_survive() {
        let json = manifest_json("").replace(
            r#""exit_code": 0"#,
            r#""exit_code": 0, "gpu": "RADV NAVI23""#,
        );
        let manifest = ReceiptManifest::parse(&json).unwrap();
        assert_eq!(
            manifest.extra.get("gpu").and_then(Value::as_str),
            Some("RADV NAVI23"),
            "a newer lane's provenance must not be dropped on ingest",
        );
    }

    #[test]
    fn ingest_stores_the_blob_and_emits_the_facts() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let manifest = write_receipt(dir.path(), b"frame bytes");
            let store = BlobStore::new(MemoryBackend::new());

            let ingested = ingest_directory(dir.path(), &store, "laptop")
                .await
                .unwrap();

            assert_eq!(ingested.node, manifest.node_id());
            assert_eq!(ingested.blobs.len(), 1);
            let (name, hash) = &ingested.blobs[0];
            assert_eq!(name, "frame.png");
            assert_eq!(
                store.get(hash).await.unwrap().as_deref(),
                Some(&b"frame bytes"[..]),
            );

            // The node comes first; the facets and availability follow.
            assert!(matches!(
                ingested.events.first(),
                Some(PersonalGraphEvent::AddNode { .. })
            ));
            let facets: Vec<&str> = ingested
                .events
                .iter()
                .filter_map(|e| match e {
                    PersonalGraphEvent::SetFacet { facet, .. } => Some(facet.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(facets, vec![FACET_RUN, FACET_ARTIFACTS]);
            assert_eq!(
                ingested
                    .events
                    .iter()
                    .filter(|e| matches!(e, PersonalGraphEvent::ObserveBlobAvailability { .. }))
                    .count(),
                1,
            );
        });
    }

    /// The R0 done-condition: ingesting the same directory twice changes
    /// nothing. This is content addressing proving itself.
    #[test]
    fn ingesting_twice_is_a_no_op() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            write_receipt(dir.path(), b"frame bytes");
            let store = BlobStore::new(MemoryBackend::new());

            let first = ingest_directory(dir.path(), &store, "laptop")
                .await
                .unwrap();
            let second = ingest_directory(dir.path(), &store, "laptop")
                .await
                .unwrap();

            assert_eq!(first.node, second.node);
            assert_eq!(first.blobs, second.blobs);
            assert_eq!(
                serde_json::to_string(&first.events).unwrap(),
                serde_json::to_string(&second.events).unwrap(),
                "a re-ingest must restate the same facts, not append new ones",
            );
        });
    }

    /// Two machines capturing the same frame store one blob. The point of
    /// content addressing, and the reason this replaces a folder sync.
    #[test]
    fn identical_captures_from_two_machines_dedupe() {
        pollster::block_on(async {
            let store = BlobStore::new(MemoryBackend::new());
            let laptop = tempfile::tempdir().unwrap();
            let imac = tempfile::tempdir().unwrap();
            write_receipt(laptop.path(), b"identical pixels");
            std::fs::write(imac.path().join("frame.png"), b"identical pixels").unwrap();
            std::fs::write(
                imac.path().join("manifest.json"),
                manifest_json(&format!(
                    r#"{{ "name": "frame.png", "bytes": 16, "sha256": "{}" }}"#,
                    hex_sha256(b"identical pixels")
                ))
                .replace("mark@thinkpad", "mark@imac"),
            )
            .unwrap();

            let a = ingest_directory(laptop.path(), &store, "laptop")
                .await
                .unwrap();
            let b = ingest_directory(imac.path(), &store, "imac").await.unwrap();

            assert_ne!(a.node, b.node, "two runs, two receipts");
            assert_eq!(a.blobs[0].1, b.blobs[0].1, "one blob");
        });
    }

    /// A corrupted artifact must not enter the graph looking sound.
    #[test]
    fn a_digest_mismatch_is_refused() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            write_receipt(dir.path(), b"frame bytes");
            // Something ate the file in transit.
            std::fs::write(dir.path().join("frame.png"), b"truncated").unwrap();

            let store = BlobStore::new(MemoryBackend::new());
            let error = ingest_directory(dir.path(), &store, "laptop")
                .await
                .unwrap_err();

            assert!(
                matches!(error, ReceiptError::DigestMismatch { ref name, .. } if name == "frame.png"),
                "got {error}",
            );
        });
    }

    /// R1's guard. A facet the sync selection does not list is silently not
    /// projected, so a receipt gaining a third facet without gaining a lane
    /// entry would replicate half-formed and nothing would say so. This makes
    /// that a test failure instead.
    #[test]
    fn every_emitted_facet_is_in_the_sync_lane() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            write_receipt(dir.path(), b"frame bytes");
            let store = BlobStore::new(MemoryBackend::new());
            let ingested = ingest_directory(dir.path(), &store, "laptop")
                .await
                .unwrap();

            let lane = sync_facets();
            for event in &ingested.events {
                if let PersonalGraphEvent::SetFacet { facet, .. } = event {
                    assert!(
                        lane.contains(&facet.as_str()),
                        "`{facet}` is emitted but not in sync_facets(), so it \
                         would not replicate",
                    );
                }
            }

            // And the node has to match the projection rule, or a device that
            // declines the lane still materializes bare receipt nodes.
            assert!(ingested.address.starts_with(&sync_address_rule().prefix));
            assert_eq!(sync_address_rule().facet, FACET_RUN);
            assert!(lane.contains(&FACET_RUN));
        });
    }

    #[test]
    fn a_directory_without_a_manifest_is_not_a_receipt() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("stray.png"), b"x").unwrap();
            let store = BlobStore::new(MemoryBackend::new());
            assert!(matches!(
                ingest_directory(dir.path(), &store, "laptop").await,
                Err(ReceiptError::NoManifest(_))
            ));
        });
    }
}
