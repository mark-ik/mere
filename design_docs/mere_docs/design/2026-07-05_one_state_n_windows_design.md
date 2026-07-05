# One State, N Windows: windows as projections (design)

**Date**: 2026-07-05
**Status**: Design statement from a Mark session, verified against the live runner,
keyed-view, and incremental-layout code. Not a build plan yet; the sequencing at the
end names done conditions, not slices.
**Related**: [multi_window_plan](../implementation_strategy/2026-06-10_multi_window_plan.md)
(built the current N-runner shape this supersedes the framing of; MW1-MW3 machinery
stays), [tearout_composability_plan](../implementation_strategy/2026-06-19_tearout_composability_plan.md)
and [tearout_gestures_plan](../implementation_strategy/2026-06-24_tearout_gestures_plan.md)
(the trichotomy this makes state-level), [swatch_primitive_design](2026-06-27_swatch_primitive_design.md)
(swatches as view-instances over shared truth: the same shape one level down).
Code receipts: `serval/components/xilem-serval/src/runner.rs` (the runner),
`xilem-serval/src/keyed.rs` (keyed sequences), `serval/components/serval-layout/incremental.rs`
(`graft_subtree`, the splice path), `meerkat/src/shell_access.rs::build_window_view_for`
(the sync-seeding exhibit), `meerkat/src/render/cards.rs::render_chrome_scene`
(mutation routing by root, the miniature).

## 1. The thesis

One app state, N windows, each a projection. The runner owns app state and a
retained view tree; a ScriptedDom is just a target. Each window's view function is
a lens over the shared state. Multi-window synced panels stop being a sync feature
and become the architecture: there is nothing to sync because there is one state.
Tear-out becomes "re-parent this view subtree to another window's root," and if
keyed view identity survives the move, the moved card keeps its DOM identity and
its internal state across it. The tear-out trichotomy falls out of the framework
instead of being built on top of it.

## 2. What is true today (the receipts)

- **N windows = N ScriptedDoms + N runners + N ShellStates.** `shell_runner(dom,
  chrome)` is called once per window; `build_window_view_for` mints a fresh
  `ScriptedDom` per spawned leaf.
- **Shared truth is host-mirrored into each window per frame.** `SharedState` on
  `Shell` is the real state; `chrome_update` fan-outs push it into each window's
  `ShellState`. Spawn-time code hand-seeds the new leaf's sync and crawl chips from
  the primary "so it shows real standing immediately." That seeding plus the
  fan-out is the sync feature this design dissolves.
- **ShellState mixes two kinds of state.** Projected-shared truths (chrome
  location, sync chip, session chips, comms) sit beside genuinely window-local
  view state (orrery rect, pane rects, wheel one-shots). The mirroring exists
  because the shared half lives in N copies.
- **The runner is one (dom, state, logic, view, root, focus, capture).**
  `ServalAppRunner`'s own doc names it the owner of state, root, and rebuild
  cadence. Nothing in xilem_core schedules; the runner is the artifact.

## 3. The topology fork, resolved

The thesis as spoken holds two halves that pull apart: "one runner drives several
DOMs" and "keyed identity maps onto graft_subtree." They conflict:

- `ScriptedDom` NodeIds are per-arena. A node cannot move between two doms. With
  N doms, tear-out is always teardown in the source dom and rebuild in the target
  dom. Keyed identity could at most carry view-side retained state across (with a
  new cross-tree key registry); DOM nodes, scroll positions, text buffers, and
  layout never survive.
- `graft_subtree` is `BoxTree::graft_subtree`, an intra-session splice: when a
  structural batch replaces a subtree whose outer size holds, the session re-lays
  just that subtree scoped and grafts its box tree plus shaped text into the
  retained paint side-table (`Applied::Spliced`). It requires the move to be a
  mutation over a dom the target session already owns.

So the strong form of the thesis is **one dom as a forest**: one `ScriptedDom`,
one window-root element per OS window under the document root, one runner, one
state. Per window: a lens view function, a layout session rooted at that window's
element with its own viewport and DPI-baked sheet, a rasterizer, and its own
focus/pointer-capture bookkeeping (OS focus is per window). Mutations drain once
and route to the owning window's session by root containment. Meerkat already
contains the miniature: `render_chrome_scene` routes drained mutations by
`mutation_is_under_root` and emits the orrery as a session subtree
(`scene_from_session_subtree`, emit-excluding-subtrees).

Thread-compatibility is free: every window already renders on the kernel thread,
and the dom handle is `Rc<RefCell<ScriptedDom>>`.

## 4. What tear-out honestly gets

With one dom, tear-out is a real `insert_before` re-parent under the target
window's root. What survives the move:

- **DOM node identity**: per-node scroll, text input buffers, handler
  registrations, a11y node identity.
- **View-side retained state**, if the view move is expressed as a keyed move
  (see §5).
- **Scoped relayout**: the target session sees an ordinary structural batch. A
  moved subtree lays out once in the target (different viewport, possibly
  different DPI sheet make this unavoidable), but relayout is scoped to the
  subtree, and the source window's removal is incremental too.

"Keeps its layout" overstates it. The honest claim: keeps its identity and its
internal state, relayout scoped to the moved subtree rather than the whole target
window. The splice fast path applies when its outer-size precondition holds; a
fresh insert into a session that never laid the subtree takes the fuller
incremental path once.

## 5. The one genuinely new mechanism

Everything above is generalization of existing machinery. The one new piece is
**cross-parent keyed adoption**. `keyed.rs` exists, but its own doc states the
limit: keys are sibling-scoped, and even an in-parent reorder degrades to
teardown + build under the `ElementSplice` cursor contract. A cross-window move
crosses parents by definition.

The concrete mechanism: a runner-level nursery. At rebuild start, teardown of a
keyed subtree marked **portable** parks its (element, view_state) in the nursery
keyed by K instead of destroying it. A build during the same rebuild with the
same K adopts the parked pair and re-parents the existing DOM node rather than
creating fresh ones. At rebuild end, unclaimed parked entries get real teardown.
Scope it to portable subtrees only (tiles, cards); ordinary keyed rows keep the
current contract.

## 6. The trichotomy as state operations

- **Leaf (move)**: the tile's window assignment changes in state; the forest diff
  moves the keyed subtree; the nursery preserves it across the move.
- **Sticky-note**: state grows a note object where the tile was; both lenses
  re-render from the same truth.
- **Rekey**: the subtree's binding changes in state. Same key with new binding
  keeps the element and changes its content; a deliberate new key is a deliberate
  rebuild.

No per-gesture wiring: each is a state mutation plus the ordinary rebuild.

## 7. What dissolves in meerkat

- The `chrome_update` mirror fan-outs and spawn-time chip seeding.
- The shared half of `ShellState` (N copies of chrome truths collapse into the
  runner's one state; what remains per window is honest view-local state: rects,
  wheel one-shots, focus).
- Multi-window synced panels as a feature category. The panels are lenses.

`SharedState`/`WindowCtx` keep their roles on the host side; the split this
forces (AppState vs WindowLocal) matches the seam `WindowCtx` already draws
between `shared` and `view`.

## 8. Sequencing (done conditions, not slices)

1. **State split.** ShellState divides into one shared AppState and per-window
   WindowLocal; lens signature `Logic: FnMut(&AppState, &WindowLocal) -> V`.
   Done when the chrome truths live once and every `chrome_update` mirror
   call site is deleted.
2. **Multi-projection runner.** One runner owns the state and N projections
   (still N doms at this step). Done when one `update` diffs every window's
   lens and no host-side mirroring remains.
3. **Forest dom.** One ScriptedDom, N window roots, per-window sessions rooted
   per window element, mutation routing by root containment. Done when two
   windows at different sizes and DPIs lay out and rasterize from one dom.
4. **Portable keyed adoption.** The nursery protocol; tiles/cards marked
   portable; tear-out as re-parent. Done when dragging a tile to another window
   preserves its DOM nodes (scroll position observably survives) and the target
   window's apply is scoped rather than a full recompute, with the trichotomy
   expressed as state mutations.

## 9. Open questions

- **OQ-1 Input routing**: runner dispatch takes a hit NodeId today; per-window
  focus and capture need the dispatch entry points to carry the window (or
  resolve it from the root chain).
- **OQ-2 Where adoption lives**: in xilem-serval core (a `portable_keyed`
  wrapper) or as runner-level machinery outside the view contract.
- **OQ-3 A11y**: per-window AccessKit bridges currently read per-window doms;
  under a forest they read per-window subtrees of one dom. The bridge split by
  window root needs its own look.
- **OQ-4 Content actors**: unaffected in principle (external textures composite
  per window), but tile rects and scroll maps in WindowLocal must follow the
  moved subtree.
