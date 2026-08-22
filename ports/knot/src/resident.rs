//! Always-on Knot sync.
//!
//! [`KnotSyncStore::join`](crate::KnotSyncStore::join) has been production code
//! for a while, but nothing shipped ever called it: `transport` was a
//! dev-dependency, so Knot's p2panda convergence was real and exercised only by
//! tests. The shipped endpoint bound no transport at all and therefore never
//! synchronised anything. This is the missing half.
//!
//! Identity here is deliberately split, because carrying a persona's private
//! epoch between devices makes the two halves pull in opposite directions:
//!
//! - the **vault key** and **space id** must be identical across a persona's
//!   devices, or they cannot decrypt or address the same space;
//! - the **writer key** must not be, because its public half is also the
//!   transport node id. Two devices deriving one writer would be a single node
//!   on the network and a single author in a per-author log.
//!
//! [`StartupUnlockedPersonalVault`](crate::StartupUnlockedPersonalVault)
//! resolves that by mixing the device's own Personae root into the writer
//! derivation only.

use std::collections::BTreeSet;
use std::sync::Arc;

use stickleback::{JoinError, JoinedSpace, SyncStatus};
use transport::p2panda_transport::{KnownPeer, RelayUrl};
use transport::{
    BlobPeerAuthorizer, BlobReadAuthorizer, BlobScope, BlobStore, P2pandaHostPolicy,
    P2pandaOverlayHost, P2pandaTransport, PeerID, sync_overlay_topic,
};

use crate::VaultDocument;
use crate::authority::{KnotAuthoritySource, KnotSpaceAuthoritySnapshot};
use crate::clip_evidence::{KnotClipEvidenceRef, clip_evidence_references};
use crate::sync::{KnotEncryptionProfile, KnotSyncExt, KnotSyncFileStore};

/// How this device reaches the persona's other devices.
#[derive(Clone, Debug, Default)]
pub struct KnotSyncHostConfig {
    /// One materialization consumed by operation, evidence, and route policy.
    pub authority: KnotSpaceAuthoritySnapshot,
    /// iroh relays. Empty leaves this device LAN-only: p2panda registers no
    /// relay by default.
    pub relay_urls: Vec<RelayUrl>,
}

#[derive(Debug, thiserror::Error)]
pub enum KnotSyncHostError {
    #[error("Knot sync transport failed: {0}")]
    Transport(String),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("Knot evidence reference failed: {0}")]
    EvidenceReference(String),
    #[error("Knot evidence blob failed: {0}")]
    EvidenceBlob(String),
    #[error("peer is not authorized for this Knot evidence store")]
    EvidenceUnauthorized,
    #[error("Knot evidence is {actual} bytes; configured limit is {limit}")]
    EvidenceTooLarge { actual: u64, limit: u64 },
    #[error("Knot resident authority failed: {0}")]
    Authority(String),
}

/// Result of resolving one portable evidence reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotEvidenceFetchStatus {
    /// Verified bytes were already present locally.
    AlreadyPresent,
    /// Verified bytes were fetched from the named peer.
    Fetched,
}

/// Receipt for one verified evidence reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotEvidenceFetchReceipt {
    pub reference: KnotClipEvidenceRef,
    pub status: KnotEvidenceFetchStatus,
}

struct KnotEvidenceHost {
    blobs: Arc<BlobStore>,
    readers: BlobReadAuthorizer,
    scope: BlobScope,
    sources: BlobPeerAuthorizer,
    max_artifact_bytes: u64,
}

/// Which peers' addresses are worth writing back to settings.
///
/// Only the ones this host currently holds a live path to. The distinction
/// between `reachable` and `connected` is the whole of this function, and it
/// is not pedantry: an address the endpoint holds for a peer it is *not*
/// talking to may be exactly the stale route a working hint would replace, so
/// writing it back would overwrite good information with bad. A firewall can
/// drop every packet to a device while its address stays in the book, which
/// makes `reachable` look healthy while nothing replicates at all.
fn writers_to_refresh(peers: &[KnownPeer]) -> Vec<[u8; 32]> {
    peers
        .iter()
        .filter(|peer| peer.connected)
        .map(|peer| peer.peer.to_bytes())
        .collect()
}

/// A bound transport and live LogSync session over one Knot space.
pub struct KnotSyncHost {
    joined: JoinedSpace<KnotSyncExt>,
    network: P2pandaOverlayHost,
    store: KnotSyncFileStore,
    space_id: [u8; 32],
    evidence: Option<KnotEvidenceHost>,
    authority: KnotSpaceAuthoritySnapshot,
}

impl KnotSyncHost {
    /// Bind a transport for `signing_seed` and join `store`'s space.
    ///
    /// The transport key is the writer seed, so a device's node id and its
    /// author identity are the same value. That is what lets a paired writer
    /// serve as both the thing admitted and the thing dialled.
    pub async fn open(
        store: &KnotSyncFileStore,
        signing_seed: [u8; 32],
        config: KnotSyncHostConfig,
    ) -> Result<Self, KnotSyncHostError> {
        Self::open_inner(store, signing_seed, config, None).await
    }

    /// Bind personal-device sync and serve clip evidence only to paired
    /// Personae-derived writer identities.
    pub async fn open_with_evidence(
        store: &KnotSyncFileStore,
        signing_seed: [u8; 32],
        config: KnotSyncHostConfig,
        blobs: Arc<BlobStore>,
        max_artifact_bytes: u64,
    ) -> Result<Self, KnotSyncHostError> {
        if store.encryption_profile() != KnotEncryptionProfile::PersonalVaultV1 {
            return Err(KnotSyncHostError::Authority(
                "personal pairing cannot authorize a communal Knot space".into(),
            ));
        }
        let readers = BlobReadAuthorizer::new();
        Self::open_with_scoped_evidence(
            store,
            signing_seed,
            config,
            blobs,
            readers,
            max_artifact_bytes,
        )
        .await
    }

    /// Bind personal-device sync against a caller-shared serving authorizer.
    ///
    /// The content-retention actor receives the same handle and binds each
    /// retained hash to this space. This keeps one custody truth across local
    /// authoring and remote serving.
    pub async fn open_with_scoped_evidence(
        store: &KnotSyncFileStore,
        signing_seed: [u8; 32],
        config: KnotSyncHostConfig,
        blobs: Arc<BlobStore>,
        readers: BlobReadAuthorizer,
        max_artifact_bytes: u64,
    ) -> Result<Self, KnotSyncHostError> {
        if store.encryption_profile() != KnotEncryptionProfile::PersonalVaultV1 {
            return Err(KnotSyncHostError::Authority(
                "personal pairing cannot authorize a communal Knot space".into(),
            ));
        }
        let scope = BlobScope::new(store.space_id());
        readers.replace_readers(scope, config.authority.evidence_readers());
        let sources = BlobPeerAuthorizer::from_peers(config.authority.evidence_sources());
        Self::open_inner(
            store,
            signing_seed,
            config,
            Some(KnotEvidenceHost {
                blobs,
                readers,
                scope,
                sources,
                max_artifact_bytes,
            }),
        )
        .await
    }

    /// Bind a communal space with an authorizer materialized from Gemot facts.
    pub async fn open_with_communal_evidence(
        store: &KnotSyncFileStore,
        signing_seed: [u8; 32],
        config: KnotSyncHostConfig,
        blobs: Arc<BlobStore>,
        max_artifact_bytes: u64,
    ) -> Result<Self, KnotSyncHostError> {
        if store.encryption_profile() != KnotEncryptionProfile::CommonsDataV1 {
            return Err(KnotSyncHostError::Authority(
                "Gemot authority requires a communal Knot space".into(),
            ));
        }
        if config.authority.source() != KnotAuthoritySource::GemotCapabilities {
            return Err(KnotSyncHostError::Authority(
                "communal Knot host requires Gemot capability authority".into(),
            ));
        }
        if store.admitted_writers() != config.authority.writers().collect::<Vec<_>>() {
            return Err(KnotSyncHostError::Authority(
                "communal store writers do not match materialized Gemot authority".into(),
            ));
        }
        let sources = BlobPeerAuthorizer::from_peers(config.authority.evidence_sources());
        let scope = BlobScope::new(store.space_id());
        let readers = BlobReadAuthorizer::new();
        readers.replace_readers(scope, config.authority.evidence_readers());
        Self::open_inner(
            store,
            signing_seed,
            config,
            Some(KnotEvidenceHost {
                blobs,
                readers,
                scope,
                sources,
                max_artifact_bytes,
            }),
        )
        .await
    }

    async fn open_inner(
        store: &KnotSyncFileStore,
        signing_seed: [u8; 32],
        config: KnotSyncHostConfig,
        evidence: Option<KnotEvidenceHost>,
    ) -> Result<Self, KnotSyncHostError> {
        validate_authority_source(store.encryption_profile(), config.authority.source())?;
        let mut builder = P2pandaTransport::builder_from_seed(signing_seed).gossip();
        if let Some(evidence) = &evidence {
            builder =
                builder.scoped_blobs(&evidence.blobs, evidence.scope, evidence.readers.clone());
        }
        let network = P2pandaOverlayHost::bind(
            builder,
            sync_overlay_topic(store.space_id()),
            &P2pandaHostPolicy {
                relay_urls: config.relay_urls,
                ..P2pandaHostPolicy::default()
            },
        )
        .await
        .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;
        network
            .seed_peers(config.authority.writers())
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;

        // The cached-address rung, as Graphshell has it: a device that has
        // connected once can redial after both ends restart with no discovery
        // working at all.
        for (expected, hint) in config.authority.route_hints() {
            match network.add_peer_ticket(hint).await {
                Ok(peer) if peer.to_bytes() == *expected => {}
                Ok(peer) => tracing::warn!(
                    expected = %crate::hex32(expected),
                    actual = %crate::hex32(&peer.to_bytes()),
                    "a stored dial hint named another peer; skipping it"
                ),
                Err(error) => tracing::warn!(
                    %error,
                    "a stored dial hint was unusable; skipping it"
                ),
            }
        }

        let (endpoint, gossip) = network
            .transport()
            .sync_parts()
            .ok_or_else(|| KnotSyncHostError::Transport("gossip is unavailable".into()))?;
        let joined = store.join(endpoint, gossip).await?;
        Ok(Self {
            joined,
            network,
            store: store.clone(),
            space_id: store.space_id(),
            evidence,
            authority: config.authority,
        })
    }

    /// This device's node id, which is also its writer key: what the other
    /// devices must admit.
    pub fn node_id(&self) -> [u8; 32] {
        self.network.local_peer_id().to_bytes()
    }

    /// Stable Knot space carried by this host.
    pub fn space_id(&self) -> [u8; 32] {
        self.space_id
    }

    pub fn sync_status(&self) -> SyncStatus {
        self.joined.sync_status()
    }

    /// Leave LogSync and close the transport before a persistent blob store is
    /// reopened by another resident.
    pub async fn close(self) -> Result<(), KnotSyncHostError> {
        let Self {
            joined, network, ..
        } = self;
        joined.leave_and_wait().await?;
        network
            .close()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Current evidence-fetch admission handle, when this host serves blobs.
    pub fn evidence_authorizer(&self) -> Option<BlobReadAuthorizer> {
        self.evidence
            .as_ref()
            .map(|evidence| evidence.readers.clone())
    }

    /// Bind an already-retained reference to this host's serving scope.
    ///
    /// Startup uses this while replaying resident documents whose evidence was
    /// retained on an earlier run. It changes authority only; bytes are not
    /// copied or opened a second time.
    pub fn retain_evidence_custody(
        &self,
        reference: &KnotClipEvidenceRef,
    ) -> Result<bool, KnotSyncHostError> {
        let evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| KnotSyncHostError::EvidenceBlob("blob serving is disabled".into()))?;
        let hash = reference
            .blob_hash()
            .map_err(KnotSyncHostError::EvidenceReference)?;
        Ok(evidence.readers.retain(evidence.scope, hash))
    }

    /// Revision currently applied by operation, evidence, and route policy.
    pub fn authority_revision(&self) -> [u8; 32] {
        self.authority.revision()
    }

    /// Apply one new authority materialization to every live consumer.
    pub async fn apply_authority(
        &mut self,
        next: KnotSpaceAuthoritySnapshot,
    ) -> Result<bool, KnotSyncHostError> {
        validate_authority_source(self.store.encryption_profile(), next.source())?;
        if self.authority.revision() == next.revision() {
            return Ok(false);
        }

        let previous_writers = self.authority.writers().collect::<BTreeSet<_>>();
        let next_writers = next.writers().collect::<BTreeSet<_>>();
        match next.source() {
            KnotAuthoritySource::PersonalPairing => {
                for writer in previous_writers.difference(&next_writers) {
                    self.store.deny_writer(writer);
                }
                for writer in next_writers.difference(&previous_writers).copied() {
                    self.store.admit_writer(writer);
                }
            }
            KnotAuthoritySource::GemotCapabilities => {
                self.store
                    .replace_admitted_writers(next_writers.iter().copied());
            }
        }
        if let Some(evidence) = &self.evidence {
            evidence
                .readers
                .replace_readers(evidence.scope, next.evidence_readers());
            evidence.sources.replace(next.evidence_sources());
        }

        for writer in previous_writers.difference(&next_writers).copied() {
            if let Err(error) = self.network.remove_peer(writer).await {
                tracing::warn!(
                    %error,
                    writer = %crate::hex32(&writer),
                    "authority was revoked but its stale route could not be detached"
                );
            }
        }
        for writer in next_writers.difference(&previous_writers).copied() {
            if let Err(error) = self.network.add_peer(writer).await {
                tracing::warn!(
                    %error,
                    writer = %crate::hex32(&writer),
                    "authority was granted but its route is not yet available"
                );
            }
        }
        for (expected, hint) in next.route_hints() {
            match self.network.add_peer_ticket(hint).await {
                Ok(peer) if peer.to_bytes() == *expected => {}
                Ok(peer) => tracing::warn!(
                    expected = %crate::hex32(expected),
                    actual = %crate::hex32(&peer.to_bytes()),
                    "an authority route hint named another peer; ignoring the route"
                ),
                Err(error) => tracing::warn!(
                    %error,
                    "an authority route hint was unavailable; authority still applied"
                ),
            }
        }
        self.authority = next;
        Ok(true)
    }

    /// Read and verify local evidence bytes before exposing them to a caller.
    pub async fn read_evidence(
        &self,
        reference: &KnotClipEvidenceRef,
    ) -> Result<Vec<u8>, KnotSyncHostError> {
        let evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| KnotSyncHostError::EvidenceBlob("blob serving is disabled".into()))?;
        if reference.byte_size > evidence.max_artifact_bytes {
            return Err(KnotSyncHostError::EvidenceTooLarge {
                actual: reference.byte_size,
                limit: evidence.max_artifact_bytes,
            });
        }
        let hash = reference
            .blob_hash()
            .map_err(KnotSyncHostError::EvidenceReference)?;
        let bytes = evidence
            .blobs
            .get_bytes(hash)
            .await
            .map_err(|error| KnotSyncHostError::EvidenceBlob(error.to_string()))?;
        reference
            .verify_bytes(&bytes)
            .map_err(KnotSyncHostError::EvidenceReference)?;
        Ok(bytes.to_vec())
    }

    /// Fetch one reference from an authorized peer and verify it before it can
    /// be read through [`Self::read_evidence`].
    pub async fn fetch_evidence(
        &self,
        reference: &KnotClipEvidenceRef,
        writer: [u8; 32],
    ) -> Result<KnotEvidenceFetchReceipt, KnotSyncHostError> {
        let evidence = self
            .evidence
            .as_ref()
            .ok_or_else(|| KnotSyncHostError::EvidenceBlob("blob serving is disabled".into()))?;
        if !evidence.sources.allows(&writer) {
            return Err(KnotSyncHostError::EvidenceUnauthorized);
        }
        if reference.byte_size > evidence.max_artifact_bytes {
            return Err(KnotSyncHostError::EvidenceTooLarge {
                actual: reference.byte_size,
                limit: evidence.max_artifact_bytes,
            });
        }
        let hash = reference
            .blob_hash()
            .map_err(KnotSyncHostError::EvidenceReference)?;
        if evidence
            .blobs
            .has(hash)
            .await
            .map_err(|error| KnotSyncHostError::EvidenceBlob(error.to_string()))?
        {
            self.read_evidence(reference).await?;
            evidence.readers.retain(evidence.scope, hash);
            return Ok(KnotEvidenceFetchReceipt {
                reference: reference.clone(),
                status: KnotEvidenceFetchStatus::AlreadyPresent,
            });
        }
        let peer = PeerID::from_bytes(&writer)
            .map_err(|error| KnotSyncHostError::EvidenceBlob(error.to_string()))?;
        let tag = evidence_tag(self.space_id, reference);
        evidence
            .blobs
            .fetch_from_named(self.network.transport(), peer, hash, tag.as_bytes())
            .await
            .map_err(|error| KnotSyncHostError::EvidenceBlob(error.to_string()))?;
        if let Err(error) = self.read_evidence(reference).await {
            let _ = evidence.blobs.release(tag.as_bytes()).await;
            return Err(error);
        }
        evidence.readers.retain(evidence.scope, hash);
        Ok(KnotEvidenceFetchReceipt {
            reference: reference.clone(),
            status: KnotEvidenceFetchStatus::Fetched,
        })
    }

    /// Resolve every clip evidence reference carried by a replicated Djot
    /// document from one authorized peer.
    pub async fn fetch_document_evidence(
        &self,
        document: &VaultDocument,
        writer: [u8; 32],
    ) -> Result<Vec<KnotEvidenceFetchReceipt>, KnotSyncHostError> {
        let references = clip_evidence_references(&document.body)
            .map_err(KnotSyncHostError::EvidenceReference)?;
        let mut receipts = Vec::with_capacity(references.len());
        for reference in references {
            receipts.push(self.fetch_evidence(&reference, writer).await?);
        }
        Ok(receipts)
    }

    /// A ticket for the across-network case a relay cannot serve. Rebuilt on
    /// every bind, so it is a bootstrap value and never a stored one.
    pub async fn ticket(&self) -> Result<String, KnotSyncHostError> {
        self.network
            .ticket()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Which paired devices the transport currently associates with this
    /// space, and whether each is merely known or actually talking.
    ///
    /// Pairing records identity; this reports reachability, which is the fact
    /// a writer key cannot carry on its own.
    pub async fn known_peers(&self) -> Result<Vec<KnownPeer>, KnotSyncHostError> {
        self.network
            .known_peers()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Where the endpoint currently believes `writer` lives, as a ticket, if
    /// it holds any addresses for it. The value the cached-address rung
    /// persists back into settings.
    pub async fn peer_ticket(&self, writer: [u8; 32]) -> Result<Option<String>, KnotSyncHostError> {
        self.network
            .peer_ticket(writer)
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Write back the addresses of devices this host is actually talking to.
    ///
    /// The other half of the cached-address rung: [`open`](Self::open) seeds
    /// stored hints, and this is what puts them there in the first place.
    /// Without it a hint only ever arrives if something outside Knot records
    /// one.
    ///
    /// Three disciplines, each of which the Graphshell lane learned the hard
    /// way:
    ///
    /// - **Connected peers only**, per [`writers_to_refresh`].
    /// - **Only on change.** [`KnotSyncHost::peer_ticket`] sorts addresses
    ///   before serialising, so an unchanged address set yields an identical
    ///   string and costs no settings write.
    /// - **Reload before saving.** The settings file has a second writer: a
    ///   `--pair-writer` invocation can land between the caller's read and
    ///   this write, so the refresh loads the latest, modifies, and saves
    ///   rather than persisting a snapshot taken seconds ago.
    pub async fn refresh_dial_hints(&self, settings_file: &std::path::Path) {
        let peers = match self.known_peers().await {
            Ok(peers) => peers,
            Err(error) => {
                tracing::warn!(%error, "could not read the peer directory");
                return;
            }
        };

        for writer in writers_to_refresh(&peers) {
            let ticket = match self.peer_ticket(writer).await {
                Ok(Some(ticket)) => ticket,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(%error, "could not read a peer's current address");
                    continue;
                }
            };
            if self.authority.route_hint(&writer) == Some(ticket.as_str()) {
                continue;
            }
            let mut latest = match crate::KnotSettings::load(settings_file) {
                Ok(latest) => latest,
                Err(error) => {
                    tracing::warn!(%error, "could not reload settings to refresh a hint");
                    continue;
                }
            };
            let Some(live) = latest.sync.as_mut() else {
                continue;
            };
            // `remember_endpoint` ignores a writer that is no longer paired,
            // so an unpair landing in this window cannot be undone by a route.
            if !live.remember_endpoint(writer, &ticket) {
                continue;
            }
            match latest.save(settings_file) {
                Ok(()) => tracing::info!(
                    writer = %crate::hex32(&writer),
                    "recorded a fresh dial hint for a connected device"
                ),
                Err(error) => tracing::warn!(%error, "could not persist a refreshed dial hint"),
            }
        }
    }
}

fn validate_authority_source(
    profile: KnotEncryptionProfile,
    source: KnotAuthoritySource,
) -> Result<(), KnotSyncHostError> {
    match (profile, source) {
        (KnotEncryptionProfile::PersonalVaultV1, KnotAuthoritySource::PersonalPairing)
        | (KnotEncryptionProfile::CommonsDataV1, KnotAuthoritySource::GemotCapabilities) => Ok(()),
        (KnotEncryptionProfile::PersonalVaultV1, KnotAuthoritySource::GemotCapabilities) => {
            Err(KnotSyncHostError::Authority(
                "Gemot authority cannot alter a personal Knot space".into(),
            ))
        }
        (KnotEncryptionProfile::CommonsDataV1, KnotAuthoritySource::PersonalPairing) => {
            Err(KnotSyncHostError::Authority(
                "Personae pairing cannot alter a communal Knot space".into(),
            ))
        }
    }
}

fn evidence_tag(space_id: [u8; 32], reference: &KnotClipEvidenceRef) -> String {
    format!(
        "knot/evidence/{}/{}",
        crate::hex32(&space_id),
        reference.digest
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use chirograph::{KnotClipArtifactRoleV1, KnotClipArtifactV1, PortableContentRefV1};
    use gemot::moot::constitution::{CapabilityGrant, ConstitutionRules};
    use gemot::moot::{MOOT_ACT_ACTION, MOOT_DELEGATION_DOMAIN, MootAuthority, MootDelegations};
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
        SignedDelegationCertificate, SignedDelegationRevocation,
    };
    use personae::{IdentityProvider, InMemoryProvider};
    use servitor::cap::Cap;
    use servitor::cap_path;
    use tempfile::tempdir;

    use crate::{BlobClipEvidenceStore, KnotSyncEvent, KnotVault};

    const SPACE: [u8; 32] = [0x51; 32];
    const VAULT_KEY: [u8; 32] = [0x52; 32];
    const MOOT: [u8; 32] = [0x53; 32];
    const ROOT_GRANT: [u8; 32] = [0x54; 32];

    /// A real Ed25519 public key: a peer id is a curve point, so an array of
    /// repeated bytes will not parse as one.
    fn writer(seed: u8) -> [u8; 32] {
        personae::Ed25519Keypair::from_seed([seed; 32])
            .public_key()
            .to_bytes()
    }

    fn peer(seed: u8, reachable: bool, connected: bool) -> KnownPeer {
        KnownPeer {
            peer: PeerID::from_bytes(&writer(seed)).expect("a valid peer key"),
            reachable,
            bootstrap: false,
            connected,
        }
    }

    #[test]
    fn only_connected_peers_have_their_addresses_written_back() {
        let peers = [
            // Known and addressed, but nothing is flowing: its address may be
            // the stale one a good hint would replace.
            peer(1, true, false),
            // Actually talking: this address is true right now.
            peer(2, true, true),
            // Named by discovery, no address at all.
            peer(3, false, false),
        ];

        assert_eq!(
            writers_to_refresh(&peers),
            vec![writer(2)],
            "a reachable-but-silent peer must not overwrite a working hint"
        );
    }

    #[test]
    fn nothing_is_written_back_when_no_device_is_talking() {
        let peers = [peer(1, true, false), peer(2, true, false)];
        assert!(
            writers_to_refresh(&peers).is_empty(),
            "an address book full of unreachable devices records no routes"
        );
    }

    #[test]
    fn communal_blob_and_document_permissions_come_from_gemot_paths() {
        let founder = InMemoryProvider::from_seed([61; 32]);
        let document_writer = InMemoryProvider::from_seed([62; 32]);
        let evidence_reader = InMemoryProvider::from_seed([63; 32]);
        let outsider = InMemoryProvider::from_seed([64; 32]);
        let evidence_source = InMemoryProvider::from_seed([65; 32]);
        let space_scope = Cap::scope(&format!("knot/{}", crate::hex32(&SPACE))).unwrap();
        let document_scope =
            Cap::scope(&format!("knot/{}/document", crate::hex32(&SPACE))).unwrap();
        let evidence_read_scope =
            Cap::scope(&format!("knot/{}/evidence/read", crate::hex32(&SPACE))).unwrap();
        let evidence_source_scope =
            Cap::scope(&format!("knot/{}/evidence/source", crate::hex32(&SPACE))).unwrap();
        let mut rules = ConstitutionRules::founder_only(founder.master_public_key().to_bytes());
        rules.grant(CapabilityGrant {
            id: ROOT_GRANT,
            subject: founder.master_public_key().to_bytes(),
            path_prefix: cap_path(&space_scope),
            not_before_ms: 10,
            expires_at_ms: Some(1_000),
            delegation_depth: 2,
        });
        let issue = |subject: &InMemoryProvider, capability: &Cap, nonce: u8| {
            SignedDelegationCertificate::issue(
                &founder,
                DelegationCertificate::new(
                    DelegationParent::Root(ROOT_GRANT),
                    founder.master_public_key().to_bytes(),
                    subject.master_public_key().to_bytes(),
                    CapabilityScope {
                        domain: MOOT_DELEGATION_DOMAIN.into(),
                        resource: MOOT.to_vec(),
                        path_prefix: cap_path(capability),
                        actions: [MOOT_ACT_ACTION.to_string()].into_iter().collect(),
                    },
                    15,
                    20,
                    Some(900),
                    0,
                    [nonce; 32],
                ),
            )
            .unwrap()
        };
        let mut delegations = MootDelegations::new();
        delegations
            .accept_certificate(MOOT, &rules, issue(&document_writer, &document_scope, 1))
            .unwrap();
        delegations
            .accept_certificate(
                MOOT,
                &rules,
                issue(&evidence_reader, &evidence_read_scope, 2),
            )
            .unwrap();
        delegations
            .accept_certificate(
                MOOT,
                &rules,
                issue(&evidence_source, &evidence_source_scope, 3),
            )
            .unwrap();
        let authority = MootAuthority {
            delegations: &delegations,
            rules: &rules,
            moot_id: MOOT,
            now_ms: 50,
        };
        let materialized = KnotSpaceAuthoritySnapshot::from_gemot_authority(
            &authority,
            SPACE,
            [
                outsider.master_public_key().to_bytes(),
                evidence_reader.master_public_key().to_bytes(),
                document_writer.master_public_key().to_bytes(),
                evidence_source.master_public_key().to_bytes(),
            ],
            [],
        )
        .unwrap();

        assert_eq!(
            materialized.writers().collect::<Vec<_>>(),
            vec![document_writer.master_public_key().to_bytes()]
        );
        assert!(
            materialized
                .evidence_readers()
                .any(|peer| peer == evidence_reader.master_public_key().to_bytes())
        );
        assert!(
            !materialized
                .evidence_readers()
                .any(|peer| peer == document_writer.master_public_key().to_bytes())
        );
        assert!(
            !materialized
                .writers()
                .any(|peer| peer == evidence_reader.master_public_key().to_bytes())
        );
        assert!(
            materialized
                .evidence_sources()
                .any(|peer| peer == evidence_source.master_public_key().to_bytes())
        );
        assert!(
            !materialized
                .evidence_sources()
                .any(|peer| peer == document_writer.master_public_key().to_bytes())
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn signed_gemot_grant_and_revocation_update_every_live_consumer() {
        let roots = tempdir().unwrap();
        let founder = InMemoryProvider::from_seed([66; 32]);
        let peer = InMemoryProvider::from_seed([67; 32]);
        let resident = InMemoryProvider::from_seed([68; 32]);
        let peer_id = peer.master_public_key().to_bytes();
        let space_scope = Cap::scope(&format!("knot/{}", crate::hex32(&SPACE))).unwrap();
        let document_scope =
            Cap::scope(&format!("knot/{}/document", crate::hex32(&SPACE))).unwrap();
        let evidence_read_scope =
            Cap::scope(&format!("knot/{}/evidence/read", crate::hex32(&SPACE))).unwrap();
        let evidence_source_scope =
            Cap::scope(&format!("knot/{}/evidence/source", crate::hex32(&SPACE))).unwrap();
        let mut rules = ConstitutionRules::founder_only(founder.master_public_key().to_bytes());
        rules.grant(CapabilityGrant {
            id: ROOT_GRANT,
            subject: founder.master_public_key().to_bytes(),
            path_prefix: cap_path(&space_scope),
            not_before_ms: 10,
            expires_at_ms: Some(1_000),
            delegation_depth: 2,
        });
        let issue = |capability: &Cap, nonce: u8| {
            SignedDelegationCertificate::issue(
                &founder,
                DelegationCertificate::new(
                    DelegationParent::Root(ROOT_GRANT),
                    founder.master_public_key().to_bytes(),
                    peer_id,
                    CapabilityScope {
                        domain: MOOT_DELEGATION_DOMAIN.into(),
                        resource: MOOT.to_vec(),
                        path_prefix: cap_path(capability),
                        actions: [MOOT_ACT_ACTION.to_string()].into_iter().collect(),
                    },
                    15,
                    20,
                    Some(900),
                    0,
                    [nonce; 32],
                ),
            )
            .unwrap()
        };
        let certificates = [
            issue(&document_scope, 4),
            issue(&evidence_read_scope, 5),
            issue(&evidence_source_scope, 6),
        ];
        let mut delegations = MootDelegations::new();
        let empty = KnotSpaceAuthoritySnapshot::from_gemot_authority(
            &MootAuthority {
                delegations: &delegations,
                rules: &rules,
                moot_id: MOOT,
                now_ms: 50,
            },
            SPACE,
            [peer_id],
            [],
        )
        .unwrap();
        let store = KnotSyncFileStore::open_commons(
            roots.path().join("communal-authority.redb"),
            SPACE,
            [],
        )
        .unwrap();
        let blobs = Arc::new(BlobStore::new());
        let bytes = b"communal evidence remains under owner custody";
        let portable = PortableContentRefV1::of(bytes);
        let reference: KnotClipEvidenceRef = serde_json::from_value(serde_json::json!({
            "content": portable,
            "media_type": "text/plain",
            "canonical_uri": "https://example.test/communal-evidence",
            "role": KnotClipArtifactRoleV1::SourceResponse,
        }))
        .unwrap();
        assert_eq!(
            blobs.put_bytes(bytes.to_vec()).await.unwrap(),
            reference.blob_hash().unwrap()
        );
        let mut host = KnotSyncHost::open_with_communal_evidence(
            &store,
            resident.master_keypair().to_seed(),
            KnotSyncHostConfig {
                authority: empty,
                relay_urls: vec![],
            },
            Arc::clone(&blobs),
            4096,
        )
        .await
        .unwrap();
        assert!(host.retain_evidence_custody(&reference).unwrap());
        let scope = BlobScope::new(SPACE);
        let readers = host.evidence_authorizer().unwrap();
        let empty_revision = host.authority_revision();
        assert!(store.admitted_writers().is_empty());
        assert!(!readers.allows(scope, &peer_id, reference.blob_hash().unwrap()));
        assert!(matches!(
            host.fetch_evidence(&reference, peer_id).await,
            Err(KnotSyncHostError::EvidenceUnauthorized)
        ));

        for certificate in certificates.iter().cloned() {
            delegations
                .accept_certificate(MOOT, &rules, certificate)
                .unwrap();
        }
        let granted = KnotSpaceAuthoritySnapshot::from_gemot_authority(
            &MootAuthority {
                delegations: &delegations,
                rules: &rules,
                moot_id: MOOT,
                now_ms: 50,
            },
            SPACE,
            [peer_id],
            [],
        )
        .unwrap();
        assert!(host.apply_authority(granted).await.unwrap());
        let granted_revision = host.authority_revision();
        assert_ne!(granted_revision, empty_revision);
        assert_eq!(store.admitted_writers(), vec![peer_id]);
        assert!(readers.allows(scope, &peer_id, reference.blob_hash().unwrap()));
        assert_eq!(
            host.fetch_evidence(&reference, peer_id)
                .await
                .unwrap()
                .status,
            KnotEvidenceFetchStatus::AlreadyPresent
        );

        for (certificate, nonce) in certificates.iter().zip(7_u8..) {
            let revocation = DelegationRevocation::new(
                certificate.certificate.id(),
                founder.master_public_key().to_bytes(),
                certificate.certificate.scope.clone(),
                60,
                [nonce; 32],
            );
            delegations
                .accept_revocation(SignedDelegationRevocation::issue(&founder, revocation).unwrap())
                .unwrap();
        }
        let revoked = KnotSpaceAuthoritySnapshot::from_gemot_authority(
            &MootAuthority {
                delegations: &delegations,
                rules: &rules,
                moot_id: MOOT,
                now_ms: 70,
            },
            SPACE,
            [peer_id],
            [],
        )
        .unwrap();
        assert!(host.apply_authority(revoked).await.unwrap());
        assert_ne!(host.authority_revision(), granted_revision);
        assert_eq!(host.authority_revision(), empty_revision);
        assert!(store.admitted_writers().is_empty());
        assert!(!readers.allows(scope, &peer_id, reference.blob_hash().unwrap()));
        assert!(matches!(
            host.fetch_evidence(&reference, peer_id).await,
            Err(KnotSyncHostError::EvidenceUnauthorized)
        ));
        assert_eq!(host.read_evidence(&reference).await.unwrap(), bytes);

        host.close().await.unwrap();
        blobs.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn paired_peers_replicate_djot_then_fetch_and_reopen_verified_evidence() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([71; 32]);
        let bob = InMemoryProvider::from_seed([72; 32]);
        let alice_writer = alice.master_public_key().to_bytes();
        let bob_writer = bob.master_public_key().to_bytes();
        let writers = [alice_writer, bob_writer];
        let alice_store =
            KnotSyncFileStore::open(roots.path().join("alice.redb"), SPACE, writers).unwrap();
        let bob_store =
            KnotSyncFileStore::open(roots.path().join("bob.redb"), SPACE, writers).unwrap();
        let alice_vault = KnotVault::open(roots.path().join("alice-vault"), VAULT_KEY).unwrap();
        let bob_vault = KnotVault::open(roots.path().join("bob-vault"), VAULT_KEY).unwrap();

        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: "https://example.test/source".into(),
            bytes: b"<main>source bytes travel separately</main>".to_vec(),
        };
        let alice_evidence_root = roots.path().join("alice-evidence");
        let bob_evidence_root = roots.path().join("bob-evidence");
        let alice_evidence = BlobClipEvidenceStore::open_async(&alice_evidence_root, 4096)
            .await
            .unwrap();
        let bob_evidence = BlobClipEvidenceStore::open_async(&bob_evidence_root, 4096)
            .await
            .unwrap();
        let reference = alice_evidence.retain_async(&artifact).await.unwrap();
        let provenance = serde_json::json!({
            "schema": "knot.clip.insert/v2",
            "evidence": [reference.clone()]
        });
        let source = format!(
            "# Replicated note\n\nThe authored body is ordinary Djot.\n\n```knot.clip.provenance\n{}\n```\n",
            serde_json::to_string(&provenance).unwrap()
        )
        .into_bytes();
        assert!(
            source
                .windows(artifact.bytes.len())
                .all(|window| window != artifact.bytes.as_slice())
        );
        let authored = VaultDocument {
            id: "portable-clip".into(),
            title: "Portable clip".into(),
            body: source,
            media_type: "text/vnd.djot".into(),
        };
        alice_store
            .author(
                alice.master_keypair().to_seed(),
                &alice_vault,
                &KnotSyncEvent::Put(authored.clone()),
            )
            .await
            .unwrap();

        let alice_blobs = alice_evidence.resident_blob_store().unwrap();
        let bob_blobs = bob_evidence.resident_blob_store().unwrap();
        let mut alice_host = KnotSyncHost::open_with_evidence(
            &alice_store,
            alice.master_keypair().to_seed(),
            KnotSyncHostConfig {
                authority: KnotSpaceAuthoritySnapshot::new(
                    KnotAuthoritySource::PersonalPairing,
                    [bob_writer],
                    [bob_writer],
                    [bob_writer],
                    [],
                ),
                relay_urls: vec![],
            },
            Arc::clone(&alice_blobs),
            4096,
        )
        .await
        .unwrap();
        assert!(alice_host.retain_evidence_custody(&reference).unwrap());
        let alice_ticket = alice_host.ticket().await.unwrap();
        let bob_host = KnotSyncHost::open_with_evidence(
            &bob_store,
            bob.master_keypair().to_seed(),
            KnotSyncHostConfig {
                authority: KnotSpaceAuthoritySnapshot::new(
                    KnotAuthoritySource::PersonalPairing,
                    [alice_writer],
                    [alice_writer],
                    [alice_writer],
                    [(alice_writer, alice_ticket)],
                ),
                relay_urls: vec![],
            },
            Arc::clone(&bob_blobs),
            4096,
        )
        .await
        .unwrap();

        let replicated = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Some(document) = bob_store
                    .projection(&bob_vault)
                    .await
                    .unwrap()
                    .documents
                    .into_iter()
                    .find(|document| document.id == authored.id)
                {
                    break document;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("paired Knot peers did not replicate the Djot operation");
        assert_eq!(replicated, authored);
        assert!(
            replicated
                .body
                .windows(reference.content_uri.len())
                .any(|window| window == reference.content_uri.as_bytes())
        );
        assert!(
            replicated
                .body
                .windows(artifact.bytes.len())
                .all(|window| window != artifact.bytes.as_slice())
        );

        let receipts = bob_host
            .fetch_document_evidence(&replicated, alice_writer)
            .await
            .unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].status, KnotEvidenceFetchStatus::Fetched);
        assert_eq!(
            bob_host.read_evidence(&reference).await.unwrap(),
            artifact.bytes
        );
        let second = bob_host
            .fetch_evidence(&reference, alice_writer)
            .await
            .unwrap();
        assert_eq!(second.status, KnotEvidenceFetchStatus::AlreadyPresent);
        let mut false_size = reference.clone();
        false_size.byte_size += 1;
        assert!(matches!(
            bob_host.read_evidence(&false_size).await,
            Err(KnotSyncHostError::EvidenceReference(_))
        ));

        let post_unpair_artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/plain".into(),
            canonical_uri: "https://example.test/after-unpair".into(),
            bytes: b"fresh bytes retained after peer revocation".to_vec(),
        };
        let post_unpair_reference = alice_evidence
            .retain_async(&post_unpair_artifact)
            .await
            .unwrap();
        assert!(
            alice_host
                .retain_evidence_custody(&post_unpair_reference)
                .unwrap()
        );
        assert!(
            alice_host
                .apply_authority(KnotSpaceAuthoritySnapshot::default())
                .await
                .unwrap()
        );
        assert!(!alice_host.evidence_authorizer().unwrap().allows(
            BlobScope::new(SPACE),
            &bob_writer,
            post_unpair_reference.blob_hash().unwrap()
        ));
        assert!(matches!(
            bob_host
                .fetch_evidence(&post_unpair_reference, alice_writer)
                .await,
            Err(KnotSyncHostError::EvidenceBlob(_))
        ));
        assert_eq!(
            alice_host
                .read_evidence(&post_unpair_reference)
                .await
                .unwrap(),
            post_unpair_artifact.bytes
        );

        alice_host.close().await.unwrap();
        bob_host.close().await.unwrap();
        drop(alice_evidence);
        drop(bob_evidence);
        alice_blobs.shutdown().await.unwrap();
        bob_blobs.shutdown().await.unwrap();
        drop(alice_blobs);
        drop(bob_blobs);

        let reopened = BlobStore::open(&bob_evidence_root).await.unwrap();
        let offline = reopened
            .get_bytes(reference.blob_hash().unwrap())
            .await
            .unwrap();
        reference.verify_bytes(&offline).unwrap();
        assert_eq!(offline.as_ref(), artifact.bytes.as_slice());
        reopened.shutdown().await.unwrap();
    }
}
