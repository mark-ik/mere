# register-renderer

v0 minimal of the renderer registry — the trait + dispatch contract that lets
renderer crates (`mere-masonry`, `scrying-engine`, future `serval-web` /
`wry-web` / `cartography-graph` / `vello-canvas` / `chrome-edge` adapters)
coexist as co-resident tenants of Mere's spatial chrome IR substrate.

Built against [the contract brief](../../design_docs/mere_docs/research/2026-05-15_renderer_registry_contract_brief.md).
This crate is the implementation of the brief's §3 trait surface, §5 selector
chain (stub for v0), and §3.3 registry shape.

## What's here (v0)

- **Three composition modes** — [`CompositionMode::InScenePaint`](src/composition.rs),
  `EmbeddedFrame`, `Overlay` — per brief §2. The vocabulary every renderer
  declares.
- **`NodeRenderer`** common trait + the three composition-mode sub-traits
  ([`InScenePaintRenderer`](src/renderer.rs), `EmbeddedFrameRenderer`,
  `OverlayRenderer`). Renderers run on the host UI thread; not `Send + Sync`.
- **`RendererRegistry`** — owns boxed renderers, indexes by content kind,
  resolves dispatch via a configurable [`RendererSelector`](src/registry.rs)
  policy. v0 ships [`DefaultSelector`](src/registry.rs) (first-candidate);
  brief §5's full chain (per-node pin → profile binding → host capability
  filter → default policy → last-resort) is the v1 selector and lands when
  per-node pins, profile binding, and host capabilities are real types in
  `host-runtime`.
- **Input vocabulary** — [`InputEvent`](src/input.rs) is the renderer-facing
  input contract: `Pointer` / `Key` / `Text` / `Ime` / `Focus`. Substrate-side
  producer (OS event → `InputEvent`) lives in `host-runtime::input`
  (placeholder there pending the actual OS event source).
- **Scene contract** — [`SceneNodeRef`](src/scene.rs) (borrowed view of a
  substrate node passed to renderers each frame), `NodeContentKind` (dispatch
  key), `Placement` / `LodLevel` / `NodeIdentity`.
- **Paint contract** — [`PaintCtx`](src/paint.rs) carries the host-owned
  `&mut vello::Scene` plus the node's transform; renderers `scene.append(...)`
  into it.
- **Handle types** — [`ProducerHandle`](src/handles.rs) /
  [`OverlayHandle`](src/handles.rs) for embedded-frame / overlay lifecycle.

## What's not here (intentionally)

- **Profile binding integration** — [`ProfileBindingExpectation`](src/renderer.rs)
  is the *capability declaration* a renderer makes; the actual
  `EngineProfileBinding` carried on a `SceneNodeRef` is left as `()` for v0.
  Wires up when [the engine-profile-store](../../design_docs/mere_docs/implementation_strategy/2026-05-14_engine_profile_boundary_plan.md)
  lands its substrate-side handle type.
- **Capability gates** — brief §6.2 says per-node-pin and profile-escalation
  fire `engine.route_override` / `engine.profile.escalate` gates through the
  action bus. v0 has no gate hook; the `DefaultSelector` ignores the would-be
  pin field. Adds when [the capability-gate catalogue](../../design_docs/mere_docs/research/2026-05-14_capability_gate_catalogue_brief.md)
  lands.
- **Diagnostics** — brief §6.3 names six diagnostic events the registry should
  emit (`renderer.registered` / `unregistered` / `hot_swapped`,
  `engine.route_chosen` / `degraded`, `surface.attach_failed`). v0 does not
  emit; instrumentation is a thin wrapper at registration / dispatch time.
- **Action-bus integration** — `InputDisposition::ConsumedWithEffect(Action)`
  from brief §3.2 is dropped for v0 in favour of `Consumed` / `Passthrough`
  binary; renderers route their own actions through their own sinks (e.g.
  `mere-masonry::TileSignal::Action`). Restore when the action bus's
  `ActionTarget` shape stabilises.
- **`Send + Sync` on `NodeRenderer`** — relaxed for v0 since masonry renderers
  use `Rc<RefCell<...>>` internally and substrate dispatch runs on the UI
  thread anyway. Restore later if a future renderer (e.g. headless workers
  per the [SessionServiceRunner plan](../../design_docs/mere_docs/implementation_strategy/2026-05-14_session_service_runner_plan.md))
  legitimately needs cross-thread dispatch.

## Build status

This crate compiles standalone (no path deps). It does *not* yet have a
mere-side consumer wired through cargo, so adding it to the workspace
`members` list is the obvious next step — done in the same diff that adds
this crate, see `mere/Cargo.toml`.

`vello` and `wgpu` are pinned to `0.5` and `26` respectively as v0
versions — verify against `netrender`'s pin before integrating; the contract
brief §10.6 (now folded into this crate's open questions) flagged version
coordination.
