# Host Wiring Grab-Bag Plan

**Date**: 2026-06-11
**Status**: Planned. Spun out of the
[host cheap-path plan](2026-06-10_host_cheap_path_plan.md)'s C6 (which is now
otherwise done: C0–C5 + C4c shipped). This is the checklist of record for the
remaining host-wiring parity items.
**Scope**: The grab-bag of serval / xilem-serval host capability that is wired
and tested one layer down with zero or stub meerkat callers. Each item lands
separately; this plan phases them by what unblocks what, not by date.
**Related**: the [host cheap-path plan](2026-06-10_host_cheap_path_plan.md) (the
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

### G1.1 — `on_wheel` event view (serval)

**Now**: meerkat hand-routes wheel input (`app_handler.rs:370-414`) and owns
`ScrollOffsets`; serval has the one open Stage-3 event-view gap here — no
`on_wheel` registry/dispatch parallel to `on_pointer`.

**Do**: add an `on_wheel` registry + dispatch mirroring `on_pointer`, so wheel
becomes view-owned and per-pane.

**Done when**: meerkat's hand-routed wheel and host-owned `ScrollOffsets` retire
in favour of per-pane view-routed wheel.

### G1.2 — Transform-aware hit-testing (serval)

**Now**: `walk_for_hit` (`serval_lane.rs`) composes box *locations* only, not CSS
transforms; the matrices it needs already exist in the same crate
(`compute_transform_matrix` + `conjugate_at`, used by paint). So a hit inside a
`transform`ed subtree (the orrery camera container) mis-resolves.

**Do**: thread the same transform composition paint uses into the hit walk.

**Done when**: a point inside a transformed subtree hit-tests correctly — gating
any interactive DOM content under the orrery camera (and, with G1.3, the
*interactive* external-texture element).

### G1.3 — Pointer cancellation (xilem-serval)

**Now**: `PointerEvent` has no propagation/cancel channel; `route_pointer`
leaves the stale click/key `default_prevented` value rather than recording the
pointer pass's own.

**Do**: give `PointerEvent` a `Propagation` cell and have `route_pointer` record
`default_prevented` per pointer event.

**Done when**: a drag routed through `on_pointer` can cancel/stop-propagate —
relevant the moment drags move onto the pointer path.

### G1.4 — `memoize` the stable chrome subtrees (xilem-serval)

**Now**: `memoize` is re-exported and tested over `ServalCtx` but has zero
meerkat callers, so the whole view tree rebuilds per event.

**Do**: wrap the stable chrome subtrees in `memoize`.

**Done when**: an event that touches one pane does not rebuild the
`O(view tree)` of the unaffected stable subtrees — the rebuild cost of a growing
pane tree is bounded.

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

### G2.2 — Environment threading (xilem-serval)

**Now**: dispatch builds `MessageCtx::new(Environment::new(), ...)` while builds
use the real environment — a split-brain that surfaces the moment an
environment-dependent view (theming, scaling) lands in chrome.

**Do**: pass `ctx.environment` at the three construction sites (mechanical).

**Done when**: dispatch and build share one environment; an environment-reading
view in chrome behaves the same on both paths.

### G2.3 — Keyboard-model escape hatches (xilem-serval; per the configurability rule)

**Now**: three gaps. (1) `focusable == has-on_key`, so a plain button is
keyboard-unreachable; (2) Tab is swallowed pre-routing (no tab char in
textareas, no custom order); (3) a second `on_click` on a node silently clobbers
the first (single-listener-per-node).

**Do**: add an explicit `focusable()` marker + synthetic Enter/Space activation;
an overridable Tab default; Vec-per-node listener registries.

**Done when**: a plain button is keyboard-activatable, Tab is overridable
per-view, and a node can carry multiple listeners of one kind.

### G2.4 — Chrome a11y actions

**Now**: C4c landed the chrome a11y *tree* (roles/names/bounds derive from the
rendered chrome `ScriptedDom` via `serval_a11y`), but the chrome is not
*actionable* — a screen reader cannot activate the omnibar or a toolbar button.
(No regression: chrome was unactionable before too.)

**Do**: wire accesskit actions on the chrome a11y nodes back to the host's
existing activation paths.

**Done when**: a screen reader can activate a chrome control (omnibar focus, a
toolbar button) through the a11y tree.

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
