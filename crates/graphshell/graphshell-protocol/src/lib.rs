//! Carrier-neutral Graphshell wire vocabulary.
//!
//! A message carries Scenograph's product-free score and scene types. Transport,
//! authorization, application models, and rendered content stay outside `sceno`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sceno::{InstanceId, Score};
use serde::{Deserialize, Serialize};

pub use scenotime::{Revision, SceneDiff, SceneEpoch, SceneSnapshot};

/// The first compatible Graphshell wire version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };
}

/// An endpoint-scoped projection session. It is opaque to Graphshell clients.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProjectionSession(pub String);

/// A requested score plus the client's observed protocol version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectionRequest {
    pub version: ProtocolVersion,
    pub session: ProjectionSession,
    pub score: Score,
}

/// One endpoint-advertised projection that a generic Graphshell host may open.
///
/// The request carries only the product-free score vocabulary needed to select
/// the projection. Source data remains endpoint-side until `snapshot`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectionOffer {
    pub label: String,
    pub request: ProjectionRequest,
}

/// The projections available from one endpoint process.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EndpointDescriptor {
    pub label: String,
    pub projections: Vec<ProjectionOffer>,
}

/// Client presentation features negotiated independently of the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PresentationCapability {
    NativeGlyph,
    PortableCard,
    Image,
}

/// One named capability set used during offer selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProfile {
    pub capabilities: BTreeSet<PresentationCapability>,
}

impl CapabilityProfile {
    pub fn new(capabilities: impl IntoIterator<Item = PresentationCapability>) -> Self {
        Self {
            capabilities: capabilities.into_iter().collect(),
        }
    }

    pub fn supports(&self, capability: PresentationCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

/// A snapshot-local handle to one set of ordered presentation offers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PresentationKey(pub String);

/// A content address for a separately transferred resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A stable session-scoped action reference advertised by an endpoint.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IntentReference(pub String);

/// Whether invoking an advertised action changes local curation, domain truth,
/// or asks the endpoint to perform an external effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentEffect {
    Curation,
    DomainTruth,
    ExternalEffect,
}

/// An action carried into accessibility and permission surfaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvertisedAction {
    pub intent: IntentReference,
    pub label: String,
    pub explanation: String,
    pub payload_schema: String,
    pub effect: IntentEffect,
}

/// The semantic role available before any resource bytes arrive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticRole {
    Graphic,
    Article,
    Image,
}

/// How the realized content relates to the footprint placed by Scenograph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundsRelationship {
    FillFootprint,
    FitWithinFootprint,
    IntrinsicWithinFootprint,
}

/// Semantics that remain usable when the richest resource cannot be decoded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationSemantics {
    pub label: String,
    pub role: SemanticRole,
    pub bounds: BoundsRelationship,
    pub actions: Vec<AdvertisedAction>,
}

/// Versioned payload encodings understood by the first Graphshell host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresentationCodec {
    NativeGlyphV1,
    PortableCardV1,
    ImageV1 { mime_type: String },
}

/// One independently fetchable representation, ordered richest-first within
/// a manifest entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationOffer {
    pub codec: PresentationCodec,
    pub resource: ContentHash,
    pub byte_size: u64,
    pub requires: PresentationCapability,
    pub semantics: PresentationSemantics,
}

/// Connects one scene instance to one presentation key without adding a
/// Graphshell-owned reference to `sceno::ProjectedItem`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationBinding {
    pub instance: InstanceId,
    pub key: PresentationKey,
}

/// Presentation metadata beside a scene. Resource bytes travel separately.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationManifest {
    pub bindings: Vec<PresentationBinding>,
    pub offers: BTreeMap<PresentationKey, Vec<PresentationOffer>>,
}

impl PresentationManifest {
    pub fn offers_for(&self, instance: InstanceId) -> Option<&[PresentationOffer]> {
        let key = self
            .bindings
            .iter()
            .find(|binding| binding.instance == instance)?
            .key
            .clone();
        self.offers.get(&key).map(Vec::as_slice)
    }
}

/// Whether disclosed scene and resource data may survive process exit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheRetention {
    #[default]
    MemoryOnly,
    EncryptedPersistent,
    Exportable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachePolicy {
    pub retention: CacheRetention,
    pub expires_at_ms: Option<u64>,
    pub purge_on_revocation: bool,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            retention: CacheRetention::MemoryOnly,
            expires_at_ms: None,
            purge_on_revocation: true,
        }
    }
}

/// A complete epoch-preserving scene snapshot plus its presentation sidecar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectionSnapshot {
    pub version: ProtocolVersion,
    pub session: ProjectionSession,
    pub scene: SceneSnapshot,
    #[serde(default)]
    pub presentation: PresentationManifest,
    #[serde(default)]
    pub cache_policy: CachePolicy,
}

/// Presentation and cache changes that accompany a Scenotime scene diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresentationChange {
    Bind(PresentationBinding),
    Unbind {
        instance: InstanceId,
    },
    ReplaceOffers {
        key: PresentationKey,
        offers: Vec<PresentationOffer>,
    },
    RemoveOffers {
        key: PresentationKey,
    },
    InvalidateResource {
        resource: ContentHash,
    },
}

/// One revision transition in a projection session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDiff {
    pub version: ProtocolVersion,
    pub session: ProjectionSession,
    pub scene: SceneDiff,
    pub presentation: Vec<PresentationChange>,
    pub status: Option<SessionStatus>,
}

/// The last revision a client has durably applied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionAck {
    pub session: ProjectionSession,
    pub epoch: SceneEpoch,
    pub revision: Revision,
}

/// Reconnect from a client's last acknowledged epoch and revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeRequest {
    pub session: ProjectionSession,
    pub epoch: SceneEpoch,
    pub revision: Revision,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ResumeReply {
    Current(ProjectionAck),
    Diffs(Vec<ProjectionDiff>),
    Snapshot(Box<ProjectionSnapshot>),
}

/// A content-addressed resource request scoped to the disclosing session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRequest {
    pub session: ProjectionSession,
    pub resource: ContentHash,
}

/// Independently transferred bytes. Clients verify the address before caching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceResponse {
    pub session: ProjectionSession,
    pub resource: ContentHash,
    pub bytes: Vec<u8>,
}

impl ResourceResponse {
    pub fn new(session: ProjectionSession, bytes: Vec<u8>) -> Self {
        let resource = ContentHash::of(&bytes);
        Self {
            session,
            resource,
            bytes,
        }
    }

    pub fn has_valid_address(&self) -> bool {
        ContentHash::of(&self.bytes) == self.resource
    }
}

/// The payload for a native Graphshell glyph resource.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeGlyphV1 {
    pub label: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// One labeled value in a portable card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardValueV1 {
    pub label: String,
    pub value: String,
}

/// A deliberately small semantic card, not a serialized widget tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableCardV1 {
    pub title: String,
    pub values: Vec<CardValueV1>,
    pub badges: Vec<String>,
    pub media: Vec<ContentHash>,
}

/// A semantic intent invocation. `payload` is deliberately opaque at G1; its
/// advertised schema is versioned and validation remains endpoint-side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentInvocation {
    pub session: ProjectionSession,
    pub target: InstanceId,
    pub observed_epoch: SceneEpoch,
    pub observed_revision: Revision,
    pub intent: String,
    pub payload: Vec<u8>,
}

/// The result of endpoint-side intent validation and dispatch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentResult {
    Accepted,
    Rejected {
        reason: String,
    },
    Stale {
        current_epoch: SceneEpoch,
        current_revision: Revision,
    },
}

/// The session status a client may render without inferring authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Live,
    Stale,
    Disconnected,
    Expired,
    Revoked,
}

/// Open a session **after** the carrier has admitted it.
///
/// This message carries no principal, and that absence is the design. The
/// carrier's own handshake (`notochord`) is the sole admission step: it
/// binds a Personae subject, its attested signer, its delegation chain, the
/// requested action, and a nonce to *this* connection, and withholds the
/// application stream until that verifies. A principal repeated here would be
/// a claim bound to nothing — a second, weaker admission path beside the real
/// one, which is how an identity ends up trusted because it was asserted twice
/// rather than proved once.
///
/// So the session plane spans two layers: the carrier establishes **who**, and
/// this message negotiates **what we can both speak**. On a carrier that
/// cannot authenticate (loopback, stdio), there is no principal to have, and
/// `open` is refused rather than answered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionOpen {
    pub version: ProtocolVersion,
    /// What the client can render, so an endpoint may refuse early rather than
    /// after a snapshot it cannot present.
    pub capabilities: CapabilityProfile,
}

/// An accepted session.
///
/// Carries no endpoint key either: the peer's identity is whatever the
/// carrier authenticated, and a key asserted in this frame would be
/// unverified by construction. A client pins the peer its transport proved,
/// never a field its peer chose.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionOpened {
    pub version: ProtocolVersion,
    pub descriptor: EndpointDescriptor,
    pub status: SessionStatus,
    /// When the admitting grant expires, if it is bounded. A hint for
    /// pre-emptive renewal — never a substitute for the endpoint's own check,
    /// since a grant can be revoked long before it expires.
    pub expires_at_ms: Option<u64>,
}

/// Carrier-neutral requests used by the local process proof and future
/// transports. The carrier supplies framing; these variants supply meaning.
///
/// Section 4.1's five session verbs map here as: `open` →
/// [`CarrierRequestBody::Open`], `close` → [`CarrierRequestBody::Close`],
/// `suspend` → [`CarrierRequestBody::Suspend`], and **both** `resume` and
/// `resynchronize` → [`CarrierRequestBody::Resume`]. That last pairing is not
/// a shortcut: a client that finds its epoch or base revision disagreeing
/// emits a `ResumeRequest` and the endpoint answers with diffs, an ack, or a
/// full snapshot — `graphshell-client` already names that path
/// `Resynchronize(ResumeRequest)`. [`CarrierRequestBody::Snapshot`] is the
/// projection plane's request and carries a score, so it selects a projection
/// rather than recovering a session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CarrierRequestBody {
    Discover,
    Snapshot(ProjectionRequest),
    Resource(ResourceRequest),
    Resume(ResumeRequest),
    Intent(IntentInvocation),
    /// Negotiate version and capabilities on an already-admitted carrier.
    Open(Box<SessionOpen>),
    /// Tear the session down; the endpoint may release its state.
    Close,
    /// Going away, but keep the session resumable.
    Suspend,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CarrierRequest {
    pub id: u64,
    pub body: CarrierRequestBody,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CarrierResponseBody {
    Descriptor(EndpointDescriptor),
    Snapshot(Box<ProjectionSnapshot>),
    Resource(ResourceResponse),
    Resume(ResumeReply),
    Intent(IntentResult),
    Opened(Box<SessionOpened>),
    Closed,
    Suspended,
}

/// A failure reported by an endpoint without exposing its native error type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarrierFailure {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CarrierResponse {
    pub id: u64,
    pub body: Result<CarrierResponseBody, CarrierFailure>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::{Arrangement, Scene, Spiral};

    #[test]
    fn an_open_negotiates_capabilities_and_carries_no_principal() {
        // The absence is the assertion: admission belongs to the carrier's
        // handshake, so this message has nowhere to put a claimed identity.
        let open = SessionOpen {
            version: ProtocolVersion::V1,
            capabilities: CapabilityProfile::new([PresentationCapability::PortableCard]),
        };
        let request = CarrierRequest {
            id: 1,
            body: CarrierRequestBody::Open(Box::new(open.clone())),
        };
        let wire = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<CarrierRequest>(&wire).unwrap(),
            request
        );
        assert!(
            !wire.contains("subject") && !wire.contains("grant"),
            "no principal travels in the application protocol: {wire}"
        );
    }

    #[test]
    fn an_accepted_session_asserts_no_endpoint_key() {
        // A key in this frame would be unverified by construction; the peer's
        // identity is whatever the carrier authenticated.
        let response = CarrierResponse {
            id: 1,
            body: Ok(CarrierResponseBody::Opened(Box::new(SessionOpened {
                version: ProtocolVersion::V1,
                descriptor: EndpointDescriptor {
                    label: "turnstone".into(),
                    projections: Vec::new(),
                },
                status: SessionStatus::Live,
                expires_at_ms: Some(1_000),
            }))),
        };
        let wire = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<CarrierResponse>(&wire).unwrap(),
            response
        );
        assert!(
            !wire.contains("endpoint_subject"),
            "the endpoint does not assert its own key: {wire}"
        );
    }

    #[test]
    fn resynchronize_is_resume_not_snapshot() {
        // The client's own recovery path is `Resynchronize(ResumeRequest)`:
        // when an epoch or base revision disagrees it resumes, and the
        // endpoint may answer with diffs, an ack, or a snapshot. `Snapshot`
        // belongs to the projection plane and carries a score, so it selects a
        // projection rather than recovering a session.
        let session = ProjectionSession("s".into());
        let resync = CarrierRequestBody::Resume(ResumeRequest {
            session: session.clone(),
            epoch: SceneEpoch(1),
            revision: Revision(4),
        });
        match &resync {
            CarrierRequestBody::Resume(request) => {
                assert_eq!(request.epoch, SceneEpoch(1));
                assert_eq!(request.revision, Revision(4));
            }
            other => panic!("resynchronize recovers a session: {other:?}"),
        }
        // Every session verb still round-trips.
        for verb in [
            CarrierRequestBody::Open(Box::new(SessionOpen {
                version: ProtocolVersion::V1,
                capabilities: CapabilityProfile::default(),
            })),
            CarrierRequestBody::Close,
            CarrierRequestBody::Suspend,
            resync,
        ] {
            let request = CarrierRequest { id: 0, body: verb };
            let wire = serde_json::to_string(&request).unwrap();
            assert_eq!(
                serde_json::from_str::<CarrierRequest>(&wire).unwrap(),
                request
            );
        }
        let _ = session;
    }

    #[test]
    fn request_serializes_a_product_free_score() {
        let request = ProjectionRequest {
            version: ProtocolVersion::V1,
            session: ProjectionSession("local:fixture".into()),
            score: Score::new(Arrangement::Spiral(Spiral::default())),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<ProjectionRequest>(&json).unwrap(),
            request
        );
        assert!(std::any::type_name::<Score>().starts_with("sceno::"));
    }

    #[test]
    fn snapshot_keeps_presentation_beside_the_scene() {
        let snapshot = ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: ProjectionSession("local:fixture".into()),
            scene: SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), Scene::new()).unwrap(),
            presentation: PresentationManifest::default(),
            cache_policy: CachePolicy::default(),
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("presentation"));
        assert_eq!(
            serde_json::from_str::<ProjectionSnapshot>(&json).unwrap(),
            snapshot
        );
    }

    #[test]
    fn resource_address_detects_changed_bytes() {
        let mut response = ResourceResponse::new(
            ProjectionSession("local:fixture".into()),
            b"card bytes".to_vec(),
        );
        assert!(response.has_valid_address());
        response.bytes.push(b'!');
        assert!(!response.has_valid_address());
    }

    #[test]
    fn diff_replay_round_trips_as_a_carrier_neutral_message() {
        let reply = ResumeReply::Diffs(vec![ProjectionDiff {
            version: ProtocolVersion::V1,
            session: ProjectionSession("local:fixture".into()),
            scene: SceneDiff {
                epoch: SceneEpoch(5),
                base: Revision(8),
                revision: Revision(9),
                operations: Vec::new(),
            },
            presentation: Vec::new(),
            status: Some(SessionStatus::Live),
        }]);
        let json = serde_json::to_string(&reply).unwrap();
        assert_eq!(serde_json::from_str::<ResumeReply>(&json).unwrap(), reply);
    }

    #[test]
    fn discovery_and_requests_share_one_framed_vocabulary() {
        let request = CarrierRequest {
            id: 7,
            body: CarrierRequestBody::Discover,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<CarrierRequest>(&json).unwrap(),
            request
        );

        let descriptor = EndpointDescriptor {
            label: "Fixture".into(),
            projections: vec![ProjectionOffer {
                label: "Notes".into(),
                request: ProjectionRequest {
                    version: ProtocolVersion::V1,
                    session: ProjectionSession("fixture:notes".into()),
                    score: Score::new(Arrangement::Spiral(Spiral::default())),
                },
            }],
        };
        let response = CarrierResponse {
            id: 7,
            body: Ok(CarrierResponseBody::Descriptor(descriptor)),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<CarrierResponse>(&json).unwrap(),
            response
        );
    }
}
