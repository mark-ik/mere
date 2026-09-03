// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One mounted endpoint over a blocking carrier.
//!
//! The session machinery every native Graphshell host repeats: discover once,
//! mount projections, resolve presentations on demand, submit advertised
//! actions, and recover through resume when a revision bell arrives.
//!
//! It holds a [`Carrier`] as a trait object and never asks which one, which is
//! what makes "an embedded endpoint" and "a remote one" the same code path with
//! a different argument. Constructing the carrier is the host's business: a
//! stdio carrier needs a program to spawn, a network carrier needs a peer to
//! dial and a service to be admitted to, and neither concern belongs to the
//! state machine behind it.
//!
//! ## This is now an adapter, not the protocol
//!
//! The sequencing moved to [`SessionCore`], which owns no carrier and performs
//! no I/O. What remains here is the blocking half: take the request the core
//! asks for, put it to the carrier, hand the answer back, repeat until the core
//! says the operation is done. That loop is `drive`, private to this file,
//! and the only place in it that blocks.
//!
//! The split exists because the browser cannot block — see the [`crate::core`]
//! module doc for why a worker does not rescue it either. Every public method
//! below keeps its exact signature and behaviour, so no native host changes;
//! the browser writes a second, event-driven adapter over the same core.

use chirograph::{
    AdvertisedAction, CapabilityProfile, Carrier, CarrierError, CarrierNotice, CarrierRequestBody,
    CarrierResponseBody, EndpointDescriptor, IntentResult, ProjectionSession, ResumeRequest,
};
use sceno::InstanceId;
use serde::Serialize;

use crate::action_draft::{ActionDraft, ActionDraftTarget};
use crate::core::{Outcome, Progress, SessionCore};
use crate::{ClientState, ResolvedPresentation};

/// One retained endpoint and its Graphshell client state.
///
/// Resource bytes remain in client state only for this object's lifetime.
/// `close` and `Drop` both release the carrier and discard every mounted
/// session, including memory-only editable source.
pub struct RetainedEndpointSession {
    /// Boxed rather than concrete: the protocol has always described itself as
    /// running over an unspecified carrier, and this is the field that makes
    /// that true. Every constructor supplies one already built.
    carrier: Option<Box<dyn Carrier>>,
    core: SessionCore,
}

impl RetainedEndpointSession {
    /// Mount an endpoint over any carrier.
    ///
    /// The only constructor, and deliberately so. A host embedding an endpoint
    /// passes a `LocalCarrier`, one spawning a process passes a `StdioCarrier`,
    /// one reaching across a connection passes a `NetworkCarrier`. Everything
    /// after discovery is identical, which is the point of the seam: where the
    /// endpoint runs stops being a different code path and becomes a different
    /// argument.
    pub fn over(mut carrier: Box<dyn Carrier>, profile: CapabilityProfile) -> Result<Self, String> {
        let descriptor = match carrier
            .request(SessionCore::discover_request())
            .map_err(|error| error.to_string())?
        {
            CarrierResponseBody::Descriptor(descriptor) => descriptor,
            other => return Err(unexpected("descriptor", &other)),
        };
        Ok(Self {
            carrier: Some(carrier),
            core: SessionCore::from_descriptor(descriptor, profile),
        })
    }

    pub fn descriptor(&self) -> &EndpointDescriptor {
        self.core.descriptor()
    }

    pub fn client(&self) -> &ClientState {
        self.core.client()
    }

    pub fn profile(&self) -> &CapabilityProfile {
        self.core.profile()
    }

    /// Forget one mounted projection and every resource cached beneath it.
    ///
    /// The endpoint process may remain alive for another document, but a closed
    /// editor must not leave its memory-only source in client state.
    pub fn forget(&mut self, session: &ProjectionSession) {
        self.core.forget(session);
    }

    /// Mount one discovered projection without resolving resources or invoking
    /// any of its actions.
    pub fn mount(&mut self, offer_index: usize) -> Result<ProjectionSession, String> {
        let start = self.core.mount(offer_index)?;
        match self.drive(start)? {
            Outcome::Mounted(session) => Ok(session),
            other => Err(misdriven("a mounted session", &other)),
        }
    }

    /// Request a fresh full snapshot using the same projection request that
    /// mounted this session. This is the simple, source-authoritative recovery
    /// path after an accepted action when a host is not waiting on notices.
    pub fn resnapshot(&mut self, session: &ProjectionSession) -> Result<(), String> {
        let start = self.core.resnapshot(session)?;
        match self.drive(start)? {
            Outcome::Resnapshotted => Ok(()),
            other => Err(misdriven("a refreshed snapshot", &other)),
        }
    }

    /// Open one endpoint-advertised bounded action form at the client's current
    /// acknowledgement. The action remains endpoint-authored; this method only
    /// captures the exact action and the snapshot position that may submit it.
    pub fn open_action_draft(
        &self,
        session: &ProjectionSession,
        target: InstanceId,
        intent: &str,
    ) -> Result<(ActionDraft, ActionDraftTarget), String> {
        self.core.open_action_draft(session, target, intent)
    }

    /// Submit a draft composed from endpoint-advertised values. Missing or
    /// invalid local selections fail before the carrier is touched; endpoint
    /// authorization, replay, and stale checks remain authoritative.
    pub fn submit_action_draft(
        &mut self,
        target: &ActionDraftTarget,
        draft: &mut ActionDraft,
    ) -> Result<IntentResult, String> {
        let start = self.core.submit_action_draft(target, draft)?;
        match self.drive(start)? {
            Outcome::Intent(result) => Ok(*result),
            other => Err(misdriven("an intent result", &other)),
        }
    }

    /// Resolve one presentation on demand, fetching only the selected
    /// capability's resource.
    pub fn resolve(
        &mut self,
        session: &ProjectionSession,
        instance: InstanceId,
    ) -> Result<ResolvedPresentation, String> {
        let start = self.core.resolve(session, instance)?;
        match self.drive(start)? {
            Outcome::Resolved(presentation) => Ok(*presentation),
            other => Err(misdriven("a resolved presentation", &other)),
        }
    }

    pub fn resolve_all(
        &mut self,
        session: &ProjectionSession,
    ) -> Result<Vec<(InstanceId, ResolvedPresentation)>, String> {
        let instances = self
            .core
            .client()
            .mounted(session)
            .ok_or_else(|| format!("Graphshell did not mount {}", session.0))?
            .scene
            .active_items_in_order()
            .into_iter()
            .map(|(instance, _)| instance)
            .collect::<Vec<_>>();
        instances
            .into_iter()
            .map(|instance| {
                self.resolve(session, instance)
                    .map(|value| (instance, value))
            })
            .collect()
    }

    /// Invoke an action exactly as advertised, using the current client
    /// acknowledgement and a typed, versioned payload.
    pub fn invoke<T: Serialize>(
        &mut self,
        session: &ProjectionSession,
        target: InstanceId,
        action: &AdvertisedAction,
        payload: &T,
    ) -> Result<IntentResult, String> {
        let start = self.core.invoke(session, target, action, payload)?;
        match self.drive(start)? {
            Outcome::Intent(result) => Ok(*result),
            other => Err(misdriven("an intent result", &other)),
        }
    }

    /// Block for one revision bell and recover through the ordinary resume
    /// path. Source bytes never travel in the notice.
    pub fn wait_for_change(&mut self) -> Result<bool, String> {
        let heard = match self.carrier.as_deref_mut() {
            Some(carrier) => carrier.wait_for_notice(),
            None => return Err(CLOSED.to_string()),
        };
        let notice = self.observe(heard)?;
        self.resume(notice)
    }

    /// Pump the carrier without waiting for a notice.
    ///
    /// The discovery request is a harmless round trip that lets the carrier
    /// collect any notices already written by the endpoint. This is intended for
    /// a background owner that polls on a short cadence while its UI remains
    /// entirely local.
    pub fn poll_for_change(&mut self) -> Result<bool, String> {
        let start = self.core.poll();
        self.drive(start)?;
        let mut changed = false;
        loop {
            let notice = self
                .carrier
                .as_mut()
                .and_then(|carrier| carrier.take_notice());
            let Some(notice) = notice else {
                break;
            };
            changed |= self.resume(notice)?;
        }
        Ok(changed)
    }

    pub fn close(mut self) -> Result<(), String> {
        let carrier_result = if self.carrier.is_some() {
            let start = self.core.close();
            let close = self.drive(start).map(|_| ());
            // Taken after the request, so the close verb still travels.
            let mut carrier = self.carrier.take().expect("carrier was present");
            let shutdown = carrier
                .shutdown()
                .map_err(|error| format!("endpoint did not stop cleanly: {error}"));
            close.and(shutdown)
        } else {
            Ok(())
        };
        self.core.purge();
        carrier_result
    }

    /// The carrier itself, for a host sending a verb this wrapper does not
    /// model.
    ///
    /// An escape hatch rather than the ordinary path: everything above keeps
    /// client state consistent with what the endpoint was told, and a caller
    /// reaching past it owns that consistency itself.
    pub fn carrier_mut(&mut self) -> Result<&mut (dyn Carrier + 'static), String> {
        self.carrier
            .as_deref_mut()
            .ok_or_else(|| CLOSED.to_string())
    }

    // ── The blocking half ───────────────────────────────────────────────────

    /// Carry the core's requests to the carrier until the operation finishes.
    ///
    /// The whole of this adapter. Every multi-step operation — a resolve that
    /// needs two resources, a resume the endpoint answers with a resynchronize —
    /// is the core asking again, so this loop does not know or care which
    /// operation it is driving.
    fn drive(&mut self, mut progress: Progress<Outcome>) -> Result<Outcome, String> {
        loop {
            match progress {
                Progress::Done(outcome) => return Ok(outcome),
                Progress::Ask(body) => {
                    let answer = self.ask(body)?;
                    progress = self.core.on_response(answer)?;
                }
            }
        }
    }

    /// Send one request, and notice when the answer means the session is gone.
    ///
    /// The single place disconnection is observed. A refusal leaves every
    /// mounted scene exactly as it was, because the endpoint is still there and
    /// merely said no; a disconnection marks them all, so a host stops
    /// presenting a document it can no longer save to.
    fn ask(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String> {
        let outcome = match self.carrier.as_deref_mut() {
            Some(carrier) => carrier.request(body),
            None => return Err(CLOSED.to_string()),
        };
        self.observe(outcome)
    }

    /// Record what a carrier outcome means for the scenes this session holds.
    fn observe<T>(&mut self, outcome: Result<T, CarrierError>) -> Result<T, String> {
        match outcome {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.is_disconnected() {
                    self.core.disconnect();
                }
                Err(error.to_string())
            }
        }
    }

    /// Drive one notice through the core's resume machine.
    fn resume(&mut self, notice: CarrierNotice) -> Result<bool, String> {
        let start = self.core.resume_from_notice(notice)?;
        match self.drive(start)? {
            Outcome::Changed(changed) => Ok(changed),
            other => Err(misdriven("a resume result", &other)),
        }
    }
}

impl Drop for RetainedEndpointSession {
    fn drop(&mut self) {
        // A carrier may hold a process, a socket, or nothing. Taking it makes
        // the release unconditional, so whatever the carrier holds is released.
        drop(self.carrier.take());
        self.core.purge();
    }
}

const CLOSED: &str = "endpoint carrier is closed";

/// Recover from a revision notice over a blocking carrier.
///
/// Retained for hosts that drive a carrier directly rather than through a
/// [`RetainedEndpointSession`]. New code should prefer
/// [`SessionCore::resume_from_notice`], which is the same sequencing without
/// the carrier.
pub fn resume_after_notice(
    carrier: &mut (dyn Carrier + 'static),
    client: &mut ClientState,
    notice: &CarrierNotice,
) -> Result<bool, String> {
    let Some(mut request) = resume_request_for_notice(client, notice)? else {
        return Ok(false);
    };
    for _ in 0..crate::core::RESUME_ATTEMPTS {
        let reply = match carrier
            .request(CarrierRequestBody::Resume(request))
            .map_err(|error| error.to_string())?
        {
            CarrierResponseBody::Resume(reply) => reply,
            other => return Err(unexpected("resume reply", &other)),
        };
        match client
            .apply_resume(&notice.session, reply)
            .map_err(|error| {
                format!(
                    "Graphshell rejected resume for {}: {error:?}",
                    notice.session.0
                )
            })? {
            crate::ResumeApplication::Current(_) | crate::ResumeApplication::Applied(_) => {
                return Ok(true);
            }
            crate::ResumeApplication::Resynchronize(next) => request = next,
        }
    }
    Err("endpoint did not produce an applicable resume after four attempts".into())
}

pub fn resume_request_for_notice(
    client: &mut ClientState,
    notice: &CarrierNotice,
) -> Result<Option<ResumeRequest>, String> {
    let acknowledged = client
        .acknowledgement(&notice.session)
        .ok_or_else(|| format!("revision notice names unknown session {}", notice.session.0))?;
    if notice.epoch == acknowledged.epoch && notice.revision <= acknowledged.revision {
        return Ok(None);
    }
    client.mark_stale(&notice.session);
    Ok(client.resume_request(&notice.session))
}

/// Name the answer an endpoint gave when it was not the one asked for.
pub fn unexpected(expected: &str, actual: &CarrierResponseBody) -> String {
    format!(
        "endpoint returned {} while Graphshell expected {expected}",
        match actual {
            CarrierResponseBody::Descriptor(_) => "a descriptor",
            CarrierResponseBody::Snapshot(_) => "a snapshot",
            CarrierResponseBody::Resource(_) => "a resource",
            CarrierResponseBody::ResourceChunk(_) => "a resource chunk",
            CarrierResponseBody::Resume(_) => "a resume reply",
            CarrierResponseBody::Intent(_) => "an intent result",
            CarrierResponseBody::Opened(_) => "an opened session",
            CarrierResponseBody::Closed => "a session close",
            CarrierResponseBody::Suspended => "a session suspend",
        }
    )
}

/// Name the outcome the core produced when it was not the one this operation
/// started.
///
/// Unreachable unless the core and this adapter disagree about which operation
/// is in flight, which would be a bug in one of them rather than anything an
/// endpoint can cause. It is a message rather than a panic because a host
/// should be able to report it and carry on.
fn misdriven(expected: &str, actual: &Outcome) -> String {
    format!("Graphshell expected {expected} but the session core produced {actual:?}")
}
