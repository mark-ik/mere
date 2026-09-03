# Browser WebRTC Carrier Plan

**Date:** 2026-08-25
**Status:** in progress. C0-C2 landed 2026-08-26. C3 landed 2026-08-28: the
forced relay is physically proven over a TURN relay on a second machine, so the
stop line is CLEARED. C4 landed 2026-09-02 (C4a 09-01, C4b 09-02): the real
Graphshell web client mounts a native-owned projection over WebRTC in both
surfaces, with the browser carrier profile marked physically proven in the
remote projection plan. C5, public rendezvous, is the next phase.
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

**The browser glue and the headed receipt landed 2026-09-01. C4a is closed.**
`graphshell::webrtc_browser` (behind `webrtc-browser`, wasm32 only) is the
last seam: `BrowserFrames` adapts the carrier's `BrowserInitiator` to the
join's `JoinFrames` — an inbox and a waker, `poll_fn` over both — and
`BrowserJoin` runs `peer_join`/`peer_rejoin` over it, then hands the channel
to `SessionDriver` as NDJSON lines through a `LineAssembler`. Fingerprints
come from the `a=fingerprint:` line of each SDP, parsed with the carrier's own
`DtlsFingerprint::parse_sdp_attribute`. The join sequence itself moved to
`webrtc_join.rs`, wasm-clean, so the host half and the browser half share one
protocol file and one set of tests; `webrtc_door` split the same way (the
client functions ungated, the issuing side native). The identity stack builds
for wasm32 — personae without `agent`, getrandom 0.3 on `wasm_js`, uuid on
`js` — and the port's `native` feature implies `webrtc-join`, so the default
build is unchanged.

The receipt is `Code/testing/mere/webrtc_c4a_receipt.md`: real Chrome against
the product host, one page load, five actions — invite, join, refused intent,
admitted intent, reconnect — with every fact in one JSON line and a clean
console. The revision moved exactly once, for the admitted intent, and the
admitted change was read back *by diff* off the bell. The reconnect was a
rejoin: same subject, second session id, invitation use count untouched.

The headed run caught three defects no native test had reached, each fixed in
shipping code with a test rather than in the page: the fixture registered its
endpoint with `register_notifying`, which erases resume (now
`register_resumable_notifying`, and the composition test resumes by diff);
the session core had no model for discovering *again* on a live session, so a
reconnect's first descriptor arrived as an answer nobody asked for (now
`SessionCore::rediscover`, and `SessionDriver::discover` routes through the
core when one exists); and the page dropped a retired `BrowserInitiator` while
its own close was still delivering into it — the C1 never-drop rule, one layer
up — so `BrowserSession::retire` now hands back a `RetiredSession` to park,
making the rule a type rather than a convention.

Deferred to C4b with the surfaces: replacing `canary::FixtureEndpoint` in
`web.rs` with the real mount, which is the point at which the session meets
the canvas. A row exercising resume-on-reconnect with a native change during
the outage is also carried; the machinery it would use is the one the
admitted-intent row proved.

### C4b: assessment (2026-09-01)

C4b owns the two remaining done-conditions of §7 — keyboard and
accessibility-tree checks in the embedded and full-page surfaces — plus the
real mount deferred from C4a and the profile mark for the remote projection
plan. What follows is what the code says today, then the phases and the
decisions they hang on.

**The web client as it stands.** `ports/graphshell/src/web.rs` (1,985 lines)
is the H3 reference host page. `BrowserHost` holds a
`GraphshellApp<IndexedDbBackend>` for the local graph and a
`canary::FixtureEndpoint` for the remote session, mounted in-process by
`app.mount_remote(remote.snapshot(..))` and driven *synchronously*: an intent
is `self.remote.invoke(..)` followed by a re-snapshot (`web.rs:954`), and the
scene is read back from `app.client.mounted(&remote_session)`. Eleven sites
touch the remote session this way. The chrome is a Cambium view
(`web_view.rs`) presented over WebGPU (`web_gpu.rs`); every fact the chrome
shows is mirrored into the DOM under `#semantic-host` by `update_semantics`
(`aria-pressed`, `aria-hidden`, `role="status"`, and `data-*` tokens on
`<body>` — `data-session`, `data-detail-open`, `data-action-count` — plus the
title `GRAPHSHELL H3 READY`). `loader.js` derives an accessibility tree from
that DOM (`semanticNode`: role, label, `aria-hidden` filtering), which is what
the H3 receipts checked. The canvas is `role="application"` with an
`aria-label` naming the keys, and `web_events.rs:189` maps them: arrows pan,
`+`/`-` zoom, Enter opens detail, Escape closes it, form fields excepted.

**Four findings that shape the phase.**

1. *Two `ClientState`s.* `GraphshellApp.client` holds the local mount;
   `SessionCore` owns its own (`core.client()`). A WebRTC-mounted session
   lives in the driver's core, which the presenter never reads. The mount
   is therefore not a swap of endpoint but a change of *where the remote
   scene lives* and *when answers arrive*: every synchronous remote call
   becomes ask → send line → later `on_line` → outcome → `update_semantics`.
2. *The keyboard handler is document-wide.* An embed with that listener
   steals its host page's arrow keys. It has to become root-scoped, keyed to
   focus inside the component — which is also the keyboard check worth
   making: keys outside the component do nothing to it.
3. *DOM addressing is by global id* — 13 `element(&document, id)` /
   `set_text` sites across `web.rs` and `web_product.rs`, and `index.html`
   owns the ids. An embed in a page that has its own `#search-input` breaks.
   The mirror has to address a root element, not the document.
4. *There is no browser harness.* `docs/2026-08-06_browser_storage_persistence_receipt.md`
   closes with a stop rule: the next browser-facing claim decides whether
   receipts stay hand-taken or become wired. C4a's receipt was hand-driven
   through a real Chrome (DOM-addressed clicks, not synthetic OS input).
   C4b is that next claim.

Also found: the remote projection plan's condition for the browser carrier
profile — *"claim this profile only after a headed browser-to-native session
reconnects"* — was met by C4a's rejoin row. The mark is a doc edit and a
timing decision, below. And the a11y work under way in genet (`genet-render`,
`inker`) does not touch this lane: the web client's accessibility surface is
the DOM mirror, and it imports nothing from `genet_render::a11y`.

**Phases.**

*C4b.1 — the real mount.* `BrowserHost.remote` becomes a link with two
realizations: the in-process fixture (kept, for a page with no invite) and a
WebRTC session (`BrowserJoin` → `BrowserSession` → `SessionDriver`), with one
accessor answering which `ClientState` holds the remote scene. The eleven
synchronous sites are rewritten against the core's operations; answers land
through the `BrowserSession::next_line` loop and re-run `update_semantics`.
Cards from `LiveEndpoint` present through the existing portable-card path
(`CapabilityProfile` already declares `PortableCard`). *Done when* the page,
given an invite, mounts the fixture host's live board on the canvas, shows
the refused and admitted intents through the existing action surface, and
reads the admitted change back by diff off the bell — the C4a receipt's rows,
now rendered.

**C4b.1 landed 2026-09-02.** `ports/graphshell/src/web_remote.rs` is the
link: `RemoteLink::{Fixture, WebRtc}`, one accessor
(`BrowserHost::remote_client`) answering which `ClientState` holds the
remote scene, and the eleven synchronous sites rewritten over it. The
WebRTC realization holds the `SessionDriver`, an outbound queue a writer
task drains onto the channel (`BrowserSession::writer`, the glue's new
outbound half, so one task writes while another sits in `next_line`), and
the [`RemoteOp`] in flight; a reader task folds every line the host writes
through `on_remote_line`, and the answer finishes the operation and begins
what it implies — a mount after discovery, a poll after an acceptance, a
resume for every bell, a resnapshot after a refusal so "unchanged" is
measured rather than assumed. `?signal=<url>` (and `?invite=`) makes
`loader.js` call `connect_remote` once the host is ready; without it the
fixture stays, as ruled. The endpoint's advertised actions render as
buttons under `#remote-actions` (one per intent, `data-intent` on each),
so the accessibility tree carries what the endpoint offers and a scenario
can press it; a plain action submits at once, a bounded form opens as the
draft it always did. The live fixture's cards require `NativeGlyph`, which
the remote profile now declares.

Receipt: `web/scenarios/c4b1_live_board.scn` against `c4_webrtc_host`,
`RESULT ok` in 61 frames — join, mount at revision 1 with one card, the
forbidden intent `Rejected` with the revision standing after a resnapshot,
the append `Accepted`, the bell resumed by **`diff · 1 → 2`**, two cards on
the canvas, no page errors; `h3_boot` still green on the fixture path.
Captures show the board, the detail surface and the status text. Two
findings: a poll answers `Changed`, not a descriptor (the probe never looked
at that outcome, so it never mattered); and on loopback an intent round trip
plus a resnapshot complete inside one animation frame, so `wait` may hold
zero frames and is still correct. The wasm32 identity stack needs
`--cfg getrandom_backend="wasm_js"` in the web crate's rustflags; the
machine-local config carries it and a committed
`.cargo/config.toml.example` says so.

*C4b.2 — one component, two surfaces.* The wasm entry becomes a mount on a
root element; `index.html` is the full-page surface calling it on the body,
and a second page places the same mount beside unrelated content of its own
(its own inputs, its own headings) as the embedded surface. Every DOM lookup
and the keyboard listener scope to the root. *Done when* both pages run the
same bundle and the same `BrowserHost`, and the embed page's own controls are
untouched by the component's keys and ids.

**C4b.2 landed 2026-09-02.** One component, two surfaces. The markup that
was `index.html`'s body is `web/component.html`, shipped inside the wasm
(`include_str!`), and `mount(root)` — a `wasm_bindgen` export — writes it
into whatever element it is given and runs the host there; `loader.js`
exposes the same entry two ways, `window.mountGraphshell(element)` and the
custom element `<graphshell-view>`, which mounts on connect. Every id in
the component is prefixed `gs-`; every Rust lookup goes through `root()`
and `element(part)` (a `querySelector` under the root, never
`getElementById`); every `data-*` token sits on the root rather than the
body; and the click, change, input and keydown listeners are on the root,
so keys with focus outside the component never reach it. The stylesheet
positions the overlay within the component (`position: absolute` in a
`position: relative` box) instead of the viewport, and `html.full-page` /
`body.full-page` carry what used to be `:root` rules. `index.html` is the
full-page surface (`<graphshell-view data-owns-title>` filling the body);
`embed.html` is someone else's page — its own title, heading, an input and
a button deliberately carrying the component's *old unprefixed* ids
(`search-input`, `session-local`), and the component in a 960×600 box
between them. The extension packaging is untouched: it copies `index.html`,
`styles.css` and `loader.js`, and the markup now travels in the bundle.

Receipts: `c4b2_full_page.scn` and `c4b2_embed.scn`, both `RESULT ok`, no
page errors, the browser closed after each. The embed rows: the host title
and heading intact; a key typed with focus on the host's input counted by
the host and leaving the camera where it was; a click on the host's
`#session-local` counted by the host with the component's session
unchanged; then, with focus on the canvas, the same key moving the camera
and reaching the component as `command pan-left`, Enter and Escape opening
and closing the detail. `compare-graphshell-surfaces.py` over the two
receipts: **53 semantic nodes each, one difference**, the viewport label
(`1282 by 722` vs `960 by 600`), which is the component measuring its own
box. `h3_boot` and `c4b1_live_board` rerun green on the new surface.

Findings. A pan's distance is velocity-shaped and differs run to run
(108 px, then 134 px), so the snapshot exposes `camera-x`/`camera-y` and
scenarios assert a bound, not a pixel. The receipt profile persists across
runs, so the page fetches scenarios with `cache: "no-store"` and the sink
sends the same header; an edited scenario had otherwise run stale. Chrome's
default `WebRtcHideLocalIpsWithMdns` returns to a fresh profile and hides
every host candidate behind `.local` names; the driver disables it on the
command line, which the user profile had done by hand since C1. And the
web build now resolves genet from a clean worktree at genet's HEAD
(`Code/worktrees/genet-head`, the web crate's machine-local config) rather
than the working copy: the other lane's in-flight livery edits had broken
the build mid-refactor, and a receipt must not depend on someone else's
unsaved state. Not in scope, stated: one instance per page — the `gs-`
prefix isolates the component from its host, not two components from each
other.

**C4b.3 landed 2026-09-02; C4b is closed, and with it C4.** The receipt is
`Code/testing/mere/webrtc_c4b_receipt.md`: five page-driven receipts, all
green, the keyboard and accessibility rows in both surfaces with equal
semantic trees, and the carried resume-on-reconnect row. That row needed
three things. The fixture opened a `LiveEndpoint` per session, so a change
during an outage could not reach the peer that came back;
`SharedLiveEndpoint` is one board behind a lock, handed to every session,
and `POST /nudge` moves it natively (`LiveEndpoint::append`, the same path
an admitted intent takes). The page gained `remote-disconnect` (close from
this end; the driver, core and mount kept), `remote-reconnect`
(`BrowserJoin::complete_rejoin` with the retired session's subject and
delegation, then a rediscovery that keeps the mount and a poll that rings
the missed bell) and `remote-nudge` (the receipt hook). Observed: the host
served the first session five requests and saw it end `Disconnected`,
appended natively to revision 3, admitted the same subject on a new
transcript, and the page resumed by `diff · 2 → 3` with three cards on the
canvas — never a re-snapshot. The remote projection plan's browser profile
is marked physically proven, with its condition (a headed browser-to-native
session reconnects) quoted against the row that met it. The driver runs the
fixture itself on request, so a live-board receipt starts from revision 1.

*C4b.3 — the checks and the receipt.* Keyboard: with focus on the canvas,
arrows move `data-camera`, Enter and Escape toggle `data-detail-open`, Tab
reaches every control in the semantic host, and in the embed the same keys
with focus outside the component change nothing in it. Accessibility: the
`semanticNode` tree of the component root is equal in both surfaces, and the
remote action form renders with its labels and descriptions (the G11 bounded
form path) from the WebRTC-mounted session. Both surfaces, one JSON receipt
each, plus the carried row: a native change *during* a disconnect, resumed by
diff on reconnect. *Done when* the receipt is in `Code/testing/mere/` and the
profile mark is in the remote projection plan.

**Ruled 2026-09-01.** The embed is *both*: a root-scoped mount entry and a
custom element over it, in this phase. Receipts are taken by a *page-side
scenario lane* — the page drives itself from a URL parameter with the
genet-probe verb vocabulary and reports into the DOM — which ends the
hand-taken era the 2026-08-06 stop rule named; it lands first, as C4b.0,
because every later done-condition is checked through it. The in-process
fixture *stays* as the no-invite realization beside the WebRTC one. The
profile mark lands *at C4b close*, with the rendered session behind it.

*C4b.0 — the scenario lane.* `?scenario=` names a script (a URL the page
fetches, or an inline body); verbs follow genet-probe's grammar as far as the
DOM allows — click by selector or text, key chords through the real keydown
path, act by command id, settle by frames, assert on text and attributes,
capture, log — and the run ends with a `RESULT ok|fail` line, the step log,
and any captures reachable from the DOM. *Done when* a scenario reproduces
the H3 boot receipt (title, storage tokens, semantic tree) without a hand on
the page, in both surfaces once C4b.2 exists.

**C4b.0 landed 2026-09-01** (full-page surface; the embed follows C4b.2).
`ports/graphshell/src/web_scenario.rs` consumes `genet-probe` — the same
parser and loop woodshed and turnstone run — and implements `Automatable`
and `Driveable` over the browser DOM: `act` is `run_command` (which now
answers whether the command exists), `assert snap` reads every `data-*`
token on `<body>` plus the title and the canvas camera, and the DOM verbs
the shared grammar lacks (`dom click`, `focus`, `key`, `type`, `click-at`,
`assert dom | attr | title | focused`) are the page's `app_step`. `capture`
composes the frame exactly as `present` does but into an owned `COPY_SRC`
target, reads it back asynchronously (`web_gpu::PendingCapture`; `wait`
holds on it through `busy`), and encodes through a 2D canvas so no image
crate joins the bundle. The page reports into the DOM as it runs
(`data-scenario`, `data-scenario-frames`, the live log) and, given
`?sink=`, POSTs the finished receipt and captures to a local sink. The
driver is `Code/testing/mere/scripts/run-graphshell-web-scenario.ps1` with
`graphshell-web-sink.py` beside it; receipts land in
`Code/testing/mere/scenarios/graphshell-web/<name>/` as `scenario.done`,
`result.json` and PNGs — woodshed's shape. Seed scenario
`web/scenarios/h3_boot.scn`: 27 steps, `RESULT ok` in 36 frames, two
1282×722 captures, no page errors, title, storage tokens and the whole
semantic tree in the receipt. The 2026-08-06 stop rule is answered.

Five findings from landing it, three of them about the instrument:

1. *DOM events must be dispatched after the tick.* The host's own listeners
   `borrow_mut` the host; a synthetic `keydown` dispatched from inside the
   tick, where the host is already borrowed, re-enters that borrow and
   traps — and the browser swallows a listener's exception, so
   `dispatchEvent` returns normally and the step looks run. The first
   scenario failed exactly there (Enter never opened the detail). Verbs now
   queue `DomAction`s; the frame pump dispatches them once the borrow ends.
   Page errors are also kept (`window.graphshellErrors`) and carried in the
   receipt, because `update_semantics` rewrites the title every frame and
   would hide a loader's `FAIL` title.
2. *A hidden tab gets no animation frames.* The app's Browser pane fired no
   `requestAnimationFrame` even while `document.hidden` was false, so a
   frame-pumped lane cannot run there; a real headed Chrome ran the same
   scenario in three seconds. The sink exists so the driver never needs to
   read the DOM of a browser it cannot reach.
3. *`rust-lld` crashes on this crate's DWARF* on the pinned 1.97.1
   toolchain (`0xc0000005`, reproducible, at HEAD without any of this
   work). Investigated 2026-09-02: it is size, not a linker bug. With
   `debug = "line-tables-only"` the module links at **1.59 GB**, of which
   `.debug_str` is 1.37 GB and the `name` section 123 MB against 40 MB of
   code; full debug info is larger still, and LLD 22.1.6 dies on it. Even
   the line-tables module is past the browser's 1 GiB limit, so
   `debug = 0` is the only setting that yields a loadable bundle, not a
   preference. The last bundle that linked *with* full debug info was
   2026-08-19 at 106 MB; the Livery/Buckram migration landed 2026-08-21.
   The module now holds 271,644 functions: 148k in `core` and **44k in
   `taffy`** (genet-taffy's generic layout, instantiated over Buckram's
   trees) — a monomorphization explosion that belongs to the genet lane,
   and the reason bindgen's demangled names (finding 4) run to 2 GB. Names
   are v0-mangled because 1.97 defaults to v0 and `legacy` is nightly-only
   now, so no mangling knob exists on stable.
   **Scoped 2026-09-02 (read-only, subagent).** The multiplier is not the
   DOM type (one `LayoutDom` in this build, `ScriptedDom`) but the *measure
   closure*: buckram implements all of taffy's tree traits on
   `AlgorithmRun<'a, S, Context, Source, Measure>`
   (`buckram/src/taffy_adapter/run.rs:9`) with the caller's measure closure
   taken by value, so every `compute_layout_with_measure` call site is its
   own `Measure` type and its own full copy of taffy's algorithm.
   genet-livery's `layout.rs` has twelve such call sites, most inside
   `<D>`-generic functions, plus a nesting wrapper
   (`positioned_intrinsic_sizes`, layout.rs:1360) that spawns a second-level
   closure per caller: **~14 distinct tree types**, confirmed two ways in the
   module (24 `round_layout` symbols ÷ 2). ~3,150 taffy functions per copy
   (taffy has 1,200 `fn`s and ~610 closures, all separate in debug). Every
   one of those symbols is codegen'd into **genet-render** (19,396 tagged
   `genet_render`, none `graphshell_web`), so a per-package profile knob on
   `taffy` cannot bite. The migration explains the growth: `04e303ac`
   replaced `genet-layout` with `genet-render` in the web manifest on
   2026-08-21, pulling the whole Livery/Buckram/taffy cone in two days after
   the 106 MB build. Secondary: the web lock carries *two* copies each of
   `read-fonts`, `skrifa` and `font-types` (parley/harfrust on 0.39/0.42,
   netrender/vello on 0.41/0.44) — ~3,000 functions, a version-alignment
   job. Fix shapes, all genet-owned: (1) erase `Measure` behind
   `&mut dyn FnMut` in the adapter — public `impl FnMut` entries unchanged,
   no genet-livery call site moves, ~38,000 functions and ~85% of taffy's
   `.debug_str` gone; (2) erase `Context` too, 2 → 1 tree types, more
   invasive, only if (1) is not enough; (3) `[profile.dev] opt-level = 1`
   (or per-package on `genet-render`/`genet-livery`/`buckram`) in the *web*
   manifest, which lets inlining collapse the 148k `core` adapters — a
   complement, not a fix, and the only knob mere owns. Ruled 2026-09-02:
   (1), then (2) only if (1) is not enough.
   **Shape (1) landed 2026-09-02 (genet, buckram).** `AlgorithmRun` holds
   `&mut dyn FnMut` (`MeasureFn`); the three public entries keep their
   `impl FnMut` signatures and coerce at the boundary, so no genet-livery
   call site changed. Measured on graphshell-web: taffy **44,095 → 9,036**
   functions, the whole module **271,644 → 158,317**; debug-info-off
   **167 MB → 70 MB**; line-tables-only **1.59 GB → 265 MB**, well under
   the 1 GiB limit. buckram 257, genet-livery's full suite, and
   genet-render's tests green. (1) is enough; (2) is not taken. The
   **full-debug-info link now succeeds** — 974 MB, so the crash is gone,
   though that bundle sits 10% under the browser limit and `debug = 0`
   (70 MB) or line tables (265 MB) remain the sensible browser builds; the
   driver keeps `debug = 0` for size, no longer for survival. The
   duplicated `read-fonts`/`skrifa` stack is still open, genet-owned.
4. *wasm-bindgen's demangled name section is 2 GB* on this module — 12× the
   167 MB it links to, past the browser's 1 GiB limit
   (`WebAssembly.instantiateStreaming(): size > maximum module size`).
   `--no-demangle` gives 162 MB; stripping the section gives 40 MB. Same
   root as finding 3.
5. *The canvas chrome renders no glyphs* in this build: pills, rail and
   status are bare boxes in both captures and in the pane's own screenshot,
   and `GraphshellSans.ttf` is never requested. Traced 2026-09-02: `web.rs`
   used to call `genet_layout::register_host_font(include_bytes!(
   "../web/GraphshellSans.ttf"))`; commit `04e303ac` (2026-08-21, "Migrate
   consumers to Livery and Buckram") removed that line with the retired
   crate and put nothing in its place. On the Livery path a font is
   per-`TextSystem` (`register_font_bytes`), genet-render's
   `scene_from_scripted_dom` → `compute_layout` calls the plain
   `genet_livery::layout`, which makes a fresh, empty `TextSystem`, and on
   wasm32 fontique has no system fonts to fall back on — so every native
   host shows text and the browser shows boxes. `layout_with_text_system`
   already exists; the fix is a genet-render entry that accepts a text
   system (or a host font registry) plus the one-line registration back in
   `web.rs`. `cambium-genet-web-host` has no font handling either, so it
   has the same gap.
   **Fixed 2026-09-02 (ruled by Mark):** genet-render gained
   `scene_from_scripted_dom_with_text_system` (and the paint-list twin) and
   re-exports `TextSystem`; `BrowserHost` keeps one with `GraphshellSans.ttf`
   (Roboto Regular) registered at boot, the chrome sheet names `Roboto`, and
   both captures now show brand, pills, rail, product proof and the WebGPU
   badge. The genet change is committed on genet `main`; mere's web manifest
   still pins genet at `eff0cb6`, so a clean checkout renders boxes until
   the pin is aligned past it — locally the config redirect makes it live.

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

- **2026-08-31 → 2026-09-01: C4a landed.** In order: the sans-I/O
  `SessionCore` with the blocking `RetainedEndpointSession` and the
  event-driven `SessionDriver` as its two adapters; the carrier's frame pump
  (`stream_over_frames`) and exported loopback harness; the join protocol over
  frames with `host_join`/`peer_join`/`peer_rejoin` and `serve_webrtc_join`;
  `ResidentProjectionHost::serve_admitted` so the WebRTC lane reaches the
  product host; `LiveEndpoint` with an advertised-and-refused intent; the
  `c4_webrtc_host` fixture with a test that drives the binary; and the browser
  glue `webrtc_browser` on a wasm-clean `webrtc_join`. Headed receipt in real
  Chrome: `Code/testing/mere/webrtc_c4a_receipt.md` — five actions, one JSON
  line, clean console, revision moved once and read back by diff, reconnect as
  a rejoin. Three defects caught only by the headed run (resume erased by the
  registration, no model for a second discovery, a retired channel dropped
  mid-close), all fixed in shipping code with tests. Recurring hazard worth a
  type if it appears again: a `CarrierControl` dropped early cancels the
  driver, found twice in the composition test and once as the page-side
  never-drop rule. Native tally: graphshell lib 154 under `webrtc-session`
  plus 2 composition and 2 signaling receipts; graphshell-client 46; carrier
  68. CARRIED to C4b: the real mount in `web.rs` and the surfaces;
  resume-on-reconnect with a change during the outage.

- **2026-09-01: C4b assessed and ruled; C4b.0 landed.** Assessment in §7:
  the web client's remote session is in-process and synchronous, two
  `ClientState`s, a document-wide keyboard handler, global-id DOM
  addressing, and no browser harness. Ruled: embed as both a root-scoped
  mount entry and a custom element; receipts by a page-side scenario lane;
  the in-process fixture stays beside the WebRTC realization; the profile
  mark at C4b close. C4b.0 is that lane: `web_scenario.rs` over
  `genet-probe`, GPU-readback captures, a receipt sink and driver under
  `Code/testing/mere/`, `h3_boot.scn` green in real Chrome. Findings: DOM
  dispatch deferred past the tick (re-entrant borrow), hidden tabs get no
  frames, `rust-lld` DWARF crash worked around with `debug = 0`, bindgen's
  2 GB name section worked around with `--no-demangle`, and the canvas
  chrome renders no glyphs in this build. Next: C4b.1, the real mount.

- **2026-09-02: both investigations closed; C4b.1 landed.** The glyphs:
  the Livery migration had dropped the host font registration; genet-render
  gained `scene_from_scripted_dom_with_text_system` and the web host keeps
  a text system with the shipped font (genet `893ccb9b3d9`). The size:
  buckram's measure closure was the multiplier; erased behind `dyn FnMut`
  (genet `577e2471e97`), taffy 44k → 9k functions, the module 167 → 70 MB,
  full debug info links again. C4b.1: `web_remote.rs` — `RemoteLink` with
  the fixture and WebRTC realizations, the driver's answers arriving through
  a reader task, advertised actions as DOM buttons; `c4b1_live_board.scn`
  green against `c4_webrtc_host` with the append read back by diff. Next:
  C4b.2, the two surfaces.

- **2026-09-02: C4b.2 landed.** The component's markup ships in the bundle
  and `mount(root)` puts it anywhere; `<graphshell-view>` and
  `mountGraphshell(element)` are the two ways in. Ids prefixed, lookups,
  tokens and listeners root-scoped, the overlay positioned within the box.
  `index.html` (full page) and `embed.html` (a host page with colliding ids
  and its own key and click counters) both `RESULT ok`; the semantic trees
  compare equal but for the viewport label. Driver: its own Chrome profile,
  closed after every run, mDNS candidate hiding off, no HTTP cache; the web
  build resolves genet from a clean worktree at HEAD. Next: C4b.3, the
  keyboard and accessibility rows as a receipt, the profile mark, and the
  carried resume-on-reconnect row.

- **2026-09-02: C4b.3 landed; C4 is closed.** `SharedLiveEndpoint` (one
  board for every session) and `POST /nudge` on the fixture;
  `remote-disconnect` / `remote-reconnect` / `remote-nudge` on the page;
  `c4b3_reconnect.scn` green: a native append while the link was down,
  rejoined as the same subject, resumed by `diff · 2 → 3`. The remote
  projection plan's browser profile is marked physically proven. Receipt:
  `Code/testing/mere/webrtc_c4b_receipt.md`. The driver runs the fixture
  itself (`-Fixture`) and the page posts progress while a run is alive, so
  a stall leaves its last state. Open, deliberately: one instance per
  page; the duplicated `read-fonts`/`skrifa` stack (genet); the web
  manifest's `[profile.dev]` setting; the DOC_README index line for this
  plan still reads "C4 open" and belongs to the lane holding that file.
  Next: C5, public rendezvous.
