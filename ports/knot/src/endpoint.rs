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
    PresentationManifest, PresentationOffer, PresentationSemantics, ProjectionAck,
    ProjectionOffer, ProjectionRequest, ProjectionSession, ProjectionSnapshot, ProtocolVersion,
    ResourceRequest, ResourceResponse, ResumeReply, ResumeRequest, SaveTextV1, SemanticRole,
    TextEncoding,
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
    pub fn open_writable(
        root: impl AsRef<Path>,
        grant: KnotWriteGrant,
    ) -> io::Result<Self> {
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

    /// Lock a vault endpoint, dropping its key and decrypted documents.
    pub fn lock_vault(&mut self) -> bool {
        let Source::Vault(vault) = &mut self.source else {
            return false;
        };
        vault.lock();
        self.snapshot = None;
        self.resources.clear();
        true
    }

    /// Unlock a vault endpoint with a recovered root key.
    pub fn unlock_vault(&mut self, key: [u8; 32]) -> Result<bool, String> {
        let Source::Vault(vault) = &mut self.source else {
            return Ok(false);
        };
        vault.unlock(key)?;
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
            Source::Vault(vault) => vault
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
                .collect(),
        }
    }

    fn revision(&self) -> Revision {
        match &self.source {
            Source::Directory { source, .. } => Revision(source.revision().max(1)),
            Source::Fixture(_) => Revision(1),
            Source::Vault(vault) => Revision(vault.revision().max(1)),
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
                badges: match &self.source {
                    Source::Vault(_) => vec!["sealed vault".into(), "read only".into()],
                    _ => vec!["files in place".into(), "read only".into()],
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
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        };
        self.last_announced = Some(snapshot.scene.revision);
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn validate_request(&self, request: &ProjectionRequest) -> Result<(), String> {
        if request.session != self.session {
            return Err("projection request names the wrong Knot session".into());
        }
        if request.version.major != ProtocolVersion::V1.major {
            return Err("projection request uses an unsupported protocol".into());
        }
        if request.score.version != sceno::SCORE_VERSION {
            return Err("projection request uses an unsupported score".into());
        }
        Ok(())
    }
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
        let Some(snapshot) = &self.snapshot else {
            return Err("intent arrived before a Knot snapshot".into());
        };
        if intent.observed_epoch != snapshot.scene.epoch
            || intent.observed_revision != snapshot.scene.revision
        {
            return Ok(IntentResult::Stale {
                current_epoch: snapshot.scene.epoch,
                current_revision: snapshot.scene.revision,
            });
        }
        Ok(IntentResult::Rejected {
            reason: "this Knot slice advertises a read-only directory".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use graphshell_endpoint::{
        PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
        ResumableProjectionSource,
    };
    use graphshell_protocol::{ResourceRequest, ResumeReply, ResumeRequest};
    use tempfile::tempdir;

    use super::*;

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
