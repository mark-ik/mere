// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! C4a's composition receipt: the whole native stack over real DTLS.
//!
//! `webrtc_session`'s own tests prove the join sequence over memory frames;
//! the carrier's suite proves frames and the pump over DTLS. This test is the
//! composition neither can claim: one real loopback WebRTC pair — real ICE,
//! real DTLS, real SCTP — carrying the join, Notochord admission, the frame
//! pump, and a Graphshell session served by `serve_admitted_session` to the
//! event-driven `SessionDriver`. Everything the C4a fixture binary does,
//! minus HTTP signaling and a browser.

use std::sync::RwLock;
use std::time::Duration;

use graphshell::carrier::projection_policy;
use graphshell::lifecycle::SessionAuthority;
use graphshell::resume::ResumeFixtureEndpoint;
use graphshell::session_loop::serve_admitted_session;
use graphshell::live_endpoint::{self, LiveEndpoint};
use graphshell::native::endpoint_catalog::{ResidentEndpointCatalog, ResidentEndpointRoute};
use graphshell::native::projection_host::ResidentProjectionHost;
use graphshell::webrtc_door::{InviteTerms, issue_invite};
use graphshell::webrtc_session::{HostedInvite, peer_join, serve_webrtc_join};
use graphshell_client::{Advance, Outcome, SessionDriver};
use graphshell_endpoint::ResumableProjectionSource;
use chirograph::IntentResult;
use notochord::{NetworkId, ProfileRef, RevocationLedger, TrustedRoot};
use sceno::InstanceId;
use personae::{IdentityProvider, InMemoryProvider};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use webrtc_carrier::ReleaseRefV1;
use webrtc_carrier::native::{CarrierConfig, loopback_pair, stream_over_frames};

const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const NOW_MS: u64 = 50;
const TTL_MS: u64 = 10_000;
const INVITE_EXPIRY_MS: u64 = 20_000;

fn profile() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}

/// Drive one operation of the event-driven adapter over NDJSON lines.
async fn drive<R, W>(
    driver: &mut SessionDriver,
    write: &mut W,
    lines: &mut tokio::io::Lines<BufReader<R>>,
    start: Advance,
) -> Result<Outcome, String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut advance = start;
    loop {
        match advance {
            Advance::Done(outcome) => return Ok(outcome),
            Advance::Noted | Advance::Send(_) => {
                if let Advance::Send(line) = &advance {
                    write
                        .write_all(line.as_bytes())
                        .await
                        .map_err(|error| error.to_string())?;
                    write
                        .write_all(b"\n")
                        .await
                        .map_err(|error| error.to_string())?;
                }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_browser_shaped_peer_joins_and_is_served_over_real_dtls() {
    let outcome = tokio::time::timeout(Duration::from_secs(30), async {
        // One real WebRTC pair. The answerer half is the host fixture's
        // carrier; the offerer half plays the browser.
        let (host_carrier, mut peer_carrier) = loopback_pair(CarrierConfig::default()).await;

        // The invitation, split the way a real join splits it.
        let host_provider = InMemoryProvider::from_seed([1; 32]);
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
        .expect("the invitation issues");
        let fragment = issue.descriptor.invite.to_fragment();
        let peer_invite =
            webrtc_carrier::InviteV1::parse_fragment(&fragment).expect("the fragment parses");
        let mut hosted = HostedInvite {
            invite: issue.descriptor.invite,
            redemption: issue.redemption,
        };

        let policy = projection_policy(
            NETWORK,
            vec![TrustedRoot {
                authority: ROOT_AUTHORITY,
                issuer: host_provider.master_public_key().to_bytes(),
            }],
            vec![profile()],
            None,
        );
        let ledger = RevocationLedger::new();
        let limits = policy.limits.clamped();

        // Host side: join over the live carrier, then serve the admitted
        // session — the exact composition the fixture binary runs.
        let host_policy = policy.clone();
        let host = async move {
            let mut served = serve_webrtc_join(
                host_carrier,
                &host_provider,
                &mut hosted,
                &host_policy,
                &ledger,
                ROOT_AUTHORITY,
                TTL_MS,
                NOW_MS,
                0,
            )
            .await
            .expect("the join admits over real DTLS");
            let admitted = &mut served.session;

            let authority = SessionAuthority::retain_admitted(&admitted);
            let revocations = RwLock::new(RevocationLedger::new());
            let mut endpoint = ResumeFixtureEndpoint::new();
            let mut resume = |endpoint: &mut ResumeFixtureEndpoint, request| {
                ResumableProjectionSource::resume(endpoint, request)
                    .map_err(|error| error.to_string())
            };
            let summary = serve_admitted_session(
                admitted,
                &authority,
                &revocations,
                &mut endpoint,
                &mut resume,
                || NOW_MS,
            )
            .await
            .expect("the serve loop ends without a session error");
            // The polite ending: flush the final answer before the carrier
            // goes. Dropping `served` here instead is the bug this receipt
            // caught — the driver cancels under the close reply.
            served
                .finish()
                .await
                .expect("the session flushes and closes cleanly");
            summary
        };

        // Peer side: join, then drive the session the way the browser will —
        // the event-driven adapter over the pumped byte stream.
        let peer = async {
            let fingerprints = peer_carrier
                .fingerprints()
                .expect("the peer observed the handshake");
            let label = peer_carrier.channel_label().to_string();
            let joined = tokio::time::timeout(
                Duration::from_secs(5),
                peer_join(
                    &mut peer_carrier,
                    &InMemoryProvider::from_seed([9; 32]),
                    &peer_invite,
                    &label,
                    *fingerprints.client(),
                    *fingerprints.server(),
                    &limits,
                ),
            )
            .await
            .expect("PHASE join timed out")
            .expect("the peer is admitted over real DTLS");

            let (reader, writer, _control) = peer_carrier.into_parts();
            let (stream, _pump) = stream_over_frames(reader, writer);
            let (read, mut write) = tokio::io::split(stream);
            let mut lines = BufReader::new(read).lines();
            let mut driver = SessionDriver::new(chirograph::CapabilityProfile::default());

            let start = driver.discover().expect("discovery starts");
            let discovered = tokio::time::timeout(
                Duration::from_secs(5),
                drive(&mut driver, &mut write, &mut lines, start),
            )
            .await
            .expect("PHASE discovery timed out")
            .expect("discovery completes");
            let Outcome::Descriptor(descriptor) = discovered else {
                panic!("expected a descriptor, got {discovered:?}");
            };
            assert_eq!(descriptor.projections.len(), 1);

            let start = driver
                .core_mut()
                .expect("discovered")
                .mount(0)
                .expect("offer 0 exists");
            let start = driver.begin(start).expect("mount encodes");
            let mounted = tokio::time::timeout(
                Duration::from_secs(5),
                drive(&mut driver, &mut write, &mut lines, start),
            )
            .await
            .expect("PHASE mount timed out")
            .expect("the mount completes");
            let Outcome::Mounted(session) = mounted else {
                panic!("expected a mount, got {mounted:?}");
            };
            assert!(
                driver
                    .core()
                    .and_then(|core| core.client().mounted(&session))
                    .is_some(),
                "the driver's client state holds the native-owned scene"
            );

            let start = driver.core_mut().expect("core").close();
            let start = driver.begin(start).expect("close encodes");
            let closed = tokio::time::timeout(
                Duration::from_secs(5),
                drive(&mut driver, &mut write, &mut lines, start),
            )
            .await
            .expect("PHASE close timed out")
            .expect("the close completes");
            assert!(matches!(closed, Outcome::Closed));
            joined
        };

        let (summary, joined) = tokio::join!(host, peer);
        assert_eq!(
            summary.answered, 3,
            "discover, snapshot, close — and nothing else"
        );
        assert_eq!(joined.shared_link.len(), 16);
    })
    .await;
    outcome.expect("the whole composition completes inside its budget");
}

/// The C4a done-conditions, over one real WebRTC pair and the product host.
///
/// The test above proves the plumbing composes. This proves the *claims*: a
/// browser-shaped peer joins, the resident host serves it through the same
/// catalog route a dialled peer gets, and the peer invokes one admitted intent
/// and one refused one — with the native revision moving exactly once.
///
/// Nothing here is a fixture standing in for the product path.
/// `serve_admitted` is the entry a pre-admitted carrier uses, and the endpoint
/// comes from `ResidentEndpointCatalog` exactly as it would for a peer that
/// dialled a transport.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_admitted_intent_moves_the_revision_and_the_refused_one_does_not() {
    let outcome = tokio::time::timeout(Duration::from_secs(30), async {
        let (host_carrier, mut peer_carrier) = loopback_pair(CarrierConfig::default()).await;

        let host_provider = InMemoryProvider::from_seed([1; 32]);
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
        .expect("the invitation issues");
        let fragment = issue.descriptor.invite.to_fragment();
        let peer_invite =
            webrtc_carrier::InviteV1::parse_fragment(&fragment).expect("the fragment parses");
        let mut hosted = HostedInvite {
            invite: issue.descriptor.invite,
            redemption: issue.redemption,
        };

        let policy = projection_policy(
            NETWORK,
            vec![TrustedRoot {
                authority: ROOT_AUTHORITY,
                issuer: host_provider.master_public_key().to_bytes(),
            }],
            vec![profile()],
            None,
        );
        let ledger = RevocationLedger::new();
        let limits = policy.limits.clamped();

        // The product host, with the live endpoint on a catalog route. The
        // revision this test watches lives inside the endpoint the catalog
        // builds, so it is read back over the wire rather than from a handle.
        let mut catalog = ResidentEndpointCatalog::new();
        catalog
            .register_notifying("live", "C4 live board", |_context| Ok(LiveEndpoint::new()))
            .expect("the route registers");
        let route = ResidentEndpointRoute::new("live", Duration::from_millis(50))
            .expect("a valid route");
        let mut host = ResidentProjectionHost::new(policy.clone(), route, catalog);

        let host_side = async {
            let served = serve_webrtc_join(
                host_carrier,
                &host_provider,
                &mut hosted,
                &policy,
                &ledger,
                ROOT_AUTHORITY,
                TTL_MS,
                NOW_MS,
                0,
            )
            .await
            .expect("the join admits over real DTLS");

            // Pre-admitted: the door decided, the product host serves.
            let projection = host
                .serve_admitted(served.session, || NOW_MS)
                .expect("the resident host serves a pre-admitted session");
            assert_eq!(host.live_sessions(), 1, "the session is counted live");
            (projection, served.pump, served.control)
        };

        let peer_side = async {
            let fingerprints = peer_carrier
                .fingerprints()
                .expect("the peer observed the handshake");
            let label = peer_carrier.channel_label().to_string();
            peer_join(
                &mut peer_carrier,
                &InMemoryProvider::from_seed([9; 32]),
                &peer_invite,
                &label,
                *fingerprints.client(),
                *fingerprints.server(),
                &limits,
            )
            .await
            .expect("the peer is admitted over real DTLS");

            let (reader, writer, peer_control) = peer_carrier.into_parts();
            let (stream, pump) = stream_over_frames(reader, writer);
            // Returned, not dropped: `CarrierControl` cancels its driver on
            // drop, so letting it fall here would kill the peer's own carrier
            // the instant this block ended.
            (stream, pump, peer_control)
        };

        let ((projection, host_pump, control), (stream, peer_pump, peer_control)) =
            tokio::join!(host_side, peer_side);

        let (read, mut write) = tokio::io::split(stream);
        let mut lines = BufReader::new(read).lines();
        // The profile is the negotiation, not a formality: an empty one
        // supports no offer, so every advertised action reads as unadvertised
        // and `invoke` refuses before anything reaches the wire. Declaring
        // what this peer can actually present is what makes the endpoint's
        // offers selectable.
        let mut driver = SessionDriver::new(chirograph::CapabilityProfile::new([
            chirograph::PresentationCapability::NativeGlyph,
        ]));

        // Discover and mount, so the client holds a real acknowledgement to
        // invoke against.
        let start = driver.discover().expect("discovery starts");
        drive(&mut driver, &mut write, &mut lines, start)
            .await
            .expect("discovery completes");
        let start = driver
            .core_mut()
            .expect("discovered")
            .mount(0)
            .expect("offer 0 exists");
        let start = driver.begin(start).expect("mount encodes");
        let mounted = drive(&mut driver, &mut write, &mut lines, start)
            .await
            .expect("the mount completes");
        let Outcome::Mounted(session) = mounted else {
            panic!("expected a mount, got {mounted:?}");
        };

        let revision_after_mount = driver
            .core()
            .and_then(|core| core.client().acknowledgement(&session))
            .expect("the client acknowledged the snapshot")
            .revision;

        // The action to invoke, taken from what the endpoint advertised —
        // never hand-built, so the test can only invoke what a peer can see.
        let action_for = |driver: &SessionDriver, intent: &str| {
            driver
                .core()
                .and_then(|core| core.client().mounted(&session))
                .and_then(|mounted| mounted.presentation.offers_for(InstanceId(0)))
                .and_then(|offers| offers.first())
                .and_then(|offer| {
                    offer
                        .semantics
                        .actions
                        .iter()
                        .find(|action| action.intent.0 == intent)
                        .cloned()
                })
                .unwrap_or_else(|| panic!("the endpoint advertises {intent}"))
        };

        // The refused intent first, so any revision movement afterwards
        // cannot be blamed on ordering.
        let refused = action_for(&driver, live_endpoint::REFUSED_INTENT);
        let start = driver
            .core_mut()
            .expect("core")
            .invoke(&session, InstanceId(0), &refused, &())
            .expect("the refused action is advertised and composes");
        let start = driver.begin(start).expect("intent encodes");
        let result = drive(&mut driver, &mut write, &mut lines, start)
            .await
            .expect("the endpoint answers");
        let Outcome::Intent(result) = result else {
            panic!("expected an intent result, got {result:?}");
        };
        assert!(
            matches!(*result, IntentResult::Rejected { .. }),
            "the advertised-but-forbidden action is refused: {result:?}"
        );

        // Resnapshot to read the native position back over the wire.
        let start = driver
            .core_mut()
            .expect("core")
            .resnapshot(&session)
            .expect("the session is mounted");
        let start = driver.begin(start).expect("snapshot encodes");
        drive(&mut driver, &mut write, &mut lines, start)
            .await
            .expect("the resnapshot completes");
        let after_refusal = driver
            .core()
            .and_then(|core| core.client().acknowledgement(&session))
            .expect("acknowledged")
            .revision;
        assert_eq!(
            after_refusal, revision_after_mount,
            "a refused intent must leave the native revision exactly where it was"
        );

        // Now the admitted one.
        let admitted = action_for(&driver, live_endpoint::ADMITTED_INTENT);
        let start = driver
            .core_mut()
            .expect("core")
            .invoke(&session, InstanceId(0), &admitted, &())
            .expect("the admitted action composes");
        let start = driver.begin(start).expect("intent encodes");
        let result = drive(&mut driver, &mut write, &mut lines, start)
            .await
            .expect("the endpoint answers");
        let Outcome::Intent(result) = result else {
            panic!("expected an intent result, got {result:?}");
        };
        assert_eq!(*result, IntentResult::Accepted);

        let start = driver
            .core_mut()
            .expect("core")
            .resnapshot(&session)
            .expect("the session is mounted");
        let start = driver.begin(start).expect("snapshot encodes");
        drive(&mut driver, &mut write, &mut lines, start)
            .await
            .expect("the resnapshot completes");
        let after_admission = driver
            .core()
            .and_then(|core| core.client().acknowledgement(&session))
            .expect("acknowledged")
            .revision;
        assert_eq!(
            after_admission.0,
            revision_after_mount.0 + 1,
            "the admitted intent moved the native revision exactly once"
        );

        // End politely, in the order `ServedJoin::finish` documents.
        let start = driver.core_mut().expect("core").close();
        let start = driver.begin(start).expect("close encodes");
        let closed = drive(&mut driver, &mut write, &mut lines, start)
            .await
            .expect("the close completes");
        assert!(matches!(closed, Outcome::Closed));

        drop(write);
        drop(lines);
        let _ = peer_pump.await;
        let summary = projection
            .finished()
            .await
            .expect("the served projection task does not panic")
            .expect("the serve loop ends without a session error");
        assert!(
            summary.answered >= 6,
            "discover, snapshot, intent, snapshot, intent, snapshot, close — got {}",
            summary.answered
        );
        let _ = host_pump.await;
        let _ = control.close().await;
        drop(peer_control);
    })
    .await;
    outcome.expect("the whole composition completes inside its budget");
}
