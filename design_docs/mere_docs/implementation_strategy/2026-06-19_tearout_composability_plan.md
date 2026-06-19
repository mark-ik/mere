# Tear-out + cross-graph composability plan (window-composition continuation)

**Date**: 2026-06-19
**Status**: Planned. The live continuation of the now-completed, archived
[window_composition_plan](../../archive_docs/2026-06-19_completed_plans/2026-06-11_window_composition_plan.md).
That plan's enabling move (P1, the pooled orrery authorities) is **banked and
load-bearing**, and **C1 (the per-pane focus / active-session decoupling) shipped
2026-06-14** as the pane-as-unit refactor (see C1). This plan owns what remains of the
host **interact** stage: the external-texture-input bridge (narrowed to genuinely
external content) and the tear-out + cross-graph composability gestures. **The later
phases ride the unified document model** (one shell document, shell hit-test, DOM
node-cards, orrery-as-element; Path A) and are sequenced *after* unified Phase 2 (see
Relationship to the unified document model).
**Code**: `crates/meerkat/`, `crates/orrery/`, `crates/forme/`, `crates/shell/frame/`.

Cross-refs:

- [window_composition_plan (archived)](../../archive_docs/2026-06-19_completed_plans/2026-06-11_window_composition_plan.md)
  — origin; P1 done; the architecture this rests on (orrery = authority, panes =
  views resolving by `graph_id`).
- [tearout_operations_brief](../research/2026-05-11_tearout_operations_brief.md) —
  the design source for the leaf / branch / fork gesture model and the cascade rules.
- [unified_document_host_plan](2026-06-17_unified_document_host_plan.md) — the shell
  document + shell hit-test substrate this rides; where the P2 input spine landed.
- [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md)
  — names the host **interact** stage; this plan inherits that ownership from
  window-composition.
- [multi_graph_activation_plan](2026-06-09_multi_graph_activation_plan.md),
  [host_wiring_grabbag_plan](2026-06-11_host_wiring_grabbag_plan.md) — consumers that
  rode "window-composition P2+"; now this.

---

## Inherited foundation (verified done 2026-06-19, do not redo)

Code-checked against the tree, not the prior log:

- **P1 orrery pool**: `orreries: HashMap<GraphId, Orrery>` + `orrery_lru` +
  `MAX_POOLED_ORRERIES = 8` (main.rs), `reap_graph` (constellation.rs:537),
  `park_physics` (orrery lib.rs:449), eviction skipping focused graphs
  (session_ops.rs:563-583), Steward live-count (pane_data.rs:228). OQ2 (park /
  unload / LRU) resolved.
- **P2 load-bearing half**: the ctx borrows the whole pool; render draws multiple
  orreries (`secondary_orreries`, render.rs:578); `OpenGraphBeside` summons a second
  graph pane (input.rs:283 → app_handler.rs:730 → session_ops.rs:594); per-pane
  render + wheel/hover live.
- **frame_ops split**: 505 LOC across nine modules (the old 2.7k-LOC ceiling risk is
  closed).
- **C1 focus/session decoupling — done** (pane-as-unit 1/5-5/5, commits
  `9876110`..`7f6671d`, 2026-06-14): `focus_pane_graph` (lightweight focus-follows-click,
  session_ops.rs:111), per-pane `save_session`, scoped switch re-point
  (`retag_graph_bound_from`, layout.rs:261), display/binding off the focused pane,
  `session_for_graph` (session_ops.rs:86). Regression test added 2026-06-19.

## Relationship to the unified document model

The [unified_document_host_plan](2026-06-17_unified_document_host_plan.md) changed the
substrate under this plan, mostly in its favor:

- **A "pane" is now a DOM subtree hit-tested once.** This plan was specced when panes
  were `netrender::Scene` bands stitched by Y-coordinate with ~5 disjoint hit-test
  entry points. The unified model gives one shell document per window, one shell
  hit-test (with the DOM-vs-gyre two-hit-test split), one focus ring, one a11y tree.
  The resolution key is unchanged (a pane resolves to a pooled orrery by `graph_id`);
  the substrate is DOM, and "which pane is under the cursor" is answered by the shell
  hit-test, not hand-summed rects.
- **C1 is done; C2 is narrowed.** C1 (focus/session decoupling) shipped as the
  pane-as-unit refactor using meerkat's own `orrery_pane_at` leaf hit-test (not the
  shell hit-test); Path A's DOM node-cards narrow C2. See each phase.
- **Sequencing dependency.** C2-C5 ride unified Phase 2 (partly shipped); sequence the
  later phases after it. (C1 shipped already, on meerkat's own hit-test.)
- **Contingent on Path A.** The above assumes Path A (DOM node-cards), which Phase 2a
  landed on. Under Path B (orrery stays a scene-surface compositor), C2's bridge would
  expand to the orrery itself.
- **Open integration point between the two plans: N orrery elements.** Unified Phase 2
  makes *the* orrery one `<orrery>`-style element. Today the side-by-side graph panes
  work on meerkat's leaf-rect hit-test (`orrery_pane_at`); when the orrery becomes a DOM
  element, that must generalize to **one orrery element per visible graph pane**, each
  resolving to its pooled orrery and taking input via the shell hit-test. Neither plan
  owns that migration yet.

## C1 — Per-pane focus / active-session decoupling — DONE (2026-06-14)

**Shipped as the pane-as-unit refactor** (commits `9876110`..`7f6671d`, the 1/5-5/5
series + scoping doc `1916f73`). The audit that scoped this plan was stale: it grepped
for a `focused_pane` field that was never the chosen design. The realized design keys
focus on **`focused_graph` + `focus_pane_graph`**, not a separate field:

- `focus_pane_graph(graph_id)` (session_ops.rs:111) is the lightweight focus-follows-
  click: it sets `focused_graph` and re-keys `active_session_id` / `session_dir` to that
  graph's session, and does **not** reload the graph, clear caches, or re-point the frame
  layout (unlike `switch_session`).
- Wired into the orrery press via `orrery_pane_at` (pane_geom.rs:23, the Orrery leaf
  under the cursor), so a press on a second graph-pane focuses it and the pointer /
  context menu / selection then act on it (input.rs:345).
- `save_session` resolves the focused pane's session (1/5); the switch path re-points
  only the outgoing graph's panes via `retag_graph_bound_from` (3/5); display / binding
  re-key off the focused pane (4/5); a second graph-pane reloads on restart.

Verified 2026-06-19: meerkat builds, 64 lib / 94 bin tests green, plus a new regression
test `focus_pane_graph_moves_focus_without_a_switch_or_clobber` (agent_harness.rs) that
locks the contract (focus + active-session re-key; both graphs stay pooled — no reload,
no clobber).

Note vs the unified model: this was done with meerkat's own `orrery_pane_at` leaf-rect
hit-test, *complementary to* (not dependent on) the unified shell hit-test. When the
orrery fully becomes a DOM element (unified Phase 2 cond 1), focus-follows-click migrates
onto the shell hit-test (the N-orrery-elements integration point above).

Done: clicking a second graph-pane navigates + saves it independently, with no re-point
of the first pane and no cache clobber. Verified by the regression test + the
pane-as-unit series.

## C2 — External-texture-input bridge (the P2-companion lynchpin)

**Narrowed by the unified model.** Path A makes node-cards DOM subtrees that already
take input through the shell hit-test (Phase 2a), so the bridge is *no longer needed
for node cards*. It is needed only for **genuinely external content**: scrying /
WebView tiles and mixed pelt-surface content, which stay `<external-texture>` blits.
The remaining work is the external-texture *element that bears input* (an
`<external-texture>` that takes `on_wheel` / pointer cancellation + layout-derived
placement, not the output-only replaced leaf it is today). Lives in serval
(xilem-serval / serval-scripted-dom), **coordinated with the serval/pelt agent, not
solo**; it is the unified plan's "tiles follow-on" (`platen-view` + `<external-texture>`,
pelt V6). Unblocks the host-wiring G1.1 / G1.3 callers for external tiles. (In-graph
DOM node-card interactivity, the old G1.2 framing, is delivered by unified Phase 2.)

Done when: an external content tile (scrying / WebView) is an input-bearing element in
the pane DOM taking `on_wheel` + placement from layout, with a live G1 caller.

## C3 — Cross-window pane resolution (the leaf)

A pane in window B that resolves to an orrery whose spatial view lives in window A. A
torn-out workbench tile is a `Workbench` pane in a new window resolving to the donor's
orrery (same graph), with no `Orrery` pane of its own. Independently navigable; edits
propagate because it resolves to the *same* orrery (leaf semantics, tear-out brief
§4.1).

Done when: a torn `Workbench`-pane window shows a shared node's live tile, navigates
on its own, propagates edits to the donor, and instantiates no orrery of its own.

## C4 — Cross-graph composability (re-point / copy a pane, with provenance)

Move or copy a tile/node across orreries. Copy mints a node in the destination orrery
via the cross-graph rekey (tear-out brief §7.5) and records a **provenance edge**
(origin: source node) + lineage; move re-points the binding. Surfaced as a drag
between two panes resolving to different orreries (and a palette/command form).

Done when: a tile dragged from a pane on graph A into a pane on graph B produces a
node in B with provenance + lineage back to A, source left intact (copy) or releasing
its binding (move).

## C5 — The tear-out gesture model (leaf / branch / fork + toast)

Implement the [tear-out brief](../research/2026-05-11_tearout_operations_brief.md) on
top of C1-C4: drag = leaf (a `Workbench` pane resolving to the donor's orrery),
Shift+drag = branch (donor's orrery, new `GraphletId`), Ctrl/Cmd+Shift+drag = fork (a
fresh orrery via the C4 rekey + a thin `Orrery` pane), toast on ambiguous drag.
Spawn-on-drop with an in-donor drag ghost.

Done when: all three operations run from the gesture model with the brief's identity
semantics, and the toast escalates a leaf in place.

## Deferred — per-pane camera (two spatial views of one graph)

Pull the camera out of `Orrery` into the `Orrery` *pane*, so two `Orrery` panes of the
same graph hold distinct cameras. Not needed by C1-C5 (those have at most one spatial
view per graph). **Likely eased, possibly dissolved, by the unified model:** under
orrery-as-element the camera is the element's transform/viewport, so two `<orrery>`
elements over one pooled orrery hold distinct cameras naturally, without pulling the
camera out of the `Orrery` struct. Revisit as part of the N-orrery-elements work rather
than as a separate extraction.

## Open questions (carried from window-composition)

- **OQ-A (was OQ3) — provenance edge direction + family.** Confirm the existing
  provenance edge family carries "copied-from across graphs" cleanly, or whether copy
  wants a distinct sub-kind (C4). Check before building, per the consumer-pull rule.
  (Ties to the per-statement-edge + projection-profile work in
  [petgraph_rdf_plan](2026-06-18_petgraph_rdf_plan.md): a cross-graph copy is exactly a
  provenance statement.)
- **OQ-B (was OQ4) — move vs copy default.** A cross-graph drag defaults to which?
  Likely copy (safer, provenance-tracked); move on a modifier. Decide in C4.
- **OQ-C (was OQ5) — linked-tile lifecycle on donor delete.** The brief §8.3 cascade
  rules (leaves lose their node, branches die, forks survive) map onto the linkage
  axis; wire them in C3/C5.

## Progress

- **2026-06-19** — Spun out of the completed [window_composition_plan](../../archive_docs/2026-06-19_completed_plans/2026-06-11_window_composition_plan.md)
  on a code-verified audit: P1 banked and load-bearing, the P2 input tail migrated to
  the unified-document-host plan, `frame_ops` split done, OQ2 resolved. C1-C5 +
  deferred camera are the live forward scope; C2 is coordinated with the serval/pelt
  agent. No code written.
- **2026-06-19** — Folded in the unified-document-model ramifications: added the
  Relationship section; reworded C1 ("delivered by", not "converges with") and narrowed
  C2 to genuinely external content (node-card interactivity is delivered by unified
  Path A); recorded the N-orrery-elements integration point, the Path A contingency,
  and the sequencing dependency; noted the deferred camera as eased by orrery-as-element.
- **2026-06-19** — **C1 verified already done** and regression-tested. Doing C1 surfaced
  that it shipped 2026-06-14 as the pane-as-unit 1/5-5/5 series (`focus_pane_graph` +
  `focused_graph`, not a `focused_pane` field — the audit grepped the wrong symbol).
  meerkat builds + tests green; added
  `focus_pane_graph_moves_focus_without_a_switch_or_clobber`. Reframed C1 to DONE; the
  live forward scope is now C2-C5 + the deferred camera.
