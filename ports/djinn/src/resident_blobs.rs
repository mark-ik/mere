//! Djinn-owned physical blob custody.
//!
//! Personal graph transfers and Knot evidence have distinct domain leases and
//! serving authority, but they borrow one iroh store from the process owner.
//! Old per-lane stores are copied and verified here; they are never deleted.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use transport::{BlobHash, BlobLease, BlobReadAuthorizer, BlobScope, BlobStore};

use crate::settings::{ResidentContentSettings, hex32, parse_hex32};

/// Lease namespace for personal-graph bytes staged on this device.
pub const PERSONAL_STAGE_LEASE: &str = "graphshell.transfer";
/// Lease namespace for personal-graph bytes fetched from a sibling.
pub const PERSONAL_FETCH_LEASE: &str = "graphshell.fetch";
/// Namespace used while importing the old per-graph blob store.
pub const LEGACY_PERSONAL_LEASE: &str = "legacy.personal";
/// Namespace used while importing the old per-persona evidence store.
pub const LEGACY_KNOT_LEASE: &str = "legacy.knot";

/// Result of considering one legacy store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyBlobMigration {
    /// The old root does not exist. No completion marker was written.
    SourceAbsent,
    /// The source is the shared root already, so there is nothing to import.
    AlreadyShared,
    /// A prior verified import remains complete.
    AlreadyComplete { blobs: usize },
    /// This run copied and verified all retained source blobs.
    Copied { blobs: usize },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct MigrationMarker {
    schema: String,
    source: String,
    scope: String,
    namespace: String,
    hashes: Vec<String>,
}

/// The device host's single physical iroh store and serving authorizer.
#[derive(Clone)]
pub struct ResidentBlobCustody {
    root: PathBuf,
    migration_root: PathBuf,
    blobs: Arc<BlobStore>,
    readers: BlobReadAuthorizer,
}

impl ResidentBlobCustody {
    /// Open the resident store and re-use it for every composed content lane.
    pub async fn open(
        data_root: &Path,
        settings: &ResidentContentSettings,
    ) -> Result<Self, String> {
        if settings.gc_interval_seconds == 0 {
            return Err("resident content gc_interval_seconds must be greater than zero".into());
        }
        let content_root = data_root.join("content");
        let root = settings
            .root
            .clone()
            .unwrap_or_else(|| content_root.join("blobs"));
        let blobs =
            BlobStore::open_collecting(&root, Duration::from_secs(settings.gc_interval_seconds))
                .await
                .map_err(|error| format!("could not open resident blob store: {error}"))?;
        Ok(Self {
            root,
            migration_root: content_root.join("migrations"),
            blobs: Arc::new(blobs),
            readers: BlobReadAuthorizer::new(),
        })
    }

    pub fn blobs(&self) -> Arc<BlobStore> {
        Arc::clone(&self.blobs)
    }

    pub fn authorizer(&self) -> BlobReadAuthorizer {
        self.readers.clone()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Rebuild in-memory serving custody from durable leases after restart.
    pub async fn bind_scope(&self, scope: BlobScope) -> Result<usize, String> {
        let hashes = self
            .blobs
            .leased_hashes(scope)
            .await
            .map_err(|error| format!("could not read resident blob leases: {error}"))?;
        for hash in &hashes {
            self.readers.retain(scope, *hash);
        }
        Ok(hashes.len())
    }

    /// Copy all tagged bytes from an old lane store under verified legacy
    /// leases. The source remains untouched, and the marker lands only after
    /// every destination hash and lease has been verified and flushed.
    pub async fn migrate_legacy_store(
        &self,
        source: &Path,
        scope: BlobScope,
        namespace: &str,
    ) -> Result<LegacyBlobMigration, String> {
        if !source.exists() {
            return Ok(LegacyBlobMigration::SourceAbsent);
        }
        if same_path(source, &self.root)? {
            return Ok(LegacyBlobMigration::AlreadyShared);
        }
        std::fs::create_dir_all(&self.migration_root)
            .map_err(|error| format!("could not create blob migration directory: {error}"))?;
        let source_name = normalized_path(source)?;
        let marker_path = self.marker_path(&source_name, scope, namespace);
        if marker_path.exists() {
            let marker = load_marker(&marker_path)?;
            self.verify_marker(&marker, &source_name, scope, namespace)
                .await?;
            return Ok(LegacyBlobMigration::AlreadyComplete {
                blobs: marker.hashes.len(),
            });
        }

        let legacy = BlobStore::open(source)
            .await
            .map_err(|error| format!("could not open legacy blob store: {error}"))?;
        let hashes = legacy
            .retained_hashes()
            .await
            .map_err(|error| format!("could not list legacy blob store: {error}"))?;
        let mut copied = Vec::with_capacity(hashes.len());
        for hash in hashes {
            let bytes = legacy.get_bytes(hash).await.map_err(|error| {
                format!(
                    "could not read legacy blob {}: {error}",
                    hex32(hash.as_bytes())
                )
            })?;
            let lease = BlobLease::new(scope, namespace, hash.as_bytes())
                .map_err(|error| format!("could not name imported blob lease: {error}"))?;
            let installed = self
                .blobs
                .put_bytes_leased(bytes, &lease)
                .await
                .map_err(|error| format!("could not import legacy blob: {error}"))?;
            if installed != hash {
                return Err(format!(
                    "legacy blob {} changed digest while importing",
                    hex32(hash.as_bytes())
                ));
            }
            copied.push(hash);
        }
        self.blobs
            .flush()
            .await
            .map_err(|error| format!("could not flush imported blobs: {error}"))?;
        legacy
            .shutdown()
            .await
            .map_err(|error| format!("could not close legacy blob store: {error}"))?;

        let marker = MigrationMarker {
            schema: "mere.graphshell/blob-migration/v1".into(),
            source: source_name,
            scope: hex32(&scope.to_bytes()),
            namespace: namespace.into(),
            hashes: copied.iter().map(|hash| hex32(hash.as_bytes())).collect(),
        };
        save_marker(&marker_path, &marker)?;
        self.bind_scope(scope).await?;
        Ok(LegacyBlobMigration::Copied {
            blobs: copied.len(),
        })
    }

    async fn verify_marker(
        &self,
        marker: &MigrationMarker,
        source: &str,
        scope: BlobScope,
        namespace: &str,
    ) -> Result<(), String> {
        if marker.schema != "mere.graphshell/blob-migration/v1"
            || marker.source != source
            || marker.scope != hex32(&scope.to_bytes())
            || marker.namespace != namespace
        {
            return Err(
                "resident blob migration marker does not match its source and scope".into(),
            );
        }
        for value in &marker.hashes {
            let hash = BlobHash::from_bytes(
                parse_hex32(value).map_err(|error| format!("migration marker hash: {error}"))?,
            );
            let lease = BlobLease::new(scope, namespace, hash.as_bytes())
                .map_err(|error| format!("could not name imported blob lease: {error}"))?;
            if self
                .blobs
                .lease_hash(&lease)
                .await
                .map_err(|error| format!("could not verify imported blob lease: {error}"))?
                != Some(hash)
            {
                return Err(format!(
                    "resident blob migration marker claims a missing lease for {value}"
                ));
            }
            let bytes = self
                .blobs
                .get_bytes(hash)
                .await
                .map_err(|error| format!("could not verify imported blob {value}: {error}"))?;
            if blake3::hash(&bytes).as_bytes() != hash.as_bytes() {
                return Err(format!(
                    "resident blob migration marker claims corrupt blob {value}"
                ));
            }
            self.readers.retain(scope, hash);
        }
        Ok(())
    }

    fn marker_path(&self, source: &str, scope: BlobScope, namespace: &str) -> PathBuf {
        let source_hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let namespace = namespace.replace('.', "-");
        self.migration_root.join(format!(
            "{}-{namespace}-{}.json",
            hex32(&scope.to_bytes()),
            &source_hash[..16]
        ))
    }

    /// Flush and shut down the physical store after every borrower is gone.
    pub async fn shutdown(self) -> Result<(), String> {
        let blobs = Arc::try_unwrap(self.blobs)
            .map_err(|_| "resident blob store still has active borrowers".to_string())?;
        blobs
            .shutdown()
            .await
            .map_err(|error| format!("could not shut down resident blob store: {error}"))
    }
}

fn normalized_path(path: &Path) -> Result<String, String> {
    std::fs::canonicalize(path)
        .map(|path| path.display().to_string())
        .map_err(|error| format!("could not resolve blob store {}: {error}", path.display()))
}

fn same_path(left: &Path, right: &Path) -> Result<bool, String> {
    Ok(normalized_path(left)?.eq_ignore_ascii_case(&normalized_path(right)?))
}

fn load_marker(path: &Path) -> Result<MigrationMarker, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not read blob migration marker: {error}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("could not decode blob migration marker: {error}"))
}

fn save_marker(path: &Path, marker: &MigrationMarker) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(marker)
        .map_err(|error| format!("could not encode blob migration marker: {error}"))?;
    bytes.push(b'\n');
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("could not write blob migration marker: {error}"))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("could not install blob migration marker: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn migration_is_verified_restart_safe_and_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_root = temp.path().join("old.blobs");
        let legacy = BlobStore::open(&legacy_root).await.unwrap();
        let first = legacy
            .put_bytes_named(b"first legacy blob".to_vec(), b"old/first")
            .await
            .unwrap();
        let second = legacy
            .put_bytes_named(b"second legacy blob".to_vec(), b"old/second")
            .await
            .unwrap();
        legacy.shutdown().await.unwrap();
        let mut expected = vec![first, second];
        expected.sort_unstable();

        let scope = BlobScope::new([0x81; 32]);
        let custody = ResidentBlobCustody::open(
            &temp.path().join("data"),
            &ResidentContentSettings {
                gc_interval_seconds: 60,
                ..ResidentContentSettings::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            custody
                .migrate_legacy_store(&legacy_root, scope, LEGACY_PERSONAL_LEASE)
                .await
                .unwrap(),
            LegacyBlobMigration::Copied { blobs: 2 }
        );
        assert_eq!(
            custody.blobs().leased_hashes(scope).await.unwrap(),
            expected
        );

        // Losing only the final marker models interruption after verified copy.
        // The source remains, so the next run repeats idempotently and marks it.
        let marker = custody.marker_path(
            &normalized_path(&legacy_root).unwrap(),
            scope,
            LEGACY_PERSONAL_LEASE,
        );
        std::fs::remove_file(marker).unwrap();
        assert_eq!(
            custody
                .migrate_legacy_store(&legacy_root, scope, LEGACY_PERSONAL_LEASE)
                .await
                .unwrap(),
            LegacyBlobMigration::Copied { blobs: 2 }
        );
        assert_eq!(
            custody
                .migrate_legacy_store(&legacy_root, scope, LEGACY_PERSONAL_LEASE)
                .await
                .unwrap(),
            LegacyBlobMigration::AlreadyComplete { blobs: 2 }
        );
        drop(custody.blobs());
        custody.shutdown().await.unwrap();

        let reopened = ResidentBlobCustody::open(
            &temp.path().join("data"),
            &ResidentContentSettings {
                gc_interval_seconds: 60,
                ..ResidentContentSettings::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(reopened.bind_scope(scope).await.unwrap(), 2);
        assert_eq!(
            reopened.blobs().leased_hashes(scope).await.unwrap(),
            expected
        );
        drop(reopened.blobs());
        reopened.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn absent_source_does_not_claim_completion() {
        let temp = tempfile::tempdir().unwrap();
        let custody = ResidentBlobCustody::open(
            &temp.path().join("data"),
            &ResidentContentSettings::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            custody
                .migrate_legacy_store(
                    &temp.path().join("missing"),
                    BlobScope::new([0x91; 32]),
                    LEGACY_KNOT_LEASE,
                )
                .await
                .unwrap(),
            LegacyBlobMigration::SourceAbsent
        );
        assert!(!custody.migration_root.exists());
        custody.shutdown().await.unwrap();
    }
}
