//! Scene-geometry queries over the live simulation: edge geometry, edge picking,
//! and marquee rect-select for the orrery's canvas hit-testing.
//!
//! Node *point* picking ([`Simulation::hit_test`]) and frustum cull
//! ([`Simulation::cull_aabb`]) live on `Simulation` directly (in `lib.rs`),
//! resolved against each node body's live position and radius. Edges add a wrinkle:
//! seiche stores them as opaque `(NodeKey, NodeKey)` pairs with no body, so a
//! body-position scan can't see them. This module adds the edge geometry (resolved
//! from the endpoints' live positions) and the picks built on it, plus a
//! rect-select that returns both nodes and edges. Split out of `lib.rs` to keep
//! both files under the workspace's per-file size ceiling.

use crate::NodeKey;
use euclid::default::{Box2D, Point2D};

use crate::view::{point_segment_distance, segment_intersects_box};
use crate::{RectSelection, Simulation};

impl Simulation {
    /// The live geometry of every edge: each `(a, b)` pair with both endpoints'
    /// current positions. Edges with a missing body (either endpoint absent, e.g.
    /// before [`Simulation::sync_nodes`]) are skipped. The orrery underlay
    /// draws edges from this; [`Simulation::edge_hit_test`] picks from it.
    /// Reflects the most recent [`Simulation::tick`].
    pub fn edge_segments(
        &self,
    ) -> impl Iterator<Item = (NodeKey, NodeKey, Point2D<f32>, Point2D<f32>)> + '_ {
        self.edges.iter().filter_map(move |&(a, b)| {
            let pa = self.position_of(a)?;
            let pb = self.position_of(b)?;
            Some((a, b, pa, pb))
        })
    }

    /// Pick the edge whose segment passes within `tolerance` (world units) of
    /// `point`, nearest first; `None` if no edge is within tolerance. The scene
    /// half of the two-hit-test split for edges (nodes go through
    /// [`Simulation::hit_test`]). On a tie the earliest edge in
    /// [`Simulation::sync_edges`] order wins.
    pub fn edge_hit_test(&self, point: Point2D<f32>, tolerance: f32) -> Option<(NodeKey, NodeKey)> {
        let mut best: Option<((NodeKey, NodeKey), f32)> = None;
        for (a, b, pa, pb) in self.edge_segments() {
            let d = point_segment_distance(point, pa, pb);
            if d <= tolerance && best.map_or(true, |(_, bd)| d < bd) {
                best = Some(((a, b), d));
            }
        }
        best.map(|(edge, _)| edge)
    }

    /// Marquee / rubber-band select over a world-space region: nodes whose center
    /// lies inside `region`, and edges whose segment intersects it. Order is
    /// unspecified. (Center-in-rect for nodes is the common rubber-band semantic;
    /// for an intersection-based node set use [`Simulation::cull_aabb`].)
    pub fn rect_select(&self, region: Box2D<f32>) -> RectSelection {
        let nodes = self
            .positions()
            .filter(|(_, p)| region.contains(*p))
            .map(|(node, _)| node)
            .collect();
        let edges = self
            .edge_segments()
            .filter(|&(_, _, pa, pb)| segment_intersects_box(pa, pb, region))
            .map(|(a, b, _, _)| (a, b))
            .collect();
        RectSelection { nodes, edges }
    }
}

#[cfg(test)]
mod tests {
    use euclid::default::{Box2D, Point2D};

    use crate::{NodeKey, Simulation};

    /// a@(0,0), b@(100,0), one edge a–b. Index refreshed so positions are live.
    fn sim_with_edge() -> (Simulation, NodeKey, NodeKey) {
        let a = NodeKey::new(0);
        let b = NodeKey::new(1);
        let mut sim = Simulation::new();
        sim.sync_nodes([(a, Point2D::new(0.0, 0.0)), (b, Point2D::new(100.0, 0.0))]);
        sim.sync_edges([(a, b)]);
        sim.refresh_spatial_index();
        (sim, a, b)
    }

    #[test]
    fn edge_segments_resolves_endpoints() {
        let (sim, a, b) = sim_with_edge();
        let segs: Vec<_> = sim.edge_segments().collect();
        assert_eq!(segs.len(), 1);
        let (sa, sb, pa, pb) = segs[0];
        assert_eq!((sa, sb), (a, b));
        assert!((pa - Point2D::new(0.0, 0.0)).length() < 0.5);
        assert!((pb - Point2D::new(100.0, 0.0)).length() < 0.5);
    }

    #[test]
    fn edge_hit_test_picks_near_segment_only() {
        let (sim, a, b) = sim_with_edge();
        // On the segment (midpoint) → hit.
        assert_eq!(
            sim.edge_hit_test(Point2D::new(50.0, 0.0), 5.0),
            Some((a, b))
        );
        // Just off it, within tolerance → hit.
        assert_eq!(
            sim.edge_hit_test(Point2D::new(50.0, 3.0), 5.0),
            Some((a, b))
        );
        // Far from it → miss.
        assert!(sim.edge_hit_test(Point2D::new(50.0, 50.0), 5.0).is_none());
        // Beyond an endpoint (past b along the line) → miss (segment, not line).
        assert!(sim.edge_hit_test(Point2D::new(200.0, 0.0), 5.0).is_none());
    }

    #[test]
    fn rect_select_returns_nodes_and_crossed_edges() {
        let (sim, a, b) = sim_with_edge();
        // Box around the origin: contains a's center, not b's; the a–b segment
        // crosses it (runs out along +x from the origin).
        let near_origin = sim.rect_select(Box2D::new(
            Point2D::new(-10.0, -10.0),
            Point2D::new(10.0, 10.0),
        ));
        assert_eq!(near_origin.nodes, vec![a]);
        assert_eq!(near_origin.edges, vec![(a, b)]);

        // A box straddling the segment's middle but no node center: no nodes, the
        // edge still selected (segment crosses it).
        let midspan = sim.rect_select(Box2D::new(
            Point2D::new(40.0, -5.0),
            Point2D::new(60.0, 5.0),
        ));
        assert!(midspan.nodes.is_empty());
        assert_eq!(midspan.edges, vec![(a, b)]);

        // A box far above the segment: nothing.
        let empty = sim.rect_select(Box2D::new(
            Point2D::new(40.0, 500.0),
            Point2D::new(60.0, 600.0),
        ));
        assert!(empty.nodes.is_empty() && empty.edges.is_empty());
    }
}
