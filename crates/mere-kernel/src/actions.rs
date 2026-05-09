/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable action catalogue.
//!
//! The shell's command surfaces (command palette, context palette,
//! radial palette) share a single enumerated vocabulary of actions
//! (`ActionId`) grouped into logical families (`ActionCategory`). The
//! pre-M4-slice-1 home for these types was `render::action_registry`
//! in the graphshell crate, which also carried host-coupled helpers
//! (input-binding lookup, action-registry-driven `ActionContext`
//! filtering). This module splits off the portable leaves so that
//! downstream portable state — `CommandPaletteSession`'s Tier 1
//! selection, the omnibar's action-match types, future iced-host
//! action-dispatch — can reference the vocabulary without pulling in
//! the host-side registry machinery.
//!
//! The render-side [`render::action_registry`] module re-exports the
//! symbols here for zero-churn at call sites; host-coupled helpers
//! (`ActionId::shortcut_hints`, the `ActionContext` / `ActionEntry`
//! filtering surface, the `list_actions_for_context` resolver) stay
//! alongside the re-exports.

use serde::{Deserialize, Serialize};

/// Preferred input mode, used as a layout hint by control surfaces.
///
/// `InputMode` is not a gate; surfaces may use it to choose their
/// default presentation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum InputMode {
    #[default]
    MouseKeyboard,
}

/// Logical grouping of actions, used for separators and ordering in
/// the command palette and as sector grouping in the radial palette.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ActionCategory {
    Node,
    Edge,
    Graph,
    Persistence,
}

/// Persistent-storage key for the recency ring used by palette ranking.
pub const CATEGORY_RECENCY_PERSIST_KEY: &str = "command_palette_category_recency";

/// Persistent-storage key for user-pinned category ordering.
pub const CATEGORY_PIN_ORDER_PERSIST_KEY: &str = "command_palette_category_pins";

impl ActionCategory {
    /// Display label for the category group heading.
    pub fn label(self) -> &'static str {
        match self {
            Self::Node => "Node",
            Self::Edge => "Edge",
            Self::Graph => "Graph",
            Self::Persistence => "Persistence",
        }
    }
}

/// Canonical base ordering for action categories in command surfaces.
/// Callers may re-rank based on context, recency, and user pinning;
/// this is the default when no other signal applies.
pub fn default_category_order() -> [ActionCategory; 4] {
    [
        ActionCategory::Node,
        ActionCategory::Edge,
        ActionCategory::Graph,
        ActionCategory::Persistence,
    ]
}

/// Stable machine-readable name for a category. Used when persisting
/// user preferences (pin order, recency ring) across sessions.
pub fn category_persisted_name(category: ActionCategory) -> &'static str {
    match category {
        ActionCategory::Node => "node",
        ActionCategory::Edge => "edge",
        ActionCategory::Graph => "graph",
        ActionCategory::Persistence => "persistence",
    }
}

/// Inverse of [`category_persisted_name`]. Returns `None` for unknown
/// names so callers can decide whether to fall back to a default or
/// surface a diagnostic.
pub fn category_from_persisted_name(name: &str) -> Option<ActionCategory> {
    match name {
        "node" => Some(ActionCategory::Node),
        "edge" => Some(ActionCategory::Edge),
        "graph" => Some(ActionCategory::Graph),
        "persistence" => Some(ActionCategory::Persistence),
        _ => None,
    }
}

/// Stable identifier for a registered action.
///
/// Each variant corresponds to one logical operation. The action
/// content (label, category, enabled state) is resolved at runtime
/// via the render-side `list_actions_for_context` resolver so that
/// control surfaces remain free of hardcoded dispatch tables.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ActionId {
    // Node actions
    NodeNew,
    NodeNewAsTab,
    NodePinToggle,
    NodePinSelected,
    NodeUnpinSelected,
    NodeDelete,
    NodeEditTags,
    NodeChooseFrame,
    NodeAddToFrame,
    NodeAddConnectedToFrame,
    NodeOpenFrame,
    NodeOpenNeighbors,
    NodeOpenConnected,
    NodeOpenSplit,
    NodeDetachToSplit,
    NodeMoveToActivePane,
    NodeWarmSelect,
    NodeRemoveFromGraphlet,
    NodeImportWebFinger,
    NodeResolveNip05,
    NodeResolveMatrix,
    NodeResolveActivityPub,
    NodeRefreshPersonIdentity,
    /// Soft-delete selected nodes → Ghost Node (Tombstone lifecycle).
    NodeMarkTombstone,
    NodeCopyUrl,
    NodeCopyTitle,
    NodeRenderAuto,
    NodeRenderWebView,
    NodeRenderWry,
    // Edge actions
    EdgeConnectPair,
    EdgeConnectBoth,
    EdgeRemoveUser,
    // Graph actions
    GraphFit,
    GraphFitGraphlet,
    GraphCycleFocusRegion,
    GraphToggleOverviewPlane,
    GraphTogglePhysics,
    /// Toggle per-view ghost node (tombstone) visibility.
    GraphToggleGhostNodes,
    GraphPhysicsConfig,
    GraphCommandPalette,
    GraphRadialMenu,
    WorkbenchToggleOverlay,
    FrameSelect,
    FrameOpen,
    FrameOpenAsSplit,
    FrameRename,
    FrameSettings,
    FrameSuppressSplitOffer,
    FrameDelete,
    FrameEnableSplitOffer,
    WorkbenchUnlockSurfaceLayout,
    WorkbenchLockSurfaceLayout,
    WorkbenchRememberLayoutPreference,
    WorkbenchGroupSelectedTiles,
    // Persistence actions
    PersistUndo,
    PersistRedo,
    PersistSaveSnapshot,
    PersistRestoreSession,
    PersistSaveGraph,
    PersistRestoreLatestGraph,
    PersistOpenHub,
    PersistImportBookmarks,
    WorkbenchOpenSettingsPane,
    WorkbenchOpenSettingsOverlay,
    PersistOpenHistoryManager,
    WorkbenchActivateWorkflowDefault,
    WorkbenchActivateWorkflowResearch,
    WorkbenchActivateWorkflowReading,
}

impl ActionId {
    /// Stable `namespace:name` identifier for persistence, logging,
    /// and diagnostic routing. Never changes for a given variant.
    pub fn key(self) -> &'static str {
        match self {
            Self::NodeNew => "node:new",
            Self::NodeNewAsTab => "node:new_as_tab",
            Self::NodePinToggle => "node:pin_toggle",
            Self::NodePinSelected => "node:pin_selected",
            Self::NodeUnpinSelected => "node:unpin_selected",
            Self::NodeDelete => "node:delete",
            Self::NodeEditTags => "node:edit_tags",
            Self::NodeChooseFrame => "node:choose_frame",
            Self::NodeAddToFrame => "node:add_to_frame",
            Self::NodeAddConnectedToFrame => "node:add_connected_to_frame",
            Self::NodeOpenFrame => "node:open_frame",
            Self::NodeOpenNeighbors => "node:open_neighbors",
            Self::NodeOpenConnected => "node:open_connected",
            Self::NodeOpenSplit => "node:open_split",
            Self::NodeDetachToSplit => "node:detach_to_split",
            Self::NodeMoveToActivePane => "node:move_to_active_pane",
            Self::NodeWarmSelect => "node:warm_select",
            Self::NodeRemoveFromGraphlet => "node:remove_from_graphlet",
            Self::NodeImportWebFinger => "node:import_webfinger",
            Self::NodeResolveNip05 => "node:resolve_nip05",
            Self::NodeResolveMatrix => "node:resolve_matrix",
            Self::NodeResolveActivityPub => "node:resolve_activitypub",
            Self::NodeRefreshPersonIdentity => "node:refresh_person_identity",
            Self::NodeMarkTombstone => "node:mark_tombstone",
            Self::NodeCopyUrl => "node:copy_url",
            Self::NodeCopyTitle => "node:copy_title",
            Self::NodeRenderAuto => "node:render_auto",
            Self::NodeRenderWebView => "node:render_webview",
            Self::NodeRenderWry => "node:render_wry",
            Self::EdgeConnectPair => "edge:connect_pair",
            Self::EdgeConnectBoth => "edge:connect_both",
            Self::EdgeRemoveUser => "edge:remove_user",
            Self::GraphFit => "graph:fit",
            Self::GraphFitGraphlet => "graph:fit_graphlet",
            Self::GraphCycleFocusRegion => "graph:cycle_focus_region",
            Self::GraphToggleOverviewPlane => "graph:toggle_overview_plane",
            Self::GraphTogglePhysics => "graph:toggle_physics",
            Self::GraphToggleGhostNodes => "graph:toggle_ghost_nodes",
            Self::GraphPhysicsConfig => "graph:physics_config",
            Self::GraphCommandPalette => "workbench:command_palette_open",
            Self::GraphRadialMenu => "workbench:radial_menu_open",
            Self::WorkbenchToggleOverlay => "workbench:toggle_workbench_overlay",
            Self::FrameSelect => "frame:select",
            Self::FrameOpen => "frame:open",
            Self::FrameOpenAsSplit => "frame:open_as_split",
            Self::FrameRename => "frame:rename",
            Self::FrameSettings => "frame:settings",
            Self::FrameSuppressSplitOffer => "frame:suppress_split_offer",
            Self::FrameDelete => "frame:delete",
            Self::FrameEnableSplitOffer => "frame:enable_split_offer",
            Self::WorkbenchUnlockSurfaceLayout => "workbench:unlock_surface_layout",
            Self::WorkbenchLockSurfaceLayout => "workbench:lock_surface_layout",
            Self::WorkbenchRememberLayoutPreference => "workbench:remember_layout_preference",
            Self::WorkbenchGroupSelectedTiles => "workbench:group_selected_tiles",
            Self::PersistUndo => "persistence:undo",
            Self::PersistRedo => "persistence:redo",
            Self::PersistSaveSnapshot => "persistence:save_snapshot",
            Self::PersistRestoreSession => "persistence:restore_session",
            Self::PersistSaveGraph => "persistence:save_graph",
            Self::PersistRestoreLatestGraph => "persistence:restore_latest_graph",
            Self::PersistOpenHub => "persistence:open_hub",
            Self::PersistImportBookmarks => "import:bookmarks_from_file",
            Self::WorkbenchOpenSettingsPane => "workbench:settings_pane",
            Self::WorkbenchOpenSettingsOverlay => "workbench:settings_overlay",
            Self::PersistOpenHistoryManager => "workbench:open_history_manager",
            Self::WorkbenchActivateWorkflowDefault => "workbench:activate_workflow_default",
            Self::WorkbenchActivateWorkflowResearch => "workbench:activate_workflow_research",
            Self::WorkbenchActivateWorkflowReading => "workbench:activate_workflow_reading",
        }
    }

    /// Short label suitable for a radial menu sector (≤ 12 chars).
    pub fn short_label(self) -> &'static str {
        match self {
            Self::NodeNew => "New",
            Self::NodeNewAsTab => "New Tab",
            Self::NodePinToggle => "Pin",
            Self::NodePinSelected => "Pin",
            Self::NodeUnpinSelected => "Unpin",
            Self::NodeDelete => "Delete",
            Self::NodeEditTags => "Tags",
            Self::NodeChooseFrame => "Choose F",
            Self::NodeAddToFrame => "Add F",
            Self::NodeAddConnectedToFrame => "Add Conn F",
            Self::NodeOpenFrame => "Frame",
            Self::NodeOpenNeighbors => "Neighbors",
            Self::NodeOpenConnected => "Connected",
            Self::NodeOpenSplit => "Split",
            Self::NodeDetachToSplit => "Detach",
            Self::NodeMoveToActivePane => "Move",
            Self::NodeWarmSelect => "Open Cold",
            Self::NodeRemoveFromGraphlet => "Leave Group",
            Self::NodeImportWebFinger => "WebFinger",
            Self::NodeResolveNip05 => "NIP-05",
            Self::NodeResolveMatrix => "Matrix",
            Self::NodeResolveActivityPub => "ActivityPub",
            Self::NodeRefreshPersonIdentity => "Refresh Identity",
            Self::NodeMarkTombstone => "Ghost",
            Self::NodeCopyUrl => "Copy URL",
            Self::NodeCopyTitle => "Copy Title",
            Self::NodeRenderAuto => "Auto",
            Self::NodeRenderWebView => "WebView",
            Self::NodeRenderWry => "Wry",
            Self::EdgeConnectPair => "Pair",
            Self::EdgeConnectBoth => "Both",
            Self::EdgeRemoveUser => "Remove",
            Self::GraphFit => "Fit",
            Self::GraphFitGraphlet => "Fit Graphlet",
            Self::GraphCycleFocusRegion => "Focus",
            Self::GraphToggleOverviewPlane => "Overview",
            Self::GraphTogglePhysics => "Physics",
            Self::GraphToggleGhostNodes => "Ghosts",
            Self::GraphPhysicsConfig => "Config",
            Self::GraphCommandPalette => "Cmd",
            Self::GraphRadialMenu => "Radial",
            Self::WorkbenchToggleOverlay => "Workbench Ovl",
            Self::FrameSelect => "Select F",
            Self::FrameOpen => "Open F",
            Self::FrameOpenAsSplit => "Split F",
            Self::FrameRename => "Rename F",
            Self::FrameSettings => "F Settings",
            Self::FrameSuppressSplitOffer => "Suppress",
            Self::FrameDelete => "Delete F",
            Self::FrameEnableSplitOffer => "Re-enable",
            Self::WorkbenchUnlockSurfaceLayout => "Unlock",
            Self::WorkbenchLockSurfaceLayout => "Lock",
            Self::WorkbenchRememberLayoutPreference => "Remember",
            Self::WorkbenchGroupSelectedTiles => "Group Tiles",
            Self::PersistUndo => "Undo",
            Self::PersistRedo => "Redo",
            Self::PersistSaveSnapshot => "Save W",
            Self::PersistRestoreSession => "Restore W",
            Self::PersistSaveGraph => "Save G",
            Self::PersistRestoreLatestGraph => "Latest G",
            Self::PersistOpenHub => "Persist Ovl",
            Self::PersistImportBookmarks => "Import Bm",
            Self::WorkbenchOpenSettingsPane => "Set Pane",
            Self::WorkbenchOpenSettingsOverlay => "Set Ovl",
            Self::PersistOpenHistoryManager => "History",
            Self::WorkbenchActivateWorkflowDefault => "Workflow D",
            Self::WorkbenchActivateWorkflowResearch => "Workflow R",
            Self::WorkbenchActivateWorkflowReading => "Workflow Read",
        }
    }

    /// Full label suitable for the command palette list.
    pub fn label(self) -> &'static str {
        match self {
            Self::NodeNew => "Create Node",
            Self::NodeNewAsTab => "Create Node as Tab",
            Self::NodePinToggle => "Toggle Pin",
            Self::NodePinSelected => "Pin Selected",
            Self::NodeUnpinSelected => "Unpin Selected",
            Self::NodeDelete => "Delete Selected Node(s)",
            Self::NodeEditTags => "Edit Tags...",
            Self::NodeChooseFrame => "Choose Frame...",
            Self::NodeAddToFrame => "Add To Frame...",
            Self::NodeAddConnectedToFrame => "Add Connected To Frame...",
            Self::NodeOpenFrame => "Open via Frame Route",
            Self::NodeOpenNeighbors => "Open with Neighbors",
            Self::NodeOpenConnected => "Open with Connected",
            Self::NodeOpenSplit => "Open Node in Split",
            Self::NodeDetachToSplit => "Detach Focused to Split",
            Self::NodeMoveToActivePane => "Move Node to Active Pane",
            Self::NodeWarmSelect => "Open Cold Selection as Tiles",
            Self::NodeRemoveFromGraphlet => "Remove from Graphlet",
            Self::NodeImportWebFinger => "Import WebFinger Discovery",
            Self::NodeResolveNip05 => "Resolve NIP-05 Identity",
            Self::NodeResolveMatrix => "Resolve Matrix Profile",
            Self::NodeResolveActivityPub => "Import ActivityPub Actor",
            Self::NodeRefreshPersonIdentity => "Refresh Person Identity",
            Self::NodeMarkTombstone => "Ghost Selected Node(s)",
            Self::NodeCopyUrl => "Copy Node URL",
            Self::NodeCopyTitle => "Copy Node Title",
            Self::NodeRenderAuto => "Render With Auto",
            Self::NodeRenderWebView => "Render With WebView",
            Self::NodeRenderWry => "Render With Wry",
            Self::EdgeConnectPair => "Connect Source -> Target",
            Self::EdgeConnectBoth => "Connect Both Directions",
            Self::EdgeRemoveUser => "Remove User Edge",
            Self::GraphFit => "Fit Graph to Screen",
            Self::GraphFitGraphlet => "Fit Graphlet to Screen",
            Self::GraphCycleFocusRegion => "Cycle Focus Region",
            Self::GraphToggleOverviewPlane => "Toggle Overview Plane",
            Self::GraphTogglePhysics => "Toggle Physics Simulation",
            Self::GraphToggleGhostNodes => "Toggle Ghost Node Visibility",
            Self::GraphPhysicsConfig => "Open Physics Settings",
            Self::GraphCommandPalette => "Open Command Palette",
            Self::GraphRadialMenu => "Open Radial Palette",
            Self::WorkbenchToggleOverlay => "Toggle Workbench Overlay",
            Self::FrameSelect => "Select Frame",
            Self::FrameOpen => "Open Frame",
            Self::FrameOpenAsSplit => "Open Frame As Split",
            Self::FrameRename => "Rename Frame",
            Self::FrameSettings => "Open Frame Settings",
            Self::FrameSuppressSplitOffer => "Suppress Split Offer",
            Self::FrameDelete => "Delete Frame",
            Self::FrameEnableSplitOffer => "Re-enable Split Offer",
            Self::WorkbenchUnlockSurfaceLayout => "Unlock Surface Layout",
            Self::WorkbenchLockSurfaceLayout => "Lock Surface Layout",
            Self::WorkbenchRememberLayoutPreference => "Remember Layout Preference",
            Self::WorkbenchGroupSelectedTiles => "Group Selected Tiles",
            Self::PersistUndo => "Undo",
            Self::PersistRedo => "Redo",
            Self::PersistSaveSnapshot => "Save Frame Snapshot",
            Self::PersistRestoreSession => "Restore Session Frame",
            Self::PersistSaveGraph => "Save Graph Snapshot",
            Self::PersistRestoreLatestGraph => "Restore Latest Graph",
            Self::PersistOpenHub => "Open Persistence Overlay",
            Self::PersistImportBookmarks => "Import Browser Bookmarks...",
            Self::WorkbenchOpenSettingsPane => "Open Settings Pane",
            Self::WorkbenchOpenSettingsOverlay => "Open Settings Overlay",
            Self::PersistOpenHistoryManager => "Open History Manager",
            Self::WorkbenchActivateWorkflowDefault => "Activate Default Workflow",
            Self::WorkbenchActivateWorkflowResearch => "Activate Research Workflow",
            Self::WorkbenchActivateWorkflowReading => "Activate Reading Workflow",
        }
    }

    /// Logical category for grouping in command surfaces.
    pub fn category(self) -> ActionCategory {
        match self {
            Self::NodeNew
            | Self::NodeNewAsTab
            | Self::NodePinToggle
            | Self::NodePinSelected
            | Self::NodeUnpinSelected
            | Self::NodeDelete
            | Self::NodeEditTags
            | Self::NodeChooseFrame
            | Self::NodeAddToFrame
            | Self::NodeAddConnectedToFrame
            | Self::NodeOpenFrame
            | Self::NodeOpenNeighbors
            | Self::NodeOpenConnected
            | Self::NodeOpenSplit
            | Self::NodeDetachToSplit
            | Self::NodeMoveToActivePane
            | Self::NodeCopyUrl
            | Self::NodeCopyTitle
            | Self::NodeRenderAuto
            | Self::NodeRenderWebView
            | Self::NodeRenderWry
            | Self::NodeWarmSelect
            | Self::NodeRemoveFromGraphlet
            | Self::NodeImportWebFinger
            | Self::NodeResolveNip05
            | Self::NodeResolveMatrix
            | Self::NodeResolveActivityPub
            | Self::NodeRefreshPersonIdentity
            | Self::NodeMarkTombstone => ActionCategory::Node,
            Self::EdgeConnectPair | Self::EdgeConnectBoth | Self::EdgeRemoveUser => {
                ActionCategory::Edge
            }
            Self::GraphFit
            | Self::GraphFitGraphlet
            | Self::GraphCycleFocusRegion
            | Self::GraphToggleOverviewPlane
            | Self::GraphTogglePhysics
            | Self::GraphToggleGhostNodes
            | Self::GraphPhysicsConfig
            | Self::GraphCommandPalette
            | Self::GraphRadialMenu
            | Self::WorkbenchToggleOverlay
            | Self::FrameSelect
            | Self::FrameOpen
            | Self::FrameOpenAsSplit
            | Self::FrameRename
            | Self::FrameSettings
            | Self::FrameSuppressSplitOffer
            | Self::FrameDelete
            | Self::FrameEnableSplitOffer
            | Self::WorkbenchUnlockSurfaceLayout
            | Self::WorkbenchLockSurfaceLayout
            | Self::WorkbenchRememberLayoutPreference
            | Self::WorkbenchGroupSelectedTiles => ActionCategory::Graph,
            Self::PersistUndo
            | Self::PersistRedo
            | Self::PersistSaveSnapshot
            | Self::PersistRestoreSession
            | Self::PersistSaveGraph
            | Self::PersistRestoreLatestGraph
            | Self::PersistOpenHub
            | Self::PersistImportBookmarks
            | Self::WorkbenchOpenSettingsPane
            | Self::WorkbenchOpenSettingsOverlay
            | Self::PersistOpenHistoryManager
            | Self::WorkbenchActivateWorkflowDefault
            | Self::WorkbenchActivateWorkflowResearch
            | Self::WorkbenchActivateWorkflowReading => ActionCategory::Persistence,
        }
    }
}

/// Every `ActionId` variant. Useful for generators (settings UI that
/// lists every keybinding, doc exporters, consistency audits) and
/// the render-side dispatch table that wires each variant's enabled
/// predicate into the palette context.
///
/// Returning `&'static [ActionId]` keeps this zero-allocation and
/// embeddable in `const` contexts.
pub fn all_action_ids() -> &'static [ActionId] {
    use ActionId::*;
    &[
        NodeNew,
        NodeNewAsTab,
        NodePinToggle,
        NodePinSelected,
        NodeUnpinSelected,
        NodeDelete,
        NodeEditTags,
        NodeChooseFrame,
        NodeAddToFrame,
        NodeAddConnectedToFrame,
        NodeOpenFrame,
        NodeOpenNeighbors,
        NodeOpenConnected,
        NodeOpenSplit,
        NodeDetachToSplit,
        NodeMoveToActivePane,
        NodeCopyUrl,
        NodeCopyTitle,
        NodeRenderAuto,
        NodeRenderWebView,
        NodeRenderWry,
        NodeWarmSelect,
        NodeRemoveFromGraphlet,
        NodeImportWebFinger,
        NodeResolveNip05,
        NodeResolveMatrix,
        NodeResolveActivityPub,
        NodeRefreshPersonIdentity,
        NodeMarkTombstone,
        EdgeConnectPair,
        EdgeConnectBoth,
        EdgeRemoveUser,
        GraphFit,
        GraphFitGraphlet,
        GraphCycleFocusRegion,
        GraphToggleOverviewPlane,
        GraphTogglePhysics,
        GraphToggleGhostNodes,
        GraphPhysicsConfig,
        GraphCommandPalette,
        GraphRadialMenu,
        WorkbenchToggleOverlay,
        FrameSelect,
        FrameOpen,
        FrameOpenAsSplit,
        FrameRename,
        FrameSettings,
        FrameSuppressSplitOffer,
        FrameDelete,
        FrameEnableSplitOffer,
        WorkbenchUnlockSurfaceLayout,
        WorkbenchLockSurfaceLayout,
        WorkbenchRememberLayoutPreference,
        WorkbenchGroupSelectedTiles,
        PersistUndo,
        PersistRedo,
        PersistSaveSnapshot,
        PersistRestoreSession,
        PersistSaveGraph,
        PersistRestoreLatestGraph,
        PersistOpenHub,
        PersistImportBookmarks,
        WorkbenchOpenSettingsPane,
        WorkbenchOpenSettingsOverlay,
        PersistOpenHistoryManager,
        WorkbenchActivateWorkflowDefault,
        WorkbenchActivateWorkflowResearch,
        WorkbenchActivateWorkflowReading,
    ]
}

/// `true` when `id` matches the `namespace:name` key format the
/// registry enforces for cross-surface routing. Both `namespace` and
/// `name` must be lowercase ASCII alphanumerics plus `_`; exactly one
/// `:` separator; no empty parts.
pub fn action_id_has_namespace_format(id: &str) -> bool {
    let mut parts = id.split(':');
    let Some(namespace) = parts.next() else {
        return false;
    };
    let Some(name) = parts.next() else {
        return false;
    };
    if parts.next().is_some() || namespace.is_empty() || name.is_empty() {
        return false;
    }
    namespace
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn action_category_labels_are_stable() {
        assert_eq!(ActionCategory::Node.label(), "Node");
        assert_eq!(ActionCategory::Edge.label(), "Edge");
        assert_eq!(ActionCategory::Graph.label(), "Graph");
        assert_eq!(ActionCategory::Persistence.label(), "Persistence");
    }

    #[test]
    fn category_persisted_name_round_trips_all_variants() {
        for category in default_category_order() {
            let name = category_persisted_name(category);
            assert_eq!(
                category_from_persisted_name(name),
                Some(category),
                "round-trip for {category:?} (name={name})"
            );
        }
    }

    #[test]
    fn category_from_persisted_name_rejects_unknown() {
        assert_eq!(category_from_persisted_name(""), None);
        assert_eq!(category_from_persisted_name("Node"), None); // case-sensitive
        assert_eq!(category_from_persisted_name("nodes"), None); // plural
    }

    #[test]
    fn default_category_order_contains_each_category_exactly_once() {
        let order = default_category_order();
        let set: HashSet<_> = order.iter().copied().collect();
        assert_eq!(set.len(), order.len());
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn every_action_id_has_a_category() {
        // Pinned invariant: `ActionId::category()` covers every
        // variant. If a new variant is added without a category
        // arm, this test fails at the moment `all_action_ids` is
        // updated — catches the mistake in core's own suite before
        // it reaches the render-side resolver.
        for id in all_action_ids() {
            let _ = id.category();
        }
    }

    #[test]
    fn every_action_id_key_is_namespace_format() {
        for id in all_action_ids() {
            let key = id.key();
            assert!(
                action_id_has_namespace_format(key),
                "action id {id:?} has key {key:?} which violates the `namespace:name` format"
            );
        }
    }

    #[test]
    fn action_id_keys_are_unique() {
        // If two variants map to the same key, runtime dispatch
        // tables collapse and a user action routes to the wrong
        // handler. Pin the uniqueness invariant.
        let mut seen = HashSet::new();
        for id in all_action_ids() {
            let key = id.key();
            assert!(
                seen.insert(key),
                "duplicate action key {key:?} — two ActionId variants map to the same key"
            );
        }
    }

    #[test]
    fn action_id_labels_are_non_empty() {
        for id in all_action_ids() {
            assert!(!id.label().is_empty(), "ActionId {id:?} has an empty label");
            assert!(
                !id.short_label().is_empty(),
                "ActionId {id:?} has an empty short_label"
            );
        }
    }

    #[test]
    fn action_id_labels_differ_from_short_labels() {
        // Not always — a few ActionIds use the same text in both
        // labels. Just spot-check a couple that definitely differ so
        // a silent regression to "all short_labels == label" would
        // surface.
        assert_ne!(
            ActionId::NodeNewAsTab.label(),
            ActionId::NodeNewAsTab.short_label()
        );
        assert_ne!(
            ActionId::GraphToggleOverviewPlane.label(),
            ActionId::GraphToggleOverviewPlane.short_label()
        );
    }

    #[test]
    fn action_id_has_namespace_format_rejects_malformed_inputs() {
        assert!(!action_id_has_namespace_format(""));
        assert!(!action_id_has_namespace_format("no_colon"));
        assert!(!action_id_has_namespace_format(":missing_namespace"));
        assert!(!action_id_has_namespace_format("missing_name:"));
        assert!(!action_id_has_namespace_format("too:many:colons"));
        assert!(!action_id_has_namespace_format("BadCase:name")); // uppercase namespace
        assert!(!action_id_has_namespace_format("ns:Name")); // uppercase name
        assert!(!action_id_has_namespace_format("ns:na-me")); // hyphen not allowed
    }

    #[test]
    fn action_id_has_namespace_format_accepts_valid_inputs() {
        assert!(action_id_has_namespace_format("node:new"));
        assert!(action_id_has_namespace_format("graph:fit_graphlet"));
        assert!(action_id_has_namespace_format("ns2:name3"));
        assert!(action_id_has_namespace_format("a:b"));
    }

    #[test]
    fn action_id_serde_json_round_trips_a_sample() {
        // ActionId variants are unit-like; serde_json emits them as
        // JSON strings matching the variant name. Pin a handful of
        // variants so a change to the serde derive is noticed.
        for id in [
            ActionId::NodeNew,
            ActionId::EdgeConnectPair,
            ActionId::GraphTogglePhysics,
            ActionId::PersistUndo,
        ] {
            let encoded = serde_json::to_string(&id).expect("serialize");
            let decoded: ActionId = serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(decoded, id);
        }
    }
}
