/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # meerkat
//!
//! Mere's serval-as-host shell — the chrome (toolbar, omnibar, command palette,
//! frametree) built as [`xilem_serval`] views over the **reused** `graphshell`
//! chrome domain, presented by serval. This is flip Phase 3 (chrome-as-DOM): the
//! eventual replacement for the Xilem + Masonry `mere-app` host.
//!
//! ## Reuse, not rewrite
//!
//! The chrome *model* already exists and is tested: `chrome::toolbar`,
//! `chrome::omnibar`, `chrome::command_palette`, `chrome::frame_model`,
//! `chrome::routing` are host-neutral, WASM-clean view-models built to the
//! contract *"host widgets render from these types through the view-model; the
//! shell owns the mutations."* meerkat is the next such host widget (after the
//! egui and iced ones), so it renders those types as serval DOM and routes
//! mutations back through the runner. Only the *rendering* is new.
//!
//! ## Shell document vs content roots
//!
//! The **shell document** (this crate's view tree: chrome plus the folded panes
//! and orrery node-cards, diffed by `xilem_serval` from app state) and each
//! **content-root** (a fetched page mutated by its engine, or the orrery graph
//! scene) stay distinct document authorities; neither sees the other's tree (flip
//! plan, Phase 3 + Standing constraints). Unified-document-host Phase 1 folded the
//! panes into the one shell document; the content surfaces stay separate roots.
//!
//! ## Status
//!
//! The chrome renders from the reused [`chrome`] view-models into a serval
//! `ScriptedDom` via [`ServalAppRunner`]: toolbar, an editable omnibar
//! ([`TextInput`] lensed into the view), command palette, find bar, settings,
//! comms pane, shellbar, and context menu are all `xilem_serval` views. The
//! omnibar drives a real linear [`History`](nav::History) (text classified and
//! resolved to a URL, `can_go_*` mirrored back into the toolbar), and
//! [`Chrome::content_location`] is the entry a content-root loads. The bin
//! (`main.rs`) folds the roster, the four list panes, and the orrery's node-cards
//! into that same document under one runner: unified-document-host Phase 1 is
//! complete (one document, one focus ring, one a11y tree).

use chrome::command_palette::{CommandPaletteSession, SearchPaletteScope};
use chrome::omnibar::OmnibarMatch;
use chrome::toolbar::ToolbarState;
use comms::{CommsPane, ConversationId, Draft, ProtocolKind};
use forme::GraphMemberId;
use frame::SessionId;
pub use session_runtime::ShellbarEdge;
use xilem_serval::TextInput;

pub mod command;
pub mod crawl_indicator;
pub mod ingest;
pub mod nav;
pub mod shell_eval;
pub mod suggest;
pub mod sync_indicator;

use command::{Command, PaletteItem};
pub use crawl_indicator::CrawlIndicator;
use nav::History;
pub use sync_indicator::SyncIndicator;

/// Meerkat's chrome app state.
///
/// Holds the reused, host-neutral [`ToolbarState`] view-model plus the live
/// omnibar editor. meerkat renders these as DOM and owns the mutations — the
/// host-widget half of the M4 contract. Later slices fold in the omnibar session,
/// command palette, and frame model (all already-built `chrome` types).
pub struct Chrome {
    /// The toolbar's session state — location bar, load status, nav-capability
    /// flags. Reused verbatim from `chrome::toolbar`; the canonical sink the
    /// omnibar submits into.
    pub toolbar: ToolbarState,
    /// The omnibar's live editing buffer (caret / selection / IME), edited by the
    /// `text_field`. `xilem_serval` text editing rides a `TextInput`, while the
    /// reused `ToolbarState.editable.location` is a `String`; the host syncs the
    /// buffer into the session state on submit (Enter), keeping the domain
    /// unchanged.
    pub omnibar: TextInput,
    /// meerkat's own linear back/forward history. graphshell projects the
    /// toolbar's `can_go_*` flags from a servo viewer's history; meerkat has no
    /// such viewer, so it owns the stack and mirrors the flags into
    /// [`ToolbarState`]. Its current entry is the URL the content root shows.
    pub history: History,
    /// Live omnibar suggestions (history + search), in the reused
    /// [`OmnibarMatch`] vocabulary. Empty ⇒ the dropdown is closed. Regenerated
    /// from the omnibar text on each edit via [`Chrome::refresh_suggestions`].
    pub suggest: Vec<OmnibarMatch>,
    /// Highlighted suggestion row, if any. `None` ⇒ Enter navigates the typed
    /// text; `Some(i)` ⇒ Enter (or a click) navigates `suggest[i]`.
    pub suggest_active: Option<usize>,
    /// Whether the command palette overlay is open.
    pub palette_open: bool,
    /// The reused command-palette session — its `query`, `selected_index`, and
    /// `step_selection` cursor logic drive meerkat's palette over a
    /// meerkat-owned command set ([`command`]).
    pub palette: CommandPaletteSession,
    /// The palette's live query buffer (caret / editing), mirrored into
    /// `palette.query` — the same host-owns-the-buffer split the omnibar uses.
    pub palette_input: TextInput,
    /// Whether the find-in-page bar is open (Ctrl+F; HTML/serval lane).
    pub find_open: bool,
    /// The find query buffer (caret / editing); the host pushes it to the content
    /// actor via `Constellation::request_find` on each edit.
    pub find_input: TextInput,
    /// The active match index, cycled by next/prev. Clamped against the live match
    /// count (held host-side in the constellation) when rendered.
    pub find_active: usize,
    /// The live match count for the focused node, synced host-side from the
    /// constellation each frame the bar is open so the bar can show "active/total".
    pub find_count: usize,
    /// The p2p sync-status chip's view-model (S5.0). The host folds the joined
    /// lane's real `SyncStatus` in here; default reads "p2p off".
    pub sync: SyncIndicator,
    /// The crawl-progress chip's view-model (relational-browse V2). The host folds the
    /// crawl actor's progress in each frame it drains; empty (hidden) when none ran.
    pub crawl: CrawlIndicator,
    /// A pending "connect to peer" request the host must execute (S5.1): the
    /// ticket string captured from the address bar when the verb ran. The chrome
    /// records the intent; the host drains it, drives the sync actor, and clears
    /// it. (The chrome cannot reach the sync actor itself.)
    pub pending_connect: Option<String>,
    /// A pending host action the shell must run — the payload-free host verbs
    /// (toggle workbench / delete node / background a node). Like `pending_connect`
    /// but for actions the chrome can't reach (the orrery, the actor pool): the
    /// chrome records the intent, the host drains it and runs the matching method.
    pub pending_command: Option<Command>,
    /// User settings the chrome renders + edits (the settings overlay). The host
    /// applies them (e.g. the tab cap to the actor pool) and persists them.
    pub settings: Settings,
    /// The open right-click context menu (host-populated from the selection), or
    /// `None` when no menu is showing.
    pub context_menu: Option<ContextMenu>,
    /// The tear-out drag ghost's label (the dragged node's title) while a tear-out drag
    /// is in flight, else `None`. The host renders a small pill carrying it and positions
    /// it at the live cursor each frame (so it follows without a chrome re-render per
    /// move). (Tear-out gestures, GA-5.)
    pub tear_ghost: Option<String>,
    /// If this window is a tear-out **branch**, the anchor node's label, shown as a chip
    /// in the chrome so the branch reads as a distinct grouping (not a plain leaf). Set
    /// when the branch window spawns; `None` everywhere else. (Graphlet wiring Phase 2;
    /// tear-out gestures G3.)
    pub branch_label: Option<String>,
    /// A context-menu action a row captured, for the host to run over the menu's
    /// member set.
    pub pending_context: Option<ContextAction>,
    /// A context action invoked from the **command palette** (not the menu), for the host
    /// to apply to the *current selection*: the host seeds `context_set` from the live
    /// selection, moves this into `pending_context`, and drains it. Separate from
    /// `pending_context` because the menu path already seeds `context_set` itself, whereas
    /// the palette path has no menu working set. (Command registry P2.)
    pub pending_palette_action: Option<ContextAction>,
    /// The URL the content root currently shows. The host drives it: an omnibar
    /// submit sets the typed target (then `sync_orrery` navigates the focused node
    /// to it); a back/forward step sets the revealed page. Decoupled from
    /// [`History`] (now only the omnibar suggestions log) so per-node back/forward
    /// never re-navigates through here.
    pub content_location: String,
    /// A one-shot back/forward step the host applies to the **focused node's** own
    /// history (not a chrome-global linear history). The buttons / palette record
    /// it; the host drains it via `orrery.member_history_*` and clears it.
    pub history_step: Option<HistoryStep>,
    /// A one-shot "toggle the layout physics" intent — set by the toolbar's
    /// pause/play button; the host drains it into `orrery.toggle_physics_paused`.
    pub physics_toggle: bool,
    /// Whether the layout physics is paused, synced from the orrery each frame so the
    /// pause/play button shows the right glyph (▶ when paused, ⏸ when running).
    pub physics_paused: bool,
    /// A one-shot "open the submitted address as a **new node**" intent — set by
    /// Ctrl/Cmd-Enter in the omnibar (the `OpenAddressAsNewNode` gesture). When
    /// set, the host's `sync_orrery` mints a new browsing surface linked from the
    /// focused node (navigated-from edge) instead of navigating in place, then
    /// clears it. P2 of the node-navigation-lineage plan.
    pub open_as_new_node: bool,
    /// The docked comms pane's view-model (P6): conversations + the open thread +
    /// the draft + dock geometry. meerkat renders it the way it renders the other
    /// reused view-models. Seeded with placeholder data; the live misfin / murm
    /// adapters fill it through the event loop in a later slice.
    pub comms: CommsPane,
    /// The comms compose field's live editing buffer (caret / IME), the same
    /// host-owns-the-buffer split as the omnibar. Synced into `comms.draft` on
    /// send.
    pub comms_draft: TextInput,
    /// A pending comms request the host must run against the live `Comms` (the
    /// chrome can't reach the comms actor). Recorded here; the host drains it and
    /// issues the matching `CommsCommand`. Mirrors `pending_command`.
    pub comms_intent: Option<CommsIntent>,
    /// The compose-new form's recipient editing buffer (misfin `mailbox@host`).
    pub comms_new_to: TextInput,
    /// The compose-new form's body editing buffer.
    pub comms_new_body: TextInput,
    /// Which panes are currently open — mirrored from Shell's frame_layout each
    /// frame so the shellbar buttons show the correct active state.
    pub shellbar_panes: ShellbarPaneStates,
    /// Which window edge the shellbar is docked to — mirrored from Shell so the
    /// view builds the right flex direction.
    pub shellbar_edge: ShellbarEdge,
    /// Whether the shellbar is hidden by the user's toggle — mirrored from Shell so the
    /// chrome view omits the strip. Distinct from `slim` (a leaf's chrome): this hides
    /// the shellbar on a full-chrome window. (Hide-shellbar.)
    pub shellbar_hidden: bool,
    /// Slim chrome: a leaf (torn-out) window omits the shellbar (and the host omits
    /// the switcher), leaving just the toolbar over workbench-only content. Set once
    /// from the window's `WindowKind`. (Multi-window MW3 step 4.)
    pub slim: bool,
    /// The knot editor's live source buffer (caret / selection / IME), edited by a
    /// `text_field` like the omnibar — the single source of truth the editor pipe
    /// highlights and the engine renders, same host-owns-the-buffer split.
    pub knot_source: TextInput,
    /// Whether the docked knot-editor panel is open.
    pub knot_editor_open: bool,
    /// The graph member this editor is currently writing, if it is bound to a note tile.
    pub knot_target: Option<GraphMemberId>,
    /// Header label for the editor, usually the bound `knot://` URL.
    pub knot_editor_label: String,
    /// Window-space content rect of the bound tile, when the editor can sit over it.
    pub knot_editor_rect: Option<[f32; 4]>,
    /// One-shot save request captured by the editor's chrome button.
    pub knot_save_requested: bool,
    /// The open graph sessions, as toolbar chips (Chrome bar P4 — sessions moved out
    /// of the shellbar). Host-synced each frame from the session pool, ordered like
    /// `cycle_session`; the active one carries `active`. Rendered inline up to a cap,
    /// the rest folding into the overflow dropdown.
    pub sessions: Vec<SessionChip>,
    /// Whether the session overflow dropdown (`+N ⌄`) is open. Toggled by its button;
    /// closed when a session is picked. (Chrome bar P4.)
    pub sessions_overflow_open: bool,
    /// A one-shot session gesture a chip captured, for the host to drain into the
    /// matching `ShellCommand` (the chrome can't reach the session pool). (Chrome bar P4.)
    pub session_intent: Option<SessionIntent>,
}

/// Which panes are currently open in the frame tree. The host syncs this into
/// Chrome before each render pass so the shellbar buttons reflect live state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShellbarPaneStates {
    pub workbench: bool,
    pub roster: bool,
    pub gloss: bool,
    pub trail: bool,
    pub alembic: bool,
    pub apparatus: bool,
    pub inspector: bool,
    pub steward: bool,
    pub comms: bool,
}

/// One open graph session as a toolbar chip: its id (carried in the gesture the host
/// drains), a short label, and whether it is the focused session (the active
/// highlight). (Chrome bar P4 — sessions in the toolbar.)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChip {
    pub id: SessionId,
    pub label: String,
    pub active: bool,
}

/// A session gesture a toolbar chip captured, drained by the host into the matching
/// `ShellCommand`. `Activate` becomes a plain switch, or an open-beside when Shift is
/// held at drain time (preserving the old switcher's Shift+click). (Chrome bar P4.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionIntent {
    Activate(SessionId),
    Close(SessionId),
    Create,
}

/// A comms action the host runs against the live `Comms` on the chrome's behalf:
/// reload the list, open a conversation's thread, or send a draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommsIntent {
    /// Reload the merged conversation list.
    Refresh,
    /// Load the messages for a conversation.
    Open(ConversationId),
    /// Send a draft.
    Send(Draft),
    /// Connect the cabal from a received join ticket.
    ConnectCabal(String),
}

/// A within-node history step the host applies to the focused node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryStep {
    Back,
    Forward,
}

/// User-tunable settings the chrome surfaces. Small for now (the active-tab cap);
/// dark/light, edge-family visibility, and the rest join as their controls land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The most warm tabs the actor pool keeps before LRU eviction.
    pub tab_cap: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self { tab_cap: 12 }
    }
}

/// A right-click context menu the chrome floats at the cursor. The host computes
/// the rows from the current selection (open in splits / group into one stack) and
/// remembers which members they act on; the chrome renders the menu and routes a
/// row click back as a [`ContextAction`].
#[derive(Clone, Debug, PartialEq)]
pub struct ContextMenu {
    pub x: f32,
    pub y: f32,
    pub items: Vec<ContextItem>,
    /// The keyboard-highlighted row (arrow-key navigation), or `None` until the user steps
    /// into the menu. `Enter` runs it; the render pass scrolls it into view. Mirrors the
    /// command palette's `selected_index`. (Context-menu keyboard nav.)
    pub selected: Option<usize>,
    /// The search query typed into the menu (the cursor palette). Empty -> the curated rows
    /// (pins / suggestions); non-empty -> the registry filtered by it. Edited by
    /// `on_context_menu_key`, which rebuilds `items` from it. (Searchable context menu S1.)
    pub query: String,
    /// The open submenu (depth-1), if a parent row has been expanded — the child panel rendered
    /// beside the parent. While `Some`, keyboard nav targets the child panel and `Escape` /
    /// `ArrowLeft` collapses it back to the root. (Nested submenus.)
    pub submenu: Option<SubmenuState>,
}

/// The open child panel of a [`ContextMenu`]: which parent row it hangs off, and its own
/// keyboard highlight. The child rows are `items[parent].children`. Depth-1 only — the
/// submenu substitutes (relation kinds, layout strategies, shellbar edges) are all one level.
#[derive(Clone, Debug, PartialEq)]
pub struct SubmenuState {
    /// Index into the root menu's `items` of the expanded parent row.
    pub parent: usize,
    /// The keyboard-highlighted child row, or `None` until the user steps into it.
    pub selected: Option<usize>,
}

/// Wrap a keyboard highlight by `delta` within `count` rows: `None` -> first on a step down, last
/// on a step up; otherwise modular. `count` must be > 0. (Context-menu / submenu nav.)
fn step_wrapped(cur: Option<usize>, delta: isize, count: usize) -> usize {
    match cur {
        None if delta < 0 => count - 1,
        None => 0,
        Some(c) => {
            let n = count as isize;
            (((c as isize + delta) % n + n) % n) as usize
        }
    }
}

/// The pin affordance on a context-menu row (the cursor palette's search results): the registry
/// `id` to pin / unpin and whether it is currently in the curated menu. `None` on curated rows
/// (they are already the menu); `Some` on searched rows, where it renders a pin toggle.
/// (Searchable context menu S2.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinSpec {
    pub id: &'static str,
    pub pinned: bool,
}

/// One row of a [`ContextMenu`]: its label + the action it runs, plus an optional pin toggle.
/// A row with non-empty `children` is a **submenu parent**: clicking (or `ArrowRight`-ing) it
/// expands the children beside it rather than running `action`. (Nested submenus.)
#[derive(Clone, Debug, PartialEq)]
pub struct ContextItem {
    pub label: String,
    pub action: ContextAction,
    /// The pin toggle for a search result, or `None` for a plain (curated) row.
    pub pin: Option<PinSpec>,
    /// The child rows shown when this row is expanded as a submenu; empty for a leaf row.
    /// (Nested submenus — depth-1: the children are themselves leaves.)
    pub children: Vec<ContextItem>,
}

impl ContextItem {
    /// A row pairing `label` with the `action` it captures when clicked.
    pub fn new(label: impl Into<String>, action: ContextAction) -> Self {
        Self {
            label: label.into(),
            action,
            pin: None,
            children: Vec::new(),
        }
    }

    /// A search-result row: `label` runs `action`, and a pin toggle pins / unpins `pin_id` (already
    /// in the curated menu when `pinned`). (Searchable context menu S2.)
    pub fn searchable(
        label: impl Into<String>,
        action: ContextAction,
        pin_id: &'static str,
        pinned: bool,
    ) -> Self {
        Self {
            label: label.into(),
            action,
            pin: Some(PinSpec { id: pin_id, pinned }),
            children: Vec::new(),
        }
    }

    /// A submenu-parent row: `label` expands `children` beside it instead of running an action.
    /// Carries the [`ContextAction::OpenSubmenu`] sentinel so a stray dispatch is a harmless no-op;
    /// the real expansion is driven by `children` being non-empty. (Nested submenus.)
    pub fn with_children(label: impl Into<String>, children: Vec<ContextItem>) -> Self {
        Self {
            label: label.into(),
            action: ContextAction::OpenSubmenu,
            pin: None,
            children,
        }
    }

    /// Whether this row expands a submenu (has child rows) rather than running an action.
    pub fn has_submenu(&self) -> bool {
        !self.children.is_empty()
    }
}

/// What a context-menu row asks the host to do. Orrery node menus use
/// `OpenSplits`/`Stack` (against the `context_set`); the shellbar right-click
/// menu uses `ShellbarMove` (a global preference change, no member set needed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextAction {
    OpenSplits,
    Stack,
    /// Redock the shellbar to `edge`. Drains without touching `context_set`.
    ShellbarMove(ShellbarEdge),
    /// Hide the shellbar (its right-click menu offers this; revealed again from the
    /// palette / `>shellbar`). Drains like `ShellbarMove`, no member set. (Hide-shellbar.)
    ShellbarToggleVisibility,
    /// Relate the two selected nodes (a user-grouped relation). Offered only for a
    /// two-node selection; drains like `ShellbarMove` without opening tiles.
    Relate,
    /// Relate the two selected nodes as a specific semantic kind — the relation-kind
    /// picker offered for a two-node selection. Like [`Relate`](Self::Relate) but carries
    /// the chosen kind instead of defaulting to `UserGrouped`; drains the same way (no
    /// tiles, no member-set mutation). The kind is `Copy`, so `ContextAction` stays `Copy`.
    /// (Audit A3 — relation-kind picker.)
    RelateAs(kernel::graph::SemanticSubKind),
    /// Sentinel action for a submenu-parent row (`ContextItem::with_children`). A parent row
    /// expands its children rather than running an action, so this is never meant to drain; a
    /// stray dispatch is a harmless no-op. (Nested submenus.)
    OpenSubmenu,
    /// Mint a fresh node at the saved cursor point (the no-selection right-click).
    /// The anchor in `context_origin` is leaf-local screen px; the camera inversion
    /// to world happens inside `Orrery::add_node_at`. Drains like `ShellbarMove` /
    /// `Relate` without touching `context_set`. From the add-pill (no cursor anchor)
    /// it mints at the default position.
    AddNode,
    /// Mint a fresh node and open it as a workbench tile (the add-pill's "Add tile").
    AddTile,
    /// Mint a fresh graph session (the add-pill's "Add session") — a cross-window op
    /// the host queues as a `ShellCommand`.
    AddSession,
    /// Place a fresh field region at the saved cursor point (the no-selection
    /// right-click's "Add field" / the add-pill's "Add field"). The anchor in
    /// `context_origin` is leaf-local screen px; the camera inversion to world
    /// happens inside `Orrery::add_field_at`. From the add-pill (no anchor) it
    /// places at the orrery view center. (Field regions P0.)
    AddField,
    /// Delete the field under the right-click (retire it; the kernel keeps its definition).
    /// The target field is stored in `context_field`; drains without touching `context_set`.
    /// (Field regions — delete.)
    DeleteField,
    /// Close the focused graph (Orrery) pane — offered only when more than one
    /// graph pane is open. Drains like `ShellbarMove` without touching
    /// `context_set`. (Window composition — pane-as-unit.)
    CloseGraphPane,
    /// Pin the context node(s) to a specific engine — the per-node engine picker
    /// ("Open in <engine>"). The id is a stable engine constant (`&'static str`, so
    /// `ContextAction` stays `Copy`). Routing then prefers this engine for the node.
    /// (engine-picker Phase 3.)
    PinEngine(&'static str),
    /// Clear the context node(s)' engine pin — "Auto (default routing)". The node
    /// returns to scheme/content-type routing. (engine-picker Phase 3.)
    AutoEngine,
    /// Begin tagging the selected node(s): open the host tag-entry prompt, whose
    /// committed text inserts as a tag on the selection. Drains by opening the
    /// prompt; no member set consumed. (Add-tag.)
    AddTag,
    /// Open the right-clicked link as a new tab (a new node stacked into the source
    /// tile's slot, or a fresh tile). The link is in `context_link`. (Browser flow.)
    OpenLinkNewTab,
    /// Copy the right-clicked link's resolved url to the clipboard. (Browser flow.)
    CopyLink,
    /// Set the focused orrery pane's layout strategy — the per-pane layout picker. The
    /// id is a cartography adapter `projection_id` (`&'static str`, so `ContextAction`
    /// stays `Copy`), or `""` for force-directed (gyre, the default). Drains like
    /// `ShellbarMove` without touching `context_set`. (Layout picker.)
    SetLayoutStrategy(&'static str),
    /// Toggle the focused orrery pane's size-by-degree mode (node faces grow with their
    /// undirected degree). A scene-level presentation choice; drains like `SetLayoutStrategy`
    /// without touching `context_set`. (Node representation P0 — resize.)
    ToggleSizeByDegree,
    /// Toggle the focused orrery pane's **size-by-importance** mode (node faces grow with their
    /// graph-signals importance — degree-based now, betweenness later). A scene-level choice;
    /// drains like `ToggleSizeByDegree`. (Graph signals — importance encoding.)
    ToggleSizeByImportance,
    /// Override the context node(s)' **face** (the texture axis) — the per-node face picker.
    /// The id is a face key (`&'static str`, so `ContextAction` stays `Copy`): `"favicon"`,
    /// `"sprite"`, or `"bare"`. Applies to the context set like the engine pins. The body
    /// (collider shape) is a separate axis. (Node body & face — the Face axis.)
    SetFace(&'static str),
    /// Summon the object card for the single selected node — a light per-object action
    /// card (P0: the size-tier stepper) in the focus slot, in place of the snapshot preview.
    /// Drains by setting `view.object_card`; no member set consumed. (Object card — P0.)
    ResizeNode,
    /// Scope the focused orrery to the selection (plus its neighbors) — the "Isolate"
    /// lens. Drains like `ShellbarMove` without touching `context_set`. (Curated orrery.)
    IsolateSelection,
    /// Crystallize the current multi-selection into a **Session** graphlet tagged with its
    /// dominant shape (the classifier's top kind), then scope the orrery to it — the commit that
    /// makes the connections-swatch preview real (the 2026-06-13 crystallize / reconciliation
    /// ruling 4). Multi-node; pushes a `ShellCommand` (Shell-level mint). (Swatch primitive — P3b.)
    CrystallizeSelection,
    /// Open the focused node's connected component as a persistent **Linked** graphlet in
    /// its own scoped window — the manual consumer for auto-derived graphlets (Phase 3
    /// slice 2). Unlike `IsolateSelection` (a transient lens on the current orrery), this
    /// mints a `Linked { Component }` graphlet that reconciles as the graph drifts, and
    /// opens a window scoped to it. Pushes a `ShellCommand` (Shell-level open). (Graphlet
    /// wiring Phase 3.)
    OpenComponentGraphlet,
    /// Open the focused node's **neighborhood** (an Ego graphlet, radius 2) as a Linked
    /// graphlet in its own scoped window. Like `OpenComponentGraphlet` but radius-bounded,
    /// so the scoped window shows a *subset* (the node plus two hops), not the whole
    /// component — the second derived kind surfaced in the UI. (Graphlet wiring Phase 3.)
    OpenNeighborhoodGraphlet,
    /// Open the focused node's **link web** — its connected component under the Semantic
    /// edge projection only (selectors = `["Semantic"]`), so the roster follows links /
    /// citations, not containment or arrangement relations. Exercises the selector
    /// (edge-projection) control end to end from the UI. (Graphlet wiring Phase 3.)
    OpenLinkWebGraphlet,
    /// Drop the orrery's scope lens — show the whole graph again ("Show all"). Drains
    /// like `ShellbarMove` without touching `context_set`. (Curated orrery.)
    ShowAllNodes,
    /// Scope the orrery to the workbench's open tiles — the same arrangement rendered
    /// as both a tiled workbench and a spatial map. Drains like `ShellbarMove` without
    /// touching `context_set`. (Curated orrery — workbench mirror.)
    MirrorTiles,
    /// Open the context node's facets settings tile (the `node:<id>` provider) — the menu's
    /// pointer to the per-node config that used to be inlined as the engine / representation
    /// pickers. Drains over the first `context_set` member. (Settings lane P3.)
    OpenNodeFacets,
    /// Run a global [`Command`](crate::command::Command) by its verb — the seam that lets the
    /// configurable context menu carry *any* registry command, not just the menu's native
    /// context actions. The verb is `&'static` (from `Command::verb`), so this stays `Copy`.
    /// Drains by dispatching to the host's command runner. (Command registry P4.)
    RunCommand(&'static str),
    /// Pin / unpin a registry command (by verb / id) to the persona's curated context menu — the
    /// pin toggle on a search result (the cursor palette). Unlike the other actions it does **not**
    /// close the menu (you can pin several in a row); the host toggles `menu_actions`, persists, and
    /// rebuilds the open menu. (Searchable context menu S2.)
    PinToMenu(&'static str),
}

mod chrome_comms;
mod chrome_menu;
mod chrome_nav;

pub mod knot_highlight;
pub mod note_view;
mod views;
use views::sync_chrome_from_history;
pub use views::{ChromeLogic, ChromeView, chrome_view, runner, submit_omnibar};
#[cfg(test)]
mod tests;
