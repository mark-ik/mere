// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Traits application adapters implement beside their own source truth.

use std::fmt::Display;

use chirograph::{
    CarrierFailure, CarrierNotice, CarrierRequest, CarrierRequestBody, CarrierResponse,
    CarrierResponseBody, EndpointDescriptor, IntentInvocation, IntentResult, LiveViewReferenceV1,
    ProjectionRequest, ProjectionSnapshot, ResourceChunkRequest, ResourceChunkResponse,
    ResourceRequest, ResourceResponse, ResumeReply, ResumeRequest, SessionOpen,
};

/// Everything a carrier needs from an endpoint to serve the common verbs.
///
/// A name for the bound `dispatch_common` already requires, so a carrier can
/// say "a complete endpoint" once instead of repeating four traits at every
/// signature. Blanket-implemented: satisfying the four IS satisfying this,
/// and no adapter implements it directly.
pub trait CompleteEndpoint:
    ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink
{
}

impl<T> CompleteEndpoint for T where
    T: ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink
{
}

/// Discovery boundary for a product-neutral host.
pub trait ProjectionCatalog {
    fn describe(&self) -> EndpointDescriptor;
}

/// The read boundary. Implementations authorize selection before they disclose
/// a score or scene and retain ownership of native source data.
pub trait ProjectionSource {
    type Error;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error>;
}

/// The presentation-resource boundary. Resource authorization remains
/// endpoint-side and is evaluated independently of scene disclosure.
pub trait PresentationSource {
    type Error;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error>;

    /// Serve one slice of a resource, for resources larger than a carrier
    /// frame.
    ///
    /// Defaulted so every existing endpoint can serve large resources
    /// correctly the moment a client asks, without being rewritten. The
    /// default reads the whole resource and cuts one chunk from it, which is
    /// right but not cheap; an endpoint that can seek should override it and
    /// read only the requested range.
    fn resource_chunk(
        &mut self,
        request: ResourceChunkRequest,
    ) -> Result<ResourceChunkResponse, Self::Error> {
        let whole = self.resource(ResourceRequest {
            session: request.session,
            resource: request.resource,
        })?;
        Ok(ResourceChunkResponse::slice(
            &whole,
            request.offset,
            request.length,
        ))
    }
}

/// Reconnect and acknowledgement boundary. An endpoint may replay contiguous
/// diffs or fall back to an epoch-preserving snapshot.
pub trait ResumableProjectionSource {
    type Error;

    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, Self::Error>;
}

/// Payload-free change signal for carriers that support endpoint-initiated
/// frames. Returning `None` means the source has not advanced.
pub trait ProjectionNoticeSource {
    type Error;

    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error>;
}

/// The write boundary. Implementations validate revision and authority before
/// lowering an intent into a product-specific action.
pub trait IntentSink {
    type Error;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error>;
}

/// Dispatch the request verbs that mean the same thing on every carrier.
///
/// `Discover`, `Snapshot`, `Resource`, `Resume`, and `Intent` are pure
/// delegation to the traits above: no carrier has an opinion about them, and
/// every carrier would otherwise write the same five match arms. The session
/// plane is the opposite — `Open`, `Close`, and `Suspend` are answers about
/// the *carrier*, not the endpoint, so they are returned unhandled for the
/// caller to answer for itself.
///
/// Stdio refuses `Open` because inherited pipes perform no key exchange, and
/// refuses `Suspend` because no session outlives its process. An admitted
/// carrier that authenticated its peer and can be reconnected answers both.
/// Neither answer belongs in here.
pub fn dispatch_common<E, F>(
    endpoint: &mut E,
    request: CarrierRequest,
    resume: &mut F,
) -> Result<CarrierResponse, SessionPlaneRequest>
where
    E: ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink,
    <E as ProjectionSource>::Error: Display,
    <E as PresentationSource>::Error: Display,
    <E as IntentSink>::Error: Display,
    F: FnMut(&mut E, ResumeRequest) -> Result<ResumeReply, String>,
{
    let id = request.id;
    let body = match request.body {
        CarrierRequestBody::Discover => Ok(CarrierResponseBody::Descriptor(endpoint.describe())),
        CarrierRequestBody::Snapshot(request) => endpoint
            .snapshot(request)
            .map(|snapshot| CarrierResponseBody::Snapshot(Box::new(snapshot)))
            .map_err(|error| error.to_string()),
        CarrierRequestBody::Resource(request) => endpoint
            .resource(request)
            .map(CarrierResponseBody::Resource)
            .map_err(|error| error.to_string()),
        CarrierRequestBody::ResourceChunk(request) => endpoint
            .resource_chunk(request)
            .map(|chunk| CarrierResponseBody::ResourceChunk(Box::new(chunk)))
            .map_err(|error| error.to_string()),
        CarrierRequestBody::Resume(request) => {
            resume(endpoint, request).map(CarrierResponseBody::Resume)
        }
        CarrierRequestBody::Intent(intent) => endpoint
            .invoke(intent)
            .map(CarrierResponseBody::Intent)
            .map_err(|error| error.to_string()),
        CarrierRequestBody::Open(open) => {
            return Err(SessionPlaneRequest {
                id,
                verb: SessionPlaneVerb::Open(open),
            });
        }
        CarrierRequestBody::Close => {
            return Err(SessionPlaneRequest {
                id,
                verb: SessionPlaneVerb::Close,
            });
        }
        CarrierRequestBody::Suspend => {
            return Err(SessionPlaneRequest {
                id,
                verb: SessionPlaneVerb::Suspend,
            });
        }
    };
    Ok(CarrierResponse {
        id,
        body: body.map_err(|message| CarrierFailure { message }),
    })
}

/// A session-plane request [`dispatch_common`] declined to answer.
///
/// Carries the request id so the caller's answer still correlates.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionPlaneRequest {
    /// The id to answer under.
    pub id: u64,
    /// What was asked.
    pub verb: SessionPlaneVerb,
}

/// The session-plane verbs whose answer depends on the carrier.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionPlaneVerb {
    /// Negotiate version and capabilities on an already-admitted carrier.
    Open(Box<SessionOpen>),
    /// Tear the session down.
    Close,
    /// Going away, but keep the session resumable.
    Suspend,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chirograph::{ContentHash, MAX_RESOURCE_CHUNK_BYTES, ProjectionSession, ResourceAssembly};

    struct FixtureLiveViewGate {
        answer: Result<(), LiveViewReferenceRefusal>,
        opened: Vec<String>,
    }

    impl LiveViewReferenceGate for FixtureLiveViewGate {
        fn open_live_view_reference(
            &mut self,
            reference: &LiveViewReferenceV1,
        ) -> Result<(), LiveViewReferenceRefusal> {
            self.opened.push(reference.record.clone());
            self.answer.clone()
        }
    }

    #[test]
    fn participant_gate_refuses_unreadable_live_view_without_a_redacted_scene() {
        let payload = LiveViewReferenceV1::new("eidetic:private-view-record")
            .encode()
            .expect("reference serializes");
        let mut gate = FixtureLiveViewGate {
            answer: Err(LiveViewReferenceRefusal::AccessDenied),
            opened: Vec::new(),
        };
        assert_eq!(
            resolve_live_view_reference(&payload, &mut gate),
            IntentResult::Rejected {
                reason: "the recipient is not authorized to read this live-view source".to_string()
            }
        );
        assert_eq!(gate.opened, vec!["eidetic:private-view-record"]);
    }

    #[test]
    fn malformed_live_view_reference_is_rejected_before_the_gate() {
        let mut gate = FixtureLiveViewGate {
            answer: Ok(()),
            opened: Vec::new(),
        };
        assert!(matches!(
            resolve_live_view_reference(
                br#"{"schema":"mere.live-view-reference/v1","record":"file:C:/secret"}"#,
                &mut gate
            ),
            IntentResult::Rejected { .. }
        ));
        assert!(
            gate.opened.is_empty(),
            "malformed input never reaches source authority"
        );
    }

    /// An endpoint that knows only how to hand over a whole resource, which is
    /// every endpoint written before chunking existed.
    struct WholeResourceOnly {
        bytes: Vec<u8>,
    }

    impl PresentationSource for WholeResourceOnly {
        type Error = String;

        fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
            let response = ResourceResponse::new(request.session, self.bytes.clone());
            (response.resource == request.resource)
                .then_some(response)
                .ok_or_else(|| "unknown resource".to_string())
        }
    }

    /// The default is the whole point of putting chunking on the trait: an
    /// endpoint that never heard of it still serves a resource larger than a
    /// carrier frame, correctly and verifiably, without being rewritten.
    #[test]
    fn an_endpoint_that_only_serves_whole_resources_still_serves_chunks() {
        let bytes: Vec<u8> = (0..(MAX_RESOURCE_CHUNK_BYTES as usize + 1_234))
            .map(|index| (index % 253) as u8)
            .collect();
        let session = ProjectionSession("fixture:legacy".into());
        let resource = ContentHash::of(&bytes);
        let mut endpoint = WholeResourceOnly {
            bytes: bytes.clone(),
        };

        let mut assembly = ResourceAssembly::new();
        let mut frames = 0;
        let assembled = loop {
            let chunk = endpoint
                .resource_chunk(ResourceChunkRequest {
                    session: session.clone(),
                    resource,
                    offset: assembly.received(),
                    length: 0,
                })
                .unwrap();
            frames += 1;
            if let Some(done) = assembly.accept(&chunk).unwrap() {
                break done;
            }
        };

        assert_eq!(assembled, bytes);
        assert_eq!(frames, 2, "one full chunk and one short tail");
    }

    #[test]
    fn a_chunk_request_for_an_unknown_resource_fails_rather_than_serving_bytes() {
        let mut endpoint = WholeResourceOnly {
            bytes: b"real".to_vec(),
        };
        let refused = endpoint.resource_chunk(ResourceChunkRequest {
            session: ProjectionSession("fixture:legacy".into()),
            resource: ContentHash::of(b"something else"),
            offset: 0,
            length: 16,
        });
        assert!(refused.is_err());
    }
}

/// The participant-gate boundary for a producer-owned live-view record. The
/// reference has crossed Graphshell as opaque bytes; only this source owner can
/// decide whether its scope and cursor may be read by the recipient.
pub trait LiveViewReferenceGate {
    fn open_live_view_reference(
        &mut self,
        reference: &LiveViewReferenceV1,
    ) -> Result<(), LiveViewReferenceRefusal>;
}

/// A source owner's explicit reason for refusing an opaque live-view record.
/// These are intentionally not represented as a redacted scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveViewReferenceRefusal {
    MissingSource,
    StaleCursor,
    AccessDenied,
    UnsupportedArrangement,
}

impl std::fmt::Display for LiveViewReferenceRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSource => {
                write!(formatter, "the requested live-view source is unavailable")
            }
            Self::StaleCursor => write!(formatter, "the requested live-view cursor is unavailable"),
            Self::AccessDenied => write!(
                formatter,
                "the recipient is not authorized to read this live-view source"
            ),
            Self::UnsupportedArrangement => write!(
                formatter,
                "the requested live-view arrangement is unsupported"
            ),
        }
    }
}

impl std::error::Error for LiveViewReferenceRefusal {}

/// Decode one `mere.live-view-reference/v1` intent payload and ask the
/// participant gate to open it. Callers still perform their normal
/// epoch/revision stale check before this function, exactly as for every other
/// Graphshell intent.
pub fn resolve_live_view_reference(
    payload: &[u8],
    gate: &mut impl LiveViewReferenceGate,
) -> IntentResult {
    let reference = match LiveViewReferenceV1::decode(payload) {
        Ok(reference) => reference,
        Err(error) => {
            return IntentResult::Rejected {
                reason: error.to_string(),
            };
        }
    };
    match gate.open_live_view_reference(&reference) {
        Ok(()) => IntentResult::Accepted,
        Err(refusal) => IntentResult::Rejected {
            reason: refusal.to_string(),
        },
    }
}
