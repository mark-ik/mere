# Tear-out gestures plan (leaf / branch / fork + cross-graph drag)

**Date**: 2026-06-24
**Status**: Planned. Spun out of the now-closed
[tearout_composability_plan](2026-06-19_tearout_composability_plan.md) (kept in place as the
foundational record, like the unified-document-host plan), which finished the **foundation**
(pooled-orrery authorities, per-pane focus, kernel cross-graph copy, camera-on-the-view,
multi-window 5/6). This plan owns the **named purpose that remains**: the user-facing tear-out
gestures and the cross-graph drag, plus the carried open questions. It implements the
[tear-out operations brief](../research/2026-05-11_tearout_operations_brief.md) on top of the
banked substrate.
**Code**: `crates/meerkat/` (input, chrome, window registry), `crates/graph/graph-kernel/`
(subgraph copy), `crates/graph/forme/` (graphlets), `crates/shell/frame/`.

Cross-refs:

- [tearout_operations_brief](../research/2026-05-11_tearout_operations_brief.md) — the design
  source. Locks the trichotomy, the gesture model, the stability principle, and the cascade
  rules. Written 2026-05-11 (pre-host-pivot; its gpui-era file paths are stale, the model holds).
- [tearout_composability_plan (closed)](2026-06-19_tearout_composability_plan.md)
  — the foundation this rests on; C1-C4-core + camera + MW3 done there.
- [memory_tiers_brief](../research/2026-05-11_memory_tiers_brief.md) — branch/fork default to
  short-term memory; consolidation to engrams is a separate affirmative gesture.

---

## Banked foundation (done, do not redo)

Carried from the closed plan, verified at HEAD and (for the last three) driven headed:

- **Kernel cross-graph copy** — `Graph::copy_node_from` / `copy_node_from_with_id`
  (graph/cross_graph.rs), minting a fresh node from a donor node with a `CopiedFrom`
  `NodeDerivation`. Single-node today; fork needs a subgraph variant (G4).
- **Leaf window-kind** — `WindowKind::Leaf` + slim chrome; `build_window_view` spawns it
  (keyboard Ctrl+Shift+N today, not a drag). It opens a single-orrery frame on the shared
  graph, not a torn Workbench tile (G2).
- **Camera-on-the-view** — per-pane `Viewport` on the window; two views of one graph hold
  distinct cameras (so a torn window is a real independent view).
- **Multi-window 5/6** — redraw + chrome fan-out to secondaries, per-window AccessKit bridges.

## The gesture model (from the brief, locked)

A tear-out drag pulls a tile out of its donor window. Three coexisting operations, picked at
gesture time (brief §1, §3):

| Gesture | Operation | Identity change | Survives donor delete? |
| --- | --- | --- | --- |
| Drag (no modifier) | **Leaf** | none (tile facet of the donor's node, donor's graph) | no |
| Shift + drag | **Branch** | new `GraphletId` in the donor's graph (forme) | no |
| Ctrl/Cmd + Shift + drag | **Fork** | new `SessionId` + `GraphId` (snapshot subgraph) | yes |

Stability principle (brief §2): a tile's binding to its node, and a node's binding to its
graph, change only on an affirmative user action. No auto-fork-on-edit, no implicit graphlet.

Toast on the ambiguous (no-modifier) drag (brief §3.2): the drop defaults to **leaf** and a
toast offers `[Branch] [Fork] [Keep as leaf]`, escalating the leaf *in place* (not re-running
the drag). Auto-dismiss (default 8s) equals "keep as leaf". The toast is also the discovery
path for the modifier gestures.

## Slices

Ordered by dependency. Each is independently landable.

### G1 — Tear-out gesture plumbing (the drag + modifiers + toast)

The drag does not exist yet (no `tearout` / drag-ghost anywhere in meerkat; spawn is a keyboard
verb). Build:

- A drag that starts on a tile/node and crosses out of its pane's rect (distinct from the
  orrery's in-canvas node-drag, which pins). An in-donor drag ghost follows the cursor.
- Modifier read at drop selects leaf / branch / fork (Cmd==Ctrl on macOS, per convention).
- Spawn-on-drop: reuse the `SpawnWindow` path, parameterized by the chosen operation + the
  dragged node.
- The toast: a new chrome element (transient, like the context menu), three buttons routed
  through the command/action path, mutating the just-made leaf in place.

Done when: a no-modifier tile drag-out opens a leaf + shows the toast; Shift / Ctrl+Shift
select branch / fork directly; the toast escalates a leaf to branch or fork in place.

### G2 — Leaf content (the C3 remainder)

Today's leaf opens a single-orrery frame on the shared graph. The brief's leaf (§4.1) is a
**`Workbench` pane** holding the dragged node's tile, resolving to the **donor's pooled
orrery** (same `GraphId`), with **no `Orrery` pane of its own**. Edits propagate because both
windows resolve the same pooled orrery; closing the leaf does not delete the node.

Done when: a torn leaf window shows the dragged node's live tile, navigates on its own,
propagates node edits to the donor, and instantiates no orrery of its own.

### G3 — Branch (Shift+drag)

Mint a forme `GraphletRef` with `GraphletBinding::Forked { parent_spec, reason:
"tearout-branch" }` in the donor's graph (brief §4.2); the torn window's leaf carries the
donor `GraphId` + the new `GraphletId`. Branch + donor share kernel nodes, diverge in the
graphlet's lineage facet.

**Substrate check first:** the brief assumes forme's `GraphletRef` / `GraphletBinding::Forked`
are first-class, but a grep of `crates/graph/forme/src` found neither (the API may have moved or
not be wired into meerkat). Verify / wire the forme graphlet API before G3; if absent, that is
its own prerequisite.

Done when: Shift+drag mints a branch graphlet in the donor; the branch window populates its own
lineage while node edits still reach the donor.

### G4 — Fork (Ctrl+Shift+drag) — the subgraph copy

The big kernel piece. Fork (brief §4.3) mints a new `SessionId` + `GraphId`, snapshots the
**reachable connected component** of the dragged node into it, and records a **weak**
`parent_session` ref on the new manifest. Donor unchanged; the two are independent.

- **Subgraph copy** is the gap: `copy_node_from` is single-node. Build `copy_component_from`
  (or similar) on the existing building blocks: `weakly_connected_components()` (query.rs:468)
  to scope the component, copy each node via `copy_node_from_with_id`, then re-point the
  component's internal edges onto the new node keys, and record per-node `CopiedFrom`
  provenance (kernel; buildable now).
- Session/graph minting reuses the existing `CreateSession` + pool path.

Done when: Ctrl+Shift+drag mints an independent session whose graph holds a copy of the
dragged node's connected component, with provenance + a weak parent ref, donor intact.

### G5 — Cross-graph single-node drag (C4 surface): copy or move

A node dragged from a pane on graph A into a pane on graph B (different pooled orreries). Copy
mints a node in B via `copy_node_from` (with `CopiedFrom` provenance); move re-points the
binding and releases A's. This is a **different axis** from leaf/branch/fork (which tear into a
new *window* of the *same* graph); G5 is graph→graph within the existing panes. See OQ-1 on how
the two gesture axes coexist.

Done when: a tile dragged from a graph-A pane into a graph-B pane produces a provenance-tracked
node in B, source intact (copy) or released (move).

### G6 — Cascade on donor delete (the brief's §8.3, carried OQ-C)

When a donor session with live tear-outs is killed: **branches die** with it (they live in the
donor's graph), **forks survive** (independent; the weak `parent_session` dangles), **leaves
lose their node** (the leaf window closes or shows a dismissible "donor deleted" state). Fire
the `session.cascaded_branch_delete` diagnostic.

Done when: killing a donor with a live leaf + branch + fork applies the three outcomes.

### Related — N-orrery-elements seam (rendering, not a gesture)

Tracked here because the gestures multiply panes. Today side-by-side graph panes work on
meerkat's leaf-rect hit-test (`orrery_pane_at`); when the orrery becomes a DOM element
(unified Phase 2), that generalizes to one orrery element per visible pane, each resolving to
its pooled orrery via the shell hit-test, each carrying its own camera-as-transform (which also
gives per-window camera *persistence*, the one camera tail left from the foundation). Mostly
orthogonal to the gestures; sequence when the unified element work is picked up.

## Open questions

- **OQ-1 (new) — the two gesture axes. RESOLVED 2026-06-24 (with Mark): drop-target-determined.**
  The drop target picks the axis: **empty space / a new window = the tear trichotomy** (leaf /
  branch / fork, refined by the drag modifier); **another graph's pane = copy/move** (G5, refined
  by OQ-2's copy-vs-move modifier). So the same press can become either gesture; what it *means*
  is decided at drop by where it lands, not at press. G1's drop dispatch hangs off this: hit-test
  the drop point, branch on (empty/new-window) vs (a pane resolving to a different graph) vs (a
  pane on the *same* graph — see OQ-8).
- **OQ-2 (was OQ-B) — move vs copy default for G5.** Default to **copy** (safer,
  provenance-tracked); move on a modifier. Confirm at G5.
- **OQ-3 (was OQ-C) — cascade.** Resolved by the brief §8.3 (G6 above): leaves lose node,
  branches die, forks survive. Implement as specified.
- **OQ-4 (brief §8.1) — fork subgraph scope.** Default to the connected component; whole-graph
  / BFS-depth-N / explicit-selection are later options. Pick at G4.
- **OQ-5 (brief §8.4) — toast styling / placement.** Tile-pane-top is the obvious anchor;
  open. Decide at G1.
- **OQ-6 — node-per-tile. RESOLVED 2026-06-24 (by code): holds.** `navigate_member`
  (orrery/lib.rs:1721) routes through `navigate_node`, which reuses the node and records a visit;
  it does **not** mint a node. So within-tile navigation in a leaf adds lineage, not kernel
  nodes, exactly as the brief §6.1 assumes. No prerequisite work.
- **OQ-7 (new) — branch forme API.** Confirm `GraphletRef` / `GraphletBinding::Forked` exist
  and are reachable from meerkat (G3 substrate check); wire if absent.

## Gesture-design decisions (2026-06-24 probe)

An adversarial probe (4 lenses, deduped against OQ-1..7) surfaced the decisions a builder hits.
**Decided with Mark (2026-06-24): every recommendation below is adopted** (the two-drag-origins
model, the Group A defaults incl. node-only v0 and OQ-8 = no-op and empty-donor-persists, and
the Group B directions). **Group A gates G1's drop-dispatch; Group B is decide-at-implementation.**

The foundational reconciliation (resolves the apparent conflict between OQ-1's drop-target axis,
the brief's "no-modifier drag = leaf", and the orrery's existing node-drag-pins): **two drag
origins.** A drag from a **workbench tile's tab/handle** is the tear-out gesture (no modifier =
leaf + toast, per the brief). A drag of a **node in the orrery canvas** keeps pin-by-default and
only tears with a modifier (so it never steals the pin). Modifiers are captured at *press*; the
*axis* is resolved at *drop* by target (OQ-1); the modifier is then interpreted within that axis.

**Group A — blocking (decide before G1):**

- **GA-1 press-time gate** (orrery-canvas drags): a node press with Shift / Ctrl+Shift arms a
  tear-out instead of the orrery pin (`self.view.modifiers` is already read at the press,
  input.rs); no-modifier node press stays a pin. Tile-tab drags always tear.
- **GA-2 modifier timing**: capture at press, interpret at drop; do not re-evaluate mid-drag.
- **GA-3 cross-window drops (v0)**: any drop outside the origin window = new window; real
  cross-window targeting deferred. The dispatcher seam is a Shell-level
  `hit_test_drop(point) -> Option<(WindowId, GraphId, PaneId, is_orrery)>` (Shell owns the
  window registry).
- **GA-4 tearable unit (v0)**: a single node tile, via the tab/handle affordance. Field
  regions, edges, multi-select, the orrery-view itself = named follow-ups.
- **GA-5 drag-ghost**: a screen-space Chrome transient (positioned DOM, mirroring the context
  menu), following the cursor with the node's icon/color/title; the origin node stays put.
- **GA-6 empty-donor fate**: when the last tile is torn, the donor window persists empty with an
  explicit close (optional "no tiles left, close?" safety toast). No silent auto-close.
- **GA-7 branch frame binding**: add `graphlet_id: Option<GraphletId>` to the leaf pane (None =
  donor default; G3 sets it) so a branch's workbench populates the new graphlet.
- **OQ-8 same-graph other-pane drop** (the third drop-target case, beyond OQ-1's two): a node
  dropped on another pane resolving to the *same* graph. Recommend **no-op** for v0 (a node is
  already in that graph; a same-graph "copy" is a duplicate, deferred), with new-window as the
  fallback if a drop must do something.

**Group B — decide at implementation / deferred:** leaf-of-a-leaf chain (allow; same graph);
no-modifier leaf is final on toast dismiss (no auto-close); toast placement (measured toolbar
height, not hardcoded); toast dismiss on first-input-or-8s; keyboard/touch path via node +
tile-tab context-menu "Tear out as…" entries (command-registry routed); origin node selection
unchanged during drag; pre-drop drop-zone highlight + operation badge (only live-updating if
modifiers are release-time, which GA-2 says they are not, so the badge is fixed once armed);
fork/branch not undoable as a gesture (undo is intra-session); consolidation surface = opt-in
palette action + a close-time prompt for an unsaved short-term fork; fork provenance stays
per-node `CopiedFrom` (+ an optional `ForkedComponent` diagnostic); cross-fork merge = none in
v0 (palette parent link suffices); auto-consolidation policy disabled by default.

## Progress

- **2026-06-24** — Spun out of the completed tearout-composability plan (foundation done +
  driven headed). Scoped the gesture model against the current substrate: leaf window-kind +
  camera + MW3 + single-node `copy_node_from` are banked; the drag gesture, toast, leaf
  content, branch graphlet wiring, fork subgraph copy, and G5 copy/move are the live work.
  Grounded the gaps (no tear-out drag in meerkat; forme graphlet API unconfirmed; subgraph copy
  absent but `weakly_connected_components` is the building block). No code written.
- **2026-06-24** — **OQ-1 resolved (with Mark): drop-target-determined** (empty/new-window =
  trichotomy; another graph's pane = copy/move). Ran an adversarial 4-lens probe for further
  decisions; **OQ-6 resolved by code** (node-per-tile holds via `navigate_member`). Added the
  Gesture-design decisions section: the two-drag-origins reconciliation + Group A (GA-1..7 + OQ-8,
  blocking) + Group B (deferred). The drag-origin model (tile-tab tear vs orrery-node
  modifier-tear) is the foundational call to confirm before G1.
- **2026-06-24** — **Mark ratified all probe recommendations.** The two-drag-origins model,
  Group A defaults (GA-1..7 + OQ-8 = no-op + node-only v0 + empty-donor-persists), and the Group B
  directions are decided. The plan's design space is now settled; G1 (the drag + toast plumbing)
  is the clean build entry, with the Shell-level `hit_test_drop` dispatcher seam as its spine.
