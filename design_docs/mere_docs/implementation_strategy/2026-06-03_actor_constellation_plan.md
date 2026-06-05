# Actor Constellation Plan

A message-passing runtime for Mere: a single-threaded **host kernel** (graph +
frame + compositor + routing) surrounded by a constellation of **actors** (I/O,
content, compute). "Host kernel" throughout this doc means the runtime center,
not the `kernel` graph crate. It is Servo's constellation done **in-process**:
the kernel fuses the constellation and compositor; content actors are the
pipelines; scenes travel as messages rather than IPC-serialized surfaces. The
big simplification over Servo is that an in-process content actor hands the
kernel a `Scene` directly, so there is no cross-process surface sharing to
engineer.

The spine is the **actor kernel** (the converged shape of Zed/GPUI, Flutter,
Figma): one thread owns canonical state, heavy work is offloaded, results return
as messages. The discipline that keeps it safe is GPUI's: **the kernel/actor
boundary is a type, not a convention.** A `!Send` kernel context versus `Send`
actors makes a violation a compile error rather than a code-review catch. The
reactive view layer (`xilem_serval`) is the kernel-thread programming model that
runs on that `!Send` context. This plan names the pattern, encodes the boundary
in types, and shows how the other concurrency models compose into it rather than
compete with it.

This is an evolution of [meerkat](../../../crates/meerkat), not a rewrite. The
kernel already exists (meerkat's winit `App`), the I/O actors already exist
(`fetch`, `sync`), and the content pipeline is already a pure scene producer. The
plan names the pattern and generalizes it.

The runtime lives in a new host-neutral crate, **`armillary`**: the kernel
harness (inbox router plus dispatch), the actor traits and lifecycle, the typed
boundary, and the message taxonomy. meerkat is the concrete kernel built on it
(winit, wgpu, the graph); content and I/O actors are armillary actors. The name
fits the shape: an armillary sphere is a frame of rings around a central point,
which is exactly this structure, the kernel at the center with the actor rings
(its constellation) around it.

Related: [modular integration plan](2026-06-02_modular_integration_plan.md) (the
host model and S-phases), [linked-data ingest plan](2026-05-22_linked_data_ingest_export_plan.md)
(the contribution boundary + v5 identity), [netfetcher plan](2026-05-25_netfetcher_plan.md)
(the network I/O actor). The concurrency ground truth (wasm-in-browser, browser
threading, the model menu) was researched and adversarially verified 2026-06-03;
the load-bearing facts are inlined under **Technical ground truth** below.

## The four layers

- **Host kernel (one thread).** Owns the graph, the frame (tile layout), the
  reactive view (`xilem_serval`), message routing, and render. Lock-free because
  it is the single owner of the two things everything else wants: the render GPU
  and the graph.
- **I/O actors.** Network fetch, p2p sync, persistence writes, link resolution.
  Background work that returns data, never UI.
- **Content actors (the constellation).** One per origin (agent cluster) once
  scripting lands, per-tile before then. Owns its DOM (and later its Nova JS
  engine, `!Send`, pinned to the thread), ships paint-lists/scenes to the kernel.
  Isolation is the actor boundary. A Servo content pipeline, minus the process.
  Confidentiality limits are real: see **Threat model**.
- **Compute actors (later).** Heavy layout, graph physics, Burn. Offloaded only
  when a frame cannot hold the work. The tier splits in two: **raw compute** (Burn,
  gyre physics, aether fields) and **orchestration**, an async agent/intelligence
  actor that chains model and tool calls and drives state by message. The scripting
  layers map onto the actors (2026-06-04): **Rust + per-owner arena** = kernel and
  actor internals (hot paths); **JS (Nova/Boa)** = content actors; **Rune** = the
  orchestration/agent actor (async); **Rhai** = declarative authoring (aether
  fields, scene effects, and sandboxed coordination policy on the sync/mesh actor).
  Placement verified 2026-06-04: Rune (0.14.2, pre-1.0, Runestick stack-VM,
  first-class async) wins the orchestration role because Rhai has no async at all;
  Rhai keeps the untrusted-policy and declarative roles **on maturity, not a
  capability gap** (Rune has an opt-in empty `Context`, a per-instruction budget,
  and allocator-level memory caps, but self-labels its sandbox "work in progress,
  no warranty" at 0.x, while Rhai v1.25 markets untrusted scripts with a Don't-Panic
  guarantee and a documented limit suite). Both run in wasm32/browser with no JIT.
  Caveat: **determinism is documented by neither**, so any re-run-to-a-hash
  verification must enforce it host-side (no floats, canonical iteration order, no
  host nondeterminism).

## The typed boundary (the GPUI lesson, made structural)

Invariants 1 and 2 below are not prose discipline to "guard above all";
`armillary` encodes them in the type system, so the compiler refuses violations.
This is the single commitment that is cheapest to make now and a multi-year
retrofit later (see **Decide now vs defer**).

- The **kernel context** (owns the `Orrery`, the `SurfaceHost` / wgpu device, the
  chrome DOM, and every graph mutation path) is `!Send` by construction, via a
  `PhantomData<Rc<()>>` marker modelled on GPUI's `ForegroundExecutor`. It cannot
  be moved onto an actor thread even by accident.
- Kernel authority is private, not just `!Send`: `armillary` must not expose a
  graph handle, GPU handle, or `Arc<Mutex<...>>` escape hatch through a `Send`
  wrapper. The only public actor-facing state is an immutable snapshot DTO or a
  command/update message. This is what keeps the marker from becoming decorative.
- An **actor** has a `Send` handle and `Send` boundary messages; its internals
  may be `!Send` and pinned to its own thread (Nova, DOM, Stylo). The boundary
  shape is `spawn(...) -> (Handle: Send, Receiver<Update: Send>)`, and produced
  messages are owned `Send` values (`Scene`, `GraphContribution`, status/events).
  Invariant 2 becomes structural without requiring actor-local engine state to be
  movable.
- The **Xilem view layer** runs on the kernel thread, reaching the graph and
  frame through the `!Send` kernel context (GPUI's `Context` / `cx` pattern). The
  view layer is the kernel's programming model; actors are the off-thread escape
  hatch that talk to it only by message. That is "GPUI discipline + Xilem top" in
  one sentence, and it is the architectural through-line of this plan.

## Composing the alternative models

The actor kernel is the spine. The other concurrency and isolation models are not
competitors; each contributes a lesson at a specific layer. Borrow the lesson,
not the whole engine. (Sourced from the verified model menu, 2026-06-03.)

- **ECS / staged-parallel (Bevy).** Borrow the *data layout* for the graph
  canvas: archetype-packed node/edge storage with tick-based change detection is
  cache-friendly at high node counts. Do **not** adopt the parallel scheduler. It
  is the one model that does not compose here, because its value is a multicore
  scheduler that does not run on wasm (Bevy itself ships single-threaded there).
  Lives in: the `Orrery` / gyre node storage, kernel-side.
- **Incremental computation (salsa, self-adjusting computation).** Recompute only
  what a changed input invalidated. Serval already has the *domain-specific*
  instance: `IncrementalLayout` with damage classes (RepaintOnly / Restyled /
  Spliced / FullRecompute) over Stylo's restyle-with-snapshots. Keep it
  domain-specific; a generic salsa graph would carry heavier dependency-tracking
  overhead than the damage classification for this domain. Lives in:
  serval-layout, inside the content actor. The kernel-side lesson: derived state
  (frame layout, LOD, arrangement) carries explicit invalidation, never a shared
  mutable cache.
- **Structured concurrency / fork-join (rayon).** Parallelize *inside* one
  bounded operation (a single graph relayout, a Burn batch) over owned data,
  opportunistically. Gate it off on wasm (it needs `SharedArrayBuffer`; see
  ground truth). Servo's Layout 2020 lesson holds: opportunistic over a clean
  data model, never mandatory parallelism (mandatory broke on floats). Lives in:
  compute actors (P6), feature-gated.
- **Reactive dataflow (Xilem, signals).** The view layer. Declarative views
  diffed into serval's DOM, running on the `!Send` kernel context. Composes
  directly: actor kernel for state and ownership, reactive dataflow for the view.
  Lives in: `xilem_serval`, the kernel's programming model.
- **Utility-first systems (Tailwind).** Borrow the *bounded primitive vocabulary*
  with static lowering, not class-string authoring: a small token set the runtime
  sees as a finite, normalized representation beats open-ended per-component style
  APIs. For Mere, typed tokens / variants / primitives that lower to
  `netrender::Scene` or serval styles. Lives in: chrome/style tokens and
  projection lowering. Do **not** make Tailwind a dependency or utility strings a
  protocol.
- **Capability / wasm-component isolation.** Shared-nothing linking with explicit
  capabilities, for memory-safety isolation of semi-trusted modules. A *future
  plugin seam*, not a near-term primitive, and not a confidentiality boundary
  (see ground truth). Lives in: a deferred actor variant, decided when a plugin
  use-case is real.
- **OS multiprocess (Servo constellation, Chrome Site Isolation).** The only real
  confidentiality boundary, and the one Mere cut. Because `armillary`'s boundary
  is message-passing from day one, running a subsystem in its own OS process on
  native becomes a deployment toggle, not a rewrite. Servo's constellation works
  precisely because the message boundary predated the processes. Lives in: a
  deferred native-only switch. Do not build it; do not foreclose it.
- **CRDT / local-first (p2panda, Automerge).** The tessera sync layer. One
  reconciliation actor per peer: range-based set reconciliation (RBSR) to catch
  up, then gossip live-mode, feeding *validated, ordered* contributions to the
  kernel as messages, applied transactionally. The trust check (authorization as
  data, p2panda-auth / Keyhive style) is a validation pass inside that same
  actor, never a separate privileged guard. Lives in: the `sync` actor,
  generalized. It is the constellation pattern applied to peers instead of pages.

The convergent shape is one stack: **actor kernel (ownership) + reactive-dataflow
view (Xilem) + bounded utility primitives (Tailwind's lesson) + incremental
recompute (IncrementalLayout) + opportunistic in-operation parallelism (rayon,
gated) + per-peer sync actors (CRDT/log)**, with capability and multiprocess
isolation as deferred seams the message boundary keeps cheap.

## Findings (verified against the codebase, 2026-06-03)

**meerkat's winit `App` already is the kernel.** `crates/meerkat/src/main.rs`: it
owns the `Orrery` (graph + gyre physics), the chrome runner, the `SurfaceHost`
(the sole wgpu device), input routing, the composite pass
(`compose_external_texture`), session persistence, and the channel drains. Its
`render()` already composites an orrery scene **plus** a per-tile content scene.
Its `user_event()` already drains several typed channels. That drain is the
kernel inbox in embryo.

**`fetch` and `sync` are one proven actor shape.** `fetch::Fetcher` and
`sync::SyncHost` are the same pattern with different payloads: own a tokio
runtime, run background work, push a typed update over an `mpsc` channel plus an
`EventLoopProxy<()>` wake ("delivery model 2"), drained in `user_event`. Both
return `(Handle, Receiver<Update>)`; both treat networking as never-fatal. This
is the harness to generalize. The comment in `fetch.rs` already anticipates it:
the wake stays trivial "so persistence and sync can push their own typed
channels."

**The content pipeline is a pure `Scene` producer, but verify it off-thread.**
`crates/meerkat/src/card.rs::render_content_scene(url, state, registry, loader,
w, h) -> Scene` is pure CPU and runs on the UI thread today: HTML parses through
`serval_static_dom::StaticDocument` and the shared serval cascade/layout to a
scene; markdown/gemtext/text/feed route through a nematic `EngineDocument`;
everything else renders a synthesized document. Moving it onto a content-actor
thread is a relocation in principle. The caveat to clear before P2 is called
low-risk: the serval cascade leans on Stylo's process-global `GLOBAL_STYLE_DATA`
plus a `CascadeGuard` thread-local and a leaked per-thread sharing cache. Confirm
by runtime test that the cascade initializes and runs correctly off the main
thread and concurrently across N content-actor threads. That is the class of
latent single-thread assumption that turns a "relocation" into a debugging week,
so runtime-verify it rather than assume it. **Confirmed 2026-06-03** by the
`cascade-offthread` probe: identical glyph output on the main thread, off-thread,
and across 8 concurrent threads, no panic (see Progress).

**`Scene` is `Send` (confirmed); serializability is a separate, weaker claim.**
`netrender`'s `Scene` is a display list of plain-data `SceneOp`s. Fonts are
`FontBlob { data: peniko::Blob<u8>, index }` with custom blob serde; image
sources are CPU-side data keyed by `ImageKey` with custom deterministic serde.
The type holds no `Rc` and no GPU handle, so `Scene: Send` holds. That is the
fact the in-process architecture rests on: a content actor builds a `Scene` and
moves it to the kernel thread, and the kernel stays the sole GPU owner (GPU
appears only when the kernel's `Renderer` lowers the scene to `vello::Scene` at
rasterize time). *Serializability* (the `serde` feature) is a separate and weaker
property: before any cross-process path (P5 or a future multiprocess form)
relies on it, round-trip-test fonts and images. In-process P2-P4 need only
`Send`, which is solid.

**The graph/contribution boundary exists one level down.** The pure producers are
in `linked-data`: `from_jsonld_with_contexts(...) -> GraphContribution` and
`from_html_with_contexts(...) -> Vec<GraphContribution>`. meerkat's current
`harvest(&mut Graph, ...)` *fuses* produce and apply, so it is not the actor
boundary as written. P2 splits it into a pure `harvest_contributions(...) ->
Vec<GraphContribution>` (the content-actor half) plus the host kernel applying
through `Orrery::ingest_graph`. That producer/applier split *is* the
content-actor to host-kernel contract: an actor ships `Contribution`s, the kernel
applies them, the actor never touches the graph. v5 (`Graph::node_namespace_id`)
is the merge key, so contributions from different content actors citing the same
`@id` converge to one node.

**Nova is not in the tree yet.** No `nova` dependency anywhere in the workspace.
The content actor's first incarnation is the script-free `StaticDocument` (already
in `card.rs`); Nova is a later phase, and `!Send` from the day it lands.

## Load-bearing invariants

1. **The host kernel is the sole owner of the render/present GPU path and the
   graph.** Enforced by types, not vigilance: the kernel context is `!Send` (see
   **The typed boundary**). GPU *compute* (P6) is a separate concern handled
   there.
2. **Actors are CPU-only producers of `Send` messages** (`Scene`s,
   `GraphContribution`s, content events). Enforced by the actor produce signature
   returning only `Send` messages, so an actor holding a GPU handle or a mutable
   graph reference does not compile.
3. **State has one owner; cross-actor reads use immutable, purpose-built
   snapshots.** Hot reads (theme, a tile's graph slice) are `Arc` DTOs swapped by
   generation, which is what makes "lock-free" literal. Do **not** publish the
   whole `Graph` or `Graph::to_snapshot()` on every mutation: that snapshot is a
   persistence DTO and clones node metadata, media sidecars, edges, imports,
   fields, and couplings. Actor read models are narrow (`TileGraphSlice`,
   `ThemeSnapshot`, `NodeMediaSnapshot`) and rebuilt only when their inputs
   change. **Arena discipline lives inside this, not across it (2026-06-04):** an
   owner may store its state as a data-oriented arena (handle-indexed contiguous
   vectors, Nova-shaped; the ECS lesson for the graph), which is where deep arena
   integration belongs, within one owner on one thread; the arena is never shared
   across the boundary, and cross-owner data still moves as `Send`, handle-shaped
   snapshots. Actors between owners, arenas within owners: the boundary is the
   long-term structural commitment, the arena the within-owner optimization that
   also keeps the boundary cheap by keeping each owner's data clean and
   value-shaped.
4. **The kernel never blocks on an actor.** It composites each tile's
   last-delivered scene (stale-but-live) and keeps rendering. A slow actor
   degrades its own tile, not the frame. This discipline *is* the "offload only
   when a frame can't hold the work" rule.

The typed boundary makes invariants 1-2 compile-enforced rather than guarded; the
drift failure mode it prevents is the dominant risk (see **Risks**), and the
general law behind it is in **Decide now vs defer**.

## Message taxonomy

Inbound to the host kernel (the existing typed `fetch` / `sub` / `sync` receivers
behind the one bare wake, plus the new ones; a thin `KernelInbox` holds them, P0):

- `FetchDone`, `SubresourceDone`, `SyncStatus` (exist today)
- `SceneReady { tile, nav_generation, viewport_generation, scene }` — a content
  actor's new visual; dropped if either generation is stale (see Backpressure)
- `Contribution { actor, contribution }` — a graph mutation to apply
- `Title { tile, text }`, `LinkClicked { url }` — content events
- `ActorDied { tile, reason }` — fault, for the broken-tile placeholder

Outbound to a content actor:

- `Navigate { url }`, `Input { event }`, `Resize { w, h }`, `Teardown`
- `EvalScript { source }` (Nova phase)

## Actor lifecycle records

The host kernel owns a declarative `ActorSpec` per live tile / origin. This is
the respawn source of truth, not an actor-held state bag:

```rust
ActorSpec {
    tile_id,
    actor_kind,
    agent_cluster,
    current_url,
    nav_generation,
    viewport_generation,
    viewport,
    profile,
    capabilities,
}
```

On crash or channel disconnect, the kernel marks the tile broken, drops stale
messages by generation, spawns a fresh actor from the `ActorSpec`, and replays
only `Navigate` + `Resize` (plus the profile/capability setup). It does not replay
graph contributions as history; contributions are kernel-applied, idempotent
facts. If an actor needs restored document/session state later, that state is a
separate explicit snapshot in the spec, not an implicit borrow from the graph.

For script-free P2, the spec is per tile. When scripting lands, specs are keyed by
agent cluster (origin / browsing-context group semantics); non-scripting graph
tiles should remain projection-shaped by default, not origin-shaped, while sharing
fetch/cache/render assets underneath.

## Script protocol floor

P3's protocol is still real design work, but the minimum message families are
already constrained:

- **Lifecycle:** create actor, navigate, resize, teardown, crash/dead.
- **Input:** generation-tagged pointer/keyboard/text events, delivered only after
  kernel hit-testing and ignored by the actor if stale.
- **Script turns:** input task / timer task / fetch callback runs to completion;
  microtasks drain at checkpoints before a paint commit.
- **Network:** actor requests subresources from the I/O fetch actor; fetch replies
  are generation-tagged and delivered as actor input, not direct netfetcher calls.
- **Paint:** actor emits `SceneReady` only after DOM/style/layout has reached a
  coherent commit point; the kernel composites the latest accepted scene.
- **Graph output:** actor emits `Contribution` messages; the kernel validates and
  applies them through the graph boundary.

Borrow Servo's constellation/pipeline taxonomy for lifecycle and responsibility
names, but keep Mere's protocol explicit and channel-shaped rather than importing
Servo's process/IPC model.

## Backpressure and generations

Async scenes can arrive stale: a scene built for an old URL or an old size lands
after the tile navigated or resized. So every `SceneReady` carries a
`nav_generation` and a `viewport_generation`. The host kernel keeps the current
pair per tile, bumps them on navigate / resize, and **drops any scene whose
generations do not match**. Delivery is bounded and coalesced per tile (keep the
latest, never queue a backlog). Input is generation-aware too: an `Input` event
carries the generation it was hit-tested against, so a content actor ignores
input meant for a page it has already replaced.

## Threat model

Pinned: **Mere's content is semi-trusted.** Own code, audited engines (serval,
Nova), authenticated federation peers. Not arbitrary hostile multi-origin web
content.

This legitimizes the cut process boundary. In-process content actors give failure
isolation and memory-safety (a panicking or buggy page degrades its tile, see
Backpressure), which is sufficient for semi-trusted content. It is not a
confidentiality boundary. Inside a single browser renderer or OS process every
in-process boundary shares one address space; Chrome's own model assumes "any
active code can read any data in the same address space"
(<https://chromium.googlesource.com/chromium/src/+/HEAD/docs/security/side-channel-threat-model.md>).
Actor boundaries, a wasm sandbox, and component isolation all give memory-safety
and failure isolation, not Spectre resistance.

**The tripwire.** The day Mere wants to render genuinely hostile content
(arbitrary multi-origin web pages, untrusted third-party plugins), the in-process
constellation is not enough, and the answer is the OS-process boundary (native)
or the browser's own cross-origin isolation (PWA) that this design cut. On the
browser/PWA target, content Mere fetches and renders itself in its own origin
gets no Site Isolation; only content delegated to a cross-origin iframe or worker
does. Treat any feature that crosses the semi-trusted line as the feature that
pays for the process boundary, and design it knowing the cost is coming.

## Technical ground truth (verified 2026-06-03)

Grounds P5 (sandbox) and the browser target. Researched and adversarially
verified against current primary sources.

- **Wasmtime cannot JIT inside browser-wasm.** WebAssembly is a Harvard-architecture
  model with no instruction to emit and then execute machine code; Wasmtime's
  Cranelift and Winch backends require host-OS executable pages the browser never
  grants a guest (<https://docs.wasmtime.dev/stability-platform-support.html>).
  Wasmtime's **Pulley** interpreter backend can in principle compile to wasm32 and
  interpret guests, but an in-browser end-to-end run is undemonstrated as of
  mid-2026 and self-estimates roughly 10x slowdown. So even the technically
  possible path is slow and unproven. This confirms the scripting decision: Rhai
  (a Rust interpreter that runs anywhere wasm32 ships) plus Burn-wgpu (which
  delegates to WebGPU), with no Wasmtime.
- **Browser parallelism: Web Workers are the baseline; shared-memory threads are
  gated.** Message-passing Web Workers need no special headers and work
  everywhere; this is the guaranteed path and the one any hot path may depend on.
  Shared-memory threads (`SharedArrayBuffer` plus atomics, what rayon and
  `wasm-bindgen-rayon` need) require cross-origin isolation (COOP plus COEP,
  <https://web.dev/articles/coop-coep>) and a nightly toolchain with `build-std`
  that has been intermittently broken through 2025 (rust-lang #145101). Treat SAB
  threads as an optional, feature-gated accelerator with a clean single-threaded
  fallback; never let a hot path require them.
- **There is no in-process Spectre boundary** (see Threat model). Confidentiality
  isolation is OS processes (native) or the browser (PWA), not anything Mere
  builds in-process.

P5 follows from this. An in-process wasmtime sandbox would require compiling the
entire serval + Nova content engine to wasm32 to run under it, for a boundary
that is memory-safety rather than Spectre resistance anyway. For semi-trusted
content the actor-thread boundary already gives the failure and memory-safety
isolation that matters, so P5 is descoped: real isolation, if ever needed, is an
OS subprocess on native or the browser on PWA.

## Decide now vs defer

The historical law (Stylo parallelized cheaply because its cascade data model was
already a pure function of parent values plus matched rules; Chrome's Site
Isolation cost roughly five years and about four thousand commits because the web
assumed synchronous cross-frame scripting): **the cost of adding a boundary later
is set almost entirely by whether the surrounding code assumed synchronous
shared-memory access across it.** Designed-in is near-free; retrofitted against
synchronous shared state is a multi-year ordeal. The test for any commitment:
does it establish or violate the message-passing boundary?

Decide now (expensive to retrofit), addressed in P0-P1:

- The async message-passing boundary, universal: no subsystem ever gets a
  synchronous handle into kernel state. Mere is already about 90% there.
- `Send`-ness on the *messages*, not the state: the chrome DOM stays `!Send` on
  the host-kernel thread, while content DOM / Nova / Stylo may be `!Send` on
  their owning actor thread. Only boundary messages and handles are `Send`.
- The unit of isolation: kernel plus one task per long-lived subsystem, one sync
  actor per peer, the graph-session / window as the coarse unit.
- Owned-not-shared data with explicit invalidation (IncrementalLayout,
  kernel-owned physics).
- The written threat model (semi-trusted).

Defer (cheap once the boundary holds): extra workers, executor choice, leaf
parallelization (rayon, gated), SAB threads, the CRDT library and reconciliation
primitive, capability/plugin isolation, the OS-multiprocess toggle, and JSPI
adoption (the wasm stack-switching API for ergonomic async; stable in Chrome and
Firefox, not yet Safari as of mid-2026).

## Implementation status (audited 2026-06-04 against the code)

The first Progress entry ("no code written yet") is superseded; a later entry logs
P0-P2 as implemented. This consolidates the per-phase status in one place, audited
against the tree, and surfaces the gaps the log leaves implicit (P0 is built but not
yet embedded in the `App`; P4's plural pool exists, arrived via the by-hand-tiles
arc, but its self-healing does not). Per phase, with evidence:

- **P0 typed boundary — done** (2026-06-04, `bf7b9b1`). `armillary::KernelThread`
  (the `!Send` kernel marker, built + tested in `boundary.rs`) is now embedded in
  meerkat's `App`, and the I/O receivers are grouped behind a named `KernelInbox`
  (fetch + sync) with `user_event` documented as the single dispatch. Per-subsystem
  channels kept; no mega enum.
- **P1 harness — done** (sync landed 2026-06-04, `1131de2`). `armillary::{spawn,
  spawn_on, ActorHandle, Emitter, Wake, Pool}` + `Generations`. All three I/O actors
  run through it: `fetch` (`spawn_fetcher`), the content actor (`spawn_content`, now
  pooled), and `sync` (`spawn_sync`) — sync's run closure builds its tokio runtime on
  the actor thread, so armillary stays runtime-free / wasm-clean while sync joins the
  typed boundary. (The "third trivial actor" is effectively the content actor.)
- **P2 content off-thread + producer/applier split — done.** `spawn_content` runs
  the cascade off the UI thread and ships generation-tagged `ContentUpdate::Scene`;
  the constellation accepts the latest. `ingest::harvest_contributions` is the pure
  producer, split from `harvest` (the kernel-side apply). Cascade-off-thread is
  guarded by serval's `cascade_is_deterministic_off_thread_and_concurrent` test.
- **P3 Nova — not started.** Content is `StaticDocument`; no JS engine yet.
- **P4 N actors + lifecycle — done** (2026-06-04). `meerkat::Constellation` is the
  plural, per-tile actor pool: spawn / reap / LRU eviction over a cap / keep-warm /
  background. **Self-healing** (`a3d6f8e`): `drain` detects a dead actor's
  disconnected channel and respawns it (fresh worker, `shown` cleared so the next
  `drive` re-`Show`s the page), keeping the last scene until it recovers, capped at
  `MAX_RESPAWNS`. **Broken-tile placeholder** (`120f8e3`): a tab that died before
  rendering shows a "Reloading…" label (`is_recovering`). **Thread pooling**
  (`251e205`): content actors run on `armillary::Pool`, a growable reusing worker
  pool, so OS threads (and the leaked Stylo thread-local) are bounded by peak
  concurrent tabs, not total opened.
- **P5 — descoped** (as written).
- **P6 compute actors — not started.**

P0, P1, P2, and P4 are now done (2026-06-04), as is the compositor perf fix (a
per-tile texture cache keyed by the scene generation; not a phase, a kernel
concern). **The only phases left are the two big features: P3** (a JS engine in the
content actor) and **P6** (compute / mesh actors). The actor spine — typed boundary,
harness, all three I/O actors, off-thread content, the plural self-healing pooled
constellation — is complete.

## Phases (done-conditions, not dates)

- **P0 Name the host-kernel inbox and the typed boundary.** A thin `KernelInbox`
  that *holds* the existing typed receivers (`fetch` / `sub` / `sync`) behind the
  one bare `EventLoopProxy<()>` wake, with a documented dispatch in `user_event`;
  and the `!Send` kernel-context marker plus the `Send` actor-handle shape
  introduced in `armillary`'s types. Keep the typed-channel-per-subsystem
  ownership; do not collapse it into one mega enum (that worsens ownership). No
  behavior change. *Done:* one documented place that reads what the kernel is
  told, the wake/receiver seam intact, and the kernel context is `!Send` while
  actor handles are `Send` by type.
- **P1 The `armillary` harness.** Stand up the `armillary` crate: one `Subsystem`
  shape (`spawn(proxy, ...) -> (Handle, Receiver<Update>)`) plus the
  inbox/dispatch from P0 and the typed boundary. Express `fetch` and `sync`
  through it; add a third trivial actor (persistence-write or link-resolution) to
  prove generality. *Done:* three actors, one harness, one type-enforced
  boundary, in armillary.
- **P2 Static-DOM content actor.** Move `render_content_scene` onto a per-tile
  content-actor thread; the actor ships generation-tagged `SceneReady` and the
  host kernel composites the latest. (The cascade off-thread caveat is cleared by
  the `cascade-offthread` probe; see Progress.) Split meerkat's
  `harvest(&mut Graph, ...)` into a pure `harvest_contributions(...) ->
  Vec<GraphContribution>` the actor runs, shipping `Contribution`s the kernel
  applies through `Orrery::ingest_graph` (the actor never touches the graph). A
  panicking actor is isolated to its thread, so the host survives (the broken-tile
  placeholder + respawn from `ActorSpec` is P4). *Done:* content leaves the UI
  thread with the
  cascade confirmed safe off-thread, the producer/applier split is honest, and a
  content panic no longer kills the host.
- **P3 Nova.** A scripted page runs JS in the content actor (Nova, dedicated
  thread, `!Send`); DOM mutations reflow to a new `Scene`; the input -> script ->
  paint protocol implements the **Script protocol floor** above, including
  run-to-completion script turns, microtask checkpoints, subresource requests by
  message, and generation-tagged scene commits. *Done:* a scripted page is
  interactive in a tile without giving the actor synchronous kernel access. **The
  JS engine is a per-target binding, not fixed to Nova** (2026-06-04): Nova is 1.0
  (2026-03-15) but ~80% Test262 with no wasm execution, and its data-oriented
  arenas hit the wasm32 4GB ceiling in-browser, relieved only by Memory64 (shipped
  in Chrome/Firefox/Node, absent in Safari/WebKit, so absent on iOS). So the
  content actor runs **Nova native** and **Boa in-browser** (v0.21.1, ~94% Test262,
  wasm-safe), especially on Safari/iOS; Nova-in-browser waits on a wasm32 build plus
  WebKit Memory64. The engine is internal to the content actor, so swapping it is
  actor-local, not a boundary change.
- **P4 N actors + lifecycle.** One actor per open origin; the kernel spawns and
  reaps; per-origin pooling; respawn uses the kernel-owned `ActorSpec`. The fault
  default is thread respawn, not in-place `catch_unwind`: a panic mid-cascade can
  leave Stylo's thread-local sharing cache inconsistent, so let the actor thread
  die (the kernel observes the channel disconnect, paints the broken-tile
  placeholder, and respawns a fresh thread with fresh thread-locals from the
  spec). **Leak caveat (verified 2026-06-04):** Stylo's per-thread sharing cache
  is a *leaked* thread-local, so every fresh content-actor thread leaks one that
  is never reclaimed. Recycle content-actor threads (a bounded pool, which the
  per-origin pooling above already wants) rather than spinning a brand-new OS
  thread per fault / navigation, or thread churn leaks unboundedly. *Done:* the
  constellation is plural and self-healing.
- **P5 Isolation, if ever needed (descoped).** No in-process wasm sandbox (see
  **Technical ground truth** for why it is the wrong boundary). If a feature
  crosses the semi-trusted line, the isolation answer is an OS subprocess on
  native or the browser on PWA, and the P0 message boundary makes the
  native-subprocess form a toggle, not a rewrite. *Done:* the tripwire is
  documented and the subprocess path is a known option, not built until a real
  untrusted-content use-case exists.
- **P6 Compute actors.** Gyre physics / heavy layout / Burn spill to a compute
  actor when the kernel's frame budget is blown; results composite async
  (last-writer-wins). Shared-memory parallelism (rayon over `SharedArrayBuffer`)
  is feature-gated per **Technical ground truth**, never load-bearing. GPU compute
  (Burn-wgpu) is the boundary case: it must not touch the render/present path, so
  it either submits jobs through a kernel-owned GPU service or runs on a separate
  compute device/queue. CPU compute (Burn-ndarray, physics) has no such
  constraint. *Done:* a frame that cannot hold the work sheds it without
  stalling, and GPU compute never contends with render. **A compute actor is the
  local case of the mesh (2026-06-04):** the same request-to-result message can
  target a local thread or a remote device over p2panda, so local frame-spill,
  federated mesh compute, and communal big-model hosting are one abstraction at two
  scopes (the mesh is a compute actor with a remote recipient). See the
  [resource-coordination brief](../research/2026-06-04_resource_coordination_brief.md).

## Risks and hard parts

Each risk's full treatment lives in its home section; this is the short list of
what is most likely to break, the mitigation, and a pointer.

- **Boundary drift (dominant).** An actor acquiring a synchronous handle into
  kernel state re-creates the synchronous-shared-memory assumption that cost the
  Gecko/Chrome-Site-Isolation retrofits. Mitigation: the `!Send` kernel context +
  `Send`-only messages make it a compile error (P0). Cheapest-now,
  most-expensive-later.
- **The script-to-kernel protocol (P3) is the real design work.** The message
  families are constrained (see **Script protocol floor**); the work is
  implementing them over channels without giving the actor synchronous kernel
  access.
- **`!Send` Nova needs a dedicated thread from P2,** so the content actor is
  thread-shaped before scripting lands and Nova drops in without a rewrite.
- **Cascade off-thread (P2) — cleared 2026-06-03.** The `cascade-offthread` probe
  confirms the serval cascade runs correctly off the main thread and under 8-way
  concurrency (see **Findings** / **Progress**).
- **Declarative lifecycle.** Respawn from the kernel-owned `ActorSpec`, never ad
  hoc closure state, or fault recovery becomes another hidden shared-state seam
  (see **Actor lifecycle records**).
- **Live constraints, not open problems.** The kernel never awaits an actor (see
  **Backpressure**); any feature crossing the semi-trusted line pays for a process
  boundary (see **Threat model**).

## Progress

- **2026-06-03.** Examined the codebase and wrote this plan. Verified: meerkat's
  `App` is the kernel; `fetch`/`sync` are one actor shape; `render_content_scene`
  is a pure `Scene` producer; `netrender::Scene` is `Send` + serializable
  (refined later: `Send` confirmed, serializability is the weaker separate claim);
  `StaticDocument` is the near-term content engine and Nova is not yet in the
  tree; the linked-data contribution/apply split is the content-actor contract
  and v5 is the merge key. No code written yet; P0 is the first step.
- **2026-06-03.** Decisions resolved (Mark): per-origin content actors (the
  Firefox model); subresource fetch through the I/O fetch actor by message; the
  runtime crate is **`armillary`** (new, host-neutral). Shared-read snapshots and
  the fault model stand as working defaults.
- **2026-06-03.** Incorporated an external review. Two load-bearing edits: the
  contribution boundary was overstated (the pure producer is `linked-data`'s
  `from_*_with_contexts`, not meerkat's `harvest`, which fuses produce + apply),
  so P2 now splits it into `harvest_contributions`; and `SceneReady` gained a
  generation / backpressure protocol. Also: "host kernel" terminology vs the
  `kernel` crate; invariant 1 narrowed to the render/present GPU path with P6
  GPU-compute called out; granularity refined to the agent-cluster model; P0 kept
  as a `KernelInbox` wrapper, not a mega enum.
- **2026-06-03.** Refactored against verified concurrency research (wasm-in-browser,
  browser threading, the model menu) and a codebase critique. Major changes: the
  kernel/actor boundary is now **type-enforced** (`!Send` kernel context, `Send`
  actors), making the GPUI lesson structural, with the Xilem view layer named as
  the kernel-thread programming model on that context. Added a **Composing the
  alternative models** section (ECS data layout, incremental-query,
  rayon, reactive-dataflow, capability isolation, OS-multiprocess, CRDT) showing
  each as a borrowed lesson at a layer rather than a competitor. Pinned the
  **threat model** to semi-trusted with the Spectre / Site-Isolation tripwire, and
  descoped **P5** (in-process wasmtime) accordingly. Added the **Technical ground
  truth** (verified, cited). Affirmed `Scene: Send` by code inspection and
  separated it from the weaker serializability claim. Flagged the serval cascade's
  off-thread and concurrent thread-local/global behavior as a P2 verification
  gate. Added the **Decide now vs defer** framing, switched the fault default to
  thread respawn over in-place `catch_unwind`, and recorded the snapshot
  granularity/cost open question (invariant 3).
- **2026-06-03.** Added the Tailwind lesson to the model menu: borrow the bounded
  primitive vocabulary and static lowering pattern, not Tailwind itself. For Mere,
  that means typed tokens / variants / small composable primitives that lower into
  serval styles or `netrender::Scene`, never utility strings as protocol and never
  a styling framework inside `armillary`.
- **2026-06-03.** Tightened the implementation constraints: kernel authority must
  remain private (`!Send` alone is not enough if a `Send` wrapper leaks graph/GPU
  access); actor internals may be `!Send` on their own thread while handles and
  boundary messages are `Send`; actor read models are purpose-built immutable DTOs
  rather than whole-graph persistence snapshots; respawn is driven by a
  kernel-owned `ActorSpec`; and P3 now has a script-protocol floor (lifecycle,
  generation-tagged input, run-to-completion turns, microtask checkpoints,
  message-based fetch, coherent paint commits, contribution output).
- **2026-06-04.** Cascade off-thread guarantee hardened on the serval side
  (lesson #1). Added `cascade_is_deterministic_off_thread_and_concurrent` to
  `ports/pelt-live` (serval): the Scene's draw ops are byte-identical on the main
  thread, off-thread, and across 8 concurrent threads, so the property P2 leans on
  is now a `cargo test` regression guard living with the engine (stronger than the
  mere-side glyph-count probe, which counts rather than compares). Path audit: safe
  because Stylo's `GLOBAL_STYLE_DATA` is read-shared, the `Stylist` is built fresh
  per call, and the cascade context (`CascadeGuard`) + Stylo's sharing cache are
  thread-locals. The one finding fed back into **P4**: that sharing cache is a
  *leaked* thread-local, so respawn-by-fresh-thread leaks one per thread; pool /
  recycle content-actor threads rather than spinning a new OS thread per fault.
- **2026-06-03.** Redundancy pass. The Risks section had become a restatement of
  its home sections, so it was compressed to terse risk + mitigation + pointer
  bullets; P5 and the invariants-closing note were trimmed to point at
  **Technical ground truth** and **Risks** rather than re-argue them; invariant 2
  broadened from "two message kinds" to match the taxonomy (scenes, contributions,
  content events). Follow-ups: P6's shared-memory line trimmed to a pointer, the
  Tailwind bullet halved, JSPI given inline context in the defer list, and Progress
  entry 1's `Scene` serializability wording tagged as later-refined.
- **2026-06-03 — P0-P2 implemented and smoke-validated.** First code. **`armillary`**
  scaffolded as the host-neutral runtime: the `!Send` `KernelThread` boundary marker
  (proven by a `compile_fail` doctest, not asserted), the actor harness
  (`spawn(wake, run) -> (ActorHandle, Receiver)` with the runtime / engine built *on*
  the actor thread so `!Send` internals never cross), and the generation types
  (`39a8530`). The **serval cascade off-thread gate** flagged above is **cleared** by
  the `cascade-offthread` probe ([`crates/probes/cascade-offthread`](../../../crates/probes/cascade-offthread)):
  the `card.rs::html_scene` path renders identically on the main thread, off-thread,
  and across 8 concurrent threads, no panic (`f8f79e5`). **P1:** the `fetch`
  subsystem runs through the harness (`FetchCommand` / `FetchUpdate` via
  `spawn_fetcher`; `8924018`). `harvest` split into the pure `harvest_contributions`
  producer (`d02feb6`). **P2:** the `meerkat::content` actor renders the focused card
  off the UI thread (`Show` / `Resize` / `Resource` -> `Scene` / `Wanted` /
  `Contribution`, harvesting on `Show`; `9291c72`, `a6339f7`), wired into the render
  loop (`render()` composites the actor's latest scene; `user_event` drains it; stale
  scenes dropped by generation; the demand-fetch is `Wanted` -> host fetch ->
  `Resource`; `dfe2153`). **Smoke-validated on screen:** `https://example.com`
  renders off-thread *with its own CSS* (the full `Show` -> `Scene` -> `Wanted` ->
  fetch -> `Resource` -> re-render loop), plus async card, resize, and
  click-blank-then-show. meerkat 40 lib + 19 bin tests green throughout. **Carried
  gaps:** fault recovery (detect a dead content actor -> broken-tile placeholder ->
  respawn from `ActorSpec`) is **P4**, not built, so a content panic isolates the
  host but freezes the card; fetch-on-focus (a node click loading its page) is an
  open enhancement (pre-existing behavior, not a P2 regression); and
  `sync`-via-armillary is deferred (the last I/O subsystem).
- **2026-06-04.** Scripting/engine layer folded in (from the resource-coordination
  thread). Four consequences recorded: (1) **P3** is now a per-target JS-engine
  binding (Nova native; Boa in-browser, the only option on Safari/iOS until WebKit
  ships Memory64; Nova-in-browser waits on a wasm32 build + Memory64), an
  actor-local swap not a boundary change; engine facts verified 2026-06-04 (Nova 1.0
  but ~80% Test262 and no wasm execution; Boa v0.21.1 ~94%, wasm-safe; Memory64 in
  Chrome/Firefox/Node, not Safari). (2) **Invariant 3** sharpened: arena discipline
  is per-owner, never shared across the boundary (actors between owners, arenas
  within owners). (3) **P6** compute actors are the local case of the mesh (same
  request-to-result message, local thread or remote device over p2panda). (4) The
  compute tier splits into raw compute and an async **Rune agent/intelligence
  actor**, with the scripting layers mapped onto the actor tiers (Rust+arena / JS
  content / Rune orchestration / Rhai declarative + coordination policy). The Rune
  verification pass landed same day (23/25 claims confirmed) and **confirmed the
  placement with one correction**: Rhai's edge for untrusted policy is maturity and
  breadth of documented limits, not a Rune capability gap (Rune has opt-in Context +
  per-instruction budget + memory caps but self-labels its sandbox WIP at 0.x), and
  **determinism is documented by neither engine**, so re-run-to-a-hash verification
  must be constrained host-side. No code; design-dissemination edit.
- **2026-06-04 — Status audit against the code.** Added the **Implementation status**
  section above, verified by reading the tree, not the log. Confirms the P0-P2 entry
  and surfaces what it left implicit: `armillary::KernelThread` is built + tested but
  **not embedded** in meerkat's `App`, and there is no named `KernelInbox` (the kernel
  drains `fetch_rx` / `sync_rx` / the constellation raw in `user_event`); `sync` is
  still its own tokio runtime, not an armillary actor. **P4's plural pool is done**
  (`meerkat::Constellation`: spawn / reap / LRU / keep-warm / background, arrived via
  the by-hand-tiles arc), but its **self-healing is not** (no `ActorSpec` respawn or
  broken-tile placeholder). Real next steps: finish P0 (embed the marker), close P4's
  self-healing, then P3. Separately, the tiled-view perf cost (re-rasterizing every
  tile each frame) is a compositor concern — a per-tile texture cache keyed by the
  scene generation — not one of these phases.
- **2026-06-04 — P0 + P4 self-healing + the perf fix landed.** **P0** (`bf7b9b1`):
  `armillary::KernelThread` embedded in `App` (the kernel context is `!Send` by type)
  and the I/O receivers grouped behind a named `KernelInbox`, with `user_event` the
  documented dispatch. **Perf** (`158354f`): the constellation stamps a `scene_version`
  per accepted scene; the host caches each tile's rasterized texture and re-rasterizes
  only on a version/size change (idle tiles cost ~zero per frame, where before every
  tile was re-rasterized every frame). **P4 self-healing** (`a3d6f8e`): `drain` detects
  a disconnected actor channel (its thread died) and respawns the tab on a fresh
  thread, clearing `shown` to replay the page, capped at `MAX_RESPAWNS`; the last scene
  holds until recovery. Remaining: P3 (JS engine), P4 polish (labeled placeholder +
  thread pooling), sync-through-armillary (needs an async-actor shape), P6.
- **2026-06-04 — P4 polish + P1 closed; the actor spine is complete.** **Broken-tile
  placeholder** (`120f8e3`): `Constellation::is_recovering` drives a "Reloading…"
  label (platen-view `SlotPlan.recovering`) for a tab that died before rendering.
  **Thread pooling** (`251e205`): `armillary::Pool` — a growable, reusing worker pool
  (`Mutex<queue + idle>` + `Condvar`, the grow decision under the lock so no job is
  stranded); content actors run on it via `spawn_on`, bounding OS threads and the
  leaked Stylo thread-local to peak concurrent tabs. The fetcher stays a plain
  thread (one long-lived actor). **sync through armillary** (`1131de2`): the prior
  "needs an async-actor shape" worry was wrong — armillary's `run` closure can build
  the tokio runtime on the actor thread, so `sync::spawn_sync` makes sync an actor
  (poll task emits via the `Emitter`; `connect` is `SyncCommand::Connect`) with
  armillary still tokio-free. So **P0-P2 + P4** and **P1** are all done; only P3 and
  P6 (the two big features) remain. Whole workspace builds; armillary 7, meerkat
  43+23 green.
