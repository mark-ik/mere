//! One mounted endpoint and the client state behind it.
//!
//! The session machinery every Graphshell host repeats: discover once, mount
//! projections, resolve presentations on demand, submit advertised actions,
//! and recover through resume when a revision bell arrives.
//!
//! It holds a [`Carrier`] as a trait object and never asks which one, which
//! is what makes "an embedded endpoint" and "a remote one" the same code path
//! with a different argument. Constructing the carrier is the host's business:
//! a stdio carrier needs a program to spawn, a network carrier needs a peer to
//! dial and a service to be admitted to, and neither concern belongs to the
//! state machine that follows.

use std::collections::{BTreeMap, BTreeSet};

use graphshell_protocol::{
    AdvertisedAction, CapabilityProfile, Carrier, CarrierError, CarrierNotice, CarrierRequestBody,
    CarrierResponseBody, EndpointDescriptor, IntentInvocation, IntentResult, ProjectionRequest,
    ProjectionSession, ResumeRequest,
};
use sceno::InstanceId;
use serde::Serialize;

use crate::action_draft::{ActionDraft, ActionDraftTarget};
use crate::{ClientState, PresentationResolution, ResolvedPresentation, ResumeApplication};

/// One retained endpoint and its Graphshell client state.
///
/// Resource bytes remain in `ClientState` only for this object's lifetime.
/// `close` and `Drop` both release the carrier and discard every mounted
/// session, including memory-only editable source.
pub struct RetainedEndpointSession {
    /// Boxed rather than concrete: the protocol has always described itself as
    /// running over an unspecified carrier, and this is the field that makes
    /// that true. Every constructor above supplies one already built.
    carrier: Option<Box<dyn Carrier>>,
    client: ClientState,
    profile: CapabilityProfile,
    descriptor: EndpointDescriptor,
    mounted: BTreeSet<ProjectionSession>,
    requests: BTreeMap<ProjectionSession, ProjectionRequest>,
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
            .request(CarrierRequestBody::Discover)
            .map_err(|error| error.to_string())?
        {
            CarrierResponseBody::Descriptor(descriptor) => descriptor,
            other => return Err(unexpected("descriptor", &other)),
        };
        Ok(Self {
            carrier: Some(carrier),
            client: ClientState::default(),
            profile,
            descriptor,
            mounted: BTreeSet::new(),
            requests: BTreeMap::new(),
        })
    }

    pub fn descriptor(&self) -> &EndpointDescriptor {
        &self.descriptor
    }

    pub fn client(&self) -> &ClientState {
        &self.client
    }

    pub fn profile(&self) -> &CapabilityProfile {
        &self.profile
    }

    /// Forget one mounted projection and every resource cached beneath it.
    ///
    /// The endpoint process may remain alive for another document, but a
    /// closed editor must not leave its memory-only source in client state.
    pub fn forget(&mut self, session: &ProjectionSession) {
        self.mounted.remove(session);
        self.requests.remove(session);
        self.client.forget_session(session);
    }

    /// Mount one discovered projection without resolving resources or invoking
    /// any of its actions.
    pub fn mount(&mut self, offer_index: usize) -> Result<ProjectionSession, String> {
        let request = self
            .descriptor
            .projections
            .get(offer_index)
            .map(|offer| offer.request.clone())
            .ok_or_else(|| format!("endpoint has no projection {offer_index}"))?;
        let snapshot = match self.ask(CarrierRequestBody::Snapshot(request.clone()))? {
            CarrierResponseBody::Snapshot(snapshot) => *snapshot,
            other => return Err(unexpected("snapshot", &other)),
        };
        self.apply_snapshot(snapshot, request)
    }

    /// Request a fresh full snapshot using the same projection request that
    /// mounted this session. This is the simple, source-authoritative recovery
    /// path after an accepted action when a host is not waiting on notices.
    pub fn resnapshot(&mut self, session: &ProjectionSession) -> Result<(), String> {
        let request = self
            .requests
            .get(session)
            .cloned()
            .ok_or_else(|| format!("Graphshell did not mount {}", session.0))?;
        let snapshot = match self.ask(CarrierRequestBody::Snapshot(request.clone()))? {
            CarrierResponseBody::Snapshot(snapshot) => *snapshot,
            other => return Err(unexpected("snapshot", &other)),
        };
        if snapshot.session != *session {
            return Err(format!(
                "endpoint resnapshot changed session {} to {}",
                session.0, snapshot.session.0
            ));
        }
        self.apply_snapshot(snapshot, request).map(|_| ())
    }

    /// Open one endpoint-advertised bounded action form at the client's
    /// current acknowledgement. The action remains endpoint-authored; this
    /// method only captures the exact action and the snapshot position that
    /// may submit it.
    pub fn open_action_draft(
        &self,
        session: &ProjectionSession,
        target: InstanceId,
        intent: &str,
    ) -> Result<(ActionDraft, ActionDraftTarget), String> {
        let tree = self
            .client
            .accessibility_tree(session, &self.profile)
            .map_err(|error| format!("could not inspect {}: {error:?}", session.0))?;
        let action = tree
            .children
            .iter()
            .find(|item| item.instance == target)
            .and_then(|item| item.actions.iter().find(|action| action.intent.0 == intent))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "intent {intent} was not advertised for {} in {}",
                    target.0, session.0
                )
            })?;
        if action.input_form.is_none() {
            return Err(format!(
                "intent {intent} does not advertise a bounded action form"
            ));
        }
        let acknowledgement = self
            .client
            .acknowledgement(session)
            .ok_or_else(|| format!("Graphshell did not acknowledge {}", session.0))?;
        Ok((
            ActionDraft::new(action),
            ActionDraftTarget {
                session: session.clone(),
                target,
                observed_epoch: acknowledgement.epoch,
                observed_revision: acknowledgement.revision,
            },
        ))
    }

    /// Submit a draft composed from endpoint-advertised values. Missing or
    /// invalid local selections fail before the carrier is touched; endpoint
    /// authorization, replay, and stale checks remain authoritative.
    pub fn submit_action_draft(
        &mut self,
        target: &ActionDraftTarget,
        draft: &mut ActionDraft,
    ) -> Result<IntentResult, String> {
        if !self.mounted.contains(&target.session) {
            return Err(format!("Graphshell did not mount {}", target.session.0));
        }
        let invocation = draft
            .invocation(target)
            .map_err(|error| format!("could not compose advertised action: {error}"))?;
        match self.ask(CarrierRequestBody::Intent(invocation))? {
            CarrierResponseBody::Intent(result) => Ok(result),
            other => Err(unexpected("intent result", &other)),
        }
    }

    fn apply_snapshot(
        &mut self,
        snapshot: graphshell_protocol::ProjectionSnapshot,
        request: ProjectionRequest,
    ) -> Result<ProjectionSession, String> {
        let session = snapshot.session.clone();
        self.client
            .apply_snapshot(snapshot)
            .map_err(|error| format!("Graphshell rejected {session:?}: {error:?}"))?;
        self.mounted.insert(session.clone());
        self.requests.insert(session.clone(), request);
        Ok(session)
    }

    /// Resolve one presentation on demand, fetching only the selected
    /// capability's resource.
    pub fn resolve(
        &mut self,
        session: &ProjectionSession,
        instance: InstanceId,
    ) -> Result<ResolvedPresentation, String> {
        loop {
            match self
                .client
                .resolve(session, instance, &self.profile)
                .map_err(|error| format!("could not resolve {}: {error:?}", session.0))?
            {
                PresentationResolution::Ready(presentation) => return Ok(presentation),
                PresentationResolution::NeedsResource(request) => {
                    let response = match self.ask(CarrierRequestBody::Resource(request))? {
                        CarrierResponseBody::Resource(response) => response,
                        other => return Err(unexpected("resource", &other)),
                    };
                    self.client
                        .apply_resource(response)
                        .map_err(|error| format!("resource was rejected: {error:?}"))?;
                }
            }
        }
    }

    pub fn resolve_all(
        &mut self,
        session: &ProjectionSession,
    ) -> Result<Vec<(InstanceId, ResolvedPresentation)>, String> {
        let instances = self
            .client
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
        let advertised = self
            .client
            .mounted(session)
            .and_then(|mounted| mounted.presentation.offers_for(target))
            .and_then(|offers| {
                offers
                    .iter()
                    .find(|offer| self.profile.supports(offer.requires))
            })
            .is_some_and(|offer| {
                offer
                    .semantics
                    .actions
                    .iter()
                    .any(|candidate| candidate == action)
            });
        if !advertised {
            return Err("intent was not advertised for the selected presentation".into());
        }
        let ack = self
            .client
            .acknowledgement(session)
            .ok_or_else(|| format!("Graphshell did not acknowledge {}", session.0))?;
        let payload = serde_json::to_vec(payload)
            .map_err(|error| format!("could not encode intent payload: {error}"))?;
        match self.ask(CarrierRequestBody::Intent(IntentInvocation {
            session: session.clone(),
            target,
            observed_epoch: ack.epoch,
            observed_revision: ack.revision,
            intent: action.intent.0.clone(),
            payload,
        }))? {
            CarrierResponseBody::Intent(result) => Ok(result),
            other => Err(unexpected("intent result", &other)),
        }
    }

    /// Block for one revision bell and recover through the ordinary resume
    /// path. Source bytes never travel in the notice.
    pub fn wait_for_change(&mut self) -> Result<bool, String> {
        let heard = match self.carrier.as_deref_mut() {
            Some(carrier) => carrier.wait_for_notice(),
            None => return Err("endpoint carrier is closed".to_string()),
        };
        let notice = self.observe(heard)?;
        let carrier = self
            .carrier
            .as_deref_mut()
            .ok_or_else(|| "endpoint carrier is closed".to_string())?;
        resume_after_notice(carrier, &mut self.client, &notice)
    }

    /// Pump the carrier without waiting for a notice.
    ///
    /// The discovery request is a harmless round trip that lets the carrier
    /// collect any notices already written by the endpoint. This is intended
    /// for a background owner that polls on a short cadence while its UI
    /// remains entirely local.
    pub fn poll_for_change(&mut self) -> Result<bool, String> {
        match self.ask(CarrierRequestBody::Discover)? {
            CarrierResponseBody::Descriptor(_) => {}
            other => return Err(unexpected("descriptor", &other)),
        }
        let mut changed = false;
        loop {
            let notice = self
                .carrier
                .as_mut()
                .and_then(|carrier| carrier.take_notice());
            let Some(notice) = notice else {
                break;
            };
            let carrier = self
                .carrier
                .as_deref_mut()
                .ok_or_else(|| "endpoint carrier is closed".to_string())?;
            changed |= resume_after_notice(carrier, &mut self.client, &notice)?;
        }
        Ok(changed)
    }

    pub fn close(mut self) -> Result<(), String> {
        let carrier_result = if let Some(mut carrier) = self.carrier.take() {
            let response = carrier.request(CarrierRequestBody::Close);
            let close = match response {
                Ok(CarrierResponseBody::Closed) => Ok(()),
                Ok(other) => Err(unexpected("session close", &other)),
                Err(error) => Err(error.to_string()),
            };
            let shutdown = carrier
                .shutdown()
                .map_err(|error| format!("endpoint did not stop cleanly: {error}"));
            close.and(shutdown)
        } else {
            Ok(())
        };
        self.purge();
        carrier_result
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
            None => return Err("endpoint carrier is closed".to_string()),
        };
        self.observe(outcome)
    }

    /// Record what a carrier outcome means for the scenes this session holds.
    fn observe<T>(&mut self, outcome: Result<T, CarrierError>) -> Result<T, String> {
        match outcome {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.is_disconnected() {
                    self.disconnect();
                }
                Err(error.to_string())
            }
        }
    }

    /// Every mounted projection stops being live.
    ///
    /// The scene is kept rather than dropped: a host still wants to show what
    /// was there, it just must not offer to save into it.
    fn disconnect(&mut self) {
        let Self {
            mounted, client, ..
        } = self;
        for session in mounted.iter() {
            client.mark_disconnected(session);
        }
    }

    /// The carrier itself, for a host sending a verb this wrapper does not
    /// model.
    ///
    /// An escape hatch rather than the ordinary path: everything above keeps
    /// client state consistent with what the endpoint was told, and a caller
    /// reaching through here owns that consistency itself.
    pub fn carrier_mut(&mut self) -> Result<&mut (dyn Carrier + 'static), String> {
        self.carrier
            .as_deref_mut()
            .ok_or_else(|| "endpoint carrier is closed".to_string())
    }

    fn purge(&mut self) {
        for session in std::mem::take(&mut self.mounted) {
            self.client.forget_session(&session);
        }
        self.requests.clear();
    }
}

impl Drop for RetainedEndpointSession {
    fn drop(&mut self) {
        self.purge();
        // A carrier may hold a process, a socket, or nothing. Taking it makes
        // the order explicit: source bytes are purged before whatever the
        // carrier holds is released.
        drop(self.carrier.take());
    }
}

pub fn resume_after_notice(
    carrier: &mut (dyn Carrier + 'static),
    client: &mut ClientState,
    notice: &CarrierNotice,
) -> Result<bool, String> {
    let Some(mut request) = resume_request_for_notice(client, notice)? else {
        return Ok(false);
    };
    for _ in 0..4 {
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
            ResumeApplication::Current(_) | ResumeApplication::Applied(_) => return Ok(true),
            ResumeApplication::Resynchronize(next) => request = next,
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
