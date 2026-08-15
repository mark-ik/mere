//! Graphshell endpoint adapter for the resident Personae authority.
//!
//! The browser-facing side sees ordinary portable cards and typed intents.
//! This adapter remains native because it holds the in-process authority. A
//! carrier must admit a session before passing this endpoint to
//! `serve_admitted_session`; nothing here invents a second principal field.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource};
use chirograph::{
    AdvertisedAction, BoundsRelationship, CachePolicy, CacheRetention, ContentHash,
    EndpointDescriptor, IntentEffect, IntentInvocation, IntentReference, IntentResult,
    PortableCardV1, PresentationBinding, PresentationCapability, PresentationCodec,
    PresentationKey, PresentationManifest, PresentationOffer, PresentationSemantics,
    ProjectionOffer, ProjectionRequest, ProjectionSession, ProjectionSnapshot, ProtocolVersion,
    ResourceChunkRequest, ResourceChunkResponse, ResourceRequest, ResourceResponse, SemanticRole,
};
use personae::IdentityStorage;
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Rect, Representation, Scene, Score, Size2,
    SourceRef, Transform2, Vec2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};

use crate::identity::IdentitySurfaceSnapshot;
use crate::identity_projection::{
    IdentityProjectionAction, SIGNING_APPROVE_IDLE_INTENT, SIGNING_APPROVE_ONCE_INTENT,
    SIGNING_DENY_INTENT, project_identity,
};
use crate::native::browser_host::now_ms;
use crate::native::personae_host::PersonaeHost;

pub const IDENTITY_SESSION: &str = "native:personae";

/// Reads a blob the endpoint does not hold, by content hash.
///
/// Synchronous because the resource trait is; a composer over an async store
/// bridges it (see [`IdentityEndpoint::with_reader`]).
pub type ResourceReader = Box<dyn Fn(&ContentHash) -> Option<Vec<u8>> + Send + Sync>;

/// How much read-through content the endpoint keeps resident.
///
/// Generous enough that browsing a run's captures does not re-read on every
/// chunk, small enough that a graph of thousands cannot exhaust memory.
const READ_CACHE_BUDGET: usize = 64 * 1024 * 1024;

/// One card composed beside the Personae surface.
///
/// Read-only by default: `actions` is empty unless the composer has something
/// the person can actually decide, which today is accepting a waiting
/// transfer. Keeping the default empty is what stops "supplemental" from
/// quietly becoming a second, unaudited action surface.
#[derive(Clone, Debug, PartialEq)]
pub struct SupplementalCard {
    pub adapter: String,
    pub source_id: String,
    pub card: PortableCardV1,
    pub actions: Vec<IdentityProjectionAction>,
}

impl SupplementalCard {
    /// A card that shows and does nothing.
    pub fn read_only(
        adapter: impl Into<String>,
        source_id: impl Into<String>,
        card: PortableCardV1,
    ) -> Self {
        Self {
            adapter: adapter.into(),
            source_id: source_id.into(),
            card,
            actions: Vec::new(),
        }
    }
}

/// Intent a person invokes to accept a waiting transfer.
pub const TRANSFER_ACCEPT_INTENT: &str = "graphshell.transfer.accept/v1";
pub const TRANSFER_ACCEPT_SCHEMA: &str = "graphshell.TransferAcceptIntent/v1";

/// The payload the accept action carries.
///
/// The transfer id is an advertised choice on the action's bounded form, so
/// the browser composes a decision about one named transfer rather than "the
/// transfer", which would be ambiguous the moment two are waiting.
///
/// `schema` is the form's own marker, carried so a payload composed against a
/// different or stale form is refused rather than read as an accept.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TransferAcceptIntentV1 {
    pub schema: String,
    pub transfer_id: String,
}

/// One accepted transfer, recorded for the resident host to act on.
///
/// The gesture is synchronous and fetching bytes is not, so accepting records
/// a decision rather than performing it. What the person agreed to is durable
/// the moment they agree; the work it implies happens where awaiting is
/// possible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferDecision {
    pub transfer_id: String,
    pub at_ms: u64,
}

/// Decisions waiting for the resident host. A `std::sync::Mutex` rather than
/// tokio's, because the endpoint that pushes cannot await and the critical
/// section is a push or a drain.
pub type TransferDecisions = std::sync::Arc<std::sync::Mutex<Vec<TransferDecision>>>;

#[derive(Debug, thiserror::Error)]
pub enum IdentityEndpointError {
    #[error("identity authority read failed: {0}")]
    Read(#[from] std::io::Error),
    #[error("identity projection serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("identity projection snapshot is invalid: {0}")]
    InvalidSnapshot(String),
    #[error("request names another projection session")]
    WrongSession,
    #[error("identity resource was not disclosed by this session")]
    MissingResource,
    #[error(
        "transfer holds {bytes} bytes, over the {ceiling}-byte ceiling for \
         blobs released to a browser; this transfer needs the streaming path"
    )]
    TransferTooLarge { bytes: usize, ceiling: usize },
}

/// Most bytes one accepted transfer may hold resident for a browser to pull.
///
/// A ceiling exists because these bytes stay in memory until the browser has
/// them: the endpoint trait is synchronous, so it cannot read a blob store on
/// demand, and the resident host loads them ahead of time instead. Sized for
/// the documents and images a person moves between their own devices, not for
/// archives. Raising it trades memory for reach; removing the ceiling wants
/// the streaming path rather than a bigger number.
pub const MAX_RELEASED_TRANSFER_BYTES: usize = 64 * 1024 * 1024;

/// Native identity authority exposed through Graphshell's ordinary endpoint
/// vocabulary.
pub struct IdentityEndpoint<S: IdentityStorage> {
    host: Arc<PersonaeHost<S>>,
    session: ProjectionSession,
    epoch: u64,
    revision: u64,
    last_public_snapshot: Option<Vec<u8>>,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    /// Blobs granted to this browser by an accepted transfer, kept out of
    /// `resources` because a projection refresh replaces that map wholesale.
    released: BTreeMap<ContentHash, Vec<u8>>,
    /// Reads a blob this endpoint does not hold, from wherever the composing
    /// host keeps them. `None` means resources are exactly what was staged.
    reader: Option<ResourceReader>,
    /// What `reader` has produced, bounded by [`READ_CACHE_BUDGET`].
    fetched: BTreeMap<ContentHash, Vec<u8>>,
    /// Insertion order, for evicting the oldest first.
    fetched_order: VecDeque<ContentHash>,
    fetched_bytes: usize,
    instance_actions: Vec<BTreeSet<String>>,
    supplemental_cards: Vec<SupplementalCard>,
    /// Where an accepted transfer is recorded. `None` leaves the accept action
    /// unserved, so a surface composed without it refuses rather than
    /// pretending to accept.
    decisions: Option<TransferDecisions>,
}

impl<S: IdentityStorage + 'static> IdentityEndpoint<S> {
    pub fn new(host: Arc<PersonaeHost<S>>) -> Self {
        Self::with_session(host, ProjectionSession(IDENTITY_SESSION.to_string()))
    }

    /// Bind the projection to the transcript-derived session retained after
    /// carrier admission.
    pub fn for_admitted(
        host: Arc<PersonaeHost<S>>,
        authority: &crate::lifecycle::SessionAuthority,
    ) -> Self {
        Self::with_session(host, authority.session().clone())
    }

    pub fn for_admitted_with_cards(
        host: Arc<PersonaeHost<S>>,
        authority: &crate::lifecycle::SessionAuthority,
        supplemental_cards: Vec<SupplementalCard>,
    ) -> Self {
        let mut endpoint = Self::with_session(host, authority.session().clone());
        endpoint.supplemental_cards = supplemental_cards;
        endpoint
    }

    fn with_session(host: Arc<PersonaeHost<S>>, session: ProjectionSession) -> Self {
        Self {
            host,
            session,
            epoch: 1,
            revision: 1,
            last_public_snapshot: None,
            resources: BTreeMap::new(),
            released: BTreeMap::new(),
            reader: None,
            fetched: BTreeMap::new(),
            fetched_order: VecDeque::new(),
            fetched_bytes: 0,
            instance_actions: Vec::new(),
            supplemental_cards: Vec::new(),
            decisions: None,
        }
    }

    /// Where this endpoint records a person's accept.
    pub fn with_decisions(&mut self, decisions: TransferDecisions) {
        self.decisions = Some(decisions);
    }

    /// Record one accept. The intent gating upstream has already confirmed
    /// this action was advertised on the card the person acted on.
    ///
    /// Idempotent by transfer: a double-click, or a browser retrying after a
    /// dropped reply, must not queue the same transfer twice.
    fn accept_transfer(&self, payload: &[u8]) -> IntentResult {
        let Some(decisions) = self.decisions.as_ref() else {
            return IntentResult::Rejected {
                reason: "this device is not composing transfers, so it cannot accept one"
                    .to_string(),
            };
        };
        let accepted: TransferAcceptIntentV1 = match serde_json::from_slice(payload) {
            Ok(accepted) => accepted,
            Err(error) => {
                return IntentResult::Rejected {
                    reason: format!("accept payload was not understood: {error}"),
                };
            }
        };
        // Everything the payload can be wrong about is settled before the
        // queue is touched, so a refusal never holds the lock.
        if accepted.schema != TRANSFER_ACCEPT_SCHEMA {
            return IntentResult::Rejected {
                reason: format!(
                    "accept payload names schema {}, not {TRANSFER_ACCEPT_SCHEMA}",
                    accepted.schema
                ),
            };
        }
        if accepted.transfer_id.trim().is_empty() {
            return IntentResult::Rejected {
                reason: "accept payload names no transfer".to_string(),
            };
        }
        let mut queue = match decisions.lock() {
            Ok(queue) => queue,
            // A poisoned queue means another thread panicked mid-decision.
            // Refusing is right: the person can act again, and pushing onto
            // state of unknown shape is not recoverable by them.
            Err(_) => {
                return IntentResult::Rejected {
                    reason: "transfer decisions are unavailable on this device".to_string(),
                };
            }
        };
        if !queue
            .iter()
            .any(|decision| decision.transfer_id == accepted.transfer_id)
        {
            queue.push(TransferDecision {
                transfer_id: accepted.transfer_id,
                at_ms: now_ms(),
            });
        }
        IntentResult::Accepted
    }

    /// Make one accepted transfer's blobs pullable by this admitted browser.
    ///
    /// Held apart from `resources`, which a projection refresh replaces
    /// wholesale, and which is derived from identity rather than granted.
    /// Keeping them in separate maps is what makes "which bytes has this
    /// browser been given access to" a question with an answer.
    ///
    /// Nothing outside this set is servable, whether or not the device holds
    /// it. An admitted extension knowing a hash is not authorization to read
    /// the bytes behind it.
    ///
    /// Refuses rather than truncates past [`MAX_RELEASED_TRANSFER_BYTES`]:
    /// these bytes stay resident until the browser has pulled them, which is
    /// the cost of an endpoint that cannot await a store read. A transfer over
    /// the ceiling needs the streaming path, and should say so rather than
    /// half-arrive.
    pub fn release_transfer(
        &mut self,
        blobs: Vec<(ContentHash, Vec<u8>)>,
    ) -> Result<(), IdentityEndpointError> {
        let total: usize = blobs.iter().map(|(_, bytes)| bytes.len()).sum();
        if total > MAX_RELEASED_TRANSFER_BYTES {
            return Err(IdentityEndpointError::TransferTooLarge {
                bytes: total,
                ceiling: MAX_RELEASED_TRANSFER_BYTES,
            });
        }
        self.released = blobs.into_iter().collect();
        Ok(())
    }

    /// Drop every released blob. Called once a transfer has been applied, so
    /// the grant does not outlive the reason for it.
    pub fn retire_released(&mut self) {
        self.released.clear();
    }

    pub fn released_count(&self) -> usize {
        self.released.len()
    }

    /// Identity resources first, then blobs released by an accepted transfer.
    /// Both are readable by an admitted browser; only the second was granted.
    /// The bytes for a resource, reading through to the store if the endpoint
    /// does not already hold them.
    ///
    /// Takes `&mut self` because a read populates the cache; both callers
    /// already had `&mut self`. Staged and released blobs are answered without
    /// touching the reader, so a transfer still costs nothing extra.
    fn bytes_for(&mut self, resource: &ContentHash) -> Option<&[u8]> {
        if self.resources.contains_key(resource) {
            return self.resources.get(resource).map(Vec::as_slice);
        }
        if self.released.contains_key(resource) {
            return self.released.get(resource).map(Vec::as_slice);
        }
        if !self.fetched.contains_key(resource) {
            let bytes = (self.reader.as_ref()?)(resource)?;
            self.admit_fetched(*resource, bytes);
        }
        self.fetched.get(resource).map(Vec::as_slice)
    }

    /// Cache a read blob, evicting oldest-first to stay inside the budget.
    ///
    /// Bounded rather than unbounded because the whole point of reading
    /// through is that the store may hold far more than fits in memory; a
    /// cache that grew without limit would reintroduce exactly the problem
    /// the read-through solves.
    fn admit_fetched(&mut self, resource: ContentHash, bytes: Vec<u8>) {
        let size = bytes.len();
        while self.fetched_bytes + size > READ_CACHE_BUDGET {
            let Some(oldest) = self.fetched_order.pop_front() else {
                break;
            };
            if let Some(dropped) = self.fetched.remove(&oldest) {
                self.fetched_bytes -= dropped.len();
            }
        }
        self.fetched_bytes += size;
        self.fetched_order.push_back(resource);
        self.fetched.insert(resource, bytes);
    }

    /// Read blobs this endpoint does not hold from the composing host's store.
    ///
    /// The endpoint deliberately does not know what a store is: it lives in
    /// the `native` cone and the stores live in `web`/`personal-sync`, and the
    /// resource trait is synchronous while a store read is not. So the
    /// composer supplies a closure and decides how to bridge that (a
    /// multi-thread runtime can use `block_in_place`).
    pub fn with_reader(&mut self, reader: ResourceReader) {
        self.reader = Some(reader);
    }

    pub fn host(&self) -> &Arc<PersonaeHost<S>> {
        &self.host
    }

    pub fn session(&self) -> ProjectionSession {
        self.session.clone()
    }

    pub fn request(&self) -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: self.session(),
            score: Score::new(Arrangement::Spiral(Default::default())),
        }
    }

    fn observe(&mut self) -> Result<IdentitySurfaceSnapshot, IdentityEndpointError> {
        let snapshot = self.host.snapshot()?;
        let public = serde_json::to_vec(&snapshot)?;
        if self
            .last_public_snapshot
            .as_ref()
            .is_some_and(|previous| previous != &public)
        {
            self.revision = self.revision.saturating_add(1);
        }
        self.last_public_snapshot = Some(public);
        Ok(snapshot)
    }

    fn build_snapshot(&mut self) -> Result<ProjectionSnapshot, IdentityEndpointError> {
        let snapshot = self.observe()?;
        let mut cards = project_identity(&snapshot)
            .into_iter()
            .map(|projected| {
                (
                    "personae",
                    "personae.public".to_string(),
                    projected.key,
                    projected.card,
                    projected.actions,
                )
            })
            .collect::<Vec<_>>();
        cards.extend(self.supplemental_cards.iter().cloned().map(|supplemental| {
            (
                "device",
                supplemental.adapter,
                supplemental.source_id,
                supplemental.card,
                supplemental.actions,
            )
        }));
        let mut scene = Scene::new();
        let mut presentation = PresentationManifest::default();
        let mut resources = BTreeMap::new();
        let mut instance_actions = Vec::with_capacity(cards.len());

        const WIDTH: f32 = 260.0;
        const HEIGHT: f32 = 136.0;
        const GAP_X: f32 = 28.0;
        const GAP_Y: f32 = 24.0;
        const COLUMNS: usize = 3;

        for (index, (presentation_namespace, adapter, source_id, card, card_actions)) in
            cards.into_iter().enumerate()
        {
            let instance = InstanceId(index as u32);
            let column = (index % COLUMNS) as f32;
            let row = (index / COLUMNS) as f32;
            let x = column * (WIDTH + GAP_X);
            let y = row * (HEIGHT + GAP_Y);
            let source = scene.intern_source(SourceRef::new(adapter, source_id));
            scene.items.push(ProjectedItem {
                source,
                space: Scene::WORLD,
                transform: Transform2::translation(x, y),
                footprint: Footprint::Rect {
                    size: Size2::new(WIDTH, HEIGHT),
                },
                representation: Representation::Card,
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });

            let bytes = serde_json::to_vec(&card)?;
            let resource = ContentHash::of(&bytes);
            let key = PresentationKey(format!("{presentation_namespace}:card:{index}"));
            let actions = card_actions
                .iter()
                .map(advertised_action)
                .collect::<Vec<_>>();
            instance_actions.push(
                card_actions
                    .into_iter()
                    .map(|action| action.intent.to_string())
                    .collect(),
            );
            presentation.bindings.push(PresentationBinding {
                instance,
                key: key.clone(),
            });
            presentation.offers.insert(
                key,
                vec![PresentationOffer {
                    codec: PresentationCodec::PortableCardV1,
                    resource,
                    byte_size: bytes.len() as u64,
                    requires: PresentationCapability::PortableCard,
                    semantics: PresentationSemantics {
                        label: card.title,
                        role: SemanticRole::Article,
                        bounds: BoundsRelationship::FillFootprint,
                        actions,
                    },
                }],
            );
            resources.insert(resource, bytes);
        }

        let rows = scene.items.len().div_ceil(COLUMNS);
        let columns = scene.items.len().min(COLUMNS);
        scene.bounds = if scene.items.is_empty() {
            Rect::new(Vec2::new(0.0, 0.0), Size2::new(0.0, 0.0))
        } else {
            Rect::new(
                Vec2::new(0.0, 0.0),
                Size2::new(
                    columns as f32 * WIDTH + columns.saturating_sub(1) as f32 * GAP_X,
                    rows as f32 * HEIGHT + rows.saturating_sub(1) as f32 * GAP_Y,
                ),
            )
        };
        scene.generation = self.revision;
        let scene =
            SceneSnapshot::from_dense(SceneEpoch(self.epoch), Revision(self.revision), scene)
                .map_err(|error| IdentityEndpointError::InvalidSnapshot(format!("{error:?}")))?;

        self.resources = resources;
        self.instance_actions = instance_actions;
        Ok(ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session(),
            scene,
            presentation,
            cache_policy: CachePolicy {
                retention: CacheRetention::Exportable,
                expires_at_ms: None,
                purge_on_revocation: true,
            },
        })
    }

    fn refresh_revision(&mut self) -> Result<(), IdentityEndpointError> {
        self.observe().map(drop)
    }

    fn mark_changed(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.last_public_snapshot = None;
    }
}

fn advertised_action(action: &IdentityProjectionAction) -> AdvertisedAction {
    let signing_decision = matches!(
        action.intent,
        SIGNING_APPROVE_ONCE_INTENT | SIGNING_APPROVE_IDLE_INTENT | SIGNING_DENY_INTENT
    );
    AdvertisedAction {
        intent: IntentReference(action.intent.to_string()),
        label: action.label.to_string(),
        explanation: if action.native_only {
            "Runs in the native Personae authority through the admitted device session.".to_string()
        } else {
            "Runs in the disclosing identity authority.".to_string()
        },
        payload_schema: action.schema.to_string(),
        // Carried through rather than dropped. An action whose payload the
        // browser must compose is unusable without its form: the button either
        // does not render or submits a payload naming nothing.
        input_form: action.input_form.clone(),
        effect: if signing_decision {
            IntentEffect::ExternalEffect
        } else {
            IntentEffect::DomainTruth
        },
    }
}

impl<S: IdentityStorage + 'static> ProjectionCatalog for IdentityEndpoint<S> {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Local identity authority".to_string(),
            projections: vec![ProjectionOffer {
                label: "Identity".to_string(),
                request: self.request(),
            }],
        }
    }
}

impl<S: IdentityStorage + 'static> ProjectionSource for IdentityEndpoint<S> {
    type Error = IdentityEndpointError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session() || request.version.major != ProtocolVersion::V1.major {
            return Err(IdentityEndpointError::WrongSession);
        }
        self.build_snapshot()
    }
}

impl<S: IdentityStorage + 'static> PresentationSource for IdentityEndpoint<S> {
    type Error = IdentityEndpointError;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session() {
            return Err(IdentityEndpointError::WrongSession);
        }
        let bytes = self
            .bytes_for(&request.resource)
            .ok_or(IdentityEndpointError::MissingResource)?
            .to_vec();
        Ok(ResourceResponse {
            session: request.session,
            resource: request.resource,
            bytes,
        })
    }

    /// Overridden so serving a blob in N pieces copies it once rather than N
    /// times. The default reads the whole resource per chunk, which is right
    /// but quadratic, and a released transfer blob is exactly the case where
    /// that shows.
    fn resource_chunk(
        &mut self,
        request: ResourceChunkRequest,
    ) -> Result<ResourceChunkResponse, Self::Error> {
        if request.session != self.session() {
            return Err(IdentityEndpointError::WrongSession);
        }
        let bytes = self
            .bytes_for(&request.resource)
            .ok_or(IdentityEndpointError::MissingResource)?;
        Ok(ResourceChunkResponse::from_slice(
            request.session,
            request.resource,
            bytes,
            request.offset,
            request.length,
        ))
    }
}

impl<S: IdentityStorage + 'static> IntentSink for IdentityEndpoint<S> {
    type Error = IdentityEndpointError;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session() {
            return Err(IdentityEndpointError::WrongSession);
        }
        self.refresh_revision()?;
        if intent.observed_epoch != SceneEpoch(self.epoch)
            || intent.observed_revision != Revision(self.revision)
        {
            return Ok(IntentResult::Stale {
                current_epoch: SceneEpoch(self.epoch),
                current_revision: Revision(self.revision),
            });
        }
        let Some(actions) = self.instance_actions.get(intent.target.0 as usize) else {
            return Ok(IntentResult::Rejected {
                reason: "intent target is not in the disclosed identity scene".to_string(),
            });
        };
        if !actions.contains(&intent.intent) {
            return Ok(IntentResult::Rejected {
                reason: "intent was not advertised for the selected identity card".to_string(),
            });
        }

        // A supplemental card's action is not the identity authority's to
        // answer. Routing it there would either fail confusingly or, worse,
        // grow into a path where a composed card can reach the vault.
        if intent.intent == TRANSFER_ACCEPT_INTENT {
            return Ok(self.accept_transfer(&intent.payload));
        }

        match self.host.apply_intent(&intent.intent, &intent.payload) {
            Ok(_) => {
                self.mark_changed();
                Ok(IntentResult::Accepted)
            }
            Err(error) => Ok(IntentResult::Rejected {
                reason: error.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use graphshell_client::{ClientState, PresentationResolution, ResolvedContent};
    use chirograph::ResourceAssembly;
    use personae::{Ed25519Keypair, IdentityVault, InMemoryStorage, Profile, ProfileId};
    use ssh_key::{Algorithm, LineEnding};

    use super::*;
    use crate::identity::VaultProtectionView;
    use crate::identity_projection::{
        GenerateSshKeyIntentV1, SSH_GENERATE_INTENT, SshUnlockPolicyIntentV1,
    };

    /// Content a card names but nothing staged is read through to the store.
    /// This is what lets a receipt's captures be opened without holding every
    /// capture in the graph resident.
    #[test]
    fn a_resource_absent_from_memory_is_read_through() {
        let (mut endpoint, _) = endpoint_with_private_sentinel();
        let bytes = b"capture pixels".to_vec();
        let hash = ContentHash::of(&bytes);
        let served = bytes.clone();
        endpoint.with_reader(Box::new(move |asked| {
            (*asked == hash).then(|| served.clone())
        }));

        let response = endpoint
            .resource(ResourceRequest {
                session: endpoint.session(),
                resource: hash,
            })
            .expect("the store answers for a blob it holds");
        assert_eq!(response.bytes, bytes);

        // An unknown hash is still a miss: read-through must not invent bytes.
        assert!(
            endpoint
                .resource(ResourceRequest {
                    session: endpoint.session(),
                    resource: ContentHash::of(b"never stored"),
                })
                .is_err()
        );
    }

    /// Without a reader the endpoint serves exactly what was staged, which is
    /// the behaviour every composer had before read-through existed.
    #[test]
    fn no_reader_means_no_invention() {
        let (mut endpoint, _) = endpoint_with_private_sentinel();
        assert!(
            endpoint
                .resource(ResourceRequest {
                    session: endpoint.session(),
                    resource: ContentHash::of(b"anything"),
                })
                .is_err()
        );
    }

    /// The read cache is bounded, so browsing a large graph cannot grow
    /// memory without limit.
    #[test]
    fn the_read_cache_evicts_oldest_first() {
        let (mut endpoint, _) = endpoint_with_private_sentinel();
        // Each blob is a quarter of the budget, so the fifth evicts the first.
        let size = READ_CACHE_BUDGET / 4;
        let blobs: Vec<Vec<u8>> = (0..5u8).map(|n| vec![n; size]).collect();
        let table: Vec<(ContentHash, Vec<u8>)> = blobs
            .iter()
            .map(|b| (ContentHash::of(b), b.clone()))
            .collect();
        let lookup = table.clone();
        endpoint.with_reader(Box::new(move |asked| {
            lookup
                .iter()
                .find(|(hash, _)| hash == asked)
                .map(|(_, bytes)| bytes.clone())
        }));

        for (hash, _) in &table {
            endpoint.bytes_for(hash).expect("each blob reads through");
        }
        assert!(
            endpoint.fetched_bytes <= READ_CACHE_BUDGET,
            "the cache stayed inside its budget",
        );
        assert!(
            !endpoint.fetched.contains_key(&table[0].0),
            "the oldest was evicted to make room",
        );
    }

    fn endpoint_with_private_sentinel() -> (IdentityEndpoint<InMemoryStorage>, String) {
        let mut private =
            ssh_key::PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("endpoint-receipt");
        let private_openssh = private.to_openssh(LineEnding::LF).unwrap().to_string();
        let mut profile = Profile::new(
            ProfileId("research".to_string()),
            "Research",
            Ed25519Keypair::from_seed([0x6b; 32]),
        );
        profile.slots.insert(
            personae::ssh_slot::protocol_key_for(&private),
            personae::ssh_slot::slot_for(&private, personae::UnlockTier::PerUse).unwrap(),
        );
        let host = Arc::new(PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
        ));
        (IdentityEndpoint::new(host), private_openssh)
    }

    #[test]
    fn portable_client_mounts_only_public_identity_resources() {
        let (mut endpoint, private_openssh) = endpoint_with_private_sentinel();
        let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
        let resources = snapshot
            .presentation
            .offers
            .values()
            .flatten()
            .map(|offer| offer.resource)
            .collect::<Vec<_>>();
        let session = snapshot.session.clone();
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot).unwrap();
        for resource in resources {
            let response = endpoint
                .resource(ResourceRequest {
                    session: session.clone(),
                    resource,
                })
                .unwrap();
            let text = String::from_utf8(response.bytes.clone()).unwrap();
            assert!(!text.contains(&private_openssh));
            assert!(!text.contains("BEGIN OPENSSH PRIVATE KEY"));
            client.apply_resource(response).unwrap();
        }

        let mounted = client.mounted(&session).unwrap();
        for instance in mounted
            .scene
            .active_items_in_order()
            .into_iter()
            .map(|(id, _)| id)
        {
            assert!(matches!(
                client.resolve(
                    &session,
                    instance,
                    &chirograph::CapabilityProfile::new([
                        PresentationCapability::PortableCard,
                    ]),
                ),
                Ok(PresentationResolution::Ready(resolved))
                    if matches!(resolved.content, ResolvedContent::PortableCard(_))
            ));
        }
    }

    #[test]
    fn only_an_advertised_target_can_generate_a_key() {
        let (mut endpoint, _) = endpoint_with_private_sentinel();
        let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
        let vault = snapshot
            .presentation
            .bindings
            .iter()
            .find(|binding| {
                snapshot.presentation.offers.get(&binding.key).unwrap()[0]
                    .semantics
                    .actions
                    .iter()
                    .any(|action| action.intent.0 == SSH_GENERATE_INTENT)
            })
            .unwrap()
            .instance;
        let payload = serde_json::to_vec(&GenerateSshKeyIntentV1 {
            comment: "generated through endpoint".to_string(),
            unlock_policy: SshUnlockPolicyIntentV1::Session,
        })
        .unwrap();

        let rejected = endpoint
            .invoke(IntentInvocation {
                session: snapshot.session.clone(),
                target: InstanceId(vault.0 + 1),
                observed_epoch: snapshot.scene.epoch,
                observed_revision: snapshot.scene.revision,
                intent: SSH_GENERATE_INTENT.to_string(),
                payload: payload.clone(),
            })
            .unwrap();
        assert!(matches!(rejected, IntentResult::Rejected { .. }));

        let accepted = endpoint
            .invoke(IntentInvocation {
                session: snapshot.session,
                target: vault,
                observed_epoch: snapshot.scene.epoch,
                observed_revision: snapshot.scene.revision,
                intent: SSH_GENERATE_INTENT.to_string(),
                payload,
            })
            .unwrap();
        assert_eq!(accepted, IntentResult::Accepted);
        assert_eq!(endpoint.host().snapshot().unwrap().ssh_keys.len(), 2);
    }

    #[test]
    fn supplemental_public_cards_share_the_device_projection() {
        let (mut endpoint, private_openssh) = endpoint_with_private_sentinel();
        let identity_count = endpoint
            .snapshot(endpoint.request())
            .unwrap()
            .scene
            .active_item_count();
        endpoint.supplemental_cards = vec![SupplementalCard {
            adapter: "graphshell.personal-sync".into(),
            source_id: "graph-1".into(),
            card: PortableCardV1 {
                title: "Personal graph sync".into(),
                values: vec![chirograph::CardValueV1 {
                    label: "Nodes".into(),
                    value: "2".into(),
                }],
                badges: vec!["Durable".into()],
                media: Vec::new(),
            },
            actions: Vec::new(),
        }];

        let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
        assert_eq!(snapshot.scene.active_item_count(), identity_count + 1);
        let resource = snapshot
            .presentation
            .offers
            .values()
            .flatten()
            .find(|offer| offer.semantics.label == "Personal graph sync")
            .unwrap()
            .resource;
        let response = endpoint
            .resource(ResourceRequest {
                session: snapshot.session,
                resource,
            })
            .unwrap();
        let card: PortableCardV1 = serde_json::from_slice(&response.bytes).unwrap();
        assert_eq!(card.title, "Personal graph sync");
        assert!(
            !String::from_utf8(response.bytes)
                .unwrap()
                .contains(&private_openssh)
        );
    }

    /// The release set is the boundary. An admitted browser knowing a hash is
    /// not authorization to read the bytes behind it, so a blob the device
    /// holds but has not released must be indistinguishable from one it does
    /// not hold at all.
    #[test]
    fn only_released_blobs_are_servable_and_they_arrive_in_verified_chunks() {
        let (mut endpoint, _) = endpoint_with_private_sentinel();
        let session = endpoint.session();
        let payload: Vec<u8> = (0..200_000u32).map(|index| (index % 241) as u8).collect();
        let granted = ContentHash::of(&payload);
        let withheld = ContentHash::of(b"a blob this device holds but did not release");

        assert!(
            endpoint
                .resource_chunk(ResourceChunkRequest {
                    session: session.clone(),
                    resource: granted,
                    offset: 0,
                    length: 1024,
                })
                .is_err(),
            "nothing is servable before a transfer is accepted"
        );

        endpoint
            .release_transfer(vec![(granted, payload.clone())])
            .unwrap();
        assert_eq!(endpoint.released_count(), 1);

        let mut assembly = ResourceAssembly::new();
        let mut frames = 0;
        let pulled = loop {
            let chunk = endpoint
                .resource_chunk(ResourceChunkRequest {
                    session: session.clone(),
                    resource: granted,
                    offset: assembly.received(),
                    length: 64 * 1024,
                })
                .unwrap();
            frames += 1;
            if let Some(bytes) = assembly.accept(&chunk).unwrap() {
                break bytes;
            }
        };
        assert_eq!(pulled, payload);
        assert!(
            frames > 1,
            "the point is that it did not arrive in one frame"
        );

        assert!(
            endpoint
                .resource_chunk(ResourceChunkRequest {
                    session: session.clone(),
                    resource: withheld,
                    offset: 0,
                    length: 1024,
                })
                .is_err(),
            "an unreleased hash must not be servable"
        );

        // A different session must not reach another one's grant.
        assert!(
            endpoint
                .resource_chunk(ResourceChunkRequest {
                    session: ProjectionSession("native:someone-else".into()),
                    resource: granted,
                    offset: 0,
                    length: 1024,
                })
                .is_err()
        );

        endpoint.retire_released();
        assert_eq!(endpoint.released_count(), 0);
        assert!(
            endpoint
                .resource(ResourceRequest {
                    session,
                    resource: granted,
                })
                .is_err(),
            "retiring the grant must actually revoke it"
        );
    }

    /// Resident bytes are the cost of a synchronous endpoint, so the ceiling
    /// refuses rather than half-arriving. A partial release would look like a
    /// transfer that worked.
    #[test]
    fn a_transfer_over_the_ceiling_is_refused_whole() {
        let (mut endpoint, _) = endpoint_with_private_sentinel();
        let oversized = vec![0u8; MAX_RELEASED_TRANSFER_BYTES + 1];
        let refused = endpoint
            .release_transfer(vec![(ContentHash::of(&oversized), oversized)])
            .unwrap_err();
        assert!(matches!(
            refused,
            IdentityEndpointError::TransferTooLarge { .. }
        ));
        assert_eq!(
            endpoint.released_count(),
            0,
            "a refused release must leave nothing servable"
        );
    }

    /// Accepting is a decision, not the work it implies. The endpoint answers
    /// synchronously and records what the person agreed to; the resident host
    /// fetches later, where awaiting is possible.
    #[test]
    fn accepting_a_transfer_records_one_decision_and_never_reaches_the_vault() {
        let (mut endpoint, _) = endpoint_with_private_sentinel();
        let decisions: TransferDecisions = Default::default();
        endpoint.with_decisions(Arc::clone(&decisions));

        let transfer = "3f6b1e28-0000-4000-8000-00000000ffee";
        let payload = serde_json::to_vec(&TransferAcceptIntentV1 {
            schema: TRANSFER_ACCEPT_SCHEMA.to_string(),
            transfer_id: transfer.to_string(),
        })
        .unwrap();

        // Composed with the accept action, the way the sync host composes an
        // offer card. Without it the intent is not advertised and is refused.
        endpoint.supplemental_cards = vec![SupplementalCard {
            adapter: "mere.graph".into(),
            source_id: transfer.into(),
            card: PortableCardV1 {
                title: "Transfer from o-pc".into(),
                values: Vec::new(),
                badges: Vec::new(),
                media: Vec::new(),
            },
            actions: vec![IdentityProjectionAction {
                intent: TRANSFER_ACCEPT_INTENT,
                schema: TRANSFER_ACCEPT_SCHEMA,
                label: "Accept transfer",
                payload: None,
                native_only: true,
                input_form: None,
            }],
        }];
        let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
        let target = snapshot
            .presentation
            .bindings
            .iter()
            .find(|binding| {
                snapshot
                    .presentation
                    .offers_for(binding.instance)
                    .is_some_and(|offers| {
                        offers.iter().any(|offer| {
                            offer.semantics.actions.iter().any(|action| {
                                action.intent == IntentReference(TRANSFER_ACCEPT_INTENT.into())
                            })
                        })
                    })
            })
            .expect("the offer card advertises accept")
            .instance;

        let invoke =
            |endpoint: &mut IdentityEndpoint<InMemoryStorage>, instance, payload: &[u8]| {
                endpoint.invoke(IntentInvocation {
                    session: endpoint.session(),
                    target: instance,
                    intent: TRANSFER_ACCEPT_INTENT.to_string(),
                    payload: payload.to_vec(),
                    observed_epoch: SceneEpoch(endpoint.epoch),
                    observed_revision: Revision(endpoint.revision),
                })
            };

        assert!(matches!(
            invoke(&mut endpoint, target, &payload).unwrap(),
            IntentResult::Accepted
        ));
        // A double-click, or a browser retrying after a dropped reply, must
        // not queue the same transfer twice.
        assert!(matches!(
            invoke(&mut endpoint, target, &payload).unwrap(),
            IntentResult::Accepted
        ));
        let queued = decisions.lock().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].transfer_id, transfer);
        drop(queued);

        // A card that did not advertise accept cannot be used to accept.
        let unadvertised = snapshot
            .presentation
            .bindings
            .iter()
            .map(|binding| binding.instance)
            .find(|instance| *instance != target)
            .expect("the identity cards are still there");
        assert!(matches!(
            invoke(&mut endpoint, unadvertised, &payload).unwrap(),
            IntentResult::Rejected { .. }
        ));
        assert_eq!(decisions.lock().unwrap().len(), 1);
    }

    /// A surface composed without a decision queue must refuse rather than
    /// report an accept nothing will act on.
    #[test]
    fn accepting_without_a_place_to_record_it_is_refused() {
        let (endpoint, _) = endpoint_with_private_sentinel();
        let payload = serde_json::to_vec(&TransferAcceptIntentV1 {
            schema: TRANSFER_ACCEPT_SCHEMA.to_string(),
            transfer_id: "whatever".into(),
        })
        .unwrap();
        assert!(matches!(
            endpoint.accept_transfer(&payload),
            IntentResult::Rejected { .. }
        ));
    }
}
