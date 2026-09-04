// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The live half of the WebRTC door: the join sequence over a real channel,
//! and the admitted session it produces.
//!
//! [`crate::webrtc_door`] is deliberately sans-I/O — bytes in, bytes out,
//! every rule testable without a socket. This module is where those bytes
//! actually travel. It stays as thin as that division demands: the sequence
//! below contains no signature check, no use-count, no policy rule. Every one
//! of those lives in the door and is exercised by the door's fail-closed
//! matrix; what is tested here is only that the messages arrive in order and
//! that each refusal is *written to the peer* before the channel closes.
//!
//! ## Frames for the join, a stream for the application
//!
//! The join travels as carrier frames — four JSON messages and two binary
//! ones — because frames give the door's bytes-in/bytes-out functions their
//! message boundaries for free. Only after admission does
//! [`stream_over_frames`] start,
//! and the byte stream it presents carries the ordinary NDJSON session that
//! [`crate::session_loop::serve_admitted_session`] already speaks. The pump
//! starting *after* the admission reply is what keeps the plan's rule — a
//! refused stream never reaches Graphshell — structural rather than
//! disciplinary: before that point there is no stream for an application
//! byte to be on.
//!
//! ## The sequence
//!
//! ```text
//! browser                                   host
//!   |-- Open { invite, client_nonce } ------->|
//!   |<------ Challenge { server_nonce, sig } -|   host signs the transcript
//!   |   (browser verifies against the         |
//!   |    invitation's expected host key)      |
//!   |-- Redeem { subject, proof } ----------->|   one use spent, or Refused
//!   |<------------- Grant { delegation } -----|
//!   |-- hello bytes (binary frame) ---------->|   Notochord admission
//!   |<---------- reply bytes (binary frame) --|   accept, or deny and close
//!   |            ... NDJSON session ...       |
//! ```
//!
//! Position defines interpretation: the frame after `Grant` is hello bytes,
//! the frame after that is reply bytes, and neither is JSON. A peer that
//! sends anything else where a message is expected is refused as malformed.
//!
//! ## Where the other role went
//!
//! The peer half — [`peer_join`], [`peer_rejoin`], the wire messages, and
//! [`JoinFrames`] itself — lives in [`crate::webrtc_join`], with no runtime
//! beneath it, so the browser can run it. It is re-exported here, and the
//! tests below still drive both ends of the real sequence against each other,
//! which is what keeps the two from drifting.

use notochord::{
    AdmittedPrincipal, AdmittedSession, LocalNetworkPolicy, RevocationLedger, SessionHello,
};
use personae::IdentityProvider;
use rand_core::{OsRng, RngCore};
use tokio::io::DuplexStream;
use tokio::task::JoinHandle;
use webrtc_carrier::native::{Carrier, CarrierControl, PumpEnd, stream_over_frames};
use webrtc_carrier::{DtlsFingerprint, InviteId, InviteV1, LinkChallenge};

use crate::admission::PROJECTION_PROTOCOL;
use crate::webrtc_door::{
    RedemptionState, admit_webrtc_session, mint_delegation, redeem, sign_challenge,
    webrtc_session_facts,
};
use crate::webrtc_join::{ToHost, ToPeer, recv_binary, recv_json, send_json, unexpected_message};

/// The peer half and the shared vocabulary, re-exported from where they now
/// live so a caller that learned this module's name keeps working.
pub use crate::webrtc_join::{JoinError, JoinFrames, PeerJoin, peer_join, peer_rejoin};

/// One frame channel, whatever carries it.
///
/// [`Carrier`] is the real one. Tests use an in-memory pair, which is what
/// keeps the sequence's rules runnable on every `cargo test` rather than only
/// where a DTLS handshake can happen — frames-over-DTLS is already the
/// carrier suite's receipt, and composing the two is the fixture's.
impl JoinFrames for Carrier {
    async fn recv(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.recv_frame().await.map_err(|error| error.to_string())
    }

    async fn send(&mut self, payload: &[u8]) -> Result<(), String> {
        self.send_frame(payload)
            .await
            .map_err(|error| error.to_string())
    }
}

/// The host's in-memory record of one outstanding invitation.
///
/// Holds a whole [`InviteV1`] — including its seed — which is exactly what the
/// door's storage note forbids *persisting*. This type is the reason that note
/// can be honest: the pairing lives in the accepting process for the life of
/// the offer and is never what a host writes down. A host that persists
/// invitations keeps [`RedemptionState`] plus the invitation's public terms,
/// and rebuilds this only in memory.
#[derive(Debug)]
pub struct HostedInvite {
    /// The host's own copy of the invitation. [`mint_delegation`] scopes the
    /// grant from this copy, never from anything the peer presented.
    pub invite: InviteV1,
    /// The verifier, use count and expiry the redemption spends against.
    pub redemption: RedemptionState,
}

/// Everything the host concluded from one completed join.
#[derive(Debug)]
pub struct JoinConclusion {
    /// Who was admitted, and under what.
    pub principal: AdmittedPrincipal,
    /// The verified claims behind the principal, re-decoded from the accepted
    /// hello exactly as `notochord::admit` re-decodes its own.
    pub claims: notochord::SessionClaims,
    /// The link both ends derived. Already inside the admission transcript;
    /// kept for receipts.
    pub shared_link: [u8; 16],
}

impl JoinConclusion {
    /// Assemble the admitted session over whatever stream now carries the
    /// application bytes.
    ///
    /// Split from [`host_join`] so the stream can be anything — the pumped
    /// carrier in production, a plain duplex in a test — without the join
    /// sequence caring.
    pub fn admitted_over<S>(self, stream: S, policy: &LocalNetworkPolicy) -> AdmittedSession<S> {
        AdmittedSession {
            stream,
            principal: self.principal,
            claims: self.claims,
            facts: webrtc_session_facts(self.shared_link),
            limits: policy.limits.clamped(),
        }
    }
}

/// Run the host side of the join over one connected channel.
///
/// `client_fingerprint` and `server_fingerprint` are this connection's, read
/// off the carrier after DTLS; `channel_label` likewise. On every refusing
/// path the refusal is written to the peer before this returns, so the caller
/// only has to close the channel, not explain it.
#[allow(clippy::too_many_arguments)]
pub async fn host_join<P: IdentityProvider, F: JoinFrames>(
    frames: &mut F,
    provider: &P,
    hosted: &mut HostedInvite,
    channel_label: &str,
    client_fingerprint: DtlsFingerprint,
    server_fingerprint: DtlsFingerprint,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    root_authority: [u8; 32],
    delegation_ttl_ms: u64,
    now_ms: u64,
    active_sessions: u32,
) -> Result<JoinConclusion, JoinError> {
    // Open: which invitation, and the client half of the transcript.
    let (invite_id, client_nonce) = match recv_json::<ToHost, _>(frames).await? {
        ToHost::Open {
            invite,
            client_nonce,
        } => (InviteId::from_bytes(invite), client_nonce),
        other => return Err(unexpected_message("Open", &other)),
    };
    if invite_id != hosted.invite.rendezvous() {
        // Named before any signature: an unknown invitation is not a secret,
        // and a peer probing ids learns only that this host is not offering
        // the one it guessed.
        refuse(frames, "unknown invitation").await;
        return Err(JoinError::UnknownInvite);
    }

    // Challenge: the host contributes its nonce and signs the transcript.
    let mut server_nonce = [0u8; 32];
    OsRng.fill_bytes(&mut server_nonce);
    let challenge = LinkChallenge::new(
        PROJECTION_PROTOCOL,
        channel_label,
        invite_id,
        client_nonce,
        server_nonce,
        client_fingerprint,
        server_fingerprint,
    )?;
    let signature = sign_challenge(provider, &challenge)?;
    send_json(
        frames,
        &ToPeer::Challenge {
            server_nonce,
            signature,
        },
    )
    .await?;

    // Redeem, or resume. A first join spends a use and is granted a
    // delegation; a reconnecting peer already holds one and sends `Resume`,
    // spending nothing — its delegation is judged by admission below, where
    // revocation and expiry already live.
    match recv_json::<ToHost, _>(frames).await? {
        ToHost::Redeem { subject, proof } => {
            let proof: [u8; 64] = proof
                .try_into()
                .map_err(|_| JoinError::Malformed("the redemption proof is not 64 bytes".into()))?;
            if let Err(refusal) =
                redeem(&mut hosted.redemption, &challenge, &subject, &proof, now_ms)
            {
                refuse(frames, &refusal.to_string()).await;
                return Err(refusal.into());
            }
            // Grant: the delegation the redemption bought, scoped from the
            // host's own copy of the invitation.
            let mut nonce = [0u8; 32];
            OsRng.fill_bytes(&mut nonce);
            let delegation = mint_delegation(
                provider,
                root_authority,
                subject,
                &hosted.invite,
                now_ms,
                delegation_ttl_ms,
                nonce,
            )?;
            send_json(frames, &ToPeer::Grant { delegation }).await?;
        }
        ToHost::Resume {} => {}
        other => return Err(unexpected_message("Redeem or Resume", &other)),
    }

    // Hello and reply: Notochord admission, bytes in and bytes out, exactly
    // as the door declares. The reply travels on the deny path too — that is
    // the "write the refusal, then close" rule.
    let hello = recv_binary(frames, "a Notochord hello").await?;
    let shared_link = challenge.shared_link();
    let (reply, outcome) =
        admit_webrtc_session(policy, ledger, &hello, shared_link, now_ms, active_sessions);
    frames.send(&reply).await.map_err(JoinError::Channel)?;
    let principal = outcome.map_err(JoinError::Denied)?;

    // Post-accept re-decode, notochord's own precedent: `respond` accepted,
    // so this frame decoded once already, and a failure here is a bug rather
    // than a hostile frame.
    let claims = SessionHello::decode(&hello, &policy.limits.clamped())
        .map_err(|error| {
            JoinError::Malformed(format!("accepted hello did not re-decode: {error}"))
        })?
        .claims();

    Ok(JoinConclusion {
        principal,
        claims,
        shared_link,
    })
}

/// Best-effort: a refusal the peer never sees still refuses, so the send
/// error is not the story here.
async fn refuse<F: JoinFrames>(frames: &mut F, reason: &str) {
    let _ = send_json(
        frames,
        &ToPeer::Refused {
            reason: reason.to_string(),
        },
    )
    .await;
}

/// A served join: the admitted session, and the two handles that keep it
/// alive.
///
/// `control` is not optional bookkeeping. [`CarrierControl`] cancels the
/// driver when dropped — the deliberate dead-man's switch that stops a leaked
/// carrier from spinning forever — so a caller that lets it fall ends the
/// session it just admitted. It surfaced exactly that way: the first
/// composition test hung with both joins green, because `serve_webrtc_join`
/// originally dropped the control on return and the driver cancelled under a
/// perfectly admitted session.
#[derive(Debug)]
pub struct ServedJoin {
    /// The admitted session, over the pumped byte stream.
    pub session: AdmittedSession<DuplexStream>,
    /// The frame pump. Ends when the peer closes or the stream is dropped.
    pub pump: JoinHandle<PumpEnd>,
    /// The carrier's driver handle. Hold it for the session's life; drop it
    /// to cancel.
    pub control: CarrierControl,
}

impl ServedJoin {
    /// End a served session politely, flushing what the peer is still owed.
    ///
    /// The ordering is the whole function. The serve loop's final answer —
    /// `Closed`, or a refusal — may still be sitting in the duplex when the
    /// loop returns, so: drop the stream, which lets the pump drain those
    /// bytes into the carrier and then observe end-of-stream; wait for the
    /// pump to finish carrying; only then close the carrier, whose own close
    /// flushes the outbound queue to the wire before shutting down. Dropping
    /// a [`ServedJoin`] instead of calling this cancels the driver
    /// immediately, and the peer's last answer loses the race — found live,
    /// as a close reply that reproducibly never arrived.
    pub async fn finish(self) -> Result<(), JoinError> {
        drop(self.session);
        match self.pump.await {
            Ok(end) if end.is_clean() => {}
            Ok(end) => return Err(JoinError::Channel(format!("the pump ended badly: {end}"))),
            Err(join) => return Err(JoinError::Channel(format!("the pump panicked: {join}"))),
        }
        self.control
            .close()
            .await
            .map_err(|error| JoinError::Channel(format!("the carrier did not close: {error}")))
    }
}

/// Run the whole host side over a live carrier and hand back the session the
/// application serves.
///
/// The one function a host fixture needs: join over frames, then start the
/// pump and assemble the [`AdmittedSession`] over its stream. On a refusing
/// join the refusal has already been written; this closes the carrier and
/// returns the error.
#[allow(clippy::too_many_arguments)]
pub async fn serve_webrtc_join<P: IdentityProvider>(
    mut carrier: Carrier,
    provider: &P,
    hosted: &mut HostedInvite,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    root_authority: [u8; 32],
    delegation_ttl_ms: u64,
    now_ms: u64,
    active_sessions: u32,
) -> Result<ServedJoin, JoinError> {
    let fingerprints = carrier
        .fingerprints()
        .ok_or_else(|| JoinError::Channel("the carrier has no DTLS fingerprints yet".into()))?;
    let channel_label = carrier.channel_label().to_string();

    let joined = host_join(
        &mut carrier,
        provider,
        hosted,
        &channel_label,
        *fingerprints.client(),
        *fingerprints.server(),
        policy,
        ledger,
        root_authority,
        delegation_ttl_ms,
        now_ms,
        active_sessions,
    )
    .await;

    let conclusion = match joined {
        Ok(conclusion) => conclusion,
        Err(error) => {
            // The refusal frame is already written; all that is left is to
            // stop carrying.
            let _ = carrier.close().await;
            return Err(error);
        }
    };

    let (reader, writer, control) = carrier.into_parts();
    let (stream, pump) = stream_over_frames(reader, writer);
    Ok(ServedJoin {
        session: conclusion.admitted_over(stream, policy),
        pump,
        control,
    })
}

#[cfg(test)]
mod tests {
    //! The join sequence with both roles live, over in-memory frames.
    //!
    //! The door's fail-closed matrix already proves every rule; the pump test
    //! in `webrtc-carrier` already proves frames-over-DTLS become a byte
    //! stream. What is proven here is the piece neither can: the two roles
    //! agree on the sequence, a refusal is *told* to the peer, and a completed
    //! join carries a real Graphshell session driven by the event-driven
    //! adapter against the real serve loop.

    use std::sync::RwLock;

    use super::*;
    use crate::carrier::projection_policy;
    use crate::lifecycle::SessionAuthority;
    use crate::resume::ResumeFixtureEndpoint;
    use crate::session_loop::serve_admitted_session;
    use crate::webrtc_door::RedemptionRefusal;
    use crate::webrtc_door::{InviteTerms, issue_invite};
    use graphshell_client::{Advance, Outcome, SessionDriver};
    use graphshell_endpoint::ResumableProjectionSource;
    use notochord::TrustedRoot;
    use notochord::{NetworkId, ProfileRef};
    use personae::InMemoryProvider;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::sync::mpsc;
    use webrtc_carrier::{DTLS_FINGERPRINT_BYTES, FingerprintRole, ReleaseRefV1};

    const NETWORK: NetworkId = NetworkId([3; 32]);
    const ROOT_AUTHORITY: [u8; 32] = [7; 32];
    const NOW_MS: u64 = 50;
    const TTL_MS: u64 = 10_000;
    const INVITE_EXPIRY_MS: u64 = 20_000;
    const LABEL: &str = "mere-graphshell";

    /// One direction of an in-memory frame channel.
    struct MemFrames {
        tx: mpsc::Sender<Vec<u8>>,
        rx: mpsc::Receiver<Vec<u8>>,
    }

    impl JoinFrames for MemFrames {
        async fn recv(&mut self) -> Result<Option<Vec<u8>>, String> {
            Ok(self.rx.recv().await)
        }

        async fn send(&mut self, payload: &[u8]) -> Result<(), String> {
            self.tx
                .send(payload.to_vec())
                .await
                .map_err(|_| "the far end hung up".to_string())
        }
    }

    fn frame_pair() -> (MemFrames, MemFrames) {
        let (a_tx, a_rx) = mpsc::channel(8);
        let (b_tx, b_rx) = mpsc::channel(8);
        (
            MemFrames { tx: a_tx, rx: b_rx },
            MemFrames { tx: b_tx, rx: a_rx },
        )
    }

    fn fingerprints() -> (DtlsFingerprint, DtlsFingerprint) {
        (
            DtlsFingerprint::new(FingerprintRole::Client, [0xaa; DTLS_FINGERPRINT_BYTES]),
            DtlsFingerprint::new(FingerprintRole::Server, [0xbb; DTLS_FINGERPRINT_BYTES]),
        )
    }

    fn profile() -> ProfileRef {
        ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }
    }

    fn release() -> ReleaseRefV1 {
        ReleaseRefV1 {
            manifest_blake3: [0x5a; 32],
            publisher_key_id: [0x6b; 32],
        }
    }

    fn host() -> InMemoryProvider {
        InMemoryProvider::from_seed([1; 32])
    }

    fn policy() -> LocalNetworkPolicy {
        projection_policy(
            NETWORK,
            vec![TrustedRoot {
                authority: ROOT_AUTHORITY,
                issuer: host().master_public_key().to_bytes(),
            }],
            vec![profile()],
            None,
        )
    }

    /// Issue an invitation and split it the way a real join splits it: the
    /// fragment travels, the host keeps its own copy plus the redemption.
    fn issued() -> (HostedInvite, InviteV1) {
        let issue = issue_invite(
            &host(),
            &InviteTerms::projection(NETWORK, profile(), INVITE_EXPIRY_MS, 1, release()),
        )
        .expect("the invitation issues");
        let fragment = issue.descriptor.invite.to_fragment();
        let peer_copy = InviteV1::parse_fragment(&fragment).expect("the fragment parses");
        (
            HostedInvite {
                invite: issue.descriptor.invite,
                redemption: issue.redemption,
            },
            peer_copy,
        )
    }

    /// Run both roles to completion and return what each concluded.
    async fn joined() -> (JoinConclusion, PeerJoin, InMemoryProvider) {
        let (mut host_frames, mut peer_frames) = frame_pair();
        let (client_fp, server_fp) = fingerprints();
        let (mut hosted, peer_invite) = issued();
        let host_provider = host();
        let ephemeral = InMemoryProvider::from_seed([9; 32]);
        let policy = policy();
        let ledger = RevocationLedger::new();
        let limits = policy.limits.clamped();

        let host_side = host_join(
            &mut host_frames,
            &host_provider,
            &mut hosted,
            LABEL,
            client_fp,
            server_fp,
            &policy,
            &ledger,
            ROOT_AUTHORITY,
            TTL_MS,
            NOW_MS,
            0,
        );
        let peer_side = peer_join(
            &mut peer_frames,
            &ephemeral,
            &peer_invite,
            LABEL,
            client_fp,
            server_fp,
            &limits,
        );
        let (host_end, peer_end) = tokio::join!(host_side, peer_side);
        (
            host_end.expect("the host admits"),
            peer_end.expect("the peer is admitted"),
            ephemeral,
        )
    }

    /// Reconnect is a new link with fresh admission and no second redemption.
    ///
    /// The first join spends the invitation's only use. The rejoin runs on a
    /// fresh channel with fresh fingerprints — a genuinely different DTLS link
    /// — presenting the retained delegation, and is admitted even though the
    /// invitation has nothing left to spend. That is C2's one-use ceiling and
    /// C3's fresh-admission rule holding at the same time.
    #[tokio::test]
    async fn a_rejoin_reuses_the_delegation_and_spends_nothing() {
        let (mut host_frames, mut peer_frames) = frame_pair();
        let (client_fp, server_fp) = fingerprints();
        let (mut hosted, peer_invite) = issued();
        let host_provider = host();
        let ephemeral = InMemoryProvider::from_seed([9; 32]);
        let policy = policy();
        let ledger = RevocationLedger::new();
        let limits = policy.limits.clamped();

        // First join, spending the one use.
        let first = {
            let host_side = host_join(
                &mut host_frames,
                &host_provider,
                &mut hosted,
                LABEL,
                client_fp,
                server_fp,
                &policy,
                &ledger,
                ROOT_AUTHORITY,
                TTL_MS,
                NOW_MS,
                0,
            );
            let peer_side = peer_join(
                &mut peer_frames,
                &ephemeral,
                &peer_invite,
                LABEL,
                client_fp,
                server_fp,
                &limits,
            );
            let (host_end, peer_end) = tokio::join!(host_side, peer_side);
            host_end.expect("the first join admits");
            peer_end.expect("the peer is admitted")
        };
        assert_eq!(hosted.redemption.remaining_uses(), 0, "the use is spent");

        // A new link: new frames, new fingerprints, same delegation.
        let (mut host_frames, mut peer_frames) = frame_pair();
        let client_fp =
            DtlsFingerprint::new(FingerprintRole::Client, [0xcc; DTLS_FINGERPRINT_BYTES]);
        let server_fp =
            DtlsFingerprint::new(FingerprintRole::Server, [0xdd; DTLS_FINGERPRINT_BYTES]);

        let host_side = host_join(
            &mut host_frames,
            &host_provider,
            &mut hosted,
            LABEL,
            client_fp,
            server_fp,
            &policy,
            &ledger,
            ROOT_AUTHORITY,
            TTL_MS,
            NOW_MS,
            0,
        );
        let peer_side = peer_rejoin(
            &mut peer_frames,
            &ephemeral,
            &peer_invite,
            first.delegation,
            LABEL,
            client_fp,
            server_fp,
            &limits,
        );
        let (host_end, peer_end) = tokio::join!(host_side, peer_side);
        let conclusion = host_end.expect("the rejoin admits over the spent invitation");
        let rejoined = peer_end.expect("the peer is admitted again");

        assert_eq!(
            hosted.redemption.remaining_uses(),
            0,
            "a rejoin spends nothing and cannot go negative"
        );
        assert_ne!(
            first.shared_link, rejoined.shared_link,
            "a new link is a new link — different fingerprints, different transcript"
        );
        assert_eq!(conclusion.principal.session_id, rejoined.session_id);
        assert_eq!(
            conclusion.principal.subject,
            ephemeral.master_public_key().to_bytes(),
            "the same subject, admitted afresh"
        );
    }

    /// The positive control: both roles, one sequence, agreeing conclusions.
    #[tokio::test]
    async fn a_join_over_frames_is_admitted_end_to_end() {
        let (conclusion, peer, ephemeral) = joined().await;
        assert_eq!(
            conclusion.principal.subject,
            ephemeral.master_public_key().to_bytes(),
            "the admitted subject is the ephemeral key the peer generated"
        );
        assert_eq!(
            conclusion.principal.session_id, peer.session_id,
            "both ends name the same transcript"
        );
        assert_eq!(
            conclusion.shared_link, peer.shared_link,
            "both ends derived the same link"
        );
    }

    /// A second redemption of a one-use invitation is refused, and the peer is
    /// told rather than left watching a dead channel.
    #[tokio::test]
    async fn a_spent_invitation_refuses_and_says_so() {
        let (mut host_frames, mut peer_frames) = frame_pair();
        let (client_fp, server_fp) = fingerprints();
        let (mut hosted, peer_invite) = issued();
        // Spend the single use before the join.
        hosted.redemption = RedemptionState::new(hosted.redemption.verifier(), 0, INVITE_EXPIRY_MS);
        let host_provider = host();
        let ephemeral = InMemoryProvider::from_seed([9; 32]);
        let policy = policy();
        let ledger = RevocationLedger::new();
        let limits = policy.limits.clamped();

        let host_side = host_join(
            &mut host_frames,
            &host_provider,
            &mut hosted,
            LABEL,
            client_fp,
            server_fp,
            &policy,
            &ledger,
            ROOT_AUTHORITY,
            TTL_MS,
            NOW_MS,
            0,
        );
        let peer_side = peer_join(
            &mut peer_frames,
            &ephemeral,
            &peer_invite,
            LABEL,
            client_fp,
            server_fp,
            &limits,
        );
        let (host_end, peer_end) = tokio::join!(host_side, peer_side);
        assert!(
            matches!(
                host_end,
                Err(JoinError::Redemption(RedemptionRefusal::Exhausted))
            ),
            "the host records the exhaustion: {host_end:?}"
        );
        let refused = peer_end.expect_err("the peer is refused");
        assert!(
            refused.to_string().contains("no remaining uses"),
            "the peer is told why: {refused}"
        );
    }

    /// A host that cannot prove the invitation's key gets no redemption proof.
    ///
    /// The peer walks away after the challenge, so the invitation's use count
    /// is never spent — which is the property that makes a relay-in-the-middle
    /// unable to exhaust an invitation it cannot sign for.
    #[tokio::test]
    async fn a_wrong_host_is_refused_before_any_secret_crosses() {
        let (mut host_frames, peer_frames) = frame_pair();
        let (client_fp, server_fp) = fingerprints();
        let (mut hosted, peer_invite) = issued();
        let ephemeral = InMemoryProvider::from_seed([9; 32]);
        let policy = policy();
        let ledger = RevocationLedger::new();
        let limits = policy.limits.clamped();
        // The signer is not the host the invitation names.
        let impostor = InMemoryProvider::from_seed([2; 32]);

        // The peer runs in its own task and *drops its channel* on return —
        // walking away is the refusal, and the host has to cope with a peer
        // that says nothing more, not one that politely explains.
        let peer = tokio::spawn(async move {
            let mut frames = peer_frames;
            peer_join(
                &mut frames,
                &ephemeral,
                &peer_invite,
                LABEL,
                client_fp,
                server_fp,
                &limits,
            )
            .await
        });
        let host_end = host_join(
            &mut host_frames,
            &impostor,
            &mut hosted,
            LABEL,
            client_fp,
            server_fp,
            &policy,
            &ledger,
            ROOT_AUTHORITY,
            TTL_MS,
            NOW_MS,
            0,
        )
        .await;
        let peer_end = peer.await.expect("the peer task does not panic");
        assert!(
            matches!(peer_end, Err(JoinError::HostUnverified)),
            "the peer refuses the channel: {peer_end:?}"
        );
        // The peer hung up without redeeming, so the host ends on the closed
        // channel — and critically, the use count is intact.
        assert!(
            matches!(host_end, Err(JoinError::Channel(_))),
            "{host_end:?}"
        );
        assert_eq!(
            hosted.redemption.remaining_uses(),
            1,
            "an unverified host cannot cost the invitation anything"
        );
    }

    /// The flagship: a completed join carries a real Graphshell session.
    ///
    /// The host end is the real `serve_admitted_session` over a real
    /// `ResumeFixtureEndpoint`; the peer end is the event-driven
    /// `SessionDriver` — the browser's adapter — speaking NDJSON lines. This
    /// is every C4a seam except DTLS itself composed in one test: join,
    /// admission, authority, serve loop, discovery, mount, close.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_completed_join_serves_a_real_session_to_the_event_driver() {
        let (conclusion, _peer, _ephemeral) = joined().await;
        let policy = policy();

        let (host_stream, peer_stream) = tokio::io::duplex(64 * 1024);
        let mut admitted = conclusion.admitted_over(host_stream, &policy);
        let authority = SessionAuthority::retain_admitted(&admitted);
        let revocations = RwLock::new(RevocationLedger::new());
        let mut endpoint = ResumeFixtureEndpoint::new();

        let serving = async {
            let mut resume = |endpoint: &mut ResumeFixtureEndpoint, request| {
                ResumableProjectionSource::resume(endpoint, request)
                    .map_err(|error| error.to_string())
            };
            serve_admitted_session(
                &mut admitted,
                &authority,
                &revocations,
                &mut endpoint,
                &mut resume,
                || NOW_MS,
            )
            .await
        };

        let driving = async {
            let (read, mut write) = tokio::io::split(peer_stream);
            let mut lines = BufReader::new(read).lines();
            let mut driver = SessionDriver::new(chirograph::CapabilityProfile::default());

            // Carry one Advance to the wire, then feed lines back until the
            // operation concludes — the event-driven loop a browser runs, with
            // a duplex where the data channel will be.
            async fn drive<R, W>(
                driver: &mut SessionDriver,
                write: &mut W,
                lines: &mut tokio::io::Lines<BufReader<R>>,
                start: Advance,
            ) -> Result<Outcome, String>
            where
                R: tokio::io::AsyncRead + Unpin,
                W: tokio::io::AsyncWrite + Unpin,
            {
                let mut advance = start;
                loop {
                    match advance {
                        Advance::Done(outcome) => return Ok(outcome),
                        Advance::Noted => {
                            let line = lines
                                .next_line()
                                .await
                                .map_err(|error| error.to_string())?
                                .ok_or("the session ended mid-operation")?;
                            advance = driver.on_line(&line)?;
                        }
                        Advance::Send(line) => {
                            write
                                .write_all(line.as_bytes())
                                .await
                                .map_err(|error| error.to_string())?;
                            write
                                .write_all(b"\n")
                                .await
                                .map_err(|error| error.to_string())?;
                            let line = lines
                                .next_line()
                                .await
                                .map_err(|error| error.to_string())?
                                .ok_or("the session ended mid-operation")?;
                            advance = driver.on_line(&line)?;
                        }
                    }
                }
            }

            let start = driver.discover()?;
            let discovered = drive(&mut driver, &mut write, &mut lines, start).await?;
            let Outcome::Descriptor(descriptor) = discovered else {
                return Err(format!("expected a descriptor, got {discovered:?}"));
            };
            assert_eq!(descriptor.projections.len(), 1, "the fixture offers one");

            let start = driver
                .core_mut()
                .ok_or("no core after discovery")?
                .mount(0)?;
            let start = driver.begin(start)?;
            let mounted = drive(&mut driver, &mut write, &mut lines, start).await?;
            let Outcome::Mounted(session) = mounted else {
                return Err(format!("expected a mount, got {mounted:?}"));
            };
            assert!(
                driver
                    .core()
                    .and_then(|core| core.client().mounted(&session))
                    .is_some(),
                "the driver's client state holds the mounted scene"
            );

            let close = driver.core_mut().ok_or("no core")?.close();
            let close = driver.begin(close)?;
            let closed = drive(&mut driver, &mut write, &mut lines, close).await?;
            assert!(matches!(closed, Outcome::Closed));
            Ok::<(), String>(())
        };

        let (summary, drove) = tokio::join!(serving, driving);
        drove.expect("the event driver completes discovery, mount, and close");
        let summary = summary.expect("the serve loop ends without a session error");
        assert_eq!(
            summary.answered, 3,
            "discover, snapshot, close — and nothing else"
        );
    }
}
