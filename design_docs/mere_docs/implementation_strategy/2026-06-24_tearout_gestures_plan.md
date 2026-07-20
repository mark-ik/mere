# Tear-out gestures plan (leaf / branch / fork + cross-graph drag)

**Date**: 2026-06-24
**⚠️ 2026-07-19 — the entire implementation below was meerkat/orrery, deleted with meerkat
(2026-07-18). The gesture stack is UNWIRED on merecat.** The kernel primitives survive
(`copy_component_from`, `copy_node_from`, graphlet bindings, `CopiedFrom` provenance); the host
half (shell commands, `fork_session_from`, the live drag seam) died with meerkat and awaits a
merecat re-wire. Fork specifically now carries layout through the `arrangement.*` / `scene.*`
facet families, not the retired `commit_positions_to_graph` write-back — see **G4-R** at the
tail. The status line below is the meerkat-era record, kept for the design (not the wiring).

**Status (meerkat-era)**: **Trichotomy + cross-graph copy/move + cascade DONE + driven** (G1
plumbing, G3 branch, G4 fork, G5 copy+move, G6 cascade via graphlet #3; graphlet #1 per-window
focus also landed). **Tile-tab leaf tear-out is now DONE + fresh-headed verified.** The one substantial
interactive item still open is the ambiguous **no-modifier orrery drag** path, which now cleanly
belongs to the notification/toast subsystem plus the pin-vs-drag-out gesture split:

- **Ambiguous-drag toast → notification subsystem.** Notifications are a **Steward-accounted
  subsystem**; toasts are their transient view; the prompt is one actionable notification.
  **Foundation DONE + tested** (`NotificationRecord` log + `record_notification` +
  `notification_rows` surfaced by the Steward). Remaining (interactive, headed): the chrome toast
  view, actionable notifications, and the no-modifier drag-out-vs-pin gesture.
- **Tile-tab origin → DONE + fresh-headed verified.** A workbench tile-tab drag-out now resolves
  to `TileEvent::Dragged { to: Outside }`, and the host maps that to `TearOut { node, from }`.
  Fresh leaf / branch / fork runs off a fresh app binary each ended with a second Meerkat window.
  The broader "orrery as desktop" dock-side + ratio setting reframe is still a separate follow-up
  if we want it; it is no longer the blocker for the tile-tab origin itself.

Both triggers queue the now-built `TearOut { node, from }`. Spun out of the
now-closed
[tearout_composability_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-19_tearout_composability_plan.md) (kept in place as the
foundational record, like the unified-document-host plan), which finished the **foundation**
(pooled-orrery authorities, per-pane focus, kernel cross-graph copy, camera-on-the-view,
multi-window 5/6). This plan owns the **named purpose that remains**: the user-facing tear-out
gestures and the cross-graph drag, plus the carried open questions. It implements the
[tear-out operations brief](../research/2026-05-11_tearout_operations_brief.md) on top of the
banked substrate.
**Code**: `crates/meerkat/` (input, chrome, window registry), `crates/graph/graph-kernel/`
(subgraph copy), `crates/forme/forme/` (graphlets), `crates/shell/frame/`.

Cross-refs:

- [tearout_operations_brief](../research/2026-05-11_tearout_operations_brief.md) — the design
  source. Locks the trichotomy, the gesture model, the stability principle, and the cascade
  rules. Written 2026-05-11 (pre-host-pivot; its gpui-era file paths are stale, the model holds).
- [tearout_composability_plan (closed)](../../archive_docs/2026-07-04_completed_plans/2026-06-19_tearout_composability_plan.md)
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

**Slice 1 — the drag→spawn pipeline — DONE (2026-06-25), driven headed.** Shift-dragging an
orrery node out spawns a leaf window carrying the node:

- `TearOutDrag { node, origin }` on `WindowView`; the GA-1 press-gate in input.rs arms it on a
  Shift-held left-press *on a node* (`orrery::node_at_screen` hit-test), so it never steals the
  orrery's node-pin pick; an empty Shift-press falls through to the marquee.
- The release dispatches: a drag past the slop queues `ShellCommand::TearOut { node }`; a
  non-moved Shift-click clears (a no-op for now). `spawn_torn_window` records the torn node and
  spawns the leaf.
- Verified by driving: a Shift-drag of the "info" node opened a second leaf window (its own
  toolbar, "+11 standing" chip). meerkat 81 lib / 153 bin green.

**Slice 2a — drag-ghost — DONE (2026-06-25), driven.** A `Chrome::tear_ghost: Option<String>`
(the dragged node's label) rendered as a `.tear-ghost` pill, positioned at the live cursor each
frame by `render` (no chrome re-render per move). Set on arm, cleared on release. Drove headed:
the pill follows the cursor during the drag and clears on drop.

**Slice 2b — `hit_test_drop` dispatcher + G5 cross-graph copy — DONE (2026-06-25).** The release
hit-tests the drop with `orrery_pane_at` and applies the drop-target grammar (OQ-1): a drop on a
**different-graph** orrery pane queues `CopyNodeAcross` (G5: `copy_node_across` →
`Graph::copy_node_from_xy`, with `CopiedFrom` provenance); anything else (the source pane,
chrome, off-window) tears into a leaf. `TearOutDrag` now carries `source_graph`. The leaf path is
driven (a same-graph drop spawns a leaf); the copy path is built + kernel-tested (drive needs a
side-by-side second graph pane).

**Slice 3 — operation split — DONE (2026-06-25).** `TearOutDrag` carries a `TearOp` fixed at
press by the modifier (Ctrl+Shift = `Fork`, plain Shift = `Branch`). The release dispatches the
tear axis on it: `Fork` → `ForkNode` (G4, wired + driven); `Branch` → `BranchNode` (G3, wired +
driven — mints a `Branched` graphlet, no longer a leaf-stub). Cross-graph-pane drops still
short-circuit to the G5 copy before the op split.

**Slice 2+ — remaining:** the **toast** on the ambiguous drop (new chrome element, escalating the
leaf in place) and the **no-modifier orrery drag-out vs pin-drag** split. Branch's real operation,
fork, move, and the **tile-tab origin** are all live.

Done when: a no-modifier tile-tab drag-out opens a leaf + shows the toast; Shift / Ctrl+Shift
select branch / fork directly; the toast escalates a leaf to branch or fork in place. **As of
2026-07-03, the tile-tab drag-out half is live and headed-verified; the toast half remains.**

### G2 — Leaf content (the C3 remainder) — content DONE (2026-06-27); one trigger live

The brief's leaf (§4.1) is a **`Workbench` pane** holding the dragged node's tile, resolving to
the **donor's pooled orrery** (same `GraphId`), with **no `Orrery` pane of its own**. Edits
propagate because both windows resolve the same pooled orrery; closing the leaf does not delete
the node.

Done when: a torn leaf window shows the dragged node's live tile, navigates on its own,
propagates node edits to the donor, and instantiates no orrery of its own.

**Status (2026-07-03): content built + tested; one trigger live.** `Shell::build_leaf_view_for(bind_graph, node)`
builds the leaf as a single `leaf_workbench_frame` (`PaneContent::Workbench` over the donor
graph, no orrery leaf) with the node opened as the focused tile
(`Workbench::ensure_tiled` + `open_tile`); `spawn_torn_window` uses it, and `TearOut` now carries
`{ node, from }` so the leaf binds the donor's pooled graph. Unit-tested
`torn_leaf_is_a_workbench_pane_with_the_node_tile`. **Trigger status:** the **tile-tab origin** now
queues `TearOut { node, from }` and is fresh-headed verified; the **ambiguous-drag toast**
("Keep as leaf") still remains.

### G3 — Branch (Shift+drag) — Phase 1 + Phase 2 DONE + driven (2026-06-25)

Mint a forme graphlet with `GraphletBinding::Branched { parent_spec, reason: "tearout-branch" }`
in the donor's graph (brief §4.2); the torn window's leaf carries the donor `GraphId` + the new
`GraphletId`. Branch + donor share kernel nodes, diverge in the graphlet's lineage facet.

**Status (2026-06-25): Phases 1 + 2 of the [graphlet wiring plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md)
landed + driven.** Shift+drag dispatches `BranchNode` → `Shell::branch_graphlet_from`, which mints
a persisted `Branched` graphlet anchored on the torn node (in the donor's `SessionGraphlets`
sidecar, round-trips a restart) and opens a window on the donor's *same* graph carrying the new
`GraphletId`. The branch window **reads as distinct** (an accent `⎇ <anchor>` chip) and
**accumulates its own lineage**: navigating in it grows the graphlet's roster (via `sync_orrery` →
`RecordBranchMember`), diverging from the donor while the new node joins the shared graph. Branch is
no longer a leaf-stub or a bare graphlet. Remaining graphlet work (reconciliation, orrery scope +
per-window focus) is Phase 3 / Slice 3 of that plan. The scouting that established the prerequisite
is preserved below.

**Substrate scouted (2026-06-25).** The prior "grep found neither" note was a wrong path — forme
lives at `crates/forme/forme`, not `crates/graph/forme`. At the right path the API is **first-class
and unit-tested**: `GraphletId`, `GraphletRef<N>`, `GraphletBinding::{UnlinkedSession, Linked,
Branched}` (graphlet.rs), 9 `GraphletKind`s, `GraphletSpec`, projection specs, and full
reconciliation types, with `GraphTree::add_graphlet` + fork transitions in reconciliation.rs.

**But the layer is built and UNWIRED.** A workspace grep finds zero live construction of a
`GraphTree` or `GraphletRef` outside forme's own tests. meerkat consumes forme's *member /
tile-tree* layer heavily (`forme::GraphMemberId` is in ~20 files) but holds no live
`GraphTree`-with-graphlets. So branch's real prerequisite is not "mint a `GraphletRef`" — it is
wiring a whole subsystem: a per-session `GraphTree`, projected into the workbench, persisted, that
a branch graphlet can group tiles + accumulate lineage in. Without that a branch graphlet is an
orphan and branch collapses to leaf.

**Decision (Mark, 2026-06-25): graphlet layer wired as its own plan** —
[graphlet wiring](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md), with broader payoff (document groups,
reconciliation, the relational-browse front-end all want it); branch is its first consumer.
**Phase 1 has since landed** (Shift+drag mints + persists a `Branched` graphlet via
`branch_graphlet_from`, driven), so branch is no longer a leaf-stub. Its **Phase 2** (the graphlet
visibly scoping the window + accumulating lineage) is tracked in that plan. The remaining
tear-out-gesture wins (the toast, the tile-tab leaf origin, G5 move) are independent of it.

**Done in passing (2026-06-25):** renamed forme's `GraphletBinding::Forked` → `Branched` (forme
green, 114 tests) so it no longer collides with the host's real `ForkNode` / `fork_session_from`
(the *opposite* operation a layer up). Follow-on: the producing functions `apply_fork` /
`detect_fork_on_manual_override` and the `ReconciliationChoice::SaveAsNewFork` choice still carry
the old "fork" word — rename when the layer is wired.

Done when: Shift+drag mints a branch graphlet in the donor; the branch window populates its own
lineage while node edits still reach the donor.

### G4 — Fork (Ctrl+Shift+drag) — the subgraph copy — DONE (2026-06-25), driven

Fork (brief §4.3) mints a new `SessionId` + `GraphId`, snapshots the **reachable connected
component** of the dragged node into it, and records a **weak** `parent_session` ref on the new
manifest. Donor unchanged; the two are independent.

- **Subgraph copy (kernel), unit-tested.** `Graph::copy_component_from(source, seed, source_graph)`
  (cross_graph.rs): finds the seed's `weakly_connected_components` component, copies each node via
  `copy_node_from` (keeping its layout position, recording per-node `CopiedFrom` provenance), and
  re-points the component's internal edges by cloning each `EdgePayload` onto the new keys (edges
  leaving the component are dropped). Test
  `copy_component_clones_the_connected_subgraph_with_edges_and_provenance` (2 of 3 nodes copied,
  the edge re-pointed, provenance on each).
- **Fork gesture wiring.** `ShellCommand::ForkNode { node, from }` → `Shell::fork_session_from`
  (session_ops.rs): clones the donor graph, `copy_component_from` into a fresh `Graph`, mints the
  fork `SessionId` + `GraphId` + manifest with `parent_session = donor`, persists the fork graph,
  and pools its orrery via `Orrery::with_graph` — **without** switching the active session. The
  command handler then opens a window onto the fork graph via the new
  `build_window_view_for(graph_id)` + the extracted `spawn_window_with_view`. The donor window is
  untouched.
- **Layout commit.** The graph's own node positions are only the spawn seed (physics owns the live
  layout in the orrery's `view`), so a naive clone opened the fork with every node piled at the
  seed. `Orrery::commit_positions_to_graph` writes the donor's live positions back into its graph
  before the clone, so the fork opens with the donor's layout. (Found + fixed by driving.)

Driven (2026-06-25): Ctrl+Shift+drag a node → a new window opens showing the full connected
component as distinct, laid-out gnodes; the donor window is intact.

**Refinements:** a borrow-split instead of the whole-donor clone (fine at demo scale); the exact
camera framing of the fork window. (**Restore-on-restart: DONE / was already working** —
`fork_session_from` inserts + `flush_dirty`s the fork manifest and saves its graph, identical to
`create_session`, so `bootstrap_sessions`' `load_from_disk` re-lists it in the switcher on
restart. The earlier "not wired" note predated that code. Locked by
`fork_session_restores_on_restart`.)

### G5 — Cross-graph single-node drag (C4 surface): copy or move

A node dragged from a pane on graph A into a pane on graph B (different pooled orreries). Copy
mints a node in B via `copy_node_from` (with `CopiedFrom` provenance); move re-points the
binding and releases A's. This is a **different axis** from leaf/branch/fork (which tear into a
new *window* of the *same* graph); G5 is graph→graph within the existing panes.

**Copy — DONE (2026-06-25)** as slice 2b's copy path: a drop on a different-graph orrery pane
queues `CopyNodeAcross { node, from, to }`, handled by `Shell::copy_node_across` (clone the source
node out of the donor, `copy_node_from_xy` into the destination with provenance, repaint). Placed
at the origin for v0 (a drop-point placement + multi-graph persistence of the copy are
refinements). **Move — DONE (2026-06-27).** An **Alt**-modified cross-graph drop queues
`MoveNodeAcross`, handled by `Shell::move_node_across` (copy via `copy_node_across`, then release
the source through the same `remove_node` + `reconcile_derived` path as `remove_focused`, targeted
by uuid; a relocation, so no eidetic tombstone, but it reaps the source-side activation). Alt is
the move modifier because Shift starts the tear and Ctrl picks fork, and desktop convention reads
Ctrl as copy. v0 is copy+release (the destination node carries a fresh uuid + the same URL);
identity-preserving move (re-point the same uuid) is a refinement. Unit-tested
`move_node_across_relocates_releasing_the_source`. **Remaining:** a drive against a side-by-side
second graph pane.

Done when: a tile dragged from a graph-A pane into a graph-B pane produces a provenance-tracked
node in B, source intact (copy) or released (move).

### G6 — Cascade on donor delete — DONE (2026-06-25) via graphlet plan #3

When a donor session with live tear-outs is killed: **branches die** with it (they live in the
donor's graph), **forks survive** (independent; the weak `parent_session` dangles), **leaves
lose their node** (the leaf window closes or shows a dismissible "donor deleted" state). Fire
the `session.cascaded_branch_delete` diagnostic.

Done when: killing a donor with a live leaf + branch + fork applies the three outcomes.

**Status (2026-06-25): met.** Graphlet plan open-item **#3** landed + driven:
`close_session` calls `Shell::close_windows_on_graph(graph)` (closes every secondary window
whose `focused_graph` is the dead graph, so branches + leaves close) then drops the dead
`graphlets` / `orreries` / `orrery_lru` pool entries; forks live on their own `GraphId` so they
survive; the session dir trash carries `graphlets.json`. Drove a leaf on the active graph →
deleting the session closed the leaf + switched to the survivor. The only unbuilt nuance is the
"donor deleted" *dismissible* leaf state (today the leaf window just closes, which the done-when
allows).

**Implementation seam (2026-06-25):** the branch-die half is scoped as **#3** in the
[graphlet wiring plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md) — `close_session` already trashes
the session dir (so `graphlets.json` trashes with it), so the work is dropping the in-memory
`graphlets` / `orreries` pool entries and closing windows on the deleted graph. Closing
windows on session-delete is a *general* multi-window gap (not branch-specific); do this with
that general handling.

### Trailing tear-out items — scope (2026-06-25)

The remaining gesture work, with seams and rough size. Independent of the graphlet subsystem
(whose own open items #1 focus-isolation and #3 donor-delete live in the graphlet plan).

- **Ambiguous-drag toast (G1) — now a notification-subsystem consumer; foundation DONE
  (2026-06-27).** Mark's reframe: notifications are a **Steward-accounted subsystem**, and toasts
  are their *transient view* — the ambiguous-drag prompt is one **actionable** notification, not a
  one-off chrome widget. The subsystem has its own plan now —
  [notification subsystem plan](2026-06-27_notification_subsystem_plan.md) (this item is its P2
  actionable consumer); the tear-out side here is just the no-modifier drag-out-vs-pin gesture.
  **Foundation built + tested:** a `NotificationRecord` log in
  `HostObservability` (beside `diagnostics`), `record_notification(severity, title, body,
  transient)`, and `HostObservability::notification_rows` which the **Steward** now surfaces
  (`steward_rows`); unit-tested `notifications_log_and_surface_for_the_steward`. **Remaining
  (interactive, headed):** (a) the **chrome toast** = the transient view that drains recent
  `transient` notifications + auto-dismisses (a `recent_notifications` accessor, a chrome toast
  queue, and a render modelled on the branch chip / context menu); (b) **actionable** notifications
  (buttons → host verbs, held in the chrome where commands are reachable); (c) the **no-modifier
  drag-out gesture** that must disambiguate from the orrery **pin-drag** (a plain node drag pins;
  the tear is a *drag-out* released outside the source orrery pane → snap back → toast). `tear_out_
  drag` arms only on a Shift-press today, so the no-modifier path + snap-back are new. Keep-as-leaf
  queues the built `TearOut { node, from }`; Branch/Fork queue `BranchNode`/`ForkNode`.
- **Tile-tab no-modifier-leaf origin (G1) — DONE + fresh-headed verified (2026-07-03).**
  A workbench tile-tab drag-out is now a plain leaf. The pelt drag-out path resolves to
  `TileEvent::Dragged { tile, to: Outside }`, and `apply_tile_event` maps that to
  `TearOut { node: tile_member, from }`. Instrumented directly at
  `workbench_pointer_down/move/up` and the emitted `TileEvent`, then driven fresh-headed: the live
  path logs `Dragged { to: Outside }`, and fresh leaf / branch / fork runs each end with a second
  Meerkat window. The broader "orrery as desktop" dock-side + ratio setting remains a separate
  design follow-up if we still want that model explicit in settings.
- **G2 leaf content — DONE (2026-06-27).** `build_leaf_view_for` / `leaf_workbench_frame`: the
  leaf is a single `PaneContent::Workbench` pane over the donor graph with the torn node as its
  focused tile, no orrery pane. `TearOut` carries `{ node, from }`. Unit-tested. (Remaining trigger:
  the toast path; the tile-tab origin is now live.)
- **G5 move-vs-copy — DONE (2026-06-27).** Alt-modified cross-graph drop → `MoveNodeAcross` →
  `move_node_across` (copy + release the source by uuid, no tombstone). v0 copy+release (fresh
  dest uuid); identity-preserving move is a refinement. Unit-tested.
- **Fork session restore-into-switcher (G4 refinement) — DONE (was already working).** Verified
  the stale note: `fork_session_from` persists the fork manifest + graph the same way
  `create_session` does, and `bootstrap_sessions` (`load_from_disk`) re-scans every session dir,
  so a fork re-appears in the switcher after restart. Locked by `fork_session_restores_on_restart`.

Done order (2026-07-03): **fork restore**, **G5 move**, **G2 leaf content**, and the
**tile-tab origin** are all landed, with the tile-tab path fresh-headed verified. Remaining as
one focused interactive session: the **ambiguous-drag toast** (new chrome + the no-modifier
orrery drag-out vs pin-drag). The leaf content + `TearOut { node, from }` are already live for
the workbench-tab path and waiting only on that toast path for the orrery-origin no-modifier case.

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
- **OQ-7 (new) — branch forme API. RESOLVED (2026-06-25).** `GraphletRef` /
  `GraphletBinding::Branched` (renamed from `Forked`) exist and are first-class + unit-tested at
  `crates/forme/forme`, but the graphlet layer is **unwired** — no live `GraphTree` outside forme's
  tests. So the substrate is the *whole layer*, not the variant. Decision: **defer branch**, wire
  the graphlet layer as its own plan (see G3). Branch stays the leaf-stub meanwhile.

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
- **2026-06-25** — **G1 slice 1 built + driven: the drag→spawn pipeline works.** `TearOutDrag`
  state + the GA-1 Shift-on-node press-gate (input.rs) + release-dispatch +
  `ShellCommand::TearOut { node }` + `spawn_torn_window`. Drove headed: a Shift-drag of an orrery
  node spawns a leaf window. meerkat 81 lib / 153 bin green. Remaining G1: the drag-ghost, the
  operation split (Shift=branch / Ctrl+Shift=fork), `hit_test_drop`, the toast, and the tile-tab
  no-modifier-leaf origin. Next natural slice: the drag-ghost (visual feedback) or the
  `hit_test_drop` dispatcher (the operation/axis split).
- **2026-06-25** — **Slices 2a + 2b + G4 kernel built + verified.** (2a) the **drag-ghost**
  (`Chrome::tear_ghost` pill positioned at the cursor each frame) — drove headed, the pill follows
  the drag and clears on drop. (2b) the **`hit_test_drop` dispatcher** (`orrery_pane_at` + the OQ-1
  drop-target grammar) + the **G5 cross-graph copy** (`CopyNodeAcross` → `copy_node_from_xy`); the
  leaf path is driven, the copy path built + kernel-tested. (G4) **`copy_component_from`** (the
  fork subgraph copy: connected-component nodes + re-pointed edge payloads + provenance), unit-
  tested. meerkat 81 lib / 153 bin + kernel green. Remaining for the operations: the modifier op
  split (branch/fork) + the toast + branch's forme wiring (OQ-7) + the fork/move gesture wiring.
- **2026-06-25** — **Op split + fork (G4) wired + driven.** `TearOutDrag` carries a `TearOp` fixed
  at press by the modifier (Ctrl+Shift = fork, Shift = branch); the release dispatches on it.
  **Fork** is end-to-end: `ForkNode` → `fork_session_from` mints an independent session + graph
  (weak `parent_session` ref), `copy_component_from` snapshots the connected component in, pools
  the orrery, and opens a new window via `build_window_view_for` + `spawn_window_with_view` —
  donor untouched, no active-session switch. Drove headed: Ctrl+Shift+drag → a new window with the
  full component laid out. Driving caught a pile-at-seed bug (kernel node positions are the spawn
  seed, not the live layout); fixed with `Orrery::commit_positions_to_graph` (bake live positions
  before the clone). meerkat 81 lib / 153 bin + kernel green. Branch is a leaf stub pending its
  forme graphlet op (G3 / OQ-7); the toast + tile-tab leaf origin + G5 move variant remain.
- **2026-06-25** — **G3 branch fully live (Phases 1+2 via the graphlet wiring plan); trailing
  items scoped.** Branch now mints + persists a `Branched` graphlet, shows a `⎇ <anchor>` chip,
  and accumulates lineage on navigation (driven). Added a scoped "Trailing tear-out items"
  section (toast, tile-tab leaf origin, G2 leaf content, G5 move, fork restore-into-switcher)
  with seams + a rough order, and pointed G6's branch-die half at the graphlet plan's #3. The
  graphlet subsystem's own open items (#1 per-window focus isolation, #3 donor-delete) live in
  the [graphlet wiring plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md).
- **2026-06-27** — **Trailing items pass: G6 + fork-restore + G5 move + G2 content done; toast +
  tile-tab reframed.** Verified **G6 cascade** is met via the graphlet plan's #3 (close-windows +
  pool-drop; marked done). **Fork restore** turned out to be already working — `fork_session_from`
  persists the manifest+graph like `create_session`, so `bootstrap_sessions`' `load_from_disk`
  re-lists it; the "not wired" note predated the code. Locked by `fork_session_restores_on_restart`.
  **G5 move:** an **Alt**-modified cross-graph drop → `MoveNodeAcross` → `move_node_across` (copy +
  release the source by uuid, no eidetic tombstone; Alt because Shift/Ctrl are the tear modifiers
  and Ctrl reads as copy). v0 copy+release (fresh dest uuid); identity-preserving move is a
  refinement. Unit-tested. **G2 leaf content:** `build_leaf_view_for` / `leaf_workbench_frame` —
  the torn leaf is a single `PaneContent::Workbench` pane over the donor graph with the node as its
  focused tile, no orrery pane; `TearOut` now carries `{ node, from }`. Unit-tested. **Reframes
  (Mark):** the **toast** becomes a **notification subsystem accounted for by the Steward** + toasts
  as its transient view — foundation built + tested (`NotificationRecord` log in
  `HostObservability`, `record_notification`, `notification_rows` surfaced by the Steward;
  `notifications_log_and_surface_for_the_steward`); the **tile-tab origin** rides an
  **orrery-as-desktop** reframe (the orrery is the persistent dock anchor; make the workbench dock
  side + ratio a setting; add a `pelt_core` drag-*out* signal). The interactive remainder (the
  chrome toast view, actionable notifications, the no-modifier drag-out-vs-pin gesture, the
  dock-side setting + toggle UI, and the `pelt_core` `DropTarget::Outside`) is sequenced for a
  headed session. Full suite green: forme 114, kernel 257, meerkat 92 lib / 178 bin, orrery 80.
  Nothing committed.
- **2026-07-03** — **Tile-tab origin landed + fresh-headed verified.** Instrumented the live
  workbench seam directly around `workbench_pointer_down/move/up` and the emitted `TileEvent`s,
  then drove the workbench tab drag-out on a fresh app binary. The live path now logs
  `Dragged { to: Outside }` and maps it to `TearOut { node, from }`; fresh leaf / branch / fork
  runs each ended with a second Meerkat window. That clears the tile-tab trigger from the
  remaining list. The only material tear-out-gesture item still open is the ambiguous
  no-modifier orrery drag path, which stays coupled to the notification/toast work.

## G4-R — Fork re-wiring on merecat (facet-carry)

**Date**: 2026-07-19. **Status**: PLAN (not started). Fork was G4-DONE in meerkat and died with
it; this re-wires it on merecat, and folds in the layout-carry change the position retirement
(S2) forced. The design (brief §4.3, G4 above) is unchanged: Ctrl+Shift+drag mints a new
`SessionId` + `GraphId`, snapshots the dragged node's reachable connected component into it, and
records a weak `parent_session` on the fork manifest; donor untouched, the two independent.

What changed since meerkat: **positions are no longer graph truth (S2).** The meerkat fork opened
with the donor's layout by calling `commit_positions_to_graph` to write live positions into the
donor graph before the clone (G4 "Layout commit" above). That method is now a no-op — the durable
layout lives in `arrangement.position` facets, and the scene's own settings in `scene.*` container
facets. So the fork carries layout by **copying facets**, not graph truth.

Rungs (kernel → host, each landable behind the one before; nothing speculative lands without the
gesture at R3):

- **R0 — expose the fork remap (kernel).** `Graph::copy_component_from` returns only the new
  `Vec<NodeKey>`; it builds a `source_key → new_key` remap internally and discards it. Return it
  too (e.g. `Vec<(Uuid /*source id*/, Uuid /*new id*/)>`, or a small `ForkCopy` struct), so the
  facet-carry can map donor facets onto the fork's fresh node ids. Pure kernel change, unit-tested
  against the existing `copy_component_*` test; no host needed to land it.
- **R1 — facet-carry helper (session-runtime).** `copy_node_facets(donor: &NodeFacetStore, fork:
  &mut NodeFacetStore, remap: &[(Uuid, Uuid)])`: for each `(source, new)` copy every facet of
  `source` onto `new` — so `arrangement.*` (position/size/sprite/…) rides along automatically, and
  so do foreign / `web.*` / `denizen.*` namespaces (a forked node keeps its whole character). Plus
  carry the container's `scene.*` from the donor `root_graph_id` to the fork's, so the fork opens
  with the donor's sizing mode + damping. This is the replacement for the retired
  `commit_positions_to_graph` layout carry.
- **R2 — the fork operation (merecat).** Re-mint meerkat's `fork_session_from` on merecat: new
  `SessionId` + `GraphId` + manifest (`parent_session = donor`), `copy_component_from` the donor
  graph into a fresh `Graph`, `copy_node_facets` (R1) via the R0 remap, persist the fork's
  `graph.json` + `facets.json`, and open it. *(Correction 2026-07-19: an earlier draft called
  merecat "single-window" — wrong; merecat has full lens multi-window. The real constraint is
  that a lens shows the ONE app's panes, and a fork is a new session.)* **"Open the fork"
  resolved by the overmap reframe (Mark, 2026-07-19; recorded in the facets plan):** the
  sessions themselves are container nodes in a graph one level up, and fork is node lineage at
  that level — so opening a fork is *navigating to its container node*, the same enter-nested-
  graph gesture as any other container, not a window question. Windows stay lenses. **v0 ships
  as session-switch** (adopt the fork after minting — the existing switch path, no window work);
  the overmap navigation replaces it when the overmap lands.
- **R3 — the gesture (merecat).** Ctrl+Shift+drag → `Action::ForkNode { node }`. frisket's
  `TileDragPayload` already scaffolds the modifier branch (leaf / branch / fork at drop); the
  ambiguous no-modifier drag + toast escalation ride the notification-subsystem follow-on, not
  this rung.
- **Cleanup.** Retire the no-op `Canvas::commit_positions_to_graph` seam once R1/R2 land (its only
  reason to exist was the old graph-truth layout carry).

Out of scope here (later, per brief §5–6): consolidation of a fork's short-term graph into an
eidetic engram, and the `parent_session` back-reference UI.
