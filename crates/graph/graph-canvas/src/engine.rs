/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Interaction engine: processes input events against a `ProjectedScene`
//! and emits `CanvasAction`s.
//!
//! The engine is stateful — it tracks pointer position, drag state, and
//! lasso gesture progress between frames. The host feeds events in, reads
//! actions out, and converts them into its own mutation model.

use euclid::default::{Point2D, Vector2D};
use std::hash::Hash;

use crate::camera::{CanvasCamera, CanvasViewport};
use crate::hit_test::{HitTestResult, hit_test_point, nodes_in_screen_rect};
use crate::input::{CanvasInputEvent, Modifiers, PointerButton};
use crate::interaction::{CanvasAction, EdgeRef, InteractionState, LassoState};
use crate::navigation::LassoModifier;
use crate::packet::HitProxy;

/// Configuration for the interaction engine.
///
/// Mirror of the input-relevant subset of
/// [`crate::navigation::NavigationPolicy`] — hosts that want to stay
/// in sync with a user-tunable policy should resolve the policy and
/// call `policy.to_interaction_config()` rather than building an
/// `InteractionConfig` directly.
#[derive(Debug, Clone)]
pub struct InteractionConfig {
    /// Minimum drag distance in screen pixels before a drag gesture starts.
    pub drag_threshold_px: f32,
    /// Zoom factor per scroll unit (applied when `Ctrl`/`Cmd` is held).
    pub scroll_zoom_factor: f32,
    /// Screen pixels of pan per unit of scroll delta (applied when no
    /// modifier is held). Matches typical infinite-canvas feel (Figma /
    /// Miro): one mousewheel notch pans roughly `50 px`.
    pub scroll_pan_pixels_per_unit: f32,
    /// Whether node dragging is enabled (disabled in Browse mode).
    pub node_drag_enabled: bool,
    /// Whether lasso selection is enabled.
    pub lasso_enabled: bool,
    /// Whether to capture pan velocity on drag release so the host can
    /// coast the camera via [`CanvasCamera::tick_inertia`].
    pub pan_inertia_enabled: bool,
    /// Which modifier routes primary-drag on empty background into a
    /// lasso marquee. Defaults to `Shift` (Miro convention).
    pub lasso_modifier: LassoModifier,
}

impl Default for InteractionConfig {
    fn default() -> Self {
        Self {
            drag_threshold_px: 6.0,
            scroll_zoom_factor: 0.1,
            scroll_pan_pixels_per_unit: 50.0,
            node_drag_enabled: true,
            lasso_enabled: true,
            pan_inertia_enabled: true,
            lasso_modifier: LassoModifier::default(),
        }
    }
}

impl InteractionConfig {
    /// True when a press with these modifiers should start a lasso
    /// marquee instead of a pan, given the engine's configured
    /// lasso modifier. Keeps the gate in one place so engine and tests
    /// don't drift.
    fn press_should_lasso(&self, modifiers: Modifiers) -> bool {
        if !self.lasso_enabled {
            return false;
        }
        match self.lasso_modifier {
            LassoModifier::Shift => modifiers.shift,
            LassoModifier::Ctrl => modifiers.ctrl,
            LassoModifier::Alt => modifiers.alt,
            LassoModifier::None => true,
        }
    }
}

/// Internal drag state.
#[derive(Debug, Clone)]
enum DragState<N> {
    /// No drag in progress.
    None,
    /// Pointer pressed but hasn't moved past threshold yet.
    Pending {
        origin: Point2D<f32>,
        button: PointerButton,
        /// Modifiers captured at press time. Shift routes
        /// primary-drag on empty background to a lasso marquee instead
        /// of the default pan.
        modifiers: Modifiers,
        target: HitTestResult<N>,
    },
    /// Actively dragging a node.
    DraggingNode { node: N, last_pos: Point2D<f32> },
    /// Actively dragging a lasso rectangle.
    Lasso { origin: Point2D<f32> },
    /// Panning the camera. `last_world_delta` is the world-space delta
    /// applied by the most recent pointer move; on release the engine
    /// converts it to a per-second pan velocity for inertial coast.
    Panning {
        last_pos: Point2D<f32>,
        last_world_delta: Vector2D<f32>,
    },
}

/// The interaction engine. Maintains state between frames and emits actions.
pub struct InteractionEngine<N: Clone + Eq + Hash> {
    pub state: InteractionState<N>,
    pub config: InteractionConfig,
    drag: DragState<N>,
    last_pointer_pos: Option<Point2D<f32>>,
}

impl<N: Clone + Eq + Hash> InteractionEngine<N> {
    pub fn new(config: InteractionConfig) -> Self {
        Self {
            state: InteractionState::default(),
            config,
            drag: DragState::None,
            last_pointer_pos: None,
        }
    }

    /// Process a single input event and return any resulting actions.
    ///
    /// The caller provides the current frame's hit proxies (from the most
    /// recent `ProjectedScene`), camera, and viewport so the engine can
    /// resolve pointer positions against the scene.
    pub fn process_event(
        &mut self,
        event: &CanvasInputEvent,
        hit_proxies: &[HitProxy<N>],
        camera: &CanvasCamera,
        viewport: &CanvasViewport,
    ) -> Vec<CanvasAction<N>> {
        let mut actions = Vec::new();

        match event {
            CanvasInputEvent::PointerMoved { position } => {
                self.last_pointer_pos = Some(*position);
                self.handle_pointer_move(*position, hit_proxies, camera, viewport, &mut actions);
            }
            CanvasInputEvent::PointerPressed {
                position,
                button,
                modifiers,
            } => {
                self.handle_pointer_press(
                    *position,
                    *button,
                    *modifiers,
                    hit_proxies,
                    &mut actions,
                );
            }
            CanvasInputEvent::PointerReleased {
                position,
                button,
                modifiers,
            } => {
                self.handle_pointer_release(
                    *position,
                    *button,
                    *modifiers,
                    hit_proxies,
                    &mut actions,
                );
            }
            CanvasInputEvent::Scroll {
                delta,
                position,
                modifiers,
            } => {
                // Browser-style default: plain wheel pans, `Ctrl`/`Cmd`
                // + wheel zooms. Matches Firefox/Chrome infinite-canvas
                // convention and most trackpad pinch gestures (which
                // synthesize `ctrlKey = true`).
                if modifiers.ctrl {
                    let factor = 1.0 + delta * self.config.scroll_zoom_factor;
                    actions.push(CanvasAction::ZoomCamera {
                        factor,
                        focus: *position,
                    });
                } else {
                    // Treat scroll delta as vertical pan amount in screen
                    // pixels. Convert to world units via current camera
                    // zoom. Positive delta (wheel-up) scrolls content up —
                    // i.e. the viewport reveals content above, which in
                    // our pan convention means `pan.y` increases.
                    let pan_pixels = *delta * self.config.scroll_pan_pixels_per_unit;
                    actions.push(CanvasAction::PanCamera(Vector2D::new(0.0, pan_pixels)));
                }
            }
            CanvasInputEvent::PointerLeft => {
                self.last_pointer_pos = None;
                if self.state.hovered_node.is_some() {
                    self.state.hovered_node = None;
                    actions.push(CanvasAction::HoverNode(None));
                }
                if self.state.hovered_edge.is_some() {
                    self.state.hovered_edge = None;
                    actions.push(CanvasAction::HoverEdge(None));
                }
                if self.state.hovered_scene_object.is_some() {
                    self.state.hovered_scene_object = None;
                    actions.push(CanvasAction::HoverSceneObject(None));
                }
                if matches!(self.drag, DragState::Lasso { .. }) {
                    self.state.lasso = None;
                    self.drag = DragState::None;
                    actions.push(CanvasAction::LassoCancel);
                }
            }
            CanvasInputEvent::PointerDoubleClick { .. } => {
                // Double-click handling (e.g. FocusNode) is host-specific.
                // The host should handle this by inspecting the current
                // hovered_node in InteractionState.
            }
        }

        actions
    }

    fn handle_pointer_move(
        &mut self,
        position: Point2D<f32>,
        hit_proxies: &[HitProxy<N>],
        camera: &CanvasCamera,
        viewport: &CanvasViewport,
        actions: &mut Vec<CanvasAction<N>>,
    ) {
        match &self.drag {
            DragState::None => {
                // Hover resolution.
                self.resolve_hover(position, hit_proxies, actions);
            }
            DragState::Pending {
                origin,
                button,
                modifiers,
                target,
            } => {
                let dist = (position - *origin).length();
                if dist >= self.config.drag_threshold_px {
                    let origin = *origin;
                    let button = *button;
                    let modifiers = *modifiers;
                    let target = target.clone();
                    // Threshold crossed — start the appropriate drag.
                    //
                    // Miro-style default on the background layer:
                    // primary-drag pans, `Shift`+primary-drag starts a
                    // lasso marquee. Middle-drag and secondary-drag on
                    // background also pan (redundant entry points that
                    // users reach for by habit).
                    match (&target, button) {
                        (HitTestResult::Node(id), PointerButton::Primary)
                            if self.config.node_drag_enabled =>
                        {
                            self.drag = DragState::DraggingNode {
                                node: id.clone(),
                                last_pos: position,
                            };
                            let delta = position - origin;
                            let world_delta =
                                Vector2D::new(delta.x / camera.zoom, delta.y / camera.zoom);
                            actions.push(CanvasAction::DragNode {
                                node: id.clone(),
                                delta: world_delta,
                            });
                        }
                        (HitTestResult::None, PointerButton::Primary)
                            if self.config.press_should_lasso(modifiers) =>
                        {
                            // Primary-drag on empty background that
                            // matches the configured lasso modifier
                            // (default `Shift`).
                            let world_origin = camera.screen_to_world(origin, viewport);
                            self.state.lasso = Some(LassoState {
                                origin: world_origin,
                                current: camera.screen_to_world(position, viewport),
                            });
                            self.drag = DragState::Lasso { origin };
                            actions.push(CanvasAction::LassoBegin {
                                origin: world_origin,
                            });
                        }
                        // Scene objects are not draggable — scripts control
                        // their position. Fall through to panning alongside
                        // plain-primary-background, middle, and secondary.
                        (HitTestResult::None, PointerButton::Primary)
                        | (HitTestResult::SceneObject(_), PointerButton::Primary)
                        | (_, PointerButton::Middle)
                        | (HitTestResult::None, PointerButton::Secondary) => {
                            let delta = position - origin;
                            let world_delta =
                                Vector2D::new(delta.x / camera.zoom, delta.y / camera.zoom);
                            self.drag = DragState::Panning {
                                last_pos: position,
                                last_world_delta: world_delta,
                            };
                            actions.push(CanvasAction::PanCamera(world_delta));
                        }
                        _ => {
                            // Not a recognized drag gesture — cancel.
                            self.drag = DragState::None;
                        }
                    }
                }
            }
            DragState::DraggingNode { node, last_pos } => {
                let delta = position - *last_pos;
                let world_delta = Vector2D::new(delta.x / camera.zoom, delta.y / camera.zoom);
                let node = node.clone();
                self.drag = DragState::DraggingNode {
                    node: node.clone(),
                    last_pos: position,
                };
                actions.push(CanvasAction::DragNode {
                    node,
                    delta: world_delta,
                });
            }
            DragState::Lasso { origin } => {
                let origin = *origin;
                let world_current = camera.screen_to_world(position, viewport);
                self.state.lasso = Some(LassoState {
                    origin: camera.screen_to_world(origin, viewport),
                    current: world_current,
                });
                actions.push(CanvasAction::LassoUpdate {
                    current: world_current,
                });
            }
            DragState::Panning { last_pos, .. } => {
                let delta = position - *last_pos;
                let world_delta = Vector2D::new(delta.x / camera.zoom, delta.y / camera.zoom);
                self.drag = DragState::Panning {
                    last_pos: position,
                    last_world_delta: world_delta,
                };
                actions.push(CanvasAction::PanCamera(world_delta));
            }
        }
    }

    fn handle_pointer_press(
        &mut self,
        position: Point2D<f32>,
        button: PointerButton,
        modifiers: Modifiers,
        hit_proxies: &[HitProxy<N>],
        actions: &mut Vec<CanvasAction<N>>,
    ) {
        let target = hit_test_point(position, hit_proxies);
        self.drag = DragState::Pending {
            origin: position,
            button,
            modifiers,
            target: target.clone(),
        };
        // Don't emit select actions on press — wait for release (click)
        // so we can distinguish click from drag.
        let _ = actions;
    }

    fn handle_pointer_release(
        &mut self,
        position: Point2D<f32>,
        _button: PointerButton,
        modifiers: Modifiers,
        hit_proxies: &[HitProxy<N>],
        actions: &mut Vec<CanvasAction<N>>,
    ) {
        match std::mem::replace(&mut self.drag, DragState::None) {
            DragState::Pending { target, .. } => {
                // Click (not a drag). Resolve the action.
                match target {
                    HitTestResult::Node(id) => {
                        if modifiers.ctrl {
                            actions.push(CanvasAction::ToggleSelectNode(id));
                        } else {
                            actions.push(CanvasAction::SelectNode(id));
                        }
                    }
                    HitTestResult::Edge { .. } => {
                        // Edge click — host handles this via hovered_edge state.
                    }
                    HitTestResult::SceneObject(id) => {
                        actions.push(CanvasAction::ClickSceneObject(id));
                    }
                    HitTestResult::None => {
                        if !modifiers.ctrl && !modifiers.shift {
                            actions.push(CanvasAction::ClearSelection);
                        }
                    }
                }
            }
            DragState::Lasso { origin } => {
                let lasso_nodes = nodes_in_screen_rect(origin, position, hit_proxies);
                self.state.lasso = None;
                actions.push(CanvasAction::LassoComplete { nodes: lasso_nodes });
            }
            DragState::Panning {
                last_world_delta, ..
            } => {
                // Drag ended. If inertia is enabled and the terminal
                // delta is above the engine's drag threshold (scaled
                // against the assumed 60fps tick), seed the camera's
                // inertial velocity so the host can coast the view.
                if self.config.pan_inertia_enabled {
                    // Convert last per-frame world delta to per-second
                    // velocity. Engine has no real frame clock, so assume
                    // a 60hz tick — good enough for "feel".
                    let velocity_per_second = last_world_delta * 60.0;
                    if velocity_per_second.length() >= crate::camera::PAN_VELOCITY_EPSILON {
                        actions.push(CanvasAction::SetPanInertia(velocity_per_second));
                    }
                }
            }
            DragState::DraggingNode { .. } => {
                // Node drag ended. No additional action needed.
            }
            DragState::None => {}
        }
    }

    fn resolve_hover(
        &mut self,
        position: Point2D<f32>,
        hit_proxies: &[HitProxy<N>],
        actions: &mut Vec<CanvasAction<N>>,
    ) {
        let hit = hit_test_point(position, hit_proxies);
        match &hit {
            HitTestResult::Node(id) => {
                if self.state.hovered_node.as_ref() != Some(id) {
                    self.state.hovered_node = Some(id.clone());
                    self.state.hovered_edge = None;
                    self.clear_scene_object_hover(actions);
                    actions.push(CanvasAction::HoverNode(Some(id.clone())));
                }
            }
            HitTestResult::Edge { source, target } => {
                let edge_ref = EdgeRef {
                    source: source.clone(),
                    target: target.clone(),
                };
                if self.state.hovered_edge.as_ref() != Some(&edge_ref) {
                    self.state.hovered_node = None;
                    self.state.hovered_edge = Some(edge_ref.clone());
                    self.clear_scene_object_hover(actions);
                    actions.push(CanvasAction::HoverEdge(Some(edge_ref)));
                }
            }
            HitTestResult::SceneObject(id) => {
                if self.state.hovered_scene_object.as_ref() != Some(id) {
                    self.state.hovered_scene_object = Some(*id);
                    self.state.hovered_node = None;
                    self.state.hovered_edge = None;
                    actions.push(CanvasAction::HoverSceneObject(Some(*id)));
                }
            }
            HitTestResult::None => {
                if self.state.hovered_node.is_some() {
                    self.state.hovered_node = None;
                    actions.push(CanvasAction::HoverNode(None));
                }
                if self.state.hovered_edge.is_some() {
                    self.state.hovered_edge = None;
                    actions.push(CanvasAction::HoverEdge(None));
                }
                self.clear_scene_object_hover(actions);
            }
        }
    }

    fn clear_scene_object_hover(&mut self, actions: &mut Vec<CanvasAction<N>>) {
        if self.state.hovered_scene_object.is_some() {
            self.state.hovered_scene_object = None;
            actions.push(CanvasAction::HoverSceneObject(None));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{CanvasCamera, CanvasViewport};
    use crate::input::CanvasInputEvent;
    use crate::packet::HitProxy;
    use crate::scripting::SceneObjectId;

    fn default_engine() -> InteractionEngine<u32> {
        InteractionEngine::new(InteractionConfig::default())
    }

    fn default_camera() -> CanvasCamera {
        CanvasCamera::default()
    }

    fn default_viewport() -> CanvasViewport {
        CanvasViewport::default()
    }

    fn sample_proxies() -> Vec<HitProxy<u32>> {
        vec![
            HitProxy::Node {
                id: 0,
                center: Point2D::new(400.0, 300.0),
                radius: 16.0,
            },
            HitProxy::Node {
                id: 1,
                center: Point2D::new(500.0, 300.0),
                radius: 16.0,
            },
        ]
    }

    fn no_mods() -> Modifiers {
        Modifiers::default()
    }

    #[test]
    fn hover_emits_action() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(400.0, 300.0),
            },
            &proxies,
            &default_camera(),
            &default_viewport(),
        );
        assert!(actions.contains(&CanvasAction::HoverNode(Some(0))));
        assert_eq!(engine.state.hovered_node, Some(0));
    }

    #[test]
    fn hover_clears_on_miss() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        // Hover node 0.
        engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(400.0, 300.0),
            },
            &proxies,
            &default_camera(),
            &default_viewport(),
        );
        assert_eq!(engine.state.hovered_node, Some(0));
        // Move away.
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(0.0, 0.0),
            },
            &proxies,
            &default_camera(),
            &default_viewport(),
        );
        assert!(actions.contains(&CanvasAction::HoverNode(None)));
        assert_eq!(engine.state.hovered_node, None);
    }

    #[test]
    fn click_node_selects() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        // Press.
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(400.0, 300.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        // Release at same position (click).
        let actions = engine.process_event(
            &CanvasInputEvent::PointerReleased {
                position: Point2D::new(400.0, 300.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(actions.contains(&CanvasAction::SelectNode(0)));
    }

    #[test]
    fn ctrl_click_toggles() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        let mods = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(400.0, 300.0),
                button: PointerButton::Primary,
                modifiers: mods,
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerReleased {
                position: Point2D::new(400.0, 300.0),
                button: PointerButton::Primary,
                modifiers: mods,
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(actions.contains(&CanvasAction::ToggleSelectNode(0)));
    }

    #[test]
    fn click_background_clears_selection() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        // Click on empty space.
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(0.0, 0.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerReleased {
                position: Point2D::new(0.0, 0.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(actions.contains(&CanvasAction::ClearSelection));
    }

    #[test]
    fn drag_node_emits_delta() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        // Press on node 0.
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(400.0, 300.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        // Move past threshold.
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(420.0, 300.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        let has_drag = actions
            .iter()
            .any(|a| matches!(a, CanvasAction::DragNode { node: 0, .. }));
        assert!(has_drag, "expected DragNode action, got: {:?}", actions);
    }

    #[test]
    fn scroll_without_modifier_emits_pan() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        let actions = engine.process_event(
            &CanvasInputEvent::Scroll {
                delta: 1.0,
                position: Point2D::new(400.0, 300.0),
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::PanCamera(..))),
            "plain wheel must pan, got: {actions:?}",
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, CanvasAction::ZoomCamera { .. })),
            "plain wheel must not zoom, got: {actions:?}",
        );
    }

    #[test]
    fn scroll_with_ctrl_emits_zoom() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        let mods = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let actions = engine.process_event(
            &CanvasInputEvent::Scroll {
                delta: 1.0,
                position: Point2D::new(400.0, 300.0),
                modifiers: mods,
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::ZoomCamera { .. })),
            "ctrl+wheel must zoom, got: {actions:?}",
        );
    }

    #[test]
    fn pan_release_emits_inertia_when_drag_has_momentum() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        // Middle-drag across background to pan.
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(100.0, 100.0),
                button: PointerButton::Middle,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(200.0, 100.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        // Second move keeps the drag active and seeds a non-zero
        // last_world_delta so release can emit inertia.
        engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(260.0, 100.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        let release_actions = engine.process_event(
            &CanvasInputEvent::PointerReleased {
                position: Point2D::new(260.0, 100.0),
                button: PointerButton::Middle,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            release_actions
                .iter()
                .any(|a| matches!(a, CanvasAction::SetPanInertia(..))),
            "pan release must seed inertia, got: {release_actions:?}",
        );
    }

    #[test]
    fn pan_release_skips_inertia_when_disabled() {
        let mut engine = InteractionEngine::<u32>::new(InteractionConfig {
            pan_inertia_enabled: false,
            ..InteractionConfig::default()
        });
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(100.0, 100.0),
                button: PointerButton::Middle,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(260.0, 100.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        let release_actions = engine.process_event(
            &CanvasInputEvent::PointerReleased {
                position: Point2D::new(260.0, 100.0),
                button: PointerButton::Middle,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            !release_actions
                .iter()
                .any(|a| matches!(a, CanvasAction::SetPanInertia(..))),
            "inertia must not fire when the feature is disabled, got: {release_actions:?}",
        );
    }

    #[test]
    fn plain_primary_drag_on_background_pans() {
        // Miro-style default: plain primary-drag on empty background
        // pans the camera; no lasso is started.
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(50.0, 50.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(80.0, 80.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::PanCamera(..))),
            "plain primary-drag on background must pan, got: {actions:?}",
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, CanvasAction::LassoBegin { .. })),
            "plain primary-drag on background must NOT start a lasso, got: {actions:?}",
        );
        assert!(engine.state.lasso.is_none());
    }

    #[test]
    fn ctrl_primary_drag_lassos_when_policy_selects_ctrl() {
        // Swapping the modifier in config re-routes plain-Shift drags
        // back to a pan, and Ctrl drags become the lasso gesture.
        let mut engine = InteractionEngine::<u32>::new(InteractionConfig {
            lasso_modifier: LassoModifier::Ctrl,
            ..InteractionConfig::default()
        });
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(50.0, 50.0),
                button: PointerButton::Primary,
                modifiers: ctrl,
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(80.0, 80.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::LassoBegin { .. })),
            "Ctrl+primary-drag must lasso when policy is Ctrl, got: {actions:?}",
        );
    }

    #[test]
    fn shift_primary_drag_pans_when_policy_selects_ctrl() {
        // Shift is no longer the lasso modifier — plain-Shift drag on
        // background falls through to pan like any other "no gate matched" press.
        let mut engine = InteractionEngine::<u32>::new(InteractionConfig {
            lasso_modifier: LassoModifier::Ctrl,
            ..InteractionConfig::default()
        });
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(50.0, 50.0),
                button: PointerButton::Primary,
                modifiers: shift,
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(80.0, 80.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::PanCamera(..))),
            "Shift-drag must pan when policy is Ctrl, got: {actions:?}",
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, CanvasAction::LassoBegin { .. })),
            "Shift-drag must NOT lasso when policy is Ctrl, got: {actions:?}",
        );
    }

    #[test]
    fn modifier_none_routes_plain_primary_drag_to_lasso() {
        // Figma-style config: no modifier gating. Plain-primary drag
        // on empty background always starts a lasso.
        let mut engine = InteractionEngine::<u32>::new(InteractionConfig {
            lasso_modifier: LassoModifier::None,
            ..InteractionConfig::default()
        });
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(50.0, 50.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(80.0, 80.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::LassoBegin { .. })),
            "plain primary-drag must lasso when policy is None, got: {actions:?}",
        );
    }

    #[test]
    fn shift_primary_drag_on_background_lassos() {
        // `Shift`+primary-drag on empty background starts a marquee
        // select. The modifier is captured at press time and checked
        // when the threshold is crossed.
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(50.0, 50.0),
                button: PointerButton::Primary,
                modifiers: shift,
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(80.0, 80.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::LassoBegin { .. })),
            "Shift+primary-drag must start a lasso, got: {actions:?}",
        );
        assert!(engine.state.lasso.is_some());

        // Release completes the lasso.
        let actions = engine.process_event(
            &CanvasInputEvent::PointerReleased {
                position: Point2D::new(80.0, 80.0),
                button: PointerButton::Primary,
                modifiers: shift,
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::LassoComplete { .. }))
        );
        assert!(engine.state.lasso.is_none());
    }

    #[test]
    fn pointer_left_clears_hover() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        // Hover node.
        engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(400.0, 300.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert_eq!(engine.state.hovered_node, Some(0));
        // Leave.
        let actions = engine.process_event(&CanvasInputEvent::PointerLeft, &proxies, &cam, &vp);
        assert!(actions.contains(&CanvasAction::HoverNode(None)));
        assert_eq!(engine.state.hovered_node, None);
    }

    fn scene_object_proxies() -> Vec<HitProxy<u32>> {
        vec![
            HitProxy::Node {
                id: 0,
                center: Point2D::new(400.0, 300.0),
                radius: 16.0,
            },
            HitProxy::SceneObject {
                id: SceneObjectId(10),
                center: Point2D::new(500.0, 300.0),
                radius: 20.0,
            },
        ]
    }

    #[test]
    fn hover_scene_object_emits_action() {
        let mut engine = default_engine();
        let proxies = scene_object_proxies();
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(500.0, 300.0),
            },
            &proxies,
            &default_camera(),
            &default_viewport(),
        );
        assert!(actions.contains(&CanvasAction::HoverSceneObject(Some(SceneObjectId(10)))));
        assert_eq!(engine.state.hovered_scene_object, Some(SceneObjectId(10)));
    }

    #[test]
    fn click_scene_object_emits_action() {
        let mut engine = default_engine();
        let proxies = scene_object_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(500.0, 300.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerReleased {
                position: Point2D::new(500.0, 300.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(actions.contains(&CanvasAction::ClickSceneObject(SceneObjectId(10))));
    }

    #[test]
    fn hover_scene_object_clears_on_leave() {
        let mut engine = default_engine();
        let proxies = scene_object_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        // Hover scene object.
        engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(500.0, 300.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert_eq!(engine.state.hovered_scene_object, Some(SceneObjectId(10)));
        // Leave.
        let actions = engine.process_event(&CanvasInputEvent::PointerLeft, &proxies, &cam, &vp);
        assert!(actions.contains(&CanvasAction::HoverSceneObject(None)));
        assert_eq!(engine.state.hovered_scene_object, None);
    }

    #[test]
    fn scene_object_drag_falls_through_to_pan() {
        let mut engine = default_engine();
        let proxies = scene_object_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        // Press on scene object.
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(500.0, 300.0),
                button: PointerButton::Primary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        // Drag past threshold — should pan, not drag.
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(530.0, 300.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::PanCamera(..))),
            "expected PanCamera for scene object drag, got: {:?}",
            actions
        );
    }

    #[test]
    fn secondary_drag_on_background_pans() {
        let mut engine = default_engine();
        let proxies = sample_proxies();
        let cam = default_camera();
        let vp = default_viewport();
        engine.process_event(
            &CanvasInputEvent::PointerPressed {
                position: Point2D::new(50.0, 50.0),
                button: PointerButton::Secondary,
                modifiers: no_mods(),
            },
            &proxies,
            &cam,
            &vp,
        );
        let actions = engine.process_event(
            &CanvasInputEvent::PointerMoved {
                position: Point2D::new(70.0, 50.0),
            },
            &proxies,
            &cam,
            &vp,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, CanvasAction::PanCamera(..))),
            "expected PanCamera, got: {:?}",
            actions
        );
    }
}
