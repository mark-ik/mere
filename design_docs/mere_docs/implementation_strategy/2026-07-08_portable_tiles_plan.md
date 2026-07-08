# Portable tiles: cross-window DOM identity (design step 4)

**Date**: 2026-07-08
**Status**: Planning, downstream of the forest dom. Same-document cross-window moves are
**blocked on step 3** ([forest_dom_plan](2026-07-08_forest_dom_plan.md)); this plan captures
the target + the open questions so the shape is settled before step 3 lands.
**Parent**: [one_state_n_windows_design](../design/2026-07-05_one_state_n_windows_design.md)
— **step 4** (§8), the mechanism in §5, the trichotomy in §6.
**Serval dependency (LANDED)**: the moveBefore engine
([serval movebefore plan](../../../../serval/docs/2026-07-05_movebefore_dom_standard_plan.md),
S1–S5): the `PortableKeyed` View wrapper, the `Moved` mutation, the ctx **nursery** (parks a
departing keyed subtree until the target adopts it), and `(node, path)` handler
reconciliation. A cross-parent keyed move already preserves element + DOM node + view state
+ live handlers with one atomic `Moved`. This plan is the **meerkat consumer** of that.
**Gesture source**: [tearout_gestures_plan](2026-06-24_tearout_gestures_plan.md) owns the
user-facing tear-out gestures (drag = leaf, Shift = branch, Ctrl+Shift = fork — the *graph
scope* axis); this plan owns what happens to the tile's *DOM identity* when a gesture moves
it. The two axes are orthogonal.

## What "done" is (design §8 step 4)

Dragging a tile to another window preserves its DOM node (same `NodeId`), its scroll position
observably survives, the target window's apply is scoped rather than a full recompute, and the
§6 trichotomy is expressed as plain state mutations. The serval half is done; the meerkat half
is the below.

## The first question, and it is load-bearing: which content is even DOM-lane?

The design models a tile as a **keyed DOM subtree** the forest diff moves. meerkat's content
is split across two lanes, and only one is DOM:

- **DOM-lane** (shell document subtrees): the folded panes (roster, gloss outline/recent/
  minimap-nodes, list panes, settings tiles) and the document-lane focus cards. These *are*
  `ScriptedDom` subtrees under the window root, so `PortableKeyed` + a cross-window
  `move_before` applies directly. Scroll, text buffers, focus, a11y identity survive.
- **Surface-lane** (pelt + external texture): the **workbench content tiles** render through
  `WindowView.pelt_shell` (a pelt `TileShell` surface) and composite per-member **actor**
  textures (`tile_textures`), not shell DOM. Their *content* is window-agnostic already — the
  content actor is per-member and does not care which window composites its external texture —
  so a cross-window move of a workbench tile's *content* survives for free by reassigning the
  member's window, **without** the forest dom or moveBefore. What is NOT DOM here is the tile
  chrome (tab, frame), which pelt owns per surface.

So step 4 is not one mechanism but a **lane split**:
1. **DOM-lane tiles** move by `PortableKeyed` + `move_before` (needs the forest dom).
2. **Surface-lane tiles** move by **member→window reassignment** (the actor re-composites in
   the target; needs no forest dom). The scroll offset is the one host-side bit to carry
   (below).

Reconciling this with the design's single "keyed subtree" model is the first thing to settle:
either the workbench migrates into the shell DOM (a much larger change, probably not worth it),
or step 4 is explicitly the two-lane story above (recommended: it matches the built
architecture and keeps the forest dom's blast radius to the DOM-lane content).

## The §6 trichotomy as state operations (both lanes)

Once a tile is portable, the three modes fall out of state mutations plus the ordinary rebuild,
with no per-gesture wiring:

- **Leaf-move**: the tile's window assignment changes in `AppState` (which `WindowLocal` owns
  it). DOM-lane: the forest diff moves the keyed subtree A-root → B-root; the nursery preserves
  it. Surface-lane: the member's window reassigns; the actor composites in B.
- **Sticky-note**: state grows a note object where the tile was; both windows' lenses render
  the same truth (a synced panel, which is free under one state).
- **Rekey**: the tile's binding changes. Same key + new binding keeps the element and swaps its
  content; a deliberate new key is a deliberate rebuild.

## The host-side state that must follow the move (OQ-4)

The DOM move (or the actor reassignment) does not carry meerkat's per-window `WindowView`
bookkeeping. Of it:

- **`view.scroll[member]` must follow the tile** — else a torn-out tile jumps to the top in its
  new window. This is the one genuinely load-bearing carry.
- **`tile_rects` self-heal** (recomputed every frame) — nothing to carry.
- **`tile_textures[member]`** is a perf cache — rebuild in the target, or hand it over for a
  frame-perfect handoff (a nicety, not correctness).

## The middle path (#1): portability without the forest dom

Recorded because it is a real option and the gate on step 3 may keep the forest dom out for a
while (see the [forest_dom_plan](2026-07-08_forest_dom_plan.md) gate, where this is the third
option between fresh-build and the full forest dom):

- **Surface-lane tiles already have it** — member→window reassignment needs no forest dom, so a
  torn-out *content* tile keeps its live page + (with the scroll carry) its scroll position
  today, on N doms. This is most of what a user feels when they tear out a web tile.
- **DOM-lane tiles** cannot keep DOM-node identity across N doms (per-arena `NodeId`s;
  `move_before` throws cross-document). The ceiling on N doms (design §3) is a **new cross-tree
  key registry** carrying *view-side* retained state only: a keyed component's internal state
  survives, but the DOM node, scroll, text buffer, and layout rebuild fresh in the target.
- **Recommendation**: do NOT build the cross-tree key registry speculatively. It is a distinct
  new mechanism (a cross-*document* state carrier, not the same-document nursery serval already
  has), and for the content that matters most (web tiles) the surface-lane path already
  preserves the live state. Build it only if a specific **view-heavy** DOM-lane tile (a
  stateful widget, not a web page) needs its state to survive a tear-out before the forest dom
  lands. Otherwise the forest dom is the better investment for the general DOM-lane case.

## Slices (after step 3, except P0)

- **P0 (now, no forest dom needed): surface-lane portability + the scroll carry.** Make a
  cross-window workbench-tile move reassign the member's window and carry `view.scroll[member]`.
  **Done when** a torn-out web tile keeps its live page and scroll position in the new window (a
  headed drive on mk-harness). This banks the felt payoff early and is independent of the forest
  dom.
- **P1 (needs step 3): DOM-lane tiles as `PortableKeyed` children.** Wrap the portable shell-DOM
  subtrees (start with settings tiles or a document-lane card) in `PortableKeyed` keyed by
  member. **Done when** a cross-window move of a DOM-lane tile keeps its `NodeId` + scroll, with
  the target apply scoped (not a full recompute), under test + a headed drive.
- **P2: the §6 trichotomy wired to the gestures.** The tear-out gesture (leaf/branch/fork) sets
  the tile's window assignment / note / rekey in `AppState`; the portable machinery lowers each
  to the right DOM operation. **Done when** all three modes produce the right survival behavior
  from state alone, no per-gesture DOM wiring.

## Risks / watch-items

- **The lane split is the design-vs-build reconciliation** — settle it (two-lane story vs
  workbench-into-DOM) before P1, or P1 chases a model meerkat does not have.
- **Blocked on the forest-dom gate.** P1/P2 cannot start until step 3 is committed; P0 can.
- **Do not over-build the middle path** (see recommendation) — it is a real option, not a
  default.
- **Scroll carry is the recurring load-bearing bit** across both lanes and both the middle path
  and the full path; get it right once (a `member → scroll` handoff on window reassignment) and
  reuse it.

## Progress

*(none yet — planning stage, downstream of the forest-dom gate. P0, the surface-lane scroll
carry, is the one slice that can start independently.)*
