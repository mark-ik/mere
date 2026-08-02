//! Traits application adapters implement beside their own source truth.

use std::fmt::Display;

use graphshell_protocol::{
    CarrierFailure, CarrierNotice, CarrierRequest, CarrierRequestBody, CarrierResponse,
    CarrierResponseBody, EndpointDescriptor, IntentInvocation, IntentResult, ProjectionRequest,
    ProjectionSnapshot, ResourceRequest, ResourceResponse, ResumeReply, ResumeRequest, SessionOpen,
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
