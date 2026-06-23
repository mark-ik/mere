# Gloss Outline Lens Plan

**Planning (with Mark), 2026-06-23.** A hierarchical **djot outline of the graph**
plus a compact **metrics** readout, surfaced as the gloss Navigator's long-deferred
**outline form factor** (the interaction-model spine's named-but-unscheduled
"gloss-outline / A3"). The pure `Graph -> view` projection logic lives in the crate
slice 4 just freed up (today `mere-orrery`, to be renamed). The outline is a real djot
document, so it doubles as the seed of the first notetaking feature: the read view now,
an editable knot later.

This plan **implements** existing design; it does not re-design. It realizes the
[gloss Navigator design](../design/2026-06-07_gloss_navigator_design.md)'s deferred
outline form factor (G3) and its §2a DOM-not-Scene decision, consumes the
[graph signals layer](2026-06-22_graph_signals_layer_plan.md) for the expensive
metrics (never reproducing them), and registers the outline as the **sixth projection**
beyond the five in the [graph projections research](../research/2026-06-22_graph_projections_research.md),
under that doc's projection contract and gloss no-split rule.

---

## Where it fits (one paragraph)

The gloss is one configurable Navigator surface (no-split rule): scope (active-doc /
graph / graphlet) x form factor (outline / swatch). Today only the **swatch** factor
exists, rendered as two stacked netrender Scene textures (minimap + recently-visited,
`meerkat/src/gloss.rs` `minimap_scene` / `recent_scene`, composited at
`render.rs:1765-1810`). The **outline** factor was never built. This plan builds it,
and builds it as **chrome-understood DOM** rather than a Scene texture, because (a) §2a
already decided swatches *should* migrate to DOM for flow / theming / keyboard-nav /
embeddability, and (b) a textual outline is natively DOM. So the outline lens is also
the **wedge**: the first DOM gloss section, proving the path the minimap and recent list
later follow.

---

## Findings (code-verified 2026-06-23, 5-agent grounding)

- **djot is live and default.** `jotdown` 0.10 is a nematic dependency;
  `inker/engines/nematic/src/knot/djot.rs` parses + round-trips djot knot bodies
  (`parse_djot_knot_body`, `blocks_to_djot`, `DjotKnotEngine`); `inker/src/routing.rs:339-344`
  routes `text/x-knot` to `ENGINE_NEMATIC_KNOT_DJOT`. CommonMark is compat-import only.
  So an emitted djot outline is renderable + editable by an existing engine.
- **The outline nests by Containment.** `Graph::containment_edges()`
  (`graph-kernel/src/graph/query.rs:211-226`) yields `ContainmentEdgeView { from, to, sub_kind }`;
  the **UrlPath** and **Domain** sub-kinds (`edge_taxonomy.rs:117-125`) encode URL-structural
  parent/child, the natural outline tree. Node text comes from `Node.title` +
  `Node.primary_address()` (`node.rs:36-37`), identity from `Node.id: Uuid` (`node.rs:31`).
- **Cheap metrics already exist in the kernel.** `node_count` / `edge_count`
  (`query.rs:491-498`), `weakly_connected_components` (`query.rs:458`), `out/in_neighbors`
  (degree, `query.rs:321-328`), `orphan_node_keys`, and per-edge `EdgeMetrics`
  (`total_navigations` etc., `edge_data.rs:38-46`). These are free, inline, no producer.
- **Expensive metrics belong to graph_signals.** Centrality (betweenness / PageRank),
  community (Leiden / Louvain), and bridge/articulation scores are **not** in the kernel
  and are reserved on `cartography::IntelligenceSignals` (`signals.rs:21-32`) for the
  unbuilt `intel/signals` producer ([graph_signals P1-P3](2026-06-22_graph_signals_layer_plan.md)).
  The outline **consumes** them when they ship and **falls back** (degree for importance,
  components for community) until then. It must not become a second producer.
- **mere-orrery is the right home + already nearly free.** Pure `Graph -> view` domain
  crate, deps `kernel` / `uxtree` / `accesskit` / `tracing` (`mere-orrery/Cargo.toml:16-20`);
  its only function `project_graph` (the a11y projection) was retired host-side by slice 4
  and is now dead. It grows `outline_djot` + `graph_metrics` and sheds `project_graph`.
- **The gloss currently bypasses cartography.** `orrery/lib.rs:831` `minimap_geometry`
  returns positions + edges only, discarding the `Projection`. The gloss reading its own
  `ProjectionRequest` (for signal-driven encodings) is graph_signals **P6**, not yet plumbed;
  the outline lens does not need it for P0/P1 (counts + structure), only for P3 (signals).
- **The projection contract + no-split rule are in force.** The outline must be a gloss
  **lens** (a form factor within the one Navigator), read over kernel truth (no second
  tree), curation writing back via `assert_relation` with honest provenance, with node
  representation (color / form) riding through orthogonally. It is a sixth projection
  distinct from Trail (time) / Claim map (stance) / Provenance (derivation) / Facet (pivot) /
  Neighborhoods (community).

---

## The crate: rename + repurpose `mere-orrery`

Slice 4 retired `project_graph`'s only caller, so the crate's stated role ("projects
Graph into AccessKit nodes") is dead and its name (tied to the *orrery*, the spatial view)
no longer fits a pure textual / statistical projection. The repurpose:

- **Role:** the pure-data `Graph -> consumable-view` projection backend. Houses
  `outline_djot(&Graph) -> String`, `graph_metrics(&Graph) -> GraphMetrics`, and the natural
  future home for other read-side textual projections (the Facet matrix's text form, export
  digests). Stays `&Graph`-immutable, host-free, DOM-free; the host renders, the crate projects.
- **Drop** the dead `project_graph` + its four tests (the a11y projection lives in meerkat's
  `orrery_a11y_tree` now).
- **Name (open, Mark's call).** Lead candidate `graph-digest` (a digest = outline + metrics);
  alternatives `graph-views`, `graph-projection` (accurate but overloads cartography /
  `forme::ProjectionLens` / the graph-rooted "projection" model). Plain over soulful per the
  domain-vocabulary rule; the aesthetic word list is reserved for product names.
- **Location (open, Mark's call).** Lean: move from `crates/orrery/` to `crates/graph/`,
  beside `linked-data` (both are `Graph -> external-representation` projections; the outline is
  djot the way linked-data is RDF/JSON-LD). Staying under `crates/orrery/` keeps the spatial-view
  grouping it no longer belongs to.

---

## Design

### `outline_djot(&Graph) -> String`
Walk `containment_edges()` filtered to `UrlPath` (then `Domain`) into a parent->children
tree; emit djot, each node a nested bullet or heading carrying `[title](url)` (title, or the
URL slug when unloaded, the label rule `frame_a11y_panes.rs:298-301` already uses). Flat list
fallback when no containment tree exists (a fresh graph of unrelated tabs). Plain string
emission, no nematic dependency (djot is plain text; the round-trip engine is the *renderer's*
concern, not the projector's). Open: depth cap / breadth cap for a constrained pane vs a full
document; nesting axis configurability (see P2).

### `graph_metrics(&Graph) -> GraphMetrics`
A struct of the free, kernel-sourced aggregates: `node_count`, `edge_count`,
`relation_counts` (histogram over the six edge families), `traversal_total` /
`avg_navigations_per_edge` (from `EdgeMetrics`), `orphan_count`, `component_count` +
largest-component size. Consumes kernel queries, never re-walks to recompute what the kernel
stores. The signal-driven fields (importance, community) are **not** in this struct; they
arrive on the gloss side from `intel/signals` (P3) so this stays cheap + producer-free.

### The gloss outline lens (host / DOM)
Render the outline as a **DOM** section in the gloss pane: a scrollable list of indented
rows, each row a node carrying its `data-member` (the slice-4 stamp) so click routes to
`SelectNodeByUrl` (the minimap / roster pattern) and the row takes the node's NODE_SHEET color
+ selection highlight (representations carry node identity). The metrics render as a compact
header or footer readout. Obeys the no-split rule: this is a section / form factor within the
one gloss surface, not a new pane.

**Reconciling "third section" with "form-factor-switched."** Mark's framing is a third
stacked section between minimap and recent; the gloss design's is one surface that *switches*
between outline and swatch factors. P1 ships the literal third stacked section (always visible,
simplest, matches the mental model + the current two-stacked-section reality). The form-factor
**toggle** that swaps the whole gloss between outline and swatch is the gloss design's G3 and
stays deferred; when it lands, this section becomes the outline factor's body. The 58% / ~42%
minimap/recent split (`render.rs:1757`) becomes a three-way auto-size (minimap flex / outline
flex-scroll / recent fixed).

### The notetaking seed
The emitted djot is a real document. "Open outline as knot" hands the string to the djot knot
engine for an editable, annotatable, exportable note (the spine's "first notetaking feature").
The outline is the read view; knot-ification + curation write-back is P4. This is why djot, not
an ad-hoc list: the format *is* the editing + export path.

---

## Phases (cheapest-first; done-conditions, not dates)

- **P0 — the projection crate.** Rename + relocate the crate (Mark picks name + home); drop
  dead `project_graph` + tests; add `outline_djot` + `graph_metrics` + `GraphMetrics` behind
  unit tests over a fixture graph (containment tree -> expected djot; counts / histogram /
  components exact). Pure data, fully testable with no host. Done: the crate builds, its tests
  pass, meerkat's now-live dep points at it.
- **P1 — the gloss outline DOM section.** A third DOM section rendering the outline + metrics;
  rows route `SelectNodeByUrl` and carry node color / selection; three-way gloss auto-size.
  The first DOM gloss section. Done: opening the gloss shows the live outline + counts, clicking
  a row focuses the node, headed-verified.
- **P2 — hierarchy + scope lens.** Make the nesting axis configurable (Containment default ->
  Arrangement sub-kind / Semantic family, via `forme::ProjectionLens`) and the scope honor the
  gloss scope picker (full graph / active selection / graphlet). Folds into the gloss design's
  G3 form-factor/scope work. Done: the outline re-nests by a chosen edge family and re-scopes.
- **P3 — signals-fed metrics.** When `intel/signals` ships (graph_signals P1-P3), the outline
  consumes importance (node emphasis) + community (grouping) with degree / components fallback
  until then. Gated on graph_signals; no work here lands ahead of that producer. Done: importance
  / community appear in the outline when signals are present, fallback otherwise.
- **P4 — knot-ification + curation.** "Open outline as knot" -> editable djot knot; outline
  gestures (drag-to-reorder, promote-to-section) write back via `assert_relation` with typed
  provenance (projection contract). The notetaking feature proper. Done: the outline opens as an
  editable note and a reorder persists as a containment/arrangement edge.

---

## Open decisions

1. **Crate name + location** (P0; Mark). `graph-digest` @ `crates/graph/` is the lean.
2. **Outline nesting axis** (P0 default, P2 general). Containment/UrlPath for P0; which families
   earn a lens, and whether outline-order is its own Arrangement sub-kind, is P2 + a
   [projections-research](../research/2026-06-22_graph_projections_research.md) question.
3. **Depth / breadth caps** for the constrained pane vs the full export document.
4. **Metrics surface split.** Which metrics live in the gloss readout vs apparatus diagnostics
   (the [system-diagnostics plan](2026-06-08_system_diagnostics_and_accessibility_plan.md) owns
   read-only inspection; quick graph stats may belong there, not the gloss).
5. **Scene -> DOM for minimap / recent.** Out of scope here (owned by the gloss design's G-series),
   but the outline lens is its proof-of-path; sequence the migration after P1.

---

## Cross-references (consume, do not duplicate)

- [gloss Navigator design](../design/2026-06-07_gloss_navigator_design.md) — the outline form
  factor (deferred G3) + §2a DOM decision this plan realizes.
- [graph signals layer plan](2026-06-22_graph_signals_layer_plan.md) — P6 gloss lens + the
  `intel/signals` producer the outline's expensive metrics consume (P3).
- [graph projections research](../research/2026-06-22_graph_projections_research.md) — the
  projection contract, the no-split rule, the outline as the sixth projection.
- [interaction model spine](../technical_architecture/2026-06-18_interaction_model_spine.md) —
  the djot lane + the named "gloss-outline / A3" / first-notetaking-feature slot.
- [modular integration plan](2026-06-02_modular_integration_plan.md) — the graph-rooted
  projection model (graph is root; gloss is a projection).
- [node representation / arrangement plan](2026-06-18_node_representation_arrangement_plan.md) —
  node color / form rides into the outline rows (representation orthogonal).
- nematic knot ([djot design](../../nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md),
  [evaluation/export](../../nematic_docs/implementation_strategy/2026-06-12_knot_evaluation_export_plan.md))
  — the djot engine that renders + (P4) edits the outline.

---

## Progress

- **2026-06-23 (scoped).** Spun out of the slice-4 finding that `mere-orrery` became
  consumer-less, plus Mark's recognition that a graph outline + metrics is the gloss's
  third lens and the first notetaking feature. Grounded by a 5-agent code+doc sweep (gloss
  render path, signals layer, djot/knot lane, kernel graph model, projection contract);
  findings above. No code yet; P0 is the first build step. Crate name + location left open for
  Mark per the ask-before-dropping/renaming-deps rule.
