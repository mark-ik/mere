# Understory at the Orrery Graduation — Evaluation Brief

**Date**: 2026-05-27
**Status**: Design probe / decision framework. Not a commitment to a dependency; records *how and when* to evaluate [forest-rs/understory](https://github.com/forest-rs/understory) against Mere's graph-canvas/projection layer, and what was already decided about it.

> **Crate-name note (2026-06-09 audit):** the comparison premise shifted: `graph-canvas` has since **dissolved** into the `orrery/*` family (so "rebuild graph-canvas vs understory" is partly OBE), `mere/app/src/camera.rs`→`orrery/arrangements` camera, and the host is `meerkat` (genet-as-host, not Masonry). The understory-evaluation shape (camera boundary, hit-test/cull seam, responder/focus) still applies to the orrery element.
**Related**: [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md), [component fit-map](../technical_architecture/2026-05-26_component_fit_map.md), [renderer registry contract](2026-05-15_renderer_registry_contract_brief.md), [spatial-chrome adoption plan](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md), [donor docs full harvest](2026-05-27_graphshell_docs_full_harvest.md) (its §1/§4 focus/event/UxTree + canvas specs are the yardsticks below).

---

## What understory is, in one line

~18 `no_std`, Kurbo-based, minimal-dependency crates providing the *spatial/scene data-structure layer* under a UI — a three-tree split of widget tree (state/interaction) / box tree (geometry + spatial index) / presentation tree (resolved draw intent). Explicitly not a layout engine, renderer, or widget toolkit. MIT/Apache; active but young (stability is a stated goal, not yet proven).

## Stance (decided)

**Parts shelf and boundary model, not a new spine.** Mere already has its product ontology — `kernel → forme → platen → verso → inker → host`. Understory must not become "Mere's UI substrate." Its value is narrower and real: sharpen the portable scene/projection layer, and keep geometry, hit-testing, event routing, and presentation caches from leaking into host widgets. The trap to avoid is adopting the whole family because the boundaries feel clean.

Borrowing *boundaries and shapes* rather than *pinning crates* is also the hedge against the project's youth: we get the design validation and the boundary clarity without betting the workspace on a small dependency's trajectory.

## Per-crate verdict

Borrow-modes: **steal the shape** (copy the API boundary, no dep) · **probe the crate** (clone, evaluate as a real dep) · **validate only** (it confirms our boundary; borrow nothing) · **no**.

| understory crate | Mere home | Mode | Note |
|---|---|---|---|
| `understory_index` (AABB index; FlatVec/R-tree/BVH) | graph-canvas hit-test + cull | **probe the crate** | Genuinely hard to hand-roll well; but contingent on the rapier question below. Pluggable backend is the right shape for a settling sim. |
| `understory_box_tree` (world-space AABB sync) | graph-canvas, downstream of cartography bounds | **probe the crate** | Same contingency. Consumes positions; never owns them. |
| `understory_view2d` (pan/zoom, world↔view, fit, clamp) | `mere/app/src/camera.rs` | **steal the shape** | The polished version of the hand-rolled camera. Take the boundary, keep Mere's canvas navigation defaults (wheel=pan, ctrl+wheel=zoom, middle-drag=pan, inertia; infinite-canvas doc, not webpage). |
| `understory_responder` (capture→target→bubble) · `understory_focus` · `understory_event_state` | in-canvas interaction routing (graph nodes inside the GraphCanvas widget); verso | **steal the shape** | Align with the harvest's nine-region focus + semantic-event specs. Must reconcile with Masonry's event/focus so there are not two routing systems (rule: Masonry routes the chrome widget tree; this routes the scene *inside* the canvas widget). |
| `understory_outline` (hierarchical visible-row projection) | Navigator / orrery tree rows (`forme` uxtree) | **steal the shape** | Tree-view projection boundary. |
| `understory_node_graph` (graph doc + projection + session state + computed geometry) | — | **validate only** | Its truth/projection/session/geometry split *validates* Mere's truth/arrangement/projection/view-intent boundary, but `GraphDoc` is too editor-graph-shaped to replace `kernel`/`forme`. Caveat: `forme` (arrangement) has no clean node_graph analog — node_graph is truth+projection-shaped — so don't let its tidy split tempt collapsing `forme` into projection. |
| `understory_selection`, `understory_virtual_list` | selection container; long history/Navigator lists | **steal the shape** (virtual_list: probe if a list gets large) | Small, peripheral. |
| `understory_presentation` (retained resolved draw tree) | — | **no** (unless a concrete renderer path pulls) | Firmest decline of the family. The spine's single projection path is `forme → platen → verso → inker → host` over a vello scene; a second retained presentation tree duplicates it — the exact two-layers-doing-the-same-job problem the fit-map is retiring. |

## The rapier question (the operational test the ontology defers)

The per-crate verdict says *where* `index`/`box_tree` belong — downstream of cartography's computed world bounds. That framing is the **settled-layout** regime. The orrery's live phase is different: `gyre` (rapier, at `graphshell/graph/gyre`) moves every node every frame until damping/`auto_pause` settles it (the motion presets now live in the just-pulled `register-lens`). Two questions decide whether a spatial index earns its place there, and neither is answerable by layering analysis:

1. **Churn regime.** Does a box-tree/R-tree survive per-frame AABB resync, or do we run a flat/grid index while hot and only build the tree on `auto_pause`? understory's pluggable backends + `commit()` batching + coarse damage tracking are designed for exactly this split; the probe confirms the cost is acceptable.
2. **Redundancy with rapier.** Node-node separation is modelled as collision (`node_separation` in the lens presets), so nodes are already rapier colliders — meaning rapier's `QueryPipeline` already does point/AABB hit-testing for nodes. If so, `understory_index` earns its keep only for the *non-collider* visuals (edges, labels, glyph overlays) and for frustum culling, not for node hit-testing. The alternative is to make understory the single index for *all* visuals (nodes included) to avoid "is this a collider?" branching at every query site.

**The spike**: a hit-test/cull bake-off — rapier `QueryPipeline` vs `understory_index` (R-tree and FlatVec backends) — over a live physics layout, measured against the harvest's `canvas_behavior_contract` readability/scenario metrics. Two seams to confirm cheap: the f32-nalgebra (rapier) → f64-Kurbo (understory) per-frame convert (already a seam Mere straddles), and the hot-phase index cost. **Done when** we can state, with numbers, whether understory_index (a) beats rapier's pipeline as a single all-visuals index, (b) layers cleanly above it (rapier owns node bodies, understory owns the rest), or (c) is not worth the seam during the hot phase.

## Trigger and comparison

Run this comparison **when the orrery graduates** from the hand-rolled `mere/app/src/camera.rs` + `graph_canvas.rs` widget — the open reconciliation the fit-map already names (the ~24 KB hand-rolled host widget vs the 9.6k `graph-canvas` crate). At that point, before committing to either rebuilding the `graph-canvas` crate or hardening the hand-rolled widget, compare `graph-canvas` against understory's `node_graph + view2d + box_tree + responder` shape.

The expected outcome is not adoption of understory wholesale: `graph-canvas` keeps its Mere-specific scene packets, physics binding, projection presets, and graph semantics, and **borrows understory's sharper internal layering** (the camera boundary, the box-tree/index hit-test/cull seam if the rapier spike favours it, the responder/focus routing shape) so those concerns stop leaking into host widgets.

## What is NOT in scope

Understory does not touch `kernel`/`forme` truth or arrangement, the inker engine layer, comms/federation, or the projection path's presentation stage. This is strictly about the portable scene/interaction layer beneath the graph canvas.
