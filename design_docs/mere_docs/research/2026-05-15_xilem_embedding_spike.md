# Xilem Embedding Spike — research note

**Date**: 2026-05-15
**Status**: Read-only research note (clone + architectural read; no code written; no embedding actually attempted yet)
**Scope**: Investigate whether Linebender's xilem + masonry stack can be embedded into a foreign-owned host (Mere's spatial chrome IR substrate) — specifically, whether xilem's reactive runtime and masonry's widget engine can render *into a vello scene the substrate owns*, with input routed by Mere's spatial input router and accessibility flowing into Mere's uxtree, *without* xilem owning the window or the wgpu device.

**Related**:

- [`2026-05-15_spatial_chrome_ir_brief.md`](2026-05-15_spatial_chrome_ir_brief.md) — parent. §10.2 ("Reactive runtime — own vs. borrow") flagged this exact question. §11 commits the substrate-as-host preconditions.
- [`2026-05-15_renderer_registry_contract_brief.md`](2026-05-15_renderer_registry_contract_brief.md) — sibling. xilem (if embeddable) becomes the implementation of the `mere-domain.panel` in-scene-paint renderer in §7's per-renderer mapping table.
- [`2026-05-15_os_plumbing_reuse_audit_brief.md`](2026-05-15_os_plumbing_reuse_audit_brief.md) — sibling. xilem brings its own answers to several subsystems (cursors via `cursor-icon`, IME area routing via `RenderRootSignal`, AccessKit via `accesskit::TreeUpdate`); embedding posture changes which subsystems Mere implements vs. inherits.
- Memory: `user_testing_hardware` (Windows + iMac + Fedora 44 + Mint Acer all locally validatable as of today; supersedes prior `user_linux_hardware` framing).
- Upstream: [linebender/xilem](https://github.com/linebender/xilem) (cloned shallow at `c:/Users/mark_/Code/repos/xilem/`).

---

## TL;DR

> **Embedding is not just possible — Linebender designed for it explicitly.** The driver pattern is the canonical extension point. Two drivers already exist (`masonry_winit`, `masonry_android_view`); the masonry_core README explicitly names "Writing an alternative driver for Masonry (alike to Masonry Winit)" as a supported extension scenario. masonry_core depends on `imaging` (an abstract scene format), not on vello or wgpu. xilem_core depends on nothing rendering-related at all. The path for Mere is: **use xilem_core + masonry_core directly, write a `mere_masonry` driver that translates masonry's `imaging::record::Scene` into the substrate's vello scene at the right `Placement` transform, and route input via the existing `ui-events` vocabulary.**

This dramatically de-risks the substrate-as-host conversation. Linebender did the layering work that gpui never did.

Five concrete findings below.

---

## 1. What I did

Cloned `linebender/xilem` shallow into `c:/Users/mark_/Code/repos/xilem/`. Read:

- `ARCHITECTURE.md` (top-level repo structure + Mermaid dep graph)
- `README.md`
- `xilem_core/Cargo.toml` (deps; renderer-agnosticism check)
- `masonry_core/Cargo.toml` (deps; renderer-agnosticism check)
- `masonry_core/README.md` (driver-extension language)
- `masonry_core/src/lib.rs` (top-level module structure)
- `masonry_core/src/app/render_root.rs` first 150 lines (the composition root)
- Grepped `RenderRootSignal` to characterise driver-facing signal vocabulary
- `masonry_imaging/src/` (multi-backend imaging layer)
- `masonry_winit/Cargo.toml` (canonical reference driver)
- `masonry_winit/src/app_driver.rs` (the `AppDriver` trait — what application code provides)
- `masonry_winit/src/event_loop_runner.rs` first 80 lines (how winit binds in)
- `masonry_winit/src/vello_util.rs` (full file — the wgpu/vello binding)

No code written; no compilation attempted. This is the read-the-architecture pass; an actual embedding implementation is the natural next step (and is small — see §6).

## 2. Five concrete findings

### 2.1 xilem_core has zero rendering dependencies

`xilem_core/Cargo.toml`:

```toml
[dependencies]
tracing.workspace = true
kurbo = { optional = true, workspace = true }   # optional; geometry only
hashbrown = { workspace = true }
anymore = { workspace = true }
```

That's it. No vello. No wgpu. No winit. No masonry. Pure reactive view-tree machinery. xilem_web uses xilem_core with a DOM backend; xilem_masonry uses it with masonry. **Mere can use xilem_core with a Mere-substrate backend** — this is exactly the same shape as the existing two consumers.

This is the gpui-Entity<T>+cx-equivalent layer, fully decoupled from rendering. Borrow it directly.

### 2.2 masonry_core depends on `imaging`, not on vello or wgpu

`masonry_core/Cargo.toml` deps (rendering-relevant only):

```toml
imaging.workspace = true              # abstract scene format
peniko.workspace = true               # paint vocabulary (brushes, colors, gradients)
parley.workspace = true               # text layout
kurbo.workspace = true                # 2D geometry
accesskit.workspace = true            # accessibility tree
ui-events.workspace = true            # input event vocabulary
cursor-icon = "1.2.0"
dpi.workspace = true
tree_arena.workspace = true           # the widget arena
```

No vello. No wgpu. No winit. Confirmed by [masonry_core/src/lib.rs:82-86](repos/xilem/masonry_core/src/lib.rs#L82-L86) which re-exports `imaging`, `peniko`, `kurbo`, `accesskit`, `parley`, `ui_events`, `dpi`, `anymore` — the entire public surface is renderer-agnostic.

`RenderRoot` (the composition root) caches per-widget output as `imaging::record::Scene` ([render_root.rs:150](repos/xilem/masonry_core/src/app/render_root.rs#L150)). The driver receives these scenes and is responsible for translating them into the actual GPU output.

### 2.3 The `AppDriver` trait is the canonical extension seam

[masonry_winit/src/app_driver.rs:81-130](repos/xilem/masonry_winit/src/app_driver.rs#L81-L130) defines:

```rust
pub trait AppDriver {
    fn on_action(&mut self, window_id: WindowId, ctx: &mut DriverCtx<'_, '_>,
                 widget_id: WidgetId, action: ErasedAction);
    fn on_async_action(...);
    fn on_start(&mut self, state: &mut MasonryState<'_>);
    fn on_close_requested(...);
    fn on_wgpu_ready(&mut self, _wgpu: &WgpuContext<'_>);
}
```

This is the application-facing trait *for masonry_winit specifically* — it bundles winit+wgpu+vello+accesskit_winit+copypasta(clipboard). For Mere, the equivalent is a *new driver crate* (call it `mere_masonry`) that wraps `RenderRoot` with Mere-substrate-shaped integration: substrate input → masonry events; masonry signals → substrate output.

The `signal_sink: Box<dyn FnMut(RenderRootSignal) + 'static>` callback ([render_root.rs:74](repos/xilem/masonry_core/src/app/render_root.rs#L74), [render_root.rs:317](repos/xilem/masonry_core/src/app/render_root.rs#L317)) is the lower-level driver seam — `RenderRoot::new` takes the callback, signals come out, the driver decides what to do.

`RenderRootSignal` includes: `Action(ErasedAction, WidgetId)`, `RequestRedraw`, `RequestAnimFrame`, `new_ime_moved_signal(ime_area)`, plus a handful of others. This is the *exact* surface area Mere's substrate has to react to.

### 2.4 Two drivers already exist — pattern is exercised

The dep graph in [ARCHITECTURE.md](repos/xilem/ARCHITECTURE.md) shows:

- `masonry_winit → masonry_core`
- `masonry_android_view → masonry_core`
- `masonry_testing → masonry_core` (in-process test driver)

Three concrete drivers in-tree, with the README explicitly naming "alternative driver" as a supported extension. This isn't a theoretical extension point; it's load-bearing for Linebender's own multi-platform story.

A `mere_masonry` driver is the same shape as adding macOS / iOS / Wayland-native support — it's a known-good pattern, not a research adventure.

### 2.5 `masonry_imaging` is itself multi-backend

[masonry_imaging/src/](repos/xilem/masonry_imaging/src/) has explicit backend files:

```
vello.rs              # full vello backend
vello_cpu.rs          # vello CPU fallback
vello_hybrid.rs       # vello hybrid (compute + raster)
skia.rs               # skia backend
headless_wgpu.rs      # headless wgpu
texture_render.rs     # generic texture rendering
lib.rs                # backend abstraction
```

Cargo features in `masonry_winit/Cargo.toml`:

```toml
[features]
default = ["imaging_vello"]
imaging_vello = ["masonry_imaging/imaging_vello"]
imaging_vello_hybrid = ["masonry_imaging/imaging_vello_hybrid"]
imaging_skia = ["masonry_imaging/imaging_skia"]
```

So even masonry_winit doesn't lock in vello — it's the default but swappable. Mere can pick `imaging_vello` and let masonry produce a vello scene, then merge into the substrate's vello scene via vello's native scene-append mechanics. Or write a custom `imaging_mere` backend that paints directly into the substrate's scene without an intermediate vello::Scene.

## 3. The two embedding options

### 3.1 Option A — masonry produces a vello::Scene; Mere appends it

**Mechanism**: `mere_masonry` driver uses `imaging_vello` backend. masonry's per-frame output is a `vello::Scene`. The Mere substrate's panel renderer receives this scene and *appends* it into the substrate's own vello scene at the panel's `Placement` transform (vello supports scene-append-with-transform natively).

**Pros**: minimal new code in Mere; reuses Linebender's well-tested vello binding wholesale; vello's append semantics handle clipping and transform composition.

**Cons**: produces an intermediate `vello::Scene` per panel per frame; potentially wasteful if many panels render simultaneously; one extra copy of the scene-encoding bytes.

**Cost shape**: small — write the driver crate, hand masonry a vello Scene per panel, append into substrate scene. The hardest part is bridging input/IME/clipboard/cursor signals through `RenderRootSignal` into Mere's bus.

### 3.2 Option B — custom `imaging_mere` backend; masonry paints directly into substrate scene

**Mechanism**: write a new `masonry_imaging` backend (`imaging_mere`) that translates masonry's imaging primitives directly into the substrate's vello `Scene` at the panel's transform — no intermediate per-panel `vello::Scene` allocation.

**Pros**: zero per-panel scene allocation; direct paint; hot loop is as tight as Linebender's own vello backend.

**Cons**: requires implementing the masonry_imaging backend trait surface (modest; probably ~few hundred lines mirroring the existing `vello.rs`); committed to staying current with imaging API churn.

**Cost shape**: medium — the imaging backend is bounded but non-trivial. Worth doing only if Option A's per-panel-scene-allocation cost shows up in practice.

### 3.3 Recommendation

**Start with Option A.** Prove the embedding works end-to-end with the cheapest possible glue. If/when profiling shows the per-panel scene allocation matters, migrate to Option B (the masonry imaging backend trait is bounded; the migration is contained). This matches the project pattern of "ship the simple thing first; optimise on profile, not on speculation."

## 4. What the `mere_masonry` driver actually does

Concrete sketch of the driver's responsibilities. Illustrative-signature-only, not implementation-ready.

```text
struct MasonryMereDriver {
    render_root: RenderRoot,                      // owns the masonry composition root
    panel_node_id: SceneNodeId,                   // which substrate node this driver renders
    accesskit_bridge: UxtreeAccessKitBridge,      // forwards TreeUpdate → uxtree
    bus: ActionBusHandle,                         // for emitting RenderRootSignal::Action
}

impl InScenePaintRenderer for MasonryMereDriver {
    fn paint(&mut self, node: &SceneNode, ctx: &mut PaintCtx) -> PaintResult {
        // 1. Run masonry's render passes (layout, paint, etc.)
        let masonry_scene: vello::Scene = self.render_root.render(node.size());
        // 2. Append into substrate vello scene at node's Placement transform
        ctx.scene.append(&masonry_scene, Some(node.placement.transform));
        // 3. Forward AccessKit TreeUpdate into uxtree
        if let Some(tree_update) = self.render_root.take_accesskit_update() {
            self.accesskit_bridge.apply(node.identity, tree_update);
        }
        PaintResult::ok()
    }

    fn input(&mut self, node: &SceneNode, event: &InputEvent) -> InputDisposition {
        // Translate substrate InputEvent → ui-events vocabulary masonry expects
        let masonry_event = translate_to_ui_events(event, node.placement);
        let handled = self.render_root.handle_event(masonry_event);
        // Drain RenderRootSignals (Actions, RequestRedraw, IME area updates)
        for signal in self.render_root.drain_signals() {
            self.handle_signal(signal);
        }
        if handled { InputDisposition::Consumed } else { InputDisposition::Passthrough }
    }
}
```

Driver also handles:

- **IME area routing** — `RenderRootSignal::new_ime_moved_signal(rect)` arrives; driver translates the rect from masonry's logical space into substrate physical-screen space and routes to the OS IME bridge (per [OS-plumbing audit §2.1](2026-05-15_os_plumbing_reuse_audit_brief.md)).
- **Cursor icon** — masonry tracks `cursor_icon: CursorIcon` per RenderRootState; driver reads on hover and sets the substrate's cursor.
- **Focus** — masonry has its own focused_widget tracking; driver translates `RenderRootSignal::RequestRedraw` triggered by focus changes into substrate redraws and exposes the focused-widget AccessKit node id to the substrate's focus router.
- **Clipboard** — masonry's clipboard ops route through the driver (`copypasta` is the clipboard crate masonry_winit uses; for `mere_masonry` we'd substitute Mere's `mere-os-clipboard` per the audit's umbrella sketch — almost certainly arboard).

Most of these are 5-20 line bridging functions. The big work is *getting the wiring right*, not writing volume.

## 5. What this means for the substrate plan

This spike resolves three open questions from the spatial chrome IR brief:

### 5.1 Resolves §10.2 — reactive runtime own vs. borrow

**Borrow.** xilem_core is exactly the right shape — pure reactive runtime, no rendering deps, gpui-equivalent ergonomics, two existing consumers proving the layering works. Save weeks of "build our own Entity<T>+cx" work; gain Linebender's hardening, parley integration, AccessKit story, signals + futures + memoization machinery they've already written.

### 5.2 Sharpens §6 — the host shrink

The substrate-as-host scenario from §6 was:

```
mere-host = window manager + GPU surface + spatial scene graph runtime + input router
```

With xilem+masonry embedded, the *panel-shaped subset* of "spatial scene graph runtime" gets cheaper — masonry handles per-panel layout, paint, focus, accessibility, IME area. The substrate handles placement, LOD, identity, relations, dispatch. **Two well-defined layers, with a clean trait interface (`AppDriver` / `signal_sink`) between them.**

### 5.3 Refines [renderer registry §7](2026-05-15_renderer_registry_contract_brief.md) per-renderer mapping

The `mere-domain.panel` row in the per-renderer mapping table:

| Renderer            | Composition mode    | Handles content kinds                                | Status                                     |
| ------------------- | ------------------- | ---------------------------------------------------- | ------------------------------------------ |
| `mere-domain.panel` | InScenePaint        | `Panel(PanelKind, ViewIntent)`                       | needs reactive runtime — biggest new piece |

becomes:

| Renderer            | Composition mode    | Handles content kinds                                | Status                                     |
| ------------------- | ------------------- | ---------------------------------------------------- | ------------------------------------------ |
| `mere-domain.panel` | InScenePaint        | `Panel(PanelKind, ViewIntent)`                       | xilem_core + masonry_core via `mere_masonry` driver (Option A) |

The "biggest new piece" framing dissolves.

### 5.4 OS-plumbing audit gets re-shaped

Several subsystems the [OS-plumbing audit](2026-05-15_os_plumbing_reuse_audit_brief.md) recommended `reimplement-against-trait` are now `borrow-from-masonry`:

- **Focus + keyboard navigation** (§2.3) — masonry has it, scoped to widget tree; substrate-level focus router only needs to manage focus *between* panels, not within them.
- **Cursor management** (§2.9) — masonry tracks cursor_icon per widget; driver propagates upward.
- **Pointer / touch / pen** (§2.14) — masonry uses ui-events; ui-events handles touch + pen vocabulary; driver translates substrate events into ui-events and back.

Doesn't change the macOS IME hardening commitment (§2.1) — that's downstream of whatever reactive layer we use; ui-events is the intermediate vocabulary either way; the actual OS-side IME plumbing still needs hardening.

## 6. Risks and open questions

### 6.1 vello version coordination

Mere's substrate uses vello via netrender (per [netrender-for-engine-documents brief](2026-05-09_netrender_for_engine_documents_brief.md), netrender pins vello at git main / 0.8.0). xilem also pins vello — version coordination matters for cargo unification. Likely fine if both track recent vello main; needs a one-shot check at integration time.

### 6.2 ui-events vocabulary fit

masonry uses the [`ui-events`](https://crates.io/crates/ui-events) crate as its input event vocabulary. Mere's spatial input router would translate `InputEvent` (substrate-shaped) into ui-events when dispatching to a masonry-rendered panel. ui-events is an active Linebender project; vocabulary fit looks reasonable but the touch/gesture surface needs verification. Worth a half-hour read-through of ui-events' surface before committing.

### 6.3 xilem maturity

Per README: "An experimental Rust architecture for reactive UI." xilem is not 1.0; API churn is plausible. Linebender has a history of soft-landing breaking changes (druid → xilem migration was deliberate). Substrate adoption couples Mere to xilem's release cadence; mitigations: pin a known-good version; track main upstream on a separate branch; contribute back where Mere-relevant changes shake out.

### 6.4 The reactive-layer / substrate seam

xilem_core's reactive model assumes a view-tree-shaped output; the substrate's spatial scene graph is *not* tree-shaped at the substrate level — it's a graph. The cleanest seam: masonry-rendered panels are *trees inside scene-graph nodes*, with the substrate's spatial graph above. This works because each `mere-domain.panel` *node* is one tree of widgets, not a slice of a larger tree. Cross-panel relations (sync-bindings, transclusions) live in the substrate; intra-panel structure lives in masonry. **The view-tree / scene-graph mismatch is resolved by scoping**, not by trying to fit a graph into a tree or vice versa.

### 6.5 Cross-platform validation

Per `user_testing_hardware`, all four desktop targets (Windows / macOS / Wayland / X11) are now locally validatable. xilem's masonry_winit driver runs on all four; if the embedding works on Windows (the dev box), porting smoke validation across the other three is straightforward — masonry does the cross-platform input lift.

### 6.6 AccessKit bridge into uxtree

Per `project_mere_domain_layer`, mere-domain's `uxtree` is intended to be AccessKit-native. The bridge from `accesskit::TreeUpdate` (out of masonry's RenderRoot) into uxtree is plausibly *trivial* if uxtree owns an `accesskit::Tree` — possibly just `tree.update(masonry_update)`. Needs verification in mere-kernel/uxtree code; out of this spike's scope.

## 7. Recommendation

**Three actions, in order:**

1. **Read-only follow-up: read xilem_core's `View` trait + masonry_core's pass system docs.** Half-day. Confirms the reactive-layer integration shape from a code-reading perspective. Done conditions: can describe the `View::View<T, A>` machinery in one paragraph; understand the four passes (event → update → layout → paint).
2. **Spike the integration: write a minimal `mere_masonry` driver.** Done conditions: a single-pane Mere window where the pane content is a masonry "hello world" widget; mouse hover + keyboard focus work; AccessKit tree shows the masonry widget; vello scene is owned by Mere's substrate (not masonry's wgpu binding). Possibly a stress-test stretch goal: two panes both rendering different masonry view trees, demonstrating multi-tenant embedding.
3. **If spike succeeds: register `mere-domain.panel` against the `mere_masonry` driver in the renderer-registry adoption plan.** This is the natural insertion point into [renderer-registry §12 v0 done conditions](2026-05-15_renderer_registry_contract_brief.md) — replaces the "needs reactive runtime" gap with "embed masonry."

If (2) surfaces unexpected friction (vello version mismatch, ui-events vocabulary gap, AccessKit-uxtree integration trouble), this brief gets re-stated with concrete blockers. If (2) succeeds, **the long-term host story becomes "vello + xilem-as-panel-renderer + Mere substrate above" with much less greenfield code than the spatial chrome IR brief §6 anticipated.**

## 8. Decisions and non-decisions

**Decides:**

- xilem + masonry can be embedded; the architecture explicitly supports it. Confidence: high (read the architecture, dep graph, driver trait, signal vocabulary, multi-backend imaging layer, and observed two existing drivers).
- **Borrow xilem_core for the reactive runtime** (resolves spatial chrome IR §10.2 in favour of borrow).
- **Use Option A** (masonry produces vello::Scene; substrate appends) as the v0 embedding strategy. Migrate to Option B only on profiling evidence.
- The `mere-domain.panel` renderer in the renderer registry maps to a `mere_masonry` driver wrapping `xilem_core + masonry_core`.

**Does not decide:**

- Whether to ship the embedding (just like every other 2026-05-15 brief, this is research). Adoption schedule lives in a future plan.
- The xilem version pin (one-shot check at integration time).
- The ui-events vocabulary fit (needs the half-hour follow-up read).
- The AccessKit-uxtree bridge details (out of scope).
- Whether to contribute upstream (Linebender appears welcoming; defer until there's something to contribute).

## 9. What this spike does and does not deliver

**Delivers:** read-the-architecture-grade confidence that embedding is possible and well-supported. A concrete sketch of the `mere_masonry` driver. Resolution of three open questions from sibling briefs (spatial chrome IR §10.2, renderer registry §7 panel renderer status, OS-plumbing audit §2.3 / §2.9 / §2.14 reuse posture).

**Does not deliver:** running code. The actual `mere_masonry` driver isn't written. The spike's natural progression is §7's three actions — particularly §7.2, the minimal embedding spike with a pane rendering a masonry widget tree, which is the next concrete artefact.
