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
Code receipts: `genet/components/xilem-serval/src/runner.rs` (the runner),
`xilem-serval/src/keyed.rs` (keyed sequences), `genet/components/genet-layout/incremental.rs`
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
  `GenetAppRunner`'s own doc names it the owner of state, root, and rebuild
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

**Reframed 2026-07-05 (same day, Mark): the execution half is a web standard.**
The WHATWG DOM grew `Node.moveBefore()` (shipped in Chromium early 2025): an
atomic move that preserves what `removeChild`/`insertBefore` destroys (iframe
documents, animations, focus; custom elements get `connectedMoveCallback`).
That is the splice/graft contract as a standard, and its own constraint
ratifies §3's topology: `moveBefore` throws across documents, so same-document
is the only case, which is the case the forest dom creates. Chrome tear-out and
a page reparenting a live iframe become the same engine code path. Genet keeps
the full WPT suite on disk (`tests/wpt/tests/dom/nodes/moveBefore/`) wired into
`ports/genet-wpt` with expectations currently `"fail"`, so done conditions are
expectation flips. Plan and slices:
`repos/genet/docs/2026-07-05_movebefore_dom_standard_plan.md`.

The mechanism therefore splits in two:

- **Execution (engine, standard)**: `ScriptedDom::move_before` emitting a
  `DomMutation::Moved { node, from_parent, to_parent }`, with genet-layout
  handling `Moved` via the splice/graft path. Also fixes a live defect this
  design would have hit: today's `insert_before` already moves an in-tree node
  but emits only `Inserted`, so the source parent's session never learns the
  child left; a cross-window move would fail to invalidate the source window.
- **Recognition (view layer, still ours)**: `keyed.rs` is sibling-scoped, and
  even an in-parent reorder degrades to teardown + build under the
  `ElementSplice` cursor contract. A portable keyed subtree surviving a rebuild
  under a different parent still needs nursery-style bookkeeping so the view
  layer knows not to tear down; what the bookkeeping *does* on a match is now
  one `move_before` call rather than a bespoke park/adopt element protocol.
  Scope it to portable subtrees (tiles, cards); ordinary keyed rows keep the
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
   **Framework half landed 2026-07-06**: `GenetAppRunner`'s per-tree guts
   (dom, ctx, retained view, focus, capture) moved into a crate-internal
   `RunnerTree` with state + logic threaded per call — public API unchanged,
   every consumer untouched — and `GenetMultiRunner` (`multi.rs`) owns one
   state plus N `(logic, tree)` projections with stable `ProjectionId`s.
   One `update` rebuilds every projection; a dispatch into any window routes
   through that window's tree then rebuilds the others, so a click in window
   A updates window B in the same pass. Focus/capture stay per-window.
   Projections rebuild in insertion order (the portable parking order); the
   per-tree nursery drain confines parked elements to their own dom, so a
   cross-window move safely degrades to fresh-build until the forest dom
   (step 3). Receipts in `tests.rs::multi`: one-update-projects-everywhere,
   dispatch-in-A-updates-B, per-window focus, remove-projection teardown.
   **Consumer half done 2026-07-07** — meerkat's migration (Slices 0-3; the plan is
   archived to `archive_docs/2026-07-07_one_state_migration/`): the ShellState/Chrome split
   into `AppState` (owned `shared: SharedChrome`) vs one `WindowLocal` per window;
   `Shell.multi: GenetMultiRunner`; `WindowView.projection_id` replaced `runner`; the
   `chrome_update` fan-outs + spawn-time chip seeding deleted; `shell_view(app, i)` is the
   per-window lens (`ProjectionId(pub usize)` added genet-side for index alignment).
   Verified headless (302 meerkat + 89 xilem-serval tests) and headed (primary + slim leaf
   render from the one runner, no crash). **Step 2 is complete; steps 3-4 below remain.**
3. **Forest dom.** One ScriptedDom, N window roots, per-window sessions rooted
   per window element, mutation routing by root containment. Done when two
   windows at different sizes and DPIs lay out and rasterize from one dom.
   **Plan (2026-07-08): [forest_dom_plan](../implementation_strategy/2026-07-08_forest_dom_plan.md)**
   (gate → F0 spike → F1 genet → F2 genet-layout → F3 meerkat → F4 multi-DPI).
4. **Portable keyed adoption over moveBefore.** Engine slices first (the genet
   plan's S1/S2: the `Moved` mutation vocabulary, `move_before` semantics, the
   splice fast path), then the view-layer recognition (S5) lowering a portable
   keyed move to one `move_before` call. Done when dragging a tile to another
   window preserves its DOM nodes (same NodeId, scroll position observably
   survives), the target window's apply is scoped rather than a full recompute,
   and the trichotomy is expressed as state mutations. WPT expectation flips in
   `ports/genet-wpt` gate the engine slices.
   **Framework half done 2026-07-06** (genet plan S1-S5 all landed:
   `PortableKeyed` + ctx nursery + `(node, path)` handler reconciliation; a
   cross-parent keyed move preserves element, DOM node, view state, and live
   handlers, with one atomic `Moved`). What remains here is the consumer half:
   meerkat tiles as `PortableKeyed` children, which needs steps 1-3 above —
   in particular the forest dom, since cross-window is only same-document
   there, and the source-first rebuild order the multi-projection runner
   makes host-controllable.
   **Plan (2026-07-08): [portable_tiles_plan](../implementation_strategy/2026-07-08_portable_tiles_plan.md)**
   — the meerkat consumer half. Key finding: meerkat's tiles split into a **DOM lane**
   (folded panes / document-lane cards → `move_before`, needs the forest dom) and a
   **surface lane** (pelt/external-texture workbench tiles → member→window reassignment,
   needs no forest dom), so much of the felt payoff is reachable before step 3.

## 9. Open questions

- **OQ-1 Input routing — RESOLVED (2026-07-07 migration).** The multi-runner carries the
  window explicitly: every dispatch entry point takes `ProjectionId` first
  (`dispatch_click(pid, node, event)` / `dispatch_key(pid, event)` / `focus(pid)` /
  `set_focus(pid, node)` / `pointer_target(pid, hit)` / `wheel_target(pid, hit)`), and focus
  and pointer capture both live per-`RunnerTree`. meerkat drives them through
  `self.multi.<m>(self.view.projection_id, …)`. Carrying the window beats the root-chain
  alternative the OQ floated: no root walk today, and still correct under the forest dom.
- **OQ-2 Where recognition lives — framework RESOLVED; the rest is a step-4 build.** genet's
  moveBefore S1–S5 landed the answer as a hybrid, not an either/or: recognition is a **View
  wrapper** (`PortableKeyed`) in xilem-serval, backed by **runner-level nursery** machinery for
  the cross-tree parking. What remains is not a placement question but the meerkat consumer:
  express a tile as a portable child and map the leaf / sticky-note / rekey trichotomy onto it
  (step 4, gated on the forest dom).
- **OQ-3 A11y — open, and it carries an id-collision risk the earlier note skipped.** The
  bridge lifecycle is settled (one AccessKit bridge per window, unchanged). Two real pieces:
  (1) scope each bridge's read to its **window-root subtree** of the one dom, mechanical once
  `render_chrome_scene`'s root-partition logic generalizes; and (2) **a11y node ids must stay
  window-unique** — chrome a11y ids are salted today, but `NodeId::raw()` packs a per-*doc* tag
  on debug builds, so collapsing to one dom shares that tag and two windows on the same graph
  could mint colliding ids. The salt must fold in the `ProjectionId`.
- **OQ-4 Content actors — unaffected; the work reduces to carrying the scroll.** Actors are
  genuinely untouched (per-member, external-texture composited per window). Correction to the
  earlier phrasing: this per-window host state lives on **`WindowView`** (host bookkeeping),
  not `WindowLocal` (the projected view-state). And only some of it must follow a cross-window
  move: tile rects **self-heal** (recomputed every frame) and the tile texture cache is a
  **perf nicety** (rebuild in the target), but **`view.scroll[member]` must move with the
  tile** or a torn-out tile jumps to the top in its new window. So OQ-4 is really "carry the
  scroll offset across the tear-out."

### Watch-items surfaced by the landed migration (2026-07-07)

- **Dual-collection alignment is a load-bearing invariant.** `Shell.windows` (HashMap, host
  bookkeeping) and `AppState.windows` (Vec, projected view-state) are kept index-aligned by
  append-together / tombstone-on-close / never-pop, so `ProjectionId(i) == windows[i]`. The
  forest-dom + portable-tile work churns this area; a debug assert (`pid.0 < windows.len()`
  plus a spawn/close parity check) would stop a future refactor from silently desyncing it.
- **Shared-state edits rebuild every projection.** A change to `AppState.shared` (the crawl
  chip) rebuilds all N windows via `multi.update`; a per-window snapshot uses `update_local`
  and rebuilds one. Correct and cheap at N=1–3 (an unchanged window diffs to zero DOM
  mutations), but it is O(N) per shared tick — worth knowing the crawl actor drives N rebuilds
  when many windows are open. If it ever bites, dirty-flag the shared reads so an unchanged
  value skips the rebuild.
