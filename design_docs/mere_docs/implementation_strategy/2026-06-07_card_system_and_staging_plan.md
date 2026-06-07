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

- 2026-06-07: design captured from Mark's text + schematic. Implementation not started. Physics tuning (separate) landed in the same session; omnibar focus fix pending.
