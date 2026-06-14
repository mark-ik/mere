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
    /// Open the settings overlay. A chrome-level action (opens the panel right
    /// there, like toggling the palette), not a host intent.
    OpenSettings,
    /// Toggle the docked comms pane (conversations: misfin mail + murm cabals). A
    /// chrome-level action, like opening settings.
    ToggleComms,
    /// Toggle the roster pane (graph manifest: nodes/edges/fields) (host action).
    ToggleRoster,
    /// Toggle the gloss / Navigator pane (host action).
    ToggleGloss,
    /// Toggle the apparatus pane (host diagnostics + settings) (host action).
    ToggleApparatus,
    /// Toggle the selected-object inspector pane (host action).
    ToggleInspector,
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
}

impl Command {
    /// Every command, in display order.
    pub const ALL: [Command; 23] = [
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
        Command::OpenSettings,
        Command::ToggleComms,
        Command::ToggleInspector,
        Command::ToggleSteward,
        Command::RetryFocusedContent,
        Command::StopFocusedOperation,
        Command::PinFocusedOperation,
        Command::ToggleCompatView,
        Command::AssertEdge,
        Command::RetractEdge,
        Command::CloseGraphPane,
    ];

    /// Whether this command is a *host* action (run by the shell over the graph /
    /// workbench / actors) rather than a chrome-level history verb. Host actions
    /// are recorded as a pending intent the host drains, like `ConnectPeer`.
    pub fn is_host_action(self) -> bool {
        matches!(
            self,
            Command::ToggleWorkbench
                | Command::ToggleRoster
                | Command::ToggleGloss
                | Command::ToggleApparatus
                | Command::DeleteNode
                | Command::BackgroundNode
                | Command::HideSelectedEdge
                | Command::ShowAllEdges
                | Command::ToggleInspector
                | Command::ToggleSteward
                | Command::RetryFocusedContent
                | Command::StopFocusedOperation
                | Command::PinFocusedOperation
                | Command::ToggleCompatView
                | Command::AssertEdge
                | Command::RetractEdge
                | Command::CloseGraphPane
        )
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
            Command::ToggleSteward => "steward",
            Command::ToggleComms => "comms",
            Command::OpenSettings => "settings",
            Command::ToggleCompatView => "compat_view",
            Command::DeleteNode => "delete_node",
            Command::BackgroundNode => "background_node",
            Command::HideSelectedEdge => "hide_edge",
            Command::ShowAllEdges => "show_all_edges",
            Command::RetryFocusedContent => "retry",
            Command::StopFocusedOperation => "stop",
            Command::PinFocusedOperation => "pin",
            Command::AssertEdge => "relate",
            Command::RetractEdge => "unrelate",
            Command::CloseGraphPane => "close_pane",
        }
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
            Command::OpenSettings => "Settings",
            Command::ToggleComms => "Comms (conversations)",
            Command::ToggleRoster => "Roster (graph manifest)",
            Command::ToggleGloss => "Gloss (navigator / map)",
            Command::ToggleApparatus => "Apparatus (diagnostics + settings)",
            Command::ToggleInspector => "Inspector (selected object)",
            Command::ToggleSteward => "Steward (live operations)",
            Command::RetryFocusedContent => "Retry focused content",
            Command::StopFocusedOperation => "Stop focused operation",
            Command::PinFocusedOperation => "Pin focused operation",
            Command::ToggleCompatView => "Compatibility view (system WebView, focused node)",
            Command::AssertEdge => "Relate selected nodes",
            Command::RetractEdge => "Unrelate selected edge",
            Command::CloseGraphPane => "Close graph view (focused pane)",
        }
    }
}

/// The commands whose label contains `query` (case-insensitive, whitespace
/// trimmed). An empty query returns every command in [`Command::ALL`] order.
pub fn filter(query: &str) -> Vec<Command> {
    let needle = query.trim().to_lowercase();
    Command::ALL
        .iter()
        .copied()
        .filter(|cmd| needle.is_empty() || cmd.label().to_lowercase().contains(&needle))
        .collect()
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
