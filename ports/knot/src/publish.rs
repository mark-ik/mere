// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit, read-only publication of selected retained Knot documents.
//!
//! This module owns publication selection and source eligibility. It has no
//! transport: a carrier must admit a reader before asking it for a catalog or
//! a document, and it must turn every unavailable source state into the same
//! wire result.

use std::collections::{BTreeMap, BTreeSet};

use muniment::Backend;
use notochord::{NetworkId, RetainedAuthority, RevocationLedger};
use personae::IdentityProvider;
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationError, DelegationParent,
    DelegationRevocation, SignedDelegationCertificate, SignedDelegationRevocation,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{KnotSyncError, KnotSyncStore, KnotVault, VaultDocument};

/// The authenticated ALPN served by the Phase A publishing host.
pub const KNOT_PUBLISH_ALPN: &[u8] = b"mere/knot-publish/v1";
/// Notochord domain owning the publishing action vocabulary.
pub const KNOT_PUBLISH_DOMAIN: &str = "mere.knot";
/// Notochord service path used to admit a publishing session.
pub const KNOT_PUBLISH_SERVICE: &str = "/services/knot-publish";
/// Read-only action admitted and rechecked for every publication response.
pub const KNOT_PUBLISH_READ_ACTION: &str = "read";
/// Serialized handoff version for a Phase A share ticket.
pub const KNOT_SHARE_TICKET_VERSION: u16 = 1;

/// An owner-chosen opaque handle for one published document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PublicationId(Uuid);

impl PublicationId {
    /// Allocate a new opaque publication handle.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Construct a stable handle for persistence or test fixtures.
    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    /// The UUID form used by a candidate codec and logs that are allowed to
    /// name a publication.
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for PublicationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Scope path checked after admission for one publication.
pub fn publication_path(publication: PublicationId) -> String {
    format!("{KNOT_PUBLISH_SERVICE}/{}", publication.as_uuid())
}

/// A selected source document in the holder's local causal vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotPublication {
    /// Opaque id exposed to readers.
    pub id: PublicationId,
    /// Holder-local Knot document id. This never crosses the publication wire.
    pub source_document: String,
}

/// Owner-visible eligibility for one retained source document.
///
/// This is local control-plane information. A reader always receives the
/// single non-disclosing [`KnotPublishRead::NotAvailable`] outcome instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotPublishEligibility {
    Eligible,
    PendingHistory,
    Conflicted,
    AutomaticMerge,
    NoCurrentHead,
    UnsupportedMediaType,
}

/// A current source document the owner may inspect before explicitly selecting
/// it for publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotPublishCandidate {
    pub source_document: String,
    pub title: String,
    pub media_type: String,
    pub head: Option<[u8; 32]>,
    pub eligibility: KnotPublishEligibility,
}

/// Owner-controlled, explicit publication selection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnotPublishCatalog {
    publications: BTreeMap<PublicationId, KnotPublication>,
}

impl KnotPublishCatalog {
    /// Select a current retained source document for publication.
    pub fn publish(&mut self, source_document: impl Into<String>) -> PublicationId {
        let id = PublicationId::new();
        self.publish_as(id, source_document);
        id
    }

    /// Select a source under a caller-provided opaque handle.
    ///
    /// Replacing an existing handle is intentional: it lets an owner correct a
    /// local selection without exposing an intermediate catalog state.
    pub fn publish_as(&mut self, id: PublicationId, source_document: impl Into<String>) {
        self.publications.insert(
            id,
            KnotPublication {
                id,
                source_document: source_document.into(),
            },
        );
    }

    /// Withdraw a publication. Historical reads through this host stop too.
    pub fn unpublish(&mut self, id: PublicationId) -> Option<KnotPublication> {
        self.publications.remove(&id)
    }

    /// Whether this catalog contains an explicit selection for `id`.
    pub fn contains(&self, id: PublicationId) -> bool {
        self.publications.contains_key(&id)
    }

    /// Inspect the holder's retained sources before adding one explicitly to
    /// the catalog. This intentionally belongs to the owner control plane,
    /// never the reader protocol.
    pub async fn candidates<B>(
        &self,
        store: &KnotSyncStore<B>,
        vault: &KnotVault,
    ) -> Result<Vec<KnotPublishCandidate>, KnotPublishError>
    where
        B: Backend + Clone,
    {
        let projection = store.projection(vault).await?;
        let has_pending = !projection.pending.is_empty();
        let conflicts = projection
            .conflicts
            .iter()
            .map(|conflict| conflict.id.clone())
            .collect::<BTreeSet<_>>();
        let automatic_merges = projection
            .automatic_merges
            .iter()
            .map(|merge| merge.id.clone())
            .collect::<BTreeSet<_>>();
        let heads = projection.document_heads;
        Ok(projection
            .documents
            .into_iter()
            .map(|document| {
                let head = heads.get(&document.id).copied();
                let eligibility = if has_pending {
                    KnotPublishEligibility::PendingHistory
                } else if conflicts.contains(&document.id) {
                    KnotPublishEligibility::Conflicted
                } else if automatic_merges.contains(&document.id) {
                    KnotPublishEligibility::AutomaticMerge
                } else if head.is_none() {
                    KnotPublishEligibility::NoCurrentHead
                } else if !is_publishable_media_type(&document.media_type) {
                    KnotPublishEligibility::UnsupportedMediaType
                } else {
                    KnotPublishEligibility::Eligible
                };
                KnotPublishCandidate {
                    source_document: document.id,
                    title: document.title,
                    media_type: document.media_type,
                    head,
                    eligibility,
                }
            })
            .collect())
    }

    /// The holder-local selection for a publication, for the host's final
    /// unpublish check immediately before it writes a response.
    pub(crate) fn selection(&self, id: PublicationId) -> Option<&KnotPublication> {
        self.publications.get(&id)
    }

    /// List only publication ids the live authority covers.
    pub fn list(
        &self,
        authority: &RetainedAuthority,
        ledger: &RevocationLedger,
        now_ms: u64,
    ) -> Vec<PublicationId> {
        if authority.lapse(ledger, now_ms).is_some() {
            return Vec::new();
        }
        self.publications
            .keys()
            .copied()
            .filter(|id| authority.covers(&publication_path(*id), KNOT_PUBLISH_READ_ACTION, now_ms))
            .collect()
    }

    /// Fetch the selected document's sole current causal head.
    pub async fn get_current<B>(
        &self,
        store: &KnotSyncStore<B>,
        vault: &KnotVault,
        authority: &RetainedAuthority,
        ledger: &RevocationLedger,
        now_ms: u64,
        id: PublicationId,
    ) -> Result<KnotPublishRead, KnotPublishError>
    where
        B: Backend + Clone,
    {
        let Some(publication) = self.authorized_publication(authority, ledger, now_ms, id) else {
            return Ok(KnotPublishRead::NotAvailable);
        };
        let Some((document, head)) = self.current_eligible(store, vault, publication).await? else {
            return Ok(KnotPublishRead::NotAvailable);
        };
        Ok(KnotPublishRead::Document(KnotPublishedDocument::new(
            id, document, head,
        )))
    }

    /// Fetch one exact retained document-producing operation for a selected
    /// publication. The source must still be currently eligible: publication
    /// never turns an unresolved conflict, deletion, or incomplete history
    /// into an historical-export policy.
    pub async fn get_version<B>(
        &self,
        store: &KnotSyncStore<B>,
        vault: &KnotVault,
        authority: &RetainedAuthority,
        ledger: &RevocationLedger,
        now_ms: u64,
        id: PublicationId,
        operation: [u8; 32],
    ) -> Result<KnotPublishRead, KnotPublishError>
    where
        B: Backend + Clone,
    {
        let Some(publication) = self.authorized_publication(authority, ledger, now_ms, id) else {
            return Ok(KnotPublishRead::NotAvailable);
        };
        if self
            .current_eligible(store, vault, publication)
            .await?
            .is_none()
        {
            return Ok(KnotPublishRead::NotAvailable);
        }
        let Some(document) = store
            .document_version(vault, &publication.source_document, operation)
            .await?
        else {
            return Ok(KnotPublishRead::NotAvailable);
        };
        if !is_publishable_media_type(&document.media_type) {
            return Ok(KnotPublishRead::NotAvailable);
        }
        Ok(KnotPublishRead::Document(KnotPublishedDocument::new(
            id, document, operation,
        )))
    }

    /// Owner-side materialization for the separately configured Mark adapter.
    ///
    /// This intentionally has no reader authority parameter: the adapter has
    /// its own explicit export selection and Mark access policy. It does retain
    /// every source-eligibility rule from the native lane, so an unresolved
    /// causal state cannot become a false numeric Mark history.
    pub(crate) async fn current_for_mark_export<B>(
        &self,
        store: &KnotSyncStore<B>,
        vault: &KnotVault,
        id: PublicationId,
    ) -> Result<Option<KnotPublishedDocument>, KnotPublishError>
    where
        B: Backend + Clone,
    {
        let Some(publication) = self.publications.get(&id) else {
            return Ok(None);
        };
        let Some((document, head)) = self.current_eligible(store, vault, publication).await? else {
            return Ok(None);
        };
        Ok(Some(KnotPublishedDocument::new(id, document, head)))
    }

    /// Issue a recipient-bound, single-publication read ticket. Publication is
    /// never inferred from an open document: it must already be selected by
    /// this catalog. The ticket is secret material when its grant is secret.
    pub fn issue_share<P: IdentityProvider>(
        &self,
        issuer: &P,
        request: KnotShareRecipient,
    ) -> Result<KnotShareTicket, KnotShareControlError> {
        if !self.contains(request.publication) {
            return Err(KnotShareControlError::UnknownPublication);
        }
        let certificate = SignedDelegationCertificate::issue(
            issuer,
            DelegationCertificate::new(
                DelegationParent::Root(request.root_authority),
                issuer.master_public_key().to_bytes(),
                request.reader,
                CapabilityScope {
                    domain: KNOT_PUBLISH_DOMAIN.into(),
                    resource: request.network.0.to_vec(),
                    path_prefix: publication_path(request.publication),
                    actions: [KNOT_PUBLISH_READ_ACTION.into()].into_iter().collect(),
                },
                request.issued_at_ms,
                request.issued_at_ms,
                request.expires_at_ms,
                0,
                share_nonce(),
            ),
        )?;
        Ok(KnotShareTicket::new(
            request.publisher,
            request.endpoint_ticket,
            request.network,
            request.publication,
            vec![certificate],
            request.pinned_head,
        ))
    }

    fn authorized_publication<'a>(
        &'a self,
        authority: &RetainedAuthority,
        ledger: &RevocationLedger,
        now_ms: u64,
        id: PublicationId,
    ) -> Option<&'a KnotPublication> {
        authority
            .lapse(ledger, now_ms)
            .is_none()
            .then(|| self.publications.get(&id))
            .flatten()
            .filter(|_| authority.covers(&publication_path(id), KNOT_PUBLISH_READ_ACTION, now_ms))
    }

    async fn current_eligible<B>(
        &self,
        store: &KnotSyncStore<B>,
        vault: &KnotVault,
        publication: &KnotPublication,
    ) -> Result<Option<(VaultDocument, [u8; 32])>, KnotPublishError>
    where
        B: Backend + Clone,
    {
        let projection = store.projection(vault).await?;
        // A pending encrypted operation cannot safely be associated with a
        // document without decoding causal history the holder does not have.
        // Refusing publication while one exists is conservative and prevents
        // a partial causal view being presented as a stable source.
        if !projection.pending.is_empty()
            || projection
                .conflicts
                .iter()
                .any(|conflict| conflict.id == publication.source_document)
            || projection
                .automatic_merges
                .iter()
                .any(|merge| merge.id == publication.source_document)
        {
            return Ok(None);
        }
        let Some(head) = projection
            .document_heads
            .get(&publication.source_document)
            .copied()
        else {
            return Ok(None);
        };
        let Some(document) = projection
            .documents
            .into_iter()
            .find(|document| document.id == publication.source_document)
        else {
            return Ok(None);
        };
        if !is_publishable_media_type(&document.media_type) {
            return Ok(None);
        }
        Ok(Some((document, head)))
    }
}

/// Source outcome presented to a carrier. Every unavailable source state maps
/// to the same variant so the carrier cannot become a catalog oracle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnotPublishRead {
    Document(KnotPublishedDocument),
    NotAvailable,
}

/// The exact authored bytes and checks a reader may verify after a successful
/// source read. The holder-local document id is intentionally absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotPublishedDocument {
    pub publication: PublicationId,
    pub media_type: String,
    pub body: Vec<u8>,
    pub operation: [u8; 32],
    pub body_digest: [u8; 32],
}

impl KnotPublishedDocument {
    fn new(publication: PublicationId, document: VaultDocument, operation: [u8; 32]) -> Self {
        let body_digest = *blake3::hash(&document.body).as_bytes();
        Self {
            publication,
            media_type: document.media_type,
            body: document.body,
            operation,
            body_digest,
        }
    }

    /// Verify the advertised digest over the exact authored source bytes.
    pub fn body_digest_matches(&self) -> bool {
        *blake3::hash(&self.body).as_bytes() == self.body_digest
    }
}

/// A Phase A out-of-band handoff. Its delegation is supplied only to the
/// Notochord hello, never to a publication request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotShareTicket {
    pub version: u16,
    pub publisher: [u8; 32],
    pub endpoint_ticket: String,
    pub network: NetworkId,
    pub service_path: String,
    pub publication: PublicationId,
    pub delegations: Vec<SignedDelegationCertificate>,
    pub pinned_head: Option<[u8; 32]>,
}

/// The owner-visible facts required to share one selected publication with one
/// recipient. This has no vault key, writer key, or source pathname.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotShareRecipient {
    pub publication: PublicationId,
    /// The stable transport identity serving this publication. It is allowed
    /// to differ from the Personae root that signs the reader delegation: a
    /// product root can authorize one device's retained publishing host
    /// without turning that device key into the authority root.
    pub publisher: [u8; 32],
    pub reader: [u8; 32],
    pub network: NetworkId,
    pub endpoint_ticket: String,
    pub root_authority: [u8; 32],
    pub issued_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub pinned_head: Option<[u8; 32]>,
}

/// Revoke a share ticket's final certificate. The caller folds the returned
/// signed statement into the live [`RevocationLedger`] held by its host.
pub fn revoke_share<P: IdentityProvider>(
    issuer: &P,
    ticket: &KnotShareTicket,
    at_ms: u64,
) -> Result<SignedDelegationRevocation, KnotShareControlError> {
    let certificate = ticket
        .delegations
        .last()
        .ok_or(KnotShareControlError::MissingDelegation)?;
    Ok(SignedDelegationRevocation::issue(
        issuer,
        DelegationRevocation::new(
            certificate.certificate.id(),
            issuer.master_public_key().to_bytes(),
            certificate.certificate.scope.clone(),
            at_ms,
            share_nonce(),
        ),
    )?)
}

impl KnotShareTicket {
    /// Construct a ticket the recipient may carry out of band.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        publisher: [u8; 32],
        endpoint_ticket: impl Into<String>,
        network: NetworkId,
        publication: PublicationId,
        delegations: Vec<SignedDelegationCertificate>,
        pinned_head: Option<[u8; 32]>,
    ) -> Self {
        Self {
            version: KNOT_SHARE_TICKET_VERSION,
            publisher,
            endpoint_ticket: endpoint_ticket.into(),
            network,
            service_path: KNOT_PUBLISH_SERVICE.into(),
            publication,
            delegations,
            pinned_head,
        }
    }

    /// Check the invariant a reader applies before accepting a source body.
    pub fn accepts(&self, document: &KnotPublishedDocument) -> bool {
        self.version == KNOT_SHARE_TICKET_VERSION
            && self.service_path == KNOT_PUBLISH_SERVICE
            && self.publication == document.publication
            && self
                .pinned_head
                .is_none_or(|expected| expected == document.operation)
            && document.body_digest_matches()
    }
}

/// Internal source-materialization failure. This is never a wire refusal.
#[derive(Debug, thiserror::Error)]
pub enum KnotPublishError {
    #[error(transparent)]
    Sync(#[from] KnotSyncError),
}

/// Explicit owner-control failure. These are local UI/API results, never a
/// response to a reader that could use them to enumerate a catalog.
#[derive(Debug, thiserror::Error)]
pub enum KnotShareControlError {
    #[error("the document is not selected for publication")]
    UnknownPublication,
    #[error("the share ticket has no delegation to revoke")]
    MissingDelegation,
    #[error(transparent)]
    Delegation(#[from] DelegationError),
}

fn is_publishable_media_type(media_type: &str) -> bool {
    matches!(media_type, "text/vnd.knot" | "text/djot")
}

fn share_nonce() -> [u8; 32] {
    let left = Uuid::new_v4();
    let right = Uuid::new_v4();
    let mut nonce = [0u8; 32];
    nonce[..16].copy_from_slice(left.as_bytes());
    nonce[16..].copy_from_slice(right.as_bytes());
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KnotSyncEvent;
    use notochord::{AdmittedPrincipal, RequestedAction, TrafficClass};
    use personae::delegation::{CapabilityScope, DelegationCertificate, DelegationParent};
    use personae::{IdentityProvider, InMemoryProvider};
    use tempfile::tempdir;

    const NETWORK: NetworkId = NetworkId([0x91; 32]);
    const ROOT: [u8; 32] = [0x92; 32];
    const NOW_MS: u64 = 50;
    const EXPIRY_MS: u64 = 100;
    const SPACE: [u8; 32] = [0x93; 32];
    const VAULT_KEY: [u8; 32] = [0x94; 32];

    fn owner() -> InMemoryProvider {
        InMemoryProvider::from_seed([0x95; 32])
    }

    fn reader() -> InMemoryProvider {
        InMemoryProvider::from_seed([0x96; 32])
    }

    fn doc(id: &str, body: &str) -> VaultDocument {
        VaultDocument {
            id: id.into(),
            title: id.into(),
            body: body.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }
    }

    fn authority(publication: PublicationId) -> RetainedAuthority {
        let certificate = SignedDelegationCertificate::issue(
            &owner(),
            DelegationCertificate::new(
                DelegationParent::Root(ROOT),
                owner().master_public_key().to_bytes(),
                reader().master_public_key().to_bytes(),
                CapabilityScope {
                    domain: KNOT_PUBLISH_DOMAIN.into(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: publication_path(publication),
                    actions: [KNOT_PUBLISH_READ_ACTION.to_string()].into_iter().collect(),
                },
                5,
                10,
                Some(EXPIRY_MS),
                1,
                [0x97; 32],
            ),
        )
        .unwrap();
        RetainedAuthority::new(
            AdmittedPrincipal {
                subject: reader().master_public_key().to_bytes(),
                class: TrafficClass::Interactive,
                session_id: [0x98; 32],
                action: RequestedAction {
                    domain: KNOT_PUBLISH_DOMAIN.into(),
                    path: KNOT_PUBLISH_SERVICE.into(),
                    action: KNOT_PUBLISH_READ_ACTION.into(),
                },
            },
            vec![certificate],
        )
    }

    #[test]
    fn owner_controls_issue_and_revoke_one_recipient_share() {
        let mut catalog = KnotPublishCatalog::default();
        let selected = catalog.publish("field-notes");
        let ticket = catalog
            .issue_share(
                &owner(),
                KnotShareRecipient {
                    publication: selected,
                    publisher: [0x77; 32],
                    reader: reader().master_public_key().to_bytes(),
                    network: NETWORK,
                    endpoint_ticket: "endpoint-ticket".into(),
                    root_authority: ROOT,
                    issued_at_ms: NOW_MS,
                    expires_at_ms: Some(EXPIRY_MS),
                    pinned_head: Some([0x9a; 32]),
                },
            )
            .unwrap();
        assert_eq!(ticket.publisher, [0x77; 32]);
        assert_eq!(
            ticket.delegations.last().unwrap().certificate.issuer,
            owner().master_public_key().to_bytes(),
            "the Personae issuer and carrier publisher may be different keys"
        );
        assert_eq!(ticket.publication, selected);
        assert_eq!(ticket.pinned_head, Some([0x9a; 32]));
        let certificate = ticket.delegations.last().unwrap();
        assert!(certificate.verify());
        assert_eq!(
            certificate.certificate.scope.path_prefix,
            publication_path(selected)
        );
        assert_eq!(
            certificate.certificate.scope.actions,
            [KNOT_PUBLISH_READ_ACTION.to_string()].into_iter().collect()
        );

        let revocation = revoke_share(&owner(), &ticket, NOW_MS + 1).unwrap();
        assert!(revocation.verify());
        assert_eq!(
            revocation.revocation.certificate,
            certificate.certificate.id()
        );

        let unselected = PublicationId::from_uuid(Uuid::from_u128(99));
        assert!(matches!(
            catalog.issue_share(
                &owner(),
                KnotShareRecipient {
                    publication: unselected,
                    publisher: owner().master_public_key().to_bytes(),
                    reader: reader().master_public_key().to_bytes(),
                    network: NETWORK,
                    endpoint_ticket: "unreachable".into(),
                    root_authority: ROOT,
                    issued_at_ms: NOW_MS,
                    expires_at_ms: Some(EXPIRY_MS),
                    pinned_head: None,
                },
            ),
            Err(KnotShareControlError::UnknownPublication)
        ));
    }

    #[tokio::test]
    async fn selected_current_and_retained_versions_are_the_only_source_reads() {
        let directory = tempdir().unwrap();
        let vault = KnotVault::open(directory.path(), VAULT_KEY).unwrap();
        let writer = owner().master_public_key().to_bytes();
        let store = KnotSyncStore::in_memory(SPACE, [writer]);
        let first = store
            .author(
                owner().master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("selected", "first")),
            )
            .await
            .unwrap();
        let second = store
            .author(
                owner().master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("selected", "second")),
            )
            .await
            .unwrap();
        let unrelated = store
            .author(
                owner().master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("unrelated", "hidden")),
            )
            .await
            .unwrap();

        let mut catalog = KnotPublishCatalog::default();
        let selected = catalog.publish("selected");
        let other = catalog.publish("unrelated");
        let authority = authority(selected);
        assert_eq!(
            catalog.list(&authority, &RevocationLedger::default(), NOW_MS),
            vec![selected],
            "a one-publication grant does not enumerate a neighbour"
        );

        let current = catalog
            .get_current(
                &store,
                &vault,
                &authority,
                &RevocationLedger::default(),
                NOW_MS,
                selected,
            )
            .await
            .unwrap();
        let KnotPublishRead::Document(current) = current else {
            panic!("the selected current document must be available")
        };
        assert_eq!(current.body, b"second");
        assert_eq!(current.operation, *second.hash.as_bytes());
        assert!(current.body_digest_matches());

        let retained = catalog
            .get_version(
                &store,
                &vault,
                &authority,
                &RevocationLedger::default(),
                NOW_MS,
                selected,
                *first.hash.as_bytes(),
            )
            .await
            .unwrap();
        let KnotPublishRead::Document(retained) = retained else {
            panic!("the exact retained Put must be available")
        };
        assert_eq!(retained.body, b"first");
        assert_eq!(retained.operation, *first.hash.as_bytes());

        for outcome in [
            catalog
                .get_current(
                    &store,
                    &vault,
                    &authority,
                    &RevocationLedger::default(),
                    NOW_MS,
                    other,
                )
                .await
                .unwrap(),
            catalog
                .get_version(
                    &store,
                    &vault,
                    &authority,
                    &RevocationLedger::default(),
                    NOW_MS,
                    selected,
                    *unrelated.hash.as_bytes(),
                )
                .await
                .unwrap(),
        ] {
            assert_eq!(outcome, KnotPublishRead::NotAvailable);
        }
    }

    #[tokio::test]
    async fn deletes_conflicts_and_pending_history_are_not_available() {
        let directory = tempdir().unwrap();
        let vault = KnotVault::open(directory.path(), VAULT_KEY).unwrap();
        let alice = owner();
        let bob = InMemoryProvider::from_seed([0x99; 32]);
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];

        let deleted = KnotSyncStore::in_memory(SPACE, writers);
        deleted
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("deleted", "before delete")),
            )
            .await
            .unwrap();
        deleted
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Delete {
                    id: "deleted".into(),
                },
            )
            .await
            .unwrap();

        let conflict_left = KnotSyncStore::in_memory(SPACE, writers);
        let conflict_right = KnotSyncStore::in_memory(SPACE, writers);
        let left = conflict_left
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("conflict", "alice")),
            )
            .await
            .unwrap();
        let right = conflict_right
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("conflict", "bob")),
            )
            .await
            .unwrap();
        conflict_left.accept(&right).await.unwrap();

        let parent = KnotSyncStore::in_memory(SPACE, writers);
        let child = KnotSyncStore::in_memory(SPACE, writers);
        let pending = KnotSyncStore::in_memory(SPACE, writers);
        let base = parent
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("pending", "base")),
            )
            .await
            .unwrap();
        child.accept(&base).await.unwrap();
        let child_operation = child
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("pending", "child")),
            )
            .await
            .unwrap();
        pending.accept(&child_operation).await.unwrap();

        for (store, source) in [
            (&deleted, "deleted"),
            (&conflict_left, "conflict"),
            (&pending, "pending"),
        ] {
            let mut catalog = KnotPublishCatalog::default();
            let publication = catalog.publish(source);
            let outcome = catalog
                .get_current(
                    store,
                    &vault,
                    &authority(publication),
                    &RevocationLedger::default(),
                    NOW_MS,
                    publication,
                )
                .await
                .unwrap();
            assert_eq!(outcome, KnotPublishRead::NotAvailable);
        }
        assert_ne!(*left.hash.as_bytes(), *right.hash.as_bytes());
    }

    #[tokio::test]
    async fn a_ticket_checks_its_pinned_causal_head_and_exact_bytes() {
        let directory = tempdir().unwrap();
        let vault = KnotVault::open(directory.path(), VAULT_KEY).unwrap();
        let writer = owner().master_public_key().to_bytes();
        let store = KnotSyncStore::in_memory(SPACE, [writer]);
        let operation = store
            .author(
                owner().master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("selected", "source")),
            )
            .await
            .unwrap();
        let mut catalog = KnotPublishCatalog::default();
        let publication = catalog.publish("selected");
        let current = catalog
            .get_current(
                &store,
                &vault,
                &authority(publication),
                &RevocationLedger::default(),
                NOW_MS,
                publication,
            )
            .await
            .unwrap();
        let KnotPublishRead::Document(document) = current else {
            panic!("selected source must be available")
        };
        let ticket = KnotShareTicket::new(
            writer,
            "endpoint-ticket",
            NETWORK,
            publication,
            Vec::new(),
            Some(*operation.hash.as_bytes()),
        );
        assert!(ticket.accepts(&document));

        let wrong_head = KnotShareTicket::new(
            writer,
            "endpoint-ticket",
            NETWORK,
            publication,
            Vec::new(),
            Some([0; 32]),
        );
        assert!(!wrong_head.accepts(&document));
        let mut tampered = document;
        tampered.body.push(b'!');
        assert!(!ticket.accepts(&tampered));
    }
}
