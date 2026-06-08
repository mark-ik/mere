# Graph Roster (primitives inspector) + the surface / frame taxonomy

**Date**: 2026-06-07
**Status**: Design, from a Mark + Claude session. No code yet.
**Related**: [gloss = Navigator](2026-06-07_gloss_navigator_design.md) (sibling surface), [card system + staging](../implementation_strategy/2026-06-07_card_system_and_staging_plan.md), `frame` crate (`FrameLayout`), `platen` (tile tree), `cartography`, `aether` (fields).

---

## 1. The roster: a dedicated graph-primitives inspector

A first-class view of the graph's **data**, the counterpart to the orrery's view
of the graph's **space**. It is a **roster of all graph primitives**, not just a
node detail. Three primitive kinds, all first-class:

- **Nodes** — facets: url, title, content type, tags, provenance,
  classification, lineage (history / branch / semantic projections).
- **Edges** — relations: from/to, family + sub-kind, payload, weight, traversal
  metrics, **visible and hidden** alike. Edge data is stored node-associated,
  but the roster surfaces relations **systemically** (enumerate all edges).
- **Fields** — the aether couplings: definition (AST), response, which
  nodes/edges they couple, Rhai/Burn params.

"Equivalent scope to content" (Mark): the roster is a first-class view, like a
database/table view sitting beside the spatial graph, not a peripheral strip.

**Shape: node-rooted, drill to edges/fields.** The roster is primarily the node
list; a node's detail expands to its edges + fields inline; drill into any edge
or field for its own detail. Relations are seen in context (matching
"edge data is node-associated"), while still being enumerable systemically.

**Sort / filter — by content type** (and more). Sorting nodes by content type is
a first requirement.

The data is all readable already (kernel nodes + `relations()` + orrery
`hidden_edges`; node-lineage projections; aether fields/couplings + kernel
`Coupling`/`Field`). So the roster is largely a **presentation layer** over
existing primitives; the new part is the systemic roster + cross-linking + field
rendering.

---

## 2. Content type as a first-class, visible node attribute

A node's resolved content type (from inker's content-type → engine routing)
drives **two** things:

- **Roster sort** — group/sort nodes by content type.
- **Orrery node shape** — the default node shape reflects its type: square for
  documents, distinct shapes per renderable type. Nodes are uniform rects today;
  this makes content type visible in the graph itself. The host already knows
  each node's type, so it passes a per-node shape hint to the orrery the same way
  it passes `node_states` (the color states).

So content type becomes a shared attribute the roster sorts by and the orrery
shapes by — one source, two surfaces.

---

## 3. The surface / frame taxonomy

Four surfaces, distinct roles over the same graph:

- **orrery** — the graph in **space** (spatial, visual; nodes/edges/fields drawn
  under one camera).
- **gloss / Navigator** — **navigate / summarize**: swatches, graphlets,
  minimap, outline (see the gloss doc).
- **roster / inspector** — the graph's **data**: every primitive, examinable.
- **apparatus** — the **system** (host internals), not the graph.

And two different trees, often conflated:

- **frame tree** (`frame::FrameLayout`) — **window-level** arrangement of
  resizable panes: orrery, gloss, roster, apparatus, *and* workbench panes.
  Split horizontally / vertically with adjustable margin; tear a pane out to a
  new window (`summon_leaf` / `reparent_leaf` / `close_leaf`). Panes are *not*
  tiles.
- **tile tree** (`platen` workbench) — tile-stacking **within** a single
  workbench pane (tabs / slots).

A workbench is one *kind* of pane in the frame tree; its tiles are its internal
tile tree. All panes (including the roster and gloss) live in the frame tree.

---

## 4. Prerequisite: wire the frame tree into meerkat

**meerkat does not use `frame::FrameLayout` today.** The content band is a hard
**orrery-XOR-workbench toggle**, and the "splits" are tile-tree splits inside
the one workbench pane. So the multi-pane vision (a roster pane beside the
orrery, gloss as a pane, tear-out) has no substrate yet.

The **frame-tree pane system is the foundational arc** that unlocks gloss, the
roster, side-by-side-with-orrery, and tear-out. `FrameLayout` exists as a crate
(savable, projects to a uxtree); the work is wiring it into meerkat as the
window-level pane arranger, with the orrery / workbench / gloss / roster as
leaves. This likely comes *before* (or as the enabling first phase of) the gloss
and roster builds, not after.

---

## 5. Phasing (proposed)

1. **F1 — frame tree in meerkat.** Wire `FrameLayout` as the content-region pane
   arranger: orrery + workbench as leaves, h/v splits with adjustable margins.
   Replaces the orrery-XOR-workbench toggle. (Tear-out to a new window is a
   later sub-phase; the memory's multi-window/synced-panels work.)
2. **F2 — panes as leaves.** gloss and roster become frame-tree leaves you can
   split beside the orrery and rearrange.
3. **R1 — roster node list + facets** (node-rooted detail: metadata + lineage).
4. **R2 — edges + fields** in the node detail; drill-through; systemic edge/field
   enumeration.
5. **R3 — sort/filter by content type**; **content-type node shapes** in the
   orrery (shared attribute).
6. gloss G-phases (see the gloss doc) interleave once the frame tree exists.

---

## 6. Open questions

- **Tear-out / multi-window** timing — `FrameLayout` supports reparenting; the
  synced-multi-window work (shared `Entity<T>`) is a known later arc.
- **Field detail rendering** — how to present an aether field's AST / couplings
  legibly in the roster.
- **Node shape vocabulary** — the set of shapes per content-type family (square =
  document; what for feeds, media, smolweb, unknown). Likely a theme-token / lens
  concern, not hardcoded.
- **Selection sharing** — roster ↔ orrery ↔ gloss shared selection (lean: shared,
  consistent with the gloss doc).
