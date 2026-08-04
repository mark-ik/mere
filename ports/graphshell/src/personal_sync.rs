//! H7 personal-device synchronization for Graphshell's local Mere graph.
//!
//! Graphshell keeps its own event grammar and deterministic fold. Stickleback
//! supplies the shared causal authoring helpers, policy-before-insert storage,
//! and LogSync join/drain used by Commons and Knot.

use std::collections::{BTreeMap, BTreeSet};

use eidetic::PrivacyClass;
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
use mere::kernel::graph::{EdgeAssertion, Graph, RelationSelector};
use muniment::Backend;
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use personae::{DerivedKeyAttestation, IdentityError, IdentityProvider};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stickleback::{
    Admission, CausalEntry, CausalError, CausalLimits, MunimentStore, OperationPolicy,
    OperationProcessor, PendingCausalOperation, ProcessError, Reject, StoreTarget, author_head,
    causal_projection, happens_before, observed_frontier, stable_writer_subject,
    validate_causal_metadata,
};
use uuid::Uuid;

use crate::access::{ACCESS_HISTORY_FACET, AccessHistory, AccessRecord};
use crate::product::{SAVED_SCENE_FACET, SavedSceneV1};

pub const PERSONAL_GRAPH_LOG: u64 = 0;
pub const PERSONAL_GRAPH_LIMITS: CausalLimits = CausalLimits {
    max_parents: 64,
    max_payload_bytes: 1024 * 1024,
};
pub const MAX_EVENTS_PER_OPERATION: usize = 256;

/// Signed addressing extension for one personal graph pool.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalGraphExt {
    pub graph: [u8; 32],
}

/// The bounded, secret-free events Graphshell can place on its generic lane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PersonalGraphEvent {
    AddNode {
        id: Uuid,
        address: String,
        title: String,
    },
    RemoveNode {
        id: Uuid,
    },
    SetTitle {
        node: Uuid,
        title: String,
    },
    AddTag {
        node: Uuid,
        tag: String,
    },
    RemoveTag {
        node: Uuid,
        tag: String,
    },
    AssertRelation {
        from: Uuid,
        to: Uuid,
        assertion: EdgeAssertion,
    },
    RetractRelation {
        from: Uuid,
        to: Uuid,
        selector: RelationSelector,
    },
    SetFacet {
        node: Uuid,
        facet: String,
        value: Value,
    },
    RemoveFacet {
        node: Uuid,
        facet: String,
    },
    AppendAccess {
        record: AccessRecord,
    },
    SaveScene {
        node: Uuid,
        scene: SavedSceneV1,
    },
    SetHandlerPreference {
        key: String,
        handler: String,
    },
    ObserveBlobAvailability {
        observation: BlobAvailabilityObservation,
    },
}

/// One immutable statement about which device currently holds addressed bytes.
///
/// Blob bytes never enter this record. Later observations supersede the
/// current device state without deleting its earlier chronology.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlobAvailabilityObservation {
    pub record_id: Uuid,
    pub container_id: Uuid,
    pub blob: [u8; 32],
    pub device: String,
    pub available: bool,
    pub at_ms: u64,
}

/// One signed authoring turn.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersonalGraphRecord {
    pub events: Vec<PersonalGraphEvent>,
    #[serde(default)]
    pub parents: Vec<[u8; 32]>,
    #[serde(default)]
    pub writer_attestation: Option<DerivedKeyAttestation>,
}

/// Stable persona roots admitted to one personal graph pool.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncRoster {
    subjects: BTreeSet<[u8; 32]>,
}

impl SyncRoster {
    pub fn new(subjects: impl IntoIterator<Item = [u8; 32]>) -> Self {
        Self {
            subjects: subjects.into_iter().collect(),
        }
    }

    pub fn admits(&self, subject: &[u8; 32]) -> bool {
        self.subjects.contains(subject)
    }
}

/// One class of synthetic node: a node that exists only to carry a facet.
///
/// `AddNode` is otherwise ungated, so a carrier materializes on every admitted
/// device even where its facet is filtered out, leaving a titled node with
/// nothing on it. A rule ties the carrier to the facet it exists for.
///
/// This is presentation, not confidentiality. The personal lane is plaintext,
/// so a device that declines to project a carrier still receives and stores
/// the operation. The roster is what bounds who can read a personal graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntheticAddressRule {
    /// Address prefix that marks the carrier, e.g. `mere://transfer/`.
    pub prefix: String,
    /// The facet whose selection the carrier follows.
    pub facet: String,
    /// When set, the carrier projects only where the address names the local
    /// device as one of its path segments. A device that has not been told its
    /// own key projects every carrier, so a missed setting over-shows rather
    /// than silently hiding the feature.
    pub device_scoped: bool,
}

/// Local, user-owned projection settings. Operations remain retained so a
/// later settings change can reproject without asking peers to resend them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncSelection {
    facets: BTreeSet<String>,
    synthetic: Vec<SyntheticAddressRule>,
    local_device: Option<String>,
    pub access_records: bool,
    pub saved_scenes: bool,
    pub handler_preferences: bool,
    pub blob_availability: bool,
}

impl SyncSelection {
    pub fn with_facets(mut self, facets: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.facets = facets.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_access_records(mut self, enabled: bool) -> Self {
        self.access_records = enabled;
        self
    }

    pub fn with_saved_scenes(mut self, enabled: bool) -> Self {
        self.saved_scenes = enabled;
        self
    }

    pub fn with_handler_preferences(mut self, enabled: bool) -> Self {
        self.handler_preferences = enabled;
        self
    }

    pub fn with_blob_availability(mut self, enabled: bool) -> Self {
        self.blob_availability = enabled;
        self
    }

    pub fn with_synthetic_addresses(
        mut self,
        rules: impl IntoIterator<Item = SyntheticAddressRule>,
    ) -> Self {
        self.synthetic = rules.into_iter().collect();
        self
    }

    /// Tell this replica its own device key, so device-scoped carriers
    /// addressed elsewhere stay out of the projection.
    pub fn with_local_device(mut self, device: impl Into<String>) -> Self {
        self.local_device = Some(device.into());
        self
    }

    fn projects(&self, event: &PersonalGraphEvent) -> bool {
        match event {
            PersonalGraphEvent::AddNode { address, .. } => self.projects_node(address),
            PersonalGraphEvent::SetFacet { facet, .. }
            | PersonalGraphEvent::RemoveFacet { facet, .. } => self.facets.contains(facet),
            PersonalGraphEvent::AppendAccess { .. } => self.access_records,
            PersonalGraphEvent::SaveScene { .. } => self.saved_scenes,
            PersonalGraphEvent::SetHandlerPreference { .. } => self.handler_preferences,
            PersonalGraphEvent::ObserveBlobAvailability { .. } => self.blob_availability,
            _ => true,
        }
    }

    /// Whether a node address materializes here. Plain addresses always do;
    /// a carrier follows its [`SyntheticAddressRule`].
    ///
    /// A filtered carrier takes its facet with it: the facet event replays as
    /// `ReplaySetNodeFacetById`, which the kernel drops when the node is
    /// absent, so one gate covers both events.
    fn projects_node(&self, address: &str) -> bool {
        let Some(rule) = self
            .synthetic
            .iter()
            .find(|rule| address.starts_with(&rule.prefix))
        else {
            return true;
        };
        if !self.facets.contains(&rule.facet) {
            return false;
        }
        let Some(local) = self.local_device.as_deref().filter(|_| rule.device_scoped) else {
            return true;
        };
        address[rule.prefix.len()..]
            .split('/')
            .any(|segment| segment.eq_ignore_ascii_case(local))
    }
}

/// A scalar or mutually exclusive target edited concurrently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncConflict {
    pub target: String,
    pub operations: Vec<[u8; 32]>,
}

/// Stable identity receipt retained beside the projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriterReceipt {
    pub operation: [u8; 32],
    pub signer: [u8; 32],
    pub stable_subject: [u8; 32],
}

/// Deterministic projection of one retained operation set.
pub struct SyncProjection {
    pub graph: Graph,
    pub access_records: Vec<AccessRecord>,
    pub scenes: BTreeMap<Uuid, SavedSceneV1>,
    pub handler_preferences: BTreeMap<String, String>,
    pub blob_availability: Vec<BlobAvailabilityObservation>,
    pub available_blobs: BTreeMap<[u8; 32], BTreeSet<String>>,
    pub pending: Vec<PendingCausalOperation>,
    pub conflicts: Vec<SyncConflict>,
    pub writers: Vec<WriterReceipt>,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PersonalGraphWireError {
    #[error("personal graph operation has no body")]
    MissingBody,
    #[error("personal graph operation body is malformed")]
    Malformed,
}

#[derive(Debug, thiserror::Error)]
pub enum PersonalGraphError {
    #[error(transparent)]
    Store(#[from] muniment::StoreError),
    #[error(transparent)]
    Wire(#[from] PersonalGraphWireError),
    #[error(transparent)]
    Causal(#[from] CausalError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("local authoring is blocked by {0} operations with missing causal history")]
    PendingHistory(usize),
    #[error("event is outside this device's selected sync profile: {0}")]
    NotSelected(String),
    #[error("event is categorically excluded from generic graph sync: {0}")]
    Excluded(String),
    #[error("writer is not in this personal graph roster")]
    WriterNotAdmitted,
}

#[derive(Debug, thiserror::Error)]
pub enum PersonalGraphIdentityError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("identity provider returned an invalid personal-graph writer attestation")]
    InvalidAttestation,
    #[error("identity provider attested a different personal-graph writer")]
    WriterMismatch,
}

#[derive(Clone)]
struct PersonalGraphPolicy {
    graph: [u8; 32],
    roster: SyncRoster,
}

impl OperationPolicy<PersonalGraphExt> for PersonalGraphPolicy {
    type LogId = u64;

    fn admit(
        &self,
        operation: &Operation<PersonalGraphExt>,
    ) -> Result<Admission<Self::LogId>, Reject> {
        if operation.header.extensions.graph != self.graph {
            return Err(Reject::new(
                "wrong-personal-graph",
                "operation addresses another personal graph",
            ));
        }
        let record = from_operation(operation)
            .map_err(|error| Reject::new("invalid-personal-graph-record", error.to_string()))?;
        validate_causal_metadata(operation, &record.parents, PERSONAL_GRAPH_LIMITS)
            .map_err(|error| Reject::new("invalid-personal-graph-causality", error.to_string()))?;
        if record.events.is_empty() || record.events.len() > MAX_EVENTS_PER_OPERATION {
            return Err(Reject::new(
                "personal-graph-event-count",
                format!(
                    "operation carries {} events; expected 1..={MAX_EVENTS_PER_OPERATION}",
                    record.events.len()
                ),
            ));
        }
        for event in &record.events {
            validate_event(event)?;
        }
        let subject = stable_subject(operation, &record)?;
        if !self.roster.admits(&subject) {
            return Err(Reject::new(
                "personal-graph-writer-not-admitted",
                "stable writer subject is outside this graph roster",
            ));
        }
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(self.graph),
            PERSONAL_GRAPH_LOG,
        )))
    }
}

fn validate_event(event: &PersonalGraphEvent) -> Result<(), Reject> {
    match event {
        PersonalGraphEvent::AddNode { address, .. } if address.trim().is_empty() => Err(
            Reject::new("empty-personal-graph-address", "node address is empty"),
        ),
        PersonalGraphEvent::AddTag { tag, .. } | PersonalGraphEvent::RemoveTag { tag, .. }
            if tag.trim().is_empty() =>
        {
            Err(Reject::new("empty-personal-graph-tag", "tag is empty"))
        }
        PersonalGraphEvent::SetFacet { facet, .. }
        | PersonalGraphEvent::RemoveFacet { facet, .. } => validate_facet_name(facet),
        PersonalGraphEvent::AppendAccess { record }
            if matches!(
                record.privacy,
                PrivacyClass::LocalOnly | PrivacyClass::MootScoped
            ) =>
        {
            Err(Reject::new(
                "access-record-not-portable",
                "access record was not marked for trusted-peer or public portability",
            ))
        }
        PersonalGraphEvent::SetHandlerPreference { key, handler }
            if key.trim().is_empty() || handler.trim().is_empty() =>
        {
            Err(Reject::new(
                "empty-handler-preference",
                "handler preference key and value must be non-empty",
            ))
        }
        PersonalGraphEvent::ObserveBlobAvailability { observation }
            if observation.device.trim().is_empty() =>
        {
            Err(Reject::new(
                "empty-blob-availability-device",
                "blob availability observation must name its source device",
            ))
        }
        _ => Ok(()),
    }
}

fn validate_facet_name(facet: &str) -> Result<(), Reject> {
    let normalized = facet.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.len() > 256 {
        return Err(Reject::new(
            "invalid-personal-graph-facet",
            "facet id is empty or too long",
        ));
    }
    if facet == ACCESS_HISTORY_FACET || facet == SAVED_SCENE_FACET {
        return Err(Reject::new(
            "reserved-personal-graph-facet",
            "facet has a dedicated append or scene event",
        ));
    }
    const SECRET_MARKERS: &[&str] = &[
        "credential",
        "decrypted",
        "private-epoch",
        "private_key",
        "private-key",
        "seed",
        "vault-root",
        "vault_root",
    ];
    if SECRET_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
    {
        return Err(Reject::new(
            "secret-bearing-facet",
            "credential and secret-bearing facets cannot use generic graph sync",
        ));
    }
    Ok(())
}

pub fn personal_graph_identity_salt(graph: [u8; 32]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(b"graphshell.personal-graph.writer.v1/");
    salt.extend_from_slice(&graph);
    salt
}

fn stable_subject(
    operation: &Operation<PersonalGraphExt>,
    record: &PersonalGraphRecord,
) -> Result<[u8; 32], Reject> {
    stable_writer_subject(
        *operation.header.verifying_key.as_bytes(),
        record.writer_attestation.as_ref(),
        &personal_graph_identity_salt(operation.header.extensions.graph),
    )
    .map_err(|error| Reject::new(error.code(), error.to_string()))
}

pub fn from_operation(
    operation: &Operation<PersonalGraphExt>,
) -> Result<PersonalGraphRecord, PersonalGraphWireError> {
    let body = operation
        .body
        .as_ref()
        .ok_or(PersonalGraphWireError::MissingBody)?;
    decode_cbor(body.to_bytes().as_slice()).map_err(|_| PersonalGraphWireError::Malformed)
}

fn to_operation(
    signing_seed: [u8; 32],
    graph: [u8; 32],
    record: &PersonalGraphRecord,
    seq_num: u32,
    backlink: Option<[u8; 32]>,
) -> Operation<PersonalGraphExt> {
    let signing_key = SigningKey::from_bytes(&signing_seed);
    let body_bytes = encode_cbor(record).expect("a personal graph record always CBOR-encodes");
    let body = Body::new(&body_bytes);
    let mut header = Header {
        version: 1,
        verifying_key: signing_key.verifying_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        seq_num,
        backlink: backlink.map(Hash::from),
        extensions: PersonalGraphExt { graph },
    };
    header.sign(&signing_key);
    let hash = header.hash();
    Operation {
        hash,
        header,
        body: Some(body),
    }
}

#[derive(Clone)]
struct StoredRecord {
    operation: Operation<PersonalGraphExt>,
    record: PersonalGraphRecord,
    log_id: u64,
}

async fn load_records<B: Backend + Clone + Send + Sync + 'static>(
    store: &MunimentStore<B, PersonalGraphExt>,
    graph: [u8; 32],
) -> Result<Vec<StoredRecord>, PersonalGraphError> {
    let by_author: BTreeMap<VerifyingKey, Vec<u64>> =
        TopicStore::<Topic, VerifyingKey, u64>::resolve(store, &Topic::from(graph)).await?;
    let mut records = Vec::new();
    for (author, mut logs) in by_author {
        logs.sort_unstable();
        logs.dedup();
        for log_id in logs {
            let entries = LogStore::<
                Operation<PersonalGraphExt>,
                VerifyingKey,
                u64,
                u32,
                Hash,
            >::get_log_entries(store, &author, &log_id, None, None)
            .await?
            .unwrap_or_default();
            for (operation, _) in entries {
                let record = from_operation(&operation)?;
                records.push(StoredRecord {
                    operation,
                    record,
                    log_id,
                });
            }
        }
    }
    Ok(records)
}

fn causal_entries(records: &[StoredRecord]) -> Vec<CausalEntry<u64>> {
    records
        .iter()
        .map(|record| {
            CausalEntry::from_operation(
                &record.operation,
                record.log_id,
                record.record.parents.clone(),
            )
        })
        .collect()
}

pub async fn accept_into<B: Backend + Clone + Send + Sync + 'static>(
    store: &MunimentStore<B, PersonalGraphExt>,
    graph: [u8; 32],
    roster: &SyncRoster,
    operation: &Operation<PersonalGraphExt>,
) -> Result<bool, ProcessError> {
    let processor = OperationProcessor::new(
        store.clone(),
        PersonalGraphPolicy {
            graph,
            roster: roster.clone(),
        },
    );
    Ok(processor.process(operation).await?.inserted())
}

/// One local device replica.
pub struct PersonalGraphReplica<B: Backend + Clone + Send + Sync + 'static> {
    store: MunimentStore<B, PersonalGraphExt>,
    graph: [u8; 32],
    signing_seed: [u8; 32],
    writer_attestation: Option<DerivedKeyAttestation>,
    roster: SyncRoster,
    selection: SyncSelection,
}

impl<B: Backend + Clone + Send + Sync + 'static> PersonalGraphReplica<B> {
    pub fn new(
        backend: B,
        graph: [u8; 32],
        signing_seed: [u8; 32],
        roster: SyncRoster,
        selection: SyncSelection,
    ) -> Self {
        Self {
            store: MunimentStore::new(backend),
            graph,
            signing_seed,
            writer_attestation: None,
            roster,
            selection,
        }
    }

    /// Replace the roster this replica projects through.
    ///
    /// The replica keeps its own copy because projection re-checks every
    /// stored operation against it. Admission and projection must therefore
    /// move together: a device admitted on intake but absent here has its
    /// operations accepted into the store and then refused on the way out,
    /// which fails the whole projection rather than the one operation.
    pub fn set_roster(&mut self, roster: SyncRoster) {
        self.roster = roster;
    }

    pub fn for_identity<P: IdentityProvider + ?Sized>(
        backend: B,
        graph: [u8; 32],
        identity: &P,
        roster: SyncRoster,
        selection: SyncSelection,
    ) -> Result<Self, PersonalGraphIdentityError> {
        let salt = personal_graph_identity_salt(graph);
        let keypair = identity.derive_keypair(&salt)?;
        let attestation = identity.attest_derived_key(&salt)?;
        if !attestation.verify(&salt) {
            return Err(PersonalGraphIdentityError::InvalidAttestation);
        }
        if attestation
            .derived_public_key()
            .map_err(|_| PersonalGraphIdentityError::InvalidAttestation)?
            .to_bytes()
            != keypair.public_key().to_bytes()
        {
            return Err(PersonalGraphIdentityError::WriterMismatch);
        }
        Ok(Self {
            store: MunimentStore::new(backend),
            graph,
            signing_seed: keypair.to_seed(),
            writer_attestation: Some(attestation),
            roster,
            selection,
        })
    }

    pub fn sync_store(&self) -> MunimentStore<B, PersonalGraphExt> {
        self.store.clone()
    }

    pub fn roster(&self) -> &SyncRoster {
        &self.roster
    }

    pub async fn author(
        &mut self,
        events: Vec<PersonalGraphEvent>,
    ) -> Result<Operation<PersonalGraphExt>, PersonalGraphError> {
        if events.is_empty() || events.len() > MAX_EVENTS_PER_OPERATION {
            return Err(PersonalGraphError::Excluded(format!(
                "expected 1..={MAX_EVENTS_PER_OPERATION} events"
            )));
        }
        for event in &events {
            validate_event(event)
                .map_err(|error| PersonalGraphError::Excluded(error.to_string()))?;
            if !self.selection.projects(event) {
                return Err(PersonalGraphError::NotSelected(event_target(event)));
            }
        }

        let records = load_records(&self.store, self.graph).await?;
        let entries = causal_entries(&records);
        let causal = causal_projection(&entries)?;
        if !causal.pending.is_empty() {
            return Err(PersonalGraphError::PendingHistory(causal.pending.len()));
        }
        let parents = observed_frontier(&entries)?;
        let signing_key = SigningKey::from_bytes(&self.signing_seed);
        let subject = if let Some(attestation) = self.writer_attestation.as_ref() {
            attestation
                .master_public_key()
                .map(|key| key.to_bytes())
                .map_err(|_| PersonalGraphError::WriterNotAdmitted)?
        } else {
            *signing_key.verifying_key().as_bytes()
        };
        if !self.roster.admits(&subject) {
            return Err(PersonalGraphError::WriterNotAdmitted);
        }
        let (seq_num, backlink) = author_head(
            &entries,
            *signing_key.verifying_key().as_bytes(),
            &PERSONAL_GRAPH_LOG,
        )?;
        let operation = to_operation(
            self.signing_seed,
            self.graph,
            &PersonalGraphRecord {
                events,
                parents,
                writer_attestation: self.writer_attestation.clone(),
            },
            seq_num,
            backlink,
        );
        self.accept(&operation).await?;
        Ok(operation)
    }

    pub async fn accept(
        &self,
        operation: &Operation<PersonalGraphExt>,
    ) -> Result<bool, ProcessError> {
        accept_into(&self.store, self.graph, &self.roster, operation).await
    }

    pub async fn projection(&self) -> Result<SyncProjection, PersonalGraphError> {
        materialize(&self.store, self.graph, &self.roster, &self.selection).await
    }
}

pub async fn materialize<B: Backend + Clone + Send + Sync + 'static>(
    store: &MunimentStore<B, PersonalGraphExt>,
    graph_id: [u8; 32],
    roster: &SyncRoster,
    selection: &SyncSelection,
) -> Result<SyncProjection, PersonalGraphError> {
    let records = load_records(store, graph_id).await?;
    let entries = causal_entries(&records);
    let causal = causal_projection(&entries)?;
    let policy = PersonalGraphPolicy {
        graph: graph_id,
        roster: roster.clone(),
    };
    let processor = OperationProcessor::new(store.clone(), policy);
    let conflicts = collect_conflicts(&records, &entries, &causal.order, selection);
    let mut graph = Graph::new();
    let mut access = BTreeMap::<Uuid, AccessRecord>::new();
    let mut scenes = BTreeMap::new();
    let mut handlers = BTreeMap::new();
    let mut blob_availability = BTreeMap::<Uuid, BlobAvailabilityObservation>::new();
    let mut writers = Vec::new();

    for &index in &causal.order {
        let stored = &records[index];
        processor.preflight(&stored.operation)?;
        let subject = stable_subject(&stored.operation, &stored.record)
            .map_err(|error| PersonalGraphError::Excluded(error.to_string()))?;
        writers.push(WriterReceipt {
            operation: *stored.operation.hash.as_bytes(),
            signer: *stored.operation.header.verifying_key.as_bytes(),
            stable_subject: subject,
        });
        for event in &stored.record.events {
            if selection.projects(event) {
                apply_event(
                    &mut graph,
                    &mut access,
                    &mut scenes,
                    &mut handlers,
                    &mut blob_availability,
                    event,
                );
            }
        }
    }

    let mut access_records: Vec<_> = access.into_values().collect();
    access_records.sort_by_key(|record| (record.at_ms, record.device.clone(), record.record_id));
    if selection.access_records {
        let mut by_node = BTreeMap::<Uuid, Vec<AccessRecord>>::new();
        for record in &access_records {
            by_node
                .entry(record.container_id)
                .or_default()
                .push(record.clone());
        }
        for (node, records) in by_node {
            apply_graph_delta(
                &mut graph,
                GraphDelta::ReplaySetNodeFacetById {
                    node_id: node,
                    facet: ACCESS_HISTORY_FACET.to_string(),
                    value: serde_json::to_value(AccessHistory { records })
                        .expect("access history always serializes"),
                },
            );
        }
    }

    let mut blob_availability = blob_availability.into_values().collect::<Vec<_>>();
    blob_availability.sort_by_key(|observation| {
        (
            observation.at_ms,
            observation.device.clone(),
            observation.record_id,
        )
    });
    let mut latest = BTreeMap::<([u8; 32], String), &BlobAvailabilityObservation>::new();
    for observation in &blob_availability {
        let key = (observation.blob, observation.device.clone());
        let replace = latest.get(&key).is_none_or(|current| {
            (observation.at_ms, observation.record_id) > (current.at_ms, current.record_id)
        });
        if replace {
            latest.insert(key, observation);
        }
    }
    let mut available_blobs = BTreeMap::<[u8; 32], BTreeSet<String>>::new();
    for ((blob, device), observation) in latest {
        if observation.available {
            available_blobs.entry(blob).or_default().insert(device);
        }
    }

    Ok(SyncProjection {
        graph,
        access_records,
        scenes,
        handler_preferences: handlers,
        blob_availability,
        available_blobs,
        pending: causal.pending,
        conflicts,
        writers,
    })
}

fn apply_event(
    graph: &mut Graph,
    access: &mut BTreeMap<Uuid, AccessRecord>,
    scenes: &mut BTreeMap<Uuid, SavedSceneV1>,
    handlers: &mut BTreeMap<String, String>,
    blob_availability: &mut BTreeMap<Uuid, BlobAvailabilityObservation>,
    event: &PersonalGraphEvent,
) {
    match event {
        PersonalGraphEvent::AddNode { id, address, title } => {
            if graph.get_node_key_by_id(*id).is_none() {
                let key = add_node(
                    graph,
                    Some(*id),
                    address.clone(),
                    PortablePoint::new(0.0, 0.0),
                );
                if !title.is_empty() {
                    apply_graph_delta(
                        graph,
                        GraphDelta::SetNodeTitle {
                            key,
                            title: title.clone(),
                        },
                    );
                }
            }
        }
        PersonalGraphEvent::RemoveNode { id } => {
            apply_graph_delta(graph, GraphDelta::ReplayRemoveNodeById { node_id: *id });
        }
        PersonalGraphEvent::SetTitle { node, title } => {
            apply_graph_delta(
                graph,
                GraphDelta::ReplaySetNodeTitleById {
                    node_id: *node,
                    title: title.clone(),
                },
            );
        }
        PersonalGraphEvent::AddTag { node, tag } => {
            apply_graph_delta(
                graph,
                GraphDelta::ReplayInsertNodeTagById {
                    node_id: *node,
                    tag: tag.clone(),
                },
            );
        }
        PersonalGraphEvent::RemoveTag { node, tag } => {
            apply_graph_delta(
                graph,
                GraphDelta::ReplayRemoveNodeTagById {
                    node_id: *node,
                    tag: tag.clone(),
                },
            );
        }
        PersonalGraphEvent::AssertRelation {
            from,
            to,
            assertion,
        } => {
            apply_graph_delta(
                graph,
                GraphDelta::ReplayAssertRelationByIds {
                    from_id: *from,
                    to_id: *to,
                    assertion: assertion.clone(),
                },
            );
        }
        PersonalGraphEvent::RetractRelation { from, to, selector } => {
            apply_graph_delta(
                graph,
                GraphDelta::ReplayRetractRelationsByIds {
                    from_id: *from,
                    to_id: *to,
                    selector: *selector,
                },
            );
        }
        PersonalGraphEvent::SetFacet { node, facet, value } => {
            apply_graph_delta(
                graph,
                GraphDelta::ReplaySetNodeFacetById {
                    node_id: *node,
                    facet: facet.clone(),
                    value: value.clone(),
                },
            );
        }
        PersonalGraphEvent::RemoveFacet { node, facet } => {
            apply_graph_delta(
                graph,
                GraphDelta::ReplayRemoveNodeFacetById {
                    node_id: *node,
                    facet: facet.clone(),
                },
            );
        }
        PersonalGraphEvent::AppendAccess { record } => {
            access
                .entry(record.record_id)
                .or_insert_with(|| record.clone());
        }
        PersonalGraphEvent::SaveScene { node, scene } => {
            scenes.insert(*node, scene.clone());
            apply_graph_delta(
                graph,
                GraphDelta::ReplaySetNodeFacetById {
                    node_id: *node,
                    facet: SAVED_SCENE_FACET.to_string(),
                    value: serde_json::to_value(scene).expect("saved scene always serializes"),
                },
            );
        }
        PersonalGraphEvent::SetHandlerPreference { key, handler } => {
            handlers.insert(key.clone(), handler.clone());
        }
        PersonalGraphEvent::ObserveBlobAvailability { observation } => {
            blob_availability
                .entry(observation.record_id)
                .or_insert_with(|| observation.clone());
        }
    }
}

fn collect_conflicts(
    records: &[StoredRecord],
    entries: &[CausalEntry<u64>],
    order: &[usize],
    selection: &SyncSelection,
) -> Vec<SyncConflict> {
    let effective: BTreeSet<_> = order.iter().copied().collect();
    let mut conflicts = BTreeMap::<String, BTreeSet<[u8; 32]>>::new();
    for left in 0..records.len() {
        if !effective.contains(&left) {
            continue;
        }
        for right in (left + 1)..records.len() {
            if !effective.contains(&right)
                || happens_before(entries, entries[left].operation, entries[right].operation)
                || happens_before(entries, entries[right].operation, entries[left].operation)
            {
                continue;
            }
            for left_event in &records[left].record.events {
                if !selection.projects(left_event) {
                    continue;
                }
                for right_event in &records[right].record.events {
                    if !selection.projects(right_event)
                        || left_event == right_event
                        || event_target(left_event) != event_target(right_event)
                    {
                        continue;
                    }
                    let target = event_target(left_event);
                    conflicts.entry(target).or_default().extend([
                        *records[left].operation.hash.as_bytes(),
                        *records[right].operation.hash.as_bytes(),
                    ]);
                }
            }
        }
    }
    conflicts
        .into_iter()
        .map(|(target, operations)| SyncConflict {
            target,
            operations: operations.into_iter().collect(),
        })
        .collect()
}

fn event_target(event: &PersonalGraphEvent) -> String {
    match event {
        PersonalGraphEvent::AddNode { id, .. } | PersonalGraphEvent::RemoveNode { id } => {
            format!("node/{id}")
        }
        PersonalGraphEvent::SetTitle { node, .. } => format!("node/{node}/title"),
        PersonalGraphEvent::AddTag { node, tag } | PersonalGraphEvent::RemoveTag { node, tag } => {
            format!("node/{node}/tag/{tag}")
        }
        PersonalGraphEvent::AssertRelation {
            from,
            to,
            assertion,
        } => format!("relation/{from}/{to}/{}", relation_key(assertion)),
        PersonalGraphEvent::RetractRelation { from, to, selector } => {
            format!("relation/{from}/{to}/{selector:?}")
        }
        PersonalGraphEvent::SetFacet { node, facet, .. }
        | PersonalGraphEvent::RemoveFacet { node, facet } => {
            format!("node/{node}/facet/{facet}")
        }
        PersonalGraphEvent::AppendAccess { record } => {
            format!("access/{}", record.record_id)
        }
        PersonalGraphEvent::SaveScene { node, .. } => format!("scene/{node}"),
        PersonalGraphEvent::SetHandlerPreference { key, .. } => format!("handler/{key}"),
        PersonalGraphEvent::ObserveBlobAvailability { observation } => {
            format!("blob-availability/{}", observation.record_id)
        }
    }
}

fn relation_key(assertion: &EdgeAssertion) -> String {
    serde_json::to_string(assertion).expect("edge assertion always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{AccessAction, AccessTransition};
    use mere::canvas::CartographyGeometry;
    use mere::kernel::graph::{EdgeFamily, RelationKind, SemanticSubKind};
    use muniment::{MemoryBackend, RedbBackend};
    use personae::{IdentityProvider, InMemoryProvider};
    use std::sync::Arc;
    use std::time::Duration;
    use stickleback::JoinedSpace;
    use transport::{P2pandaTransport, PeerID, sync_overlay_topic};

    const GRAPH: [u8; 32] = [0x77; 32];
    const A: Uuid = Uuid::from_u128(0xa1);
    const B: Uuid = Uuid::from_u128(0xb2);

    fn relation() -> EdgeAssertion {
        EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::Cites,
            label: None,
            decay_progress: None,
        }
    }

    fn selection() -> SyncSelection {
        SyncSelection::default()
            .with_facets(["graphshell.test/v1"])
            .with_access_records(true)
            .with_saved_scenes(true)
            .with_handler_preferences(true)
            .with_blob_availability(true)
    }

    fn access(device: &str, id: u128, at_ms: u64) -> AccessRecord {
        AccessRecord {
            record_id: Uuid::from_u128(id),
            container_id: A,
            address: "https://a.test/".into(),
            action: AccessAction::Examine,
            persona: "personae://profile/shared".into(),
            device: device.into(),
            application: "graphshell".into(),
            at_ms,
            handler: "graphshell.inspect".into(),
            dwell_ms: None,
            referring_container_id: None,
            referring_address: None,
            transition: AccessTransition::Unknown,
            capture_source: "graphshell.h7-test".into(),
            source_event_id: None,
            privacy: PrivacyClass::TrustedPeersOnly,
        }
    }

    fn fingerprint(projection: &SyncProjection) -> (Vec<(Uuid, Vec<String>)>, usize, Vec<Uuid>) {
        let mut nodes = projection
            .graph
            .nodes()
            .map(|(_, node)| {
                let mut tags = node.tags.iter().cloned().collect::<Vec<_>>();
                tags.sort();
                (node.id, tags)
            })
            .collect::<Vec<_>>();
        nodes.sort_by_key(|(id, _)| *id);
        let mut accesses = projection
            .access_records
            .iter()
            .map(|record| record.record_id)
            .collect::<Vec<_>>();
        accesses.sort();
        (nodes, projection.graph.edge_count(), accesses)
    }

    #[tokio::test]
    async fn partitioned_graph_events_converge_and_retain_each_device_chronology() {
        let alice_identity = Arc::new(InMemoryProvider::from_seed([0xa1; 32]));
        let bob_identity = Arc::new(InMemoryProvider::from_seed([0xb2; 32]));
        let roster = SyncRoster::new([
            alice_identity.master_public_key().to_bytes(),
            bob_identity.master_public_key().to_bytes(),
        ]);
        let mut alice = PersonalGraphReplica::for_identity(
            MemoryBackend::new(),
            GRAPH,
            alice_identity.as_ref(),
            roster.clone(),
            selection(),
        )
        .unwrap();
        let mut bob = PersonalGraphReplica::for_identity(
            MemoryBackend::new(),
            GRAPH,
            bob_identity.as_ref(),
            roster.clone(),
            selection(),
        )
        .unwrap();

        let seed = alice
            .author(vec![
                PersonalGraphEvent::AddNode {
                    id: A,
                    address: "https://a.test/".into(),
                    title: "A".into(),
                },
                PersonalGraphEvent::AddNode {
                    id: B,
                    address: "https://b.test/".into(),
                    title: "B".into(),
                },
            ])
            .await
            .unwrap();
        bob.accept(&seed).await.unwrap();

        alice
            .author(vec![
                PersonalGraphEvent::AddTag {
                    node: A,
                    tag: "alice".into(),
                },
                PersonalGraphEvent::AssertRelation {
                    from: A,
                    to: B,
                    assertion: relation(),
                },
                PersonalGraphEvent::AppendAccess {
                    record: access("alice-laptop", 1, 100),
                },
            ])
            .await
            .unwrap();
        bob.author(vec![
            PersonalGraphEvent::AddTag {
                node: A,
                tag: "bob".into(),
            },
            PersonalGraphEvent::AppendAccess {
                record: access("bob-phone", 2, 90),
            },
            PersonalGraphEvent::SetFacet {
                node: A,
                facet: "graphshell.test/v1".into(),
                value: serde_json::json!({"device": "bob"}),
            },
            PersonalGraphEvent::SaveScene {
                node: B,
                scene: SavedSceneV1 {
                    name: "Shared scene".into(),
                    selected: vec![A, B],
                    layout_strategy: Some("grid.default".into()),
                    physics_paused: true,
                    physics_damping: 0.7,
                    arrangement_pull: 0.4,
                    camera_offset: (12.0, 24.0),
                    camera_zoom: 1.2,
                    default_handler: "graphshell.inspect".into(),
                    cartography: CartographyGeometry::default(),
                },
            },
            PersonalGraphEvent::SetHandlerPreference {
                key: "https".into(),
                handler: "turnstone".into(),
            },
            PersonalGraphEvent::ObserveBlobAvailability {
                observation: BlobAvailabilityObservation {
                    record_id: Uuid::from_u128(3),
                    container_id: A,
                    blob: [0x42; 32],
                    device: "bob-phone".into(),
                    available: true,
                    at_ms: 95,
                },
            },
        ])
        .await
        .unwrap();

        let alice_transport = P2pandaTransport::builder(alice_identity.master_keypair())
            .gossip()
            .bind()
            .await
            .unwrap();
        let bob_transport = P2pandaTransport::builder(bob_identity.master_keypair())
            .gossip()
            .bind()
            .await
            .unwrap();
        alice_transport
            .add_peer(bob_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        alice_transport
            .set_topics(
                PeerID::from_public_key(bob_identity.master_public_key()),
                &[sync_overlay_topic(GRAPH)],
            )
            .await
            .unwrap();
        bob_transport
            .add_peer(alice_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob_transport
            .set_topics(
                PeerID::from_public_key(alice_identity.master_public_key()),
                &[sync_overlay_topic(GRAPH)],
            )
            .await
            .unwrap();

        let alice_store = alice.sync_store();
        let alice_accept = alice_store.clone();
        let alice_roster = roster.clone();
        let (alice_endpoint, alice_gossip) = alice_transport.sync_parts().unwrap();
        let alice_space = JoinedSpace::join::<_, u64, _, _>(
            stickleback::lane_id("graphshell/personal-graph/v1", GRAPH),
            alice_store,
            alice_endpoint,
            alice_gossip,
            GRAPH,
            move |operation| {
                let store = alice_accept.clone();
                let roster = alice_roster.clone();
                async move {
                    accept_into(&store, GRAPH, &roster, &operation)
                        .await
                        .unwrap_or(false)
                }
            },
        )
        .await
        .unwrap();

        let bob_store = bob.sync_store();
        let bob_accept = bob_store.clone();
        let bob_roster = roster.clone();
        let (bob_endpoint, bob_gossip) = bob_transport.sync_parts().unwrap();
        let bob_space = JoinedSpace::join::<_, u64, _, _>(
            stickleback::lane_id("graphshell/personal-graph/v1", GRAPH),
            bob_store,
            bob_endpoint,
            bob_gossip,
            GRAPH,
            move |operation| {
                let store = bob_accept.clone();
                let roster = bob_roster.clone();
                async move {
                    accept_into(&store, GRAPH, &roster, &operation)
                        .await
                        .unwrap_or(false)
                }
            },
        )
        .await
        .unwrap();

        let (alice_projection, bob_projection) =
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let a = alice.projection().await.unwrap();
                    let b = bob.projection().await.unwrap();
                    if a.access_records.len() == 2
                        && b.access_records.len() == 2
                        && a.graph.edge_count() == 1
                        && b.graph.edge_count() == 1
                    {
                        break (a, b);
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .expect("personal graph replicas did not converge");

        assert_eq!(fingerprint(&alice_projection), fingerprint(&bob_projection));
        let node = alice_projection.graph.get_node_by_id(A).unwrap().1;
        assert!(node.tags.contains("alice"));
        assert!(node.tags.contains("bob"));
        assert_eq!(
            alice_projection
                .graph
                .relations()
                .filter(|relation| matches!(relation.kind, RelationKind::Semantic(_)))
                .count(),
            1
        );
        assert_eq!(
            alice_projection
                .access_records
                .iter()
                .map(|record| (record.device.as_str(), record.at_ms))
                .collect::<Vec<_>>(),
            [("bob-phone", 90), ("alice-laptop", 100)]
        );
        assert_eq!(
            alice_projection
                .graph
                .facets()
                .get(&A, &chartulary::FacetId::new("graphshell.test/v1"),),
            Some(&serde_json::json!({"device": "bob"}))
        );
        assert_eq!(
            alice_projection
                .scenes
                .get(&B)
                .map(|scene| scene.name.as_str()),
            Some("Shared scene")
        );
        assert_eq!(
            alice_projection
                .handler_preferences
                .get("https")
                .map(String::as_str),
            Some("turnstone")
        );
        assert_eq!(alice_projection.blob_availability.len(), 1);
        assert_eq!(
            alice_projection.available_blobs.get(&[0x42; 32]),
            Some(&BTreeSet::from(["bob-phone".to_string()]))
        );
        assert!(
            alice_projection
                .writers
                .iter()
                .all(|receipt| roster.admits(&receipt.stable_subject))
        );
        assert!(alice_projection.pending.is_empty());
        assert!(alice_projection.conflicts.is_empty());
        assert!(alice_space.sync_status().sync_rounds > 0);
        assert!(bob_space.sync_status().sync_rounds > 0);
    }

    #[tokio::test]
    async fn concurrent_scalar_edits_are_exposed_and_secret_facets_are_refused() {
        let alice_seed = [0x31; 32];
        let bob_seed = [0x32; 32];
        let alice_subject = *SigningKey::from_bytes(&alice_seed)
            .verifying_key()
            .as_bytes();
        let bob_subject = *SigningKey::from_bytes(&bob_seed).verifying_key().as_bytes();
        let roster = SyncRoster::new([alice_subject, bob_subject]);
        let mut alice = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            alice_seed,
            roster.clone(),
            selection(),
        );
        let mut bob =
            PersonalGraphReplica::new(MemoryBackend::new(), GRAPH, bob_seed, roster, selection());
        let seed = alice
            .author(vec![PersonalGraphEvent::AddNode {
                id: A,
                address: "https://a.test/".into(),
                title: "A".into(),
            }])
            .await
            .unwrap();
        bob.accept(&seed).await.unwrap();
        let left = alice
            .author(vec![PersonalGraphEvent::SetTitle {
                node: A,
                title: "Alice".into(),
            }])
            .await
            .unwrap();
        let right = bob
            .author(vec![PersonalGraphEvent::SetTitle {
                node: A,
                title: "Bob".into(),
            }])
            .await
            .unwrap();
        alice.accept(&right).await.unwrap();
        bob.accept(&left).await.unwrap();

        let projection = alice.projection().await.unwrap();
        assert_eq!(projection.conflicts.len(), 1);
        assert_eq!(projection.conflicts[0].target, format!("node/{A}/title"));

        let refused = alice
            .author(vec![PersonalGraphEvent::SetFacet {
                node: A,
                facet: "personae.vault-root/v1".into(),
                value: serde_json::json!("never"),
            }])
            .await
            .unwrap_err();
        assert!(matches!(refused, PersonalGraphError::Excluded(_)));

        let local_only = alice
            .author(vec![PersonalGraphEvent::AppendAccess {
                record: AccessRecord {
                    privacy: PrivacyClass::LocalOnly,
                    ..access("alice", 3, 110)
                },
            }])
            .await
            .unwrap_err();
        assert!(matches!(local_only, PersonalGraphError::Excluded(_)));

        assert_eq!(
            projection
                .graph
                .relations()
                .filter(|relation| relation.kind.family() == EdgeFamily::Semantic)
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn redb_reopen_restores_projection_and_resumes_the_author_head() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("graphshell-personal-sync.redb");
        let seed = [0x51; 32];
        let subject = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let roster = SyncRoster::new([subject]);

        let first = {
            let backend = RedbBackend::open(&path).unwrap();
            let mut replica =
                PersonalGraphReplica::new(backend, GRAPH, seed, roster.clone(), selection());
            replica
                .author(vec![
                    PersonalGraphEvent::AddNode {
                        id: A,
                        address: "https://a.test/".into(),
                        title: "A".into(),
                    },
                    PersonalGraphEvent::AddTag {
                        node: A,
                        tag: "before-restart".into(),
                    },
                    PersonalGraphEvent::AppendAccess {
                        record: access("durable-device", 4, 120),
                    },
                ])
                .await
                .unwrap()
        };

        let backend = RedbBackend::open(&path).unwrap();
        let mut reopened =
            PersonalGraphReplica::new(backend, GRAPH, seed, roster.clone(), selection());
        let restored = reopened.projection().await.unwrap();
        assert!(
            restored
                .graph
                .get_node_by_id(A)
                .unwrap()
                .1
                .tags
                .contains("before-restart")
        );
        assert_eq!(restored.access_records.len(), 1);

        let second = reopened
            .author(vec![PersonalGraphEvent::AddTag {
                node: A,
                tag: "after-restart".into(),
            }])
            .await
            .unwrap();
        assert_eq!(second.header.seq_num, first.header.seq_num + 1);
        assert_eq!(
            second.header.backlink.as_ref().map(|hash| *hash.as_bytes()),
            Some(*first.hash.as_bytes())
        );
        let final_projection = reopened.projection().await.unwrap();
        let node = final_projection.graph.get_node_by_id(A).unwrap().1;
        assert!(node.tags.contains("before-restart"));
        assert!(node.tags.contains("after-restart"));
        assert!(final_projection.pending.is_empty());
    }
}
