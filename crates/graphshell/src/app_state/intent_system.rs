/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure reducer intent vocabulary for `GraphWorkspace`.
//!
//! The donor `intent_system` module is still coupled to `GraphBrowserApp`,
//! service handles, and host workbench types. This module is the first portable
//! subset: intents either mutate reducer-owned state directly or enqueue typed
//! effects for composition glue to consume.

use forme::ProjectionLens;
use inker::routing::EngineRouteRequest;
use kernel::graph::{GraphViewId, NodeKey};

use super::{
    ChromeState, DiagnosticRecord, FrameId, FrameState, GraphWorkspace, SurfaceCommand,
    WorkspaceEffect, WorkspaceId, WorkspacePreferences,
};

/// Pure app-state intent accepted by [`reduce_workspace_intent`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceIntent {
    EnsureGraphView {
        view_id: GraphViewId,
    },
    FocusGraphView {
        view_id: GraphViewId,
    },
    SetGraphViewLens {
        view_id: GraphViewId,
        lens: ProjectionLens,
    },
    SetGraphViewActiveNode {
        view_id: GraphViewId,
        node: Option<NodeKey>,
    },
    UpsertFrame {
        frame: FrameState,
    },
    ActivateFrame {
        frame_id: FrameId,
    },
    SetWorkspaceDirty {
        dirty: bool,
    },
    ReplaceChromeState {
        chrome: ChromeState,
    },
    SetCommandPaletteOpen {
        open: bool,
    },
    UpdatePreferences {
        preferences: WorkspacePreferences,
    },
    RequestPersist {
        workspace_id: WorkspaceId,
    },
    RequestEngineRoute {
        request: EngineRouteRequest,
    },
    RequestSurface {
        command: SurfaceCommand,
    },
    EmitDiagnostic {
        record: DiagnosticRecord,
    },
}

/// Summary of one reducer application.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReducerOutcome {
    pub state_changed: bool,
    pub effects_emitted: usize,
}

/// Reducer validation error. Effects and concrete services are intentionally
/// absent from this error type so tests can stay pure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReducerError {
    MissingGraphView(GraphViewId),
    MissingFrame(FrameId),
}

pub type ReducerResult = Result<ReducerOutcome, ReducerError>;

/// Apply one portable intent to reducer-owned workspace state.
pub fn reduce_workspace_intent(
    workspace: &mut GraphWorkspace,
    intent: WorkspaceIntent,
) -> ReducerResult {
    match intent {
        WorkspaceIntent::EnsureGraphView { view_id } => {
            let existed = workspace.views.graph_views.contains_key(&view_id);
            workspace.views.ensure_view(view_id);
            Ok(ReducerOutcome {
                state_changed: !existed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::FocusGraphView { view_id } => {
            if !workspace.views.graph_views.contains_key(&view_id) {
                return Err(ReducerError::MissingGraphView(view_id));
            }
            let changed = workspace.views.focused_view != Some(view_id);
            workspace.views.focused_view = Some(view_id);
            Ok(ReducerOutcome {
                state_changed: changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::SetGraphViewLens { view_id, lens } => {
            let view = workspace
                .views
                .graph_views
                .get_mut(&view_id)
                .ok_or(ReducerError::MissingGraphView(view_id))?;
            let changed = view.projection.active_lens != lens;
            view.projection.active_lens = lens;
            Ok(ReducerOutcome {
                state_changed: changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::SetGraphViewActiveNode { view_id, node } => {
            let view = workspace
                .views
                .graph_views
                .get_mut(&view_id)
                .ok_or(ReducerError::MissingGraphView(view_id))?;
            let changed = view.projection.active_node != node;
            view.projection.active_node = node;
            Ok(ReducerOutcome {
                state_changed: changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::UpsertFrame { frame } => {
            let changed = workspace.workbench.frames.get(&frame.id) != Some(&frame);
            workspace.workbench.frames.insert(frame.id.clone(), frame);
            Ok(ReducerOutcome {
                state_changed: changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::ActivateFrame { frame_id } => {
            if !workspace.workbench.frames.contains_key(&frame_id) {
                return Err(ReducerError::MissingFrame(frame_id));
            }
            let changed = workspace.workbench.active_frame.as_ref() != Some(&frame_id);
            workspace.workbench.active_frame = Some(frame_id);
            Ok(ReducerOutcome {
                state_changed: changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::SetWorkspaceDirty { dirty } => {
            let changed = workspace.workbench.has_unsaved_changes != dirty;
            workspace.workbench.has_unsaved_changes = dirty;
            Ok(ReducerOutcome {
                state_changed: changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::ReplaceChromeState { chrome } => {
            let changed = workspace.chrome != chrome;
            workspace.chrome = chrome;
            Ok(ReducerOutcome {
                state_changed: changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::SetCommandPaletteOpen { open } => {
            let flag_changed = workspace.chrome.command_palette_open != open;
            let app_ux_changed =
                super::app_ux::set_legacy_command_palette_visibility(workspace, open);
            Ok(ReducerOutcome {
                state_changed: flag_changed || app_ux_changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::UpdatePreferences { preferences } => {
            let changed = workspace.chrome.preferences != preferences;
            workspace.chrome.preferences = preferences;
            Ok(ReducerOutcome {
                state_changed: changed,
                effects_emitted: 0,
            })
        }
        WorkspaceIntent::RequestPersist { workspace_id } => {
            workspace.push_effect(WorkspaceEffect::PersistWorkspace { workspace_id });
            Ok(ReducerOutcome {
                state_changed: false,
                effects_emitted: 1,
            })
        }
        WorkspaceIntent::RequestEngineRoute { request } => {
            workspace.push_effect(WorkspaceEffect::RouteEngine(request));
            Ok(ReducerOutcome {
                state_changed: false,
                effects_emitted: 1,
            })
        }
        WorkspaceIntent::RequestSurface { command } => {
            workspace.push_effect(WorkspaceEffect::RequestSurface(command));
            Ok(ReducerOutcome {
                state_changed: false,
                effects_emitted: 1,
            })
        }
        WorkspaceIntent::EmitDiagnostic { record } => {
            workspace.push_effect(WorkspaceEffect::EmitDiagnostic(record));
            Ok(ReducerOutcome {
                state_changed: false,
                effects_emitted: 1,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use kernel::graph::GraphViewId;

    use super::*;
    use crate::app_state::{NavigatorSidebarPreference, ThemeModePreference};

    fn view_id(value: u128) -> GraphViewId {
        GraphViewId::from_uuid(uuid::Uuid::from_u128(value))
    }

    #[test]
    fn ensure_then_focus_graph_view_mutates_only_state() {
        let mut workspace = GraphWorkspace::new();
        let view_id = view_id(1);

        let ensure =
            reduce_workspace_intent(&mut workspace, WorkspaceIntent::EnsureGraphView { view_id })
                .unwrap();
        let focus =
            reduce_workspace_intent(&mut workspace, WorkspaceIntent::FocusGraphView { view_id })
                .unwrap();

        assert!(ensure.state_changed);
        assert!(focus.state_changed);
        assert_eq!(workspace.views.focused_view, Some(view_id));
        assert!(workspace.pending_effects.effects.is_empty());
    }

    #[test]
    fn focus_missing_view_is_rejected_without_creating_state() {
        let mut workspace = GraphWorkspace::new();
        let view_id = view_id(2);

        let error =
            reduce_workspace_intent(&mut workspace, WorkspaceIntent::FocusGraphView { view_id })
                .unwrap_err();

        assert_eq!(error, ReducerError::MissingGraphView(view_id));
        assert!(workspace.views.graph_views.is_empty());
    }

    #[test]
    fn update_preferences_keeps_settings_inside_chrome_state() {
        let mut workspace = GraphWorkspace::new();
        let preferences = WorkspacePreferences {
            theme_mode: ThemeModePreference::Dark,
            navigator_sidebar: NavigatorSidebarPreference::End,
        };

        let outcome = reduce_workspace_intent(
            &mut workspace,
            WorkspaceIntent::UpdatePreferences {
                preferences: preferences.clone(),
            },
        )
        .unwrap();

        assert!(outcome.state_changed);
        assert_eq!(workspace.chrome.preferences, preferences);
        assert!(workspace.pending_effects.effects.is_empty());
    }

    #[test]
    fn request_persist_emits_effect_without_marking_state_changed() {
        let mut workspace = GraphWorkspace::new();
        let workspace_id = WorkspaceId::new("default");

        let outcome = reduce_workspace_intent(
            &mut workspace,
            WorkspaceIntent::RequestPersist {
                workspace_id: workspace_id.clone(),
            },
        )
        .unwrap();

        assert!(!outcome.state_changed);
        assert_eq!(outcome.effects_emitted, 1);
        assert_eq!(
            workspace.drain_effects(),
            vec![WorkspaceEffect::PersistWorkspace { workspace_id }]
        );
    }

    #[test]
    fn frame_upsert_and_activation_are_portable_state_changes() {
        let mut workspace = GraphWorkspace::new();
        let frame = FrameState {
            id: FrameId::new("main"),
            label: "Main".to_string(),
            root_view: Some(view_id(3)),
            panes: Vec::new(),
        };

        reduce_workspace_intent(
            &mut workspace,
            WorkspaceIntent::UpsertFrame {
                frame: frame.clone(),
            },
        )
        .unwrap();
        reduce_workspace_intent(
            &mut workspace,
            WorkspaceIntent::ActivateFrame {
                frame_id: frame.id.clone(),
            },
        )
        .unwrap();

        assert_eq!(workspace.workbench.active_frame, Some(frame.id));
        assert_eq!(workspace.workbench.frames.len(), 1);
    }
}
