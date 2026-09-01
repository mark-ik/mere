# Browser WebRTC Carrier Plan

**Date:** 2026-08-25
**Status:** in progress. C0-C2 landed 2026-08-26. C3 landed 2026-08-28: the
forced relay is physically proven over a TURN relay on a second machine, so the
stop line is CLEARED. The `reconnect` defect is fixed and headed-verified
2026-08-28; the snapshot resume branch stays carried. C4 is the next phase.
**Scope:** Let an ordinary browser join a bounded live Graphshell session whose
native application retains state and authority. Build the carrier and admission
proof before adding public rendezvous infrastructure.

**Related:**

- [browser-to-native WebRTC feasibility](../research/2026-08-25_browser_native_webrtc_carrier_probe.md)
- [Graphshell remote projection host](2026-07-22_graphshell_remote_projection_host_plan.md)
- [Graphshell reference host](2026-07-27_graphshell_reference_host_plan.md)
- [reachability rungs and privacy lanes](2026-08-03_reachability_rungs_and_privacy_lanes_plan.md)
- [net-media plan](2026-05-26_net_media_plan.md)
- [Djinn family resident services](2026-08-22_djinn_family_resident_services_plan.md)
- [auto-update brief](../../2026-07-22_auto-update_brief.md)
- [Luggage current API](../../../crates/system/luggage/README.md)

## 1. Ruling

Build a direct browser-to-native WebRTC data-channel carrier. Keep these layers
distinct:

1. `mer3ly.net` serves a versioned client and relays opaque signaling.
2. WebRTC supplies an encrypted, ordered, reliable peer data channel, using
   TURN only when direct connectivity fails.
3. A carrier challenge authenticates the native host and derives Notochord's
   connection-specific `shared_link`.
4. The browser redeems the private capability into a narrow delegation for a
   locally generated ephemeral Personae subject.
5. Notochord admits that subject before the application receives a byte.
6. Graphshell carries projections, diffs, resources, and admitted intents.
7. The native application remains authoritative for state and action.

The Iroh custom-transport donor is superseded as a candidate: iroh 1.1 ships
first-party browser support that needs no port and carries no version skew
(Findings, 2026-08-26). That path is the **second lane** of §9, for networks
where WebRTC is blocked. Neither is the browser join path.

## 2. Boundaries

```text
Cloudflare loader/signaling/TURN
          |
WebRTC offer/answer and ICE
          |
webrtc-carrier
  |- bounded data-channel frames
  |- host challenge + DTLS fingerprint binding
  `- honest AcceptedSession facts
          |
Notochord SessionHello / SessionReply
          |
Graphshell session protocol
          |
product endpoint and native authority
```

The following rules hold through every phase:

- The WebRTC carrier reports no authenticated initiator. Notochord establishes
  the ephemeral or persistent Personae subject.
- Signaling data never becomes a carrier fact merely because a Worker relayed
  it.
- `InviteV1` grants only a named service action, has a bounded expiry, and does
  not alter executable trust.
- Its Luggage release reference is a claim until a separately trusted publisher
  key verifies the exact signed manifest. Neither the host's Personae key nor a
  feed location grants that trust.
- The ordinary first visit still trusts the `mer3ly.net` HTTPS origin to deliver
  the bootstrap verifier honestly. Luggage constrains later manifest and
  artifact substitution; it does not erase initial web-origin trust.
- Joining never installs a release or changes publisher, feed, channel, or
  update policy.
- A refused stream never reaches Graphshell.
- Browser and native share wire types and test vectors, not one runtime.
- The current bilateral `Transport` trait remains intact until a second
  listener-only carrier proves a shared acceptor trait useful.
- Application state, graph authority, and intent policy stay in the native
  product endpoint.

## 3. C0: accepted-stream seam and carrier core

Create one Wasm-clean `webrtc-carrier` crate under the Murm transport family.
Its default core contains only bounded frame types, invitation identifiers,
the link-challenge transcript, fingerprint canonicalization, and shared-link
derivation. Native and browser runtimes are feature-selected adapters.

In `mere-transport`:

- add `TransportKind::WebRtc` and an `IngressContext::webrtc(shared_link)`
  constructor;
- map it honestly to `notochord::CarrierKind::Other` until a distinct public
  Notochord variant has a policy consumer;
- keep `peer: None` on accepted browser sessions.

In Graphshell, extract an accepted-stream function from
`accept_projection_session`. It receives `AcceptedSession<S>`, maps its facts
through the existing audited adapter, runs Notochord, checks the served action,
and returns `AdmittedSession<S>`. The existing `Transport` wrapper calls it
after `transport.accept`. The WebRTC listener calls it directly.

**Done when:**

- the core builds for native and `wasm32-unknown-unknown`;
- role-tagged fingerprint and shared-link vectors match on both targets;
- existing Memory, P2panda, Reticulum, and Noise mappings remain exhaustive;
- existing Graphshell carrier tests pass through the extracted function;
- a WebRTC acceptance fixture reaches the same function with `peer: None` and
  a nonempty shared link.

**Landed 2026-08-26.** All five conditions met; see Findings and Progress.

## 4. C1: direct data channel

Add a native answerer and browser initiator around the shared core. Use an
ordered, reliable data channel. Define explicit maximum frame and queued-byte
limits. The browser driver must stop sending above its high-water mark and
resume only after `bufferedAmount` crosses a configurable low-water mark. The
native stream adapter must propagate close, cancellation, and write failure.

Use local copy/paste or a tiny loopback signaling fixture for this phase. Public
infrastructure is deliberately absent so transport behaviour can be inspected
without mixing it with service deployment.

Record the exact native WebRTC dependency, feature graph, native binary delta,
browser Wasm delta, and why that version was selected. Compare current
`webrtc-rs` with `str0m` at this gate. A version choice is an evidence result,
not a permanent product rule.

**Done when:**

- a headed ordinary browser opens, exchanges bounded ping frames, and closes
  cleanly against a native process;
- oversize frames are rejected before allocation or deserialization;
- a sustained transfer observes the configured high and low water marks;
- browser refresh and native cancellation terminate both tasks without a hot
  loop;
- the receipt records direct ICE candidate selection and measured artifact
  deltas.

## 5. C2: invitation and Notochord admission

Define `InviteV1` as a versioned, bounded fragment payload containing:

- public rendezvous id;
- random redemption secret;
- expected native host public key;
- network and profile references;
- permitted service action;
- expiry and one configurable session-use ceiling;
- `ReleaseRefV1 { manifest_blake3, publisher_key_id }` from Luggage.

The release reference identifies the exact signed manifest bytes and the
publisher-key identity used to look up trust. It carries neither the manifest,
public key, nor feed. The host-signed invite descriptor binds the reference so
signaling cannot substitute a different release claim. Publisher verification
remains a separate loader decision and never contributes to Notochord
admission.

The browser imports the invitation into memory and clears the fragment from
visible history before loading any optional resource, then generates an
ephemeral Personae subject locally. The host challenge binds the protocol,
data-channel label, invite id, fresh client and server nonces, and role-tagged
SHA-256 DTLS fingerprints. The host signs that transcript. Both sides derive
Notochord's 16-byte `shared_link` from it.

After verifying the host, the browser proves possession of the redemption
secret, bound to the challenge transcript and its ephemeral public subject.
The host keeps only the redemption verifier and use state, then issues the
subject a short-lived delegation for the permitted action. It never learns the
browser's private subject key. The browser uses the returned delegation and the
existing Personae/Notochord sans-I/O path to issue `SessionHello`. The host
supplies `SessionFacts` from its accepted channel and runs the current
Graphshell admission function. It writes a refusal or accepted reply before
Graphshell opens.

**Done when:**

- a valid invite admits the browser-generated ephemeral subject and exact
  Graphshell action without disclosing the subject's private key to the host;
- a one-use redemption cannot mint a second delegation;
- altered action, expired certificate, revoked delegation, wrong host key,
  substituted SDP fingerprint, and oversized invite all fail closed;
- a captured hello replayed on a second WebRTC connection fails because the
  host challenge and shared link differ;
- a refused session cannot send `SessionOpen` successfully;
- browser logs, server logs, referrers, and signaling records contain neither
  the fragment nor its redemption secret;
- altering the bound release reference is rejected, while an unknown publisher
  remains joinable only through a compatible already-trusted generic client.

**Landed 2026-08-26** except the headed browser-log/referrer/signaling hygiene
check, which needs a real browser and travels with C4's client work; the
structural half (redacted `Debug`, no secret in any log path, `#`-tolerant
fragment parsing) is in. See Findings and Progress.

## 6. C3: forced relay and reconnect

Add configurable STUN/TURN servers and credential expiry. First run against a
controlled TURN service where direct candidates can be disabled. Then run the
same receipt against Cloudflare TURN using credentials minted server-side from
the long-term key. The long-term key never reaches the browser.

Exercise ICE restart before short-lived TURN credentials expire. Treat a new
DTLS connection as a new carrier link and run the host challenge plus Notochord
admission again. Graphshell may resume from its last acknowledged revision only
after that fresh admission.

**Done when:**

- the selected candidate pair is demonstrably relay-only in the forced case;
- the TURN service carries encrypted WebRTC packets but never receives a
  Graphshell key or application capability;
- credential expiry causes refresh or a clean failure, not an infinite retry;
- direct loss followed by relay recovery runs fresh admission and resumes by
  diff when the retained revision is valid, otherwise by snapshot;
- the same test matrix passes with direct connectivity restored.

**Stop line:** do not begin product or public-service wiring if C0 through C3
cannot demonstrate bounded backpressure, connection-bound admission, and a
forced relay. Revisit the native WebRTC stack or move the runtime adapter to
`str0m` while preserving the core and receipts.

## 7. C4: real Graphshell session

Replace the ping protocol with the current Graphshell web client and native
projection host. The browser requests one disclosed score, receives a snapshot,
applies a diff, disconnects, and resumes. It invokes one permitted intent and
receives one policy refusal for a second intent.

The browser rendering surface should use the shared Cambium web host. This
phase does not duplicate Graphshell's protocol or create an iframe-only app
model. The same client component must be usable as the full application and as
an embed.

**Done when:**

- a headed browser renders a native-owned projection through the real
  Graphshell protocol;
- snapshot, diff, reconnect, admitted intent, and refused intent are present in
  one machine-readable receipt;
- the native state revision changes only for the admitted intent;
- keyboard and accessibility-tree checks pass in the embedded and full-page
  surfaces;
- the browser carrier profile in the remote projection plan can be marked
  physically proven.

### C4a / C4b, and how the browser drives the protocol

C4 splits (ruled 2026-08-31). **C4a** earns the first three done-conditions: a
headed browser rendering a native-owned projection over the real protocol, with
snapshot, diff, reconnect, admitted intent and refused intent in one
machine-readable receipt, and the native revision changing only for the
admitted intent. **C4b** takes the embedded and full-page surfaces and their
keyboard and accessibility-tree checks.

Two findings from the C4 assessment reshaped the phase.

**The shared Cambium web host is already in use**, so §7's sentence about it
describes the present rather than a task. `web_view.rs` builds the chrome
through `cambium::GenetAppRunner`/`GenetElement` into a netrender `Scene`, and
`web_gpu.rs` presents it over `genet_render_host::RenderCore`. Graphshell's
`BrowserHost` (`web.rs:272`) is the application-state struct — canvas, presenter,
UI flags — and collides with the phrase "web host" in name only. There is no
duplicate host to converge.

**The browser cannot use `chirograph::Carrier`.** The trait is blocking, and its
own doc defers the question to whoever writes a network carrier; C4 is that
moment. The only stream implementation, `NetworkCarrier`, bridges the blocking
shape with `tokio::runtime::Runtime::block_on`, which cannot run on a browser
main thread — and blocking there would deadlock against the very data-channel
callbacks delivering the bytes being waited on.

A Web Worker does not rescue the blocking shape, which is worth recording
because it looks like it should. `RTCDataChannel` *is* transferable to a
`DedicatedWorker`, but a worker blocked in `request` has stopped the event loop
that delivers the message it is blocked on — the same deadlock on a new thread.
Blocking *and* receiving needs `Atomics.wait` on a `SharedArrayBuffer` fed by a
second thread, `SharedArrayBuffer` needs cross-origin isolation (COOP
`same-origin` + COEP `require-corp`), and a cross-origin iframe inherits
isolation only when its embedder grants the `cross-origin-isolated` permission.
That would impose COOP/COEP on anyone embedding Graphshell, which contradicts
C4b's own requirement that one component serve as both the full application and
an embed. `Document-Isolation-Policy` would have removed the COEP cost; it is a
Chrome explainer and has not shipped. Rendering compounds it: `web_gpu.rs` takes
an `HtmlCanvasElement`, so a client in a worker either ships scenes over
`postMessage` per frame or moves rendering to `OffscreenCanvas`, which is a
change in `genet-render-host` — the genet repo, outside this plan.

**Ruled: a sans-I/O session core.** `RetainedEndpointSession`'s protocol
sequencing moves into an I/O-free core; the blocking `Carrier` drives it on
native and an event-driven adapter drives it in the browser. One protocol
implementation with two thin adapters, which is the shape `notochord`'s
handshake and this carrier's own core already have, and the reason str0m was
chosen at C1. No cross-origin isolation, no genet change, and no second copy of
the protocol to keep in step. The surface is bounded: of 22 methods on
`RetainedEndpointSession`, 8 are pure state and **10 operations** touch the
carrier — `over`, `mount`, `resnapshot`, `open_action_draft`,
`submit_action_draft`, `resolve`, `invoke`, `wait_for_change`,
`poll_for_change`, `close`. `RetainedEndpointSession` keeps its public API
exactly and becomes the blocking adapter over the core, so no native consumer
changes.

**Landed 2026-08-31.** `graphshell_client::core::SessionCore` (559 lines) holds
the sequencing; `session.rs` (412) is the blocking adapter over it, and its
whole blocking surface is one loop, `drive`, which carries whatever the core
asks for to the carrier until the core says the operation is finished. The
count above was 10 by a doc-comment false match — `open_action_draft` reads the
advertised accessibility tree and asks the endpoint nothing, so it stayed pure
and the real figure is **9** I/O operations. The three multi-step ones are
where the state machine earns itself: a resolve wanting a second resource, a
resume answered with a resynchronize (bounded by `RESUME_ATTEMPTS`), and a poll
draining queued notices each return `Progress::Ask` again rather than looping
over a carrier they do not hold.

The core names `CarrierRequestBody`, `CarrierResponseBody` and `CarrierNotice`
— plain value types — and never the `Carrier` trait; its only mention of it is
the doc paragraph saying why. Verified against the pre-refactor baseline:
graphshell-client 30 tests before and after (36 with the core's own), knot-editor's
`place_projection` and `rosette_projection` unchanged, `graphshell` and
`knot-editor` compiling on all targets, rustdoc clean, and no warning anywhere
in the crate. Six new tests cover the paths `session.rs` never had any for,
each written without a carrier, which is the split demonstrating itself.

**The event-driven adapter landed the same day.**
`graphshell_client::driver::SessionDriver` (429 lines) is the second adapter:
it turns what the core asks for into an NDJSON line, accepts lines coming back,
and routes them. It is *not* browser code and has no browser dependency —
nothing in it knows what a data channel is, so it compiles and its tests run on
an ordinary `cargo test`. The browser glue left for the port is `send` and
`onmessage`. That is the division the WebRTC probe already draws between its
`sdp` module and its wasm half, for the same reason: a rule only exercisable in
a browser is a rule nobody exercises.

The wire is the one the native side already speaks — `CarrierRequest` out,
`CarrierOutput` (untagged: response or notice) in, one JSON value per line, the
same lines `graphshell-stdio` speaks and `serve_admitted_session` reads.

It also carries a property the blocking adapter got free from its call stack and
this one has to earn: **request identity**. A blocking carrier reads the answer
to the question it just asked, so correlation is implicit. Here lines arrive
whenever they arrive, so each request carries an id, only one may be in flight,
and a response naming a different id is refused rather than folded into
whatever happens to be pending. That is the difference between a stalled
session and a silently wrong one, and it has its own test. Nine tests cover the
adapter: discovery building the core, an answer to a request never sent, an
answer naming the wrong request, a second request while one is in flight, an
endpoint refusal clearing the in-flight slot without killing the session, a
notice queueing without disturbing a pending answer, an unreadable line, a
blank line, and a disconnect clearing what was in flight.

graphshell-client now runs 45 tests (30 before this work), compiles for wasm32,
and has no warning of any kind attributed to it, rustdoc included.

**The frame pump landed 2026-08-31.** `serve_admitted_session` wants an
`AsyncRead + AsyncWrite`; the carrier moves bounded frames; nothing between
them should have to learn what a data channel is.
`webrtc_carrier::native::stream_over_frames` is that seam — it returns a
`DuplexStream` and runs a pump carrying frame payloads onto it and whatever is
written to it back out as frames.

A pump rather than a hand-written `AsyncRead`/`AsyncWrite`, for a reason worth
recording: the carrier's backpressure policy is an `.await` on a watch channel
between the high and low water marks, and there is no honest way to spell that
as a `Poll::Pending` without a waker the channel does not expose. Awaiting
`FrameWriter::send_frame` gets the policy already written and already tested.
It is also the shape `graphshell::browser_carrier` uses on the native-messaging
lane for the same reason: framing at the edge, a private duplex behind it.

Two receipts, both over the same real DTLS loopback as the rest of the C1
suite, not a mock. **Frame boundaries are invisible to a line reader** — a line
three frames long arrives byte-for-byte as one line, and two short lines
sharing a frame arrive as two lines, which is exactly what Graphshell's NDJSON
rests on. And **dropping the stream ends its pump**, promptly and reporting a
clean end, so a finished session cannot leave a task spinning on a carrier
nobody reads.

Cost: one tokio feature, `io-util`, on the already-native-gated dependency.
Asserted rather than assumed — tokio still resolves to **0 crates** in the
wasm32 tree, so the Wasm-clean core is untouched. Carrier suite now 68 tests
(66 before), wasm32 core and `browser` both still clean, rustdoc clean.

**The admission wiring landed 2026-08-31.** `graphshell::webrtc_session`
(~1,000 lines with its tests, behind a new `webrtc-session` feature) is the
live half of the door: the join sequence over a real channel, and the admitted
session it produces. The door stays sans-I/O and keeps every rule; this module
keeps none — what it owns is only that the messages travel in order and that
every refusal is *written to the peer* before the channel closes.

The join travels as carrier frames — four JSON messages (`Open`, `Challenge`,
`Redeem`, `Grant`/`Refused`) and two binary ones (the Notochord hello and
reply) — because frames give the door's bytes-in/bytes-out functions their
message boundaries for free. Only after the admission reply does
`stream_over_frames` start, which makes the plan's "a refused stream never
reaches Graphshell" rule structural: before that point there is no stream for
an application byte to be on. `serve_webrtc_join` composes the whole host
side over a live `Carrier`; `JoinConclusion::admitted_over` assembles the
`AdmittedSession` (claims re-decoded from the accepted hello, notochord's own
precedent) over whatever stream carries the application.

Both roles live in the module on purpose — `peer_join` is the sequence the
browser's event glue drives and a future native client reuses, and keeping it
beside `host_join` is what lets one test drive both ends and keeps them from
drifting, exactly as the door already does with its client functions.

Four tests over in-memory frames (frames-over-DTLS being already the carrier
suite's receipt): the positive control with both ends agreeing on subject,
session id and link; a spent invitation refused *with the reason told to the
peer*; a wrong host refused before any secret crosses — the peer walks away
after the challenge and the test asserts the use count is intact, which is
what makes a relay-in-the-middle unable to exhaust an invitation it cannot
sign for; and the flagship: **a completed join carries a real Graphshell
session**, `serve_admitted_session` over a real `ResumeFixtureEndpoint` on the
host end, the event-driven `SessionDriver` on the peer end, discover → mount →
close, exactly three requests answered. That is every C4a seam except DTLS
itself composed in one test.

The feature is separate from `native` because it is what pulls str0m into the
build; asserted rather than assumed — str0m resolves to **0 crates** in the
default graphshell tree (positive control on the tree itself). Full graphshell
lib suite: 140 tests green under the feature; default build untouched.

**Rejoin, the exported loopback harness, and the composition receipt landed
2026-08-31.** Three pieces, each proven as it went in:

*Rejoin.* The join protocol gained `ToHost::Resume` and `peer_rejoin`: a
reconnecting peer verifies the host over the new link (a new link is a new
channel someone else could be terminating), then presents its retained
delegation — carried in `PeerJoin` since the first join — and goes straight to
admission. No redemption, so the invitation's use count belongs to first joins
alone. The test does both halves at once: a one-use invitation, spent, then a
rejoin over fresh fingerprints admitted with the count still at zero and a
*different* shared link. C2's one-use ceiling and C3's fresh-admission rule
holding simultaneously.

*The harness.* The carrier's private loopback rig — str0m offerer, real ICE,
real DTLS, real SCTP over two `127.0.0.1` sockets — moved to
`webrtc_carrier::native::loopback` (`LoopbackOfferer`, `loopback_pair`), and
the carrier's own 15-receipt suite now runs on the exported copy. One harness,
however many crates take receipts over it.

*The composition receipt.* `ports/graphshell/tests/webrtc_join_loopback.rs`:
one real WebRTC pair carrying the join, Notochord admission, the frame pump,
`serve_admitted_session` over a fixture endpoint, and the event-driven
`SessionDriver` doing discover → mount → close. Everything the C4a fixture
binary will do except HTTP signaling and the browser itself. Passes in ~0.3s
of protocol time.

That receipt caught a real lifetime bug, and it is the reason `ServedJoin`
exists in the shape it does. `CarrierControl` cancels the driver when dropped
— the deliberate dead-man's switch — so `serve_webrtc_join` returning a bare
session meant the host cancelled its own carrier while the serve loop's final
`Closed` reply was still in the pump; the peer's close reproducibly never
answered. Worse, the failure was timing-shaped: it *passed* with debug prints
in and failed 4-of-5 without them, and only per-phase timeouts pinned it to
the close. The fix is `ServedJoin { session, pump, control }` plus
`ServedJoin::finish()`, whose ordering is the whole function: drop the stream
so the pump drains the final bytes and sees end-of-stream, await the pump,
*then* the carrier's polite close flushes the outbound queue. Hammered six
times green; the steady ~5s in the test is the carrier's own bounded polite
close waiting out a peer that vanished, not a defect.

graphshell lib under the feature: 141 tests. Default build untouched, docs
clean.

**Pre-admitted sessions, the live endpoint, and the intent receipt landed
2026-09-01.** Mark ruled pre-admitted sessions in, so the WebRTC lane reaches
the *product* host rather than a fixture beside it.

`ResidentProjectionHost::serve_admitted` is the split: everything
`accept_one` did after the decision — authority retained from the chain the
conclusion was drawn from, the route opened from the catalog, the live-session
count, the notifying loop — now takes an `AdmittedSession` somebody else
produced. `accept_one` is that function plus its own admission. A pre-admitted
session is served exactly as a dialled one, because after the decision there
is no difference worth having. It deliberately does not consult
`max_sessions`: the caller admitted this peer, so the ceiling was that
decision's to apply, and `live_sessions()` is what a caller passes to its own
admission to make the check.

`graphshell::live_endpoint::LiveEndpoint` is the endpoint C4a's third
done-condition needs. `ResumeFixtureEndpoint` has a history decided at
construction, which is right for proving resume picks the correct branch and
useless for proving *what makes* a revision. This one advances on invocation,
and the interesting half is that it advertises a **refused** intent as
visibly as the admitted one — a receipt row saying "an action the endpoint
never offered was not performed" proves nothing about policy; the claim worth
making is that a peer can see an action, invoke it correctly, and be told no
with the revision standing still. Seven unit tests state the rule directly,
including that a refusal rings no bell.

The composition receipt gained the row that matters: over one real WebRTC
pair, a browser-shaped peer joins, the **resident host** serves it through the
same catalog route a dialled peer gets, and the peer invokes the refused
intent then the admitted one — reading the native position back over the wire
by resnapshot each time. Refused: revision unmoved. Admitted: **exactly one**.
Five consecutive runs green.

Two findings from writing it. The `CarrierControl`-drop hazard recurred
immediately on the *peer* side (`let (reader, writer, _control) = …` cancels
the driver the moment the binding falls), which says the hazard is easy to hit
and worth a type-level fix if it appears a third time. And the capability
profile is a real negotiation, not a formality: a default `CapabilityProfile`
supports no offer, so every advertised action reads as unadvertised and
`invoke` refuses before anything reaches the wire — the browser must declare
what it can present.

graphshell lib: 148 tests. Default build untouched, docs clean.

**The host fixture binary landed 2026-09-01.** `c4_webrtc_host` is the last
native piece: it prints an invite fragment, answers `POST /offer`, and runs
the shipping path underneath — `serve_webrtc_join` for the door,
`ResidentProjectionHost::serve_admitted` for the product host,
`LiveEndpoint` on a catalog route. The signaling is deliberately the dumbest
thing that works, one POST for the offer and one response for the answer, the
same shape webrtc-ping established at C1; C5 replaces it with `mer3ly.net`.

Two decisions worth keeping. The host identity is generated fresh from
`OsRng` on every run rather than seeded from a constant — a fixture with a
hard-coded seed is a private key in a repository, and the first person to
reuse the shape inherits it; a restarted fixture is a different host and its
old invitations correctly stop verifying. And `--advertise` is documented as
not-optional-in-practice, with the C1 receipt's reason attached: the carrier
labels inbound datagrams with the *first* advertised address, so advertising
both loopback and a LAN address attributes a browser's packets to `127.0.0.1`
and DTLS times out with no useful error.

`tests/c4_host_signaling.rs` drives the **binary**, not the library: it spawns
the real executable, waits for its `READY` line rather than polling a port,
fetches the fragment over HTTP, and joins as a str0m peer through the real
`POST /offer` — the browser's exact path minus the browser. It asserts the
fragment served over HTTP is the one printed, that a projection mounts, that a
*second* visitor is served rather than queued, and that an invitation this
host never issued is refused by name (`unknown invitation`) with the serving
test as its positive control. A `Drop` guard kills the child, verified by a
leak check finding no surviving fixture after four consecutive runs.

Three findings, all of which a headed run would otherwise have hit first.
`InMemoryProvider` is deliberately not `Clone` (it holds a master private
key), so the fixture shares one behind an `Arc` rather than handing out
copies. Cargo runs integration tests in parallel, so two fixtures on one
signaling port produced "the fixture exited before it was ready" — a port
collision wearing a startup failure's clothes; each test now owns a port. And
a fixture left running holds its own binary against the next `cargo build` on
Windows, which is an argument for the `Drop` guard rather than manual cleanup.

graphshell lib: 148 tests, plus 2 composition and 2 signaling receipts.
Default build untouched; no warnings in any file this work added.

Still open for C4a: the browser glue binding `SessionDriver` + `peer_join` to
the data channel, replacing `canary::FixtureEndpoint` in the web client with
the real mount, and the headed machine-readable receipt. The native half is
complete: a browser now has a host to talk to.

## 8. C5: public rendezvous

Deploy `https://mer3ly.net/join/<rendezvous>#<private-capability>` only after C4.
The join route serves a small pinned bootstrap loader and security headers. The
loader resolves the invitation's `ReleaseRefV1`, verifies the detached manifest
signature against its own publisher trust store, selects the browser artifact,
and verifies its digest and signature before importing JavaScript or
instantiating Wasm. A small Worker brokers opaque offer, answer, and ICE
messages and mints short-lived TURN credentials. A Durable Object is
permissible for bounded rendezvous mailboxes and presence; it does not hold
application state or decide admission.

The service stores only operationally necessary, expiring rendezvous material.
Rate, size, and lifetime limits are user-configurable at the native host within
safe protocol ceilings. The page displays verified or unknown publisher,
source revision, release identity, client selection, requested action, and
invite expiry before joining. Exact-release client selection is preferred.
A separately trusted generic Graphshell client may join a signed-compatible
wire version without claiming to be the host's exact release.

**Done when:**

- a fresh browser with no extension joins through the public URL;
- the URL fragment is absent from HTTP requests and service logs;
- signaling substitution cannot impersonate the signed native host;
- destroying or revoking the invite prevents a later join;
- direct and TURN paths both close the C4 receipt through the public service;
- official release manifests and browser bundles verify before client code
  executes, while an altered bundle is refused before import;
- an unknown fork publisher is never described as Merely-signed and cannot add
  itself to the trust store through the invitation;
- service deletion leaves the native application state intact.

## 9. C6: Luggage release offer and native adoption

Once the public live-session receipt closes, make the release reference useful
beyond display. Luggage owns a Wasm-clean signed-envelope core; its native side
keeps feed polling, verified staging, update policy, and platform apply. The
signed manifest names application id, version, source revision, supported
invitation and Graphshell wire versions, and artifacts by kind and target.

Artifact locators live in a disposable `ReleaseOfferV1`, outside the signed
content manifest. A mirror, native host, or peer may change without changing
the release identity. The source revision inside the manifest is a publisher
claim; a reproducible-build receipt remains separate.

“Install this release” offers the same `ReleaseRefV1` plus the exact platform
artifact selected from its verified manifest. Artifact bytes may arrive from
`assets.mer3ly.net`, the native host over the admitted session, or an Iroh peer.
The source does not affect verification. Existing installations hand a
verified self-sufficient offer to Luggage and Djinn's configured policy. A
fresh browser may verify and download an OS installer, but installation remains
an explicit operating-system action.

Adopting a fork requires two visible changes: its trusted publisher key and its
configured update feed. Joining the fork's live session performs neither.

**Done when:**

- one signed manifest names distinct browser, Windows, Linux, and macOS hashes
  under one release identity and source revision;
- native and browser verifiers derive the same `ReleaseRefV1` from frozen
  vectors;
- corrupted manifests and artifacts fail identically whether bytes came from
  HTTPS, WebRTC, or Iroh;
- changing only offer locators preserves the release reference, while changing
  any signed content fact changes it;
- the host can seed an exact platform artifact by content hash without becoming
  publisher authority;
- an installed app stages and applies the offer through Luggage's
  self-sufficient signed stage, while a browser download produces a verification
  receipt and waits for explicit installation;
- accepting a session leaves publisher trust, feeds, channels, and update
  policy byte-for-byte unchanged.

## 10. C7: the iroh browser relay as a second lane

Direct WebRTC is the join path. This phase exists for the case it cannot serve:
a network that blocks WebRTC outright, where a relayed session beats no session.
It is a **fallback, not a competitor** — it does not start until C4 closes, and
it does not dilute the direct carrier's receipts.

The candidate is **iroh 1.1's own browser support, not the external donor**. It
needs no port, no version bump, and no new manifest entry: `iroh-relay` 1.1.0
declares a `wasm32-unknown-unknown` target block using `ws_stream_wasm`, and that
crate is already in this workspace's lock. A browser `Endpoint` binds with
`presets::N0` and dials by `EndpointId` over an ALPN — the shape
`mere-transport::Transport` already has, so the identity plane is preserved
rather than rebuilt.

Two properties are settled and need no re-litigating (Findings, 2026-08-26):

- it is **relay-only**. A browser tab cannot hole-punch, so this lane always
  carries traffic through a relay. That is the price of the fallback, not a
  defect to engineer away.
- it costs **10.8x the browser payload** of the direct carrier, because iroh
  compiles into the wasm rather than being supplied by the browser. That is why
  it is the second lane and not the first.

What must be proven before adopting it, per lane rather than per crate:

- a headed browser reaches a native host through a relay, and Notochord admits
  the session on facts the relay cannot forge;
- `shared_link` binds iroh's connection identity — there are no DTLS
  fingerprints here, so the C0 transcript survives but `fingerprint.rs` does not;
- reconnect runs fresh admission, exactly as C3 requires of the direct lane;
- the relay operator learns no Graphshell key and no application capability;
- setup and resume latency measured against the direct carrier, and the payload
  cost re-measured optimised (`wasm-opt`, `opt-level="z"`, LTO, `panic="abort"`),
  since both current figures are unoptimised floors.

Adopt it only for a named consumer the direct carrier cannot reach. The direct
WebRTC/Notochord path remains the product path regardless of the result.

## 11. Outside this plan

- Gemini or HTML projection from Knot;
- WebRTC audio/video tracks and media decode;
- multi-host fanout, SFU operation, and cloud-held graph state;
- operating-system code-signing procurement and unattended first installation.

Each becomes separate work after the live browser session closes its authority,
relay, and reconnect receipts.

## Findings

- A headed browser-to-native data-channel ping is physically proven with an
  external donor.
- Local Notochord and Personae compile for Wasm when JavaScript randomness
  features are explicit.
- The donor's 7,060,115-byte example Wasm, Iroh version skew, absent TURN, and
  benchmark ALPN mismatch rule out adopting it unchanged.
- The current `Transport` trait is wider than an inbound browser invitation.
  Reusing `AcceptedSession` preserves honest carrier facts without weakening
  the trait.
- A URL capability can enter the existing Personae delegation grammar by
  redeeming into a delegation for a browser-generated ephemeral subject.
- WebRTC needs an explicit host-signed fingerprint challenge to supply
  Notochord's connection-specific link binding.
- Luggage, rather than `InviteV1`, owns release meaning. The invite carries only
  a signed-manifest hash and publisher-key identity; release trust stays in the
  loader or installed application's publisher store.
- Exact release identity and protocol compatibility are related but distinct.
  Exact artifacts make receipts reproducible; a compatible trusted generic
  client preserves interoperability without pretending to be that release.
- Luggage narrows byte-source authority but cannot secure an ordinary browser
  against a compromised first-load web origin that replaces the verifier
  itself. That bootstrap trust stays explicit.
- **2026-08-26:** WebRTC is structurally the Reticulum case, not a new proof
  shape. `IngressContext::webrtc(shared_link)` fills the same
  `ingress.link` slot Reticulum uses, and the derived
  `SessionFacts::proof_binding()` is equal to
  `transport::initiator_link_binding(&alpn, link)` — asserted in
  `crates/murm/transport/src/notochord.rs`
  (`a_webrtc_binding_pins_the_link_and_names_no_peer`). C2 therefore redeems
  into the existing Personae delegation grammar rather than adding one.
- **2026-08-26:** the carrier vocabulary has exactly one mapping site.
  `carrier_of` in `crates/murm/transport/src/notochord.rs` is the only match on
  `TransportKind` in the workspace; `notochord/src/facts.rs:19` mirrors the
  enum in prose only. Adding a carrier is a one-file change, and the
  exhaustiveness guarantee is real rather than assumed.
- **2026-08-26:** Graphshell's accept path splits cleanly at the accept call.
  `admit_accepted_session` in `ports/graphshell/src/carrier.rs` takes
  `AcceptedSession<S>` where `S: AsyncRead + AsyncWrite + Unpin` — the bounds
  `notochord::admit_session` already requires, with no `Send`/`'static` added,
  which a browser-side stream could not have satisfied. All five existing call
  sites compile unchanged.
- **2026-08-26:** a transcript-link derivation already existed.
  `ports/graphshell/src/browser_carrier.rs` derives a 16-byte link for the same
  Notochord slot over the WebExtensions bridge. `webrtc-carrier` was written
  against SHA-256 and was unified onto browser_carrier's discipline — blake3,
  domain raw at the front, `u64` little-endian length prefixes — so one
  derivation convention holds across carriers. The frozen vector moved from
  `383d1883…5960` (SHA-256, `u32be`) to `4692c88e70470f7e4b7ba46b7fce78b2`
  (blake3, `u64le`, 280-byte transcript).
- **2026-08-26:** the browser payload decides this, and the donor's size was
  never an argument against WebRTC. Three routes, built release for
  `wasm32-unknown-unknown` the same unoptimised way and each verified to contain
  its transport rather than having been dead-code-eliminated:

  | route | raw | gzipped |
  |---|---|---|
  | direct WebRTC (C0 core + `web-sys`) | 540,025 | 116 KB |
  | iroh 1.1 first-party browser client | 5,828,687 | 1.57 MB |
  | donor `iroh-webrtc-transport` example | 7,060,115 | — |

  The browser *is* a WebRTC implementation, so a direct carrier ships only the
  C0 core and bindings; iroh must be compiled into the payload. "Already in the
  dependency graph" is a native saving that inverts in the browser. The donor's
  7 MB indicted putting Iroh in a browser, not WebRTC.
- **2026-08-26:** iroh 1.1 has first-party browser support, which the 2026-08-25
  probe never tested — it evaluated the donor instead. `Endpoint`,
  `builder(presets::N0)`, `bind()` and `connect(EndpointId, alpn)` all
  type-check for `wasm32-unknown-unknown` with **no addition to this
  workspace's manifests**: `iroh-relay` 1.1.0 declares a wasm target block
  using `ws_stream_wasm`, already resolved in `Cargo.lock`. It is relay-only —
  a browser cannot hole-punch — and carries no version skew, unlike the donor.
- **2026-08-26:** the dual-target receipt was not reproducible as written.
  `rust-toolchain.toml` declared only `wasm32-wasip2`, so C0's
  `wasm32-unknown-unknown` check passed only on machines that happened to have
  the target installed. The target is now declared.
- **2026-08-26 (C1 stack choice):** measured, not asserted. Against a
  tokio-only baseline, `webrtc` 0.20.3 adds 199 crates and ~329 MB of compiled
  code; `str0m` 0.23.1 adds 76 crates and ~214 MB. str0m selected — also for
  its Sans-IO shape, which matches Notochord's sans-I/O handshake core and the
  deliberately I/O-free carrier core. Features: `default-features = false,
  features = ["rust-crypto"]`, avoiding `aws-lc-sys` (C/asm, needs cmake and a
  C toolchain on every validation target). Native feature delta measured:
  12 -> 114 crates, +314,752,494 bytes of release artifacts. Per the plan's own
  rule this is an evidence result, not a permanent product rule.
- **2026-08-26 (C1 headed receipt, session 2):** a real Chrome and the native
  str0m answerer: ICE `checking -> connected` on a direct host pair
  (7 binding round trips), 200 frames echoed both ways, 3,277,600 bytes each
  way (payload + exactly 200 x 4-byte headers), `malformed_datagrams: 0`,
  clean close ("peer closed the channel"), both DTLS fingerprints captured.
  Fingerprints are read on `Event::Connected` — the certificate actually
  presented, not the SDP's claim (str0m fills `remote_dtls_fingerprint` only
  after verifying the presented certificate). C2 binds the stronger value.
- **2026-08-26 (headed-only defects):** the headed run found four defects that
  compile checks, unit tests, and the loopback suite all passed over:
  `create_offer` treated the `RTCSessionDescriptionInit` *dictionary* that
  `createOffer()` resolves to as a failed prototype cast, reporting every
  valid offer as an error; two browser handlers held a `RefMut` across user
  callbacks and tripped wasm-bindgen's reentrancy guard during ICE gathering;
  `Driver` handed its 0.0.0.0 bind address to `Receive::new`, so str0m
  discarded every inbound datagram as addressed to no local candidate
  (signature: 78 STUN requests sent, zero answered, both sides parked in
  `checking`); and a peer vanishing mid-transfer (browser refresh with ~400 KiB
  buffered) crashed the native process with a tokio worker stack overflow.
  The first two are fixed; the last two are in repair as this is written.
- **2026-08-26 (constraint for C2/C5):** default Chrome emits ONLY
  mDNS-obfuscated `.local` host candidates, and str0m has no mDNS resolver, so
  a candidate-bearing offer requires either an mDNS answer path, an ICE/TURN
  server, or a browser flag no ordinary user will set. str0m also performs no
  peer-reflexive discovery from an empty remote candidate list — an offer
  stripped of candidates hangs both ends in `checking` with no error. The C5
  rendezvous design must treat "at least one resolvable remote candidate" as a
  hard precondition, and the probe now refuses to signal a candidate-free
  offer rather than letting it fail silently.
- **2026-08-26 (C2 redemption scheme):** the redemption secret is an ed25519
  seed. The browser derives the redemption keypair and signs
  `redemption_signing_bytes(challenge, subject)`; the host stores only the
  redemption PUBLIC key plus use-state and expiry, so a stolen host store
  cannot mint redemptions. A refused redemption costs the invitation nothing
  (`a_refused_redemption_costs_the_invitation_nothing`), and the proof binds
  both the challenge transcript and the ephemeral subject, so it neither
  crosses connections nor transfers between subjects.
- **2026-08-26 (C2 signing discipline):** no bare master key signs anything —
  both host signatures (invite descriptor, challenge transcript) use one
  derived key under `mere.graphshell/webrtc-host-signing/v1` with a
  `DerivedKeyAttestation` traveling alongside, the same shape
  `SignedDelegationCertificate` and `SessionHello` already use. Message-level
  separation lives in three frozen wire domains:
  `mere.webrtc-carrier/invite-descriptor/v1`,
  `mere.webrtc-carrier/host-challenge-signature/v1`,
  `mere.webrtc-carrier/redemption-proof/v1`.
- **2026-08-26 (C2 authority boundaries):** `mint_delegation` takes the trust
  root as an explicit host-side parameter — an invitation carrying its own
  root would choose which authority admits it. The mint reads the HOST's copy
  of the invite (a client-presented copy could choose its own scope), the
  minted scope is the invitation's exact action triple at delegation depth 0,
  and the grant's expiry is clamped to `min(now + ttl, invite expiry)` — the
  owner's bound on the offer beats the host's bound on one session.
- **2026-08-26 (C2 matrix):** 26 fail-closed rows in
  `ports/graphshell/src/webrtc_door.rs`, each asserting the exact
  `DenyReason`/`ChainFault`/`RedemptionRefusal` variant with positive controls
  first, plus one integration receipt over the real str0m loopback using
  certificate-actually-presented fingerprints
  (`a_real_webrtc_channel_admits_an_invited_browser`). Cost worth knowing: the
  loopback test's dev-dependency feature pulls str0m (~76 crates) into every
  graphshell test build.
- **2026-08-26 (C3 relay is browser-side):** str0m accepts a remote relay
  candidate unconditionally (`add_remote_candidate` never inspects kind,
  is-0.11.0/agent.rs:729), so forcing relay is entirely the browser's
  `iceTransportPolicy: "relay"`. The native side needs no relay config. TURN
  credentials are minted server-side by the coturn REST convention
  (`username = "<expiry>:<name>"`, `credential = base64(HMAC-SHA1(secret,
  username))`) at the answerer's `/turn-credentials` endpoint; the long-term
  secret never reaches the browser. The HMAC is verified against RFC 2202 and
  a coturn interop vector, so a live TURN server will accept a minted
  credential because the bytes already match the reference.
- **2026-08-26 (C3 reconnect = re-admission, not restart):** two distinct
  events. An ICE RESTART (browser `create_restart_offer`, str0m detects the new
  ufrag/pwd in `accept_offer` and restarts transparently, change/sdp.rs:715)
  keeps the DTLS connection, so it is the SAME carrier link and needs no
  re-admission. A NEW DTLS connection is a new link and runs the full host
  challenge + Notochord admission again — the plan's rule. The browser retains
  its C2-minted delegation across a drop (only a page refresh loses it), so
  reconnect reuses that delegation in a fresh hello; the invite stays one-use
  and the C2 TTL clamp bounds the reconnect window. No re-redemption path is
  needed.
- **2026-08-26 (C3 no-Failed-state constraint):** is-0.11.0's
  `IceConnectionState` has no `Failed` or `Closed` variant by design —
  `Disconnected` can self-heal if new candidates arrive (agent.rs:185). So the
  "credential expiry causes refresh or a clean failure, not an infinite retry"
  done-condition cannot key off an ICE state; it needs a bounded timer. Any
  reconnect trigger must be timer-bounded rather than state-watched.
- **2026-08-26 (C3 driver placement, window default = Mark's call):**
  `DriverPlacement::DedicatedThread` moves the driver off tokio's shared
  runtime onto a std::thread with an explicit 8 MiB stack, measured to survive
  str0m's full 128 KiB SCTP window (a 2 MiB tokio worker overflows it). The
  dedicated SCTP window default was set to **64 KiB** (Mark, 2026-08-26):
  below str0m's own 128 KiB ceiling, so this crate's `window_holds` guard stays
  the primary defense and the stack is margin, not the sole line. Correction to
  an earlier claim: the 65x throughput penalty is a LATENCY effect (one round
  trip per frame) and does NOT reproduce on loopback — there the raised window
  is ~2.6x SLOWER; the LAN figure is inherited, unverified here, and only the
  machine-independent mechanism (`window_holds` 410-524 -> 0) is asserted.
- **2026-08-26 (C3 application hazard, unowned):** a naive read-then-write
  echo loop wedges a session on BOTH driver placements — parked in
  `send_frame` the end stops reading, its queue fills, SCTP retransmits, the
  session dies mid-transfer. The loopback tests split reader/writer to avoid
  it; a real application (C4) must not couple the directions head-to-head.
- **2026-08-26 (str0m upstream defect, mitigated):** the refresh crash was
  str0m's own recursion — `Rtc::do_poll_output` (str0m 0.23.1, lib.rs:1719 and
  1734) self-calls once per queued SCTP packet in its `SctpEvent::Transmit`
  arm, so on teardown the stack depth equals the outstanding SCTP window in
  packets; str0m's own 128 KiB `MAX_BUFFERED_ACROSS_STREAMS` overflows a 2 MiB
  tokio worker stack. Proven by dose-response (halving the window halves the
  required stack), not by reading. Mitigated here by
  `CarrierConfig::sctp_window_bytes` (default 16 KiB): `try_flush` declines to
  hand SCTP a frame past that mark, the same contract as SCTP declining a
  write, observable via `CarrierStats::window_holds`. Two consequences worth
  keeping visible: (1) the default caps throughput at ~16 KiB per RTT, fine on
  LAN, wrong for long links — it is a config field with the measured stack
  table in its doc comment, and C3's relay work should revisit the default;
  (2) this is an upstream-filing candidate against str0m — the recursion, the
  repro shape (vanished peer with a full send buffer), and the dose-response
  table are all in `tests/native_loopback.rs` and the driver comments.
- **2026-08-26 (frame ceiling):** `MAX_FRAME_BYTES` was 4 + 65,536 = 65,540 —
  over a 64 KiB SCTP `max-message-size` peer by exactly the header. Browsers
  advertise 262,144 (confirmed in the live SDP), so C1 passed on luck. The
  payload ceiling is being corrected to `65,536 - FRAME_HEADER_BYTES` so the
  whole frame fits a default peer; the frozen shared-link vector binds the
  transcript, not frame sizes, and does not move.

## Progress

- **2026-08-25:** live seams and active plans reconciled; external donor built;
  headed browser/native ping passed; donor benchmark mismatch identified;
  Notochord/Personae external Wasm compile passed; architecture and gates
  recorded. Production code remains untouched.
- **2026-08-26: C0 landed.** New Wasm-clean `crates/murm/webrtc-carrier`
  (bounded frames, `InviteId`, role-tagged DTLS fingerprints, the link-challenge
  transcript, `shared_link` derivation); `TransportKind::WebRtc` +
  `IngressContext::webrtc` mapped honestly to `CarrierKind::Other`;
  `admit_accepted_session` extracted in Graphshell with every call site
  unchanged; and an acceptance fixture
  (`a_webrtc_session_is_admitted_with_no_authenticated_peer`) that derives its
  link through the core rather than pasting a constant, so a transcript change
  fails admission too. Verified: `mere-transport` 56 tests with the `notochord`
  feature on (the default feature set does not compile that module — a green
  run without it proves nothing), `webrtc-carrier` 29 on native and a clean
  `--all-targets` check for `wasm32-unknown-unknown`, `graphshell` 110.
  Open, deliberately: the SDP fingerprint parser accepts uppercase hex only
  (RFC 8122's grammar, and what Chrome and Firefox emit) — correct per spec,
  but it makes a nonconforming stack fail loudly rather than interoperate, and
  that posture should be revisited when C1 meets a real browser.
- **2026-08-26: Luggage integration ruled.** `InviteV1` now carries a
  `ReleaseRefV1`, C5 verifies the exact browser bundle before execution, and C6
  turns that same signed release into an explicit native adoption offer. The
  prerequisite portable release envelope and self-sufficient stage remain open
  in the Djinn resident plan; no release or trust code landed in this slice.
- **2026-08-26: C1 substantially landed.** str0m answerer (Sans-IO driver:
  single-mutation turn loop, dual-source backpressure gauge, fingerprint
  capture on `Event::Connected`) and web-sys browser initiator (event-driven
  backpressure off `bufferedamountlow`, one-shot terminal-error funnel) behind
  `native`/`browser` features, both off by default; the Wasm-clean default
  core and its frozen vector are untouched. Water marks observed 14 up /
  14 down over 400 loopback frames with str0m's own `low_water_events: 39`
  confirming the engine sees the threshold. Headed receipt: session 2 green
  end-to-end (200/200 frames, byte-exact framing, fingerprints captured,
  clean close); the refresh condition initially failed by crashing the answerer
  (stack overflow) — both that and the 0.0.0.0 `local_addr` defect were fixed
  with negative-control regression tests, and the refresh re-run passed: the
  answerer survived a peer vanishing with ~525 KB buffered, printed an honest
  receipt (`ended: "echo write failed: the data channel is closed"`, 0 dropped,
  0 malformed), and served a fresh session on the same process. Full receipt:
  `Code/testing/mere/webrtc_ping_receipt.md` (RESULT ok, 7 runs, 5 defects).
  Open items carried out of C1: the `sctp_window_bytes` default (16 KiB) costs
  ~65x on a LAN echo workload because the window is smaller than one maximum
  frame — revisit at C3; wildcard bind with a MULTI-address advertise list
  still mislabels inbound datagrams as `advertise[0]` (single-declared-address
  assumption is documented but the probe violates it) — either the probe
  advertises one address or the carrier learns per-packet destinations; and
  the str0m `do_poll_output` recursion is an upstream-filing candidate.
  Harness: `crates/probes/webrtc-ping` (standalone, `wasm-bindgen` pinned
  =0.2.126 to match the installed CLI — an unpinned range resolves 0.2.127
  and fails binding generation after a clean compile).
- **2026-08-26: C2 landed.** Core: `InviteV1` (bounded canonical encoding,
  strict hand-rolled base64url fragment codec, seed-redacting `Debug`, three
  frozen signing domains) in `webrtc-carrier` — 44 unit tests, no new
  dependencies, frozen vector untouched. Door: `webrtc_door.rs` beside the
  native-messaging precedent — invite issue/redeem/mint on the derived-key
  pattern, sans-I/O admission through the audited N1 facts adapter, pure
  client half a wasm build can reuse. Verified independently: graphshell 136
  lib tests (26 new matrix rows), the real-WebRTC loopback receipt, carrier
  58, C0 wasm receipt, and `cargo check --workspace --all-targets`, all exit
  0. Deferred to C4: the headed fragment-hygiene row.
- **2026-08-26: C3 code landed; receipts gated on live TURN.** Carrier:
  `DriverPlacement::DedicatedThread` (8 MiB stack, 64 KiB window default) with
  13 loopback tests across both placements; browser `IceServer`+credentials,
  `IceTransportPolicy::Relay`, `create_restart_offer`; the shared `lib.rs`
  re-export edited once against a settled file. Probe: `/turn-credentials`
  coturn-REST minting (HMAC verified against RFC 2202 + a coturn vector, and
  the running endpoint cross-checked against an independent implementation),
  forced-relay toggle, reconnect button, and a revisioned diff-vs-snapshot
  resume stand-in above the carrier — 17 probe tests. Verified independently:
  carrier 44+13+8+1 native, browser+default wasm clean, probe green.
  AWAITING a live TURN URL+secret from Mark for the three receipts that need
  a real relay: relay-only candidate pair, TURN-carries-encrypted-packets-but-
  no-Graphshell-key, and credential-expiry clean give-up; plus the headed
  reconnect resume run. The plan's stop line (no product wiring until a forced
  relay is demonstrated) holds until those pass.
- **2026-08-28: C3 landed; the stop line is cleared.** Forced relay proven
  across two machines: coturn 4.17.2 on the Fedora ThinkPad (`192.168.4.28`,
  unprivileged, no sudo — FedoraWorkstation already opens 1025-65535/udp),
  Chrome and the answerer on the Windows box. The probe's coturn-REST minted
  credential was accepted by the real server (Allocate 401 -> `0x0103`,
  `XOR-RELAYED-ADDRESS 192.168.4.28:49195`); Chrome under
  `iceTransportPolicy:"relay"` gathered exactly one relay candidate; and with
  the new native TURN client offering `typ relay`, a session opened in 366 ms
  and echoed 50/50 frames with the answerer seeing its peer at the RELAY
  address, not the browser's. Credential expiry fails clean in 169 ms with a
  positive control in the same run. Resume-by-diff proven over the relay
  (`rev 1..=20, 81920 B`) on a new DTLS link with fresh admission. Receipt:
  `Code/testing/mere/webrtc_forced_relay_receipt.md`.
  Four defects the live run caught, all recorded there: the answerer could not
  offer a relay candidate at all (str0m does no TURN allocation and
  `AnswererConfig::advertise` forces `typ host` at the carrier's own port —
  fixed with a hand-rolled TURN client plus per-peer shim sockets, so str0m
  never learns a relay exists); per-session channel numbers violate RFC 5766
  §11.2; a short host-side credential kills the allocation at first Refresh
  (RFC 5766 §4 binds it to the creating username); and `reconnect` writes its
  resume HELLO into the channel its own new offer just preempted.
  CARRIED: the snapshot resume branch (needs a revision aged out of the
  retained window), and a packet capture if the relay-carries-ciphertext-only
  claim should be evidenced rather than reasoned.

- **2026-08-28 (`reconnect` fixed — and it is a rebuild, not an ordering
  tweak):** the carried `reconnect` defect is closed, but not the way the C3
  receipt predicted. Waiting for the channel to reopen cannot work: the fixture
  answers every offer from a freshly bound answerer with a new DTLS
  certificate, which tears the SCTP association down, and an `RTCDataChannel`
  is not recreated when SCTP restarts — `BrowserInitiator` opens its channel
  once, in `new`. There is no new channel on that peer connection to wait for.
  `reconnect` now stands up a whole new initiator, waits for *its* channel, and
  carries only the revision across; `create_restart_offer` is consequently
  unused by the probe, though it remains exported and tested on the carrier.
  This is §6's rule earning itself rather than being worked around: a reconnect
  here really is a new DTLS connection, so it really does need fresh admission.
  Two things the rebuild required that the ordering framing would have missed —
  stats-writing callbacks are now gated per session, because a retired
  session's `onclose` otherwise stamps `terminal` onto the session that
  replaced it; and the offer POST is the point of no return, so the old session
  is released exactly there, leaving a reconnect that fails while gathering
  with its predecessor intact. RECONNECT also now works on an already-closed
  session, which is when you most want it. Headed receipt:
  `Code/testing/mere/webrtc_reconnect_receipt.md`.
  ALSO CARRIED from Mark's review: `ReleaseRefV1` is defined in the carrier but
  the plan assigns release identity to Luggage; ruled 2026-08-28 to make
  `crates/system/luggage` Wasm-clean by feature (its release-identity core
  compiling for wasm32 with default-features off) and have the carrier consume
  it rather than define it.

- **2026-08-28 (Luggage is Wasm-clean by feature):** first half of that ruling
  done. `luggage` grew a `native` feature, on by default, carrying the whole
  update pipeline; `default-features = false` leaves `error` + `release` — the
  manifest types — and compiles for `wasm32-unknown-unknown`. Core is 103
  crates against the native build's 261, on six direct deps (`semver`, `serde`,
  `serde_json`, `thiserror`, `time`, `url`). Native build, all 36 tests, and
  rustdoc are unchanged in both configurations. Divergence recorded in the
  crate README per its fork convention.
  Two scope calls made inside the ruling rather than by it, both reversible and
  both flagged for Mark. First, `config` (`Config`/`Feed`) went to `native`,
  not core: a feed is *where releases are fetched from*, an updater concern,
  and its `file://` branch needs `Url::to_file_path`, which the `url` crate
  does not provide on wasm32. Second, `signing` went with it, because every one
  of its functions is `pub(crate)` and the core had no caller — the core can
  name and compare a release but not verify one. Exporting a verification entry
  point would add public API to a shipping crate, so it was left open.
  Ruled 2026-08-30: a new small type, not a collapse into `RemoteRelease`.

- **2026-08-30 (release identity has one definition):** `ReleaseRefV1` now
  lives in `luggage::release` and the carrier consumes it. The type needs no
  dependency at all, so it is reachable with `default-features = false` on
  every target the crate builds for; its doc states the `V1` suffix is a wire
  contract, so a different field set is a `ReleaseRefV2` and never an edit.
  `invite.rs` imports it, `lib.rs` re-exports it so an `InviteV1` holder can
  name what `release()` returns without depending on Luggage, and the
  canonical encoding is untouched — it reads the same two fields in the same
  order, so no vector moves.
  Carrier verified in all four configurations: Wasm-clean core (no features,
  wasm32), `browser` on wasm32, `native` all-targets, and 66 tests green.
  The core's direct deps are now `blake3`, `luggage`, `thiserror`, and an
  assertion over the resolved tree confirms none of Luggage's native pipeline
  (reqwest, tokio, dirs, tempfile, minisign-verify, base64,
  cargo-packager-utils) reaches it.
  Bundle cost measured rather than assumed, against a baseline taken before
  the change (970,489 B raw / 223,700 B gz). With the invite path dead —
  webrtc-ping never names it — the delta is **zero raw bytes**: dead-code
  elimination removes the whole of Luggage. With a throwaway export forcing
  `InviteV1::parse_fragment` and `ReleaseRefV1` live, the cost is **+17,886 B
  raw (+1.84%) and +5,221 B gzipped (+2.33%)**, and that figure carries
  InviteV1's entire encode/decode/base64 machinery, not release identity
  alone. Removing the throwaway restored the baseline byte-for-byte, which is
  the control that makes the reading trustworthy.
  One Cargo constraint worth recording, because it is invisible until it
  bites: **a member cannot turn a workspace dependency's default features
  off.** `luggage = { workspace = true, default-features = false }` is a hard
  manifest error, so the workspace declaration carries
  `default-features = false` and members opt *up* with
  `features = ["native"]`. Forgetting to is a compile error naming the missing
  item, not a silently crippled updater.
