//! H7 personal-device synchronization for Graphshell's local Mere graph.
//!
//! Graphshell keeps its own event grammar and deterministic fold. Stickleback
//! supplies the shared causal authoring helpers, policy-before-insert storage,
//! and LogSync join/drain used by Commons and Knot.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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
    Admission, CausalEntry, CausalError, CausalLimits, DataKeyring, GroupCiphertext,
    GroupPrekeyBundle, GroupSessionDispatch, MunimentStore, OperationPolicy, OperationProcessor,
    PendingCausalOperation, ProcessError, Reject, StoreTarget, author_head, causal_projection,
    happens_before, observed_frontier, stable_writer_subject, validate_causal_metadata,
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

/// How one operation's body is protected.
///
/// Carried in the signed header rather than guessed from the body, so a device
/// knows what it is holding before it tries to read it. Knot's `KnotSyncExt`
/// names its profile the same way and for the same reason.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersonalEncryption {
    /// The body is the CBOR record itself. Every operation written before
    /// encryption existed.
    #[default]
    Plaintext,
    /// The body is a `GroupCiphertext` sealed with this graph's data keyring.
    GroupV1,
}

impl PersonalEncryption {
    /// Whether this operation's body can be read without a key.
    ///
    /// Also the `skip_serializing_if` predicate below, which is load bearing:
    /// see [`PersonalGraphExt::encryption`].
    pub fn is_plaintext(&self) -> bool {
        matches!(self, Self::Plaintext)
    }
}

/// Signed addressing extension for one personal graph pool.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalGraphExt {
    pub graph: [u8; 32],
    /// **`skip_serializing_if` is not cosmetic here.** `Header::to_bytes`
    /// re-encodes the whole header, extensions included, and both `hash()` and
    /// signature verification digest that encoding. A field that serialized on
    /// every operation would change the bytes of headers already signed and
    /// stored, so every existing operation would fail to verify. Omitting the
    /// plaintext case keeps those headers byte-identical to what they already
    /// are, which is what makes reading old operations possible at all.
    #[serde(default, skip_serializing_if = "PersonalEncryption::is_plaintext")]
    pub encryption: PersonalEncryption,
    /// The frontier this operation observed, repeated where a device without
    /// the key can still read it.
    ///
    /// Knot carries its parents in the header for the same reason: causal
    /// admission cannot depend on a body it may not be able to open. The
    /// record keeps its own copy for operations written before this existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<[u8; 32]>,
    /// Who wrote this, checkable without the payload key.
    ///
    /// This is what makes a sealed operation admissible at all. The roster
    /// admits stable persona roots, and deriving one needs the attestation;
    /// while it lived only in the sealed body, a device without the key could
    /// not tell an admitted writer from a stranger, so it refused everything
    /// sealed and never saw those operations again.
    ///
    /// Not a secret: it attests a public derived key, and the operation is
    /// signed by that key in the clear regardless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_attestation: Option<DerivedKeyAttestation>,
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
    /// scope=persona; movement=persona-synced opt-in; mutability=live;
    /// security=ordinary. The handler id is a preference, not a credential or
    /// a secret-bearing payload.
    SetHandlerPreference {
        key: String,
        handler: String,
    },
    ObserveBlobAvailability {
        observation: BlobAvailabilityObservation,
    },
    /// This device's group pre-key bundle, so a keyed member can add it.
    ///
    /// Public by construction: it is what another device needs in order to
    /// key this one, and it authenticates back to a Personae root.
    PublishPrekey {
        /// `stickleback::GroupPrekeyBundle::to_bytes`.
        bundle: Vec<u8>,
    },
    /// Name a persona root that should be able to read this graph.
    ///
    /// Intent, not the key itself. The key group reconciles toward this, which
    /// is what lets a revocation stand: every device folds the same set from
    /// the same operations, so none of them re-seats a device another one
    /// retired.
    ///
    /// Authority is checked when this is folded rather than when it is
    /// admitted. Only a device already in the reader set may name another, and
    /// the first is named by whoever creates the group. Checking it at
    /// admission would be circular, since the set being decided is the one the
    /// check would need.
    AdmitReader {
        root: [u8; 32],
        /// What a person calls this device. Never load bearing.
        label: String,
    },
    /// Withdraw a persona root's ability to read this graph.
    ///
    /// The departed device keeps what it could already read; no scheme takes
    /// that back. What this ends is everything written afterwards.
    RetireReader {
        root: [u8; 32],
    },
    /// One membership or rotation step in the graph's key group.
    ///
    /// The broadcast control frame plus the per-recipient direct frames, which
    /// the group scheme has already sealed to their recipients. Carrying them
    /// in the clear here discloses nothing and is what lets a device that has
    /// no key yet still receive one.
    GroupDispatch {
        /// `stickleback::GroupSessionDispatch::to_bytes`.
        dispatch: Vec<u8>,
    },
}

impl PersonalGraphEvent {
    /// Whether this event carries key agreement rather than graph content.
    ///
    /// Key traffic must stay readable by a device that has no key, since
    /// reading it is how that device gets one. Sealing it would be a lock
    /// whose only key is inside the box.
    pub fn is_key_agreement(&self) -> bool {
        matches!(
            self,
            Self::PublishPrekey { .. }
                | Self::GroupDispatch { .. }
                | Self::AdmitReader { .. }
                | Self::RetireReader { .. }
        )
    }
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
    /// Opt-in persona preference lane. Disabled by default, so a device can
    /// retain the operation without projecting the preference locally.
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

/// One key-agreement step read off the lane, in causal order.
///
/// `author_root` is the stable writer subject the lane already authenticates,
/// which is exactly what `GroupSession::process` requires. Carrying it here
/// means the caller never has to re-derive who authored a control frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyAgreementStep {
    pub operation: [u8; 32],
    pub author_root: [u8; 32],
    pub step: KeyAgreementEvent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAgreementEvent {
    /// A device published a pre-key bundle so it can be added.
    Prekey(Vec<u8>),
    /// A membership or rotation step.
    Dispatch(Vec<u8>),
}

/// Deterministic projection of one retained operation set.
pub struct SyncProjection {
    pub graph: Graph,
    pub access_records: Vec<AccessRecord>,
    pub scenes: BTreeMap<Uuid, SavedSceneV1>,
    pub handler_preferences: BTreeMap<String, String>,
    pub blob_availability: Vec<BlobAvailabilityObservation>,
    pub available_blobs: BTreeMap<[u8; 32], BTreeSet<String>>,
    /// Persona roots the graph says may read it, with their labels.
    ///
    /// The condition the key group reconciles toward. Every device folds this
    /// identically from the same operations, so a device retired on one is
    /// retired everywhere without anyone being told twice.
    pub readers: BTreeMap<[u8; 32], String>,
    /// Operations this device holds and cannot read yet, because they are
    /// sealed and it has no key.
    ///
    /// Reported rather than hidden: a graph that is quietly missing part of
    /// itself looks identical to one that is complete, and the difference is
    /// exactly what an owner needs to see while a device waits to be keyed.
    pub unreadable: usize,
    /// Key-agreement steps in causal order, for a caller to hand to its group
    /// session. Present whether or not this device can read the sealed
    /// operations around them, since reading these is how it gets the key.
    pub key_agreement: Vec<KeyAgreementStep>,
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
    /// The operation is sealed and this device holds no key for this graph.
    ///
    /// Loud rather than silent. A device that cannot read an operation must
    /// not store it as though it had, because the graph would then differ
    /// between devices with nothing saying why.
    #[error("personal graph operation is sealed and this device has no key for the graph")]
    NoKey,
    #[error("personal graph operation could not be unsealed: {0}")]
    Unsealable(String),
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
    #[error("personal graph operation could not be sealed: {0}")]
    Seal(String),
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
    /// Admission reads the record to check its events, so a sealed operation
    /// cannot be admitted without the key. Absent, a sealed operation is
    /// refused with a reason rather than stored unvalidated.
    keyring: Option<Arc<DataKeyring>>,
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
        // Everything checkable without the body, first. A sealed operation
        // this device cannot open is still attributable and still bounded,
        // and refusing it would lose it: LogSync does not offer twice, so a
        // device keyed later would carry a permanent hole.
        let subject = header_subject(operation)?;
        let record = match from_operation(operation, self.keyring.as_deref()) {
            Ok(record) => record,
            Err(PersonalGraphWireError::NoKey) => {
                let Some(subject) = subject else {
                    return Err(Reject::new(
                        "unattributable-sealed-operation",
                        "a sealed operation carries no writer attestation in its header, so this                          device cannot tell whether its writer is admitted",
                    ));
                };
                if !self.roster.admits(&subject) {
                    return Err(Reject::new(
                        "personal-graph-writer-not-admitted",
                        "stable writer subject is outside this graph roster",
                    ));
                }
                validate_causal_metadata(
                    operation,
                    &operation.header.extensions.parents,
                    PERSONAL_GRAPH_LIMITS,
                )
                .map_err(|error| {
                    Reject::new("invalid-personal-graph-causality", error.to_string())
                })?;
                // Its events go unchecked, which is the price of holding it.
                // They were authored by an admitted writer and the payload is
                // size-bounded above; the alternative is losing the operation
                // entirely. Projection re-admits it once the key arrives, so
                // nothing unread is ever folded into the graph.
                return Ok(Admission::keep(StoreTarget::new(
                    Topic::from(self.graph),
                    PERSONAL_GRAPH_LOG,
                )));
            }
            Err(error) => {
                return Err(Reject::new(
                    "invalid-personal-graph-record",
                    error.to_string(),
                ));
            }
        };
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
        one_kind_per_operation(&record.events)?;
        // Key agreement must be readable without a key, so it may not be
        // sealed. Checked at intake rather than trusted, because a sealed
        // dispatch would be undiagnosable from the outside: it would look
        // exactly like an operation this device simply has no key for.
        if operation.header.extensions.encryption != PersonalEncryption::Plaintext
            && record
                .events
                .iter()
                .any(PersonalGraphEvent::is_key_agreement)
        {
            return Err(Reject::new(
                "sealed-key-agreement",
                "key agreement must travel in the clear; sealing it would need the key it carries",
            ));
        }
        let subject = match subject {
            Some(subject) => subject,
            None => stable_subject(operation, &record)?,
        };
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

/// The stable writer subject, from the header alone.
///
/// `None` for operations written before the attestation moved into the header,
/// which are readable anyway and fall back to the record.
fn header_subject(operation: &Operation<PersonalGraphExt>) -> Result<Option<[u8; 32]>, Reject> {
    let Some(attestation) = operation.header.extensions.writer_attestation.as_ref() else {
        return Ok(None);
    };
    stable_writer_subject(
        *operation.header.verifying_key.as_bytes(),
        Some(attestation),
        &personal_graph_identity_salt(operation.header.extensions.graph),
    )
    .map(Some)
    .map_err(|error| Reject::new(error.code(), error.to_string()))
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
        // Decoding a bundle verifies its identity signature back to a Personae
        // root, so a forged pre-key is refused at intake rather than at the
        // moment somebody tries to key it.
        PersonalGraphEvent::PublishPrekey { bundle } => GroupPrekeyBundle::from_bytes(bundle)
            .map(|_| ())
            .map_err(|error| Reject::new("invalid-group-prekey", error.to_string())),
        PersonalGraphEvent::GroupDispatch { dispatch } => {
            decode_cbor::<GroupSessionDispatch, _>(dispatch.as_slice())
                .map(|_| ())
                .map_err(|error| Reject::new("invalid-group-dispatch", error.to_string()))
        }
        _ => Ok(()),
    }
}

/// Refuse an operation that mixes key agreement with graph content.
///
/// Not tidiness. Key agreement is authored in the clear, so a content event
/// sharing the operation would be published in the clear with it. Separating
/// them at the grammar means that cannot happen by accident.
fn one_kind_per_operation(events: &[PersonalGraphEvent]) -> Result<(), Reject> {
    let key = events
        .iter()
        .filter(|event| event.is_key_agreement())
        .count();
    if key == 0 || key == events.len() {
        return Ok(());
    }
    Err(Reject::new(
        "mixed-key-and-content-operation",
        "an operation carries either key agreement or graph content, never both",
    ))
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

/// Read one operation's record, unsealing it when the header says it is sealed.
///
/// The header names the protection, so this never guesses from the bytes. A
/// device without the key gets [`PersonalGraphWireError::NoKey`] rather than a
/// parse failure, because "I cannot read this" and "this is corrupt" call for
/// different responses from whoever sees the error.
pub fn from_operation(
    operation: &Operation<PersonalGraphExt>,
    keyring: Option<&DataKeyring>,
) -> Result<PersonalGraphRecord, PersonalGraphWireError> {
    let body = operation
        .body
        .as_ref()
        .ok_or(PersonalGraphWireError::MissingBody)?;
    let bytes = body.to_bytes();
    match operation.header.extensions.encryption {
        PersonalEncryption::Plaintext => {
            decode_cbor(bytes.as_slice()).map_err(|_| PersonalGraphWireError::Malformed)
        }
        PersonalEncryption::GroupV1 => {
            let keyring = keyring.ok_or(PersonalGraphWireError::NoKey)?;
            let envelope: GroupCiphertext =
                decode_cbor(bytes.as_slice()).map_err(|_| PersonalGraphWireError::Malformed)?;
            let plaintext = keyring
                .open(&envelope)
                .map_err(|error| PersonalGraphWireError::Unsealable(error.to_string()))?;
            decode_cbor(plaintext.as_slice()).map_err(|_| PersonalGraphWireError::Malformed)
        }
    }
}

/// Write one operation, sealing it when a keyring is present.
///
/// Sealing is opt-in by the caller holding a key, not a default: a device that
/// sealed while a sibling had no key would still replicate, and the sibling
/// would simply stop being able to read the graph. Until keys are distributed,
/// no keyring means no sealing means nothing breaks.
fn to_operation(
    signing_seed: [u8; 32],
    graph: [u8; 32],
    record: &PersonalGraphRecord,
    seq_num: u32,
    backlink: Option<[u8; 32]>,
    keyring: Option<&DataKeyring>,
) -> Result<Operation<PersonalGraphExt>, PersonalGraphError> {
    let signing_key = SigningKey::from_bytes(&signing_seed);
    let record_bytes = encode_cbor(record).expect("a personal graph record always CBOR-encodes");
    // Key agreement stays in the clear even here. A device with no key reads
    // this operation to get one, so sealing it would be a lock whose only key
    // is inside the box. Admission refuses a sealed one for the same reason.
    let carries_keys = record
        .events
        .iter()
        .any(PersonalGraphEvent::is_key_agreement);
    let (body_bytes, encryption) = match keyring.filter(|_| !carries_keys) {
        Some(keyring) => {
            let envelope = keyring
                .seal_random(&record_bytes)
                .map_err(|error| PersonalGraphError::Seal(error.to_string()))?;
            let sealed = encode_cbor(&envelope)
                .map_err(|error| PersonalGraphError::Seal(error.to_string()))?;
            (sealed, PersonalEncryption::GroupV1)
        }
        None => (record_bytes, PersonalEncryption::Plaintext),
    };
    let body = Body::from_bytes(&body_bytes);
    // p2panda 0.7.1 made the header's CBOR cache, size and digest private
    // and folded signing into the builder: `build` encodes, signs and
    // caches the digest in one step, so the struct-literal + `sign` pair
    // has no equivalent. `body` sets payload_size and payload_hash.
    let header = Header::builder()
        .body(&body_bytes)
        .seq_num(seq_num)
        .backlink(backlink.map(Hash::from))
        .build(&signing_key, PersonalGraphExt {
                graph,
                encryption,
                parents: record.parents.clone(),
                writer_attestation: record.writer_attestation.clone(),
            });
    let hash = header.hash();
    Ok(Operation {
        hash,
        header,
        body: Some(body),
    })
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
    keyring: Option<&Arc<DataKeyring>>,
) -> Result<(Vec<StoredRecord>, usize), PersonalGraphError> {
    let by_author: BTreeMap<VerifyingKey, Vec<u64>> =
        TopicStore::<Topic, VerifyingKey, u64>::resolve(store, &Topic::from(graph)).await?;
    let mut records = Vec::new();
    let mut unreadable = 0usize;
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
                match from_operation(&operation, keyring.map(Arc::as_ref)) {
                    Ok(record) => records.push(StoredRecord {
                        operation,
                        record,
                        log_id,
                    }),
                    // Held but not readable yet. Skipped rather than fatal:
                    // one operation this device has no key for must not stop
                    // it projecting the rest, and it becomes readable the
                    // moment the key arrives, with nothing to re-fetch.
                    Err(PersonalGraphWireError::NoKey) => unreadable += 1,
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    Ok((records, unreadable))
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
    keyring: Option<&Arc<DataKeyring>>,
    operation: &Operation<PersonalGraphExt>,
) -> Result<bool, ProcessError> {
    let processor = OperationProcessor::new(
        store.clone(),
        PersonalGraphPolicy {
            graph,
            roster: roster.clone(),
            keyring: keyring.map(Arc::clone),
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
    /// Present only once this device has been given the graph's key. Absent,
    /// this replica writes plaintext exactly as before, which is what keeps a
    /// sibling that has no key still able to read what this one writes.
    keyring: Option<Arc<DataKeyring>>,
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
            keyring: None,
        }
    }

    /// Hand this replica the graph's data keyring.
    ///
    /// Sealing starts from the next operation it authors. Reading is unrelated
    /// to this call: an operation already stored plaintext stays readable, and
    /// the header on each operation says which it is.
    pub fn set_keyring(&mut self, keyring: Arc<DataKeyring>) {
        self.keyring = Some(keyring);
    }

    /// Whether this replica seals what it writes.
    pub fn seals(&self) -> bool {
        self.keyring.is_some()
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
            keyring: None,
        })
    }

    pub fn sync_store(&self) -> MunimentStore<B, PersonalGraphExt> {
        self.store.clone()
    }

    pub fn roster(&self) -> &SyncRoster {
        &self.roster
    }

    /// Whether this device authors and projects blob-availability records.
    ///
    /// Anything that stages bytes for a sibling depends on this: without it,
    /// the bytes are held but never advertised, so no peer can learn to ask
    /// for them. Callers check it before staging rather than discovering it as
    /// a refused event afterwards.
    pub fn serves_blob_availability(&self) -> bool {
        self.selection.blob_availability
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

        let (records, _) = load_records(&self.store, self.graph, self.keyring.as_ref()).await?;
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
            self.keyring.as_deref(),
        )?;
        self.accept(&operation).await?;
        Ok(operation)
    }

    pub async fn accept(
        &self,
        operation: &Operation<PersonalGraphExt>,
    ) -> Result<bool, ProcessError> {
        accept_into(
            &self.store,
            self.graph,
            &self.roster,
            self.keyring.as_ref(),
            operation,
        )
        .await
    }

    pub async fn projection(&self) -> Result<SyncProjection, PersonalGraphError> {
        materialize(
            &self.store,
            self.graph,
            &self.roster,
            self.keyring.as_ref(),
            &self.selection,
        )
        .await
    }
}

/// Read the key agreement without holding a key.
///
/// The pass a device runs before it can read anything else: it has been
/// paired, it has published a pre-key, and it needs the dispatch that keys it.
/// Sealed operations are skipped rather than refused, because being unable to
/// read them is the entire reason this runs.
///
/// Skipping is safe here and nowhere else. Key agreement is plaintext by
/// construction and admission refuses a sealed one, so a sealed operation
/// cannot be a step this pass needs.
pub async fn key_agreement<B: Backend + Clone + Send + Sync + 'static>(
    store: &MunimentStore<B, PersonalGraphExt>,
    graph: [u8; 32],
) -> Result<Vec<KeyAgreementStep>, PersonalGraphError> {
    let by_author: BTreeMap<VerifyingKey, Vec<u64>> =
        TopicStore::<Topic, VerifyingKey, u64>::resolve(store, &Topic::from(graph)).await?;
    let mut steps = Vec::new();
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
                if operation.header.extensions.encryption != PersonalEncryption::Plaintext {
                    continue;
                }
                let Ok(record) = from_operation(&operation, None) else {
                    continue;
                };
                let Ok(subject) = stable_subject(&operation, &record) else {
                    continue;
                };
                for event in &record.events {
                    let step = match event {
                        PersonalGraphEvent::PublishPrekey { bundle } => {
                            KeyAgreementEvent::Prekey(bundle.clone())
                        }
                        PersonalGraphEvent::GroupDispatch { dispatch } => {
                            KeyAgreementEvent::Dispatch(dispatch.clone())
                        }
                        _ => continue,
                    };
                    steps.push((
                        operation.header.seq_num,
                        KeyAgreementStep {
                            operation: *operation.hash.as_bytes(),
                            author_root: subject,
                            step,
                        },
                    ));
                }
            }
        }
    }
    // Causal order needs every operation, which this pass deliberately does not
    // have. Sequence per author with the operation hash as a tiebreak is a
    // deterministic order every device agrees on, which is what the group
    // scheme needs; it orders its own steps internally.
    steps.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.operation.cmp(&right.1.operation))
    });
    Ok(steps.into_iter().map(|(_, step)| step).collect())
}

pub async fn materialize<B: Backend + Clone + Send + Sync + 'static>(
    store: &MunimentStore<B, PersonalGraphExt>,
    graph_id: [u8; 32],
    roster: &SyncRoster,
    keyring: Option<&Arc<DataKeyring>>,
    selection: &SyncSelection,
) -> Result<SyncProjection, PersonalGraphError> {
    let (records, unreadable) = load_records(store, graph_id, keyring).await?;
    let entries = causal_entries(&records);
    let causal = causal_projection(&entries)?;
    let policy = PersonalGraphPolicy {
        graph: graph_id,
        roster: roster.clone(),
        keyring: keyring.map(Arc::clone),
    };
    let processor = OperationProcessor::new(store.clone(), policy);
    let conflicts = collect_conflicts(&records, &entries, &causal.order, selection);
    let mut graph = Graph::new();
    let mut access = BTreeMap::<Uuid, AccessRecord>::new();
    let mut scenes = BTreeMap::new();
    let mut handlers = BTreeMap::new();
    let mut blob_availability = BTreeMap::<Uuid, BlobAvailabilityObservation>::new();
    let mut writers = Vec::new();
    let mut key_agreement = Vec::new();

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
            // Never gated by selection. A device cannot opt out of the traffic
            // that keys it and still expect to read the graph.
            match event {
                PersonalGraphEvent::PublishPrekey { bundle } => {
                    key_agreement.push(KeyAgreementStep {
                        operation: *stored.operation.hash.as_bytes(),
                        author_root: subject,
                        step: KeyAgreementEvent::Prekey(bundle.clone()),
                    });
                    continue;
                }
                PersonalGraphEvent::GroupDispatch { dispatch } => {
                    key_agreement.push(KeyAgreementStep {
                        operation: *stored.operation.hash.as_bytes(),
                        author_root: subject,
                        step: KeyAgreementEvent::Dispatch(dispatch.clone()),
                    });
                    continue;
                }
                _ => {}
            }
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
        readers: fold_readers(&records, &causal.order),
        unreadable,
        key_agreement,
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
        // Key agreement is not graph content and folds into nothing. It is
        // collected separately, because acting on it changes this device's
        // keys rather than the graph everyone shares.
        PersonalGraphEvent::PublishPrekey { .. }
        | PersonalGraphEvent::GroupDispatch { .. }
        | PersonalGraphEvent::AdmitReader { .. }
        | PersonalGraphEvent::RetireReader { .. } => {}
    }
}

/// Fold the reader set from the operations that named it.
///
/// Deterministic and total: every device that holds the same operations
/// computes the same set, which is the whole point. A revocation is not an
/// action one device performs on another; it is a fact all of them read.
///
/// The first admit bootstraps. Before anyone is a reader there is nobody who
/// could authorize one, so the earliest admit in causal order stands on its
/// author's own authority and seats that author too. After that, only a
/// current reader may admit or retire, and an event from anyone else is
/// ignored rather than refused: it was validly authored on the lane and is
/// simply not authoritative about this.
fn fold_readers(records: &[StoredRecord], order: &[usize]) -> BTreeMap<[u8; 32], String> {
    let mut readers: BTreeMap<[u8; 32], String> = BTreeMap::new();
    for &index in order {
        let stored = &records[index];
        let Ok(author) = stable_subject(&stored.operation, &stored.record) else {
            continue;
        };
        for event in &stored.record.events {
            match event {
                PersonalGraphEvent::AdmitReader { root, label } => {
                    if readers.is_empty() {
                        readers.insert(author, String::new());
                    }
                    if readers.contains_key(&author) {
                        readers.insert(*root, label.clone());
                    }
                }
                PersonalGraphEvent::RetireReader { root } => {
                    // A reader may retire itself; that is leaving, not a
                    // coup. What it may not do is retire the set out of
                    // existence, because nobody could then admit anyone.
                    if readers.contains_key(&author) && readers.len() > 1 {
                        readers.remove(root);
                    }
                }
                _ => {}
            }
        }
    }
    readers
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
        // Concurrent key steps are not a conflict to report: the group scheme
        // orders them itself, and two devices publishing at once is ordinary.
        PersonalGraphEvent::PublishPrekey { bundle } => {
            format!("group-prekey/{}", Hash::digest(bundle))
        }
        PersonalGraphEvent::GroupDispatch { dispatch } => {
            format!("group-dispatch/{}", Hash::digest(dispatch))
        }
        // Concurrent edits to one root's readership are a genuine conflict
        // worth surfacing: two devices disagreeing about who may read is not
        // something to resolve quietly.
        PersonalGraphEvent::AdmitReader { root, .. }
        | PersonalGraphEvent::RetireReader { root } => {
            format!("reader/{}", hex32(root))
        }
    }
}

fn hex32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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
                    accept_into(&store, GRAPH, &roster, None, &operation)
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
                    accept_into(&store, GRAPH, &roster, None, &operation)
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
    async fn persona_handler_preference_is_opt_in_and_secret_free() {
        let alice_seed = [0x41; 32];
        let bob_seed = [0x42; 32];
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
            SyncSelection::default().with_handler_preferences(true),
        );
        let bob = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            bob_seed,
            roster.clone(),
            SyncSelection::default().with_handler_preferences(true),
        );
        let bystander = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            [0x43; 32],
            roster,
            SyncSelection::default(),
        );

        let operation = alice
            .author(vec![PersonalGraphEvent::SetHandlerPreference {
                key: "https".into(),
                handler: "turnstone".into(),
            }])
            .await
            .unwrap();
        bob.accept(&operation).await.unwrap();
        bystander.accept(&operation).await.unwrap();

        assert_eq!(
            bob.projection()
                .await
                .unwrap()
                .handler_preferences
                .get("https")
                .map(String::as_str),
            Some("turnstone")
        );
        assert!(
            bystander
                .projection()
                .await
                .unwrap()
                .handler_preferences
                .is_empty(),
            "the ordinary preference remains opt-in at projection time"
        );

        let secret = alice
            .author(vec![PersonalGraphEvent::SetFacet {
                node: A,
                facet: "personae.vault-root/v1".into(),
                value: serde_json::json!("never"),
            }])
            .await
            .unwrap_err();
        assert!(matches!(secret, PersonalGraphError::Excluded(_)));
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

    fn keyring() -> Arc<DataKeyring> {
        let mut keyring = DataKeyring::new();
        keyring.rotate_random().expect("a fresh secret");
        Arc::new(keyring)
    }

    /// The property the whole migration rests on.
    ///
    /// `Header::to_bytes` re-encodes extensions, and both `hash()` and
    /// signature verification digest that encoding. If adding `encryption`
    /// changed how a plaintext extension encodes, every operation already
    /// signed and stored would stop verifying. `skip_serializing_if` is what
    /// prevents that, and this is the assertion that keeps it.
    #[test]
    fn the_header_fields_added_since_do_not_change_an_old_header() {
        #[derive(Serialize)]
        struct ExtensionAsItWasBeforeEncryption {
            graph: [u8; 32],
        }

        let before = encode_cbor(&ExtensionAsItWasBeforeEncryption { graph: GRAPH }).unwrap();
        let after = encode_cbor(&PersonalGraphExt {
            graph: GRAPH,
            encryption: PersonalEncryption::Plaintext,
            parents: Vec::new(),
            writer_attestation: None,
        })
        .unwrap();
        assert_eq!(
            before, after,
            "a plaintext extension must encode exactly as it did before the field existed"
        );

        // And the field is really carried when it is not plaintext, so the
        // omission above is not simply the field never being written.
        let sealed = encode_cbor(&PersonalGraphExt {
            graph: GRAPH,
            encryption: PersonalEncryption::GroupV1,
            parents: Vec::new(),
            writer_attestation: None,
        })
        .unwrap();
        assert_ne!(before, sealed);

        // Same for the two fields that moved out of the sealed body. Each is
        // absent on an operation written before it existed, and each changes
        // the bytes when present, so neither can quietly rewrite a signed
        // header nor quietly fail to travel.
        let with_parents = encode_cbor(&PersonalGraphExt {
            graph: GRAPH,
            encryption: PersonalEncryption::Plaintext,
            parents: vec![[0x01; 32]],
            writer_attestation: None,
        })
        .unwrap();
        assert_ne!(before, with_parents);
    }

    #[tokio::test]
    async fn a_sealed_graph_reads_back_and_still_reads_what_it_wrote_in_plaintext() {
        let seed = [0x71; 32];
        let subject = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let roster = SyncRoster::new([subject]);
        let backend = MemoryBackend::new();
        let mut replica =
            PersonalGraphReplica::new(backend.clone(), GRAPH, seed, roster.clone(), selection());

        // Written before this device had a key.
        assert!(!replica.seals());
        let plain = replica
            .author(vec![PersonalGraphEvent::AddNode {
                id: A,
                address: "https://plain.test/".into(),
                title: "Written in the open".into(),
            }])
            .await
            .unwrap();
        assert_eq!(
            plain.header.extensions.encryption,
            PersonalEncryption::Plaintext
        );

        // The key arrives. Sealing starts from the next operation; nothing is
        // rewritten, which is the whole point of reading both.
        let keys = keyring();
        replica.set_keyring(Arc::clone(&keys));
        assert!(replica.seals());
        let sealed = replica
            .author(vec![PersonalGraphEvent::AddNode {
                id: B,
                address: "https://sealed.test/".into(),
                title: "Written under a key".into(),
            }])
            .await
            .unwrap();
        assert_eq!(
            sealed.header.extensions.encryption,
            PersonalEncryption::GroupV1
        );

        // The sealed body must not carry the plaintext it protects.
        let body = sealed.body.as_ref().unwrap().to_bytes();
        assert!(
            !String::from_utf8_lossy(&body).contains("sealed.test"),
            "a sealed operation must not leave its address in the body"
        );

        let projection = replica.projection().await.unwrap();
        assert!(
            projection.graph.get_node_by_id(A).is_some(),
            "the plaintext operation is still readable"
        );
        assert!(
            projection.graph.get_node_by_id(B).is_some(),
            "the sealed one is readable with the key"
        );
        assert!(projection.pending.is_empty());
    }

    /// A device without the key must say so, not quietly diverge. Two devices
    /// silently holding different graphs is the failure mode worth refusing.
    #[tokio::test]
    async fn a_sealed_operation_is_refused_by_a_device_with_no_key() {
        let seed = [0x72; 32];
        let subject = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let roster = SyncRoster::new([subject]);
        let mut sealing = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            seed,
            roster.clone(),
            selection(),
        );
        sealing.set_keyring(keyring());
        let sealed = sealing
            .author(vec![PersonalGraphEvent::AddNode {
                id: A,
                address: "https://sealed.test/".into(),
                title: "Sealed".into(),
            }])
            .await
            .unwrap();

        let keyless =
            PersonalGraphReplica::new(MemoryBackend::new(), GRAPH, seed, roster, selection());
        let refused = keyless.accept(&sealed).await;
        assert!(
            refused.is_err() || !refused.unwrap(),
            "a device with no key must not report having accepted a sealed operation"
        );
        assert!(
            keyless
                .projection()
                .await
                .unwrap()
                .graph
                .get_node_by_id(A)
                .is_none(),
            "and it must not have materialized anything from it"
        );
    }

    /// A different keyring is not a wrong password prompt: it is an operation
    /// this device cannot read, and it must be reported as such.
    #[tokio::test]
    async fn a_foreign_keyring_cannot_open_another_graphs_operations() {
        let seed = [0x73; 32];
        let subject = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let roster = SyncRoster::new([subject]);
        let mut sealing =
            PersonalGraphReplica::new(MemoryBackend::new(), GRAPH, seed, roster, selection());
        sealing.set_keyring(keyring());
        let sealed = sealing
            .author(vec![PersonalGraphEvent::AddNode {
                id: A,
                address: "https://sealed.test/".into(),
                title: "Sealed".into(),
            }])
            .await
            .unwrap();

        let stranger = keyring();
        let read = from_operation(&sealed, Some(stranger.as_ref()));
        assert!(
            matches!(read, Err(PersonalGraphWireError::Unsealable(_))),
            "expected an unsealable error, got {read:?}"
        );
    }

    /// The property the whole distribution design rests on: a device with no
    /// key must still be able to read the traffic that keys it.
    #[tokio::test]
    async fn key_agreement_stays_readable_when_everything_else_is_sealed() {
        let seed = [0x74; 32];
        let subject = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let roster = SyncRoster::new([subject]);
        let backend = MemoryBackend::new();
        let mut replica =
            PersonalGraphReplica::new(backend.clone(), GRAPH, seed, roster, selection());
        replica.set_keyring(keyring());

        // Content authored under a key, and a pre-key published beside it.
        replica
            .author(vec![PersonalGraphEvent::AddNode {
                id: A,
                address: "https://sealed.test/".into(),
                title: "Sealed".into(),
            }])
            .await
            .unwrap();
        let (_, bundle) = stickleback::GroupSession::new(
            stickleback::GroupSessionId(GRAPH),
            &personae::InMemoryProvider::from_seed([0x75; 32]),
        )
        .unwrap();
        let published = replica
            .author(vec![PersonalGraphEvent::PublishPrekey {
                bundle: bundle.to_bytes().unwrap(),
            }])
            .await
            .unwrap();

        assert_eq!(
            published.header.extensions.encryption,
            PersonalEncryption::Plaintext,
            "key agreement must not be sealed even by a device that holds a key"
        );

        // The reader a new device is: no key at all.
        let steps = key_agreement(&replica.sync_store(), GRAPH).await.unwrap();
        assert_eq!(
            steps.len(),
            1,
            "the sealed content is skipped, the pre-key is not"
        );
        assert!(matches!(steps[0].step, KeyAgreementEvent::Prekey(_)));
        assert_eq!(
            steps[0].author_root, subject,
            "the lane authenticates the author"
        );
    }

    /// An operation may carry key agreement or graph content, never both.
    /// Mixing would publish content in the clear alongside the key traffic.
    #[tokio::test]
    async fn an_operation_carrying_keys_may_not_also_carry_content() {
        let seed = [0x76; 32];
        let subject = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let mut replica = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            seed,
            SyncRoster::new([subject]),
            selection(),
        );
        let (_, bundle) = stickleback::GroupSession::new(
            stickleback::GroupSessionId(GRAPH),
            &personae::InMemoryProvider::from_seed([0x77; 32]),
        )
        .unwrap();
        let mixed = replica
            .author(vec![
                PersonalGraphEvent::PublishPrekey {
                    bundle: bundle.to_bytes().unwrap(),
                },
                PersonalGraphEvent::AddNode {
                    id: A,
                    address: "https://would-be-public.test/".into(),
                    title: "Content beside a key".into(),
                },
            ])
            .await;
        assert!(
            mixed.is_err(),
            "mixing must be refused, not merely discouraged"
        );
    }

    /// A forged pre-key is refused at intake, not when somebody tries to use it.
    #[tokio::test]
    async fn a_prekey_that_does_not_authenticate_is_refused() {
        let seed = [0x78; 32];
        let subject = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let mut replica = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            seed,
            SyncRoster::new([subject]),
            selection(),
        );
        let refused = replica
            .author(vec![PersonalGraphEvent::PublishPrekey {
                bundle: b"not a bundle".to_vec(),
            }])
            .await;
        assert!(refused.is_err());
    }

    /// The property that makes revocation fall out rather than be performed:
    /// every device folds the same set, so none of them re-seats what another
    /// retired.
    #[tokio::test]
    async fn the_reader_set_is_a_fold_every_device_computes_the_same_way() {
        let owner_seed = [0x81; 32];
        let sibling_seed = [0x82; 32];
        let owner_subject = *SigningKey::from_bytes(&owner_seed)
            .verifying_key()
            .as_bytes();
        let sibling_subject = *SigningKey::from_bytes(&sibling_seed)
            .verifying_key()
            .as_bytes();
        let roster = SyncRoster::new([owner_subject, sibling_subject]);
        let mut owner = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            owner_seed,
            roster.clone(),
            selection(),
        );
        let mut sibling = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            sibling_seed,
            roster,
            selection(),
        );

        // The first admit bootstraps: nobody could authorize it, so it stands
        // on its author's own authority and seats the author too.
        let opened = owner
            .author(vec![PersonalGraphEvent::AdmitReader {
                root: sibling_subject,
                label: "sibling".into(),
            }])
            .await
            .unwrap();
        sibling.accept(&opened).await.unwrap();

        let both = owner.projection().await.unwrap().readers;
        assert_eq!(
            both.len(),
            2,
            "the author seats itself alongside the one it named"
        );
        assert!(both.contains_key(&owner_subject));
        assert!(both.contains_key(&sibling_subject));
        assert_eq!(
            sibling.projection().await.unwrap().readers,
            both,
            "both devices fold the same set from the same operations"
        );

        // Retiring on ONE device is enough, because the other reads the same
        // fact rather than being told. This is the case the command-shaped
        // version could not do.
        let retired = sibling
            .author(vec![PersonalGraphEvent::RetireReader {
                root: owner_subject,
            }])
            .await
            .unwrap();
        owner.accept(&retired).await.unwrap();

        let after = sibling.projection().await.unwrap().readers;
        assert_eq!(
            after.keys().copied().collect::<Vec<_>>(),
            vec![sibling_subject]
        );
        assert_eq!(
            owner.projection().await.unwrap().readers,
            after,
            "the device that was retired agrees it was retired"
        );
    }

    /// Authority is checked in the fold. An admit from a device that is not a
    /// reader is ignored rather than refused: it was validly authored and is
    /// simply not authoritative about who reads.
    #[tokio::test]
    async fn only_a_reader_can_name_another() {
        let owner_seed = [0x83; 32];
        let stranger_seed = [0x84; 32];
        let owner_subject = *SigningKey::from_bytes(&owner_seed)
            .verifying_key()
            .as_bytes();
        let stranger_subject = *SigningKey::from_bytes(&stranger_seed)
            .verifying_key()
            .as_bytes();
        let roster = SyncRoster::new([owner_subject, stranger_subject]);
        let mut owner = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            owner_seed,
            roster.clone(),
            selection(),
        );
        let mut stranger = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            stranger_seed,
            roster,
            selection(),
        );

        let opened = owner
            .author(vec![PersonalGraphEvent::AdmitReader {
                root: owner_subject,
                label: "owner".into(),
            }])
            .await
            .unwrap();
        stranger.accept(&opened).await.unwrap();

        // On the roster, so this operation is admitted and stored. Not a
        // reader, so it does not get to decide who reads.
        let overreach = stranger
            .author(vec![PersonalGraphEvent::AdmitReader {
                root: stranger_subject,
                label: "helping myself".into(),
            }])
            .await
            .unwrap();
        owner.accept(&overreach).await.unwrap();

        let readers = owner.projection().await.unwrap().readers;
        assert_eq!(
            readers.keys().copied().collect::<Vec<_>>(),
            vec![owner_subject],
            "a writer that is not a reader cannot admit itself"
        );
    }

    /// The set must never empty out, or nobody could ever admit anyone again.
    #[tokio::test]
    async fn the_last_reader_cannot_retire_itself() {
        let seed = [0x85; 32];
        let subject = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let mut replica = PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            seed,
            SyncRoster::new([subject]),
            selection(),
        );
        replica
            .author(vec![PersonalGraphEvent::AdmitReader {
                root: subject,
                label: "only".into(),
            }])
            .await
            .unwrap();
        replica
            .author(vec![PersonalGraphEvent::RetireReader { root: subject }])
            .await
            .unwrap();
        assert_eq!(
            replica.projection().await.unwrap().readers.len(),
            1,
            "retiring the last reader would strand the graph with no one able to admit"
        );
    }
}
