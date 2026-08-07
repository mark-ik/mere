//! The in-process carrier: an endpoint hosted where its host runs.
//!
//! The degenerate carrier, and the one that makes "an embeddable component"
//! and "a projectable remote surface" the same statement instead of competing
//! ones. A host that embeds an endpoint uses the same client, the same
//! protocol, and the same capability negotiation it would use across a
//! process or a network; only the transport goes away.
//!
//! ## It still serializes, on purpose
//!
//! Every request and response round-trips through the same
//! `serde_json` encoding the wire uses, and only the I/O is skipped. That
//! costs a local call one encode and one decode, and buys the guarantee that
//! the in-process path exercises the real wire format.
//!
//! Skipping it would give the local case a private path that drifts from the
//! wire silently, and the drift surfaces as "works embedded, breaks
//! remotely" — the most expensive place to discover it, because by then the
//! remote case is the one under a deadline. A type that fails to serialize is
//! a bug on every carrier; this one just finds it first.

use std::fmt::Display;

use graphshell_protocol::{
    Carrier, CarrierError, CarrierNotice, CarrierRequest, CarrierRequestBody, CarrierResponseBody,
    ResumeReply, ResumeRequest,
};

use graphshell_endpoint::{CompleteEndpoint, ProjectionNoticeSource, dispatch_common};

/// An endpoint carried in the host's own process.
///
/// `resume` is supplied by the caller for the same reason `dispatch_common`
/// takes it: resume is the one verb whose meaning is endpoint-specific, and a
/// carrier has no business inventing an answer to it.
pub struct LocalCarrier<E, F> {
    endpoint: E,
    resume: F,
    next_id: u64,
}

impl<E, F> LocalCarrier<E, F>
where
    E: CompleteEndpoint + ProjectionNoticeSource,
    F: FnMut(&mut E, ResumeRequest) -> Result<ResumeReply, String>,
{
    pub fn new(endpoint: E, resume: F) -> Self {
        Self {
            endpoint,
            resume,
            next_id: 1,
        }
    }

    /// The hosted endpoint, for a caller that also drives it directly.
    pub fn endpoint(&self) -> &E {
        &self.endpoint
    }

    pub fn endpoint_mut(&mut self) -> &mut E {
        &mut self.endpoint
    }
}

/// Round-trip a value through the wire encoding. See the module note: this is
/// the point, not an oversight.
fn through_the_wire<T>(value: &T) -> Result<T, CarrierError>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    // A message that will not round-trip is a defect in the message type, and
    // no in-process session survives it, so it reports as disconnection rather
    // than as something the endpoint decided.
    let bytes = serde_json::to_vec(value).map_err(|error| {
        CarrierError::Disconnected(format!("carrier could not encode a message: {error}"))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        CarrierError::Disconnected(format!("carrier could not decode a message: {error}"))
    })
}

impl<E, F> Carrier for LocalCarrier<E, F>
where
    E: CompleteEndpoint + ProjectionNoticeSource,
    <E as graphshell_endpoint::ProjectionSource>::Error: Display,
    <E as graphshell_endpoint::PresentationSource>::Error: Display,
    <E as graphshell_endpoint::IntentSink>::Error: Display,
    <E as ProjectionNoticeSource>::Error: Display,
    F: FnMut(&mut E, ResumeRequest) -> Result<ResumeReply, String>,
{
    fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, CarrierError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = through_the_wire(&CarrierRequest { id, body })?;
        let response =
            dispatch_common(&mut self.endpoint, request, &mut self.resume).map_err(|plane| {
                // Session-plane verbs are the host's to answer, and an
                // in-process host has no session plane to answer them with.
                // Refusing names the verb rather than pretending it worked.
                CarrierError::Refused(format!(
                    "in-process carrier does not serve the session plane (request {})",
                    plane.id
                ))
            })?;
        let response = through_the_wire(&response)?;
        match response.body {
            Ok(body) => Ok(body),
            Err(failure) => Err(CarrierError::Refused(failure.message)),
        }
    }

    fn take_notice(&mut self) -> Option<CarrierNotice> {
        self.endpoint.poll_notice().ok().flatten()
    }

    /// There is no blocking to do: an in-process endpoint has already produced
    /// whatever it is going to produce by the time the host asks. A caller
    /// that wants to wait owns that loop, because only it knows what it is
    /// waiting for and how long is too long.
    fn wait_for_notice(&mut self) -> Result<CarrierNotice, CarrierError> {
        self.take_notice().ok_or_else(|| {
            CarrierError::Refused("in-process endpoint has no pending notice".to_string())
        })
    }

    /// Nothing is held open. Dropping the carrier drops the endpoint.
    fn shutdown(&mut self) -> Result<(), CarrierError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphshell_endpoint::{
        IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource,
    };
    use graphshell_protocol::{
        EndpointDescriptor, IntentInvocation, IntentResult, ProjectionRequest, ProjectionSnapshot,
        ResourceRequest, ResourceResponse,
    };

    /// The smallest endpoint that answers discovery, so the carrier is what is
    /// under test rather than any product's adapter.
    #[derive(Default)]
    struct CountingEndpoint {
        describes: std::cell::Cell<usize>,
        notice: Option<CarrierNotice>,
    }

    impl ProjectionCatalog for CountingEndpoint {
        fn describe(&self) -> EndpointDescriptor {
            self.describes.set(self.describes.get() + 1);
            EndpointDescriptor {
                label: "counting".into(),
                projections: Vec::new(),
            }
        }
    }

    impl ProjectionSource for CountingEndpoint {
        type Error = String;
        fn snapshot(&mut self, _: ProjectionRequest) -> Result<ProjectionSnapshot, String> {
            Err("no projections".into())
        }
    }

    impl PresentationSource for CountingEndpoint {
        type Error = String;
        fn resource(&mut self, _: ResourceRequest) -> Result<ResourceResponse, String> {
            Err("no resources".into())
        }
    }

    impl IntentSink for CountingEndpoint {
        type Error = String;
        fn invoke(&mut self, _: IntentInvocation) -> Result<IntentResult, String> {
            Err("no intents".into())
        }
    }

    impl ProjectionNoticeSource for CountingEndpoint {
        type Error = String;
        fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, String> {
            Ok(self.notice.take())
        }
    }

    fn carrier() -> LocalCarrier<
        CountingEndpoint,
        impl FnMut(&mut CountingEndpoint, ResumeRequest) -> Result<ResumeReply, String>,
    > {
        LocalCarrier::new(
            CountingEndpoint::default(),
            |_: &mut CountingEndpoint, _| Err("resume is the endpoint's to answer".to_string()),
        )
    }

    #[test]
    fn a_request_reaches_the_endpoint_and_its_answer_comes_back() {
        let mut carrier = carrier();
        match carrier.request(CarrierRequestBody::Discover).unwrap() {
            CarrierResponseBody::Descriptor(descriptor) => {
                assert_eq!(descriptor.label, "counting");
            }
            other => panic!("expected a descriptor, got {other:?}"),
        }
        assert_eq!(
            carrier.endpoint().describes.get(),
            1,
            "reached exactly once"
        );
    }

    #[test]
    fn an_endpoint_failure_arrives_as_a_failure_not_a_panic() {
        let mut carrier = carrier();
        let error = carrier
            .request(CarrierRequestBody::Resource(ResourceRequest {
                session: graphshell_protocol::ProjectionSession("absent".into()),
                resource: graphshell_protocol::ContentHash::of(b"absent"),
            }))
            .unwrap_err();
        assert!(error.message().contains("no resources"), "{error}");
        assert!(
            !error.is_disconnected(),
            "an endpoint saying no leaves the session intact"
        );
    }

    #[test]
    fn every_message_survives_the_wire_encoding() {
        // The reason this carrier serializes at all. If a message type stops
        // round-tripping, the in-process path must fail here rather than work
        // locally and break the first time a real carrier moves it.
        let mut carrier = carrier();
        assert!(carrier.request(CarrierRequestBody::Discover).is_ok());

        let notice = CarrierNotice {
            session: graphshell_protocol::ProjectionSession("counting".into()),
            epoch: graphshell_protocol::SceneEpoch(7),
            revision: graphshell_protocol::Revision(3),
        };
        carrier.endpoint_mut().notice = Some(notice.clone());
        assert_eq!(carrier.take_notice(), Some(notice));
        assert_eq!(carrier.take_notice(), None, "a notice is delivered once");
    }

    #[test]
    fn the_session_plane_is_refused_by_name_rather_than_faked() {
        // An in-process carrier has no session plane. Answering Open as though
        // it had succeeded would be worse than refusing it.
        let mut carrier = carrier();
        let error = carrier.request(CarrierRequestBody::Close).unwrap_err();
        assert!(error.message().contains("session plane"), "{error}");
    }
}
