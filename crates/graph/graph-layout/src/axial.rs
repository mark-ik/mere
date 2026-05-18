/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Axial layouts — place nodes along one or two explicit axes driven by
//! host-provided per-node values (`LayoutExtras::axis_value_by_node`).
//!
//! - [`Timeline`] — numeric x-axis coordinate (time, recency, any
//!   ordinal). Secondary y-axis stacks nodes with the same x.
//! - [`Kanban`] — categorical column bucket. Nodes with the same tag
//!   land in one column; columns are ordered by config.
//!
//! Both share an upstream dependency on [`super::AxisValue`] in
//! `LayoutExtras`. Hosts compute the axis values and pass them through;
//! graph-canvas stays framework-agnostic.

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use super::{AxisValue, Layout, LayoutExtras, StaticLayoutState};
use graph_canvas::camera::CanvasViewport;
use graph_canvas::scene::CanvasSceneInput;

// ── Timeline ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineConfig {
    /// World-space origin of the time axis (leftmost edge of the timeline).
    pub origin: Point2D<f32>,
    /// World-unit length of the time axis.
    pub axis_length: f32,
    /// Vertical spacing between rows when multiple nodes share the same
    /// or nearby x-coordinates.
    pub row_gap: f32,
    /// Behavior for nodes without an `AxisValue::Numeric` entry in
    /// `axis_value_by_node`.
    pub fallback: TimelineFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimelineFallback {
    /// Leave unassigned nodes at their current position.
    LeaveInPlace,
    /// Stack unassigned nodes vertically below the axis origin.
    StackBelowOrigin,
    /// Stack unassigned nodes vertically past the axis end.
    StackPastEnd,
}

impl Default for TimelineFallback {
    fn default() -> Self {
        Self::LeaveInPlace
    }
}

impl Default for TimelineConfig {
    fn default() -> Self {
        Self {
            origin: Point2D::new(0.0, 0.0),
            axis_length: 800.0,
            row_gap: 40.0,
            fallback: TimelineFallback::default(),
        }
    }
}

/// Numeric-axis layout: nodes placed by `AxisValue::Numeric` on the x-axis.
#[derive(Debug, Default)]
pub struct Timeline {
    pub config: TimelineConfig,
}

impl Timeline {
    pub fn new(config: TimelineConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for Timeline
where
    N: Clone + Eq + Hash,
{
    type State = StaticLayoutState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        _dt: f32,
        _viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        state.step_count = state.step_count.saturating_add(1);
        if scene.nodes.is_empty() {
            return HashMap::new();
        }

        // Compute min/max of numeric axis values among scene nodes.
        let mut min_val = f64::INFINITY;
        let mut max_val = f64::NEG_INFINITY;
        let mut has_any = false;
        for node in &scene.nodes {
            if let Some(AxisValue::Numeric(v)) = extras.axis_value_by_node.get(&node.id) {
                min_val = min_val.min(*v);
                max_val = max_val.max(*v);
                has_any = true;
            }
        }
        if !has_any {
            return HashMap::new();
        }
        let span = (max_val - min_val).max(f64::EPSILON);

        // Group nodes by quantized x-slot so same-time nodes stack.
        let slot_width = (self.config.axis_length / 40.0).max(1.0);
        let mut by_slot: HashMap<i32, Vec<&N>> = HashMap::new();
        let mut targets: HashMap<N, Point2D<f32>> = HashMap::with_capacity(scene.nodes.len());
        let mut unassigned_count = 0u32;

        for node in &scene.nodes {
            match extras.axis_value_by_node.get(&node.id) {
                Some(AxisValue::Numeric(v)) => {
                    let normalized = ((*v - min_val) / span) as f32;
                    let x = self.config.origin.x + normalized * self.config.axis_length;
                    let slot = (x / slot_width).round() as i32;
                    let stack_idx = by_slot.entry(slot).or_default().len();
                    by_slot.get_mut(&slot).unwrap().push(&node.id);
                    let y = self.config.origin.y + stack_idx as f32 * self.config.row_gap;
                    targets.insert(node.id.clone(), Point2D::new(x, y));
                }
                _ => match self.config.fallback {
                    TimelineFallback::LeaveInPlace => {}
                    TimelineFallback::StackBelowOrigin => {
                        let y = self.config.origin.y
                            - self.config.row_gap
                            - unassigned_count as f32 * self.config.row_gap;
                        targets.insert(node.id.clone(), Point2D::new(self.config.origin.x, y));
                        unassigned_count += 1;
                    }
                    TimelineFallback::StackPastEnd => {
                        let y =
                            self.config.origin.y + unassigned_count as f32 * self.config.row_gap;
                        targets.insert(
                            node.id.clone(),
                            Point2D::new(self.config.origin.x + self.config.axis_length + 60.0, y),
                        );
                        unassigned_count += 1;
                    }
                },
            }
        }

        emit_targets(scene, targets, state, extras)
    }
}

// ── Kanban ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KanbanConfig {
    pub origin: Point2D<f32>,
    /// Horizontal spacing between columns.
    pub column_gap: f32,
    /// Vertical spacing between entries within a column.
    pub row_gap: f32,
    /// Canonical ordering of columns left-to-right. Nodes whose tag is
    /// not in this list go into an "other" column appended at the end.
    pub column_order: Vec<String>,
    /// Include a trailing "other" column for unrecognized tags.
    pub include_other_column: bool,
}

impl Default for KanbanConfig {
    fn default() -> Self {
        Self {
            origin: Point2D::new(0.0, 0.0),
            column_gap: 240.0,
            row_gap: 80.0,
            column_order: Vec::new(),
            include_other_column: true,
        }
    }
}

/// Categorical-bucket layout: nodes placed by `AxisValue::Categorical`
/// tag into named columns.
#[derive(Debug, Default)]
pub struct Kanban {
    pub config: KanbanConfig,
}

impl Kanban {
    pub fn new(config: KanbanConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for Kanban
where
    N: Clone + Eq + Hash,
{
    type State = StaticLayoutState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        _dt: f32,
        _viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        state.step_count = state.step_count.saturating_add(1);
        if scene.nodes.is_empty() {
            return HashMap::new();
        }

        // Group nodes by categorical tag; nodes without a Categorical
        // entry go into "other" if enabled, else skipped.
        let mut column_members: HashMap<String, Vec<&N>> = HashMap::new();
        let mut unassigned: Vec<&N> = Vec::new();
        for node in &scene.nodes {
            match extras.axis_value_by_node.get(&node.id) {
                Some(AxisValue::Categorical(tag)) => {
                    column_members
                        .entry(tag.clone())
                        .or_default()
                        .push(&node.id);
                }
                _ => unassigned.push(&node.id),
            }
        }

        // Build the effective column order: configured order + any tags
        // in the data that weren't listed + "other" bucket if enabled.
        let mut effective_order: Vec<String> = self
            .config
            .column_order
            .iter()
            .filter(|col| column_members.contains_key(*col))
            .cloned()
            .collect();
        let listed: std::collections::HashSet<&String> = self.config.column_order.iter().collect();
        let mut extras_cols: Vec<&String> = column_members
            .keys()
            .filter(|k| !listed.contains(*k))
            .collect();
        extras_cols.sort();
        for col in extras_cols {
            effective_order.push(col.clone());
        }
        let other_col = "_other_".to_string();
        if self.config.include_other_column && !unassigned.is_empty() {
            effective_order.push(other_col.clone());
        }

        let mut targets: HashMap<N, Point2D<f32>> = HashMap::with_capacity(scene.nodes.len());
        for (col_idx, col_name) in effective_order.iter().enumerate() {
            let members = if col_name == &other_col {
                &unassigned
            } else {
                column_members.get(col_name).unwrap()
            };
            let col_x = self.config.origin.x + col_idx as f32 * self.config.column_gap;
            for (row_idx, member_id) in members.iter().enumerate() {
                let row_y = self.config.origin.y + row_idx as f32 * self.config.row_gap;
                targets.insert((*member_id).clone(), Point2D::new(col_x, row_y));
            }
        }

        emit_targets(scene, targets, state, extras)
    }
}

// ── Shared emit helper ────────────────────────────────────────────────────────

fn emit_targets<N>(
    scene: &CanvasSceneInput<N>,
    mut targets: HashMap<N, Point2D<f32>>,
    state: &StaticLayoutState,
    extras: &LayoutExtras<N>,
) -> HashMap<N, Vector2D<f32>>
where
    N: Clone + Eq + Hash,
{
    let damping = state.damping.clamp(0.0, 1.0);
    let mut deltas = HashMap::with_capacity(targets.len());
    for node in &scene.nodes {
        if extras.pinned.contains(&node.id) {
            continue;
        }
        let Some(target) = targets.remove(&node.id) else {
            continue;
        };
        let delta = (target - node.position) * damping;
        if delta.length() > f32::EPSILON {
            deltas.insert(node.id.clone(), delta);
        }
    }
    deltas
}

#[cfg(test)]
mod tests {
    use super::*;
    use graph_canvas::projection::ProjectionMode;
    use graph_canvas::scene::{CanvasEdge, CanvasNode, SceneMode, ViewId};
    use euclid::default::{Rect, Size2D};

    fn viewport() -> CanvasViewport {
        CanvasViewport {
            rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
            scale_factor: 1.0,
        }
    }

    fn scene(nodes: Vec<(u32, f32, f32)>) -> CanvasSceneInput<u32> {
        CanvasSceneInput {
            view_id: ViewId(0),
            nodes: nodes
                .into_iter()
                .map(|(id, x, y)| CanvasNode {
                    id,
                    position: Point2D::new(x, y),
                    radius: 16.0,
                    label: None,
                })
                .collect(),
            edges: Vec::<CanvasEdge<u32>>::new(),
            scene_objects: Vec::new(),
            overlays: Vec::new(),
            scene_mode: SceneMode::Browse,
            projection: ProjectionMode::default(),
        }
    }

    #[test]
    fn timeline_orders_nodes_by_numeric_axis() {
        let mut layout = Timeline::new(TimelineConfig {
            origin: Point2D::new(0.0, 0.0),
            axis_length: 100.0,
            row_gap: 20.0,
            fallback: TimelineFallback::LeaveInPlace,
        });
        let mut state = StaticLayoutState::default();
        let input = scene(vec![(0, 50.0, 50.0), (1, 50.0, 50.0), (2, 50.0, 50.0)]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.axis_value_by_node.insert(0, AxisValue::Numeric(0.0));
        extras
            .axis_value_by_node
            .insert(1, AxisValue::Numeric(50.0));
        extras
            .axis_value_by_node
            .insert(2, AxisValue::Numeric(100.0));

        let deltas = layout.step(&input, &mut state, 0.0, &viewport(), &extras);
        let pos_0 = input.nodes[0].position + deltas[&0];
        let pos_1 = input.nodes[1].position + deltas[&1];
        let pos_2 = input.nodes[2].position + deltas[&2];

        assert!(pos_0.x < pos_1.x);
        assert!(pos_1.x < pos_2.x);
    }

    #[test]
    fn timeline_fallback_leaves_unassigned_in_place() {
        let mut layout = Timeline::new(TimelineConfig::default());
        let input = scene(vec![(0, 123.0, 456.0), (1, 0.0, 0.0)]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras
            .axis_value_by_node
            .insert(1, AxisValue::Numeric(10.0));
        let deltas = layout.step(
            &input,
            &mut StaticLayoutState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(!deltas.contains_key(&0));
    }

    #[test]
    fn kanban_buckets_nodes_into_configured_columns() {
        let mut layout = Kanban::new(KanbanConfig {
            origin: Point2D::new(0.0, 0.0),
            column_gap: 100.0,
            row_gap: 30.0,
            column_order: vec!["todo".into(), "doing".into(), "done".into()],
            include_other_column: false,
        });
        // Non-zero starting positions so zero-length deltas don't drop
        // silently from the result map.
        let input = scene(vec![
            (0, 1000.0, 1000.0),
            (1, 1001.0, 1000.0),
            (2, 1002.0, 1000.0),
        ]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras
            .axis_value_by_node
            .insert(0, AxisValue::Categorical("doing".into()));
        extras
            .axis_value_by_node
            .insert(1, AxisValue::Categorical("todo".into()));
        extras
            .axis_value_by_node
            .insert(2, AxisValue::Categorical("done".into()));

        let deltas = layout.step(
            &input,
            &mut StaticLayoutState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        // todo column (col 0) gets node 1; doing (col 1) gets node 0; done (col 2) gets node 2.
        let pos_0 = input.nodes[0].position + deltas[&0];
        let pos_1 = input.nodes[1].position + deltas[&1];
        let pos_2 = input.nodes[2].position + deltas[&2];
        assert!(pos_1.x < pos_0.x);
        assert!(pos_0.x < pos_2.x);
    }

    #[test]
    fn kanban_places_unassigned_in_other_column_when_enabled() {
        let mut layout = Kanban::new(KanbanConfig {
            origin: Point2D::new(0.0, 0.0),
            column_gap: 100.0,
            row_gap: 30.0,
            column_order: vec!["todo".into()],
            include_other_column: true,
        });
        let input = scene(vec![(0, 1000.0, 1000.0), (1, 1001.0, 1000.0)]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras
            .axis_value_by_node
            .insert(0, AxisValue::Categorical("todo".into()));
        // node 1 has no axis value.
        let deltas = layout.step(
            &input,
            &mut StaticLayoutState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        let pos_0 = input.nodes[0].position + deltas[&0];
        let pos_1 = input.nodes[1].position + deltas[&1];
        // "other" column sits right of "todo".
        assert!(pos_0.x < pos_1.x);
    }

    #[test]
    fn kanban_pinned_nodes_skipped() {
        let mut layout = Kanban::new(KanbanConfig {
            column_order: vec!["a".into()],
            ..Default::default()
        });
        let input = scene(vec![(0, 0.0, 0.0)]);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.pinned.insert(0);
        extras
            .axis_value_by_node
            .insert(0, AxisValue::Categorical("a".into()));
        let deltas = layout.step(
            &input,
            &mut StaticLayoutState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(!deltas.contains_key(&0));
    }
}
