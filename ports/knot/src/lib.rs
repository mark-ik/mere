//! Knot: Mere's files-in-place authoring port.
//!
//! The first slice is deliberately read-only. [`DirectorySource`] discovers
//! files without storing their bytes in graph state, while [`KnotEndpoint`]
//! discloses those containers through Graphshell. The directory remains source
//! truth.

mod content_classes;
mod directory;
mod editor;
mod endpoint;
mod mark;
mod publish;
mod publish_carrier;
mod publish_client;
mod publish_host;
mod publish_wire;
mod resident;
mod rosette;
mod search;
mod settings;
mod startup;
mod sync;
mod vault;
mod watcher;
mod writer;

pub use content_classes::{
    FILE_CLASS, FILE_DOCUMENT_FACET, KnotContentClasses, NOTE_CLASS, NOTE_DOCUMENT_FACET,
};
pub use directory::{DirectorySource, DiskDocument, IgnorePolicy};
pub use editor::{EditOutcome, KnotEditor};
pub use endpoint::{
    KnotEffectAuthority, KnotEffectFetcher, KnotEffectMode, KnotEffectPolicy, KnotEndpoint,
    KnotRosetteConfig, KnotWriteGrant,
};
pub use mark::{
    MARK_ALPN, MARK_DEFAULT_PORT, MARK_MAX_DOCUMENT_BYTES, MARK_MAX_METADATA_BYTES,
    MARK_MAX_REQUEST_BYTES, MarkAdapterError, MarkQuicHost, MarkReadAccess, MarkReadAdapter,
    MarkReadAdapterLimits, MarkRequest, MarkResponse, MarkServerError, MarkSnapshotOutcome,
    MarkTimestamp, MarkVersion, MarkVersionId, decode_mark_request, mark_server_config,
};
/// Admission-policy vocabulary belongs to the same Notochord instance as
/// Knot's publishing carrier. Product hosts should take these through Knot,
/// not add a second direct Notochord dependency.
pub use notochord::{NetworkId, ProfileRef, TrustedRoot};
pub use publish::{
    KNOT_PUBLISH_ALPN, KNOT_PUBLISH_DOMAIN, KNOT_PUBLISH_READ_ACTION, KNOT_PUBLISH_SERVICE,
    KNOT_SHARE_TICKET_VERSION, KnotPublication, KnotPublishCandidate, KnotPublishCatalog,
    KnotPublishEligibility, KnotPublishError, KnotPublishRead, KnotPublishedDocument,
    KnotShareControlError, KnotShareRecipient, KnotShareTicket, PublicationId, publication_path,
    revoke_share,
};
pub use publish_carrier::{
    PublishCarrierError, PublishRefusal, accept_publish_session, publish_alpn, publish_policy,
};
pub use publish_client::{
    KNOT_PUBLISH_READER_KEY_CONTEXT, KnotPublishClientError, decode_share_ticket,
    encode_share_ticket, fetch_published_document,
};
pub use publish_host::{
    KnotPublishHost, KnotPublishHostError, KnotPublishHostLimits, KnotPublishServeOutcome,
    KnotPublishSource,
};
pub use publish_wire::{
    CandidateFixture, CandidateFixtureOutcome, HARD_MAX_CATALOG_ENTRIES, HARD_MAX_DOCUMENT_BYTES,
    HARD_MAX_REQUEST_BYTES, HARD_MAX_RESPONSE_BYTES, PublishRequest, PublishResponse,
    PublishWireError, PublishWireLimits, candidate_fixture_corpus, decode_request, decode_response,
    encode_request, encode_response,
};
pub use resident::{KnotSyncHost, KnotSyncHostConfig, KnotSyncHostError};
pub use rosette::{
    CmudictPronunciations, LexiconCoverage, PronunciationLexicon, RosetteConfig, RosetteInterior,
    RosetteInteriorKind, RosetteProjection, UnresolvedToken, project_rosette,
};
pub use search::{KnotSearch, SearchConfig, SearchHit, SearchLane};
pub use settings::{
    KnotSettings, KnotSettingsError, KnotSyncSettings, hex32, knot_settings_path, parse_hex32,
};
pub use startup::{StartupUnlockedPersonalVault, local_device_root, persona_vault_root};
pub use sync::{
    KNOT_COMMONS_ENCRYPTION_PROFILE, KnotAutomaticTextMerge, KnotCheckpointSnapshot,
    KnotDocumentConflict, KnotDocumentProjection, KnotDocumentVersion, KnotEncryptionProfile,
    KnotEpochExecutionReceipt, KnotOfflineMemberEpochHold, KnotOfflineMemberRecovery,
    KnotProjectionCheckpoint, KnotSyncCipher, KnotSyncError, KnotSyncEvent, KnotSyncExt,
    KnotSyncFileStore, KnotSyncStore, KnotTailReceipt,
};
pub use vault::{KnotVault, VaultDocument};
pub use watcher::DirectoryWatcher;
pub use writer::{AuthoredFile, DocumentFormat, SaveOutcome};
