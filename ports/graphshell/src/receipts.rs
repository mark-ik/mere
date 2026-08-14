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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use muniment::{Backend, BlobStore, Hash, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::personal_sync::{
    BlobAvailabilityObservation, PersonalGraphEvent, SyntheticAddressRule,
};

/// Namespace for deriving a receipt's node id from its address.
///
/// A v5 (name-based) UUID, not v4: the id has to be a function of the receipt
/// so that ingesting the same directory twice reaches the same node instead of
/// minting a second one.
const RECEIPT_NAMESPACE: Uuid = Uuid::from_u128(0x8f2c_41d7_5e3b_4a90_9c17_6de2_0b84_f5a3);

/// The facet carrying a run's provenance.
pub const FACET_RUN: &str = "receipt.run";
/// The facet listing a run's artifacts.
pub const FACET_ARTIFACTS: &str = "receipt.artifacts";
/// Address prefix for every receipt node.
pub const ADDRESS_PREFIX: &str = "receipt:";

/// The facets a replica must select for receipts to arrive with their
/// provenance.
///
/// Named here rather than spelled into the sync wiring, because a facet the
/// selection does not list is silently not projected: receipts would replicate
/// as bare titled nodes carrying none of the context that makes them evidence.
pub fn sync_facets() -> [&'static str; 2] {
    [FACET_RUN, FACET_ARTIFACTS]
}

/// The projection rule for receipt nodes.
///
/// Receipt addresses are synthetic (a run is not a navigable location), so
/// they follow their facet the way the transfer carrier does: a device that
/// does not select [`FACET_RUN`] does not materialize receipt nodes at all,
/// rather than showing a list of titles with nothing behind them. Not
/// device-scoped — a receipt from the ThinkPad is exactly what the laptop
/// wants to see.
pub fn sync_address_rule() -> SyntheticAddressRule {
    SyntheticAddressRule {
        prefix: ADDRESS_PREFIX.to_string(),
        facet: FACET_RUN.to_string(),
        device_scoped: false,
    }
}

/// What went wrong reading or ingesting a receipt directory.
#[derive(Debug)]
pub enum ReceiptError {
    /// The directory has no `manifest.json`, so nothing here has provenance.
    NoManifest(PathBuf),
    /// The manifest did not parse.
    Manifest(serde_json::Error),
    /// An artifact named in the manifest is missing from the directory.
    MissingArtifact(String),
    /// An artifact's bytes do not match the digest the producing machine
    /// recorded. The transfer corrupted it, or the manifest is not this
    /// directory's.
    DigestMismatch {
        /// The artifact's file name.
        name: String,
        /// What the manifest claimed.
        expected: String,
        /// What the bytes here actually hash to.
        found: String,
    },
    /// Reading a file failed.
    Io(std::io::Error),
    /// Writing a blob failed.
    Store(StoreError),
}

impl std::fmt::Display for ReceiptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoManifest(path) => {
                write!(f, "no manifest.json in {}", path.display())
            }
            Self::Manifest(error) => write!(f, "manifest.json did not parse: {error}"),
            Self::MissingArtifact(name) => {
                write!(f, "manifest names `{name}`, which is not in the directory")
            }
            Self::DigestMismatch {
                name,
                expected,
                found,
            } => write!(
                f,
                "`{name}` does not match its recorded digest \
                 (expected {expected}, found {found})"
            ),
            Self::Io(error) => write!(f, "reading the receipt failed: {error}"),
            Self::Store(error) => write!(f, "storing a blob failed: {error:?}"),
        }
    }
}

impl std::error::Error for ReceiptError {}

impl From<std::io::Error> for ReceiptError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<StoreError> for ReceiptError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// One artifact as the producing machine recorded it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ManifestArtifact {
    /// File name within the receipt directory.
    pub name: String,
    /// Size in bytes.
    pub bytes: u64,
    /// Lowercase hex SHA-256, computed where the artifact was produced.
    pub sha256: String,
}

/// `manifest.json`, as written by the remote-receipt lane.
///
/// Unknown fields are kept rather than rejected: a newer lane may record more
/// provenance than this build knows about, and dropping it on ingest would
/// lose exactly the context the receipt exists to carry.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReceiptManifest {
    /// Repository the scenario belongs to.
    pub repo: String,
    /// Cargo package that ran.
    pub package: String,
    /// Scenario path, relative to the checkout.
    pub scenario: String,
    /// SSH destination the run happened on.
    pub target: String,
    /// `linux` / `macos` / `windows`.
    pub platform: String,
    /// `uname -sr` (or equivalent) of the producing machine.
    pub remote_os: String,
    /// Commit the checkout was on.
    pub remote_commit: String,
    /// How many files were dirty. Non-zero means the receipt is not
    /// attributable to a commit alone.
    #[serde(default)]
    pub remote_dirty: u32,
    /// `wayland (…)`, `x11 (…)`, `aqua`.
    #[serde(default)]
    pub session: String,
    /// When the run happened, RFC 3339.
    pub ran_at_utc: String,
    /// Process exit code.
    pub exit_code: i32,
    /// The artifacts, with the digests to verify them against.
    #[serde(default)]
    pub artifacts: Vec<ManifestArtifact>,
    /// Everything else the lane recorded, preserved untouched.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ReceiptManifest {
    /// Parse `manifest.json`.
    pub fn parse(json: &str) -> Result<Self, ReceiptError> {
        serde_json::from_str(json).map_err(ReceiptError::Manifest)
    }

    /// The receipt's stable address: what identifies this run forever.
    ///
    /// Built from the run's own facts rather than the directory name, so the
    /// same run ingested from a copied or renamed directory is still the same
    /// receipt.
    pub fn address(&self) -> String {
        format!(
            "receipt:{}/{}/{}/{}",
            self.repo, self.host(), self.scenario, self.ran_at_utc
        )
    }

    /// The machine part of `target`.
    pub fn host(&self) -> &str {
        self.target.rsplit('@').next().unwrap_or(&self.target)
    }

    /// The node id this receipt always lands on.
    pub fn node_id(&self) -> Uuid {
        Uuid::new_v5(&RECEIPT_NAMESPACE, self.address().as_bytes())
    }

    /// A one-line title for a list.
    pub fn title(&self) -> String {
        let verdict = if self.exit_code == 0 { "ok" } else { "failed" };
        format!(
            "{} · {} on {} · {verdict}",
            self.repo,
            scenario_name(&self.scenario),
            self.host()
        )
    }

    /// Whether the run passed.
    pub fn passed(&self) -> bool {
        self.exit_code == 0
    }

    /// The run's timestamp in unix milliseconds, for the facts that need one.
    /// `0` when the manifest's timestamp does not parse, which keeps ingest
    /// total rather than making a clock the failure path.
    pub fn ran_at_ms(&self) -> u64 {
        parse_rfc3339_ms(&self.ran_at_utc).unwrap_or(0)
    }
}

/// The bare scenario file name, for titles and tags.
fn scenario_name(scenario: &str) -> &str {
    scenario
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(scenario)
        .trim_end_matches(".scn")
}

/// Minimal RFC 3339 → unix milliseconds.
///
/// Hand-rolled because this crate has no date dependency and needs exactly one
/// direction of one format: the lane writes `o`-round-trip UTC. Anything else
/// returns `None` and the caller treats the time as unknown rather than
/// failing an otherwise-good ingest.
fn parse_rfc3339_ms(text: &str) -> Option<u64> {
    let text = text.trim();
    let (date, rest) = text.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;

    let time = rest
        .trim_end_matches('Z')
        .split_once('+')
        .map(|(t, _)| t)
        .unwrap_or_else(|| rest.trim_end_matches('Z'));
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let seconds_field = time_parts.next()?;
    let (secs, frac) = seconds_field
        .split_once('.')
        .unwrap_or((seconds_field, "0"));
    let second: i64 = secs.parse().ok()?;
    let millis: i64 = format!("{frac:0<3}")[..3].parse().ok()?;

    // Days from the civil calendar (Howard Hinnant's algorithm), which is
    // exact and needs no table.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    let total = ((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000) + millis;
    u64::try_from(total).ok()
}

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
        if manifest.passed() { "receipt:ok".into() } else { "receipt:failed".into() },
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
        let other = ReceiptManifest::parse(
            &manifest_json("").replace("mark@thinkpad", "mark@imac"),
        )
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
    fn rfc3339_parses_to_unix_millis() {
        // 2026-08-10T14:31:05.123Z. Derived by hand so the constant is an
        // independent check on the algorithm rather than a restatement of it:
        // 20675 days from the epoch (20454 to 2026-01-01, of which 14 are leap
        // days, plus 221 into the year), 20675 * 86400 = 1_786_320_000s, plus
        // 14:31:05 = 52_265s, plus 123ms.
        assert_eq!(
            parse_rfc3339_ms("2026-08-10T14:31:05.1234567Z"),
            Some(1_786_372_265_123),
        );
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00.000Z"), Some(0));
        // Sub-second precision is optional and over-long fractions truncate
        // rather than fail; the lane writes seven digits.
        assert_eq!(
            parse_rfc3339_ms("2026-08-10T14:31:05Z"),
            Some(1_786_372_265_000),
        );
        assert_eq!(parse_rfc3339_ms("not a time"), None);
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
                    .filter(|e| matches!(
                        e,
                        PersonalGraphEvent::ObserveBlobAvailability { .. }
                    ))
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

            let a = ingest_directory(laptop.path(), &store, "laptop").await.unwrap();
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
