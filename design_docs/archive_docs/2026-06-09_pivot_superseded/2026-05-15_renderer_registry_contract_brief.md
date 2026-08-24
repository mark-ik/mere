# Renderer Registry Contract — research brief

**Date**: 2026-05-15
**Status**: Research brief (defines a contract; does not yet propose adoption)
**Scope**: Defines `NodeRenderer` — the trait + registry shape that lets genet / scrying / wry / platen / vello-direct / parley / cartography coexist as **co-resident renderers of node content kinds**, dispatched per `NodeContent` variant under one host-agnostic contract. Independent of whether the [spatial chrome IR](2026-05-15_spatial_chrome_ir_brief.md) substrate-as-host ever ships — useful even under the current gpui host because it normalises how renderers are described in every adjacent doc and gives the inker `Engine` / `SurfaceEngine` / `SurfaceProducer` triad a single roof.

**Related**:

- [`2026-05-15_spatial_chrome_ir_brief.md`](2026-05-15_spatial_chrome_ir_brief.md) — parent framing. §4 sketches the registry shape; this brief makes it concrete.
- [`2026-05-11_engine_peers_and_scrying_library_brief.md`](2026-05-11_engine_peers_and_scrying_library_brief.md) — engines as content-production functions; pins genet / scrying / wry composition models. The `NodeRenderer` trait defined here is the abstraction those three implement.
- [`2026-05-11_scrying_web_tile_plan.md`](../implementation_strategy/2026-05-11_scrying_web_tile_plan.md) — currently introduces `SurfaceEngine` + `SurfaceProducer` traits in inker, parallel to the existing `Engine` trait. This brief reframes both as concrete *composition modes* under one `NodeRenderer` umbrella, without churning the inker code (§9).
- [`2026-05-09_netrender_for_engine_documents_brief.md`](2026-05-09_netrender_for_engine_documents_brief.md) — vello vs netrender stacking; informs the in-scene vs embedded-frame composition split.
- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md) — §5.4 (engine profile binding), §7 (capability gates), §8 (diagnostic events). The renderer registry composes with all three; this brief specifies the seams.
- [`2026-05-10_cartography_layer_brief.md`](2026-05-10_cartography_layer_brief.md) — `LayoutStrategy` / `Projection` is what one specific renderer (the graph-view renderer) consumes. Cartography is upstream of the registry, not part of it.
- Memory: `project_browser_pwa_shapes_scripting`, `project_host_framework_glass_gpui`, `project_mere_domain_layer`, `feedback_spec_code_samples_illustrative_vs_implementation_ready`, `feedback_consumer_pull_gates_check_first`.

---

## Thesis

> **There are three composition modes (in-scene paint, embedded-frame, overlay), and every Mere renderer fits into exactly one. The registry dispatches per node content kind to a renderer of the appropriate mode; the host owns the per-mode composition path. The trait surface is the same under gpui-via-PlatformSurface (today) and under a substrate-as-host (future); only the host's composition wiring changes.**

Three concrete consequences:

1. The current `Engine` / `SurfaceEngine` / `SurfaceProducer` triad in inker is reframed as **two of the three composition modes** (document-engine produces in-scene paintable scenes via platen; SurfaceEngine produces embedded-frame textures). No code churn — only the description normalises.
2. Adding wry as `wry.web` becomes the third composition mode (overlay) without expanding the registry's surface area.
3. Future renderers — graph-canvas, panel chrome, edge renderer, knot renderer, future video/3D engines — are described in the same vocabulary instead of growing per-renderer ad-hoc integration code in the host.

This brief decides the contract shape. It does not decide adoption beyond §11.

---

## 1. Why a registry, even under the current host

The case for the registry doesn't depend on the substrate. Today, under gpui:

- The host has bespoke per-renderer wiring. mere-host renders document tiles via gpui-shaped layout; scrying tiles via `SurfaceProducer`; genet (in the planned netrender path) via netrender's wgpu integration; future wry tiles via overlay composition. Each path is described differently in each plan, with no shared vocabulary.
- The [scrying-web plan](../implementation_strategy/2026-05-11_scrying_web_tile_plan.md) already had to introduce a parallel trait (`SurfaceEngine`/`SurfaceProducer`) because the existing `Engine` trait couldn't express long-lived producers + frame streams + textures. Without a registry framing, every new composition shape will spawn a similar parallel trait.
- Capability gates ([multiplexer framing §7](2026-05-11_browser_multiplexer_framing.md)) and diagnostics ([§8](2026-05-11_browser_multiplexer_framing.md)) need a uniform place to attach. Today they attach per-renderer because there's no single seam.

The registry pays for itself by giving inker, mere-host, and the platen/genet/scrying/wry stack one vocabulary. Under the current gpui host the `register` / `dispatch` / `compose` lifecycle still applies — the only thing that changes under substrate-as-host is *what the host does with the renderer's output* (paint into the substrate's vello scene vs hand to gpui).

## 2. Three composition modes

Every renderer Mere has, plans, or might add fits into one of three modes. The mode is a property of the renderer; node content kinds may have multiple registered renderers (different modes or different implementations of the same mode — see §5).

### 2.1 In-scene paint

The renderer paints scene operations into the chrome's vello scene during the host's paint pass. The host composites the result with everything else painted that frame.

- **Composition cost**: cheap (no texture handoff; one vello scene).
- **Frame coupling**: paint-time-bound (renderer runs during the host's paint cycle).
- **Cross-renderer effects**: in-scene blurs, clips, blends, filters all work uniformly.
- **Examples**: graph canvas, edge renderer, panel chrome, knot/document tile rendering via platen, future custom canvases.

### 2.2 Embedded-frame

The renderer produces an independent wgpu texture / surface (or stream of textures) on its own schedule. The host composites that texture into the chrome at paint time as an external texture.

- **Composition cost**: medium (texture handoff, fence sync, possibly cross-queue synchronisation).
- **Frame coupling**: independent frame rate (renderer can be 60fps while host is 30fps or vice versa; host samples the latest texture).
- **Cross-renderer effects**: limited — the host can blend/clip the output rectangle as a unit, but in-scene effects don't reach inside the texture.
- **Examples**: genet (rendering pages into its own netrender Scene → wgpu texture), scrying (system WebView frame stream → wgpu texture), future video decoders, future 3D scene renderers.

### 2.3 Overlay

The renderer renders into an out-of-band OS surface (a separate window, OS compositor layer, or platform-specific overlay) positioned over the chrome. The host does not composite the output — the OS does.

- **Composition cost**: zero (OS handles it).
- **Frame coupling**: fully independent.
- **Cross-renderer effects**: none — the overlay is opaque to in-scene effects, cannot be clipped by chrome shapes, cannot participate in chrome blends.
- **Examples**: wry (OS WebView in its own window/layer per [engine-peers brief](2026-05-11_engine_peers_and_scrying_library_brief.md)), system-popup menus, future native video overlays where embedded-frame is too expensive.

### 2.4 Why three is enough

These three modes correspond to the three ways a 2D/3D renderer can hand pixels to a hosting compositor:

- **In-scene** = "I produce scene ops; you call me during your paint pass."
- **Embedded-frame** = "I produce a texture on my own clock; you sample it during your paint pass."
- **Overlay** = "I produce pixels into my own OS surface; you tell me where to be."

There are subtler variations (e.g., shared-memory framebuffer for cross-process renderers, GPU-direct-scanout for low-latency cases) but they are *transports* of one of the three modes, not new modes. The registry's surface stays at three.

## 3. The `NodeRenderer` trait surface

Illustrative-signature-only sketch — types renamed for clarity, error / lifetime / async details elided. Not implementation-ready.

### 3.1 Common surface

```text
trait NodeRenderer: Send + Sync {
    fn renderer_id(&self) -> RendererId;
    fn handles(&self) -> NodeContentKindSet;
    fn composition_mode(&self) -> CompositionMode;
    fn capabilities(&self) -> RendererCapabilities;
    fn lifecycle(&self) -> &dyn RendererLifecycle;
}

enum CompositionMode {
    InScenePaint,
    EmbeddedFrame,
    Overlay,
}

struct RendererCapabilities {
    accepts_input:        bool,
    handles_ime:          bool,
    handles_a11y:         bool,
    scrollable:           bool,
    hit_testable_subregions: bool,  // can the renderer report sub-regions for fine-grained hit-testing
    profile_binding:      ProfileBindingExpectation, // see §6
    supports_capture:     bool,     // can produce a snapshot for switcher thumbnails / accessibility preview
}
```

### 3.2 Mode-specific sub-traits

Renderers implement one of these in addition to `NodeRenderer`:

```text
trait InScenePaintRenderer: NodeRenderer {
    fn paint(&self, node: &SceneNode, ctx: &mut PaintCtx) -> PaintResult;
    fn input(&self, node: &SceneNode, event: &InputEvent) -> InputDisposition;
}

trait EmbeddedFrameRenderer: NodeRenderer {
    fn ensure_producer(&self, node: &SceneNode) -> ProducerHandle;
    fn next_frame(&self, handle: &ProducerHandle) -> Option<FrameTexture>;
    fn deliver_input(&self, handle: &ProducerHandle, event: &InputEvent) -> InputDisposition;
    fn release(&self, handle: ProducerHandle);
}

trait OverlayRenderer: NodeRenderer {
    fn ensure_overlay(&self, node: &SceneNode) -> OverlayHandle;
    fn position(&self, handle: &OverlayHandle, rect: ScreenRect, z: i32);
    fn deliver_input(&self, handle: &OverlayHandle, event: &InputEvent) -> InputDisposition;
    fn release(&self, handle: OverlayHandle);
}
```

The `InputDisposition` return is `{ Consumed, Passthrough, ConsumedWithEffect(Action) }` — renderers either eat the event, pass it back, or eat-and-emit a typed action that the action bus handles.

### 3.3 The registry

```text
struct RendererRegistry {
    renderers: HashMap<RendererId, Box<dyn NodeRenderer>>,
    by_kind:   HashMap<NodeContentKind, Vec<RendererId>>,
    selector:  Box<dyn RendererSelector>,
}

trait RendererSelector: Send + Sync {
    fn select(
        &self,
        kind: NodeContentKind,
        candidates: &[RendererId],
        node: &SceneNode,
        host_caps: &HostCapabilities,
    ) -> Option<RendererId>;
}
```

Selection is policy-controlled, not first-match — see §5 for resolution rules.

## 4. Lifecycle

Five lifecycle moments the trait surface has to support. None of them are mode-specific.

### 4.1 Registration

Renderers register at host bootstrap. Registration declares `(renderer_id, handles, composition_mode, capabilities)`. The registry indexes by content kind for fast dispatch.

Hot-registration (registering a renderer at runtime — e.g., a user-installed engine) is supported by the registry's mutability, but **must route through the action bus + capability gate** ([multiplexer framing §7](2026-05-11_browser_multiplexer_framing.md)) so the host can policy-check before binding new content kinds.

### 4.2 Init per node

When a `SceneNode` first needs rendering, the registry resolves a `RendererId` (§5) and the renderer initialises its node-local state. For `InScenePaint`, this is typically a no-op (paint is stateless from the renderer's perspective; vello scene state is supplied per-call). For `EmbeddedFrame` and `Overlay`, this is `ensure_producer` / `ensure_overlay` — non-trivial, often async, often spawning a background producer.

### 4.3 Frame

- In-scene paint: host calls `paint(node, ctx)` during its paint cycle.
- Embedded-frame: host calls `next_frame(handle)`; if `Some(texture)`, composes; if `None`, reuses last frame.
- Overlay: host calls `position(handle, rect, z)` if rect changed; otherwise no-op.

### 4.4 Hot-swap

Per [engine-peers brief](2026-05-11_engine_peers_and_scrying_library_brief.md), the user (or auto-fallback) can switch a tile from `genet.web` to `scrying.web` to `wry.web`. This is the registry's `select` re-running for the same node content with a different `engine_id` constraint.

The trait surface supports hot-swap natively: the old renderer's `release` is called; the new renderer's `ensure_*` runs; cookies/session state continuity is the concern of `EngineProfileBinding` (§6), not the renderer registry — the new renderer reads the same UDF the old one wrote to.

### 4.5 Teardown

`release(handle)` is called when a node is removed from the scene, when the renderer is unregistered, or when a session is killed. Producers / overlays MUST stop their work and release GPU resources synchronously enough that a session-kill produces no orphans.

## 5. Resolution rules — multi-renderer per content kind

The interesting case: `NodeContent::WebPage(profile, url)` has *three* registered renderers (genet, scrying, wry). The selector picks one. Resolution chain (first match wins):

1. **Per-node pin**. The `SceneNode` carries an optional `RendererId` override. Per [engine-peers brief](2026-05-11_engine_peers_and_scrying_library_brief.md), the user can pin a tile's engine — that pin lives on the node and dominates everything else.
2. **Profile-binding constraint**. The `EngineProfileBinding` (§6) may declare an engine the profile is bound to (e.g., a UDF written by scrying isn't readable by wry, and vice versa). If the profile is bound, the binding constrains the candidate set to compatible renderers.
3. **Host capability filter**. Renderers requiring host capabilities the host can't provide (e.g., wry on a host without OS WebView; embedded-frame requiring external-texture support that the gpui host doesn't yet provide on macOS) drop out of the candidate set. Filtered renderers emit a `engine.route_degraded` diagnostic ([§8](2026-05-11_browser_multiplexer_framing.md)) so the user knows why their preferred engine wasn't chosen.
4. **Default policy**. Per [engine-peers brief](2026-05-11_engine_peers_and_scrying_library_brief.md), `genet.web` is the preferred web renderer. If genet succeeded, it's chosen. If genet reported failure, `scrying.web` is offered (the user accepts via a tile-engine switch). Auto-fallback rule is a follow-up.
5. **Last resort**. If nothing matches, the registry returns `None` and the host paints a "no renderer available for this content" placeholder with a diagnostic. This should be vanishingly rare — it indicates a registration bug or a missing default-coverage renderer.

The selector is a trait (§3.3), so the resolution policy is **configurable per `feedback_configurability_over_opinionated_defaults`** rather than hardcoded. v0 ships a `DefaultSelector` implementing the chain above; later policies (per-persona engine preferences, per-graph engine policies, content-type-aware fallbacks) plug in without touching the registry's core.

## 6. Composition with multiplexer brief seams

Three places the registry composes with already-decided multiplexer concerns. The registry doesn't *own* any of these — it threads them through.

### 6.1 Engine profile binding (multiplexer §5.4)

`SceneNode::content` carries an `EngineProfileBinding` for any node whose renderer needs persistent engine state (cookies, permissions, cache, IndexedDB, localStorage). The renderer reads the binding at `ensure_producer` / `ensure_overlay` time and resolves the actual UDF path via the [`engine_profile_store` module](../implementation_strategy/2026-05-14_engine_profile_boundary_plan.md). The renderer registry never touches profile bytes — it just makes sure the right binding reaches the right renderer.

`RendererCapabilities::profile_binding` (§3.1) declares which scopes a renderer accepts (e.g., scrying might accept all three; nematic engines might accept `None` since they have no persistent state). The selector (§5.2) uses this to filter compatible renderers when a profile is bound.

### 6.2 Capability gates (multiplexer §7)

The cross-renderer operations the [capability-gate-catalogue brief](2026-05-14_capability_gate_catalogue_brief.md) enumerates compose with the registry at three points:

- `engine.route_override` fires when a per-node pin (§5.1) overrides the default selector. Gate decision: `RequireConsent` per the catalogue.
- `engine.profile.escalate` fires when the profile binding for a node changes (e.g., persona-scoped → session-scoped). Gate decision: `RequireConsent`.
- `attach.cross_session` is unrelated to the registry — it's the multiplexer's own concern.

Gate denials emit `permission.denied` and the registry leaves the previously-selected renderer in place (no silent fallback to a different engine).

### 6.3 Diagnostics (multiplexer §8)

The registry emits, at minimum:

```
engine.route_chosen       { graph_id, address, renderer_id, mode }
engine.route_degraded     { graph_id, address, attempted: [renderer_id], reason }
surface.attach_failed     { pane_id, surface_id, renderer_id, error }
renderer.registered       { renderer_id, handles, mode }
renderer.unregistered     { renderer_id }
renderer.hot_swapped      { node_id, from: renderer_id, to: renderer_id, reason }
```

The first three already appear in the multiplexer's diagnostic vocabulary; the last three are the registry's contribution. All route through the apparatus diagnostics buffer — the existing infrastructure covers them.

## 7. Per-renderer mapping

Concrete registrations under v0 of the contract. *Status* column reflects 2026-05-15 reality (per relevant briefs / plans).

| Renderer            | Composition mode    | Handles content kinds                                | Profile binding scope        | Status                                                    |
| ------------------- | ------------------- | ---------------------------------------------------- | ---------------------------- | --------------------------------------------------------- |
| `genet.web`        | EmbeddedFrame       | `WebPage(*)`                                         | Persona / Session / Graph    | netrender mainline shipped 2026-05-04                     |
| `scrying.web`       | EmbeddedFrame       | `WebPage(*)`                                         | Persona / Session / Graph    | per [scrying-web plan](../implementation_strategy/2026-05-11_scrying_web_tile_plan.md), Windows-first |
| `wry.web`           | Overlay             | `WebPage(*)`                                         | Persona / Session            | per [engine-peers brief](2026-05-11_engine_peers_and_scrying_library_brief.md), composition-different from scrying |
| `nematic.markdown`  | InScenePaint*       | `DocumentTile(EngineDocument{markdown})`             | None                         | shipped; renders via platen → netrender → vello           |
| `nematic.gemtext`   | InScenePaint*       | `DocumentTile(EngineDocument{gemtext})`              | None                         | shipped; same path                                        |
| `nematic.gopher`    | InScenePaint*       | `DocumentTile(EngineDocument{gopher})`               | None                         | shipped; same path                                        |
| `nematic.feed`      | InScenePaint*       | `DocumentTile(EngineDocument{feed})`                 | None                         | shipped; same path                                        |
| `nematic.knot`      | InScenePaint*       | `Knot(EngramHandle)`, `DocumentTile(.knot)`          | None                         | per polyglot-knot design (`genet/design_docs/nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md`) |
| `nematic.{others}`  | InScenePaint*       | `DocumentTile(EngineDocument{<protocol>})`           | None                         | shipped (text/file/finger/scroll/misfin/nex/guppy)        |
| `cartography.graph` | InScenePaint        | `GraphView(GraphId, ViewIntent)`                     | None                         | per [cartography brief](2026-05-10_cartography_layer_brief.md), strategies in flight |
| `mere-domain.panel` | InScenePaint        | `Panel(PanelKind, ViewIntent)`                       | None                         | needs reactive runtime — biggest new piece                |
| `chrome.edge`       | InScenePaint        | `(SceneEdge, EdgeRendering)`                         | None                         | small; isolated; vello + parley                           |
| `vello.canvas`      | InScenePaint        | `CustomCanvas(CanvasHandle)`                         | None                         | already the graph canvas's pattern                        |

\* The nematic engines themselves don't paint — they produce `EngineDocument` values that `platen` lays out and `netrender` converts to a vello scene that the host composites. Functionally InScenePaint via the platen → netrender pipeline; the engine isn't the renderer in the trait sense, the *(engine, platen, netrender)* triple is. See §9 for the framing.

`parley` doesn't appear in the table — it's a layout helper used *by* other renderers (cartography for node labels, chrome.edge for edge labels, platen for document text). Not a `NodeRenderer` itself.

## 8. The host's per-mode composition path

For each composition mode the host implements one composition path. The renderer registry stays mode-agnostic above this; the host's GPU layer absorbs the per-mode wiring.

### 8.1 In-scene paint composition

Under gpui (today): the host paints a vello scene per frame; in-scene-paint renderers contribute scene ops; gpui composites the resulting pixels via PlatformSurface.

Under substrate-as-host (future): the host paints directly into the substrate's vello scene; in-scene-paint renderers are called during the substrate's paint pass.

**Renderer contract is identical in both cases.** Only the host's "what do I do with the vello scene at end of paint" changes.

### 8.2 Embedded-frame composition

Under gpui (today): per the [scrying-web plan](../implementation_strategy/2026-05-11_scrying_web_tile_plan.md), embedded-frame textures are composed via gpui's external-texture mechanism (CompositionController on Windows; equivalent paths for macOS / Linux as scrying lands there). Fence sync handled per the plan's slice (3) (`SurfaceTileState`).

Under substrate-as-host (future): the substrate's renderer composites the external texture as a quad in its own vello scene with the right transform. Possibly cheaper (one less composition layer) but the path the texture takes from renderer to screen is essentially the same.

**Renderer contract is identical in both cases.** The host's "how do I get this wgpu texture onto the screen at this rect" changes; the renderer just produces the texture.

### 8.3 Overlay composition

Under gpui (today): wry overlays go into their own OS surfaces; the host tells wry where to be via `position`; gpui's window manager doesn't touch the overlay's pixels.

Under substrate-as-host (future): same.

**Overlay composition is host-mode-invariant by definition** — the OS does the composition, not Mere.

## 9. Relationship to existing inker traits

The current state of inker:

```
trait Engine {
    fn engine_id(&self) -> &str;
    fn render(&self, input: &EngineInput) -> Result<EngineDocument, EngineError>;
}

trait SurfaceEngine {                  // per scrying-web plan slice (1)
    fn engine_id(&self) -> &str;
    fn create_producer(&self, ...) -> Box<dyn SurfaceProducer>;
}

trait SurfaceProducer {                // per scrying-web plan slice (1)
    fn next_frame(&self) -> Option<FrameTexture>;
    // input, navigate, etc.
}
```

The renderer registry `NodeRenderer` framing **does not require renaming, deprecating, or churning these traits**. The mapping:

- Each `Engine` implementation becomes the *content production* half of an in-scene-paint renderer; the *paint* half is `(platen + netrender)`. There is one canonical "document tile renderer" — a thin adapter that takes an `Engine`'s `EngineDocument` output, hands it to platen, and returns the resulting netrender Scene. That adapter implements `InScenePaintRenderer` once and serves all 13 nematic engines uniformly.
- Each `SurfaceEngine` + `SurfaceProducer` pair maps directly to `EmbeddedFrameRenderer::ensure_producer` + `next_frame` + `release`. Mechanical translation; no semantic change. The `SurfaceEngine` / `SurfaceProducer` traits become *internal implementation types* of the embedded-frame renderer adapter, not exposed at the registry surface.
- A future overlay renderer (`wry.web`) implements `OverlayRenderer` directly. There's no inker analogue to rename; this is new code.

This is the case for the registry being **additive over inker, not a rewrite**. The renderer registry is a host-side seam; inker stays the engine-controller it already is, and grows a small adapter layer that exposes its engines through the registry surface.

The renaming concern (per [feedback_consumer_pull_gates_check_first](<user-home>/.claude/projects/c--Users-mark--Code/memory/feedback_consumer_pull_gates_check_first.md)): we're the only consumer of the inker trait surface today, so even a more invasive rename would be cheap — but additive is cheaper, and additive doesn't preclude later consolidation.

## 10. Open questions

### 10.1 InScenePaint vs EmbeddedFrame for genet

Genet today is described as embedded-frame (it produces its own netrender Scene → wgpu texture). But genet's *output* is a netrender Scene, which could in principle be merged into the host's vello scene directly (in-scene-paint mode) instead of rasterised to a texture and re-composited.

In-scene merge would eliminate a render-target round-trip but couples genet's frame rate to the host's paint cycle and forces single-threaded paint coordination. Embedded-frame keeps genet independently driven.

Probable answer: **embedded-frame** for genet as the default; in-scene-paint as an opt-in optimisation when genet and the host can be coordinated on a single GPU queue. Defer until measured. Tracked here so the registry shape doesn't preclude either choice.

### 10.2 Multi-mode renderer (same renderer, different modes per node)

Could a renderer implement multiple modes — e.g., a video renderer that does embedded-frame for full panes and in-scene-paint for thumbnails? Two answers:

- *Yes*, by registering two `RendererId`s sharing internal state, each implementing the appropriate sub-trait. Cheap.
- *No*, by picking the best single mode for the renderer and accepting some overhead at non-canonical sizes.

Lean *yes* (registry supports it implicitly via `RendererId` per-mode) but flag that no concrete renderer demands it yet — premature to bake multi-mode into the trait.

### 10.3 Cross-renderer effects across modes

In-scene effects (blur, clip, blend) can't reach inside an embedded-frame texture's pixels (you can blur the rect, not the page being painted). For some cases (workbench-style frosted-glass overlays over a genet page) this is a legitimate limitation. Worked-around possibilities: (a) pre-render the embedded frame to a texture the host can sample for blur; (b) push effect chains into the embedded-frame renderer (it does its own blur). Both are heavier than in-scene blur.

Not a registry concern per se — flag for the chrome-effects design when it arises.

### 10.4 Input ownership for overlay renderers

Overlay surfaces have their own OS-level input routing — wry's WebView eats clicks and key events directly. The registry's `deliver_input` for `OverlayRenderer` is a *fallback path* for events the OS routes back to Mere (focus changes, IME composition begin/end, drag-drop boundary crossings). Most input never goes through the registry for overlay renderers.

This breaks the spatial input router model: hit-testing through the substrate doesn't reach overlay content. Not necessarily wrong — it's how every browser treats native widgets — but worth flagging because *spatial relations* between overlay and chrome (a chrome edge connecting to a node inside a wry-rendered page) are unrenderable.

Probable resolution: overlay renderers carry an "overlay opacity" cost; relations terminating inside an overlay-rendered surface anchor at the overlay's bounding rect, not its inner content. Defer until wry adoption; flag.

### 10.5 Capture for switcher thumbnails

`RendererCapabilities::supports_capture` lets the registry produce a snapshot for the switcher's thumbnail render ([multiplexer §5.5](2026-05-11_browser_multiplexer_framing.md)). Trivially true for in-scene-paint renderers (re-run paint at a smaller scale into a thumbnail vello scene) and embedded-frame renderers (sample the most recent texture). Hard for overlay renderers — the OS surface isn't readback-friendly, especially on macOS / Wayland.

For overlay renderers, capture probably needs OS-level screen-capture APIs (with associated permission gates) or a fallback "this session has overlay content; rendering placeholder thumbnail" branch. Defer; flag.

### 10.6 Async ergonomics

Lifecycle methods on `EmbeddedFrameRenderer` and `OverlayRenderer` (`ensure_producer`, `ensure_overlay`) are naturally async (they spawn producers, wait for first frame, etc.). The trait sketch in §3 hides this — production trait would need either async-trait, returning futures, or a sync facade with internal task spawning. Lean on the [`SessionServiceRunner`](../implementation_strategy/2026-05-14_session_service_runner_plan.md) precedent: sync facade returning a handle, internal async owned by the renderer.

## 11. Decisions and non-decisions

**Decides:**

1. **Three composition modes are the v0 vocabulary.** In-scene paint, embedded-frame, overlay. Future composition shapes are framed as transports of these three, not new modes.
2. **Renderers register against `NodeContentKind`s, not against URLs / addresses / engine IDs.** The kind is the dispatch key; URL routing is upstream.
3. **Multi-renderer per kind is supported and resolved by a configurable `RendererSelector` policy.** Default policy chain in §5; user-overrideable per `feedback_configurability_over_opinionated_defaults`.
4. **The registry threads engine profile binding, capability gates, and diagnostics through to renderers but owns none of them.** Same multiplexer seams, one consumer surface.
5. **Inker's existing `Engine` / `SurfaceEngine` / `SurfaceProducer` traits stay.** Adapters expose them through the registry. No churn.
6. **Trait surface is host-agnostic.** Identical under gpui-via-PlatformSurface and substrate-as-host; only the host's per-mode composition path changes.
7. **`InputDisposition` is the input-dispatch return type.** Renderers can consume, pass through, or consume-and-emit a typed action that the action bus handles.

**Does not decide:**

- Whether to ship the registry. This brief defines the contract; adoption is a separate plan.
- Specific renderer-selector default policies beyond §5's chain.
- The cross-renderer-effects question (§10.3).
- The overlay-input-routing model beyond §10.4's framing.
- Async trait shape (§10.6).

## 12. Done conditions for v0

If/when the registry ships, v0 is done when:

1. `RendererRegistry` + `NodeRenderer` + the three sub-traits + `RendererSelector` exist as Rust types in a `mere-renderer-registry` crate (or comparable home; possibly `mere-host-runtime`).
2. The document-tile adapter wraps inker's existing 13 `Engine` implementations as a single `InScenePaintRenderer` and routes through platen + netrender to vello.
3. The scrying surface adapter wraps `SurfaceEngine` + `SurfaceProducer` from the [scrying-web plan](../implementation_strategy/2026-05-11_scrying_web_tile_plan.md) as an `EmbeddedFrameRenderer`.
4. `cartography.graph` + `chrome.edge` + `vello.canvas` are registered as in-scene-paint renderers (even if their content kinds aren't all wired in mere-domain yet).
5. The host's tile dispatch routes through `RendererRegistry::resolve_and_dispatch` instead of bespoke per-renderer match arms.
6. Diagnostics events from §6.3 emit through the apparatus buffer.
7. Capability gates from §6.2 fire on per-node pin and profile escalation.

Adoption order: (1) registry + adapters first (no behaviour change for users); (2) host dispatch routing (small refactor); (3) wry / future renderers register naturally afterwards.

## 13. What this brief does and does not decide

**Decides:** the renderer registry contract. Three modes, trait surface, registry shape, resolution policy, lifecycle, multiplexer seam composition, per-renderer mapping, relationship to existing inker traits, v0 done conditions.

**Does not decide:** adoption schedule (left to a future plan), registry crate location (`mere-renderer-registry` is a candidate name; could equally live in `mere-host-runtime`), default-selector specific policies beyond §5's chain.

**Implies follow-ups:**

- *Renderer registry adoption plan* — filed as Phase 2 of [`../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md). It keeps adoption under the current gpui host first; no substrate-as-host dependency.
- *Cross-renderer effects design* — addresses §10.3.
- *Overlay input routing model* — addresses §10.4 alongside wry adoption.
- *Capture for switcher thumbnails refinement* — addresses §10.5.
