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
    /// Protocol 1.1 adds the payload-free carrier revision notice.
    pub const V1_1: Self = Self { major: 1, minor: 1 };
    /// Protocol 1.2 adds editable-text resources.
    pub const V1_2: Self = Self { major: 1, minor: 2 };
    /// Latest protocol 1.x. Protocol 1.3 adds attributable derived-cache
    /// metadata to editable-text resources.
    pub const V1: Self = Self { major: 1, minor: 3 };
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
    EditableText,
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
    EditableTextV1,
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

/// Most raw bytes one resource chunk carries before encoding.
///
/// Sized against the native-messaging frame cap rather than chosen round.
/// `bytes` is base64 in the chunk reply, which costs 4/3, so 512 KiB raw
/// arrives as about 683 KiB and leaves room for the rest of the frame under a
/// 1 MiB cap. Serializing raw `Vec<u8>` through JSON instead would cost
/// roughly 4x and put a single chunk over the cap on its own, which is why
/// this reply carries text where [`ResourceResponse`] carries bytes.
pub const MAX_RESOURCE_CHUNK_BYTES: u32 = 512 * 1024;

/// Read part of a resource, for resources too large for one carrier frame.
///
/// Deliberately not a range on [`ResourceRequest`]. That reply's `resource`
/// is the address *of its bytes*, checked by
/// [`has_valid_address`](ResourceResponse::has_valid_address); a partial reply
/// would make that field either wrong or ambiguous. Chunks address themselves
/// separately instead, so both checks stay honest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceChunkRequest {
    pub session: ProjectionSession,
    /// The whole resource being read, not this chunk.
    pub resource: ContentHash,
    pub offset: u64,
    /// Most bytes this client will accept back. `0` lets the endpoint choose;
    /// any value is clamped to [`MAX_RESOURCE_CHUNK_BYTES`]. A zero-length
    /// reply below the end of a resource would strand a client in a loop, so
    /// the endpoint never sends one.
    pub length: u32,
}

/// One addressed slice of a resource.
///
/// Carries two hashes because they answer different questions: `resource`
/// says which whole thing this belongs to, and `chunk` says these particular
/// bytes arrived intact. A client verifies `chunk` per frame and the assembled
/// `resource` once at the end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceChunkResponse {
    pub session: ProjectionSession,
    pub resource: ContentHash,
    pub offset: u64,
    /// Length of the whole resource, so a client can size its buffer and know
    /// when it is finished without waiting for a sentinel frame.
    pub total_len: u64,
    pub chunk: ContentHash,
    /// Base64 (standard alphabet, padded). See [`MAX_RESOURCE_CHUNK_BYTES`].
    pub bytes: String,
}

impl ResourceChunkResponse {
    /// Cut one reply out of a whole resource already in hand.
    ///
    /// The default path for any endpoint that cannot seek. An offset past the
    /// end yields an empty final chunk rather than an error, so a client that
    /// raced a shrinking resource stops instead of retrying forever.
    pub fn slice(whole: &ResourceResponse, offset: u64, length: u32) -> Self {
        Self::from_slice(
            whole.session.clone(),
            whole.resource,
            &whole.bytes,
            offset,
            length,
        )
    }

    /// Cut one reply from bytes held by reference.
    ///
    /// The form to use when the whole resource is already in hand: taking it
    /// as a slice means serving N chunks copies the resource once, not N
    /// times.
    pub fn from_slice(
        session: ProjectionSession,
        resource: ContentHash,
        whole: &[u8],
        offset: u64,
        length: u32,
    ) -> Self {
        let total_len = whole.len() as u64;
        let start = offset.min(total_len) as usize;
        let want = if length == 0 {
            MAX_RESOURCE_CHUNK_BYTES
        } else {
            length.min(MAX_RESOURCE_CHUNK_BYTES)
        } as usize;
        let end = start.saturating_add(want).min(whole.len());
        let slice = &whole[start..end];
        Self {
            session,
            resource,
            offset: start as u64,
            total_len,
            chunk: ContentHash::of(slice),
            bytes: base64_encode(slice),
        }
    }

    /// The raw bytes, if this frame is intact. `None` when the payload is not
    /// valid base64 or does not match `chunk`.
    pub fn decode(&self) -> Option<Vec<u8>> {
        let bytes = base64_decode(&self.bytes)?;
        (ContentHash::of(&bytes) == self.chunk).then_some(bytes)
    }

    /// Whether this frame reaches the end of the resource.
    pub fn is_final(&self, decoded_len: usize) -> bool {
        self.offset.saturating_add(decoded_len as u64) >= self.total_len
    }
}

/// Reassembles a resource from sequential chunk replies.
///
/// Sequential on purpose: chunks are requested one window at a time, so an
/// out-of-order or duplicated frame is a protocol fault to report rather than
/// a case to reorder around. The whole-resource hash is verified once at the
/// end, so a corrupted frame that somehow passed its own check still cannot
/// be delivered as the resource.
#[derive(Clone, Debug, Default)]
pub struct ResourceAssembly {
    bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblyError {
    /// The frame failed its own `chunk` address, or was not valid base64.
    CorruptChunk,
    /// The frame did not begin where the previous one ended.
    OutOfOrder { expected: u64, found: u64 },
    /// Every byte arrived, but the whole does not match `resource`.
    WrongResource,
}

impl fmt::Display for AssemblyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CorruptChunk => write!(formatter, "resource chunk failed its own address"),
            Self::OutOfOrder { expected, found } => {
                write!(formatter, "expected chunk at {expected}, received {found}")
            }
            Self::WrongResource => {
                write!(
                    formatter,
                    "assembled bytes do not match the resource address"
                )
            }
        }
    }
}

impl ResourceAssembly {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bytes received so far; also the offset of the next chunk to request.
    pub fn received(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Take one reply. `Ok(Some(bytes))` means the resource is complete and
    /// verified; `Ok(None)` means keep going from [`received`](Self::received).
    pub fn accept(
        &mut self,
        response: &ResourceChunkResponse,
    ) -> Result<Option<Vec<u8>>, AssemblyError> {
        let decoded = response.decode().ok_or(AssemblyError::CorruptChunk)?;
        if response.offset != self.received() {
            return Err(AssemblyError::OutOfOrder {
                expected: self.received(),
                found: response.offset,
            });
        }
        self.bytes.extend_from_slice(&decoded);
        if !response.is_final(decoded.len()) {
            return Ok(None);
        }
        if ContentHash::of(&self.bytes) != response.resource {
            return Err(AssemblyError::WrongResource);
        }
        Ok(Some(std::mem::take(&mut self.bytes)))
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text).ok()
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

/// Text encodings accepted by the first editable-text resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEncoding {
    Utf8,
}

/// A versioned editable source disclosed by an endpoint.
///
/// `base_token` is opaque to the client. It binds the save to the exact
/// document version the endpoint disclosed without exposing a native path,
/// vault identifier, or causal frontier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditableTextV1 {
    pub address: String,
    pub media_type: String,
    pub encoding: TextEncoding,
    pub source: String,
    pub base_token: Vec<u8>,
    /// A host-authorized rendering derived from `source`. A sealed source
    /// endpoint may restore this projection from its attributable cache.
    ///
    /// The base token still names the authored source. Consumers must not
    /// offer this text as the save buffer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived: Option<DerivedTextV1>,
}

/// A revisioned effect result attached to an editable source presentation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedTextV1 {
    pub source: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<DerivedCacheInfoV1>,
}

/// Attribution for a resolved result. Sealed source endpoints may persist it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedCacheInfoV1 {
    pub effect: String,
    pub sources: Vec<String>,
    pub provider_version: String,
    pub policy_fingerprint: String,
    pub fetched_at_unix_ms: u64,
    pub source_revision: u64,
}

/// Typed payload for [`EDITABLE_TEXT_SAVE_INTENT`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveTextV1 {
    pub base_token: Vec<u8>,
    pub source: String,
}

pub const EDITABLE_TEXT_SAVE_INTENT: &str = "graphshell.editable-text.save";
pub const EDITABLE_TEXT_SAVE_SCHEMA: &str = "graphshell.editable-text.save/v1";

/// Typed payload for [`KNOT_CLIP_INSERT_INTENT`].
///
/// The target document comes from the invocation binding. `knot_body` is a
/// semantic Knot block stream without document frontmatter; the endpoint
/// records the source fields as structured provenance before appending it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsertKnotClipV1 {
    pub base_token: Vec<u8>,
    pub source_url: String,
    pub title: Option<String>,
    pub selector: Option<String>,
    pub knot_body: String,
}

pub const KNOT_CLIP_INSERT_INTENT: &str = "knot.clip.insert";
pub const KNOT_CLIP_INSERT_SCHEMA: &str = "knot.clip.insert/v1";

/// Typed consent receipt for Knot's derived-state document effects.
///
/// `confirmed` records an explicit product gesture when policy requires one.
/// The endpoint still decides whether the action is admitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnotEffectV1 {
    pub base_token: Vec<u8>,
    pub confirmed: bool,
}

pub const KNOT_TRANSCLUSION_RESOLVE_INTENT: &str = "knot.transclusion.resolve";
pub const KNOT_TRANSCLUSION_RESOLVE_SCHEMA: &str = "knot.transclusion.resolve/v1";
pub const KNOT_BLOCK_RUN_INTENT: &str = "knot.block.run";
pub const KNOT_BLOCK_RUN_SCHEMA: &str = "knot.block.run/v1";

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
    /// Read part of a resource. Requesting the next chunk is what
    /// acknowledges the last one, so flow control is the client's outstanding
    /// request count and needs no separate ack frame.
    ResourceChunk(ResourceChunkRequest),
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
    ResourceChunk(Box<ResourceChunkResponse>),
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

/// Payload-free signal that a mounted projection has advanced.
///
/// The host still resumes from its own last acknowledgement before receiving
/// any scene or presentation data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarrierNotice {
    pub session: ProjectionSession,
    pub epoch: SceneEpoch,
    pub revision: Revision,
}

/// One line emitted by a carrier endpoint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CarrierOutput {
    Response(CarrierResponse),
    Notice(CarrierNotice),
}

/// How a host reaches one endpoint, whatever sits between them.
///
/// The protocol has always described itself as running over an *unspecified*
/// carrier; this is that seam made real. A carrier moves request and response
/// bodies and surfaces notices, and knows nothing about what they mean.
///
/// Deliberately not `Send + Sync`: a browser carrier is single-threaded, and
/// requiring thread-safety here would exclude the target the protocol most
/// needs to reach. A host that wants a carrier on another thread owns that
/// choice; the trait does not impose it.
///
/// The surface is blocking, which is what the first carrier is. A network
/// carrier will want either an async sibling or a worker thread behind this
/// same shape, and that is a decision to make when one is written rather than
/// guessed at now.
pub trait Carrier {
    /// Send one request and wait for its response.
    fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String>;

    /// Take one already-received notice, if any is queued. Never blocks.
    fn take_notice(&mut self) -> Option<CarrierNotice>;

    /// Block until a notice arrives.
    fn wait_for_notice(&mut self) -> Result<CarrierNotice, String>;

    /// Release whatever the carrier holds open.
    ///
    /// `&mut self` rather than `self` so this is callable on a boxed carrier,
    /// which is the whole point of the seam. Implementations should tolerate a
    /// second call rather than assuming exactly one.
    fn shutdown(&mut self) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::{Arrangement, Scene, Spiral};

    #[test]
    fn a_revision_notice_is_distinct_from_a_keyed_response() {
        let notice = CarrierNotice {
            session: ProjectionSession("knot:directory".into()),
            epoch: SceneEpoch(1),
            revision: Revision(7),
        };
        let wire = serde_json::to_string(&CarrierOutput::Notice(notice.clone())).unwrap();
        assert_eq!(
            serde_json::from_str::<CarrierOutput>(&wire).unwrap(),
            CarrierOutput::Notice(notice)
        );
        assert!(!wire.contains("\"id\""));
        assert!(!wire.contains("scene"));
    }

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
    fn editable_text_and_save_payloads_are_versioned_and_strict() {
        assert_eq!(ProtocolVersion::V1_1.minor, 1);
        assert_eq!(ProtocolVersion::V1_2.minor, 2);
        assert_eq!(ProtocolVersion::V1.minor, 3);
        let editable = EditableTextV1 {
            address: "knot://field-note".into(),
            media_type: "text/vnd.knot".into(),
            encoding: TextEncoding::Utf8,
            source: "# Field note\n".into(),
            base_token: vec![1, 2, 3],
            derived: Some(DerivedTextV1 {
                source: "# Field note\n\nFetched.\n".into(),
                summary: "resolved 1; denied 0; failed 0".into(),
                cache: Some(DerivedCacheInfoV1 {
                    effect: "resolve".into(),
                    sources: vec!["https://example.test/field".into()],
                    provider_version: "fixture/v1".into(),
                    policy_fingerprint: "policy-digest".into(),
                    fetched_at_unix_ms: 42,
                    source_revision: 7,
                }),
            }),
        };
        let bytes = serde_json::to_vec(&editable).unwrap();
        assert_eq!(
            serde_json::from_slice::<EditableTextV1>(&bytes).unwrap(),
            editable
        );
        let legacy = br#"{"address":"knot://legacy","media_type":"text/vnd.knot","encoding":"Utf8","source":"old","base_token":[1]}"#;
        assert!(
            serde_json::from_slice::<EditableTextV1>(legacy)
                .unwrap()
                .derived
                .is_none()
        );
        let protocol_1_2_derived =
            br##"{"source":"# Field note\n\nFetched.\n","summary":"resolved 1"}"##;
        assert!(
            serde_json::from_slice::<DerivedTextV1>(protocol_1_2_derived)
                .unwrap()
                .cache
                .is_none()
        );

        let save = SaveTextV1 {
            base_token: vec![1, 2, 3],
            source: "# Revised\n".into(),
        };
        let wire = serde_json::to_string(&save).unwrap();
        assert_eq!(serde_json::from_str::<SaveTextV1>(&wire).unwrap(), save);
        assert!(
            serde_json::from_str::<SaveTextV1>(
                r#"{"base_token":[1,2,3],"source":"x","path":"outside.knot"}"#
            )
            .is_err(),
            "a save payload cannot smuggle endpoint-native authority"
        );

        let clip = InsertKnotClipV1 {
            base_token: vec![1, 2, 3],
            source_url: "https://example.test/field".into(),
            title: Some("Field report".into()),
            selector: Some("main > article".into()),
            knot_body: "# Field report\n\nA finding.\n".into(),
        };
        let wire = serde_json::to_string(&clip).unwrap();
        assert_eq!(
            serde_json::from_str::<InsertKnotClipV1>(&wire).unwrap(),
            clip
        );
        assert!(
            serde_json::from_str::<InsertKnotClipV1>(
                r#"{"base_token":[1],"source_url":"https://example.test","title":null,"selector":null,"knot_body":"x","path":"outside.knot"}"#
            )
            .is_err(),
            "a clip payload cannot smuggle endpoint-native authority"
        );

        let effect = KnotEffectV1 {
            base_token: vec![1, 2, 3],
            confirmed: true,
        };
        let wire = serde_json::to_string(&effect).unwrap();
        assert_eq!(serde_json::from_str::<KnotEffectV1>(&wire).unwrap(), effect);
        assert!(
            serde_json::from_str::<KnotEffectV1>(
                r#"{"base_token":[1,2,3],"confirmed":true,"command":"rm"}"#
            )
            .is_err(),
            "an effect payload cannot smuggle an evaluator or fetch command"
        );
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

    fn resource(bytes: Vec<u8>) -> ResourceResponse {
        ResourceResponse::new(ProjectionSession("fixture:blobs".into()), bytes)
    }

    /// Pull a whole resource the way a client does: request, accept, repeat
    /// from wherever the assembly says it is.
    fn pull(whole: &ResourceResponse, window: u32) -> Result<Vec<u8>, AssemblyError> {
        let mut assembly = ResourceAssembly::new();
        loop {
            let chunk = ResourceChunkResponse::slice(whole, assembly.received(), window);
            if let Some(bytes) = assembly.accept(&chunk)? {
                return Ok(bytes);
            }
        }
    }

    #[test]
    fn a_resource_larger_than_one_frame_arrives_whole_and_verified() {
        // Deliberately not a multiple of the window, so the last chunk is short.
        let bytes: Vec<u8> = (0..300_000u32).map(|index| (index % 251) as u8).collect();
        let whole = resource(bytes.clone());
        assert_eq!(pull(&whole, 64 * 1024).unwrap(), bytes);
        // A window of one proves the loop terminates on the awkward sizes too.
        assert_eq!(pull(&resource(vec![7; 5]), 1).unwrap(), vec![7; 5]);
        // Zero means "endpoint chooses" rather than an infinite stall.
        assert_eq!(pull(&whole, 0).unwrap(), bytes);
    }

    #[test]
    fn an_empty_resource_completes_in_one_chunk() {
        let whole = resource(Vec::new());
        let chunk = ResourceChunkResponse::slice(&whole, 0, 1024);
        assert_eq!(chunk.total_len, 0);
        assert_eq!(
            ResourceAssembly::new().accept(&chunk).unwrap(),
            Some(Vec::new())
        );
    }

    #[test]
    fn a_client_request_is_clamped_rather_than_trusted() {
        let whole = resource(vec![0; 4 * 1024 * 1024]);
        let chunk = ResourceChunkResponse::slice(&whole, 0, u32::MAX);
        assert_eq!(
            chunk.decode().unwrap().len(),
            MAX_RESOURCE_CHUNK_BYTES as usize,
            "a client asking for everything must not get a frame over the cap"
        );
    }

    #[test]
    fn base64_keeps_a_full_chunk_inside_the_native_message_frame() {
        let whole = resource(vec![0xa5; MAX_RESOURCE_CHUNK_BYTES as usize]);
        let chunk = ResourceChunkResponse::slice(&whole, 0, MAX_RESOURCE_CHUNK_BYTES);
        let framed = serde_json::to_vec(&CarrierResponse {
            id: 1,
            body: Ok(CarrierResponseBody::ResourceChunk(Box::new(chunk))),
        })
        .unwrap();
        // The 1 MiB cap is browser_carrier's MAX_NATIVE_MESSAGE_BYTES. This is
        // the assertion that keeps the chunk size and the frame cap in step;
        // raising one without the other should fail here rather than in a
        // browser.
        assert!(
            framed.len() < 1024 * 1024,
            "a full chunk framed to {} bytes, over the native-messaging cap",
            framed.len()
        );
    }

    #[test]
    fn a_damaged_or_misordered_chunk_is_refused_rather_than_assembled() {
        let whole = resource(vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let mut assembly = ResourceAssembly::new();

        let mut corrupt = ResourceChunkResponse::slice(&whole, 0, 4);
        corrupt.bytes = base64_encode(b"xxxx");
        assert_eq!(
            assembly.accept(&corrupt),
            Err(AssemblyError::CorruptChunk),
            "a frame that fails its own address must not enter the assembly"
        );

        let skipped = ResourceChunkResponse::slice(&whole, 4, 4);
        assert_eq!(
            assembly.accept(&skipped),
            Err(AssemblyError::OutOfOrder {
                expected: 0,
                found: 4
            })
        );

        // Intact frames, but the whole they claim to form is another resource.
        let mut first = ResourceChunkResponse::slice(&whole, 0, 8);
        first.resource = ContentHash::of(b"a different resource entirely");
        assert_eq!(
            assembly.accept(&first),
            Err(AssemblyError::WrongResource),
            "per-frame checks passing must not be mistaken for the whole verifying"
        );
    }

    #[test]
    fn a_chunk_reply_round_trips_through_the_carrier_envelope() {
        let whole = resource(b"round trip".to_vec());
        let response = CarrierResponse {
            id: 11,
            body: Ok(CarrierResponseBody::ResourceChunk(Box::new(
                ResourceChunkResponse::slice(&whole, 0, 4),
            ))),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<CarrierResponse>(&json).unwrap(),
            response
        );
        let request = CarrierRequest {
            id: 12,
            body: CarrierRequestBody::ResourceChunk(ResourceChunkRequest {
                session: ProjectionSession("fixture:blobs".into()),
                resource: whole.resource,
                offset: 4,
                length: 4,
            }),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<CarrierRequest>(&json).unwrap(),
            request
        );
    }
}
