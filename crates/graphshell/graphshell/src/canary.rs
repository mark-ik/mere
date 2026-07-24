use std::collections::BTreeMap;

use graphshell_client::{
    AccessibilityTree, ClientState, PresentationResolution, ResolutionError, ResolvedPresentation,
    ResourceCacheError, SnapshotApplyError,
};
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionSource};
use graphshell_protocol::{
    AdvertisedAction, BoundsRelationship, CachePolicy, CapabilityProfile, CardValueV1, ContentHash,
    IntentEffect, IntentInvocation, IntentReference, IntentResult, NativeGlyphV1, PortableCardV1,
    PresentationBinding, PresentationCapability, PresentationCodec, PresentationKey,
    PresentationManifest, PresentationOffer, PresentationSemantics, ProjectionRequest,
    ProjectionSession, ProjectionSnapshot, ProtocolVersion, ResourceRequest, ResourceResponse,
    SemanticRole,
};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Rect, Representation, Scene, Score, Size2,
    SourceRef, Transform2, Vec2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};

const FIXTURE_SESSION: &str = "loopback:g1-presentation";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanaryError {
    WrongSession,
    MissingResource,
    Cache(ResourceCacheError),
    Resolution(ResolutionError),
    Snapshot(SnapshotApplyError),
}

impl From<ResourceCacheError> for CanaryError {
    fn from(value: ResourceCacheError) -> Self {
        Self::Cache(value)
    }
}

impl From<ResolutionError> for CanaryError {
    fn from(value: ResolutionError) -> Self {
        Self::Resolution(value)
    }
}

impl From<SnapshotApplyError> for CanaryError {
    fn from(value: SnapshotApplyError) -> Self {
        Self::Snapshot(value)
    }
}

/// An in-memory endpoint whose only authority is the deterministic G1 fixture.
pub struct FixtureEndpoint {
    session: ProjectionSession,
    snapshot: ProjectionSnapshot,
    resources: BTreeMap<ContentHash, Vec<u8>>,
}

impl FixtureEndpoint {
    pub fn new() -> Self {
        let session = ProjectionSession(FIXTURE_SESSION.into());
        let mut scene = Scene::new();
        let note = scene.intern_source(SourceRef::new("fixture.graphshell", "note:recent"));
        let map = scene.intern_source(SourceRef::new("fixture.graphshell", "tile:coast"));
        scene.items.push(ProjectedItem {
            source: note,
            space: Scene::WORLD,
            transform: Transform2::translation(156.0, 146.0),
            footprint: Footprint::Rect {
                size: Size2::new(248.0, 168.0),
            },
            representation: Representation::Card,
            layer: 1,
            visible: true,
            hit: None,
        });
        scene.items.push(ProjectedItem {
            source: map,
            space: Scene::WORLD,
            transform: Transform2::translation(454.0, 146.0),
            footprint: Footprint::Rect {
                size: Size2::new(248.0, 168.0),
            },
            representation: Representation::Sprite,
            layer: 0,
            visible: true,
            hit: None,
        });
        scene.bounds = Rect::new(Vec2::new(32.0, 62.0), Size2::new(546.0, 168.0));
        scene.generation = 7;

        let open_note = AdvertisedAction {
            intent: IntentReference("fixture.open-note".into()),
            label: "Open field note".into(),
            explanation: "Open the disclosed note in its owning application.".into(),
            payload_schema: "graphshell.fixture/open-note/v1".into(),
            effect: IntentEffect::DomainTruth,
        };
        let inspect_map = AdvertisedAction {
            intent: IntentReference("fixture.inspect-tile".into()),
            label: "Inspect map tile".into(),
            explanation: "Inspect the disclosed map tile without changing source truth.".into(),
            payload_schema: "graphshell.fixture/inspect-tile/v1".into(),
            effect: IntentEffect::Curation,
        };

        let card = PortableCardV1 {
            title: "Projection boundary".into(),
            values: vec![
                CardValueV1 {
                    label: "Source".into(),
                    value: "Owned application".into(),
                },
                CardValueV1 {
                    label: "State".into(),
                    value: "Live · revision 1".into(),
                },
            ],
            badges: vec!["portable card".into(), "granted".into()],
            media: Vec::new(),
        };
        let card_bytes = serde_json::to_vec(&card).expect("fixture card serializes");
        let glyph = NativeGlyphV1 {
            label: "Projection boundary".into(),
            icon: Some("◎".into()),
            color: Some("#d8a657".into()),
        };
        let glyph_bytes = serde_json::to_vec(&glyph).expect("fixture glyph serializes");
        let image_bytes = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 280" role="img" aria-label="Coastal map tile"><rect width="480" height="280" rx="24" fill="#163044"/><path d="M0 214 C76 176 110 188 164 132 C223 71 280 111 325 65 C374 16 431 40 480 18 V280 H0Z" fill="#c8b38a"/><path d="M0 214 C76 176 110 188 164 132 C223 71 280 111 325 65 C374 16 431 40 480 18" fill="none" stroke="#ebd9b2" stroke-width="8"/><circle cx="257" cy="104" r="12" fill="#e46d5c"/><circle cx="257" cy="104" r="25" fill="none" stroke="#e46d5c" stroke-width="3" opacity=".55"/><text x="282" y="99" fill="#fff8e8" font-family="system-ui" font-size="18" font-weight="700">FIELD NODE</text><text x="282" y="124" fill="#bad1df" font-family="system-ui" font-size="14">42.36° N · 71.06° W</text></svg>"##
            .as_bytes()
            .to_vec();

        let card_hash = ContentHash::of(&card_bytes);
        let glyph_hash = ContentHash::of(&glyph_bytes);
        let image_hash = ContentHash::of(&image_bytes);
        let note_key = PresentationKey("fixture:note".into());
        let map_key = PresentationKey("fixture:map".into());
        let mut presentation = PresentationManifest {
            bindings: vec![
                PresentationBinding {
                    instance: InstanceId(0),
                    key: note_key.clone(),
                },
                PresentationBinding {
                    instance: InstanceId(1),
                    key: map_key.clone(),
                },
            ],
            ..PresentationManifest::default()
        };
        presentation.offers.insert(
            note_key,
            vec![
                PresentationOffer {
                    codec: PresentationCodec::PortableCardV1,
                    resource: card_hash,
                    byte_size: card_bytes.len() as u64,
                    requires: PresentationCapability::PortableCard,
                    semantics: PresentationSemantics {
                        label: "Projection boundary card".into(),
                        role: SemanticRole::Article,
                        bounds: BoundsRelationship::FillFootprint,
                        actions: vec![open_note.clone()],
                    },
                },
                PresentationOffer {
                    codec: PresentationCodec::NativeGlyphV1,
                    resource: glyph_hash,
                    byte_size: glyph_bytes.len() as u64,
                    requires: PresentationCapability::NativeGlyph,
                    semantics: PresentationSemantics {
                        label: "Projection boundary glyph".into(),
                        role: SemanticRole::Graphic,
                        bounds: BoundsRelationship::FitWithinFootprint,
                        actions: vec![open_note],
                    },
                },
            ],
        );
        presentation.offers.insert(
            map_key,
            vec![PresentationOffer {
                codec: PresentationCodec::ImageV1 {
                    mime_type: "image/svg+xml".into(),
                },
                resource: image_hash,
                byte_size: image_bytes.len() as u64,
                requires: PresentationCapability::Image,
                semantics: PresentationSemantics {
                    label: "Coastal map tile".into(),
                    role: SemanticRole::Image,
                    bounds: BoundsRelationship::FillFootprint,
                    actions: vec![inspect_map],
                },
            }],
        );

        let resources = BTreeMap::from([
            (card_hash, card_bytes),
            (glyph_hash, glyph_bytes),
            (image_hash, image_bytes),
        ]);
        let scene = SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), scene)
            .expect("fixture scene is valid");
        let snapshot = ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: session.clone(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        };
        Self {
            session,
            snapshot,
            resources,
        }
    }
}

impl Default for FixtureEndpoint {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectionSource for FixtureEndpoint {
    type Error = CanaryError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session {
            return Err(CanaryError::WrongSession);
        }
        Ok(self.snapshot.clone())
    }
}

impl PresentationSource for FixtureEndpoint {
    type Error = CanaryError;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session {
            return Err(CanaryError::WrongSession);
        }
        let bytes = self
            .resources
            .get(&request.resource)
            .cloned()
            .ok_or(CanaryError::MissingResource)?;
        Ok(ResourceResponse {
            session: request.session,
            resource: request.resource,
            bytes,
        })
    }
}

impl IntentSink for FixtureEndpoint {
    type Error = CanaryError;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session {
            return Err(CanaryError::WrongSession);
        }
        if intent.observed_epoch != self.snapshot.scene.epoch
            || intent.observed_revision != self.snapshot.scene.revision
        {
            return Ok(IntentResult::Stale {
                current_epoch: self.snapshot.scene.epoch,
                current_revision: self.snapshot.scene.revision,
            });
        }
        Ok(match intent.intent.as_str() {
            "fixture.open-note" | "fixture.inspect-tile" => IntentResult::Accepted,
            _ => IntentResult::Rejected {
                reason: "intent was not advertised by this projection".into(),
            },
        })
    }
}

/// The result of one complete in-memory projection and resource exchange.
pub struct CanaryRun {
    pub session: ProjectionSession,
    pub rich: Vec<ResolvedPresentation>,
    pub compact: Vec<ResolvedPresentation>,
    pub rich_accessibility: AccessibilityTree,
    pub compact_accessibility: AccessibilityTree,
}

pub fn run_loopback_canary() -> Result<CanaryRun, CanaryError> {
    let mut endpoint = FixtureEndpoint::new();
    let session = ProjectionSession(FIXTURE_SESSION.into());
    let request = ProjectionRequest {
        version: ProtocolVersion::V1,
        session: session.clone(),
        score: Score::new(Arrangement::Spiral(Default::default())),
    };
    let snapshot = endpoint.snapshot(request)?;
    let item_count = snapshot.scene.active_item_count();
    let mut client = ClientState::default();
    client.apply_snapshot(snapshot)?;

    let rich_profile = CapabilityProfile::new([
        PresentationCapability::NativeGlyph,
        PresentationCapability::PortableCard,
        PresentationCapability::Image,
    ]);
    let compact_profile = CapabilityProfile::new([PresentationCapability::NativeGlyph]);
    let rich = resolve_all(
        &mut endpoint,
        &mut client,
        &session,
        &rich_profile,
        item_count,
    )?;
    let compact = resolve_all(
        &mut endpoint,
        &mut client,
        &session,
        &compact_profile,
        item_count,
    )?;
    let rich_accessibility = client.accessibility_tree(&session, &rich_profile)?;
    let compact_accessibility = client.accessibility_tree(&session, &compact_profile)?;
    Ok(CanaryRun {
        session,
        rich,
        compact,
        rich_accessibility,
        compact_accessibility,
    })
}

fn resolve_all(
    endpoint: &mut FixtureEndpoint,
    client: &mut ClientState,
    session: &ProjectionSession,
    profile: &CapabilityProfile,
    item_count: usize,
) -> Result<Vec<ResolvedPresentation>, CanaryError> {
    let mut resolved = Vec::with_capacity(item_count);
    for index in 0..item_count {
        loop {
            match client.resolve(session, InstanceId(index as u32), profile)? {
                PresentationResolution::Ready(presentation) => {
                    resolved.push(presentation);
                    break;
                }
                PresentationResolution::NeedsResource(request) => {
                    let response = endpoint.resource(request)?;
                    client.apply_resource(response)?;
                }
            }
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphshell_client::ResolvedContent;

    #[test]
    fn loopback_resolves_rich_and_compact_profiles() {
        let run = run_loopback_canary().unwrap();
        assert!(matches!(
            run.rich[0].content,
            ResolvedContent::PortableCard(_)
        ));
        assert!(matches!(run.rich[1].content, ResolvedContent::Image { .. }));
        assert!(matches!(
            run.compact[0].content,
            ResolvedContent::NativeGlyph(_)
        ));
        assert_eq!(run.compact[1].content, ResolvedContent::LabeledPlaceholder);
    }

    #[test]
    fn both_profiles_keep_advertised_actions_accessible() {
        let run = run_loopback_canary().unwrap();
        for tree in [&run.rich_accessibility, &run.compact_accessibility] {
            assert_eq!(tree.children.len(), 2);
            assert_eq!(tree.children[0].actions[0].label, "Open field note");
            assert_eq!(tree.children[1].actions[0].label, "Inspect map tile");
        }
    }
}
