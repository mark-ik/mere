// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Static positional layouts — pure analytic placement with no iterative
//! force state. Each layout computes a target position per node and returns
//! the delta-to-target. Hosts that want instant placement set `damping = 1.0`
//! and apply one step; hosts that want animate-in can repeatedly step with
//! `damping < 1.0` to ease into position.
//!
//! Available:
//!
//! - [`Grid`] — row-major grid with `sqrt(n)` columns
//! - [`Radial`] — BFS rings from a focal node
//! - [`Phyllotaxis`] — Fibonacci (golden-angle) spiral
//!
//! All share [`StaticLayoutState`]: a simple damping field and step count.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use super::{Layout, LayoutExtras};
use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

/// Shared state for static positional layouts. Carries a damping factor so
/// callers get full-instant placement (`damping = 1.0`) or eased motion
/// (`0.0 < damping < 1.0`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticLayoutState {
    /// Fraction of the delta-to-target applied per step. `1.0` snaps
    /// instantly; `0.2` eases in over ~20 frames.
    pub damping: f32,
    pub step_count: u64,
}

impl Default for StaticLayoutState {
    fn default() -> Self {
        Self {
            damping: 1.0,
            step_count: 0,
        }
    }
}

fn emit<N>(
    scene: &CanvasSceneInput<N>,
    mut targets: HashMap<N, Point2D<f32>>,
    state: &mut StaticLayoutState,
    extras: &LayoutExtras<N>,
) -> HashMap<N, Vector2D<f32>>
where
    N: Clone + Eq + Hash,
{
    state.step_count = state.step_count.saturating_add(1);
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

// ── Grid ──────────────────────────────────────────────────────────────────────

/// How to choose the grid's column count.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GridColumns {
    /// `columns = ceil(sqrt(n))`. Keeps the grid roughly square. Default.
    Auto,
    /// Fixed number of columns regardless of node count. Rows grow
    /// vertically. Value of 0 falls back to `Auto`.
    Explicit(u32),
    /// Choose the column count that best approximates the given
    /// width/height ratio. `2.0` prefers wide grids; `0.5` prefers tall.
    AspectRatio(f32),
}

impl Default for GridColumns {
    fn default() -> Self {
        Self::Auto
    }
}

/// Traversal order — which cell index 0 occupies, and how successive
/// indices fill the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridTraversal {
    /// Left-to-right within a row, top-to-bottom between rows.
    RowMajor,
    /// Top-to-bottom within a column, left-to-right between columns.
    ColumnMajor,
    /// Rows alternate direction (boustrophedon / ox-turning). Reduces
    /// long-distance jumps between adjacent indices.
    Snaking,
    /// Starts at the center, spirals outward. Good for priority-ordered
    /// node lists where index 0 is the most important.
    Spiral,
}

impl Default for GridTraversal {
    fn default() -> Self {
        Self::RowMajor
    }
}

/// Grid layout. Places nodes in a configurable `columns × rows`
/// arrangement with selectable traversal order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridConfig {
    /// Spacing in world units between cell centers.
    pub gap: f32,
    /// World-space origin of the grid's top-left cell (for non-spiral
    /// traversals) or center (for `Spiral`).
    pub origin: Point2D<f32>,
    pub columns: GridColumns,
    pub traversal: GridTraversal,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            gap: 120.0,
            origin: Point2D::new(0.0, 0.0),
            columns: GridColumns::default(),
            traversal: GridTraversal::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Grid {
    pub config: GridConfig,
}

impl Grid {
    pub fn new(config: GridConfig) -> Self {
        Self { config }
    }
}

fn resolve_grid_columns(columns: GridColumns, n: usize) -> usize {
    match columns {
        GridColumns::Auto => (n as f32).sqrt().ceil().max(1.0) as usize,
        GridColumns::Explicit(0) => (n as f32).sqrt().ceil().max(1.0) as usize,
        GridColumns::Explicit(c) => (c as usize).max(1),
        GridColumns::AspectRatio(ratio) => {
            // columns × rows ≈ n; columns / rows = ratio.
            // So columns ≈ sqrt(n × ratio).
            let raw = (n as f32 * ratio.max(0.01)).sqrt().ceil();
            (raw as usize).max(1).min(n.max(1))
        }
    }
}

fn grid_cell_for_index(
    traversal: GridTraversal,
    index: usize,
    columns: usize,
    total: usize,
) -> (i32, i32) {
    match traversal {
        GridTraversal::RowMajor => {
            let col = (index % columns) as i32;
            let row = (index / columns) as i32;
            (col, row)
        }
        GridTraversal::ColumnMajor => {
            let rows = (total as f32 / columns as f32).ceil() as usize;
            let rows = rows.max(1);
            let col = (index / rows) as i32;
            let row = (index % rows) as i32;
            (col, row)
        }
        GridTraversal::Snaking => {
            let row = (index / columns) as i32;
            let col_raw = (index % columns) as i32;
            let col = if row % 2 == 0 {
                col_raw
            } else {
                (columns as i32 - 1) - col_raw
            };
            (col, row)
        }
        GridTraversal::Spiral => spiral_cell(index),
    }
}

/// Square-spiral cell coordinates. `index = 0` is `(0, 0)`; successive
/// indices wind outward in a square spiral: right, down, left, left, up,
/// up, right, right, right, …
fn spiral_cell(index: usize) -> (i32, i32) {
    if index == 0 {
        return (0, 0);
    }
    let mut x = 0i32;
    let mut y = 0i32;
    let mut dx = 1i32;
    let mut dy = 0i32;
    let mut segment_len = 1i32;
    let mut steps_in_segment = 0i32;
    let mut segments_at_length = 0;
    for _ in 0..index {
        x += dx;
        y += dy;
        steps_in_segment += 1;
        if steps_in_segment == segment_len {
            steps_in_segment = 0;
            // Rotate right 90°: (dx, dy) = (dy, -dx) for clockwise spiral
            // starting rightward, then down, left, up, …
            let (new_dx, new_dy) = (dy, -dx);
            dx = new_dx;
            dy = new_dy;
            segments_at_length += 1;
            if segments_at_length == 2 {
                segments_at_length = 0;
                segment_len += 1;
            }
        }
    }
    (x, y)
}

impl<N> Layout<N> for Grid
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
        if scene.nodes.is_empty() {
            state.step_count = state.step_count.saturating_add(1);
            return HashMap::new();
        }
        let n = scene.nodes.len();
        let columns = resolve_grid_columns(self.config.columns, n);
        let mut targets: HashMap<N, Point2D<f32>> = HashMap::with_capacity(n);
        for (idx, node) in scene.nodes.iter().enumerate() {
            let (col, row) = grid_cell_for_index(self.config.traversal, idx, columns, n);
            targets.insert(
                node.id.clone(),
                Point2D::new(
                    self.config.origin.x + col as f32 * self.config.gap,
                    self.config.origin.y + row as f32 * self.config.gap,
                ),
            );
        }
        emit(scene, targets, state, extras)
    }
}

// ── Radial ────────────────────────────────────────────────────────────────────

/// How to distribute angular positions of nodes on a given ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RadialAngularPolicy {
    /// All nodes on a ring are evenly spaced. Default.
    Uniform,
    /// Nodes with higher adjacency degree get wider angular slots.
    /// Useful for showing hub-and-satellite structure.
    DegreeWeighted,
    /// Nodes placed in a stable order derived from their id's hash
    /// (deterministic but ignores graph structure).
    HashSorted,
}

impl Default for RadialAngularPolicy {
    fn default() -> Self {
        Self::Uniform
    }
}

/// How to treat nodes that are not reachable from the focal node via
/// graph adjacency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RadialUnreachablePolicy {
    /// Place unreachable nodes on an outer ring, one beyond the deepest
    /// reachable ring. Default.
    OuterRing,
    /// Collapse all unreachable nodes to `center`. Visually hides
    /// disconnected graph components.
    Center,
    /// Leave unreachable nodes at their current position (no delta
    /// emitted for them).
    LeaveInPlace,
}

impl Default for RadialUnreachablePolicy {
    fn default() -> Self {
        Self::OuterRing
    }
}

/// BFS rings from a focal node. Ring `n` is at radius `n × ring_spacing`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadialConfig<N>
where
    N: Clone + Eq + Hash,
{
    /// Focal node (radius zero).
    pub focus: Option<N>,
    /// World-space position of the focal node.
    pub center: Point2D<f32>,
    /// Radial distance between successive rings.
    pub ring_spacing: f32,
    /// Angular distribution policy within each ring.
    pub angular_policy: RadialAngularPolicy,
    /// Global rotation offset applied to every ring, in radians. Zero
    /// puts the first angular slot at the +x axis; `PI / 2.0` rotates
    /// it to +y.
    pub rotation_offset: f32,
    /// Treatment for nodes unreachable from the focal node.
    pub unreachable_policy: RadialUnreachablePolicy,
}

impl<N> Default for RadialConfig<N>
where
    N: Clone + Eq + Hash,
{
    fn default() -> Self {
        Self {
            focus: None,
            center: Point2D::new(0.0, 0.0),
            ring_spacing: 120.0,
            angular_policy: RadialAngularPolicy::default(),
            rotation_offset: 0.0,
            unreachable_policy: RadialUnreachablePolicy::default(),
        }
    }
}

#[derive(Debug)]
pub struct Radial<N>
where
    N: Clone + Eq + Hash,
{
    pub config: RadialConfig<N>,
}

impl<N> Default for Radial<N>
where
    N: Clone + Eq + Hash,
{
    fn default() -> Self {
        Self {
            config: RadialConfig::default(),
        }
    }
}

impl<N> Radial<N>
where
    N: Clone + Eq + Hash,
{
    pub fn new(config: RadialConfig<N>) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for Radial<N>
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
        if scene.nodes.is_empty() {
            state.step_count = state.step_count.saturating_add(1);
            return HashMap::new();
        }

        let Some(focus) = self.config.focus.as_ref() else {
            return HashMap::new();
        };
        if !scene.nodes.iter().any(|n| &n.id == focus) {
            return HashMap::new();
        }

        // Build undirected adjacency with degree counts.
        let mut adj: HashMap<&N, Vec<&N>> = HashMap::with_capacity(scene.nodes.len());
        for node in &scene.nodes {
            adj.entry(&node.id).or_default();
        }
        for edge in &scene.edges {
            adj.entry(&edge.source).or_default().push(&edge.target);
            adj.entry(&edge.target).or_default().push(&edge.source);
        }
        let degree_of = |id: &N| adj.get(id).map(|v| v.len()).unwrap_or(0);

        // BFS layers from focus.
        let mut layer_by_node: HashMap<&N, usize> = HashMap::new();
        let mut queue: VecDeque<&N> = VecDeque::new();
        queue.push_back(focus);
        layer_by_node.insert(focus, 0);
        while let Some(node) = queue.pop_front() {
            let current_layer = layer_by_node[node];
            if let Some(neighbors) = adj.get(node) {
                for nbr in neighbors {
                    if !layer_by_node.contains_key(*nbr) {
                        layer_by_node.insert(*nbr, current_layer + 1);
                        queue.push_back(*nbr);
                    }
                }
            }
        }

        let max_reachable_layer = layer_by_node.values().copied().max().unwrap_or(0);
        let mut layers: Vec<Vec<&N>> = Vec::new();
        layers.resize_with(max_reachable_layer + 1, Vec::new);
        let mut unreachable: Vec<&N> = Vec::new();
        for node in &scene.nodes {
            match layer_by_node.get(&node.id) {
                Some(&layer) => layers[layer].push(&node.id),
                None => unreachable.push(&node.id),
            }
        }

        let mut targets: HashMap<N, Point2D<f32>> = HashMap::with_capacity(scene.nodes.len());
        targets.insert(focus.clone(), self.config.center);

        // Place reachable rings (layer ≥ 1).
        for (layer_idx, members) in layers.iter().enumerate().skip(1) {
            if members.is_empty() {
                continue;
            }
            let radius = layer_idx as f32 * self.config.ring_spacing;
            distribute_ring(
                &mut targets,
                members,
                self.config.center,
                radius,
                self.config.rotation_offset,
                self.config.angular_policy,
                &degree_of,
            );
        }

        // Handle unreachable nodes per policy.
        match self.config.unreachable_policy {
            RadialUnreachablePolicy::OuterRing if !unreachable.is_empty() => {
                let radius = (max_reachable_layer + 1) as f32 * self.config.ring_spacing;
                distribute_ring(
                    &mut targets,
                    &unreachable,
                    self.config.center,
                    radius,
                    self.config.rotation_offset,
                    self.config.angular_policy,
                    &degree_of,
                );
            }
            RadialUnreachablePolicy::Center => {
                for id in &unreachable {
                    targets.insert((*id).clone(), self.config.center);
                }
            }
            RadialUnreachablePolicy::LeaveInPlace | RadialUnreachablePolicy::OuterRing => {
                // LeaveInPlace: no target emitted → `emit` naturally
                // skips. OuterRing with empty set: nothing to do.
            }
        }

        emit(scene, targets, state, extras)
    }
}

fn distribute_ring<N: Clone + Eq + Hash>(
    targets: &mut HashMap<N, Point2D<f32>>,
    members: &[&N],
    center: Point2D<f32>,
    radius: f32,
    rotation_offset: f32,
    policy: RadialAngularPolicy,
    degree_of: &impl Fn(&N) -> usize,
) {
    let member_count = members.len();
    if member_count == 0 {
        return;
    }

    match policy {
        RadialAngularPolicy::Uniform => {
            let step = std::f32::consts::TAU / member_count as f32;
            for (i, id) in members.iter().enumerate() {
                let angle = rotation_offset + i as f32 * step;
                let target = Point2D::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                );
                targets.insert((*id).clone(), target);
            }
        }
        RadialAngularPolicy::HashSorted => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::Hasher;
            let mut sorted: Vec<(&&N, u64)> = members
                .iter()
                .map(|id| {
                    let mut h = DefaultHasher::new();
                    (*id).hash(&mut h);
                    (id, h.finish())
                })
                .collect();
            sorted.sort_by_key(|(_, hash)| *hash);
            let step = std::f32::consts::TAU / member_count as f32;
            for (i, (id, _)) in sorted.into_iter().enumerate() {
                let angle = rotation_offset + i as f32 * step;
                let target = Point2D::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                );
                targets.insert((**id).clone(), target);
            }
        }
        RadialAngularPolicy::DegreeWeighted => {
            // Total angular weight = sum(degree + 1) so zero-degree
            // nodes still get a slot.
            let total_weight: f32 = members.iter().map(|id| (degree_of(id) + 1) as f32).sum();
            let mut cursor = rotation_offset;
            for id in members {
                let weight = (degree_of(id) + 1) as f32;
                let arc = std::f32::consts::TAU * weight / total_weight;
                let angle = cursor + arc * 0.5;
                let target = Point2D::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                );
                targets.insert((*id).clone(), target);
                cursor += arc;
            }
        }
    }
}

pub mod phyllotaxis;
pub use phyllotaxis::*;
#[cfg(test)]
mod tests;
