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
//! ## Separate roots
//!
//! From the first commit the **chrome-root** (this view tree, diffed by
//! `xilem_serval` from app state) and each **content-root** (mutated by its
//! engine / script) are distinct document authorities; neither sees the other's
//! tree (flip plan, Phase 3 + Standing constraints).
//!
//! ## Status
//!
//! The toolbar renders from a reused [`chrome::toolbar::ToolbarState`] into a
//! serval `ScriptedDom` via [`ServalAppRunner`], on screen, with an editable
//! omnibar ([`TextInput`] lensed into the view). Submitting (Enter) and the
//! back / forward buttons drive a real linear [`History`](nav::History): the
//! omnibar text is classified and resolved to a URL, pushed onto the stack, and
//! mirrored back into the toolbar (location text, `can_go_*` flags). The current
//! history entry — [`Chrome::content_location`] — is what the **content-root**
//! slice (next) will load into a separate document authority.

use std::cell::RefCell;
use std::rc::Rc;

use chrome::command_palette::{CommandPaletteSession, SearchPaletteScope};
use chrome::omnibar::OmnibarMatch;
use chrome::toolbar::ToolbarState;
use comms::{
    CommsPane, Conversation, ConversationId, Direction, Identity, Inbox, Message, MessageBody,
    MessageId, ProtocolKind,
};
use serval_scripted_dom::ScriptedDom;
use xilem_serval::{
    el, lens, on_click, text_field_typed, AnyView, PointerClick, ServalAppRunner, ServalCtx,
    ServalElement, TextField, TextInput,
};

pub mod command;
pub mod ingest;
pub mod nav;
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
    /// Whether the settings overlay is open.
    pub settings_open: bool,
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
        Self { label: label.into(), action }
    }
}

/// What a context-menu row asks the host to do with the menu's member set: open
/// each in its own split, or gather them into one tab-stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextAction {
    OpenSplits,
    Stack,
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
            open_as_new_node: false,
            suggest: Vec::new(),
            suggest_active: None,
            palette_open: false,
            palette: CommandPaletteSession::default(),
            palette_input: TextInput::new(""),
            sync: SyncIndicator::default(),
            pending_connect: None,
            pending_command: None,
            settings: Settings::default(),
            settings_open: false,
            context_menu: None,
            pending_context: None,
            comms: {
                let mut pane = CommsPane::new();
                pane.set_inbox(placeholder_inbox());
                pane
            },
            comms_draft: TextInput::new(""),
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
            },
            Command::ConnectPeer => {
                // Record the intent; the host executes it (the chrome cannot reach
                // the sync actor). The ticket is whatever is in the address bar.
                self.pending_connect = Some(self.omnibar.text().trim().to_string());
            },
            // A chrome-level action: open the settings overlay right here (no host
            // intent needed, like toggling the palette).
            Command::OpenSettings => self.open_settings(),
            // Chrome-level too: toggle the docked comms pane in place.
            Command::ToggleComms => self.toggle_comms(),
            Command::ToggleWorkbench
            | Command::DeleteNode
            | Command::BackgroundNode
            | Command::HideSelectedEdge
            | Command::ShowAllEdges => {
                // Host actions over the orrery / workbench / actor pool: record the
                // intent; the host drains it and runs the matching method.
                self.pending_command = Some(cmd);
            },
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

    /// Open the settings overlay (closing the palette + suggestions dropdown).
    pub fn open_settings(&mut self) {
        self.settings_open = true;
        self.close_palette();
        self.close_suggestions();
    }

    /// Close the settings overlay.
    pub fn close_settings(&mut self) {
        self.settings_open = false;
    }

    /// Raise the active-tab cap by one (bounded, so the overlay can't set an
    /// absurd value). The host applies + persists the change.
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

    /// Toggle the docked comms pane open/closed.
    pub fn toggle_comms(&mut self) {
        self.comms.toggle();
    }

    /// Close the comms pane.
    pub fn close_comms(&mut self) {
        self.comms.close();
    }

    /// Open conversation `id` in the comms pane: select it, load its thread
    /// (placeholder for now), and arm a fresh compose buffer. The live adapters
    /// will load the real thread through the event loop later.
    pub fn select_conversation(&mut self, id: ConversationId) {
        self.comms.select(id.clone());
        self.comms.set_thread(placeholder_thread(&id));
        self.comms_draft = TextInput::new("");
    }

    /// Send the composed reply. Placeholder: echoes the draft into the open thread
    /// as an outgoing message and clears the buffer. The live path routes the
    /// draft to `Comms::send` through the event loop.
    pub fn send_comms(&mut self) {
        let body = self.comms_draft.text().trim().to_string();
        if body.is_empty() {
            return;
        }
        if let Some(id) = self.comms.selected().cloned() {
            self.comms.thread.push(Message {
                id: MessageId(format!("local-{}", self.comms.thread.len())),
                author: Identity::new(id.protocol, "me"),
                body: MessageBody::PlainText(body),
                subject: None,
                timestamp_ms: None,
                direction: Direction::Outgoing,
            });
            self.comms.clear_draft();
            self.comms_draft = TextInput::new("");
        }
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

/// Placeholder conversation list for the comms pane shell, until the live misfin /
/// murm adapters fill it through the event loop. One murm cabal, one misfin
/// correspondent.
fn placeholder_inbox() -> Inbox {
    Inbox {
        conversations: vec![
            Conversation {
                id: ConversationId::new(ProtocolKind::Murm, "cabal-demo"),
                title: "Project cabal".to_string(),
                participants: Vec::new(),
                last_activity_ms: Some(5_000),
                unread: 2,
            },
            Conversation {
                id: ConversationId::new(ProtocolKind::Misfin, "ana@example.test"),
                title: "ana@example.test".to_string(),
                participants: Vec::new(),
                last_activity_ms: Some(3_000),
                unread: 1,
            },
        ],
        failures: Vec::new(),
    }
}

/// A placeholder thread for `id`, so selecting a conversation shows something.
fn placeholder_thread(id: &ConversationId) -> Vec<Message> {
    vec![
        Message {
            id: MessageId("d1".to_string()),
            author: Identity::new(id.protocol, "peer"),
            body: MessageBody::PlainText("Hey — this is a placeholder conversation.".to_string()),
            subject: None,
            timestamp_ms: Some(1_000),
            direction: Direction::Incoming,
        },
        Message {
            id: MessageId("d2".to_string()),
            author: Identity::new(id.protocol, "me"),
            body: MessageBody::PlainText("Yep. Live misfin + murm backends wire in next.".to_string()),
            subject: None,
            timestamp_ms: Some(2_000),
            direction: Direction::Outgoing,
        },
    ]
}

mod views;
pub use views::{ChromeView, ChromeLogic, chrome_view, submit_omnibar, runner};
use views::sync_chrome_from_history;
#[cfg(test)]
mod tests;
