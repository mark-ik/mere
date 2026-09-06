# Graph Roster (the graph's manifest) + the surface / frame taxonomy

> Naming note (2026-06-07): "roster" is the **graph manifest** (this doc). The
> **content inspector** (page devtools: metadata, links, source, trackers,
> scripts, cookies) is a *separate* surface — see §3. Earlier drafts used
> "inspector" for the roster; that conflation is corrected here.

**Date**: 2026-06-07
**Status**: Design, from a Mark + Claude session. Frame-tree substrate and a seed
roster pane now exist; the R-phase roster details here remain target design.
**Related**: [gloss = Navigator](2026-06-07_gloss_navigator_design.md) (sibling surface), [card system + staging](../implementation_strategy/2026-06-07_card_system_and_staging_plan.md) *(historical citation)* <!-- doc-audit: historical-link -->, `frame` crate (`FrameLayout`), `platen` (tile tree), `cartography`, `aether` (fields).

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

The surfaces, distinct roles (Mark split roster vs inspector 2026-06-07):

- **orrery** — the graph in **space** (spatial, visual; nodes/edges/fields drawn
  under one camera).
- **gloss / Navigator** — **navigate / summarize**: swatches, graphlets,
  minimap, outline (see the gloss doc).
- **workbench** — **tile** content: the tile tree (tabs / slots of node content).
- **roster** — the graph's **manifest**: every primitive (nodes/facets,
  edges/relations, fields), examinable. *This doc.* Graph-scoped.
- **inspector** — the active **content's diagnostics**: a page's metadata +
  structure, its links (with actions like "spawn all / selected links as
  nodes"), page source, trackers / scripts / cookies. Devtools-like and
  **content-scoped** — distinct from the roster (graph-scoped). Its own design,
  TBD.
- **apparatus** — the **system**: host diagnostics + settings.
- **comms** — misfin / murm messaging (existing crate).

Roster answers "what is in the graph"; inspector answers "what is this page made
of (and let me act on it)"; gloss answers "how do I see/navigate the graph";
apparatus answers "how is the system doing." Four different questions, four
panes, all frame-tree leaves.

**Surfaces not yet placed (completeness check, 2026-06-07).** Beyond the panes
above, these exist or are implied and need a home:

- **shellbar** — the control surface that *summons / toggles / arranges* panes
  (the frame tree's chrome). The memory's "summonable from shellbar." Pairs with
  the frame-tree arc (it is the frame tree's UI). Without it there is no way to
  open gloss / roster / inspector / apparatus.
- **sync status + "sync now"** — real peers / items / last-synced feedback backed
  by a genuine RBSR round, not a placebo spinner (a standing Mark requirement).
  Today only a `tessera: idle` chip. Lives in comms, apparatus, or its own strip.
- **search / find** — graph search + find-in-page (intel `embed::canvas_search`
  exists). The omnibar covers navigation; a search affordance/surface is separate.
- **session / graph switcher** — "window = graph-shaped session"; switching
  between graphs/sessions (pane UX brief's session-switcher row). Pairs with the
  shellbar + multi-window.
- **settings** — folded into **apparatus** (Mark), not its own pane.
- **import / export** — linked-data ingest/export (the `import` crate); surfaced
  as actions (the inspector's "spawn links as nodes" is a graph-building action
  of the same family) rather than a pane.
- **persona / identity** — persona is first-class; a switcher likely lives on the
  shellbar or in apparatus, not its own pane.

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

### 3.1 Inspecting: node vs tile (the bridge, 2026-06-09)

**Current state (2026-06-09):** the shipped Inspector (`meerkat/src/inspector.rs`)
is a single focused-node pane blending the active node's facets and its content
diagnostics, following selection; Steward and Roster ship as their own panes. The
node-vs-tile split and bridge below are the **target**, not yet built (no
inspect-node / inspect-tile context menus or parameterized leaves yet).

A node and its tile are two projections of one node (orrery = graph primitive,
workbench = addressed content), so inspecting is contextual:

- **`inspect node`** (orrery context menu) roots the **roster** on that node (its
  facets), split beside the orrery. Facets is the roster at node scope, not a
  separate pane.
- **`inspect tile`** (workbench context menu) opens the **inspector** (content
  devtools), split beside the workbench.

The two are **bridged by node id**, so you cross between a node's data face and its
content face without losing your place:

- From the roster rooted on a node: "inspect content" opens the inspector on that
  node (and tiles it).
- From the inspector: "show facets" / "reveal in orrery" roots the roster, or
  focuses the orrery, on that node.

Mechanically this is the first **parameterized** frame leaf: the target carries a
node id (and, for the inspector, that its scope is content), and it summons
**beside its source leaf** rather than at a fixed anchor. The context menus
themselves (a node menu in the orrery, a tile menu in the workbench) are net-new
shared infrastructure; `inspect node` / `inspect tile` are their first entries.

---

## 4. Shellbar (2026-06-09)

**Design decision: docked chrome strip, outside the frame tree.**

The shellbar is the persistent strip of pane-toggle buttons at one edge of the
window. It is **not a frame-tree leaf** — the frame tree owns content panes
(roster, orrery, workbench, …); the shellbar is the chrome that *operates* the
frame tree from the outside.

**Spatial placement.** The shellbar docks to one of four window edges: Left
(default), Right, Top, or Bottom. The content band (the region the frame tree
fills) is computed as the window minus the toolbar band (top) minus the shellbar
strip. Moving the shellbar means changing the edge preference — no
`reparent_leaf`, no frame tree mutation.

**Why not a frame leaf.** A leaf can be closed, split against, and have
siblings. The shellbar can do none of these. Any "never-close, never-split"
invariant enforced in the frame tree is steady-state complexity for the life of
the codebase. Outside is simpler.

**Move gesture.** Right-click the shellbar → "Move to left / right / top /
bottom." Sets the edge preference and recomputes the band. Drag-to-edge is a
later enhancement.

**Contents (F2.1).** Five toggle buttons: Workbench, Roster, Gloss, Apparatus,
Comms. Each is illuminated when its pane is open. The orrery is always-on (no
toggle). The existing keybinds (Ctrl+R, Ctrl+T, …) remain; the shellbar is the
mouse-accessible equivalent.

**Persistence.** `shellbar_edge` lives in `PersistedSettings` (sibling to
`theme_id`) under `settings.json`.

---

## 6. Prerequisite: wire the frame tree into meerkat

**Complete as of 2026-06-08.** `meerkat` now uses `frame::FrameLayout` for the
window-level content-region pane arranger. The orrery is its own frame leaf, the
tiled workbench is a summonable sibling pane, and roster / gloss / apparatus /
inspector / steward can be summoned as frame leaves. Tile splits remain internal
to the workbench pane.

The **frame-tree pane system** remains the substrate that unlocks richer gloss,
roster, side-by-side-with-orrery, and future tear-out. `FrameLayout` stays the
window-level pane tree; `platen` stays the workbench's internal tile tree.

---

## 7. Phasing (proposed)

1. **F1 — frame tree in meerkat.** Done. `FrameLayout` arranges the orrery,
   workbench, roster, gloss, apparatus, inspector, steward, and comms as
   window-level pane leaves. Tear-out remains later.
2. **F2 — pane summon/arrange UI.** A shellbar should replace keybind-only pane
   toggles and expose arranging/maximizing affordances.
3. **R1 — roster node list + facets** (node-rooted detail: metadata + lineage).
   This is where node facets move as Inspector narrows to addressed-content
   diagnostics.
4. **R2 — edges + fields** in the node detail; drill-through; systemic edge/field
   enumeration.
5. **R3 — sort/filter by content type**; **content-type node shapes** in the
   orrery (shared attribute).
6. gloss G-phases (see the gloss doc) interleave once the frame tree exists.

---

## 8. Open questions

- **Tear-out / multi-window** timing — `FrameLayout` supports reparenting; the
  synced-multi-window work (shared `Entity<T>`) is a known later arc.
- **Field detail rendering** — how to present an aether field's AST / couplings
  legibly in the roster.
- **Node shape vocabulary** — the set of shapes per content-type family (square =
  document; what for feeds, media, smolweb, unknown). Likely a theme-token / lens
  concern, not hardcoded.
- **Selection sharing** — roster ↔ orrery ↔ gloss shared selection (lean: shared,
  consistent with the gloss doc).
