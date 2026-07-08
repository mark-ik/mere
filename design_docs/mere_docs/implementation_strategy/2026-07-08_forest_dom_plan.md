# Forest dom (design step 3): one ScriptedDom, N window roots

**Date**: 2026-07-08
**Status**: Planning, pre-implementation. The gate below (is state-preserving tear-out
wanted now?) should be answered before F0.
**Parent**: [one_state_n_windows_design](../design/2026-07-05_one_state_n_windows_design.md)
— this executes its **step 3**. Step 2 (the meerkat one-state migration) landed + is
verified ([archived](../../archive_docs/2026-07-07_one_state_migration/)); its watch-items
(§9 of the design doc) fold in here.
**Serval dependency**: the moveBefore engine
([serval movebefore plan](../../../../serval/docs/2026-07-05_movebefore_dom_standard_plan.md),
S1–S5 landed: `PortableKeyed` + ctx nursery + `(node, path)` handler reconciliation) is what
step 4 spends *after* this lands; the forest-dom-specific serval capabilities (F1–F2 below)
are new and tracked here until/unless they earn a serval-doc home.

## The gate: forest dom buys exactly one thing

The forest dom's only payoff is **state-preserving cross-window tear-out**. Multi-window
already works today on N independent doms (step 2 proved it headed: primary + a leaf both
render from one `ServalMultiRunner`). What the forest unlocks is dragging a *scrolled,
focused, playing* tile from window A to window B and having it keep its DOM node, scroll
position, focus, and animation state — because `move_before` is intra-document and **throws
across documents** (the standard's own guarantee, which is why the design chose the forest
topology). That is the sticky-note and rekey arms of the tear-out trichotomy; the plain leaf
arm (fresh content in a new window) needs none of it.

**So decide first**: is "the tile you grabbed is the tile that lands" a must-have soon?

**Three options, not the binary a first draft implied** (sharpened 2026-07-08): (a) accept
**fresh-build** on move — N doms, nothing survives, the status quo; (b) the **forest dom**,
this plan, full DOM-identity survival; or (c) a **middle path** on N doms — surface-lane
content tiles (the pelt/external-texture workbench tiles) already move by member→window
reassignment, so a web tile keeps its live page and (with the scroll carry) its scroll
position **today, without the forest dom**, while a new cross-tree key registry could carry
*view-side* state for DOM-lane tiles but never their DOM node / scroll / layout. The middle
path is detailed in [portable_tiles_plan](2026-07-08_portable_tiles_plan.md).

The sharpened read: most of the *felt* tear-out payoff (a web tile keeping its live state) is
reachable on N doms via the surface lane, so the forest dom earns its cost **specifically for
DOM-lane identity survival** (settings panes, document-lane cards, chrome subtrees). If that
narrower prize is wanted, the forest dom is the next piece and the rest of this plan assumes
yes; if not, the surface-lane path (portable_tiles P0) banks the common case first and this
waits.

## The leverage: the runner API already anticipates it

`ServalMultiRunner` already exposes `dom(id) -> Option<DomHandle>` and `root(id) ->
Option<NodeId>` (`multi.rs:296/301`). Today `dom(id)` is a per-projection dom and `root(id)`
is that dom's document root. The forest dom is mostly an **internal reimplementation**: all
projections share one `ScriptedDom`, and `root(id)` returns a distinct **window-root
element** within it. meerkat then lays out from `multi.root(pid)` instead of `view.dom`'s
document root.

That asymmetry is the point: unlike the Slice-3 flip (a state-ownership rewrite touching ~250
sites), the forest dom is a **render-path rewire** with a stable public seam. The consumer
churn is small if we hold that line.

## The work: three serval capabilities + one meerkat rewire

1. **RunnerTree mounts at a node** (serval / xilem-serval). `build`/`rebuild` attach under a
   given window-root, not the document root. `push_projection` (currently `(dom: DomHandle,
   logic)`, `multi.rs:100`) changes so the runner owns the **shared** dom and each push mints
   a window-root child + builds the projection there. Focus/capture stay per-tree (already
   true). API shape for meerkat (`dom(id)` shared, `root(id)` the window-root) is unchanged.
2. **Subtree layout at (root, viewport, sheet)** (serval-layout). Each window gets its own
   `PaneSession` laying out *its* subtree from `multi.root(pid)` at *its* size / DPI /
   resolved sheet. This is the piece that mostly does not exist: `PaneSession` today lays out
   a whole dom at `w×h`. `render/paint.rs`'s `ChromeRasterPlan::Partitioned` (base texture +
   orrery-subtree texture) is the *rasterization* miniature of "partition one document's
   output into regions" — it proves the shape but not the layout/mutation half.
3. **Mutation routing by root containment** (serval-scripted-dom). One dom means one mutation
   queue; each mutation must attribute to a window-root so only that window relayouts. The
   dom tracking per-node root membership beats meerkat walking ancestors per mutation. This is
   also where the payoff lives: a cross-root `move_before` (the portable-tile move) is a node
   changing its root ancestor, so routing must **cooperate with** the moveBefore `Moved`
   vocabulary serval already has, not fight it.
4. **meerkat rewire.** One shared dom on `Shell` (handed to `multi` at boot); a per-window
   `PaneSession` keyed to `multi.root(pid)`; drain-then-route the one mutation stream per
   root. `WindowView.dom` (per-window today) becomes a handle to the shared dom; the
   window's distinct part is `multi.root(pid)`.

## F0: spike the riskiest assumption before slicing

The load-bearing unknown is #2 + #3. Everything else follows if they hold and is blocked if
they don't, so the first move is a **serval-side spike, not more plan**:

> One `ScriptedDom` with two sibling roots; two `PaneSession`s each laying out one root at its
> own size; mutate one subtree; confirm **only that session relayouts** and the other's
> retained layout is untouched. Then move a node from root A to root B (`move_before`) and
> confirm both sessions see the re-root correctly.

**Done when**: the spike prints per-session relayout counts that isolate correctly. If
subtree-scoped layout secretly forces a full-document relayout, *that* is the blocker to
surface at once (per the debugging doctrine: runtime-verify, surface blockers early) — it
would mean serval-layout needs a real per-root incremental session, a bigger lift to scope
before committing.

## Slices (serval-first, same-DPI before multi-DPI, green checkpoints)

- **F1 (serval).** Multi-runner owns the shared dom; `push_projection` mints a window-root;
  RunnerTree mounts there. **Done when** the existing `tests.rs::multi` receipts
  (one-update-projects-everywhere, dispatch-in-A-updates-B, per-window focus,
  remove-projection teardown) stay green with two projections in **one** dom.
- **F2 (serval-layout).** The F0 spike hardened into real per-root sessions + mutation
  partition (the scripted-dom root-attribution). **Done when** two roots lay out + rasterize
  independently and a mutation to one does not touch the other's layout, under test.
- **F3 (meerkat).** Shared dom on `Shell`; per-window `PaneSession` from `multi.root(pid)`;
  routed draining. **Done when** the headed check (primary + leaf, the step-2 drive on
  mk-harness) renders from **one** dom at the **same** size/DPI, tests green. Fold in here,
  not later:
  - **OQ-3 a11y salting.** The instant the dom collapses, `NodeId::raw()`'s per-*doc* tag is
    shared, so two windows on the same graph can mint colliding a11y ids. Fold `ProjectionId`
    into the chrome-a11y-id salt **in this slice**.
  - **The dual-collection assert.** `Shell.windows` (HashMap) and `AppState.windows` (Vec)
    are index-aligned (`ProjectionId(i) == windows[i]`) by append-together / tombstone /
    never-pop; this slice churns that wiring, so add the debug assert now.
- **F4 (multi-DPI).** Per-window DPI/viewport/cascade. **Deferred** until a real multi-monitor
  case wants it — do not gold-plate the per-window cascade before F3 banks the topology.
- **Step 4 (portable tiles)** then becomes a same-document `move_before` for **DOM-lane** tiles
  and is finally reachable — now its own plan:
  [portable_tiles_plan](2026-07-08_portable_tiles_plan.md) (which also carries the surface-lane
  P0 that needs no forest dom). **Done when** a torn-out DOM-lane tile keeps its DOM NodeId +
  scroll (`view.scroll[member]`, the one WindowView bit that must follow the move — rects
  self-heal per frame, textures are perf) across the window boundary, target apply scoped.

## Risks / watch-items

- **Subtree layout is the make-or-break** (F0). If serval-layout can't isolate per-root
  relayout, the forest dom's perf story collapses (every keystroke in window A relayouts
  window B's whole subtree). Spike before promising.
- **Rebuild cost is O(N) on a shared edit** (design §9 watch-item, unchanged by the forest): a
  crawl-chip change rebuilds every projection. Cheap at N=1–3; if it ever bites, dirty-flag
  the shared reads. The forest does not fix this and does not worsen it.
- **a11y id collision** (OQ-3) is a *correctness* bug the moment the dom unifies, not a
  polish item — hence folded into F3, not deferred.
- **NodeId arena unification.** One dom = one NodeId arena, so any code assuming per-window
  NodeId scoping needs an audit at F3 (the a11y salt is the known one; sweep for others).
- **Keep the public seam stable.** If `dom(id)`/`root(id)` hold, meerkat stays a render-path
  rewire. If the forest tempts a wider serval API change, re-justify it against that
  leverage.

## Progress

*(none yet — planning stage. F0 spike is the first action once the gate is answered.)*
