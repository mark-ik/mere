//! The C2 receipt: a real WebRTC channel carries a real browser join.
//!
//! The module tests in `graphshell::webrtc_door` fabricate their DTLS
//! fingerprints, because a pure test has no handshake to read one off. This
//! one does not. Two str0m peers over a loopback UDP pair run a real ICE and
//! DTLS handshake and open a real SCTP data channel; the link challenge binds
//! [`Carrier::fingerprints`] — the certificates actually presented, not the
//! SDP's claims — and every step after that crosses the channel as a frame:
//! the two nonces, the host's signature over the transcript, the redemption
//! proof, the minted delegation, the `SessionHello`, and the `SessionReply`.
//!
//! The offerer plays the browser. It is str0m rather than Chrome for the
//! reason `webrtc-carrier`'s own loopback suite gives — this is as close as a
//! test gets to "a headed browser opened it" without a browser — and it calls
//! exactly the client-side functions a wasm build would: [`verify_host_challenge`],
//! [`build_redemption_proof`], [`open_webrtc_session`]. Nothing here is a
//! host-side shortcut wearing a client's name.
//!
//! What the test asserts, beyond "it connected": both ends derive the same
//! shared link from their own view of the handshake, the admitted principal is
//! the subject the browser generated locally, and the action is exactly
//! `connect`.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use graphshell::admission::{CONNECT_ACTION, PROJECTION_PROTOCOL, connect_action};
use graphshell::carrier::projection_policy;
use graphshell::webrtc_door::{
    InviteTerms, RedemptionState, SignedInviteDescriptor, admit_webrtc_session,
    build_redemption_proof, issue_invite, mint_delegation, open_webrtc_session, redeem,
    sign_challenge, verify_host_challenge,
};
use notochord::{
    LocalNetworkPolicy, NetworkId, ProfileRef, RevocationLedger, SessionReply, TrustedRoot,
};
use personae::IdentityProvider;
use personae::InMemoryProvider;
use personae::delegation::SignedDelegationCertificate;
use tokio::net::UdpSocket;
use webrtc_carrier::native::str0m::change::{SdpAnswer, SdpPendingOffer};
use webrtc_carrier::native::str0m::channel::{ChannelConfig, Reliability};
use webrtc_carrier::native::str0m::{Candidate, Rtc, RtcConfig};
use webrtc_carrier::native::{Answerer, AnswererConfig, Carrier, CarrierConfig, serve};
use webrtc_carrier::{FingerprintRole, InviteV1, LinkChallenge, ReleaseRefV1};

const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const NOW_MS: u64 = 50;
const TTL_MS: u64 = 30;
const INVITE_EXPIRY_MS: u64 = 10_000;
const HELLO_NONCE: [u8; 32] = [5; 32];
const DELEGATION_NONCE: [u8; 32] = [4; 32];
const SERVER_NONCE: [u8; 32] = [0x22; 32];
const CLIENT_NONCE: [u8; 32] = [0x11; 32];

/// Long enough that a slow machine is not the thing being measured, short
/// enough that a genuinely stuck handshake fails rather than hangs.
const HANDSHAKE_BUDGET: Duration = Duration::from_secs(20);
const FRAME_BUDGET: Duration = Duration::from_secs(10);

fn host_identity() -> InMemoryProvider {
    InMemoryProvider::from_seed([1; 32])
}

fn profile() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}

fn policy() -> LocalNetworkPolicy {
    projection_policy(
        NETWORK,
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: host_identity().master_public_key().to_bytes(),
        }],
        vec![profile()],
        None,
    )
}

fn carrier_config() -> CarrierConfig {
    CarrierConfig {
        open_timeout: HANDSHAKE_BUDGET,
        idle_timeout: Duration::from_secs(10),
        ..CarrierConfig::default()
    }
}

/// A str0m offerer: bound socket, declared candidate, one data channel.
///
/// The browser's role, played by str0m through the crate's own re-export so
/// the engine is the exact version the adapter compiled against.
struct Offerer {
    rtc: Rtc,
    socket: UdpSocket,
    pending: Option<SdpPendingOffer>,
    offer: String,
}

impl Offerer {
    async fn create(label: &str) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("a loopback UDP port");
        let addr: SocketAddr = socket.local_addr().expect("a local address");

        let mut rtc = RtcConfig::new().clear_codecs().build(Instant::now());
        let candidate = Candidate::host(addr, "udp").expect("a host candidate");
        assert!(
            rtc.add_local_candidate(candidate).is_some(),
            "the loopback host candidate must be accepted, or nothing below tests anything"
        );

        let mut change = rtc.sdp_api();
        let _channel = change.add_channel_with_config(ChannelConfig {
            label: label.to_owned(),
            ordered: true,
            reliability: Reliability::Reliable,
            ..ChannelConfig::default()
        });
        let (offer, pending) = change.apply().expect("a data channel requires negotiation");
        let offer = offer.to_sdp_string();

        Self {
            rtc,
            socket,
            pending: Some(pending),
            offer,
        }
    }

    fn accept_answer(&mut self, answer: &str) {
        let answer = SdpAnswer::from_sdp_string(answer).expect("the answer parses");
        let pending = self.pending.take().expect("one answer per offer");
        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .expect("the answer applies");
    }
}

/// Runs one full handshake and returns both live carriers, `(host, browser)`.
async fn connect(config: CarrierConfig) -> (Carrier, Carrier) {
    let mut offerer = Offerer::create(&config.channel_label).await;

    let mut answerer = Answerer::bind(AnswererConfig {
        bind: "127.0.0.1:0".parse().expect("a literal address"),
        advertise: Vec::new(),
        carrier: config.clone(),
    })
    .await
    .expect("the answerer binds");

    let declared = answerer
        .local_fingerprint()
        .expect("the host's own fingerprint is sha-256");
    assert_eq!(declared.role().name(), "server");

    let answer = answerer.answer(&offerer.offer).expect("an SDP answer");
    offerer.accept_answer(&answer);

    let peer_config = CarrierConfig {
        local_dtls_role: FingerprintRole::Client,
        ..config
    };
    let (answered, offered) = tokio::join!(
        answerer.accept(),
        serve(offerer.rtc, offerer.socket, peer_config)
    );

    (
        answered.expect("the host's channel opens"),
        offered.expect("the browser's channel opens"),
    )
}

async fn next_frame(carrier: &mut Carrier) -> Vec<u8> {
    tokio::time::timeout(FRAME_BUDGET, carrier.recv_frame())
        .await
        .expect("a frame arrives within the budget")
        .expect("the session is still live")
        .expect("the stream has not ended")
}

fn as_32(bytes: &[u8]) -> [u8; 32] {
    bytes.try_into().expect("a 32-byte field")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_real_webrtc_channel_admits_an_invited_browser() {
    let host_provider = host_identity();
    let host_key = host_provider.master_public_key().to_bytes();

    // The host mints one single-use invitation. What leaves is the fragment
    // plus the descriptor's signature; what stays is the redemption state.
    let issue = issue_invite(
        &host_provider,
        &InviteTerms::projection(
            NETWORK,
            profile(),
            INVITE_EXPIRY_MS,
            1,
            ReleaseRefV1 {
                manifest_blake3: [0x5a; 32],
                publisher_key_id: [0x6b; 32],
            },
        ),
    )
    .expect("issue the invitation");

    // The URL the browser was handed, out of band. Round-tripped through the
    // fragment text on purpose: this is the path a real join takes, so a
    // change to the invite encoding fails this receipt too.
    let fragment = issue.descriptor.invite.to_fragment();
    let descriptor_signature = issue.descriptor.signature().to_vec();
    let descriptor_signer = issue.descriptor.signer.clone();
    let host_invite: InviteV1 = issue.descriptor.invite;
    let mut redemption: RedemptionState = issue.redemption;

    let config = carrier_config();
    let channel_label = config.channel_label.clone();
    let (mut host_carrier, mut browser_carrier) = connect(config).await;

    let host_fingerprints = host_carrier
        .fingerprints()
        .expect("the host observed the handshake");
    let browser_fingerprints = browser_carrier
        .fingerprints()
        .expect("the browser observed the handshake");
    assert_eq!(
        host_fingerprints, browser_fingerprints,
        "the two ends disagree about the DTLS handshake that just ran"
    );

    let host_label = channel_label.clone();
    let host_side = async move {
        // 1. nonces.
        host_carrier
            .send_frame(&SERVER_NONCE)
            .await
            .expect("the host writes its nonce");
        let client_nonce = as_32(&next_frame(&mut host_carrier).await);

        // 2. the transcript, from the certificates this handshake presented.
        let challenge = LinkChallenge::new(
            PROJECTION_PROTOCOL,
            host_label.as_str(),
            host_invite.rendezvous(),
            client_nonce,
            SERVER_NONCE,
            *host_fingerprints.client(),
            *host_fingerprints.server(),
        )
        .expect("the host's link challenge");

        // 3. the host authenticates itself, and only then does the browser
        //    have any reason to send a secret over this channel.
        let signed = sign_challenge(&host_provider, &challenge).expect("the host signs");
        host_carrier
            .send_frame(&serde_json::to_vec(&signed).expect("encode the host signature"))
            .await
            .expect("the host writes its signature");

        // 4. redemption: subject then proof, in one frame.
        let redemption_frame = next_frame(&mut host_carrier).await;
        assert_eq!(redemption_frame.len(), 96);
        let subject = as_32(&redemption_frame[..32]);
        let proof: [u8; 64] = redemption_frame[32..]
            .try_into()
            .expect("a 64-byte signature");
        redeem(&mut redemption, &challenge, &subject, &proof, NOW_MS).expect("the redemption");
        assert_eq!(
            redemption.remaining_uses(),
            0,
            "a single-use invitation is spent by its first join"
        );

        let minted = mint_delegation(
            &host_provider,
            ROOT_AUTHORITY,
            subject,
            &host_invite,
            NOW_MS,
            TTL_MS,
            DELEGATION_NONCE,
        )
        .expect("mint the leaf delegation");
        host_carrier
            .send_frame(&serde_json::to_vec(&minted).expect("encode the delegation"))
            .await
            .expect("the host writes the delegation");

        // 5. the hello arrives as one frame, and admission is sans-I/O over
        //    those bytes: the reply is written whichever way it goes.
        let hello = next_frame(&mut host_carrier).await;
        let (reply, outcome) = admit_webrtc_session(
            &policy(),
            &RevocationLedger::new(),
            &hello,
            challenge.shared_link(),
            NOW_MS,
            0,
        );
        host_carrier
            .send_frame(&reply)
            .await
            .expect("the host writes the reply");

        (outcome, subject, challenge.shared_link(), host_carrier)
    };

    let browser_label = channel_label.clone();
    let browser_side = async move {
        // The browser reads its invitation out of the fragment and puts the
        // signed descriptor back together.
        let invite = InviteV1::parse_fragment(&fragment).expect("the fragment parses");
        let descriptor =
            SignedInviteDescriptor::from_parts(invite, descriptor_signer, descriptor_signature);
        assert!(
            descriptor.verify(&host_key),
            "the descriptor must verify against the key the invitation names"
        );

        // The subject is generated here and stays here. `random()` in a real
        // browser; a seed so a failure is reproducible.
        let ephemeral = InMemoryProvider::from_seed([2; 32]);
        let subject = ephemeral.master_public_key().to_bytes();

        // 1. nonces.
        let server_nonce = as_32(&next_frame(&mut browser_carrier).await);
        browser_carrier
            .send_frame(&CLIENT_NONCE)
            .await
            .expect("the browser writes its nonce");

        let challenge = LinkChallenge::new(
            PROJECTION_PROTOCOL,
            browser_label.as_str(),
            descriptor.invite.rendezvous(),
            CLIENT_NONCE,
            server_nonce,
            *browser_fingerprints.client(),
            *browser_fingerprints.server(),
        )
        .expect("the browser's link challenge");

        // 2. verify the host before trusting the channel with the secret.
        let signed = serde_json::from_slice(&next_frame(&mut browser_carrier).await)
            .expect("decode the host signature");
        assert!(
            verify_host_challenge(descriptor.invite.expected_host_key(), &challenge, &signed),
            "the browser must refuse a channel it cannot attribute to its host"
        );

        // 3. prove the seed, bound to this transcript and this subject.
        let proof =
            build_redemption_proof(descriptor.invite.redemption_seed(), &challenge, &subject);
        let mut redemption_frame = Vec::with_capacity(96);
        redemption_frame.extend_from_slice(&subject);
        redemption_frame.extend_from_slice(&proof);
        browser_carrier
            .send_frame(&redemption_frame)
            .await
            .expect("the browser writes its redemption");

        // 4. the delegation comes back, and the browser opens a session with
        //    it over the link both ends derived.
        let minted: SignedDelegationCertificate =
            serde_json::from_slice(&next_frame(&mut browser_carrier).await)
                .expect("decode the delegation");
        assert_eq!(minted.certificate.subject, subject);

        let hello = open_webrtc_session(
            &ephemeral,
            NETWORK,
            profile(),
            HELLO_NONCE,
            challenge.shared_link(),
            vec![minted],
        )
        .expect("issue the hello")
        .encode(&policy().limits.clamped())
        .expect("encode the hello");
        browser_carrier
            .send_frame(&hello)
            .await
            .expect("the browser writes its hello");

        let reply = SessionReply::decode(
            &next_frame(&mut browser_carrier).await,
            &policy().limits.clamped(),
        )
        .expect("a well-formed reply");

        (reply, subject, challenge.shared_link(), browser_carrier)
    };

    let (
        (outcome, host_subject, host_link, host_carrier),
        (reply, subject, browser_link, browser_carrier),
    ) = tokio::join!(host_side, browser_side);

    assert_eq!(
        host_link, browser_link,
        "both ends must derive one link from their own view of the handshake"
    );
    assert_eq!(host_subject, subject);

    let principal = outcome.expect("a real invited browser must be admitted");
    assert_eq!(
        principal.subject, subject,
        "the admitted principal is the subject the browser generated locally"
    );
    assert_eq!(principal.action, connect_action());
    assert_eq!(principal.action.action, CONNECT_ACTION);
    assert!(reply.is_accept(), "the browser sees its own admission");

    host_carrier.close().await.expect("the host closes cleanly");
    browser_carrier
        .close()
        .await
        .expect("the browser closes cleanly");
}
