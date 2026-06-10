# Cartography Layer — design brief

**Date**: 2026-05-10
**Status**: Design probe (pre-implementation)

> **Crate-name note (2026-06-09 audit):** `cartography` shipped and now lives at `crates/orrery/cartography`. `graph-canvas` has since **dissolved** into the `orrery/*` family; `graph-layout`→`orrery/arrangements`; `mere-kernel`/`mere_kernel`→`graph/graph-kernel`. The strategy catalogue and v0 "landed" receipts below are historical record.
**Scope**: Proposes a new `cartography` crate as the **non-destructive projection layer** between graph truth + intelligence signals on the input side, and canvas swatches on the output side. Owns contracts (strategy traits, overlay shapes, minimap descriptors); does not own strategies themselves — those live in a sibling layout crate (proposed name: `graph-layout`, sitting beside `graph-canvas`).

**Related**:

- [`../implementation_strategy/2026-05-09_post_engine_layer_priorities.md`](../implementation_strategy/2026-05-09_post_engine_layer_priorities.md) — §2.3 resolved the donor `graph-cartography` crate as **drop, don't migrate**. This brief reclaims the name for a substantially different concern.
- [`../../graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md`](../../graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md) — field algebra living inside `graph-canvas` (ZSource → FieldProjection, Burn lowering). Cartography sits *above* this: the field algebra is one of the primitives strategies use; cartography is the contract layer that picks and composes strategies.
- [`2026-05-08_local_intelligence_integration_research.md`](2026-05-08_local_intelligence_integration_research.md) — clusters, affinity, embeddings. The primary upstream signal source for cartography overlays.

---

## 1. The framing

Cartography is the discipline of making the **territory** (graph truth) read as a **map** at the user's current scale and intent — without ever mutating the territory.

Inputs:

- **Graph truth** — `mere_kernel::graph::Graph` (immutable reference).
- **Intelligence signals** — clusters, affinity scores, hot regions, bridge nodes, importance hints. Produced by `intelligence-embeddings` and consumed through a narrow contract type, not a direct dependency on the crate's internals.
- **View intent** — what the user is trying to see right now: scale, dimension (2D/2.5D/3D), focus node, filter ("only kin"), form factor (full-screen orrery vs. workbench swatch vs. minimap thumbnail vs. volvelle radial).

Outputs:

- **Projections** — a positioned representation of the graph at a chosen layout, ready for `graph-canvas` to render.
- **Overlays** — semantic emphases the canvas applies on top of geometry: cluster halos, edge weights, activity heat, bridge emphasis, importance scaling.
- **Minimaps** — extreme-zoom-out projections of any swatch (graph or document), with viewport rectangle overlay. Same primitives, re-tuned for legibility at thumbnail scale.

The graph stays canonical. Cartography is *representation*, not truth.

## 2. Why a separate layer

Without cartography, the alternatives are:

1. **Layout logic inside `graph-canvas`** — what's there today (`scene_physics.rs`, `projection.rs`). Workable for one strategy but starts hurting when the orrery wants force-directed-by-meaning, the moot-volvelle wants radial, and the workbench-swatch minimap wants cluster-collapsed hub-summary view. Three layouts × N intelligence-signal types × M swatch contexts → cross-product explosion inside the renderer.
2. **Cartography spread across consumers** — each consumer of the graph (orrery domain, moot domain, workbench domain) reinvents its own layout-and-overlay pipeline. Patterns rediverge; minimaps look different in every surface.

A dedicated contract layer:

- Forces a single shape for "graph truth + intelligence signals + view intent → projection."
- Lets strategy authors target one trait, regardless of which surface their strategy runs in.
- Lets surface authors compose strategies and overlays without knowing how a strategy is implemented.
- Makes minimaps a first-class concern by giving them the same vocabulary as full-scale views.

## 3. Architecture sketch

```text
       graph truth (mere_kernel::graph::Graph)
                    │
                    │ &Graph
                    │
intelligence-       │
embeddings ────────►│  (ClusterSet, AffinityScores, BridgeNodes …)
                    │
                    ▼
              ┌──────────────┐
              │ cartography  │
              │              │  view intent
              │   contracts  │◄────────────
              │              │  scale, focus, filter, form factor
              └──────┬───────┘
                     │
       picks strategy + overlays
                     │
                     ▼
              ┌──────────────┐
              │ graph-layout │  (strategies: force-directed,
              │              │   radial, cluster-collapsed,
              │  strategies  │   time-banded, importance-scaled,
              │              │   filtered, force-directed-by-meaning)
              └──────┬───────┘
                     │
                     │ Projection (positioned nodes + edges + overlays)
                     │
                     ▼
              ┌──────────────┐
              │ graph-canvas │  ← pure renderer; consumes a Projection
              │   (renderer) │     and draws it. No layout decisions
              └──────────────┘     here past geometry.
```

For non-graph swatches (document-canvas, future swatches), the same pattern applies with a document-layout cousin. The cartography contracts are surface-agnostic enough to cover both — that's why the crate is **not** named `graph-cartography`.

## 4. Contract surface — **dual strategy contract**

**Decision (2026-05-11)**: cartography exposes **two** strategy traits because two kinds of layout exist, and forcing both through a single contract creates an honest-scope problem.

- **Analytic** (`LayoutStrategy::project`) — one-shot, stateless from cartography's perspective. Phyllotaxis, Penrose, Radial, Grid, Timeline, Kanban, L-system, ClusterCollapsed (astroid). The output is determined entirely by the input; running the algorithm twice with the same input yields the same projection.
- **Streaming** (`StreamingLayoutStrategy::step`) — iterative, state-carrying. ForceDirected, BarnesHut, SemanticEmbedding, SemanticEdgeWeight — anything that converges over multiple frames. The canvas calls `step()` each frame, threads mutable state, and stops when `is_converged()` reports true.

Strategies pick which trait fits their algorithm. Canvases that support iteration call `step()` on streaming strategies and `project()` on analytic ones. Both emit the same [`Projection`] output type so canvases consume one shape uniformly.

**Why dual rather than unified**: graph-canvas already has a rich, well-shaped iterative `Layout<N>` trait (`step(&scene, &mut state, dt, viewport, extras) -> HashMap<N, Vector2D<f32>>`) with associated `State`, `is_converged()`, and `DynLayout<N>` for trait-object dispatch. Forcing all of that through a one-shot `project()` would mean either (a) hiding the iteration loop inside each streaming strategy's `project()` and losing the per-frame "watch it settle" UX, or (b) threading state through `ProjectionMetadata` as opaque bytes and losing type-safety. Both compromises are worse than just acknowledging the two shapes exist.

```rust
// illustrative — final shapes are in crates/cartography/src/lib.rs

pub struct ProjectionRequest<'a> {
    pub graph: &'a Graph,
    pub signals: &'a IntelligenceSignals,
    pub intent: ViewIntent,
}

pub struct ViewIntent {
    pub form_factor: FormFactor,   // Canvas, Orrery, Volvelle, Astroid, Minimap
    pub dimension: ProjectionDimension,
    pub focus: Option<NodeKey>,
    pub filter: Option<NodeFilter>,
    pub target_size: TargetSize,
}

// Analytic — one-shot.
pub trait LayoutStrategy {
    fn projection_id(&self) -> &'static str;
    fn project(&self, req: &ProjectionRequest<'_>) -> Projection;
}

// Streaming — iterative with associated state. Mirrors graph-canvas's
// existing `Layout<N>` so existing iterative impls conform with thin
// adapters.
pub trait StreamingLayoutStrategy {
    type State: Default + Clone + Serialize + for<'de> Deserialize<'de>;
    fn projection_id(&self) -> &'static str;
    fn step(&self, req: &ProjectionRequest<'_>, state: &mut Self::State, dt: f32) -> Projection;
    fn is_converged(&self, _state: &Self::State) -> bool { false }
    fn project_initial(&self, req: &ProjectionRequest<'_>) -> (Projection, Self::State) { /* ... */ }
}

pub struct Projection {
    pub nodes: Vec<PositionedNode>,
    pub edges: Vec<PositionedEdge>,
    pub overlays: Vec<Overlay>,    // ClusterHalo, ActivityHeat, BridgeEmphasis, ImportanceScale, EdgeWeight
    pub minimap: Option<MinimapDescriptor>,
    pub content_bounds: PortableRect,
    pub binding_mode: ProjectionBindingMode,  // see below
    pub metadata: ProjectionMetadata,
}

/// Whether a projection re-renders when its source graph mutates, or is
/// captured at one moment and frozen.
///
/// Per [graphshell harvest brief](2026-05-17_graphshell_harvest_brief.md) Tier 1 / T1-5: making
/// this explicit catches subtle bugs. Today every projection is implicitly
/// "live-bound" — that's wrong for switcher thumbnails (captured snapshots
/// that should not re-render when the captured graph mutates), wrong for
/// printed/exported maps (frozen at export time), and right for the active
/// orrery's projection (must re-render).
pub enum ProjectionBindingMode {
    /// Re-render when the source graph mutates. Default for active views.
    Linked,
    /// Captured at projection time; ignores subsequent graph mutations.
    /// Default for thumbnails, exports, switcher previews, and any
    /// projection that lives outside an active view's render loop.
    Unlinked,
}

pub struct IntelligenceSignals {
    pub clusters: Option<ClusterSet>,
    pub affinity: Option<AffinityScores>,
    pub bridges: Option<BridgeNodes>,
    pub importance: Option<ImportanceWeights>,
    // … grows as intelligence-embeddings produces more signals
}
```

**Deferred from v0** until a concrete consumer materializes: object-safe `DynStreamingStrategy` + `ErasedState` (needed when a `LayoutRegistry` holds heterogeneous strategies), `LayoutCapability` / `LayoutCategory` / `LayoutProvenance` metadata (needed for a picker UI), and the registry itself. Graph-canvas's existing `DynLayout` / `LayoutRegistry` / `LayoutCapability` types serve graph-layout's internal needs until cartography needs to own a cross-swatch registry.

The donor `graph-cartography`'s `CartographyCanvasSummary` / `CanvasClusterHalo` / `CanvasBridgeEmphasis` / `CanvasActivityHeat` types are the natural starting point for the `Overlay` variant set — that's exactly the level the donor crate was operating at, just with the visit-aggregate baggage stripped out.

## 5. Strategy bundle (lives in `graph-layout`, not cartography)

First strategies worth shipping with the layout crate:

- `ForceDirected` — current default. Already exists inside `graph-canvas`; gets moved.
- `ForceDirectedByMeaning` — force-directed where the spring rest length is determined by embedding distance. The first concrete intelligence-signal consumer (per the [local-intelligence research](2026-05-08_local_intelligence_integration_research.md)).
- `Radial` — for `Volvelle` form factor (expanded moot).
- `ClusterCollapsed` (a.k.a. `Astroid`) — community-detection-driven hub-collapse. Matches the existing UX vocab item.
- `Tree` (Sugiyama-shaped) — for hierarchical sub-views and DAG-shaped topology.
- `TimeBanded` — horizontal time axis by `created_at` / `last_visited`; vertical position by cluster or domain group.
- `ImportanceScaled` — node screen-area proportional to importance signal.
- `Filtered` — subset projection ("only kin nodes", "only the 2-hop neighborhood of focus").
- `BarnesHut` — O(n log n) force-directed variant for large graphs (>500 nodes).
- `Phyllotaxis` — golden-angle spiral; position-in-graph encodes priority/recency (near-center = more salient). Closed-form, ~5 lines of math.
- `Penrose` — aperiodic tiling vertex placement; "more than grid, less than organic" with non-repeating local structure that aids spatial memory. ~150 LOC of geometric subdivision.
- `LSystem` — nodes along an L-system fractal path (Hilbert curve / Koch / dragon / space-filling). Cache-coherent spatial locality for very large graphs.
- `Grid` — snapped-grid layout for structured note-taking views.
- `MapProjection` — geospatial placement from lat/long node metadata.
- `Rapier` — full rigid-body simulation (already partly landed at `crates/graph/graph-canvas/src/simulate.rs`); a `LayoutStrategy` adapter drives positions from rapier body translations. Best for zone-pinned and physically-constrained layouts.

These are not exclusive — a single projection can combine them (force-directed-by-meaning over a filtered subset with importance-scaling applied). The bundle is also not meant to land all at once; strategies arrive as their consumers materialize. The catalogue exists so future authors don't reinvent shapes that already have known good algorithms.

**Inherited from the [graphshell 2026-02-24 physics engine extensibility plan](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/graph/2026-02-24_physics_engine_extensibility_plan.md)**: that doc's catalogue + three-level extension framing + sum-type dispatcher pattern + helper-era preset portfolio (drift/scatter/settle/archipelago/resonance/constellation) all carry forward. What does NOT carry: the egui_graphs `Layout<S>` trait import, the app-local `graph/physics.rs` location, the `inventory::submit!` registration mechanism, `PhysicsProfileRegistry` / `LayoutRegistry` / `LensCompositor` coupling, Wasmtime/extism scripting (replaced by Rhai per [browser/PWA scripting memory](memory)), Verse-named persistence concepts (Verse folded into Mere-at-network-scope), and the rapier2d Canvas Editor desktop panel (out of scope for Mere's host shape).

**Helper-era preset portfolio recast as cartography view-intent presets** (named compositions of strategy + tuning + overlay set):

| Preset | Strategy | Overlay emphasis | Intent |
| --- | --- | --- | --- |
| `drift` | ForceDirected (gentle) | minimal | Default browse / gentle exploration |
| `scatter` | ForceDirected (high-repulsion, no gravity) | minimal | Overview / import explode |
| `settle` | ForceDirected (tight attraction) + degree repulsion overlay | minimal | Stable working set |
| `archipelago` | ForceDirected + ClusterCollapsed hints | strong ClusterHalo | Domain islands |
| `resonance` | ForceDirectedByMeaning | strong ClusterHalo (semantic) | Semantic neighborhoods |
| `constellation` | ForceDirected + ImportanceScaled | BridgeEmphasis + ImportanceScale | Hub-and-spoke readability |

The aesthetic vein of the prior doc's catalogue (Phyllotaxis for priority queues, Penrose for spatial-memory aid, L-system for topological paths) is worth preserving — these layouts fit Mere's evocative-vocabulary lean better than generic force-directed and earn their keep as user-pickable shapes.

## 6. Minimap as cartography

Minimap is **not** a separate concern. It's a `FormFactor::Minimap` view intent with a small `target_size` and (often) a stripped-down overlay set. The same strategy that produced the full-screen projection produces the minimap, possibly with cheaper variants of overlays at thumbnail scale.

This generalizes beyond graph swatches: `document-canvas` minimaps (Sublime-style document overviews) are the same shape — a projection at extreme zoom-out with the viewport rectangle highlighted. The cartography contracts are surface-agnostic; a `document-layout` cousin would implement them for document swatches.

## 7. Open scoping decisions

Two minor scoping calls worth making before the first line lands:

1. **`graph-layout` placement**: sibling crate (`crates/graph/graph-layout/`) or submodule of `graph-canvas` (`crates/graph/graph-canvas/src/layout/`). Sibling matches the "graph-canvas is the renderer; layout is separate" framing better and avoids the renderer crate growing back into a kitchen sink. Recommended: sibling.
2. **Overlay vocabulary in `cartography` or in `graph-layout`**: cartography owns the *contracts*, but the `Overlay` variant set is a vocabulary that strategies emit. Putting it in cartography keeps the vocabulary stable across strategy implementations (every strategy in graph-layout speaks the same overlay nouns); putting it in graph-layout couples the vocabulary to a particular strategy crate's evolution. Recommended: cartography.

## 8. Relationship to existing work

- **`graph-canvas` field algebra** ([2026-05-07 plan](../../graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md)) — unaffected. The field algebra is *primitive math* that strategies in `graph-layout` use. Cartography sits above; field algebra is below. The field-algebra plan's "Burn lowering for GPU evaluation" is still load-bearing — strategies that need GPU-accelerated layout (large graphs) lower through it.
- **`intelligence-embeddings`** ([2026-05-08 research](2026-05-08_local_intelligence_integration_research.md)) — the primary upstream signal source. The narrow `IntelligenceSignals` contract type in cartography is the firewall: cartography depends on the *shape* of the signals, not on intelligence-embeddings' internals.
- **`platen`** — the orchestrator. `platen` already pairs reducer state with rendering output; it gains a new responsibility of building `ProjectionRequest`s and passing them to cartography, then handing the result to the appropriate canvas swatch.
- **`mere-domain/orrery`** — first natural consumer. The orrery is the t1 graph view; making it read as a *map* rather than a network is exactly what cartography is for.

## 9. Sequence — **revised 2026-05-11**

**Scope correction**: the original §9 estimated step 2 as "~300 LOC moved" for "ForceDirected from graph-canvas." That was wrong. Reading `crates/graph/graph-canvas/src/layout/` showed the existing layout subsystem is far richer:

- `Layout<N>` trait with associated `State`, `step()`, `is_converged()` — the right shape, parallel to cartography's new `StreamingLayoutStrategy`.
- Already-implemented strategies: `ForceDirected` (518 LOC), `BarnesHut` (566), `Phyllotaxis`+`Radial`+`Grid` (`static_layouts.rs` 888), `Penrose` (596), `LSystem` (447), `Kanban`+`Timeline` (`axial.rs` 476), `SemanticEmbedding` (587), `SemanticEdgeWeight`.
- Extras passes: `DegreeRepulsion`, `DomainClustering`, `FrameAffinity`, `HubPull`, `SemanticClustering` (`extras.rs` 904).
- A full `LayoutRegistry` + `LayoutProvider` + `BuiltinProvider` + `DynLayout` + `ErasedState` + `LayoutCapability` + `LayoutCategory` + `LayoutProvenance` system (`registry.rs` 810).
- Total: ~6500 LOC across 12 files. The catalogue I sketched in §5 is already implemented.

This changes the work shape entirely. Cartography's job is no longer "introduce a strategy trait + bring `ForceDirected` over." It's **"adopt cartography contracts on top of the existing well-shaped subsystem, then lift the subsystem into a sibling crate."**

**Revised sequence:**

1. **Cartography v0** — contract types only. Dual strategy contract (`LayoutStrategy` + `StreamingLayoutStrategy`), `ProjectionRequest`, `ViewIntent`, `IntelligenceSignals`, `Projection`, `Overlay`, `MinimapDescriptor`. **Landed 2026-05-11** (`crates/cartography/`, 6 modules, 13 tests at default features).
2. **Adapter shims, feature-gated inside cartography** — implement `StreamingLayoutStrategy` / `LayoutStrategy` for each existing graph-canvas layout. **First adapter (`ForceDirectedAdapter`) landed 2026-05-11** under cartography's `graph-canvas-adapters` feature, 21 tests at full features. **Important location decision**: the adapters live inside cartography itself (under a feature flag), not inside `graph-canvas`. Reason: `mere-kernel` has a legacy dependency on `graph-canvas` (for `graph_canvas::packet::Stroke` in `OverlayStrokePass`), so adding `graph-canvas → cartography` would make `mere-kernel → graph-canvas → cartography → mere-kernel` a dependency cycle. Putting the adapter in cartography (which depends one-way on graph-canvas under the feature) sidesteps the cycle. Future adapters for `BarnesHut`, `SemanticEmbedding`, `SemanticEdgeWeight`, `Phyllotaxis`, `Penrose`, `Radial`, `Grid`, `Timeline`, `Kanban`, `LSystem` land here. ~50 LOC per strategy × ~10 strategies = ~500 LOC. No file moves in graph-canvas.
3. **Bridge `IntelligenceSignals` ↔ `LayoutExtras<N>`** — graph-canvas's `LayoutExtras` carries `pinned`, `domain_by_node`, `semantic_similarity`, `frame_regions`, `embedding_by_node`, `axis_value_by_node`, `dragging`. Cartography's `IntelligenceSignals` carries `clusters` / `affinity` / `bridges` / `importance`. The shapes overlap but aren't identical — the adapter does the conversion. First pass (in `ForceDirectedAdapter`): clusters → `domain_by_node`, affinity → `semantic_similarity`. Other extras stay empty until concrete consumers materialize. ~100 LOC of bridge logic per adapter that needs it.
4. **Sibling-crate move** — once adapters are healthy AND the `mere-kernel → graph-canvas` dependency is broken (a separate prerequisite — move `OverlayStrokePass`'s `Stroke` field downstream of the kernel, or inline `Stroke` into the kernel), lift `crates/graph/graph-canvas/src/layout/` to `crates/graph/graph-layout/` as its own crate. The adapters move with it. `graph-canvas` keeps a thin re-export for backward compat through one cycle, then drops it. `LayoutRegistry` moves with the layouts. Cartography stays minimal (contracts only); the `graph-canvas-adapters` feature gets retired in favor of `graph-layout` depending directly on cartography.
5. **Wire `platen`** — build `ProjectionRequest`s for the workbench's active graph view, dispatch through cartography. ~50 LOC.
6. **First minimap consumer** — workbench swatch thumbnails. Same cartography path, `FormFactor::Minimap`.
7. **Intelligence-embeddings wire-in** — when `intelligence-embeddings` produces real cluster / affinity output, wire signal types into `IntelligenceSignals` and add `ForceDirectedByMeaning` as a `StreamingLayoutStrategy`-shaped variant of the existing `SemanticEmbedding`.

**Sub-task identified during step 2**: the `mere-kernel → graph-canvas` dependency is technical-debt-shaped — kernel pulls in graph-canvas only to reference `Stroke` in `OverlayStrokePass`. Resolving this (either by moving `OverlayStrokePass` downstream or by inlining `Stroke` into the kernel) is a prerequisite for step 4. ~~Until then, the feature-gated adapter location in cartography is the right unblock.~~

**Resolved 2026-05-11**: `Color` and `Stroke` moved from `graph_canvas::packet` into `mere_kernel::paint`. graph-canvas re-exports them from mere-kernel (`pub use mere_kernel::paint::{Color, Stroke}`) so the public API stays stable; downstream consumers (mere-host-contract, mere-host, mere-domain/graphshell, cartography adapters) keep working unchanged. Dependency direction flipped: `graph-canvas → mere-kernel` (correct — kernel is foundational; renderer depends on it), `mere-kernel → graph-canvas` is gone. The cartography `graph-canvas-adapters` feature flag remains useful as an opt-in for consumers that don't want graph-canvas in their dependency closure, but it's no longer needed to avoid a cycle. Step 4 can now lift the layout subsystem into a `graph-layout` sibling crate when ready.

The big change from the original sequence: **step 2 is no longer "move ForceDirected" — it's "adopt cartography contracts on top of every existing strategy where it lives."** The actual file move (step 4) is a separate concern that can wait for the contract layer to prove itself in place.

This is meaningfully smaller per-slice work than the original sequence suggested, because no code moves until the contracts have validated value with the existing code in its current home. It's also higher-confidence: each adapter slice is a few-dozen-LOC change, fully testable against existing tests, with zero risk to existing call sites.

No deadline — sequenced by which surface needs which strategy first.

## 10. What this brief does NOT decide

- Concrete strategy algorithms (force-directed parameter choices, cluster-detection algorithm, etc.).
- GPU vs. CPU layout — strategies pick that internally; cartography is agnostic.
- The exact `IntelligenceSignals` field set — grows with what `intelligence-embeddings` actually produces.
- Whether `document-layout` is a separate crate or a module inside `document-canvas` — that decision is local to document-canvas and can wait until a real document-minimap consumer appears.
- A formal taxonomy of `FormFactor` variants — sketch above is illustrative; final set lands with first implementation.

The point of this brief is to plant the architectural flag and reclaim the name. Concrete shapes follow when implementation starts.
