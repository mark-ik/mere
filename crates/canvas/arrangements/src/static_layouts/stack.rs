// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic layered placement for directed graphs.
//!
//! `Stack` treats an arrangement as a set of target slots. It does not own
//! motion or collision; a host can snap to the slots, pin selected nodes, or
//! feed the slots to seiche as anchor springs.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use super::{StaticLayoutState, emit};
use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;
use crate::{Layout, LayoutExtras};

/// Which end of a directed relation occupies the first layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackFlow {
    /// Sources precede their targets. This follows the conventional visual
    /// reading of an arrow from left to right.
    SourcesFirst,
    /// Targets precede their sources. Useful for dependency graphs where an
    /// edge reads `consumer -> dependency`.
    TargetsFirst,
}

impl Default for StackFlow {
    fn default() -> Self {
        Self::SourcesFirst
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StackConfig {
    /// Distance between successive topology layers.
    pub layer_gap: f32,
    /// Distance between nodes within one layer.
    pub row_gap: f32,
    /// Center of the arrangement in world space.
    pub center: Point2D<f32>,
    pub flow: StackFlow,
}

impl Default for StackConfig {
    fn default() -> Self {
        Self {
            layer_gap: 180.0,
            row_gap: 110.0,
            center: Point2D::origin(),
            flow: StackFlow::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Stack {
    pub config: StackConfig,
}

impl Stack {
    pub fn new(config: StackConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for Stack
where
    N: Clone + Eq + Hash + Ord,
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

        let mut outgoing = scene
            .nodes
            .iter()
            .map(|node| (node.id.clone(), Vec::<N>::new()))
            .collect::<HashMap<_, _>>();
        let mut indegree = scene
            .nodes
            .iter()
            .map(|node| (node.id.clone(), 0usize))
            .collect::<HashMap<_, _>>();

        for edge in &scene.edges {
            let (source, target) = match self.config.flow {
                StackFlow::SourcesFirst => (&edge.source, &edge.target),
                StackFlow::TargetsFirst => (&edge.target, &edge.source),
            };
            if !outgoing.contains_key(source) || !indegree.contains_key(target) {
                continue;
            }
            outgoing
                .entry(source.clone())
                .or_default()
                .push(target.clone());
            *indegree.entry(target.clone()).or_default() += 1;
        }
        for neighbors in outgoing.values_mut() {
            neighbors.sort();
            neighbors.dedup();
        }

        let mut ready = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(id.clone()))
            .collect::<Vec<_>>();
        ready.sort();
        let mut queue = VecDeque::from(ready);
        let mut depth = scene
            .nodes
            .iter()
            .map(|node| (node.id.clone(), 0usize))
            .collect::<HashMap<_, _>>();
        let mut placed = HashSet::with_capacity(scene.nodes.len());

        while let Some(node) = queue.pop_front() {
            placed.insert(node.clone());
            let node_depth = depth[&node];
            let mut newly_ready = Vec::new();
            for target in outgoing.get(&node).into_iter().flatten() {
                depth
                    .entry(target.clone())
                    .and_modify(|value| *value = (*value).max(node_depth + 1));
                if let Some(degree) = indegree.get_mut(target) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        newly_ready.push(target.clone());
                    }
                }
            }
            newly_ready.sort();
            queue.extend(newly_ready);
        }

        // Cycles do not have a topological first layer. Keep each cyclic
        // component visible in one stable overflow layer rather than
        // inventing an arbitrary break in graph meaning.
        let overflow_depth = depth.values().copied().max().unwrap_or(0) + 1;
        for node in &scene.nodes {
            if !placed.contains(&node.id) {
                depth.insert(node.id.clone(), overflow_depth);
            }
        }

        let max_depth = depth.values().copied().max().unwrap_or(0);
        let mut layers = vec![Vec::<N>::new(); max_depth + 1];
        for node in &scene.nodes {
            layers[depth[&node.id]].push(node.id.clone());
        }
        for layer in &mut layers {
            layer.sort();
        }

        let width = max_depth as f32 * self.config.layer_gap;
        let mut targets = HashMap::with_capacity(scene.nodes.len());
        for (layer_index, layer) in layers.into_iter().enumerate() {
            let height = layer.len().saturating_sub(1) as f32 * self.config.row_gap;
            for (row_index, id) in layer.into_iter().enumerate() {
                targets.insert(
                    id,
                    Point2D::new(
                        self.config.center.x - width * 0.5
                            + layer_index as f32 * self.config.layer_gap,
                        self.config.center.y - height * 0.5
                            + row_index as f32 * self.config.row_gap,
                    ),
                );
            }
        }

        emit(scene, targets, state, extras)
    }
}
