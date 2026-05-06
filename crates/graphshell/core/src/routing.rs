/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable return-target classification used by focus-restore and
//! command-surface routing.
//!
//! [`ToolSurfaceReturnTarget`] identifies *where* to restore focus /
//! visibility when an overlay (command palette, radial menu, etc.)
//! dismisses: back to a graph view, a node pane, or a specific tool
//! surface. The runtime stores these in `FocusAuthorityMut`,
//! `ReturnAnchor`, and `FocusCaptureEntry` so chrome dismissal routes
//! are carried across the update-frame pipeline without the host
//! having to re-derive the caller's origin.
//!
//! Pre-M4 slice 10 (2026-04-22) this enum lived in `app/routing.rs`
//! (the shell-side app crate); moved here so focus-authority bundles
//! + view-model types can be fully portable.

use serde::{Deserialize, Serialize};

use crate::graph::{GraphViewId, NodeKey};
use crate::pane::ToolPaneState;

/// Where focus / visibility should return after an overlay dismisses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolSurfaceReturnTarget {
    /// Restore focus to a graph-view pane identified by its stable
    /// `GraphViewId`.
    Graph(GraphViewId),
    /// Restore focus to a node pane identified by its `NodeKey`.
    Node(NodeKey),
    /// Restore focus to a tool pane of the given kind (Settings,
    /// History, Navigator, etc.).
    Tool(ToolPaneState),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_surface_return_target_variants_distinct() {
        let a = ToolSurfaceReturnTarget::Tool(ToolPaneState::Diagnostics);
        let b = ToolSurfaceReturnTarget::Tool(ToolPaneState::Settings);
        assert_ne!(a, b);
    }

    #[test]
    fn tool_surface_return_target_serde_tool_variant() {
        // Tool(ToolPaneState) persists in session-restore state; pin
        // the wire shape so a change in enum order doesn't break
        // existing sessions.
        let target = ToolSurfaceReturnTarget::Tool(ToolPaneState::FileTree);
        let json = serde_json::to_string(&target).unwrap();
        let decoded: ToolSurfaceReturnTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, target);
    }

    #[test]
    fn tool_surface_return_target_node_variant_roundtrips_nodekey() {
        let target = ToolSurfaceReturnTarget::Node(NodeKey::new(42));
        let json = serde_json::to_string(&target).unwrap();
        let decoded: ToolSurfaceReturnTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, target);
    }
}
