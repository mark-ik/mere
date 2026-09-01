// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The joining side of the WebRTC door, and the wire both sides share.
//!
//! `webrtc_session` used to hold both roles. It still holds the host
//! — which needs a real carrier, a tokio pump and the resident host — but the
//! *peer* half moved here so a browser can run it. This module has no runtime
//! beneath it: no tokio, no carrier crate beyond its Wasm-clean core, no
//! transport. What it needs from its caller is exactly one thing, a
//! [`JoinFrames`] that moves bytes, and the browser's data channel is as good
//! a one as a native carrier.
//!
//! The wire messages live here too, because a message type the host encodes
//! and the peer decodes is neither side's property. The host half imports them
//! from this module rather than the other way round, so the dependency points
//! from the runtime-bound half to the runtime-free one, never back.
//!
//! ## Entropy without `rand`
//!
//! Nonces come from [`getrandom`] 0.3 directly rather than through `rand_core`.
//! Not a preference: the identity stack already carries two `getrandom` majors
//! into a wasm build, each needing its own opt-in to reach the JavaScript
//! backend, and `rand_core 0.6`'s `getrandom` feature would have added a third.
//! One more major is one more way for the browser build to fail at link time
//! with a message about a crate nobody named.

use notochord::{DenyReason, HandshakeError, HandshakeLimits, NetworkId, ProfileRef, SessionReply};
use personae::IdentityProvider;
use personae::delegation::SignedDelegationCertificate;
use serde::{Deserialize, Serialize};
use webrtc_carrier::{DtlsFingerprint, InviteV1, LinkChallenge};

use crate::admission::PROJECTION_PROTOCOL;
use crate::webrtc_door::{
    DoorError, HostChallengeSignature, RedemptionRefusal, build_redemption_proof,
    verify_host_challenge,
};

// `async fn` in a public trait, on purpose: the join runs where it is called,
// never inside a `tokio::spawn`, so nobody needs a `Send` bound on these
// futures — and the browser adapter that eventually joins from wasm is
// single-threaded by construction, which is the whole reason this lane exists.
#[allow(async_fn_in_trait)]
pub trait JoinFrames {
    /// The next frame, or `None` when the channel closed.
    async fn recv(&mut self) -> Result<Option<Vec<u8>>, String>;
    /// Send one frame.
    async fn send(&mut self, payload: &[u8]) -> Result<(), String>;
}


/// What the joining end sends, one JSON value per frame.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum ToHost {
    /// Names the invitation and contributes the client half of the transcript.
    Open {
        invite: [u8; 16],
        client_nonce: [u8; 32],
    },
    /// Spends one use: the ephemeral subject and the seed's signature over
    /// this transcript and that subject.
    Redeem {
        subject: [u8; 32],
        /// 64 bytes; a `Vec` because serde arrays stop at 32.
        proof: Vec<u8>,
    },
    /// A reconnecting peer that already holds its delegation.
    ///
    /// C3's ruling made reconnect a new DTLS link with fresh admission, and
    /// C2's that the invitation stays one-use: the delegation minted at the
    /// first join travels inside the new hello, so there is nothing to redeem
    /// and no use to spend. The host skips straight to admission — which is
    /// where a revoked or expired delegation is refused, exactly as it would
    /// be mid-session.
    Resume {},
}

/// What the host sends back, one JSON value per frame.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum ToPeer {
    /// The host's half of the transcript, and its signature over the whole.
    Challenge {
        server_nonce: [u8; 32],
        signature: HostChallengeSignature,
    },
    /// The redeemed delegation. The next two frames are binary: hello, reply.
    Grant {
        delegation: SignedDelegationCertificate,
    },
    /// The join stops here. Written before the host closes, so a refused peer
    /// learns why rather than watching a channel die.
    Refused { reason: String },
}

/// Why a join did not produce an admitted session.
#[derive(Debug, thiserror::Error)]
pub enum JoinError {
    /// The channel failed or closed mid-join.
    #[error("join channel: {0}")]
    Channel(String),
    /// The peer sent something other than the message the sequence expects.
    #[error("malformed join message: {0}")]
    Malformed(String),
    /// The peer named an invitation this host is not offering.
    #[error("the join names an unknown invitation")]
    UnknownInvite,
    /// The transcript could not be assembled from this connection's facts.
    #[error("link challenge: {0}")]
    Challenge(#[from] webrtc_carrier::ChallengeError),
    /// A host-side door operation failed.
    #[error(transparent)]
    Door(#[from] DoorError),
    /// The redemption was refused. The refusal was written to the peer.
    #[error("redemption refused: {0}")]
    Redemption(#[from] RedemptionRefusal),
    /// Notochord refused the hello. The reply carrying the refusal was
    /// written to the peer.
    #[error("admission denied: {0:?}")]
    Denied(DenyReason),
    /// The host's challenge signature did not verify against the key the
    /// invitation named. Peer side only, and terminal: an unverified channel
    /// gets no redemption proof.
    #[error("the host did not prove the invitation's key over this link")]
    HostUnverified,
    /// A Notochord frame could not be built or read.
    #[error("handshake: {0}")]
    Handshake(#[from] HandshakeError),
    /// The platform would not supply randomness for a nonce.
    ///
    /// Terminal rather than retried: a nonce drawn from anything less than the
    /// platform's entropy source is a transcript an attacker can predict.
    #[error("no randomness for a join nonce: {0}")]
    Entropy(String),
}

/// What the joining end holds after a completed join.
#[derive(Debug)]
pub struct PeerJoin {
    /// The transcript-derived id the host admitted. Matches the host's
    /// `JoinConclusion::principal` on the host side.
    pub session_id: [u8; 32],
    /// The link this end derived. Equal to the host's, or admission could not
    /// have accepted.
    pub shared_link: [u8; 16],
    /// The delegation this session runs under. Retained because a reconnect
    /// presents it again through [`peer_rejoin`] rather than redeeming anew —
    /// the invitation stays spent, the delegation keeps working until its own
    /// expiry or revocation says otherwise.
    pub delegation: SignedDelegationCertificate,
}

/// Run the joining side of the sequence over one connected channel.
///
/// `invite` is this end's own copy, parsed from the fragment. The ephemeral
/// provider is the freshly generated subject; its master key is what the
/// redemption binds and the delegation names.
#[allow(clippy::too_many_arguments)]
pub async fn peer_join<P: IdentityProvider, F: JoinFrames>(
    frames: &mut F,
    ephemeral: &P,
    invite: &InviteV1,
    channel_label: &str,
    client_fingerprint: DtlsFingerprint,
    server_fingerprint: DtlsFingerprint,
    limits: &HandshakeLimits,
) -> Result<PeerJoin, JoinError> {
    let challenge = open_and_verify(
        frames,
        invite,
        channel_label,
        client_fingerprint,
        server_fingerprint,
    )
    .await?;

    // The channel is now the invitation's host. Spend one use on it.
    let subject = ephemeral.master_public_key().to_bytes();
    let proof = build_redemption_proof(invite.redemption_seed(), &challenge, &subject);
    send_json(
        frames,
        &ToHost::Redeem {
            subject,
            proof: proof.to_vec(),
        },
    )
    .await?;

    let delegation = match recv_json::<ToPeer, _>(frames).await? {
        ToPeer::Grant { delegation } => delegation,
        ToPeer::Refused { reason } => return Err(JoinError::Channel(reason)),
        other => return Err(unexpected_message("Grant", &other)),
    };

    hello_and_reply(frames, ephemeral, invite, &challenge, delegation, limits).await
}

/// Reconnect: the sequence a peer that already holds its delegation runs on a
/// fresh DTLS link.
///
/// The host is verified again — a new link is a new channel someone else could
/// be terminating — and admission runs again, which is where a delegation
/// revoked since the first join is refused. What does *not* happen is a
/// redemption: the invitation's use count belongs to first joins alone.
#[allow(clippy::too_many_arguments)]
pub async fn peer_rejoin<P: IdentityProvider, F: JoinFrames>(
    frames: &mut F,
    ephemeral: &P,
    invite: &InviteV1,
    delegation: SignedDelegationCertificate,
    channel_label: &str,
    client_fingerprint: DtlsFingerprint,
    server_fingerprint: DtlsFingerprint,
    limits: &HandshakeLimits,
) -> Result<PeerJoin, JoinError> {
    let challenge = open_and_verify(
        frames,
        invite,
        channel_label,
        client_fingerprint,
        server_fingerprint,
    )
    .await?;
    send_json(frames, &ToHost::Resume {}).await?;
    hello_and_reply(frames, ephemeral, invite, &challenge, delegation, limits).await
}

/// The prefix both joins share: name the invitation, contribute the client
/// nonce, and refuse the channel unless the host proves the invitation's key
/// over this exact link.
async fn open_and_verify<F: JoinFrames>(
    frames: &mut F,
    invite: &InviteV1,
    channel_label: &str,
    client_fingerprint: DtlsFingerprint,
    server_fingerprint: DtlsFingerprint,
) -> Result<LinkChallenge, JoinError> {
    let mut client_nonce = [0u8; 32];
    fill_random(&mut client_nonce)?;
    send_json(
        frames,
        &ToHost::Open {
            invite: invite.rendezvous().to_bytes(),
            client_nonce,
        },
    )
    .await?;

    // The host's challenge signature, checked against the key the invitation
    // named before anything secret crosses this channel.
    let (server_nonce, signature) = match recv_json::<ToPeer, _>(frames).await? {
        ToPeer::Challenge {
            server_nonce,
            signature,
        } => (server_nonce, signature),
        ToPeer::Refused { reason } => return Err(JoinError::Channel(reason)),
        other => return Err(unexpected_message("Challenge", &other)),
    };
    let challenge = LinkChallenge::new(
        PROJECTION_PROTOCOL,
        channel_label,
        invite.rendezvous(),
        client_nonce,
        server_nonce,
        client_fingerprint,
        server_fingerprint,
    )?;
    if !verify_host_challenge(invite.expected_host_key(), &challenge, &signature) {
        return Err(JoinError::HostUnverified);
    }
    Ok(challenge)
}

/// The suffix both joins share: the ordinary hello bound to this link, and
/// the host's reply.
async fn hello_and_reply<P: IdentityProvider, F: JoinFrames>(
    frames: &mut F,
    ephemeral: &P,
    invite: &InviteV1,
    challenge: &LinkChallenge,
    delegation: SignedDelegationCertificate,
    limits: &HandshakeLimits,
) -> Result<PeerJoin, JoinError> {
    let shared_link = challenge.shared_link();
    let profile = ProfileRef {
        id: invite.profile_id().to_string(),
        revision: u32::try_from(invite.profile_revision())
            .map_err(|_| JoinError::Malformed("profile revision exceeds u32".into()))?,
    };
    let mut hello_nonce = [0u8; 32];
    fill_random(&mut hello_nonce)?;
    let hello = crate::webrtc_door::open_webrtc_session(
        ephemeral,
        NetworkId(*invite.network()),
        profile,
        hello_nonce,
        shared_link,
        vec![delegation.clone()],
    )?;
    frames
        .send(&hello.encode(limits)?)
        .await
        .map_err(JoinError::Channel)?;

    let reply = recv_binary(frames, "a Notochord reply").await?;
    match SessionReply::decode(&reply, limits)? {
        SessionReply::Accept { session_id, .. } => Ok(PeerJoin {
            session_id,
            shared_link,
            delegation,
        }),
        SessionReply::Reject { reason } => Err(JoinError::Denied(reason)),
    }
}

// ── Frame helpers ───────────────────────────────────────────────────────────

pub(crate) async fn recv_json<T: for<'de> Deserialize<'de>, F: JoinFrames>(
    frames: &mut F,
) -> Result<T, JoinError> {
    let payload = recv_binary(frames, "a join message").await?;
    serde_json::from_slice(&payload)
        .map_err(|error| JoinError::Malformed(format!("undecodable join message: {error}")))
}

pub(crate) async fn recv_binary<F: JoinFrames>(frames: &mut F, expected: &str) -> Result<Vec<u8>, JoinError> {
    match frames.recv().await.map_err(JoinError::Channel)? {
        Some(payload) => Ok(payload),
        None => Err(JoinError::Channel(format!(
            "the channel closed while waiting for {expected}"
        ))),
    }
}

pub(crate) async fn send_json<T: Serialize, F: JoinFrames>(frames: &mut F, message: &T) -> Result<(), JoinError> {
    let payload = serde_json::to_vec(message)
        .map_err(|error| JoinError::Malformed(format!("unencodable join message: {error}")))?;
    frames.send(&payload).await.map_err(JoinError::Channel)
}

pub(crate) fn unexpected_message(expected: &str, actual: &impl std::fmt::Debug) -> JoinError {
    JoinError::Malformed(format!("expected {expected}, received {actual:?}"))
}


/// Bytes in, complete lines out, with the partial tail carried across calls.
///
/// The host writes NDJSON to a byte stream and its pump cuts that stream at
/// frame boundaries with no regard for lines; this is the browser's side of
/// that bargain. Pure, so its cases are ordinary tests: a line split across
/// two pushes, two lines in one push, an empty line, and bytes with no newline
/// yet.
#[derive(Debug, Default)]
pub struct LineAssembler {
    buffer: Vec<u8>,
}

impl LineAssembler {
    /// Append bytes as they arrived.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next complete line, without its newline, if one is buffered.
    ///
    /// A trailing carriage return is dropped too, so a host writing CRLF is
    /// read the same as one writing LF.
    pub fn next_line(&mut self) -> Option<String> {
        let end = self.buffer.iter().position(|byte| *byte == b'\n')?;
        let mut line: Vec<u8> = self.buffer.drain(..=end).collect();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        Some(String::from_utf8_lossy(&line).into_owned())
    }

    /// Whatever is buffered without a terminating newline, taken.
    ///
    /// For end-of-stream: a host that closed after a final line with no
    /// newline still said something, and this is how it is heard.
    pub fn take_partial(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        let tail = std::mem::take(&mut self.buffer);
        Some(String::from_utf8_lossy(&tail).into_owned())
    }
}

/// Fill `buf` from the platform's entropy source.
pub(crate) fn fill_random(buf: &mut [u8]) -> Result<(), JoinError> {
    getrandom::fill(buf).map_err(|error| JoinError::Entropy(error.to_string()))
}

#[cfg(test)]
mod assembler_tests {
    use super::LineAssembler;

    #[test]
    fn a_line_split_across_two_pushes_arrives_once() {
        let mut lines = LineAssembler::default();
        lines.push(b"{\"id\":1,\"bo");
        assert_eq!(lines.next_line(), None, "half a line is not a line");
        lines.push(b"dy\":null}\n");
        assert_eq!(lines.next_line().as_deref(), Some("{\"id\":1,\"body\":null}"));
        assert_eq!(lines.next_line(), None);
    }

    #[test]
    fn two_lines_in_one_push_come_out_as_two() {
        let mut lines = LineAssembler::default();
        lines.push(b"a\nb\n");
        assert_eq!(lines.next_line().as_deref(), Some("a"));
        assert_eq!(lines.next_line().as_deref(), Some("b"));
        assert_eq!(lines.next_line(), None);
    }

    #[test]
    fn crlf_reads_the_same_as_lf() {
        let mut lines = LineAssembler::default();
        lines.push(b"a\r\nb\n");
        assert_eq!(lines.next_line().as_deref(), Some("a"));
        assert_eq!(lines.next_line().as_deref(), Some("b"));
    }

    #[test]
    fn an_empty_line_is_a_line() {
        let mut lines = LineAssembler::default();
        lines.push(b"\n");
        assert_eq!(lines.next_line().as_deref(), Some(""));
    }

    #[test]
    fn the_unterminated_tail_is_taken_only_on_request() {
        let mut lines = LineAssembler::default();
        lines.push(b"unfinished");
        assert_eq!(lines.next_line(), None);
        assert_eq!(lines.take_partial().as_deref(), Some("unfinished"));
        assert_eq!(lines.take_partial(), None, "taken once");
    }
}
