# Swatch Primitive Plan — one configurable, embeddable projection of the graph

**Date**: 2026-06-27
**Status**: Planning, no code yet. Sequence: **primitive-first** (Mark, 2026-06-27):
P1 extract the generic component, P2 the connections swatch, P3 the classifier +
strip, then the edge enrichment (P4/P5), the gloss migration (P6), and templates
(P7).
**The model is canonical, do not re-derive it.** This plan builds
[the swatch primitive design](../design/2026-06-27_swatch_primitive_design.md): the
swatch as the fourth (canvas) graph primitive, the orrery as its root instance, two
planes (truth vs per-instance curation), the pipeline, the element-model /
rasterization split, cells-as-edges, the visibility stack, and templates. Read it
first; this plan is the build, not the argument.
**Lane / conflict posture**: meerkat (`swatch.rs`, `render/cards.rs`,
`window_view`), orrery + gyre (edge render, springs), graph-kernel (the GraphDefault
visibility base), and cartography (the geometry provider, already shaped for this).
This plan owns the net-new **spine** and **coordinates** the sibling plans that own
each existing fragment; it extends them, it does not duplicate them.
**Relationship to existing plans** (coordinate, do not restate):
- [node body & face model plan](2026-06-23_node_body_face_model_plan.md) **owns
  `swatch.rs`** (the B3 body designer). P1 is the generalization B3 already names
  ("generalizing over the host state is the reuse step", `swatch.rs:18`).
- [object card plan](2026-06-21_object_card_plan.md) **owns the focus-card slot**.
  P2 adds the slot's multi-selection branch (`render/cards.rs:129` TODO).
- [graphlet wiring plan](2026-06-25_graphlet_wiring_plan.md) **owns the per-window
  instance machinery + graphlet derivation/reconcile**. P3 crystallize reuses it.
- [petgraph / RDF plan](2026-06-18_petgraph_rdf_plan.md) **owns edge multigraph
  storage** and the ruling that visual collapse is an experience-LOD setting. P4 is
  that experience-LOD render; it needs no kernel storage change.
- [graph signals layer plan](2026-06-22_graph_signals_layer_plan.md) **owns the
  gloss lens / its own `ProjectionRequest`**. P6 migrates the minimap render onto
  the DOM swatch under it.
- [scriptable field regions plan](2026-06-13_scriptable_field_regions_plan.md) owns
  field-region edge visibility, which P5 resolves into the visibility stack.

---

## Thesis

The shell already holds the swatch's pieces, scattered: a single-node DOM editor
(`swatch.rs`), a Scene-based gloss minimap (`render/paint.rs`), a DOM object card in
the orrery's focus-card slot (`FocusCardKind::ObjectCard`), per-window instance
isolation (`WindowView`), and a projection layer literally shaped "graph truth in,
canvas swatches out" (cartography). What is missing is the one configurable component
those are all facets of, plus the two capabilities it surfaces as gaps: a multi-node
(selection) swatch, and a real edge-control surface. This plan assembles the
component and closes both gaps, primitive-first so the visible derivation UX lands
before the deeper edge work.

---

## Phases (each independently landable; done conditions, not dates)

### P1 — Extract the generic swatch component (the keystone)

Lift `swatch_view` (`swatch.rs:56`) off `SettingsPanesView` / `SettingsPanesState`
to a host-generic view. The DOM-building is portable (positioned divs, an `<img>`, a
clip-path polygon, the `node-swatch` + `data-subject` hit-test contract at
`swatch.rs:107-119`); the lift is parameterizing the host-state type and introducing
the **element model** (the list of semantic placed elements with `data-*` node /
edge / vertex ids) as the render output. Add the `SwatchInstance` config (design §3)
and a geometry provider keyed by scope: `Scope::Node` is the degenerate single-node
face/hull (today's path); `Scope::Selection / Graphlet / Graph` route to cartography
`project()`. The node-body editor becomes **template #1** riding the generic
component, behavior-preserved (the hull-vertex drag at `input/editing.rs:208` keeps
working).

**Done when**: the node editor renders and edits through the generic swatch
component with no behavior change; the component takes a `Scope` + a `SwatchInstance`
config and emits the element model; the facet pane is one embedder of it; files
under the 600-LOC ceiling.

### P2 — Connections swatch (scope = Selection)

Wire the focus-card slot's multi-selection branch (`render/cards.rs:129` TODO,
gated `selected_members().len() > 1`). Issue a selection-scoped `ProjectionRequest`
to cartography `project()`, take the `Projection` geometry (the selected nodes plus
their inter-edges), build the element model, render as DOM (nodes as glyphs, edges
as family-coloured lines). It rides the focus-card slot socket (`compute_focus_cards`
+ the `FocusCardKind` dispatch in `render/setup.rs`) as a new `FocusCardKind` or a
generalization of `ObjectCard`.

**Done when**: selecting more than one node summons a DOM connections swatch in the
focus-card slot showing the selected nodes and their inter-edges, family-coloured and
hit-testable; a single-node selection still summons the snapshot card.

### P3 — Shape classifier + the strip

Build the **shape classifier**, the 2026-06-13 doc's named "one real gap": the nine
detectors (Ego / Corridor / Component / Loop / Frontier / Facet / Bridge /
WorkbenchCorrespondence / Session) over the induced subgraph under a chosen edge
projection, ranked by fit and edge strength. The strip in the connections swatch
reads it: ranked chips, the projection toggle (the instance's `EdgeProjection`,
re-deriving the shape live), frontier ghosts, crystallize. Crystallize reuses the
built graphlet machinery (`graphlets.rs` `record_linked` / `record_branch`): Session
is ephemeral, Linked is drift-tracking. This is the derivation-from-selection UX
routed through the swatch (reconciliation ruling 4), and the `SelectionShapeEdit`
edit strategy (design §6).

**Done when**: the connections swatch names the dominant shape (all-families default,
dominant pre-ranked, per 2026-06-13), re-derives it live as the projection toggles,
ghosts the one-hop frontier, and crystallizes the selection to a Session or Linked
graphlet through the existing index.

### P4 — Cells-as-edges (Mark, 2026-06-27)

Un-collapse the orrery edge render. The "one undirected line per pair"
(`orrery/.../build.rs:91`) and the collapsed edge list (`frame.rs:77`) become
per-cell: each populated `(family, sub_kind)` cell between a pair draws as its own
fanned, family-coloured line. The gyre `edge_hit_test` (`gyre/.../query.rs`) resolves
to the specific cell `(source, target, RelationSelector)`, which is the per-edge
selection graphlet-wiring open item #1 deferred. Thickness becomes per-cell (the
cell's own metric) rather than per-pair density. No kernel storage change: the cells
already exist in `EdgePayload`'s family sidecars (`edge_payload.rs`); this is the
experience-LOD render the petgraph-RDF plan assigns here. Benefits the orrery (root
swatch) and the connections swatch alike, since both render edges through the P1
element model.

**Done when**: each relation between a pair draws and selects as its own edge; edge
thickness is per-cell; selecting an edge resolves to its `RelationSelector`; the
orrery and the connections swatch share the per-cell edge render.

### P5 — Edge visibility: the default plus non-propagating override stack

Build the visibility curation (design §8): `GraphDefault` (a graph-native base, the
"hidden in the graph" layer) under `GraphViewOverride` (per-instance) under
`SelectionOverride` (per-selection), forme's precedence reused, where a higher layer
wins and never writes down into a lower one. **Hiding relaxes the spring in that
instance only** (Mark, 2026-06-27): feed the instance's effective visibility into the
gyre spring set (spring set = visible cells intersected with projection); the spring
and drawn-edge list already rebuild on reconcile (`orrery/.../selection.rs:95`).
Membership stays on truth: derivation follows projection over real edges, so a hide
never drops a node out of a graphlet. The selected-cell edge controls (show / hide /
relate / retract) live in the strip. Field-region visibility
(scriptable-field-regions) resolves into this stack as another contributor.

**Done when**: hiding a cell in one instance relaxes its spring there and nowhere
else; the cell stays drawn and live in every other instance, in a selection, and in
a Linked graphlet; a `GraphDefault` hide applies as the base everywhere; graphlet
membership is unaffected by any hide.

### P6 — Migrate the gloss minimap onto the DOM swatch

Replace the Scene-based gloss render (`render/paint.rs:278`, via
`Orrery::gloss_geometry`) with the swatch primitive at `scope = Graph,
layout = minimap`, rendering the cartography `Projection` as the element model, with
the Scene fill as the **dense-LOD backend** (design §5), not a separate path.
Click-to-focus and theming preserved. This proves the primitive scales to the root
and that the element-model / rasterization split holds at whole-graph density.

**Done when**: the gloss minimap renders through the swatch component; the Scene
fill is the high-density LOD backend under the element model, not a parallel render;
click-to-focus and theme sourcing are unchanged.

### P7 — Templates

A template is a named preset over `(scope, layout, lens, projection, mode)` plus a
**lock** and applicability **conditions** (design §9). Hold the registry in
`PersonaSettings` (`personas/<id>/settings/`, the per-persona UI-curation pattern the
command-registry plan uses), so a persona carries its swatch library across graphs.
Lock is config-lock, user-toggleable, with **reset to the template default** as the
guaranteed way back to the context's tool; parametric (point it at a target) versus
bound (carries its own subgraph) scope binding. The three unlocked-swatch verbs:
copy the orrery's current config, reset to default, apply a saved template. Seed the
built-ins: node editor (locked, parametric), minimap (unlocked), connections (the
selection preset).

**Done when**: a swatch instantiates a named template and defaults to its config;
lock toggles, with reset-to-default always available; the three verbs work on
unlocked swatches; templates persist per persona across graphs; a locked / bound
template renders a purpose-built UI over its own subgraph.

### Non-goals (named, deferred)

- **True parallel edge instances** (two distinct `Cites` between one pair with
  separate metadata): the petgraph-RDF plan's multigraph storage, not this plan.
- **The djot-block and menu embeddings** of the swatch: later consumers once the
  component exists.
- **The research-surface lenses** (Trail / Claim / Provenance / Neighborhoods):
  graph-projections research; this plan ships the lens slot, not those lenses.
- **The window-per-graphlet to scope-nav reform** (reconciliation ruling 1): related
  but separately greenlit; tracked in the scope reconciliation.

---

## Findings (verified against the code this session)

- **Cartography is already the geometry provider.** Its description: contracts
  "between graph truth + intelligence signals on the input side and **canvas swatches
  on the output side**," with `LayoutStrategy::project(&ProjectionRequest)
  -> Projection` (`cartography/src/strategy.rs:47`) and the `Projection` /
  `MinimapDescriptor` vocabulary. The connections swatch issues a selection-scoped
  `ProjectionRequest`; the gap is rendering `Projection` as DOM (today the minimap
  renders it as a Scene).
- **The `swatch.rs` lift is a type-parameterization, not a rewrite.** `swatch_view`
  returns `SettingsPanesView` over `SettingsPanesState` (`swatch.rs:56`), but the
  element-building (divs, `<img>`, clip-path, `node-swatch` + `data-subject`) is
  portable.
- **The focus-card slot is the ready socket.** `FocusCardKind` is `Snapshot /
  Unvisited / ObjectCard { widgets }` (`window_view/mod.rs:473`); the multi-select
  branch is the marked `TODO(swatch agent)` at `render/cards.rs:129`, gated today on
  `selected_members().len() == 1`. The live-preview card was retired (`d4375d0`).
- **The edge un-collapse site is real.** One `EdgePayload` per pair with a family
  sidecar holding a set of sub-kinds (`edge_payload.rs:31`); each cell is already
  addressable (`retract_relation(RelationSelector::Semantic(Cites))`). The collapse
  is the render (`build.rs:91`, `frame.rs:77`), not the storage.
- **No edge visibility state exists.** Visibility covers ghost nodes and facet scope,
  never edges; the GraphDefault base is net-new.
- **The instance machinery exists for branches.** `WindowView` carries per-graph
  selection, camera, and scope, isolated per window (graphlet-wiring open item #1),
  the substrate the swatch-as-instance generalizes.
- **The shape classifier does not exist.** A workspace search finds no kind dispatch
  / induced-subgraph fit ranking; forward derivation (kind -> members) is built
  (`graphlets.rs`), the inverse (selection -> kind) is the P3 gap.

---

## Open questions

- **Element model shape.** Whether the element model is a new cartography output type
  or an adapter over the existing `Projection` + `MinimapDescriptor`. Lean: adapt,
  since cartography already produces the geometry.
- **`FocusCardKind` vs a general embedder.** Whether the connections swatch is a new
  `FocusCardKind` or whether the slot is refactored to host any swatch. Lean: a new
  kind in P2, refactor to a general embedder when the djot/menu consumers land.
- **Conditions vocabulary** for templates (design §11 carries this).
- **Per-cell fan geometry** legibility before a pair needs its own collapse/expand
  affordance (design §11).
- **Edit-layer reach**: `GraphArrangeEdit` versus the orrery's existing direct
  manipulation, subsume or wrap (design §11).
- **Classifier ranking strength**: a unified strength number across traversal counts
  / semantic decay / arrangement durability; likely a setting (2026-06-13 +
  reconciliation open question).

---

## Progress

- **2026-06-27** — Plan created from the swatch-primitive design session (Mark +
  Claude). Sequence set primitive-first (Mark): P1 extract the generic component ->
  P2 connections swatch -> P3 classifier + strip -> P4 cells-as-edges -> P5
  visibility stack -> P6 gloss migration -> P7 templates. Seams verified against the
  code this session (file:line in Findings): cartography already the geometry
  provider, the `swatch.rs` lift a type-parameterization, the focus-card slot the
  ready socket, the edge un-collapse a render change with no storage edit, no edge
  visibility state yet, the classifier the one genuinely-new piece. Ownership split
  recorded: this plan owns the spine; node-body-face B3, object-card, graphlet-wiring,
  petgraph-RDF, graph-signals, and field-regions own the fragments it coordinates. No
  code yet.
