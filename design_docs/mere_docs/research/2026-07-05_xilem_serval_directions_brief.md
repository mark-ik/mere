# xilem_serval directions: seven load-bearing ideas

**Status (2026-07-05):** directions brief, not a plan. Captures the seven
big ideas for xilem_serval raised in the 2026-07-05 architecture session,
so they survive alongside the same-day
[standards review](2026-07-05_w3c_standards_architecture_review.md), which
absorbed four of them into spec-shaped form and deliberately could not
absorb the other three (they go beyond what the platform standardizes).
Context: xilem_serval is the third `xilem_core` backend (beside Masonry and
`xilem_web`), diffing typed view trees into serval's `ScriptedDom`; see
`serval/docs/2026-05-27_serval_as_host_xilem_serval_plan.md` and the crate
docs at `serval/components/xilem-serval/src/lib.rs`.

What makes these compound: xilem_serval is a reactive layer that owns a
real DOM inside an engine we also own. Each idea exploits that double
ownership; none is available to a stack that borrows either half.

---

## 1. One app state, N windows, each a projection

The `ServalAppRunner` owns app state and a retained view tree; a
`ScriptedDom` is just a target. Let one runner drive several DOMs, one per
OS window, each window's view function a lens over shared state.

- Multi-window synced panels stop being a sync feature: there is one
  state, so there is nothing to synchronize.
- Tear-out becomes "re-parent this view subtree to another window's root."
  With keyed view identity mapped onto `graft_subtree`, a moved card keeps
  layout and tiles across the move (standards review C1; the tear-out
  trichotomy in the
  [tearout brief](2026-05-11_tearout_operations_brief.md)).
- **Divergence on the record:** the platform has atomic moves only within
  a document (`moveBefore`). Cross-window state-preserving moves are ours
  alone; the runner-projection architecture is what makes them tractable.
- **Graduated (same day):** the design landed as
  [one_state_n_windows_design](../design/2026-07-05_one_state_n_windows_design.md),
  which resolves the divergence by dissolving it — one forest dom makes a
  cross-window move same-document, so the platform primitive covers it.
  Engine side: `serval/docs/2026-07-05_movebefore_dom_standard_plan.md`,
  S1-S3 landed 2026-07-05 (`DomMutation::Moved` + `ScriptedDom::move_before`
  + splice-path verification + `Node.prototype.moveBefore`); S5 there is
  this idea's keyed-view half.

## 2. LOD as a style axis — absorbed into the standards review (C2)

The canvas exposes `--lod` on a gnode's container; container `style()`
queries restyle glance/card/full forms declaratively; node identity rides
the cascade as custom properties from the orrery NODE_SHEET. Recorded here
only for provenance; the spec-shaped version in the review is canonical.

## 3. Views projected into content, not just chrome — absorbed (C3)

Overlay-roots: xilem_serval subtrees anchored to nodes in content
documents (find-in-page, reader mode, annotations, autofill chips, agent
affordances), positioned by the engine's own layout. The review decomposes
this into top layer + anchor positioning + custom-highlight painting + UA
shadow isolation; browser features become ordinary views with app state.
The residual product idea beyond the specs: overlay views carry *app
state* and compose through `lens`/`map_action` like any chrome, so a
browser feature costs a view function, not a subsystem.

## 4. UA widgets implemented in xilem_serval — absorbed (C4)

Rebuilt form controls render their UA shadow content as the same
`text_field`/`select`/`slider` views chrome uses; `ElementInternals` is
the contract; built `base-select`-shaped. One implementation for the
omnibar and every page `<input>`; the knockout-then-rebuild strategy gets
its first rebuild in the layer where iteration is headless-testable.

## 5. The mutation log as a first-class artifact

Every state change already becomes an explicit `DomMutation` at a clean
boundary (eager apply, batched at `drain_mutations`). Record the stream:

- Deterministic replay for bug reports.
- Session restore that restores chrome state, not just tabs.
- UI-level undo.
- Device sync through the moot/moothold tiers, with the real-sync
  principle getting literal receipts: sync status is which mutations have
  propagated where, never a performed spinner.
- The same stream powers a spec-correct `MutationObserver` later
  (standards review C8): one stream, three consumers.

## 6. Extensions as lens + view

With the rhai host lane, a chrome extension is exactly two things: a
capability-scoped lens over a slice of app state, and a view function over
it. The sandbox boundary is the lens; there is no extension DOM API to
design and no message-passing bridge. Composition uses the vocabulary
xilem_serval already re-exports (`lens`, `map_state`, `map_action`,
`memoize`).

- **Collision, on the record:** WebExtensions is on a W3C track (WECG) and
  is the opposite shape (message-passing, host permissions, content
  scripts). Keep the lanes separate: lens + view for chrome extensions
  where we own both sides; WebExtensions compat for content-facing
  extensions is a separate, deferrable, possibly-never lane. Do not let
  the standard's existence bend the chrome model.
- Capability scoping should ride the same permissions spine as the
  capability-gate catalogue (standards review S2), so extension grants and
  web-content grants are one audited system.

## 7. Dual-target chrome via xilem_web

xilem_serval is deliberately "xilem_web, but native." A thin compatibility
layer would let the same chrome views run against the browser DOM when
serval-web is not the right vehicle (an ordinary PWA shell, hosted
settings/docs pages). Speculative; the cheap move now is discipline, not
code: keep view code on standard DOM/CSS idioms (the standards review is
the whitelist) so the delta between `ScriptedDom` and the browser DOM
stays near the type-erasure difference. Serval-web itself already runs in
a browser (the 2026-07-04 receipt), so this is about reach into
non-serval hosts, not about reaching the web at all.

---

## How they compound

Ideas 1 + 2 + 3 are most of meerkat's remaining chrome architecture
(windows as projections, LOD in the cascade, features as overlay views).
Idea 4 is the form-controls lane already queued for woodshed pressure.
Idea 5 underwrites sync, undo, and debugging with one mechanism. Ideas 6
and 7 are kept doors, cheap to hold open, expensive to reopen later.

Priority lives in the standards review's consumer-pulled order; this brief
adds no separate schedule. When any idea graduates to implementation, it
gets a dated plan per DOC_POLICY §8 and this brief gains a pointer.
