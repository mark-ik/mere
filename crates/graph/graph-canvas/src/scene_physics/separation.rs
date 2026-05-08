/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Node-node circle separation pass.
//!
//! Iteratively pushes overlapping nodes apart so their padded radii do not
//! intersect. Pinned nodes are immovable. Returns a per-node position delta
//! the host applies to its own position storage.

use euclid::default::{Point2D, Vector2D};
use std::collections::HashMap;
use std::hash::Hash;

use super::NodeSnapshot;

/// Compute node separation deltas using circle-circle collision detection.
///
/// Returns a map of node ID → position delta for nodes that overlap.
/// Uses iterative multi-pass resolution. Pinned nodes are immovable;
/// unpinned nodes share the overlap equally.
pub fn compute_node_separation<N: Clone + Eq + Hash>(
    nodes: &[NodeSnapshot<N>],
    padding: f32,
    passes: u32,
) -> HashMap<N, Vector2D<f32>> {
    if nodes.len() < 2 {
        return HashMap::new();
    }

    let mut positions: HashMap<N, Point2D<f32>> =
        nodes.iter().map(|n| (n.id.clone(), n.position)).collect();

    for _ in 0..passes {
        let mut changed = false;
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let id_a = &nodes[i].id;
                let id_b = &nodes[j].id;
                let pos_a = positions[id_a];
                let pos_b = positions[id_b];
                let radius_a = nodes[i].radius + padding;
                let radius_b = nodes[j].radius + padding;
                let min_distance = radius_a + radius_b;

                let delta = pos_b - pos_a;
                let distance = delta.length();

                if distance >= min_distance {
                    continue;
                }

                let normal = if distance > f32::EPSILON {
                    delta / distance
                } else {
                    Vector2D::new(1.0, 0.0)
                };
                let overlap = (min_distance - distance).max(0.0) + 0.5;
                if overlap <= f32::EPSILON {
                    continue;
                }

                let a_pinned = nodes[i].pinned;
                let b_pinned = nodes[j].pinned;
                if a_pinned && b_pinned {
                    continue;
                }

                if a_pinned {
                    positions.insert(id_b.clone(), pos_b + normal * overlap);
                } else if b_pinned {
                    positions.insert(id_a.clone(), pos_a - normal * overlap);
                } else {
                    let push = normal * (overlap * 0.5);
                    positions.insert(id_a.clone(), pos_a - push);
                    positions.insert(id_b.clone(), pos_b + push);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    nodes
        .iter()
        .filter_map(|node| {
            let next = positions.get(&node.id)?;
            let delta = *next - node.position;
            (delta.length() > f32::EPSILON).then_some((node.id.clone(), delta))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: u32, x: f32, y: f32, radius: f32) -> NodeSnapshot<u32> {
        NodeSnapshot {
            id,
            position: Point2D::new(x, y),
            radius,
            pinned: false,
        }
    }

    fn snap_pinned(id: u32, x: f32, y: f32, radius: f32) -> NodeSnapshot<u32> {
        NodeSnapshot {
            id,
            position: Point2D::new(x, y),
            radius,
            pinned: true,
        }
    }

    #[test]
    fn no_separation_when_far_apart() {
        let nodes = vec![snap(0, 0.0, 0.0, 10.0), snap(1, 100.0, 0.0, 10.0)];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(deltas.is_empty());
    }

    #[test]
    fn separation_pushes_overlapping_nodes() {
        let nodes = vec![snap(0, 0.0, 0.0, 10.0), snap(1, 15.0, 0.0, 10.0)];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(!deltas.is_empty());
        if let Some(d0) = deltas.get(&0) {
            assert!(d0.x < 0.0);
        }
        if let Some(d1) = deltas.get(&1) {
            assert!(d1.x > 0.0);
        }
    }

    #[test]
    fn pinned_node_is_immovable() {
        let nodes = vec![snap_pinned(0, 0.0, 0.0, 10.0), snap(1, 15.0, 0.0, 10.0)];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(deltas.get(&0).is_none());
        assert!(deltas.get(&1).is_some());
    }

    #[test]
    fn both_pinned_no_movement() {
        let nodes = vec![
            snap_pinned(0, 0.0, 0.0, 10.0),
            snap_pinned(1, 5.0, 0.0, 10.0),
        ];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(deltas.is_empty());
    }

    #[test]
    fn single_node_no_separation() {
        let nodes = vec![snap(0, 0.0, 0.0, 10.0)];
        let deltas = compute_node_separation(&nodes, 4.0, 3);
        assert!(deltas.is_empty());
    }
}
