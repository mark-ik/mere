//! The request script the rehearsal plays, and how answers are reported.

use chirograph::{
    CapabilityProfile, CarrierRequestBody, IntentInvocation, ProjectionSession, ProtocolVersion,
    Revision, SceneEpoch, SessionOpen,
};
use sceno::{Arrangement, InstanceId, Score};

/// The endpoint's own projection session, which is independent of admission:
/// two separate admissions resume the same projection, which is what makes an
/// interruption survivable at all.
pub(crate) const RESUME_SESSION: &str = "loopback:g2-resume";

pub(crate) fn open_body() -> CarrierRequestBody {
    CarrierRequestBody::Open(Box::new(SessionOpen {
        version: ProtocolVersion::V1,
        capabilities: CapabilityProfile::default(),
    }))
}

pub(crate) fn intent_body() -> CarrierRequestBody {
    CarrierRequestBody::Intent(IntentInvocation {
        session: ProjectionSession(RESUME_SESSION.into()),
        target: InstanceId(1),
        observed_epoch: SceneEpoch(3),
        observed_revision: Revision(3),
        intent: "fixture.inspect".to_string(),
        payload: Vec::new(),
    })
}

pub(crate) fn projection_request() -> chirograph::ProjectionRequest {
    chirograph::ProjectionRequest {
        version: ProtocolVersion::V1,
        session: ProjectionSession(RESUME_SESSION.into()),
        score: Score::new(Arrangement::Spiral(Default::default())),
    }
}

pub(crate) fn summarize(body: &chirograph::CarrierResponseBody) -> String {
    use chirograph::CarrierResponseBody as B;
    match body {
        B::Opened(opened) => format!(
            "opened, status {:?}, expires {:?}",
            opened.status, opened.expires_at_ms
        ),
        B::Descriptor(descriptor) => format!(
            "descriptor {:?} with {} projection(s)",
            descriptor.label,
            descriptor.projections.len()
        ),
        B::Snapshot(snapshot) => {
            format!("snapshot of {} item(s)", snapshot.scene.active_item_count())
        }
        B::Resource(_) => "resource".to_string(),
        B::ResourceChunk(chunk) => format!(
            "resource chunk at {} of {} byte(s)",
            chunk.offset, chunk.total_len
        ),
        // Which reply matters: replayed diffs are the thing that makes this a
        // resume rather than a reconnect that started over.
        B::Resume(reply) => match reply {
            chirograph::ResumeReply::Diffs(diffs) => format!(
                "resumed by replaying {} contiguous diff(s), revisions {}",
                diffs.len(),
                diffs
                    .iter()
                    .map(|d| format!("{}->{}", d.scene.base.0, d.scene.revision.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            chirograph::ResumeReply::Snapshot(snapshot) => format!(
                "resumed by full snapshot at revision {} (history could not bridge the gap)",
                snapshot.scene.revision.0
            ),
            chirograph::ResumeReply::Current(ack) => {
                format!("already current at revision {}", ack.revision.0)
            }
        },
        B::Intent(result) => format!("intent {result:?}"),
        B::Closed => "closed".to_string(),
        B::Suspended => "suspended".to_string(),
    }
}
