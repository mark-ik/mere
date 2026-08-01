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

### C0. Extract the trait

One trait from the surface above, implemented by `StdioCarrier`, with
`RetainedEndpointSession` generic or boxed over it. No behaviour change, no new
carrier, no moved code.

**Done when:** the Knot path works exactly as before, `ports/graphshell` and
Turnstone both build, and adding a carrier is a matter of writing an impl.

### C1. The in-memory carrier

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

The remaining carrier, and the one that makes remote projection real rather
than architectural.

**Decision needed first:** the surface above is blocking (`wait_for_notice`).
A network carrier wants async, or the same blocking shape driven on a worker
thread. Both work; they differ in what they impose on every existing caller.
Settle it before C3 and not before C0, because C0 and C1 are unaffected either
way.

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
