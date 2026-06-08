# Gloss = the Navigator (design)

**Date**: 2026-06-07
**Status**: Design, from a Mark + Claude session. Supersedes the narrow gloss
v0 (a document outline strip) by expanding gloss into the **Navigator**: the
single configurable summary surface. No code yet.
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

- **Where latent graphlets live** — gloss-owned graphlet store vs `forme`
  `GraphletRef` vs a `cartography` projection-spec list. (This is the card-plan
  §8 / #5 open call, now centralized here. Lean: reuse `forme` graphlets +
  cartography projection specs rather than a new store, so gloss reads existing
  structure.)
- **Gloss ↔ orrery shared state** — do they share selection / focus, or is gloss
  a read-only summary? (Lean: shared selection, so picking in one reflects in the
  other.)
- **LOD levels** — swatch detail vs scale (the project's LOD terminology); how
  much a minimap-scale swatch renders.
- **Doc scope vs graph scope as one surface** — a single re-scoped surface (per
  the no-split rule) vs distinct modes within it; almost certainly the former.
