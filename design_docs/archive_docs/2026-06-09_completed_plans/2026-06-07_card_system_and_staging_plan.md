# Card System + Node Staging Plan

**Date**: 2026-06-07
**Status**: Design captured from Mark (text + schematic, 2026-06-07). Not yet implemented.
**Touches**: `meerkat` (constellation, card render, input), `orrery` (focus/selection), `platen` (card scene), `gloss` (latent graphlet surfacing). Supersedes the earlier "preview-activation fork" question.

The trigger: in Cartography (graph) view today the focused node's preview card is a *live actor*, so focusing a node activates it (`needed_members()` includes `focused_member()`). You can't get the last node in a tile group back to inactive, because returning to the graph re-activates it through the preview. Mark's resolution is a richer, two-stage card plus an explicit staging step, which decouples "looking at a node" from "activating a node."

---

## 1. Two-stage card (memory → live)

A node's card has two distinct stages. Focus/select alone never spawns an actor.

### Stage 1 — snapshot card ("last visit")

- Small: **~1/9 of today's card** (maybe smaller). Floats in **next to the node**, anchored to it (moves with the node / camera).
- Content: a **scrollable snapshot of the node's last activation** — rendered from the durable content cache, not a live actor. No actor is spawned; the node stays **inactive**.
- Labelled **"last visit."**
- A **never-visited** node has no snapshot: instead a thin strip reading **"unvisited: click for preview."**

### Stage 2 — live preview card

- **Double-click** the snapshot (or the unvisited strip) → it expands into a **live actor** card.
- Size: **~1/3 of today's card** (between the 1/9 snapshot and the old full card).
- **Resizable and movable**, but **anchored to its node**.
- Scrollable and interactive; **persists on click-away** (does not vanish when you click elsewhere).
- **Multiple live preview cards** can be open at once.
- Top-right corner: an **X** (close) and a **tile button** (stage for workbench), side by side.
- This stage is what **activates** the node (spawns/keeps the content actor).

### Activation-model consequence

| State | Actor? | Node color |
|---|---|---|
| Dormant (not focused) | no | Idle/Closed |
| Snapshot card showing (focused) | **no** | not active |
| Live preview card open | **yes** | Open/active |
| Tiled in workbench | yes | Open/active |

So `needed_members()` in Cartography should derive from **open live-preview cards**, not from `focused_member()`. Focus drives the *snapshot* (cache read), not the pool.

---

## 2. Node staging → workbench

- The live preview card's **tile button** *stages* its node (it does not open a tile immediately).
- The **tile-tree button up top** (the `⊞` view toggle) **commits** all staged nodes into the workbench as tiles when pressed.
- Nodes **staged together** are linked by a **latent relation**, in **staging order**:
  - **chain** — sequential edges (A→B→C), the leading interpretation, or
  - **bus** — a shared hub linking the staged set.
- **Latent** means this relation does **not** reorganize the graph or auto-surface as a drawn edge. It is one **graphlet** among many possible ones, surfaced (or hidden) through **gloss + swatches** depending on which edge families you surface/hide, tags, and arrangement rules. Chronological/staging relations live in the same latent space as other derived relations.

---

## 3. Schematic (Mark, 2026-06-07)

Annotated over a live gopher card:
- **green** box = the small snapshot card, anchored next to the node.
- **red** box = the larger live-preview card it expands into (~1/3), same anchor.
- **orange** lines = anchor leaders from the node (3) to the card.
- **red-highlighted top `⊞` button** = the tile-tree commit for staged nodes.
- a small corner box on the card = the X + tile-button cluster.

---

## 4. Phasing (proposed)

1. **Decouple focus from activation.** Cartography `needed_members()` stops including `focused_member()`; the pool is driven by open live-preview cards (+ background-flagged). Closing the last tile returns to the graph with the node inactive (folds in the earlier tile-close fix).
2. **Snapshot card.** Render the small "last visit" card from the durable content cache (no actor); "unvisited: click for preview" strip otherwise. Anchored next to the node.
3. **Live preview card.** Double-click promotes snapshot → live actor card: ~1/3 size, resizable/movable/anchored, persistent, multiple, with X + tile buttons.
4. **Staging + commit.** Tile button stages; top `⊞` commits staged → workbench tiles.
5. **Latent staging relation + gloss.** Record the staging-order relation as a latent graphlet; surface via gloss/swatches (chain default, bus option).

---

## 5. Open questions

- **chain vs bus** default for staged-together nodes (Mark leans chain, in staging order). Make it a gloss/arrangement choice rather than a hardcode.
- Exact sizes (1/9, 1/3) — calibrate against readability once rendered.
- Snapshot fidelity: static texture of last scene vs a re-laid cache render.
- Where staged-but-uncommitted state lives (per-card flag vs a staging set on the host).

---

## 6. Progress

- 2026-06-07: design captured from Mark's text + schematic.
- 2026-06-07 (same session), landed + confirmed at runtime:
  - **P1** close-last-tile → graph, node inactive (`d89dba7`).
  - **Core** decouple focus from activation + snapshot↔live loop, double-click
    *card* → live / double-click *node* → workbench, shift-click multi-select
    (`d66b76c`).
  - **#1** anchor the card next to its node; live = medium card, snapshot = small
    thumbnail (uniform-scaled, captured-width fixed); navigating promotes to live
    (`4806035`).
  - **#2** close (X) button on the live card (`3d09f85`).
  - **#3** unvisited dashed "Double-click to load" placeholder (`19ff894`).
  - Adjacent: physics tuning + omnibar focus fix (separate commits).
- Pending: **#4** cross-session snapshots (§7), **#5** staging + gloss (§8).

## 7. Cross-session snapshots (#4) — design

The in-session snapshot is the last live activation's retained scene; it does not
survive a restart. To persist it must be **re-derivable from durable state**:

- **Render host-side, not from a retained scene.** Give the host an
  `EngineRegistry` (today only the content actor has one) + a minimal (empty)
  resource loader, and render the snapshot through
  [`card::render_content_scene`]. That covers **both** fetched pages (from the
  durable content cache, which already persists bytes) **and** synthesized pages
  (`mere://welcome` etc., regenerated from the URL) — `render_content_scene`
  already falls back to the synthesized document, so one path handles both. This
  also lets the in-memory snapshot retention (the `constellation` `snapshots`
  map) retire.
- **"Visited" is durable.** Showing snapshot-vs-unvisited needs a persisted
  notion of which nodes have been visited. **Lean: derive it from the node's
  nav-lineage history** — a node with history *is* a visited node — rather than a
  separate persisted set.
- Cache the host-rendered snapshot texture by URL so it is not re-rendered every
  frame; invalidate on content/size change.

Open: nav-lineage-derived "visited" vs a separate visited-set; snapshot staleness
when cached content changes.

## 8. Staging + gloss (#5) — implementation scope

The deliberate-staging flow, to be built after the cards settle. **No code yet.**

**Staging state.** A host-owned ordered set `staged: Vec<GraphMemberId>` (staging
order is meaningful). A live card's **tile button** (top-right, beside the X)
toggles its node in `staged`; staged cards carry a marker (e.g. a filled tile
glyph). Staging is host UI state, not graph data.

**Commit.** The top `⊞` button commits: when `staged` is non-empty it opens the
staged nodes into the workbench as tiles **in staging order**, then clears
`staged`. When empty it keeps today's behavior (open the selection's working
set). So `⊞` becomes "commit staged, else open selection."

**Latent staging relation.** On commit, record a relation among the staged set in
**staging order**:

- **chain** — sequential `A→B→C` (the lean default), or
- **bus** — a shared hub linking the set.

This relation is **latent**: it does *not* add drawn edges or reorganize the
graph. It is one **graphlet** among many, surfaced (or hidden) by **gloss +
swatches** per the surfaced/hidden edge families, tags, and arrangement rules.

**Open / to decide:**

- **Where the latent relation lives.** Options: (a) a gloss-owned graphlet store
  (keeps it fully out of the kernel graph), (b) a kernel edge family flagged
  `latent`/hidden (reuses the existing hidden-edge machinery + `ArrangementRelation`
  vocabulary). Lean (a) for "doesn't touch graph truth," but (b) reuses real
  infrastructure — needs Mark's call.
- chain vs bus default (lean chain, staging order; make it a gloss/arrangement
  choice, not a hardcode).
- Whether commit clears `staged` or keeps it for re-commit.
- The tile-button glyph + staged-state affordance on the card.
