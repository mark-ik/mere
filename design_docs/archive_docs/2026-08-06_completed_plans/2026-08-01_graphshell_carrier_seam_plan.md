# Graphshell Carrier Seam Plan

**Date:** 2026-08-01
**Status:** complete. C0, C1, and C3 landed; C2 followed on C3's evidence and
moved the session machinery to `graphshell-client` and the carrier body to a
new `graphshell-network`. K2 consumed it on 2026-08-06, so this is ready to
archive; C3's carrier error type gained the `Refused`/`Disconnected` split
there, which is recorded in the Knot plan rather than restated here.
**Depends on:** nothing. Every step below is independent of the authority
decisions open under Turnstone's place port (the DCGKA carrier and the shared
Knot space), which is the reason to do this now.

## Why

Graphshell's README states the design: `chirograph` carries
"versioned session messages over an **unspecified carrier**", and
`graphshell-stdio` is "the **first** local newline-delimited JSON carrier".

The code does not implement that yet. There is no `Carrier` trait anywhere in
`crates/graphshell/` or `ports/graphshell/`. `StdioCarrier` is a concrete
struct and `RetainedEndpointSession` holds an `Option<StdioCarrier>` by type.
So today there is exactly one carrier and no seam to add a second.

The consequence is not abstract. Turnstone drives the Knot editor by spawning
`ports/knot`'s binary and talking to it over that stdio carrier, which means
**the Knot editor cannot ship on two of three targets**:

| | Native desktop | Web | Mobile |
|---|---|---|---|
| Subprocess + stdio | works | impossible | impossible (app sandbox) |

Not slow on web and mobile. Absent. A browser tab cannot spawn a process and
neither can an iOS app.

## What the seam is

`StdioCarrier`'s whole surface, minus its constructor:

```rust
fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String>;
fn take_notice(&mut self) -> Option<CarrierNotice>;
fn wait_for_notice(&mut self) -> Result<CarrierNotice, String>;
fn shutdown(self) -> io::Result<()>;
```

Four methods. `spawn` is stdio-specific and stays out.

## Steps

### C0. Extract the trait - DONE 2026-08-02

`chirograph::Carrier`, implemented by `StdioCarrier`, with
`RetainedEndpointSession` holding `Option<Box<dyn Carrier>>`. No behaviour
change, no new carrier, no moved code. chirograph 81, stdio 10,
graphshell 5, and Turnstone builds unchanged.

Three decisions the extraction forced, none of them free:

- **Not `Send + Sync`.** A browser carrier is single-threaded, and requiring
  thread-safety on the trait would exclude the target the protocol most needs
  to reach. A host wanting a carrier on another thread owns that choice.
- **`shutdown` takes `&mut self`, not `self`.** A consuming method cannot live
  on a trait object, which is the entire point of the seam. `StdioCarrier`'s
  body only ever needed `&mut self`; the consuming form stays as an inherent
  method, so existing callers are untouched.
- **The trait object is `+ 'static` explicitly, everywhere.** `&mut T` is
  invariant in `T`, so an elided `'_` will not accept the `'static` object the
  boxed field holds. Writing the lifetime out at every signature is not
  ceremony; eliding it does not compile.

The blocking surface is unchanged and still the open question for C3.

### C1. The in-memory carrier - LANDED 2026-08-02 (crate); K0 consumes it next

`graphshell-local`, a carrier crate parallel to `graphshell-stdio`:
`LocalCarrier<E, F>` wraps a `CompleteEndpoint` and answers through
`dispatch_common`. Four receipts, including one whose only job is to fail if a
message stops round-tripping.

**It serializes, and the decision below stands as recommended.** Every request
and response goes through the same `serde_json` encoding the wire uses; only
I/O is skipped.

Three things the implementation settled:

- **`CompleteEndpoint`**, a blanket-implemented supertrait over the four
  `dispatch_common` already requires, so a carrier says "a complete endpoint"
  once instead of repeating four bounds at every signature. No adapter
  implements it directly.
- **The session plane is refused by name.** `Open`, `Close`, and `Suspend` are
  the host's to answer and an in-process host has no session plane. Refusing
  and saying so beats returning a success that means nothing.
- **`wait_for_notice` does not block.** An in-process endpoint has already
  produced whatever it will produce by the time the host asks, so a caller
  that wants to wait owns that loop; only it knows what it waits for and how
  long is too long. This is the first real evidence for the C3 blocking
  question: the surface's blocking shape is a stdio assumption, not a
  protocol one.

The crate is landed and tested; **wiring Turnstone's Knot path onto it is K0**
in the Knot plan, and that is what retires the subprocess.

### C1 (original scoping)

The degenerate case: endpoint adapter and client in one process, no I/O. This
is what makes "an embeddable component" and "a projectable remote surface" the
same statement rather than competing ones, because the local case stops being a
different code path and becomes a different carrier.

**Decision, and it is the load-bearing one:** does the in-memory carrier still
serialize? Recommend **yes** — encode and decode the same message types,
skipping only the transport. It costs a round trip through serde on a local
call and buys the guarantee that the local path exercises the real protocol.
Bypassing serialization gives the local case a private path that will silently
diverge from the wire, and the divergence surfaces as "works locally, breaks
remotely", which is the worst place to find it.

**Done when:** Turnstone opens a Knot document with no subprocess and no
`TURNSTONE_KNOT_ROOT`, through the same protocol messages the stdio carrier
sends.

### C2. Where the session machinery lives

`RetainedEndpointSession` is reusable session machinery, and it lives in
`ports/graphshell`. Turnstone consumes it from there, so a product depends on
another product's port for something the README describes as belonging to the
crate layer.

Decide after C0/C1, when the trait has shown what actually generalises: either
it moves to `graphshell-client`, or it stays and the layering note is written
down rather than left as drift.

**What C3 showed, 2026-08-06.** The line does not fall where the module
boundary does. `NetworkCarrier` itself, the framing and the notice queue,
needs `chirograph` and tokio and nothing else; it is generic over any
`AsyncRead + AsyncWrite` and never learns who its peer is. Its two companions
in the same file, `dial_projection_session` and `projection_binding`, need the
ALPN and the admission vocabulary, which are the port's.

So the port currently holds one crate-shaped thing and one port-shaped thing
in one module, which is the same complaint C2 records about
`RetainedEndpointSession`, now with a second instance and a clean seam through
it:

- crate-shaped: `NetworkCarrier`, `CarrierRuntime`, and the session machinery
  `RetainedEndpointSession` already is. A `graphshell-network` crate parallel
  to `-stdio` and `-local` is the obvious home, and the three would then be
  the same shape as each other.
- port-shaped: `dial_projection_session` and `projection_binding`, which
  belong beside `accept_projection_session` because they share its ALPN and
  its admission vocabulary. The dialling half and the accepting half of one
  service want to stay adjacent.

**DONE 2026-08-06 (Mark's call).** The split above is the split that landed.

- **`graphshell-network`** is founded, beside `-stdio` and `-local`, holding
  `NetworkCarrier` and `CarrierRuntime`. Its whole dependency list is
  `chirograph`, `serde_json`, and `tokio`; it never learns who dialled
  or which ALPN they asked for. The port keeps `dial_projection_session` and
  `projection_binding` and re-exports the carrier types, so a caller still gets
  a projection carrier from one place.
- **`RetainedEndpointSession` moved to `graphshell-client`**, with
  `resume_after_notice`, `resume_request_for_notice`, and `unexpected`. C2's
  original complaint is answered: Turnstone now names the crate layer
  (`graphshell::client::RetainedEndpointSession`) rather than another product's
  port.
- **`action_draft` moved with it**, because the session type constructs drafts
  and there was no seam between them worth inventing. The port's `web.rs` was
  already consuming it from a wasm target, which is the second consumer that
  makes it crate-shaped rather than merely movable.

Two things the move settled that scoping had not:

- **`spawn` could not come along.** It built a `StdioCarrier`, and a session
  type that holds `Box<dyn Carrier>` has no business knowing processes exist.
  It is now `graphshell::sessions::spawn_endpoint_session`, a free function in
  the port beside the other stdio deployment code, and `over` is the type's
  only constructor. That is the honest shape: constructing a carrier is the
  host's business, and each carrier needs different things to construct.
- **`carrier_mut` had to become public.** The G4 receipt harness drives every
  advertised action through the raw carrier, which the wrapper does not model.
  It is documented as an escape hatch whose caller owns client-state
  consistency, rather than pretending the wrapper covers every verb.

What stayed in the port is what needs it: the stdio deployment, the receipt
views, and the G4 harness. Nothing moved that a second host would not want.

### C3. A network carrier - LANDED 2026-08-06; K2 is unblocked

Four things this settled, two of them unscoped when C3 was written.

**Most of the server half already existed, and the client half did not exist
at all.** `carrier.rs` has accepted admitted projection sessions since G5d,
and `session_loop.rs` has served NDJSON over them just as long. What was
missing was the dialling side: exactly two `Carrier` implementations existed,
stdio and local, and the code that dials, handshakes, and speaks the protocol
lived hand-rolled inside the `g5_peer` receipt binary, where nothing could
consume it. `ports/graphshell/src/network_carrier.rs` is that binary's inner
loop, made into the third `Carrier`.

**The notice lane did not exist on an admitted session, and K2 needs it.**
`serve_admitted_session` never polled `ProjectionNoticeSource`, so a remote
client calling `wait_for_notice` would have waited on a frame the server was
never going to send. Stdio has had this since `serve_resumable_notifying`;
the admitted loop simply never grew it, and nothing noticed because no remote
client existed to be kept waiting. Two peers editing one document is precisely
the case that breaks without it: the second peer would see the first peer's
edit only when it next happened to ask. `session_notices.rs` adds it as one
poller passed into the existing loop, not a second loop, so authority
rechecks, lapse handling, and the session plane keep one owner.

Two details worth keeping, because both were nearly got wrong:

- **Poll before each read as well as on the timeout.** A peer sending faster
  than the poll interval never lets the timeout arm fire, so a loop that rang
  only on timeout would go silent for exactly the busy client that most needs
  the bell.
- **Drain, rather than one notice per wake.** An endpoint that moved several
  revisions while the peer was quiet has all of them pending, and delivering
  one per wake lets a busy source outrun its own bell.

**The two server halves already agreed on the wire, by a fortunate accident.**
Stdio writes a `CarrierOutput` envelope and the admitted loop writes a bare
`CarrierResponse`, which reads like a divergence. It is not: `CarrierOutput`
is `#[serde(untagged)]` and its two variants are structurally disjoint, so one
client decode reads both framings. Recorded because the next reader will spot
the asymmetry and reach for a fix that is not needed.

**The blocking decision held, and cost nothing.** No caller changed. The
carrier holds a runtime handle and blocks against it, which requires only that
the calling thread not be a runtime worker; that is what every caller already
is. `tokio::io::Lines::next_line` being cancel safe is what lets the poll arm
wrap a read in `tokio::time::timeout` without risking a partial frame.

**Proven by** `ports/graphshell/tests/projection_round_trip.rs`: a viewer
dials, the owner's policy admits it, the served loop answers `Open` and rings,
and a blocking `NetworkCarrier` on its own thread drives all of it over one
transport, with no subprocess and no stdio anywhere. Eleven further unit tests
cover the framing, the id matching, the queueing, and the drain.

### C3 (original scoping)

The remaining carrier, the one that makes remote projection real rather than
architectural, and **the blocker for the Knot plan's K2**: projecting a
place-held document needs a member to reach the holder's endpoint, and stdio
and local both only reach this machine.

**It is much smaller than it looks, and the hard part is already proven on real
hardware.** Cross-machine connectivity is not the work here and never was:

- **Tickets** carry a peer across a network today. The place port's T3a receipt
  joins two peers over one, and that is a passing test, not a plan.
- **mDNS** exists (`MdnsDiscoveryMode`, and the `iroh-mdns-address-lookup`
  fork), with the macOS unsigned-binary limitation already recorded.
- **Relays** are supported (`relay_url` on the builder), and the transport
  documents the consequence plainly: "an unrelayed transport is a LAN-only
  transport". Relay was the first path that worked between the ThinkPad and
  the iMac.

So NAT traversal, hole-punching, discovery, and peer identity are done and
exercised across two real machines. C3 inherits all of it for free.

What is actually missing is narrow: **an ALPN and a framing loop.** Both sides
already have templates.

- **Client:** `connect_raw(peer, alpn) -> Connection`, then a bi-stream.
- **Server:** the transport already routes per-ALPN through `StreamQueueHandler`
  into queues `Transport::accept` drains, and ALPNs are first-class
  (`Alpn::new("mere/cable/v1")`). A graphshell ALPN sits beside the existing
  ones.
- **Framing:** stdio's newline-delimited JSON is carrier-shaped rather than
  stdio-specific, and moves onto a stream unchanged.

Calling this "a network carrier" overstates it. It is the graphshell protocol
spoken over a connection the place already has.

**Decision, settled 2026-08-02: keep the blocking surface, drive async behind
it.**

C1 supplied the evidence the scoping asked for, and it cuts both ways. The
in-process carrier does not block at all — `wait_for_notice` has nothing to
wait for, because an in-process endpoint has already produced whatever it
will. So blocking is a stdio assumption, not a protocol truth, and an async
trait would not be wrong in principle.

It is still the wrong move now, for a reason about callers rather than
carriers. `run_hub` is already a dedicated thread doing a blocking
`recv_timeout`, and Turnstone's place worker already owns a tokio runtime
inside a synchronous worker. A network carrier fits that shape with a runtime
of its own and no churn anywhere above it. Making the trait async instead
would colour `RetainedEndpointSession`, `KnotHub`, and every hub caller, to
serve one carrier that can perfectly well own its runtime.

**What would reopen it**, named so it is recognised rather than rediscovered:
**a browser reaching a *remote* endpoint.** Blocking is impossible on the web's
main thread, and a worker thread is not the same escape hatch there. Today
`graphshell-web` hosts its endpoint locally and needs no carrier at all, so
the case does not exist yet. When it does, the answer is an async sibling
trait rather than a conversion — the blocking one still suits every native
caller, and two shapes beat one shape that fits neither.

## Not in scope

- **Wasm.** A separate question, and not a trust one: Knot is first-party, so
  sandboxing is not the argument. The argument is whether the editor must be
  swappable or shippable without recompiling the host. If not, linking wins on
  simplicity. Note the browser asymmetry: no JIT means a wasm component inside
  a wasm32 app pays interpretation, while a linked component there is just code.
- **The shared Knot space's authority.** Open under the place-port plan's T4,
  and unaffected by any of this: a carrier decides how a session is carried,
  never who may write.
- **The DCGKA carrier**, open under T3b, for the same reason.
- **Presentation capabilities.** `PresentationCapability` (`NativeGlyph`,
  `PortableCard`, `Image`, `EditableText`) negotiates what a client can render.
  That is orthogonal to write authority and is not a second gate; it does not
  change with the carrier.

## Ordering

C0 then C1 unblock the local Knot case, are small, and depend on none of the
open authority decisions. C3 is the one that needs a decision first, and it is
the one that pays for the web and mobile targets.
