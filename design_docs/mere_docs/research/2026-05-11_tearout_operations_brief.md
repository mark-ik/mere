# Tear-out operations (leaf, branch, fork) — design brief

**Date**: 2026-05-11
**Status**: Design brief — supersedes the earlier "sticky-note fork-model decision brief" (same date), which framed this as three competing options. The resolution is that those weren't options to pick between — they were three coexisting operations the user picks at gesture time.

**Naming note (2026-05-17)**: This brief was written when the workbench arrangement authority crate was called `forme`. It has since been renamed to `forme` (per the [lineage / forme rename plan](../../archive_docs/2026-06-09_completed_plans/2026-05-17_lineage_forme_rename_plan.md)). The body below still says "forme" — read those as "forme" until a follow-up edits the prose. Function/type identifiers like `GraphletRef`, `GraphletBinding::Forked`, `GraphTree<NodeKey>` stay valid (only the crate name changed).

> **Crate-name note (2026-06-09 audit):** host-file references below (`crates/mere-host/src/tearout.rs` *(historical citation)* <!-- doc-audit: historical-path -->, `host_helpers.rs`, `host_navigation.rs`) are gpui-era; `mere-host`→`meerkat` and `mere-kernel`→`graph/graph-kernel`. The leaf/branch/fork model holds; the file homes moved with the host pivot.
**Scope**: Defines the three tear-out operations Mere supports (**leaf**, **branch**, **fork**), their identity semantics in terms of `SessionId` / `GraphId` / `GraphletId`, the gesture model that selects between them (modifier-keyed drags + a toast for ambiguous gestures), and the substrate primitives they rest on (forme graphlets, eidetic engrams, short-term memory). Resolves §11.3 of the [browser multiplexer framing brief](2026-05-11_browser_multiplexer_framing.md).

**Related**:

- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md) — §11.3 raised the question; the answer in this brief replaces the three-option framing with a three-operation one.
- [`2026-05-11_memory_tiers_brief.md`](2026-05-11_memory_tiers_brief.md) — short-term vs. long-term memory partitioning. Diff/branch state lives in short-term by default; consolidation into engrams is an affirmative gesture. This brief depends on the memory-tiers framing for its diff substrate.
- [`../../archive_docs/2026-06-09_completed_plans/2026-05-11_graph_session_manifest_plan.md`](../../archive_docs/2026-06-09_completed_plans/2026-05-11_graph_session_manifest_plan.md) — `parent_session` reference fields used by **fork**.
- Phase 2 Part 1 tear-out: [`crates/mere-host/src/tearout.rs`](../../../crates/mere-host/src/tearout.rs) *(historical citation)* <!-- doc-audit: historical-link -->. The current sticky-note implementation is the **leaf** operation in this brief's vocabulary, made explicit.
- Graphlet primitives: [`crates/forme/forme/src/graphlet.rs`](../../../crates/forme/forme/src/graphlet.rs) — `GraphletId`, `GraphletRef`, `GraphletBinding::{UnlinkedSession, Linked, Branched}`. Already first-class (the types + reconciliation are unit-tested); **branch** uses these. Note (2026-06-25): the layer is built but **not yet wired into the live shell** — no live `GraphTree` exists outside forme's tests, so branch needs that wiring first (its own plan).
- Eidetic engrams: [`crates/eidetic/src/engram.rs`](../../../crates/eidetic/src/engram.rs) *(historical citation)* <!-- doc-audit: historical-link --> — content-addressed immutable snapshots; the long-term substrate for consolidated branches and forks.

---

## 1. The trichotomy

A tear-out gesture pulls a tile out of its donor window. The user has three legitimate intents for what that should mean — and the design fits all three rather than picking one:

| Operation  | What it does                                                                                | Identity change                                  | Survives donor deletion? |
| ---------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------ | ------------------------ |
| **Leaf**   | UI-only. New window holds a tile facet of the donor's existing node in the donor's graph.   | None.                                            | No (node is in donor).   |
| **Branch** | New **graphlet** in the donor's graph. Same nodes, new lineage / grouping in forme.    | New `GraphletId` (donor `SessionId` + `GraphId`).| No (lives in donor).     |
| **Fork**   | New **graph** entirely. Snapshots the reachable subgraph into a freshly-minted session.     | New `SessionId` + new `GraphId`.                 | Yes — independent.       |

The earlier brief asked "do we eager-promote, lazy-promote, or do view-with-diff?" Wrong question. The real question is **which of these three the user wants right now**, and the answer differs per gesture. The brief picks how the user selects between them and what each one means.

## 2. Stability principle

> **A tile's binding to its node, and a node's binding to its graph, never change silently. State changes only on an affirmative user action.**

This rules out:

- Auto-fork-on-first-edit. (Was Option B of the prior brief; rejected.)
- View-with-diff with silent overlay accumulation. (Was Option C; not rejected outright but deferred to the memory-tiers brief's consolidation gesture, which is explicit.)
- Implicit graphlet creation. Branches happen because the user said "branch this," not because the system inferred divergence.

What stays explicit-only: any operation that changes `SessionId`, `GraphId`, or `GraphletId` membership. What stays implicit: routine in-tile navigation, node updates, lineage edges added by traversal — these don't change identity, they fill in detail.

## 3. Gesture model

Two ways to invoke a tear-out operation:

### 3.1 Modifier keys on drag (explicit intent)

| Gesture                    | Operation |
| -------------------------- | --------- |
| Drag (no modifier)         | **Leaf**  |
| Shift + drag               | **Branch**|
| Ctrl + Shift + drag        | **Fork**  |

Picked because:

- Drag-with-no-modifier is the cheapest, most-used path — should map to the cheapest, most-reversible operation. Leaf is exactly that.
- Shift = "structured but local" (within-donor). Branch is a new structure inside the donor.
- Ctrl+Shift = "structural and external" (cross-graph). Fork mints a new graph.

On macOS, `Ctrl` reads as `Cmd` per project convention (the keymap registrations bind both).

### 3.2 Toast on ambiguous (no-modifier) drag

When the user drags without a modifier, the default outcome is **leaf** — but a toast appears on the new window's tile pane offering escalation:

```
┌─────────────────────────────────────────────┐
│ Leafed out. Promote this to a…              │
│  [ Branch ]  [ Fork ]  [ Keep as leaf ]     │
└─────────────────────────────────────────────┘
```

Mechanics:

- Toast appears immediately on no-modifier drop.
- "Keep as leaf" dismisses the toast; the leaf is the final state. (Same as letting the toast auto-dismiss.)
- "Branch" or "Fork" runs the corresponding operation *in place* — mutating the existing leaf into a branch or fork, not re-running the tear-out gesture.
- Toast auto-dismisses after some duration (default 8 seconds; configurable). Auto-dismiss = "Keep as leaf."

Why toast and not persistent pane chrome:

- Pane chrome is already finite real estate; persistent affordances clutter it.
- The decision is moment-of-action; the toast is the right scope.
- Explicit modifier-key gestures bypass the toast entirely — power users never see it.

The toast is also the **discoverable** path for the gestures: a user who doesn't know `Shift+drag = branch` will discover it through the toast on their first ambiguous drag.

## 4. Per-operation semantics

### 4.1 Leaf

What happens on drag:

- New window opens.
- Frame layout: workbench-only (no orrery), exactly like today's Phase 2 Part 1 sticky-note.
- The new window's leaf in `FrameLayout` carries the **donor's `GraphId`** (and, transitively, `SessionId`).
- The tile's `(NodeKey, EngineDocument)` moves to the new window's `TileManager`. The donor closes its open-binding of that tile.
- No new identity. No new graphlet.

Live behaviour:

- The tile is one facet of the node. The node hasn't moved; the tile has.
- Edits to the node propagate to every window/pane that's rendering it — same as today, because both windows hold handles to the same `Entity<Graph>` via the registry.
- Within-tile navigation in the leaf window adds **lineage edges** (forme's per-node lineage facet), not new mere-kernel nodes (see §6.1 on node-per-tile).
- Closing the leaf window does not delete the node. The node stays in the donor's graph.

This is exactly what the v0 sticky-note tear-out implementation does today — re-described in the trichotomy's vocabulary.

### 4.2 Branch

What happens on Shift+drag:

- New window opens (workbench-only, like leaf).
- A new `GraphletRef` is created **in the donor's forme**:
  ```rust
  GraphletRef {
      id: <next graphlet_id>,
      anchors: vec![<the leaf tile's node>],
      primary_anchor: Some(<the leaf tile's node>),
      binding: GraphletBinding::Branched {
          parent_spec: <donor graphlet's spec, if any>,
          reason: "tearout-branch".to_string(),
      },
      kind: Some(GraphletKind::Session),
  }
  ```
- The new window's leaf carries the donor's `GraphId` plus the new `GraphletId`.
- The user's subsequent in-window actions (navigation, node selection, tile additions) populate the branch graphlet's lineage. The branch and donor diverge in their lineage facet while sharing mere-kernel nodes.

Vocabulary note: forme's graphlet-binding variant for this is `GraphletBinding::Branched` (renamed from `Forked` on 2026-06-25, once the host gained a real `fork` operation a layer up — the old name collided). It means what this brief calls **branch** at the multiplexer level, and matches the user-facing word.

Live behaviour:

- Edits to the underlying node propagate to donor (the node is shared). Edits to the *branch graphlet's lineage* are local to the branch.
- Killing the donor's session also kills the branch — the branch lives inside the donor's graph; if the graph goes, the graphlet goes.
- Consolidation: branch state is **short-term memory by default** (cheap, ephemeral, no engram cost). Explicit "consolidate this branch" gesture promotes it to long-term (engram-backed). See [memory-tiers brief](2026-05-11_memory_tiers_brief.md).

### 4.3 Fork

What happens on Ctrl+Shift+drag:

- New window opens (workbench + thin orrery, like Phase 2 Part 1's "new-graph minimized" mode today).
- A new `SessionId` and `GraphId` are minted via `GraphRegistry::create_graph` (post-manifest: `ManifestStore::create_session`).
- A snapshot of the **reachable subgraph** from the active tile's node is copied into the new graph. Cross-graph rekey assigns new `NodeKey`s during the copy.
- The new session's manifest records a weak `parent_session: Option<SessionId>` reference to the donor.
- Donor unchanged.

Reachable-subgraph scope — default to **connected component** containing the active tile's node. Matches "this thing and what it's connected to." Configurable later (whole graph, BFS depth N, explicit selection) but not v0.

Live behaviour:

- Donor and fork are independent. Edits in one do not propagate.
- Killing the donor does not affect the fork. The `parent_session` reference becomes dangling; the fork keeps working.
- Consolidation: fork starts in short-term memory; explicit gesture promotes the fork's graph state to long-term (engram-backed).

## 5. Substrate primitives

The three operations rest on three existing or already-planned primitives:

### 5.1 Graphlet (`forme`)

Already first-class. `GraphletRef` carries anchors + binding + kind. Branch is exactly "mint a new graphlet of kind `Session` with binding `Forked { parent_spec: donor_spec, ... }`."

The brief does **not** propose changes to forme's API. Branch is a use case for what's already there.

### 5.2 Engram (eidetic)

The long-term substrate. Engrams are content-addressed immutable snapshots; "edits do not exist; refreshing produces a new engram with a fresh content hash." Consolidated branches and forks become engrams.

The brief does **not** propose changes to eidetic. Engrams are the storage shape consolidation writes into.

### 5.3 Short-term memory

The cheap, ephemeral substrate. Branch state and fork state live here by default. Consolidation = "freeze this short-term state into an engram." See the memory-tiers brief.

This substrate is **not yet decided in detail** — that's the memory-tiers brief's job. For this brief's purposes, "short-term memory" means "doesn't cost an engram unless the user says so."

## 6. Upstream dependencies

Two architectural commitments this brief depends on. Both are flagged here so they don't get lost.

### 6.1 Node-per-tile (committed 2026-05-11)

Today, [`host_helpers::ensure_node_for_address_near`](../../../crates/mere-host/src/host_helpers.rs) *(historical citation)* <!-- doc-audit: historical-link --> creates a new node on every omnibar submit with a new URL. That's **node-per-navigation**. This brief assumes **node-per-tile**: a new node exists only when the user creates a new tile; within-tile navigation adds lineage edges, not nodes.

Implementation impact (out of scope for this brief; needs its own plan):

- `navigate_to` (in [`host_navigation.rs`](../../../crates/mere-host/src/host_navigation.rs) *(historical citation)* <!-- doc-audit: historical-link -->) reshapes: navigation within an existing tile updates lineage in forme without calling `ensure_node_for_address_near`.
- Opening a new tile (omnibar submit when no active tile, or explicit "open in new tile" gesture) is what creates a new mere-kernel node.
- Lineage edges in forme become the primary record of within-tile traversal history.

This is a meaningful change to existing host behaviour. Filed as a follow-up plan: "node-per-tile + lineage facet implementation" (todo).

### 6.2 Memory tiers (alongside this brief)

Branch and fork's "short-term by default; consolidate on affirmative gesture" model depends on the partition story. See [memory-tiers brief](2026-05-11_memory_tiers_brief.md), filed concurrently.

## 7. Phase 3 reshaped

Original framing: "diff capture + cross-graph rekey for sticky-notes."
New framing: **implement the leaf / branch / fork trichotomy + toast + memory-tier integration.**

Concrete deliverables:

1. **Toast UI** on no-modifier tear-out drag. Three buttons + auto-dismiss. Routes through the action bus (per the [typed action bus plan](../../archive_docs/2026-06-09_pivot_superseded/2026-05-11_typed_action_bus_plan.md)).
2. **Leaf operation** — already implemented as today's `TearOutTileAsStickyNote`. Renamed action: `TearOutTileAsLeaf`. Behaviour unchanged. (Or keep the old action name as an alias if external configs reference it.)
3. **Branch operation** — new action `TearOutTileAsBranch`:
   - Creates `GraphletRef` in donor's forme with `GraphletBinding::Branched`.
   - Opens new window with workbench-only layout; leaf carries donor `GraphId` + new `GraphletId`.
   - Emits `session.branched { session_id, parent_graphlet, child_graphlet }`.
4. **Fork operation** — new action `TearOutTileAsFork`:
   - Mints new `SessionId` + `GraphId` via `ManifestStore::create_session`.
   - Snapshots reachable connected component from the active tile's node.
   - Records weak `parent_session` reference on the new session's manifest.
   - Opens new window (workbench + thin orrery).
   - Emits `session.forked { parent, child, node_count }`.
5. **Cross-graph rekey utility** in `mere-kernel::graph` — copy a subgraph from graph A into graph B, returning the new `NodeKey`s. Used by **fork**; reusable for future cross-session import.
6. **Consolidation gesture** — `ConsolidateBranchToEngram` and `ConsolidateForkToEngram` actions, palette-exposed, no default keybinding. Promote short-term state to engrams. Detailed in the memory-tiers brief.

## 8. Open sub-decisions

### 8.1 Reachable-subgraph scope for fork

Default to **connected component** (graph-theoretic). Configurable later (whole graph, BFS depth N, explicit selection). Not blocking; pick during fork implementation.

### 8.2 Parent reference strength

For **fork**: `parent_session` is **weak** — informational only; the parent is allowed to be killed without breaking the child. Reference surfaces in the session switcher as lineage breadcrumbs.

For **branch**: parent is the donor graphlet via `GraphletBinding::Branched { parent_spec, ... }` — this is forme's existing mechanism; nothing new to decide.

### 8.3 Cascade behaviour on donor delete

If the user kills a donor session that has live branches and forks:

- **Branches** die with the donor (they live inside the donor's graph).
- **Forks** survive (they're independent; the `parent_session` reference becomes dangling but the fork keeps working).
- **Leaves** lose their node binding (the underlying node went away with the donor's graph); affected leaf windows close, or display a "donor deleted" state that the user can dismiss.

Diagnostic event `session.cascaded_branch_delete { donor, branches: [...] }` fires when branches are cascaded.

### 8.4 Toast styling / placement

Open. Tile-pane-top is the obvious anchor. Could also appear in a corner of the new window. Decide during implementation; not blocking.

## 9. What this brief locks in / doesn't

**Locks in:**

- Three coexisting operations: leaf, branch, fork. Picked at gesture time.
- Gesture model: drag = leaf; Shift+drag = branch; Ctrl+Shift+drag = fork; toast on ambiguous drag.
- Stability principle: identity changes only on affirmative user action.
- Branch = new graphlet in donor's forme. Fork = new session + graph. Leaf = no new identity.
- Branch/fork state defaults to short-term memory; consolidation to engrams is its own gesture.
- `parent_session` reference is weak.

**Doesn't preclude:**

- Future "diff" UX surfacing differences between a branch and its donor in a dedicated apparatus pane or gloss overlay.
- Future automatic consolidation policies (e.g., "consolidate branches inactive for >7 days").
- Future cross-fork merge gestures (merge a fork back into its `parent_session`).
- The `GraphletBinding::Branched` rename in forme if it becomes useful.

**Defers to follow-up plans:**

- Node-per-tile implementation (§6.1) — separate plan.
- Memory-tiers detailed substrate decisions — the memory-tiers brief.
- Toast pixel-perfect styling — implementation time.
- Diff UX — open; possibly its own brief once consolidation patterns settle.
