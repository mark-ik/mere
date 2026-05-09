/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable host-to-runtime intents.
//!
//! Host adapter files (`iced_app.rs`, `iced_host.rs`, etc.) are
//! forbidden by the sanctioned-writes contract (§12.17) from calling
//! the canonical typed-mutation entrypoints
//! (`apply_graph_delta_and_sync`, `apply_arrangement_snapshot`)
//! directly. The prescribed path is: hosts produce `HostIntent`s, the
//! runtime translates them to internal reducer actions during its
//! `tick` pass.
//!
//! `HostIntent` lives in `mere-kernel` so `FrameHostInput` (also
//! core) can carry a `Vec<HostIntent>`. This is deliberately a
//! **parallel portable enum** — not a move of the shell crate's
//! `GraphIntent` into core. Hosts only need a small surface of intent
//! variants (what the user can express through chrome); the larger
//! `GraphIntent` surface (with PendingTileOpenMode, workbench layout
//! commands, etc.) stays shell-side because it references types the
//! host doesn't need to know about.
//!
//! The runtime translates `HostIntent` → internal actions. Variants
//! whose internal equivalents haven't landed yet route through
//! whatever the runtime currently supports (e.g. `CreateNodeAtUrl`
//! goes through `GraphBrowserApp::add_node_and_sync`, the same path
//! the egui toolbar uses today but via the port contract instead of
//! a direct call).

use serde::{Deserialize, Serialize};

use mere_kernel::actions::ActionId;
use mere_kernel::geometry::PortablePoint;
use mere_kernel::graph::NodeKey;

/// Portable intent a host can push into `FrameHostInput.host_intents`
/// for the runtime to translate and apply during its tick.
///
/// Variants are added here only when a host has a real use case; the
/// enum intentionally stays small so the portable contract doesn't
/// drift toward mirroring the shell crate's full `GraphIntent`
/// surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HostIntent {
    /// User submitted a URL through chrome (toolbar, omnibar,
    /// command palette). Runtime creates a new graph node at the
    /// given world-space position.
    ///
    /// Position is typically `PortablePoint::origin()` — force-directed
    /// physics will reposition the node. Hosts that want to pin
    /// placement (e.g., drop-to-canvas gestures that know a target
    /// world coordinate) supply it explicitly.
    CreateNodeAtUrl {
        url: String,
        position: PortablePoint,
    },
    /// User picked a registered action from a command surface
    /// (Command Palette, Context Menu, future radial / hotkey path).
    /// The runtime resolves the `ActionId` against its dispatch table
    /// during the next tick.
    ///
    /// Hosts must only push this when the action's `is_available`
    /// predicate is satisfied; the runtime treats unknown / disabled
    /// dispatches as logged no-ops rather than panicking, so missing
    /// runtime handlers degrade gracefully while the dispatch table
    /// is being filled in incrementally.
    Action { action_id: ActionId },
    /// Variant of [`Self::Action`] that targets a specific node. Used
    /// by surfaces that know exactly which node the action applies to
    /// (e.g., a right-click context menu on a canvas node, or a Tree
    /// Spine row's inline action). The runtime sets
    /// `focused_node_hint = Some(node_key)` *before* running the
    /// per-action handler, so handlers that operate on focused
    /// selection (`NodePinToggle`, `NodeMarkTombstone`, etc.) act on
    /// the named node instead of whatever happened to be focused.
    ActionOnNode {
        action_id: ActionId,
        node_key: NodeKey,
    },
    /// User picked a node from a finder surface (Node Finder, future
    /// Tree Spine "reveal in workbench" row). The runtime promotes the
    /// node to focused state — concrete pane-routing semantics
    /// (active pane vs new pane vs replace focused pane) are picked
    /// up from the user's `WorkbenchProfile` once that surface lands;
    /// for now the runtime sets `focused_node_hint` so downstream
    /// systems (focus ring, tile activation) can react.
    OpenNode { node_key: NodeKey },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_node_at_url_serde_roundtrip() {
        let intent = HostIntent::CreateNodeAtUrl {
            url: "https://example.com/".to_string(),
            position: PortablePoint::new(0.0, 0.0),
        };
        let json = serde_json::to_string(&intent).expect("serialize");
        let back: HostIntent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(intent, back);
    }

    #[test]
    fn action_serde_roundtrip() {
        let intent = HostIntent::Action {
            action_id: ActionId::WorkbenchOpenSettingsPane,
        };
        let json = serde_json::to_string(&intent).expect("serialize");
        let back: HostIntent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(intent, back);
    }

    #[test]
    fn open_node_serde_roundtrip() {
        // NodeKey is petgraph::NodeIndex with the serde-1 feature.
        let intent = HostIntent::OpenNode {
            node_key: NodeKey::new(7),
        };
        let json = serde_json::to_string(&intent).expect("serialize");
        let back: HostIntent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(intent, back);
    }

    #[test]
    fn action_on_node_serde_roundtrip() {
        let intent = HostIntent::ActionOnNode {
            action_id: ActionId::NodePinToggle,
            node_key: NodeKey::new(3),
        };
        let json = serde_json::to_string(&intent).expect("serialize");
        let back: HostIntent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(intent, back);
    }
}
