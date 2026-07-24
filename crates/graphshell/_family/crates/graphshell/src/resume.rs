//! Deterministic disconnect and resume acceptance fixture for G2.

use graphshell_client::{ClientState, DiffApplication, ResumeApplication, ResumeApplyError};
use graphshell_endpoint::{ProjectionSource, ResumableProjectionSource};
use graphshell_protocol::{
    BoundsRelationship, CachePolicy, PresentationBinding, PresentationCapability,
    PresentationChange, PresentationCodec, PresentationKey, PresentationManifest,
    PresentationOffer, PresentationSemantics, ProjectionAck, ProjectionDiff, ProjectionRequest,
    ProjectionSession, ProjectionSnapshot, ProtocolVersion, ResumeReply, ResumeRequest,
    SemanticRole, SessionStatus,
};
use sceno::{
    Footprint, InstanceId, ProjectedItem, Representation, Scene, Size2, SourceIx, SourceRef,
    Transform2,
};
use scenotime::{Revision, SceneDiff, SceneEpoch, SceneOp, SceneSnapshot};

const SESSION: &str = "loopback:g2-resume";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeFixtureError {
    WrongSession,
}

/// A source with a two-diff history and an epoch-preserving current snapshot.
pub struct ResumeFixtureEndpoint {
    session: ProjectionSession,
    initial: ProjectionSnapshot,
    current: ProjectionSnapshot,
    history: Vec<ProjectionDiff>,
}

impl ResumeFixtureEndpoint {
    pub fn new() -> Self {
        let session = ProjectionSession(SESSION.into());
        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new("fixture.graphshell", "card:0"));
        scene.items.push(item(source, 0.0));
        let scene = SceneSnapshot::from_dense(SceneEpoch(3), Revision(1), scene)
            .expect("initial fixture scene is valid");
        let key_0 = PresentationKey("fixture:0".into());
        let key_1 = PresentationKey("fixture:1".into());
        let key_2 = PresentationKey("fixture:2".into());
        let mut initial_presentation = PresentationManifest {
            bindings: vec![PresentationBinding {
                instance: InstanceId(0),
                key: key_0.clone(),
            }],
            ..PresentationManifest::default()
        };
        initial_presentation
            .offers
            .insert(key_0.clone(), vec![offer("First", b"first")]);
        let initial = ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: session.clone(),
            scene,
            presentation: initial_presentation,
            cache_policy: CachePolicy::default(),
        };

        let diff_2 = ProjectionDiff {
            version: ProtocolVersion::V1,
            session: session.clone(),
            scene: SceneDiff {
                epoch: SceneEpoch(3),
                base: Revision(1),
                revision: Revision(2),
                operations: vec![SceneOp::AddItem {
                    index: InstanceId(1),
                    value: item(source, 80.0),
                    order: -1,
                }],
            },
            presentation: vec![
                PresentationChange::Bind(PresentationBinding {
                    instance: InstanceId(1),
                    key: key_1.clone(),
                }),
                PresentationChange::ReplaceOffers {
                    key: key_1.clone(),
                    offers: vec![offer("Second", b"second")],
                },
            ],
            status: Some(SessionStatus::Live),
        };
        let diff_3 = ProjectionDiff {
            version: ProtocolVersion::V1,
            session: session.clone(),
            scene: SceneDiff {
                epoch: SceneEpoch(3),
                base: Revision(2),
                revision: Revision(3),
                operations: vec![
                    SceneOp::TombstoneItem {
                        index: InstanceId(0),
                    },
                    SceneOp::AddItem {
                        index: InstanceId(2),
                        value: item(source, 160.0),
                        order: 0,
                    },
                    SceneOp::SetItemLayer {
                        index: InstanceId(1),
                        layer: 4,
                    },
                ],
            },
            presentation: vec![
                PresentationChange::Unbind {
                    instance: InstanceId(0),
                },
                PresentationChange::RemoveOffers { key: key_0 },
                PresentationChange::Bind(PresentationBinding {
                    instance: InstanceId(2),
                    key: key_2.clone(),
                }),
                PresentationChange::ReplaceOffers {
                    key: key_2.clone(),
                    offers: vec![offer("Third", b"third")],
                },
            ],
            status: Some(SessionStatus::Live),
        };

        let mut current_scene = initial.scene.clone();
        current_scene
            .apply_diff(&diff_2.scene)
            .expect("revision 2 is valid");
        current_scene
            .apply_diff(&diff_3.scene)
            .expect("revision 3 is valid");
        let mut current_presentation = PresentationManifest {
            bindings: vec![
                PresentationBinding {
                    instance: InstanceId(1),
                    key: key_1.clone(),
                },
                PresentationBinding {
                    instance: InstanceId(2),
                    key: key_2.clone(),
                },
            ],
            ..PresentationManifest::default()
        };
        current_presentation
            .offers
            .insert(key_1, vec![offer("Second", b"second")]);
        current_presentation
            .offers
            .insert(key_2, vec![offer("Third", b"third")]);
        let current = ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: session.clone(),
            scene: current_scene,
            presentation: current_presentation,
            cache_policy: CachePolicy::default(),
        };
        Self {
            session,
            initial,
            current,
            history: vec![diff_2, diff_3],
        }
    }

    pub fn initial_snapshot(&self) -> ProjectionSnapshot {
        self.initial.clone()
    }

    pub fn diff(&self, revision: Revision) -> ProjectionDiff {
        self.history
            .iter()
            .find(|diff| diff.scene.revision == revision)
            .expect("fixture revision exists")
            .clone()
    }
}

impl Default for ResumeFixtureEndpoint {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectionSource for ResumeFixtureEndpoint {
    type Error = ResumeFixtureError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session {
            return Err(ResumeFixtureError::WrongSession);
        }
        Ok(self.current.clone())
    }
}

impl ResumableProjectionSource for ResumeFixtureEndpoint {
    type Error = ResumeFixtureError;

    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, Self::Error> {
        if request.session != self.session {
            return Err(ResumeFixtureError::WrongSession);
        }
        if request.epoch != self.current.scene.epoch {
            return Ok(ResumeReply::Snapshot(Box::new(self.current.clone())));
        }
        if request.revision == self.current.scene.revision {
            return Ok(ResumeReply::Current(ProjectionAck {
                session: self.session.clone(),
                epoch: self.current.scene.epoch,
                revision: self.current.scene.revision,
            }));
        }
        if let Some(start) = self
            .history
            .iter()
            .position(|diff| diff.scene.base == request.revision)
        {
            return Ok(ResumeReply::Diffs(self.history[start..].to_vec()));
        }
        Ok(ResumeReply::Snapshot(Box::new(self.current.clone())))
    }
}

fn item(source: SourceIx, x: f32) -> ProjectedItem {
    ProjectedItem {
        source,
        space: Scene::WORLD,
        transform: Transform2::translation(x, 0.0),
        footprint: Footprint::Rect {
            size: Size2::new(64.0, 40.0),
        },
        representation: Representation::Card,
        layer: 0,
        visible: true,
        hit: None,
    }
}

fn offer(label: &str, bytes: &[u8]) -> PresentationOffer {
    PresentationOffer {
        codec: PresentationCodec::NativeGlyphV1,
        resource: graphshell_protocol::ContentHash::of(bytes),
        byte_size: bytes.len() as u64,
        requires: PresentationCapability::NativeGlyph,
        semantics: PresentationSemantics {
            label: label.into(),
            role: SemanticRole::Graphic,
            bounds: BoundsRelationship::FitWithinFootprint,
            actions: Vec::new(),
        },
    }
}

/// Disconnect after revision 2, replay revision 3, then compare against the
/// endpoint's complete current snapshot.
pub fn run_resume_canary() -> Result<ClientState, ResumeApplyError> {
    let mut endpoint = ResumeFixtureEndpoint::new();
    let session = ProjectionSession(SESSION.into());
    let mut client = ClientState::default();
    client
        .apply_snapshot(endpoint.initial_snapshot())
        .map_err(ResumeApplyError::InvalidSnapshot)?;
    let diff_2 = endpoint.diff(Revision(2));
    assert!(matches!(
        client.apply_diff(&diff_2),
        Ok(DiffApplication::Applied(_))
    ));
    assert!(matches!(
        client.apply_diff(&diff_2),
        Ok(DiffApplication::AlreadyApplied(_))
    ));
    client.mark_disconnected(&session);
    let request = client
        .resume_request(&session)
        .expect("mounted fixture has an acknowledgement");
    let reply = endpoint
        .resume(request)
        .expect("fixture session is correct");
    assert!(matches!(
        client.apply_resume(&session, reply)?,
        ResumeApplication::Applied(ProjectionAck {
            revision: Revision(3),
            ..
        })
    ));
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::Arrangement;

    #[test]
    fn disconnected_client_resumes_to_the_full_snapshot_without_slot_reuse() {
        let client = run_resume_canary().unwrap();
        let session = ProjectionSession(SESSION.into());
        let mounted = client.mounted(&session).unwrap();
        let endpoint = ResumeFixtureEndpoint::new();
        assert_eq!(mounted.scene, endpoint.current.scene);
        assert_eq!(mounted.presentation, endpoint.current.presentation);
        assert_eq!(mounted.status, SessionStatus::Live);
        assert_eq!(mounted.scene.tables.items.len(), 3);
        assert!(mounted.scene.tables.items[0].is_none());
        assert!(mounted.scene.tables.items[1].is_some());
        assert!(mounted.scene.tables.items[2].is_some());
        assert_eq!(mounted.scene.active_item_count(), 2);
    }

    #[test]
    fn resumed_diff_and_current_ack_are_idempotent() {
        let mut endpoint = ResumeFixtureEndpoint::new();
        let session = ProjectionSession(SESSION.into());
        let mut client = run_resume_canary().unwrap();
        let once = client.clone();
        let duplicate = ResumeReply::Diffs(vec![endpoint.diff(Revision(3))]);
        client.apply_resume(&session, duplicate).unwrap();
        assert_eq!(client, once);

        client.mark_disconnected(&session);
        let reply = endpoint
            .resume(client.resume_request(&session).unwrap())
            .unwrap();
        assert!(matches!(
            client.apply_resume(&session, reply).unwrap(),
            ResumeApplication::Current(_)
        ));
        assert_eq!(
            client.mounted(&session).unwrap().status,
            SessionStatus::Live
        );
    }

    #[test]
    fn unavailable_base_falls_back_to_an_epoch_preserving_snapshot() {
        let mut endpoint = ResumeFixtureEndpoint::new();
        let session = ProjectionSession(SESSION.into());
        let reply = endpoint
            .resume(ResumeRequest {
                session: session.clone(),
                epoch: SceneEpoch(3),
                revision: Revision(99),
            })
            .unwrap();
        assert!(
            matches!(reply, ResumeReply::Snapshot(snapshot) if snapshot.scene.tables.items[0].is_none())
        );

        let full = endpoint
            .snapshot(ProjectionRequest {
                version: ProtocolVersion::V1,
                session,
                score: sceno::Score::new(Arrangement::Spiral(Default::default())),
            })
            .unwrap();
        assert_eq!(full.scene.revision, Revision(3));
        assert!(full.scene.tables.items[0].is_none());
    }
}
