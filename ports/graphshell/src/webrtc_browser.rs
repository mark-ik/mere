// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The browser's glue: a `BrowserInitiator` as the join's frame channel, and
//! the NDJSON session over its frames.
//!
//! Everything with a rule in it lives elsewhere and runs on an ordinary
//! `cargo test`: the join sequence in [`crate::webrtc_join`], the session
//! protocol in `graphshell_client::SessionDriver`, and the byte-to-line
//! reassembly in [`LineAssembler`]. This
//! module is what is left once those are taken out — a few dozen lines that
//! know what a data channel is, and nothing else does.
//!
//! ## Frames in, lines out
//!
//! The host presents its carrier as a byte stream and writes NDJSON to it, so
//! a line may span frames and a frame may hold several lines. The browser has
//! no pump: each inbound frame's bytes go through the assembler, and each
//! complete line reaches the driver. Outbound, a request line is cut at the
//! carrier's frame ceiling, which the host's pump reassembles the same way.
//!
//! ## Two steps, because signaling is the page's
//!
//! [`BrowserJoin::create_offer`] produces the SDP; the page carries it wherever the
//! host is and brings back the answer; [`BrowserJoin::complete`] does the
//! rest. Nothing here fetches. The one HTTP request a receipt page makes is
//! the page's own, and C5 replaces it with `mer3ly.net` without this module
//! changing.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

use personae::InMemoryProvider;
use webrtc_carrier::{
    BrowserInitiator, DtlsFingerprint, FingerprintRole, MAX_FRAME_PAYLOAD_BYTES,
};

// What a page needs to name to drive a join, re-exported so the page
// depends on this module alone.
pub use notochord::HandshakeLimits;
pub use webrtc_carrier::{BrowserInitiatorConfig, InviteV1};

use crate::admission::PROJECTION_PROTOCOL;
use crate::webrtc_join::{
    JoinError, JoinFrames, LineAssembler, PeerJoin, fill_random, peer_join, peer_rejoin,
};

/// Frames arriving from the channel, queued until something asks.
///
/// `on_frame` runs with the initiator's own state already mutably borrowed
/// (the C1 probe learned that the hard way), so the callback touches only this
/// — a queue it owns and a waker it stores — and never the initiator.
#[derive(Default)]
struct Inbox {
    frames: VecDeque<Vec<u8>>,
    waker: Option<Waker>,
    closed: Option<String>,
}

/// A data channel presented as the join's frame channel.
pub struct BrowserFrames {
    initiator: Rc<BrowserInitiator>,
    inbox: Rc<RefCell<Inbox>>,
}

impl BrowserFrames {
    fn new(initiator: Rc<BrowserInitiator>) -> Self {
        let inbox = Rc::new(RefCell::new(Inbox::default()));
        {
            let inbox = inbox.clone();
            initiator.on_frame(move |payload| {
                let mut guard = inbox.borrow_mut();
                guard.frames.push_back(payload);
                if let Some(waker) = guard.waker.take() {
                    waker.wake();
                }
            });
        }
        {
            let inbox = inbox.clone();
            initiator.on_closed(move |error| {
                let mut guard = inbox.borrow_mut();
                guard.closed = Some(error.to_string());
                if let Some(waker) = guard.waker.take() {
                    waker.wake();
                }
            });
        }
        Self { initiator, inbox }
    }

    /// The initiator, for `buffered_amount` and friends.
    pub fn initiator(&self) -> &BrowserInitiator {
        &self.initiator
    }
}

impl JoinFrames for BrowserFrames {
    async fn recv(&mut self) -> Result<Option<Vec<u8>>, String> {
        let inbox = self.inbox.clone();
        poll_fn(move |cx| {
            let mut guard = inbox.borrow_mut();
            if let Some(frame) = guard.frames.pop_front() {
                return Poll::Ready(Ok(Some(frame)));
            }
            if guard.closed.is_some() {
                return Poll::Ready(Ok(None));
            }
            guard.waker = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }

    async fn send(&mut self, payload: &[u8]) -> Result<(), String> {
        self.initiator
            .send_frame(payload)
            .await
            .map_err(|error| error.to_string())
    }
}

/// A join in progress: offered, waiting for the page to bring back an answer.
pub struct BrowserJoin {
    initiator: Rc<BrowserInitiator>,
    channel_label: String,
    /// Known once the offer exists; `complete` refuses to run without it.
    own_fingerprint: Option<DtlsFingerprint>,
}

/// What survives a session: the subject and delegation a reconnect presents
/// again, and the channel, which must be kept alive rather than dropped.
pub struct RetiredSession {
    /// The subject, for [`BrowserJoin::complete_rejoin`].
    pub ephemeral: InMemoryProvider,
    /// The join it concluded; `joined.delegation` is what a rejoin presents.
    pub joined: PeerJoin,
    /// The closed channel. Park it for the life of the page.
    pub frames: BrowserFrames,
}

/// A joined session: the frame channel and what the join concluded.
pub struct BrowserSession {
    /// The channel, now carrying the NDJSON session.
    pub frames: BrowserFrames,
    /// The subject this session runs as. Kept so a reconnect can present the
    /// same key with the retained delegation.
    pub ephemeral: InMemoryProvider,
    /// What the host admitted.
    pub joined: PeerJoin,
    assembler: LineAssembler,
}

impl BrowserJoin {
    /// Build the peer connection.
    ///
    /// `config.protocol` is set to the projection protocol here whatever the
    /// caller passed: the data channel's subprotocol is the string the link
    /// challenge binds, and a caller choosing another would produce a
    /// transcript the host cannot reproduce.
    pub fn new(mut config: BrowserInitiatorConfig) -> Result<Self, JoinError> {
        config.protocol = String::from_utf8_lossy(PROJECTION_PROTOCOL).into_owned();
        let channel_label = config.channel_label.clone();
        let initiator = Rc::new(
            BrowserInitiator::new(config)
                .map_err(|error| JoinError::Channel(format!("initiator: {error}")))?,
        );
        Ok(Self {
            initiator,
            channel_label,
            own_fingerprint: None,
        })
    }

    /// Create the offer, remembering this end's fingerprint from it.
    ///
    /// Register a candidate collector on [`initiator`](Self::initiator)
    /// *before* this: gathering starts at `setLocalDescription`.
    pub async fn create_offer(&mut self) -> Result<String, JoinError> {
        let sdp = self
            .initiator
            .create_offer()
            .await
            .map_err(|error| JoinError::Channel(format!("create_offer: {error}")))?;
        self.own_fingerprint = Some(fingerprint_in(&sdp, FingerprintRole::Client)?);
        Ok(sdp)
    }

    /// The initiator, so the page can register its candidate collector before
    /// offering — gathering starts at `setLocalDescription`, and a collector
    /// registered after it races the end-of-candidates signal.
    pub fn initiator(&self) -> &Rc<BrowserInitiator> {
        &self.initiator
    }

    /// Apply the host's answer, open the channel, and run the join.
    ///
    /// The host's fingerprint is read from the answer SDP. That is the same
    /// certificate the host's carrier binds from its handshake — the carrier's
    /// loopback suite asserts negotiated equals declared — so both ends derive
    /// the same transcript from what each can see.
    pub async fn complete(
        self,
        answer_sdp: &str,
        invite: &InviteV1,
        limits: &HandshakeLimits,
    ) -> Result<BrowserSession, JoinError> {
        let own_fingerprint = self.own_fingerprint.ok_or_else(|| {
            JoinError::Malformed("complete() before create_offer(): no local fingerprint".into())
        })?;
        let host_fingerprint = fingerprint_in(answer_sdp, FingerprintRole::Server)?;
        self.initiator
            .set_remote_answer(answer_sdp)
            .await
            .map_err(|error| JoinError::Channel(format!("set_remote_answer: {error}")))?;
        self.initiator
            .wait_until_open()
            .await
            .map_err(|error| JoinError::Channel(format!("channel never opened: {error}")))?;

        // The ephemeral subject: fresh per join, never persisted, its private
        // half never leaving this process.
        let mut seed = [0u8; 32];
        fill_random(&mut seed)?;
        let ephemeral = InMemoryProvider::from_seed(seed);

        let mut frames = BrowserFrames::new(self.initiator);
        let joined = peer_join(
            &mut frames,
            &ephemeral,
            invite,
            &self.channel_label,
            own_fingerprint,
            host_fingerprint,
            limits,
        )
        .await?;

        Ok(BrowserSession {
            frames,
            ephemeral,
            joined,
            assembler: LineAssembler::default(),
        })
    }
}

impl BrowserJoin {
    /// Apply the host's answer and rejoin as a subject the host already knows.
    ///
    /// The reconnect path. A fresh DTLS link, the host verified again over it,
    /// and admission run again — but the delegation is the one the first join
    /// was granted, presented by the same ephemeral subject, so the invitation
    /// is not spent twice. Everything [`complete`](Self::complete) does except
    /// the redemption.
    pub async fn complete_rejoin(
        self,
        answer_sdp: &str,
        invite: &InviteV1,
        ephemeral: InMemoryProvider,
        delegation: personae::delegation::SignedDelegationCertificate,
        limits: &HandshakeLimits,
    ) -> Result<BrowserSession, JoinError> {
        let own_fingerprint = self.own_fingerprint.ok_or_else(|| {
            JoinError::Malformed("complete_rejoin() before create_offer(): no local fingerprint".into())
        })?;
        let host_fingerprint = fingerprint_in(answer_sdp, FingerprintRole::Server)?;
        self.initiator
            .set_remote_answer(answer_sdp)
            .await
            .map_err(|error| JoinError::Channel(format!("set_remote_answer: {error}")))?;
        self.initiator
            .wait_until_open()
            .await
            .map_err(|error| JoinError::Channel(format!("channel never opened: {error}")))?;

        let mut frames = BrowserFrames::new(self.initiator);
        let joined = peer_rejoin(
            &mut frames,
            &ephemeral,
            invite,
            delegation,
            &self.channel_label,
            own_fingerprint,
            host_fingerprint,
            limits,
        )
        .await?;

        Ok(BrowserSession {
            frames,
            ephemeral,
            joined,
            assembler: LineAssembler::default(),
        })
    }
}

impl BrowserSession {
    /// End this session politely and hand back its channel to be *parked*,
    /// never dropped.
    ///
    /// The initiator registers closures on the JS peer connection and data
    /// channel. Dropping it frees those closures, but the browser still fires
    /// the channel close event that the close itself provokes — into a
    /// closure that no longer exists, which wasm-bindgen reports as "closure
    /// invoked recursively or after being dropped". The C1 probe learned this
    /// and wrote it down; the first headed reconnect here forgot it and threw
    /// that exception on every reconnect. So: close, then keep the frames
    /// alive for the life of the page. A few hundred bytes per retired
    /// session, in exchange for a console clean enough to screenshot.
    pub fn retire(self) -> RetiredSession {
        self.frames.initiator().close();
        RetiredSession {
            ephemeral: self.ephemeral,
            joined: self.joined,
            frames: self.frames,
        }
    }

    /// Send one NDJSON line to the host, cut at the carrier's frame ceiling.
    pub async fn send_line(&mut self, line: &str) -> Result<(), String> {
        self.writer().send_line(line).await
    }

    /// The subject this session runs as, hex-encoded, for a receipt.
    pub fn subject_hex(&self) -> String {
        use personae::IdentityProvider;
        self.ephemeral
            .master_public_key()
            .to_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// The outbound half on its own, so one task can write while another
    /// sits in [`next_line`](Self::next_line). Sending needs only the
    /// initiator, which is shared; receiving owns the inbox.
    pub fn writer(&self) -> BrowserWriter {
        BrowserWriter {
            initiator: self.frames.initiator.clone(),
        }
    }

    /// The next complete line from the host, or `None` when the channel is
    /// gone.
    ///
    /// Frames are consumed as needed: several lines may arrive in one frame
    /// and are handed out one per call, and a line spanning frames is held
    /// until its end arrives.
    pub async fn next_line(&mut self) -> Result<Option<String>, String> {
        loop {
            if let Some(line) = self.assembler.next_line() {
                return Ok(Some(line));
            }
            match self.frames.recv().await? {
                Some(frame) => self.assembler.push(&frame),
                None => return Ok(self.assembler.take_partial()),
            }
        }
    }
}

/// The outbound half of a session; see [`BrowserSession::writer`].
pub struct BrowserWriter {
    initiator: Rc<BrowserInitiator>,
}

impl BrowserWriter {
    /// Send one NDJSON line to the host, cut at the carrier's frame ceiling.
    pub async fn send_line(&self, line: &str) -> Result<(), String> {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        for chunk in bytes.chunks(MAX_FRAME_PAYLOAD_BYTES) {
            self.initiator
                .send_frame(chunk)
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// The `a=fingerprint:` attribute of an SDP, as this crate's fingerprint type.
fn fingerprint_in(sdp: &str, role: FingerprintRole) -> Result<DtlsFingerprint, JoinError> {
    let value = sdp
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("a=fingerprint:"))
        .ok_or_else(|| JoinError::Malformed("the SDP carries no a=fingerprint line".into()))?;
    DtlsFingerprint::parse_sdp_attribute(role, value)
        .map_err(|error| JoinError::Malformed(format!("SDP fingerprint: {error}")))
}
