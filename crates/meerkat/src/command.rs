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
}

impl Command {
    /// Every command, in display order.
    pub const ALL: [Command; 9] = [
        Command::Back,
        Command::Forward,
        Command::Home,
        Command::ConnectPeer,
        Command::ToggleWorkbench,
        Command::DeleteNode,
        Command::BackgroundNode,
        Command::HideSelectedEdge,
        Command::ShowAllEdges,
    ];

    /// Whether this command is a *host* action (run by the shell over the graph /
    /// workbench / actors) rather than a chrome-level history verb. Host actions
    /// are recorded as a pending intent the host drains, like `ConnectPeer`.
    pub fn is_host_action(self) -> bool {
        matches!(
            self,
            Command::ToggleWorkbench
                | Command::DeleteNode
                | Command::BackgroundNode
                | Command::HideSelectedEdge
                | Command::ShowAllEdges
        )
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
        assert!(!Command::Back.is_host_action(), "history verbs are not host actions");
    }

    #[test]
    fn unmatched_query_is_empty() {
        assert!(filter("zzz").is_empty());
    }
}
