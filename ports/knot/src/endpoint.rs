//! Graphshell disclosure for Knot directory state.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use chartulary::{Addressed, Labeled};
use chirograph::{
    AdvertisedAction, BoundsRelationship, CachePolicy, CardValueV1, CarrierNotice, ContentHash,
    DerivedCacheInfoV1, DerivedTextV1, EDITABLE_TEXT_SAVE_INTENT, EDITABLE_TEXT_SAVE_SCHEMA,
    EditableTextV1, EndpointDescriptor, InsertKnotClipV1, InsertKnotClipV2, IntentEffect,
    IntentInvocation, IntentReference, IntentResult, KNOT_BLOCK_RUN_INTENT, KNOT_BLOCK_RUN_SCHEMA,
    KNOT_CLIP_INSERT_INTENT, KNOT_CLIP_INSERT_SCHEMA, KNOT_CLIP_INSERT_SCHEMA_V2,
    KNOT_TRANSCLUSION_RESOLVE_INTENT, KNOT_TRANSCLUSION_RESOLVE_SCHEMA, KnotClipArtifactRoleV1,
    KnotClipArtifactV1, KnotClipSelectorV1, KnotEffectV1, NativeGlyphV1, PortableCardV1,
    PresentationBinding, PresentationCapability, PresentationCodec, PresentationKey,
    PresentationManifest, PresentationOffer, PresentationSemantics, ProjectionAck, ProjectionOffer,
    ProjectionRequest, ProjectionSession, ProjectionSnapshot, ProtocolVersion, ResourceRequest,
    ResourceResponse, ResumeReply, ResumeRequest, SaveTextV1, SemanticRole, TextEncoding,
};
use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
    ResumableProjectionSource,
};
use inker::{
    BlockEvaluators, DocumentTrustState, Engine, EngineDocument, EngineInput, EvaluationPolicy,
    Fetched, TransclusionPolicy, evaluate_blocks, resolve_transclusions,
};
use personae::{IdentityProvider, InMemoryProvider};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Rect, Representation, Scene, Score, Size2,
    SourceRef, Transform2, Vec2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use serde::{Deserialize, Serialize};
use stickleback::{DataKeyring, GroupCiphertext, GroupSecretId};
use zeroize::Zeroizing;

use crate::{
    CmudictPronunciations, DirectorySource, DirectoryWatcher, DiskDocument, DocumentFormat,
    KnotClipEvidenceRef, KnotClipEvidenceStore, KnotDocumentProjection, KnotSyncEvent,
    KnotSyncFileStore, KnotVault, RosetteConfig, RosetteInteriorKind, VaultDocument,
    project_rosette,
};

const FIXTURE_SESSION: &str = "loopback:knot:k0";
const SOURCE_KIND: &str = "knot.file";
const FILE_TOKEN_CONTEXT: &str = "mere.knot.file-base-token.v1";
const VAULT_TOKEN_CONTEXT: &str = "mere.knot.vault-base-token.v1";

/// Host-selected limits and geometry for Knot's Rosette projections.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnotRosetteConfig {
    /// Scene geometry used by every document-scoped Rosette from this endpoint.
    pub geometry: RosetteConfig,
    /// Largest authored UTF-8 document the endpoint will disclose as a Rosette.
    pub max_source_bytes: u64,
}

impl Default for KnotRosetteConfig {
    fn default() -> Self {
        Self {
            geometry: RosetteConfig::default(),
            max_source_bytes: 2 * 1024 * 1024,
        }
    }
}

/// Authority injected into one endpoint session after its caller has been
/// admitted. Keeping this separate from `IntentInvocation` prevents payloads
/// from claiming their own grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnotWriteGrant {
    pub max_source_bytes: u64,
}

impl KnotWriteGrant {
    pub const fn new(max_source_bytes: u64) -> Self {
        Self { max_source_bytes }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KnotEffectMode {
    Auto,
    Ask,
    #[default]
    Never,
}

/// User settings and hard limits for Knot's derived document effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotEffectPolicy {
    pub resolve: KnotEffectMode,
    pub run: KnotEffectMode,
    pub allowed_schemes: Vec<String>,
    pub allowed_languages: Vec<String>,
    pub max_depth: u8,
    pub max_ops: u64,
}

impl Default for KnotEffectPolicy {
    fn default() -> Self {
        Self {
            resolve: KnotEffectMode::Never,
            run: KnotEffectMode::Never,
            allowed_schemes: Vec::new(),
            allowed_languages: Vec::new(),
            max_depth: 1,
            max_ops: 100_000,
        }
    }
}

/// Fetch authority injected by the endpoint host. Implementations own any
/// path, network, or vault checks before returning source bytes.
pub trait KnotEffectFetcher: Send {
    fn fetch(&mut self, address: &str) -> Result<Fetched, String>;

    /// Stable implementation identity bound into reusable derived cache
    /// entries. Providers with behavior-affecting configuration must include
    /// it here and change the value when their interpretation changes.
    fn cache_version(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// Effect capabilities admitted for one endpoint process.
pub struct KnotEffectAuthority {
    policy: KnotEffectPolicy,
    fetcher: Option<Box<dyn KnotEffectFetcher>>,
    evaluators: BlockEvaluators,
}

impl KnotEffectAuthority {
    pub fn new(policy: KnotEffectPolicy) -> Self {
        Self {
            policy,
            fetcher: None,
            evaluators: BlockEvaluators::new(),
        }
    }

    pub fn with_fetcher(mut self, fetcher: impl KnotEffectFetcher + 'static) -> Self {
        self.fetcher = Some(Box::new(fetcher));
        self
    }

    pub fn register_evaluator(mut self, evaluator: impl inker::BlockEvaluator + 'static) -> Self {
        self.evaluators.register(Box::new(evaluator));
        self
    }
}

enum Source {
    Directory {
        source: DirectorySource,
        watcher: Box<DirectoryWatcher>,
    },
    Fixture(Vec<DiskDocument>),
    Vault(VaultSource),
}

struct VaultSource {
    vault: KnotVault,
    sync: Option<VaultSyncAuthority>,
    conflicts: BTreeSet<String>,
    document_heads: BTreeMap<String, [u8; 32]>,
    pending_history: bool,
}

enum VaultSyncAuthority {
    Personal {
        store: KnotSyncFileStore,
        signing_seed: Zeroizing<[u8; 32]>,
    },
    Commons {
        store: KnotSyncFileStore,
        signing_seed: Zeroizing<[u8; 32]>,
        keys: DataKeyring,
    },
}

#[derive(Clone)]
struct PresentedDocument {
    id: String,
    container: chartulary::Container,
    byte_size: u64,
}

struct DerivedDocument {
    base_token: Vec<u8>,
    document: EngineDocument,
    summary: String,
    cache: Option<CacheAttribution>,
}

#[derive(Clone)]
struct CacheAttribution {
    info: DerivedCacheInfoV1,
    epoch: Option<GroupSecretId>,
}

#[derive(Clone, Serialize, Deserialize)]
struct DerivedCacheRecord {
    version: u64,
    document_id: String,
    base_token: Vec<u8>,
    document: EngineDocument,
    summary: String,
    info: DerivedCacheInfoV1,
}

#[derive(Serialize, Deserialize)]
enum StoredDerivedCache {
    Personal(DerivedCacheRecord),
    Commons(GroupCiphertext),
}

const DERIVED_CACHE_RECORD_VERSION: u64 = 1;

/// Knot's read-only Graphshell endpoint.
pub struct KnotEndpoint {
    source: Source,
    session: ProjectionSession,
    write_grant: Option<KnotWriteGrant>,
    snapshot: Option<ProjectionSnapshot>,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    bindings: BTreeMap<u32, String>,
    protocol_version: ProtocolVersion,
    last_announced: Option<Revision>,
    observed_source_revision: Option<u64>,
    scene_revision: Revision,
    effects: Option<KnotEffectAuthority>,
    clip_evidence: Option<Box<dyn KnotClipEvidenceStore>>,
    derived: BTreeMap<String, DerivedDocument>,
    rosette_config: KnotRosetteConfig,
    rosette_snapshots: BTreeMap<ProjectionSession, ProjectionSnapshot>,
    rosette_resources: BTreeMap<ProjectionSession, BTreeMap<ContentHash, Vec<u8>>>,
    rosette_last_announced: BTreeMap<ProjectionSession, Revision>,
    rosette_document_ids: BTreeMap<ProjectionSession, String>,
}

impl KnotEndpoint {
    /// Serve a real directory. The session id is derived from its canonical path.
    pub fn open(root: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_with_identity(root, &InMemoryProvider::random())
    }

    /// Serve a real directory with one explicitly injected write grant.
    pub fn open_writable(root: impl AsRef<Path>, grant: KnotWriteGrant) -> io::Result<Self> {
        Self::open_writable_with_identity(root, &InMemoryProvider::random(), grant)
    }

    /// Serve a real directory with the watcher key derived from `identity`.
    pub fn open_with_identity(
        root: impl AsRef<Path>,
        identity: &impl IdentityProvider,
    ) -> io::Result<Self> {
        Self::open_directory(root, identity, None)
    }

    pub fn open_writable_with_identity(
        root: impl AsRef<Path>,
        identity: &impl IdentityProvider,
        grant: KnotWriteGrant,
    ) -> io::Result<Self> {
        Self::open_directory(root, identity, Some(grant))
    }

    fn open_directory(
        root: impl AsRef<Path>,
        identity: &impl IdentityProvider,
        write_grant: Option<KnotWriteGrant>,
    ) -> io::Result<Self> {
        let source = DirectorySource::open(root)?;
        let watcher = DirectoryWatcher::new(source.root(), identity).map_err(io::Error::other)?;
        let digest = blake3::hash(source.root().to_string_lossy().as_bytes());
        Ok(Self {
            source: Source::Directory {
                source,
                watcher: Box::new(watcher),
            },
            session: ProjectionSession(format!("knot:directory:{}", &digest.to_hex()[..16])),
            write_grant,
            snapshot: None,
            resources: BTreeMap::new(),
            bindings: BTreeMap::new(),
            protocol_version: ProtocolVersion::V1,
            last_announced: None,
            observed_source_revision: None,
            scene_revision: Revision(0),
            effects: None,
            clip_evidence: None,
            derived: BTreeMap::new(),
            rosette_config: KnotRosetteConfig::default(),
            rosette_snapshots: BTreeMap::new(),
            rosette_resources: BTreeMap::new(),
            rosette_last_announced: BTreeMap::new(),
            rosette_document_ids: BTreeMap::new(),
        })
    }

    /// Deterministic fixed disclosure used by K0's process receipt.
    pub fn fixture() -> Self {
        use chartulary::Container;
        use std::path::PathBuf;

        let documents = [
            (
                "field-notes",
                "Field notes",
                "file:///fixture/field-notes.knot",
                184,
            ),
            (
                "reading-list",
                "Reading list",
                "file:///fixture/reading-list.md",
                96,
            ),
            ("sources", "Sources", "file:///fixture/sources.json", 412),
        ]
        .into_iter()
        .map(|(id, title, address, byte_size)| {
            let mut container = Container::new(format!("knot:fixture:{id}"))
                .with_address(address)
                .with_title(title);
            container.media_type = Some(
                if address.ends_with(".json") {
                    "application/json"
                } else {
                    "text/markdown"
                }
                .into(),
            );
            DiskDocument {
                id: container.id.clone(),
                container,
                path: PathBuf::from(address),
                byte_size,
            }
        })
        .collect();
        Self {
            source: Source::Fixture(documents),
            session: ProjectionSession(FIXTURE_SESSION.into()),
            write_grant: None,
            snapshot: None,
            resources: BTreeMap::new(),
            bindings: BTreeMap::new(),
            protocol_version: ProtocolVersion::V1,
            last_announced: None,
            observed_source_revision: None,
            scene_revision: Revision(0),
            effects: None,
            clip_evidence: None,
            derived: BTreeMap::new(),
            rosette_config: KnotRosetteConfig::default(),
            rosette_snapshots: BTreeMap::new(),
            rosette_resources: BTreeMap::new(),
            rosette_last_announced: BTreeMap::new(),
            rosette_document_ids: BTreeMap::new(),
        }
    }

    /// Serve one unlocked sealed vault read-only.
    pub fn from_vault(vault: KnotVault) -> Self {
        let digest = blake3::hash(vault.root().to_string_lossy().as_bytes());
        Self {
            source: Source::Vault(VaultSource {
                vault,
                sync: None,
                conflicts: BTreeSet::new(),
                document_heads: BTreeMap::new(),
                pending_history: false,
            }),
            session: ProjectionSession(format!("knot:vault:{}", &digest.to_hex()[..16])),
            write_grant: None,
            snapshot: None,
            resources: BTreeMap::new(),
            bindings: BTreeMap::new(),
            protocol_version: ProtocolVersion::V1,
            last_announced: None,
            observed_source_revision: None,
            scene_revision: Revision(0),
            effects: None,
            clip_evidence: None,
            derived: BTreeMap::new(),
            rosette_config: KnotRosetteConfig::default(),
            rosette_snapshots: BTreeMap::new(),
            rosette_resources: BTreeMap::new(),
            rosette_last_announced: BTreeMap::new(),
            rosette_document_ids: BTreeMap::new(),
        }
    }

    /// Serve a personal sealed vault whose recorded truth is a signed Knot
    /// sync log. The sealed vault index is rematerialized from that log.
    pub fn from_synced_vault(
        vault: KnotVault,
        store: KnotSyncFileStore,
        signing_seed: [u8; 32],
        grant: KnotWriteGrant,
    ) -> Result<Self, String> {
        let projection = pollster::block_on(store.projection(&vault))
            .map_err(|error| format!("could not project Knot sync store: {error}"))?;
        let mut endpoint = Self::from_vault(vault);
        endpoint.install_projection(projection)?;
        let Source::Vault(source) = &mut endpoint.source else {
            unreachable!()
        };
        source.sync = Some(VaultSyncAuthority::Personal {
            store,
            signing_seed: Zeroizing::new(signing_seed),
        });
        endpoint.write_grant = Some(grant);
        Ok(endpoint)
    }

    /// Serve a Commons-backed vault using the group's retained data epochs.
    pub fn from_communal_vault(
        vault: KnotVault,
        store: KnotSyncFileStore,
        signing_seed: [u8; 32],
        keys: DataKeyring,
        grant: KnotWriteGrant,
    ) -> Result<Self, String> {
        let projection = pollster::block_on(store.communal_projection(&keys))
            .map_err(|error| format!("could not project Commons Knot store: {error}"))?;
        let mut endpoint = Self::from_vault(vault);
        endpoint.install_projection(projection)?;
        let Source::Vault(source) = &mut endpoint.source else {
            unreachable!()
        };
        source.sync = Some(VaultSyncAuthority::Commons {
            store,
            signing_seed: Zeroizing::new(signing_seed),
            keys,
        });
        endpoint.write_grant = Some(grant);
        Ok(endpoint)
    }

    /// The opaque Graphshell session.
    pub fn session(&self) -> &ProjectionSession {
        &self.session
    }

    /// Configure the Rosette scene before serving projection requests.
    pub fn with_rosette_config(mut self, config: KnotRosetteConfig) -> Self {
        self.rosette_config = config;
        self
    }

    /// The host-selected Rosette configuration.
    pub fn rosette_config(&self) -> KnotRosetteConfig {
        self.rosette_config
    }

    /// Access the directory source when this endpoint owns one.
    pub fn directory(&self) -> Option<&DirectorySource> {
        match &self.source {
            Source::Directory { source, .. } => Some(source),
            Source::Fixture(_) | Source::Vault(_) => None,
        }
    }

    /// Revoke the directory watcher grant. The endpoint keeps serving its last
    /// accepted revision.
    pub fn revoke_watcher(&mut self) -> bool {
        let Source::Directory { watcher, .. } = &mut self.source else {
            return false;
        };
        watcher.revoke();
        true
    }

    /// Restore the directory watcher grant.
    pub fn grant_watcher(&mut self) -> bool {
        let Source::Directory { watcher, .. } = &mut self.source else {
            return false;
        };
        watcher.grant();
        true
    }

    /// The watcher's attributed journal when this is a directory endpoint.
    pub fn watcher_audit(
        &self,
    ) -> Option<&chartulary::GraphLog<chartulary::Container, chartulary::Relation>> {
        match &self.source {
            Source::Directory { watcher, .. } => Some(watcher.audit()),
            Source::Fixture(_) | Source::Vault(_) => None,
        }
    }

    pub fn revoke_writes(&mut self) -> bool {
        let had_grant = self.write_grant.take().is_some();
        if had_grant {
            self.effects = None;
            self.derived.clear();
            self.snapshot = None;
            self.resources.clear();
            self.bindings.clear();
        }
        had_grant
    }

    pub fn grant_writes(&mut self, grant: KnotWriteGrant) {
        self.write_grant = Some(grant);
        self.derived.clear();
        if self.effects.is_some() && self.restore_derived_caches().is_err() {
            self.derived.clear();
        }
        self.snapshot = None;
        self.resources.clear();
        self.bindings.clear();
    }

    pub fn grant_effects(&mut self, authority: KnotEffectAuthority) {
        self.effects = Some(authority);
        self.derived.clear();
        if self.restore_derived_caches().is_err() {
            // A cache is never authority. Corruption, a missing epoch, or an
            // incompatible record degrades to a miss.
            self.derived.clear();
        }
        self.snapshot = None;
        self.resources.clear();
        self.bindings.clear();
    }

    pub fn revoke_effects(&mut self) -> bool {
        let had_authority = self.effects.take().is_some();
        if had_authority {
            self.derived.clear();
            self.snapshot = None;
            self.resources.clear();
            self.bindings.clear();
        }
        had_authority
    }

    /// Lock a vault endpoint, dropping its key and decrypted documents.
    pub fn lock_vault(&mut self) -> bool {
        let Source::Vault(source) = &mut self.source else {
            return false;
        };
        source.vault.lock();
        self.derived.clear();
        self.snapshot = None;
        self.resources.clear();
        self.bindings.clear();
        self.clear_rosette_projections();
        true
    }

    /// Unlock a vault endpoint with a recovered root key.
    pub fn unlock_vault(&mut self, key: [u8; 32]) -> Result<bool, String> {
        let Source::Vault(source) = &mut self.source else {
            return Ok(false);
        };
        source.vault.unlock(key)?;
        self.refresh_vault_projection()?;
        if self.effects.is_some() && self.restore_derived_caches().is_err() {
            self.derived.clear();
        }
        Ok(true)
    }

    /// Replace the Commons data-key view after the admitted membership layer
    /// rotates or prunes epochs. Knot owns the keys after handoff and drops
    /// every disclosed/derived resource before re-projecting under them.
    pub fn replace_communal_keys(&mut self, keys: DataKeyring) -> Result<bool, String> {
        let changed = {
            let Source::Vault(VaultSource {
                sync: Some(VaultSyncAuthority::Commons { keys: current, .. }),
                ..
            }) = &mut self.source
            else {
                return Ok(false);
            };
            let changed = current.epoch_ids() != keys.epoch_ids()
                || current.current_epoch() != keys.current_epoch();
            *current = keys;
            changed
        };
        if changed {
            self.derived.clear();
            self.snapshot = None;
            self.resources.clear();
            self.bindings.clear();
            self.clear_rosette_projections();
            self.refresh_vault_projection()?;
            self.sync_source_revision();
        }
        Ok(changed)
    }

    fn refresh(&mut self) -> Result<(), String> {
        if let Source::Directory { source, watcher } = &mut self.source {
            watcher.drain()?;
            if !watcher.is_enabled() {
                return Ok(());
            }
            source
                .refresh()
                .map_err(|error| format!("directory refresh failed: {error}"))?;
        }
        let refresh_vault = matches!(
            &self.source,
            Source::Vault(source) if source.sync.is_some() && !source.vault.is_locked()
        );
        if refresh_vault {
            self.refresh_vault_projection()?;
        }
        self.sync_source_revision();
        Ok(())
    }

    fn documents(&self) -> Vec<PresentedDocument> {
        match &self.source {
            Source::Directory { source, .. } => source
                .documents()
                .map(|document| PresentedDocument {
                    id: document.id.clone(),
                    container: document.container.clone(),
                    byte_size: document.byte_size,
                })
                .collect(),
            Source::Fixture(documents) => documents
                .iter()
                .map(|document| PresentedDocument {
                    id: document.id.clone(),
                    container: document.container.clone(),
                    byte_size: document.byte_size,
                })
                .collect(),
            Source::Vault(source) => {
                let mut documents = source
                    .vault
                    .documents()
                    .map(|document| {
                        let mut container =
                            chartulary::Container::new(format!("knot:vault:{}", document.id))
                                .with_address(format!("knot://vault/{}", document.id))
                                .with_title(document.title.clone());
                        container.media_type = Some(document.media_type.clone());
                        PresentedDocument {
                            id: container.id.clone(),
                            container,
                            byte_size: document.body.len() as u64,
                        }
                    })
                    .collect::<Vec<_>>();
                documents.extend(source.conflicts.iter().map(|id| {
                    let mut container = chartulary::Container::new(format!("knot:vault:{id}"))
                        .with_address(format!("knot://vault/{id}"))
                        .with_title(format!("Conflict: {id}"));
                    container.media_type = Some("text/vnd.knot".into());
                    PresentedDocument {
                        id: container.id.clone(),
                        container,
                        byte_size: 0,
                    }
                }));
                documents.sort_by(|left, right| left.id.cmp(&right.id));
                documents
            }
        }
    }

    /// Install host-owned clip evidence retention. The configured store is the
    /// authority for all paths, limits, and persistence; clip payloads cannot
    /// choose a destination.
    pub fn grant_clip_evidence(&mut self, store: impl KnotClipEvidenceStore + 'static) {
        self.clip_evidence = Some(Box::new(store));
        self.snapshot = None;
        self.resources.clear();
        self.bindings.clear();
    }

    /// Remove clip evidence authority and return to the v1 clip contract.
    pub fn revoke_clip_evidence(&mut self) -> bool {
        let had_authority = self.clip_evidence.take().is_some();
        if had_authority {
            self.snapshot = None;
            self.resources.clear();
            self.bindings.clear();
        }
        had_authority
    }

    fn rosette_documents(&self) -> Vec<PresentedDocument> {
        if matches!(&self.source, Source::Fixture(_)) {
            return Vec::new();
        }
        self.documents()
            .into_iter()
            .filter(|document| {
                document.byte_size <= self.rosette_config.max_source_bytes
                    && document
                        .container
                        .media_type
                        .as_deref()
                        .is_some_and(is_rosette_media_type)
                    && match &self.source {
                        Source::Vault(source) => {
                            let id = document
                                .id
                                .strip_prefix("knot:vault:")
                                .unwrap_or(&document.id);
                            source.vault.body(id).is_some()
                        }
                        Source::Directory { .. } => true,
                        Source::Fixture(_) => false,
                    }
            })
            .collect()
    }

    fn rosette_session(&self, document_id: &str) -> ProjectionSession {
        ProjectionSession(format!(
            "{}:rosette:{}",
            self.session.0,
            blake3::hash(document_id.as_bytes()).to_hex()
        ))
    }

    fn current_rosette_document(&self, session: &ProjectionSession) -> Option<PresentedDocument> {
        self.rosette_documents()
            .into_iter()
            .find(|document| self.rosette_session(&document.id) == *session)
    }

    fn rosette_text(&self, document: &PresentedDocument) -> Result<String, String> {
        let bytes = match &self.source {
            Source::Directory { source, .. } => {
                let path = source
                    .readable_document_path(&document.id)
                    .map_err(|error| format!("could not open Rosette source: {error}"))?;
                fs::read(path).map_err(|error| format!("could not read Rosette source: {error}"))?
            }
            Source::Vault(source) => {
                let id = document
                    .id
                    .strip_prefix("knot:vault:")
                    .unwrap_or(&document.id);
                source
                    .vault
                    .body(id)
                    .ok_or_else(|| {
                        "Rosette source is unavailable while the vault is locked".to_string()
                    })?
                    .to_vec()
            }
            Source::Fixture(_) => return Err("fixture documents have no Rosette source".into()),
        };
        if bytes.len() as u64 > self.rosette_config.max_source_bytes {
            return Err(format!(
                "Rosette source exceeds the configured {} byte limit",
                self.rosette_config.max_source_bytes
            ));
        }
        String::from_utf8(bytes).map_err(|_| "Rosette source is not UTF-8".to_string())
    }

    fn clear_rosette_projections(&mut self) {
        self.rosette_snapshots.clear();
        self.rosette_resources.clear();
        self.rosette_last_announced.clear();
        self.rosette_document_ids.clear();
    }

    fn raw_source_revision(&self) -> u64 {
        match &self.source {
            Source::Directory { source, .. } => source.revision(),
            Source::Fixture(_) => 1,
            Source::Vault(source) => source.vault.revision(),
        }
    }

    fn sync_source_revision(&mut self) {
        let current = self.raw_source_revision();
        if self.observed_source_revision != Some(current) {
            let changed_after_observation = self.observed_source_revision.is_some();
            self.observed_source_revision = Some(current);
            self.scene_revision = Revision(self.scene_revision.0.saturating_add(1).max(1));
            if changed_after_observation {
                self.derived.clear();
            }
        }
    }

    fn advance_derived_revision(&mut self) {
        self.scene_revision = Revision(self.scene_revision.0.saturating_add(1).max(1));
    }

    fn revision(&self) -> Revision {
        self.scene_revision
    }

    fn install_projection(&mut self, projection: KnotDocumentProjection) -> Result<(), String> {
        let Source::Vault(source) = &mut self.source else {
            return Err("Knot sync projection requires a vault source".into());
        };
        source.conflicts = projection
            .conflicts
            .iter()
            .map(|conflict| conflict.id.clone())
            .collect();
        source.document_heads = projection.document_heads;
        source.pending_history = !projection.pending.is_empty();
        source.vault.replace_projection(projection.documents)?;
        Ok(())
    }

    fn refresh_vault_projection(&mut self) -> Result<(), String> {
        let projection = {
            let Source::Vault(source) = &self.source else {
                return Ok(());
            };
            match &source.sync {
                Some(VaultSyncAuthority::Personal { store, .. }) => Some(
                    pollster::block_on(store.projection(&source.vault))
                        .map_err(|error| format!("could not project Knot sync store: {error}"))?,
                ),
                Some(VaultSyncAuthority::Commons { store, keys, .. }) => Some(
                    pollster::block_on(store.communal_projection(keys)).map_err(|error| {
                        format!("could not project Commons Knot store: {error}")
                    })?,
                ),
                None => None,
            }
        };
        if let Some(projection) = projection {
            self.install_projection(projection)?;
        }
        Ok(())
    }

    fn editable_text(&self, document: &PresentedDocument) -> Option<EditableTextV1> {
        let grant = self.write_grant?;
        let address = document.container.primary_address()?.0;
        let media_type = document.container.media_type.clone()?;
        let format = DocumentFormat::from_media_type(&media_type)?;
        match &self.source {
            Source::Directory { source, .. } => {
                let path = source.writable_document_path(&document.id).ok()?;
                if DocumentFormat::from_path(&path) != Some(format) {
                    return None;
                }
                let bytes = fs::read(path).ok()?;
                if bytes.len() as u64 > grant.max_source_bytes {
                    return None;
                }
                let source = String::from_utf8(bytes.clone()).ok()?;
                let base_token = file_base_token(&document.id, &bytes);
                Some(EditableTextV1 {
                    address,
                    media_type,
                    encoding: TextEncoding::Utf8,
                    source,
                    derived: self.derived_text(&document.id, &base_token),
                    base_token,
                })
            }
            Source::Vault(source) => {
                let id = document
                    .id
                    .strip_prefix("knot:vault:")
                    .unwrap_or(&document.id);
                if source.sync.is_none() || source.pending_history || source.conflicts.contains(id)
                {
                    return None;
                }
                let body = source.vault.body(id)?;
                if body.len() as u64 > grant.max_source_bytes {
                    return None;
                }
                let text = String::from_utf8(body.to_vec()).ok()?;
                let head = source.document_heads.get(id)?;
                let base_token = vault_base_token(id, head);
                Some(EditableTextV1 {
                    address,
                    media_type,
                    encoding: TextEncoding::Utf8,
                    source: text,
                    derived: self.derived_text(&document.id, &base_token),
                    base_token,
                })
            }
            Source::Fixture(_) => None,
        }
    }

    fn derived_text(&self, id: &str, base_token: &[u8]) -> Option<DerivedTextV1> {
        self.derived.get(id).and_then(|derived| {
            let cache_is_current = derived
                .cache
                .as_ref()
                .is_none_or(|cache| self.cache_attribution_is_current(cache));
            (derived.base_token == base_token && cache_is_current).then(|| DerivedTextV1 {
                source: derived.document.to_knot(),
                summary: derived.summary.clone(),
                cache: (self.protocol_version.minor >= ProtocolVersion::V1.minor)
                    .then(|| derived.cache.as_ref().map(|cache| cache.info.clone()))
                    .flatten(),
            })
        })
    }

    fn cache_attribution_is_current(&self, cache: &CacheAttribution) -> bool {
        cache.info.source_revision == self.raw_source_revision()
            && (cache.epoch.is_none() || cache.epoch == self.current_commons_epoch())
    }

    fn current_commons_epoch(&self) -> Option<GroupSecretId> {
        match &self.source {
            Source::Vault(VaultSource {
                sync: Some(VaultSyncAuthority::Commons { keys, .. }),
                ..
            }) => keys.current_epoch(),
            _ => None,
        }
    }

    fn restore_derived_caches(&mut self) -> Result<(), String> {
        let Some(effects) = &self.effects else {
            return Ok(());
        };
        if effects.policy.resolve == KnotEffectMode::Never || effects.fetcher.is_none() {
            return Ok(());
        }
        let provider_version = effects
            .fetcher
            .as_ref()
            .expect("checked above")
            .cache_version();
        let policy_fingerprint = resolve_policy_fingerprint(&effects.policy);
        let source_revision = self.raw_source_revision();
        let candidates = self
            .documents()
            .into_iter()
            .filter_map(|document| {
                let editable = self.editable_text(&document)?;
                Some((document.id, editable.base_token))
            })
            .collect::<Vec<_>>();

        let Source::Vault(source) = &self.source else {
            return Ok(());
        };
        for (id, base_token) in candidates {
            let Some(stored) = source.vault.load_derived_cache::<StoredDerivedCache>(&id)? else {
                continue;
            };
            let (record, epoch) = match (&source.sync, stored) {
                (
                    Some(VaultSyncAuthority::Commons { keys, .. }),
                    StoredDerivedCache::Commons(envelope),
                ) => {
                    let current = keys.current_epoch();
                    if current != Some(envelope.epoch) {
                        continue;
                    }
                    let plaintext = match keys.open(&envelope) {
                        Ok(plaintext) => plaintext,
                        Err(_) => continue,
                    };
                    let record = match serde_json::from_slice::<DerivedCacheRecord>(&plaintext) {
                        Ok(record) => record,
                        Err(_) => continue,
                    };
                    (record, Some(envelope.epoch))
                }
                (Some(VaultSyncAuthority::Commons { .. }), _) => continue,
                (_, StoredDerivedCache::Personal(record)) => (record, None),
                (_, StoredDerivedCache::Commons(_)) => continue,
            };
            if record.version != DERIVED_CACHE_RECORD_VERSION
                || record.document_id != id
                || record.base_token != base_token
                || record.info.effect != "resolve"
                || record.info.provider_version != provider_version
                || record.info.policy_fingerprint != policy_fingerprint
                || record.info.source_revision != source_revision
            {
                continue;
            }
            self.derived.insert(
                id,
                DerivedDocument {
                    base_token: record.base_token,
                    document: record.document,
                    summary: record.summary,
                    cache: Some(CacheAttribution {
                        info: record.info,
                        epoch,
                    }),
                },
            );
        }
        Ok(())
    }

    fn persist_derived_cache(&self, id: &str, record: &DerivedCacheRecord) -> Result<(), String> {
        let Source::Vault(source) = &self.source else {
            // Files-in-place have no sealing profile. They retain only the
            // in-memory projection.
            return Ok(());
        };
        let stored = match &source.sync {
            Some(VaultSyncAuthority::Commons { keys, .. }) => {
                let plaintext = serde_json::to_vec(record)
                    .map_err(|error| format!("could not encode Commons derived cache: {error}"))?;
                StoredDerivedCache::Commons(
                    keys.seal_random(&plaintext).map_err(|error| {
                        format!("could not seal Commons derived cache: {error}")
                    })?,
                )
            }
            _ => StoredDerivedCache::Personal(record.clone()),
        };
        source.vault.store_derived_cache(id, &stored)
    }

    fn build_snapshot(&mut self) -> Result<ProjectionSnapshot, String> {
        let documents = self.documents();
        let mut scene = Scene::new();
        let mut presentation = PresentationManifest::default();
        let mut resources = BTreeMap::new();
        let columns = 3usize;
        let card = Size2::new(248.0, 168.0);
        let step_x = 298.0;
        let step_y = 218.0;
        self.bindings.clear();

        for (index, document) in documents.iter().enumerate() {
            let source = scene.intern_source(SourceRef::new(SOURCE_KIND, document.id.clone()));
            let x = 156.0 + (index % columns) as f32 * step_x;
            let y = 146.0 + (index / columns) as f32 * step_y;
            scene.items.push(ProjectedItem {
                source,
                space: Scene::WORLD,
                transform: Transform2::translation(x, y),
                footprint: Footprint::Rect { size: card },
                representation: Representation::Card,
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });

            let title = document.container.title().unwrap_or("Untitled").to_string();
            let address = document
                .container
                .primary_address()
                .map_or_else(String::new, |address| address.0);
            let media_type = document
                .container
                .media_type
                .as_deref()
                .unwrap_or("application/octet-stream")
                .to_string();
            let editable = (self.protocol_version.minor >= ProtocolVersion::V1_2.minor)
                .then(|| self.editable_text(document))
                .flatten();
            let conflicted = match &self.source {
                Source::Vault(source) => source.conflicts.contains(
                    document
                        .id
                        .strip_prefix("knot:vault:")
                        .unwrap_or(&document.id),
                ),
                _ => false,
            };
            let card_payload = PortableCardV1 {
                title: title.clone(),
                values: vec![
                    CardValueV1 {
                        label: "Address".into(),
                        value: address,
                    },
                    CardValueV1 {
                        label: "Type".into(),
                        value: media_type,
                    },
                    CardValueV1 {
                        label: "Size".into(),
                        value: format!("{} bytes", document.byte_size),
                    },
                ],
                badges: match (&self.source, editable.is_some(), conflicted) {
                    (Source::Vault(_), _, true) => {
                        vec!["sealed vault".into(), "conflict".into()]
                    }
                    (Source::Vault(_), true, false) => {
                        vec!["sealed vault".into(), "editable".into()]
                    }
                    (Source::Vault(_), false, false) => {
                        vec!["sealed vault".into(), "read only".into()]
                    }
                    (_, true, _) => vec!["files in place".into(), "editable".into()],
                    (_, false, _) => vec!["files in place".into(), "read only".into()],
                },
                media: Vec::new(),
            };
            let glyph = NativeGlyphV1 {
                label: title.clone(),
                icon: Some("◇".into()),
                color: Some("#88a889".into()),
            };
            let card_bytes = serde_json::to_vec(&card_payload)
                .map_err(|error| format!("could not encode card: {error}"))?;
            let glyph_bytes = serde_json::to_vec(&glyph)
                .map_err(|error| format!("could not encode glyph: {error}"))?;
            let card_hash = ContentHash::of(&card_bytes);
            let glyph_hash = ContentHash::of(&glyph_bytes);
            resources.insert(card_hash, card_bytes.clone());
            resources.insert(glyph_hash, glyph_bytes.clone());
            let key = PresentationKey(document.id.clone());
            let semantics = PresentationSemantics {
                label: title,
                role: SemanticRole::Article,
                bounds: BoundsRelationship::FillFootprint,
                actions: Vec::new(),
            };
            presentation.bindings.push(PresentationBinding {
                instance: InstanceId(index as u32),
                key: key.clone(),
            });
            self.bindings.insert(index as u32, document.id.clone());
            let mut offers = Vec::new();
            if let Some(editable) = editable {
                let editable_bytes = serde_json::to_vec(&editable)
                    .map_err(|error| format!("could not encode editable text: {error}"))?;
                let editable_hash = ContentHash::of(&editable_bytes);
                resources.insert(editable_hash, editable_bytes.clone());
                let mut editable_semantics = semantics.clone();
                editable_semantics.actions.push(AdvertisedAction {
                    intent: IntentReference(EDITABLE_TEXT_SAVE_INTENT.into()),
                    label: "Save".into(),
                    explanation: "Write this document through Knot authority.".into(),
                    payload_schema: EDITABLE_TEXT_SAVE_SCHEMA.into(),
                    input_form: None,
                    effect: IntentEffect::DomainTruth,
                });
                editable_semantics.actions.push(AdvertisedAction {
                    intent: IntentReference(KNOT_CLIP_INSERT_INTENT.into()),
                    label: "Insert clip".into(),
                    explanation: if self.clip_evidence.is_some() {
                        "Retain observed source bytes and append a semantic clip with content-addressed provenance through Knot authority."
                    } else {
                        "Append a semantic clip with structured source provenance through Knot authority."
                    }
                    .into(),
                    payload_schema: if self.clip_evidence.is_some() {
                        KNOT_CLIP_INSERT_SCHEMA_V2
                    } else {
                        KNOT_CLIP_INSERT_SCHEMA
                    }
                    .into(),
                    input_form: None,
                    effect: IntentEffect::DomainTruth,
                });
                if self.effects.as_ref().is_some_and(|effects| {
                    effects.policy.resolve != KnotEffectMode::Never && effects.fetcher.is_some()
                }) {
                    editable_semantics.actions.push(AdvertisedAction {
                        intent: IntentReference(KNOT_TRANSCLUSION_RESOLVE_INTENT.into()),
                        label: "Resolve".into(),
                        explanation:
                            "Fetch admitted include fences into a temporary derived preview."
                                .into(),
                        payload_schema: KNOT_TRANSCLUSION_RESOLVE_SCHEMA.into(),
                        input_form: None,
                        effect: IntentEffect::ExternalEffect,
                    });
                }
                if self.effects.as_ref().is_some_and(|effects| {
                    effects.policy.run != KnotEffectMode::Never
                        && effects.evaluators.languages().into_iter().any(|language| {
                            effects
                                .policy
                                .allowed_languages
                                .iter()
                                .any(|allowed| allowed == language)
                        })
                }) {
                    editable_semantics.actions.push(AdvertisedAction {
                        intent: IntentReference(KNOT_BLOCK_RUN_INTENT.into()),
                        label: "Run".into(),
                        explanation:
                            "Evaluate admitted code fences into a temporary derived preview."
                                .into(),
                        payload_schema: KNOT_BLOCK_RUN_SCHEMA.into(),
                        input_form: None,
                        effect: IntentEffect::ExternalEffect,
                    });
                }
                offers.push(PresentationOffer {
                    codec: PresentationCodec::EditableTextV1,
                    resource: editable_hash,
                    byte_size: editable_bytes.len() as u64,
                    requires: PresentationCapability::EditableText,
                    semantics: editable_semantics,
                });
            }
            offers.extend([
                PresentationOffer {
                    codec: PresentationCodec::PortableCardV1,
                    resource: card_hash,
                    byte_size: card_bytes.len() as u64,
                    requires: PresentationCapability::PortableCard,
                    semantics: semantics.clone(),
                },
                PresentationOffer {
                    codec: PresentationCodec::NativeGlyphV1,
                    resource: glyph_hash,
                    byte_size: glyph_bytes.len() as u64,
                    requires: PresentationCapability::NativeGlyph,
                    semantics,
                },
            ]);
            presentation.offers.insert(key, offers);
        }

        let rows = documents.len().div_ceil(columns).max(1);
        scene.bounds = Rect::new(
            Vec2::new(32.0, 62.0),
            Size2::new(
                card.w + step_x * columns.saturating_sub(1) as f32,
                card.h + step_y * rows.saturating_sub(1) as f32,
            ),
        );
        scene.generation = self.revision().0;
        let scene = SceneSnapshot::from_dense(SceneEpoch(1), self.revision(), scene)
            .map_err(|error| format!("invalid Knot scene: {error:?}"))?;
        self.resources = resources;
        let snapshot = ProjectionSnapshot {
            version: self.protocol_version,
            session: self.session.clone(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        };
        self.last_announced = Some(snapshot.scene.revision);
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn build_rosette_snapshot(
        &mut self,
        session: ProjectionSession,
        document: PresentedDocument,
        version: ProtocolVersion,
    ) -> Result<ProjectionSnapshot, String> {
        let text = self.rosette_text(&document)?;
        let projection = project_rosette(
            SourceRef::new(SOURCE_KIND, document.id.clone()),
            &text,
            &CmudictPronunciations,
            self.rosette_config.geometry,
        );
        let document_title = document.container.title().unwrap_or("Untitled").to_string();
        let mut presentation = PresentationManifest::default();
        let mut resources = BTreeMap::new();

        for interior in &projection.interiors {
            let source = text
                .get(interior.byte_start..interior.byte_end)
                .ok_or_else(|| "Rosette interior does not address its source".to_string())?;
            let (kind, glyph) = match interior.kind {
                RosetteInteriorKind::Line => ("Line", "♪"),
                RosetteInteriorKind::Stanza => ("Stanza", "✦"),
            };
            let unresolved = projection
                .coverage
                .unresolved
                .iter()
                .filter(|token| {
                    token.byte_start >= interior.byte_start && token.byte_end <= interior.byte_end
                })
                .count();
            let label = match interior.kind {
                RosetteInteriorKind::Line => presentation_excerpt(source),
                RosetteInteriorKind::Stanza => format!("Stanza {}", interior.ordinal + 1),
            };
            let mut badges = vec!["Rosette".into(), kind.to_ascii_lowercase()];
            if unresolved > 0 {
                badges.push(format!("{unresolved} unknown"));
            }
            let card = PortableCardV1 {
                title: label.clone(),
                values: vec![
                    CardValueV1 {
                        label: "Document".into(),
                        value: document_title.clone(),
                    },
                    CardValueV1 {
                        label: "Interior".into(),
                        value: format!(
                            "{kind} {} · bytes {}..{}",
                            interior.ordinal + 1,
                            interior.byte_start,
                            interior.byte_end
                        ),
                    },
                    CardValueV1 {
                        label: "Text".into(),
                        value: presentation_excerpt(source),
                    },
                    CardValueV1 {
                        label: "Lexicon".into(),
                        value: format!(
                            "{} of {} tokens resolved",
                            projection.coverage.resolved_tokens, projection.coverage.total_tokens
                        ),
                    },
                ],
                badges,
                media: Vec::new(),
            };
            let glyph = NativeGlyphV1 {
                label: label.clone(),
                icon: Some(glyph.into()),
                color: Some("#d8a657".into()),
            };
            let card_bytes = serde_json::to_vec(&card)
                .map_err(|error| format!("could not encode Rosette card: {error}"))?;
            let glyph_bytes = serde_json::to_vec(&glyph)
                .map_err(|error| format!("could not encode Rosette glyph: {error}"))?;
            let card_hash = ContentHash::of(&card_bytes);
            let glyph_hash = ContentHash::of(&glyph_bytes);
            resources.insert(card_hash, card_bytes.clone());
            resources.insert(glyph_hash, glyph_bytes.clone());
            let key = PresentationKey(format!(
                "{}:rosette:{}:{}",
                document.id, kind, interior.ordinal
            ));
            let semantics = PresentationSemantics {
                label,
                role: SemanticRole::Article,
                bounds: BoundsRelationship::FillFootprint,
                actions: Vec::new(),
            };
            presentation.bindings.push(PresentationBinding {
                instance: interior.instance,
                key: key.clone(),
            });
            presentation.offers.insert(
                key,
                vec![
                    PresentationOffer {
                        codec: PresentationCodec::PortableCardV1,
                        resource: card_hash,
                        byte_size: card_bytes.len() as u64,
                        requires: PresentationCapability::PortableCard,
                        semantics: semantics.clone(),
                    },
                    PresentationOffer {
                        codec: PresentationCodec::NativeGlyphV1,
                        resource: glyph_hash,
                        byte_size: glyph_bytes.len() as u64,
                        requires: PresentationCapability::NativeGlyph,
                        semantics,
                    },
                ],
            );
        }

        let scene = SceneSnapshot::from_dense(SceneEpoch(1), self.revision(), projection.scene)
            .map_err(|error| format!("invalid Knot Rosette scene: {error:?}"))?;
        let snapshot = ProjectionSnapshot {
            version,
            session: session.clone(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        };
        self.rosette_resources.insert(session.clone(), resources);
        self.rosette_last_announced
            .insert(session.clone(), snapshot.scene.revision);
        self.rosette_document_ids
            .insert(session.clone(), document.id.clone());
        self.rosette_snapshots.insert(session, snapshot.clone());
        Ok(snapshot)
    }

    fn build_empty_rosette_snapshot(
        &mut self,
        session: ProjectionSession,
        version: ProtocolVersion,
    ) -> Result<ProjectionSnapshot, String> {
        let mut scene = Scene::new();
        scene.generation = self.revision().0;
        let scene = SceneSnapshot::from_dense(SceneEpoch(1), self.revision(), scene)
            .map_err(|error| format!("invalid empty Knot Rosette scene: {error:?}"))?;
        let snapshot = ProjectionSnapshot {
            version,
            session: session.clone(),
            scene,
            presentation: PresentationManifest::default(),
            cache_policy: CachePolicy::default(),
        };
        self.rosette_resources
            .insert(session.clone(), BTreeMap::new());
        self.rosette_last_announced
            .insert(session.clone(), snapshot.scene.revision);
        self.rosette_snapshots.insert(session, snapshot.clone());
        Ok(snapshot)
    }

    fn save_text(&mut self, id: &str, payload: SaveTextV1) -> Result<IntentResult, String> {
        let grant = self
            .write_grant
            .ok_or_else(|| "Knot session has no write grant".to_string())?;
        if payload.source.len() as u64 > grant.max_source_bytes {
            return Ok(IntentResult::Rejected {
                reason: format!(
                    "source exceeds this grant's {} byte limit",
                    grant.max_source_bytes
                ),
            });
        }
        let document = self
            .documents()
            .into_iter()
            .find(|document| document.id == id)
            .ok_or_else(|| "intent target is no longer present".to_string())?;
        let Some(current) = self.editable_text(&document) else {
            return Ok(IntentResult::Rejected {
                reason: "this document is not currently writable".into(),
            });
        };
        if payload.base_token != current.base_token {
            return Ok(self.stale_result());
        }
        if payload.source == current.source {
            return Ok(IntentResult::Accepted);
        }
        let format = DocumentFormat::from_media_type(&current.media_type)
            .ok_or_else(|| "document format is not authorable".to_string())?;
        format.validate_source(&current.address, &payload.source)?;

        let vault_projection = match &mut self.source {
            Source::Directory { source, .. } => {
                let path = source
                    .writable_document_path(id)
                    .map_err(|error| format!("document target is not writable: {error}"))?;
                let before = fs::read(&path)
                    .map_err(|error| format!("could not re-read document before save: {error}"))?;
                if file_base_token(id, &before) != payload.base_token {
                    return Ok(self.stale_result());
                }
                crate::writer::write_if_distinct(&path, &before, payload.source.as_bytes())?;
                source
                    .refresh()
                    .map_err(|error| format!("directory refresh failed after save: {error}"))?;
                None
            }
            Source::Vault(source) => {
                let native_id = id.strip_prefix("knot:vault:").unwrap_or(id);
                let Some(previous) = source
                    .vault
                    .documents()
                    .find(|document| document.id == native_id)
                    .cloned()
                else {
                    return Err("vault document is no longer present".into());
                };
                let event = KnotSyncEvent::Put(VaultDocument {
                    id: previous.id,
                    title: previous.title,
                    body: payload.source.into_bytes(),
                    media_type: previous.media_type,
                });
                let projection = match source
                    .sync
                    .as_ref()
                    .ok_or_else(|| "vault has no admitted sync author".to_string())?
                {
                    VaultSyncAuthority::Personal {
                        store,
                        signing_seed,
                    } => {
                        pollster::block_on(store.author(**signing_seed, &source.vault, &event))
                            .map_err(|error| format!("could not author Knot save: {error}"))?;
                        pollster::block_on(store.projection(&source.vault))
                            .map_err(|error| format!("could not project Knot save: {error}"))?
                    }
                    VaultSyncAuthority::Commons {
                        store,
                        signing_seed,
                        keys,
                    } => {
                        pollster::block_on(store.author_communal(**signing_seed, keys, &event))
                            .map_err(|error| {
                                format!("could not author Commons Knot save: {error}")
                            })?;
                        pollster::block_on(store.communal_projection(keys)).map_err(|error| {
                            format!("could not project Commons Knot save: {error}")
                        })?
                    }
                };
                Some(projection)
            }
            Source::Fixture(_) => {
                return Ok(IntentResult::Rejected {
                    reason: "fixture documents are read-only".into(),
                });
            }
        };
        if let Some(projection) = vault_projection {
            self.install_projection(projection)?;
        }

        self.sync_source_revision();
        let announced = self.last_announced;
        self.build_snapshot()?;
        self.last_announced = announced;
        Ok(IntentResult::Accepted)
    }

    fn insert_clip(&mut self, id: &str, payload: InsertKnotClipV1) -> Result<IntentResult, String> {
        if let Some(rejected) = validate_clip_header(
            &payload.source_url,
            payload.title.as_deref(),
            &payload.knot_body,
        ) {
            return Ok(rejected);
        }
        if payload
            .selector
            .as_ref()
            .is_some_and(|selector| selector.len() > 4096)
        {
            return Ok(IntentResult::Rejected {
                reason: "clip selector exceeds 4096 bytes".into(),
            });
        }

        let provenance = serde_json::json!({
            "schema": KNOT_CLIP_INSERT_SCHEMA,
            "source_url": payload.source_url,
            "title": payload.title,
            "selector": payload.selector,
        });
        self.append_clip(id, payload.base_token, payload.knot_body, provenance)
    }

    fn insert_clip_v2(
        &mut self,
        id: &str,
        payload: InsertKnotClipV2,
    ) -> Result<IntentResult, String> {
        if let Some(rejected) = validate_clip_header(
            &payload.source_url,
            payload.title.as_deref(),
            &payload.knot_body,
        ) {
            return Ok(rejected);
        }
        if payload.artifacts.is_empty() || payload.artifacts.len() > 2 {
            return Ok(IntentResult::Rejected {
                reason: "evidence-bearing clips require one or two source artifacts".into(),
            });
        }
        if payload.selectors.len() > 16
            || payload.fidelity.len() > 256
            || payload.discovered_edges.len() > 2048
        {
            return Ok(IntentResult::Rejected {
                reason: "clip evidence exceeds selector, fidelity, or edge count limits".into(),
            });
        }
        let structured_bytes = serde_json::to_vec(&(
            &payload.selectors,
            &payload.fidelity,
            &payload.discovered_edges,
        ))
        .map_err(|error| format!("could not validate structured clip evidence: {error}"))?;
        if structured_bytes.len() > 256 * 1024 {
            return Ok(IntentResult::Rejected {
                reason: "structured clip evidence exceeds 262144 bytes".into(),
            });
        }
        for artifact in &payload.artifacts {
            if artifact.media_type.is_empty()
                || artifact.media_type.len() > 256
                || artifact.canonical_uri.is_empty()
                || artifact.canonical_uri.len() > 8 * 1024
                || !has_absolute_uri_scheme(&artifact.canonical_uri)
            {
                return Ok(IntentResult::Rejected {
                    reason: "clip artifact metadata is invalid".into(),
                });
            }
        }
        if payload
            .selectors
            .iter()
            .any(|selector| !selector_matches_artifacts(selector, &payload.artifacts))
            || payload.fidelity.iter().any(|entry| {
                entry.selector.as_ref().is_some_and(|selector| {
                    !selector_matches_artifacts(selector, &payload.artifacts)
                })
            })
        {
            return Ok(IntentResult::Rejected {
                reason: "clip selector names an artifact role the clip did not retain".into(),
            });
        }

        // Check the revision before retaining bytes. A stale gesture must not
        // grow the evidence store.
        let current = match self.current_clip_target(id, &payload.base_token)? {
            Ok(current) => current,
            Err(result) => return Ok(result),
        };
        let Some(store) = self.clip_evidence.as_mut() else {
            return Ok(IntentResult::Rejected {
                reason: "this Knot endpoint has no clip evidence authority".into(),
            });
        };
        let mut evidence: Vec<KnotClipEvidenceRef> = Vec::with_capacity(payload.artifacts.len());
        for artifact in &payload.artifacts {
            match store.retain(artifact) {
                Ok(reference) => evidence.push(reference),
                Err(reason) => return Ok(IntentResult::Rejected { reason }),
            }
        }
        let provenance = serde_json::json!({
            "schema": KNOT_CLIP_INSERT_SCHEMA_V2,
            "source_url": payload.source_url,
            "title": payload.title,
            "selectors": payload.selectors,
            "evidence": evidence,
            "fidelity": payload.fidelity,
            "discovered_edges": payload.discovered_edges,
        });
        self.append_clip_to_current(
            id,
            payload.base_token,
            payload.knot_body,
            provenance,
            current,
        )
    }

    fn append_clip(
        &mut self,
        id: &str,
        base_token: Vec<u8>,
        knot_body: String,
        provenance: serde_json::Value,
    ) -> Result<IntentResult, String> {
        let current = match self.current_clip_target(id, &base_token)? {
            Ok(current) => current,
            Err(result) => return Ok(result),
        };
        self.append_clip_to_current(id, base_token, knot_body, provenance, current)
    }

    fn current_clip_target(
        &mut self,
        id: &str,
        base_token: &[u8],
    ) -> Result<Result<EditableTextV1, IntentResult>, String> {
        let document = self
            .documents()
            .into_iter()
            .find(|document| document.id == id)
            .ok_or_else(|| "intent target is no longer present".to_string())?;
        let Some(current) = self.editable_text(&document) else {
            return Ok(Err(IntentResult::Rejected {
                reason: "this document is not currently writable".into(),
            }));
        };
        if base_token != current.base_token {
            return Ok(Err(self.stale_result()));
        }
        Ok(Ok(current))
    }

    fn append_clip_to_current(
        &mut self,
        id: &str,
        base_token: Vec<u8>,
        knot_body: String,
        provenance: serde_json::Value,
        current: EditableTextV1,
    ) -> Result<IntentResult, String> {
        let provenance = serde_json::to_string(&provenance)
            .map_err(|error| format!("could not encode clip provenance: {error}"))?;
        let mut source = current.source.trim_end().to_string();
        if !source.is_empty() {
            source.push_str("\n\n");
        }
        source.push_str("```knot.clip.provenance\n");
        source.push_str(&provenance);
        source.push_str("\n```\n\n");
        source.push_str(knot_body.trim());
        source.push('\n');

        self.save_text(id, SaveTextV1 { base_token, source })
    }

    fn resolve_transclusions(
        &mut self,
        id: &str,
        payload: KnotEffectV1,
    ) -> Result<IntentResult, String> {
        let Some((current, mut document)) = self.effect_input(id, &payload.base_token, false)?
        else {
            return Ok(self.stale_result());
        };
        let mode = self
            .effects
            .as_ref()
            .map(|effects| effects.policy.resolve)
            .unwrap_or(KnotEffectMode::Never);
        if let Some(rejected) = self.check_effect_consent(mode, payload.confirmed) {
            return Ok(rejected);
        }
        if document.trust == DocumentTrustState::Broken {
            return Ok(IntentResult::Rejected {
                reason: "Knot refuses effects for a document with broken trust".into(),
            });
        }

        let source_revision = self.raw_source_revision();
        let encryption_epoch = self.current_commons_epoch();
        let has_sealed_cache = matches!(&self.source, Source::Vault(_));
        let retained_cache = self.derived.get(id).is_some_and(|derived| {
            derived.base_token == current.base_token
                && derived
                    .cache
                    .as_ref()
                    .is_some_and(|cache| self.cache_attribution_is_current(cache))
        });
        let (outcome, sources, provider_version, policy_fingerprint) = {
            let effects = self
                .effects
                .as_mut()
                .ok_or_else(|| "Knot session has no effect authority".to_string())?;
            let provider_version = effects
                .fetcher
                .as_ref()
                .ok_or_else(|| "Knot session has no transclusion fetcher".to_string())?
                .cache_version();
            let policy_fingerprint = resolve_policy_fingerprint(&effects.policy);
            let fetcher = effects
                .fetcher
                .as_mut()
                .ok_or_else(|| "Knot session has no transclusion fetcher".to_string())?;
            let policy = TransclusionPolicy::for_own_notes(
                effects.policy.allowed_schemes.clone(),
                effects.policy.max_depth,
            );
            let mut sources = Vec::new();
            let mut fetch = |address: &str| {
                let fetched = fetcher.fetch(address)?;
                sources.push(address.to_string());
                Ok(fetched)
            };
            let mut render = render_effect_input;
            let outcome = resolve_transclusions(&mut document, &mut fetch, &mut render, &policy);
            sources.sort();
            sources.dedup();
            (outcome, sources, provider_version, policy_fingerprint)
        };
        let summary = format!(
            "resolved {}; denied {}; failed {}",
            outcome.resolved,
            outcome.denied.len(),
            outcome.failed.len()
        );
        if retained_cache && outcome.resolved == 0 && !outcome.failed.is_empty() {
            return Ok(IntentResult::Rejected {
                reason: format!(
                    "resolve refresh failed; retained cached result ({} failure(s))",
                    outcome.failed.len()
                ),
            });
        }
        let cache = (has_sealed_cache && outcome.resolved > 0).then(|| CacheAttribution {
            info: DerivedCacheInfoV1 {
                effect: "resolve".into(),
                sources,
                provider_version,
                policy_fingerprint,
                fetched_at_unix_ms: unix_time_ms(),
                source_revision,
            },
            epoch: encryption_epoch,
        });
        self.accept_derived(id, current.base_token, document, summary, cache, true)
    }

    fn run_blocks(&mut self, id: &str, payload: KnotEffectV1) -> Result<IntentResult, String> {
        let Some((current, mut document)) = self.effect_input(id, &payload.base_token, true)?
        else {
            return Ok(self.stale_result());
        };
        let mode = self
            .effects
            .as_ref()
            .map(|effects| effects.policy.run)
            .unwrap_or(KnotEffectMode::Never);
        if let Some(rejected) = self.check_effect_consent(mode, payload.confirmed) {
            return Ok(rejected);
        }
        if document.trust == DocumentTrustState::Broken {
            return Ok(IntentResult::Rejected {
                reason: "Knot refuses effects for a document with broken trust".into(),
            });
        }

        let effects = self
            .effects
            .as_mut()
            .ok_or_else(|| "Knot session has no effect authority".to_string())?;
        let policy = EvaluationPolicy::for_own_notes(effects.policy.allowed_languages.clone());
        let max_ops = effects.policy.max_ops;
        let evaluators = &mut effects.evaluators;
        let mut evaluate =
            |language: &str, source: &str| evaluators.evaluate(language, source, max_ops);
        let mut render = render_effect_input;
        let outcome = evaluate_blocks(&mut document, &mut evaluate, &mut render, &policy);
        let summary = format!(
            "ran {}; denied {}; failed {}",
            outcome.evaluated,
            outcome.denied.len(),
            outcome.failed.len()
        );
        // Evaluation providers do not yet expose a cacheability contract.
        // The evaluated document is therefore process-local even when its
        // input began as a separately cached resolve result.
        self.accept_derived(id, current.base_token, document, summary, None, false)
    }

    fn effect_input(
        &self,
        id: &str,
        base_token: &[u8],
        use_derived: bool,
    ) -> Result<Option<(EditableTextV1, EngineDocument)>, String> {
        let document = self
            .documents()
            .into_iter()
            .find(|document| document.id == id)
            .ok_or_else(|| "intent target is no longer present".to_string())?;
        let current = self
            .editable_text(&document)
            .ok_or_else(|| "this document is not currently writable".to_string())?;
        if base_token != current.base_token {
            return Ok(None);
        }
        let derived = use_derived
            .then(|| {
                self.derived
                    .get(id)
                    .filter(|derived| derived.base_token == current.base_token)
                    .map(|derived| derived.document.clone())
            })
            .flatten();
        let document = match derived {
            Some(document) => document,
            None => render_effect_input(
                &EngineInput::new(current.address.clone(), current.source.clone())
                    .with_content_type(current.media_type.clone()),
            )?,
        };
        Ok(Some((current, document)))
    }

    fn check_effect_consent(&self, mode: KnotEffectMode, confirmed: bool) -> Option<IntentResult> {
        if mode == KnotEffectMode::Never {
            return Some(IntentResult::Rejected {
                reason: "this effect is disabled by Knot policy".into(),
            });
        }
        let received = matches!(
            &self.source,
            Source::Vault(VaultSource {
                sync: Some(VaultSyncAuthority::Commons { .. }),
                ..
            })
        );
        if !confirmed && (mode == KnotEffectMode::Ask || received) {
            return Some(IntentResult::Rejected {
                reason: if received {
                    "received Commons documents require explicit effect confirmation".into()
                } else {
                    "this effect requires explicit confirmation".into()
                },
            });
        }
        None
    }

    fn accept_derived(
        &mut self,
        id: &str,
        base_token: Vec<u8>,
        document: EngineDocument,
        summary: String,
        cache: Option<CacheAttribution>,
        persist: bool,
    ) -> Result<IntentResult, String> {
        if persist && let Some(cache) = &cache {
            self.persist_derived_cache(
                id,
                &DerivedCacheRecord {
                    version: DERIVED_CACHE_RECORD_VERSION,
                    document_id: id.to_string(),
                    base_token: base_token.clone(),
                    document: document.clone(),
                    summary: summary.clone(),
                    info: cache.info.clone(),
                },
            )?;
        }
        self.derived.insert(
            id.to_string(),
            DerivedDocument {
                base_token,
                document,
                summary,
                cache,
            },
        );
        self.advance_derived_revision();
        let announced = self.last_announced;
        self.build_snapshot()?;
        self.last_announced = announced;
        Ok(IntentResult::Accepted)
    }

    fn stale_result(&self) -> IntentResult {
        let (current_epoch, current_revision) = self
            .snapshot
            .as_ref()
            .map(|snapshot| (snapshot.scene.epoch, snapshot.scene.revision))
            .unwrap_or((SceneEpoch(1), self.revision()));
        IntentResult::Stale {
            current_epoch,
            current_revision,
        }
    }

    fn validate_request(&self, request: &ProjectionRequest) -> Result<(), String> {
        if request.session != self.session {
            return Err("projection request names the wrong Knot session".into());
        }
        self.validate_projection_contract(request)
    }

    fn validate_projection_contract(&self, request: &ProjectionRequest) -> Result<(), String> {
        if request.version.major != ProtocolVersion::V1.major {
            return Err("projection request uses an unsupported protocol".into());
        }
        if request.version.minor > ProtocolVersion::V1.minor {
            return Err("projection request uses a newer unsupported protocol minor".into());
        }
        if request.score.version != sceno::SCORE_VERSION {
            return Err("projection request uses an unsupported score".into());
        }
        Ok(())
    }
}

fn selector_matches_artifacts(
    selector: &KnotClipSelectorV1,
    artifacts: &[KnotClipArtifactV1],
) -> bool {
    let role = match selector {
        KnotClipSelectorV1::TextQuote { artifact_role, .. }
        | KnotClipSelectorV1::TextPosition { artifact_role, .. } => *artifact_role,
        KnotClipSelectorV1::DomRange { artifact_role, .. } => {
            if *artifact_role != KnotClipArtifactRoleV1::ObservedRepresentation {
                return false;
            }
            *artifact_role
        }
    };
    artifacts.iter().any(|artifact| artifact.role == role)
}

fn render_effect_input(input: &EngineInput) -> Result<EngineDocument, String> {
    let media_type = input.content_type.as_deref();
    let address = input.address.to_ascii_lowercase();
    let engine: Box<dyn Engine> = match media_type {
        Some("text/gemini") => Box::new(nematic::GemtextEngine::new()),
        Some("text/markdown") => Box::new(nematic::MarkdownEngine::new()),
        Some("text/html" | "application/xhtml+xml") => Box::new(nematic::HtmlFragmentEngine::new()),
        Some("text/x-knot" | "text/vnd.knot") => Box::new(nematic::KnotEngine::new()),
        Some("text/plain") => Box::new(nematic::TextEngine::new()),
        _ if address.ends_with(".gmi") || address.ends_with(".gemini") => {
            Box::new(nematic::GemtextEngine::new())
        }
        _ if address.ends_with(".md") || address.ends_with(".markdown") => {
            Box::new(nematic::MarkdownEngine::new())
        }
        _ if address.ends_with(".html") || address.ends_with(".htm") => {
            Box::new(nematic::HtmlFragmentEngine::new())
        }
        _ if address.ends_with(".knot") => Box::new(nematic::KnotEngine::new()),
        _ => Box::new(nematic::TextEngine::new()),
    };
    engine.render(input).map_err(|error| error.to_string())
}

fn resolve_policy_fingerprint(policy: &KnotEffectPolicy) -> String {
    let mut schemes = policy.allowed_schemes.clone();
    schemes.sort();
    schemes.dedup();
    let mut hasher = blake3::Hasher::new_derive_key("mere.knot.resolve-cache-policy.v1");
    hasher.update(&[match policy.resolve {
        KnotEffectMode::Auto => 0,
        KnotEffectMode::Ask => 1,
        KnotEffectMode::Never => 2,
    }]);
    hasher.update(&[policy.max_depth]);
    for scheme in schemes {
        hasher.update(&(scheme.len() as u64).to_le_bytes());
        hasher.update(scheme.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn file_base_token(id: &str, bytes: &[u8]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new_derive_key(FILE_TOKEN_CONTEXT);
    hasher.update(&(id.len() as u64).to_le_bytes());
    hasher.update(id.as_bytes());
    hasher.update(blake3::hash(bytes).as_bytes());
    hasher.finalize().as_bytes().to_vec()
}

fn vault_base_token(id: &str, operation: &[u8; 32]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new_derive_key(VAULT_TOKEN_CONTEXT);
    hasher.update(&(id.len() as u64).to_le_bytes());
    hasher.update(id.as_bytes());
    hasher.update(operation);
    hasher.finalize().as_bytes().to_vec()
}

fn validate_clip_header(
    source_url: &str,
    title: Option<&str>,
    knot_body: &str,
) -> Option<IntentResult> {
    if source_url.is_empty()
        || source_url.len() > 8 * 1024
        || source_url.chars().any(char::is_control)
        || !has_absolute_uri_scheme(source_url)
    {
        return Some(IntentResult::Rejected {
            reason: "clip source_url must be an absolute URI of at most 8192 bytes".into(),
        });
    }
    if title.is_some_and(|title| title.len() > 1024) {
        return Some(IntentResult::Rejected {
            reason: "clip title exceeds 1024 bytes".into(),
        });
    }
    if knot_body.trim().is_empty() {
        return Some(IntentResult::Rejected {
            reason: "clip contains no semantic Knot body".into(),
        });
    }
    None
}

fn has_absolute_uri_scheme(address: &str) -> bool {
    let Some((scheme, _)) = address.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn is_rosette_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "text/plain" | "text/markdown" | "text/djot" | "text/vnd.knot"
    )
}

fn presentation_excerpt(source: &str) -> String {
    let mut excerpt = source.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_CHARS: usize = 160;
    if let Some((byte, _)) = excerpt.char_indices().nth(MAX_CHARS) {
        excerpt.truncate(byte);
        excerpt.push('…');
    }
    excerpt
}

impl ProjectionCatalog for KnotEndpoint {
    fn describe(&self) -> EndpointDescriptor {
        let mut projections = vec![ProjectionOffer {
            label: match &self.source {
                Source::Directory { .. } => "Files in place".into(),
                Source::Fixture(_) => "Authoring fixture".into(),
                Source::Vault(_) => "Sealed vault".into(),
            },
            request: ProjectionRequest {
                version: ProtocolVersion::V1,
                session: self.session.clone(),
                score: Score::new(Arrangement::Spiral(Default::default())),
            },
        }];
        projections.extend(self.rosette_documents().into_iter().map(|document| {
            let title = document.container.title().unwrap_or("Untitled");
            ProjectionOffer {
                label: format!("Rosette · {title}"),
                request: ProjectionRequest {
                    version: ProtocolVersion::V1,
                    session: self.rosette_session(&document.id),
                    score: Score::new(Arrangement::Spiral(Default::default())),
                },
            }
        }));
        EndpointDescriptor {
            label: "Knot".into(),
            projections,
        }
    }
}

impl ProjectionSource for KnotEndpoint {
    type Error = String;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session == self.session {
            self.validate_request(&request)?;
            self.protocol_version = request.version;
            self.refresh()?;
            return self.build_snapshot();
        }
        self.validate_projection_contract(&request)?;
        self.refresh()?;
        let document = self
            .current_rosette_document(&request.session)
            .ok_or_else(|| "projection request names an unavailable Knot Rosette".to_string())?;
        self.build_rosette_snapshot(request.session, document, request.version)
    }
}

impl ResumableProjectionSource for KnotEndpoint {
    type Error = String;

    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, Self::Error> {
        self.refresh()?;
        let current = self.revision();
        if request.session == self.session {
            if request.epoch == SceneEpoch(1) && request.revision == current {
                self.last_announced = Some(current);
                return Ok(ResumeReply::Current(ProjectionAck {
                    session: self.session.clone(),
                    epoch: SceneEpoch(1),
                    revision: current,
                }));
            }
            return Ok(ResumeReply::Snapshot(Box::new(self.build_snapshot()?)));
        }

        let document = self.current_rosette_document(&request.session);
        if document.is_none() && !self.rosette_document_ids.contains_key(&request.session) {
            return Err("resume request names an unavailable Knot Rosette".into());
        }
        if request.epoch == SceneEpoch(1) && request.revision == current {
            self.rosette_last_announced
                .insert(request.session.clone(), current);
            return Ok(ResumeReply::Current(ProjectionAck {
                session: request.session,
                epoch: SceneEpoch(1),
                revision: current,
            }));
        }
        let version = self
            .rosette_snapshots
            .get(&request.session)
            .map_or(ProtocolVersion::V1, |snapshot| snapshot.version);
        let snapshot = match document {
            Some(document) => self.build_rosette_snapshot(request.session, document, version)?,
            None => self.build_empty_rosette_snapshot(request.session, version)?,
        };
        Ok(ResumeReply::Snapshot(Box::new(snapshot)))
    }
}

impl ProjectionNoticeSource for KnotEndpoint {
    type Error = String;

    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error> {
        self.refresh()?;
        let revision = self.revision();
        if self.snapshot.is_some()
            && self
                .last_announced
                .is_none_or(|announced| revision > announced)
        {
            self.last_announced = Some(revision);
            return Ok(Some(CarrierNotice {
                session: self.session.clone(),
                epoch: SceneEpoch(1),
                revision,
            }));
        }
        let pending = self.rosette_snapshots.keys().find(|session| {
            self.rosette_last_announced
                .get(*session)
                .is_none_or(|announced| revision > *announced)
        });
        let Some(session) = pending.cloned() else {
            return Ok(None);
        };
        self.rosette_last_announced
            .insert(session.clone(), revision);
        Ok(Some(CarrierNotice {
            session,
            epoch: SceneEpoch(1),
            revision,
        }))
    }
}

impl PresentationSource for KnotEndpoint {
    type Error = String;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        let bytes = if request.session == self.session {
            self.resources.get(&request.resource)
        } else {
            self.rosette_resources
                .get(&request.session)
                .and_then(|resources| resources.get(&request.resource))
        }
        .cloned()
        .ok_or_else(|| "resource was not disclosed by this Knot session".to_string())?;
        Ok(ResourceResponse {
            session: request.session,
            resource: request.resource,
            bytes,
        })
    }
}

impl IntentSink for KnotEndpoint {
    type Error = String;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session {
            if self.rosette_snapshots.contains_key(&intent.session)
                || self.current_rosette_document(&intent.session).is_some()
            {
                return Ok(IntentResult::Rejected {
                    reason: "Knot Rosette projections are read-only".into(),
                });
            }
            return Err("intent names the wrong Knot session".into());
        }
        self.refresh()?;
        let source_revision = self.revision();
        let Some(snapshot) = &self.snapshot else {
            return Err("intent arrived before a Knot snapshot".into());
        };
        if snapshot.scene.revision != source_revision {
            let announced = self.last_announced;
            let current = self.build_snapshot()?;
            self.last_announced = announced;
            return Ok(IntentResult::Stale {
                current_epoch: current.scene.epoch,
                current_revision: current.scene.revision,
            });
        }
        if intent.observed_epoch != snapshot.scene.epoch
            || intent.observed_revision != snapshot.scene.revision
        {
            return Ok(IntentResult::Stale {
                current_epoch: snapshot.scene.epoch,
                current_revision: snapshot.scene.revision,
            });
        }
        let expected_schema = match intent.intent.as_str() {
            EDITABLE_TEXT_SAVE_INTENT => EDITABLE_TEXT_SAVE_SCHEMA,
            KNOT_CLIP_INSERT_INTENT if self.clip_evidence.is_some() => KNOT_CLIP_INSERT_SCHEMA_V2,
            KNOT_CLIP_INSERT_INTENT => KNOT_CLIP_INSERT_SCHEMA,
            KNOT_TRANSCLUSION_RESOLVE_INTENT => KNOT_TRANSCLUSION_RESOLVE_SCHEMA,
            KNOT_BLOCK_RUN_INTENT => KNOT_BLOCK_RUN_SCHEMA,
            _ => {
                return Ok(IntentResult::Rejected {
                    reason: "intent was not advertised by this Knot endpoint".into(),
                });
            }
        };
        if !snapshot
            .presentation
            .offers_for(intent.target)
            .into_iter()
            .flatten()
            .flat_map(|offer| &offer.semantics.actions)
            .any(|action| {
                action.intent.0 == intent.intent && action.payload_schema == expected_schema
            })
        {
            return Ok(IntentResult::Rejected {
                reason: "intent was not advertised for this target".into(),
            });
        }
        let Some(document_id) = self.bindings.get(&intent.target.0).cloned() else {
            return Ok(IntentResult::Rejected {
                reason: "intent target is not bound in this snapshot".into(),
            });
        };
        match intent.intent.as_str() {
            EDITABLE_TEXT_SAVE_INTENT => {
                let payload: SaveTextV1 = match serde_json::from_slice(&intent.payload) {
                    Ok(payload) => payload,
                    Err(_) => {
                        return Ok(IntentResult::Rejected {
                            reason: "save payload does not match graphshell.editable-text.save/v1"
                                .into(),
                        });
                    }
                };
                self.save_text(&document_id, payload)
            }
            KNOT_CLIP_INSERT_INTENT => {
                if self.clip_evidence.is_some() {
                    let payload: InsertKnotClipV2 = match serde_json::from_slice(&intent.payload) {
                        Ok(payload) => payload,
                        Err(_) => {
                            return Ok(IntentResult::Rejected {
                                reason: "clip payload does not match knot.clip.insert/v2".into(),
                            });
                        }
                    };
                    self.insert_clip_v2(&document_id, payload)
                } else {
                    let payload: InsertKnotClipV1 = match serde_json::from_slice(&intent.payload) {
                        Ok(payload) => payload,
                        Err(_) => {
                            return Ok(IntentResult::Rejected {
                                reason: "clip payload does not match knot.clip.insert/v1".into(),
                            });
                        }
                    };
                    self.insert_clip(&document_id, payload)
                }
            }
            KNOT_TRANSCLUSION_RESOLVE_INTENT | KNOT_BLOCK_RUN_INTENT => {
                let payload: KnotEffectV1 = match serde_json::from_slice(&intent.payload) {
                    Ok(payload) => payload,
                    Err(_) => {
                        return Ok(IntentResult::Rejected {
                            reason: format!(
                                "effect payload does not match {}",
                                if intent.intent == KNOT_TRANSCLUSION_RESOLVE_INTENT {
                                    KNOT_TRANSCLUSION_RESOLVE_SCHEMA
                                } else {
                                    KNOT_BLOCK_RUN_SCHEMA
                                }
                            ),
                        });
                    }
                };
                if intent.intent == KNOT_TRANSCLUSION_RESOLVE_INTENT {
                    self.resolve_transclusions(&document_id, payload)
                } else {
                    self.run_blocks(&document_id, payload)
                }
            }
            _ => unreachable!("intent kind was checked above"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chirograph::{
        AdvertisedAction, EditableTextV1, InsertKnotClipV1, InsertKnotClipV2,
        KnotClipArtifactRoleV1, KnotClipArtifactV1, KnotClipFidelityV1, KnotClipObservedEdgeV1,
        KnotClipSelectorV1, PresentationCodec, ResourceRequest, ResumeReply, ResumeRequest,
        SaveTextV1,
    };
    use graphshell_endpoint::{
        IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource,
        ProjectionSource, ResumableProjectionSource,
    };
    use p2panda_core::SigningKey;
    use tempfile::tempdir;

    use super::*;

    fn editable_resource(
        endpoint: &mut KnotEndpoint,
        snapshot: &ProjectionSnapshot,
        address_suffix: &str,
    ) -> (InstanceId, EditableTextV1, AdvertisedAction) {
        for (instance, _) in snapshot.scene.active_items_in_order() {
            let offers = snapshot.presentation.offers_for(instance).unwrap();
            let Some(offer) = offers
                .iter()
                .find(|offer| offer.codec == PresentationCodec::EditableTextV1)
            else {
                continue;
            };
            let response = endpoint
                .resource(ResourceRequest {
                    session: snapshot.session.clone(),
                    resource: offer.resource,
                })
                .unwrap();
            let editable: EditableTextV1 = serde_json::from_slice(&response.bytes).unwrap();
            if editable.address.ends_with(address_suffix) {
                return (instance, editable, offer.semantics.actions[0].clone());
            }
        }
        panic!("snapshot did not disclose editable {address_suffix}");
    }

    fn save_invocation(
        snapshot: &ProjectionSnapshot,
        target: InstanceId,
        action: &AdvertisedAction,
        payload: &SaveTextV1,
    ) -> IntentInvocation {
        IntentInvocation {
            session: snapshot.session.clone(),
            target,
            observed_epoch: snapshot.scene.epoch,
            observed_revision: snapshot.scene.revision,
            intent: action.intent.0.clone(),
            payload: serde_json::to_vec(payload).unwrap(),
        }
    }

    fn action_for(
        snapshot: &ProjectionSnapshot,
        target: InstanceId,
        intent: &str,
    ) -> AdvertisedAction {
        snapshot
            .presentation
            .offers_for(target)
            .unwrap()
            .iter()
            .flat_map(|offer| &offer.semantics.actions)
            .find(|action| action.intent.0 == intent)
            .cloned()
            .unwrap_or_else(|| panic!("{intent} was not advertised"))
    }

    fn clip_invocation(
        snapshot: &ProjectionSnapshot,
        target: InstanceId,
        payload: &InsertKnotClipV1,
    ) -> IntentInvocation {
        IntentInvocation {
            session: snapshot.session.clone(),
            target,
            observed_epoch: snapshot.scene.epoch,
            observed_revision: snapshot.scene.revision,
            intent: KNOT_CLIP_INSERT_INTENT.into(),
            payload: serde_json::to_vec(payload).unwrap(),
        }
    }

    fn effect_invocation(
        snapshot: &ProjectionSnapshot,
        target: InstanceId,
        intent: &str,
        payload: &KnotEffectV1,
    ) -> IntentInvocation {
        IntentInvocation {
            session: snapshot.session.clone(),
            target,
            observed_epoch: snapshot.scene.epoch,
            observed_revision: snapshot.scene.revision,
            intent: intent.into(),
            payload: serde_json::to_vec(payload).unwrap(),
        }
    }

    struct StubFetcher;

    impl KnotEffectFetcher for StubFetcher {
        fn fetch(&mut self, address: &str) -> Result<Fetched, String> {
            if address == "file://fixture/included.md" {
                Ok(Fetched {
                    content_type: Some("text/markdown".into()),
                    body: "## Included\n\nFetched text.\n".into(),
                })
            } else {
                Err(format!("unexpected fetch: {address}"))
            }
        }
    }

    struct OfflineStubFetcher;

    impl KnotEffectFetcher for OfflineStubFetcher {
        fn fetch(&mut self, address: &str) -> Result<Fetched, String> {
            Err(format!("offline: {address}"))
        }

        fn cache_version(&self) -> String {
            std::any::type_name::<StubFetcher>().to_string()
        }
    }

    struct StubHtmlFetcher;

    impl KnotEffectFetcher for StubHtmlFetcher {
        fn fetch(&mut self, address: &str) -> Result<Fetched, String> {
            if address == "https://fixture.test/article" {
                Ok(Fetched {
                    content_type: Some("text/html".into()),
                    body: r#"<article>
                        <h2 onclick="steal()" style="display:none">Visible HTML</h2>
                        <p>A <a href="/safe">safe link</a>.</p>
                        <script>SECRET_SCRIPT()</script>
                        <iframe srcdoc="SECRET_FRAME"></iframe>
                    </article>"#
                        .into(),
                })
            } else {
                Err(format!("unexpected fetch: {address}"))
            }
        }
    }

    #[test]
    fn fixture_discloses_cards_and_resources() {
        let mut endpoint = KnotEndpoint::fixture();
        let offer = endpoint.describe().projections.remove(0);
        let snapshot = endpoint.snapshot(offer.request).unwrap();
        assert_eq!(snapshot.scene.active_item_count(), 3);
        assert_eq!(snapshot.presentation.bindings.len(), 3);
        let resource = snapshot.presentation.offers.values().next().unwrap()[0].resource;
        let response = endpoint
            .resource(ResourceRequest {
                session: snapshot.session,
                resource,
            })
            .unwrap();
        assert!(response.has_valid_address());
    }

    #[test]
    fn writable_file_discloses_editable_text_only_to_protocol_1_2() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("field.knot"), "# Field\n").unwrap();
        let grant = KnotWriteGrant::new(1024);
        let mut endpoint = KnotEndpoint::open_writable(temp.path(), grant).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (_, editable, action) = editable_resource(&mut endpoint, &snapshot, "field.knot");
        assert_eq!(editable.source, "# Field\n");
        assert_eq!(action.intent.0, EDITABLE_TEXT_SAVE_INTENT);
        assert_eq!(action.payload_schema, EDITABLE_TEXT_SAVE_SCHEMA);
        let clip_action = action_for(&snapshot, InstanceId(0), KNOT_CLIP_INSERT_INTENT);
        assert_eq!(clip_action.payload_schema, KNOT_CLIP_INSERT_SCHEMA);
        assert_eq!(clip_action.effect, IntentEffect::DomainTruth);
        assert!(
            snapshot
                .presentation
                .offers_for(InstanceId(0))
                .unwrap()
                .iter()
                .flat_map(|offer| &offer.semantics.actions)
                .all(|action| {
                    action.intent.0 != KNOT_TRANSCLUSION_RESOLVE_INTENT
                        && action.intent.0 != KNOT_BLOCK_RUN_INTENT
                }),
            "Never/default effect policy must not advertise Resolve or Run"
        );
        assert_eq!(
            snapshot.cache_policy,
            CachePolicy {
                retention: chirograph::CacheRetention::MemoryOnly,
                expires_at_ms: None,
                purge_on_revocation: true,
            }
        );

        let mut old_endpoint = KnotEndpoint::open_writable(temp.path(), grant).unwrap();
        let mut old_request = old_endpoint.describe().projections.remove(0).request;
        old_request.version = ProtocolVersion::V1_1;
        let old_snapshot = old_endpoint.snapshot(old_request).unwrap();
        assert!(
            old_snapshot
                .presentation
                .offers
                .values()
                .flatten()
                .all(|offer| offer.codec != PresentationCodec::EditableTextV1)
        );

        let mut read_only = KnotEndpoint::open(temp.path()).unwrap();
        let request = read_only.describe().projections.remove(0).request;
        let snapshot = read_only.snapshot(request).unwrap();
        assert!(
            snapshot
                .presentation
                .offers
                .values()
                .flatten()
                .all(|offer| offer.codec != PresentationCodec::EditableTextV1)
        );
    }

    #[test]
    fn file_save_is_revision_checked_and_rings_once() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.knot");
        fs::write(&path, "# Field\n").unwrap();
        let mut endpoint =
            KnotEndpoint::open_writable(temp.path(), KnotWriteGrant::new(1024)).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, action) = editable_resource(&mut endpoint, &snapshot, "field.knot");

        let accepted = endpoint
            .invoke(save_invocation(
                &snapshot,
                target,
                &action,
                &SaveTextV1 {
                    base_token: editable.base_token.clone(),
                    source: "# Revised\n".into(),
                },
            ))
            .unwrap();
        assert_eq!(accepted, IntentResult::Accepted);
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Revised\n");
        let notice = endpoint.poll_notice().unwrap().unwrap();
        assert!(notice.revision > snapshot.scene.revision);
        assert_eq!(endpoint.poll_notice().unwrap(), None);

        let resumed = endpoint
            .resume(ResumeRequest {
                session: snapshot.session.clone(),
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap();
        let ResumeReply::Snapshot(current) = resumed else {
            panic!("accepted save must advance the projection");
        };
        let stale = endpoint
            .invoke(save_invocation(
                &current,
                target,
                &action,
                &SaveTextV1 {
                    base_token: editable.base_token,
                    source: "# Lost update\n".into(),
                },
            ))
            .unwrap();
        assert!(matches!(stale, IntentResult::Stale { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Revised\n");

        let malformed = IntentInvocation {
            session: current.session.clone(),
            target,
            observed_epoch: current.scene.epoch,
            observed_revision: current.scene.revision,
            intent: action.intent.0,
            payload: br#"{"source":"missing token"}"#.to_vec(),
        };
        assert!(matches!(
            endpoint.invoke(malformed).unwrap(),
            IntentResult::Rejected { .. }
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Revised\n");
    }

    #[test]
    fn clip_insert_records_typed_provenance_and_refuses_stale_or_invalid_input() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.knot");
        fs::write(&path, "# Field\n").unwrap();
        let mut endpoint =
            KnotEndpoint::open_writable(temp.path(), KnotWriteGrant::new(4096)).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "field.knot");
        assert_eq!(
            endpoint
                .invoke(clip_invocation(
                    &snapshot,
                    target,
                    &InsertKnotClipV1 {
                        base_token: editable.base_token.clone(),
                        source_url: "https://example.test/post".into(),
                        title: Some("A finding".into()),
                        selector: Some("main > article".into()),
                        knot_body: "A useful paragraph.\n".into(),
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.starts_with("# Field\n\n```knot.clip.provenance\n"));
        assert!(saved.contains(r#""schema":"knot.clip.insert/v1""#));
        assert!(saved.contains(r#""source_url":"https://example.test/post""#));
        assert!(saved.ends_with("A useful paragraph.\n"));

        let notice = endpoint.poll_notice().unwrap().unwrap();
        assert!(notice.revision > snapshot.scene.revision);
        let ResumeReply::Snapshot(current) = endpoint
            .resume(ResumeRequest {
                session: snapshot.session.clone(),
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap()
        else {
            panic!("clip insert must refresh the projection");
        };
        let stale = endpoint
            .invoke(clip_invocation(
                &current,
                target,
                &InsertKnotClipV1 {
                    base_token: editable.base_token,
                    source_url: "https://example.test/stale".into(),
                    title: None,
                    selector: None,
                    knot_body: "Lost update.\n".into(),
                },
            ))
            .unwrap();
        assert!(matches!(stale, IntentResult::Stale { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), saved);

        let (_, current_editable, _) = editable_resource(&mut endpoint, &current, "field.knot");
        let invalid = endpoint
            .invoke(clip_invocation(
                &current,
                target,
                &InsertKnotClipV1 {
                    base_token: current_editable.base_token,
                    source_url: "relative/path".into(),
                    title: None,
                    selector: None,
                    knot_body: "Bad source.\n".into(),
                },
            ))
            .unwrap();
        assert!(matches!(invalid, IntentResult::Rejected { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), saved);
    }

    #[test]
    fn evidence_clip_retains_bytes_and_authors_only_a_portable_reference() {
        let temp = tempdir().unwrap();
        let evidence_temp = tempdir().unwrap();
        let evidence = evidence_temp.path();
        let path = temp.path().join("field.djot");
        fs::write(&path, "# Field\n").unwrap();
        let mut endpoint =
            KnotEndpoint::open_writable(temp.path(), KnotWriteGrant::new(4096)).unwrap();
        endpoint.grant_clip_evidence(crate::FileClipEvidenceStore::new(&evidence, 4096));
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "field.djot");
        let action = action_for(&snapshot, target, KNOT_CLIP_INSERT_INTENT);
        assert_eq!(action.payload_schema, KNOT_CLIP_INSERT_SCHEMA_V2);

        let bytes = b"<article><p>A useful finding.</p></article>".to_vec();
        let digest = blake3::hash(&bytes).to_hex().to_string();
        let accepted = endpoint
            .invoke(clip_v2_invocation(
                &snapshot,
                target,
                &InsertKnotClipV2 {
                    base_token: editable.base_token,
                    source_url: "https://example.test/report".into(),
                    title: Some("The report".into()),
                    selectors: vec![KnotClipSelectorV1::TextQuote {
                        artifact_role: KnotClipArtifactRoleV1::SourceResponse,
                        exact: "A useful finding.".into(),
                        prefix: None,
                        suffix: None,
                    }],
                    knot_body: "A useful finding.\n".into(),
                    artifacts: vec![KnotClipArtifactV1 {
                        role: KnotClipArtifactRoleV1::SourceResponse,
                        media_type: "text/html".into(),
                        canonical_uri: "https://example.test/report".into(),
                        bytes: bytes.clone(),
                    }],
                    fidelity: vec![KnotClipFidelityV1 {
                        class: "arrangement-unchecked".into(),
                        detail: "Static source capture did not compare computed layout.".into(),
                        selector: None,
                    }],
                    discovered_edges: vec![KnotClipObservedEdgeV1 {
                        target: "https://example.test/source".into(),
                        relation: "link".into(),
                    }],
                },
            ))
            .unwrap();
        assert_eq!(accepted, IntentResult::Accepted);
        assert_eq!(
            fs::read(evidence.join("blake3").join(&digest)).unwrap(),
            bytes
        );

        let saved = fs::read_to_string(path).unwrap();
        assert!(saved.contains(r#""schema":"knot.clip.insert/v2""#));
        assert!(saved.contains(&format!("urn:blake3:{digest}")));
        assert!(saved.contains(r#""class":"arrangement-unchecked""#));
        assert!(saved.contains(r#""relation":"link""#));
        assert!(!saved.contains("<article>"));
    }

    #[test]
    fn dom_range_selectors_require_an_observed_representation() {
        let source = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: "https://example.test/report".into(),
            bytes: b"<p>A useful finding.</p>".to_vec(),
        };
        let selector = KnotClipSelectorV1::DomRange {
            artifact_role: KnotClipArtifactRoleV1::SourceResponse,
            anchor_path: vec![0, 1],
            anchor_offset: 0,
            focus_path: vec![0, 1],
            focus_offset: 17,
            quote: "A useful finding.".into(),
        };
        assert!(!selector_matches_artifacts(&selector, &[source]));

        let observed = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::ObservedRepresentation,
            media_type: "application/vnd.mere.dom+json".into(),
            canonical_uri: "https://example.test/report".into(),
            bytes: br#"{"node":"p","text":"A useful finding."}"#.to_vec(),
        };
        let selector = KnotClipSelectorV1::DomRange {
            artifact_role: KnotClipArtifactRoleV1::ObservedRepresentation,
            anchor_path: vec![0, 1],
            anchor_offset: 0,
            focus_path: vec![0, 1],
            focus_offset: 17,
            quote: "A useful finding.".into(),
        };
        assert!(selector_matches_artifacts(&selector, &[observed]));
    }

    #[test]
    fn resolve_and_run_are_consented_revisioned_derived_state() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.knot");
        let authored = "\
# Field

```include file://fixture/included.md
Fallback.
```

```rhai eval
40 + 2
```
";
        fs::write(&path, authored).unwrap();
        let policy = KnotEffectPolicy {
            resolve: KnotEffectMode::Ask,
            run: KnotEffectMode::Ask,
            allowed_schemes: vec!["file".into()],
            allowed_languages: vec!["rhai".into()],
            max_depth: 1,
            max_ops: 10_000,
        };
        let effects = KnotEffectAuthority::new(policy)
            .with_fetcher(StubFetcher)
            .register_evaluator(script_rhai::RhaiEvaluator::new());
        let mut endpoint =
            KnotEndpoint::open_writable(temp.path(), KnotWriteGrant::new(4096)).unwrap();
        endpoint.grant_effects(effects);
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "field.knot");
        assert!(editable.derived.is_none());
        let resolve_action = action_for(&snapshot, target, KNOT_TRANSCLUSION_RESOLVE_INTENT);
        let run_action = action_for(&snapshot, target, KNOT_BLOCK_RUN_INTENT);
        assert_eq!(resolve_action.effect, IntentEffect::ExternalEffect);
        assert_eq!(run_action.effect, IntentEffect::ExternalEffect);

        let unconfirmed = endpoint
            .invoke(effect_invocation(
                &snapshot,
                target,
                KNOT_TRANSCLUSION_RESOLVE_INTENT,
                &KnotEffectV1 {
                    base_token: editable.base_token.clone(),
                    confirmed: false,
                },
            ))
            .unwrap();
        assert!(matches!(unconfirmed, IntentResult::Rejected { .. }));
        assert_eq!(endpoint.poll_notice().unwrap(), None);

        assert_eq!(
            endpoint
                .invoke(effect_invocation(
                    &snapshot,
                    target,
                    KNOT_TRANSCLUSION_RESOLVE_INTENT,
                    &KnotEffectV1 {
                        base_token: editable.base_token.clone(),
                        confirmed: true,
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), authored);
        let resolved_notice = endpoint.poll_notice().unwrap().unwrap();
        assert!(resolved_notice.revision > snapshot.scene.revision);
        let ResumeReply::Snapshot(resolved) = endpoint
            .resume(ResumeRequest {
                session: snapshot.session.clone(),
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap()
        else {
            panic!("resolve must refresh the derived presentation");
        };
        let (_, resolved_text, _) = editable_resource(&mut endpoint, &resolved, "field.knot");
        let derived = resolved_text.derived.expect("resolve result");
        assert!(
            derived.source.contains("Included"),
            "derived source: {}\nsummary: {}",
            derived.source,
            derived.summary
        );
        assert!(derived.source.contains("Fetched text."));
        assert!(derived.source.contains("rhai eval"));
        assert_eq!(derived.summary, "resolved 1; denied 0; failed 0");

        assert_eq!(
            endpoint
                .invoke(effect_invocation(
                    &resolved,
                    target,
                    KNOT_BLOCK_RUN_INTENT,
                    &KnotEffectV1 {
                        base_token: resolved_text.base_token,
                        confirmed: true,
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), authored);
        let run_notice = endpoint.poll_notice().unwrap().unwrap();
        assert!(run_notice.revision > resolved.scene.revision);
        let ResumeReply::Snapshot(ran) = endpoint
            .resume(ResumeRequest {
                session: resolved.session,
                epoch: resolved.scene.epoch,
                revision: resolved.scene.revision,
            })
            .unwrap()
        else {
            panic!("run must refresh the derived presentation");
        };
        let (_, ran_text, _) = editable_resource(&mut endpoint, &ran, "field.knot");
        let ran_base_token = ran_text.base_token.clone();
        let derived = ran_text.derived.expect("run result");
        assert!(derived.source.contains("Included"));
        assert!(derived.source.contains("42"));
        assert!(!derived.source.contains("rhai eval"));
        assert_eq!(derived.summary, "ran 1; denied 0; failed 0");

        fs::write(&path, "# Changed elsewhere\n").unwrap();
        let stale = endpoint
            .invoke(effect_invocation(
                &ran,
                target,
                KNOT_BLOCK_RUN_INTENT,
                &KnotEffectV1 {
                    base_token: ran_base_token,
                    confirmed: true,
                },
            ))
            .unwrap();
        assert!(matches!(stale, IntentResult::Stale { .. }));
        assert_eq!(fs::read_to_string(path).unwrap(), "# Changed elsewhere\n");
    }

    #[test]
    fn html_transclusion_lowers_only_the_sanitized_semantic_fragment() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.knot");
        let authored = "# Field\n\n```include https://fixture.test/article\nFallback.\n```\n";
        fs::write(&path, authored).unwrap();
        let effects = KnotEffectAuthority::new(KnotEffectPolicy {
            resolve: KnotEffectMode::Ask,
            allowed_schemes: vec!["https".into()],
            max_depth: 1,
            ..KnotEffectPolicy::default()
        })
        .with_fetcher(StubHtmlFetcher);
        let mut endpoint =
            KnotEndpoint::open_writable(temp.path(), KnotWriteGrant::new(4096)).unwrap();
        endpoint.grant_effects(effects);
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "field.knot");

        assert_eq!(
            endpoint
                .invoke(effect_invocation(
                    &snapshot,
                    target,
                    KNOT_TRANSCLUSION_RESOLVE_INTENT,
                    &KnotEffectV1 {
                        base_token: editable.base_token,
                        confirmed: true,
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );
        let ResumeReply::Snapshot(current) = endpoint
            .resume(ResumeRequest {
                session: snapshot.session,
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap()
        else {
            panic!("HTML resolve must refresh the derived presentation");
        };
        let (_, current, _) = editable_resource(&mut endpoint, &current, "field.knot");
        let derived = current.derived.expect("HTML resolve result");
        assert!(derived.source.contains("Visible HTML"));
        assert!(derived.source.contains("safe link"));
        assert!(!derived.source.contains("SECRET_SCRIPT"));
        assert!(!derived.source.contains("SECRET_FRAME"));
        assert!(!derived.source.contains("onclick"));
        assert!(!derived.source.contains("display:none"));
        assert_eq!(derived.summary, "resolved 1; denied 0; failed 0");
        assert_eq!(fs::read_to_string(path).unwrap(), authored);
    }

    #[test]
    fn auto_run_stops_at_the_injected_operation_budget() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.knot");
        let authored = "# Field\n\n```rhai eval\nloop { }\n```\n";
        fs::write(&path, authored).unwrap();
        let policy = KnotEffectPolicy {
            run: KnotEffectMode::Auto,
            allowed_languages: vec!["rhai".into()],
            max_ops: 100,
            ..KnotEffectPolicy::default()
        };
        let effects =
            KnotEffectAuthority::new(policy).register_evaluator(script_rhai::RhaiEvaluator::new());
        let mut endpoint =
            KnotEndpoint::open_writable(temp.path(), KnotWriteGrant::new(4096)).unwrap();
        endpoint.grant_effects(effects);
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "field.knot");

        assert_eq!(
            endpoint
                .invoke(effect_invocation(
                    &snapshot,
                    target,
                    KNOT_BLOCK_RUN_INTENT,
                    &KnotEffectV1 {
                        base_token: editable.base_token,
                        confirmed: false,
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );
        let ResumeReply::Snapshot(current) = endpoint
            .resume(ResumeRequest {
                session: snapshot.session,
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap()
        else {
            panic!("bounded run must produce a derived receipt");
        };
        let (_, current, _) = editable_resource(&mut endpoint, &current, "field.knot");
        let derived = current.derived.expect("bounded failure result");
        assert_eq!(derived.summary, "ran 0; denied 0; failed 1");
        assert!(derived.source.contains("loop { }"));
        assert_eq!(fs::read_to_string(path).unwrap(), authored);
    }

    #[test]
    fn unrelated_directory_churn_does_not_invalidate_the_document_token() {
        let temp = tempdir().unwrap();
        let field = temp.path().join("field.knot");
        let other = temp.path().join("other.knot");
        fs::write(&field, "# Field\n").unwrap();
        fs::write(&other, "# Other\n").unwrap();
        let mut endpoint =
            KnotEndpoint::open_writable(temp.path(), KnotWriteGrant::new(1024)).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (_, editable, _) = editable_resource(&mut endpoint, &snapshot, "field.knot");

        fs::write(&other, "# Other changed\n").unwrap();
        let resumed = endpoint
            .resume(ResumeRequest {
                session: snapshot.session.clone(),
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap();
        let ResumeReply::Snapshot(current) = resumed else {
            panic!("unrelated edit must advance the scene");
        };
        let (target, refreshed, action) = editable_resource(&mut endpoint, &current, "field.knot");
        assert_eq!(editable.base_token, refreshed.base_token);
        assert_eq!(
            endpoint
                .invoke(save_invocation(
                    &current,
                    target,
                    &action,
                    &SaveTextV1 {
                        base_token: editable.base_token,
                        source: "# Field changed\n".into(),
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );
        assert_eq!(fs::read_to_string(field).unwrap(), "# Field changed\n");
    }

    #[test]
    fn disk_edit_is_visible_on_next_resume() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.knot");
        fs::write(&path, "one").unwrap();
        let mut endpoint = KnotEndpoint::open(temp.path()).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request.clone()).unwrap();

        fs::write(&path, "one two three").unwrap();
        let reply = endpoint
            .resume(ResumeRequest {
                session: request.session,
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap();
        let ResumeReply::Snapshot(next) = reply else {
            panic!("changed directory should return a replacement snapshot");
        };
        assert!(next.scene.revision > snapshot.scene.revision);
        let offer = next.presentation.offers.values().next().unwrap();
        let bytes = endpoint
            .resource(ResourceRequest {
                session: next.session,
                resource: offer[0].resource,
            })
            .unwrap()
            .bytes;
        let card: PortableCardV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(card.values[2].value, "13 bytes");
    }

    #[test]
    fn disk_edit_rings_once_before_the_host_resumes() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.knot");
        fs::write(&path, "one").unwrap();
        let mut endpoint = KnotEndpoint::open(temp.path()).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();

        fs::write(&path, "one two three").unwrap();
        let notice = endpoint.poll_notice().unwrap().unwrap();
        assert_eq!(notice.session, snapshot.session);
        assert_eq!(notice.epoch, snapshot.scene.epoch);
        assert!(notice.revision > snapshot.scene.revision);
        assert_eq!(endpoint.poll_notice().unwrap(), None);
    }

    fn clip_v2_invocation(
        snapshot: &ProjectionSnapshot,
        target: InstanceId,
        payload: &InsertKnotClipV2,
    ) -> IntentInvocation {
        IntentInvocation {
            session: snapshot.session.clone(),
            target,
            observed_epoch: snapshot.scene.epoch,
            observed_revision: snapshot.scene.revision,
            intent: KNOT_CLIP_INSERT_INTENT.into(),
            payload: serde_json::to_vec(payload).unwrap(),
        }
    }

    #[test]
    fn rosette_sessions_ring_independently_and_drop_deleted_source() {
        let temp = tempdir().unwrap();
        let poem = temp.path().join("poem.knot");
        fs::write(
            &poem,
            "Morning gathers light\nBranches answer night\n\nFootsteps cross the hill\nEvening settles still\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("lyric.knot"),
            "Raise your open hand\nWe will take a stand\n\nCarry home the song\nLet the road run long\n",
        )
        .unwrap();
        let mut endpoint = KnotEndpoint::open(temp.path()).unwrap();
        let descriptor = endpoint.describe();
        assert_eq!(descriptor.projections.len(), 3);
        let sessions = descriptor
            .projections
            .into_iter()
            .map(|offer| {
                let session = offer.request.session.clone();
                endpoint.snapshot(offer.request).unwrap();
                session
            })
            .collect::<BTreeSet<_>>();

        fs::write(&poem, "The final bell\nAnswers well\n").unwrap();
        let notices = (0..sessions.len())
            .map(|_| endpoint.poll_notice().unwrap().unwrap().session)
            .collect::<BTreeSet<_>>();
        assert_eq!(notices, sessions);
        assert_eq!(endpoint.poll_notice().unwrap(), None);

        let poem_request = endpoint
            .describe()
            .projections
            .into_iter()
            .find(|offer| offer.label.contains("poem"))
            .unwrap()
            .request;
        let snapshot = endpoint.snapshot(poem_request.clone()).unwrap();
        let old_resource = snapshot.presentation.offers.values().next().unwrap()[0].resource;
        fs::remove_file(poem).unwrap();
        let reply = endpoint
            .resume(ResumeRequest {
                session: poem_request.session.clone(),
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap();
        let ResumeReply::Snapshot(removed) = reply else {
            panic!("a removed Rosette source must replace the stale scene");
        };
        assert_eq!(removed.scene.active_item_count(), 0);
        assert!(removed.presentation.bindings.is_empty());
        assert!(
            endpoint
                .resource(ResourceRequest {
                    session: poem_request.session,
                    resource: old_resource,
                })
                .is_err(),
            "removed source resources must leave the session"
        );
    }

    #[test]
    fn unchanged_resume_is_current() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("field.knot"), "one").unwrap();
        let mut endpoint = KnotEndpoint::open(temp.path()).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request.clone()).unwrap();
        let reply = endpoint
            .resume(ResumeRequest {
                session: request.session,
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap();
        assert!(matches!(reply, ResumeReply::Current(_)));
    }

    #[test]
    fn revoked_watcher_holds_the_last_revision_until_regranted() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.knot");
        fs::write(&path, "one").unwrap();
        let mut endpoint = KnotEndpoint::open(temp.path()).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request.clone()).unwrap();

        assert!(endpoint.revoke_watcher());
        fs::write(&path, "one two three").unwrap();
        let paused = endpoint
            .resume(ResumeRequest {
                session: request.session.clone(),
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap();
        assert!(matches!(paused, ResumeReply::Current(_)));

        assert!(endpoint.grant_watcher());
        let resumed = endpoint
            .resume(ResumeRequest {
                session: request.session,
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap();
        assert!(matches!(resumed, ResumeReply::Snapshot(_)));
    }

    #[test]
    fn sealed_vault_save_is_one_signed_event_then_a_rematerialized_view() {
        let vault_dir = tempdir().unwrap();
        let sync_dir = tempdir().unwrap();
        let key = [0x91; 32];
        let seed = [0x41; 32];
        let writer = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let space = [0x51; 32];
        let vault = KnotVault::open(vault_dir.path(), key).unwrap();
        let store =
            KnotSyncFileStore::open(sync_dir.path().join("knot.redb"), space, [writer]).unwrap();
        pollster::block_on(store.author(
            seed,
            &vault,
            &KnotSyncEvent::Put(VaultDocument {
                id: "field-note".into(),
                title: "Field note".into(),
                body: b"# Private\n".to_vec(),
                media_type: "text/vnd.knot".into(),
            }),
        ))
        .unwrap();
        let inspection_store = store.clone();
        let initial_head = pollster::block_on(inspection_store.projection(&vault))
            .unwrap()
            .document_heads["field-note"];

        let mut endpoint =
            KnotEndpoint::from_synced_vault(vault, store, seed, KnotWriteGrant::new(4096)).unwrap();
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, action) = editable_resource(&mut endpoint, &snapshot, "field-note");
        let editable_hash = snapshot
            .presentation
            .offers_for(target)
            .unwrap()
            .iter()
            .find(|offer| offer.codec == PresentationCodec::EditableTextV1)
            .unwrap()
            .resource;
        assert_eq!(editable.source, "# Private\n");
        assert_eq!(
            endpoint
                .invoke(save_invocation(
                    &snapshot,
                    target,
                    &action,
                    &SaveTextV1 {
                        base_token: editable.base_token,
                        source: "# Private revised\n".into(),
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );

        let Source::Vault(source) = &endpoint.source else {
            unreachable!()
        };
        let projection = pollster::block_on(inspection_store.projection(&source.vault)).unwrap();
        assert_eq!(projection.documents[0].body, b"# Private revised\n");
        assert_ne!(projection.document_heads["field-note"], initial_head);
        assert_eq!(
            source.vault.body("field-note"),
            Some(&b"# Private revised\n"[..])
        );
        let sealed = fs::read(vault_dir.path().join("knot/documents.json")).unwrap();
        assert!(
            !sealed
                .windows(b"# Private revised\n".len())
                .any(|window| window == b"# Private revised\n")
        );

        assert!(endpoint.lock_vault());
        assert!(
            endpoint
                .resource(ResourceRequest {
                    session: snapshot.session,
                    resource: editable_hash,
                })
                .is_err(),
            "locking purges previously disclosed source resources"
        );
        drop(endpoint);

        let reopened = KnotVault::open(vault_dir.path(), key).unwrap();
        assert_eq!(
            reopened.body("field-note"),
            Some(&b"# Private revised\n"[..])
        );
    }

    #[test]
    fn personal_vault_restores_only_matching_attributable_sealed_cache() {
        let vault_dir = tempdir().unwrap();
        let sync_dir = tempdir().unwrap();
        let key = [0x92; 32];
        let seed = [0x42; 32];
        let writer = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let space = [0x52; 32];
        let database = sync_dir.path().join("knot.redb");
        let vault = KnotVault::open(vault_dir.path(), key).unwrap();
        let store = KnotSyncFileStore::open(&database, space, [writer]).unwrap();
        pollster::block_on(
            store.author(
                seed,
                &vault,
                &KnotSyncEvent::Put(VaultDocument {
                    id: "field-note".into(),
                    title: "Field note".into(),
                    body: b"# Private\n\n```include file://fixture/included.md\nFallback.\n```\n"
                        .to_vec(),
                    media_type: "text/vnd.knot".into(),
                }),
            ),
        )
        .unwrap();
        let policy = KnotEffectPolicy {
            resolve: KnotEffectMode::Ask,
            allowed_schemes: vec!["file".into()],
            max_depth: 1,
            ..KnotEffectPolicy::default()
        };
        let mut endpoint =
            KnotEndpoint::from_synced_vault(vault, store, seed, KnotWriteGrant::new(4096)).unwrap();
        endpoint.grant_effects(KnotEffectAuthority::new(policy.clone()).with_fetcher(StubFetcher));
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "field-note");
        assert_eq!(
            endpoint
                .invoke(effect_invocation(
                    &snapshot,
                    target,
                    KNOT_TRANSCLUSION_RESOLVE_INTENT,
                    &KnotEffectV1 {
                        base_token: editable.base_token,
                        confirmed: true,
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );
        let cache_files = fs::read_dir(vault_dir.path().join("knot/derived-cache"))
            .unwrap()
            .map(|entry| fs::read(entry.unwrap().path()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(cache_files.len(), 1);
        assert!(cache_files.iter().all(|sealed| {
            !sealed
                .windows(b"Fetched text.".len())
                .any(|window| window == b"Fetched text.")
        }));
        drop(endpoint);

        let vault = KnotVault::open(vault_dir.path(), key).unwrap();
        let store = KnotSyncFileStore::open(&database, space, [writer]).unwrap();
        let mut reopened =
            KnotEndpoint::from_synced_vault(vault, store, seed, KnotWriteGrant::new(4096)).unwrap();
        reopened.grant_effects(
            KnotEffectAuthority::new(policy.clone()).with_fetcher(OfflineStubFetcher),
        );
        let request = reopened.describe().projections.remove(0).request;
        let snapshot = reopened.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut reopened, &snapshot, "field-note");
        let base_token = editable.base_token.clone();
        let restored = editable.derived.expect("sealed cache should restore");
        assert!(restored.source.contains("Fetched text."));
        let cache = restored.cache.expect("cache attribution");
        assert_eq!(cache.effect, "resolve");
        assert_eq!(cache.sources, vec!["file://fixture/included.md"]);
        assert!(cache.provider_version.contains("StubFetcher"));
        assert_eq!(cache.source_revision, 1);
        assert!(cache.fetched_at_unix_ms > 0);
        let refresh = reopened
            .invoke(effect_invocation(
                &snapshot,
                target,
                KNOT_TRANSCLUSION_RESOLVE_INTENT,
                &KnotEffectV1 {
                    base_token,
                    confirmed: true,
                },
            ))
            .unwrap();
        assert!(
            matches!(
                refresh,
                IntentResult::Rejected { ref reason }
                    if reason.contains("retained cached result")
            ),
            "an offline refresh must report failure without replacing the cache: {refresh:?}"
        );
        reopened.snapshot = None;
        reopened.resources.clear();
        reopened.bindings.clear();
        let request = reopened.describe().projections.remove(0).request;
        let after_failed_refresh = reopened.snapshot(request).unwrap();
        let (_, editable, _) =
            editable_resource(&mut reopened, &after_failed_refresh, "field-note");
        assert!(
            editable
                .derived
                .is_some_and(|derived| derived.source.contains("Fetched text.")),
            "an offline refresh must leave the restored document available"
        );

        reopened.snapshot = None;
        reopened.resources.clear();
        // The version has to travel in the request, because that is where a
        // real client puts it. `describe` advertises the endpoint's newest, and
        // `snapshot` adopts whatever the request carries, so assigning the
        // field alone is overwritten before the resource is ever built.
        let mut request = reopened.describe().projections.remove(0).request;
        request.version = ProtocolVersion::V1_2;
        let snapshot = reopened.snapshot(request).unwrap();
        let (_, editable, _) = editable_resource(&mut reopened, &snapshot, "field-note");
        let compatible = editable.derived.expect("1.2 still receives derived text");
        assert!(
            compatible.cache.is_none(),
            "1.2 resources must omit the 1.3 cache field"
        );
        reopened.snapshot = None;
        reopened.resources.clear();

        assert!(reopened.lock_vault());
        assert!(reopened.unlock_vault(key).unwrap());
        let request = reopened.describe().projections.remove(0).request;
        let snapshot = reopened.snapshot(request).unwrap();
        let (_, editable, _) = editable_resource(&mut reopened, &snapshot, "field-note");
        assert!(
            editable.derived.is_some(),
            "unlock under the same source authority should restore the sealed cache"
        );

        assert!(reopened.revoke_effects());
        let request = reopened.describe().projections.remove(0).request;
        let snapshot = reopened.snapshot(request).unwrap();
        let (_, editable, _) = editable_resource(&mut reopened, &snapshot, "field-note");
        assert!(
            editable.derived.is_none(),
            "effect revocation must make the cache unavailable"
        );

        reopened.grant_effects(
            KnotEffectAuthority::new(KnotEffectPolicy {
                max_depth: 2,
                ..policy
            })
            .with_fetcher(StubFetcher),
        );
        let request = reopened.describe().projections.remove(0).request;
        let snapshot = reopened.snapshot(request).unwrap();
        let (_, editable, _) = editable_resource(&mut reopened, &snapshot, "field-note");
        assert!(
            editable.derived.is_none(),
            "a changed resolve policy must invalidate the cache"
        );
    }

    #[test]
    fn commons_epoch_rotation_makes_the_old_cache_unavailable() {
        let vault_dir = tempdir().unwrap();
        let sync_dir = tempdir().unwrap();
        let vault_key = [0x93; 32];
        let seed = [0x43; 32];
        let writer = *SigningKey::from_bytes(&seed).verifying_key().as_bytes();
        let space = [0x53; 32];
        let vault = KnotVault::open(vault_dir.path(), vault_key).unwrap();
        let store =
            KnotSyncFileStore::open_commons(sync_dir.path().join("knot.redb"), space, [writer])
                .unwrap();
        let mut keys = DataKeyring::new();
        let first_epoch = keys.rotate_random().unwrap().id();
        pollster::block_on(store.author_communal(
            seed,
            &keys,
            &KnotSyncEvent::Put(VaultDocument {
                id: "shared-note".into(),
                title: "Shared note".into(),
                body:
                    b"# Shared\n\n```include file://fixture/included.md\nFallback.\n```\n".to_vec(),
                media_type: "text/vnd.knot".into(),
            }),
        ))
        .unwrap();
        let mut rotated = DataKeyring::from_bytes(&keys.to_bytes().unwrap()).unwrap();
        let second_epoch = rotated.rotate_random().unwrap().id();
        assert_ne!(first_epoch, second_epoch);

        let policy = KnotEffectPolicy {
            resolve: KnotEffectMode::Ask,
            allowed_schemes: vec!["file".into()],
            max_depth: 1,
            ..KnotEffectPolicy::default()
        };
        let mut endpoint =
            KnotEndpoint::from_communal_vault(vault, store, seed, keys, KnotWriteGrant::new(4096))
                .unwrap();
        endpoint.grant_effects(KnotEffectAuthority::new(policy.clone()).with_fetcher(StubFetcher));
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "shared-note");
        assert_eq!(
            endpoint
                .invoke(effect_invocation(
                    &snapshot,
                    target,
                    KNOT_TRANSCLUSION_RESOLVE_INTENT,
                    &KnotEffectV1 {
                        base_token: editable.base_token,
                        confirmed: true,
                    },
                ))
                .unwrap(),
            IntentResult::Accepted
        );
        let ResumeReply::Snapshot(resolved) = endpoint
            .resume(ResumeRequest {
                session: snapshot.session,
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            })
            .unwrap()
        else {
            panic!("resolve should advance");
        };
        let (_, editable, _) = editable_resource(&mut endpoint, &resolved, "shared-note");
        assert!(editable.derived.is_some());

        assert!(endpoint.replace_communal_keys(rotated).unwrap());
        endpoint.grant_effects(KnotEffectAuthority::new(policy).with_fetcher(StubFetcher));
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (_, editable, _) = editable_resource(&mut endpoint, &snapshot, "shared-note");
        assert!(
            editable.derived.is_none(),
            "a cache sealed under the previous Commons epoch must stay unavailable"
        );
    }

    #[test]
    fn vault_disclosure_contains_neither_key_nor_authored_body() {
        let temp = tempdir().unwrap();
        let key = [0xa7; 32];
        let private_body = b"private words that stay inside Knot";
        let mut vault = KnotVault::open(temp.path(), key).unwrap();
        vault
            .put(crate::VaultDocument {
                id: "private-note".into(),
                title: "Private note".into(),
                body: private_body.to_vec(),
                media_type: "text/vnd.knot".into(),
            })
            .unwrap();
        let mut endpoint = KnotEndpoint::from_vault(vault);
        let descriptor = endpoint.describe();
        let request = descriptor.projections[0].request.clone();
        let snapshot = endpoint.snapshot(request).unwrap();
        let resource = snapshot.presentation.offers.values().next().unwrap()[0].resource;
        let response = endpoint
            .resource(ResourceRequest {
                session: snapshot.session.clone(),
                resource,
            })
            .unwrap();

        let protocol_bytes = [
            serde_json::to_vec(&descriptor).unwrap(),
            serde_json::to_vec(&snapshot).unwrap(),
            serde_json::to_vec(&response).unwrap(),
        ]
        .concat();
        assert!(
            !protocol_bytes
                .windows(key.len())
                .any(|window| window == key)
        );
        assert!(
            !protocol_bytes
                .windows(private_body.len())
                .any(|window| window == private_body)
        );
        assert_eq!(snapshot.scene.active_item_count(), 1);
    }
}
