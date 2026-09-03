// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The event-driven adapter: [`SessionCore`] over an NDJSON line wire.
//!
//! [`crate::session::RetainedEndpointSession`] is the blocking adapter, which a
//! browser cannot use. This is the other one. It never blocks, never waits, and
//! never touches a socket: a caller asks it what to send, writes that line
//! wherever the endpoint is, and hands back whatever lines come off the wire.
//!
//! ## Why this is not browser code
//!
//! Nothing here knows what a data channel is. The browser's actual glue —
//! `send`, `onmessage` — is a dozen lines in the port, and everything with a
//! rule in it lives here, where it compiles and its tests *run* on an ordinary
//! `cargo test`. That is the same division the WebRTC probe already draws
//! between its `sdp` module and its wasm half, and for the same reason: a rule
//! that can only be exercised in a browser is a rule nobody exercises.
//!
//! ## The wire
//!
//! One JSON value per line, which is what `graphshell-stdio` speaks and what
//! `serve_admitted_session` reads. Outbound lines are [`CarrierRequest`];
//! inbound lines are [`CarrierOutput`], which is untagged and so is either a
//! [`chirograph::CarrierResponse`] carrying an `id` or a [`CarrierNotice`]
//! carrying a
//! session and a revision.
//!
//! ## What this adapter has that the blocking one gets for free
//!
//! Request identity. A blocking carrier reads the answer to the question it
//! just asked, so correlation is implicit in the call stack. Here it is not:
//! lines arrive whenever they arrive, so every request carries an id and a
//! response naming a different one is refused rather than folded into whatever
//! happens to be in flight. That is the difference between a stalled session
//! and a silently wrong one.

use std::collections::VecDeque;

use chirograph::{
    CapabilityProfile, CarrierNotice, CarrierOutput, CarrierRequest, CarrierRequestBody,
    CarrierResponseBody, EndpointDescriptor,
};

use crate::core::{Outcome, Progress, SessionCore};

/// What the caller should do next.
#[derive(Debug)]
#[must_use = "a driver that asked for a line to be sent is waiting on the answer"]
pub enum Advance {
    /// Write this line to the endpoint, then keep feeding [`SessionDriver::on_line`].
    Send(String),
    /// The operation finished.
    Done(Outcome),
    /// The line was a revision bell. It is queued for
    /// [`SessionDriver::take_notice`]; nothing needs sending.
    Noted,
}

/// One endpoint driven by lines rather than by blocking calls.
pub struct SessionDriver {
    /// `None` until discovery answers: a core cannot exist without the
    /// descriptor it is built from.
    core: Option<SessionCore>,
    profile: CapabilityProfile,
    next_id: u64,
    /// The request whose answer is expected next, if any.
    inflight: Option<u64>,
    notices: VecDeque<CarrierNotice>,
}

impl SessionDriver {
    /// A driver that has not discovered its endpoint yet.
    pub fn new(profile: CapabilityProfile) -> Self {
        Self {
            core: None,
            profile,
            next_id: 1,
            inflight: None,
            notices: VecDeque::new(),
        }
    }

    /// A driver for an endpoint whose descriptor is already in hand.
    pub fn from_descriptor(descriptor: EndpointDescriptor, profile: CapabilityProfile) -> Self {
        Self {
            core: Some(SessionCore::from_descriptor(descriptor, profile.clone())),
            profile,
            next_id: 1,
            inflight: None,
            notices: VecDeque::new(),
        }
    }

    /// The session state, once discovery has answered.
    pub fn core(&self) -> Option<&SessionCore> {
        self.core.as_ref()
    }

    /// The session state, for starting an operation.
    ///
    /// The ordinary shape is two statements — ask the core to start something,
    /// then hand what it wants to [`SessionDriver::begin`]:
    ///
    /// ```ignore
    /// let progress = driver.core_mut().ok_or("not discovered")?.mount(0)?;
    /// let advance = driver.begin(progress)?;
    /// ```
    pub fn core_mut(&mut self) -> Option<&mut SessionCore> {
        self.core.as_mut()
    }

    /// Whether a request is waiting on its answer.
    pub fn is_awaiting(&self) -> bool {
        self.inflight.is_some()
    }

    /// Start discovery.
    ///
    /// On a fresh driver the answer builds the core. On one that already has a
    /// core — a reconnect — the core tracks the discovery itself, so the
    /// descriptor is refreshed and everything mounted is kept.
    pub fn discover(&mut self) -> Result<Advance, String> {
        match self.core.as_mut() {
            None => self.send(SessionCore::discover_request()),
            Some(core) => {
                let progress = core.rediscover();
                self.begin(progress)
            }
        }
    }

    /// Encode whatever the core just asked for.
    ///
    /// Takes the [`Progress`] a core operation returned, so one method serves
    /// every operation rather than this type restating the core's surface.
    pub fn begin(&mut self, progress: Progress<Outcome>) -> Result<Advance, String> {
        match progress {
            Progress::Done(outcome) => Ok(Advance::Done(outcome)),
            Progress::Ask(body) => self.send(body),
        }
    }

    /// Feed one line that arrived from the endpoint.
    ///
    /// A notice is queued and reported as [`Advance::Noted`]; recovering from
    /// it is the caller's move, through [`SessionDriver::take_notice`] and
    /// [`SessionCore::resume_from_notice`], because a host may want to coalesce
    /// several bells before resuming once.
    pub fn on_line(&mut self, line: &str) -> Result<Advance, String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(Advance::Noted);
        }
        let output: CarrierOutput = serde_json::from_str(trimmed)
            .map_err(|error| format!("endpoint sent a line Graphshell could not read: {error}"))?;
        match output {
            CarrierOutput::Notice(notice) => {
                self.notices.push_back(notice);
                Ok(Advance::Noted)
            }
            CarrierOutput::Response(response) => {
                let expected = self.inflight.ok_or_else(|| {
                    format!(
                        "endpoint answered request {} that Graphshell never sent",
                        response.id
                    )
                })?;
                if response.id != expected {
                    return Err(format!(
                        "endpoint answered request {} while Graphshell was waiting on {expected}",
                        response.id
                    ));
                }
                self.inflight = None;
                let body = response.body.map_err(|failure| failure.message)?;
                let core = self.core_or_adopt(body)?;
                match core {
                    Adopted::Descriptor(descriptor) => {
                        Ok(Advance::Done(Outcome::Descriptor(Box::new(descriptor))))
                    }
                    Adopted::Folded(progress) => self.begin(progress),
                }
            }
        }
    }

    /// Take one queued revision bell.
    pub fn take_notice(&mut self) -> Option<CarrierNotice> {
        self.notices.pop_front()
    }

    /// How many bells are queued.
    pub fn queued_notices(&self) -> usize {
        self.notices.len()
    }

    /// Mark every mounted projection as no longer live.
    ///
    /// The event-driven equivalent of the blocking adapter noticing a
    /// disconnected `CarrierError`: a browser learns the link is gone from the
    /// data channel closing, not from a failed call, so the host tells the
    /// session rather than the session discovering it.
    pub fn disconnect(&mut self) {
        self.inflight = None;
        if let Some(core) = self.core.as_mut() {
            core.disconnect();
        }
    }

    // ── Internals ───────────────────────────────────────────────────────────

    /// Serialize one request body as a line and record its id as in flight.
    fn send(&mut self, body: CarrierRequestBody) -> Result<Advance, String> {
        if let Some(id) = self.inflight {
            return Err(format!(
                "Graphshell is still waiting on request {id} and will not send another"
            ));
        }
        let id = self.next_id;
        self.next_id += 1;
        let line = serde_json::to_string(&CarrierRequest { id, body })
            .map_err(|error| format!("could not encode a Graphshell request: {error}"))?;
        self.inflight = Some(id);
        Ok(Advance::Send(line))
    }

    /// Route one response body: the first descriptor builds the core, anything
    /// else is folded into it.
    fn core_or_adopt(
        &mut self,
        body: CarrierResponseBody,
    ) -> Result<Adopted, String> {
        match self.core.as_mut() {
            Some(core) => Ok(Adopted::Folded(core.on_response(body)?)),
            None => match body {
                CarrierResponseBody::Descriptor(descriptor) => {
                    self.core = Some(SessionCore::from_descriptor(
                        descriptor.clone(),
                        self.profile.clone(),
                    ));
                    Ok(Adopted::Descriptor(descriptor))
                }
                other => Err(crate::session::unexpected("descriptor", &other)),
            },
        }
    }
}

/// Whether a response built the core or was folded into it.
enum Adopted {
    Descriptor(EndpointDescriptor),
    Folded(Progress<Outcome>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use chirograph::{
        CarrierFailure, CarrierResponse, ProjectionOffer, ProjectionRequest, ProjectionSession,
        ProtocolVersion,
    };
    use sceno::score::{Arrangement, Score};
    use scenotime::{Revision, SceneEpoch};

    fn descriptor() -> EndpointDescriptor {
        let session = ProjectionSession("fixture:one".into());
        EndpointDescriptor {
            label: "fixture endpoint".into(),
            projections: vec![ProjectionOffer {
                label: "one".into(),
                request: ProjectionRequest {
                    version: ProtocolVersion::V1,
                    session,
                    score: Score::new(Arrangement::Spiral(Default::default())),
                },
            }],
        }
    }

    fn driver() -> SessionDriver {
        SessionDriver::from_descriptor(descriptor(), CapabilityProfile::default())
    }

    fn line_of(response: CarrierResponse) -> String {
        serde_json::to_string(&CarrierOutput::Response(response)).expect("encodes")
    }

    fn sent(advance: Advance) -> String {
        match advance {
            Advance::Send(line) => line,
            other => panic!("expected a line to send, got {other:?}"),
        }
    }

    fn request_id(line: &str) -> u64 {
        serde_json::from_str::<serde_json::Value>(line).expect("valid json")["id"]
            .as_u64()
            .expect("an id")
    }

    /// Start a mount and return the id of the request it put on the wire.
    fn mount_in_flight(driver: &mut SessionDriver) -> u64 {
        let progress = driver
            .core_mut()
            .expect("discovered")
            .mount(0)
            .expect("offer 0 exists");
        request_id(&sent(driver.begin(progress).expect("mount encodes")))
    }

    /// Discovery has no core to fold into, so the first descriptor builds one.
    #[test]
    fn discovery_builds_the_core() {
        let mut driver = SessionDriver::new(CapabilityProfile::default());
        assert!(driver.core().is_none());
        let line = sent(driver.discover().expect("discovery starts"));
        assert!(driver.is_awaiting());

        let reply = line_of(CarrierResponse {
            id: request_id(&line),
            body: Ok(CarrierResponseBody::Descriptor(descriptor())),
        });
        let advance = driver.on_line(&reply).expect("descriptor is readable");
        assert!(matches!(advance, Advance::Done(Outcome::Descriptor(_))));
        assert!(driver.core().is_some(), "the descriptor built the core");
        assert!(!driver.is_awaiting());
    }

    /// The reconnect case: discovering again on a driver that already has a
    /// core refreshes the descriptor and keeps the core — it must not arrive as
    /// an answer nobody asked for. The first headed reconnect died exactly
    /// there.
    #[test]
    fn discovering_again_on_a_live_core_is_answered_and_keeps_the_core() {
        let mut driver = driver();
        assert!(driver.core().is_some());
        let line = sent(driver.discover().expect("a second discovery starts"));
        let reply = line_of(CarrierResponse {
            id: request_id(&line),
            body: Ok(CarrierResponseBody::Descriptor(descriptor())),
        });
        let advance = driver
            .on_line(&reply)
            .expect("the descriptor is an answer the core was waiting for");
        assert!(matches!(advance, Advance::Done(Outcome::Descriptor(_))), "{advance:?}");
        assert!(driver.core().is_some(), "the core survives a rediscovery");
        assert!(!driver.is_awaiting());
    }

    /// The property the blocking adapter got free from its call stack: an
    /// answer to a question nobody asked is refused, not folded in.
    #[test]
    fn an_answer_to_a_request_never_sent_is_refused() {
        let mut driver = driver();
        let stray = line_of(CarrierResponse {
            id: 99,
            body: Ok(CarrierResponseBody::Closed),
        });
        let error = driver.on_line(&stray).expect_err("nothing was in flight");
        assert!(error.contains("never sent"), "{error}");
    }

    /// A late answer to a superseded request must not be mistaken for the
    /// answer to the current one.
    #[test]
    fn an_answer_naming_the_wrong_request_is_refused() {
        let mut driver = driver();
        let id = mount_in_flight(&mut driver);
        let stale = line_of(CarrierResponse {
            id: id + 1000,
            body: Ok(CarrierResponseBody::Closed),
        });
        let error = driver.on_line(&stale).expect_err("wrong id");
        assert!(error.contains("while Graphshell was waiting on"), "{error}");
        assert!(driver.is_awaiting(), "the real answer is still expected");
    }

    /// One request at a time. Two in flight would make the id correlation above
    /// meaningless, so the second is refused at the point of sending.
    #[test]
    fn a_second_request_while_one_is_in_flight_is_refused() {
        let mut driver = driver();
        let _ = mount_in_flight(&mut driver);
        let error = driver.discover().expect_err("one at a time");
        assert!(error.contains("still waiting on request"), "{error}");
    }

    /// An endpoint refusal is the operation's answer, not a broken session: the
    /// driver stops awaiting so the next request can go out.
    #[test]
    fn a_refusal_clears_the_inflight_request() {
        let mut driver = driver();
        let id = mount_in_flight(&mut driver);
        let refusal = line_of(CarrierResponse {
            id,
            body: Err(CarrierFailure {
                message: "projection is not available".into(),
            }),
        });
        let error = driver.on_line(&refusal).expect_err("the endpoint said no");
        assert!(error.contains("not available"), "{error}");
        assert!(
            !driver.is_awaiting(),
            "a refusal is an answer, so the session is free to ask again"
        );
    }

    /// Notices arrive unsolicited and must not be mistaken for the answer being
    /// waited on.
    #[test]
    fn a_notice_queues_without_disturbing_the_inflight_request() {
        let mut driver = driver();
        let _ = mount_in_flight(&mut driver);

        let bell = serde_json::to_string(&CarrierOutput::Notice(CarrierNotice {
            session: ProjectionSession("fixture:one".into()),
            epoch: SceneEpoch(1),
            revision: Revision(9),
        }))
        .expect("encodes");
        let advance = driver.on_line(&bell).expect("a bell is readable");
        assert!(matches!(advance, Advance::Noted));
        assert_eq!(driver.queued_notices(), 1);
        assert!(driver.is_awaiting(), "the snapshot answer is still expected");
        assert!(driver.take_notice().is_some());
        assert_eq!(driver.queued_notices(), 0);
    }

    #[test]
    fn an_unreadable_line_is_named_as_such() {
        let mut driver = driver();
        let error = driver.on_line("{not json").expect_err("garbage");
        assert!(error.contains("could not read"), "{error}");
    }

    /// A blank line is the ordinary consequence of a trailing newline on the
    /// wire, not an error.
    #[test]
    fn a_blank_line_is_ignored() {
        let mut driver = driver();
        assert!(matches!(
            driver.on_line("   ").expect("blank"),
            Advance::Noted
        ));
    }

    /// Losing the link clears what was in flight, so a reconnected session does
    /// not wait forever on an answer that can never arrive.
    #[test]
    fn disconnecting_clears_the_inflight_request() {
        let mut driver = driver();
        let _ = mount_in_flight(&mut driver);
        assert!(driver.is_awaiting());
        driver.disconnect();
        assert!(!driver.is_awaiting());
    }
}
