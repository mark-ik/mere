# Mere Host — Architecture & Roadmap

**Date**: 2026-05-20
**Status**: Steering doc for `crates/mere/host/`. Living — update as phases land.
**Companion to**: [`../technical_architecture/2026-05-19_workspace_topology_status.md`](../technical_architecture/2026-05-19_workspace_topology_status.md) (workspace shape), [`../research/2026-05-15_spatial_chrome_ir_brief.md`](../research/2026-05-15_spatial_chrome_ir_brief.md) (substrate framing), [`../research/2026-05-15_renderer_registry_contract_brief.md`](../research/2026-05-15_renderer_registry_contract_brief.md) (renderer tenancy), [`2026-05-15_spatial_chrome_modular_adoption_plan.md`](2026-05-15_spatial_chrome_modular_adoption_plan.md) (adoption sequence).

---

## 1. North star

**The host is the conductor, not the orchestra.**

`crates/mere/host/` exists to do four things and no more:

1. Own the OS window + GPU surface (winit + wgpu + vello).
2. Run the frame loop (event pump → resync → render → present).
3. Route input (OS events → substrate hits → actions / drags).
4. Wire the modular crates together (substrate, renderer registry, cartography, session runtime, reactive panels).

Everything with weight lives in a crate the host *composes*, never reimplements:

| Concern | Owner |
|---|---|
| Graph truth | `kernel` |
| Layout algorithms | `graph-layout` / `cartography` |
| Composition (graph→scene, doc→packet) | `platen` |
| Render contracts + dispatch | `register-renderer`, `spatial-substrate` |
| Reactive UI | xilem / masonry (`mere-masonry`) |
| Domain concepts (workbench/orrery/gloss/apparatus) | `graphshell` / mere-domain |
| Session + manifest + view-intent | `session-runtime` (via `host-substrate`) |

**Success test**: you can add a renderer, a panel, a layout strategy, a content engine, or a second window *without editing the host's core loop*. The day you reach into `render.rs` / `runtime.rs` to add a feature is the day that feature is in the wrong place.

## 2. Current module map (2026-05-20)

```text
crates/mere/host/src/
├── main.rs                  — winit entry; event-loop control flow
├── runtime.rs               — App (ApplicationHandler) + RuntimeState; event dispatch, drag
├── setup.rs                 — boot: window/wgpu/vello, register renderers, seed, session bind, tokio rt
├── render.rs                — per-frame: substrate render_scene → vello → blit/present
├── cartography_projection.rs— project_orreries: Path A (painted) + Path B (exploded)
├── graph_node_explode.rs    — projection → per-node substrate entities + NodeOverrides + reverse lookup
├── graph_node_renderer.rs   — GraphNode InScenePaint + selection highlight
├── orrery_renderer.rs       — GraphView InScenePaint (Path A whole-projection paint)
├── splitter_renderer.rs     — splitter chrome between panes
├── diagnostics_panel.rs     — reactive xilem panel (live host metrics)
├── strategy_registry.rs     — projection_id → boxed LayoutStrategy
├── view_preset.rs           — ViewPreset → ViewIntent + ProjectionPath + strategy id
├── graph_registry.rs        — GraphId → Graph
└── seed.rs                  — startup seed graph / frame layout / docs
```

`host-substrate::HostApp` is the bridge the host drives: it owns the substrate scene, the renderer registry, the tile manager, the manifest store, the action bus, and the pane/splitter/tile identity maps.

## 3. Where we are

The frame/render/input spine works end-to-end:

- Substrate scene → registry dispatch → vello → present.
- Cartography **Path A** (orrery renderer paints a whole projection in one pane) and **Path B** (projection explodes into per-node `GraphNode` substrate entities).
- Reactive **xilem panels** (diagnostics) driven by a `MasonryRoot<State>` view rebuilt each frame from a shared snapshot.
- Per-node **selection + drag**, clamped to the origin pane.
- Session bind/create/restore; manifest flush on close.

This is a real milestone — the five-property substrate test (placement / embedded surfaces / typed relations / LOD / addressable identity) has a running artifact, validated on a real Vulkan device (panel renders, nodes drag).

It is still a **single-window, hardcoded-seed** host with one content kind that matters (the diagnostics panel) and no real navigation. Skeleton sound; muscles not yet attached.

## 4. The arc

Roughly ordered; each phase has a done-condition, not a date.

### Phase A — Harden the spine ✅ *(2026-05-20)*
- **Tame `RuntimeState`** into cohesive sub-structs (`WindowGpu` / `CartographyState` / `InteractionState`) so the host stays an orchestrator, not a god-struct. ✅
- **Gate the redraw loop** — `ControlFlow::Wait` + render-on-input; idle host issues no frames. ✅
- **Seed → session-restore boundary** explicit: session-first, restore-or-seed. The graph now persists to / restores from the session's `graph.json` (`session_graph_store`); the seed is reframed as the default-workspace content a fresh session opens with. Frame-layout + tile persistence remain Phase D. ✅
- Done: adding a renderer/panel touches only `setup.rs` registration; an idle host doesn't peg a core; a session round-trips its graph across restart.

### Phase B — Real chrome + input
- Per-pane content routing (workbench / apparatus / gloss / orrery), not diagnostics-everywhere — via `renderer_pin` or a content-kind-per-pane mapping.
- Toolbar / omnibar / command palette as xilem views over the `chrome` crate's view-models.
- Keyboard + focus routing through the substrate; IME (the macOS hardening caveat from the OS-plumbing audit).
- Action bus actually driving state (pane click → real focus, omnibar → navigate).
- Done when: the chrome is interactive and host-framework-agnostic.

### Phase C — Content engines in panes
- Wire `scrying.web` (system WebView, EmbeddedFrame) into a pane; then `nematic` smolweb.
- Done when: a real web/smolweb document renders in a workbench tile.

### Phase D — Navigation + session depth
- Click node → open/create address (per the node-identity + duplicates plan); graph traversal as navigation.
- Persist frame layout + view intents; multi-graph windows.
- Done when: a session round-trips across restart with layout + content intact.

### Phase E — Multi-window
- Drag-pane-out → new OS window with shared backing state (Firefox-style; original keeps context).
- Per-window `FrameLayout`; the single-window assumptions baked in today (`Option<RuntimeState>`, `HOST_FRAME_ID`/`HOST_PANE_ID` constants, one diagnostics handle) are what this phase unwinds.
- Done when: tearing a pane out spawns a synced second window.

### Phase F — Long tail
- Federation (`murm` / `moot`), intelligence (`embed`), personas (`identity`).

## 5. Sidequests (fun, mostly defer)

- **Streaming force-directed layout** — per-frame physics driving Path B nodes. Shows off the exploded model; the `StreamingLayoutStrategy` seam exists. Not load-bearing — do it when a view needs live physics, and it pairs naturally with the redraw-gate's "animation active" path.
- **Panel action routing** — widget action → `View::message` → state. `TileSignal::Action` already carries `(WidgetId, ErasedAction)`. Do it when chrome needs a real button.
- **Camera pan / zoom + LOD** on the substrate — needed for real spatial navigation, a rabbit hole if entered early.
- **Theme system** — resist until the chrome is real enough to theme.

## 6. Pitfalls

1. **`RuntimeState` god-struct** *(addressed this pass)* — it accumulated every subsystem handle. Grouped into `WindowGpu` / `CartographyState` / `InteractionState`; `host_app` + `frame_layout` + diagnostics stay top-level. Keep this discipline: new subsystems get a sub-struct, not another top-level field.
2. **Continuous-render tech debt** *(addressed this pass)* — `ControlFlow::Poll` pegged a core. Now event-driven (`Wait` + render-on-input). The frame counter means "frames rendered" — it climbs during interaction and holds when idle, which is correct browser behavior. When an animation lane lands (streaming layout), it re-arms the redraw each active frame.
3. **Whole-scene rebuild every frame** — `sync_scene_from_frame_layout` + re-explode each resync is O(n). Fine at 6 nodes, ruinous at hundreds. Needs incremental scene updates / dirty tracking before large graphs.
4. **`host-substrate::HostApp` is a second god-struct** — scene + registry + tiles + manifests + action_bus + identity maps. Same discipline; watch its width.
5. **Per-panel `Arc<Mutex>` snapshot plumbing** — the diagnostics panel reads a shared snapshot. That fights the xilem grain; the real model is the host owns app state and panels are views over it. Proliferating one mutex-snapshot per panel is a trap as chrome grows.
6. **EmbeddedFrame texture per panel** — each panel is a full offscreen wgpu texture rendered every frame. The cheaper InScenePaint path (masonry → vello scene directly; the scene-merge TODO in `mere-masonry/src/tile.rs`) exists but isn't wired. Don't scale all chrome through textures.
7. **Graph-node hits routed via `UnknownTileHit`** — the host resolves graph-node clicks off a degraded catch-all variant. As more node kinds become interactive, hit-resolution should go through the registry's `input()` dispatch (renderer `accepts_input`), not host-side string-matching.
8. **`repos/imaging` fork coupling** — the masonry path rides a local wgpu-29 / vello-0.9 fork. Upstream drift is a standing maintenance cost; revisit when upstream xilem bumps to match.

## 7. The one rule

When in doubt about whether something belongs in the host: **if it could be unit-tested without a window, it belongs in a crate.** The host's own logic should be the irreducible glue that genuinely needs the OS, the GPU, or the event loop.
