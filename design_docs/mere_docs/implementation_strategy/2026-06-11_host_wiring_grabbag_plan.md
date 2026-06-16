# Host Wiring Grab-Bag Plan

**Date**: 2026-06-11
**Status**: **Serval-side complete (2026-06-12); meerkat adoption of 4 items still
open.** All eight items' serval / xilem-serval seams landed (G1.1–G1.4, G2.1–G2.4).
Two have live meerkat callers (G2.1 IME, G2.4 chrome a11y actions); the other four
(G1.1 on_wheel, G1.2 transform hit-test, G1.3 pointer cancel, G2.3 keyboard escapes)
are runway with **no meerkat caller**, their own done-conditions (meerkat adoption)
awaiting window-composition P2+. So this is 6-wired / 4-runway at the meerkat layer,
not 8/0. Per-item statuses below. Spun out of the
[host cheap-path plan](../../archive_docs/2026-06-15_completed_plans/2026-06-10_host_cheap_path_plan.md)'s C6 (which is now
otherwise done: C0–C5 + C4c shipped). This was the checklist of record for the
remaining host-wiring parity items; what is left is meerkat-side adoption of the
serval-side runway (G1.1–G1.3, G2.3), which rides window-composition P2+.
**Scope**: The grab-bag of serval / xilem-serval host capability that is wired
and tested one layer down with zero or stub meerkat callers. Each item lands
separately; this plan phases them by what unblocks what, not by date.
**Related**: the [host cheap-path plan](../../archive_docs/2026-06-15_completed_plans/2026-06-10_host_cheap_path_plan.md) (the
parent; C6 lived there until it grew); the
[window composition plan](2026-06-11_window_composition_plan.md) (its P2+
pane-heavy phases build directly on Phase G1 below — G1 is the runway).
*Out of scope (tracked elsewhere):* scrying X2's leftover host wiring (omnibar
`load_url`, back/forward + `can_go_*`, `poll_navigation_event`,
`poll_cursor_shape`, Tab focus) — a different crate, tracked in the
[scrying tile plan](2026-06-10_scrying_tile_plan.md), not here.

---

## The shape

Eight items, two phases. **Phase G1** (four items) is *composition-enabling*:
window-composition P2+ (per-pane input/hit-test, interactive DOM under the
orrery camera, the growing pane tree, cross-graph drags) builds directly on it,
so it is the runway to clear before the pane-heavy composition phases.
**Phase G2** (four items) is *host-completeness*: correctness gaps independent of
composition, doable anytime. Each item names its crate; most are mechanical
(the capability already exists and is tested — the gap is the meerkat/dispatch
call site).

---

## Phase G1 — Composition runway (do before window-composition P2+)

**G1 COMPLETE (2026-06-11).** All four landed: G1.1 `on_wheel` infra, G1.2
transform-aware hit-test, G1.3 pointer cancellation (serval, committed
`718cf5d7d3c` / `bab5a2c7f1c` / `173282dde89`), G1.4 `memoize` (meerkat). The
serval-side seams (G1.1–G1.3) are forward-looking runway — their meerkat callers
arrive with window-composition P2+; G1.4 ships its meerkat caller now. Per-item
status under each heading.

### G1.1 — `on_wheel` event view (serval)

**Now**: meerkat hand-routes wheel input (`app_handler.rs:370-414`) and owns
`ScrollOffsets`; serval has the one open Stage-3 event-view gap here — no
`on_wheel` registry/dispatch parallel to `on_pointer`.

**Do**: add an `on_wheel` registry + dispatch mirroring `on_pointer`, so wheel
becomes view-owned and per-pane.

**Done when**: meerkat's hand-routed wheel and host-owned `ScrollOffsets` retire
in favour of per-pane view-routed wheel.

**Status (2026-06-11): serval half DONE; meerkat retirement gated on
composition.** The event-view gap is closed: `xilem-serval` now has `on_wheel`
parallel to `on_pointer` — `WheelEvent { delta, local, size }` + `OnWheel` view
(`wheel.rs`), a no-phase `wheel_handlers` registry (`context.rs`), and
`dispatch_wheel` / `wheel_target` / `route_wheel` (`runner.rs`, ancestor-walk to
the nearest handler, no capture). Covered by
`wheel_routes_to_nearest_handler_no_capture`; 47/47 xilem-serval tests green.
The meerkat-side retirement of the hand-routed wheel (`app_handler.rs`) +
host-owned `ScrollOffsets` is **deferred**: meerkat's scrollable surfaces (the
orrery, content cards, roster) are hand-composited, not `xilem-serval` view
nodes, so there is nothing in today's view tree to attach `on_wheel` to. That
retirement lands when window-composition P2+ expresses those panes as views —
the infra is now the runway it needs.

### G1.2 — Transform-aware hit-testing (serval)

**Now**: `walk_for_hit` (`serval_lane.rs`) composes box *locations* only, not CSS
transforms; the matrices it needs already exist in the same crate
(`compute_transform_matrix` + `conjugate_at`, used by paint). So a hit inside a
`transform`ed subtree (the orrery camera container) mis-resolves.

**Do**: thread the same transform composition paint uses into the hit walk.

**Done when**: a point inside a transformed subtree hit-tests correctly — gating
any interactive DOM content under the orrery camera (and, with G1.3, the
*interactive* external-texture element).

**Status (2026-06-11): DONE.** `walk_for_hit` (`serval_lane.rs`) now folds each
node's CSS transform into the walk: it maps the incoming point through the
node's transform conjugated at the box origin (`conjugate_at` +
`compute_transform_matrix`, made `pub(crate)`), the exact composition
`paint_emit::walk` paints with, so a hit resolves where paint drew it. Identity
is a no-op (untransformed DOMs byte-identical); the inverse telescopes through
nesting (each node maps the already-mapped point); a singular transform skips its
subtree. Guards: `hit_test_resolves_a_point_inside_a_translated_subtree` and
`..._scaled_subtree` (scale exercises the around-origin conjugation a translate
can't); clip/scroll/topmost tests unregressed; 141/141 serval-layout green,
meerkat builds clean. No current consumer (the orrery picks geometrically), so
this is runway — interactive DOM under the orrery camera in composition P2+ is
the first caller.

### G1.3 — Pointer cancellation (xilem-serval)

**Now**: `PointerEvent` has no propagation/cancel channel; `route_pointer`
leaves the stale click/key `default_prevented` value rather than recording the
pointer pass's own.

**Do**: give `PointerEvent` a `Propagation` cell and have `route_pointer` record
`default_prevented` per pointer event.

**Done when**: a drag routed through `on_pointer` can cancel/stop-propagate —
relevant the moment drags move onto the pointer path.

**Status (2026-06-11): DONE.** `PointerEvent` now carries a clone-through
`prop: Propagation` (+ a `PointerEvent::new` constructor), the twin of the
`PointerClick` / `KeyEvent` cell, so a drag handler can call
`e.prop.prevent_default()`. `route_pointer` records the pass's own
`default_prevented` (cloning the event into the message so the shared cell is
read back), and `dispatch_pointer_down`/`_move`/`_up` reset it first — so a
pointer pass no longer leaves the stale click/key value. Guard
`each_pointer_event_records_its_own_default_prevented` (the press prevents, the
move resets). Constructor sites updated (pelt-live + tests); 48/48 xilem-serval
green, pelt-live builds clean. Forward-looking like G1.2 — no meerkat drag rides
the pointer path yet.

### G1.4 — `memoize` the stable chrome subtrees (xilem-serval)

**Now**: `memoize` is re-exported and tested over `ServalCtx` but has zero
meerkat callers, so the whole view tree rebuilds per event.

**Do**: wrap the stable chrome subtrees in `memoize`.

**Done when**: an event that touches one pane does not rebuild the
`O(view tree)` of the unaffected stable subtrees — the rebuild cost of a growing
pane tree is bounded.

**Status (2026-06-11): DONE (meerkat caller landed).** `chrome_view`
(`meerkat/views.rs`) now wraps its stable subtrees in `memoize`: the **shellbar**
on `c.shellbar_panes` (a `Copy + PartialEq` key) — the substantial win, since an
omnibar keystroke leaves the panes unchanged and so skips rebuilding the whole
toggle strip — plus the **nav buttons** (each on its `can_go_*` bool) and the
**static workbench button** (on `()`, built once). `memoize` is transparent (no
wrapper element), so the chrome DOM is byte-identical: all 7 meerkat chrome tests
green, build clean. Further per-element memoization (omnibar / suggestions) is
not worthwhile — those change on the very keystroke that drives the rebuild. The
larger payoff arrives when window-composition P2+ multiplies the panes; this is
the seam it rides.

---

## Phase G2 — Host-completeness (correctness; anytime)

### G2.1 — IME wiring in meerkat

**Now**: the library + demo are complete (the C1 caret seam already places the
candidate window via `set_ime_cursor_area`), but meerkat has no `winit::Ime`
arm, no `set_ime_allowed`, no `set_ime_cursor_area` call.

**Do**: add the `winit::Ime` event arm + the two winit calls, sourcing the caret
rect from the session's C1 caret seam.

**Done when**: CJK/composition preedit renders in the omnibar, positioned at the
caret.

**Status (2026-06-11): DONE (on-device IME round-trip pending).** New
`meerkat/ime.rs` adds `WindowCtx::handle_ime`, wired from a `WindowEvent::Ime`
arm (`app_handler.rs`), with `set_ime_allowed(true)` on window creation. Preedit
→ `set_preedit` on the *focused* field (resolved by the same mapping as
`caret_field`, meerkat's twist over pelt-live's single field); commit → clear +
`dispatch_key(Character)` (the focus-routed insert path); disabled → clear. The
candidate window follows the caret via `set_ime_cursor_area`, sourcing the rect
from a new `PaneSession::caret_rect` passthrough (the same C1 rect the painted
caret uses). Guard `ime_preedit_and_commit_route_to_the_focused_field` (preedit
stays out of the committed buffer; commit inserts 你好) — the meerkat-specific
routing. Build clean. The OS-IME round-trip (a real IME firing `winit::Ime` +
candidate placement) can't be unit-tested headlessly and is the one on-device
check left.

### G2.2 — Environment threading (xilem-serval)

**Now**: dispatch builds `MessageCtx::new(Environment::new(), ...)` while builds
use the real environment — a split-brain that surfaces the moment an
environment-dependent view (theming, scaling) lands in chrome.

**Do**: pass `ctx.environment` at the three construction sites (mechanical).

**Done when**: dispatch and build share one environment; an environment-reading
view in chrome behaves the same on both paths.

**Status (2026-06-11): DONE.** Correction to the "(mechanical)" note above:
`Environment` is not `Clone`, and `MessageCtx::new` takes it by value while
`finish()` hands it back — so the fix is a **take → route → finish → restore**
thread, not a swap, and there are **four** dispatch sites (click, key, pointer,
and the G1.1 wheel), the click/key ones looping over multiple paths (env threaded
through the loop). New `ServalCtx::take_environment` / `set_environment` back it.
Behaviorally a no-op today — nothing reads the env in a message path, so the real
env and `Environment::new()` are both empty — so verification is the full suite
staying green (48/48; no dispatch regression) plus by-construction correctness;
the first environment-reading view inherits the shared env for free.

### G2.3 — Keyboard-model escape hatches (xilem-serval; per the configurability rule)

**Now**: three gaps. (1) `focusable == has-on_key`, so a plain button is
keyboard-unreachable; (2) Tab is swallowed pre-routing (no tab char in
textareas, no custom order); (3) a second `on_click` on a node silently clobbers
the first (single-listener-per-node).

**Do**: add an explicit `focusable()` marker + synthetic Enter/Space activation;
an overridable Tab default; Vec-per-node listener registries.

**Done when**: a plain button is keyboard-activatable, Tab is overridable
per-view, and a node can carry multiple listeners of one kind.

**Status (2026-06-12): DONE (serval-side; meerkat adoption is the follow-up).**
All three escape hatches landed in xilem-serval.
(1) **Vec-per-node registries** — `click_handlers` / `key_handlers` now map a node
to a `Vec<Handler>` (idempotent per routing path); `register_*` appends rather than
overwrites and `unregister_*` removes by path, so stacked listeners coexist and
`phase_ordered_paths` routes every one, in registration order within its phase.
Guard: `stacked_click_listeners_all_fire`.
(2) **`focusable()` marker** — a new transparent view (`focusable.rs`) registers a
node in an explicit, refcounted focusable set, so `is_focusable` becomes "has a key
handler **or** a marker"; a plain `focusable(button(..))` joins the Tab order.
(3) **Enter/Space activation + overridable Tab** — `dispatch_key` now delivers Tab
to the focused element's handlers *first* and traverses focus only when none
prevented it (so a `textarea` can insert a tab char or a view impose a custom
order), and synthesizes a click on Enter/Space for a focusable control that has a
click handler but no key handler of its own (the plain-button case; a text field's
own `on_key` owns the key, so its Space still inserts a space — the guard is "click
handler present, key handler absent"). Guards:
`enter_and_space_activate_a_focusable_button`, `tab_is_overridable_by_a_handler`.
51/51 xilem-serval tests green. Forward-looking runway like G1.1–G1.3: no meerkat
caller yet. Meerkat adoption (wrapping chrome buttons in `focusable()`, delivering
winit Tab/Enter/Space through `dispatch_key`) is the on-device follow-up.

### G2.4 — Chrome a11y actions

**Now**: C4c landed the chrome a11y *tree* (roles/names/bounds derive from the
rendered chrome `ScriptedDom` via `serval_a11y`), but the chrome is not
*actionable* — a screen reader cannot activate the omnibar or a toolbar button.
(No regression: chrome was unactionable before too.)

**Do**: wire accesskit actions on the chrome a11y nodes back to the host's
existing activation paths.

**Done when**: a screen reader can activate a chrome control (omnibar focus, a
toolbar button) through the a11y tree.

**Status (2026-06-12): DONE.** Two parts. **Part 1** (2026-06-11): chrome controls
*advertise* their action (`serval_a11y::build` calls `Node::add_action` —
`Button`→`Click`, `TextInput`→`Focus`); guard in
`chrome_dom_projects_to_a11y_subtree` (`supports_action`). **Part 2** (2026-06-12):
the host now *routes* the resulting `ActionRequest` back to the chrome's activation
paths. `chrome_a11y_tree` hands back the actionable nodes alongside the tree;
`build_a11y_projection` keys each into an `A11yHostAction::ChromeNode(NodeId)` route
by its `chrome_a11y_id`, and `apply_a11y_request`'s `Some` arm applies it — `Focus`
→ `runner.set_focus(Some(node))`, `Click` → a new `chrome_activate` helper (the
dispatch+drain tail factored out of the pointer `chrome_click`, dispatching at the
element-local origin the chrome's position-agnostic button handlers ignore).

The route stores the whole `NodeId`, **not** the salted id reversed: the first cut
tried `chrome_node_from_a11y_id` (`SALT | raw` then `& !SALT`), which is
**debug-broken** — on 64-bit debug builds `NodeId::raw()` packs a process-unique
doc-tag into the same high bits `CHROME_A11Y_SALT` (`0xC04E…`) uses, so the tag
corrupts the salted id and `& !SALT` can't recover it (it works only in release,
where the doc-tag fence compiles out; the false-passing test only passed as the
first document, tag 0). Storing the node whole sidesteps the id entirely — the
orrery's `SelectNodeByUrl` pattern.

Verified end-to-end, headless:
`accesskit_focus_on_a_chrome_control_routes_to_the_runner` builds a real chrome
session (`PaneSession::scene`, CPU layout — no GPU), runs the real projection, and
asserts the omnibar's `chrome_a11y_id` keys a `ChromeNode` route to its own node and
that a `Focus` request at that id lands the runner's focus on the omnibar — so the
projection id and the route key round-trip (the exact seam the reversal got wrong).
69/69 meerkat bin tests green. The on-device screen-reader round-trip (a real AT
firing the `ActionRequest`) is the one check left, as with G2.1's IME round-trip.

---

## Findings

- The four G1 items are the *composition-enabling* subset called out in the host
  cheap-path plan's C6 (2026-06-11 sequencing split): window composition P2+
  rides them, so they clear the runway before the pane-heavy phases. The G2
  items are correctness, independent of that ordering.
- Almost everything here is "wired one layer down, no caller up top": the
  capability exists and is tested in serval / xilem-serval; the work is the
  meerkat (or dispatch) call site, not new infrastructure. The exceptions that
  need real serval code are G1.1 (`on_wheel` dispatch) and G1.2 (transform
  threading into the hit walk).
- Constraint carried from the flip plan: keep every new host-coupling
  retargetable. These are serval-side seams; meerkat's adoption stays confined
  to its render / input / app-handler call sites.

## Progress

- **2026-06-11** — Plan spun out of the host cheap-path plan's C6 once that plan's
  perf chain (C0–C5 + C4c) finished. No code yet. Phase G1 is the entry point
  (composition runway); start with G1.1 / G1.2 (the two that need real serval
  code) since G1.3 / G1.4 are mechanical wraps that ride them.
- **2026-06-11** — Phase G1 complete (G1.1–G1.4); G2.1 (IME), G2.2 (env threading),
  and G2.4 part 1 (advertise actions) landed. G2.4 part 2 deferred with a design
  note after the first cut (salted-id reversal) proved debug-broken.
- **2026-06-12** — G2.4 **complete**: part 2 host-routes chrome `ActionRequest`s via
  `A11yHostAction::ChromeNode(NodeId)` (the whole node, never the reversed id),
  end-to-end verified headless. Then **G2.3 complete** (the last item): the three
  keyboard-model escape hatches landed in xilem-serval — Vec-per-node listener
  registries, a `focusable()` marker, and Enter/Space activation + overridable Tab.
  **Phase G2 done; the grab-bag plan is complete** (G1.1–G1.4, G2.1–G2.4 all
  landed). The remaining work is meerkat-side adoption of the serval-side runway
  (G1.1–G1.3 wheel/hit-test/cancel, G2.3 keyboard escapes), which arrives with
  window-composition P2+ and on-device chrome-control work.
