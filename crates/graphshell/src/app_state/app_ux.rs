/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure app-UX reducer state for chrome modal and action surfaces.
//!
//! This is the portable subset of the donor `app_ux` seam. It owns the
//! mutually-exclusive modal/action-surface state that can be tested without a
//! host. Renderer-scoped clip capture, diagnostics emission, focus restoration,
//! and node creation remain outside this module.

use graphshell_core::graph::{GraphViewId, NodeKey};
use serde::{Deserialize, Serialize};

use super::{FrameId, GraphWorkspace};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppUxState {
    pub modal_surface: Option<ModalSurface>,
    pub action_surface: ActionSurfaceState,
    pub scene_overlay_view: Option<GraphViewId>,
}

impl AppUxState {
    pub fn is_modal_surface_open(&self, surface: ModalSurface) -> bool {
        self.modal_surface == Some(surface)
    }

    pub fn any_other_modal_surface_open(&self, except: ModalSurface) -> bool {
        self.modal_surface.is_some_and(|surface| surface != except)
    }

    pub fn any_other_focus_capturing_modal_open(&self, except: ModalSurface) -> bool {
        self.modal_surface
            .is_some_and(|surface| surface != except && surface.captures_focus())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModalSurface {
    CommandPalette,
    ContextPalette,
    HelpPanel,
    SettingsOverlay,
    SceneOverlay,
    RadialMenu,
    ClipInspector,
}

impl ModalSurface {
    pub fn captures_focus(self) -> bool {
        !matches!(self, Self::ClipInspector)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionScope {
    #[default]
    Global,
    Graph {
        view_id: GraphViewId,
        target: ScopeTarget,
    },
    Workbench,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScopeTarget {
    #[default]
    None,
    Node(NodeKey),
    Frame(FrameId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Anchor {
    TargetNode(NodeKey),
    TargetFrame(FrameId),
    ViewportPoint { x: i32, y: i32 },
    ScreenCenter,
}

impl Anchor {
    pub fn viewport_point(x: i32, y: i32) -> Self {
        Self::ViewportPoint { x, y }
    }

    pub fn resolved_screen_point(&self) -> Option<(i32, i32)> {
        match self {
            Self::ViewportPoint { x, y } => Some((*x, *y)),
            Self::TargetNode(_) | Self::TargetFrame(_) | Self::ScreenCenter => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionSurfaceState {
    #[default]
    Closed,
    PaletteGlobal,
    PaletteContextual {
        scope: ActionScope,
        anchor: Anchor,
    },
    Radial {
        scope: ActionScope,
        anchor: Anchor,
    },
}

impl ActionSurfaceState {
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub fn is_palette_global(&self) -> bool {
        matches!(self, Self::PaletteGlobal)
    }

    pub fn is_palette_contextual(&self) -> bool {
        matches!(self, Self::PaletteContextual { .. })
    }

    pub fn is_any_palette(&self) -> bool {
        self.is_palette_global() || self.is_palette_contextual()
    }

    pub fn is_radial(&self) -> bool {
        matches!(self, Self::Radial { .. })
    }

    pub fn scope(&self) -> Option<&ActionScope> {
        match self {
            Self::Closed | Self::PaletteGlobal => None,
            Self::PaletteContextual { scope, .. } | Self::Radial { scope, .. } => Some(scope),
        }
    }

    pub fn anchor(&self) -> Option<&Anchor> {
        match self {
            Self::Closed | Self::PaletteGlobal => None,
            Self::PaletteContextual { anchor, .. } | Self::Radial { anchor, .. } => Some(anchor),
        }
    }

    pub fn targets_node(&self, node: NodeKey) -> bool {
        matches!(
            self.scope(),
            Some(ActionScope::Graph {
                target: ScopeTarget::Node(target),
                ..
            }) if *target == node
        )
    }

    pub fn is_graph_scoped(&self) -> bool {
        matches!(self.scope(), Some(ActionScope::Graph { .. }))
    }

    pub fn is_in_other_view(&self, current: GraphViewId) -> bool {
        matches!(
            self.scope(),
            Some(ActionScope::Graph { view_id, .. }) if *view_id != current
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppUxIntent {
    OpenModal { surface: ModalSurface },
    CloseModal { surface: ModalSurface },
    CloseModalSurfacesExcept { keep: Option<ModalSurface> },
    OpenGlobalPalette,
    OpenContextPalette { scope: ActionScope, anchor: Anchor },
    OpenRadial { scope: ActionScope, anchor: Anchor },
    CloseActionSurface,
    CloseActionSurfaceIfTargetsNode { node: NodeKey },
    CloseActionSurfaceIfInOtherView { current_view: GraphViewId },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppUxOutcome {
    pub state_changed: bool,
    pub effects_emitted: usize,
}

pub fn reduce_app_ux_intent(workspace: &mut GraphWorkspace, intent: AppUxIntent) -> AppUxOutcome {
    let before = workspace.chrome.app_ux.clone();

    match intent {
        AppUxIntent::OpenModal { surface } => open_modal(workspace, surface),
        AppUxIntent::CloseModal { surface } => {
            if workspace.chrome.app_ux.modal_surface == Some(surface) {
                workspace.chrome.app_ux.modal_surface = None;
            }
            if matches!(surface, ModalSurface::CommandPalette) {
                close_action_surface(workspace);
            }
        }
        AppUxIntent::CloseModalSurfacesExcept { keep } => {
            workspace.chrome.app_ux.modal_surface = keep;
            if !matches!(keep, Some(ModalSurface::CommandPalette))
                && workspace.chrome.app_ux.action_surface.is_palette_global()
            {
                workspace.chrome.app_ux.action_surface = ActionSurfaceState::Closed;
            }
            if !matches!(
                keep,
                Some(ModalSurface::ContextPalette | ModalSurface::RadialMenu)
            ) && !workspace.chrome.app_ux.action_surface.is_palette_global()
            {
                workspace.chrome.app_ux.action_surface = ActionSurfaceState::Closed;
            }
        }
        AppUxIntent::OpenGlobalPalette => open_global_palette(workspace),
        AppUxIntent::OpenContextPalette { scope, anchor } => {
            workspace.chrome.app_ux.modal_surface = Some(ModalSurface::ContextPalette);
            workspace.chrome.app_ux.action_surface =
                ActionSurfaceState::PaletteContextual { scope, anchor };
        }
        AppUxIntent::OpenRadial { scope, anchor } => {
            workspace.chrome.app_ux.modal_surface = Some(ModalSurface::RadialMenu);
            workspace.chrome.app_ux.action_surface = ActionSurfaceState::Radial { scope, anchor };
        }
        AppUxIntent::CloseActionSurface => close_action_surface(workspace),
        AppUxIntent::CloseActionSurfaceIfTargetsNode { node } => {
            if workspace.chrome.app_ux.action_surface.targets_node(node) {
                close_action_surface(workspace);
            }
        }
        AppUxIntent::CloseActionSurfaceIfInOtherView { current_view } => {
            if workspace
                .chrome
                .app_ux
                .action_surface
                .is_in_other_view(current_view)
            {
                close_action_surface(workspace);
            }
        }
    }

    sync_legacy_command_palette_flag(workspace);
    AppUxOutcome {
        state_changed: workspace.chrome.app_ux != before,
        effects_emitted: 0,
    }
}

pub(crate) fn set_legacy_command_palette_visibility(
    workspace: &mut GraphWorkspace,
    open: bool,
) -> bool {
    let before = workspace.chrome.app_ux.clone();
    if open {
        open_global_palette(workspace);
    } else if workspace.chrome.app_ux.action_surface.is_palette_global() {
        close_action_surface(workspace);
    }
    sync_legacy_command_palette_flag(workspace);
    workspace.chrome.app_ux != before
}

fn open_modal(workspace: &mut GraphWorkspace, surface: ModalSurface) {
    match surface {
        ModalSurface::CommandPalette => open_global_palette(workspace),
        ModalSurface::ContextPalette => {
            workspace.chrome.app_ux.modal_surface = Some(ModalSurface::ContextPalette);
            workspace.chrome.app_ux.action_surface = ActionSurfaceState::PaletteContextual {
                scope: ActionScope::Global,
                anchor: Anchor::ScreenCenter,
            };
        }
        ModalSurface::RadialMenu => {
            workspace.chrome.app_ux.modal_surface = Some(ModalSurface::RadialMenu);
            workspace.chrome.app_ux.action_surface = ActionSurfaceState::Radial {
                scope: ActionScope::Global,
                anchor: Anchor::ScreenCenter,
            };
        }
        ModalSurface::SceneOverlay => {
            workspace.chrome.app_ux.modal_surface = Some(surface);
            workspace.chrome.app_ux.scene_overlay_view = workspace.views.focused_view;
            workspace.chrome.app_ux.action_surface = ActionSurfaceState::Closed;
        }
        ModalSurface::HelpPanel | ModalSurface::SettingsOverlay | ModalSurface::ClipInspector => {
            workspace.chrome.app_ux.modal_surface = Some(surface);
            workspace.chrome.app_ux.action_surface = ActionSurfaceState::Closed;
        }
    }
}

fn open_global_palette(workspace: &mut GraphWorkspace) {
    workspace.chrome.app_ux.modal_surface = Some(ModalSurface::CommandPalette);
    workspace.chrome.app_ux.action_surface = ActionSurfaceState::PaletteGlobal;
}

fn close_action_surface(workspace: &mut GraphWorkspace) {
    if matches!(
        workspace.chrome.app_ux.modal_surface,
        Some(
            ModalSurface::CommandPalette | ModalSurface::ContextPalette | ModalSurface::RadialMenu
        )
    ) {
        workspace.chrome.app_ux.modal_surface = None;
    }
    workspace.chrome.app_ux.action_surface = ActionSurfaceState::Closed;
}

fn sync_legacy_command_palette_flag(workspace: &mut GraphWorkspace) {
    workspace.chrome.command_palette_open = workspace.chrome.app_ux.action_surface.is_any_palette();
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn view_id(value: u128) -> GraphViewId {
        GraphViewId::from_uuid(Uuid::from_u128(value))
    }

    fn node(value: usize) -> NodeKey {
        NodeKey::new(value)
    }

    #[test]
    fn global_palette_opens_single_action_surface() {
        let mut workspace = GraphWorkspace::new();

        let outcome = reduce_app_ux_intent(&mut workspace, AppUxIntent::OpenGlobalPalette);

        assert!(outcome.state_changed);
        assert_eq!(
            workspace.chrome.app_ux.modal_surface,
            Some(ModalSurface::CommandPalette)
        );
        assert!(workspace.chrome.app_ux.action_surface.is_palette_global());
        assert!(workspace.chrome.command_palette_open);
    }

    #[test]
    fn opening_help_panel_closes_palette_state() {
        let mut workspace = GraphWorkspace::new();
        reduce_app_ux_intent(&mut workspace, AppUxIntent::OpenGlobalPalette);

        reduce_app_ux_intent(
            &mut workspace,
            AppUxIntent::OpenModal {
                surface: ModalSurface::HelpPanel,
            },
        );

        assert_eq!(
            workspace.chrome.app_ux.modal_surface,
            Some(ModalSurface::HelpPanel)
        );
        assert_eq!(
            workspace.chrome.app_ux.action_surface,
            ActionSurfaceState::Closed
        );
        assert!(!workspace.chrome.command_palette_open);
    }

    #[test]
    fn clip_inspector_is_not_focus_capturing_modal() {
        let mut state = AppUxState {
            modal_surface: Some(ModalSurface::ClipInspector),
            ..AppUxState::default()
        };

        assert!(!state.any_other_focus_capturing_modal_open(ModalSurface::HelpPanel));
        assert!(state.any_other_modal_surface_open(ModalSurface::HelpPanel));
        state.modal_surface = Some(ModalSurface::SceneOverlay);
        assert!(state.any_other_focus_capturing_modal_open(ModalSurface::HelpPanel));
    }

    #[test]
    fn contextual_palette_closes_when_target_node_is_removed() {
        let mut workspace = GraphWorkspace::new();
        let view_id = view_id(1);
        let target = node(42);
        reduce_app_ux_intent(
            &mut workspace,
            AppUxIntent::OpenContextPalette {
                scope: ActionScope::Graph {
                    view_id,
                    target: ScopeTarget::Node(target),
                },
                anchor: Anchor::TargetNode(target),
            },
        );

        let outcome = reduce_app_ux_intent(
            &mut workspace,
            AppUxIntent::CloseActionSurfaceIfTargetsNode { node: target },
        );

        assert!(outcome.state_changed);
        assert_eq!(
            workspace.chrome.app_ux.action_surface,
            ActionSurfaceState::Closed
        );
        assert_eq!(workspace.chrome.app_ux.modal_surface, None);
    }

    #[test]
    fn graph_scoped_surface_closes_on_view_change() {
        let mut workspace = GraphWorkspace::new();
        reduce_app_ux_intent(
            &mut workspace,
            AppUxIntent::OpenRadial {
                scope: ActionScope::Graph {
                    view_id: view_id(10),
                    target: ScopeTarget::None,
                },
                anchor: Anchor::ScreenCenter,
            },
        );

        reduce_app_ux_intent(
            &mut workspace,
            AppUxIntent::CloseActionSurfaceIfInOtherView {
                current_view: view_id(11),
            },
        );

        assert_eq!(
            workspace.chrome.app_ux.action_surface,
            ActionSurfaceState::Closed
        );
        assert_eq!(workspace.chrome.app_ux.modal_surface, None);
    }
}
