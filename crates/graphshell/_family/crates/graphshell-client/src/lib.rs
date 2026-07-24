//! Transport-independent, endpoint-scoped projection state.

use std::collections::{BTreeMap, BTreeSet};

use graphshell_protocol::{
    AdvertisedAction, CachePolicy, CacheRetention, CapabilityProfile, ContentHash, NativeGlyphV1,
    PortableCardV1, PresentationChange, PresentationCodec, PresentationManifest,
    PresentationSemantics, ProjectionAck, ProjectionDiff, ProjectionSession, ProjectionSnapshot,
    ResourceRequest, ResourceResponse, ResumeReply, ResumeRequest, SemanticRole, SessionStatus,
};
use sceno::InstanceId;
use scenotime::{ApplyOutcome, DiffError, SceneSnapshot, SnapshotError};
use serde::{Deserialize, Serialize};

/// The local cache entry for one disclosed remote projection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MountedScene {
    pub status: SessionStatus,
    pub scene: SceneSnapshot,
    pub presentation: PresentationManifest,
    pub cache_policy: CachePolicy,
}

/// A decoded representation plus the semantics that survive fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPresentation {
    pub semantics: PresentationSemantics,
    pub content: ResolvedContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedContent {
    NativeGlyph(NativeGlyphV1),
    PortableCard(PortableCardV1),
    Image { mime_type: String, bytes: Vec<u8> },
    LabeledPlaceholder,
}

/// Resolution is explicit about bytes that have not arrived yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationResolution {
    Ready(ResolvedPresentation),
    NeedsResource(ResourceRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceCacheError {
    UnknownSession,
    UnadvertisedResource,
    AddressMismatch,
    SizeMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolutionError {
    UnknownSession,
    UnknownPresentation,
    InvalidPayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotApplyError {
    InvalidScene(SnapshotError),
    InvalidPresentation(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientDiffError {
    UnknownSession,
    UnsupportedVersion,
    InvalidScene(DiffError),
    InvalidPresentation(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffApplication {
    Applied(ProjectionAck),
    AlreadyApplied(ProjectionAck),
    Resynchronize(ResumeRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeApplication {
    Current(ProjectionAck),
    Applied(ProjectionAck),
    Resynchronize(ResumeRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeApplyError {
    UnknownSession,
    WrongSession,
    InvalidSnapshot(SnapshotApplyError),
    InvalidDiff(ClientDiffError),
}

/// A renderer-neutral accessibility projection owned by the Graphshell client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessibilityTree {
    pub label: String,
    pub children: Vec<AccessibleItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessibleItem {
    pub instance: InstanceId,
    pub label: String,
    pub role: SemanticRole,
    pub actions: Vec<AdvertisedAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreProtection {
    Plain,
    EncryptedAtRest,
}

/// Injected storage. Graphshell decides what may persist; the host decides how
/// bytes are protected and where they live.
pub trait ProjectionStore {
    type Error;

    fn protection(&self) -> StoreProtection;
    fn put(&mut self, session: &ProjectionSession, bytes: &[u8]) -> Result<(), Self::Error>;
    fn get(&self, session: &ProjectionSession) -> Result<Option<Vec<u8>>, Self::Error>;
    fn remove(&mut self, session: &ProjectionSession) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum PersistenceError<E> {
    UnknownSession,
    Missing,
    NotPermitted,
    RequiresEncryptedStore,
    Expired,
    Corrupt,
    Store(E),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PersistedProjection {
    version: u16,
    session: ProjectionSession,
    mounted: MountedScene,
    resources: Vec<PersistedResource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedResource {
    hash: ContentHash,
    bytes: Vec<u8>,
}

/// Curation state. It receives scenes but never obtains source truth.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientState {
    mounted: BTreeMap<ProjectionSession, MountedScene>,
    // Disclosure stays session-scoped even when two endpoints advertise the
    // same content hash. Cross-session reuse requires an explicit later policy.
    resources: BTreeMap<(ProjectionSession, ContentHash), Vec<u8>>,
}

impl ClientState {
    /// Replace one endpoint's last acknowledged epoch-preserving snapshot.
    pub fn apply_snapshot(
        &mut self,
        snapshot: ProjectionSnapshot,
    ) -> Result<(), SnapshotApplyError> {
        snapshot
            .scene
            .validate()
            .map_err(SnapshotApplyError::InvalidScene)?;
        let session = snapshot.session;
        let mounted = MountedScene {
            status: SessionStatus::Live,
            scene: snapshot.scene,
            presentation: snapshot.presentation,
            cache_policy: snapshot.cache_policy,
        };
        validate_presentation(&mounted).map_err(SnapshotApplyError::InvalidPresentation)?;
        let advertised = mounted
            .presentation
            .offers
            .values()
            .flatten()
            .map(|offer| offer.resource)
            .collect::<BTreeSet<_>>();
        self.resources
            .retain(|(owner, hash), _| owner != &session || advertised.contains(hash));
        self.mounted.insert(session, mounted);
        Ok(())
    }

    /// Apply scene, presentation, resource-invalidation, and status changes as
    /// one client transaction.
    pub fn apply_diff(
        &mut self,
        diff: &ProjectionDiff,
    ) -> Result<DiffApplication, ClientDiffError> {
        if diff.version.major != graphshell_protocol::ProtocolVersion::V1.major {
            return Err(ClientDiffError::UnsupportedVersion);
        }
        let current = self
            .mounted
            .get(&diff.session)
            .cloned()
            .ok_or(ClientDiffError::UnknownSession)?;
        let mut next = current.clone();
        let mut next_resources = self.resources.clone();
        let outcome = match next.scene.apply_diff(&diff.scene) {
            Ok(outcome) => outcome,
            Err(DiffError::WrongEpoch { .. } | DiffError::MissingBase { .. }) => {
                return Ok(DiffApplication::Resynchronize(ResumeRequest {
                    session: diff.session.clone(),
                    epoch: current.scene.epoch,
                    revision: current.scene.revision,
                }));
            }
            Err(error) => return Err(ClientDiffError::InvalidScene(error)),
        };
        let ack = ProjectionAck {
            session: diff.session.clone(),
            epoch: next.scene.epoch,
            revision: next.scene.revision,
        };
        if outcome == ApplyOutcome::AlreadyApplied {
            return Ok(DiffApplication::AlreadyApplied(ack));
        }

        for change in &diff.presentation {
            match change {
                PresentationChange::Bind(binding) => {
                    next.presentation
                        .bindings
                        .retain(|existing| existing.instance != binding.instance);
                    next.presentation.bindings.push(binding.clone());
                }
                PresentationChange::Unbind { instance } => next
                    .presentation
                    .bindings
                    .retain(|binding| binding.instance != *instance),
                PresentationChange::ReplaceOffers { key, offers } => {
                    next.presentation.offers.insert(key.clone(), offers.clone());
                }
                PresentationChange::RemoveOffers { key } => {
                    next.presentation.offers.remove(key);
                }
                PresentationChange::InvalidateResource { resource } => {
                    next_resources.remove(&(diff.session.clone(), *resource));
                }
            }
        }
        validate_presentation(&next).map_err(ClientDiffError::InvalidPresentation)?;
        if let Some(status) = diff.status {
            next.status = status;
            if status == SessionStatus::Revoked && next.cache_policy.purge_on_revocation {
                next_resources.retain(|(session, _), _| session != &diff.session);
            }
        }
        self.mounted.insert(diff.session.clone(), next);
        self.resources = next_resources;
        Ok(DiffApplication::Applied(ack))
    }

    /// Verify and cache one resource only if this session advertised it.
    pub fn apply_resource(&mut self, response: ResourceResponse) -> Result<(), ResourceCacheError> {
        let mounted = self
            .mounted
            .get(&response.session)
            .ok_or(ResourceCacheError::UnknownSession)?;
        if !response.has_valid_address() {
            return Err(ResourceCacheError::AddressMismatch);
        }
        let offers = mounted.presentation.offers.values().flatten();
        let mut advertised = false;
        let mut size_matches = false;
        for offer in offers {
            if offer.resource == response.resource {
                advertised = true;
                size_matches |= offer.byte_size == response.bytes.len() as u64;
            }
        }
        if !advertised {
            return Err(ResourceCacheError::UnadvertisedResource);
        }
        if !size_matches {
            return Err(ResourceCacheError::SizeMismatch);
        }
        self.resources
            .insert((response.session, response.resource), response.bytes);
        Ok(())
    }

    /// Resolve the richest supported offer, or a semantic placeholder when no
    /// offered codec fits the client's capabilities.
    pub fn resolve(
        &self,
        session: &ProjectionSession,
        instance: InstanceId,
        profile: &CapabilityProfile,
    ) -> Result<PresentationResolution, ResolutionError> {
        let mounted = self
            .mounted
            .get(session)
            .ok_or(ResolutionError::UnknownSession)?;
        if mounted.scene.active_item(instance).is_none() {
            return Err(ResolutionError::UnknownPresentation);
        }
        let offers = mounted
            .presentation
            .offers_for(instance)
            .ok_or(ResolutionError::UnknownPresentation)?;
        let Some(offer) = offers.iter().find(|offer| profile.supports(offer.requires)) else {
            let semantics = offers
                .first()
                .ok_or(ResolutionError::UnknownPresentation)?
                .semantics
                .clone();
            return Ok(PresentationResolution::Ready(ResolvedPresentation {
                semantics,
                content: ResolvedContent::LabeledPlaceholder,
            }));
        };

        let Some(bytes) = self.resources.get(&(session.clone(), offer.resource)) else {
            return Ok(PresentationResolution::NeedsResource(ResourceRequest {
                session: session.clone(),
                resource: offer.resource,
            }));
        };

        let content = match &offer.codec {
            PresentationCodec::NativeGlyphV1 => ResolvedContent::NativeGlyph(
                serde_json::from_slice(bytes).map_err(|_| ResolutionError::InvalidPayload)?,
            ),
            PresentationCodec::PortableCardV1 => ResolvedContent::PortableCard(
                serde_json::from_slice(bytes).map_err(|_| ResolutionError::InvalidPayload)?,
            ),
            PresentationCodec::ImageV1 { mime_type } => ResolvedContent::Image {
                mime_type: mime_type.clone(),
                bytes: bytes.clone(),
            },
        };
        Ok(PresentationResolution::Ready(ResolvedPresentation {
            semantics: offer.semantics.clone(),
            content,
        }))
    }

    /// The semantic tree is available from the manifest before optional bytes
    /// arrive, so fallback does not erase names or actions.
    pub fn accessibility_tree(
        &self,
        session: &ProjectionSession,
        profile: &CapabilityProfile,
    ) -> Result<AccessibilityTree, ResolutionError> {
        let mounted = self
            .mounted
            .get(session)
            .ok_or(ResolutionError::UnknownSession)?;
        let mut children = Vec::new();
        for (instance, _) in mounted.scene.active_items_in_order() {
            let Some(offers) = mounted.presentation.offers_for(instance) else {
                continue;
            };
            let semantics = offers
                .iter()
                .find(|offer| profile.supports(offer.requires))
                .or_else(|| offers.first())
                .ok_or(ResolutionError::UnknownPresentation)?
                .semantics
                .clone();
            children.push(AccessibleItem {
                instance,
                label: semantics.label,
                role: semantics.role,
                actions: semantics.actions,
            });
        }
        Ok(AccessibilityTree {
            label: "Graphshell projection".into(),
            children,
        })
    }

    pub fn acknowledgement(&self, session: &ProjectionSession) -> Option<ProjectionAck> {
        let mounted = self.mounted.get(session)?;
        Some(ProjectionAck {
            session: session.clone(),
            epoch: mounted.scene.epoch,
            revision: mounted.scene.revision,
        })
    }

    pub fn resume_request(&self, session: &ProjectionSession) -> Option<ResumeRequest> {
        let ack = self.acknowledgement(session)?;
        Some(ResumeRequest {
            session: ack.session,
            epoch: ack.epoch,
            revision: ack.revision,
        })
    }

    /// Apply one endpoint resume reply as a transaction. A broken replay keeps
    /// the last acknowledged scene available for stale display.
    pub fn apply_resume(
        &mut self,
        session: &ProjectionSession,
        reply: ResumeReply,
    ) -> Result<ResumeApplication, ResumeApplyError> {
        let mut next = self.clone();
        match reply {
            ResumeReply::Current(ack) => {
                if &ack.session != session {
                    return Err(ResumeApplyError::WrongSession);
                }
                let current = next
                    .acknowledgement(session)
                    .ok_or(ResumeApplyError::UnknownSession)?;
                if current.epoch != ack.epoch || current.revision != ack.revision {
                    return Ok(ResumeApplication::Resynchronize(ResumeRequest {
                        session: session.clone(),
                        epoch: current.epoch,
                        revision: current.revision,
                    }));
                }
                next.mounted.get_mut(session).unwrap().status = SessionStatus::Live;
                *self = next;
                Ok(ResumeApplication::Current(ack))
            }
            ResumeReply::Diffs(diffs) => {
                let mut last_ack = next
                    .acknowledgement(session)
                    .ok_or(ResumeApplyError::UnknownSession)?;
                for diff in diffs {
                    if &diff.session != session {
                        return Err(ResumeApplyError::WrongSession);
                    }
                    match next
                        .apply_diff(&diff)
                        .map_err(ResumeApplyError::InvalidDiff)?
                    {
                        DiffApplication::Applied(ack) | DiffApplication::AlreadyApplied(ack) => {
                            last_ack = ack
                        }
                        DiffApplication::Resynchronize(request) => {
                            return Ok(ResumeApplication::Resynchronize(request));
                        }
                    }
                }
                next.mounted.get_mut(session).unwrap().status = SessionStatus::Live;
                *self = next;
                Ok(ResumeApplication::Applied(last_ack))
            }
            ResumeReply::Snapshot(snapshot) => {
                if &snapshot.session != session {
                    return Err(ResumeApplyError::WrongSession);
                }
                next.apply_snapshot(*snapshot)
                    .map_err(ResumeApplyError::InvalidSnapshot)?;
                let ack = next
                    .acknowledgement(session)
                    .ok_or(ResumeApplyError::UnknownSession)?;
                *self = next;
                Ok(ResumeApplication::Applied(ack))
            }
        }
    }

    pub fn mark_stale(&mut self, session: &ProjectionSession) {
        if let Some(mounted) = self.mounted.get_mut(session) {
            mounted.status = SessionStatus::Stale;
        }
    }

    pub fn mark_disconnected(&mut self, session: &ProjectionSession) {
        if let Some(mounted) = self.mounted.get_mut(session) {
            mounted.status = SessionStatus::Disconnected;
        }
    }

    pub fn mounted(&self, session: &ProjectionSession) -> Option<&MountedScene> {
        self.mounted.get(session)
    }

    pub fn forget_session(&mut self, session: &ProjectionSession) {
        self.mounted.remove(session);
        self.resources.retain(|(owner, _), _| owner != session);
    }

    pub fn persist_session<S: ProjectionStore>(
        &self,
        session: &ProjectionSession,
        now_ms: u64,
        store: &mut S,
    ) -> Result<(), PersistenceError<S::Error>> {
        let mounted = self
            .mounted
            .get(session)
            .ok_or(PersistenceError::UnknownSession)?;
        check_persistence_policy(&mounted.cache_policy, now_ms, store.protection())?;
        let advertised = mounted
            .presentation
            .offers
            .values()
            .flatten()
            .map(|offer| offer.resource)
            .collect::<BTreeSet<_>>();
        let resources = self
            .resources
            .iter()
            .filter(|((owner, hash), _)| owner == session && advertised.contains(hash))
            .map(|((_, hash), bytes)| PersistedResource {
                hash: *hash,
                bytes: bytes.clone(),
            })
            .collect();
        let record = PersistedProjection {
            version: 1,
            session: session.clone(),
            mounted: mounted.clone(),
            resources,
        };
        let bytes = serde_json::to_vec(&record).map_err(|_| PersistenceError::Corrupt)?;
        store.put(session, &bytes).map_err(PersistenceError::Store)
    }

    pub fn restore_session<S: ProjectionStore>(
        &mut self,
        session: &ProjectionSession,
        now_ms: u64,
        store: &S,
    ) -> Result<(), PersistenceError<S::Error>> {
        let bytes = store
            .get(session)
            .map_err(PersistenceError::Store)?
            .ok_or(PersistenceError::Missing)?;
        let record: PersistedProjection =
            serde_json::from_slice(&bytes).map_err(|_| PersistenceError::Corrupt)?;
        if record.version != 1 || &record.session != session {
            return Err(PersistenceError::Corrupt);
        }
        check_persistence_policy(&record.mounted.cache_policy, now_ms, store.protection())?;
        record
            .mounted
            .scene
            .validate()
            .map_err(|_| PersistenceError::Corrupt)?;

        validate_presentation(&record.mounted).map_err(|_| PersistenceError::Corrupt)?;
        let mut restored = ClientState::default();
        let mut restored_mounted = record.mounted.clone();
        restored_mounted.status = SessionStatus::Stale;
        restored.mounted.insert(session.clone(), restored_mounted);
        for resource in record.resources {
            restored
                .apply_resource(ResourceResponse {
                    session: session.clone(),
                    resource: resource.hash,
                    bytes: resource.bytes,
                })
                .map_err(|_| PersistenceError::Corrupt)?;
        }
        self.forget_session(session);
        self.mounted.extend(restored.mounted);
        self.resources.extend(restored.resources);
        Ok(())
    }

    pub fn remove_persisted<S: ProjectionStore>(
        session: &ProjectionSession,
        store: &mut S,
    ) -> Result<(), PersistenceError<S::Error>> {
        store.remove(session).map_err(PersistenceError::Store)
    }
}

fn validate_presentation(mounted: &MountedScene) -> Result<(), String> {
    let mut instances = BTreeSet::new();
    for binding in &mounted.presentation.bindings {
        if mounted.scene.active_item(binding.instance).is_none() {
            return Err(format!(
                "binding for absent or tombstoned item {}",
                binding.instance.0
            ));
        }
        if !instances.insert(binding.instance.0) {
            return Err(format!("duplicate binding for item {}", binding.instance.0));
        }
    }
    Ok(())
}

fn check_persistence_policy<E>(
    policy: &CachePolicy,
    now_ms: u64,
    protection: StoreProtection,
) -> Result<(), PersistenceError<E>> {
    if policy.expires_at_ms.is_some_and(|expiry| now_ms >= expiry) {
        return Err(PersistenceError::Expired);
    }
    match policy.retention {
        CacheRetention::MemoryOnly => Err(PersistenceError::NotPermitted),
        CacheRetention::EncryptedPersistent if protection != StoreProtection::EncryptedAtRest => {
            Err(PersistenceError::RequiresEncryptedStore)
        }
        CacheRetention::EncryptedPersistent | CacheRetention::Exportable => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphshell_protocol::{
        BoundsRelationship, PresentationBinding, PresentationCapability, PresentationKey,
        PresentationOffer, ProtocolVersion,
    };
    use sceno::{Footprint, ProjectedItem, Representation, Scene, Size2, SourceRef, Transform2};
    use scenotime::{Revision, SceneDiff, SceneEpoch, SceneOp};

    fn semantics(label: &str, role: SemanticRole) -> PresentationSemantics {
        PresentationSemantics {
            label: label.into(),
            role,
            bounds: BoundsRelationship::FillFootprint,
            actions: Vec::new(),
        }
    }

    fn scene_with_one_item() -> Scene {
        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new("fixture", "one"));
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::IDENTITY,
            footprint: Footprint::Rect {
                size: Size2::new(40.0, 24.0),
            },
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
        });
        scene
    }

    fn snapshot_with_offer(
        session: &ProjectionSession,
        codec: PresentationCodec,
        requires: PresentationCapability,
        semantics: PresentationSemantics,
        bytes: &[u8],
        retention: CacheRetention,
    ) -> ProjectionSnapshot {
        let key = PresentationKey("item:0".into());
        let hash = ContentHash::of(bytes);
        let mut presentation = PresentationManifest {
            bindings: vec![PresentationBinding {
                instance: InstanceId(0),
                key: key.clone(),
            }],
            ..PresentationManifest::default()
        };
        presentation.offers.insert(
            key,
            vec![PresentationOffer {
                codec,
                resource: hash,
                byte_size: bytes.len() as u64,
                requires,
                semantics,
            }],
        );
        ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: session.clone(),
            scene: SceneSnapshot::from_dense(SceneEpoch(1), Revision(4), scene_with_one_item())
                .unwrap(),
            presentation,
            cache_policy: CachePolicy {
                retention,
                expires_at_ms: None,
                purge_on_revocation: true,
            },
        }
    }

    #[test]
    fn snapshot_and_resource_stay_scoped_to_their_session() {
        let session = ProjectionSession("loopback:one".into());
        let glyph = NativeGlyphV1 {
            label: "One".into(),
            icon: Some("1".into()),
            color: None,
        };
        let bytes = serde_json::to_vec(&glyph).unwrap();
        let snapshot = snapshot_with_offer(
            &session,
            PresentationCodec::NativeGlyphV1,
            PresentationCapability::NativeGlyph,
            semantics("One", SemanticRole::Graphic),
            &bytes,
            CacheRetention::MemoryOnly,
        );
        let hash = ContentHash::of(&bytes);
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot).unwrap();
        client
            .apply_resource(ResourceResponse {
                session: session.clone(),
                resource: hash,
                bytes,
            })
            .unwrap();
        client.mark_stale(&session);
        assert_eq!(
            client.mounted(&session).unwrap().status,
            SessionStatus::Stale
        );
        assert!(matches!(
            client.resolve(
                &session,
                InstanceId(0),
                &CapabilityProfile::new([PresentationCapability::NativeGlyph])
            ),
            Ok(PresentationResolution::Ready(ResolvedPresentation {
                content: ResolvedContent::NativeGlyph(_),
                ..
            }))
        ));
    }

    #[test]
    fn unsupported_image_becomes_a_labeled_placeholder_without_fetching() {
        let session = ProjectionSession("loopback:image".into());
        let snapshot = snapshot_with_offer(
            &session,
            PresentationCodec::ImageV1 {
                mime_type: "image/svg+xml".into(),
            },
            PresentationCapability::Image,
            semantics("Map tile", SemanticRole::Image),
            b"<svg/>",
            CacheRetention::MemoryOnly,
        );
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot).unwrap();
        assert_eq!(
            client
                .resolve(
                    &session,
                    InstanceId(0),
                    &CapabilityProfile::new([PresentationCapability::NativeGlyph])
                )
                .unwrap(),
            PresentationResolution::Ready(ResolvedPresentation {
                semantics: semantics("Map tile", SemanticRole::Image),
                content: ResolvedContent::LabeledPlaceholder,
            })
        );
    }

    #[test]
    fn missing_base_requests_resync_without_changing_stale_display() {
        let session = ProjectionSession("loopback:diff".into());
        let glyph = serde_json::to_vec(&NativeGlyphV1 {
            label: "One".into(),
            icon: None,
            color: None,
        })
        .unwrap();
        let mut client = ClientState::default();
        client
            .apply_snapshot(snapshot_with_offer(
                &session,
                PresentationCodec::NativeGlyphV1,
                PresentationCapability::NativeGlyph,
                semantics("One", SemanticRole::Graphic),
                &glyph,
                CacheRetention::MemoryOnly,
            ))
            .unwrap();
        client.mark_disconnected(&session);
        let before = client.clone();
        let result = client
            .apply_diff(&ProjectionDiff {
                version: ProtocolVersion::V1,
                session: session.clone(),
                scene: SceneDiff {
                    epoch: SceneEpoch(1),
                    base: Revision(8),
                    revision: Revision(9),
                    operations: Vec::new(),
                },
                presentation: Vec::new(),
                status: None,
            })
            .unwrap();
        assert_eq!(
            result,
            DiffApplication::Resynchronize(ResumeRequest {
                session,
                epoch: SceneEpoch(1),
                revision: Revision(4)
            })
        );
        assert_eq!(client, before);
    }

    #[test]
    fn scene_and_presentation_diff_commit_once() {
        let session = ProjectionSession("loopback:diff".into());
        let glyph = serde_json::to_vec(&NativeGlyphV1 {
            label: "One".into(),
            icon: None,
            color: None,
        })
        .unwrap();
        let mut client = ClientState::default();
        client
            .apply_snapshot(snapshot_with_offer(
                &session,
                PresentationCodec::NativeGlyphV1,
                PresentationCapability::NativeGlyph,
                semantics("One", SemanticRole::Graphic),
                &glyph,
                CacheRetention::MemoryOnly,
            ))
            .unwrap();
        let new_glyph = serde_json::to_vec(&NativeGlyphV1 {
            label: "Two".into(),
            icon: None,
            color: None,
        })
        .unwrap();
        let key = PresentationKey("item:1".into());
        let source = client.mounted(&session).unwrap().scene.tables.items[0]
            .as_ref()
            .unwrap()
            .source;
        let diff = ProjectionDiff {
            version: ProtocolVersion::V1,
            session: session.clone(),
            scene: SceneDiff {
                epoch: SceneEpoch(1),
                base: Revision(4),
                revision: Revision(5),
                operations: vec![SceneOp::AddItem {
                    index: InstanceId(1),
                    value: ProjectedItem {
                        source,
                        space: Scene::WORLD,
                        transform: Transform2::translation(10.0, 0.0),
                        footprint: Footprint::Point,
                        representation: Representation::Glyph,
                        layer: 0,
                        visible: true,
                        hit: None,
                    },
                    order: -1,
                }],
            },
            presentation: vec![
                PresentationChange::Bind(PresentationBinding {
                    instance: InstanceId(1),
                    key: key.clone(),
                }),
                PresentationChange::ReplaceOffers {
                    key,
                    offers: vec![PresentationOffer {
                        codec: PresentationCodec::NativeGlyphV1,
                        resource: ContentHash::of(&new_glyph),
                        byte_size: new_glyph.len() as u64,
                        requires: PresentationCapability::NativeGlyph,
                        semantics: semantics("Two", SemanticRole::Graphic),
                    }],
                },
            ],
            status: Some(SessionStatus::Live),
        };
        assert!(matches!(
            client.apply_diff(&diff),
            Ok(DiffApplication::Applied(_))
        ));
        let once = client.clone();
        assert!(matches!(
            client.apply_diff(&diff),
            Ok(DiffApplication::AlreadyApplied(_))
        ));
        assert_eq!(client, once);
        assert_eq!(
            client
                .mounted(&session)
                .unwrap()
                .scene
                .active_items_in_order()[0]
                .0,
            InstanceId(1)
        );
        assert_eq!(
            client.mounted(&session).unwrap().status,
            SessionStatus::Live
        );
    }

    #[test]
    fn resource_and_status_changes_commit_with_the_scene_revision() {
        let session = ProjectionSession("loopback:resource-change".into());
        let first = serde_json::to_vec(&NativeGlyphV1 {
            label: "First".into(),
            icon: None,
            color: None,
        })
        .unwrap();
        let second = serde_json::to_vec(&NativeGlyphV1 {
            label: "Second".into(),
            icon: None,
            color: None,
        })
        .unwrap();
        let snapshot = snapshot_with_offer(
            &session,
            PresentationCodec::NativeGlyphV1,
            PresentationCapability::NativeGlyph,
            semantics("First", SemanticRole::Graphic),
            &first,
            CacheRetention::MemoryOnly,
        );
        let key = snapshot.presentation.bindings[0].key.clone();
        let first_hash = ContentHash::of(&first);
        let second_hash = ContentHash::of(&second);
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot).unwrap();
        client
            .apply_resource(ResourceResponse {
                session: session.clone(),
                resource: first_hash,
                bytes: first,
            })
            .unwrap();

        let result = client
            .apply_diff(&ProjectionDiff {
                version: ProtocolVersion::V1,
                session: session.clone(),
                scene: SceneDiff {
                    epoch: SceneEpoch(1),
                    base: Revision(4),
                    revision: Revision(5),
                    operations: Vec::new(),
                },
                presentation: vec![
                    PresentationChange::InvalidateResource {
                        resource: first_hash,
                    },
                    PresentationChange::ReplaceOffers {
                        key,
                        offers: vec![PresentationOffer {
                            codec: PresentationCodec::NativeGlyphV1,
                            resource: second_hash,
                            byte_size: second.len() as u64,
                            requires: PresentationCapability::NativeGlyph,
                            semantics: semantics("Second", SemanticRole::Graphic),
                        }],
                    },
                ],
                status: Some(SessionStatus::Stale),
            })
            .unwrap();
        assert!(matches!(result, DiffApplication::Applied(_)));
        assert_eq!(
            client.mounted(&session).unwrap().status,
            SessionStatus::Stale
        );
        assert_eq!(
            client
                .resolve(
                    &session,
                    InstanceId(0),
                    &CapabilityProfile::new([PresentationCapability::NativeGlyph]),
                )
                .unwrap(),
            PresentationResolution::NeedsResource(ResourceRequest {
                session,
                resource: second_hash,
            })
        );
    }

    #[derive(Default)]
    struct MemoryStore {
        encrypted: bool,
        records: BTreeMap<ProjectionSession, Vec<u8>>,
    }

    impl ProjectionStore for MemoryStore {
        type Error = ();

        fn protection(&self) -> StoreProtection {
            if self.encrypted {
                StoreProtection::EncryptedAtRest
            } else {
                StoreProtection::Plain
            }
        }

        fn put(&mut self, session: &ProjectionSession, bytes: &[u8]) -> Result<(), Self::Error> {
            self.records.insert(session.clone(), bytes.to_vec());
            Ok(())
        }

        fn get(&self, session: &ProjectionSession) -> Result<Option<Vec<u8>>, Self::Error> {
            Ok(self.records.get(session).cloned())
        }

        fn remove(&mut self, session: &ProjectionSession) -> Result<(), Self::Error> {
            self.records.remove(session);
            Ok(())
        }
    }

    #[test]
    fn permitted_encrypted_cache_restores_scene_and_resource_once() {
        let session = ProjectionSession("loopback:persist".into());
        let glyph_value = NativeGlyphV1 {
            label: "Persisted".into(),
            icon: Some("P".into()),
            color: None,
        };
        let glyph = serde_json::to_vec(&glyph_value).unwrap();
        let hash = ContentHash::of(&glyph);
        let mut client = ClientState::default();
        client
            .apply_snapshot(snapshot_with_offer(
                &session,
                PresentationCodec::NativeGlyphV1,
                PresentationCapability::NativeGlyph,
                semantics("Persisted", SemanticRole::Graphic),
                &glyph,
                CacheRetention::EncryptedPersistent,
            ))
            .unwrap();
        client
            .apply_resource(ResourceResponse {
                session: session.clone(),
                resource: hash,
                bytes: glyph,
            })
            .unwrap();

        let mut store = MemoryStore {
            encrypted: true,
            ..MemoryStore::default()
        };
        client.persist_session(&session, 10, &mut store).unwrap();
        let mut restored = ClientState::default();
        restored.restore_session(&session, 11, &store).unwrap();
        assert_eq!(
            restored.acknowledgement(&session).unwrap().revision,
            Revision(4)
        );
        assert_eq!(
            restored.mounted(&session).unwrap().status,
            SessionStatus::Stale
        );
        assert_eq!(
            restored
                .resolve(
                    &session,
                    InstanceId(0),
                    &CapabilityProfile::new([PresentationCapability::NativeGlyph])
                )
                .unwrap(),
            PresentationResolution::Ready(ResolvedPresentation {
                semantics: semantics("Persisted", SemanticRole::Graphic),
                content: ResolvedContent::NativeGlyph(glyph_value)
            })
        );
    }

    #[test]
    fn memory_only_cache_refuses_persistence() {
        let session = ProjectionSession("loopback:memory".into());
        let glyph = serde_json::to_vec(&NativeGlyphV1 {
            label: "Memory".into(),
            icon: None,
            color: None,
        })
        .unwrap();
        let mut client = ClientState::default();
        client
            .apply_snapshot(snapshot_with_offer(
                &session,
                PresentationCodec::NativeGlyphV1,
                PresentationCapability::NativeGlyph,
                semantics("Memory", SemanticRole::Graphic),
                &glyph,
                CacheRetention::MemoryOnly,
            ))
            .unwrap();
        let mut store = MemoryStore::default();
        assert_eq!(
            client.persist_session(&session, 0, &mut store),
            Err(PersistenceError::NotPermitted)
        );
    }
}
