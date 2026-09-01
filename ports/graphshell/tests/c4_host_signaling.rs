// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The fixture binary, driven through its own HTTP signaling.
//!
//! `webrtc_join_loopback` composes the same stack in-process, which proves the
//! pieces fit. It cannot prove the *binary* works: that its argument parsing
//! reaches the carrier, that the invitation it prints is the one it will
//! redeem, that an offer posted over HTTP produces an answer the peer can
//! apply, and that a second visitor is served rather than queued behind the
//! first.
//!
//! So this test spawns the real binary, fetches the real fragment over HTTP,
//! and joins as a str0m peer through `POST /offer` — the exact path a browser
//! takes, minus the browser. Everything it exercises is a thing a headed run
//! would otherwise be the first to discover.

use std::process::Stdio;
use std::time::Duration;

use graphshell::webrtc_session::peer_join;
use graphshell_client::{Advance, Outcome, SessionDriver};
use notochord::HandshakeLimits;
use personae::InMemoryProvider;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use webrtc_carrier::InviteV1;
use webrtc_carrier::native::{CarrierConfig, LoopbackOfferer, serve, stream_over_frames};

/// One signaling port per test, because cargo runs them in parallel and a
/// second fixture on a bound port exits instead of serving. Found exactly that
/// way: the first test passed and the second reported "the fixture exited
/// before it was ready", which is a port collision wearing a startup failure's
/// clothes.
const PORT_SERVES: u16 = 8791;
const PORT_REFUSES: u16 = 8792;

/// The fixture under test, killed when this guard drops.
struct Fixture(Child);

impl Drop for Fixture {
    fn drop(&mut self) {
        // `start_kill` rather than an awaited kill: `Drop` cannot await, and a
        // fixture that outlived its test would hold the signaling port against
        // the next run.
        let _ = self.0.start_kill();
    }
}

/// Start the binary and wait until it says it is ready.
///
/// Reads its stdout rather than polling `/health`, because "the port answers"
/// and "the fixture finished starting" are different claims and only the
/// second one is safe to build a test on.
async fn start_fixture(port: u16) -> (Fixture, String) {
    let exe = env!("CARGO_BIN_EXE_c4_webrtc_host");
    let mut child = Command::new(exe)
        .args([
            "--bind",
            "127.0.0.1",
            "--advertise",
            "127.0.0.1",
            "--signal-port",
            &port.to_string(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the fixture binary starts");

    let stdout = child.stdout.take().expect("piped stdout");
    let mut lines = BufReader::new(stdout).lines();
    let mut fragment = String::new();
    let ready = tokio::time::timeout(Duration::from_secs(20), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if line.starts_with("READY") {
                return true;
            }
            if line.starts_with("mwi1.") {
                fragment = line;
            }
        }
        false
    })
    .await
    .expect("the fixture becomes ready inside its budget");
    assert!(ready, "the fixture exited before it was ready");
    assert!(
        !fragment.is_empty(),
        "the fixture must print an invite fragment"
    );

    // Keep draining, or the child blocks on a full pipe mid-session.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    (Fixture(child), fragment)
}

/// One HTTP request over a bare TCP socket, so the test carries no HTTP client.
async fn http(port: u16, method: &str, path: &str, body: &str) -> Result<String, String> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|error| format!("connect: {error}"))?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: text/plain\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| format!("write: {error}"))?;
    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .map_err(|error| format!("read: {error}"))?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("malformed response: {response:?}"))?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains("200") && !status.contains("204") {
        return Err(format!("{status}: {body}"));
    }
    Ok(body.to_string())
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

/// Join through the fixture's signaling and mount its projection.
///
/// Returns the number of cards the mounted scene holds, which is the fixture's
/// revision made observable: one card at revision 1, one more per admitted
/// intent.
async fn join_and_count(port: u16, fragment: &str) -> usize {
    let invite = InviteV1::parse_fragment(fragment).expect("the printed fragment parses");

    // The browser's role, played by str0m over the real signaling.
    let config = CarrierConfig::default();
    let mut offerer = LoopbackOfferer::create(&config.channel_label).await;
    let answer = http(port, "POST", "/offer", &offerer.offer)
        .await
        .expect("the fixture answers the offer");
    offerer.accept_answer(&answer);

    let peer_config = CarrierConfig {
        local_dtls_role: webrtc_carrier::FingerprintRole::Client,
        ..config
    };
    let mut carrier = serve(offerer.rtc, offerer.socket, peer_config)
        .await
        .expect("the peer's channel opens");

    let fingerprints = carrier.fingerprints().expect("the handshake was observed");
    let label = carrier.channel_label().to_string();
    peer_join(
        &mut carrier,
        &InMemoryProvider::from_seed([9; 32]),
        &invite,
        &label,
        *fingerprints.client(),
        *fingerprints.server(),
        &HandshakeLimits::default().clamped(),
    )
    .await
    .expect("the peer is admitted through the fixture");

    let (reader, writer, control) = carrier.into_parts();
    let (stream, pump) = stream_over_frames(reader, writer);
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    let mut driver = SessionDriver::new(chirograph::CapabilityProfile::new([
        chirograph::PresentationCapability::NativeGlyph,
    ]));

    let start = driver.discover().expect("discovery starts");
    drive(&mut driver, &mut write, &mut lines, start)
        .await
        .expect("discovery completes");
    let start = driver
        .core_mut()
        .expect("discovered")
        .mount(0)
        .expect("the fixture offers one projection");
    let start = driver.begin(start).expect("mount encodes");
    let mounted = drive(&mut driver, &mut write, &mut lines, start)
        .await
        .expect("the mount completes");
    let Outcome::Mounted(session) = mounted else {
        panic!("expected a mount, got {mounted:?}");
    };

    let cards = driver
        .core()
        .and_then(|core| core.client().mounted(&session))
        .expect("the scene is mounted")
        .scene
        .active_items_in_order()
        .len();

    // End politely, in the order `ServedJoin::finish` documents.
    let start = driver.core_mut().expect("core").close();
    let start = driver.begin(start).expect("close encodes");
    let _ = drive(&mut driver, &mut write, &mut lines, start).await;
    drop(write);
    drop(lines);
    let _ = pump.await;
    let _ = control.close().await;
    cards
}

/// The fixture answers a real offer and serves a real projection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_fixture_binary_serves_a_projection_over_its_own_signaling() {
    let outcome = tokio::time::timeout(Duration::from_secs(60), async {
        let (_fixture, fragment) = start_fixture(PORT_SERVES).await;

        assert_eq!(
            http(PORT_SERVES, "GET", "/health", "").await.expect("health answers"),
            "ok"
        );
        assert_eq!(
            http(PORT_SERVES, "GET", "/invite", "").await.expect("invite answers"),
            fragment,
            "the fragment served over HTTP is the one the fixture printed"
        );

        let cards = join_and_count(PORT_SERVES, &fragment).await;
        assert_eq!(cards, 1, "the live endpoint starts at one card");

        // A second visitor, on the same invitation's remaining uses. The
        // resident host serves sessions concurrently rather than queueing
        // them, and a fixture that could only ever admit one peer would hide
        // that.
        let cards = join_and_count(PORT_SERVES, &fragment).await;
        assert_eq!(cards, 1, "a second visitor is served its own session");
    })
    .await;
    outcome.expect("the fixture receipt completes inside its budget");
}

/// A fragment the fixture did not issue is refused.
///
/// The positive control is the test above: the same code path with the real
/// fragment is admitted, so this refusal is about the invitation rather than
/// about the fixture being unreachable.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invitation_this_host_never_issued_is_refused() {
    let outcome = tokio::time::timeout(Duration::from_secs(60), async {
        let (_fixture, _fragment) = start_fixture(PORT_REFUSES).await;

        // A well-formed invitation from a different host entirely.
        let other = InMemoryProvider::from_seed([42; 32]);
        let issue = graphshell::webrtc_door::issue_invite(
            &other,
            &graphshell::webrtc_door::InviteTerms::projection(
                notochord::NetworkId([3; 32]),
                notochord::ProfileRef {
                    id: "mere.base".into(),
                    revision: 1,
                },
                u64::MAX / 2,
                1,
                webrtc_carrier::ReleaseRefV1 {
                    manifest_blake3: [0x11; 32],
                    publisher_key_id: [0x22; 32],
                },
            ),
        )
        .expect("the other host issues an invitation");
        let invite = issue.descriptor.invite;

        let config = CarrierConfig::default();
        let mut offerer = LoopbackOfferer::create(&config.channel_label).await;
        let answer = http(PORT_REFUSES, "POST", "/offer", &offerer.offer)
            .await
            .expect("the fixture answers any well-formed offer");
        offerer.accept_answer(&answer);
        let peer_config = CarrierConfig {
            local_dtls_role: webrtc_carrier::FingerprintRole::Client,
            ..config
        };
        let mut carrier = serve(offerer.rtc, offerer.socket, peer_config)
            .await
            .expect("the peer's channel opens");
        let fingerprints = carrier.fingerprints().expect("the handshake was observed");
        let label = carrier.channel_label().to_string();

        let refused = peer_join(
            &mut carrier,
            &InMemoryProvider::from_seed([9; 32]),
            &invite,
            &label,
            *fingerprints.client(),
            *fingerprints.server(),
            &HandshakeLimits::default().clamped(),
        )
        .await
        .expect_err("an invitation this host never issued cannot be redeemed");
        assert!(
            refused.to_string().contains("unknown invitation"),
            "the peer is told which check refused it: {refused}"
        );
    })
    .await;
    outcome.expect("the refusal receipt completes inside its budget");
}
