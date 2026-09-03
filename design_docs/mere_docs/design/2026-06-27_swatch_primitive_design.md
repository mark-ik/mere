# The Swatch Primitive (design)

**Date**: 2026-06-27
**Status**: Design, from a Mark + Claude session. Elevates
[gloss = the Navigator](2026-06-07_gloss_navigator_design.md) §2a/§2b's "the swatch
wants to be a standalone component" into the primitive's own doc. This is the model,
not a build plan; the current tree has only fragments, each owned by a sibling plan
(§10). No code from this session.
**Related**: [gloss = the Navigator](2026-06-07_gloss_navigator_design.md) (the swatch
as the Navigator; this realizes it),
[scope model reconciliation](2026-06-27_scope_model_reconciliation.md),
[graphlet derivation from selection](2026-06-13_graphlet_derivation_from_selection.md),
[node body & face model plan](../implementation_strategy/2026-06-23_node_body_face_model_plan.md)
(owns the node-body-editor swatch, B3),
[object card plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_object_card_plan.md)
(owns the focus-card-slot card),
[graphlet wiring plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md)
(owns the per-window instance machinery + graphlets),
[graph signals layer plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md)
(owns the gloss swatch lens),
[petgraph / RDF plan](../implementation_strategy/2026-06-18_petgraph_rdf_plan.md)
(owns edge multigraph storage + the collapse-as-LOD ruling),
[scriptable field regions plan](../implementation_strategy/2026-06-13_scriptable_field_regions_plan.md)
(owns field-region edge visibility).

---

## 1. Identity: the swatch is the fourth graph primitive

The graph has three content primitives: **node** (entity), **edge** (relation),
**field** (the ambient force / continuous layer). The swatch is a fourth, of a
different kind: it is the **canvas** primitive. A swatch is an embeddable,
manipulable, configurable projection of the graph, a little slice of it, scoped to a
node, a graphlet, a selection, or the whole graph.

The consequence that organizes everything below: **the orrery is the root swatch**
(scope = whole graph). It is not a privileged "truth surface" that swatches imitate.
It is one curation among many. You could turn every edge off, drop the nodes into a
grid, and call that grid your orrery dashboard, while a gloss swatch of the same
graph still shows it force-directed with edges live. Same graph, two curations, side
by side.

So a swatch is almost a new instance of the graph, in the spirit of the tearout
plan: a view-instance with its own state over shared graph truth, exactly the
per-window isolation the branch/tearout work already built (`WindowView` carries
per-graph selection, camera, and scope). The swatch generalizes that instance to any
scope, any arrangement, embeddable anywhere.

## 2. Two planes: truth and curation

Only one plane is shared.

- **Truth** (kernel): which nodes exist, which relations connect them, the fields.
  Shared by every instance. Changed only by assert / retract / delete. **Hiding is
  not deleting**, so no view operation touches this plane.
- **Curation** (per-instance): scope, arrangement, lens, which cells are hidden,
  camera, selection, mode. Private to each instance.

This is the rule that keeps the model honest: a swatch is as flexible as the real
graph, but its flexibility lives entirely in its own curation. Hiding a relation in
one instance leaves it drawn and live in every other instance, in a selection, in a
Linked graphlet. The facts persist; the picture changes.

## 3. The instance config

A swatch instance is its curation over shared truth (illustrative-signature-only):

```rust
// illustrative-signature-only — the fields, not the final type
struct SwatchInstance {
    scope:      Scope,        // Node | Selection | Graphlet | Graph
    layout:     Layout,       // minimap | body | radial | astroid | timeline | grid | force | ...
    lens:       Lens,         // content-peek | signal-heatmap | facet | revealed-edges | ...
    projection: EdgeProjection,       // which families this instance's layout + derivation follow
    hidden:     VisibilityOverride,   // this instance's cell hides (§8)
    camera:     Camera,
    selection:  Selection,
    mode:       Mode,         // Map (view) | Editor (edit layer on)
}
```

That bundle is the swatch. The orrery is one; a branch window is one; a gloss
minimap is one; the node editor is one.

## 4. The pipeline

A swatch renders by a pipeline, not by reading five flat dials. Naming the flow
shows where each parameter bites:

```
scope + projection  ->  (members, edges)     // cartography / kernel derivation
        ->  layout   ->  positions            // arrangements
        ->  lens     ->  decorated elements   // overlays: revealed edges, heatmap, peek, ghosts
        ->  raster   ->  DOM | Scene | img     // LOD + context (§5)
   mode  ----------->  edit layer (off = Map)
```

So the derivation-from-selection preview (2026-06-13) is just `scope:Selection +
projection:toggle + layout:force + lens:revealed-edges + mode:view->edit`. The body
editor is `scope:Node + layout:body + lens:face + mode:edit`. The minimap is
`scope:Graph + layout:minimap + lens:heatmap + mode:view`. One pipeline, four
presets.

## 5. Element model and rasterization (the DOM / Scene resolution)

"Render as DOM or as a Scene" is a false choice. Split the two things it conflates:

- **The element model** is the swatch's interface: a list of semantic placed
  elements, `{id, kind (node / edge / vertex / handle / ghost), rect, z}`. This
  carries hit-testing, theming intent, accessibility, and the embedder contract.
- **The rasterization** is how each element becomes pixels: a chrome DOM box, a
  Scene draw, an `<img>`, an instanced point.

Rasterization is an **LOD and context** decision under a stable element-model
interface. A single-node editor or a twelve-member connections swatch rasterizes
every element as DOM, fully themeable and navigable inside a note or a menu. A
whole-graph minimap keeps the same element registry for hit-test while the dense
fill rasterizes as a Scene. The orrery already works this way (host-side node
hit-rects beside a Scene fill), so the root swatch is the proof the split scales.
**The embedder context sets the LOD ceiling**: a swatch in a djot block or menu is
small-scope and all-DOM; the gloss-pane minimap is allowed the Scene fill.

## 6. The factoring: one view layer, pluggable geometry and edit

To stay one component rather than a god-object:

- **One shared view layer** runs the pipeline, emits the element model, and owns the
  hit-test / gesture contract (`gesture G on element E in swatch S`, routed through
  the chrome press-gate since genet has no native pointer-drag).
- **A geometry provider keyed by scope**: a single node's face + hull versus a
  cartography subgraph layout. The single node is the degenerate case.
- **An edit strategy keyed by scope**: `NodeBodyEdit` (hull vertices, sprite) /
  `SelectionShapeEdit` (reveal -> project -> crystallize) / `GraphArrangeEdit`
  (move, arrange, edge) / `FacetEdit` (values).

The view is genuinely one thing. Edit is a trait family, because "what edit means
follows the scope" (gloss §2b).

## 7. Edges as a primitive: cells-as-edges

The swatch surfaced that the **edge** primitive is impoverished: it supports relate
/ delete and draws as one collapsed line, while the node primitive got rich (face,
hull, sprite). Enriching the edge is the real new primitive work, and the kernel
already makes it cheap.

The kernel stores one `EdgePayload` per node pair, with a sidecar per family, each
holding a set of sub-kinds (`graph-kernel/.../edge_payload.rs`). So the relations
between a pair are **already distinct, addressable cells**
(`retract_relation(RelationSelector::Semantic(Cites))` already removes exactly one).
What flattens them is purely the orrery draw, "one undirected line per pair"
(`orrery/.../build.rs:91`).

**Decision (Mark, 2026-06-27): the addressable edge is a cell**, identified
`(source, target, RelationSelector)`. No storage change. The work is:

1. **Un-collapse the render**: draw each visible cell between a pair as its own
   fanned, family-coloured line, individually selectable.
2. **Per-cell weight**: thickness becomes each relation's own metric, not per-pair
   density.
3. **Per-cell hit-test**: selection resolves to the specific relation, which is the
   per-edge selection the graphlet-wiring open item #1 deferred.

This aligns with the [petgraph / RDF plan](../implementation_strategy/2026-06-18_petgraph_rdf_plan.md),
which rules that the multigraph is logical (one statement per fact, enumerated as
`SemanticStatement` records inside the pair-local `EdgePayload` bucket, each with its
own `StatementId` — the statement-bucket revision 2026-07-04) and visual edge-collapse
is an experience-layer LOD setting. Cells-as-edges is that experience layer: the
swatch reads the per-relation cells and chooses, per instance, whether to fan or
collapse them; storage stays one payload per pair either way. True parallel instances
(two distinct `Cites` between one pair with separate metadata) ride those statement
records if and when P1 lands; the swatch model needs no storage change to start.

## 8. Visibility: a default plus non-propagating override stack

Hiding is curation, not truth (§2), and it is **per-instance**. The model reuses
forme's existing precedence, applied to visibility:

```
GraphDefault  <  GraphViewOverride (per-instance)  <  SelectionOverride (per-selection)
```

Higher layers win, and a higher layer never writes down into a lower one. So:

- **GraphDefault** is the graph-native "hidden in the graph" base, the answer to
  "choose which edges are hidden in the graph." It is the only graph-wide layer.
- **A per-instance hide stays local.** Hiding a cell in the orrery or one swatch
  never hides it in another context, a selection, or a Linked graphlet. This is the
  crucial property.

**Hiding relaxes the spring, in that instance only** (Mark, 2026-06-27). Visibility
feeds the physics: an instance's spring set is its visible cells intersected with
its projection. So decluttering a view also de-tangles its layout, and because the
hide is per-instance, no other view re-settles. Membership stays on truth:
derivation follows projection over real edges, so a hide never drops a node out of a
graphlet.

A placed field-region rule
([scriptable field regions](../implementation_strategy/2026-06-13_scriptable_field_regions_plan.md))
governs edge visibility inside its extent; it resolves into this stack as another
contributor rather than a parallel mechanism.

## 9. Templates: the variant library made first-class

gloss §2b's "variant" (one point in the `(scope, layout, lens, projection, mode)`
space) becomes a first-class, named, saveable **template**, with two additions: a
**lock** and the **conditions** for where it applies.

A fresh swatch instantiates a template, and its default is **that template's**
config, the default of what the swatch is for, not a universal one. Two ways a
template binds scope:

- **Parametric**: works on any element of a kind; you point it at a target (the node
  editor on any node).
- **Bound**: carries its own subgraph as the scope (a "settings tiles" template over
  the settings subgraph, a filesystem template over a filesystem subgraph).

**Lock is config-lock, not interaction-lock.** A locked swatch cannot be
reconfigured into something else, but using it is the whole point (the hull editor
stays the hull editor, and you still fit hulls in it). **Lock is user-toggleable**
(Mark, 2026-06-27): you can unlock a locked swatch (unlock the settings editor to
borrow the orrery view, pick the next node, and the context snaps you back to the
editor for it). The one rule: there is always a way back to the correct tool for the
swatch's context, which the **reset to template default** verb guarantees, since the
instance remembers its template even while unlocked.

For unlocked swatches, three runtime verbs:

- **Copy the orrery's current config** into this swatch.
- **Reset to the template default.**
- **Apply a saved template.**

Locked swatches expose none of the three. That is what locked means.

**Templates persist per persona** (Mark, 2026-06-27). A persona's saved swatch
templates ride the persona settings store (`personas/<id>/settings/`, beside the
curated context menu in `PersonaSettings`), the same per-persona UI-curation pattern
the [command registry / configurable menus plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_command_registry_configurable_menus_plan.md)
uses, so a persona carries its own swatch library across graphs.

The consequence worth stating: the swatch primitive plus the template mechanism is
the **generic engine for purpose-built graph UIs**. The settings surface, a
filesystem view, the gloss, and the node editor are all the same primitive under
different templates (locked / bound, unlocked, locked / parametric). Build the
primitive and templates once and those fall out.

## 10. Where this meets the current build

Fragments exist, each owned by a sibling plan; the unifying primitive does not.

- **`meerkat/src/swatch.rs`** is the single-node hull editor only (the node-body
  designer), concrete over `SettingsPanesView`. It is template #1 (locked,
  parametric) in all but name, and it has only ever rendered one node's hull in the
  facet pane. Owned by
  [node body & face model plan](../implementation_strategy/2026-06-23_node_body_face_model_plan.md)
  B3.
- **The gloss minimap** is a separate Scene-based, host-drawn path
  (`render/paint.rs`, via `Orrery::gloss_geometry`). It does not use `swatch.rs`.
  gloss §2a already names it "a candidate to migrate onto the DOM swatch"; its lens
  is owned by
  [graph signals layer plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md)
  P6.
- **The object card** is `FocusCardKind::ObjectCard { widgets }`, DOM, in the
  orrery's single focus-card slot (`window_view/mod.rs`). The slot also holds
  `Snapshot` and `Unvisited`. As of 2026-06-27 the snapshot / unvisited card summons
  **only on single-node selection** (`selected_members().len() == 1`), so the summon
  split is explicit: a single selected node → its snapshot card; a multi-node
  selection → the connections swatch (next bullet). The live-preview card was retired
  (`d4375d0`) and its remaining vestiges (the snapshot↔live toggle, the
  promote-to-live paths, the stale "live card" comments) were stripped 2026-06-27;
  going live is opening a workbench tile now. Owned by
  [object card plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_object_card_plan.md).
- **The connections swatch** is unbuilt, marked `TODO(swatch agent)` at
  `render/cards.rs:129`: a multi-node selection (`selected_members().len() > 1`)
  should summon the selected nodes plus their edges as a DOM swatch. That is where
  the derivation-from-selection preview and the shape classifier land.
- **The instance machinery** exists for branches (per-window `WindowView` selection
  / camera / scope isolation, branch-scoped orrery), the substrate the
  swatch-as-instance generalizes. From the
  [graphlet wiring plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md).
- **The shape classifier** (selection -> ranked graphlet kinds) is still unbuilt,
  the named gap from the 2026-06-13 doc and reconciliation ruling 6.

## 11. Open questions

- **Build order.** This doc is the model, not a plan. A sequencing plan (lift
  `swatch.rs` off `SettingsPanesView` to host-generic with a scope parameter, then
  the connections swatch and the classifier, then migrate the minimap, then
  templates and the visibility stack) is the next artifact when a build is
  greenlit. See the
  [scope model reconciliation](2026-06-27_scope_model_reconciliation.md)
  "what changes when a build is greenlit" section.
- **Conditions vocabulary.** A template's applicability conditions (which scope
  kinds, which subgraph signatures) need a concrete form.
- **Per-cell fan geometry.** How many parallel lines between a pair stay legible
  before they need their own collapse / expand affordance.
- **Edit-layer reach.** `GraphArrangeEdit` overlaps the orrery's existing direct
  manipulation; whether the swatch edit layer subsumes it or wraps it is unsettled.
- **Classifier ranking strength.** Carried from the 2026-06-13 and
  scope-reconciliation open questions; likely a setting, not a constant.
- **Field as a scope** (Mark, 2026-06-28; deferred). A field region's spatial extent is a
  candidate swatch scope: the nodes inside it become the scoped set, a convenient grab-bag that
  need not be connected (so it reads as a Loose / Session shape, not a structural one). Extends the
  scope axis (node / selection / graphlet / graph, plus **field**); cross-refs the
  [scriptable field regions plan](../implementation_strategy/2026-06-13_scriptable_field_regions_plan.md).
