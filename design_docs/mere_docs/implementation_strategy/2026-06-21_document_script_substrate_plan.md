# DocumentScript Substrate Plan

**Status:** accepted (direction) 2026-06-21; **P0 sync probe green** (built same day,
`crates/probes/document-script-p0/`). Mark: go, betting on the future. The §9 open
decisions are resolved (see §9 head); §10 is the binding scope. Architecture proposal +
sequenced build-out.
Codebase-grounded; external upstream facts (Component Model, WASI 0.3, WasmGC)
verified against primary sources 2026-06-20 at the confidence levels stated.
**Date:** 2026-06-21.
**Scope:** a capability-scoped, typed, cross-language contract for what a *script*
or *extension* is allowed to mean in Mere, carried over the WebAssembly Component
Model, sitting **above** serval's JS-engine `ScriptEngine` rather than replacing
it.

The realistic dream is not "every language becomes interchangeable behind
`eval(source)`." It is: every language can implement the same typed,
capability-scoped application contract, with Wasm providing isolation,
portability, ownership, and scheduling. The missing invention is therefore not
universal execution (Wasm largely supplies that) but a good capability-oriented
definition of what a document script is *permitted to do*.

Related: [actor_constellation_plan](2026-06-03_actor_constellation_plan.md) (the
host kernel + actor boundary this rides on; the deferred "capability / wasm-component
isolation" seam this fills), [protocol_architecture_plan](2026-05-05_protocol_architecture_plan.md)
(the deferred Extism/Wasmtime/Boa scripting-host probe this supersedes),
[cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
(the no-JIT browser policy + the PWA-vs-open-web lane fork this inherits),
[polyglot_block_resolver_plan](../../nematic_docs/implementation_strategy/2026-06-13_polyglot_block_resolver_plan.md)
(the native-only wasm-block kind that wants this contract). Serval side:
`script-engine-api/lib.rs`, `script-runtime-api/fetch.rs`,
`docs/2026-05-20_serval_script_engine_plan.md`, `docs/2026-05-25_js_execution_strategy.md`.

---

## 1. The decision in one line, and where the boundary sits

Keep the JS-engine `ScriptEngine` trait for what it is good at (driving Nova
native / Boa wasm over reflectored DOM access), and add a **higher
`DocumentScript` boundary** expressed as a WIT world. A script is an actor that
consumes an event stream and issues mutations against a document revision; no
language-native value crosses the boundary; network, clocks, storage, layout and
rendering are separately granted capabilities.

The load-bearing structural claim, which the rest of the plan depends on:

> **The reflector model is not superseded. It is demoted to the *innards* of one
> particular interpreter component: the `html-document` one.**

`script-engine-api/lib.rs` is JS-value-shaped to the bone (`Self::Value`,
reflector identity, `pump_microtasks`, `settle_host_promise`, `set_function`
over bare `fn` pointers, per-target Nova/Boa selection). That is correct for what
it is, and it is the wrong shape for a cross-language WIT interface. The WIT
boundary is **coarse, async, command/event**; the reflector boundary is **fine,
synchronous, in-process**. They nest rather than compete (§3). (Confidence: high
on the trait shape; verified in source 2026-06-20.)

---

## 2. Findings

### 2.1 Current reality (verified 2026-06-20)

| Layer | Status | Confidence |
|---|---|---|
| Portable sandboxed execution | Mature core Wasm | high |
| Typed cross-language ABI (Component Model / WIT) | Developer Preview | high |
| Async, futures, streams, cancellation, backpressure | **Shipped in WASI 0.3.0, 2026-06-11** (nine days before this doc) | high |
| Managed-language runtime (WasmGC) | Ships in V8 / SpiderMonkey / JSC; does **not** cross the component boundary | high |
| Browser/system capabilities | Fragmentary; the host defines most | high |
| Native browser Component Model | Does not exist; CM 1.0 is **gated on two native browser engines, neither committed**; interim path is an AOT polyfill (jco transpile → core wasm + JS glue) | high |
| Universal document-script contract | Does not exist; this is the part Mere must design | high |

The single most recently moved fact: **WASI 0.3.0 with native async shipped
2026-06-11.** Async functions, `stream<T>`, `future<T>`, cooperative
cancellation, and counter-based backpressure are in the Component Model now.
Concurrency.md confirms both hard claims: it names the multi-language mapping
intent (C#/JS/Python/Rust/Swift async, Kotlin/C++ coroutines, Go/Java green
threads, host threads) and states the async model "composes with but doesn't
actually depend on" the core stack-switching proposal (it can be polyfilled via
JSPI). So Rune-style async is exactly the sort of runtime the 0.3 ABI is built to
accommodate, rather than an argument against Wasm. (Confidence: high; primary
source `WebAssembly/component-model` Concurrency.md.)

### 2.2 The critical limitation: WIT is not a universal object model

WIT exchanges copied records, variants, lists, strings, resources, futures and
streams. It cannot transparently exchange arbitrary language object graphs,
language-native closures, GC pointers, exceptions with identical semantics,
shared mutable language heaps, or cross-language cyclic garbage. Choices.md is
explicit (direct quotes, verified): the model "assumes no global inter-component
garbage or cycle collector"; resources "require explicit acyclic ownership
through handles" with destructors "called deterministically"; and it "assumes
that Just-In-Time compilation is not available at runtime." (Confidence: high on
the prohibitions as design intent; medium that the negative list is a single
verbatim enumeration. It is inferred from the resource + linking model, not one
bullet list.)

This is why **WasmGC does not solve Nova/Python/Rune interoperability.** WasmGC
improves how a managed language represents *its own* heap inside one component;
the component boundary still consists of values and resource handles. It rules
out the fantasy where every language shares one DOM object graph, which is the
same conclusion the two-heap reflector finding reached from the other direction
(the DOM and the JS heap are two arenas bridged by reflectors, never one heap).

### 2.3 Codebase grounding (what exists, what is deferred)

- **Absent as a top-level idea.** Neither serval/docs nor mere/design_docs frames
  the Component Model as a cross-language script substrate today. It is not ruled
  out; it is simply not in the architectural conversation. (Confidence: high; full
  design-corpus sweep 2026-06-20.)
- **The deferred seam is named.** [actor_constellation_plan](2026-06-03_actor_constellation_plan.md)
  lists "capability / wasm-component isolation" as a *future plugin seam, not a
  near-term primitive*, and descopes the in-process wasmtime sandbox (P5) on the
  grounds that it "would require compiling the entire serval + Nova content engine
  to wasm32." **That reasoning does not touch this proposal:** we confine the
  *script/extension*, not the *engine*. The host engine stays native and supplies
  the imports; the thing compiled to wasm is a Rune runtime or a Rust extension,
  which is small. (This holds for the app/extension lane. For `html-document` the
  script *is* the engine, so the descoped cost returns; the escape is overbroad as
  written. See §10.1.)
- **The Extism gesture.** [protocol_architecture_plan:650](2026-05-05_protocol_architecture_plan.md)
  named Extism as "the right shape for a plugin model, typed host calls, defined
  boundaries," then deferred with "don't pre-commit before knowing the capability
  surface." `DocumentScript` is the principled Component-Model successor to that
  probe.
- **The deferred-fetch seam already fits.** `script-runtime-api/fetch.rs:86-118`
  carries the async shape (`start() -> Option<FetchOutcome>` returns `None` to
  leave the promise pending; `cancel(id)` for abort; `request_chunk(id)` for lazy
  streaming) and calls itself "the actor-mailbox seam." Only the file header still
  describes the synchronous default. This is the right shape for a `network`
  capability with an async `fetch` and a `stream<u8>` body.
- **The scripting map is Rust + JS** (2026-06-10): Rune and Rhai were dropped, on
  the axis of *first-party language placement* (async is JS's home turf). The
  reopen trigger written down is "a Rune 1.0 with a sandbox warranty." See §9.

---

## 3. The two integration kinds, and the coarse/fine impedance

You need both shapes, and the second is where the reflector model lives.

**Compiled component.** Rust, C#, Swift, Go, or a future Rune-to-wasm compiler
produces a component implementing `document-script` directly. Cleanest isolation,
smallest semantic impedance. This is the app/extension lane's native form.

**Interpreter component.** A runtime (Nova, Piccolo, Rune, Rhai, Python) is
packaged as a component; source or bytecode arrives as data; the adapter
translates the WIT document world into that language's native objects. Rune
support therefore does not require a Rune-to-wasm compiler: compile the runtime,
or keep it native initially, and make its adapter implement the same
`DocumentScript` behavior.

**The impedance to state plainly.** The contract is a coarse async command/event
boundary (apply `list<mutation>` against a revision; consume an `event` stream;
`inspect` a copied `document-view`). Legacy web JS expects fine-grained
synchronous DOM access with interleaved reads after writes, and synchronous
forced layout (`getBoundingClientRect`, `getComputedStyle`, `offsetWidth`). You
cannot run that against an async `apply`. So you do not:

> The `html-document` profile is an **interpreter component that carries Nova plus
> a full DOM shim**, services synchronous reads from its *local* mirror, mutates
> that mirror in place, and crosses the WIT boundary only at flush and inspect
> points.

That is the Servo model (script owns a DOM; layout is reached by message) recast
as a component boundary, and it is exactly the existing `serval-scripted-dom` +
reflector machinery, now scoped as the innards of one component instead of the
top-level architecture. It also classifies the profiles cleanly: `tree-document`
and `document-core` scripts can speak the contract directly; `html-document`
always needs the interpreter-component wrapper because of synchronous layout.
(Corrected by review: legacy web JS stays the existing *native* `Runtime<Nova/Boa>`
lane, not a Wasm component, and leaves P0/P1. See §10.1.) The
forced-reflow wall is a **feature for the app lane** (it eliminates layout
thrashing by construction) and a **hard wall for legacy**, which is precisely why
legacy lives inside the wrapper. (Confidence: high on the architectural
necessity; the cost of the per-interaction boundary is unquantified, §7.)

---

## 4. Profiles, not fake universality

Formats and protocols select profiles; adapters compose compatible versions. A
Gemtext script must not automatically receive an HTML DOM. Define small worlds
and let the format pick:

- `document-core` (observe + mutate a generic tree, events, lifecycle)
- `tree-document` (typed node tree without HTML layout semantics)
- `html-document` (the interpreter-component wrapper of §3)
- `layout-query` (measurement; **async by design**, so it cannot reintroduce
  synchronous forced reflow)
- `canvas`
- `peer-messaging` (murm / moot capability, granted, not ambient)
- `persistent-storage` (eidetic / OPFS, granted, scoped)

Avoid one giant "browser" interface. The capability-gate catalogue
([capability_gate_catalogue_brief](../research/2026-05-14_capability_gate_catalogue_brief.md))
is the natural home for which profile a given origin/format/extension is granted.

---

## 5. The WIT vocabulary sketch

**Illustrative-signature-only** (shape, not compile-ready; exact syntax will move
with the spec):

```wit
package mere:script@0.1.0;

interface document {
    resource session {
        events: func() -> stream<event>;
        apply: async func(expected-revision: u64, changes: list<mutation>)
            -> result<u64, document-error>;
        inspect: func(query: document-query) -> result<document-view, document-error>;
    }
}

interface network {
    fetch: async func(request: request) -> result<response, network-error>;
    resource response { body: func() -> stream<u8>; }
}

world document-script {
    import document;
    import network;
    import clock;
    import log;
    export run: async func(session: own<document.session>) -> result<_, script-error>;
}
```

The shape that matters: a script is an actor over an event stream; mutations are
commands against a revision (optimistic-concurrency `expected-revision`); network
/ clock / storage / layout are separately granted; backpressure and cancellation
are in the protocol; the host keeps document, origin, protocol and security
authority. `inspect` is synchronous but returns a *copied snapshot*, which is the
seam an interpreter component refreshes its local mirror through.

---

## 6. What must be built (phases, native-first)

The viable first target is **native Mere using a component runtime, a Mere-owned
WIT world, and native adapters for engines that cannot yet live in wasm** (Nova
stays native while Memory64 / Nova portability mature; Memory64 is not a
prerequisite). Later, the same contract becomes the browser/polyfill and
remote-component boundary.

- **P0 — decision + probe (start here).** Settle the Wasmtime-on-native
  dependency call (§7.3) and the descope reconsideration (§9). Stand up a minimal
  `document-core` world + a direct-Rust component proving the contract end to end
  (events in, mutations out, one capability granted), independent of any
  interpreter. Decision-gating; no host wiring yet. **Done 2026-06-21:**
  `crates/probes/document-script-p0/` — a direct-Rust `document-core` guest
  component driven by a Wasmtime 45 host, green end to end (per-turn
  `handle-event`, both mutation variants, both typed-error variants incl.
  `revision-conflict(u64)`, the `log` capability called from the guest with only
  WASI + `log` linked). Findings folded into the Progress log and README.
- **P1 — the versioned WIT vocabulary.** `document-core` observe/mutate/events,
  `network` (over the `fetch.rs` deferred seam), `clock`, `log`, `lifecycle`.
  Small worlds, versioned; avoid the giant browser interface.
- **P2 — component host actor.** One armillary actor owning each script instance:
  mailbox delivery, cancellation, quotas, deterministic teardown. Components need
  not be `Send`; parallelism is across instances. Maps onto the existing
  "one `!Send` content actor per origin."
- **P3 — resource + GC discipline.** Generational resource handles, explicit drop,
  weak cross-runtime references, no mutual strong ownership across collectors
  (the Component Model gives no cross-component cycle collector, so this is on us).
- **P4 — resource controls.** Fuel / epoch interruption, memory limits, task
  limits, stream backpressure, bandwidth/storage quotas, deadline cancellation.
  Wasm isolation without quotas is incomplete isolation. (The JS `Budget` /
  `Steps` guard in `script-engine-api` is the in-component analogue, not a
  substitute.)
- **P5 — language adapters + conformance.** Three intentionally different
  implementations against one suite (events / mutation / fetch / cancellation /
  teardown): Rune (async orchestration), Nova (dynamic GC + browser-like
  scripting via the §3 wrapper), and a direct-Rust component (proves the contract
  independent of any interpreter).
- **P6 — tooling.** Component packaging, signatures, source maps, cross-component
  traces, WIT version adapters, runtime sharing, debugging, live upgrade. Package
  management and live upgrade are out of scope for the Component Model standard,
  so Mere must own them; do not wait for the standard to supply them.

---

## 7. Costs the proposal under-weights

1. **Per-instance runtime multiplication.** (Confidence: high it is real; medium
   on magnitude.) Each interpreter component carries its whole runtime (Nova is
   megabytes). One per origin/frame/extension multiplies that. "Runtime sharing"
   is a first-order sizing question for anything with many frames, not just a
   tooling line item.
2. **The per-interaction serialization tax.** (Confidence: high it is real;
   unquantified.) `apply(list<mutation>)` and `events: stream<event>` copy through
   the canonical ABI on every interaction. Same family as the per-frame
   scene-serialization cost the [parallelism strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
   flagged and left unmeasured. Streams remove the single-giant-copy case; a
   chatty extension still pays per call. Measure before assuming the boundary is
   free.
3. **Wasmtime-on-native is an unmade dependency decision.** (Confidence: high.)
   "Native first using a component runtime" means embedding Wasmtime, which puts a
   Cranelift JIT *in the native process*. The no-JIT policy is browser-only, so
   this is permitted, but it is a heavy dependency (codegen backend, compile
   times, binary size) the corpus has taken no position on. Note the honest value
   split: on native the component runtime buys the *typed capability contract* and
   *cross-language uniformity* first, and in-process confinement second; raw
   failure/memory isolation between actors already exists via the actor boundary
   and (for hostile content) OS subprocesses. The big isolation win lands later,
   in the browser/remote lane. Wasmtime supports `wasm32-wasip2` stably today;
   P3/async host support is recent and was still marked experimental as of the
   2026-03 releases, so pin against the post-0.3.0 point release.
4. **The synchronous-forced-layout wall** (§3) is not a detail. It is the reason
   `html-document` cannot be a thin adapter and must carry a DOM mirror.

---

## 8. What not to wait for

Do not gate this on: every language targeting Wasm directly; native browser
Component Model support; WasmGC becoming "complete"; parallel shared-memory
threads; WASI defining a browser or document model (it will not, by charter); or
Nova-in-wasm. WASI itself is still phased (filesystem / sockets / HTTP / CLI in
implementation, messaging / key-value earlier), so the host defines most
capabilities regardless. The native-first target depends on none of these.

---

## 9. Open decisions and triggers

**Decision (2026-06-21, Mark): accepted, betting on the future.** All three below are
resolved *go*, scoped by §10. The bullets stand as the rationale.

1. **Wasmtime-on-native: accept** the component runtime in the native process for the
   typed-capability + cross-language-uniformity payoff. Prefer AOT (`.cwasm` +
   no-codegen executor, e.g. Pulley) over an in-process Cranelift JIT where it fits
   (§10.7), so the compiler stays a build-time concern.
2. **The P5-descope third option: accept** in-process capability confinement of
   untrusted *extensions / scripts* via the Component Model. Legacy web content keeps
   the actor-boundary + OS-subprocess answer; it is not wrapped as a component (§10.1).
   The new sandbox covers mods, extensions, and native / interpreter scripts, not
   arbitrary hostile web pages in-process.
3. **Rune: reopened** as one P5 adapter, not a first-party language placement, with the
   sandbox warranty read as isolation-only (Wasm gives memory + ambient-authority
   confinement, not maturity / determinism / tenant isolation; §10.7).

Scope note carried from §2.1 / §10: this is a concurrency + capability + sandboxing
substrate (parallel *across* instances, async *within* one), not an in-script
shared-memory parallelism model (deferred, §8). Native payoff lands now; the
browser/web lane is the deliberate future bet (AOT polyfill; native browser CM is gated
on two uncommitted engines).

- **Wasmtime-on-native dependency call** (P0 gate, §7.3). Accept Cranelift in the
  native process for the contract + uniformity payoff, or defer until the
  untrusted-extension use-case is concrete.
- **The P5-descope reconsideration.** [actor_constellation_plan](2026-06-03_actor_constellation_plan.md)
  framed isolation as binary: semi-trusted in-process, or hostile to an
  OS-subprocess. The Component Model is the **missing third option**: in-process
  capability confinement of genuinely untrusted extensions. An actor boundary
  gives crash isolation, but a malicious *native* actor can do whatever its code
  does; a malicious *component* can do only what its imports grant. This is the
  strongest argument for the proposal and the gap the plan said it had no answer
  for. (Confidence: high that it is a genuine new capability, not a reframing.)
- **Rune reopen, on a different axis.** Reintroducing Rune as a P5 adapter trips
  the written reopen trigger ("a Rune 1.0 with a sandbox warranty"), because Wasm
  isolation *is* that warranty. Before leaning on it, note the axis shift: Rune
  was dropped as a *first-party language placement*; it returns here as *one
  extension adapter among several*. Different question, legitimately reopened; do
  not conflate the two or it reads as relitigating the placement call.

---

## 10. Before P0: review corrections (2026-06-21)

Two independent reviews, both codebase-verified. They converge on a restructuring
and barely overlap, so this is the consolidated punch list P0 must clear. Where a
finding corrects an earlier section, that section now carries an inline pointer.

### 10.1 Legacy web JS is its own native lane (corrects §2.3, §3, §6 P5)

The largest correction. `html-document` should not be a Wasm interpreter component,
and it leaves P0 and P1.

Verified in source (`script-runtime-api/lib.rs`): serval's live runtime already
runs the model §3 treats as the hard case, and it is not a component.

- The runtime owns `dom: ScriptedDom` (L80) and never lays out (L67, L73, L99).
- `getComputedStyle` is a **synchronous host seam**, `ComputedStyleHandler` over the
  host's `IncrementalLayout` (L98-103): the host lays out, the seam reads it back
  synchronously during the run.
- The execution model is already turn-based (L62-70, the `viewport_scroll` comment):
  the host syncs layout state *in* before the run, the script runs to completion,
  the host reconciles *out* after. A mid-run read of a value just written sees the
  unreconciled value, "the script/layout split's one fidelity gap."

So serval does not do true synchronous forced reflow. It does synchronous reads of
the previous frame's layout and accepts a documented fidelity gap. The §3 premise
("you cannot run that against an async apply") is true for genuine forced reflow,
which serval never does, so the bar the existing native lane already clears is lower
than §3 implies.

Consequence for §2.3: the actor-plan P5 descope ("compiling the entire serval +
Nova content engine to wasm32") does touch this proposal for `html-document`.
Legacy-with-synchronous-layout *is* the engine, so wrapping it as a component
reintroduces that cost. "We confine the script, not the engine" holds only where the
script is not itself the engine.

The honest lane split:

- **Mere-native extension / app scripts.** Direct Wasm components over the coarse
  `DocumentScript` contract. The real P0/P1 target.
- **Legacy web JS.** The existing native `Runtime<Nova/Boa>` plus reflectors and the
  sync host seams, implementing a Rust counterpart to the contract, not a component.
  A peer lane, not a subordinate profile.
- **Wasm interpreter components.** Later, only for runtimes that genuinely compile to
  wasm and whose document/layout needs fit inside the boundary.

Action: drop `html-document` from P0/P1 and record it as a separate later
compatibility track. Conformance later demonstrates whether the lanes can share more
than vocabulary.

### 10.2 Make the outer contract per-turn; keep run() optional (refines §5)

The WIT export `run: async func(session)` puts the script in charge of its own loop.
The engine is already per-turn (§10.1): snapshot in, run to completion, reconcile
out, metered by `pump(budget)`. An export shaped like
`handle-event(event, expected-revision) -> list<mutation>` keeps ordering, metering,
cancellation, and paint-commit points under the host, and matches what the engine
does today. Make per-turn the default; offer long-running `run(stream)` as an
optional service profile for scripts that need their own loop.

### 10.3 Write the transaction contract, do not gesture at it (extends §5)

Optimistic concurrency needs more than `expected-revision`:

- `inspect` returns the revision its snapshot was taken at, so a later `apply` can
  cite it.
- Events carry their source revision.
- `apply` rejection returns the *current* revision in the error, so the script can
  rebase rather than just learn "conflict."
- Mutation batches are atomic against the cited revision, with a declared size limit.
- **Stable node identity across the copied snapshot.** Define how a node named in an
  `inspect` result is referenced by a later `mutation`, since the snapshot is a value
  copy with no shared handle. This is the seam an interpreter mirror refreshes through.

**Resolved (2026-06-21), node identity.** Opaque host-assigned `node-id` (u64) is the
canonical identity for mutation targeting and cross-turn references: stable across content
edits and moves, unambiguous, capability-scoped (the host mints ids only for in-scope
nodes, so a script cannot forge an out-of-scope reference), and a match for serval's
existing `NodeId` (`ReflectorData = u64`). Content-hash is **rejected as the identity** (it
collides on identical-content nodes, and changes under the very edit being made, Merkle-
propagating to ancestors) but **kept as a per-node change-detection token** in the snapshot
(a script's mirror compares it to skip unchanged subtrees; aligns with the content-addressed
sync substrate). Positional ops use **id-relative anchors** (parent / sibling node-ids), the
robust form of "paths" with no index-shift fragility. Implemented + verified in the probe:
the snapshot carries `revision` + per-node `id` + `content-hash`; mutations are id-targeted
(`set-text` / `remove` / `insert-before` / `append-child`); the host applies batches
atomically against `expected-revision`, rejecting a stale revision with the current one and
an unknown id before any change lands.

### 10.4 Capability granting: the mechanism, not just the catalogue (extends §4)

The world imports `document` / `network` / `clock` / `log` as a fixed set. A grant
has to become a linking decision: a denied capability is either an import bound to an
always-erroring stub, or an unlinked import (the component fails to instantiate).
Pick one, and define how a script discovers what it holds. This is the headline
native payoff (§7.3: the runtime buys the typed capability contract first), so it
cannot stay a forward-reference to the capability-gate catalogue.

### 10.5 Runtime sharing vs per-origin capability are in tension (connects §7.1)

For an interpreter component, untrusted code runs inside the component, sharing its
linear memory and its granted imports, so confinement is at *component* granularity,
not *script* granularity. The §7.1 mitigation (share one runtime across origins to
amortize the megabytes) collapses the per-origin capability boundary those origins
then share. Per-origin runtime keeps the boundary and pays the §7.1 cost; shared
runtime saves the cost and loses the boundary. P0 needs a position, because this
bounds how much the "third isolation option" (§9) delivers for the interpreter lane.

### 10.6 Profile evolution vs frozen compiled components (extends §6 P6)

The compiled-component lane is the cleanest (§3) and therefore the most brittle: a
`mere:script@0.2.0` that changes a `mutation` variant breaks every third-party
compiled artifact. For a contract whose point is third-party extensibility, profile
evolution without orphaning installed components is plausibly the dominant long-term
cost. It is currently one P6 bullet ("WIT version adapters") and needs a real
position: adapter shims, deprecation windows, or a frozen core profile that only
grows.

### 10.7 Smaller corrections

- **AOT, not only JIT** (corrects §7.3, §9). "Native means a Cranelift JIT in the
  process" is too binary. Precompiled `.cwasm` artifacts plus a no-codegen runtime
  (Wasmtime's Pulley, already noted in [actor_constellation_plan](2026-06-03_actor_constellation_plan.md))
  keep the compiler at build time and embed only the executor. Add this as a third
  option in the §9 decision.
- **Trim "remote-component boundary"** (corrects §6, §8). The Component Model gives
  shared vocabulary, not distributed-failure semantics (Goals.md is explicit it does
  not solve distributed computing). Remote execution needs its own identity,
  transport, retry, resumption, and partial-failure protocol. WIT can name the types;
  it does not supply the protocol.
- **Rune's warranty is partial** (refines §9). Wasm confines memory and removes
  ambient authority. It does not establish interpreter maturity, determinism, or
  tenant isolation inside a shared runtime, all of which the corpus already flags
  ([actor_constellation_plan](2026-06-03_actor_constellation_plan.md):66-71: Rune
  self-labels its sandbox "no warranty" at 0.x; determinism documented by neither).
  Reopen Rune as a contained experiment, not a warranted one.
- **Define "script instance"** (refines §6 P2). The actor model is one content actor
  per origin / agent cluster ([actor_constellation_plan](2026-06-03_actor_constellation_plan.md):51),
  and an origin hosts many scripts. "One armillary actor per script instance" would
  multiply actors past that. Use one execution-host actor per origin (or trust
  principal) that schedules several component instances locally.

### 10.8 Split P0 itself

- **Name the first consumer.** The concrete one is the polyglot wasm-block
  ([polyglot_block_resolver_plan](../../nematic_docs/implementation_strategy/2026-06-13_polyglot_block_resolver_plan.md)
  P3): "text in, blocks out, no ambient capability." That is far narrower than
  `document-core`'s document-session world. Start P0 from the text-to-blocks shape
  and grow toward document-session, rather than building the rich profile first.
- **Split the runtime probe.** A synchronous contract / ownership probe on a released
  Wasmtime, and a separately pinned async / future / stream probe. Promote async only
  after cancellation, backpressure, fuel, and teardown work end to end. Verified
  2026-06-21 against primary sources: the latest *released* Wasmtime is **45.0.2**
  (2026-06-15), whose component-model async is self-described "*very* incomplete" in
  `config.rs` (`wasm_component_model_async` and the stackful / more-builtins variants).
  WASI 0.3.0 with Component Model Async *enabled by default* is slated for **Wasmtime
  46, not yet released**. So no released runtime ships default WASI 0.3 today: the sync
  probe runs on 45.0.2 now; the async probe either waits for 46 or rides 45's flagged,
  incomplete path. This empirically grounds the split (and tempers §7.3's "pin against
  the post-0.3.0 point release", which assumes a released default that does not exist
  yet).

---

## 11. P2 design — the component host actor (proposed 2026-06-22, pending §11.7)

A read-only design pass (6-reader workflow) verified against armillary, the meerkat
content actor, serval's scripted-DOM, `register-mod-loader`, and `kernel::permissions`.
The probe's WIT + host logic transplant nearly 1:1; the only genuinely new code is the
serval-backed imports, a lifecycle/quota wrapper, and the linker policy.

### 11.1 Placement (decision, pending confirm)

A new leaf crate `crates/script/document-host` (sibling to `crates/script/rhai`),
consumed by `meerkat` via a thin `meerkat::script` wiring module (~80 LOC). **Extend
`register-mod-loader` by *implementing* its `WasmModRuntime` DI trait, not by editing
it** — that crate is deliberately runtime-free (the trait exists precisely so wasmtime
stays host-side). Do not bury Wasmtime in meerkat (`content.rs` is ~455 LOC against the
600 ceiling, and the capability boundary deserves headless testability). Module split,
each < 600 LOC: `engine` / `host` / `imports/{inspect,apply,log}` / `instance` /
`linker` / `runtime` (the mod-loader bridge) / `dom_view` (the only serval-coupled file).
Deps one-way: `document-host` → {serval-scripted-dom, register-mod-loader, kernel,
wasmtime}; `meerkat` → `document-host`.

### 11.2 Shape on armillary

Not a new actor: a `!Send` subsystem built inside the content actor's existing
`spawn_on` run closure (`content.rs:176`), beside the serval registry + ResourceStore.
The `Engine` is shared process-wide (`Arc<Engine>`, Send+Sync, passed in like the other
build args); the `Store<ScriptHost>` + component instance are built on the thread, so the
`!Send` state never crosses. Mailbox: new `ContentCommand::{AttachScript, DeliverEvent,
DetachScript}` arms in the existing `recv` loop; results via a new `ContentUpdate` arm.
Cancellation: epoch interruption (one shared watchdog tick + a per-turn deadline; works
on sync calls, no async). Quotas: `StoreLimits` caps linear memory; batch size enforced
host-side in `apply` → `refused` (no WIT change). Teardown: `deactivate` + drop the
instance when the actor's channel closes (deterministic, no separate cleanup path).

### 11.3 Wiring to the live ScriptedDom

`inspect` → walk the live DOM, emit view-nodes with `id = NodeId.raw()` (literally the
WIT `node-id`; `ReflectorData = u64`), a **shallow** content-hash (kind+attrs+text+
child-count, not a Merkle subtree hash), `revision` = the host counter; `subtree` scopes
the walk. `apply` → expected-revision check → atomic `is_live` precheck (`unknown-node`)
→ quota check → ordered `LayoutDomMut` calls (`set_text`/`remove`/`insert_before`/
`append_child`) → bump revision → drain mutations into serval-layout's incremental
scheduler → re-render. **Revision counter lives host-side (`ScriptHost`), not in
`ScriptedDom`** (serval has none today; keep it render-state-free) and MUST be bumped by
*all* actor writers (script apply, Resource arrival, Retheme), or a stale batch won't
conflict correctly. `log` → the actor's tracing span. `network`: a SYNC-signature
`fetch: func(request) -> result<response, error>` implemented host-side as an `async fn`
over the existing fetch actor / `fetch.rs` seam; turns run via `call_async`, so a script's
plain `fetch()` suspends the turn's fiber during I/O without blocking the executor (fiber
async, proven on stock WT45 — §11.7 #7; not the "very incomplete" component-model-async,
not 46-gated). The same `call_async` invocation serves the sync `document-core` turns (they
just never suspend), so one mechanism covers both. Deferred only by sequencing, not by
runtime maturity.

### 11.4 Capability grants

`kernel::permissions::resolve_permission` (five-scope narrowing) is the input; a
profile→imports table in `document-host::linker` decides which imports get linked. Per
import: Allow → `add_to_linker`; Deny → omit (unlinked, instantiation fails — the secure
default the probe proves); Prompt → resolve before instantiation. Posture: unlinked-by-
default for the trust boundary, an always-erroring stub only for manifest-declared
*optional* imports. A tiny always-linked `caps.granted()` import gives the script
discovery. `register-mod-loader`'s `ModManifest.capabilities` feeds profile selection.

### 11.5 Build implications

Wasmtime 45 → rustc ≥ 1.93; adopting `document-host` into the workspace forces a 1.93
MSRV bump (meerkat pins 1.92). AOT preferred: compile trusted bundled components to
`.cwasm` at build time, load via `Component::deserialize` with codegen disabled (keeps
the Cranelift compile off the actor hot path); `.cwasm` is a per-target build artifact,
never committed; JIT stays for untrusted. Size: share one `Engine` across origins
(amortize the megabytes), per-origin `Store`+`Linker` (preserve the boundary). The
shipped `document-core` guest should be `no_std` (only `log` + `document-host` imports,
no WASI floor — the probe's std+WASI is a probe artifact).

### 11.6 Phased build order

P2.0 crate skeleton + transplant the probe host (still `Doc`-backed; the 8-turn driver
passes as a crate test) → P2.1 swap `Doc` for `ScriptedDom` (`dom_view` + inspect/apply)
→ P2.2 engine config (epoch + StoreLimits; trap a loop/mem-bomb) → P2.3 linker policy
(profile table + permissions mapping + `caps.granted`) → P2.4 `WasmModRuntime` bridge →
P2.5 actor wiring in meerkat (ContentCommand arms + `ScriptInstance` on `Content`) → P2.6
AOT path. Each headless-testable. `fetch` (fiber-async, sync WIT signature; §11.7 #7) is a
later P2 step, not 46-gated: it needs turns invoked via `call_async`, so P2 should use the
async bindgen (`imports`/`exports: { default: async }`) from P2.0 — sync turns run via the
same `call_async` without suspending, and `fetch` slots in when wired.

### 11.7 Decisions (resolved 2026-06-22 where marked)

1. **MSRV** — **RESOLVED + DONE 2026-06-22: workspace bumped to 1.93, `document-host` folded
   into the workspace.** `mere/rust-toolchain.toml` pins 1.93 (+ wasm32-wasip2);
   `document-host` is a `members` entry with the git serval deps that unify with meerkat via
   mere's `[paths]` override; all its tests pass in-workspace. The 6 crates still declaring
   `rust-version = "1.92.0"` are harmless under the 1.93 toolchain (the effective MSRV is 1.93).
   The meerkat-wiring half of P2.5 (the content-actor integration) still remains.
2. **Denied-cap behavior** — proceeding on rec: unlinked default + opt-in stub for
   *optional* imports.
3. **Placement** — **RESOLVED: new leaf crate `crates/script/document-host` + thin
   `meerkat::script`, extending `register-mod-loader` via its `WasmModRuntime` trait** (Mark).
4. **Revision home** — proceeding on rec: host-side `ScriptHost`, bumped by all actor writers.
5. **AOT trust scope** — proceeding on rec: P2 ships bundled first-party only; defer untrusted.
6. **Cancellation values** — proceeding on rec: fixed constants in P2.
7. **Async re-entry shape** — **RESOLVED 2026-06-22 by empirical probe: fiber async,
   suspension *in addition to* sync.** WT45's component-model-async (Model B: WIT
   `async func`, streams, concurrent tasks) is genuinely "very incomplete" (feature-gated,
   self-described) and **not needed**. The mature **fiber-async** path (Model A) works on
   *stock* WT45 — proven in `crates/probes/wasmtime-async-p1`: a SYNC-signature
   `fetch: func(request) -> result<response, error>` implemented as a host `async fn`, with
   turns invoked via `call_async`, so the script's plain `fetch()` suspends the turn's fiber
   during real I/O while the executor stays live. The guest writes no async coloring; the host
   thread is not blocked. This supersedes the earlier separate-event-vs-suspension dichotomy:
   synchronous-looking for scripts AND non-blocking, today, no incomplete feature, no
   compromise. (`Config::async_support` is a deprecated no-op — async is always on;
   `Engine::default()` suffices. Still needs rustc 1.93 for WT45, so the §11.7-1 MSRV bump
   stands.)

---

## Progress

- **2026-06-21.** Plan created from a session synthesis. External standards
  verified against primary sources (component-model Concurrency.md / Choices.md /
  Goals.md, WASI Proposals.md + releases, gc MVP.md, Wasmtime releases): WASI
  0.3.0 async shipped 2026-06-11; CM is Developer Preview, CM 1.0 gated on two
  uncommitted browser engines; WasmGC ships in all three engines but does not
  cross the component boundary; Choices.md confirms no cross-component cycle
  collector + acyclic resource ownership + no runtime-JIT primitive. Codebase
  grounding verified in source: `script-engine-api/lib.rs` is JS-shaped;
  `fetch.rs:86-118` already carries the deferred async seam; the Component-Model
  substrate idea is absent from the corpus; the actor-constellation "wasm-component
  isolation" seam is deferred and its P5 descope reasoning targets the engine, not
  the script. No code written. P0 is a decision gate.
- **2026-06-21 (review pass).** Two independent codebase-verified reviews folded into
  §10 as the "Before P0" punch list. Headline correction: `html-document` leaves
  P0/P1 and legacy web JS becomes a separate native lane, not a Wasm component.
  Grounded by `script-runtime-api/lib.rs` (L62-70, L80, L98-103): serval's runtime
  already runs legacy turn-based (snapshot in / reconcile out) with a synchronous
  `ComputedStyleHandler` seam and a documented fidelity gap, and never does true
  forced reflow; so §2.3's "the P5 descope does not touch this" is overbroad for the
  legacy lane (there the script *is* the engine). Also folded: per-turn outer
  contract over `run(stream)`; a real transaction contract (revision on inspect /
  events, conflict revision in errors, batch atomicity, stable node identity across
  the copy); capability-grant linking mechanics; the runtime-sharing vs per-origin
  capability tension; profile-version brittleness of compiled components; AOT as a
  third option vs Cranelift-or-defer; trim "remote-component boundary"; Rune's
  warranty is partial (isolation only, not maturity/determinism); define "script
  instance" against the per-origin actor model; and split P0 into a sync probe + a
  pinned async probe, started from the polyglot wasm-block consumer. Inline pointers
  added at §2.3 and §3. No code written.
- **2026-06-21 (upstream verification).** The external facts (previously verified
  2026-06-20, re-checked against primary sources) all hold:
  - **WASI 0.3.0 shipped 2026-06-11.** Confirmed: WASI GitHub release `v0.3.0` and the
    Bytecode Alliance "WASI 0.3 Launched" article (June 11, 2026).
  - **Async / cancellation / backpressure are in the Component Model.** Confirmed in
    `component-model` Concurrency.md: `subtask.cancel` (cooperative cancellation),
    `backpressure.inc` / `backpressure.dec` (counter-based backpressure), the async ABI
    "doesn't actually depend on the Core WebAssembly stack-switching proposal" and "can
    be polyfilled in browsers via JSPI." So §2.1's "counter-based backpressure" and the
    "Rune-style async is accommodated" argument stand verbatim.
  - **CM 1.0 is gated on two browser engines, neither committed.** Confirmed near-verbatim
    in "The Road to Component Model 1.0": "can't formally reach 1.0 without native
    implementation in at least two browser engines"; Mozilla + Chrome are "paying
    attention" but "these aren't commitments."
  - **WasmGC ships in all three engines** (Chrome 119, Firefox 120, Safari 18.2; baseline
    Dec 2024); the no-cross-boundary point is a canonical-ABI design fact, unchanged.
  - **One correction folded into §10.8:** the latest *released* Wasmtime is 45.0.2
    (2026-06-15) with component async self-described "*very* incomplete" in `config.rs`;
    WASI 0.3 *by default* lands in the unreleased Wasmtime 46. So §7.3's "pin against the
    post-0.3.0 point release" was optimistic, which is exactly why the §10.8 sync/async
    probe split is the right shape. Both the plan's and the source review's upstream
    claims check out.
- **2026-06-21 (decision: go).** Mark accepted the direction ("betting on the future").
  §9's three open decisions resolved *go* (Wasmtime-on-native accepted, AOT-preferred;
  the CM third-isolation option accepted for the extension/script lane; Rune reopened as
  an adapter). Binding scope is §10: legacy web JS stays a native lane, P0 starts from
  the polyglot wasm-block (text-to-blocks direct-Rust component) and grows toward
  document-session, runtime probe split into sync (on 45.0.2) + async (46-gated). Status
  flipped to accepted. Next: P0 scoping. Still no code written (P0 is a probe gate).
- **2026-06-21 (P0 sync probe — green).** Built `crates/probes/document-script-p0/`: a
  WIT `document-core` world (`mere:script@0.1.0`), a direct-Rust guest component, and a
  Wasmtime 45.0.2 host runner. Runs end to end: host-driven per-turn `handle-event`,
  `set-text`/`append` events producing `replace-all`/`append` mutations applied to host
  state with the revision advancing, and both typed-error variants
  (`revision-conflict(u64)`, `refused(string)`) round-tripping. The guest calls its one
  granted `log` capability; only WASI + `log` are linked. Validates the §10.2 per-turn
  shape, the §10.3 typed-error/revision shape, and the §10.4 capability seam on a real
  Component Model boundary, on the released runtime (§10.8 sync probe). Findings:
  - **Toolchain (for §10.8):** wasmtime 45.0.2 requires rustc >= 1.93.0; the workspace
    default is 1.92.0. Adopting wasmtime in the workspace proper means an MSRV bump. The
    probe pins 1.93.0 locally via `rust-toolchain.toml` (probe adapts to the skew).
  - **Capability minimality is a build-mode cost (for §10.4):** a `std` guest on the
    wasip2 target imports the entire WASI world, so the host must grant WASI to
    instantiate at all. A deny-by-default component (only the application capability)
    needs a `no_std` build plus a hand-supplied allocator and mem/alloc-error intrinsics
    (hit the `env::memcmp` / alloc-error-handler wall mid-probe; deferred). A grant model
    cannot assume the app's import set is minimal by default.
  - **The enforcement point is real:** instantiation *failed* when an imported interface
    was not linked (observed first-hand with `wasi:io/error` before WASI was granted).
    "Unimported means unreachable" is the live runtime mechanism behind §9's third
    isolation option, not just an aspiration.
  - **wasmtime 45 API shape (so P1 host wiring does not re-derive):** the
    `HasData`/`HasSelf<T>` pattern for `add_to_linker`; `WasiView::ctx() -> WasiCtxView`
    carries ctx + table (no separate `IoView`); `wasmtime_wasi::p2::add_to_linker_sync`.
  - Not covered (later): async (Wasmtime 46), host-side conflict detection in a real
    `apply()` seam (P1), stable node identity across the copied snapshot, the
    interpreter-component lane, the `no_std` minimal-cap guest.
- **2026-06-21 (P1 slice — §10.3 transaction contract, green).** Grew the probe's WIT
  world from P0's `replace-all`/`append` to the §10.3 transaction contract and verified it
  end to end (`crates/probes/document-script-p0/`). The snapshot (`document-view`) carries
  its `revision` + per-node opaque `node-id` + `content-hash`; mutations are id-targeted
  (`set-text` / `remove` / `insert-before` / `append-child`) with id-relative anchors; the
  host owns the document, mints ids, and applies each batch atomically against
  `expected-revision`. The run demonstrates: editing a node by id (reading its content-hash),
  append/insert by id-anchor, subtree remove, the revision advancing 0→4, a stale revision
  rejected with the current one (conflict), an unknown id rejected before any change lands
  (atomic precheck), and a guest-side refusal. Node-identity decision recorded in §10.3
  (opaque ids canonical; content-hash = change-detection token, not identity; id-relative
  anchors, not index paths). Mere-side changes uncommitted. Remaining for P1: a real
  `inspect` import (on-demand mid-turn pull) + `lifecycle`, a batch size limit, the
  `network` seam over `fetch.rs`, and WIT versioning.
- **2026-06-21 (P1 slice — inspect import + lifecycle, green).** Added the §5 `inspect`
  seam as a host import (`document-host.inspect(query) -> document-view`): the guest now
  pulls the snapshot on demand mid-turn (a re-entrant call back into the host) instead of
  receiving it as a param, and can scope the pull (`document-query::subtree(node-id)`) to
  copy less than the whole document (the §7.2 per-interaction cost lever). Added
  `activate` / `deactivate` lifecycle exports the host calls around the event loop.
  Verified end to end: per-turn on-demand pull, a scoped subtree query, a no-op turn that
  does not advance the revision, with the prior transaction-contract behavior intact. The
  architecturally-interesting sync P1 work is now done; P1's remainder is a batch size
  limit and the async `network` seam (gated on Wasmtime 46). Mere-side uncommitted.
- **2026-06-22 (P2 design pass — proposed, pending decisions).** Ran a read-only 6-reader
  workflow over armillary, the meerkat content actor, serval's scripted-DOM,
  `register-mod-loader`, and `kernel::permissions`; synthesized §11 (the component host
  actor design). Headline: a new leaf crate `crates/script/document-host` consumed by
  meerkat via a thin `meerkat::script` module, extending `register-mod-loader` through its
  `WasmModRuntime` DI trait (not editing it); the host is a `!Send` subsystem inside the
  per-origin content actor's `spawn_on` closure (not a new actor); imports wire to the live
  `ScriptedDom` (`id = NodeId.raw()`, a host-side revision counter bumped by all writers,
  shallow content-hash); grants link/omit imports via `kernel::permissions`; one shared
  `Engine` + per-origin `Store`; AOT-preferred; MSRV bump to 1.93. The probe transplants
  ~1:1. No code written; 7 open decisions in §11.7 await Mark before P2.0.
- **2026-06-22 (async maturity probe — fiber suspension works on WT45).** Mark authorized
  the 1.93 MSRV bump and asked how incomplete WT45 async really is. Determined empirically
  (`crates/probes/wasmtime-async-p1`, gitignored): WT45's *component-model-async* (Model B —
  WIT `async func`, streams, concurrent tasks) is feature-gated + self-described "very
  incomplete" and unneeded; the mature *fiber-async* path (Model A) works on **stock WT45**.
  Built a sync-WIT `fetch` import implemented as a host `async fn` (awaiting a real
  `tokio::time::sleep`) and an export invoked via `call_async`: the run shows the host
  parking ("pending → resolving") and the fiber resuming with the value, guest code fully
  un-coloured. So suspension is doable *in addition to* sync, today, no compromise (resolves
  §11.7 #7). `Config::async_support` is a deprecated no-op; `Engine::default()` suffices.
  Network capability redesigned in §11.3 to the fiber model. MSRV bump still required (WT45 →
  rustc 1.93).
- **2026-06-22 (P2.0 — green).** Created `crates/script/document-host` (standalone for now:
  own `[workspace]` + 1.93 pin, so the mere workspace is undisturbed until P2.5). Transplanted
  the probe's WIT + guest + host into a real **library**: `Doc`-backed, the full §10.3 contract
  (id-targeted mutations, atomic revision-checked apply, `inspect` import, `activate`/
  `deactivate` lifecycle), with exports invoked via `call_async` (`exports: { default: async }`)
  — the fiber foundation for the future suspending `fetch` (§11.7-7), no suspension yet. The
  8-turn driver passes as a crate test (`tests/eight_turns.rs`): four id-targeted mutations
  applied (rev 0→4), a scoped subtree no-op, and the conflict / unknown-node / declined paths,
  with the final tree asserted. Mere-side uncommitted; document-host folds into the workspace +
  the MSRV bump at P2.5. Next: P2.1 (swap `Doc` → serval `ScriptedDom` behind the same imports).
- **2026-06-22 (P2.1 — dom_view over live ScriptedDom, green).** Added `src/dom_view.rs`: the
  serval-coupled adapter backing `inspect`/`apply` on a live `serval-scripted-dom::ScriptedDom`
  (path-depped from local serval — light: markup5ever + layout-dom-api + serval-static-dom, no
  stylo/taffy, no patches). `snapshot` projects the HTML DOM into document-core view-nodes
  (elements → tag-named, text nodes → `#text`; the document-core view *is* the DOM tree, §11.3);
  `apply` maps id-targeted mutations to `LayoutDomMut` (`set_text`/`remove`/`insert_before`/
  `append_child`), with node identity round-tripped via `opaque_id`/`NodeId::from_raw` and
  validated by `is_live` (no host id-map needed), against a host-side revision (serval tracks
  none). Three lib unit tests pass: snapshot projects the tree + ids/text round-trip; the
  subtree query scopes the view; apply mutates and enforces revision-conflict (carrying current)
  + unknown-node (nothing applied, rev unchanged). The P2.0/P2.1 state (Doc-backed lib +
  dom_view) is committed as `5825c17`.
- **2026-06-22 (P2.1b — wasm guest over the live ScriptedDom, green; P2.1 complete).** Swapped
  `ScriptHost` onto a live `ScriptedDom` (in-memory `Doc` retired): `inspect` → `dom_view`, the
  host applies returned batches via `dom_view::apply` against the live DOM + host revision. The
  guest is now DOM-shaped (operates on tag-named elements + `#text` nodes). End-to-end test
  `eight_turns_drive_the_live_dom` green: the wasm guest pulls the seeded `<body><p>…</p>` DOM,
  edits a `#text` by id, appends/inserts `<p>` by id-anchor, removes, and the conflict /
  unknown-node / declined paths fire; final DOM = 3 `<p>` with the edited `#text` live (rev 0→4).
  This is the key integration proof: a sandboxed wasm component mutating serval's real DOM through
  the capability-scoped contract. P2.1b uncommitted (changes on top of 5825c17).
- **2026-06-22 (P2.2 — cancellation + quotas, green).** Added epoch interruption + `StoreLimits`
  to the host (`run_guarded` returning a `Guarded` outcome) and a misbehaving `guest-bomb` crate.
  Three guard tests pass: an infinite loop is **epoch-cancelled** (a watchdog thread bumps the
  engine epoch; `set_epoch_deadline` traps the turn's fiber), an unbounded allocation is denied by
  the `StoreLimits` **memory cap**, and a benign turn completes — the host thread survives every
  case. Proves §11.2's "Wasm isolation without quotas is incomplete isolation." `run_turns` stays
  unguarded (unlimited); `run_guarded` is the bounded path that P2.5 will use per-origin. The
  cancellation-value knobs (epoch tick, deadline, mem cap) are fixed constants for P2 per §11.7-6.
- **2026-06-22 (P2.5 fold-in — workspace MSRV bump + membership, green).** Added
  `mere/rust-toolchain.toml` (channel 1.93 + wasm32-wasip2) and `crates/script/document-host` to
  the workspace `members`; removed document-host's standalone `[workspace]` + per-crate toolchain
  pin, and switched its serval deps from local-path to the **git dep meerkat uses** (they resolve
  to the local serval via mere's `[paths]` override, so they unify — verified: `serval-scripted-dom`
  / `layout-dom-api` compiled from the local checkout). `cargo build -p document-host` builds on
  rustc 1.93 in-workspace; all 7 tests pass via `cargo test -p document-host`. Left the 6 crates'
  `1.92` rust-version pins untouched (harmless under 1.93). Unblocks P2.3/P2.4 to depend on
  `kernel::permissions` + `register-mod-loader` as siblings. Note: `document-host` is a default
  member, so a bare `cargo build` / rust-analyzer over the workspace now also compiles wasmtime —
  a `default-members` list can keep the bare build lean if that friction bites.
- **2026-06-22 (P2.3 — capability / linker policy, green).** Added the grant→link/omit policy:
  `CapPermission` (Allow/Prompt/Deny, mirroring `kernel::permissions::Permission`), a `Grant` over
  the `mere:script` application capabilities, `link_with_grant` (WASI floor always + only `Allow`
  imports), and `instantiate_with_grant`. Tests prove the capability boundary: `allow_all`
  instantiates the document-core guest; `deny_document` (omitting `document-host`) **fails
  instantiation** because the guest requires it — enforced by the runtime, not host convention;
  `granted_names` reflects the grant. document-host stays **graph-kernel-free**: the five-scope
  `resolve_permission` → `Grant` mapping is a thin P2.5 adapter in the content actor (the policy
  lives here, the resolution is input — §11.4). Remaining for §11.4: a `caps.granted()` discovery
  import (`granted_names` is its seam) — a small additive WIT step. (cargo notes a benign
  `[paths]`-override warning for the redirected serval crates.)

## Key grounding files

- serval: `components/script-engine-api/lib.rs` (the JS-shaped trait),
  `components/script-runtime-api/fetch.rs:86-118` (the deferred fetch seam),
  `docs/2026-05-20_serval_script_engine_plan.md` (reflector model + per-target
  backend selection), `docs/2026-05-25_js_execution_strategy.md` (no-JIT, weval),
  `docs/2026-06-11_gc_arena_dom_plan.md` (the owned DOM store + mark-sweep).
- mere: [actor_constellation_plan](2026-06-03_actor_constellation_plan.md)
  (the actor boundary + deferred plugin seam + P5 descope + Rune reopen trigger),
  [protocol_architecture_plan](2026-05-05_protocol_architecture_plan.md) (Extism
  gesture), [cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
  (no-JIT policy + lane fork), [capability_gate_catalogue_brief](../research/2026-05-14_capability_gate_catalogue_brief.md)
  (profile granting), [polyglot_block_resolver_plan](../../nematic_docs/implementation_strategy/2026-06-13_polyglot_block_resolver_plan.md)
  (native-only wasm-block kind).
- external (verified 2026-06-20): `WebAssembly/component-model` design/mvp/Concurrency.md
  + design/high-level/Choices.md + Goals.md; `WebAssembly/WASI` docs/Proposals.md
  + releases (v0.3.0); `WebAssembly/gc` proposals/gc/MVP.md;
  bytecodealliance Road-to-Component-Model-1.0 + jco docs.
