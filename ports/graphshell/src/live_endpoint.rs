// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! An endpoint whose scene actually moves, and only when it should.
//!
//! [`crate::resume::ResumeFixtureEndpoint`] has a fixed history: three
//! revisions decided at construction, which is right for proving that resume
//! picks the correct branch and wrong for proving anything about *what makes*
//! a revision. C4a needs the second thing — "the native state revision changes
//! only for the admitted intent" is a done-condition, and an endpoint whose
//! revisions were written before any peer connected cannot earn it.
//!
//! So this one advances on invocation. Its whole rule is the match in
//! [`LiveEndpoint::invoke`]:
//!
//! - the **admitted** intent, at the current revision, appends a diff and
//!   returns [`IntentResult::Accepted`];
//! - the **refused** intent is advertised exactly as visibly and always
//!   returns [`IntentResult::Rejected`], changing nothing;
//! - either intent raised against a stale revision returns
//!   [`IntentResult::Stale`] with the current position, changing nothing.
//!
//! The refused intent is advertised on purpose. A receipt row that says "an
//! intent the endpoint never offered was not performed" proves nothing about
//! policy; the interesting claim is that a peer can *see* an action, invoke it
//! correctly, and still be told no — with the revision standing still to show
//! the refusal was real rather than an error in disguise.
//!
//! ## Not a product endpoint
//!
//! This serves receipts. It holds one scene in memory, has no store beneath
//! it, and its authorization is a string comparison rather than a policy. A
//! product endpoint answers to something; this answers to the receipt it makes
//! possible.

use std::collections::BTreeMap;

use chirograph::{
    AdvertisedAction, BoundsRelationship, CachePolicy, CarrierNotice, ContentHash,
    EndpointDescriptor, IntentEffect, IntentInvocation, IntentReference, IntentResult,
    PresentationBinding, PresentationCapability, PresentationChange, PresentationCodec,
    PresentationKey, PresentationManifest, PresentationOffer, PresentationSemantics, ProjectionAck,
    ProjectionDiff, ProjectionOffer, ProjectionRequest, ProjectionSession, ProjectionSnapshot,
    ProtocolVersion, ResourceRequest, ResourceResponse, ResumeReply, ResumeRequest, SemanticRole,
    SessionStatus,
};
use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
    ResumableProjectionSource,
};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Representation, Scene, Score, Size2,
    SourceIx, SourceRef, Transform2,
};
use scenotime::{Revision, SceneDiff, SceneEpoch, SceneOp, SceneSnapshot};

/// The intent this endpoint performs.
pub const ADMITTED_INTENT: &str = "mere.graphshell/live-fixture/append";
/// The intent this endpoint advertises and always refuses.
pub const REFUSED_INTENT: &str = "mere.graphshell/live-fixture/forbidden";

/// The session this endpoint projects.
pub const SESSION: &str = "live.graphshell/board";

const EPOCH: SceneEpoch = SceneEpoch(1);

/// Why a request against this endpoint could not be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveEndpointError {
    /// The request names a session this endpoint does not project.
    WrongSession,
    /// The requested resource is not one this endpoint holds.
    NoSuchResource,
}

impl std::fmt::Display for LiveEndpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiveEndpointError::WrongSession => write!(f, "request names another session"),
            LiveEndpointError::NoSuchResource => write!(f, "no such resource"),
        }
    }
}

/// A scene that grows one card per admitted intent.
pub struct LiveEndpoint {
    session: ProjectionSession,
    scene: Scene,
    revision: Revision,
    /// Every diff issued so far, so a resuming peer can be caught up rather
    /// than re-snapshotted.
    history: Vec<ProjectionDiff>,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    /// Set when a revision is issued, taken by the next notice poll.
    pending_notice: Option<CarrierNotice>,
}

impl LiveEndpoint {
    pub fn new() -> Self {
        let session = ProjectionSession(SESSION.into());
        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new("live.graphshell", "card:0"));
        scene.items.push(card(source, 0.0));
        let mut resources = BTreeMap::new();
        let bytes = label_bytes(0);
        resources.insert(ContentHash::of(&bytes), bytes);
        Self {
            session,
            scene,
            revision: Revision(1),
            history: Vec::new(),
            resources,
            pending_notice: None,
        }
    }

    /// The request that selects this projection.
    pub fn request(&self) -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            score: Score::new(Arrangement::Spiral(Default::default())),
        }
    }

    /// The revision a peer should currently be able to reach.
    pub fn current_revision(&self) -> Revision {
        self.revision
    }

    /// The session this endpoint projects.
    pub fn session(&self) -> &ProjectionSession {
        &self.session
    }

    /// The actions every card advertises: one performed, one refused.
    fn actions() -> Vec<AdvertisedAction> {
        vec![
            AdvertisedAction {
                intent: IntentReference(ADMITTED_INTENT.into()),
                label: "Append a card".into(),
                explanation: "Adds one card and advances the revision.".into(),
                payload_schema: "mere.graphshell/live-fixture/append/v1".into(),
                input_form: None,
                effect: IntentEffect::Curation,
            },
            AdvertisedAction {
                intent: IntentReference(REFUSED_INTENT.into()),
                label: "Forbidden action".into(),
                explanation: "Advertised and always refused, so a refusal is observable.".into(),
                payload_schema: "mere.graphshell/live-fixture/forbidden/v1".into(),
                input_form: None,
                effect: IntentEffect::Curation,
            },
        ]
    }

    /// The presentation manifest for the scene as it currently stands.
    fn manifest(&self) -> PresentationManifest {
        let mut manifest = PresentationManifest::default();
        for index in 0..self.card_count() {
            let key = PresentationKey(format!("live:{index}"));
            manifest.bindings.push(PresentationBinding {
                instance: InstanceId(index),
                key: key.clone(),
            });
            manifest.offers.insert(key, vec![offer(index)]);
        }
        manifest
    }

    fn card_count(&self) -> u32 {
        u32::try_from(self.scene.items.len()).expect("a fixture never grows past u32")
    }
}

/// One card, laid out along a row so a browser shows growth rather than a pile.
fn card(source: SourceIx, x: f32) -> ProjectedItem {
    ProjectedItem {
        source,
        space: Scene::WORLD,
        transform: Transform2::translation(x, 0.0),
        footprint: Footprint::Rect {
            size: Size2::new(120.0, 80.0),
        },
        representation: Representation::Card,
        layer: 0,
        visible: true,
        hit: None,
        channels: Vec::new(),
    }
}

fn label_bytes(index: u32) -> Vec<u8> {
    format!("card {index}").into_bytes()
}

fn offer(index: u32) -> PresentationOffer {
    let bytes = label_bytes(index);
    PresentationOffer {
        codec: PresentationCodec::NativeGlyphV1,
        resource: ContentHash::of(&bytes),
        byte_size: bytes.len() as u64,
        requires: PresentationCapability::NativeGlyph,
        semantics: PresentationSemantics {
            label: format!("Card {index}"),
            role: SemanticRole::Graphic,
            bounds: BoundsRelationship::FitWithinFootprint,
            actions: LiveEndpoint::actions(),
        },
    }
}

impl Default for LiveEndpoint {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectionCatalog for LiveEndpoint {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "graphshell live fixture".to_string(),
            projections: vec![ProjectionOffer {
                label: "C4 live board".to_string(),
                request: self.request(),
            }],
        }
    }
}

impl ProjectionSource for LiveEndpoint {
    type Error = LiveEndpointError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session {
            return Err(LiveEndpointError::WrongSession);
        }
        Ok(ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            scene: SceneSnapshot::from_dense(EPOCH, self.revision, self.scene.clone())
                .expect("the live fixture scene is always valid"),
            presentation: self.manifest(),
            cache_policy: CachePolicy::default(),
        })
    }
}

impl PresentationSource for LiveEndpoint {
    type Error = LiveEndpointError;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session {
            return Err(LiveEndpointError::WrongSession);
        }
        let bytes = self
            .resources
            .get(&request.resource)
            .ok_or(LiveEndpointError::NoSuchResource)?
            .clone();
        Ok(ResourceResponse {
            session: self.session.clone(),
            resource: request.resource,
            bytes,
        })
    }
}

impl ResumableProjectionSource for LiveEndpoint {
    type Error = LiveEndpointError;

    /// Catch a peer up by diff when its position is one this endpoint still
    /// remembers, and by snapshot when it is not.
    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, Self::Error> {
        if request.session != self.session {
            return Err(LiveEndpointError::WrongSession);
        }
        if request.epoch != EPOCH {
            return Ok(ResumeReply::Snapshot(Box::new(
                self.snapshot(self.request())?,
            )));
        }
        if request.revision == self.revision {
            return Ok(ResumeReply::Current(ProjectionAck {
                session: self.session.clone(),
                epoch: EPOCH,
                revision: self.revision,
            }));
        }
        let diffs: Vec<ProjectionDiff> = self
            .history
            .iter()
            .filter(|diff| diff.scene.base >= request.revision)
            .cloned()
            .collect();
        // A gap means the peer is behind what this endpoint kept, which for an
        // in-memory fixture only happens on a position it never issued.
        if diffs.is_empty() || diffs[0].scene.base != request.revision {
            return Ok(ResumeReply::Snapshot(Box::new(
                self.snapshot(self.request())?,
            )));
        }
        Ok(ResumeReply::Diffs(diffs))
    }
}

impl ProjectionNoticeSource for LiveEndpoint {
    type Error = LiveEndpointError;

    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error> {
        Ok(self.pending_notice.take())
    }
}

impl IntentSink for LiveEndpoint {
    type Error = LiveEndpointError;

    /// The whole rule of this endpoint, and the reason it exists.
    ///
    /// Staleness is checked before authorization, deliberately: a peer holding
    /// an old revision is told its *position* is wrong rather than that its
    /// intent is forbidden, which is the difference between "resynchronize"
    /// and "stop asking". Both refusals leave the revision exactly where it
    /// was.
    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session {
            return Err(LiveEndpointError::WrongSession);
        }
        if intent.observed_epoch != EPOCH || intent.observed_revision != self.revision {
            return Ok(IntentResult::Stale {
                current_epoch: EPOCH,
                current_revision: self.revision,
            });
        }
        if intent.intent == REFUSED_INTENT {
            return Ok(IntentResult::Rejected {
                reason: "this endpoint advertises the action and refuses it".into(),
            });
        }
        if intent.intent != ADMITTED_INTENT {
            return Ok(IntentResult::Rejected {
                reason: format!("unknown intent {}", intent.intent),
            });
        }
        self.append();
        Ok(IntentResult::Accepted)
    }
}

impl LiveEndpoint {
    /// Append a card natively — the host's own hand on the board — and ring
    /// the bell. The one path that moves the scene, whether an admitted intent
    /// reached it or the host changed the board while a peer was away (the
    /// resume-on-reconnect receipt needs exactly that).
    pub fn append(&mut self) -> Revision {
        let index = self.card_count();
        let source = self
            .scene
            .intern_source(SourceRef::new("live.graphshell", "card:0"));
        let value = card(source, f32::from(u16::try_from(index).unwrap_or(0)) * 140.0);
        self.scene.items.push(value.clone());

        let base = self.revision;
        self.revision = Revision(self.revision.0 + 1);
        let bytes = label_bytes(index);
        self.resources.insert(ContentHash::of(&bytes), bytes);

        let key = PresentationKey(format!("live:{index}"));
        self.history.push(ProjectionDiff {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            scene: SceneDiff {
                epoch: EPOCH,
                base,
                revision: self.revision,
                operations: vec![SceneOp::AddItem {
                    index: InstanceId(index),
                    value,
                    order: -1,
                }],
            },
            presentation: vec![
                PresentationChange::Bind(PresentationBinding {
                    instance: InstanceId(index),
                    key: key.clone(),
                }),
                PresentationChange::ReplaceOffers {
                    key,
                    offers: vec![offer(index)],
                },
            ],
            status: Some(SessionStatus::Live),
        });
        self.pending_notice = Some(CarrierNotice {
            session: self.session.clone(),
            epoch: EPOCH,
            revision: self.revision,
        });
        self.revision
    }
}

/// One [`LiveEndpoint`] shared by every session a host serves.
///
/// A catalog route opens an endpoint per admitted session, which is right
/// for a product and wrong for a board that must outlive any one visitor: a
/// peer that drops and rejoins has to find the board where the host left it,
/// with whatever changed while it was away. This wrapper is that board — one
/// endpoint behind a lock, handed to each session as its own — so a native
/// change during an outage is what the next session's poll rings for.
#[derive(Clone)]
pub struct SharedLiveEndpoint(std::sync::Arc<std::sync::Mutex<LiveEndpoint>>);

impl SharedLiveEndpoint {
    pub fn new(endpoint: LiveEndpoint) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(endpoint)))
    }

    /// Run `f` on the board.
    pub fn with<R>(&self, f: impl FnOnce(&mut LiveEndpoint) -> R) -> R {
        let mut guard = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }
}

impl ProjectionCatalog for SharedLiveEndpoint {
    fn describe(&self) -> EndpointDescriptor {
        self.with(|endpoint| endpoint.describe())
    }
}

impl ProjectionSource for SharedLiveEndpoint {
    type Error = LiveEndpointError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        self.with(|endpoint| endpoint.snapshot(request))
    }
}

impl PresentationSource for SharedLiveEndpoint {
    type Error = LiveEndpointError;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        self.with(|endpoint| endpoint.resource(request))
    }
}

impl ResumableProjectionSource for SharedLiveEndpoint {
    type Error = LiveEndpointError;

    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, Self::Error> {
        self.with(|endpoint| endpoint.resume(request))
    }
}

impl ProjectionNoticeSource for SharedLiveEndpoint {
    type Error = LiveEndpointError;

    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error> {
        self.with(|endpoint| endpoint.poll_notice())
    }
}

impl IntentSink for SharedLiveEndpoint {
    type Error = LiveEndpointError;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        self.with(|endpoint| endpoint.invoke(intent))
    }
}

#[cfg(test)]
mod tests {
    //! The done-condition, stated as tests: *the native state revision changes
    //! only for the admitted intent.*
    //!
    //! Every row here is the same invocation with one thing changed, and each
    //! asserts the revision afterwards — because "was it refused" and "did it
    //! change anything anyway" are different questions, and only the second
    //! one is the property C4a claims.

    use super::*;

    fn invocation(
        session: &ProjectionSession,
        revision: Revision,
        intent: &str,
    ) -> IntentInvocation {
        IntentInvocation {
            session: session.clone(),
            target: InstanceId(0),
            observed_epoch: EPOCH,
            observed_revision: revision,
            intent: intent.to_string(),
            payload: Vec::new(),
        }
    }

    #[test]
    fn the_admitted_intent_advances_the_revision_by_one() {
        let mut endpoint = LiveEndpoint::new();
        let session = endpoint.session().clone();
        let before = endpoint.current_revision();

        let result = endpoint
            .invoke(invocation(&session, before, ADMITTED_INTENT))
            .expect("the session matches");

        assert_eq!(result, IntentResult::Accepted);
        assert_eq!(
            endpoint.current_revision(),
            Revision(before.0 + 1),
            "the admitted intent is the one thing that moves the scene"
        );
    }

    /// The row that makes the done-condition mean something: an action the
    /// endpoint *advertises*, invoked correctly, refused, and the revision
    /// standing exactly still.
    #[test]
    fn the_refused_intent_changes_nothing() {
        let mut endpoint = LiveEndpoint::new();
        let session = endpoint.session().clone();
        let before = endpoint.current_revision();

        let result = endpoint
            .invoke(invocation(&session, before, REFUSED_INTENT))
            .expect("the session matches");

        assert!(
            matches!(result, IntentResult::Rejected { .. }),
            "expected a refusal, got {result:?}"
        );
        assert_eq!(
            endpoint.current_revision(),
            before,
            "a refusal that moved the revision would be an acceptance in disguise"
        );
    }

    /// Both intents are visible to a peer. A refusal only proves something
    /// about policy if the action was on offer in the first place.
    #[test]
    fn both_intents_are_advertised() {
        let intents: Vec<String> = LiveEndpoint::actions()
            .into_iter()
            .map(|action| action.intent.0)
            .collect();
        assert!(intents.iter().any(|intent| intent == ADMITTED_INTENT));
        assert!(
            intents.iter().any(|intent| intent == REFUSED_INTENT),
            "the refused intent must be advertised, or its refusal proves nothing"
        );
    }

    /// A stale position is answered as a position problem, not an authority
    /// one — and still changes nothing.
    #[test]
    fn a_stale_invocation_is_told_where_the_scene_is() {
        let mut endpoint = LiveEndpoint::new();
        let session = endpoint.session().clone();
        endpoint
            .invoke(invocation(
                &session,
                endpoint.current_revision(),
                ADMITTED_INTENT,
            ))
            .expect("the session matches");
        let current = endpoint.current_revision();

        let result = endpoint
            .invoke(invocation(&session, Revision(1), ADMITTED_INTENT))
            .expect("the session matches");

        assert_eq!(
            result,
            IntentResult::Stale {
                current_epoch: EPOCH,
                current_revision: current,
            }
        );
        assert_eq!(
            endpoint.current_revision(),
            current,
            "a stale intent changes nothing"
        );
    }

    /// Each accepted intent leaves a diff a resuming peer can be caught up
    /// with, rather than forcing a fresh snapshot.
    #[test]
    fn a_resuming_peer_is_caught_up_by_diff() {
        let mut endpoint = LiveEndpoint::new();
        let session = endpoint.session().clone();
        let start = endpoint.current_revision();
        for _ in 0..3 {
            endpoint
                .invoke(invocation(
                    &session,
                    endpoint.current_revision(),
                    ADMITTED_INTENT,
                ))
                .expect("the session matches");
        }

        let reply = endpoint
            .resume(ResumeRequest {
                session: session.clone(),
                epoch: EPOCH,
                revision: start,
            })
            .expect("the session matches");

        match reply {
            ResumeReply::Diffs(diffs) => {
                assert_eq!(diffs.len(), 3, "one diff per accepted intent");
                assert_eq!(diffs[0].scene.base, start);
                assert_eq!(diffs[2].scene.revision, endpoint.current_revision());
            }
            other => panic!("expected diffs, got {other:?}"),
        }
    }

    #[test]
    fn a_peer_already_current_is_told_so_rather_than_re_sent() {
        let mut endpoint = LiveEndpoint::new();
        let session = endpoint.session().clone();
        let reply = endpoint
            .resume(ResumeRequest {
                session: session.clone(),
                epoch: EPOCH,
                revision: endpoint.current_revision(),
            })
            .expect("the session matches");
        assert!(matches!(reply, ResumeReply::Current(_)), "{reply:?}");
    }

    /// Accepting an intent rings the bell exactly once, and a refusal rings
    /// nothing — the notice lane carries the same claim as the revision.
    #[test]
    fn only_an_accepted_intent_rings() {
        let mut endpoint = LiveEndpoint::new();
        let session = endpoint.session().clone();
        assert!(endpoint.poll_notice().expect("no error").is_none());

        endpoint
            .invoke(invocation(
                &session,
                endpoint.current_revision(),
                REFUSED_INTENT,
            ))
            .expect("the session matches");
        assert!(
            endpoint.poll_notice().expect("no error").is_none(),
            "a refusal must not ring"
        );

        endpoint
            .invoke(invocation(
                &session,
                endpoint.current_revision(),
                ADMITTED_INTENT,
            ))
            .expect("the session matches");
        let notice = endpoint.poll_notice().expect("no error").expect("one bell");
        assert_eq!(notice.revision, endpoint.current_revision());
        assert!(
            endpoint.poll_notice().expect("no error").is_none(),
            "the bell is taken once, not re-rung on every poll"
        );
    }
}
