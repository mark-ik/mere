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

use std::sync::Arc;

use servitor::cap::{Cap, Mode};
use servitor::{AuthorityProvider, Subject};
use stickleback::{JoinError, JoinedSpace, SyncStatus};
use transport::p2panda_transport::{KnownPeer, MdnsDiscoveryMode, RelayUrl};
use transport::{
    BlobPeerAuthorizer, BlobReadAuthorizer, BlobScope, BlobStore, P2pandaTransport, PeerID,
    Transport, sync_overlay_topic,
};

use crate::VaultDocument;
use crate::clip_evidence::{KnotClipEvidenceRef, clip_evidence_references};
use crate::sync::{KnotEncryptionProfile, KnotSyncExt, KnotSyncFileStore};

/// How this device reaches the persona's other devices.
#[derive(Clone, Debug, Default)]
pub struct KnotSyncHostConfig {
    /// Writer keys of this persona's other devices. Each doubles as that
    /// device's transport node id, so one value serves both reachability and
    /// admission; unlike the personal graph, Knot does not need them recorded
    /// separately.
    pub paired_writers: Vec<[u8; 32]>,
    /// iroh relays. Empty leaves this device LAN-only: p2panda registers no
    /// relay by default.
    pub relay_urls: Vec<RelayUrl>,
    /// Endpoint tickets recorded from previous runs, seeded at open as
    /// best-effort dial candidates.
    ///
    /// Hints, not arguments: a ticket that fails to parse or dial is logged
    /// and skipped, because a route learned last week must degrade quietly
    /// where a value the owner just typed should fail loudly. Identity stays
    /// the writer key; this only turns a paired record into a route.
    pub peer_hints: Vec<String>,
}

/// Gemot/Servitor materialization for one communal Knot space.
///
/// The provider is expected to be a `gemot::moot::MootAuthority` built only
/// from retained constitution and delegation facts. Transport/session identity
/// supplies the subject being checked, never the authority answer.
#[derive(Clone, Debug)]
pub struct KnotCommunalPeerAuthority {
    /// Peers currently allowed to contribute document operations.
    pub writers: Vec<[u8; 32]>,
    /// Peers currently allowed to fetch evidence bytes for this space.
    pub evidence_readers: BlobPeerAuthorizer,
}

impl KnotCommunalPeerAuthority {
    /// Materialize document and evidence permissions for known candidate peers.
    pub fn from_authority(
        authority: &impl AuthorityProvider,
        space_id: [u8; 32],
        candidates: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, String> {
        let scope = format!("knot/{}", crate::hex32(&space_id));
        let document = Cap::scope(&format!("{scope}/document"))
            .map_err(|error| format!("invalid Knot document capability: {error}"))?;
        let evidence = Cap::scope(&format!("{scope}/evidence"))
            .map_err(|error| format!("invalid Knot evidence capability: {error}"))?;
        let candidates = candidates.into_iter().collect::<Vec<_>>();
        let mut writers = candidates
            .iter()
            .copied()
            .filter(|peer| authority.covers(Subject(*peer), &document, Mode::Write))
            .collect::<Vec<_>>();
        writers.sort_unstable();
        writers.dedup();
        let evidence_readers = BlobPeerAuthorizer::from_peers(
            candidates
                .into_iter()
                .filter(|peer| authority.covers(Subject(*peer), &evidence, Mode::Read)),
        );
        Ok(Self {
            writers,
            evidence_readers,
        })
    }
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
    transport: P2pandaTransport,
    store: KnotSyncFileStore,
    space_id: [u8; 32],
    evidence: Option<KnotEvidenceHost>,
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
        readers.replace_readers(scope, config.paired_writers.iter().copied());
        let sources = BlobPeerAuthorizer::from_peers(config.paired_writers.iter().copied());
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
        authority: KnotCommunalPeerAuthority,
    ) -> Result<Self, KnotSyncHostError> {
        if store.encryption_profile() != KnotEncryptionProfile::CommonsDataV1 {
            return Err(KnotSyncHostError::Authority(
                "Gemot authority requires a communal Knot space".into(),
            ));
        }
        if store.admitted_writers() != authority.writers {
            return Err(KnotSyncHostError::Authority(
                "communal store writers do not match materialized Gemot authority".into(),
            ));
        }
        let sources = BlobPeerAuthorizer::from_peers(authority.writers.iter().copied());
        let scope = BlobScope::new(store.space_id());
        let readers = BlobReadAuthorizer::new();
        readers.replace_readers(scope, authority.evidence_readers.peers());
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
        let mut builder = P2pandaTransport::builder_from_seed(signing_seed)
            .gossip()
            .mdns(MdnsDiscoveryMode::Active);
        if let Some(evidence) = &evidence {
            builder =
                builder.scoped_blobs(&evidence.blobs, evidence.scope, evidence.readers.clone());
        }
        for url in config.relay_urls {
            builder = builder.relay_url(url);
        }
        let transport = builder
            .bind()
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;

        let overlay = sync_overlay_topic(store.space_id());
        for writer in &config.paired_writers {
            let peer = PeerID::from_bytes(writer)
                .map_err(|error| KnotSyncHostError::Transport(format!("paired writer {error}")))?;
            transport
                .set_topics(peer, &[overlay])
                .await
                .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;
        }

        // The cached-address rung, as Graphshell has it: a device that has
        // connected once can redial after both ends restart with no discovery
        // working at all.
        for hint in &config.peer_hints {
            match transport.add_peer_ticket(hint).await {
                Ok(peer) => {
                    tracing::debug!(peer = %crate::hex32(&peer.to_bytes()), "seeded a stored dial hint")
                }
                Err(error) => {
                    tracing::warn!(%error, "a stored dial hint was unusable; skipping it")
                }
            }
        }

        let (endpoint, gossip) = transport
            .sync_parts()
            .ok_or_else(|| KnotSyncHostError::Transport("gossip is unavailable".into()))?;
        let joined = store.join(endpoint, gossip).await?;
        Ok(Self {
            joined,
            transport,
            store: store.clone(),
            space_id: store.space_id(),
            evidence,
        })
    }

    /// This device's node id, which is also its writer key: what the other
    /// devices must admit.
    pub fn node_id(&self) -> [u8; 32] {
        self.transport.local_peer_id().to_bytes()
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
            joined, transport, ..
        } = self;
        joined.leave_and_wait().await?;
        transport
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

    /// Replace current paired Personae peers. Existing local bytes remain
    /// retained; use [`Self::refresh_communal_evidence_authority`] for Gemot.
    pub fn refresh_evidence_authority(&self, readers: impl IntoIterator<Item = [u8; 32]>) -> bool {
        if self.store.encryption_profile() != KnotEncryptionProfile::PersonalVaultV1 {
            return false;
        }
        self.evidence.as_ref().is_some_and(|evidence| {
            let readers = readers.into_iter().collect::<Vec<_>>();
            let serving_changed = evidence
                .readers
                .replace_readers(evidence.scope, readers.iter().copied());
            let source_changed = evidence.sources.replace(readers);
            serving_changed || source_changed
        })
    }

    /// Replace a communal space's separately materialized readers and writers.
    pub fn refresh_communal_evidence_authority(
        &self,
        authority: KnotCommunalPeerAuthority,
    ) -> Result<bool, KnotSyncHostError> {
        if self.store.encryption_profile() != KnotEncryptionProfile::CommonsDataV1 {
            return Err(KnotSyncHostError::Authority(
                "Gemot authority cannot be applied to a personal Knot space".into(),
            ));
        }
        let writer_changed = self
            .store
            .replace_admitted_writers(authority.writers.iter().copied());
        let evidence_changed = self.evidence.as_ref().is_some_and(|evidence| {
            let serving_changed = evidence
                .readers
                .replace_readers(evidence.scope, authority.evidence_readers.peers());
            let source_changed = evidence.sources.replace(authority.writers);
            serving_changed || source_changed
        });
        Ok(writer_changed || evidence_changed)
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
            .fetch_from_named(&self.transport, peer, hash, tag.as_bytes())
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
        self.transport
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
        self.transport
            .peers_for_topic(sync_overlay_topic(self.space_id))
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Where the endpoint currently believes `writer` lives, as a ticket, if
    /// it holds any addresses for it. The value the cached-address rung
    /// persists back into settings.
    pub async fn peer_ticket(&self, writer: [u8; 32]) -> Result<Option<String>, KnotSyncHostError> {
        let peer = PeerID::from_bytes(&writer)
            .map_err(|error| KnotSyncHostError::Transport(format!("paired writer {error}")))?;
        self.transport
            .peer_ticket(peer)
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
    }

    /// Apply a newly persisted endpoint ticket without restarting the host.
    ///
    /// The ticket supplies reachability only. Its peer still needs writer and
    /// evidence admission through pairing before any Knot data is accepted.
    pub async fn add_peer_hint(&self, ticket: &str) -> Result<[u8; 32], KnotSyncHostError> {
        let peer = self
            .transport
            .add_peer_ticket(ticket)
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;
        Ok(peer.to_bytes())
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
    pub async fn refresh_dial_hints(
        &self,
        sync: &crate::KnotSyncSettings,
        settings_file: &std::path::Path,
    ) {
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
            if sync.endpoint_for(&writer) == Some(ticket.as_str()) {
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

    /// Admit and reach another device without a restart.
    pub async fn pair_writer(&self, writer: [u8; 32]) -> Result<(), KnotSyncHostError> {
        if self.store.encryption_profile() != KnotEncryptionProfile::PersonalVaultV1 {
            return Err(KnotSyncHostError::Authority(
                "Personae pairing cannot admit a writer to a communal Knot space".into(),
            ));
        }
        let peer = PeerID::from_bytes(&writer)
            .map_err(|error| KnotSyncHostError::Transport(format!("paired writer {error}")))?;
        self.transport
            .set_topics(peer, &[sync_overlay_topic(self.space_id)])
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))?;
        self.store.admit_writer(writer);
        if let Some(evidence) = &self.evidence {
            evidence.readers.allow_reader(evidence.scope, writer);
            evidence.sources.allow(writer);
        }
        Ok(())
    }

    /// Revoke a paired personal device without waiting for a restart.
    pub async fn unpair_writer(&self, writer: [u8; 32]) -> Result<(), KnotSyncHostError> {
        if self.store.encryption_profile() != KnotEncryptionProfile::PersonalVaultV1 {
            return Err(KnotSyncHostError::Authority(
                "Personae unpairing cannot alter a communal Knot space".into(),
            ));
        }
        let peer = PeerID::from_bytes(&writer)
            .map_err(|error| KnotSyncHostError::Transport(format!("paired writer {error}")))?;
        self.store.deny_writer(&writer);
        if let Some(evidence) = &self.evidence {
            evidence.readers.deny_reader(evidence.scope, &writer);
            evidence.sources.deny(&writer);
        }
        self.transport
            .remove_topic(peer, sync_overlay_topic(self.space_id))
            .await
            .map_err(|error| KnotSyncHostError::Transport(error.to_string()))
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

    use chirograph::{KnotClipArtifactRoleV1, KnotClipArtifactV1};
    use gemot::moot::constitution::{CapabilityGrant, ConstitutionRules};
    use gemot::moot::{MOOT_ACT_ACTION, MOOT_DELEGATION_DOMAIN, MootAuthority, MootDelegations};
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    };
    use personae::{IdentityProvider, InMemoryProvider};
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
        let space_scope = Cap::scope(&format!("knot/{}", crate::hex32(&SPACE))).unwrap();
        let document_scope =
            Cap::scope(&format!("knot/{}/document", crate::hex32(&SPACE))).unwrap();
        let evidence_scope =
            Cap::scope(&format!("knot/{}/evidence", crate::hex32(&SPACE))).unwrap();
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
            .accept_certificate(MOOT, &rules, issue(&evidence_reader, &evidence_scope, 2))
            .unwrap();
        let authority = MootAuthority {
            delegations: &delegations,
            rules: &rules,
            moot_id: MOOT,
            now_ms: 50,
        };
        let materialized = KnotCommunalPeerAuthority::from_authority(
            &authority,
            SPACE,
            [
                outsider.master_public_key().to_bytes(),
                evidence_reader.master_public_key().to_bytes(),
                document_writer.master_public_key().to_bytes(),
            ],
        )
        .unwrap();

        assert_eq!(
            materialized.writers,
            vec![document_writer.master_public_key().to_bytes()]
        );
        assert!(
            materialized
                .evidence_readers
                .allows(&evidence_reader.master_public_key().to_bytes())
        );
        assert!(
            !materialized
                .evidence_readers
                .allows(&document_writer.master_public_key().to_bytes())
        );
        assert!(
            !materialized
                .evidence_readers
                .allows(&outsider.master_public_key().to_bytes())
        );
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
        let alice_host = KnotSyncHost::open_with_evidence(
            &alice_store,
            alice.master_keypair().to_seed(),
            KnotSyncHostConfig {
                paired_writers: vec![bob_writer],
                relay_urls: vec![],
                peer_hints: vec![],
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
                paired_writers: vec![alice_writer],
                relay_urls: vec![],
                peer_hints: vec![alice_ticket],
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
