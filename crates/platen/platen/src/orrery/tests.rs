/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Orrery projection tests.

use super::*;
use kernel::graph::fixtures::GraphFixtures;
use kernel::graph::{
    COUPLING_VOCAB, Coupling, CouplingId, CouplingResponse, EdgeAssertion, Field,
    FieldDefinition, FieldId, NodeSelector, ScalarField, SemanticSubKind,
};
use paint_list_api::{PaintCmd, PaintList};
use uuid::Uuid;

fn node(g: &mut Graph, id: u128, x: f32, y: f32) -> kernel::graph::NodeKey {
    let key = g.add_node_with_id(Uuid::from_u128(id), format!("mere://{id}"), PortablePoint::new(x, y));
    g.set_node_position(key, PortablePoint::new(x, y));
    key
}

fn draw_rect_count(list: &CanvasPaintList) -> usize {
    list.commands()
        .iter()
        .filter(|c| matches!(c, PaintCmd::DrawRect(_)))
        .count()
}

#[test]
fn projection_from_graph_carries_positions_and_dedupes_edges() {
    let mut g = Graph::new();
    let a = node(&mut g, 1, 10.0, 20.0);
    let b = node(&mut g, 2, 30.0, 40.0);
    // Two relations on the same pair (both directions) collapse to one edge.
    let _ = g.assert_relation(a, b, hyperlink());
    let _ = g.assert_relation(b, a, hyperlink());

    let p = projection_from_graph(&g);
    assert_eq!(p.nodes.len(), 2);
    let pa = p.nodes.iter().find(|n| n.node == a).unwrap();
    assert_eq!(pa.position, PortablePoint::new(10.0, 20.0));
    assert_eq!(p.edges.len(), 1, "the bidirectional pair de-dupes to one edge");
    // content_bounds spans the two nodes.
    assert_eq!(p.content_bounds.min_x(), 10.0);
    assert_eq!(p.content_bounds.max_y(), 40.0);
}

#[test]
fn empty_graph_projects_empty() {
    let p = projection_from_graph(&Graph::new());
    assert!(p.nodes.is_empty() && p.edges.is_empty());
}

#[test]
fn demoted_underlay_draws_only_the_off_screen_nodes() {
    // Two connected nodes; demote only `a`. The underlay must draw a's rect
    // but not b's, while still drawing the edge (which needs both positions).
    let mut g = Graph::new();
    let a = node(&mut g, 1, 0.0, 0.0);
    let b = node(&mut g, 2, 100.0, 0.0);
    let _ = g.assert_relation(a, b, hyperlink());

    let full = orrery_paint_list_demoted(
        &g,
        |k| Some(if k == a { PortablePoint::new(0.0, 0.0) } else { PortablePoint::new(100.0, 0.0) }),
        |_| true,
        |_| true,
        DeviceIntSize::new(200, 200),
        Camera::default(),
        &ScenePaintStyle::default(),
        0,
    );
    assert_eq!(draw_rect_count(&full), 2, "both nodes drawn when all demoted");

    let one = orrery_paint_list_demoted(
        &g,
        |k| Some(if k == a { PortablePoint::new(0.0, 0.0) } else { PortablePoint::new(100.0, 0.0) }),
        |k| k == a,
        |_| true,
        DeviceIntSize::new(200, 200),
        Camera::default(),
        &ScenePaintStyle::default(),
        0,
    );
    assert_eq!(draw_rect_count(&one), 1, "only the demoted node draws a rect");
    // The edge still renders (one DrawStroke) even though b's rect is culled.
    let strokes = one
        .commands()
        .iter()
        .filter(|c| matches!(c, PaintCmd::DrawStroke(_)))
        .count();
    assert_eq!(strokes, 1, "the edge to the on-screen node still draws");
}

#[test]
fn edge_visibility_predicate_hides_relations() {
    // Two connected nodes; hiding the relation drops the edge stroke but keeps
    // the node rects — the host's edge-visibility filter in action.
    let mut g = Graph::new();
    let a = node(&mut g, 1, 0.0, 0.0);
    let b = node(&mut g, 2, 100.0, 0.0);
    let _ = g.assert_relation(a, b, hyperlink());

    let strokes = |edge_visible: fn(&RelationView) -> bool| {
        orrery_paint_list_demoted(
            &g,
            |k| Some(if k == a { PortablePoint::new(0.0, 0.0) } else { PortablePoint::new(100.0, 0.0) }),
            |_| true,
            edge_visible,
            DeviceIntSize::new(200, 200),
            Camera::default(),
            &ScenePaintStyle::default(),
            0,
        )
        .commands()
        .iter()
        .filter(|c| matches!(c, PaintCmd::DrawStroke(_)))
        .count()
    };

    assert_eq!(strokes(|_| true), 1, "the edge draws when visible");
    assert_eq!(strokes(|_| false), 0, "hiding the relation drops the edge");
}

#[test]
fn projection_from_positions_overrides_committed_and_falls_back() {
    let mut g = Graph::new();
    let a = node(&mut g, 1, 10.0, 20.0); // committed (10, 20)
    let b = node(&mut g, 2, 30.0, 40.0); // committed (30, 40)
    // A live position for `a` only; `b` has none → falls back to committed.
    let live =
        move |k: NodeKey| (k == a).then_some(PortablePoint::new(99.0, 99.0));
    let p = projection_from_positions(&g, live);

    let pa = p.nodes.iter().find(|n| n.node == a).unwrap();
    let pb = p.nodes.iter().find(|n| n.node == b).unwrap();
    assert_eq!(pa.position, PortablePoint::new(99.0, 99.0), "a uses the live position");
    assert_eq!(pb.position, PortablePoint::new(30.0, 40.0), "b falls back to committed");
    assert_eq!(p.metadata.strategy_id.as_deref(), Some("orrery.live"));
}

#[test]
fn orrery_paint_list_composes_nodes_and_a_visual_overlay() {
    // One node at the origin, a Gaussian field peaking there, and a visual/halo
    // coupling over it. The paint list should carry the node rect *and* the
    // resolved halo overlay — the underlay producer wiring the field's light in.
    let mut g = Graph::new();
    node(&mut g, 1, 0.0, 0.0);

    // Baseline: no coupling → just the node rect.
    let baseline = orrery_paint_list(
        &g,
        DeviceIntSize::new(200, 200),
        Camera::default(),
        &ScenePaintStyle::default(),
        0,
    );
    assert_eq!(draw_rect_count(&baseline), 1, "node rect only");

    // Add the field + a visual/halo coupling over all nodes.
    let fid = FieldId::from_uuid(Uuid::from_u128(0xF1));
    g.add_field(Field::new(
        fid,
        FieldDefinition::Scalar(ScalarField::gaussian_at(0.0, 0.0, 50.0)),
    ));
    g.add_coupling(Coupling::new(
        CouplingId::from_uuid(Uuid::from_u128(1)),
        fid,
        NodeSelector::All,
        CouplingResponse::open(format!("{COUPLING_VOCAB}visual/halo")),
        1.0,
    ));

    let with_halo = orrery_paint_list(
        &g,
        DeviceIntSize::new(200, 200),
        Camera::default(),
        &ScenePaintStyle::default(),
        0,
    );
    assert_eq!(
        draw_rect_count(&with_halo),
        2,
        "node rect + the resolved visual/halo overlay"
    );
    assert!(matches!(
        with_halo.commands().last(),
        Some(PaintCmd::PopTransform)
    ));
}

#[test]
fn identity_arrangement_projects_byte_identically_to_the_whole_graph() {
    // Routing the orrery through its Identity arrangement must not change a pixel:
    // the member set, order, positions, and edges all match the whole-graph path.
    let mut g = Graph::new();
    let a = node(&mut g, 1, 10.0, 20.0);
    let b = node(&mut g, 2, 30.0, 40.0);
    let c = node(&mut g, 3, 50.0, 60.0);
    let _ = g.assert_relation(a, b, hyperlink());
    let _ = g.assert_relation(b, c, hyperlink());

    let pos = |k: NodeKey| (k == a).then_some(PortablePoint::new(99.0, 99.0));
    let via_graph = projection_from_positions(&g, pos);
    let via_arrangement = projection_from_arrangement(&g, &identity_arrangement(&g), pos);

    let tuples = |p: &Projection| {
        p.nodes.iter().map(|n| (n.node, n.position)).collect::<Vec<_>>()
    };
    assert_eq!(
        tuples(&via_arrangement),
        tuples(&via_graph),
        "same nodes + positions, in the same (graph) order"
    );
    assert_eq!(via_arrangement.edges.len(), via_graph.edges.len(), "same edges");
    assert_eq!(via_arrangement.content_bounds.min_x(), via_graph.content_bounds.min_x());
    assert_eq!(via_arrangement.content_bounds.max_y(), via_graph.content_bounds.max_y());
}

#[test]
fn curated_arrangement_projects_only_its_members() {
    // The seam generalizes: a curated (non-Identity) arrangement shows only its
    // members, the affordance the orrery-through-platen routing unlocks.
    let mut g = Graph::new();
    let a = node(&mut g, 1, 0.0, 0.0);
    let _b = node(&mut g, 2, 10.0, 10.0);
    let c = node(&mut g, 3, 20.0, 20.0);
    let arr = Arrangement::identity([Uuid::from_u128(1), Uuid::from_u128(3)]);
    let p = projection_from_arrangement(&g, &arr, |_| None);
    let keys: HashSet<NodeKey> = p.nodes.iter().map(|n| n.node).collect();
    assert_eq!(p.nodes.len(), 2, "only the curated members project");
    assert!(keys.contains(&a) && keys.contains(&c), "members 1 and 3, not 2");
}

#[test]
fn projected_edges_dedup_per_pair_and_weight_by_multiplicity() {
    // A line a-b-c: each pair has one hyperlink => two edges at unit weight.
    let mut g = Graph::new();
    let a = node(&mut g, 1, 0.0, 0.0);
    let b = node(&mut g, 2, 1.0, 0.0);
    let c = node(&mut g, 3, 2.0, 0.0);
    let _ = g.assert_relation(a, b, hyperlink());
    let _ = g.assert_relation(b, c, hyperlink());
    let edges = projected_undirected_edges(&g, &|_| true);
    assert_eq!(edges.len(), 2, "one edge per connected pair");
    assert!(edges.iter().all(|e| (e.weight - 1.0).abs() < 1e-6), "a single-link pair is unit weight");

    // A reciprocal pair (a->b and b->a) is two relations on the same undirected pair => the
    // edge weighs 2 (the multigraph multiplicity), so it paints thicker. (Graph signals.)
    let mut g2 = Graph::new();
    let x = node(&mut g2, 10, 0.0, 0.0);
    let y = node(&mut g2, 11, 1.0, 0.0);
    let _ = g2.assert_relation(x, y, hyperlink());
    let _ = g2.assert_relation(y, x, hyperlink());
    let edges = projected_undirected_edges(&g2, &|_| true);
    assert_eq!(edges.len(), 1, "still one edge for the pair");
    assert!((edges[0].weight - 2.0).abs() < 1e-6, "two relations on the pair => weight 2");
}

fn hyperlink() -> EdgeAssertion {
    EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
    }
}
