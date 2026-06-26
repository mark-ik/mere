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

## The crate: rename + relocate `mere-orrery` to `glossary`

Slice 4 retired `project_graph`'s only caller, so the crate's stated role ("projects
Graph into AccessKit nodes") is dead and its name (tied to the *orrery*, the spatial view)
no longer fits a pure textual / statistical projection. The repurpose (settled 2026-06-23
with Mark):

- **Role:** the pure-data `Graph -> consumable-view` projection backend, the human-facing
  *digest* sibling of `cartography` (spatial projection) and `linked-data` (machine
  interchange / RDF). Houses `outline_djot(&Graph) -> String`,
  `graph_metrics(&Graph) -> GraphMetrics`, and the natural future home for other read-side
  textual projections (the Facet matrix's text form, export digests). Stays `&Graph`-immutable,
  host-free, DOM-free; the host renders, the crate projects.
- **Drop** the dead `project_graph` + its four tests (the a11y projection lives in meerkat's
  `orrery_a11y_tree` now).
- **Name: `glossary`.** A glossary is a structured human-readable list of terms with brief
  entries, which is exactly an outline of the graph, and it carries an etymological resonance
  with the *gloss* it feeds. (Caveat acknowledged: it must stay engine-neutral, also feeding
  apparatus + export, not read as "the gloss's private crate".) `graph-projection` was rejected
  to avoid overloading cartography / `forme::ProjectionLens` / the graph-rooted "projection" model.
- **Location: `crates/graph/glossary/`,** beside `linked-data` + `node-lineage`, all children of
  the graph supercrate. Moves out of `crates/orrery/` (the *spatial* view it no longer is).
  **Considered and declined: folding `linked-data` into `glossary`** (Mark's question). The kinship
  is real (both `Graph -> representation`), but the `crates/graph/` supercrate already expresses it
  as siblings without a merge; linked-data is mature, RDF-dep-heavy, and the spine of two active
  plans ([graph_query_layer](2026-06-18_graph_query_layer_plan.md), [petgraph_rdf](2026-06-18_petgraph_rdf_plan.md)),
  so absorbing it would churn those plans, balloon glossary's dep tree, and stretch the name past
  the human-digest role. Human digest (glossary) and machine interchange (linked-data) stay distinct
  crates, same supercrate. Reversible if Mark later wants the merge.

---

## Design

### `outline_djot(&Graph) -> String`
**Nest by URL structure parsed from node addresses, not by containment edges** (settled
2026-06-23). Code check: `ContainmentSubKind::UrlPath` exists in the taxonomy but **nothing
on the live path auto-asserts it** (no node-creation hook draws a containment edge to a
URL-parent; the only references are taxonomy / theming / persistence / a forme arrangement
helper, none a populator), so reading `containment_edges()` would yield a mostly-flat tree
for a real browsing graph. Since the outline is a *projection*, it computes its own hierarchy:
parse each node's `primary_address()` into host + path segments and build the tree from the
strings (`wikipedia.org` -> `/wiki` -> `/wiki/Foo`). Always available, deterministic, real
depth, no edge-population dependency. **Overlay** explicit containment edges (user
folders / collections) where they *do* exist; flat-list singletons / non-URL nodes. Each node
is a nested bullet carrying `[title](url)` (title, or the URL slug when unloaded, the label rule
`frame_a11y_panes.rs:298-301` already uses). Plain string emission, no nematic dependency (djot
is plain text; the round-trip engine is the *renderer's* concern, not the projector's). The
nesting axis is pluggable from P2 (semantic / arrangement / recency via `forme::ProjectionLens`);
relation-driven outlines (the user's actual connections) live there, since they are sparse +
not clean trees until the graph is densely related. Open: depth cap / breadth cap for a constrained pane vs a full
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

- **P0 — the projection crate.** Rename `mere-orrery` -> `glossary` at `crates/graph/glossary/`;
  drop dead `project_graph` + tests; add `outline_djot` + `graph_metrics` + `GraphMetrics` behind
  unit tests over a fixture graph (URL-parsed tree -> expected djot; counts / histogram /
  components exact). Pure data, fully testable with no host. Done: the crate builds, its tests
  pass, meerkat's now-live dep points at it.
- **P1 — the gloss outline DOM section.** A third DOM section rendering the outline + metrics;
  rows route `SelectNodeByUrl` and carry node color / selection; three-way gloss auto-size.
  The first DOM gloss section. Done: opening the gloss shows the live outline + counts, clicking
  a row focuses the node, headed-verified.
- **P2 — hierarchy + scope lens.** Make the nesting axis configurable (URL-structure default ->
  explicit-Containment overlay / Arrangement sub-kind / Semantic family, via `forme::ProjectionLens`) and the scope honor the
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

1. ~~**Crate name + location**~~ **Settled 2026-06-23:** `glossary` @ `crates/graph/glossary/`,
   sibling to `linked-data` (merge of linked-data considered + declined; see The crate).
2. ~~**Outline nesting axis** (P0 default)~~ **Settled 2026-06-23:** parsed URL structure for P0
   (containment edges are not auto-populated, so reading them would be flat); pluggable in P2.
   Open at P2: which families earn a lens, and whether outline-order is its own Arrangement
   sub-kind, a [projections-research](../research/2026-06-22_graph_projections_research.md) question.
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
  findings above. No code yet; P0 is the first build step.
- **2026-06-23 (decisions settled with Mark).** Crate = `glossary` at `crates/graph/glossary/`
  (sibling to `linked-data`; merging linked-data in considered + declined). Nesting axis = parsed
  URL structure for P0 — a code check found `UrlPath` containment edges are **not** auto-populated
  on the live path, so reading them would yield a flat outline; the projection computes the host /
  path tree from node addresses directly instead, overlaying explicit containment where present.
  Relation-driven axes deferred to the P2 pluggable lens.
- **2026-06-23 (P0 landed).** Rename half committed first (`mere-orrery` -> `glossary` at
  `crates/graph/glossary/`, dead `project_graph` + tests retired, accesskit/uxtree deps dropped,
  meerkat's dead dep removed; commits 8d55f96 + ffb1b5e). Then the functions: `outline_djot(&Graph)
  -> String` (URL-trie nesting host -> path segments, structural intermediate bullets, non-URL nodes
  flat at the end, deterministic via `BTreeMap`; no engine dep, djot is plain text) and
  `graph_metrics(&Graph) -> GraphMetrics` (node / edge / relation counts, per-`EdgeFamily` histogram,
  orphan + component counts; cheap kernel queries only, expensive centrality/community deferred to
  `intel/signals`). 7 unit + integration tests pass (`parse_host_path`, `node_label`, outline
  nesting / flat / empty, metrics counts / empty). **P0 done.** Next: P1, the gloss DOM section
  (render the outline + metrics as the first DOM gloss section; rows route `SelectNodeByUrl`, carry
  node color/selection).
