# Mere App Architecture — Re-scaffold (2026-05-21)

**Status**: Canonical app/host architecture. Supersedes the substrate-as-host
model for the *host* (see §7).
**Supersedes (for the host layer)**: [`../research/2026-05-15_spatial_chrome_ir_brief.md`](../research/2026-05-15_spatial_chrome_ir_brief.md),
[`../research/2026-05-15_renderer_registry_contract_brief.md`](../research/2026-05-15_renderer_registry_contract_brief.md),
[`../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md),
and the host portions of [`2026-05-20_host_architecture_roadmap.md`](2026-05-20_host_architecture_roadmap.md).
Their *spatial* insights survive — relocated into the graph-canvas widget (§3).
**Evidence**: Woodshed (sibling Strophos app on the same Xilem+Masonry+Vello
stack) proves the idiomatic shape; `xilem_web` confirms the web story.
**Correction (2026-05-21)**: this doc's §5 "collapses … forme / platen" was
wrong. They have sharp, rent-paying roles — forme = graph-capable *arrangement
authority*, platen = *projection compiler* — see
[`2026-05-21_mere_composition_spine.md`](2026-05-21_mere_composition_spine.md),
the canonical product spine. What collapses is the substrate-as-host scaffolding,
the action bus, and the printing-press-as-executable-pipeline — not forme/platen
as a model→projection layer feeding Xilem.

---

## 1. The reframe

Mere had been building a **second app-coordination layer** — substrate-as-host
(flat scene + renderer registry), a bespoke action bus, and a hand-rolled
winit `ApplicationHandler` — on top of Xilem, which already *is* an
app-coordination layer. The spatial-chrome-IR brief argued "every host
framework is document-tree-shaped, but a spatial browser is graph-shaped." That
is **half right**: the *chrome* (frametree splits, workbench, panels, toolbar)
is an ordinary tiling UI — exactly what Xilem/Masonry do idiomatically. Only the
**graph canvas** (the orrery: positioned nodes, edges, pan/zoom, LOD) is
genuinely graph-shaped, and it is a *leaf widget*, not the whole host.

So: **flip figure and ground.**

- **The app is an idiomatic Xilem tree.** Chrome = Xilem views over one
  `AppState`. No substrate-as-host, no renderer registry for chrome, no action
  bus, no hand-rolled event loop.
- **The graph canvas is one custom Masonry `Widget`** that owns the spatial work
  internally.
- **Engine tiles are custom widgets** compositing engine output.
- **State + logic stay framework-free** so a future web view layer
  (`xilem_web`, DOM) swaps in over the same core.

This is Woodshed's exact shape (one `AppState`, closures mutate in place, views
rebuild on diff, no action bus, panes are composition in one RenderRoot) plus
the one thing Mere adds that Woodshed doesn't need: a spatial canvas widget.

## 2. The app shape (idiomatic Xilem)

Entry — the Woodshed/Xilem pattern:

```rust
// illustrative
fn main() {
    Xilem::new_simple(AppState::new(), app_logic, WindowOptions::new("Mere"))
        .run_in(EventLoop::with_user_event())
        .unwrap();
}

fn app_logic(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // The frametree IS the view tree. Splits are split views; panes are
    // view functions; the orrery is the custom GraphCanvas widget.
    split(
        workbench_view(state),
        split(graph_canvas_view(state), apparatus_view(state))
            .split_axis(Axis::Vertical),
    )
}
```

`AppState` is **one struct the driver owns** (Woodshed's lesson — large and
mutable is fine; group per-pane sub-state into its own struct to keep the top
level readable):

```rust
// illustrative shape, not final
struct AppState {
    graph: Graph,                 // kernel — the truth
    frame: FrameLayout,           // the split tree (chrome geometry)
    cartography: CartographyState,// strategy + projection cache for the canvas
    selection: NodeSelection,     // selected graph nodes
    session: SessionState,        // active session id + dirty tracking
    // per-pane UI sub-state structs as panes grow…
}
```

Mutation flows the Xilem way: a widget/handler closure takes `&mut AppState`,
mutates it, the view rebuilds on diff. **No action bus, no intent enum at the
app level.** (A typed graph-mutation enum *inside* the canvas widget, for
undo/journal, is a local concern — not app-wide.)

## 3. The graph canvas as a Masonry widget

The genuinely-spatial work — everything we built into the substrate path —
relocates into one custom widget, `GraphCanvas`:

- **paint**: run cartography projection (`cartography` + `graph-layout`) → paint
  nodes/edges into the widget's vello scene fragment, under its own pan/zoom
  transform. (The orrery painter + graph-node painter we wrote are the body.)
- **input**: hit-test nodes in canvas-local space; select/drag; pan/zoom. Emits
  an *action* (Xilem's `message`) so the host mutates `AppState.graph` /
  `selection` — same drag/select logic we already wrote, minus the substrate.
- **embedded tiles**: a node whose content is a web/smolweb document composites
  the engine's output. External wgpu texture (serval/scrying) or in-scene paint
  (nematic) — the EmbeddedFrame/InScenePaint distinction survives *here*, scoped
  to the canvas, not the whole host.
- **LOD / camera / addressable identity**: canvas-internal, where they belong.

This is Woodshed lesson #2 ("fretboard → custom Masonry widget; the drawing math
transfers unchanged") at canvas scale. The five spatial-IR properties become
this widget's contract, not a host mandate.

## 4. Engines & tiles

Engine choice (nematic / serval / scrying) is **"which engine backs this
tile."** A tile is a custom widget (in the workbench pane, or a canvas node)
that composites its engine's output. `inker` stays the engine-selection layer;
the renderer-registry's multi-engine routing *localizes* to the tile, not a
global registry. The four concerns map cleanly:

| Concern | Realization |
|---|---|
| frametree (splits) | Xilem `split` views; `FrameLayout` in `AppState` |
| tile tree (workbench) | a pane composing tile widgets |
| engine choice | which engine backs a tile (`inker`) |
| composable tiles (verso) | tiles compose as Masonry widgets |

## 5. What survives vs. collapses

**Survives (the genuinely hard, Mere-specific work):**
- `kernel` — graph truth. Becomes a foundational top-level crate (no longer
  nested under a chrome-named supercrate).
- `cartography` + `graph-layout` + `graph-canvas` IR — feed `GraphCanvas`.
- `inker` + engines (`nematic`, `scrying-engine`, Serval) — tile content.
- `eidetic` (+ fetchers), `intel`, `murm`, `moot`, `persona` — orthogonal; keep.
- `session-runtime`'s stores (manifest, graph, view-intent) — the persistence
  layer; keep. (Already proved: graph round-trips across restart.)
- The orrery/graph-node painting + selection/drag logic — moves into
  `GraphCanvas`.

**Collapses (the over-engineering):**
- `spatial-substrate` + `register-renderer(-types)` as the *top-level host
  model* — folds into `GraphCanvas` internals (or retires).
- `host-substrate` (`HostApp`, scene sync, pane/splitter identity maps) — the
  frametree lives in `AppState`; Xilem owns the loop.
- `control-plane` action bus — redundant with Xilem messages.
- The substrate panel renderers (orrery/graph-node/splitter/masonry-embedded
  *as registry tenants*) — chrome panels become plain Xilem views; the canvas
  becomes the widget.

**Crate shape** trends toward a few coarse crates around the four concerns + a
portable core, not 11 capability supercrates (the "too cute by half" finding).
Exact consolidation is a follow-up; the *kernel/cartography/engines/eidetic*
distinctions are real and stay.

## 6. Web (the C1 answer)

`xilem_web` is a **separate DOM backend** — shares `View` logic at
`xilem_core`, renders to `div`/`button`/`canvas`, carries **no Masonry**.
Masonry-native is inherently winit/wgpu/tokio (the `rt-multi-thread` pin is
`xilem_masonry`'s, upstream). So "Masonry on web" is not a thing; **web Mere is
a separate view layer over the portable core** — Woodshed's
`view-core`/`view-native`/`view-web` plan. Action: keep state + logic
framework-free from here on; do **not** feature-gate tokio (it wouldn't make
masonry-native a web target). Native first; web is a later, separate view layer.

## 7. Migration (jump-ship, not migrate)

Per the prototype doctrine (rewrite over migrate for zero-user code), the host
is **rewritten** as an idiomatic Xilem app, not incrementally migrated:

1. **Skeleton** — `Xilem::new_simple` + `AppState` + `app_logic` with the
   frametree as `split` views and stock-widget placeholder panes. Touchable
   idiomatic-Xilem chrome.
2. **GraphCanvas widget** — the custom Masonry widget; port the orrery/graph-node
   painting + selection/drag into it; wire cartography.
3. **Panels** — apparatus/gloss/workbench as plain Xilem views over `AppState`
   (retire the per-panel `XilemPanel`/`Arc<Mutex>` plumbing).
4. **Engine tiles** — first real engine (`scrying.web`) as a tile widget.
5. **Retire** the substrate-as-host crates from the host path once parity lands.

## 8. The one rule (unchanged, sharpened)

If it could be unit-tested without a window, it belongs in a crate — and now:
**if Xilem already provides it (coordination, widget tree, input routing,
message flow), don't reimplement it.** The host is the thinnest possible Xilem
app; the only Mere-novel host-side artifact is the `GraphCanvas` widget.
