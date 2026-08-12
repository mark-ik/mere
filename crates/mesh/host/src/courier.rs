// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Getting a job's inputs onto the device that has to run it — gate **H1**.
//!
//! Operations replicate over LogSync; blobs do not. A signed spec names bytes
//! by content address, and the worker may never have seen them.
//!
//! **One store, not two.** A host's blob space *is* `mere-transport`'s
//! `BlobStore`: the same iroh-blobs store the transport's router already
//! serves. So staging a blob makes it fetchable by peers, the restricted
//! namespace reads out of it, and pulling one in is `fetch_from` against that
//! same store. Keeping a separate muniment space beside it would mean copying
//! every blob twice and deciding, forever, which copy is real.
//!
//! **Discovery, not transfer, was the open question.** A mesh operation is
//! signed by a key derived under [`mesh::MESH_AUTHOR_SALT`]; a transport addresses
//! the persona master key. [`mesh::DeviceDirectory`] closes that gap with
//! attestations the ring publishes about itself, so a courier is handed master
//! keys and does the obvious thing with them.

use std::collections::HashSet;
use std::sync::Arc;

use mesh::{
    BlobRef, BlobSink, BlobSource, JobControl, JobSpec, LeaseActivity, NamespaceError,
    namespace::BoxFuture,
};
use transport::{BlobHash, BlobStore, P2pandaTransport, PeerID};

use crate::host::BlobSpace;

/// Why a fetch could not happen. A blob nobody served is **not** an error here:
/// that is `Ok(false)`, and the run will fail with `MissingBlob`, which the
/// supervisor reports as `InputUnavailable` rather than as a bad worker.
#[derive(Debug, thiserror::Error)]
pub enum CourierError {
    #[error("blob reference does not carry a 32-byte digest")]
    MalformedDigest,
    #[error("blob store: {0}")]
    Store(String),
}

/// Brings a named blob onto this device from somebody who has it.
pub trait BlobCourier: Send + Sync {
    /// Try each candidate in turn. `from` holds persona **master** keys, which
    /// is what a transport can address; resolving a mesh author to one is
    /// [`mesh::DeviceDirectory`]'s job.
    ///
    /// `Ok(false)` means nobody served it — a fact about the ring, not a fault.
    fn fetch<'a>(
        &'a self,
        blob: &'a BlobRef,
        from: &'a [[u8; 32]],
    ) -> BoxFuture<'a, Result<bool, CourierError>>;
}

/// A host with no delivery lane. Every job must already hold its own inputs.
pub struct NoCourier;

impl BlobCourier for NoCourier {
    fn fetch<'a>(
        &'a self,
        _blob: &'a BlobRef,
        _from: &'a [[u8; 32]],
    ) -> BoxFuture<'a, Result<bool, CourierError>> {
        Box::pin(async move { Ok(false) })
    }
}

fn blob_hash(blob: &BlobRef) -> Result<BlobHash, CourierError> {
    blob.digest
        .as_32()
        .map(BlobHash::from_bytes)
        .map_err(|_| CourierError::MalformedDigest)
}

fn mesh_blob_tag(mesh_id: [u8; 32], digest: [u8; 32]) -> Vec<u8> {
    let mut tag = Vec::with_capacity(82);
    tag.extend_from_slice(b"mere/mesh/blob/v1\0");
    tag.extend_from_slice(&mesh_id);
    tag.extend_from_slice(&digest);
    tag
}

/// This device's blobs, in the store its transport already serves.
#[derive(Clone)]
pub struct TransportBlobSpace {
    blobs: Arc<BlobStore>,
    mesh_id: Option<[u8; 32]>,
}

impl TransportBlobSpace {
    /// An indefinitely retained transport space for compatibility callers.
    /// A production mesh host should use [`for_mesh`](Self::for_mesh), whose
    /// named custody claims can be released after an accepted checkpoint.
    pub fn new(blobs: Arc<BlobStore>) -> Self {
        Self {
            blobs,
            mesh_id: None,
        }
    }

    /// A transport space whose blobs are retained under this mesh's own tag.
    pub fn for_mesh(blobs: Arc<BlobStore>, mesh_id: [u8; 32]) -> Self {
        Self {
            blobs,
            mesh_id: Some(mesh_id),
        }
    }

    /// The underlying store, for wiring into a transport at bind time.
    pub fn store(&self) -> &Arc<BlobStore> {
        &self.blobs
    }

    /// Stage bytes this device wants to hold — and, because this is the store
    /// the router serves, wants to be able to hand out.
    pub async fn put(&self, bytes: &[u8]) -> Result<BlobRef, NamespaceError> {
        let blob = BlobRef::blake3(bytes);
        let result = match self.tag(&blob) {
            Some(tag) => self.blobs.put_bytes_named(bytes.to_vec(), tag).await,
            None => self.blobs.put_bytes(bytes.to_vec()).await,
        };
        result.map_err(|err| NamespaceError::Backend(err.to_string()))?;
        Ok(blob)
    }

    pub async fn has(&self, blob: &BlobRef) -> bool {
        let Ok(hash) = blob_hash(blob) else {
            return false;
        };
        self.blobs.has(hash).await.unwrap_or(false)
    }

    /// Release this mesh's custody claims for settled blobs.
    ///
    /// The backing store may still hold a hash because another mesh or
    /// subsystem tagged the same content. The returned count is the number of
    /// mesh-owned tags actually removed, which makes repeated maintenance
    /// idempotent.
    pub async fn release(&self, blobs: &[BlobRef]) -> Result<u64, NamespaceError> {
        if self.mesh_id.is_none() {
            return Err(NamespaceError::Backend(
                "an unscoped transport blob space cannot release custody".into(),
            ));
        }
        let mut unique = HashSet::new();
        let mut released = 0;
        for blob in blobs {
            if !unique.insert(blob.clone()) {
                continue;
            }
            let Some(tag) = self.tag(blob) else {
                continue;
            };
            if self
                .blobs
                .release(tag)
                .await
                .map_err(|err| NamespaceError::Backend(err.to_string()))?
            {
                released += 1;
            }
        }
        Ok(released)
    }

    fn tag(&self, blob: &BlobRef) -> Option<Vec<u8>> {
        let mesh_id = self.mesh_id?;
        let digest = blob.digest.as_32().ok()?;
        Some(mesh_blob_tag(mesh_id, digest))
    }
}

impl BlobSource for TransportBlobSpace {
    fn fetch<'a>(
        &'a self,
        blob: &'a BlobRef,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, NamespaceError>> {
        Box::pin(async move {
            let Ok(hash) = blob_hash(blob) else {
                return Ok(None);
            };
            if !self
                .blobs
                .has(hash)
                .await
                .map_err(|err| NamespaceError::Backend(err.to_string()))?
            {
                return Ok(None);
            }
            let bytes = self
                .blobs
                .get_bytes(hash)
                .await
                .map_err(|err| NamespaceError::Backend(err.to_string()))?;
            Ok(Some(bytes.to_vec()))
        })
    }
}

impl BlobSink for TransportBlobSpace {
    fn commit<'a>(&'a self, bytes: &'a [u8]) -> BoxFuture<'a, Result<BlobRef, NamespaceError>> {
        Box::pin(self.put(bytes))
    }
}

/// The real courier: `fetch_from` over the transport the host already has.
pub struct TransportCourier {
    transport: Arc<P2pandaTransport>,
    blobs: Arc<BlobStore>,
    mesh_id: Option<[u8; 32]>,
}

impl TransportCourier {
    pub fn new(transport: Arc<P2pandaTransport>, blobs: Arc<BlobStore>) -> Self {
        Self {
            transport,
            blobs,
            mesh_id: None,
        }
    }

    /// A courier that pins fetched bytes to one mesh's retention scope.
    pub fn for_mesh(
        transport: Arc<P2pandaTransport>,
        blobs: Arc<BlobStore>,
        mesh_id: [u8; 32],
    ) -> Self {
        Self {
            transport,
            blobs,
            mesh_id: Some(mesh_id),
        }
    }
}

impl BlobCourier for TransportCourier {
    fn fetch<'a>(
        &'a self,
        blob: &'a BlobRef,
        from: &'a [[u8; 32]],
    ) -> BoxFuture<'a, Result<bool, CourierError>> {
        Box::pin(async move {
            let hash = blob_hash(blob)?;
            for master in from {
                let Ok(peer) = PeerID::from_bytes(master) else {
                    continue;
                };
                // iroh-blobs verifies the BLAKE3 tree as it transfers, so a peer
                // that sends the wrong bytes fails here rather than handing the
                // namespace something it would have to catch later.
                let fetched = match self.mesh_id {
                    Some(mesh_id) => {
                        let tag = mesh_blob_tag(mesh_id, hash.to_bytes());
                        self.blobs
                            .fetch_from_named(&self.transport, peer, hash, tag)
                            .await
                    }
                    None => self.blobs.fetch_from(&self.transport, peer, hash).await,
                };
                if fetched.is_ok() {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }
}

/// Bring every input a job names onto this device, then say so.
///
/// Called **inside** the spawned run, after the lease is granted: fetching
/// before the grant spends bandwidth on races the device may lose. Transfer
/// time is therefore lease time, which is exactly why the run reports
/// [`LeaseActivity::Fetching`] while it happens — a downloading device and a
/// stalled one must not look the same to the ring.
pub async fn deliver_inputs(
    spec: &JobSpec,
    blobs: &dyn BlobSpace,
    courier: &dyn BlobCourier,
    from: &[[u8; 32]],
    control: &JobControl,
) {
    control.set_activity(LeaseActivity::Fetching);
    for input in &spec.inputs {
        let held = matches!(blobs.as_source().fetch(&input.blob).await, Ok(Some(_)));
        if held {
            continue;
        }
        // A failure here is deliberately not fatal: `run_job` will fail on the
        // missing input with the error that says so precisely.
        let _ = courier.fetch(&input.blob, from).await;
    }
    control.set_activity(LeaseActivity::Preparing);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn the_transport_store_addresses_blobs_the_way_the_mesh_does() {
        // The whole one-store design rests on this: iroh-blobs and `BlobRef`
        // must agree on what a blob is called.
        let space = TransportBlobSpace::new(Arc::new(BlobStore::new()));
        let bytes = b"a job's input";
        let blob = space.put(bytes).await.unwrap();
        assert_eq!(blob, BlobRef::blake3(bytes));
        assert!(space.has(&blob).await);
        assert_eq!(space.fetch(&blob).await.unwrap(), Some(bytes.to_vec()));
    }

    #[tokio::test]
    async fn an_absent_blob_reads_as_absent_rather_than_erroring() {
        let space = TransportBlobSpace::new(Arc::new(BlobStore::new()));
        let never = BlobRef::blake3(b"never stored");
        assert!(!space.has(&never).await);
        assert_eq!(space.fetch(&never).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_host_with_no_lane_says_nobody_served_it() {
        let served = NoCourier
            .fetch(&BlobRef::blake3(b"x"), &[[1; 32]])
            .await
            .unwrap();
        assert!(!served, "not an error — a fact about the ring");
    }

    #[tokio::test]
    async fn a_mesh_releases_only_its_own_custody_tag() {
        const MESH: [u8; 32] = [0x4d; 32];
        let store = Arc::new(BlobStore::new_collecting(Duration::from_millis(10)));
        let space = TransportBlobSpace::for_mesh(store.clone(), MESH);
        let bytes = b"shared content";
        let blob = space.put(bytes).await.unwrap();
        store
            .put_bytes_named(bytes.to_vec(), b"eidetic/shared")
            .await
            .unwrap();

        assert_eq!(space.release(std::slice::from_ref(&blob)).await.unwrap(), 1);
        assert_eq!(space.release(std::slice::from_ref(&blob)).await.unwrap(), 0);
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(
            space.has(&blob).await,
            "eidetic's independent tag keeps the shared bytes present"
        );
    }
}
