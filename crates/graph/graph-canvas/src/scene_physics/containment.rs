/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Viewport containment pass.
//!
//! Clamps unpinned nodes to stay within a given bounding rect, respecting a
//! per-node radius and an additional padding. Returns a per-node position
//! delta the host applies.

use euclid::default::{Point2D, Rect, Vector2D};
use std::collections::HashMap;
use std::hash::Hash;

use super::NodeSnapshot;

/// Compute viewport containment deltas.
///
/// Clamps nodes to stay within the given bounds rect, respecting padding.
/// Returns a map of node ID → position delta for nodes outside bounds.
pub fn compute_viewport_containment<N: Clone + Eq + Hash>(
    nodes: &[NodeSnapshot<N>],
    bounds: Rect<f32>,
    padding: f32,
    response_scale: f32,
) -> HashMap<N, Vector2D<f32>> {
    let mut deltas = HashMap::new();
    for node in nodes {
        if node.pinned {
            continue;
        }
        let inset = node.radius + padding;
        if bounds.size.width <= inset * 2.0 || bounds.size.height <= inset * 2.0 {
            continue;
        }
        let min_x = bounds.origin.x + inset;
        let max_x = bounds.origin.x + bounds.size.width - inset;
        let min_y = bounds.origin.y + inset;
        let max_y = bounds.origin.y + bounds.size.height - inset;
        let clamped = Point2D::new(
            node.position.x.clamp(min_x, max_x),
            node.position.y.clamp(min_y, max_y),
        );
        let delta = (clamped - node.position) * response_scale.max(0.0);
        if delta.length() > f32::EPSILON {
            deltas.insert(node.id.clone(), delta);
        }
    }
    deltas
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::Size2D;

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
    fn node_inside_bounds_no_delta() {
        let nodes = vec![snap(0, 100.0, 100.0, 10.0)];
        let bounds = Rect::new(Point2D::new(0.0, 0.0), Size2D::new(200.0, 200.0));
        let deltas = compute_viewport_containment(&nodes, bounds, 4.0, 1.0);
        assert!(deltas.is_empty());
    }

    #[test]
    fn node_outside_bounds_gets_pushed_in() {
        let nodes = vec![snap(0, 250.0, 100.0, 10.0)];
        let bounds = Rect::new(Point2D::new(0.0, 0.0), Size2D::new(200.0, 200.0));
        let deltas = compute_viewport_containment(&nodes, bounds, 4.0, 1.0);
        assert!(deltas.contains_key(&0));
        assert!(deltas[&0].x < 0.0);
    }

    #[test]
    fn pinned_node_not_contained() {
        let nodes = vec![snap_pinned(0, 250.0, 100.0, 10.0)];
        let bounds = Rect::new(Point2D::new(0.0, 0.0), Size2D::new(200.0, 200.0));
        let deltas = compute_viewport_containment(&nodes, bounds, 4.0, 1.0);
        assert!(deltas.is_empty());
    }
}
