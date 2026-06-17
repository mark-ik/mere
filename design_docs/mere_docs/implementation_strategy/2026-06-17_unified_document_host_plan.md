# Unified Document Host Plan

Status: **planned**. A code-verified argument that `xilem_serval`'s role in
meerkat should grow from "reactive toolkit for the chrome strip" to "host of the
whole document shell", plus the staged path to get there and the one product
decision it turns on.

Sibling/converging docs:

- [window_composition_plan](2026-06-11_window_composition_plan.md) — orrery
  (authority) vs panes (views); this plan is the rendering-engine half of that
  same reshape (one document vs many).
- [host_wiring_grabbag_plan](2026-06-11_host_wiring_grabbag_plan.md) — G1
  composition-runway items (transform-aware hit-test, `on_wheel`, pointer
  cancellation) feed Phase 2 here.
- [modular_integration_plan](2026-06-02_modular_integration_plan.md) — the
  graph-rooted projection model (orrery/workbench/gloss are projections of graph
  truth); the surfaces this plan unifies are those projections.
- [mere_composition_spine](../technical_architecture/2026-05-21_mere_composition_spine.md)
  — truth → forme → platen → surface; Phase 2's tiles question lands on
  `platen-view`.
- [two_natured_kernel_brief](../research/2026-05-30_two_natured_kernel_brief.md)
  §4 — content-authoritative / experience-derived; the orrery as a derived
  component store with `gyre` as simulator. The forcing function for Phase 2.
- [cartography_aether_layout_seam](../technical_architecture/2026-05-29_cartography_aether_layout_seam.md)
  — `gyre` simulates a layout; the two-hit-test split.
- The orrery-as-element design originates in the archived
  `serval_as_host_evaluation` §6 (in
  [`archive_docs/2026-06-09_completed_plans/`](../../archive_docs/2026-06-09_completed_plans/)).
- Engine-side work belongs in serval's own
  `docs/2026-05-27_serval_as_host_xilem_serval_plan.md`; this plan is the
  meerkat-side consumer view and the engine asks it generates.

---

## The thesis: a bigger role for `xilem_serval`

`xilem_serval` is a third `xilem_core` backend (beside `xilem`→Masonry and
`xilem_web`→browser DOM) that diffs a Xilem view tree into serval's
`ScriptedDom`. It does state → view → diff → DOM mutation; serval does the
cascade, layout, paint, hit-test, a11y. The serval-as-host bet (architecture 3
in the engine plan) is that one engine renders chrome and content alike, so the
shell gets one layout model, one hit-test, one focus ring, and one a11y tree.

Today meerkat collects that benefit for the chrome only. The load-bearing
surfaces bypass `xilem_serval` and are hand-composited beside it. The shell has
drifted toward the architecture the engine plan explicitly excludes
(architecture 2: a host acting as a multi-surface compositor over several layout
and input authorities). It pays the engine-as-host cost and the compositor cost,
and collects the one-engine benefit for the toolbar.

The better role: `xilem_serval` hosts the whole **document shell** — chrome, all
document-shaped panes, and the node content inside the orrery — as one document.
The orrery becomes an element in that document whose geometry is delegated to
`gyre`. That collapses meerkat's bespoke compositor and band-router into "drive
one runner, present one scene" and unifies focus and a11y, which is the entire
point of the bet.

## The decision this turns on

Phase 1 is correct under either answer and should start regardless. Phase 2
turns on one product question that is the architect's to make:

- **Path A (recommended target).** Orrery node content gets document semantics
  (a11y, CSS, text selection, in-tree focus) by materializing node cards as DOM
  subtrees inside a canvas element; `gyre` owns geometry. Matches
  two_natured_kernel §4 and the archived eval §6. More engine work, full payoff.
- **Path B (fallback).** The orrery stays a scene-surface composited beside the
  document because a free-form physics scene is genuinely not a document. Then
  formalize the host as a principled surface compositor with one shared input
  router and focus arbiter, instead of ad-hoc Y-band branches. Less engine work,
  concedes one-engine-for-everything for the canvas.

The two_natured brief points at a hybrid that maps onto Path A: node *content* is
content-authoritative (DOM), node *positions* are experience-derived (`gyre`).
Lean Path A, sequenced after Phase 1, gated on serval custom-layout-element
support.

---

## Phase 1 — One document, one shell root

Consolidate the chrome and every document-shaped pane into a single
`ScriptedDom` under one `ServalAppRunner` whose single root is a **shell container**
holding the panes as subtrees, replacing the current one-runner-per-pane
fragmentation. (serval roots layout at one document element by design, so the shell
container is the proper shape, not sibling document-roots; see Open questions.)

Done conditions:

- One runner holds the chrome plus roster, apparatus, and the utility panes
  (inspector / steward / trail) as subtrees of one shell root in one `ScriptedDom`.
- Focus is one ring across chrome and all document panes; `Tab` / `Shift+Tab`
  traverses chrome into panes in a defined order.
- One AccessKit tree covers chrome plus all document panes (today each pane
  projects its own).
- The per-pane hit-test and the Y-band input branches for roster / apparatus /
  utility panes in `crates/meerkat/src/input.rs` collapse into one document
  hit-test.
- Behaviour parity: existing pane intents (`RosterIntent`, `ListPane`
  activations) still fire; theme switch still restyles.

Notes:

- `ViewPane` already centralizes the runner + `PaneSession` + sheet bundle
  (`crates/meerkat/src/view_pane.rs`), and each instance builds its own
  `ScriptedDom` (`ViewPane::new`, view_pane.rs:50). The work is to compose the
  per-pane states into one `ShellState` and one root view (the shell container),
  not one runner per pane.
- The real consolidation work is `PaneSession`'s per-pane lifecycle (activation,
  sheet bundle): each pane's session folds into one `ShellState`. The shared backing
  (graph / session) already lives in `self.shared` and composes as-is, so that part
  is not the work; the per-pane session bundles are.
- No engine prerequisite (resolved 2026-06-17): the runner attaches a single root
  under the document root, which is exactly right: that root is the shell
  container, and its children are the chrome + pane subtrees. serval lays out one
  document element by design (box_tree.rs:282-318), so this is the standard shape,
  not a workaround. See Open questions for why sibling document-roots are neither
  needed nor a serval shape.
- The shellbar and overlays are already chrome-root, so they come along for free.
- Migrate panes into the shell root one at a time with parity checks per pane, not
  big-bang: this touches five live panes' focus and a11y, and the "behaviour
  parity" condition above carries the real risk.

## Phase 2 — Orrery as element (Path A)

Make the orrery a serval custom-layout element inside the document: a
scene-paint underlay, physics-positioned DOM node-cards, and a camera transform,
with geometry delegated to `gyre`.

Done conditions:

- A serval element (an `<orrery>` custom-layout element, or a generalization of
  the replaced-element path) whose box the engine lays out and whose interior the
  host paints as a scene underlay.
- Node cards materialize as DOM subtrees inside the element (content-authoritative),
  positioned by `gyre` output via per-node transforms, with transform-only motion
  staying on the `RepaintOnly` path (the verified prerequisite, see Findings).
- Pointer and keyboard input reach the element through serval's hit-test; the
  element runs the two-hit-test split: node-content hits resolve in the DOM,
  scene-geometry hits (empty canvas, node bodies, edges) resolve via `gyre`'s
  `QueryPipeline`.
- A focused node card is a focusable DOM node, so orrery focus joins the Phase 1
  ring.
- The standalone orrery `netrender::Scene` and its bespoke pointer routing in
  `input.rs` reduce to the scene-underlay paint plus the `gyre` query.

Gated on: the Path A/B decision, and a serval **custom-layout element with
transform-positioned DOM children**. The input machinery is not the gap: serval
already dispatches pointer capture / bubble, and the host already owns the
`point → NodeId` hit-test half (xilem-serval/runner.rs:222-223), so DOM node-cards
take input for free and scene-geometry hits delegate to `gyre` at the existing host
seam. The new engine work is an element whose DOM children are placed by host /
`gyre` transforms rather than CSS flow (transform-only motion already verified
`RepaintOnly`), plus transform-aware hit-test (a G1 runway item). Today serval has
only replaced elements (`<external-texture>`, output-only), so this is a new element
kind, scoped in serval's plan.

Tiles follow-on (either path): workbench tab/divider chrome and content cards.
The composition spine's working-principles say `platen-view` realizes formes as
serval flex DOM, the natural vehicle for tile chrome, with `<external-texture>` for
genuinely external content (scrying / WebView). Confirmed 2026-06-17 this is a
migration, not a re-wire: meerkat composites a pelt `TileShell` today, and
`platen-view` does not exist yet (only `platen/lib.rs` + README). So the step is
net-new `platen-view` plus retiring pelt for tile chrome.

## Path B alternative — formalize the surface compositor

Only if Path B is chosen for the canvas. Phase 1 still ships; then:

- One `Surface` abstraction over the three kinds in play: document-surface
  (serval), scene-surface (`netrender` direct, the orrery), external-texture-surface.
- One input router and one focus arbiter across surfaces, replacing the
  hardcoded Y thresholds in `input.rs`.
- The orrery stays a scene-surface but gets a single integration contract (one
  hit-test delegate, one focus token in the shared arbiter) instead of routing
  smeared across `input.rs`.

---

## Findings (code-verified 2026-06-17)

The shape today, confirmed by an 8-agent workflow over serval + meerkat plus
targeted reads:

- **`xilem_serval` is the chrome's reactive layer.** `chrome_view(c: &Chrome)`
  returns `Box<dyn AnyView<Chrome, (), ServalCtx, ServalElement>>`
  (`crates/meerkat/src/views.rs`); one `ServalAppRunner` per window diffs it into
  the chrome `ScriptedDom`.
- **Each document pane is its own document and runner.** `ViewPane::new` builds a
  fresh `ScriptedDom` and a `ServalAppRunner` per pane (view_pane.rs:50); roster,
  apparatus, and the utility panes are each a `ViewPane`. So the document
  surfaces are several independent serval documents, each with its own focus and
  a11y projection.
- **The canvas surfaces bypass `xilem_serval` entirely.** Orrery, workbench
  tiles, gloss, and content cards render straight to `netrender::Scene`s. Meerkat
  produces roughly 7 to 10 scenes per frame and stitches them by Y-coordinate
  band (`crates/meerkat/src/render.rs`), with about five independent hit-test
  entry points and disjoint focus models (`crates/meerkat/src/input.rs`,
  documented at input.rs:60-64). There is no unified focus ring or Tab order, and
  the a11y tree is fragmented (orrery nodes appear only as their visual cards).
- **The only in-tree non-DOM bridge is output-only.** `<external-texture>` is a
  replaced element (`serval-layout/construct.rs:114-128`,
  `external_texture_key_of`) that emits `PaintCmd::DrawExternalTexture` for the
  host to composite; the view is `xilem_serval::external_texture(key, w, h)`
  (`xilem-serval/src/tags.rs:74`). It carries no input, so the orrery cannot be
  expressed as one today.
- **The perf prerequisite for an in-document orrery is met.** The host cheap-path
  work confirmed transform-only motion classifies as `RepaintOnly`, not relayout,
  which is the condition for 60fps physics on DOM-backed node cards (archived in
  [`archive_docs/2026-06-15_completed_plans/`](../../archive_docs/2026-06-15_completed_plans/),
  host_cheap_path).
- **One document via a shell root, not sibling roots.** serval roots layout at a
  single document element (`build_box_tree`'s first-element-child rule, "no synthetic
  wrapper", box_tree.rs:282-318); independent roots are expressed with `SubtreeView`
  (re-root at a sub-element, subtree.rs). So the separate-roots discipline ("separate
  roots *or* distinct documents" = capability separation) is satisfied by one
  `ScriptedDom` whose single root is a shell container with chrome + panes as subtrees.
  The current one-document-per-pane choice is an implementation default, not a
  constraint. (See Open questions, multi-root.)

## Open questions and risks

Reviewed against serval + meerkat 2026-06-17 (second pass). The first three of the
original four resolve or narrow; resolutions are reflected in the phase notes above.

- **Multi-root: resolved; Phase 1 needs no engine change.** serval roots layout at
  a *single* document element by design: `build_box_tree` takes the document's first
  element child as the root, "no synthetic wrapper" (serval-layout box_tree.rs:282-318).
  Its mechanism for an independent root is `SubtreeView`, which re-roots layout at any
  sub-element (serval-layout/subtree.rs; `render_subtree`, already used by incremental.rs).
  So "sibling roots under the document" is not a serval shape, and editing `build_box_tree`
  to wrap several top-level children would just be the container hidden in the engine,
  against its explicit no-wrapper design. The host-side **shell container** (one document
  element, panes as subtrees) is the proper model and gives the same unification;
  `SubtreeView` stays available as the per-pane isolated-relayout knob. The only axis
  multi-root would win, style *isolation*, is the opposite of Phase 1's goal. Phase 1 is
  a host-side `ShellState` / view consolidation.
- **Custom-layout element with input: narrower than stated, mostly host-side.** serval's
  pointer dispatch already does capture + bubble + ancestor walk
  (xilem-serval/runner.rs `dispatch_pointer_down/move/up`), and the host already owns the
  `point → NodeId` half (`hit_test_node`, runner.rs:222-223). So DOM node-cards receive
  input for free (they are real DOM the host resolves to), and scene-geometry input is the
  existing host seam delegating to `gyre`. The genuine engine ask is not "routed input" but
  a **custom-layout element whose DOM children are positioned by host/`gyre` transforms**
  rather than CSS flow (transform-only motion already verified `RepaintOnly`), plus
  **transform-aware hit-test** in the host (already a G1 composition-runway item). Scope
  that, not input routing, in serval's plan.
- **`gyre` two-hit-test: the primitive exists; only the boundary is unwritten.** `gyre`
  already exposes scene-geometry picking: `Simulation::hit_test(point) -> Option<NodeKey>`
  over rapier's `QueryPipeline` (every node is a collider) (orrery/gyre/lib.rs:351-358,
  query.rs). What remains is the written DOM-vs-`gyre` boundary: card-subtree hits resolve
  in the DOM, node-body / edge / empty-canvas hits resolve via `gyre`.
  cartography_aether_layout_seam is the starting point.
- **Tiles live path: confirmed a migration.** The workbench composites a pelt `TileShell`
  today (render.rs), and `platen-view` (formes as serval flex DOM) does not exist yet (only
  `platen/lib.rs` + README). So Phase 2's tiles step is net-new `platen-view` tile chrome
  plus retiring the pelt path for chrome, with `<external-texture>` for tile content, not a
  re-wire of an existing serval path.
- **Newly surfaced, genuinely open.** The focus / `Tab` traversal order across chrome and
  panes is unspecified. And one composed view function rebuilds wider than today's isolated
  per-pane runners; whether `xilem` memoization keeps that acceptable, or a hot pane wants
  its own `SubtreeView` pass, is the perf question to watch in Phase 1.

## Progress

- **2026-06-17** — Plan created from a code-verified investigation of
  `xilem_serval` usage in meerkat (8-agent workflow over serval + meerkat, plus
  targeted reads of `runner.rs`, `serval-scripted-dom/lib.rs`, `view_pane.rs`,
  `construct.rs`, `tags.rs`). Confirmed the chrome-only / pane-fragmented /
  canvas-bypass shape; confirmed `<external-texture>` is output-only; confirmed
  the `RepaintOnly` perf prerequisite; confirmed separate-roots allows one
  document. No code written. Phase 1 (document consolidation) recommended to
  start regardless of the canvas decision; Phase 2 (orrery-as-element, Path A) is
  the recommended target, gated on the A/B decision and serval custom-layout-element
  support.
- **2026-06-17 (second pass)** — Open questions reviewed against serval + meerkat.
  Multi-root resolved: serval roots layout at one document element by design
  (box_tree.rs:282-318), `SubtreeView` is its re-root mechanism, so Phase 1 is a
  host-side shell-container consolidation with no engine change (retagged above;
  "sibling roots" language corrected to "shell root / pane subtrees"). Custom-layout
  element narrowed: pointer dispatch + the host `point → NodeId` seam already exist
  (xilem-serval/runner.rs:222-223), so the engine ask is transform-positioned DOM
  children + transform-aware hit-test, not routed input. `gyre` two-hit-test primitive
  confirmed live (`Simulation::hit_test` over rapier `QueryPipeline`,
  orrery/gyre/lib.rs:351-358); only the DOM-vs-`gyre` boundary is unwritten. Tiles
  confirmed a migration (`platen-view` absent; pelt `TileShell` live). Newly surfaced
  open items: focus / `Tab` order across chrome and panes, and composed-view rebuild
  cost vs `xilem` memoization. No code written.
- **2026-06-17 (Phase 1 spike)** — Container-root mechanism proven in a passing test
  (`crates/meerkat/src/tests.rs`, `shell_container_hosts_chrome_and_pane_under_one_runner`):
  one `ServalAppRunner` hosts the real `chrome_view` plus a second pane as two
  `lens`-composed subtrees of a single "shell" container root in one `ScriptedDom`; both
  surfaces coexist in the one document, and a dispatched click routes through the single
  runner to the pane's own lensed sub-state. Confirms the host-side shell container with
  heterogeneous cross-pane lensing and no engine change. The remaining Phase 1 work is the
  state / render / input rewiring of the live panes, not a mechanism unknown.
