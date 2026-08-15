//! Stage-and-forward: putting every byte a transfer names onto the destination
//! before anything is applied.
//!
//! `apply_transfer` takes a source blob store and a destination blob store. On
//! one machine that is two arguments; across two devices it would be a store
//! that lives somewhere else, and every read inside apply would become a
//! network call that can fail halfway through a graph mutation. Fetching first
//! dissolves that: once the destination holds the manifest and every blob it
//! names, apply runs entirely locally and both arguments are the same store.
//!
//! Three BLAKE3 hash types meet here. `eidetic::Hash` addresses manifest
//! descriptors, `muniment::Hash` keys the product blob store, and
//! `transport::BlobHash` keys the iroh store the sync host serves. They agree
//! bytewise for the same content, and this module checks that rather than
//! assuming it: a silent disagreement would offer blobs under hashes no
//! destination can ask for.

use eidetic::Hash;
use chirograph::ContentHash;
use muniment::{Backend, BlobStore};
use transport::BlobHash;

use crate::native::personal_sync_host::{PersonalSyncHost, PersonalSyncHostError};
use crate::transfer::{TransferManifestV1, TransferOperation};
use crate::transfer_offer::{OfferError, TransferOfferV1, offer_events, offer_for};

#[derive(Debug, thiserror::Error)]
pub enum StagingError {
    #[error(transparent)]
    Host(#[from] PersonalSyncHostError),
    #[error(transparent)]
    Store(#[from] muniment::StoreError),
    #[error(transparent)]
    Offer(#[from] OfferError),
    #[error("transfer manifest could not be encoded: {0}")]
    Encode(String),
    #[error("blob {hash} named by the manifest is not in the source store")]
    MissingSourceBlob { hash: String },
    /// The three blob addressings disagreed about the same bytes.
    #[error("{context}: staged {staged} but the manifest names {named}")]
    HashDisagreement {
        context: &'static str,
        staged: String,
        named: String,
    },
    #[error("offer names transfer {offered} but the fetched manifest is {fetched}")]
    ManifestMismatch { offered: String, fetched: String },
    #[error(
        "this device does not advertise blob availability, so a sibling could \
         not learn where the transfer's bytes are; enable the blob-availability \
         lane before offering a transfer"
    )]
    BlobLaneDisabled,
}

/// Stage a prepared manifest and its blobs for delivery, then announce it.
///
/// Order matters. Every blob is staged and advertised before the offer is
/// authored, so a destination that acts on the offer the instant it arrives
/// finds bytes rather than a promise. `stage_blob` already flushes before it
/// advertises each blob for the same reason.
pub async fn offer_transfer<B: Backend + Clone + Send + Sync + 'static>(
    host: &PersonalSyncHost,
    source_blobs: &BlobStore<B>,
    manifest: &TransferManifestV1,
    pairing_id: &str,
    at_ms: u64,
) -> Result<TransferOfferV1, StagingError> {
    // Checked before the first byte moves. Staging authors an availability
    // record per blob, so a host with that lane off fails partway through with
    // bytes already written and nothing advertised.
    if !host.serves_blobs().await {
        return Err(StagingError::BlobLaneDisabled);
    }
    for descriptor in &manifest.blobs {
        let key = muniment_hash(descriptor.content_hash)?;
        let bytes =
            source_blobs
                .get(&key)
                .await?
                .ok_or_else(|| StagingError::MissingSourceBlob {
                    hash: descriptor.content_hash.to_hex(),
                })?;
        let staged = host.stage_blob(descriptor.node_id, bytes).await?;
        if &staged != descriptor.content_hash.as_bytes() {
            return Err(StagingError::HashDisagreement {
                context: "staging a transfer blob",
                staged: hex(&staged),
                named: descriptor.content_hash.to_hex(),
            });
        }
    }

    let manifest_bytes =
        serde_json::to_vec(manifest).map_err(|error| StagingError::Encode(error.to_string()))?;
    let manifest_blob = Hash::of(&manifest_bytes);
    let byte_len = manifest_bytes.len() as u64;
    let staged = host
        .stage_blob(manifest.transfer_id, manifest_bytes)
        .await?;
    if &staged != manifest_blob.as_bytes() {
        return Err(StagingError::HashDisagreement {
            context: "staging the transfer manifest",
            staged: hex(&staged),
            named: manifest_blob.to_hex(),
        });
    }

    let offer = offer_for(manifest, manifest_blob, byte_len, pairing_id, at_ms)?;
    host.author(offer_events(&offer)?).await?;
    tracing::info!(
        transfer = %manifest.transfer_id,
        blobs = manifest.blobs.len(),
        bytes = offer.blob_bytes,
        "offered a transfer to a paired device"
    );
    Ok(offer)
}

/// Fetch everything an offer names into the local stores, and return the
/// manifest that governs the apply.
///
/// The returned manifest comes off the wire, not from the offer: the offer's
/// counts are a display summary and this is the authoritative record. The two
/// are checked against each other for the one thing that would make the pair
/// incoherent, which is naming different transfers.
///
/// Idempotent. `fetch_blob_by_availability` short-circuits on bytes already
/// held, so a re-run after a partial fetch costs a store lookup per blob.
pub async fn receive_transfer<B: Backend + Clone + Send + Sync + 'static>(
    host: &PersonalSyncHost,
    destination_blobs: &BlobStore<B>,
    offer: &TransferOfferV1,
) -> Result<TransferManifestV1, StagingError> {
    let manifest_bytes = fetch_into(host, *offer.manifest_blob.as_bytes()).await?;
    let manifest: TransferManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| StagingError::Encode(error.to_string()))?;
    if manifest.transfer_id != offer.transfer_id {
        return Err(StagingError::ManifestMismatch {
            offered: offer.transfer_id.to_string(),
            fetched: manifest.transfer_id.to_string(),
        });
    }

    for descriptor in &manifest.blobs {
        let bytes = fetch_into(host, *descriptor.content_hash.as_bytes()).await?;
        // Writing through the product store returns its own addressing of the
        // same bytes, which is the cheapest place to notice the three hash
        // constructions drifting apart.
        let stored = destination_blobs.put(&bytes).await?;
        if stored.as_bytes() != descriptor.content_hash.as_bytes() {
            return Err(StagingError::HashDisagreement {
                context: "storing a fetched transfer blob",
                staged: stored.to_hex(),
                named: descriptor.content_hash.to_hex(),
            });
        }
    }
    tracing::info!(
        transfer = %manifest.transfer_id,
        blobs = manifest.blobs.len(),
        operation = ?manifest.operation,
        "staged a transfer locally; apply no longer needs the source"
    );
    Ok(manifest)
}

/// The blobs a staged transfer needs handed to the browser, ready to release.
///
/// Read from the local store, so this is only callable after
/// [`receive_transfer`] has put every byte here. Returned rather than released
/// directly because releasing is a grant: what makes bytes reachable from a
/// browser is a person accepting the transfer, not this device holding them.
pub async fn released_blobs_for(
    host: &PersonalSyncHost,
    manifest: &TransferManifestV1,
) -> Result<Vec<(ContentHash, Vec<u8>)>, StagingError> {
    let mut released = Vec::with_capacity(manifest.blobs.len());
    for descriptor in &manifest.blobs {
        let blob = *descriptor.content_hash.as_bytes();
        let bytes = host
            .blobs()
            .get_bytes(BlobHash::from_bytes(blob))
            .await
            .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?
            .to_vec();
        // The browser addresses resources by its own hash type. Deriving it
        // from the bytes rather than converting the manifest's hash means a
        // disagreement surfaces here instead of as a resource the browser
        // asks for and never receives.
        let resource = ContentHash::of(&bytes);
        if resource.0 != blob {
            return Err(StagingError::HashDisagreement {
                context: "releasing a transfer blob to a browser",
                staged: format!("{resource}"),
                named: descriptor.content_hash.to_hex(),
            });
        }
        released.push((resource, bytes));
    }
    Ok(released)
}

/// Whether this offer may be applied here at all, before any bytes move.
///
/// Grant is the pairing, not the device: `pairing_id` is minted at pair time
/// and retired by unpair, so a transfer queued under a retired pairing is
/// refused even though the two node ids still match.
pub fn offer_is_grantable(offer: &TransferOfferV1, current_pairing: Option<&str>) -> bool {
    match current_pairing {
        Some(pairing) => offer.pairing_id == pairing,
        None => false,
    }
}

/// Whether a replicate offer's endpoints agree with its operation. Copy across
/// personas is legitimate; replicate across them is not, and `apply_transfer`
/// refuses it, so catching it here saves a fetch.
pub fn offer_is_coherent(offer: &TransferOfferV1) -> bool {
    offer.operation != TransferOperation::Replicate
        || offer.source.persona == offer.destination.persona
}

async fn fetch_into(host: &PersonalSyncHost, blob: [u8; 32]) -> Result<Vec<u8>, StagingError> {
    host.fetch_blob_by_availability(blob).await?;
    let bytes = host
        .blobs()
        .get_bytes(BlobHash::from_bytes(blob))
        .await
        .map_err(|error| PersonalSyncHostError::Transport(error.to_string()))?;
    Ok(bytes.to_vec())
}

/// Both are BLAKE3 of the same bytes; hex is the only conversion Muniment
/// exposes. A failure here means the two are no longer the same digest, which
/// is worth an error rather than an unwrap.
fn muniment_hash(hash: Hash) -> Result<muniment::Hash, StagingError> {
    muniment::Hash::from_hex(&hash.to_hex()).ok_or_else(|| StagingError::HashDisagreement {
        context: "reading a manifest blob from the product store",
        staged: "not a Muniment BLAKE3 hash".to_string(),
        named: hash.to_hex(),
    })
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::AccessContext;
    use crate::mere_host::{MereHost, SelectedPersonaRef, fixture_handlers};
    use crate::native::personal_sync_host::PersonalSyncHostConfig;
    use crate::personal_sync::{SyncRoster, SyncSelection};
    use crate::product::{EditableRelation, ExportRequest, LocalFileMetadata, TransferScope};
    use crate::transfer::{
        AccessTransferPolicy, ApplyTransferContext, TransferAuthorization, TransferEndpointV1,
        TransferRequest, TransferRouteV1, apply_transfer, prepare_transfer,
    };
    use crate::transfer_offer::TRANSFER_OFFER_FACET;
    use eidetic::PrivacyClass;
    use muniment::MemoryBackend;
    use personae::{IdentityProvider, InMemoryProvider};
    use sha2::{Digest, Sha256};
    use uuid::Uuid;

    const PERSONA: &str = "personae://persona/staging";
    const FILE_BYTES: &[u8] = b"bytes that must survive the source going away\n";

    fn selected() -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: PERSONA.to_string(),
            profile: "profile:graphshell-s3".to_string(),
        }
    }

    fn endpoint(device: [u8; 32]) -> TransferEndpointV1 {
        TransferEndpointV1 {
            graph: "graphshell://graph/staging".to_string(),
            persona: PERSONA.to_string(),
            device: format!("personae://device/{}", hex(&device)),
        }
    }

    /// A source graph holding one URL node and one real file, related.
    fn source_host(device: [u8; 32]) -> (MereHost<MemoryBackend>, MemoryBackend, Uuid, Uuid) {
        let backend = MemoryBackend::new();
        let mut host = MereHost::empty(
            backend.clone(),
            selected(),
            fixture_handlers(),
            AccessContext {
                persona: PERSONA.to_string(),
                device: format!("personae://device/{}", hex(&device)),
                at_ms: 1_700_000_000_000,
            },
        );
        let url = host
            .create_address("https://example.test/s3", "S3 staging notes")
            .unwrap();
        host.edit_node(url, "S3 staging notes", ["transport".to_string()])
            .unwrap();
        let file = host
            .create_file_metadata(LocalFileMetadata {
                content_hash: Sha256::digest(FILE_BYTES)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
                name: "s3-reference.txt".to_string(),
                media_type: "text/plain".to_string(),
                byte_len: FILE_BYTES.len() as u64,
                last_modified_ms: 1_700_000_000_000,
            })
            .unwrap();
        host.edit_node(file, "S3 real file", ["file".to_string()])
            .unwrap();
        host.assert_product_relation(file, url, EditableRelation::Cites)
            .unwrap();
        (host, backend, url, file)
    }

    /// The whole point of stage-and-forward: by the time apply runs, the source
    /// is gone. Its sync host is closed and its blob store is never handed to
    /// apply, which receives the destination's store as both arguments. If any
    /// byte were still being read across the wire, this test could not pass.
    #[tokio::test]
    async fn a_transfer_applies_after_its_source_is_gone() {
        let directory = tempfile::tempdir().unwrap();
        let graph = [0xa3; 32];
        let source_identity = InMemoryProvider::from_seed([0xa4; 32]);
        let destination_identity = InMemoryProvider::from_seed([0xa5; 32]);
        let roster = SyncRoster::new([
            source_identity.master_public_key().to_bytes(),
            destination_identity.master_public_key().to_bytes(),
        ]);
        // Offers announce a transfer; blob availability is how its bytes are
        // found. A transfer needs both lanes, which `offer_transfer` checks.
        let selection = || {
            SyncSelection::default()
                .with_facets([TRANSFER_OFFER_FACET])
                .with_blob_availability(true)
        };

        let source_sync = PersonalSyncHost::open(
            &source_identity,
            PersonalSyncHostConfig {
                graph,
                store_path: directory.path().join("source.redb"),
                roster: roster.clone(),
                selection: selection(),
                peer_tickets: Vec::new(),
                peer_hints: Vec::new(),
                paired_nodes: Vec::new(),
                relay_urls: Vec::new(),
            },
        )
        .await
        .unwrap();
        let destination_sync = PersonalSyncHost::open(
            &destination_identity,
            PersonalSyncHostConfig {
                graph,
                store_path: directory.path().join("destination.redb"),
                roster,
                selection: selection(),
                peer_tickets: vec![source_sync.ticket().await.unwrap()],
                peer_hints: Vec::new(),
                paired_nodes: Vec::new(),
                relay_urls: Vec::new(),
            },
        )
        .await
        .unwrap();
        source_sync
            .pair_node(destination_sync.node_id())
            .await
            .unwrap();

        let (source, source_backend, url, file) = source_host(source_sync.node_id());
        let source_blobs = BlobStore::new(source_backend.clone());
        let mut source_authority = source_backend.clone();
        let manifest = prepare_transfer(
            &source,
            &mut source_authority,
            &source_blobs,
            TransferRequest {
                transfer_id: Uuid::new_v4(),
                operation: TransferOperation::Replicate,
                source: endpoint(source_sync.node_id()),
                destination: endpoint(destination_sync.node_id()),
                route: TransferRouteV1 {
                    carrier: "personal-sync".to_string(),
                    peer: hex(&destination_sync.node_id()),
                },
                selection: ExportRequest {
                    focused: file,
                    selected: vec![file, url],
                    scope: TransferScope::SelectedSubgraph,
                    exported_at_ms: 1_700_000_000_500,
                    include_local_file_locations: true,
                    scene: None,
                },
                access_policy: AccessTransferPolicy::ExcludeSourceHistory,
                privacy: PrivacyClass::TrustedPeersOnly,
            },
            vec![crate::transfer::TransferBlobInput {
                node_id: file,
                role: "primary".to_string(),
                media_type: "text/plain".to_string(),
                bytes: FILE_BYTES.to_vec(),
            }],
        )
        .await
        .unwrap();

        let offered = offer_transfer(
            &source_sync,
            &source_blobs,
            &manifest,
            "pairing-s3",
            1_700_000_002_000,
        )
        .await
        .unwrap();
        assert!(offer_is_coherent(&offered));
        assert!(offer_is_grantable(&offered, Some("pairing-s3")));
        assert!(
            !offer_is_grantable(&offered, Some("pairing-s3-after-repair")),
            "a re-paired device must not inherit a queued transfer"
        );

        // The destination learns of the transfer from the graph, not the test.
        let mut incoming = Vec::new();
        for _ in 0..80 {
            incoming = destination_sync.offers().await.unwrap();
            if !incoming.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(incoming, vec![offered.clone()]);

        let destination_backend = MemoryBackend::new();
        let destination_blobs = BlobStore::new(destination_backend.clone());
        let fetched = receive_transfer(&destination_sync, &destination_blobs, &incoming[0])
            .await
            .unwrap();
        assert_eq!(fetched.transfer_id, manifest.transfer_id);

        // Everything the source contributes ends here.
        source_sync.close().await.unwrap();
        drop(source_blobs);
        drop(source);

        let mut destination = MereHost::empty(
            destination_backend.clone(),
            selected(),
            fixture_handlers(),
            AccessContext {
                persona: PERSONA.to_string(),
                device: format!("personae://device/{}", hex(&destination_sync.node_id())),
                at_ms: 1_700_000_003_000,
            },
        );
        let mut destination_authority = destination_backend.clone();
        let receipt = apply_transfer(
            &mut destination,
            &destination_blobs,
            &destination_blobs,
            &mut destination_authority,
            &fetched,
            &ApplyTransferContext {
                authorization: TransferAuthorization {
                    grant_id: offered.pairing_id.clone(),
                    revoked: false,
                },
                application: "graphshell".to_string(),
                handler: "graphshell.transfer/v1".to_string(),
                completed_at_ms: 1_700_000_003_500,
                access_privacy: PrivacyClass::LocalOnly,
            },
        )
        .await
        .unwrap();

        assert_eq!(receipt.nodes, 2);
        assert_eq!(receipt.relations, 1);
        assert_eq!(receipt.nodes, offered.nodes, "the offer's summary held");
        assert_eq!(receipt.relations, offered.relations);
        assert!(
            receipt
                .id_map
                .iter()
                .all(|ids| ids.source == ids.destination),
            "replicate preserves ids"
        );
        assert!(destination.graph().get_node_by_id(url).is_some());
        assert!(destination.graph().get_node_by_id(file).is_some());
        assert!(
            destination
                .graph()
                .get_node_by_id(file)
                .unwrap()
                .1
                .tags
                .contains("file")
        );

        // The bytes a browser would pull are the bytes that were applied.
        // Derived from the local store, so this also proves the release list
        // survives the source being gone.
        let released = released_blobs_for(&destination_sync, &fetched)
            .await
            .unwrap();
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].1, FILE_BYTES);
        assert_eq!(
            released[0].0,
            chirograph::ContentHash::of(FILE_BYTES),
            "the browser addresses this blob by its own hash of the same bytes"
        );

        destination_sync.close().await.unwrap();
    }

    /// A missing lane must cost the offer, not leave bytes half-staged. The
    /// refusal has to land before the first `stage_blob`, because that call
    /// writes and advertises in one step.
    #[tokio::test]
    async fn offering_without_the_blob_lane_is_refused_before_any_byte_is_staged() {
        let directory = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0xa6; 32]);
        let sync = PersonalSyncHost::open(
            &identity,
            PersonalSyncHostConfig {
                graph: [0xa7; 32],
                store_path: directory.path().join("no-blob-lane.redb"),
                roster: SyncRoster::new([identity.master_public_key().to_bytes()]),
                // Offers on, blob availability off: the device can announce a
                // transfer it has no way to let anyone collect.
                selection: SyncSelection::default().with_facets([TRANSFER_OFFER_FACET]),
                peer_tickets: Vec::new(),
                peer_hints: Vec::new(),
                paired_nodes: Vec::new(),
                relay_urls: Vec::new(),
            },
        )
        .await
        .unwrap();

        let (source, backend, url, file) = source_host(sync.node_id());
        let blobs = BlobStore::new(backend.clone());
        let mut authority = backend.clone();
        let manifest = prepare_transfer(
            &source,
            &mut authority,
            &blobs,
            TransferRequest {
                transfer_id: Uuid::new_v4(),
                operation: TransferOperation::Replicate,
                source: endpoint(sync.node_id()),
                destination: endpoint([0xa8; 32]),
                route: TransferRouteV1 {
                    carrier: "personal-sync".to_string(),
                    peer: hex(&[0xa8; 32]),
                },
                selection: ExportRequest {
                    focused: file,
                    selected: vec![file, url],
                    scope: TransferScope::SelectedSubgraph,
                    exported_at_ms: 1_700_000_000_500,
                    include_local_file_locations: true,
                    scene: None,
                },
                access_policy: AccessTransferPolicy::ExcludeSourceHistory,
                privacy: PrivacyClass::TrustedPeersOnly,
            },
            vec![crate::transfer::TransferBlobInput {
                node_id: file,
                role: "primary".to_string(),
                media_type: "text/plain".to_string(),
                bytes: FILE_BYTES.to_vec(),
            }],
        )
        .await
        .unwrap();

        let refused = offer_transfer(&sync, &blobs, &manifest, "pairing-s3", 1_700_000_002_000)
            .await
            .unwrap_err();
        assert!(matches!(refused, StagingError::BlobLaneDisabled));
        for descriptor in &manifest.blobs {
            assert!(
                !sync
                    .blobs()
                    .has(BlobHash::from_bytes(*descriptor.content_hash.as_bytes()))
                    .await
                    .unwrap(),
                "a refused offer must not leave its bytes in the serving store"
            );
        }
        assert!(
            sync.offers().await.unwrap().is_empty(),
            "and it must not announce a transfer nobody can collect"
        );

        sync.close().await.unwrap();
    }
}
