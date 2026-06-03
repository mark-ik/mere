# Actor Constellation Plan

A message-passing runtime for Mere: a single-threaded **kernel** (graph + frame +
compositor + routing) surrounded by a constellation of **actors** (I/O, content,
compute). It is Servo's constellation done **in-process**: the kernel fuses the
constellation and compositor; content actors are the pipelines; scenes travel as
messages rather than IPC-serialized surfaces. The big simplification over Servo is
that an in-process content actor hands the kernel a `Scene` directly, so there is
no cross-process surface sharing to engineer.

This is an evolution of [meerkat](../../../crates/meerkat), not a rewrite. The
kernel already exists (meerkat's winit `App`), the I/O actors already exist
(`fetch`, `sync`), and the content pipeline is already a pure scene producer. The
plan names the pattern and generalizes it.

The runtime lives in a new host-neutral crate, **`armillary`**: the kernel harness
(inbox router plus dispatch), the actor traits and lifecycle, and the message
taxonomy. meerkat is the concrete kernel built on it (winit, wgpu, the graph);
content and I/O actors are armillary actors. The name fits the shape: an armillary
sphere is a frame of rings around a central point, which is exactly this structure,
the kernel at the center with the actor rings around it (the content actors its
constellation).

Related: [modular integration plan](2026-06-02_modular_integration_plan.md) (the
host model and S-phases), [linked-data ingest plan](2026-05-22_linked_data_ingest_export_plan.md)
(the contribution boundary + v5 identity), [netfetcher plan](2026-05-25_netfetcher_plan.md)
(the network I/O actor).

## The four layers

- **Kernel (one thread).** Owns the graph, the frame (tile layout), message
  routing, and render. Simple and lock-free because it is the single owner of the
  two things everything else wants: the GPU and the graph.
- **I/O actors.** Network fetch, p2p sync, persistence writes, link resolution.
  Background work that returns data, never UI.
- **Content actors (the real constellation).** One per page/origin running
  untrusted content: owns its DOM (and later its Nova JS engine, `!Send`, pinned
  to the thread), ships paint-lists/scenes to the kernel. Isolation is the actor
  boundary, optionally hardened by a native wasm sandbox. A Servo content process,
  minus the process.
- **Compute actors (later).** Heavy layout, graph physics, Burn-wgpu. Offloaded
  only when a frame cannot hold the work.

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
return `(Handle, Receiver<Update>)`; both treat networking as never-fatal. This is
the harness to generalize. The comment in `fetch.rs` already anticipates it: the
wake stays trivial "so persistence and sync can push their own typed channels."

**The content pipeline is already a pure `Scene` producer.**
`crates/meerkat/src/card.rs::render_content_scene(url, state, registry, loader, w,
h) -> Scene` is pure CPU: HTML parses through `serval_static_dom::StaticDocument`
and the shared serval cascade/layout to a scene; markdown/gemtext/text/feed route
through a nematic `EngineDocument`; everything else renders a synthesized
document. It runs on the UI thread today. Moving it onto a content-actor thread is
a relocation, not a rewrite.

**`Scene` is `Send` and serializable.** `netrender`'s `Scene` is a display list of
plain-data `SceneOp`s (`scene.rs`: every op derives `Clone` and, under the `serde`
feature, `Serialize`/`Deserialize`; no `Rc`, no GPU handles, no `!Send`). The GPU
appears only when the kernel's `Renderer` lowers the scene to `vello::Scene` at
rasterize time. So a content actor builds a `Scene` and ships it; the kernel
remains the sole GPU owner. **This is the fact the whole architecture rests on.**

**The graph/contribution boundary already exists.** This session's linked-data
work made `harvest(...) -> GraphContribution` a pure producer and
`Orrery::ingest_graph(closure)` the single-threaded applier. That producer/applier
split *is* the content-actor to kernel contract: an actor ships a `Contribution`,
the kernel applies it. The v5 deterministic node id
(`Graph::node_namespace_id`) is the merge key, so contributions from different
content actors citing the same `@id` converge to one node.

**Nova is not in the tree yet.** No `nova` dependency anywhere in the workspace.
The content actor's first incarnation is the script-free `StaticDocument` (already
in `card.rs`); Nova is a later phase, and `!Send` from the day it lands.

## Load-bearing invariants

1. **The kernel is the sole owner of the GPU and the graph.** Nothing else holds
   the wgpu device or a mutable graph reference.
2. **Actors are CPU-only producers of two `Send` message kinds:** `Scene`s and
   `GraphContribution`s. They never hold a GPU handle or mutate the graph.
3. **State has one owner; cross-actor reads use an immutable snapshot.** Hot reads
   (theme, a tile's graph slice) are an `Arc<Snapshot>` swapped atomically. This is
   what makes "lock-free" literal.
4. **The kernel never blocks on an actor.** It composites each tile's
   last-delivered scene (stale-but-live) and keeps rendering. A slow actor degrades
   its own tile, not the frame. This discipline *is* the "offload only when a frame
   can't hold the work" rule.

If an actor ever gets a mutable graph handle or a wgpu handle, invariants 1-2 are
gone and the kernel is no longer lock-free. Guard that boundary above all.

## Message taxonomy

Inbound to the kernel (one router behind the single wake; the existing `fetch` /
`sub` / `sync` receivers are early instances):

- `FetchDone`, `SubresourceDone`, `SyncStatus` (exist today)
- `SceneReady { tile, scene }` — a content actor's new visual
- `Contribution { actor, contribution }` — a graph mutation to apply
- `Title { tile, text }`, `LinkClicked { url }` — content events
- `ActorDied { tile, reason }` — fault, for the broken-tile placeholder

Outbound to a content actor:

- `Navigate { url }`, `Input { event }`, `Resize { w, h }`, `Teardown`
- `EvalScript { source }` (Nova phase)

## Decisions (resolved 2026-06-03)

- **Granularity: per-origin.** A content actor owns an origin, not a single page,
  so same-origin tiles share JS state (web-compat: `window.open`, synchronous DOM
  access). The Firefox content-process model. Non-scripting graph tiles can still
  collapse to per-page; expose that as a setting rather than hardcode.
- **Subresource fetch: through the I/O fetch actor, by message.** A content actor
  requests a subresource from the fetch actor and receives bytes, rather than
  owning its own netfetcher. This centralizes the cache / cookie jar / netfetcher
  and keeps "Mere owns networking" literal.
- **Runtime crate: `armillary`** (new, host-neutral). Houses the kernel harness,
  the actor traits + lifecycle, and the message taxonomy. meerkat consumes it.

Working defaults (not contested, revisit if they bite):

- **Shared-read kernel state.** `Arc<Snapshot>` atomic swap for hot reads (theme,
  per-tile graph slices) over a request/response round-trip.
- **Fault model.** `catch_unwind` per actor loop; on death the kernel paints a
  broken-tile placeholder and can respawn from the last navigation. A reliability
  gain over today, where a content-render panic takes the whole host down.

## Phases (done-conditions, not dates)

- **P0 Name the kernel inbox.** A single inbox type (or a documented router)
  unifies the `fetch` / `sub` / `sync` drains; `user_event` dispatches by variant.
  No behavior change. *Done:* one place to read "what the kernel is told."
- **P1 The `armillary` harness.** Stand up the `armillary` crate: one `Subsystem`
  shape (`spawn(proxy, ...) -> (Handle, Receiver<Update>)`) plus the inbox/dispatch
  from P0. Express `fetch` and `sync` through it; add a third trivial actor
  (persistence-write or link-resolution) to prove generality. *Done:* three actors,
  one harness, in armillary.
- **P2 Static-DOM content actor.** Move `render_content_scene` onto a
  content-actor thread; the focused tile's HTML renders off the UI thread and the
  kernel composites the delivered `Scene`. JSON-LD harvest ships a `Contribution`
  the kernel applies (wiring this session's linked-data path through the actor
  boundary). A panicking actor shows a broken-tile placeholder. *Done:* content
  leaves the UI thread; a content panic no longer kills the host.
- **P3 Nova.** A scripted page runs JS in the content actor (Nova, dedicated
  thread, `!Send`); DOM mutations reflow to a new `Scene`; the input -> script ->
  paint protocol is real. *Done:* a scripted page is interactive in a tile.
- **P4 N actors + lifecycle.** One actor per open origin; the kernel spawns and
  reaps; per-origin pooling; respawn-from-last-navigation on fault. *Done:* the
  constellation is plural and self-healing.
- **P5 Sandbox hardening (native-only, optional).** A content actor's engine runs
  inside a native wasm sandbox (wasmtime) for memory isolation, behind a flag.
  Absent on the browser/PWA target, where the browser is the sandbox. *Done:* an
  opt-in hardened mode on desktop.
- **P6 Compute actors.** Gyre physics / heavy layout / Burn-wgpu spill to a compute
  actor when the kernel's frame budget is blown; results composite async
  (last-writer-wins). *Done:* a frame that cannot hold the work sheds it without
  stalling.

## Risks and hard parts

- **The script-to-kernel protocol is the real design work.** Borrow Servo's
  constellation/pipeline message taxonomy (pipeline lifecycle, the
  script/layout/compositor splits) as a reference and implement it over channels.
- **`!Send` Nova forces a dedicated thread per content actor** and a careful
  message API: no shared DOM, no shared engine handle, everything by message. Build
  the content actor as a dedicated thread from P2 so Nova lands without a rewrite.
- **Backpressure discipline.** The kernel must never await an actor. Last-scene
  compositing per tile is the mechanism.
- **The wasm sandbox is the least-specified layer and is native-only.** Two
  isolation stories by target: wasmtime on desktop, the browser on the PWA target.
- **The graph boundary.** Contributions in, snapshots out, never a mutable graph
  handle to an actor.

## Progress

- **2026-06-03.** Examined the codebase and wrote this plan. Verified: meerkat's
  `App` is the kernel; `fetch`/`sync` are one actor shape; `render_content_scene`
  is a pure `Scene` producer; `netrender::Scene` is `Send` + serializable;
  `StaticDocument` is the near-term content engine and Nova is not yet in the tree;
  the linked-data contribution/apply split is the content-actor contract and v5 is
  the merge key. No code written yet; P0 is the first step.
- **2026-06-03.** Decisions resolved (Mark): per-origin content actors (the Firefox
  model); subresource fetch through the I/O fetch actor by message; the runtime
  crate is **`armillary`** (new, host-neutral). Shared-read snapshots and the
  `catch_unwind` fault model stand as the working defaults.
