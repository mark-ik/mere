# Unified Document Host Plan

**CLOSED 2026-06-23 — core complete, Phase-2 tail spun out** (see the closing Progress entry for the
thread map). Kept in place (not archived) as the still-current foundational record of the Phase-1
consolidation + orrery-as-element architecture that active sibling plans cite. Remaining threads
re-homed: cond 1 -> [orrery_custom_layout_element_plan](2026-06-23_orrery_custom_layout_element_plan.md);
slice 5 -> [layout_phase_split_probe_plan](2026-06-23_layout_phase_split_probe_plan.md);
secondary-orreries -> tearout; tiles -> composition spine.

**Rename banner (2026-07-02):** the code pointers throughout this doc (`OrreryCard`,
`node_card_view`, `.node-card`, `render_as_cards`/`set_render_as_cards`, `point_over_orrery_card`)
predate the node/card terminology cleanup and no longer exist under those names — see
[node_card_summoning_design](../design/2026-07-01_node_card_summoning_design.md) for the
consensus (a node's rendered body is a **gnode**, never a card) and current names
(`OrreryGnode`/`gnode_view`/`.gnode`/`render_gnodes_as_dom`). Left unedited below as the
historical record.

Status (reconciled 2026-06-19 cross-plan consolidation). Phase 1's document consolidation is
**done** (all 4 done-conditions). Phase 2 cond 3/4/5 **landed host-side**, **but** the
card-as-node-hit-target mechanism (cond 3/4) was subsequently **reversed** by the
[node-representation-arrangement plan](2026-06-18_node_representation_arrangement_plan.md)
(2026-06-19): the node is now grabbed **through `gyre` directly** (`CLICK_SLOP` splits select vs
drag), and the card is a **static snapshot content-preview** that is **never** the drag handle and
**not** a representation form. cond 5 (the orrery **scene** as a document `<external-texture>`
element, standalone `Scene` retired) is **done**. The custom-layout `<orrery>` element (cond 1) is
**deferred**, not a gate: the interim transform-aware ring holds and the cards work as
host-positioned DOM.

The remaining **surface migration** does **not** mean "every host-composited surface becomes a
document external-texture element". Surfaces split **by nature** (the four-way layering owned by the
[native-surface-compositing plan](2026-06-19_native_surface_compositing_plan.md)): serval-rendered
content → a real DOM subtree; genuinely-external content (scrying = a system WebView2 visual) → a
**native composition visual below the chrome**, *not* a document element; dormant surfaces → a
snapshot texture; the orrery gyre **scene** → a texture is correct. The earlier blanket recipe is
superseded for the scry case (see "Surface migration" and C2 below). A cross-plan consolidation pass
(with the node-representation and native-surface-compositing plans) reconciled this plan 2026-06-19.

Sibling/converging docs:

- [native_surface_compositing_plan](2026-06-19_native_surface_compositing_plan.md) — owns the layer
  below the shell document: scry = a native composition visual on an off-window host HWND, under the
  chrome; dormant = a snapshot texture; these are **NOT** document external-texture elements. Its
  by-nature layering (C2) **supersedes this plan's blanket external-texture-migration recipe for
  genuinely-external content**.
- [node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md) — owns
  the **sprite/representation layer** (the card is **one** sprite); this plan owns the
  **node-as-container-in-the-document** (the a11y / automation spine), **not**
  node-content-as-document. Its 2026-06-19 entry **reversed** this plan's cond-3/4
  card-as-hit-target mechanism (press routes to `gyre`; the card is a static snapshot).
- [cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
  — the **owner of the performance strategy** (C3 bake-vs-live, C4 per-lane ceilings).
  Cross-reference; do not restate.
- [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md), the
  parent spine: the one fetch→render→represent→arrange→interact→semantic pipeline over the
  definitely-support formats. This plan is its **document-shell host layer**. Per the spine's
  ownership map it does not own node forms or the LOD machine (node-representation), scene-wide or
  localized arrangement (node-representation / field-regions), or the input spine and
  external-texture-input bridge (window-composition); it owns Phase 1 (one shell-root document)
  and Phase 2 (the custom-layout `<orrery>` element and the DOM-vs-gyre two-hit-test).
- [tearout_composability_plan](2026-06-19_tearout_composability_plan.md) (continuing the archived window-composition plan) — orrery
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

## Goals: three, not one

This plan is framed around three goals, not just "one engine renders chrome + content":

1. **Unify the host** — chrome + GUI + node-as-container content in one document.
2. **Leverage xilem to reach gpui-level baseline performance** — framed as **two per-lane
   ceilings**, not a single ceiling: the **first-party / PWA (app) lane** can parallelize the cold
   cascade via an SAB-threaded parallel cascade, but the **open-web-browsing lane cannot**
   (COOP/COEP `require-corp` is incompatible with arbitrary third-party content; Safari lacks
   `credentialless`). This plan **cross-references**
   [cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
   for the strategy and **does not restate it** — honoring its confidence levels
   (parallel-cascade-on-wasm is unproven / borrowed; the wasm build is worktree-only, not mainline).
3. **Make the browser diagnosable, accessible, and automatable** — one tree to instrument, one a11y
   tree, one DOM to drive. This is the **semantic-surface / machine-legibility payoff**, and it is a
   **core goal**, not a Path-A side effect (see Payoff section below, where the old "(Path A only)"
   gating is struck).

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

- **Path A (recommended target).** The **node as a container** lives in the document and so rides
  the host spine (a11y, CSS, text selection, in-tree focus, tab order, automation); `gyre` owns
  geometry. The unification reaches the node **as a container** but **does not fix its look**: the
  "card" is **one possible sprite** (others: a recursively-split pane, an 8-bit character with
  facets as dialog choices, a block in a growing crystal), not the node itself, and the
  sprite/representation layer is owned by the
  [node-representation-arrangement plan](2026-06-18_node_representation_arrangement_plan.md) (C1).
  Matches two_natured_kernel §4 and the archived eval §6. More engine work, full payoff.
- **Path B (fallback).** The orrery stays a scene-surface composited beside the
  document because a free-form physics scene is genuinely not a document. Then
  formalize the host as a principled surface compositor with one shared input
  router and focus arbiter, instead of ad-hoc Y-band branches. Less engine work,
  concedes one-engine-for-everything for the canvas.

The two_natured brief points at a hybrid that maps onto Path A: the node **container** rides the
document spine (a11y / tab / focus / automation), node **positions** are experience-derived
(`gyre`), and the node's **look** is a free sprite the node-representation plan owns (the card is
one sprite, never the node's truth). Lean Path A, sequenced after Phase 1, gated on serval
custom-layout-element support.

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
- The node-as-container materializes as a DOM subtree inside the element (so it rides the a11y /
  tab / focus / automation spine), positioned by `gyre` output via per-node transforms, with
  transform-only motion staying on the `RepaintOnly` path (a transform-only-motion fact, see
  Findings; **not** a per-frame-perf justification — see the bake-vs-live note below). The card the
  subtree carries is **one sprite**, not the node; the sprite/representation layer is owned by the
  [node-representation-arrangement plan](2026-06-18_node_representation_arrangement_plan.md) (C1).
- Pointer and keyboard input reach the element through serval's hit-test. **REVERSED 2026-06-19**
  (per the [node-representation-arrangement plan](2026-06-18_node_representation_arrangement_plan.md)):
  the earlier two-hit-test split (DOM-card hit vs `gyre` hit) was **abandoned**. The card is **no
  longer the node's hit-target**; a press over the orrery routes to **`gyre` directly**
  (`CLICK_SLOP` splits select vs drag), and the card is a static snapshot content-preview that is
  never the drag handle. Keyboard / focus still ride the document spine via the focusable node
  container.
- The **node container** is a focusable DOM node, so orrery focus joins the Phase 1 ring. (The
  card sprite it carries is a static snapshot, not the focus target — the container is.)
- The standalone orrery `netrender::Scene` and its bespoke pointer routing in
  `input.rs` reduce to the scene-underlay paint plus the `gyre` query.

**Bake-vs-live note (C3, owner =
[cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
§4(b); cross-ref only, do not restate).** The `RepaintOnly` classification above is a
**transform-only-motion fact** (transform-only motion does not relayout), and that claim is true.
It is **not** a per-frame-perf justification, and "baking to a texture" is **not** a per-frame-perf
win: baking is the **GPU rasterize** step and does **not** avoid the **CPU layout** (you must lay
out to rasterize). serval already has `IncrementalLayout` (repaint-only vs relayout, restyle
damage); the per-frame cost is solved by **incremental layout + layerized compositing**, not by
baking everything. A snapshot texture is justified by **DORMANCY / MEMORY** (you cannot hold N live
layout sessions in RAM for N previews = the suspended-tab model), not by per-frame performance; the
**active / focused surface stays LIVE the engine way** (cold layout once, incremental thereafter).

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

### Phase 2 design pass (2026-06-19): no engine gate, Phase 2 is host-side

Reading the engine (`serval-layout/box_tree.rs`, `incremental.rs`, `serval_lane.rs`, the
`<external-texture>` wiring) against the current `orrery_element` removes Phase 2's engine gate and
questions cond 1.

- **`<external-texture>` is not the template.** It is a *replaced leaf* (`box_tree.rs:145`):
  the box lays out like `<img>` and paint emits a `DrawExternalTexture` blit, output-only, no
  children. The orrery cards are a *container's* children, so none of that machinery applies. A
  true custom-layout element (cond 1) is net-new, not an extension of the replaced path.
- **The cards are already DOM and paint correctly (cond 2 done).** `orrery_element` is a `<div>`
  whose children are `position:absolute; transform:translate(gyre.x, gyre.y)`. serval lays them
  out and the `translate` shifts only the **paint** (the verified `RepaintOnly` transform path),
  not the box geometry.
- **The engine hit-test is already transform-aware (correcting an earlier draft of this entry).**
  `IncrementalLayout::hit_test` (`incremental.rs:182`) resolves a point through
  `ServalLaneView::hit_test`, whose `walk_for_hit` (`serval_lane.rs:398-417`) reads each node's
  `transform` from its cascaded values, conjugates it at the box origin, and inverse-maps the
  incoming point into the node's pre-transform space before the box test, the exact inverse of
  `paint_emit::walk`. It also honours `pointer-events: none`. So a point over a painted card *does*
  resolve to the card; the transform-aware hit-test the gate named already exists.
- **So the gap is entirely host-side: the orrery never runs the shell hit-test.** The cards are
  pointer-transparent not because the engine cannot reach them but because an orrery-area press
  routes straight to gyre (winit -> gyre), skipping the document hit-test. Route it through the
  shell hit-test first (the `chrome_click` path the folded panes already use, which rides the
  transform-aware `walk_for_hit`): a DOM-card hit dispatches in the document (the host owns the
  `point -> NodeId` half, `runner.rs:222-223`), a miss (empty canvas, node body, edge) falls to
  gyre's `QueryPipeline`. cond 4 follows by wrapping the card view in `focusable` (the Phase 1 ring
  mechanism); cond 5 reduces the standalone orrery `Scene` + bespoke pointer routing to the
  scene-underlay paint plus the gyre query.
- **cond 1 (a real custom-layout `<orrery>` element) is cleanliness, not a payoff gate.** It moves
  per-frame card placement from the host into an engine element that calls gyre; the semantic +
  interactive payoff lands without it on the host-positioned div. Recommend deferring cond 1,
  revisiting only if host-driven transform-setting becomes a perf or correctness problem.

**Engine asks (serval): none.** The transform-aware hit-test is already in the engine
(`walk_for_hit`), so Phase 2 lost its engine gate. cond 1 (a custom-layout element kind) stays
deferred and is not needed for the payoff. (This corrects the committed first draft of this entry,
which named a transform-aware hit-test as the engine ask; reading `serval_lane.rs` to write that
change showed it already done. DOC_POLICY 9: the implementation-as-probe loop caught it before any
engine code was written.)

**Meerkat consumer asks (this plan), after the engine hit-test lands:**

- An orrery-area press routes through the shell hit-test first; a DOM-card hit dispatches in the
  document, a miss falls to the existing winit -> gyre path. The 3a Y-band collapse already
  centralized this, so it is one branch.
- Wrap the node-card view in `focusable` (cards join the ring + Tab order, cond 4) and make the
  card a11y nodes actionable (they are already in the one a11y tree as inert divs, like the roster
  rows were before 3b).
- Retire the standalone orrery `Scene` + bespoke pointer branch once the gyre query is the sole
  scene-geometry hit path (cond 5).

### Phase 2a landed (2026-06-19): cards select + focus through the shell hit-test (cond 3/4)

Wired host-side, no engine work (commit dd0500d). `OrreryCard` carries its node URL; the card view
is `focusable(on_click(...))` queuing a select; `point_over_orrery_card` (riding the shell
hit-test) gates a left orrery press into `chrome_click` on a card hit, else falls through to
gyre's pan / select / drag. Selects drain in `drain_chrome_intents`, so the keyboard path
(Enter/Space on a focused card, via `dispatch_key`) selects too, and `dispatch_click`'s
click-to-focus rings the clicked card (cond 4). Compiles, 94 tests pass. **Pending: visual
confirm** (a card press selects + rings; an empty-canvas press still pans).

This pass also needed a taffy patch mirror (commit 302a3ac): serval main adopted experimental
float-layout taffy through a vendored fork (`support/patches/taffy`) it patches in, which mere did
not inherit, so serval-layout would not compile against the published taffy. mere now mirrors that
patch the way it mirrors serval's stylo patch.

Remaining Phase 2: cond 5 (retire the standalone orrery `Scene`); the card a11y nodes still need
to become actionable (currently inert divs in the one a11y tree); the z-order follow-up (node
labels paint over the command palette).

### Overlay interim landed (2026-06-19): the focus ring tracks transform-positioned cards

The cards paint at their fragment slot plus a paint-only `transform: translate(gyre.x, gyre.y)`,
so a focus ring positioned from fragments drew at the orrery container origin, not on the card.
Fixed by giving the paint-side overlays the transform-awareness the hit-test already has: serval
gained `IncrementalLayout::accumulated_translate(node)` (the sum of transform translates root to
node, the paint-side complement to `walk_for_hit`; serval `a2d91ddc`), and `push_focus_ring` adds
it (mere `7181206`). This is the stopgap that holds the visible behavior correct until cond 1; the
same primitive can fix the card a11y bounds (which are still at the pre-transform slot).

### cond 1 design (2026-06-19): the custom-layout orrery element, Mechanism A

cond 1 is the structural form behind the interim: each **node body / sprite's** real position lives
in the layout fragments, supplied by gyre, so every consumer (overlays, a11y bounds, future text
selection, hit-test) reads correct geometry with no transform special-casing. (Re-scoped
2026-06-19: this rationale is about the **node body / sprite geometry**, not the retired
DOM-card-as-node-hit-target model — per the node-representation reversal the card is not the node's
hit-target; the geometry that must be correct is the node container's / sprite's, whatever sprite is
shown.)

**Mechanism B (absolute `left`/`top`) is rejected.** Setting each card's `left`/`top` from gyre
and letting taffy flow them would put the positions in the fragments, but a `left`/`top` change is
layout-tier: every physics frame would relayout, the "orrery freeze" the transform / RepaintOnly
path was built to avoid (regression guard, serval `incremental.rs:1232`). B reintroduces it.

**Mechanism A (custom-layout concern) is the form.** Three serval-layout pieces:

1. A custom-layout mode for the orrery element, recognized by a marker attribute the way
   external-texture is recognized by `external_texture_key_of` (not a new CSS `display`): its
   children are measured to their natural size by taffy, then placed at host-supplied per-node
   positions rather than flowed.
2. A per-child position concern: a `child-node -> (x, y)` map the host supplies into layout each
   frame, analogous to the external-texture key / scroll offsets but per child.
3. A position-only incremental path: when only the position concern changes (DOM, styles, child
   sizes unchanged), update the children's fragment locations without re-measuring, so a gyre
   frame stays cheap. This is the layout-side analog of the RepaintOnly transform path, and the
   piece that keeps A as cheap as the transform it replaces; without it A is just B.

**Host migration (meerkat).** `orrery_element` marks the container custom-layout and drops the
per-card `transform: translate`; `render.rs` feeds gyre's per-node positions into the concern each
frame instead of into transforms. `accumulated_translate` then returns 0 for the cards (their
fragments carry the real positions) so the interim ring fix becomes a harmless no-op.

**Scope.** A real multi-subsystem serval feature (box-tree layout mode + concern plumbing + a new
incremental damage class / path), not a tail change. It warrants its own focused effort; the
interim ring fix holds the visible behavior correct until then.

### cond 5 landed (2026-06-19): the orrery scene is a document element, the standalone Scene retired

cond 5 ("retire the standalone orrery `Scene`") is done via the external-texture-element path, in three
commits, using serval's existing `<external-texture>` machinery (no new element kind):

- **The scene is a document `<external-texture>` element** (mere `73af79e`): `orrery_element`'s first
  child is `external_texture(ORRERY_SCENE_KEY, ...)`, the underlay beneath the DOM cards. The host
  rasterizes the gyre scene to a texture as before; the element places it in the document.
- **The orrery wheel routes through the document** (mere `894b5dc`, verified): `orrery_element` bears
  `on_wheel`; a wheel over the orrery dispatches to it (the runner's `wheel_target` ancestor walk
  resolves a wheel over a card or the scene to the orrery element), its handler queues the delta, and
  the host drains it to `gyre.wheel`. The pane element bears input, the form `wheel.rs` and the
  window-composition plan name. This is also serval ask #3's first consumer.
- **The compose is document-driven** (mere `571c38e`, verified): the host enumerates the chrome
  document's `<external-texture>` elements (`external_texture_placements`, from the chrome session's
  layout) and composites each registered texture at its laid-out rect, resolved by key. The orrery
  scene's compose moved off the hardcoded `orrery_rect` onto its element's placement; the standalone
  host composite is retired.

**Surface migration — corrected per C2 (2026-06-19; owner =
[native_surface_compositing_plan](2026-06-19_native_surface_compositing_plan.md)).** The
external-texture-element mechanism above is correct for the orrery **scene** specifically — a
rendered *field*, not a document, so a texture is the right shape (C2 case (d)). It does **not**
generalize to "every host-composited surface becomes a document external-texture element". Surfaces
split **by nature** (the four-way layering owned by the native-surface-compositing plan):

- **(a) serval-rendered content** (a secondary orrery's chrome, the workbench tile chrome, content
  the engine lays out) → a **real DOM subtree** in the shell document (rides a11y, find-in-page,
  selection, true scroll), not a texture.
- **(b) genuinely-external content** (scrying = a system WebView2 visual) → a **native composition
  visual** composited **below the chrome**, **NOT** a document external-texture element. This is the
  case the old blanket recipe got wrong.
- **(c) dormant surfaces** → a **snapshot texture** (the suspended-tab model; justified by dormancy
  / memory, not per-frame perf — see the bake-vs-live note above).
- **(d) the orrery gyre scene** → a texture is correct (what cond 5 landed above).

A secondary orrery's *scene* rides path (d); its node chrome rides path (a); a scrying surface rides
path (b). The remaining host composites migrate onto whichever path matches their nature, not all
onto the document-element path. Cross-reference the native-surface-compositing plan for the layering.

Tiles follow-on (either path): workbench tab/divider chrome and content cards.
The composition spine's working-principles say `platen-view` realizes formes as
serval flex DOM, the natural vehicle for tile chrome, with `<external-texture>` for
genuinely external content (scrying / WebView). Confirmed 2026-06-17 this is a
migration, not a re-wire: meerkat composites a pelt `TileShell` today, and
`platen-view` does not exist yet (only `platen/lib.rs` + README). So the step is
net-new `platen-view` plus retiring pelt for tile chrome.

Payoff, semantic surface (CORE GOAL 3 — diagnosable / accessible / automatable; see "Goals"
above). This is **not** a Path-A side effect: it is a core goal of the plan. Once the
node-as-container rides the document (carrying whatever sprite — the card is one), the rendered
orrery becomes machine-legible to outside consumers (assistive tech, semantic-web
tools, agents), riding the same DOM that gives the Phase 1 a11y tree. The flow
stays one-way and kernel-sourced: emit the already-shipped
[`linked_data::to_jsonld`](../../../crates/graph/linked-data/src/lib.rs) output as
an inline `<script type="application/ld+json">` during projection (per card or per
document), rather than re-extracting the view as a source. The orrery DOM is a
lossy, viewport-dependent projection (only materialized nodes), so the kernel
stays the authority and the complete export; the view only broadcasts. Prefer the
script block over RDFa/microdata attributes (directly parseable, host paints stay
presentational); reserve element-level RDFa for when a tool must grab a specific
sub-element (a `schema:name` span). This complements the shipped JSON-LD I/O
(`linked-data` crate, `Command::ExportGraph` / `>export_graph`, plus `from_html`
foreign-page ingest; [linked-data ingest/export plan](2026-05-22_linked_data_ingest_export_plan.md)),
it does not replace it, and it is an affordance riding the node-as-container-in-the-document
done-condition (the container rides the spine; the card is one sprite) rather than a hard Phase 2
requirement.

## Pressing slices (2026-06-19 code-verified audit)

A code-verified audit (per-area plus adversarial cite-check) ranked the most pressing undone work.
The five, in implementation order (Mark, 2026-06-21: do 2, 3, 1, 4; slice 5 is documented only).
**Slices 1-4 landed 2026-06-23** (see the Progress entry of that date); slice 5 remains
documented-only.

1. **Retain an `IncrementalLayout` session in the content actor.** The web-content lane re-runs a
   full `run_cascade` plus layout from scratch on every scroll band, find keystroke, and subresource
   (`content.rs:228/240/252` reach `paint_list_band_from_layout_dom` / `find_text_rects_from_layout_dom`,
   each calling `run_cascade` fresh, serval-layout/lib.rs:220/281). The chrome lane already has the
   fix (`PaneSession`'s `IncrementalLayout`, pane_session.rs:47, with the
   `Applied::Unchanged | RepaintOnly | Restyled` cheap path); the content actor holds raw `Content`
   with no retained layout. Add a retained `IncrementalLayout` over the parsed `StaticDocument`,
   apply incrementally on Scroll/Find/Resource, rebuild only on Show/Resize/structural change. No
   serval engine change. Medium. The single biggest per-frame cost (goal 2, the perf ceiling).
2. **Fix the orrery Tab order.** `collect_focusables` (runner.rs:781) is an unfiltered DOM pre-order
   walk; the orrery element is first in document order (window_view.rs:443) and every card is
   `focusable` (window_view.rs:507), so Tab from the omnibar cycles every on-screen card before
   reaching a chrome control. cond-2 "one focus ring" is logged done but Phase 2 inverted it. Fix,
   aligned with the node-as-container reversal: the orrery is ONE focusable container with
   host-driven internal selection, not N focusable cards. Small-Medium. A live keyboard regression.
3. **Retire the orphaned cond-3/4 card-as-hit-target plumbing.** The reversal routed presses to
   `gyre` (input.rs:360) but left the old path shipping: `point_over_orrery_card` (input.rs:735) has
   zero callers, `drain_orrery_card_selects` still runs every frame (input.rs:715), two parallel
   select paths for one object. Delete `point_over_orrery_card`, `OrreryCard.url`,
   `orrery_card_selects`/take/drain, and the `on_click` wrapper. Small; lands in the same edit as
   slice 2.
4. **Source the orrery a11y from the document card divs, not the parallel graph projection.** "One
   a11y tree" (cond 3) holds only at the OS-handle level: the orrery a11y is
   `mere_orrery::project_graph` (frame_a11y_panes.rs:32), a graph side-channel, not the shell DOM
   that renders. Generalize the serval-side DOM a11y walk (the pattern chrome uses, `chrome_a11y_tree`,
   frame_a11y.rs:130) to the orrery card subtree (actionable role plus `accumulated_translate`
   bounds, the bounds the focus ring already gets at serval_render.rs:304 but a11y does not), then
   retire `project_graph` for the orrery. Medium-Large. Unblocks core goal 3 (accessible /
   automatable).
5. **Build the Step-0 cascade-vs-box-tree-vs-shaping phase-split probe in serval-layout.** Absent
   (no `Instant`/`bench` anywhere in the crate). The
   [parallelism research doc](../research/2026-06-19_cross_platform_parallelism_strategy.md) §0 names
   it the prerequisite for the whole parallel-cascade thesis: box-tree build is sequential, so it
   caps the achievable win, and every parallelism decision is guesswork until the split is measured.
   A native-release timing harness (cfg-gated out of wasm and the hot path) times each phase and
   reports the split. Small-Medium, pure measurement.

   **Wasmtime-async note (slice 5, per Mark).** The phase split also bounds the win for the
   non-browser **wasmtime lane** the parallelism research doc parks (serval-on-Wasmtime / Spin for
   server-side async/parallel layout, the SSR/edge lane), distinct from the browser's Web-Worker
   path. WASI 0.3 async (shipped 2026-06-11) is its substrate today, wasi-threads later; the
   wasmtime-async work is where async/parallel layout can run off the main thread server-side, and
   slice 5's measurement is the same prerequisite for that lane. Unlike the browser lane it is not
   COOP/COEP-gated (cross-ref the research doc, which keeps the browser SAB lane app-lane-only).

The biggest **next-phase** item is deliberately NOT among the five: the **N-secondary-orreries /
side-by-side panes** slice (`secondary_orreries: Vec<(netrender::Scene, ...)>`, render.rs:625, with
`set_render_as_cards(false)` keeping secondary panes as standalone scenes composited beside the
document, the architecture-2 multi-surface-compositor shape Phase 2 exists to kill). It is large,
co-owned with the [tearout plan](2026-06-19_tearout_composability_plan.md), and gated on slice 2's
"card or container is the node" decision. cond 1 (the custom-layout `<orrery>` element) stays
deferred (a genuine multi-subsystem serval feature, see its design above).

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
- **The motion prerequisite for an in-document orrery is met.** The host cheap-path
  work confirmed transform-only motion classifies as `RepaintOnly`, not relayout,
  which is the condition for 60fps physics on DOM-backed node sprites (archived in
  [`archive_docs/2026-06-15_completed_plans/`](../../archive_docs/2026-06-15_completed_plans/),
  host_cheap_path). This is a **transform-only-motion fact**, not a bake-as-perf-win claim: the
  per-frame cost is solved by incremental layout + layerized compositing; the active surface stays
  live the engine way; a snapshot texture is a dormancy / memory decision (C3 bake-vs-live, owner =
  [cross_platform_parallelism_strategy](../research/2026-06-19_cross_platform_parallelism_strategy.md)
  §4(b); cross-ref only).
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
- **`gyre` two-hit-test: RESOLVED the other way (2026-06-19).** The DOM-vs-`gyre` boundary was
  written, but it **collapsed toward `gyre`, not toward a card/`gyre` split**: per the
  [node-representation-arrangement plan](2026-06-18_node_representation_arrangement_plan.md)
  reversal, the **card is not the node's hit-target**; an orrery press routes to `gyre` directly
  (`CLICK_SLOP` splits select vs drag), and the card is a static snapshot content-preview. The
  `gyre` picking primitive (`Simulation::hit_test(point) -> Option<NodeKey>` over rapier's
  `QueryPipeline`, orrery/gyre/lib.rs:351-358) is the live path; the earlier "card-subtree hits
  resolve in the DOM" half is retired. cartography_aether_layout_seam remains the geometry
  reference.
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
- **2026-06-17 (semantic-surface payoff)** — Added a Path A payoff bullet to Phase 2 after
  verifying the JSON-LD claim against code: contrary to the prompting note, JSON-LD I/O is
  already shipped and kernel-sourced (`linked-data` crate: `to_jsonld` / `to_jsonld_compact`
  export, `from_jsonld` / `from_html` ingest, bundled schema.org / Dublin Core /
  ActivityStreams `@context` assets), and wired as `Command::ExportGraph` (`>export_graph`,
  meerkat/src/export.rs, "Lane 0 sidequest #1"). Only `Semantic` edges export as RDF;
  `predicate_iri()` (edge_data.rs:94) maps recognized `SemanticSubKind` → canonical IRI,
  with the `predicate: Option<String>` field carrying round-trip identity. So Path A does
  not unlock JSON-LD (already exists from the kernel); it unlocks the *view* as a one-way
  semantic broadcast surface, emitted from that same kernel export. No code written.
- **2026-06-18 (node_quads landed)** — Refactored the `linked-data` export onto a single
  canonical kernel-to-RDF projection, `pub fn node_quads(graph, key, node) -> Vec<oxrdf::Quad>`
  (lib.rs), reusing the existing `node_id` / `edge_predicates` helpers; `node_object`
  (expanded) and `compact_node_object` (compacted) now render from the quads instead of each
  walking the graph, and `insert_types` is retired. `oxrdf 0.3` was already a direct dep, so no
  new dependency. 21/21 lib tests green including the expanded + compact goldens and both
  ingest round-trips. One intentional refinement: the quad model validates IRIs, so a malformed
  predicate / subject is now skipped rather than emitted as invalid JSON-LD (no test exercised
  it). `node_quads` is `pub` so the Oxigraph `>sparql` cut consumes it directly. This is the
  substrate the Phase 2 semantic-surface payoff and the Oxigraph query direction both build on.
  The note's origin plan (`linked_data_ingest_export_plan`) is archived/completed, so this lands
  here. Follow-on (not done): a shared `mapping` module so export (`node_quads`) and ingest
  (already on `oxrdf` quads) meet at one set of decisions, retiring `ingest.rs`'s duplicate
  `RDF_TYPE`.
- **2026-06-18 (the shipped slice recorded; the plan was behind the code, rule-9 gap closed).**
  A narrow slice of both phases landed across this and the prior session, unrecorded until now.
  **Phase 1, mechanism not consolidation.** `WindowView.runner` is a
  `ServalAppRunner<ShellState, ShellLogic, ShellView>` with `ShellState { chrome, orrery }`
  (window_view.rs:318); chrome is lensed and the orrery is a sibling element under one shell
  root; ~118 chrome sites migrated behind accessors. But roster / apparatus / steward /
  inspector stay separate `RosterPane`/`ListPane` runners (window_view.rs:102/113/117/118), so
  done-conditions 1-4 (panes in the shell root, one focus ring, one a11y tree, Y-band collapse)
  are **unmet**: the document-unification core is untouched. **Phase 2, cards only.** The orrery
  snapshots on-screen nodes through the camera into `OrreryRender` and draws transform-positioned
  DOM card chips in the shell document (RepaintOnly; `set_render_as_cards` suppresses the gnode
  layer), done-condition 2. Not met: a real serval custom-layout `<orrery>` element (it is a
  host-positioned `<div>`, cond 1), the two-hit-test split (input stays winit→gyre, cards
  pointer-transparent, cond 3), focusable card DOM in the ring (cond 4), scene/edge teardown
  (cond 5). **Fix-up pass:** card label = page-title-or-URL-slug + ellipsis (`1c564ab`), off-pane
  card cull (`2c6ddb8`), snapshot/frame reorder killing the one-frame lag (`2f5141a`), favicon
  PNG data-URI (`745682a`, not yet painting, no `ImagePlane` in the chrome render). Drag confirmed
  intact (winit-driven, bypasses DOM). **Net: the visible orrery-as-element slice is done and
  polished; Phase 1's consolidation and Phase 2's element / hit-test / focus / scene-teardown
  remain.** A cross-plan consolidation pass (with the
  [node-representation + arrangement plan](2026-06-18_node_representation_arrangement_plan.md) and
  the [scriptable field regions plan](2026-06-13_scriptable_field_regions_plan.md); reduce the
  drift; formalize the supported-format interaction model) is refactoring this plan.

- **2026-06-18/19 (Phase 1 document consolidation, 3 of 4 done-conditions).** The load-bearing
  panes now live in the one shell document under the single `WindowView.runner`. The roster folded
  first (state into `ShellState`, a lensed positioned subtree, `chrome_click` routing, drained
  intents; `RosterPane` retired to `#[cfg(test)]`), then the four list panes the same way: a
  `[ListPaneState; 4]` plus four lensed `list_pane_view` subtrees, multi-classed inner roots
  (`"utility-pane steward"`, so `has_class` finds each for scroll + hit-test while the shared
  `.utility-pane` styling still applies), `snapshot_list_panes` before the chrome render, button
  activations through `drain_list_pane_activations`; `ListPane` / `ViewPane` retired to
  `#[cfg(test)]` (`9598a91`). **Cond 1 (panes in the shell root): met.** Two more fell out of the
  consolidation. **Cond 4 (Y-band collapse): met.** The per-pane content-band branches collapsed to
  one `chrome_routed_leaf_at` then `chrome_click` (`6cbc6d7`). **Cond 3 (one a11y tree): met.** The
  chrome walk skips all five folded-pane wrappers, each projecting once through its frame-tree, the
  list panes gaining a rich `list_pane_a11y_tree` (actionable buttons routed to their DOM nodes,
  labels, bounds) so they could leave the chrome walk (`4626863`, `5943157`). **Cond 2 (one focus
  ring): the remaining Phase 1 work, net-new.** No Tab navigation or focus-ring render exists today
  (Tab is only the omnibar ghost-accept). Adjacent fixes this pass: scrollbar and folded a11y
  bounds at absolute coords (taffy locations are parent-relative, so a single fragment rect is only
  the offset within its parent; `208ac13`), the command palette centred over the orrery rect's
  insets so it clears the side panes (`4cda3a3`), and the cross-repo stylo dep unblocked (mere's
  `[patch.crates-io]` synced to serval main's `8bde0e96`, the lock advanced to serval main
  `39cb5b86`; `fdead82`). Open follow-ups: orrery node labels stack over the palette (z-order;
  chrome should layer above the `orrery_element`), and scroll is laggy (re-rasterize-on-scroll).

- **2026-06-19 (Phase 1 complete, cond 2: one focus ring).** Focus ring + Tab order landed
  (`56f0e34`), the last done-condition. The engine already provides Tab traversal
  (`focus_traverse`) and Enter/Space activation over its focusable set, so the gaps were narrow:
  the folded-pane controls were `on_click`-only (not focusable) and nothing rendered a ring.
  Wrapped the roster rows + list-pane buttons in `focusable` (Tab order); routed keys to the
  runner's `dispatch_key` when a non-field focusable holds focus, so Tab continues past the
  chrome into the panes (it previously fell to the graph key handler and stalled); drew a
  scroll-aware focus-ring outline off the cursor's node (which the host builds from
  `runner.focus()`); and drained Enter/Space's synthesized-click intent so the focused control
  fires (`drain_chrome_intents`, shared with `chrome_activate`). Verified: Tab cycles omnibar to
  theme to engine buttons with the ring tracking each step, Enter on "Light" switches the theme.
  **Phase 1's document consolidation is done (all 4 done-conditions).** Phase 2 (the
  custom-layout `<orrery>` element, the two-hit-test split, scene / edge teardown) remains; the
  open follow-ups are the labels-over-palette z-order and the laggy scroll.

- **2026-06-19 (cross-plan consolidation, this plan reconciled against its siblings).** A
  three-plan reconciliation pass (this plan + the
  [node-representation-arrangement plan](2026-06-18_node_representation_arrangement_plan.md) + the
  [native-surface-compositing plan](2026-06-19_native_surface_compositing_plan.md), aligned to the
  [cross-platform parallelism research doc](../research/2026-06-19_cross_platform_parallelism_strategy.md))
  folded its decisions into this plan. **No prior progress entry was edited** (DOC_POLICY 8/9); this
  entry records what reconciles the older "Phase 2 ... remains" line above with the updated Status
  header — the older line was true when written; this entry states what changed since. Edits made:
  - **Reconciled phase state in the Status header:** Phase 1 done (4/4); Phase 2 cond 3/4/5 landed
    host-side, but cond 3/4 (the card-as-node-hit-target two-hit-test) was **reversed** by the
    node-representation plan (press routes to `gyre` directly via `CLICK_SLOP`; the card is a static
    snapshot content-preview, never the drag handle, not a representation form); cond 5 (the orrery
    **scene** as a document `<external-texture>`, standalone `Scene` retired) is done; cond 1
    (custom-layout `<orrery>` element) deferred. Struck "and verified" (the body records cond 3/4 as
    "Pending: visual confirm"). Removed the orphaned verbless thesis-abstract fragment.
  - **Absorbed the cond-3/4 reversal** into the Phase 2 done-conditions, the gyre-two-hit-test open
    question (which **collapsed toward `gyre`**, not toward a card/`gyre` split), and the cond-1
    rationale (re-scoped to **node body / sprite geometry**, not the retired DOM-card-as-node model).
    node-representation is the canonical record of the reversal.
  - **Added the three-goals frame** (unify the host; reach gpui-level baseline perf as **two
    per-lane ceilings**; diagnosable / accessible / automatable) and **promoted the
    semantic-surface payoff from a "(Path A only)" side effect to core goal 3**.
  - **Corrected the surface-migration recipe (C2):** surfaces split **by nature** — serval content →
    DOM subtree; genuinely-external scry → a native composition visual below the chrome (**not** a
    document element); dormant → snapshot texture; the orrery gyre **scene** → texture (correct,
    cond 5). The blanket "every surface → document external-texture element" recipe is superseded for
    the scry case; cross-referenced the native-surface-compositing plan (the C2 owner). The cond-5
    factual record (orrery scene on `<external-texture>`) is retained; only its generalization to
    scrying was wrong.
  - **Added the bake-vs-live correction (C3)** at every RepaintOnly / texture-as-perf site: baking is
    the GPU rasterize step and does not avoid CPU layout; serval already has `IncrementalLayout`; the
    snapshot texture is a **dormancy / memory** decision, not a per-frame-perf win; the active surface
    stays live the engine way. RepaintOnly kept correct as a transform-only-motion fact.
  - **Reframed node-card-as-document language (C1)** to the node-as-container / free-sprite framing
    (the card is **one** sprite, not the node's truth); the sprite/representation layer is owned by
    the node-representation plan, this plan owns the node-as-container (the a11y / automation spine).
  - **Added three sibling cross-references:** native-surface-compositing (C2 owner of the layer below
    the document), node-representation-arrangement (C1 owner of the sprite layer + the cond-3/4
    reversal), and the parallelism research doc (C3/C4 owner of the performance strategy;
    cross-reference only, do not restate; honor its confidence levels). No DOC_README change is
    triggered (edit-in-place; no doc added or moved; relative links only).

- **2026-06-21 (code-verified audit + the five pressing slices documented).** A 7-agent audit
  (per-area code verification + adversarial cite-check) of this plan against the tree produced the
  new **Pressing slices** section above (the five, with slice 5's wasmtime-async note). Mark
  directed: implement 2, 3, 1, 4; document slice 5 only. The audit also surfaced doc-staleness this
  entry records (corrected inline as each slice lands, or noted here):
  - **cond 3 "one a11y tree" is overstated:** true only at the OS-handle level
    (`build_a11y_projection` host-stitches a chrome DOM subtree + a per-pane frame_tree, and the
    orrery is `mere_orrery::project_graph`, frame_a11y_panes.rs:32, not the shell DOM). Slice 4 fixes
    the source.
  - **The Phase-2a "landed" section** describes `point_over_orrery_card` gating a press into
    `chrome_click` as live; it is dead code (zero callers, input.rs:735), superseded by the reversal
    (the live press routes to `gyre`, input.rs:360). Slice 3 deletes it.
  - **The scry open-question** reads as undecided; rev-2 is **landed** (off-window `new_offscreen`,
    scrying_host.rs:528; the per-frame `set_offset` chase gone; input via API/CDP), owned by the
    native-surface-compositing plan. Residual: scry still composites as a host texture-blit
    (render.rs:1398) rather than a below-chrome native composition visual.
  - **cond 1 / "one document" reads broader than the code:** four surfaces remain host-composited
    beside the shell document (workbench `pelt_shell`, gloss, the comms pane, scrying); comms was
    folded into the `chrome_routed_leaf_at` path but is not listed among the consolidated panes.
  - **"labels-over-palette z-order"** listed as open appears resolved in code (chrome paints last;
    palette re-centred over the orrery insets); verify with a headed run before striking.
  - **File-size ceiling:** `render.rs` (~1883), `input.rs` (~1732), `main.rs` (~1617) each ~3x the
    600-LOC mere ceiling, in the exact files these slices churn; split before adding.

- **2026-06-23 (slices 2, 3, 1, 4 landed; slice 4 adversarially verified sound).** The four directed
  slices implemented, meerkat green (179 tests) and serval-layout green (205) for the cross-repo cut:
  - **Slices 2 + 3 (orrery focus ring + orphaned plumbing).** The node cards left the Tab ring (the
    `focusable` wrapper + `on_click` select removed, window_view.rs); the dead two-path plumbing
    (`point_over_orrery_card`, `OrreryCard.url`, `orrery_card_selects`/take/drain) deleted. The orrery
    is one focusable container, gyre owns selection (the cond-2/4 reversal made real, retiring the
    Phase-2a dead code the 2026-06-21 entry flagged).
  - **Slice 1 (content-actor retained layout).** serval-layout gained `ContentLayout` +
    `lay_out_content` + `emit_band` + `find`; the `paint_list_band_from_layout_dom` /
    `find_text_rects_from_layout_dom` free functions became thin wrappers over it, behaviour-identical
    (205 tests). The content actor retains one `ContentLayout` per page and re-emits scroll bands /
    find off the cascade instead of re-running `run_cascade` each call (content.rs). The cross-repo
    cut Mark approved; `cargo clean -p serval-layout` cleared a stale artifact from the multi-crate
    path-override.
  - **Slice 4 (orrery a11y from the document).** `orrery_a11y_tree` (frame_a11y_panes.rs) replaces the
    `mere_orrery::project_graph` side-channel: each graph node is a `Role::Link` carrying its URL value
    (so the existing `attach_link_actions` pass, frame_a11y.rs:298, makes it click/focus-actionable and
    routes `SelectNodeByUrl`), with DOM-sourced bounds off the laid-out `.node-card` divs
    (`accumulate_origins` + `accumulated_translate`, keyed by a new `data-member` stamp) so the a11y
    rect tracks where the card paints, the offset the focus ring already applied but a11y did not.
    `"orrery"` joined `FOLDED_PANE_WRAPPERS` so the chrome walk stops double-emitting the cards as bare
    containers. Supporting: `OrreryCard.member` + `data-member`, `PaneSession::accumulated_translate`
    delegate. A 3-agent adversarial review (bounds math, wrapper skip, regression) returned all-sound:
    the unscrolled origin equals the painted origin for every card (no card ancestor is in the scroll
    map, so it matches `push_focus_ring`'s formula byte-for-byte), the token-skip hits exactly
    `.orrery` (`.orrery-scene` pruned as a descendant), the frame-leaf node + bounds survive the skip
    (separate `attach_frame_bounds` path), and project_graph's behaviour is preserved (every node
    listed incl. off-pane bound-less, label/value, empty-graph root-only). This is the cond-3 "one a11y
    tree" correction the 2026-06-21 entry noted: the orrery a11y is now the shell document's projection,
    not a graph side-channel. Unblocks goal 3 (accessible / automatable).
  - **Residual: `mere-orrery` is now consumer-less.** `project_graph` has no caller anywhere; meerkat's
    `mere-orrery` dependency (Cargo.toml) is dead (no lint flags it, build stays green). Left in place
    pending Mark's call on the crate's fate (delete / keep scaffolded / repurpose) under the
    ask-before-dropping-deps rule. Minor pre-existing dup the review surfaced (not slice 4's doing):
    `gloss_a11y_tree` also emits a `SelectNodeByUrl` Link per node, so with both panes open a node
    carries two routes under different ids, harmless (the bridge's `find_map` returns one).

- **2026-06-23 (CLOSED — core complete, Phase-2 tail spun out).** Phase 1 (one document, one shell
  root) is done 4/4 and the four pressing slices landed + verified. The plan's task-list is complete;
  what remained is re-homed, so this plan is **closed**. It stays in place (not moved to
  `archive_docs/`) because it is the **foundational, still-current record** of the Phase-1
  consolidation + the orrery-as-element architecture that six active plans cite (the cond-3/4
  reversal, the C2 surface-by-nature layering, Phase-1 done-conditions); "superseded checkpoint"
  framing does not fit live architecture. Thread map:
  - **cond 1 (custom-layout `<orrery>` element, Mechanism A)** -> spun out to
    [orrery_custom_layout_element_plan](2026-06-23_orrery_custom_layout_element_plan.md) (parked /
    deferred by design; the interim transform-aware ring + the slice-4 a11y bounds hold without it).
  - **Slice 5 (cascade-vs-box-tree-vs-shaping phase-split probe)** -> spun out to
    [layout_phase_split_probe_plan](2026-06-23_layout_phase_split_probe_plan.md) (the goal-2 perf
    measurement gate, owned jointly with the parallelism research §0).
  - **N-secondary-orreries / side-by-side panes** (`secondary_orreries`, render.rs:785/793, still
    standalone Scenes via `set_render_as_cards(false)`) -> co-owned by the
    [tearout composability plan](2026-06-19_tearout_composability_plan.md), gated on the
    card-or-container decision. Not this plan's to drive alone.
  - **Tiles -> `platen-view` migration** (workbench tile chrome) -> the composition-spine concern
    ([mere_composition_spine](../technical_architecture/2026-05-21_mere_composition_spine.md);
    net-new `platen-view`, retiring the pelt `TileShell` path).
  - **Semantic-surface JSON-LD broadcast** (goal 3's further half: inline
    `<script type="application/ld+json>` per card/document during projection) -> a small follow-on
    affordance on the shipped `linked-data` export; slice 4 landed the a11y/DOM half (the orrery a11y
    is now DOM-sourced + actionable). Unbuilt, not a hard requirement.
  - **Orrery as a sighted-keyboard focus-stop with internal arrow-key node selection** -> a UX
    follow-on in node-representation / interaction territory. Slice 2 removed the cards from the Tab
    ring and slice 4 gave assistive tech actionable node selection via the a11y tree; the visual
    keyboard ring stepping orrery nodes is not built.
