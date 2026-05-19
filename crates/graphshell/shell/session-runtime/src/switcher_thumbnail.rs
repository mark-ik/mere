/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Switcher-row thumbnail primitives per framing brief §5.5.
//!
//! v0a (this module) walks a [`Graph`] and produces typed geometry
//! the host paints into a switcher row. Layout is straight
//! fit-to-bounds — no cartography strategy in the loop yet.
//! v0b swaps in cartography's `FormFactor::Minimap` projection
//! once strategies are live in the host. See the
//! [switcher thumbnails plan](../../../../design_docs/mere_docs/implementation_strategy/2026-05-14_switcher_thumbnails_plan.md).
//!
//! Output stays gpui-free and `Color`-free — the host maps
//! `family_tag` to `platen::family_color` so switcher rows and the
//! live orrery share one palette story.

use mere_kernel::geometry::PortablePoint;
use mere_kernel::graph::Graph;

/// Caller config for [`build_switcher_thumbnail`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitcherThumbnailOptions {
    /// Output box dimensions in pixels. Default 200×120 per the
    /// framing brief §5.5.
    pub width: u32,
    pub height: u32,
    /// Cap on rendered nodes. When a graph has more, v0a strides
    /// the node iterator deterministically; v0b switches to
    /// importance-weighted sampling once cartography is host-side.
    /// `0` disables the cap.
    pub max_nodes: usize,
    /// Per-node radius in thumbnail pixels. v0 uses one size for
    /// every node; per-node sizing comes from cartography signals
    /// later.
    pub node_radius: f32,
    /// Inset (pixels) reserved on every edge of the thumbnail so
    /// nodes that land at the bounds don't paint through the row's
    /// border.
    pub padding: f32,
}

impl Default for SwitcherThumbnailOptions {
    fn default() -> Self {
        Self {
            width: 200,
            height: 120,
            max_nodes: 64,
            node_radius: 3.0,
            padding: 4.0,
        }
    }
}

/// One node in the projected thumbnail. Position is in
/// thumbnail-local pixels with origin at the top-left of the
/// thumbnail box (matches gpui paint conventions).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThumbnailNode {
    pub position: PortablePoint,
    pub radius: f32,
}

/// One projected edge segment. `family_tag` is the top byte of
/// `RelationKind::tag()` (the `EdgeFamily` ordinal) — the host's
/// renderer maps it through `platen::family_color`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThumbnailEdge {
    pub from: PortablePoint,
    pub to: PortablePoint,
    pub family_tag: u8,
}

/// Typed thumbnail output. The host paints `nodes` as circles and
/// `edges` as line segments. Dimensions echo the input options so
/// the renderer can lay out around it without re-asking.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SwitcherThumbnail {
    pub width: u32,
    pub height: u32,
    pub nodes: Vec<ThumbnailNode>,
    pub edges: Vec<ThumbnailEdge>,
}

/// Build a thumbnail of `graph` at the dimensions in `options`. The
/// projection is a straight aspect-preserving fit of `graph.nodes()`'
/// axis-aligned bounding box into the thumbnail box (less padding).
/// Degenerate cases (no nodes, single node, zero-extent bounds) are
/// handled without division-by-zero.
pub fn build_switcher_thumbnail(
    graph: &Graph,
    options: SwitcherThumbnailOptions,
) -> SwitcherThumbnail {
    let mut thumbnail = SwitcherThumbnail {
        width: options.width,
        height: options.height,
        nodes: Vec::new(),
        edges: Vec::new(),
    };

    // Snapshot node positions + apply max_nodes cap by deterministic
    // stride. v0b will route through cartography for importance-
    // weighted sampling; stride is fine for the v0a "skeleton"
    // intent.
    let raw: Vec<(_, PortablePoint)> = graph
        .nodes()
        .map(|(key, node)| (key, node.projected_position()))
        .collect();
    let total = raw.len();
    if total == 0 {
        return thumbnail;
    }
    let stride = if options.max_nodes == 0 || total <= options.max_nodes {
        1
    } else {
        // Ceiling division so we stay at or under the cap.
        total.div_ceil(options.max_nodes)
    };
    let kept: std::collections::HashMap<_, PortablePoint> = raw
        .iter()
        .enumerate()
        .filter_map(|(i, (key, pos))| (i % stride == 0).then_some((*key, *pos)))
        .collect();

    // Compute world bounds. With one surviving node we centre it;
    // with N>1 we fit-to-bounds.
    let (min_x, max_x, min_y, max_y) = bounds(&kept);

    let inset_w = (options.width as f32 - 2.0 * options.padding).max(1.0);
    let inset_h = (options.height as f32 - 2.0 * options.padding).max(1.0);

    let project = |p: PortablePoint| -> PortablePoint {
        let span_x = (max_x - min_x).max(f32::EPSILON);
        let span_y = (max_y - min_y).max(f32::EPSILON);
        // Aspect-preserving fit. Pick the tighter axis so neither
        // overflows.
        let scale = (inset_w / span_x).min(inset_h / span_y);
        let scaled_w = span_x * scale;
        let scaled_h = span_y * scale;
        // Centre the scaled bbox in the padded inset.
        let offset_x = options.padding + (inset_w - scaled_w) * 0.5;
        let offset_y = options.padding + (inset_h - scaled_h) * 0.5;
        // Translate into the bbox-origin frame, then project.
        let local_x = p.x - min_x;
        let local_y = p.y - min_y;
        PortablePoint::new(offset_x + local_x * scale, offset_y + local_y * scale)
    };

    // Centre the lone surviving node when extent collapses, instead
    // of running the degenerate division path.
    let single_node_position = if kept.len() == 1
        || (max_x - min_x).abs() < f32::EPSILON && (max_y - min_y).abs() < f32::EPSILON
    {
        Some(PortablePoint::new(
            options.width as f32 * 0.5,
            options.height as f32 * 0.5,
        ))
    } else {
        None
    };

    thumbnail.nodes = kept
        .iter()
        .map(|(_, pos)| ThumbnailNode {
            position: single_node_position.unwrap_or_else(|| project(*pos)),
            radius: options.node_radius,
        })
        .collect();

    // Edges: only include those whose endpoints survived the cap.
    thumbnail.edges = graph
        .relations()
        .filter_map(|view| {
            if view.from == view.to {
                return None;
            }
            let from = kept.get(&view.from)?;
            let to = kept.get(&view.to)?;
            Some(ThumbnailEdge {
                from: single_node_position.unwrap_or_else(|| project(*from)),
                to: single_node_position.unwrap_or_else(|| project(*to)),
                family_tag: ((view.kind.tag() >> 24) & 0xff) as u8,
            })
        })
        .collect();

    thumbnail
}

fn bounds<K>(positions: &std::collections::HashMap<K, PortablePoint>) -> (f32, f32, f32, f32) {
    if positions.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut iter = positions.values();
    let first = iter.next().unwrap();
    let mut min_x = first.x;
    let mut max_x = first.x;
    let mut min_y = first.y;
    let mut max_y = first.y;
    for p in iter {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    (min_x, max_x, min_y, max_y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere_kernel::graph::{EdgeAssertion, EdgeFamily, RelationKind, SemanticSubKind};

    fn hyperlink() -> EdgeAssertion {
        EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::Hyperlink,
            label: None,
            decay_progress: None,
        }
    }

    #[test]
    fn empty_graph_yields_empty_thumbnail_with_dimensions_preserved() {
        let graph = Graph::new();
        let opts = SwitcherThumbnailOptions::default();
        let t = build_switcher_thumbnail(&graph, opts);
        assert_eq!(t.width, opts.width);
        assert_eq!(t.height, opts.height);
        assert!(t.nodes.is_empty());
        assert!(t.edges.is_empty());
    }

    #[test]
    fn single_node_lands_at_thumbnail_center() {
        let mut graph = Graph::new();
        graph.add_node("https://a.test".into(), PortablePoint::new(42.0, 99.0));
        let opts = SwitcherThumbnailOptions::default();
        let t = build_switcher_thumbnail(&graph, opts);
        assert_eq!(t.nodes.len(), 1);
        let n = t.nodes[0];
        assert!((n.position.x - opts.width as f32 * 0.5).abs() < 0.001);
        assert!((n.position.y - opts.height as f32 * 0.5).abs() < 0.001);
    }

    #[test]
    fn multi_node_fit_keeps_nodes_inside_thumbnail_box() {
        let mut graph = Graph::new();
        let _a = graph.add_node("a".into(), PortablePoint::new(-500.0, -500.0));
        let _b = graph.add_node("b".into(), PortablePoint::new(500.0, 500.0));
        let _c = graph.add_node("c".into(), PortablePoint::new(0.0, 250.0));
        let opts = SwitcherThumbnailOptions::default();
        let t = build_switcher_thumbnail(&graph, opts);
        assert_eq!(t.nodes.len(), 3);
        for n in &t.nodes {
            assert!(n.position.x >= opts.padding);
            assert!(n.position.x <= opts.width as f32 - opts.padding);
            assert!(n.position.y >= opts.padding);
            assert!(n.position.y <= opts.height as f32 - opts.padding);
        }
    }

    #[test]
    fn fit_preserves_aspect_ratio() {
        // Square world bbox 100x100 → fitted into a 200x120 box.
        // The thinner axis (height=120 − 8 padding = 112) limits;
        // the result hugs y but leaves slack on x.
        let mut graph = Graph::new();
        let _ = graph.add_node("a".into(), PortablePoint::new(0.0, 0.0));
        let _ = graph.add_node("b".into(), PortablePoint::new(100.0, 100.0));
        let opts = SwitcherThumbnailOptions::default();
        let t = build_switcher_thumbnail(&graph, opts);
        let mut by_x: Vec<f32> = t.nodes.iter().map(|n| n.position.x).collect();
        let mut by_y: Vec<f32> = t.nodes.iter().map(|n| n.position.y).collect();
        by_x.sort_by(|a, b| a.partial_cmp(b).unwrap());
        by_y.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let dx = by_x[1] - by_x[0];
        let dy = by_y[1] - by_y[0];
        assert!((dx - dy).abs() < 0.01, "aspect preserved: dx={dx} dy={dy}");
    }

    #[test]
    fn hyperlink_edge_yields_family_tag_for_semantic() {
        let mut graph = Graph::new();
        let a = graph.add_node("a".into(), PortablePoint::new(0.0, 0.0));
        let b = graph.add_node("b".into(), PortablePoint::new(100.0, 0.0));
        graph.assert_relation(a, b, hyperlink()).unwrap();
        let t = build_switcher_thumbnail(&graph, SwitcherThumbnailOptions::default());
        assert_eq!(t.edges.len(), 1);
        // RelationKind::tag()'s top byte is the family ordinal. The
        // Semantic family is 0 per the kernel's tag scheme; the
        // thumbnail's family_tag mirrors it 1:1 so the host's palette
        // maps consistently.
        assert_eq!(
            t.edges[0].family_tag,
            expected_family_tag(EdgeFamily::Semantic)
        );
    }

    #[test]
    fn multi_relation_pair_yields_distinct_family_tags() {
        let mut graph = Graph::new();
        let a = graph.add_node("a".into(), PortablePoint::new(0.0, 0.0));
        let b = graph.add_node("b".into(), PortablePoint::new(100.0, 0.0));
        graph.assert_relation(a, b, hyperlink()).unwrap();
        graph
            .assert_relation(
                a,
                b,
                EdgeAssertion::Provenance {
                    sub_kind: mere_kernel::graph::ProvenanceSubKind::ClippedFrom,
                },
            )
            .unwrap();

        let t = build_switcher_thumbnail(&graph, SwitcherThumbnailOptions::default());
        assert_eq!(t.edges.len(), 2);
        let tags: std::collections::HashSet<u8> = t.edges.iter().map(|e| e.family_tag).collect();
        assert!(tags.contains(&expected_family_tag(EdgeFamily::Semantic)));
        assert!(tags.contains(&expected_family_tag(EdgeFamily::Provenance)));
    }

    #[test]
    fn max_nodes_cap_strides_deterministically() {
        let mut graph = Graph::new();
        for i in 0..20 {
            graph.add_node(
                format!("n{i}"),
                PortablePoint::new(i as f32, (i % 5) as f32),
            );
        }
        let opts = SwitcherThumbnailOptions {
            max_nodes: 5,
            ..SwitcherThumbnailOptions::default()
        };
        let t = build_switcher_thumbnail(&graph, opts);
        assert!(t.nodes.len() <= 5, "node count was {}", t.nodes.len());
    }

    #[test]
    fn cap_drops_edges_whose_endpoints_were_culled() {
        // Build 4 nodes with one edge between the first two and one
        // between the last two; cap at 2 with a stride that keeps
        // only the first two.
        let mut graph = Graph::new();
        let n0 = graph.add_node("0".into(), PortablePoint::new(0.0, 0.0));
        let n1 = graph.add_node("1".into(), PortablePoint::new(1.0, 0.0));
        let n2 = graph.add_node("2".into(), PortablePoint::new(2.0, 0.0));
        let n3 = graph.add_node("3".into(), PortablePoint::new(3.0, 0.0));
        graph.assert_relation(n0, n1, hyperlink()).unwrap();
        graph.assert_relation(n2, n3, hyperlink()).unwrap();

        // With max_nodes=2 over 4 nodes, stride=2 keeps indices 0
        // and 2. Only one endpoint of each edge survives, so both
        // edges are dropped.
        let opts = SwitcherThumbnailOptions {
            max_nodes: 2,
            ..SwitcherThumbnailOptions::default()
        };
        let t = build_switcher_thumbnail(&graph, opts);
        assert_eq!(t.nodes.len(), 2);
        assert!(t.edges.is_empty());
    }

    #[test]
    fn self_loops_are_filtered_out() {
        let mut graph = Graph::new();
        let a = graph.add_node("a".into(), PortablePoint::new(0.0, 0.0));
        graph.assert_relation(a, a, hyperlink()).unwrap();
        let t = build_switcher_thumbnail(&graph, SwitcherThumbnailOptions::default());
        assert_eq!(t.nodes.len(), 1);
        assert!(t.edges.is_empty());
    }

    /// Helper that derives what `family_tag` *should* be for a given
    /// `EdgeFamily` via the kernel's tag scheme. Decouples the test
    /// from the literal byte values so a future tag-scheme tweak
    /// doesn't silently flip the meaning.
    fn expected_family_tag(family: EdgeFamily) -> u8 {
        let representative = match family {
            EdgeFamily::Semantic => RelationKind::Semantic(SemanticSubKind::Hyperlink),
            EdgeFamily::Traversal => RelationKind::Traversal,
            EdgeFamily::Containment => {
                RelationKind::Containment(mere_kernel::graph::ContainmentSubKind::UrlPath)
            }
            EdgeFamily::Arrangement => {
                RelationKind::Arrangement(mere_kernel::graph::ArrangementSubKind::FrameMember)
            }
            EdgeFamily::Imported => {
                RelationKind::Imported(mere_kernel::graph::ImportedSubKind::BookmarkFolder)
            }
            EdgeFamily::Provenance => {
                RelationKind::Provenance(mere_kernel::graph::ProvenanceSubKind::ClippedFrom)
            }
        };
        ((representative.tag() >> 24) & 0xff) as u8
    }
}
