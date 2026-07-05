# Gloss = the Navigator (design)

**Date**: 2026-06-07
**Status**: Design, from a Mark + Claude session. Supersedes the narrow gloss
v0 (a document outline strip) by expanding gloss into the **Navigator**: the
single configurable summary surface. G1/G2 seed landed (§7a). **2026-06-22:**
added §2a, the swatch elevated to a portable, embeddable primitive (gloss is
one consumer), the half of the point beyond minimap / MRU / outline.
**2026-06-23:** added §2b, the swatch resolved as the Navigator itself
(scope-zoom, view/edit, a variant library), the node facet editor being variant #1.
**Related**: [card system + staging plan](../implementation_strategy/2026-06-07_card_system_and_staging_plan.md) (§8 staging surfaces here), [pane UX pass](2026-05-11_pane_ux_design_pass_brief.md) (gloss as a Pane variant), `cartography` (swatch / minimap projections), `forme` (graphlets).

---

## 1. Identity: gloss is the Navigator

The project's own terminology: **the Navigator is a single surface with
configurable scope and form factor, never split into multiple instances.** That
*is* gloss, expanded. The graphshell-era "navigator" became "gloss" in the gpui
demo but never grew past a content outline. Gloss is the better name for it, so
the Navigator lives on as gloss.

Consequence: there is **one** gloss surface per window. It is not gloss plus a
separate minimap plus a separate atlas. You re-scope and re-shape the one
surface. The current gloss crate (a heading-outline of the active document) is
the narrowest cell of a much larger matrix.

The unlock (Mark): *how do you outline a **graph**? With **swatches**.* An
outline is form-factor-agnostic; scope is the dial.

---

## 2. The two axes

Gloss is one surface across two independent axes.

**Scope** — what it summarizes:

- **Active content** — the focused node's document (its outline, commentary).
- **The graph** — and within the graph, the **whole graph** or a single
  **graphlet** (arrow between graphlets).

**Form factor** — how it is shown:

- **Outline / list** — text rows (headings; or a node / graphlet list, MRU).
- **Swatch** — a small `cartography` projection (minimap thumbnail, radial,
  astroid hub-collapse, ...).

The cells fall out:

| | outline / list | swatch |
|---|---|---|
| **active doc** | heading TOC (today's v0) + commentary | (rendered card is the orrery's job, not gloss) |
| **whole graph** | recent graphlets / nodes (MRU) | whole-graph minimap |
| **graphlet** | the graphlet's member list | the graphlet as a swatch (e.g. astroid hub-collapse) |

So gloss subsumes: document outline, content commentary, a graph minimap,
graphlet swatches, and recent-groupings lists, all as scope × form-factor cells
of one surface.

---

## 2a. The swatch is a portable primitive, not only a gloss cell (Mark, 2026-06-22)

The swatch's reach is larger than the gloss pane, and that reach is **half the point of
the gloss** (the other half being the minimap / MRU / document-outline cells above). A
swatch is a **portable, embeddable representation of graph elements**: any element (a
single node, a graphlet, the whole graph), **isolated**, with whatever conditions /
filters / arrangement you put on it (§3's graphlet vocabulary), rendered as a
self-contained `cartography` projection. The gloss is **one consumer**; the swatch is the
reusable primitive.

So a swatch embeds **anywhere a graph element wants representing**, not only in the
Navigator:

- **A node facet pane** — a swatch scoped to a *single node* (its sprite + an editable
  collider hull is the first instance; see the node-representation plan's shape editor).
  This extends §2's scope axis: the table deferred "active doc → swatch" to the orrery, but
  a single node as a swatch is exactly this — scoped tighter than a graphlet.
- **A menu** — a swatch as a live preview / pick target inside a command or context menu
  (cross-ref the command-registry / configurable-menus plan).
- **A djot note, as a script block** — a swatch embedded in authored prose the way a code
  block is, so a note carries a live, scoped, filtered view of part of the graph (cross-ref
  the polyglot block resolver plan).
- **An orrery card** — the same swatch promoted onto the canvas as an invokable card
  (invocation TBD — a context-menu action), the reuse Mark flagged for the shape editor.

Architecture implication: the swatch wants to be a **standalone component** (a scope + a
condition set + a form factor → a rendered, hit-testable projection), with the gloss pane,
facet panes, menus, and djot blocks all as embedders rather than gloss owning it.

**The swatch should render as document elements the chrome understands** — serval lays them
out, themes them, hit-tests them, exposes them to accessibility — **not as an opaque
`netrender::Scene` texture pasted into the DOM** (the gloss minimap's current form, Mark
2026-06-22). An opaque element can't flow, theme, or be navigated *inside* the note / menu /
pane that embeds it, which is the whole point of an embeddable swatch. So the `cartography`
projection layer (§5) supplies the **geometry** (scope, filters, arrangement — the positions
and the graphlet rules); the host renders that geometry as **DOM**. Two consequences: (1) a
node swatch's sprite is a DOM `<img>`, decoded by the host like a favicon — the netrender
image primitive is *not* needed (netrender supports images, but it is the wrong layer here);
(2) the existing Scene-based gloss minimap becomes a **candidate to migrate** onto the DOM
swatch, so all swatches are chrome-understood, not just the new ones.

---

## 2b. The swatch is the Navigator: scope-zoom, view/edit, a variant library (Mark, 2026-06-23)

§2a made the swatch a portable primitive; this resolves what it *is*. The swatch is the
Navigator itself (one surface, re-scoped and re-formed, never split, the founding rule), now
carrying an **edit layer**. A swatch is configured by **(scope, layout, lens, mode, filters)**;
a "variant" is one point in that space, and the node facet editor is variant #1.

**Two layers.** A swatch has a **view layer** (a scoped, filtered `cartography` projection
rendered as chrome-understood DOM) and an optional **edit layer** (draggable handles whose drags
mutate the scoped element through the host hit-test, the hull-vertex drag generalized). *Map* is
the view layer alone; *editor* turns the edit layer on. They toggle in place.

**Scope is a containment zoom:** node inside graphlet inside graph. You travel it two ways.
*Vertical:* zoom out to the container, or down into a selected child (graph, then a graphlet,
then a node). This adds **node at the floor** of §2's scope axis (the facet editor's scope).
*Horizontal:* the existing "arrow between graphlets," now also between sibling nodes. What
*edit* means follows the scope: a **node** edits its representation (hull, sprite, shape); a
**graphlet** edits its membership, grouping rule, and local arrangement; the **graph** edits
positions, edges, and arrangement. *Map* is the same projection with the edit layer off.

**The variant library** (view/edit is orthogonal to all of these):

- **Layout** — how the scope is arranged: minimap (landed), radial / volvelle, astroid (a
  graphlet drawn as a tag-hub with members), timeline, kanban, spectral, outline / list (MRU,
  members, TOC).
- **Lens** — what is overlaid: content peek (a node's page snapshot, the card the host already
  renders), signal heatmap (centrality / community / affinity from the signals layer), facet
  view (tags / PMEST / metrics, the inspector), and the research surfaces (Trail, Claim map,
  Provenance, Neighborhoods; see
  [graph_projections_research](../research/2026-06-22_graph_projections_research.md)).
- **Compositional** — diff / compare (two scopes side by side), sparkline (a node's or
  graphlet's trend over time), and **stacked** (§3's compose, e.g. a tag-hub plus the
  chronological view in one swatch).

**Why the gloss wins from the whole family:**

1. It is the one place you **switch variant**, re-scoping and re-forming in place (§2's
   form-factor axis generalized to "any variant"), so it stays one surface.
2. **One shared library, every embedder.** A variant the gloss shows is droppable in a facet
   pane, a djot block, or a menu, and the node editor flows back as a gloss form. Build once.
3. **It serves the orrery too.** Both ride `cartography`, so a variant is equally a gloss lens
   and an orrery mode (the projections research splits them this way).
4. The **MRU becomes "recent swatches":** re-summon a *configured* swatch (this scope, lens,
   mode), not just a node.
5. The **edit layer makes the gloss actionable:** mutate the graph from the Navigator, not only
   read it.

**Build consequence.** Build the swatch as one component configured by `(scope, layout, lens,
mode, filters)`, with the gloss as the switcher over that space and the facet / djot / menu as
fixed picks into it. The node facet shape editor (node scope, sprite + hull layout, edit mode)
is the first rung; the gloss minimap (graph scope, minimap layout, view mode) is a second; the
rest are presets of the same component. This is §2a's "standalone component," now with its
parameter space named.

---

## 3. Graphlets: latent, rule-defined views

A graphlet is **not** a stored subgraph. It is a derived, non-destructive view
of graph truth, defined by rules / filters / tags. This is the heart of the
graph scope, and it is exactly what the `cartography` projection layer exists to
do (project graph-truth + intelligence signals into a swatch without mutating the
graph). Examples Mark gave:

- **Edge-family filter** — strip the display to one kind of edge (only
  navigation, only semantic, ...).
- **Tag grouping** — nodes sharing a tag become a graphlet, drawn as a tag
  **hub-node** with the members attached to it.
- **Chronological** — connect the nodes in visit order; the whole graph as one
  chain is a valid graphlet.
- **Stacking** — compose several (a tag hub view *plus* the chronological view).
- **Arrangement rules** — layout / grouping strategy for the swatch (the
  `arrangements` crate: radial, phyllotaxis, penrose, ...).

Customizability should be "almost as broad as the graph's own, minus scene
customization": the full filter / rule / tag / projection / arrangement
vocabulary defines graphlets, but not arbitrary per-node scene styling.

**The staging chain/bus (#5) is just one latent graphlet.** Staging a set of
nodes records a latent chain (or bus) relation in staging order; gloss surfaces
it as a swatch. This resolves the card-plan §8 open question ("where does the
latent relation live?"): it lives in the same latent-graphlet space gloss reads,
not as a kernel edge. (Whether that space is gloss-owned, a forme `GraphletRef`,
or a cartography projection spec is the one open call below.)

---

## 4. Recent groupings (MRU)

Below the active swatch, a list of **recent graphlets and nodes** (the "recently
grouped" / MRU surface, matching the old "MRU / gloss / lineage swatch" note).
Picking one re-scopes the swatch to it. Staged groups land here.

---

## 5. Mapping to existing primitives

Everything the expanded gloss needs already has a home:

- **`cartography`** — swatch projections (swatch / volvelle radial / astroid
  hub-collapse / minimap thumbnail), `MinimapDescriptor`, `FormFactor::Minimap`,
  non-destructive projection of graph-truth + `IntelligenceSignals`. The swatch
  *is* a cartography projection at small scale.
- **`forme`** — graphlets (`GraphletRef`, graphlet membership), per-workbench
  forme views (different lenses / graphlet memberships per pane).
- **`arrangements`** — layout strategies a swatch can lay its graphlet out with.
- **kernel** — edge families (`RelationKind`), tags, the hidden-edge machinery
  (the filter primitives).
- **`gloss` crate** — currently the `{active doc, outline}` cell; it grows to
  host the Navigator (or the Navigator host wires the cells; see §8).

The astroid (graphlet hub-collapse) is already named internal UX vocab for one
swatch rendering of a graphlet.

---

## 6. Interaction model (sketch)

- **Scope toggle** — active-doc ↔ graph; within graph, whole ↔ graphlet.
- **Arrow between graphlets** — step through the graph's current graphlet set.
- **Form-factor toggle** — outline/list ↔ swatch.
- **Filter / tag / rule controls** — define what graphlets exist (edge-family
  strip, tag grouping, chronological, stacking, arrangement choice).
- **Act on a graphlet** — select it to open its members in the workbench (the
  staging-commit path, #5) or re-center the orrery on it.
- Gloss summarizes the **same** graph the orrery shows; selection / focus likely
  shared with the orrery (open question §9).

---

## 7. Phasing (proposed, after the cards)

The cards are the current arc; gloss is the next, and #5 staging feeds it.

1. **G1** — gloss as a real peripheral pane (it is already a `Pane` variant)
   with the scope + form-factor toggle skeleton; keep the `{doc, outline}` cell
   working.
2. **G2** — graph **swatch** mode: a whole-graph minimap via a cartography
   minimap projection of the orrery's graph.
3. **G3** — **graphlet scoping**: define graphlets via filters (edge-family,
   tag, chronological); arrow between them; pick an arrangement.
4. **G4** — **recent groupings / MRU** list + content **commentary** scope.
5. **G5** — **actions**: select a graphlet to stage / open in the workbench
   (joins #5), re-center the orrery.

---

## 7a. Implementation progress

- **2026-06-08 — G1/G2 seed landed (the graph-scope swatch).** Gloss is a
  frame-tree pane (`PaneContent::Gloss`, Ctrl+G), a sibling to the roster +
  apparatus. It renders a **whole-graph minimap swatch**: the orrery's live node
  positions + edges (`Orrery::minimap_geometry`) fit into the pane, focused node
  highlighted, themed from the chrome tokens; clicking a minimap node focuses it
  (shared selection). The swatch is host-drawn from graph geometry, not a second
  orrery (the Navigator stays one surface). It splits / resizes / maximizes /
  persists like the other panes.
- **Deferred (G3–G5 + the matrix):** the scope × form-factor toggles (active-doc
  outline, whole ↔ graphlet, outline ↔ swatch), graphlet scoping (filters / tag
  hubs / chronological), the MRU / recent-groupings list, and content commentary.
  The minimap is currently host-drawn; the design's cartography-projection backend
  (`MinimapDescriptor` / `FormFactor::Minimap`) is the eventual swap.

## 8. Open questions

- **Where latent graphlets live** — **RESOLVED (2026-06-25): forme `GraphletRef`**,
  in a per-session `SessionGraphlets` index over kernel uuids, per the
  [graphlet wiring plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md)
  (decision B; the gloss-owned store and `GraphTree` paths are closed). The lean
  here was right. See the
  [scope model reconciliation](2026-06-27_scope_model_reconciliation.md) for the
  cross-doc rulings this settled.
- **Gloss ↔ orrery shared state** — do they share selection / focus, or is gloss
  a read-only summary? (Lean: shared selection, so picking in one reflects in the
  other.)
- **LOD levels** — swatch detail vs scale (the project's LOD terminology); how
  much a minimap-scale swatch renders.
- **Doc scope vs graph scope as one surface** — a single re-scoped surface (per
  the no-split rule) vs distinct modes within it; almost certainly the former.
