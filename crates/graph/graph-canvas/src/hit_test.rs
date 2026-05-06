/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Hit testing against projected scene hit proxies.
//!
//! Given a screen-space point, resolve which node or edge (if any) the
//! pointer is over. This is the interaction engine's spatial query layer.

use euclid::default::Point2D;
use std::hash::Hash;

use crate::packet::HitProxy;
use crate::scripting::SceneObjectId;

/// Result of a hit test: what the pointer is over.
#[derive(Debug, Clone, PartialEq)]
pub enum HitTestResult<N> {
    Node(N),
    Edge { source: N, target: N },
    SceneObject(SceneObjectId),
    None,
}

/// Hit test a screen-space point against a slice of hit proxies.
///
/// Returns the first (topmost) hit. Hit proxies are tested in reverse order
/// so that later-drawn items (which are visually on top) take priority.
pub fn hit_test_point<N: Clone + Eq + Hash>(
    screen_pos: Point2D<f32>,
    hit_proxies: &[HitProxy<N>],
) -> HitTestResult<N> {
    // Test in reverse order so visually-topmost items win.
    for proxy in hit_proxies.iter().rev() {
        match proxy {
            HitProxy::Node { id, center, radius } => {
                let dx = screen_pos.x - center.x;
                let dy = screen_pos.y - center.y;
                if dx * dx + dy * dy <= radius * radius {
                    return HitTestResult::Node(id.clone());
                }
            }
            HitProxy::Edge {
                source,
                target,
                midpoint,
                half_width,
            } => {
                let dx = screen_pos.x - midpoint.x;
                let dy = screen_pos.y - midpoint.y;
                if dx.abs() <= *half_width && dy.abs() <= *half_width {
                    return HitTestResult::Edge {
                        source: source.clone(),
                        target: target.clone(),
                    };
                }
            }
            HitProxy::SceneObject { id, center, radius } => {
                let dx = screen_pos.x - center.x;
                let dy = screen_pos.y - center.y;
                if dx * dx + dy * dy <= radius * radius {
                    return HitTestResult::SceneObject(*id);
                }
            }
        }
    }
    HitTestResult::None
}

/// Collect all node ids whose hit proxies contain the given screen-space rect
/// (for lasso selection). Tests node center containment, not radius overlap.
pub fn nodes_in_screen_rect<N: Clone + Eq + Hash>(
    rect_min: Point2D<f32>,
    rect_max: Point2D<f32>,
    hit_proxies: &[HitProxy<N>],
) -> Vec<N> {
    let min_x = rect_min.x.min(rect_max.x);
    let max_x = rect_min.x.max(rect_max.x);
    let min_y = rect_min.y.min(rect_max.y);
    let max_y = rect_min.y.max(rect_max.y);

    hit_proxies
        .iter()
        .filter_map(|proxy| match proxy {
            HitProxy::Node { id, center, .. } => {
                if center.x >= min_x && center.x <= max_x && center.y >= min_y && center.y <= max_y
                {
                    Some(id.clone())
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_proxies() -> Vec<HitProxy<u32>> {
        vec![
            HitProxy::Node {
                id: 0,
                center: Point2D::new(100.0, 100.0),
                radius: 16.0,
            },
            HitProxy::Node {
                id: 1,
                center: Point2D::new(200.0, 200.0),
                radius: 16.0,
            },
            HitProxy::Node {
                id: 2,
                center: Point2D::new(300.0, 100.0),
                radius: 16.0,
            },
        ]
    }

    #[test]
    fn hit_test_node_direct() {
        let proxies = sample_proxies();
        let result = hit_test_point(Point2D::new(100.0, 100.0), &proxies);
        assert_eq!(result, HitTestResult::Node(0));
    }

    #[test]
    fn hit_test_node_edge_of_radius() {
        let proxies = sample_proxies();
        // Just inside radius (16.0) of node 0 at (100, 100).
        let result = hit_test_point(Point2D::new(115.0, 100.0), &proxies);
        assert_eq!(result, HitTestResult::Node(0));
    }

    #[test]
    fn hit_test_miss() {
        let proxies = sample_proxies();
        let result = hit_test_point(Point2D::new(500.0, 500.0), &proxies);
        assert_eq!(result, HitTestResult::None);
    }

    #[test]
    fn hit_test_topmost_wins() {
        // Two overlapping nodes — later one wins.
        let proxies = vec![
            HitProxy::Node {
                id: 0,
                center: Point2D::new(100.0, 100.0),
                radius: 20.0,
            },
            HitProxy::Node {
                id: 1,
                center: Point2D::new(105.0, 100.0),
                radius: 20.0,
            },
        ];
        let result = hit_test_point(Point2D::new(102.0, 100.0), &proxies);
        assert_eq!(result, HitTestResult::Node(1));
    }

    #[test]
    fn lasso_collects_contained_nodes() {
        let proxies = sample_proxies();
        let result = nodes_in_screen_rect(
            Point2D::new(50.0, 50.0),
            Point2D::new(250.0, 250.0),
            &proxies,
        );
        assert!(result.contains(&0));
        assert!(result.contains(&1));
        assert!(!result.contains(&2)); // center at (300, 100) outside rect
    }

    #[test]
    fn lasso_inverted_rect_still_works() {
        let proxies = sample_proxies();
        // min/max swapped — should still work.
        let result = nodes_in_screen_rect(
            Point2D::new(250.0, 250.0),
            Point2D::new(50.0, 50.0),
            &proxies,
        );
        assert!(result.contains(&0));
        assert!(result.contains(&1));
    }

    #[test]
    fn lasso_empty_on_no_match() {
        let proxies = sample_proxies();
        let result = nodes_in_screen_rect(
            Point2D::new(400.0, 400.0),
            Point2D::new(500.0, 500.0),
            &proxies,
        );
        assert!(result.is_empty());
    }

    #[test]
    fn hit_test_empty_proxies() {
        let result = hit_test_point::<u32>(Point2D::new(100.0, 100.0), &[]);
        assert_eq!(result, HitTestResult::None);
    }

    #[test]
    fn hit_test_scene_object() {
        let proxies = vec![HitProxy::<u32>::SceneObject {
            id: SceneObjectId(5),
            center: Point2D::new(150.0, 150.0),
            radius: 20.0,
        }];
        let result = hit_test_point(Point2D::new(155.0, 150.0), &proxies);
        assert_eq!(result, HitTestResult::SceneObject(SceneObjectId(5)));
    }

    #[test]
    fn hit_test_scene_object_miss() {
        let proxies = vec![HitProxy::<u32>::SceneObject {
            id: SceneObjectId(5),
            center: Point2D::new(150.0, 150.0),
            radius: 10.0,
        }];
        let result = hit_test_point(Point2D::new(300.0, 300.0), &proxies);
        assert_eq!(result, HitTestResult::None);
    }

    #[test]
    fn hit_test_scene_object_topmost_wins_over_node() {
        let proxies = vec![
            HitProxy::Node {
                id: 0u32,
                center: Point2D::new(100.0, 100.0),
                radius: 20.0,
            },
            HitProxy::SceneObject {
                id: SceneObjectId(1),
                center: Point2D::new(105.0, 100.0),
                radius: 20.0,
            },
        ];
        // Point overlaps both — scene object is later, so it wins.
        let result = hit_test_point(Point2D::new(102.0, 100.0), &proxies);
        assert_eq!(result, HitTestResult::SceneObject(SceneObjectId(1)));
    }

    #[test]
    fn lasso_ignores_scene_objects() {
        let proxies = vec![
            HitProxy::Node {
                id: 0u32,
                center: Point2D::new(100.0, 100.0),
                radius: 16.0,
            },
            HitProxy::<u32>::SceneObject {
                id: SceneObjectId(1),
                center: Point2D::new(120.0, 100.0),
                radius: 16.0,
            },
        ];
        let result = nodes_in_screen_rect(
            Point2D::new(50.0, 50.0),
            Point2D::new(200.0, 200.0),
            &proxies,
        );
        // Only the node should be collected, not the scene object.
        assert_eq!(result, vec![0]);
    }
}
