# Node-per-tile + lineage facet — implementation plan

**Date**: 2026-05-11
**Status**: Implementation plan — pre-build
**Scope**: Reshape Mere's node-creation semantics from **node-per-navigation** (current behaviour) to **node-per-tile**, and introduce the **lineage facet** as the second per-node aspect (the spatial/semantic facet stays in `mere-kernel::graph::Graph`). Operationalises the model committed in the [tear-out operations brief](../research/2026-05-11_tearout_operations_brief.md) §6.1. Prerequisite for Phase 3's **branch** operation, which depends on lineage edges being live and on within-tile navigation *not* polluting the canonical graph with one-shot URL nodes.

**Related**:

- [`../research/2026-05-11_tearout_operations_brief.md`](../research/2026-05-11_tearout_operations_brief.md) — §6.1 (node-per-tile commitment), §4.1 leaf semantics ("within-tile traversal becomes lineage edges in graph-tree, not new mere-kernel nodes").
- [`../research/2026-05-11_memory_tiers_brief.md`](../research/2026-05-11_memory_tiers_brief.md) — within-tile history is short-term memory; lineage edges between anchor nodes (Provenance::Traversal in graph-tree) are part of graph state and become long-term on consolidation.
- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) — §11.4 places this plan as step 4, immediately before Phase 3.
- Current node creation: [`crates/mere-host/src/host_helpers.rs`](../../../crates/mere-host/src/host_helpers.rs) — `ensure_node_for_address_near`.
- Current navigation: [`crates/mere-host/src/host_navigation.rs`](../../../crates/mere-host/src/host_navigation.rs) — `navigate_to`.
- Tile state: [`crates/mere-host/src/tiles.rs`](../../../crates/mere-host/src/tiles.rs) — `TileManager`.
- Graph-tree primitives: [`crates/graph/graph-tree/src/`](../../../crates/graph/graph-tree/src/) — `GraphTree<N>`, `MemberEntry`, `Provenance::Traversal`.

---

## 1. Goal + done conditions

**Goal:** a node in `mere-kernel::graph::Graph` represents *a thing the user explicitly opened as a tile*, not "every URL that crossed the omnibar." Within-tile navigation accumulates as ephemeral history in short-term memory; **crossing into a new tile** is the act that mints a new anchor node and (when lineage is live) a lineage edge from the source anchor.

**Done when (v0 — semantics shift, no graph-tree wiring yet):**

- `TileManager` tracks **per-tile navigation history** (URL + timestamp) for each open tile.
- `navigate_to` distinguishes **within-tile navigation** (extends history; updates omnibar; loads document; *does not* create a node) from **new-tile creation** (mints a node; opens a new tile; sets new tile as active).
- Default omnibar submit (Enter) is **within-tile** when a tile is active; **new-tile** when no tile is active (empty workbench).
- An explicit "open in new tile" gesture exists: keyboard `Ctrl/Cmd-Enter` in the omnibar, and a palette action `OpenInNewTile`. Triggers new-tile creation regardless of active-tile state.
- Within-tile back/forward navigates the tile's history list (no graph mutation).
- `ensure_node_for_address_near` is **only called** on new-tile creation, not on every navigation.
- Existing tile reload (F5 / `Reload`) reloads the active history entry, not the active node's URL specifically — the two diverge once history accumulates.
- Diagnostics: `tile.navigated_within { tile, from, to }` for within-tile; `tile.opened { tile, anchor_node, source_anchor }` for new-tile; `node.created { reason: "new_tile", url }` replaces today's implicit per-navigation node creation.

**Done when (v1 — lineage facet wired):**

- `mere-kernel::graph::Graph` owns a paired `GraphTree<NodeKey>` (or `mere-host` instantiates one per workbench pane; see §6 for the decision).
- New-tile creation calls into the graph + graph-tree pair: adds the node to the petgraph; adds the member to graph-tree with `Provenance::Traversal { source: source_anchor, edge_kind: Some("user-spawned-tile") }`.
- The first tile of a session (no source anchor) gets `Provenance::Anchor`.
- Manually-added nodes (palette: "add node," future drag-into-orrery) get `Provenance::Manual`.
- Diagnostics: `lineage.edge_added { from, to, edge_kind }` fires alongside `tile.opened`.

**Explicitly NOT in scope:**

- Within-tile back/forward **UI** beyond the F5 reload reshape. Browser-style back/forward buttons against per-tile history land in a follow-up.
- Lineage visualization (showing the graph-tree's traversal edges in the orrery). Cartography work.
- "Promote this within-tile visit to its own anchor" gesture. Future; not blocking.
- Cross-graph lineage (a lineage edge that points at an anchor in a different session). Phase 3+; not blocking for branch.
- `graph-tree`'s other concerns (lifecycle Warm/Cold, lens, layout, graphlet membership beyond §6.4). Each gets attention when consumers materialise.

## 2. The three-layer model

```
┌─────────────────────────────────────────────────────────────┐
│  mere-kernel::graph::Graph                                  │
│  (canonical anchor graph)                                   │
│                                                             │
│  Nodes:  one per *tile-worthy thing*. Anchored at the URL   │
│          the user explicitly chose to open as a tile.       │
│  Edges:  semantic — hyperlinks invoked, manual user adds,   │
│          future agent-derived. NOT navigation history.      │
└─────────────────────────────────────────────────────────────┘
                            ▲
                            │ shares NodeKeys
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  graph-tree::GraphTree<NodeKey>                             │
│  (lineage / topology facet)                                 │
│                                                             │
│  Members:    one per node in the graph; carries Provenance  │
│              (Traversal { source, edge_kind } | Anchor |    │
│              Manual | Derived | AgentDerived | Restored).   │
│  Topology:   parent/child relationships between anchors —   │
│              "I opened this tile FROM that one."            │
│  Graphlets:  named groupings of members. Phase 3 branch     │
│              creates a graphlet here.                       │
└─────────────────────────────────────────────────────────────┘
                            ▲
                            │ per-tile, ephemeral
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  Per-tile navigation history                                │
│  (short-term memory, see memory-tiers brief)                │
│                                                             │
│  Stored on TileManager: each tile carries Vec<HistoryEntry> │
│  where HistoryEntry = { url, loaded_at, doc_cache_key }.    │
│  Survives in-memory; medium-term persistence is optional    │
│  (a sidecar like `views/`; see §10).                        │
└─────────────────────────────────────────────────────────────┘
```

The key invariant: **a navigation event affects exactly one layer at a time.** Within-tile navigation → history only. New-tile creation → kernel graph + graph-tree (when live). Manual node add → kernel graph + graph-tree. Nothing implicitly creates anchors as a side effect of typing in the omnibar.

## 3. v0 — semantics shift

The v0 milestone changes behaviour without introducing graph-tree as a dependency. It's the smallest change that unblocks Phase 3.

### 3.1 `TileManager` extension

```rust
// in crates/mere-host/src/tiles.rs

pub struct HistoryEntry {
    pub url: String,
    pub loaded_at: SystemTime,
}

pub struct TileState {
    pub anchor: NodeKey,
    pub history: Vec<HistoryEntry>,
    pub history_cursor: usize,
    pub documents: HashMap<String, EngineDocument>, // keyed by URL
}

pub struct TileManager {
    open: Vec<TileState>,
    active: Option<usize>,
}
```

Today's `TileManager` stores `Vec<NodeKey>` + a flat document map; the v0 reshape gives each tile its own history list and document cache.

Migration: existing TileManager API (`open_or_focus`, `active_node`, `active_document`, `close_index`) keeps working with adapted internals. New methods: `active_history()`, `push_history(url, doc)`, `navigate_back()`, `navigate_forward()`.

### 3.2 `navigate_to` reshape

Two operating modes:

```rust
pub enum NavigateMode {
    /// Default. Updates active tile's history; loads doc; no kernel
    /// graph mutation. When no tile is active, falls through to
    /// NewTile.
    WithinTile,
    /// Explicit. Always creates a new anchor node and opens it as
    /// a new tile. Triggered by Ctrl/Cmd-Enter or palette
    /// "OpenInNewTile."
    NewTile,
}

pub fn navigate_to(
    &mut self,
    address: String,
    mode: NavigateMode,
    push_history: bool,
    cx: &mut Context<Self>,
) { /* ... */ }
```

Flow for `NavigateMode::WithinTile`:

1. Resolve active workbench's tile.
2. If no active tile → escalate to `NavigateMode::NewTile`.
3. Load the document for `address` (via `loader::load`).
4. Push `HistoryEntry { url: address.clone(), loaded_at: now() }` onto the tile's history; advance `history_cursor`.
5. Insert the loaded document into the tile's document cache.
6. Update toolbar location + omnibar text.
7. **Do not** call `ensure_node_for_address_near`.

Flow for `NavigateMode::NewTile`:

1. Resolve the active workbench's pane + the graph the pane is bound to.
2. Determine the source anchor (active tile's anchor, if any) for lineage tracking.
3. Call `ensure_node_for_address_near(graph, &address, source_anchor)` → new anchor `NodeKey`.
4. Load the document.
5. Open a new tile in the workbench: `TileManager::open_or_focus(new_anchor, doc)` → seeds the tile with one history entry.
6. Active tile = the new tile.
7. Update toolbar + omnibar.

Existing call sites of `navigate_to`:

- Omnibar submit → defaults to `WithinTile`.
- Reload (`F5`, `Cmd-R`) → reloads the *active history entry's URL*, not the anchor — `WithinTile` on the same URL (re-fetches).
- Back/forward (`Alt-Left/Right`) → operates on the **tile's history**, not on the cross-tile history that exists today. The cross-tile "history" goes away in v0 — back/forward = within-tile back/forward.
- Tile-strip clicks (`focus_tile_in`) → switch active tile; update omnibar to that tile's current history-cursor URL. Doesn't create or navigate.

### 3.3 New gesture: open-in-new-tile

Two surfaces:

- **Keyboard:** `Ctrl/Cmd-Enter` in the omnibar. Wired by the omnibar input module (mere-host-graphshell). The omnibar emits `OmnibarSubmitted { text, new_tile: bool }`; the host's subscription routes to `navigate_to` with the appropriate mode.
- **Palette:** `OpenInNewTile` action, bus-dispatched to the active workbench's pane.

Default tile creation when omnibar is submitted with no active tile is implicit `NewTile` mode — there's no other meaningful interpretation, and the user is already typing a URL.

### 3.4 Reshape `ensure_node_for_address_near` (no API change, fewer call sites)

The helper itself stays unchanged — it's still the right operation when we actually want to create a node. What changes is **who calls it**. Today: every `navigate_to`. v0: only `NewTile` mode.

A debug-only safety net (compile-time `#[cfg(debug_assertions)]` log) flags any call to `ensure_node_for_address_near` that wasn't preceded by an explicit "new tile" intent — catches accidental regressions during the refactor.

### 3.5 What about cross-tile cross-history navigation?

Today's `HostRoot` carries a single `history: Vec<String>` + `history_cursor: usize` at the *host* level. v0 removes those fields. Each tile owns its own history. The chrome's back/forward buttons (currently bound to host-level history) become tile-history buttons — they operate on the active tile.

This is a deliberate behavioural change: cross-tile-back-button isn't a thing post-v0. If the user wants to revisit a previous tile, they switch tiles via the tile strip. If they want to revisit a URL within a tile's history, they back/forward within the tile.

Diagnostics for the removal: `host.history_removed` fires once at startup so the apparatus reflects the architectural shift.

## 4. v0 — done conditions checklist

- `TileManager` carries per-tile history.
- `navigate_to` accepts `NavigateMode`; defaults to `WithinTile`.
- Within-tile navigation does not create kernel nodes.
- `Ctrl/Cmd-Enter` triggers `NewTile`.
- Tile-strip clicks update omnibar to the tile's current URL (not the anchor's URL — they may differ once within-tile navigation has happened).
- Back/forward operate on tile history.
- The host-level `history` + `history_cursor` fields are gone from `HostRoot`.
- Diagnostics: `tile.opened`, `tile.navigated_within`, `node.created { reason: "new_tile" }` fire at the right points.
- Existing tests pass; new tests cover the within-tile-vs-new-tile branching.

## 5. v1 — lineage facet wired

After v0 lands, lineage edges become explicit in `graph-tree`. v1 is the substrate piece Phase 3's **branch** operation needs.

### 5.1 Where does `GraphTree<NodeKey>` live?

Two reasonable answers:

**Option A — kernel-side ownership.** `mere-kernel::graph::Graph` grows a paired `GraphTree<NodeKey>` field. Every node addition to the petgraph mirrors into the tree with appropriate provenance. The kernel exposes one unified API. Host code talks to `Graph`; the tree is a private implementation detail of the kernel.

**Option B — host-side ownership per workbench pane.** Each `PaneState::Workbench` carries its own `GraphTree<NodeKey>` over the shared `Entity<Graph>`. Multiple workbenches over the same graph can have different graph-tree views (different lenses, different graphlet memberships, different active members).

Picked: **Option B** (host-side per workbench).

Reasoning:

- `graph-tree`'s docstring says "One `GraphTree<N>` per **graph view**." A workbench is a view; the graph is the truth. Multiple views over one truth is the natural fit.
- `graph-tree` carries session state (active member, expanded set, scroll anchor, lens) that is per-view, not per-graph. Putting it on the graph would conflate viewpoint with truth.
- Branch creates a `GraphletRef` in *the workbench's graph-tree*, not in the underlying graph — multiple branches off the same graph (one per workbench window viewing it) coexist cleanly because each workbench's graph-tree is independent.
- Per the multiplexer framing: the graph is the session's truth; the workbench is an attach client. Trees-per-attachment fits that hierarchy.

What's shared across all graph-trees over the same graph: **the set of NodeKeys**. When the graph mutates (node added in window A), every window's graph-tree gets notified and adds the member to its own tree if appropriate.

### 5.2 `graph-tree` dependency

Add to `crates/mere-host/Cargo.toml`:

```toml
graph-tree = { path = "../graph/graph-tree" }
```

`mere-kernel` already depends on graph-tree, so this isn't introducing a new cross-workspace edge — it's making mere-host a direct consumer of an already-used crate.

### 5.3 `PaneState::Workbench` extension

```rust
pub struct WorkbenchPaneState {
    pub tiles: TileManager,
    pub tree: GraphTree<NodeKey>,
}
```

The tree is constructed empty when the workbench pane is created (lens defaults to `ProjectionLens::default()`, layout to `LayoutMode::default()`). As tiles open, members are added.

### 5.4 New-tile creation wires through to graph-tree

The `NewTile` path of `navigate_to` is extended:

1. Mint new anchor node (existing `ensure_node_for_address_near`).
2. **Add to graph-tree:** `tree.apply_nav(NavAction::Attach { member: new_node, provenance: Provenance::Traversal { source: source_anchor, edge_kind: Some("user-spawned-tile") } })`.
   - If `source_anchor` is `None` (first tile of the session), use `Provenance::Anchor`.
3. Open the tile (existing).

The tree's `apply_nav` returns a `NavResult` whose intents the host doesn't need to act on in v0 (rendering still happens via the orrery and tile strip directly); future cartography/orrery work consumes them.

### 5.5 Tile close / node removal

Closing a tile in v0 keeps the underlying node alive (we discussed this in Phase 2 Part 1 — closing the tile drops the visual binding, the node stays for the orrery). v1 keeps that: the graph-tree member's lifecycle transitions from `Active` → `Warm` (or `Cold` if the user explicitly removes the node from the graph), but the member entry remains. Provenance stays intact for lineage queries.

### 5.6 Diagnostics

New events alongside the v0 set:

```text
lineage.edge_added         { from: NodeKey, to: NodeKey, edge_kind: String }
lineage.member_lifecycle   { node: NodeKey, from: Lifecycle, to: Lifecycle }
```

These flow through the typed action bus's diagnostic hooks (per the [action-bus plan](2026-05-11_typed_action_bus_plan.md)) when those land; until then, direct `tracing::event!` calls.

## 6. Sub-decisions

### 6.1 What counts as "within-tile" in edge cases?

- **External link click that opens an `mere://`-protocol URL** in the active tile: within-tile.
- **External link click marked as `target="_blank"` (when nematic/engine surfaces support it):** new-tile. The intent is preserved through the engine→host channel.
- **Drag-and-drop a node from the orrery onto a workbench:** new-tile, with the dropped node as the anchor (already-existing node in the graph; tile opens against it).
- **Restored session:** each restored tile gets its history list back from short-term memory (per [memory-tiers brief](../research/2026-05-11_memory_tiers_brief.md)). If the short-term sidecar is missing, the tile starts with one history entry (the anchor's URL).

### 6.2 Document cache invalidation

Today the workbench's `documents: HashMap<NodeKey, EngineDocument>` keys docs by `NodeKey`. v0 changes the key to `String` (URL), because within-tile navigation now hits URLs that don't have a `NodeKey`. The cache size grows somewhat — but it's bounded by the tile's history size, which is itself bounded (see §10). Memory overhead in practice is tiny.

### 6.3 The "first tile" boot case

App start with no active tile (empty workbench): omnibar submit creates the first tile with that URL as the anchor. Same as today's behaviour — but v0 makes this explicit via the `NewTile` fallback in `WithinTile` mode.

### 6.4 Graphlet membership

v0: no graphlet membership changes. The default graph-tree has no graphlets (empty `Vec<GraphletRef<N>>`).

v1: still no graphlet creation for ordinary tile opens. Graphlets get created only when the user does a **branch** tear-out (Phase 3) or explicit "group these tiles" gesture.

This keeps v0/v1 small and lets Phase 3's branch operation be a clean delta on top.

### 6.5 What happens when the graph is mutated from a different window?

Mere is multi-window over one graph (see framing brief). If window A creates a tile (adds anchor X to graph), window B's graph-tree should observe the addition. v1 handling:

- The `cx.observe(&graph, ...)` subscription in the host already notifies on graph mutations.
- On notify, each window's per-workbench graph-tree reconciles: if a new node exists in the graph but not in the tree, add it as a member with `Provenance::Derived { connection: None, derivation: "cross-window-reconcile".into() }`.
- This is a coarse default; future work could pass the originating provenance across windows via a graph-side event channel.

## 7. File-size impact

Per Mere's 600-LOC ceiling:

- `tiles.rs` grows from 116 to ~250 LOC (history list, per-tile cache, back/forward primitives). Fits.
- `host_helpers.rs` shrinks slightly — `ensure_node_for_address_near` becomes more focused (only the new-tile path).
- `host_navigation.rs` grows by ~50 LOC for the `NavigateMode` branch. Currently at 535; staying under 600 will require extraction. **Action:** before this plan's v0, extract `host_navigation`'s tile/workbench methods (`focus_tile_in`, `close_tile_in`, `rebuild_app_tree`) into a new module `tile_ops.rs`. ~120 LOC. Brings `host_navigation` to ~400 LOC before this plan's additions.
- v1 adds ~150 LOC to `panes.rs` and `pane_state.rs` for graph-tree wiring; both stay under 600.

## 8. Sequencing

Suggested commit-shaped milestones, each leaves the codebase green:

### v0 milestones

1. **Extract `tile_ops.rs` from `host_navigation.rs`.** Pure refactor; behaviour unchanged. Brings `host_navigation` under 400 LOC. Lands first to give v0 room.
2. **Per-tile history on `TileManager`.** Internal data model change; v0 still calls `ensure_node_for_address_near` per nav. Existing behaviour preserved. Tests cover the new methods.
3. **`NavigateMode` parameter on `navigate_to`.** All call sites get explicit `WithinTile` for now. Still creates nodes per nav (no semantic change yet); the mode is plumbed but its behaviour-branching is one line of code still using the old path.
4. **Semantic shift:** `WithinTile` stops calling `ensure_node_for_address_near`. `NewTile` calls it. Now nodes are per-tile. Test against fixture omnibar sequences; assert kernel-graph size only grows on `NewTile`.
5. **`Ctrl/Cmd-Enter` gesture + `OpenInNewTile` palette action.** Omnibar wiring; default keybinding registration.
6. **Back/forward = within-tile.** Remove host-level `history` + `history_cursor`. Reload reloads the active history entry. Diagnostic `host.history_removed` fires once at startup.
7. **Diagnostics events** for `tile.opened`, `tile.navigated_within`, `node.created { reason }`.

After (7), v0 is done. Phase 3's **branch** operation could be implemented on top of v0 alone — it would just have to skip the graph-tree integration. v1 makes the graph-tree integration explicit so branch's `GraphletBinding::Forked` machinery has a real graph-tree to insert into.

### v1 milestones

8. **Add `graph-tree` dependency to `mere-host`.**
9. **`WorkbenchPaneState` carries `GraphTree<NodeKey>`.** Constructed empty.
10. **New-tile path wires `NavAction::Attach`** to the workbench's tree. Provenance::Traversal for non-first tiles; Provenance::Anchor for the first.
11. **Tile-close lifecycle reconcile.** Active → Warm or Cold.
12. **Cross-window reconcile.** On graph mutation, each workbench's tree adds missing members.
13. **Diagnostics** for `lineage.edge_added`, `lineage.member_lifecycle`.

After (13), Phase 3 can ship.

## 9. Risks

- **Tile-history-vs-graph-edge confusion in user mental model.** Once back/forward stops crossing tiles, users may expect it to. Mitigation: a one-time toast on first ambiguous case (`history.cross_tile_attempted { tile_count }` diagnostic fires, host shows "Back/forward navigate within the active tile. To revisit a different tile, click it in the tile strip."). Optional follow-up: a separate cross-tile-recents affordance.
- **Document cache memory growth.** Per-tile caches indexed by URL can grow if the user opens long-lived tiles with extensive within-tile navigation. Mitigation: cap per-tile history at 200 entries (configurable); evict oldest documents when over cap. Diagnostic `tile.history_capped { tile, evicted_count }`.
- **`HostRoot.history` removal touches multiple call sites.** Toolbar's `can_go_back` / `can_go_forward` derive from host history today. After removal, they derive from the *active tile's* history. Care needed during the migration to avoid stale toolbar state when switching tiles.
- **Graph-tree's `MemberId = NodeKey` assumption.** mere-kernel's `NodeKey` is petgraph's `NodeIndex`, which is **not stable across graph mutations** in the general case. If we rely on `NodeKey` as `MemberId`, edge cases (node removal compacts indices) can corrupt the tree. v1 plan assumes the existing `StableGraph` use in mere-kernel means indices *are* stable; verify on v1 day one.
- **Engine-driven navigation.** Documents loaded by `inker` engines may try to navigate (link clicks, redirects). The intent (within-tile vs. new-tile) needs to be propagated from the engine through the host to `navigate_to`. v0 assumes a default of within-tile for engine-driven navigation; v1 surfaces the engine's intent if available (e.g., `target="_blank"`).

## 10. Configurability

Per project preference (configurability over opinionated defaults):

- `tile.max_history_entries` (default 200) — per-tile history cap.
- `tile.evict_strategy` (default `OldestFirst`) — alternatives: `LeastRecentlyAccessed`.
- `omnibar.default_mode` (default `WithinTile`) — power users could flip to `NewTile` if they always want new tiles.
- `omnibar.new_tile_modifier` (default `Ctrl-Enter`) — rebindable through user keymap.

## 11. What this plan unblocks

- **Phase 3 branch operation** — `GraphletBinding::Forked` inserts into a live `GraphTree` (v1).
- **Cartography lineage overlays** — visualising graph-tree's Provenance::Traversal edges as a temporal-edge overlay in the orrery (future).
- **Per-tile back/forward UI** — small browser-style back/forward buttons in the tile strip (small follow-up).
- **Time-axis diff via eidetic** — consolidating a graph-tree state into an engram captures both the spatial graph and the lineage facet; diffs between engrams can show "what new lineage edges appeared between t1 and t2" (memory-tiers brief consolidation work).
- **"Promote this visit to anchor" gesture** — user has been within-tile-browsing through several URLs; clicks a button on the active history entry to mint it as a real anchor node. Becomes a Manual provenance entry in graph-tree.
