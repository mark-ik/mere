/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tests for `build.rs` (sample graph, simulation construction, relation-edge
//! builders). Split into a sibling file per the 600-LOC ceiling.

use kernel::graph::fixtures::GraphFixtures;
use std::collections::{HashMap, HashSet};

use euclid::default::Point2D;
use kernel::graph::{
    ContainmentSubKind, EdgeAssertion, Graph, NodeKey, RelationSelector, SemanticSubKind,
};
use layout_dom_api::LayoutDomMut;
use genet_layout::IncrementalLayout;

use crate::build::*;
use crate::palette;

#[test]
fn sample_graph_has_nodes_and_edges() {
    let g = sample_graph();
    assert_eq!(g.nodes().count(), 12, "the ring has twelve nodes");
    assert!(g.relations().count() >= 12, "at least the ring edges");
}

#[test]
fn pool_has_a_gnode_per_node() {
    let g = sample_graph();
    let (_dom, gnode_of, _stage) = build_pool_dom(&g);
    assert_eq!(
        gnode_of.len(),
        g.nodes().count(),
        "the pre-materialized pool has one gnode per graph node",
    );
}

#[test]
fn simulation_has_a_body_per_node_and_the_edge_topology() {
    let g = sample_graph();
    let sim = build_simulation(&g);
    assert_eq!(sim.body_count(), 12, "one physics body per node");
    assert!(
        sim.edge_count() >= 12,
        "the spring topology carries the edges"
    );
}

#[test]
fn visible_relation_edges_keeps_one_tuple_per_cell_and_drops_hidden_ones() {
    let mut graph = Graph::new();
    let a = graph.add_node("https://a".to_string(), Point2D::new(0.0, 0.0));
    let b = graph.add_node("https://b".to_string(), Point2D::new(1.0, 0.0));
    graph.assert_relation(
        a,
        b,
        EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::Hyperlink,
            label: None,
            decay_progress: None,
        },
    );
    graph.assert_relation(
        a,
        b,
        EdgeAssertion::Containment {
            sub_kind: ContainmentSubKind::Domain,
        },
    );

    let edges = visible_relation_edges(&graph, &HashSet::new());
    assert_eq!(
        edges.len(),
        2,
        "two live relation cells between the same pair pull as two springs, not one"
    );

    let mut hidden = HashSet::new();
    hidden.insert(crate::EdgeCell {
        from: a,
        to: b,
        selector: RelationSelector::Containment(ContainmentSubKind::Domain),
    });
    let edges = visible_relation_edges(&graph, &hidden);
    assert_eq!(
        edges,
        vec![(a, b)],
        "hiding one cell drops exactly its own spring, leaving the other live"
    );
}

/// The load-bearing check for the palette-as-custom-properties move: the sheet
/// names no color, so if genet's cascade ever stopped substituting `var()` the
/// gnodes would silently lose their fill. Assert the *resolved* computed color,
/// not the sheet text.
#[test]
fn gnode_fill_resolves_from_the_palette_custom_properties() {
    let g = sample_graph();
    let (dom, gnode_of, _stage) = build_pool_dom(&g);
    let gnode = *gnode_of.values().next().expect("the pool has gnodes");

    let layout = IncrementalLayout::new(&dom, &NODE_SHEET, 800.0, 600.0);

    let idle_bg = palette::rgb(palette::IDLE.bg);
    assert_eq!(
        layout.computed_value(gnode, "background-color").as_deref(),
        Some(idle_bg.as_str()),
        "the default .gnode must resolve --node-idle-bg through the cascade",
    );
    let idle_fg = palette::rgb(palette::IDLE.fg);
    assert_eq!(
        layout.computed_value(gnode, "color").as_deref(),
        Some(idle_fg.as_str()),
        "the default .gnode must resolve --node-idle-fg through the cascade",
    );
}

/// Each activation class pulls its own `--node-*` pair, and selection wins. This
/// pins the class-to-var wiring the host relies on when it pushes node state.
#[test]
fn each_gnode_state_class_resolves_its_own_palette_entry() {
    let g = sample_graph();
    let (mut dom, gnode_of, _stage) = build_pool_dom(&g);
    let gnode = *gnode_of.values().next().expect("the pool has gnodes");

    for (class, expected) in [
        ("gnode gnode-open", palette::OPEN),
        ("gnode gnode-closed", palette::CLOSED),
        ("gnode gnode-idle", palette::IDLE),
        ("gnode gnode-selected", palette::SELECTED),
    ] {
        dom.set_attribute(gnode, qual("class"), class);
        let layout = IncrementalLayout::new(&dom, &NODE_SHEET, 800.0, 600.0);
        let want_bg = palette::rgb(expected.bg);
        let want_fg = palette::rgb(expected.fg);
        assert_eq!(
            layout.computed_value(gnode, "background-color").as_deref(),
            Some(want_bg.as_str()),
            "`{class}` must resolve its own --node-*-bg",
        );
        assert_eq!(
            layout.computed_value(gnode, "color").as_deref(),
            Some(want_fg.as_str()),
            "`{class}` must resolve its own --node-*-fg",
        );
    }
}

#[test]
fn ticking_moves_nodes_from_the_seed() {
    let g = sample_graph();
    let mut sim = build_simulation(&g);
    let before: Vec<(NodeKey, Point2D<f32>)> = sim.positions().collect();
    for _ in 0..60 {
        sim.tick(crate::TICK_DT);
    }
    let after: HashMap<NodeKey, Point2D<f32>> = sim.positions().collect();
    let moved = before.iter().any(|(k, p0)| {
        after
            .get(k)
            .is_some_and(|p1| (p1.x - p0.x).hypot(p1.y - p0.y) > 1.0)
    });
    assert!(moved, "the force-directed settle moves nodes off the seed");
}
