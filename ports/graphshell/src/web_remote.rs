// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The remote link: where the remote projection lives, and how its answers
//! arrive.
//!
//! Two realizations. The **fixture** is the H3 page's in-process
//! `canary::FixtureEndpoint`, driven synchronously and mounted into the
//! app's own `ClientState`. The **WebRTC** link is a real host reached
//! through the C4 door: `BrowserJoin` → `BrowserSession` →
//! `graphshell_client::SessionDriver`, whose `SessionCore` owns the
//! `ClientState` the remote scene lives in. `BrowserHost::remote_client` is
//! the one accessor that answers which of the two holds it, and everything
//! that reads the remote scene goes through it.
//!
//! The difference the presenter has to absorb is *when* answers arrive. On
//! the fixture an intent is a call and a re-snapshot. Over WebRTC every
//! operation is ask → send a line → some later frame, a line comes back →
//! outcome. So an operation is *begun* (`begin_remote`), its kind is
//! remembered as the [`RemoteOp`] in flight, and `on_remote_line` finishes
//! it when the host answers; one operation at a time, which is the driver's
//! rule too. Bells are the other direction: a host that accepted an intent
//! writes a revision notice on the next round trip, so an acceptance is
//! followed by a poll, and every queued notice drives a resume that the host
//! answers by diff — the C4a rows, now feeding the canvas.
//!
//! Given `?signal=<url>` (and optionally `?invite=<fragment>`, else fetched
//! from the signaling server's `/invite`), `loader.js` calls
//! [`connect_remote`] once the host is ready. Without it the fixture stays,
//! as ruled: the choice is exposed, not made.

use std::cell::RefCell;
use std::rc::Rc;

use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::StreamExt;
use graphshell::canary::FixtureEndpoint;
use graphshell::client::{ActionDraft, ActionDraftTarget, ClientState, MountedScene};
use graphshell::protocol::{
    AdvertisedAction, CapabilityProfile, IntentResult, PresentationCapability,
};
use graphshell::webrtc_browser::{
    BrowserInitiatorConfig, BrowserJoin, BrowserSession, BrowserWriter, HandshakeLimits,
    InMemoryProvider, InviteV1, RetiredSession, SignedDelegationCertificate,
};
use graphshell_client::Progress;
use graphshell_client::core::Outcome;
use graphshell_client::driver::{Advance, SessionDriver};
use js_sys::{Array, Function, Promise};
use sceno::InstanceId;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Document, Request, RequestInit, RequestMode, Response};

use super::{BrowserHost, element, root, update_semantics, web_scenario};

/// What the browser can present of a remote scene. `NativeGlyph` is what
/// the live fixture's cards require; the H3 canary offers portable cards.
pub(crate) fn remote_profile() -> CapabilityProfile {
    CapabilityProfile::new([
        PresentationCapability::PortableCard,
        PresentationCapability::Image,
        PresentationCapability::NativeGlyph,
    ])
}

/// How long to wait for ICE gathering before offering what has arrived.
const GATHER_TIMEOUT_MS: i32 = 3_000;

/// Where the remote projection lives.
pub(crate) enum RemoteLink {
    /// In-process, synchronous, mounted into the app's own client.
    Fixture(FixtureEndpoint),
    /// A real host over WebRTC; the driver's core holds the scene.
    WebRtc(Box<WebRtcLink>),
}

/// The WebRTC realization: the driver, the way out, and what is in flight.
pub(crate) struct WebRtcLink {
    pub(crate) driver: SessionDriver,
    outbox: UnboundedSender<String>,
    /// The channel's outbound half, kept so the page can close it.
    writer: BrowserWriter,
    pub(crate) pending: Option<RemoteOp>,
    /// The acknowledged revision before the resume in flight, for the
    /// `diff · a → b` line the receipt reads.
    resume_before: Option<u64>,
    pub(crate) subject: String,
    pub(crate) session_id: String,
    /// What a reconnect presents again: the same signaling server and the
    /// same invitation (the delegation and subject come back from the
    /// retired session).
    signal_url: String,
    /// The invitation as its fragment; `InviteV1` is deliberately not
    /// `Clone`, and a rejoin parses it again.
    invite_fragment: String,
    /// Set by `remote_disconnect`, so the reader reports "disconnected"
    /// rather than a host that went away.
    closing: bool,
    pub(crate) rejoins: u32,
}

/// The operation whose answer is awaited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemoteOp {
    Discover,
    Mount,
    Resnapshot,
    Invoke,
    Poll,
    Resume,
}

impl BrowserHost {
    /// The client holding the remote scene, whichever link is live.
    pub(crate) fn remote_client(&self) -> Option<&ClientState> {
        match &self.remote {
            RemoteLink::Fixture(_) => Some(&self.app.client),
            RemoteLink::WebRtc(link) => link.driver.core().map(|core| core.client()),
        }
    }

    /// The mounted remote scene, if any.
    pub(crate) fn remote_mounted(&self) -> Option<&MountedScene> {
        let session = self.remote_session.as_ref()?;
        self.remote_client()?.mounted(session)
    }

    /// The acknowledged remote revision, if mounted.
    pub(crate) fn remote_revision(&self) -> Option<u64> {
        let session = self.remote_session.as_ref()?;
        self.remote_client()?
            .acknowledgement(session)
            .map(|ack| ack.revision.0)
    }

    /// Whether a remote answer is still to come — what the scenario lane's
    /// `wait` holds on.
    pub(crate) fn remote_in_flight(&self) -> bool {
        self.remote_joining
            || match &self.remote {
                RemoteLink::Fixture(_) => false,
                RemoteLink::WebRtc(link) => {
                    link.pending.is_some()
                        || link.driver.is_awaiting()
                        || link.driver.queued_notices() > 0
                }
            }
    }

    pub(crate) fn remote_link_name(&self) -> &'static str {
        match &self.remote {
            RemoteLink::Fixture(_) => "fixture",
            RemoteLink::WebRtc(_) => "webrtc",
        }
    }

    /// The chrome's label for the remote session.
    pub(crate) fn remote_label(&self) -> String {
        match (&self.remote, self.remote_mounted()) {
            (RemoteLink::Fixture(_), _) => super::REMOTE_LABEL.to_string(),
            (RemoteLink::WebRtc(_), Some(mounted)) => format!(
                "Remote projection · {} objects · revision {}",
                mounted.scene.tables.items.len(),
                mounted.scene.revision.0
            ),
            (RemoteLink::WebRtc(_), None) => format!("Remote projection · {}", self.remote_status),
        }
    }

    /// The selection line and address the chrome shows for the remote session.
    pub(crate) fn remote_selection(&self) -> (String, String) {
        match &self.remote {
            RemoteLink::Fixture(_) => (
                "Projection boundary card".to_string(),
                "fixture.graphshell/note:recent".to_string(),
            ),
            RemoteLink::WebRtc(link) => {
                let label = link
                    .driver
                    .core()
                    .map(|core| core.descriptor().label.clone())
                    .unwrap_or_else(|| "joining".to_string());
                let address = self
                    .remote_session
                    .as_ref()
                    .map(|session| session.0.clone())
                    .unwrap_or_default();
                (label, address)
            }
        }
    }

    /// The actions the remote scene advertises, one per intent in tree
    /// order, each with the first instance that offers it as its target.
    /// Every card on the live board advertises the same two intents; the
    /// action surface offers what the endpoint offers, not one button per
    /// card — targeting a chosen card is the selection's job, later.
    pub(crate) fn remote_actions(&self) -> Vec<(InstanceId, AdvertisedAction)> {
        let Some(session) = self.remote_session.as_ref() else {
            return Vec::new();
        };
        let Some(client) = self.remote_client() else {
            return Vec::new();
        };
        let Ok(tree) = client.accessibility_tree(session, &remote_profile()) else {
            return Vec::new();
        };
        let mut seen = std::collections::BTreeSet::new();
        tree.children
            .iter()
            .flat_map(|item| {
                item.actions
                    .iter()
                    .cloned()
                    .map(move |action| (item.instance, action))
            })
            .filter(|(_, action)| seen.insert(action.intent.0.clone()))
            .collect()
    }

    /// Invoke the `index`th advertised action. A bounded form opens as a
    /// draft for the person to fill; a plain action submits at once.
    pub(crate) fn invoke_remote_action(&mut self, index: usize) {
        let Some((target, action)) = self.remote_actions().into_iter().nth(index) else {
            self.action_status = format!("Failed · no remote action #{index}");
            return;
        };
        let Some((session, ack)) = self.remote_session.as_ref().and_then(|session| {
            self.remote_client()?
                .acknowledgement(session)
                .map(|ack| (session.clone(), ack))
        }) else {
            self.action_status = "Failed · remote projection is not acknowledged".to_string();
            return;
        };
        let bounded = action.input_form.is_some();
        let label = action.label.clone();
        self.action_draft = Some(ActionDraft::new(action));
        self.action_draft_target = Some(ActionDraftTarget {
            session,
            target,
            observed_epoch: ack.epoch,
            observed_revision: ack.revision,
        });
        self.detail_open = true;
        if bounded {
            self.action_status = format!("Choose values · {label}");
            self.chrome_dirty = true;
        } else {
            self.submit_action_draft();
        }
    }

    /// Submit the open draft over the WebRTC link. The answer lands through
    /// `on_remote_line`.
    pub(crate) fn submit_remote_draft(&mut self) {
        let Some(target) = self.action_draft_target.clone() else {
            self.action_status = "Failed · no remote action draft target is open".to_string();
            return;
        };
        let RemoteLink::WebRtc(link) = &mut self.remote else {
            self.action_status = "Failed · not a WebRTC link".to_string();
            return;
        };
        let Some(draft) = self.action_draft.as_mut() else {
            self.action_status = "Failed · no remote action draft is open".to_string();
            return;
        };
        let Some(core) = link.driver.core_mut() else {
            self.action_status = "Failed · remote link is not discovered".to_string();
            return;
        };
        let progress = core.submit_action_draft(&target, draft);
        self.action_count = self.action_count.saturating_add(1);
        self.action_status = format!("Invoking · {}", draft.action().label);
        self.detail_open = true;
        self.begin_remote(RemoteOp::Invoke, progress);
    }

    /// Start an operation: hand the driver the core's progress and carry the
    /// first step — a line out, or an answer already at hand.
    fn begin_remote(&mut self, op: RemoteOp, progress: Result<Progress<Outcome>, String>) {
        let RemoteLink::WebRtc(link) = &mut self.remote else {
            return;
        };
        let advance = progress.and_then(|progress| link.driver.begin(progress));
        link.pending = Some(op);
        self.carry_remote(advance);
    }

    /// One line from the host, folded into the driver.
    pub(crate) fn on_remote_line(&mut self, line: &str) {
        let RemoteLink::WebRtc(link) = &mut self.remote else {
            return;
        };
        let advance = link.driver.on_line(line);
        self.carry_remote(advance);
    }

    fn carry_remote(&mut self, advance: Result<Advance, String>) {
        match advance {
            Ok(Advance::Send(line)) => {
                if let RemoteLink::WebRtc(link) = &mut self.remote {
                    if link.outbox.unbounded_send(line).is_err() {
                        self.remote_fail("the session writer is gone".to_string());
                    }
                }
            }
            Ok(Advance::Done(outcome)) => self.finish_remote(outcome),
            Ok(Advance::Noted) => self.drain_remote_bells(),
            Err(error) => self.remote_fail(error),
        }
    }

    fn remote_fail(&mut self, error: String) {
        if let RemoteLink::WebRtc(link) = &mut self.remote {
            link.pending = None;
        }
        self.remote_status = format!("error: {error}");
        self.action_status = format!("Failed · remote: {error}");
        self.probe_events.push(format!("remote-error {error}"));
        self.chrome_dirty = true;
    }

    fn link_mut(&mut self) -> Option<&mut WebRtcLink> {
        match &mut self.remote {
            RemoteLink::WebRtc(link) => Some(link),
            RemoteLink::Fixture(_) => None,
        }
    }

    /// The host answered: finish the operation in flight, and begin whatever
    /// it implies next (a mount after discovery, a poll after an acceptance,
    /// a resume for every bell).
    fn finish_remote(&mut self, outcome: Outcome) {
        let Some(op) = self.link_mut().map(|link| link.pending.take()) else {
            return;
        };
        self.probe_events.push(format!("remote-done {op:?}"));
        match (op, outcome) {
            (Some(RemoteOp::Discover), Outcome::Descriptor(descriptor)) => {
                self.remote_status = format!("discovered · {}", descriptor.label);
                if self.remote_session.is_some() {
                    // A rediscovery on a link that already mounted — the
                    // reconnect. The mount is kept; whatever moved while the
                    // link was down comes back as bells, which a poll rings.
                    self.remote_status = "open".to_string();
                    let progress = self
                        .link_mut()
                        .and_then(|link| link.driver.core_mut())
                        .map(|core| core.poll())
                        .ok_or_else(|| "not discovered".to_string());
                    self.begin_remote(RemoteOp::Poll, progress);
                } else {
                    let progress = self
                        .link_mut()
                        .and_then(|link| link.driver.core_mut())
                        .ok_or_else(|| "not discovered".to_string())
                        .and_then(|core| core.mount(0));
                    self.begin_remote(RemoteOp::Mount, progress);
                }
            }
            (Some(RemoteOp::Mount), Outcome::Mounted(session)) => {
                self.remote_session = Some(session);
                self.remote_status = "open".to_string();
                self.chrome_dirty = true;
            }
            (Some(RemoteOp::Resnapshot), Outcome::Resnapshotted) => {
                let revision = self.remote_revision().unwrap_or_default();
                self.action_status = format!("{} · revision after {revision}", self.action_status);
                self.chrome_dirty = true;
            }
            (Some(RemoteOp::Invoke), Outcome::Intent(result)) => {
                self.action_draft = None;
                self.action_draft_target = None;
                match *result {
                    IntentResult::Accepted => {
                        self.action_status =
                            format!("Accepted · {} invocation(s)", self.action_count);
                        // The acceptance rang the bell; a round trip lets the
                        // host write it, and the bell drives the resume.
                        let progress = self
                            .link_mut()
                            .and_then(|link| link.driver.core_mut())
                            .map(|core| core.poll())
                            .ok_or_else(|| "not discovered".to_string());
                        self.begin_remote(RemoteOp::Poll, progress);
                    }
                    IntentResult::Rejected { reason } => {
                        self.action_status = format!("Rejected · {reason}");
                        // No bell on a refusal: read the position back by
                        // snapshot, so the claim "unchanged" is measured.
                        let session = self.remote_session.clone();
                        let core = self.link_mut().and_then(|link| link.driver.core_mut());
                        let progress = match (core, session) {
                            (Some(core), Some(session)) => core.resnapshot(&session),
                            _ => Err("not mounted".to_string()),
                        };
                        self.begin_remote(RemoteOp::Resnapshot, progress);
                    }
                    IntentResult::Stale {
                        current_revision, ..
                    } => {
                        self.action_status =
                            format!("Stale · host at {} · reopen the action", current_revision.0);
                    }
                }
                self.chrome_dirty = true;
            }
            // A poll answers with whether the core itself folded anything;
            // the bells the driver queued are drained either way.
            (Some(RemoteOp::Poll), Outcome::Changed(_) | Outcome::Descriptor(_)) => {
                self.drain_remote_bells()
            }
            (Some(RemoteOp::Resume), Outcome::Changed(changed)) => {
                let after = self.remote_revision().unwrap_or_default();
                let before = self
                    .link_mut()
                    .and_then(|link| link.resume_before.take())
                    .map(|revision| revision.to_string())
                    .unwrap_or_else(|| "?".to_string());
                self.remote_last_resume = if changed {
                    format!("diff · {before} → {after}")
                } else {
                    format!("already current at {after}")
                };
                self.chrome_dirty = true;
                self.drain_remote_bells();
            }
            (op, outcome) => {
                self.remote_fail(format!("unexpected answer to {op:?}: {outcome:?}"));
            }
        }
    }

    /// Resume from the next queued bell, if nothing else is in flight.
    fn drain_remote_bells(&mut self) {
        let Some(link) = self.link_mut() else {
            return;
        };
        if link.pending.is_some() {
            return;
        }
        let Some(notice) = link.driver.take_notice() else {
            return;
        };
        link.resume_before = link
            .driver
            .core()
            .and_then(|core| core.client().acknowledgement(&notice.session))
            .map(|ack| ack.revision.0);
        let revision = notice.revision.0;
        let progress = link
            .driver
            .core_mut()
            .ok_or_else(|| "not discovered".to_string())
            .and_then(|core| core.resume_from_notice(notice));
        self.probe_events
            .push(format!("remote-bell revision {revision}"));
        self.begin_remote(RemoteOp::Resume, progress);
    }
}

/// Mirror the remote link into the DOM: tokens on `<body>` and the
/// advertised actions as buttons, so the accessibility tree carries what
/// the endpoint offers and a scenario can press it.
pub(super) fn update_remote_semantics(host: &BrowserHost, document: &Document) -> Result<(), String> {
    let body = root()?;
    let set = |name: &str, value: &str| {
        body.set_attribute(name, value)
            .map_err(|_| format!("could not expose {name}"))
    };
    set("data-remote-link", host.remote_link_name())?;
    set("data-remote-state", &host.remote_status)?;
    set(
        "data-remote-revision",
        &host
            .remote_revision()
            .map(|revision| revision.to_string())
            .unwrap_or_default(),
    )?;
    set("data-remote-resume", &host.remote_last_resume)?;
    set(
        "data-remote-cards",
        &host
            .remote_mounted()
            .map(|mounted| mounted.scene.tables.items.len().to_string())
            .unwrap_or_default(),
    )?;
    if let RemoteLink::WebRtc(link) = &host.remote {
        set("data-remote-subject", &link.subject)?;
        set("data-remote-session", &link.session_id)?;
        set("data-remote-rejoins", &link.rejoins.to_string())?;
    }
    let group = element("remote-actions")?;
    let actions = host.remote_actions();
    let rendered = group
        .get_attribute("data-rendered")
        .unwrap_or_default();
    let signature = actions
        .iter()
        .map(|(_, action)| action.intent.0.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if rendered == signature {
        return Ok(());
    }
    group.set_text_content(None);
    for (index, (_, action)) in actions.iter().enumerate() {
        let button = document
            .create_element("button")
            .map_err(|_| "could not create a remote action button")?;
        button
            .set_attribute("type", "button")
            .and_then(|_| button.set_attribute("data-command", &format!("remote-action-{index}")))
            .and_then(|_| button.set_attribute("data-intent", &action.intent.0))
            .and_then(|_| {
                button.set_attribute(
                    "aria-description",
                    if action.input_form.is_some() {
                        "opens a bounded form"
                    } else {
                        "invokes at once"
                    },
                )
            })
            .map_err(|_| "could not describe a remote action button")?;
        button.set_text_content(Some(&action.label));
        group
            .append_child(&button)
            .map_err(|_| "could not append a remote action button")?;
    }
    group
        .set_attribute("data-rendered", &signature)
        .map_err(|_| "could not mark the remote actions rendered")?;
    Ok(())
}

// ── Connecting ──────────────────────────────────────────────────────────

/// Join a host over WebRTC and make it the remote link. Called by
/// `loader.js` from `?signal=` (and `?invite=`) once the host is ready.
#[wasm_bindgen]
pub fn connect_remote(signal_url: String, invite: Option<String>) -> Result<(), JsValue> {
    let state = web_scenario::host().ok_or_else(|| JsValue::from_str("the host has not booted"))?;
    {
        let mut host = state.borrow_mut();
        host.remote_joining = true;
        host.remote_status = "joining".to_string();
        let _ = update_semantics(&mut host);
    }
    spawn_local(async move {
        if let Err(error) = join(state.clone(), signal_url, invite).await {
            let mut host = state.borrow_mut();
            host.remote_joining = false;
            host.remote_fail(error);
            let _ = update_semantics(&mut host);
        }
    });
    Ok(())
}

async fn join(
    state: Rc<RefCell<BrowserHost>>,
    signal_url: String,
    invite: Option<String>,
) -> Result<(), String> {
    let status = |text: &str| {
        let mut host = state.borrow_mut();
        host.remote_status = text.to_string();
        let _ = update_semantics(&mut host);
    };
    let fragment = match invite {
        Some(fragment) if !fragment.trim().is_empty() => fragment,
        _ => {
            status("fetching the invite");
            http("GET", &format!("{signal_url}/invite"), None)
                .await
                .map_err(|error| format!("GET /invite: {error}"))?
        }
    };
    let invite_fragment = fragment.trim().to_string();
    let invite = InviteV1::parse_fragment(&invite_fragment)
        .map_err(|error| format!("invite fragment: {error}"))?;
    status("building the peer connection");
    let mut browser_join =
        BrowserJoin::new(BrowserInitiatorConfig::default()).map_err(|error| error.to_string())?;
    let answer = offer_and_signal(&mut browser_join, &signal_url, &status).await?;
    status("joining: challenge, redemption, admission");
    let session = browser_join
        .complete(&answer, &invite, &HandshakeLimits::default().clamped())
        .await
        .map_err(|error| format!("join refused: {error}"))?;
    let subject = session.subject_hex();
    let session_id = hex(&session.joined.session_id);
    let writer = session.writer();
    let (outbox, inbox) = unbounded::<String>();

    {
        let mut host = state.borrow_mut();
        // The fixture mount gives way: one remote session at a time.
        if matches!(host.remote, RemoteLink::Fixture(_)) {
            if let Some(old) = host.remote_session.take() {
                host.app.client.forget_session(&old);
            }
        }
        host.remote = RemoteLink::WebRtc(Box::new(WebRtcLink {
            driver: SessionDriver::new(remote_profile()),
            outbox,
            writer: writer.clone(),
            pending: None,
            resume_before: None,
            subject,
            session_id,
            signal_url,
            invite_fragment,
            closing: false,
            rejoins: 0,
        }));
        host.remote_joining = false;
        host.remote_status = "discovering".to_string();
        host.probe_events.push("remote-joined".to_string());
        host.begin_discovery();
        let _ = update_semantics(&mut host);
    }
    spawn_pumps(state, session, writer, inbox);
    Ok(())
}

/// The two tasks a live session needs: outbound lines onto the channel, and
/// every inbound line into the driver.
fn spawn_pumps(
    state: Rc<RefCell<BrowserHost>>,
    mut session: BrowserSession,
    writer: BrowserWriter,
    mut inbox: futures_channel::mpsc::UnboundedReceiver<String>,
) {
    // Outbound: lines the host asked to send, in order.
    spawn_local(async move {
        while let Some(line) = inbox.next().await {
            if let Err(error) = writer.send_line(&line).await {
                web_sys::console::error_1(&format!("remote send: {error}").into());
                break;
            }
        }
    });

    // Inbound: every line the host writes, folded into the driver; the
    // frame after each one mirrors whatever changed.
    spawn_local(async move {
        loop {
            match session.next_line().await {
                Ok(Some(line)) => {
                    let mut host = state.borrow_mut();
                    host.on_remote_line(&line);
                    let _ = update_semantics(&mut host);
                }
                Ok(None) => {
                    let mut host = state.borrow_mut();
                    let closing = host.link_mut().is_some_and(|link| link.closing);
                    host.remote_status = if closing {
                        "disconnected".to_string()
                    } else {
                        "closed: the host closed the channel".to_string()
                    };
                    host.remote_joining = false;
                    host.probe_events.push("remote-channel-closed".to_string());
                    host.chrome_dirty = true;
                    let _ = update_semantics(&mut host);
                    break;
                }
                Err(error) => {
                    let mut host = state.borrow_mut();
                    host.remote_joining = false;
                    host.remote_fail(format!("recv: {error}"));
                    let _ = update_semantics(&mut host);
                    break;
                }
            }
        }
        // Parked, never dropped: the channel's own close event still lands
        // in the initiator's closures (see `BrowserSession::retire`).
        let retired = session.retire();
        RETIRED.with(|parked| parked.borrow_mut().push(retired));
    });
}

impl BrowserHost {
    /// Discover on the live link: the first thing said on any link, and on
    /// a reconnect the thing that keeps the mount (`SessionCore::rediscover`).
    fn begin_discovery(&mut self) {
        let advance = match self.link_mut() {
            Some(link) => {
                link.pending = Some(RemoteOp::Discover);
                link.driver.discover()
            }
            None => return,
        };
        self.carry_remote(advance);
    }

    /// Close the channel from this end. The session retires when the close
    /// lands; the driver, its core and the mount are kept for a reconnect.
    pub(crate) fn remote_disconnect(&mut self) {
        let Some(link) = self.link_mut() else {
            self.action_status = "Failed · no WebRTC link to disconnect".to_string();
            return;
        };
        link.closing = true;
        link.pending = None;
        link.driver.disconnect();
        link.writer.close();
        self.remote_joining = true;
        self.remote_status = "disconnecting".to_string();
        self.probe_events.push("remote-disconnect".to_string());
        self.chrome_dirty = true;
    }

    /// A new link to the same host as the same subject: the retired
    /// session's delegation is presented again, the invitation is not spent
    /// twice, and whatever moved while the link was down comes back by diff.
    pub(crate) fn remote_reconnect(&mut self) {
        let Some(state) = web_scenario::host() else {
            return;
        };
        let Some((signal_url, invite)) = self
            .link_mut()
            .map(|link| (link.signal_url.clone(), link.invite_fragment.clone()))
        else {
            self.action_status = "Failed · no WebRTC link to reconnect".to_string();
            return;
        };
        let Some(retired) = RETIRED.with(|parked| parked.borrow_mut().pop()) else {
            self.action_status = "Failed · no retired session to rejoin as".to_string();
            return;
        };
        // The retired channel stays parked; only the subject and the
        // delegation travel to the new link.
        PARKED.with(|parked| parked.borrow_mut().push(retired.frames));
        self.remote_joining = true;
        self.remote_status = "reconnecting".to_string();
        self.probe_events.push("remote-reconnect".to_string());
        self.chrome_dirty = true;
        spawn_local(async move {
            let result = rejoin(
                state.clone(),
                signal_url,
                invite,
                retired.ephemeral,
                retired.joined.delegation,
            )
            .await;
            if let Err(error) = result {
                let mut host = state.borrow_mut();
                host.remote_joining = false;
                host.remote_fail(error);
                let _ = update_semantics(&mut host);
            }
        });
    }

    /// Ask the signaling server to move the board natively (`POST /nudge`),
    /// as a host would while a peer is away. A receipt hook: the fixture is
    /// the only server that answers it.
    pub(crate) fn remote_nudge(&mut self) {
        let Some(state) = web_scenario::host() else {
            return;
        };
        let Some(signal_url) = self.link_mut().map(|link| link.signal_url.clone()) else {
            self.action_status = "Failed · no WebRTC link to nudge".to_string();
            return;
        };
        self.remote_joining = true;
        self.probe_events.push("remote-nudge".to_string());
        spawn_local(async move {
            let result = http("POST", &format!("{signal_url}/nudge"), Some("")).await;
            let mut host = state.borrow_mut();
            host.remote_joining = false;
            match result {
                Ok(revision) => {
                    host.action_status = format!("Nudged · host at revision {}", revision.trim());
                    host.probe_events
                        .push(format!("remote-nudged revision {}", revision.trim()));
                }
                Err(error) => host.remote_fail(format!("nudge: {error}")),
            }
            host.chrome_dirty = true;
            let _ = update_semantics(&mut host);
        });
    }
}

async fn rejoin(
    state: Rc<RefCell<BrowserHost>>,
    signal_url: String,
    invite_fragment: String,
    ephemeral: InMemoryProvider,
    delegation: SignedDelegationCertificate,
) -> Result<(), String> {
    let status = |text: &str| {
        let mut host = state.borrow_mut();
        host.remote_status = text.to_string();
        let _ = update_semantics(&mut host);
    };
    let invite = InviteV1::parse_fragment(&invite_fragment)
        .map_err(|error| format!("invite fragment: {error}"))?;
    status("building the peer connection");
    let mut browser_join =
        BrowserJoin::new(BrowserInitiatorConfig::default()).map_err(|error| error.to_string())?;
    let answer = offer_and_signal(&mut browser_join, &signal_url, &status).await?;
    status("rejoining: challenge, admission");
    let session = browser_join
        .complete_rejoin(
            &answer,
            &invite,
            ephemeral,
            delegation,
            &HandshakeLimits::default().clamped(),
        )
        .await
        .map_err(|error| format!("rejoin refused: {error}"))?;
    let session_id = hex(&session.joined.session_id);
    let writer = session.writer();
    let (outbox, inbox) = unbounded::<String>();
    {
        let mut host = state.borrow_mut();
        let Some(link) = host.link_mut() else {
            return Err("the link went away during the rejoin".to_string());
        };
        link.outbox = outbox;
        link.writer = writer.clone();
        link.session_id = session_id;
        link.closing = false;
        link.pending = None;
        link.rejoins += 1;
        host.remote_joining = false;
        host.remote_status = "rediscovering".to_string();
        host.probe_events.push("remote-rejoined".to_string());
        host.begin_discovery();
        let _ = update_semantics(&mut host);
    }
    spawn_pumps(state, session, writer, inbox);
    Ok(())
}

thread_local! {
    static RETIRED: RefCell<Vec<RetiredSession>> = const { RefCell::new(Vec::new()) };
    /// Channels of sessions that were rejoined as: their frames, kept alive.
    static PARKED: RefCell<Vec<graphshell::webrtc_browser::BrowserFrames>> =
        const { RefCell::new(Vec::new()) };
}

/// Offer, gather, splice, post, and hand back the answer — the C4a page's
/// signaling, unchanged: one `POST /offer` that C5 replaces with `mer3ly.net`.
async fn offer_and_signal(
    join: &mut BrowserJoin,
    signal_url: &str,
    status: &dyn Fn(&str),
) -> Result<String, String> {
    let candidates: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let (gathered, finish) = resolver();
    {
        let candidates = candidates.clone();
        join.initiator()
            .on_local_ice_candidate(move |candidate| match candidate {
                Some(candidate) => {
                    if candidate_is_usable(&candidate.candidate) {
                        candidates.borrow_mut().push(candidate.candidate);
                    }
                }
                None => {
                    let _ = finish.call0(&JsValue::NULL);
                }
            });
    }
    status("creating the offer");
    let offer = join
        .create_offer()
        .await
        .map_err(|error| format!("create_offer: {error}"))?;
    status("gathering ICE candidates");
    let race = Array::new();
    race.push(&gathered);
    race.push(&JsValue::from(timeout(GATHER_TIMEOUT_MS)?));
    JsFuture::from(Promise::race(&JsValue::from(race)))
        .await
        .map_err(|error| describe(&error))?;
    if candidates.borrow().is_empty() {
        return Err("offer has no usable ICE candidates (mDNS .local names only) — \
                    disable chrome://flags/#enable-webrtc-hide-local-ips-with-mdns"
            .to_string());
    }
    let full_offer = offer_with_candidates(&offer, &candidates.borrow());
    status("posting the offer");
    http("POST", &format!("{signal_url}/offer"), Some(&full_offer))
        .await
        .map_err(|error| format!("POST /offer: {error}"))
}

/// Drop what the host's ICE agent cannot pair with: mDNS `.local` names
/// and non-UDP.
fn candidate_is_usable(candidate: &str) -> bool {
    let fields: Vec<&str> = candidate.split_whitespace().collect();
    if fields.len() < 6 || !fields[2].eq_ignore_ascii_case("udp") {
        return false;
    }
    let address = fields[4];
    !address.to_ascii_lowercase().ends_with(".local") && address.parse::<std::net::IpAddr>().is_ok()
}

fn offer_with_candidates(offer: &str, candidates: &[String]) -> String {
    let mut sdp = offer.to_owned();
    if !sdp.ends_with('\n') {
        sdp.push_str("\r\n");
    }
    for candidate in candidates {
        sdp.push_str("a=");
        sdp.push_str(candidate);
        sdp.push_str("\r\n");
    }
    sdp
}

fn resolver() -> (Promise, Function) {
    let mut slot: Option<Function> = None;
    let promise = Promise::new(&mut |resolve, _reject| slot = Some(resolve));
    (promise, slot.expect("a Promise executor runs synchronously"))
}

fn timeout(ms: i32) -> Result<Promise, String> {
    let window = web_sys::window().ok_or("no window")?;
    Ok(Promise::new(&mut |resolve, _reject| {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    }))
}

async fn http(method: &str, url: &str, body: Option<&str>) -> Result<String, String> {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_mode(RequestMode::Cors);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(body));
    }
    let request = Request::new_with_str_and_init(url, &init).map_err(|error| describe(&error))?;
    if body.is_some() {
        // A CORS *simple* request: no preflight for the fixture to answer.
        request
            .headers()
            .set("content-type", "text/plain")
            .map_err(|error| describe(&error))?;
    }
    let window = web_sys::window().ok_or("no window")?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|error| describe(&error))?
        .dyn_into()
        .map_err(|_| "fetch returned no Response".to_string())?;
    let text = JsFuture::from(response.text().map_err(|error| describe(&error))?)
        .await
        .map_err(|error| describe(&error))?
        .as_string()
        .unwrap_or_default();
    if !response.ok() {
        return Err(format!(
            "{} {}: {text}",
            response.status(),
            response.status_text()
        ));
    }
    Ok(text)
}

fn describe(value: &JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            value
                .dyn_ref::<js_sys::Error>()
                .map(|error| String::from(error.message()))
        })
        .unwrap_or_else(|| format!("{value:?}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
