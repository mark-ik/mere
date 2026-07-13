# Swatch Primitive Plan — one configurable, embeddable projection of the graph

**Date**: 2026-06-27
**Status (2026-07-01)**: P1, P2, and P3a/b/c landed — see Progress below. P2b
(cartography re-layout, `Scope`/`SwatchInstance` unification) and the rest of P3's
done condition (live projection toggle, frontier ghosts, contextual detectors,
Linked/Astroid crystallize, chip-click crystallize) are deferred slices, still open.
P4/P5 now have further downstream slices through the roster/orrery relation-cell
work: 2026-07-01 closed the "gyre topology and springs remain endpoint-pair
scoped" gap (springs are now per-visible-relation-cell — see the [roster detail
cards plan](2026-06-29_graph_object_roster_detail_cards_plan.md)'s 2026-07-01
entry), but P4/P5's full done conditions (per-cell edge *thickness*, one shared
element-model edge renderer between the orrery and the connections swatch, and
the P5 `GraphDefault < GraphViewOverride < SelectionOverride` layered stack)
remain open. **P6 did not land as scoped** (see Progress 2026-07-05): the gloss
minimap migrated Scene->DOM through a separate, parallel implementation, not
this plan's component; P7 remains unstarted. Sequence:
**primitive-first** (Mark, 2026-06-27): P1 extract the
generic component, P2 the connections swatch, P3 the classifier + strip, then the
edge enrichment (P4/P5), the gloss migration (P6), and templates (P7).
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
- [graphlet wiring plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md) **owns the per-window
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

**Status: done 2026-06-27** (the host-generic lift). `swatch_view` is now
`swatch_view<S: 'static>(spec) -> SwatchView<S>` (`SwatchView<S> = Box<dyn AnyView<S,
(), GenetCtx, GenetElement>>`), generic over the embedder state because the swatch
carries no state-bound callbacks (interaction routes through the host hit-test on
`data-subject`). The facet pane mounts it as `SwatchView<SettingsPanesState>` with no
call-site change (inference), behavior-preserved; `cargo check -p meerkat --lib`
green, no new warnings. **Right-sized**: the `Scope` enum, the `SwatchInstance`
config, and the explicit element-model layer move to **P2**, where the connections
swatch is their first multi-scope consumer; landing them in P1 with only the
node-editor consumer would be unused scaffolding. The keystone P1 ships is the
host-generic component itself.

### P2 — Connections swatch (scope = Selection)

Wire the focus-card slot's multi-selection branch (`render/cards.rs:129` TODO,
gated `selected_members().len() > 1`). Issue a selection-scoped `ProjectionRequest`
to cartography `project()`, take the `Projection` geometry (the selected nodes plus
their inter-edges), build the element model, render as DOM (nodes as glyphs, edges
as family-coloured lines). It rides the focus-card slot socket (`compute_focus_cards`
+ the `FocusCardKind` dispatch in `render/setup.rs`) as a new `FocusCardKind` or a
generalization of `ObjectCard`. This phase also introduces the `Scope` enum, the
`SwatchInstance` config (design §3), and the explicit element-model layer, since the
second scope is their first real consumer (folded down from P1).

**Done when**: selecting more than one node summons a DOM connections swatch in the
focus-card slot showing the selected nodes and their inter-edges, family-coloured and
hit-testable; a single-node selection still summons the snapshot card.

**Status: built + headed-verified 2026-06-28.** `compute_focus_card` (setup.rs)
routes `selected_members().len() > 1` to a new `compute_connections_card`
(`render/connections.rs`), which reads the selected nodes' kernel positions + their inter-edges
(`graph.relations()`, family-coloured) and normalizes them through the pure, unit-tested
`swatch::connections_spec_from`. A new `FocusCardKind::Connections { spec }` renders via
`swatch::connections_swatch_view::<ShellState>` (the P1 host-generic lift now paying off:
the same DOM swatch mounts in the focus-card slot, not just the facet pane). Edges draw as
transform-free dotted lines (genet CSS has no `transform`); nodes carry `data-element` tags
for the P4 hit-test. Lands entirely in clean files (not the concurrently-edited `cards.rs`).
Bin compiles, 2 `connections_spec` unit tests green. **Deferred to P2b**: the cartography
*re-layout* (today it crops the live arrangement's positions, not a relation-driven layout),
the `Scope` / `SwatchInstance` unification (two render fns for now: node + connections), and
the actual hit-test *routing* (tags present, routing is P4). **Headed-verified 2026-06-28**: a
marquee multi-select over a connected cluster summoned the connections swatch in the focus-card
slot, a ~240x240 DOM card with the selected nodes as dots and a dotted-line family-coloured
inter-edge (`scry-shots/conn-07-selected.png` + `conn-07-crop.png`). Multi-select gestures: the
**marquee** (rect-select, replaces) and **Shift-click** (toggles a node in/out, additive) — both
already built (orrery `pointer_up` input.rs:209, host `set_shift` handler_window.rs:132). The
drive's Ctrl-click was the wrong modifier, not a missing feature.

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

**Status (2026-06-30): P3a/b/c done**, full done condition still open. P3a (2026-06-28, the
classifier keystone + the shape chip): `graphlet_classifier::classify(n, edges) -> ranked
[ShapeRank]`, pure topology over the selection's induced subgraph: structural detectors for
**Loop / Ego / Corridor / Component**, with **Loose / Session** the disconnected-grab-bag floor,
ranked by fit (6 unit tests green). P3b (2026-06-28, crystallize via context menu):
`Shell::crystallize_selection` freezes the selection as a Session graphlet tagged with the
classifier's dominant shape (`session_ops/shell_session.rs`, `graphlets.rs` `record_session`).
P3c (2026-06-28, the chip strip): `compute_connections_card` sets the classifier's top-3
fit-ranked shapes (fit ≥ 0.4) as `ConnectionsSpec.shape_chips`, rendered as a ranked chip strip
(dominant highlighted, runners-up dimmed) by `connections_swatch_view`. **Still open** (the
done-condition's remaining clauses): the live projection toggle (re-derive as families toggle,
needs the swatch hit-test), frontier ghosts, **Linked-derivation crystallize** (Component/Ego)
and the Astroid option, chip-click crystallize, and the contextual detectors (Bridge / Facet /
Frontier / WorkbenchCorrespondence). Bin compiles; the chip render is not yet headed-verified (a
low-risk text div over the verified P2 swatch; the app's pre-existing sqlx/sync crash makes a
drive flaky).

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

**Status (2026-06-30/2026-07-01)**: The first two clauses are done — the
canvas fans and picks per-cell (2026-06-30 pass) and selecting a cell resolves
to its `RelationSelector`. Still open: edge thickness is not yet per-cell
(uniform per fan lane), and the orrery + connections swatch still render edges
through two separate paths, not one shared element-model renderer.

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

**Status (2026-07-01)**: The named "hiding relaxes the spring in that instance
only" behavior is built — `orrery::build::visible_relation_edges` feeds the
instance's `hidden_edges` into the gyre spring sync, and every hide/show
mutator re-syncs immediately (see the [roster detail cards
plan](2026-06-29_graph_object_roster_detail_cards_plan.md)'s 2026-07-01 entry).
Graphlet membership is confirmed unaffected (hide/show never touches
`derive_members`/graphlet truth). **Not built**: the `GraphDefault <
GraphViewOverride < SelectionOverride` layered stack itself — hide/show is
still the one session-scoped layer it always was, just now spring-aware; there
is no graph-level default layer yet.

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

**Status (2026-07-05): shipped differently, not through this component — see
Progress.** The [gloss_scene_to_dom_migration_plan](../../archive_docs/2026-07-04_completed_plans/2026-07-01_gloss_scene_to_dom_migration_plan.md)
(archived, complete, headed-verified 2026-07-01/02) converted the minimap to
DOM node squares + an embedded-Scene edges backdrop, but via its own bespoke
`gloss_view.rs`, not `swatch_view<S>`/`Scope::Graph`. Reconciled and accepted
as a deliberate divergence, not an open gap to close later — see Progress for
the reasoning. This phase's done-condition as literally written will not be
pursued unless a third swatch consumer emerges.

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
- **2026-06-27** — **P1 done (host-generic lift)**. `swatch.rs`'s `swatch_view`
  lifted off `SettingsPanesView` / `SettingsPanesState` to `swatch_view<S>(spec) ->
  SwatchView<S>`, a single-file behavior-preserving change in a clean file (the
  concurrent knot-note / observability work touched a different region of `cards.rs`,
  ~line 293, no collision). `cargo check -p meerkat --lib` green, no new warnings; the
  facet pane embeds it with zero call-site change (`S` inferred). Right-sized P1 to the
  keystone: the `Scope` / `SwatchInstance` / element-model abstractions move to P2 with
  their first multi-scope consumer rather than landing as unused scaffolding now. Next:
  P2 (connections swatch) introduces them and mounts the same `swatch_view` in the
  focus-card slot.
- **2026-06-28** — **P2 built (connections swatch), pending headed verify.** Multi-selection
  (`len > 1`) now summons a DOM connections swatch in the focus-card slot: `compute_focus_card`
  → `compute_connections_card` (new `render/connections.rs`) → `connections_spec_from` (pure,
  normalizes the selected nodes' kernel positions + inter-edges into the card) →
  `FocusCardKind::Connections` → `connections_swatch_view::<ShellState>` (the P1 lift mounting in
  the slot). Edges are transform-free dotted lines (no genet CSS `transform`), family-coloured;
  nodes tagged `data-element` for P4. Six clean files touched, **not** the concurrently-edited
  `cards.rs` (whose `len == 1` snapshot gate already suppresses multi-select; its TODO updated to
  point here). 2 unit tests green. **Verification lesson**: `swatch` / `render` / `window_view` /
  `graphlets` are **bin** modules (declared in `main.rs`), so `cargo check -p meerkat --lib` does
  **not** compile them — verify these with `cargo test -p meerkat --bin meerkat` (or
  `cargo check -p meerkat`); the `--lib` greens on P1 + P2 were false-clean until the bin build
  caught a `u32` arg type. P2b deferrals: cartography re-layout, the `Scope`/`SwatchInstance`
  unification, hit-test routing. Next: a headed drive, then P3 (classifier + strip).
- **2026-06-28** — **P2 headed-verified**. Drove `target/debug/meerkat.exe` (scry-shots harness):
  paused the sim, panned to the static graph, marquee-selected a connected cluster — the
  connections swatch appeared in the focus-card slot as a ~240x240 DOM card with the selected
  nodes as dots and a dotted-line family-coloured inter-edge (`scry-shots/conn-07-selected.png`,
  `conn-07-crop.png`). Multi-select gestures = **marquee** (replaces) + **Shift-click** (toggles,
  additive) — both already built (orrery `pointer_up` shift-toggle + host `set_shift`); the drive's
  Ctrl-click was the wrong modifier, not a missing feature. P2 done. Next: P3 (shape classifier +
  strip).
- **2026-06-28** — **P3a built (shape classifier + chip)**. New `graphlet_classifier.rs` (bin
  module): pure `classify(n, edges) -> ranked [ShapeRank]` with structural detectors (Loop / Ego /
  Corridor / Component) over a selection's induced subgraph, Loose / Session the disconnected floor,
  ranked by fit. 6 unit tests green (path→Corridor, star→Ego, triangle→Loop, K4→Component,
  disconnected→Session, 2-node→Corridor). Wired into the connections swatch: `compute_connections_card`
  classifies the selected subgraph and sets `ConnectionsSpec.shape_label`, which `connections_swatch_view`
  renders as a top-left chip. Net-new logic = the 2026-06-13 "one real gap"; the field-as-scope grab-bag
  Mark raised falls out as the Loose case (recorded in the design doc). Also captured the field-as-scope
  idea in the design doc open questions. Deferred to P3b/c: the multi-chip strip, projection toggle,
  frontier ghosts, crystallize, and the contextual detectors. Bin compiles; chip not yet headed-verified.
- **2026-06-28** — **P3b built (crystallize via context menu)**. The commit gesture: a multi-node
  "Crystallize selection" context-menu item freezes the selection as a Session graphlet tagged with
  the classifier's dominant shape and scopes the orrery to it in place (ruling 1, not a new window).
  Primitives (clean files, unit-tested): `graphlet_classifier::classify_selection(graph, members)`
  (graph-aware classifier, now also backs the swatch chip) + `SessionGraphlets::record_session(kind,
  members)` (freeze-the-selection index method — works for any shape incl. the Loose grab-bag, unlike
  Linked derivation). Host method `Shell::crystallize_selection(from)` (`session_ops/shell_session.rs`):
  classify → record_session → persist → `scope_to_members`. Trigger mirrors "Open component": new
  `ContextAction::CrystallizeSelection` + registry tuple + `DEFAULT_MENU_ACTIONS` + `MenuScope::MultiNode`
  (command.rs) + menu row (build.rs) + dispatch pushing `ShellCommand::CrystallizeSelection` (actions.rs)
  → drain (shell_ops.rs) → the Shell method. Threaded the two concurrently-edited hot files cleanly. Bin
  compiles, 14 graphlet tests green. **Default = Session-freeze** (the 2026-06-13 default, any shape);
  Linked-derivation crystallize (Component/Ego) + the Astroid option are P3c. Not headed-verified (the
  app's pre-existing sqlx/sync crash makes a drive flaky). **P3 substantially done**; remaining P3c/P4:
  the live projection toggle, frontier ghosts, the contextual detectors, and the swatch hit-test
  (chip-click crystallize).
- **2026-06-28** — **P3c slice: the chip strip**. `ConnectionsSpec.shape_label` (single dominant) →
  `shape_chips: Vec<String>`: `compute_connections_card` now sets the classifier's top-3 fit-ranked
  shapes (fit ≥ 0.4), and `connections_swatch_view` renders them as a chip strip across the top of the
  card (dominant highlighted, runners-up dimmed). Completes the "strip" half of P3's name as a
  read-only ranked display; the live projection toggle that re-derives it is still P4 (needs the swatch
  hit-test). Bin compiles, 14 graphlet tests green.
- **2026-06-30 - P4/P5 partial via roster/orrery relation-cell work.** The
  [graph object roster detail-cards plan](2026-06-29_graph_object_roster_detail_cards_plan.md)
  landed fanned canvas relation-cell overlay/picking, selected-cell redraw, Link
  Card relation-row hide/show, per-cell session visibility keyed by `(source,
  target, RelationKind)`, and connections-swatch filtering/routing for visible
  cells. This advances P4/P5 but does not complete them: gyre topology and
  springs remain endpoint-pair scoped, edge thickness is not yet fully
  per-cell, the orrery and connections swatch do not yet share one element-model
  edge renderer, and the P5 `GraphDefault < GraphViewOverride <
  SelectionOverride` stack is still unbuilt.
- **2026-07-01 - Gyre spring topology closed the "endpoint-pair scoped" gap.**
  The roster detail-cards plan's 2026-07-01 pass replaced the pair-deduped
  `dedup_edges` spring feed with `visible_relation_edges`: one spring tuple per
  **visible** relation cell (multiplicity, not a single collapsed pair edge),
  and every hide/show mutator now re-syncs the spring set immediately instead
  of waiting for an unrelated reconcile. This is exactly the "hiding relaxes
  the spring in that instance only" behavior P5 named. `gyre`'s own edge type
  is untouched by design (still `(NodeKey, NodeKey)`, taxonomy-agnostic);
  multiplicity is how the orrery hands it weight. Graphlet family-selector
  editing also landed (Graphlet Card chips, `SessionGraphlets::
  toggle_family_selector`), separate from this P4/P5 edge work but closing the
  roster plan's other named R6 item. Still open for P4/P5: per-cell edge
  thickness, one shared element-model edge renderer between the orrery and the
  connections swatch, and the full `GraphDefault < GraphViewOverride <
  SelectionOverride` stack (today there is still only the one session-scoped
  visibility layer, now spring-aware). Not headed-verified — the app's window
  rendered at a stuck 13x13px in this session's environment; see the roster
  plan's 2026-07-01 entry.
- **2026-07-04 - Headed verification: P2 swatch + P3c chip strip seen on screen.** A
  driven session (fresh binary, marquee over two nodes) summoned the connections swatch
  in the focus-card slot with the classifier chip rendering **"Loose · 2"** and both
  selected nodes as dots, no inter-edge drawn — the correct Loose-floor output for a
  2-node selection with no direct relation (capture `C:\t\smoke16-crop.png`). Selection
  also flipped the selected nodes' incident edges to the amber selected rendering on the
  canvas (the per-cell selected-edge draw from the 06-30 P4 pass, now headed-confirmed).
  This clears the "chip render not yet headed-verified" residual from P3c; the sqlx/sync
  crash that made earlier drives flaky did not recur. Note for drive harnesses: the app's
  earlier crash-on-URL-load this session was the shell-partition dead-NodeId bug (fixed
  in `genet_render.rs`/`pane_session.rs`, logged in the surface-engine fold plan), not a
  swatch issue. Still unverified headed: the P5 hide/show spring relaxation (focused
  tests cover it) and the P4 fan/pick on a multi-relation pair.
- **2026-07-05 — Reconciled against code; P6 marked shipped-differently, retrofit
  scoped and declined.** Code check found the plan's own P2 "Findings" claim was
  never actually true in full: `Scope`/`SwatchInstance`/the element-model unification
  (P2b) still does not exist anywhere in the tree (`grep` for `enum Scope`, `struct
  SwatchInstance` — no hits). What exists is two concrete render fns, `swatch_view<S>`
  and `connections_swatch_view<S>`, sharing only the `SwatchView<S>` type alias and DOM
  conventions — not a real generic component a third scope could plug into. Separately,
  the gloss minimap *was* migrated Scene->DOM (the now-archived gloss-scene-to-dom
  plan, P1-P3, 247/247 tests, headed-verified 2026-07-01/02) but through its own
  `gloss_view.rs` (`minimap_view`/`recent_view`), not this plan's component — confirmed
  by grep, zero references to `swatch_view`/`SwatchInstance`/`Scope::` in that file.
  DOC_README's swatch-primitive-plan entry still read "P6 ... remain unstarted" even
  after the archive/reconcile pass added the gloss-migration entry beside it (same
  commit, 048e225-era docs note never updated) — fixed alongside this entry.
  **Retrofit scoped, on request, before deciding**: `swatch_view`/`connections_swatch_view`
  are generic over embedder state precisely *because* they carry no state-bound
  callbacks — clicks route through the host's external `data-subject`/`data-element`
  hit-test. `gloss_view.rs`'s minimap/recent instead use direct `clickable(...)`
  closures over a concrete state type + a local intent queue, the same convention
  `gloss_outline_view.rs` and the roster views already share. These are two
  deliberately different, each locally-correct answers to click dispatch, not
  duplicate work; unifying them means either ripping the intent-queue convention out
  of the whole DOM-folded gloss/roster/outline family, or bolting stateful callbacks
  onto `swatch_view` and losing the property that makes it generic over `S` at all.
  Actual DOM duplication between the two node-rendering paths is small (~30-40
  lines: a positioned square/dot + a color lookup). **Declined for now**: the payoff
  (one shared render path) doesn't fully materialize with only two real consumers,
  and doing it now means re-deriving and re-verifying an already-shipped, tested
  feature to satisfy this phase's letter rather than a functional gap. Revisit if a
  third swatch consumer appears — at that point the real `Scope`/`SwatchInstance`
  abstraction pays for itself across 3+ call sites instead of being bespoke-built for
  one.
