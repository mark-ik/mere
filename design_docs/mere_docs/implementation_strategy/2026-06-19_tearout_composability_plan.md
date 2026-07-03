# Tear-out + cross-graph composability plan (window-composition continuation)

**Date**: 2026-06-19
**Status**: **CLOSED 2026-06-24 — foundation complete + live-verified; the tear-out gestures
spun out** to the [tearout_gestures_plan](2026-06-24_tearout_gestures_plan.md). Kept in place
(not archived) as the foundational record active siblings cite, like the unified-document-host
plan.
**Rename banner (2026-07-02):** the code pointers below (`OrreryCard`, `node_card_view`,
`.node-card`, `render_as_cards`) predate the node/card terminology cleanup — see
[node_card_summoning_design](../design/2026-07-01_node_card_summoning_design.md) for the
consensus (a node's rendered body is a **gnode**, never a card) and current names
(`OrreryGnode`/`gnode_view`/`.gnode`/`render_gnodes_as_dom`). Left unedited below as the
historical record.
Banked: C1 (per-pane focus), C2 (gated, no work), C4 **core** (kernel cross-graph copy +
`CopiedFrom` provenance), camera-on-the-view (standalone), and **MW3 multi-window 5/6** (redraw
+ chrome fan-out, per-window a11y) — the last three driven headed and confirmed. **Remaining
(the named purpose — the actual tear-out):** C3's torn-tile *content* (a `Workbench` pane on the
donor's orrery), C4 *surface* (pane→pane drag + move-vs-copy), C5 the gesture model
(leaf/branch/fork + toast), OQ-B/OQ-C, and the N-orrery-elements seam. These are the live
forward scope; spin them into a dedicated tear-out-gestures plan when picked up. The live
continuation of the now-completed, archived
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
- **Sequencing dependency.** Only **C2** and **C5's drag-gesture UI** ride unified Phase 2;
  **C3** (multi-window leaf-shaping) and **C4** (kernel cross-graph copy + provenance) are
  largely *independent* of it and buildable on the existing multi-window + kernel substrate.
  (C1 shipped already, on meerkat's own hit-test.)
- **Contingent on Path A.** The above assumes Path A (DOM node-cards), which Phase 2a
  landed on. Under Path B (orrery stays a scene-surface compositor), C2's bridge would
  expand to the orrery itself.
- **Open integration point between the two plans: N orrery elements.** Unified Phase 2
  makes *the* orrery one `<orrery>`-style element. Today the side-by-side graph panes
  work on meerkat's leaf-rect hit-test (`orrery_pane_at`); when the orrery becomes a DOM
  element, that must generalize to **one orrery element per visible graph pane**, each
  resolving to its pooled orrery and taking input via the shell hit-test. Neither plan
  owns that migration yet.

## Window model (decided 2026-06-23, with Mark)

A spawned window today is a `WindowKind::Leaf` (slim chrome) whose orrery pane resolves the
active graph from the shared pool. **Audit finding:** the camera lives on the pooled
`Orrery` (orrery lib.rs:132), not on the window, so two windows on the same graph **mirror**
(one shared viewport); the per-window camera is the long-deferred "MW6" item.

**Decision.** Keep the **co-window** as a first-class window in its own right, with the
**torn tile** as a separate mode. This is *not* a drift into "window = graph": both are
**pane-configurations of the canonical model** (orrery = authority pooled by `GraphId`;
pane = view resolving by `graph_id`; window = a split of panes). A co-window is a window
with an `Orrery` pane (its own viewport); a torn tile is a window with a `Workbench` pane on
a torn node. They are configs, not divergent window kinds.

**The one real drift to correct: the camera is on the wrong side of the authority/view
line.** By the two-natured principle the pooled `Orrery` (authority) holds graph + physics +
node positions; the camera/viewport is experience-derived **view** state and belongs to the
**pane/window**. Moving it there is what makes a same-graph co-window a genuine independent
viewport — two windows = two viewports over the *shared* gyre positions ("two windows into
one document") rather than a mirror. It is the same payoff orrery-as-element gives for free
(the camera becomes the element's transform), so it lands either by moving the camera onto
the pane now or via the orrery-as-element / N-orrery-elements migration.

Guardrails so this stays the right model:

- **`WindowKind` is a chrome-template selector only** (slim vs full), never a graph/content
  fork; all graph/content resolves through pane → pool.
- **Dissolve "primary" into per-window capabilities** (save-its-panes-on-close, per-window
  a11y, optional shellbar), per P2's intent, not a fixed app-lifecycle kind.
- A **different-graph** window already works today (distinct pooled orrery + camera); only
  the **same-graph** co-window mirrors until the camera moves.

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

**Narrowed twice.** (1) The unified model: node interactivity is delivered by the
orrery's `gyre` hit path (the cond-3/4 reversal: the node is grabbed through `gyre`, the
card is an inert snapshot), so the bridge is *not* needed for node cards. (2) The
scry-compositing determination (2026-06-19): **scrying / WebView tiles do NOT use this
bridge**. They are genuinely-external native composition visuals captured to a texture
and composited under the chrome with input forwarded by API / CDP, owned by the
[native-surface-compositing plan](../../archive_docs/2026-07-03_completed_plans/2026-06-19_native_surface_compositing_plan.md)
(revision 2). The bridge's remaining scope is the narrower **serval-rendered
textured-body**: serval content composited as a texture (the
[node-representation](2026-06-18_node_representation_arrangement_plan.md) P2 textured-body)
whose input must relay back into *that serval content's own hit-test*, not a foreign WebView.

**The serval mechanism already exists — no engine ask remains (verified 2026-06-19).**
serval's `on_wheel` (xilem-serval/wheel.rs) and `on_pointer` (with `prevent_default` =
pointer cancellation, pointer.rs) are composable wrappers over an `external_texture`
element (tags.rs:74) that the engine lays out (placement from layout). The
`<external-texture>` leaf stays output-only by design; you make it input-bearing by
*wrapping* it in `on_wheel` / `on_pointer`, not by changing the leaf. **cond 5 already
does exactly this for the orrery**: window_view.rs:528-551 seats the scene as an
`external_texture` underlay and wraps the orrery pane in `on_wheel`, routing the wheel
through the document into gyre. So the earlier "build an input-bearing external-texture
element / not the output-only leaf it is today" framing is retired — the primitives are in
serval and demonstrated end-to-end.

What is left is **consumer-side and gated on a *live serval-rendered* textured-body form, which does not exist
yet.** Node-rep now has a real `Representation` enum (`Tile` / `Shape` / `Sprite`,
orrery/types.rs:67) with per-node overrides, but `Sprite` is a *static* PNG (no input relay
needed) and no live serval-rendered form exists yet. When such a form lands, render it as
`on_wheel(on_pointer(external_texture(...)))` and relay the events into the inner serval
content's hit-test — the one new wrinkle vs the orrery, which relays to gyre. That also
satisfies the host-wiring G1.1 / G1.3 callers. (In-graph DOM node-card interactivity, the
old G1.2 framing, is delivered by unified Phase 2 / the orrery `gyre` hit path.)

**Status: no work now.** The serval mechanism is in place and no consumer exists — the
static `Sprite` form needs no input relay, and a *live* serval-rendered textured-body form
is not built. C2 unblocks when that live form ships, and is then a thin consumer wiring. (Scry / WebView input is out of scope here, see
native-surface-compositing.)

Done when: a serval-rendered textured-body tile (once that form exists) takes `on_wheel` /
`on_pointer` and relays into the inner serval hit-test, with placement from layout.

## C3 — Cross-window pane resolution (the leaf)

A pane in window B that resolves to an orrery whose spatial view lives in window A. A
torn-out workbench tile is a `Workbench` pane in a new window resolving to the donor's
orrery (same graph), with no `Orrery` pane of its own. Independently navigable; edits
propagate because it resolves to the *same* orrery (leaf semantics, tear-out brief
§4.1).

Done when: a torn `Workbench`-pane window shows a shared node's live tile, navigates
on its own, propagates edits to the donor, and instantiates no orrery of its own.

Substrate (re-audited 2026-06-23, HEAD bde8342): the **leaf window-kind landed** —
`build_window_view` spawns a `WindowKind::Leaf` (slim chrome), bound to the active graph,
tested (`a_spawned_window_is_a_slim_leaf`, agent_harness.rs:670). But the leaf opens a
**single-orrery content frame on the shared graph** (main.rs:1358-1383), not a torn
Workbench tile, and the `user_event` actor fan-out (MW3 step 5) is still deferred, so a
secondary gets only spawn-time state. So C3's remaining work is (a) the tear-out *content*:
a torn `Workbench` pane showing the dragged node's tile, with no `Orrery` pane (resolving to
the donor's pooled orrery); and (b) **MW3 step 5** + **step 6** (per-window AccessKit).

**MW3 step 5 — DONE (2026-06-24), live-verified.** Both halves:

- **Redraw fan-out**: `Shell::redraw_secondary_windows` fans the `user_event` wake out to every
  non-primary window after the ctx borrow ends, so a secondary leaf repaints shared
  content/graph/physics changes live.
- **Chrome fan-out**: the sync-chip + comms updates collected during the actor-drain are
  replayed onto every secondary window (`apply_comms_to_chrome`). The earlier "slim leaves
  carry no such chrome" assumption was wrong and **driving caught it**: a slim leaf omits the
  *shellbar* but keeps the *toolbar* (sync chip) and can open comms, so the fan-out targets all
  secondaries, not just non-slim. A spawned window also **seeds** its sync chip from the primary
  (`build_window_view`) so it shows real standing immediately, not a stale `p2p off`.

**MW3 step 6 — per-window a11y DONE (2026-06-24).** `secondary_a11y_bridges:
HashMap<WindowId, AccessKitBridge>` on `Shell` (the primary keeps its `a11y_bridge` field, so
`ctx()` and the harness are unchanged); `window_ctx` forks the bridge by id; `spawn_window`
installs the secondary's adapter before showing it; `close_window` drops it. The bootstrap edge
was a non-issue (resumed creates the window + sets `primary` before the first `ctx()`).

**Live drive (2026-06-24, headed meerkat):** spawned a second window with Ctrl+Shift+N — a slim
leaf with its own toolbar + a11y bridge (no panic); both windows' chips show `tessera: +11
standing`; hide-shellbar (`>shellbar`) hides/restores the strip with the orrery reclaiming the
band. So step 5/6 are verified, not just compiled.

**C3 remaining = the torn-tile content only**: the leaf still opens a single-orrery content
frame on the shared graph (main.rs), not a torn `Workbench` pane on the donor's orrery. That
content + the tear-out gesture that produces it is the live forward work (with C5).

## C4 — Cross-graph composability (re-point / copy a pane, with provenance)

Move or copy a tile/node across orreries. Copy mints a node in the destination orrery
via the cross-graph rekey (tear-out brief §7.5) and records a **provenance edge**
(origin: source node) + lineage; move re-points the binding. Surfaced as a drag
between two panes resolving to different orreries (and a palette/command form).

Done when: a tile dragged from a pane on graph A into a pane on graph B produces a
node in B with provenance + lineage back to A, source left intact (copy) or releasing
its binding (move).

**C4 core — DONE (2026-06-23, kernel).** Both kernel pieces landed and are green
(kernel 251 tests pass incl. two new cross-graph tests; meerkat / orrery / session-runtime
build):

- **`ProvenanceSubKind::CopiedFrom`** appended (edge_taxonomy.rs, ordinal-safe at the end),
  mirrored by `PersistedProvenanceSubKind::CopiedFrom` (persistence_edge.rs) with both
  snapshot match arms wired (to.rs / from.rs) and the meerkat relation-label arm.
- **`NodeDerivation { sub_kind, source_node, source_graph }`** (types.rs) — a typed
  node-level record beside `import_provenance` / `classifications`. A cross-graph
  derivation's object node lives in *another* graph, so it can't be a petgraph
  `Provenance` edge; it is recorded on the derived node (`Node.derivations`,
  node.rs), persisted (`PersistedNode.derivations`, `#[serde(default)]`). It is the
  node-anchored analog of a `Provenance` edge and projects to a `wasDerivedFrom`
  statement under the RDF projection.
- **`Graph::copy_node_from` / `copy_node_from_with_id`** (new graph/cross_graph.rs):
  mints a fresh node cloning the donor's **content** (title, tags, classifications,
  properties, address, visuals, viewer prefs) while resetting **identity / runtime /
  session / arrangement** (fresh id, `Cold`, unpinned, no session scroll/draft, no
  frame hints), and records the `CopiedFrom` derivation `(source.id, source_graph)`.
  The donor's `import_provenance` is intentionally not carried (it's the donor's
  external-import story, not the copy's). Wasm-gating mirrors `add_node` /
  `add_node_with_id`.

The derivation record *is* the lineage (source node id + source graph); cross-graph
nav-tree grafting is deliberately not attempted (murky across graphs, not needed by the
core). **What remains for full C4** is *surface*, not kernel, and lands with C5's gesture
model: the pane→pane drag that calls `copy_node_from` with the source pane's real
constellation `GraphId`, the move variant (re-point the binding instead of copy), and the
OQ-B move-vs-copy default.

Prior substrate audit (2026-06-19), now superseded by the above: the `Provenance` edge
family existed but had no copied-from sub-kind and there was no cross-graph copy primitive.

## C5 — The tear-out gesture model (leaf / branch / fork + toast)

Implement the [tear-out brief](../research/2026-05-11_tearout_operations_brief.md) on
top of C1-C4: drag = leaf (a `Workbench` pane resolving to the donor's orrery),
Shift+drag = branch (donor's orrery, new `GraphletId`), Ctrl/Cmd+Shift+drag = fork (a
fresh orrery via the C4 rekey + a thin `Orrery` pane), toast on ambiguous drag.
Spawn-on-drop with an in-donor drag ghost.

Done when: all three operations run from the gesture model with the brief's identity
semantics, and the toast escalates a leaf in place.

## Camera on the view, not the authority — DONE (2026-06-24, standalone)

The camera (viewport) now lives on the **view**, not the pooled `Orrery` authority. Mark gave
the go-ahead to do it directly rather than wait for orrery-as-element, so this is a standalone
move, not folded into the element seam. Built and green (orrery builds; meerkat checks clean;
round-trip test + full suite pass, 78 lib / 138 bin).

**Why install/readback, not a camera-extraction.** The shared `Orrery` carries a *single*
`node_dom` + `stage_node` that bakes the camera transform (lib.rs). Two windows on one graph
share that one DOM, so the camera is necessarily applied *per render pass*; per-window
`node_dom` would be an enormous change. So the durable camera lives on the view and the orrery
applies it per pass. (The earlier "fold into the element seam / this is a throwaway scratch
register" call was wrong on the merits: the per-pass apply is architecturally forced by the
single `node_dom`, and the element transform would just replace the *install* step later, not
the durable-camera-on-the-view — which is the invariant both paths share.)

What shipped:

- **`orrery::Viewport`** (types.rs) = the full per-pane view state (camera offset / zoom / yaw /
  tilt + pan inertia), with `Orrery::viewport()` / `set_viewport()` (lib.rs). The orrery's own
  `camera` / `pan_velocity` are now per-pass working state the host drives.
- **`WindowView.viewports: HashMap<GraphId, Viewport>`** (window_view.rs) — one viewport per
  graph the window shows (handles both same-graph co-windows and multi-graph panes in one
  window).
- **ctx-lifecycle install/readback** (new meerkat `viewport.rs`): `WindowCtx::install_viewports`
  runs on ctx build; `readback_viewports` runs on the ctx's `Drop`. Bracketing the ctx
  *lifecycle* (not call sites) is what makes it correct — every camera read (screen<->world
  hit-tests) and write (pan, wheel, recenter, frame inertia, isometric) runs inside a ctx, and
  `Drop` fires on every exit including the early returns in `window_event`. The first time a
  window shows a graph it has no stored viewport, so it seeds from the orrery's current framing.
- **Persistence unchanged** (no repoint): because install seeds from the orrery, the existing
  per-graph `CameraView` save/restore keeps working — a boot-restored camera lands on the orrery
  and the first ctx adopts it. Known limitation: two windows on one graph share the *persisted*
  (per-graph) camera and diverge live; true **per-window persistence** is the remaining tail
  (the only piece the N-orrery / element seam would still improve).

## Open questions (carried from window-composition)

- **OQ-A (was OQ3) — provenance edge for cross-graph copy. RESOLVED 2026-06-19.** The
  `Provenance` edge family + `ProvenanceSubKind` exist and persist, but the sub-kinds are
  all content-transformation (`ClippedFrom` / `ExcerptedFrom` / …) with no plain copy, so a
  cross-graph copy wants a **new `CopiedFrom` (or `DuplicatedFrom`) `ProvenanceSubKind`**,
  recorded by the C4 rekey. (Ties to the per-statement-edge + projection-profile work in
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
- **2026-06-19** — **C2 checked against the code: nothing to implement now; doc
  corrected.** The serval input-bearing-external-texture mechanism already exists
  (`on_wheel` / `on_pointer`+`prevent_default` compose over `external_texture`) and cond 5
  demonstrated it end-to-end (the orrery scene is an `external_texture` underlay with the
  wheel routed through the document into gyre, window_view.rs:528-551). C2's consumer (a
  serval-rendered textured-body) does not exist (representation is still binary
  `render_as_cards`; node-rep at P0). So C2 has no serval engine ask and no work until
  node-representation P2 ships the textured-body form; reframed C2 from "build the bridge"
  to "thin consumer wiring, gated on the form."
- **2026-06-19** — **Full audit vs the code; live phases re-grounded.** C3: multi-window
  spawn + registry exist, but `spawn_window` makes a full-chrome window, not a leaf, so C3 =
  leaf-shaping + the inherited MW3 step-5 (`user_event` fan-out) + step-6 (secondary a11y).
  C4: the `Provenance` edge family exists but has no copied-from sub-kind, and there is no
  cross-graph rekey primitive (OQ-A resolved: add `CopiedFrom`, build the rekey — both
  kernel-level, buildable now). Corrected the sequencing claim (C3/C4 are independent of
  unified Phase 2; only C2 + C5's gesture UI ride it). **Most-pressing order:** (1) C4 core
  (rekey + `CopiedFrom`); (2) C3 usable leaf (leaf-shape + step-5 fan-out); (3) C5 gesture
  model; (4) the N-orrery-elements seam; (5) the OQ-B/OQ-C behaviour decisions. C2 (gated)
  and the deferred camera (eased) are not in the top five.
- **2026-06-19**: **C2 narrowed to exclude scry** (cross-plan consolidation + the
  scry-compositing determination). The determination chose native-surface-compositing
  revision 2 (off-window host HWND + capture-to-texture + API/CDP input) over the
  composition-tree / external-texture-element direction, so scrying / WebView tiles do
  **not** flow through this bridge; its remaining consumer is the serval-rendered
  textured-body (node-representation P2), and scry input is owned by
  native-surface-compositing. Also corrected the stale "node-cards take input via the
  shell hit-test" framing (the cond-3/4 reversal routes node interactivity through
  `gyre`). Doc reconciliation only, no code change.
- **2026-06-23** — **Re-audit at HEAD bde8342** (dozens of commits past the last pass; tree
  mid-refactor on the command/menu registry, build state unverified). Deltas: C3's **leaf
  window-kind landed** (slim leaf, tested) though it opens a single-orrery frame on the
  shared graph, not a torn tile, and step-5 fan-out is still deferred — so the prior
  "spawn = full-chrome" finding is stale. Node-rep replaced binary `render_as_cards` with a
  `Representation` enum (Tile/Shape/Sprite); `Sprite` is a *static* PNG, so C2's *live*
  serval-rendered consumer still does not exist (C2 verdict unchanged: no work).
  native-surface-compositing finished. C4 (rekey + `CopiedFrom`) still unbuilt. C1 intact
  through the refactor (`focus_pane_graph` / `orrery_pane_at` / its test present).
  **Re-ranked:** (1) C4 core; (2) C3 tear-out content + step-5 fan-out; (3) C5 gestures;
  (4) N-orrery seam; (5) OQ-B/OQ-C.
- **2026-06-23** — **Window-model decision (with Mark): co-window + torn-tile are
  pane-configs; the camera moves to the view.** Audited the leaf-as-built: the camera lives
  on the pooled `Orrery` (lib.rs:132), so a same-graph co-window mirrors. Verdict: not a
  drift into "window = graph" — both the co-window (Orrery pane) and the torn tile (Workbench
  pane) are configurations of the canonical orrery=authority / panes=views model. The one
  real drift is the **camera sitting on the authority**; the fix is to move it onto the
  pane/window (**promoted from deferred to pressing**). Guardrails recorded (`WindowKind`
  chrome-only; "primary" dissolves into per-window capabilities). Added the **Window model**
  section + reframed the camera section. Re-ranked: (1) C4 core; (2) **camera-to-the-view**
  (makes the co-window real); (3) C3 torn-tile content + step-5 fan-out; (4) C5 gestures;
  (5) the N-orrery seam (subsumes #2 if orrery-as-element is the chosen path).
- **2026-06-23** — **C4 core built (kernel), green.** Re-verified the plan at HEAD `7ae9539`
  first (35 commits past the prior audit were all adjacent lanes — DocumentScript/net.fetch,
  session-store cookies, orrery physics-scenes, verso-api flip seam, node-rep P1 + shape
  editor — none touched C3/C4/C5 or moved the camera), then implemented the #1 slice:
  `ProvenanceSubKind::CopiedFrom` (+ persisted mirror + both snapshot arms + meerkat label),
  the `NodeDerivation` typed node-record + `Node.derivations` field + persistence, and
  `Graph::copy_node_from` in new graph/cross_graph.rs. Two new kernel tests + the snapshot
  round-trip pass (kernel 251 green); meerkat / orrery / session-runtime build. The kernel
  primitive is done; the pane→pane drag + move-variant + `GraphId` wiring is C4 *surface*,
  folded into C5. **Re-ranked (C4 core off the top):** (1) **camera-to-the-view** (makes the
  same-graph co-window a real independent viewport); (2) C3 torn-tile content + MW3 step-5
  fan-out; (3) C5 gesture model (carries C4 surface: pane→pane drag + move-vs-copy / OQ-B);
  (4) the N-orrery-elements seam (subsumes #1 if orrery-as-element is the path).
- **2026-06-24** — **Camera path resolved + MW3 step-5 redraw fan-out shipped.** Investigated
  the #1 (camera-to-the-view) slice and resolved its open path fork: the orrery-as-element work
  is already in flight (render.rs:624-633 snapshots nodes through the camera into DOM cards) and
  the camera move is also a persistence-home question (per-graph today, must become per-view), so
  a standalone camera-extraction now would be largely thrown away. **Decision: fold the camera
  move into the N-orrery-elements seam** (camera-as-element-transform; the durable per-view camera
  + its persistence land there), not a separate refactor; recorded the invariant (durable camera
  belongs to the view) and reframed the camera section. Then shipped the next independent slice —
  **MW3 step 5 redraw fan-out** (`Shell::redraw_secondary_windows`): `user_event` now wakes every
  secondary window after the ctx borrow ends, so a leaf repaints shared changes live; the chrome
  fan-out half stays deferred to a chrome-bearing co-window. meerkat green (78 lib / 136 bin).
  **Re-ranked:** (1) **N-orrery-elements seam** (now carries the camera move — highest leverage,
  unblocks the real co-window); (2) C3 torn-tile content (Workbench pane on the donor's orrery) +
  step-6 per-window a11y; (3) C5 gesture model (carries C4 surface); (4) the chrome fan-out half
  of step 5 (needs a chrome-bearing secondary to test against).
- **2026-06-24** — **Camera-on-the-view BUILT (standalone), green.** Mark overrode the
  fold-into-element deferral above ("do the camera work, you have the authority"), so the camera
  moved off the pooled `Orrery` onto the view directly. Investigating it corrected the prior
  call: the orrery's *single* shared `node_dom` (whose `stage_node` bakes the camera transform)
  makes a per-pass camera apply architecturally forced, so install/readback is the right shape,
  not a throwaway. Shipped `orrery::Viewport` + `viewport()`/`set_viewport()`,
  `WindowView.viewports: HashMap<GraphId, Viewport>`, and ctx-lifecycle install/readback (new
  meerkat `viewport.rs`, readback on `WindowCtx::Drop` so no early-returning input path escapes
  it). Existing per-graph camera persistence kept working untouched (install seeds from the
  orrery). orrery builds; meerkat checks clean; new round-trip test
  `a_pane_camera_lives_on_the_view_and_round_trips_through_the_ctx` + full suite green (78 lib /
  138 bin). Remaining tail: **per-window** camera *persistence* (two windows on one graph share
  the persisted per-graph camera, diverge live) — the only piece the N-orrery seam still
  improves. **Re-ranked:** (1) C3 torn-tile content + step-6 a11y; (2) C5 gesture model (carries
  C4 surface); (3) N-orrery-elements seam (now just per-window-persistence + the shell-hit-test
  generalization, since the live camera move is done); (4) chrome fan-out half of step 5.
  Done this session under heavy concurrent churn (rapid commits + a half-scaffolded `signals`
  crate transiently broke the workspace build); the camera code is clean underneath.
- **2026-06-24** — **MW3 multi-window 5/6 finished + the whole foundation driven headed.**
  Step 5 chrome fan-out (sync chip / comms replayed to every secondary via `apply_comms_to_chrome`)
  and step 6 per-window a11y (`secondary_a11y_bridges` map; `window_ctx` forks the bridge;
  `spawn_window` installs before show; `close_window` drops it) both landed; the leaf-chrome
  (step 4) was already done (corrected a stale doc). Then **drove headed meerkat** to verify the
  session's work live: clean boot (no panic); the **tessera fold** shows `tessera: +11 standing`
  on the chip (folded ledger score on a real joined moot, not the raw op-count); **hide-shellbar**
  (`>shellbar`) hides the strip with the orrery reclaiming the band and restores it on toggle;
  **Ctrl+Shift+N** spawns a second OS window (slim leaf, own toolbar + a11y bridge). Driving
  **caught a real bug**: the leaf's sync chip read a stale `p2p off` because the chrome fan-out
  gated on `!is_slim` — but a slim leaf keeps its toolbar (sync chip); dropped the gate and seeded
  a spawned window's chip from the primary, re-drove, both chips now show `+11 standing`. meerkat
  80 lib / 147 bin green throughout. **Foundation complete + verified; the tear-out gestures (C3
  content, C4 surface, C5, OQ-B/OQ-C, N-orrery) are the remaining named scope** — spin into a
  dedicated tear-out-gestures plan when picked up.
