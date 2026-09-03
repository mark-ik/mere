// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! G5d: Graphshell's projection carrier.
//!
//! The accept path that runs *before* a single `SessionOpen` byte is read. It
//! is the second of Notochord N2's two service carriers (the first is Murm's
//! real accept path), and it deliberately owns none of the machinery it uses:
//!
//! - the carrier facts come from [`transport::AcceptedSession::into_session`],
//!   the one audited adapter (N1), so this port never hand-builds a
//!   [`SessionFacts`](notochord::SessionFacts);
//! - the framing and the admitted conclusion come from
//!   [`notochord::admit_session`], so a refusal is finished rather than
//!   flushed and a refused stream cannot reach an application;
//! - the delegation grammar stays in Personae, and the owner's rules stay in
//!   the policy the host supplies.
//!
//! What is Graphshell's, and lives here: the ALPN, the service path the owner
//! offers, and which admitted actions this service serves.
//!
//! ## The transport must outlive the session
//!
//! [`accept_projection_session`] borrows its transport and never takes
//! ownership. That is load-bearing rather than stylistic. Both carriers drain
//! buffered writes when a *stream* is dropped, but by two unrelated
//! mechanisms sharing one escape hatch:
//!
//! - p2panda: quinn's `Drop for SendStream` finishes the stream, except when
//!   the connection is already errored, which is exactly what dropping the
//!   endpoint underneath it produces;
//! - Reticulum: the outbound relay reads its duplex to EOF, unless the
//!   endpoint has been torn down and the relay task aborted.
//!
//! So a short-lived accept task owning its transport would discard the reply
//! it had already written, on either arm, whenever it returned promptly.
//! Taking `&T` makes that unspellable here.
//!
//! The accept step is therefore the one part of this path that cannot move.
//! [`admit_accepted_session`] carries everything after it, for a caller that
//! already holds an accepted session and has no `&T` to borrow from.
//!
//! ## Why the action check is not a second admission
//!
//! The owner policy allows only named admission actions. Graphshell checks the
//! admitted action again through [`crate::admission::serves_action`], because
//! the service implementation remains authoritative about what it actually
//! serves even when the owner's serialized vocabulary changes.

use notochord::{
    AdmittedSession, DenyReason, IoHandshakeError, LocalNetworkPolicy, NetworkId, ProfileRef,
    RevocationLedger, ServiceAccess, ServiceRule, TrustedRoot, admit_session,
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use transport::{AcceptedSession, Alpn, Transport, TransportError};

use crate::admission::{
    CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_PROTOCOL, PROJECTION_SERVICE, serves_action,
};

/// The ALPN a projection session is accepted for.
///
/// The same bytes are the protocol in the signed transcript, so a proof minted
/// for another protocol on the same connection does not verify here.
pub fn projection_alpn() -> Alpn {
    Alpn::from_bytes(PROJECTION_PROTOCOL)
}

/// A policy offering exactly one service: Graphshell projections.
///
/// A convenience for a host that serves projections and nothing else; a host
/// with more services builds its own and inserts the same rule. The value is
/// that the service path and its default posture are stated once.
///
/// `MemberOnly` is the default on purpose. A projection session hands out a
/// live view of a graph, so it wants a delegation chain rather than an open
/// door.
pub fn projection_policy(
    network: NetworkId,
    trusted_roots: Vec<TrustedRoot>,
    accepted_profiles: Vec<ProfileRef>,
    max_sessions: Option<u32>,
) -> LocalNetworkPolicy {
    let mut policy = LocalNetworkPolicy::closed(network);
    policy.trusted_roots = trusted_roots;
    policy.accepted_profiles = accepted_profiles;
    policy.services.insert(
        PROJECTION_SERVICE.to_string(),
        ServiceRule::new(
            ServiceAccess::MemberOnly,
            GRAPHSHELL_DOMAIN,
            [CONNECT_ACTION],
            false,
            max_sessions,
        ),
    );
    policy
}

/// Why a projection session was not served.
///
/// Split because the two mean different things to whoever reads the log: one
/// is the owner's policy or the peer's authority falling short, the other is a
/// peer who was genuinely admitted asking this service for something it does
/// not do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionRefusal {
    /// Admission refused the session, and the peer has been told why.
    NotAdmitted(DenyReason),
    /// Admitted, but for an action projections do not serve.
    ///
    /// Carries that action for the log. The peer sees the session close
    /// rather than a handshake denial, because the handshake did not deny it:
    /// this service did.
    ActionNotServed(String),
}

/// Errors that stop the accept path before it reaches a decision.
#[derive(Debug, thiserror::Error)]
pub enum ProjectionAcceptError {
    /// The carrier could not accept a session.
    #[error("projection carrier accept failed: {0}")]
    Carrier(#[from] TransportError),
    /// The handshake could not complete on the accepted stream.
    #[error(transparent)]
    Handshake(#[from] IoHandshakeError),
}

/// Accept one projection session, admitted and action-checked.
///
/// On `Ok(Ok(session))` the stream has cleared admission *and* carries an
/// action this service serves, so it is ready for `SessionOpen`. Nothing in
/// `chirograph` restates any of it: the protocol negotiates version
/// and capabilities only, because the principal was settled here.
///
/// `transport` is borrowed; see the module docs on why owning it would lose
/// the reply.
pub async fn accept_projection_session<T: Transport>(
    transport: &T,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    now_ms: u64,
    active_sessions: u32,
) -> Result<Result<AdmittedSession<T::Stream>, ProjectionRefusal>, ProjectionAcceptError> {
    let accepted = transport.accept(projection_alpn()).await?;
    admit_accepted_session(accepted, policy, ledger, now_ms, active_sessions).await
}

/// Admit and action-check a session this carrier did not accept itself.
///
/// [`accept_projection_session`] is [`admit_accepted_session`] with the
/// accept step glued on the front, for a caller that dials `&T` and lets a
/// [`Transport`] impl do the accepting. Not every carrier can do that: the
/// coming browser WebRTC lane accepts its own stream off a signalling
/// channel and has no bilateral `Transport` impl to hand a `&T` for — there
/// is nothing to `.accept(alpn)` on. Rather than make it fabricate one, this
/// function is the seam such a carrier calls directly, taking the
/// [`AcceptedSession`] it already produced and running exactly the same
/// admission and action check `accept_projection_session` runs after its own
/// accept: N1's facts, `notochord::admit_session`, then
/// [`crate::admission::serves_action`].
///
/// Same refusal semantics as `accept_projection_session`: a refusal is
/// finished, never dropped, and a refused stream never reaches the caller.
pub async fn admit_accepted_session<S>(
    accepted: AcceptedSession<S>,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    now_ms: u64,
    active_sessions: u32,
) -> Result<Result<AdmittedSession<S>, ProjectionRefusal>, ProjectionAcceptError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // N1's adapter, not a local copy: every fact below is read off the
    // acceptance record before any application byte is read.
    let (stream, facts) = accepted.into_session();

    let admitted = admit_session(stream, policy, ledger, &facts, now_ms, active_sessions).await?;
    let mut session = match admitted {
        Ok(session) => session,
        Err(reason) => return Ok(Err(ProjectionRefusal::NotAdmitted(reason))),
    };

    if !serves_action(&session.principal) {
        let action = session.principal.action.action.clone();
        // Finish rather than drop, for the same reason a refused handshake is
        // finished. Both arms happen to drain on drop today, through two Drop
        // impls nobody chose; saying it explicitly is the difference between a
        // guarantee and an accident.
        let _ = session.stream.shutdown().await;
        return Ok(Err(ProjectionRefusal::ActionNotServed(action)));
    }

    Ok(Ok(session))
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;
    use crate::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, connect_action, open_session};
    use notochord::{CarrierKind, RequestedAction, SessionHello, TrafficClass, initiate_session};
    use personae::IdentityProvider;
    use personae::InMemoryProvider;
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    };
    use tokio::io::AsyncReadExt;
    use transport::memory::MemoryTransport;
    use transport::{
        IngressContext, P2pandaTransport, PeerID, initiator_binding, initiator_link_binding,
    };

    const NETWORK: NetworkId = NetworkId([3; 32]);
    const ROOT_AUTHORITY: [u8; 32] = [7; 32];
    const NOW_MS: u64 = 50;

    fn p2panda_receipt_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    fn owner() -> InMemoryProvider {
        InMemoryProvider::from_seed([1; 32])
    }

    fn viewer() -> InMemoryProvider {
        InMemoryProvider::from_seed([4; 32])
    }

    fn profile_ref() -> ProfileRef {
        ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }
    }

    /// A grant from the owner letting `subject` perform `action` under one
    /// service triple.
    fn grant_for(
        subject: [u8; 32],
        domain: &str,
        path: &str,
        action: &str,
    ) -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            &owner(),
            DelegationCertificate::new(
                DelegationParent::Root(ROOT_AUTHORITY),
                owner().master_public_key().to_bytes(),
                subject,
                CapabilityScope {
                    domain: domain.into(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: path.into(),
                    actions: [action.to_string()].into_iter().collect(),
                },
                5,
                10,
                Some(100),
                1,
                [1; 32],
            ),
        )
        .expect("issue certificate")
    }

    /// A grant from the owner letting `subject` perform `action` at the
    /// projection service.
    fn grant(subject: [u8; 32], action: &str) -> SignedDelegationCertificate {
        grant_for(subject, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE, action)
    }

    fn policy() -> LocalNetworkPolicy {
        projection_policy(
            NETWORK,
            vec![TrustedRoot {
                authority: ROOT_AUTHORITY,
                issuer: owner().master_public_key().to_bytes(),
            }],
            vec![profile_ref()],
            None,
        )
    }

    /// Drive one session over the paired memory carrier and return what the
    /// responder decided.
    ///
    /// The client half holds its stream until the responder has decided, which
    /// is what a real client does and what the module docs require of a
    /// service: neither side may drop the connection out from under a reply
    /// that has been written.
    /// `Ok(action)` means the session was served under that action.
    async fn run(action: &str) -> Result<String, ProjectionRefusal> {
        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        // The memory carrier authenticates its counterparty by construction,
        // so policy rule D6 requires the claimed subject to *be* the peer the
        // carrier proved. Giving the client node the viewer's own key is what
        // makes this an honest fixture rather than one that dodges the rule.
        let client_peer = PeerID::from_bytes(&subject).expect("client peer");
        let server_peer =
            PeerID::from_bytes(&owner().master_public_key().to_bytes()).expect("server peer");
        let (server, client) = MemoryTransport::pair(server_peer, client_peer);

        let mut requested = connect_action();
        requested.action = action.to_string();
        let delegations = vec![grant(subject, action)];
        let action_owned = action.to_string();
        let mut serving_policy = policy();
        serving_policy
            .services
            .get_mut(PROJECTION_SERVICE)
            .expect("projection rule")
            .actions
            .insert(action.to_string());

        let client_task = tokio::spawn(async move {
            let mut stream = client
                .connect(server_peer, projection_alpn())
                .await
                .expect("dial");
            let binding = initiator_binding(&projection_alpn(), client_peer);
            // The connect case goes through Graphshell's own helper, because
            // that is the path a real viewer takes. The other case cannot:
            // `open_session` fixes the action to `connect`, and editing a
            // signed hello afterwards would invalidate it. A peer asking for
            // something else builds its own hello, which is exactly what the
            // open action vocabulary allows and what this test needs to be
            // honest about.
            let hello = if action_owned == CONNECT_ACTION {
                open_session(
                    &viewer,
                    NETWORK,
                    profile_ref(),
                    TrafficClass::Interactive,
                    [5; 32],
                    &binding,
                    delegations,
                )
            } else {
                SessionHello::issue(
                    &viewer,
                    NETWORK,
                    profile_ref(),
                    RequestedAction {
                        domain: GRAPHSHELL_DOMAIN.into(),
                        path: PROJECTION_SERVICE.into(),
                        action: action_owned,
                    },
                    TrafficClass::Interactive,
                    [5; 32],
                    &binding,
                    delegations,
                )
            }
            .expect("issue hello");
            let _ = initiate_session(&mut stream, &hello, &policy().limits.clamped()).await;
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        });

        let outcome = accept_projection_session(
            &server,
            &serving_policy,
            &RevocationLedger::default(),
            NOW_MS,
            0,
        )
        .await
        .expect("accept path");
        client_task.abort();

        outcome.map(|session| session.principal.action.action.clone())
    }

    #[tokio::test]
    async fn a_connect_grant_is_admitted_and_served() {
        let served = run(CONNECT_ACTION).await.expect("must be served");
        assert_eq!(served, CONNECT_ACTION);
    }

    /// Even when the owner document allows an action, the service remains
    /// authoritative about what its implementation serves.
    #[tokio::test]
    async fn an_admitted_action_this_service_does_not_serve_is_refused() {
        let refusal = run("administer").await.expect_err("must be refused");
        assert_eq!(
            refusal,
            ProjectionRefusal::ActionNotServed("administer".to_string())
        );
    }

    /// C0's acceptance fixture: a listener-only carrier reaches admission.
    ///
    /// The WebRTC lane has no bilateral `Transport` to accept from, so it
    /// builds its own [`AcceptedSession`] and enters at
    /// [`admit_accepted_session`]. Two facts make it the honest shape rather
    /// than a p2panda fixture wearing a different label: the carrier
    /// authenticates nobody, so `peer` is `None`, and the subject is instead
    /// bound to the link the two ends derived from the host challenge. That is
    /// the same binding Reticulum uses, which is the point — WebRTC needs no
    /// new proof grammar, only a link to put in the existing one.
    ///
    /// The link is derived through `webrtc-carrier` rather than pasted, so a
    /// change to the transcript encoding fails here too.
    #[tokio::test]
    async fn a_webrtc_session_is_admitted_with_no_authenticated_peer() {
        use webrtc_carrier::{DtlsFingerprint, FingerprintRole, InviteId, LinkChallenge};

        let challenge = LinkChallenge::new(
            PROJECTION_PROTOCOL,
            "mere-graphshell",
            InviteId::from_bytes([9; 16]),
            [0x11; 32],
            [0x22; 32],
            DtlsFingerprint::new(FingerprintRole::Client, [0xAA; 32]),
            DtlsFingerprint::new(FingerprintRole::Server, [0xBB; 32]),
        )
        .expect("link challenge");
        let shared_link = challenge.shared_link();
        assert_ne!(
            shared_link, [0u8; 16],
            "a derived link of all zeroes would make the binding vacuous"
        );

        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        let delegations = vec![grant(subject, CONNECT_ACTION)];
        let (client_half, server_half) = tokio::io::duplex(64 * 1024);

        let client_task = tokio::spawn(async move {
            let mut stream = client_half;
            // No transport identity to bind, so the shared link carries the
            // weight. `initiator_binding` would be a lie here: nothing
            // authenticated this peer.
            let binding = initiator_link_binding(&projection_alpn(), shared_link);
            let hello = open_session(
                &viewer,
                NETWORK,
                profile_ref(),
                TrafficClass::Interactive,
                [5; 32],
                &binding,
                delegations,
            )
            .expect("issue hello");
            let _ = initiate_session(&mut stream, &hello, &policy().limits.clamped()).await;
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        });

        let accepted = AcceptedSession::new(
            server_half,
            projection_alpn(),
            // The carrier authenticated no initiator, and says so.
            None,
            IngressContext::webrtc(shared_link),
        );
        assert!(
            !accepted.is_transport_authenticated(),
            "the WebRTC carrier must never look authenticated"
        );
        let facts = accepted.session_facts();
        assert_eq!(facts.authenticated_initiator, None);
        assert_eq!(facts.ingress.shared_link, Some(shared_link));
        assert_eq!(facts.ingress.local_interface, None);

        let outcome = admit_accepted_session(
            accepted,
            &policy(),
            &RevocationLedger::default(),
            NOW_MS,
            0,
        )
        .await
        .expect("admission path");
        client_task.abort();

        let session = outcome.expect("the webrtc session must be admitted");
        assert_eq!(session.principal.action.action, CONNECT_ACTION);
    }

    async fn p2panda_pair() -> (P2pandaTransport, P2pandaTransport, PeerID, PeerID) {
        let client = P2pandaTransport::builder_from_seed(viewer().master_keypair().to_seed())
            .alpns(vec![projection_alpn()])
            .bind()
            .await
            .expect("bind projection client");
        let server = P2pandaTransport::builder_from_seed(owner().master_keypair().to_seed())
            .alpns(vec![projection_alpn()])
            .bind()
            .await
            .expect("bind projection server");
        let client_peer = client.local_peer_id();
        let server_peer = server.local_peer_id();

        let client_addr =
            tokio::time::timeout(std::time::Duration::from_secs(10), client.endpoint_addr())
                .await
                .expect("client endpoint address timeout")
                .expect("client endpoint address");
        let server_addr =
            tokio::time::timeout(std::time::Duration::from_secs(10), server.endpoint_addr())
                .await
                .expect("server endpoint address timeout")
                .expect("server endpoint address");
        client
            .add_peer(server_addr)
            .await
            .expect("client registers server");
        server
            .add_peer(client_addr)
            .await
            .expect("server registers client");

        (server, client, server_peer, client_peer)
    }

    /// The literal G5d receipt. The first carrier tests exercised the generic
    /// accept path through `MemoryTransport`; this one proves the same path
    /// against p2panda-net's authenticated Iroh connection and carries
    /// application bytes only after admission.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn p2panda_admits_the_authenticated_viewer_before_application_bytes() {
        let _receipt_guard = p2panda_receipt_lock().lock().await;
        const APPLICATION_BYTES: &[u8] = b"graphshell-session-open-may-start";

        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        let (server, client, server_peer, client_peer) = p2panda_pair().await;
        let (server_finished_tx, server_finished_rx) = tokio::sync::oneshot::channel();
        assert_eq!(
            client_peer.to_bytes(),
            subject,
            "the carrier identity is the Personae subject that signs the hello"
        );

        let client_task = tokio::spawn(async move {
            let alpn = projection_alpn();
            let mut stream = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                client.connect(server_peer, alpn.clone()),
            )
            .await
            .expect("projection dial timeout")
            .expect("projection dial");
            let binding = initiator_binding(&alpn, client_peer);
            let hello = open_session(
                &viewer,
                NETWORK,
                profile_ref(),
                TrafficClass::Interactive,
                [5; 32],
                &binding,
                vec![grant(subject, CONNECT_ACTION)],
            )
            .expect("issue projection hello");
            let reply = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                initiate_session(&mut stream, &hello, &policy().limits.clamped()),
            )
            .await
            .expect("projection handshake timeout")
            .expect("projection handshake");
            assert!(reply.is_accept(), "the initiator sees admission");

            let mut application = vec![0; APPLICATION_BYTES.len()];
            tokio::time::timeout(
                std::time::Duration::from_secs(10),
                stream.read_exact(&mut application),
            )
            .await
            .expect("application read timeout")
            .expect("application read");
            let _ = server_finished_rx.await;
            application
        });

        let mut session = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            accept_projection_session(&server, &policy(), &RevocationLedger::default(), NOW_MS, 0),
        )
        .await
        .expect("projection accept timeout")
        .expect("projection accept path")
        .expect("projection session admitted");
        assert_eq!(session.principal.subject, subject);
        assert_eq!(session.facts.transport, CarrierKind::P2panda);
        assert_eq!(
            session.facts.authenticated_initiator,
            Some(client_peer.to_bytes()),
            "the admitted session retains the peer p2panda authenticated"
        );
        let retained = crate::lifecycle::SessionAuthority::retain_admitted(&session);
        assert_eq!(
            retained.deadline_ms(),
            Some(100),
            "the carrier also retains the verified chain needed for later revocation checks"
        );
        session
            .stream
            .write_all(APPLICATION_BYTES)
            .await
            .expect("application write");
        session.stream.shutdown().await.expect("finish application");
        let _ = server_finished_tx.send(());
        let principal = session.principal;

        let application = tokio::time::timeout(std::time::Duration::from_secs(10), client_task)
            .await
            .expect("projection client timeout")
            .expect("projection client task");
        assert_eq!(application, APPLICATION_BYTES);
        assert_eq!(principal.action, connect_action());
    }

    /// The same real carrier must preserve the cross-service boundary. The
    /// grant is valid and belongs to the authenticated peer, but Murm
    /// authority cannot open Graphshell and not one projection byte follows
    /// the denial.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn p2panda_murm_grant_is_refused_before_projection_bytes() {
        let _receipt_guard = p2panda_receipt_lock().lock().await;
        let viewer = viewer();
        let subject = viewer.master_public_key().to_bytes();
        let (server, client, server_peer, client_peer) = p2panda_pair().await;

        let client_task = tokio::spawn(async move {
            let alpn = projection_alpn();
            let mut stream = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                client.connect(server_peer, alpn.clone()),
            )
            .await
            .expect("projection dial timeout")
            .expect("projection dial");
            let binding = initiator_binding(&alpn, client_peer);
            let hello = open_session(
                &viewer,
                NETWORK,
                profile_ref(),
                TrafficClass::Interactive,
                [5; 32],
                &binding,
                vec![grant_for(
                    subject,
                    "mere.network",
                    "/services/murm",
                    CONNECT_ACTION,
                )],
            )
            .expect("issue projection hello with foreign grant");
            let reply = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                initiate_session(&mut stream, &hello, &policy().limits.clamped()),
            )
            .await
            .expect("projection refusal timeout")
            .expect("projection refusal");
            assert!(!reply.is_accept(), "the initiator sees refusal");

            let mut application = Vec::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(10),
                stream.read_to_end(&mut application),
            )
            .await
            .expect("refused stream close timeout")
            .expect("read refused stream");
            application
        });

        let refusal = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            accept_projection_session(&server, &policy(), &RevocationLedger::default(), NOW_MS, 0),
        )
        .await
        .expect("projection accept timeout")
        .expect("projection accept path")
        .expect_err("Murm authority must not open Graphshell");
        let application = tokio::time::timeout(std::time::Duration::from_secs(10), client_task)
            .await
            .expect("projection client timeout")
            .expect("projection client task");
        assert!(
            application.is_empty(),
            "a refusal exposes no application bytes"
        );
        assert_eq!(
            refusal,
            ProjectionRefusal::NotAdmitted(DenyReason::ActionNotCovered)
        );
    }
}
