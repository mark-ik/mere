# Graphshell Carrier Seam Plan

**Date:** 2026-08-01
**Status:** scoped, not started.
**Depends on:** nothing. Every step below is independent of the authority
decisions open under Turnstone's place port (the DCGKA carrier and the shared
Knot space), which is the reason to do this now.

## Why

Graphshell's README states the design: `graphshell-protocol` carries
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

`graphshell_protocol::Carrier`, implemented by `StdioCarrier`, with
`RetainedEndpointSession` holding `Option<Box<dyn Carrier>>`. No behaviour
change, no new carrier, no moved code. graphshell-protocol 81, stdio 10,
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

### C3. A network carrier

The remaining carrier, the one that makes remote projection real rather than
architectural, and **the blocker for the Knot plan's K2**: projecting a
place-held document needs a member to reach the holder's endpoint, and stdio
and local both only reach this machine.

**It is smaller than it looks.** No new transport is needed.
`P2pandaTransport::connect_raw(peer, alpn) -> Connection` already exists, and
a place already holds a live transport to its members with their peer ids
known. A graphshell carrier opens a stream over its own ALPN beside the seven
lanes, reusing everything the place established for admission and reachability.
Framing is already solved too: stdio's newline-delimited JSON is carrier-shaped
rather than stdio-specific.

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
