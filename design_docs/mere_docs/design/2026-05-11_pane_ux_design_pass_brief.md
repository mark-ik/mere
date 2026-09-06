# Pane UX design pass — drag-rearrange, frametree split, click hierarchy, context menus

**Date**: 2026-05-11
**Status**: Design brief — directional, not implementation-locked. Captures the UX target the foundation work (action bus, manifest store, persistence, leaf/branch/fork) is enabling, so the next slices land *shaped by* the target rather than retrofitted to it.

> **Crate-name note (2026-06-09 audit):** the implementation references below (`mere-host/src/context_menu.rs`, gpui) are a 2026-05-11 gpui-era snapshot; the host is now `meerkat` (genet-as-host). Much of this UX target has since shipped (frame-tree panes, shellbar, `reparent_leaf`); the live successor is the [graph roster + frame taxonomy](2026-06-07_graph_roster_and_frame_taxonomy.md). This brief is a merge-then-archive candidate.
**Scope**: Five UX gaps observed in real Mere use as of 2026-05-11:

1. Panes can't be rearranged within a window's frame (only splitter ratios adjust).
2. Tile drag stops at the window's bounds — no OS-level cross-window drop yet (separate blocker).
3. Workbench is one-active-tile-per-pane — no side-by-side multi-tile rendering inside a single workbench.
4. Click hierarchy on graph nodes / tile strip / pane chrome is underspecified — single-click is overloaded; double-click and right-click are unwired.
5. Shellbar toggle buttons are a stopgap for panel summon; the user wants the panel surface invokable through richer affordances (node double-click → tile open; right-click → context menu; etc.).

**Related**:

- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) — §5.2 (Windows + panes: "no drag-to-rearrange of panes within the window's frame yet"), §9 (accessibility action surfaces).
- [`../research/2026-05-11_tearout_operations_brief.md`](../research/2026-05-11_tearout_operations_brief.md) — §3.2 toast UX directly intersects this brief's right-click affordance work.
- [`../../archive_docs/2026-06-09_pivot_superseded/2026-05-11_typed_action_bus_plan.md`](../../archive_docs/2026-06-09_pivot_superseded/2026-05-11_typed_action_bus_plan.md) — every gesture in this brief routes through the bus.
- [`../../archive_docs/2026-06-09_pivot_superseded/2026-05-11_node_per_tile_lineage_plan.md`](../../archive_docs/2026-06-09_pivot_superseded/2026-05-11_node_per_tile_lineage_plan.md) — node-per-tile semantics. Double-click semantics in this brief assume node-per-tile is settled.

---

## 0. The thesis

> **Mere has three command surfaces. They are not competing — they fan out from the same action bus to suit the user's input mode.**

| Surface              | Input mode  | Discoverability | Density        |
| -------------------- | ----------- | --------------- | -------------- |
| **Palette**          | Keyboard    | Searchable      | All actions    |
| **Context menu**     | Mouse       | Right-click     | Target-scoped  |
| **Direct gesture**   | Mouse + drag | Spatial         | Common ops only |
| **Shellbar buttons** | Mouse-quick | Always visible  | High-traffic ops |

Today Mere has the palette + shellbar; the context menu and direct-gesture surfaces are underbuilt. This brief closes those gaps without removing the existing ones. **All four route through the action bus** — same target/kind types, same permission gate, same diagnostics.

## 1. Pane drag-to-rearrange

### 1.1 What's missing

`FrameLayout` supports `summon_leaf(path, side, new_leaf)` (insert beside an existing leaf), `close_leaf(path)` (remove + collapse), and `set_split_ratio(path, new_ratio)` (resize). What's missing is **move an existing leaf to a new position** without closing-then-re-summoning (which would lose the pane's state — orrery camera, workbench tiles, lineage tree).

### 1.2 What the user expects

- Grab the pane header (the row that already shows the pane's title + × close button).
- Drag it. A drag preview follows the cursor.
- As the cursor enters another pane, drop zones light up: top quadrant, right edge, bottom quadrant, left edge.
- Drop in one of those zones → the dragged pane re-attaches there, splitting the target pane along the chosen axis.
- Drop on a pane header → swap positions (Phase 2 of this; nicer-to-have).

### 1.3 Implementation sketch

**New `FrameLayout::reparent_leaf` primitive:**

```rust
impl FrameLayout {
    /// Move the leaf at `source` to a position adjacent to the
    /// leaf at `target`. Source's pane_id + content + graph_id are
    /// preserved; only its position in the tree changes. The
    /// source's split-parent collapses (sibling is promoted in
    /// place) just like `close_leaf` does. The target's leaf is
    /// re-wrapped in a new split with source + target as children.
    ///
    /// Returns `false` if either path doesn't resolve to a leaf,
    /// or if source == target.
    pub fn reparent_leaf(
        &mut self,
        source: &[SplitChoice],
        target: &[SplitChoice],
        side: InsertSide,
    ) -> bool { ... }
}
```

**Host-side gesture:**

- Pane header gains an `on_drag` carrying a `PaneDragPayload { pane_id }`.
- Pane body gains an `on_drop<PaneDragPayload>` that computes which drop zone the cursor is in (based on cursor position vs. pane bounds) and dispatches `BusAction::pane(target_pane_id, ActionKind::ReparentPane { source: src_pane_id, side })`.
- New action bus kind: `ReparentPane { source: PaneId, side: InsertSide }`.
- Execute path: resolves source/target paths via existing `path_for_pane`; calls `frame_layout.reparent_leaf(...)`; calls `rebuild_app_tree`.

**Drop-zone visualization:** while a `PaneDragPayload` drag is active, each pane shows a 4-zone overlay (subtle quadrant highlights). gpui's `drag_over<S>(...)` is the right hook for this — applies styling only while a matching drag is hovering.

### 1.4 Sub-decisions

- **Drop-zone hit areas**: top/bottom/left/right ~25% bands of the pane, or just close to the edges? Browser convention is edge bands. Lean edge bands.
- **Cross-window pane reparent**: out of scope (same OS-DnD blocker as cross-window tile drag).
- **Reparenting an orrery whose graph isn't the donor pane's graph**: allowed (we already support multi-graph windows). The reparented orrery keeps its `graph_id`.

## 2. Cross-window tile drag

Recap: tile drag *within* the window works (Phase 2 Part 1 ships the gesture; drop-anywhere fires a leaf tear-out). Drag *outside* the window to spawn a new OS window requires platform-level drag-and-drop support that gpui doesn't expose today.

**This brief doesn't try to fix it** — flagged on the Phase 2 Part 2 follow-up list. The likely path:

1. Add OS-level DnD support to gpui (upstream contribution, unrelated to Mere's roadmap).
2. Mere's drop handler sees a "drop outside any window" event and dispatches the same leaf/branch/fork tear-out based on modifier keys held.

Until then, the in-window leaf tear-out works (verified in the screenshot — new sticky-note window opens, even though it's contained in the original OS window's bounds).

Alternative until OS DnD lands: **a "tear out" item in the tile's right-click context menu** (see §4). Mouse-driven but doesn't require drag-out-of-window. Pairs with keyboard's `Ctrl/Cmd-Shift-T`-style shortcuts.

## 3. Frametree side-by-side tile rendering

### 3.1 The question

Today the workbench is **one-active-tile-per-pane** — `TileManager.active: Option<usize>` selects exactly one tile to render in the body. The user wants **multiple tiles renderable simultaneously inside a single workbench's region**.

Use cases:
- Compare two pages side-by-side.
- Reading source + reading the document about it in parallel.
- Editing one tile while the other shows reference material.

### 3.2 Three options

**Option A — Active set on `TileManager`.**

```rust
pub struct TileManager {
    open: Vec<NodeKey>,
    active: TileActiveSelection,   // was Option<usize>
    state: HashMap<NodeKey, TileState>,
}

pub enum TileActiveSelection {
    Single(usize),
    Multiple { indices: Vec<usize>, layout: TileSplitLayout },
}

pub enum TileSplitLayout {
    Horizontal { ratios: Vec<f32> },
    Vertical { ratios: Vec<f32> },
    Grid { rows: usize, cols: usize },
}
```

Pros: simple data model. Existing workbench renderer extends to render N tiles in a split.
Cons: layout state grows inside `TileManager`, which is otherwise pure tile bookkeeping. Couples tile rendering to layout decisions.

**Option B — `PaneContent::Tile(NodeKey)` leaves in `FrameLayout`.**

```rust
pub enum PaneContent {
    Workbench,
    Orrery,
    Gloss,
    Apparatus,
    System,
    Tile(NodeKey),   // NEW: pinned-tile leaf
    Custom(String),
}
```

A `Tile(NodeKey)` leaf renders that specific tile **without a strip** — just the document body. Users arrange multiple of these side-by-side via the same drag-rearrange gesture (§1). A workbench still exists for the strip + multi-tile navigation; pinned-tile leaves are the "I want this visible always" affordance.

Pros: leverages existing `FrameLayout` machinery (splits, ratios, drag-rearrange). No new data model. Multi-graph already supported. Tiles get the same close-pane + reparent affordances as orreries do.
Cons: pinned tiles don't appear in the workbench strip — they're separate things. UX needs clarity about the mental model: workbench = "current reading," pinned-tile leaves = "always-visible references."

**Option C — Workbench gains internal split.**

The workbench's render becomes a recursive splitter, same shape as `FrameLayout` but contained. Workbench owns its own `TileFrameLayout` tracking which tile is in which quadrant of its body.

Pros: keeps frame-level layout simple (workbench stays one leaf). Workbench can present a "tile arrangement" UI distinct from the window's frame.
Cons: two parallel splitter implementations. Doubles the surface area. Drag-rearrange has to support both levels.

### 3.3 Recommended pick — Option B

**Pinned-tile leaves via `PaneContent::Tile(NodeKey)`.** Reasoning:

- Reuses every primitive already built — splits, ratios, drag-rearrange (§1), close-pane, multi-graph, persistence.
- Maintains the **"workbench = navigation, pinned-tile = reference"** distinction Mark's framing implies (workbench has tile strip + back/forward + omnibar focus; pinned tile is "I commit to keeping this visible").
- The gesture is obvious: drag a tile from the workbench strip out *to a drop zone in the same window* (not just to spawn a new OS window) — that creates a `PaneContent::Tile(node)` leaf adjacent to whatever pane the user dropped on.
- This is precisely the third tear-out mode the multiplexer framing brief hinted at (§5.10: "drop onto an existing window's edge = attach as new pane there"). Brings cross-pane drop to v1 instead of waiting for cross-OS-window DnD.

**New bus action:** `ActionKind::PinTileToFrame { node: NodeKey, target: PaneId, side: InsertSide }` — bus-routable, palette-discoverable, drag-droppable.

**Storage:** `PaneContent::Tile(NodeKey)` serialises with `#[serde(default)]`; pre-existing layouts deserialize unchanged.

### 3.4 Sub-decisions

- **Strip behaviour for pinned tiles:** the workbench's strip lists open tiles in *the workbench*, not pinned tiles. Closing the pinned-tile leaf doesn't affect the workbench's strip.
- **Document cache for pinned tiles:** lives in the originating workbench's `TileState`. The pinned-tile leaf reads the cache by URL. If the workbench closes, the pinned-tile loads from disk via `loader::load`.
- **Cross-window pinned-tile sharing**: a pinned tile in window A pointed at a node in window B's session — works because session is registry-shared. Same propagation semantics as a leaf tear-out.

## 4. Click hierarchy + context menus

### 4.1 Per-surface click table

The unifying rule: **left-click = direct manipulation, double-click = strong intent, right-click = context menu, drag = move/rearrange.**

| Surface                       | Left-click       | Double-click       | Right-click             | Drag                              |
| ----------------------------- | ---------------- | ------------------ | ----------------------- | --------------------------------- |
| **Graph node (orrery)**       | Select + focus   | Open new tile (NewTile mode) | Node context menu       | Move node in 2D                   |
| **Graph edge (orrery)**       | Select edge      | (reserved)         | Edge context menu       | (reserved — bend handle later)    |
| **Empty orrery space**        | Deselect         | Pan reset?         | Orrery context menu     | Pan                               |
| **Workbench strip tile**      | Focus tile       | (reserved — full-screen?) | Tile context menu | Tear out — see Phase 2 Part 2     |
| **Workbench strip empty**     | (no-op)          | (no-op)            | Workbench context menu  | (no-op)                           |
| **Pane header**               | (no-op)          | Toggle maximize?   | Pane context menu       | Reparent pane (§1)                |
| **Empty pane region**         | Focus pane       | (no-op)            | Pane context menu       | (no-op)                           |
| **Shellbar button**           | Action           | (no-op)            | (reserved)              | (no-op)                           |
| **Switcher row**              | Attach session   | Attach in new window | Session context menu  | Reorder switcher?                 |
| **Graph switcher backdrop**   | Close switcher   | (no-op)            | (no-op)                 | (no-op)                           |

Today's behaviour for **graph node**: single-click already opens a tile (`orrery_input.rs` SelectNode handler). The shift proposed here: **single-click selects + focuses; double-click opens.** Matches file-manager / IDE conventions. Tradeoff: extra click for the common case. Alternative: keep single-click open, double-click opens in **new tile** (NewTile mode). The double-click-as-stronger-commit reading.

**Decision recommended:** **single-click stays open** (preserves current UX); **double-click opens as new tile** (NewTile mode — leans on the Ctrl/Cmd-Enter pattern, except for mouse-driven users). Right-click gets the full menu.

### 4.2 Context menu shape

A context menu is a popover with a list of entries; each entry maps to a `BusAction`. Reusing the palette's structure:

```rust
pub struct ContextMenu {
    pub entries: Vec<ContextMenuEntry>,
}

pub struct ContextMenuEntry {
    pub label: String,
    pub action: BusAction,         // dispatched on click
    pub shortcut: Option<String>,  // display-only, e.g. "Ctrl+Shift+T"
    pub disabled: Option<&'static str>, // greyed-out + tooltip explaining why
    pub separator_after: bool,
}
```

**Wire-up rule:** right-click on a surface → look up that surface's `ContextMenu` builder (host-side function that knows the surface's target context) → pop the menu at cursor → on-click dispatches through the bus → bus checks the gate + emits diagnostic.

### 4.3 Per-surface menus (v0 sketch)

**Node** (target: `Node(graph_id, node_key)`):
- Open in tile (`NavigateTo { new_tile: true }`)
- Open in pinned-tile pane (§3 `PinTileToFrame`)
- Open as branch (Phase 3 branch)
- Open as fork (Phase 3 fork)
- ---
- Pin node (toggle pin state — reserved)
- Rename (reserved — opens an inline editor)
- ---
- Delete node (with confirmation)

**Tile strip tile** (target: `Pane(workbench_pane_id)`):
- Focus (`FocusTile { index }`)
- Close (`CloseTile { index }`)
- ---
- Tear out as leaf (`TearOutTile { mode: Leaf }`)
- Tear out as branch (`TearOutTile { mode: Branch }`)
- Tear out as fork (`TearOutTile { mode: Fork }`)
- ---
- Pin to frame (§3 `PinTileToFrame`)

**Pane header** (target: `Pane(pane_id)`):
- Close pane (`ClosePane`)
- Reparent (currently drag-only; this menu surfaces "move to left/right/top/bottom" for non-drag users)
- ---
- Toggle workbench / gloss / apparatus (whatever the pane isn't)

**Session switcher row** (target: `Session(session_id)`):
- Attach here (`SummonOrreryForGraph(graph_id)`)
- Attach in new window
- Rename session (reserved — sets `manifest.display_name`)
- ---
- Kill session (with confirmation; `KillSession`)
- Show on disk (reveal `<sessions>/<uuid>/` in OS file browser)

### 4.4 Implementation sketch

- New module `mere-host/src/context_menu.rs` — defines `ContextMenu`, `ContextMenuEntry`; renders a popover via gpui's `Stateful<Div>` + `absolute()` positioning.
- Per-surface builders live alongside their renderers: `panes::context_menu_for_pane(pane_id, this)`, `orrery_input::context_menu_for_node(node_key, graph_id, this)`, etc.
- gpui's `on_mouse_down(MouseButton::Right, ...)` gates the menu pop.
- Menu actions dispatch through the existing `host_action_bus::dispatch`.

## 5. How the four surfaces compose

The thesis (§0) claims palette + context menu + direct gesture + shellbar are non-competing surfaces over one action bus. Concretely:

- **Palette** stays as-is — fuzzy-searchable, lists every registered action.
- **Context menus** present a *filtered, target-aware* subset of the same action space.
- **Direct gestures** (click, double-click, drag) bind to the most-used context-menu entries.
- **Shellbar buttons** stay as fast-access affordances for "summon panel" / "switch graph" / "+orrery" — actions that don't have a clear right-click target.

**Implication for the action bus migration**: the existing `host_action_bus::dispatch_kind` is the *only* dispatch path. Context-menu entries, drag-drop handlers, and shellbar buttons all route here. The palette's `PaletteInvoke` subscription (still TODO — palette dispatch is on the migration list) routes here too. After the bus migration completes, *every action in the application flows through one function*, with the four surfaces being four different ways of choosing which `BusAction` to fire.

## 6. Sequencing

Suggested order, smallest-to-biggest. Each leaves the codebase green.

1. **Click hierarchy v0** — settle the single-click vs. double-click question on nodes (§4.1 row 1). Tiny code change in `orrery_input.rs`. No new types. Resolves the ambiguity in the most-used gesture.
2. **Right-click context menu component** — `mere-host/src/context_menu.rs`. Renders, but no surfaces wire it yet. Standalone testable.
3. **Per-surface menu builders** — node, tile, pane header, switcher row. Each builds + dispatches.
4. **Pane drag-rearrange** — `FrameLayout::reparent_leaf` + `PaneDragPayload` + `ReparentPane` bus kind. Mid-sized.
5. **Pinned-tile leaves (`PaneContent::Tile`)** — biggest single piece. New leaf kind, new renderer surface, new drop semantics, new bus action. Approach as its own implementation plan brief.

Cross-window tile drag stays parked on the Phase 2 Part 2 follow-ups list — blocked on gpui platform DnD.

## 7. What this brief decides / doesn't decide

**Decides:**

- Four command surfaces (palette / context menu / direct gesture / shellbar), one action bus.
- Click hierarchy: left=manipulate, double=strong-intent, right=context-menu, drag=move/rearrange.
- Single-click on node stays "open tile"; double-click opens **in a new tile** (NewTile mode).
- Side-by-side tile rendering = `PaneContent::Tile(NodeKey)` leaves (Option B over A or C).
- Pane drag-rearrange uses a new `FrameLayout::reparent_leaf` primitive routed through the bus.

**Defers:**

- Context-menu styling, popover positioning details, keyboard navigation of menus.
- Whether double-click on a tile strip entry does something distinct (full-screen? expand?). Reserved.
- Edge context-menu actions (no clear v1 use case).
- "Pin node" / "Rename node" / "Show on disk" specific implementations — listed in §4.3 menus as reserved entries.
- Cross-window pane drag — same blocker as cross-window tile drag.

**Doesn't preclude:**

- Future "broadcast to all panes" gestures (right-click empty space → "Broadcast navigate…").
- Keyboard shortcut hints on context-menu entries (already in the type).
- Touch / pen / gamepad gestures — those are their own design pass; the action bus is the common substrate.
