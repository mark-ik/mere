# Frame Tree in Meerkat Plan

**Date**: 2026-06-08
**Status**: Planning, from a Mark + Claude session. Greenlit as the foundational
next step after the theming pass + content-type node shapes.
**Related**: [graph roster + frame taxonomy](../design/2026-06-07_graph_roster_and_frame_taxonomy.md) (§3 trees, §4 prerequisite, §5 F1), [gloss = Navigator](../design/2026-06-07_gloss_navigator_design.md), `frame` crate (`FrameLayout`), `platen` (tile tree).

Wire `frame::FrameLayout` into meerkat as the content-region pane arranger, so the
orrery and workbench (and later gloss / roster / inspector / apparatus) are
resizable **leaves** in a split tree rather than a hard orrery-XOR-workbench
toggle. This is the substrate the whole pane suite stands on.

---

## Correction (2026-06-08, after the forme-vs-frame check)

The earlier framing "the frame tree replaces the orrery-XOR-workbench toggle" was
wrong, and Mark flagged it. The [composition spine](../technical_architecture/2026-05-21_mere_composition_spine.md)
(§5, §7, §9) settles the layering:

- **`forme` / `platen`** own arrangement *inside* a pane. The orrery and the tiled
  workbench are **one authority, two projections** (Cartography vs Tree) of a
  forme arrangement. meerkat's `workbench.is_tiled()` is that **projection
  switch** — a forme/platen concern, not a frame concern.
- **`frame`** owns the *window-level pane splits*, deliberately a tree (spine §7:
  "FrameTree stays a tree on purpose... do graph the arrangement inside a
  workbench pane").

So the frame tree does **not** replace the orrery↔tiled toggle. It **adds sibling
panes** (gloss / roster / inspector / apparatus / comms) beside the **graph pane**
(the existing orrery/tiled content band, toggle intact). Mark's UX call —
**full-screen default + a maximize toggle when split** — fits this: the graph pane
is full-screen until a pane is summoned; a maximize toggle returns any leaf to
full-screen.

## Findings (from the code, 2026-06-08)

- **`frame` gives the model, not the geometry.** `FrameLayout { id, label, root }`
  with `PaneNode::{Split { axis, ratio, first, second }, Leaf { pane_id, content,
  graph_id }}`. Ops: `summon_leaf`, `close_leaf`, `reparent_leaf`,
  `set_split_ratio`, `split_at`, `iter_leaves`. `PaneContent` already has
  `Orrery`, `Workbench`, `Gloss`, `Apparatus`, `System`, `Tile`, `Custom`. There
  is **no pixel projection** (`project_frame` targets a uxtree, not rects), so
  meerkat computes each leaf's rect from the split ratios itself.
- **Today the content band is a hard XOR.** `render()` does
  `if workbench.is_tiled() { workbench scene } else { orrery.frame() }` over the
  whole band; cards anchor to the orrery node screen position plus `toolbar_h`;
  input routes on `is_tiled()`. A `divider_drag` already exists, but for the
  **tile tree's** slots inside the one workbench pane, not for frame splits.
- **Each surface assumes it owns the full band.** `orrery.frame(w, h)` and the
  workbench DOM both render at the full content size. Putting them in sub-rects
  means rendering each at its leaf size and compositing at the leaf offset, and
  translating input to leaf-local coordinates.

---

## Phases (corrected model)

- **F1.1 Model + geometry.** App holds a `FrameLayout` for the content region.
  Default: a single leaf = the **graph pane** (the existing orrery/tiled band; its
  projection toggle is untouched). A `leaf_rects(&layout, band_rect) ->
  Vec<(PaneId, &PaneContent, Rect)>` walks the split tree, splitting each rect by
  `axis` + `ratio` with a divider gutter reserved between siblings.
- **F1.2 Per-leaf render.** Walk the leaves; render each pane's content into its
  rect. The graph pane renders the orrery (or tiled workbench) into its leaf rect
  — sub-rect compositing + cards anchored within the leaf. Other panes render into
  their own rects.
- **F1.3 Input by leaf.** A press / move / wheel resolves the leaf under the
  cursor and dispatches in leaf-local coordinates.
- **F1.4 Summon / close panes + dividers + maximize.** A pane is summoned beside
  the graph pane (`summon_leaf`), closed (`close_leaf`), resized by dragging a
  frame divider (`set_split_ratio`), and any leaf maximizes to full-screen and
  back (a maximized-pane override on top of the layout). The graph pane is
  full-screen until a pane is summoned. The orrery↔tiled toggle stays inside the
  graph pane (unchanged).
- **F1.5 Persistence.** The `FrameLayout` (+ which leaf is maximized) saves +
  restores with the session (a small frame sidecar beside view-intent).

**F2 (after F1, separate pass):** the summon-able panes get real content (gloss,
roster, …) and a shellbar to summon / arrange them. Tear-out (multi-window) is
later still.

---

## Scope boundary for this pass

In: the frame tree as the window-pane arranger around the graph pane, summon /
close sibling panes, h/v splits, draggable frame dividers, a maximize toggle,
persistence. Out: the shellbar UI (panes summoned via a keybind / minimal control
for now), the full gloss / roster / inspector / apparatus content (F2), tear-out
to a new window (later). The orrery↔tiled projection toggle is **out of scope**
(it stays as-is — a forme/platen concern, not a frame one).

---

## Open decisions

1. **First sibling pane** — **resolved (Mark): a minimal roster node-list pane.**
   Each graph node's title / url / content type, scrollable, selecting a row
   focuses the node. The design doc calls the roster "largely a presentation layer
   over existing primitives," so it is close to a stub in effort and starts the
   R-phases early.
2. **Default split + summon side**: horizontal, graph pane left, the summoned pane
   right at ~0.3-0.4. (Lean: yes; tunable.)
3. **Persistence home**: a small frame sidecar keyed by the default frame id,
   beside view-intent. (Lean: yes.)

---

## Risks

- The invasive part is rendering the orrery / workbench into **sub-rects** (offset
  compositing, leaf-local input) and re-anchoring the cards. Expect churn in
  `render.rs` + `input.rs`.
- Two divider concepts now coexist (frame splits vs the workbench tile tree's
  slot dividers); keep their hit-tests + drags clearly separated.
- The orrery's centering / camera assumes a stable band size; resizing it per
  split needs the recenter-once logic to behave under leaf resizes.

---

## Progress

- 2026-06-08: Plan written. Frame crate API mapped; current render/input XOR
  located.
- 2026-06-08: forme-vs-frame check (Mark's prompt) → the Correction above; plan
  reframed to "frame adds sibling panes; the orrery↔tiled toggle stays." Mark's
  calls: full-screen default + maximize toggle; first sibling pane = a minimal
  roster node-list.
- 2026-06-08: **F1.1–F1.3 + roster landed and confirmed by Mark.** A `frame_layout`
  and `maximized_pane` in App; `frame_view` geometry (leaf_rects / divider_rects /
  pane_path, gutter, maximize override) with tests; `Roster` added to the frame
  crate's `PaneContent`; the graph pane renders into its leaf rect (orrery /
  tiled / cards / placements all offset by the leaf, card anchoring generalized to
  a band rect); a `roster` module (serval-DOM node list, themed from chrome
  tokens) renders into its leaf with row hit-testing. Controls: Ctrl+R summon /
  close roster (right split, graph keeps ~0.66), drag the frame divider to resize,
  Ctrl+M maximize the pane under the cursor, click a roster row to focus its node
  (shared selection). All tests green (frame_view 3, meerkat 31+44). Uncommitted.
- 2026-06-08: **F1 committed** (`4b20ed2`).
- 2026-06-08: **F1.5 persistence landed.** `session-runtime::frame_layout_store`
  (save / load `frame.json` beside `graph.json`, atomic write, native-only) with
  round-trip tests; meerkat restores the layout on launch (advancing the pane-id
  counter past the restored max) and saves it in `save_session` (on exit + graph
  change). The maximized state stays transient (un-maximized on restart). All
  green (session-runtime 67). **F1 complete.**
- Backlog: roster scroll (overflow clips on big graphs); R-phase roster (edges /
  fields / drill-through); a shellbar to summon panes (F2, replacing the Ctrl+R
  keybind); tear-out to a new window.
