/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! meerkat's command-palette command set — pure data (no [`Chrome`] dependency),
//! so it stays trivially testable. The palette *session* (query, selection
//! cursor, `step_selection`) is the reused
//! [`CommandPaletteSession`](chrome::command_palette::CommandPaletteSession);
//! the command *source* is meerkat's, the same split as omnibar suggestions
//! (reused match type, meerkat-sourced items).
//!
//! [`Chrome`]: crate::Chrome

/// A command the palette can run. Each maps to a [`Chrome`](crate::Chrome)
/// mutation in `Chrome::run_command`. The set is intentionally small (the
/// browser-chrome verbs that need no engine yet); reload / focus / settings join
/// as their wiring lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Step back one history entry.
    Back,
    /// Step forward one history entry.
    Forward,
    /// Navigate to the built-in `mere://welcome` page.
    Home,
    /// Connect the p2p sync to a peer, using the ticket pasted in the address bar
    /// (S5.1). The chrome records the intent; the host routes it to the sync actor.
    ConnectPeer,
    /// Toggle the tiled workbench (Tree) and the orrery (Cartography) projections.
    /// A host action: the chrome records the intent, the host runs it.
    ToggleWorkbench,
    /// Delete the focused node from the graph (host action).
    DeleteNode,
    /// Toggle the focused node's background-keep flag — keep its actor running
    /// when focus moves away (host action).
    BackgroundNode,
    /// Hide the selected edge(s) from the orrery — display-only, the relations
    /// persist (host action).
    HideSelectedEdge,
    /// Reveal every hidden edge (host action).
    ShowAllEdges,
    /// Toggle the orrery's isometric (2.5D foreshortened-ground) projection on/off
    /// (host action). The orrery owns the camera; the host drains this to flip it.
    ToggleProjection,
    /// Open the settings lane (the consolidated config surface) as a workbench tile, at
    /// the appearance page. A host action, drained by the host. (Settings lane P1.)
    OpenSettings,
    /// Open the focused node's facets (the `node:<id>` settings provider) as a workbench
    /// tile, at the info page. A host action. (Settings lane P3.)
    OpenNodeSettings,
    /// Toggle the docked comms pane (conversations: misfin mail + murm cabals). A
    /// chrome-level action, like toggling the palette.
    ToggleComms,
    /// Toggle the roster pane (graph manifest: nodes/edges/fields) (host action).
    ToggleRoster,
    /// Toggle the gloss / Navigator pane (host action).
    ToggleGloss,
    /// Toggle the apparatus pane (host diagnostics; settings moved to the pelt lane) (host action).
    ToggleApparatus,
    /// Toggle the selected-object inspector pane (host action).
    ToggleInspector,
    /// Toggle the trail pane: nav history + recent + removed nodes (host action).
    ToggleTrail,
    /// Toggle the live-operations steward pane (host action).
    ToggleSteward,
    /// Retry the focused node's page fetch / render operation (host action).
    RetryFocusedContent,
    /// Stop the focused node's live operation by reaping its actor (host action).
    StopFocusedOperation,
    /// Pin the focused operation as background work (host action).
    PinFocusedOperation,
    /// Toggle the focused node's compatibility view — render it through the
    /// system WebView (scrying) instead of the built-in engines (host action).
    ToggleCompatView,
    /// Assert a user relation between exactly two selected nodes — the manual
    /// edge-creation gesture (host action). Defaults to a `UserGrouped` semantic
    /// relation; honest provenance (a human asserted it).
    AssertEdge,
    /// Retract the user relation(s) on the selected edge — a true removal, not the
    /// display-only `HideSelectedEdge` (host action).
    RetractEdge,
    /// Close the focused graph (Orrery) pane when more than one is open — the
    /// dismiss for a second graph-pane. A no-op with a single graph view (host
    /// action). (Window composition — pane-as-unit.)
    CloseGraphPane,
    /// Export the focused graph as JSON-LD to a file (host action). The dormant
    /// inverse of the wired linked-data ingest; `linked_data::to_jsonld_string`.
    ExportGraph,
    /// Freeze the focused graph into a content-addressed graph engram in the private
    /// eidetic store (host action) — the Alembic memory spine's "Save as graph engram".
    /// Redacts private / heavy fields by default; `session_runtime::graph_engram`.
    SaveGraphEngram,
    /// Crawl the focused page's link neighborhood into the graph (host action): seed
    /// the crawl actor from the focused node's URL, bounded by a conservative default
    /// policy (same-host, shallow). Each fetched page's links + metadata become graph
    /// nodes. (Relational-browse V2; extraction lane.)
    CrawlFocused,
}

impl Command {
    /// Every command, in display order.
    pub const ALL: [Command; 29] = [
        Command::Back,
        Command::Forward,
        Command::Home,
        Command::ConnectPeer,
        Command::ToggleWorkbench,
        Command::ToggleRoster,
        Command::ToggleGloss,
        Command::ToggleApparatus,
        Command::DeleteNode,
        Command::BackgroundNode,
        Command::HideSelectedEdge,
        Command::ShowAllEdges,
        Command::ToggleProjection,
        Command::OpenSettings,
        Command::OpenNodeSettings,
        Command::ToggleComms,
        Command::ToggleInspector,
        Command::ToggleTrail,
        Command::ToggleSteward,
        Command::RetryFocusedContent,
        Command::StopFocusedOperation,
        Command::PinFocusedOperation,
        Command::ToggleCompatView,
        Command::AssertEdge,
        Command::RetractEdge,
        Command::CloseGraphPane,
        Command::ExportGraph,
        Command::SaveGraphEngram,
        Command::CrawlFocused,
    ];

    /// Whether this command is a *host* action (run by the shell over the graph /
    /// workbench / actors) rather than a chrome-level history verb. Host actions
    /// are recorded as a pending intent the host drains, like `ConnectPeer`.
    pub fn is_host_action(self) -> bool {
        matches!(
            self,
            Command::OpenNodeSettings
                | Command::ToggleWorkbench
                | Command::ToggleRoster
                | Command::ToggleGloss
                | Command::ToggleApparatus
                | Command::DeleteNode
                | Command::BackgroundNode
                | Command::HideSelectedEdge
                | Command::ShowAllEdges
                | Command::ToggleProjection
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
                | Command::ExportGraph
                | Command::SaveGraphEngram
                | Command::CrawlFocused
        )
    }

    /// The selection context this command belongs to in a configurable menu (command
    /// registry P4). Commands are global verbs that run in any context, so they default
    /// to [`MenuScope::Always`]; the applicability that reproduces the current context
    /// menu lives on the context actions ([`context_action_scope`]). A few selection-
    /// operating commands narrow to [`MenuScope::Selection`] so adding them to the menu
    /// doesn't clutter the empty-canvas case. Finer per-command scoping is a refinement.
    pub fn menu_scope(self) -> MenuScope {
        use Command::*;
        match self {
            // Pairwise edge gestures — two or more selected nodes define the edge.
            AssertEdge | RetractEdge => MenuScope::MultiNode,
            // Act on a selection of one or more: delete the selected node(s), hide the
            // selected edge(s).
            DeleteNode | HideSelectedEdge => MenuScope::Selection,
            // Per-node gestures on the single focused node: its facets, its content
            // operation, its background-keep flag, its compat-view engine override.
            OpenNodeSettings | BackgroundNode | RetryFocusedContent | StopFocusedOperation
            | PinFocusedOperation | ToggleCompatView | CrawlFocused => MenuScope::SingleNode,
            // Navigation, app-level pane toggles, graph / pane ops, export — available in any
            // context (they target the focused node or the whole graph, not the selection).
            Back | Forward | Home | ConnectPeer | ToggleWorkbench | ToggleRoster | ToggleGloss
            | ToggleApparatus | ToggleComms | ToggleInspector | ToggleTrail | ToggleSteward
            | ShowAllEdges | ToggleProjection | OpenSettings | CloseGraphPane | ExportGraph
            | SaveGraphEngram => {
                MenuScope::Always
            }
        }
    }

    /// The omnibar command-shell token: the short identifier a `>`-expression
    /// calls (`>back`, `>workbench`). The palette's [`label`](Self::label) is the
    /// human phrase; this is the verb. It is the single source of truth the
    /// command shell derives its bindings from (over [`ALL`](Self::ALL)), so a new
    /// command is callable from the omnibar the moment it has a verb here — the
    /// exhaustive match makes that a compile-time obligation, not a second list to
    /// remember. Must stay a valid, unique rhai identifier (lowercase / `_`).
    pub fn verb(self) -> &'static str {
        match self {
            Command::Back => "back",
            Command::Forward => "forward",
            Command::Home => "home",
            Command::ConnectPeer => "connect_peer",
            Command::ToggleWorkbench => "workbench",
            Command::ToggleRoster => "roster",
            Command::ToggleGloss => "gloss",
            Command::ToggleApparatus => "apparatus",
            Command::ToggleInspector => "inspector",
            Command::ToggleTrail => "trail",
            Command::ToggleSteward => "steward",
            Command::ToggleComms => "comms",
            Command::OpenSettings => "settings",
            Command::OpenNodeSettings => "node_settings",
            Command::ToggleCompatView => "compat_view",
            Command::DeleteNode => "delete_node",
            Command::BackgroundNode => "background_node",
            Command::HideSelectedEdge => "hide_edge",
            Command::ShowAllEdges => "show_all_edges",
            Command::ToggleProjection => "projection",
            Command::RetryFocusedContent => "retry",
            Command::StopFocusedOperation => "stop",
            Command::PinFocusedOperation => "pin",
            Command::AssertEdge => "relate",
            Command::RetractEdge => "unrelate",
            Command::CloseGraphPane => "close_pane",
            Command::ExportGraph => "export_graph",
            Command::SaveGraphEngram => "save_graph_engram",
            Command::CrawlFocused => "crawl",
        }
    }

    /// Resolve a command from its stable id (its [`verb`](Self::verb)) — the reverse
    /// lookup that lets a caller addressing a command **by id** (a script, an agent, an
    /// a11y action route) get back the typed command to run. `None` for an unknown id.
    /// This is the command-registry seam: every surface (palette, omnibar `>`-shell,
    /// agent harness, accessibility, scripting) can name a command by the same id and
    /// resolve it here. (Command registry P1.)
    pub fn from_id(id: &str) -> Option<Command> {
        Command::ALL.into_iter().find(|cmd| cmd.verb() == id)
    }

    /// The user-facing label shown in the palette and matched against the query.
    pub fn label(self) -> &'static str {
        match self {
            Command::Back => "Back",
            Command::Forward => "Forward",
            Command::Home => "Home (mere://welcome)",
            Command::ConnectPeer => "Connect to peer (ticket in address bar)",
            Command::ToggleWorkbench => "Tile view (toggle workbench)",
            Command::DeleteNode => "Delete focused node",
            Command::BackgroundNode => "Keep focused node active in background",
            Command::HideSelectedEdge => "Hide selected edge",
            Command::ShowAllEdges => "Show all edges",
            Command::ToggleProjection => "Projection (toggle 2.5D isometric)",
            Command::OpenSettings => "Settings",
            Command::OpenNodeSettings => "Node settings (focused node)",
            Command::ToggleComms => "Comms (conversations)",
            Command::ToggleRoster => "Roster (graph manifest)",
            Command::ToggleGloss => "Gloss (navigator / map)",
            Command::ToggleApparatus => "Apparatus (diagnostics)",
            Command::ToggleInspector => "Inspector (selected object)",
            Command::ToggleTrail => "Trail (history + recent + removed)",
            Command::ToggleSteward => "Steward (live operations)",
            Command::RetryFocusedContent => "Retry focused content",
            Command::StopFocusedOperation => "Stop focused operation",
            Command::PinFocusedOperation => "Pin focused operation",
            Command::ToggleCompatView => "Compatibility view (system WebView, focused node)",
            Command::AssertEdge => "Relate selected nodes",
            Command::RetractEdge => "Unrelate selected edge",
            Command::CloseGraphPane => "Close graph view (focused pane)",
            Command::ExportGraph => "Export graph (JSON-LD)",
            Command::SaveGraphEngram => "Save graph as engram",
            Command::CrawlFocused => "Crawl focused page's links into the graph",
        }
    }
}

/// One entry in the command registry's catalog: a command's stable **id** (its
/// [`verb`](Command::verb)), its display **label**, and whether it is a **host action**
/// (a coarse applicability hint — host actions act on the graph / workbench / actors, so
/// they generally want a live target, whereas the chrome verbs always apply). Pure data
/// (no host dependency), so the catalog stays trivially listable by any consumer — the
/// palette, an agent / script enumerating what it can do, an a11y projection, a
/// diagnostics completeness check. The host-side *applies* predicate (needs-selection,
/// needs-2-nodes, …) and the *invoke* effect layer over this; they live with the host.
/// (Command registry P1.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub host_action: bool,
}

/// The command registry's catalog: every command as a [`CommandEntry`], in
/// [`Command::ALL`] order. The single listable source the palette renders, an agent /
/// script enumerates, and a diagnostics probe checks for completeness against. (Command
/// registry P1.)
pub fn command_entries() -> Vec<CommandEntry> {
    Command::ALL
        .into_iter()
        .map(|cmd| CommandEntry {
            id: cmd.verb(),
            label: cmd.label(),
            host_action: cmd.is_host_action(),
        })
        .collect()
}

/// Whether `label` matches the palette `query` (case-insensitive, whitespace-trimmed
/// substring; an empty query matches everything). The one match rule the palette uses
/// for every registry entry — commands and context actions alike. (Command registry.)
pub fn label_matches(label: &str, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty() || label.to_lowercase().contains(&needle)
}

/// The commands whose label contains `query` (case-insensitive, whitespace
/// trimmed). An empty query returns every command in [`Command::ALL`] order.
pub fn filter(query: &str) -> Vec<Command> {
    Command::ALL
        .iter()
        .copied()
        .filter(|cmd| label_matches(cmd.label(), query))
        .collect()
}

/// An item the command palette lists and runs: a global [`Command`], or a
/// [`ContextAction`](crate::ContextAction) applied to the current selection. The palette
/// is the unified registry surface (P2: "every action in the palette"), so both kinds
/// appear; each dispatches through its own apply path (a command via `run_command`, a
/// context action via the host's context drain over the live selection). (Command registry P2.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteItem {
    Command(Command),
    Context(crate::ContextAction),
}

impl PaletteItem {
    /// The row label, from the underlying command / context action. (Command registry P2.)
    pub fn label(self) -> &'static str {
        match self {
            PaletteItem::Command(cmd) => cmd.label(),
            PaletteItem::Context(action) => context_action_palette_label(action).unwrap_or(""),
        }
    }
}

/// The context actions exposed in the command palette, each with its palette label. An
/// **opt-in list** (not an exhaustive match), so a new `ContextAction` is simply not in
/// the palette until added here — no compile break for in-flight variants, and the
/// selection-only gestures stay deliberate. Parameterized pickers (engine / representation /
/// layout) and link/field-contextual actions are deferred (they get picker treatment / live
/// on settings pages); `Relate` + `CloseGraphPane` are omitted as they already have palette
/// `Command`s (`AssertEdge` / `CloseGraphPane`). Applicability filtering (gray-out without a
/// selection) is P3; for now a selection-only action invoked with no selection no-ops through
/// the existing context drain. (Command registry P2.)
/// Each tuple is `(action, id, label)`: the **stable id** is the registry handle (the
/// context-action analogue of a [`Command::verb`]), unique across the whole registry, by
/// which an agent / script / a menu config names the action; the label is the palette row.
/// Where a registry action belongs in a context menu — the selection shape it acts on.
/// One model shared by the configurable menu (which filters its rows by it), the palette,
/// and scripting / agents (which can ask whether an action applies before invoking it).
/// (Command registry P4.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuScope {
    /// Any context — global verbs (most [`Command`]s) and scene toggles.
    Always,
    /// The empty-canvas menu (no selection): "Add node", scene lenses.
    Canvas,
    /// Exactly one selected member (per-node gestures: Resize, Node settings).
    SingleNode,
    /// One or more selected members (Open tile / splits, Add tag, Isolate).
    Selection,
    /// Two or more selected members (Open in a stack, Relate).
    MultiNode,
}

impl MenuScope {
    /// Whether an action of this scope applies to a selection of `selection_len` members.
    pub fn applies(self, selection_len: usize) -> bool {
        match self {
            MenuScope::Always => true,
            MenuScope::Canvas => selection_len == 0,
            MenuScope::SingleNode => selection_len == 1,
            MenuScope::Selection => selection_len >= 1,
            MenuScope::MultiNode => selection_len >= 2,
        }
    }
}

/// The [`MenuScope`] of a catalog context action — the selection shape its menu row
/// belongs to. Reproduces the hand-written menu's per-shape placement. (Command registry P4.)
pub fn context_action_scope(action: crate::ContextAction) -> MenuScope {
    use crate::ContextAction::*;
    match action {
        AddNode | AddField | AddTile | AddSession | ShowAllNodes | MirrorTiles => MenuScope::Canvas,
        OpenNodeFacets | ResizeNode => MenuScope::SingleNode,
        OpenSplits | AddTag | IsolateSelection | ToggleSizeByDegree | ToggleSizeByImportance => {
            MenuScope::Selection
        }
        Stack | Relate => MenuScope::MultiNode,
        _ => MenuScope::Always,
    }
}

/// The [`MenuScope`] of any registry id (a [`Command`] verb or a context-action id), or
/// `None` if the id is unknown. The one applicability lookup the menu builder uses to
/// filter a configured id against the current selection. (Command registry P4.)
pub fn registry_scope(id: &str) -> Option<MenuScope> {
    if let Some(action) = context_action_from_id(id) {
        Some(context_action_scope(action))
    } else {
        Command::from_id(id).map(Command::menu_scope)
    }
}

/// The human label for any registry id (a [`Command`] verb or a context-action id), or `None`
/// if unknown. The menu-config editor renders add / remove rows with it. (Command registry P4.)
pub fn registry_label(id: &str) -> Option<&'static str> {
    if let Some(action) = context_action_from_id(id) {
        context_action_palette_label(action)
    } else {
        Command::from_id(id).map(Command::label)
    }
}

/// Every registry id — [`Command`] verbs then context-action ids — for the menu-config editor's
/// "add a command" list and any other full-registry enumeration. (Command registry P4.)
pub fn all_registry_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = Command::ALL.iter().map(|c| c.verb()).collect();
    ids.extend(PALETTE_CONTEXT_ACTIONS.iter().map(|(_, id, _)| *id));
    ids
}

pub const PALETTE_CONTEXT_ACTIONS: &[(crate::ContextAction, &str, &str)] = {
    use crate::ContextAction::*;
    &[
        (AddNode, "add_node", "Add node"),
        (AddTile, "add_tile", "Add tile"),
        (AddField, "add_field", "Add field"),
        (AddSession, "add_session", "Add graph session"),
        (OpenSplits, "open_splits", "Open selection in splits"),
        (Stack, "open_stack", "Open selection in a stack"),
        (AddTag, "add_tag", "Add tag to selection"),
        (ResizeNode, "resize_node", "Resize node (object card)"),
        (ToggleSizeByDegree, "size_by_degree", "Toggle size by degree"),
        (ToggleSizeByImportance, "size_by_importance", "Toggle size by importance"),
        (IsolateSelection, "isolate", "Isolate selection"),
        (ShowAllNodes, "show_all", "Show all nodes"),
        (MirrorTiles, "mirror_tiles", "Mirror open tiles"),
        (OpenNodeFacets, "open_node_facets", "Node settings (selected node)"),
    ]
};

/// The default context-menu command order (command registry P4): the registry ids the menu
/// shows out of the box, before the user curates it. Each is filtered by its [`registry_scope`]
/// at render, so this one ordered list reproduces the canvas / single-node / multi-node menus.
/// The user's curated list (persona-scoped) replaces this; resetting restores it.
pub const DEFAULT_MENU_ACTIONS: &[&str] = &[
    "add_node",         // canvas
    "add_field",        // canvas
    "open_splits",      // selection — "Open tile" (1) / "Open in splits" (2+)
    "open_node_facets", // single — Node settings
    "open_stack",       // multi — Open in a stack
    "relate",           // multi — pairwise
    "resize_node",      // single — Resize (object card)
    "add_tag",          // selection
    "size_by_degree",   // selection — scene toggle
    "size_by_importance", // selection — scene toggle (graph-signals importance)
    "isolate",          // selection
    "show_all",         // canvas — when a scope lens is active
    "mirror_tiles",     // canvas — when tiles are open
];

/// The palette label for a context action, or `None` if it is not palette-exposed. (P2.)
pub fn context_action_palette_label(action: crate::ContextAction) -> Option<&'static str> {
    PALETTE_CONTEXT_ACTIONS
        .iter()
        .find(|(a, _, _)| *a == action)
        .map(|(_, _, label)| *label)
}

/// The stable registry id for a palette-exposed context action, or `None`. The
/// context-action half of the registry's id space (`Command::verb` is the command half),
/// by which an agent / script / menu config addresses it. (Command registry P3.)
pub fn context_action_id(action: crate::ContextAction) -> Option<&'static str> {
    PALETTE_CONTEXT_ACTIONS
        .iter()
        .find(|(a, _, _)| *a == action)
        .map(|(_, id, _)| *id)
}

/// Resolve a context action from its registry id — the reverse of [`context_action_id`],
/// the context-action analogue of [`Command::from_id`]. Together they let any surface
/// (agent, script, a11y, menu config) invoke any registry action by one id space. (P3.)
pub fn context_action_from_id(id: &str) -> Option<crate::ContextAction> {
    PALETTE_CONTEXT_ACTIONS
        .iter()
        .find(|(_, i, _)| *i == id)
        .map(|(a, _, _)| *a)
}

/// The palette's unified item list for `query`: the matching commands (in `ALL` order)
/// followed by the matching palette context actions. The single registry-driven source
/// the palette renders, steps, and runs. (Command registry P2.)
pub fn palette_items(query: &str) -> Vec<PaletteItem> {
    let mut items: Vec<PaletteItem> =
        filter(query).into_iter().map(PaletteItem::Command).collect();
    for &(action, _id, label) in PALETTE_CONTEXT_ACTIONS {
        if label_matches(label, query) {
            items.push(PaletteItem::Context(action));
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all_in_order() {
        assert_eq!(filter(""), Command::ALL.to_vec());
        assert_eq!(filter("   "), Command::ALL.to_vec());
    }

    #[test]
    fn query_filters_by_label_substring() {
        assert_eq!(filter("for"), vec![Command::Forward]);
        assert_eq!(filter("home"), vec![Command::Home]);
        // Case-insensitive; "back" also matches "...active in background".
        assert_eq!(filter("BACK"), vec![Command::Back, Command::BackgroundNode]);
    }

    #[test]
    fn host_action_commands_filter_and_flag() {
        assert_eq!(filter("tile"), vec![Command::ToggleWorkbench]);
        assert_eq!(filter("delete"), vec![Command::DeleteNode]);
        assert!(Command::DeleteNode.is_host_action());
        assert!(Command::BackgroundNode.is_host_action());
        assert!(Command::RetryFocusedContent.is_host_action());
        assert!(Command::StopFocusedOperation.is_host_action());
        assert!(Command::PinFocusedOperation.is_host_action());
        assert!(
            !Command::Back.is_host_action(),
            "history verbs are not host actions"
        );
    }

    #[test]
    fn unmatched_query_is_empty() {
        assert!(filter("zzz").is_empty());
    }

    #[test]
    fn edge_commands_are_host_actions_with_verbs() {
        assert_eq!(Command::AssertEdge.verb(), "relate");
        assert_eq!(Command::RetractEdge.verb(), "unrelate");
        assert!(Command::AssertEdge.is_host_action());
        assert!(Command::RetractEdge.is_host_action());
        // The unambiguous label substring resolves; "relate" alone also matches
        // "Unrelate", so filter on the distinct token.
        assert_eq!(filter("unrelate"), vec![Command::RetractEdge]);
        assert!(filter("Relate selected").contains(&Command::AssertEdge));
    }

    #[test]
    fn from_id_round_trips_every_verb() {
        // The registry seam: every command resolves from its own id, and an unknown id
        // resolves to nothing. This is what lets a script / agent / a11y route name a
        // command by id and get the typed command back.
        for cmd in Command::ALL {
            assert_eq!(Command::from_id(cmd.verb()), Some(cmd), "{cmd:?} must round-trip");
        }
        assert_eq!(Command::from_id("not_a_command"), None);
        assert_eq!(Command::from_id(""), None);
    }

    #[test]
    fn command_entries_catalog_covers_all_with_unique_ids() {
        let entries = command_entries();
        assert_eq!(entries.len(), Command::ALL.len(), "the catalog lists every command");
        // ids are the verbs (already proven unique + identifier-safe), in ALL order.
        for (entry, cmd) in entries.iter().zip(Command::ALL) {
            assert_eq!(entry.id, cmd.verb());
            assert_eq!(entry.label, cmd.label());
            assert_eq!(entry.host_action, cmd.is_host_action());
            // every catalog id resolves back to its command (the registry round-trip).
            assert_eq!(Command::from_id(entry.id), Some(cmd));
        }
    }

    #[test]
    fn palette_items_unify_commands_and_context_actions() {
        let all = palette_items("");
        assert_eq!(
            all.iter().filter(|i| matches!(i, PaletteItem::Command(_))).count(),
            Command::ALL.len(),
            "every command is a palette item",
        );
        assert_eq!(
            all.iter().filter(|i| matches!(i, PaletteItem::Context(_))).count(),
            PALETTE_CONTEXT_ACTIONS.len(),
            "every palette-exposed context action is a palette item",
        );
        // A query filters context-action labels too, and every returned item matches it.
        let isolate = palette_items("isolate");
        assert!(
            isolate
                .iter()
                .any(|i| matches!(i, PaletteItem::Context(crate::ContextAction::IsolateSelection))),
            "the Isolate context action filters in",
        );
        assert!(isolate.iter().all(|i| label_matches(i.label(), "isolate")));
        // The catalog round-trips a palette-exposed action and excludes a deferred one.
        assert_eq!(
            context_action_palette_label(crate::ContextAction::ShowAllNodes),
            Some("Show all nodes"),
        );
        assert_eq!(context_action_palette_label(crate::ContextAction::CloseGraphPane), None);
    }

    #[test]
    fn registry_ids_are_unique_across_commands_and_context_actions() {
        // The registry is one id space: every command verb and every context-action id is
        // unique, and each context-action id round-trips. This is what lets an agent / script /
        // menu config name any action by one id namespace. (Command registry P3.)
        let mut seen = std::collections::HashSet::new();
        for cmd in Command::ALL {
            assert!(seen.insert(cmd.verb()), "duplicate registry id: {}", cmd.verb());
        }
        for &(action, id, _) in PALETTE_CONTEXT_ACTIONS {
            assert!(seen.insert(id), "context-action id collides with the registry: {id}");
            assert_eq!(context_action_from_id(id), Some(action), "{id} must round-trip");
            assert_eq!(context_action_id(action), Some(id));
        }
        assert_eq!(context_action_from_id("not_an_action"), None);
    }

    #[test]
    fn default_menu_actions_are_all_known_registry_ids() {
        // Every default-menu id resolves to a scope + a label (no dead ids), so the config-driven
        // menu builder never silently drops a default row. (Command registry P4.)
        for &id in DEFAULT_MENU_ACTIONS {
            assert!(registry_scope(id).is_some(), "default menu id has no scope: {id}");
            assert!(registry_label(id).is_some(), "default menu id has no label: {id}");
        }
    }

    #[test]
    fn menu_scope_filters_by_selection_shape() {
        assert!(MenuScope::Canvas.applies(0) && !MenuScope::Canvas.applies(1));
        assert!(MenuScope::SingleNode.applies(1) && !MenuScope::SingleNode.applies(2));
        assert!(MenuScope::Selection.applies(1) && MenuScope::Selection.applies(3));
        assert!(!MenuScope::Selection.applies(0));
        assert!(MenuScope::MultiNode.applies(2) && !MenuScope::MultiNode.applies(1));
        assert!(MenuScope::Always.applies(0));
        // The scopes that reproduce the hand-written menu's per-shape placement, plus a global
        // command (Always) and the AssertEdge ("relate") verb the menu's Relate row resolves to.
        assert_eq!(registry_scope("add_node"), Some(MenuScope::Canvas));
        assert_eq!(registry_scope("open_splits"), Some(MenuScope::Selection));
        assert_eq!(registry_scope("resize_node"), Some(MenuScope::SingleNode));
        assert_eq!(registry_scope("open_stack"), Some(MenuScope::MultiNode));
        assert_eq!(registry_scope("relate"), Some(MenuScope::MultiNode));
        assert_eq!(registry_scope("settings"), Some(MenuScope::Always));
        // Finer per-command scopes: a selection op, a single-focus op, a global toggle.
        assert_eq!(registry_scope("delete_node"), Some(MenuScope::Selection));
        assert_eq!(registry_scope("node_settings"), Some(MenuScope::SingleNode));
        assert_eq!(registry_scope("workbench"), Some(MenuScope::Always));
        assert_eq!(registry_scope("not_a_real_id"), None);
    }

    #[test]
    fn every_verb_is_a_unique_valid_identifier() {
        // The command shell registers one rhai function per verb over `ALL`, so a
        // verb must be a non-empty, unique, identifier-safe token (lowercase /
        // digits / `_`, not starting with a digit). This guards a new command from
        // silently colliding with or shadowing another's binding.
        let mut seen = std::collections::HashSet::new();
        for cmd in Command::ALL {
            let v = cmd.verb();
            assert!(!v.is_empty(), "{cmd:?} has an empty verb");
            assert!(
                !v.starts_with(|c: char| c.is_ascii_digit()),
                "{cmd:?} verb starts with a digit: {v}"
            );
            assert!(
                v.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{cmd:?} verb is not identifier-safe: {v}"
            );
            assert!(seen.insert(v), "duplicate verb: {v}");
        }
    }
}
