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
pub use session_runtime::ShellbarEdge;
use xilem_serval::TextInput;

pub mod command;
pub mod ingest;
pub mod nav;
pub mod shell_eval;
pub mod suggest;
pub mod sync_indicator;

use command::Command;
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
    /// A context-menu action a row captured, for the host to run over the menu's
    /// member set.
    pub pending_context: Option<ContextAction>,
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
    /// Slim chrome: a leaf (torn-out) window omits the shellbar (and the host omits
    /// the switcher), leaving just the toolbar over workbench-only content. Set once
    /// from the window's `WindowKind`. (Multi-window MW3 step 4.)
    pub slim: bool,
}

/// Which panes are currently open in the frame tree. The host syncs this into
/// Chrome before each render pass so the shellbar buttons reflect live state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShellbarPaneStates {
    pub workbench: bool,
    pub roster: bool,
    pub gloss: bool,
    pub apparatus: bool,
    pub inspector: bool,
    pub steward: bool,
    pub comms: bool,
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
}

/// One row of a [`ContextMenu`]: its label + the action it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextItem {
    pub label: String,
    pub action: ContextAction,
}

impl ContextItem {
    /// A row pairing `label` with the `action` it captures when clicked.
    pub fn new(label: impl Into<String>, action: ContextAction) -> Self {
        Self {
            label: label.into(),
            action,
        }
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
    /// Relate the two selected nodes (a user-grouped relation). Offered only for a
    /// two-node selection; drains like `ShellbarMove` without opening tiles.
    Relate,
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
    /// Override the context node(s)' presentation form — the per-node representation
    /// picker ("Show as tile / shape"). The id is a representation key (`&'static str`,
    /// so `ContextAction` stays `Copy`): `"tile"` or `"shape"`. Applies to the context
    /// set like the engine pins. (Node representation P1.)
    SetRepresentation(&'static str),
    /// Summon the object card for the single selected node — a light per-object action
    /// card (P0: the size-tier stepper) in the focus slot, in place of the snapshot preview.
    /// Drains by setting `view.object_card`; no member set consumed. (Object card — P0.)
    ResizeNode,
    /// Scope the focused orrery to the selection (plus its neighbors) — the "Isolate"
    /// lens. Drains like `ShellbarMove` without touching `context_set`. (Curated orrery.)
    IsolateSelection,
    /// Drop the orrery's scope lens — show the whole graph again ("Show all"). Drains
    /// like `ShellbarMove` without touching `context_set`. (Curated orrery.)
    ShowAllNodes,
    /// Scope the orrery to the workbench's open tiles — the same arrangement rendered
    /// as both a tiled workbench and a spatial map. Drains like `ShellbarMove` without
    /// touching `context_set`. (Curated orrery — workbench mirror.)
    MirrorTiles,
}

impl Chrome {
    /// A chrome state seeded with `initial_location` (classified the same way a
    /// submission is, so a bare host normalizes) across the history, the reused
    /// toolbar session state, and the live omnibar buffer.
    pub fn new(initial_location: impl Into<String>) -> Self {
        let raw = initial_location.into();
        // A blank cold-start stays blank (not a search for the empty string).
        let location = if raw.trim().is_empty() {
            String::new()
        } else {
            nav::classify(&raw).resolve()
        };
        Self {
            toolbar: ToolbarState::with_initial_location(location.clone()),
            omnibar: TextInput::new(location.clone()),
            history: History::new(location.clone()),
            content_location: location,
            history_step: None,
            physics_toggle: false,
            physics_paused: false,
            open_as_new_node: false,
            suggest: Vec::new(),
            suggest_active: None,
            palette_open: false,
            palette: CommandPaletteSession::default(),
            palette_input: TextInput::new(""),
            find_open: false,
            find_input: TextInput::new(""),
            find_active: 0,
            find_count: 0,
            sync: SyncIndicator::default(),
            pending_connect: None,
            pending_command: None,
            settings: Settings::default(),
            context_menu: None,
            pending_context: None,
            comms: CommsPane::new(),
            comms_draft: TextInput::new(""),
            comms_intent: None,
            comms_new_to: TextInput::new(""),
            comms_new_body: TextInput::new(""),
            shellbar_panes: ShellbarPaneStates::default(),
            shellbar_edge: ShellbarEdge::default(),
            slim: false,
        }
    }

    /// The URL the content root should display. The host's `sync_orrery` reads
    /// this and navigates the focused node to it; the chrome never renders it. Set
    /// by `submit_omnibar` / `navigate_suggestion` (fresh nav) and by the host on
    /// a back/forward step. Independent of [`History`] (the suggestions log).
    pub fn content_location(&self) -> &str {
        &self.content_location
    }

    /// Display `url` as the current location (toolbar text + omnibar) without
    /// touching history — the omnibar follows the focused tile / node. The host
    /// gates this so it does not clobber the omnibar mid-edit.
    pub fn show_location(&mut self, url: &str) {
        self.toolbar.editable.location = url.to_string();
        self.toolbar.editable.location_dirty = false;
        self.toolbar.editable.location_submitted = false;
        self.omnibar = TextInput::new(url.to_string());
        self.close_suggestions();
    }

    /// Regenerate the omnibar suggestions from the current omnibar text and the
    /// history, resetting the highlight. The host calls this after each omnibar
    /// edit (keystroke / caret move).
    pub fn refresh_suggestions(&mut self) {
        self.suggest = suggest::suggestions(self.omnibar.text(), &self.history);
        self.suggest_active = None;
        self.refresh_ghost();
    }

    /// Recompute the omnibar's inline ghost completion. In command mode
    /// (`>token`) the best-matching command verb / query completes the partial
    /// token, shown dim until → / Tab accept it (the same `Command` vocabulary the
    /// palette lists); anything else clears the ghost. Only a clean token at the
    /// buffer end gets a ghost, so accepting — which appends — always lands right.
    pub fn refresh_ghost(&mut self) {
        let ghost = match nav::classify(self.omnibar.text()) {
            nav::NavTarget::Command(expr) if self.omnibar.text().ends_with(&expr) => {
                shell_eval::complete(&expr)
                    .map(|full| full[expr.len()..].to_string())
                    .unwrap_or_default()
            }
            _ => String::new(),
        };
        self.omnibar.set_ghost(ghost);
    }

    /// Move the suggestion highlight by `delta` (wrapping), opening the
    /// highlight at the first/last row when nothing is highlighted yet. A no-op
    /// when there are no suggestions.
    pub fn step_suggestion(&mut self, delta: isize) {
        let count = self.suggest.len();
        if count == 0 {
            self.suggest_active = None;
            return;
        }
        let n = count as isize;
        self.suggest_active = Some(match self.suggest_active {
            Some(cur) => ((cur as isize + delta).rem_euclid(n)) as usize,
            None if delta >= 0 => 0,
            None => (n - 1) as usize,
        });
    }

    /// Close the suggestion dropdown without navigating.
    pub fn close_suggestions(&mut self) {
        self.suggest.clear();
        self.suggest_active = None;
    }

    /// Navigate to suggestion row `i` (clicked or Enter-on-highlight): resolve
    /// its URL, push history, and mirror into the toolbar + omnibar.
    pub fn navigate_suggestion(&mut self, i: usize) {
        if let Some(url) = self.suggest.get(i).and_then(suggest::resolve_match) {
            self.content_location = url.clone();
            self.history.visit(url);
            sync_chrome_from_history(self, true);
        }
    }

    /// Navigate the content to a fully-resolved link URL — a click on a link inside
    /// a rendered content card. Records the visit and mirrors it into the toolbar +
    /// omnibar, the same record-the-visit path as a suggestion; the host resolves the
    /// href (relative links join the card's own URL) before calling. (Inline-link nav.)
    pub fn follow_link(&mut self, url: String) {
        self.content_location = url.clone();
        self.history.visit(url);
        sync_chrome_from_history(self, true);
    }

    /// Toggle the command palette open/closed.
    pub fn toggle_palette(&mut self) {
        if self.palette_open {
            self.close_palette();
        } else {
            self.open_palette();
        }
    }

    /// Open the command palette: arm the reused session (fresh query, no
    /// selection), clear the editing buffer, and close any omnibar dropdown.
    pub fn open_palette(&mut self) {
        self.palette_open = true;
        self.palette.open_fresh(SearchPaletteScope::default());
        self.palette_input = TextInput::new("");
        self.close_suggestions();
    }

    /// Close the command palette without running anything.
    pub fn close_palette(&mut self) {
        self.palette_open = false;
        self.palette.selected_index = None;
    }

    /// Toggle the find-in-page bar open/closed.
    pub fn toggle_find(&mut self) {
        if self.find_open {
            self.close_find();
        } else {
            self.open_find();
        }
    }

    /// Open the find bar: a fresh empty query, the active match reset, the omnibar
    /// dropdown closed.
    pub fn open_find(&mut self) {
        self.find_open = true;
        self.find_input = TextInput::new("");
        self.find_active = 0;
        self.close_suggestions();
    }

    /// Close the find bar (the host clears the actor's matches separately).
    pub fn close_find(&mut self) {
        self.find_open = false;
    }

    /// The commands matching the current palette query.
    pub fn palette_commands(&self) -> Vec<Command> {
        command::filter(self.palette_input.text())
    }

    /// Mirror the edited palette buffer into the reused session query and reset
    /// the selection (the filtered list just changed). Called after each edit.
    pub fn sync_palette_query(&mut self) {
        self.palette.query = self.palette_input.text().to_string();
        self.palette.selected_index = None;
    }

    /// Move the palette selection by `delta`, wrapping within the filtered
    /// commands — the reused [`CommandPaletteSession::step_selection`] cursor.
    pub fn step_palette(&mut self, delta: isize) {
        let count = self.palette_commands().len();
        self.palette.step_selection(delta, count);
    }

    /// Run the highlighted palette command (or the first, if none is
    /// highlighted) and close. A no-op close when nothing matches.
    pub fn run_palette_selection(&mut self) {
        let cmds = self.palette_commands();
        let pick = self.palette.selected_index.unwrap_or(0);
        if let Some(&cmd) = cmds.get(pick) {
            self.run_command(cmd);
        }
        self.close_palette();
    }

    /// Run `cmd` (e.g. a clicked palette row) and close the palette.
    pub fn run_command_and_close(&mut self, cmd: Command) {
        self.run_command(cmd);
        self.close_palette();
    }

    /// Apply a command to the chrome state.
    fn run_command(&mut self, cmd: Command) {
        match cmd {
            // Back / forward step the focused node's own history, not a
            // chrome-global one — record the intent; the host drains it via the
            // orrery (the chrome cannot reach it).
            Command::Back => self.history_step = Some(HistoryStep::Back),
            Command::Forward => self.history_step = Some(HistoryStep::Forward),
            Command::Home => {
                let url = "mere://welcome".to_string();
                self.content_location = url.clone();
                self.history.visit(url);
                sync_chrome_from_history(self, true);
            }
            Command::ConnectPeer => {
                // Record the intent; the host executes it (the chrome cannot reach
                // the sync actor). The ticket is whatever is in the address bar.
                self.pending_connect = Some(self.omnibar.text().trim().to_string());
            }
            // Chrome-level: toggle the docked comms pane in place.
            Command::ToggleComms => self.toggle_comms(),
            // Settings now opens as a workbench tile (the pelt settings lane), so it is a
            // host action like the other pane toggles, not the chrome overlay it was in P0.
            // (Settings lane P1.)
            Command::OpenSettings
            | Command::ToggleWorkbench
            | Command::ToggleRoster
            | Command::ToggleGloss
            | Command::ToggleApparatus
            | Command::DeleteNode
            | Command::BackgroundNode
            | Command::HideSelectedEdge
            | Command::ShowAllEdges
            | Command::ToggleInspector
            | Command::ToggleTrail
            | Command::ToggleSteward
            | Command::RetryFocusedContent
            | Command::StopFocusedOperation
            | Command::PinFocusedOperation
            | Command::ToggleCompatView
            | Command::AssertEdge
            | Command::RetractEdge
            | Command::CloseGraphPane
            | Command::ExportGraph => {
                // Host actions over the frame, orrery, workbench, or actor pool:
                // record the intent; the host drains it and runs the matching
                // method.
                self.pending_command = Some(cmd);
            }
        }
    }

    /// Take a pending "connect to peer" request, if one is queued. The host calls
    /// this after running a palette command, then drives the sync actor with it.
    pub fn take_pending_connect(&mut self) -> Option<String> {
        self.pending_connect.take()
    }

    /// Take a pending host action, if one is queued. The host calls this after a
    /// palette run / row click and dispatches it to the matching shell method.
    pub fn take_pending_command(&mut self) -> Option<Command> {
        self.pending_command.take()
    }

    /// Take a pending back/forward step, if one is queued. The host applies it to
    /// the focused node's history and mirrors the revealed page back via
    /// [`Chrome::show_location`] + [`Chrome::content_location`].
    pub fn take_history_step(&mut self) -> Option<HistoryStep> {
        self.history_step.take()
    }

    /// Take the pending pause/play toggle, if the button was clicked this pass. The
    /// host applies it to the orrery's physics. (Physics pause.)
    pub fn take_physics_toggle(&mut self) -> bool {
        std::mem::take(&mut self.physics_toggle)
    }

    /// Raise the active-tab cap by one (bounded, so it can't reach an absurd value).
    /// Driven by the `pelt/appearance` page's cap control; the host applies + persists it.
    pub fn inc_tab_cap(&mut self) {
        self.settings.tab_cap = (self.settings.tab_cap + 1).min(64);
    }

    /// Lower the active-tab cap by one (never below 1).
    pub fn dec_tab_cap(&mut self) {
        self.settings.tab_cap = self.settings.tab_cap.saturating_sub(1).max(1);
    }

    /// Open the right-click context menu at window `(x, y)` with host-computed
    /// `items` (closing the suggestions dropdown so it can't overlap).
    pub fn open_context_menu(&mut self, x: f32, y: f32, items: Vec<ContextItem>) {
        self.close_suggestions();
        self.context_menu = Some(ContextMenu { x, y, items });
    }

    /// Open the add-pill's menu at `(x, y)`: add a node, a tile, or a session. The
    /// three rows reuse the context-menu machinery; the host drains the chosen
    /// `ContextAction`.
    pub fn open_add_menu(&mut self, x: f32, y: f32) {
        self.open_context_menu(
            x,
            y,
            vec![
                ContextItem::new("Add node", ContextAction::AddNode),
                ContextItem::new("Add tile", ContextAction::AddTile),
                ContextItem::new("Add session", ContextAction::AddSession),
                ContextItem::new("Add field", ContextAction::AddField),
            ],
        );
    }

    /// Close the context menu without running anything.
    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    /// Capture `action` from a clicked menu row and close the menu; the host drains
    /// it and applies it to the menu's member set.
    pub fn pick_context(&mut self, action: ContextAction) {
        self.pending_context = Some(action);
        self.close_context_menu();
    }

    /// Take the pending context-menu action, if any.
    pub fn take_pending_context(&mut self) -> Option<ContextAction> {
        self.pending_context.take()
    }

    /// Toggle the docked comms pane open/closed. Opening records a `Refresh` so
    /// the host loads the latest conversation list.
    pub fn toggle_comms(&mut self) {
        self.comms.toggle();
        if self.comms.is_open() {
            self.comms_intent = Some(CommsIntent::Refresh);
        }
    }

    /// Close the comms pane.
    pub fn close_comms(&mut self) {
        self.comms.close();
    }

    /// Open conversation `id`: select it (clearing the prior thread) and record an
    /// `Open` so the host loads its messages from the live `Comms`.
    pub fn select_conversation(&mut self, id: ConversationId) {
        self.comms.select(id.clone());
        self.comms_draft = TextInput::new("");
        self.comms_intent = Some(CommsIntent::Open(id));
    }

    /// Record a send of the composed reply: sync the editing buffer into the draft
    /// and, if it is ready, hand it to the host (which routes it to `Comms::send`
    /// and reloads the thread). A no-op for an empty draft or no selection.
    pub fn send_comms(&mut self) {
        self.comms.set_draft_body(self.comms_draft.text().trim());
        if self.comms.can_send() {
            self.comms_intent = Some(CommsIntent::Send(self.comms.draft.clone()));
        }
    }

    /// Open the compose-new form (drops any open conversation), with fresh
    /// recipient + body buffers.
    pub fn open_new_message(&mut self) {
        self.comms.open_new_message();
        self.comms_new_to = TextInput::new("");
        self.comms_new_body = TextInput::new("");
    }

    /// Close the compose-new form without sending.
    pub fn close_new_message(&mut self) {
        self.comms.close_new_message();
    }

    /// "Share cabal invite": open a new misfin message pre-filled with the cabal
    /// join ticket as its body, so the user just adds a recipient and sends. A
    /// no-op until the cabal (and its ticket) is up.
    pub fn share_cabal_invite(&mut self) {
        let Some(ticket) = self.comms.cabal_ticket.clone() else {
            return;
        };
        self.open_new_message();
        self.comms_new_body = TextInput::new(ticket);
    }

    /// Set the compose-new form's protocol (the Misfin / Cable toggle).
    pub fn set_new_message_protocol(&mut self, protocol: ProtocolKind) {
        self.comms.set_new_protocol(protocol);
    }

    /// Send the compose-new form: build a [`Draft`] from the chosen protocol +
    /// recipient + body and hand it to the host. Misfin targets the typed
    /// `mailbox@host`; Cable targets the (first) murm cabal in the inbox. A no-op
    /// for an empty body, an empty misfin address, or no cable to target.
    pub fn send_new_message(&mut self) {
        let Some(form) = self.comms.new_message.as_ref() else {
            return;
        };
        let protocol = form.protocol;
        let to = self.comms_new_to.text().trim().to_string();
        let body = self.comms_new_body.text().trim().to_string();
        if body.is_empty() {
            return;
        }
        let conversation = match protocol {
            ProtocolKind::Misfin if !to.is_empty() => {
                Some(ConversationId::new(ProtocolKind::Misfin, to))
            }
            ProtocolKind::Misfin => return,
            ProtocolKind::Murm => self
                .comms
                .inbox
                .iter()
                .find(|c| c.id.protocol == ProtocolKind::Murm)
                .map(|c| c.id.clone()),
        };
        let Some(conversation) = conversation else {
            return;
        };
        self.comms_intent = Some(CommsIntent::Send(Draft {
            conversation: Some(conversation),
            body,
            subject: None,
        }));
        // Keep the form open so its send-status line shows the outcome, and a
        // failed send keeps the typed address + body to fix. The user closes it
        // with Cancel.
    }

    /// Record a request to connect the cabal from a received join `ticket` (a
    /// "Join this cabal" on an invite message). The host routes it to the actor.
    pub fn connect_cabal(&mut self, ticket: String) {
        self.comms_intent = Some(CommsIntent::ConnectCabal(ticket));
    }

    /// Take the pending comms request, if any. The host drains it after input and
    /// issues the matching command to the comms actor.
    pub fn take_comms_intent(&mut self) -> Option<CommsIntent> {
        self.comms_intent.take()
    }

    /// Clear the compose buffer + draft after a successful send (the host calls
    /// this when the actor reports `Sent`).
    pub fn clear_comms_draft(&mut self) {
        self.comms.clear_draft();
        self.comms_draft = TextInput::new("");
    }

    /// The text field that currently owns editing / the caret: the comms compose
    /// buffer when the pane has focus, the palette query when the palette is open,
    /// otherwise the omnibar. The host reads this to paint the caret on the right
    /// field.
    pub fn active_field(&self) -> &TextInput {
        if self.comms.is_open() && self.comms.dock.focused {
            &self.comms_draft
        } else if self.palette_open {
            &self.palette_input
        } else {
            &self.omnibar
        }
    }
}

mod views;
use views::sync_chrome_from_history;
pub use views::{ChromeLogic, ChromeView, chrome_view, runner, submit_omnibar};
#[cfg(test)]
mod tests;
