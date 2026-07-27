//! Graphshell disclosure for Knot directory state.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use chartulary::{Addressed, Labeled};
use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
    ResumableProjectionSource,
};
use graphshell_protocol::{
    AdvertisedAction, BoundsRelationship, CachePolicy, CardValueV1, CarrierNotice, ContentHash,
    EDITABLE_TEXT_SAVE_INTENT, EDITABLE_TEXT_SAVE_SCHEMA, EditableTextV1, EndpointDescriptor,
    IntentEffect, IntentInvocation, IntentReference, IntentResult, NativeGlyphV1, PortableCardV1,
    PresentationBinding, PresentationCapability, PresentationCodec, PresentationKey,
    PresentationManifest, PresentationOffer, PresentationSemantics, ProjectionAck, ProjectionOffer,
    ProjectionRequest, ProjectionSession, ProjectionSnapshot, ProtocolVersion, ResourceRequest,
    ResourceResponse, ResumeReply, ResumeRequest, SaveTextV1, SemanticRole, TextEncoding,
};
use personae::{IdentityProvider, InMemoryProvider};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Rect, Representation, Scene, Score, Size2,
    SourceRef, Transform2, Vec2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use stickleback::DataKeyring;
use zeroize::Zeroizing;

use crate::{
    DirectorySource, DirectoryWatcher, DiskDocument, DocumentFormat, KnotDocumentProjection,
    KnotSyncEvent, KnotSyncFileStore, KnotVault, VaultDocument,
};

const FIXTURE_SESSION: &str = "loopback:knot:k0";
const SOURCE_KIND: &str = "knot.file";
const FILE_TOKEN_CONTEXT: &str = "mere.knot.file-base-token.v1";
const VAULT_TOKEN_CONTEXT: &str = "mere.knot.vault-base-token.v1";

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
            self.snapshot = None;
            self.resources.clear();
            self.bindings.clear();
        }
        had_grant
    }

    pub fn grant_writes(&mut self, grant: KnotWriteGrant) {
        self.write_grant = Some(grant);
        self.snapshot = None;
        self.resources.clear();
        self.bindings.clear();
    }

    /// Lock a vault endpoint, dropping its key and decrypted documents.
    pub fn lock_vault(&mut self) -> bool {
        let Source::Vault(source) = &mut self.source else {
            return false;
        };
        source.vault.lock();
        self.snapshot = None;
        self.resources.clear();
        self.bindings.clear();
        true
    }

    /// Unlock a vault endpoint with a recovered root key.
    pub fn unlock_vault(&mut self, key: [u8; 32]) -> Result<bool, String> {
        let Source::Vault(source) = &mut self.source else {
            return Ok(false);
        };
        source.vault.unlock(key)?;
        self.refresh_vault_projection()?;
        Ok(true)
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

    fn revision(&self) -> Revision {
        match &self.source {
            Source::Directory { source, .. } => Revision(source.revision().max(1)),
            Source::Fixture(_) => Revision(1),
            Source::Vault(source) => Revision(source.vault.revision().max(1)),
        }
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
                Some(EditableTextV1 {
                    address,
                    media_type,
                    encoding: TextEncoding::Utf8,
                    source,
                    base_token: file_base_token(&document.id, &bytes),
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
                Some(EditableTextV1 {
                    address,
                    media_type,
                    encoding: TextEncoding::Utf8,
                    source: text,
                    base_token: vault_base_token(id, head),
                })
            }
            Source::Fixture(_) => None,
        }
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
            let editable = (self.protocol_version.minor >= ProtocolVersion::V1.minor)
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
                    effect: IntentEffect::DomainTruth,
                });
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

impl ProjectionCatalog for KnotEndpoint {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Knot".into(),
            projections: vec![ProjectionOffer {
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
            }],
        }
    }
}

impl ProjectionSource for KnotEndpoint {
    type Error = String;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        self.validate_request(&request)?;
        self.protocol_version = request.version;
        self.refresh()?;
        self.build_snapshot()
    }
}

impl ResumableProjectionSource for KnotEndpoint {
    type Error = String;

    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, Self::Error> {
        if request.session != self.session {
            return Err("resume request names the wrong Knot session".into());
        }
        self.refresh()?;
        let current = self.revision();
        if request.epoch == SceneEpoch(1) && request.revision == current {
            self.last_announced = Some(current);
            return Ok(ResumeReply::Current(ProjectionAck {
                session: self.session.clone(),
                epoch: SceneEpoch(1),
                revision: current,
            }));
        }
        Ok(ResumeReply::Snapshot(Box::new(self.build_snapshot()?)))
    }
}

impl ProjectionNoticeSource for KnotEndpoint {
    type Error = String;

    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error> {
        self.refresh()?;
        if self.snapshot.is_none() {
            return Ok(None);
        }
        let revision = self.revision();
        if self
            .last_announced
            .is_some_and(|announced| revision <= announced)
        {
            return Ok(None);
        }
        self.last_announced = Some(revision);
        Ok(Some(CarrierNotice {
            session: self.session.clone(),
            epoch: SceneEpoch(1),
            revision,
        }))
    }
}

impl PresentationSource for KnotEndpoint {
    type Error = String;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session {
            return Err("resource request names the wrong Knot session".into());
        }
        let bytes = self
            .resources
            .get(&request.resource)
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
        if intent.intent != EDITABLE_TEXT_SAVE_INTENT {
            return Ok(IntentResult::Rejected {
                reason: "intent was not advertised by this Knot endpoint".into(),
            });
        }
        let advertised = snapshot
            .presentation
            .offers_for(intent.target)
            .into_iter()
            .flatten()
            .flat_map(|offer| &offer.semantics.actions)
            .any(|action| {
                action.intent.0 == intent.intent
                    && action.payload_schema == EDITABLE_TEXT_SAVE_SCHEMA
            });
        if !advertised {
            return Ok(IntentResult::Rejected {
                reason: "save was not advertised for this target".into(),
            });
        }
        let Some(document_id) = self.bindings.get(&intent.target.0).cloned() else {
            return Ok(IntentResult::Rejected {
                reason: "intent target is not bound in this snapshot".into(),
            });
        };
        let payload: SaveTextV1 = match serde_json::from_slice(&intent.payload) {
            Ok(payload) => payload,
            Err(_) => {
                return Ok(IntentResult::Rejected {
                    reason: "save payload does not match graphshell.editable-text.save/v1".into(),
                });
            }
        };
        self.save_text(&document_id, payload)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use graphshell_endpoint::{
        IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource,
        ProjectionSource, ResumableProjectionSource,
    };
    use graphshell_protocol::{
        AdvertisedAction, EditableTextV1, PresentationCodec, ResourceRequest, ResumeReply,
        ResumeRequest, SaveTextV1,
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
        assert_eq!(
            snapshot.cache_policy,
            CachePolicy {
                retention: graphshell_protocol::CacheRetention::MemoryOnly,
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
