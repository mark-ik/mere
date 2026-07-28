// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The graph ↔ seiche adapters — the canvas-side bridge that keeps seiche
//! kernel-free (S0 of the spatial completion): seiche takes plain
//! `(NodeKey, …)` lists and force values; everything that reads a kernel
//! [`Graph`] to produce them lives here. Split out of `build.rs` (which keeps
//! the DOM/paint construction helpers) when that file neared the workspace
//! size ceiling; this module also hosts the graph-resolution tests that were
//! deferred at the seiche extraction (the three `from_coupling` tests removed
//! with seiche's `kernel-bridge` feature).

use std::collections::HashSet;

use euclid::default::Point2D;
use kernel::graph::{Graph, NodeKey};
use quint::FieldRegistry;
use seiche::{Boundary, CouplingForce, EdgeSpring, NodeExclusion, Simulation};

/// The relation-cell edges that feed the layout **springs**, respecting this instance's
/// hidden-cell visibility (swatch-primitive P5 — hiding relaxes the spring in that
/// instance only, never graph truth). Unlike `dedup_edges` (one topology edge per pair,
/// for graph-algorithm layout strategies that need plain connectivity, not weight), this
/// keeps one `(NodeKey, NodeKey)` tuple per **visible** relation cell: a pair with three
/// live cells pulls three times as hard as a pair with one, and hiding one cell drops the
/// pull by exactly that cell's share. seiche stays relation-taxonomy agnostic — this
/// multiplicity is how the canvas hands it weight without leaking `RelationSelector` into
/// seiche's edge type.
pub(crate) fn visible_relation_edges(
    graph: &Graph,
    hidden_edges: &HashSet<crate::EdgeCell>,
) -> Vec<(NodeKey, NodeKey)> {
    graph
        .relations()
        .filter(|r| {
            !hidden_edges.contains(&crate::edge_cells::edge_cell_for_relation(
                r.from, r.to, r.kind,
            ))
        })
        .map(|r| (r.from, r.to))
        .collect()
}

/// Build the force-directed simulation from `graph`: a body per node, one spring
/// edge per visible relation cell as the topology (see [`visible_relation_edges`]),
/// the standard force trio (exclusion + edge-springs + a centering boundary), seeded
/// into a tight central spiral so the first settle is visible.
pub(crate) fn build_simulation(graph: &Graph) -> Simulation {
    let mut sim = Simulation::new();
    sync_sim_with_graph(&mut sim, graph);
    // No `Canvas` (and so no hidden-cell set) exists yet at construction time; the first
    // real sync happens once session/view-intent restore runs and calls `reconcile_derived`.
    sim.sync_edges(visible_relation_edges(graph, &HashSet::new()));

    sim.add_force(NodeExclusion::default());
    sim.add_force(EdgeSpring::default());
    sim.add_force(Boundary::default());
    // Field couplings: each resolves to a CouplingForce seiche integrates, so a placed
    // field's well actually pulls its nodes. The live place / move / new-node rebuild
    // re-resolves these via `Physics::set_coupling_forces`. (Field regions.)
    let coupling_forces: Vec<CouplingForce> = graph
        .couplings()
        .filter_map(|c| coupling_force_from_graph(c, graph))
        .collect();
    sim.set_coupling_forces(coupling_forces);

    sim.seed_positions(seed_cluster(graph));
    sim
}

/// Reconcile a simulation's bodies to a kernel [`Graph`]'s nodes — the canvas-side
/// bridge now that seiche is kernel-free. seiche's [`Simulation::sync_nodes`] takes
/// a `(NodeKey, position)` list; positions are no longer graph truth (S2), so new
/// bodies spawn at the origin and the caller places them via [`Simulation::
/// seed_positions`] (a spiral seed, or the host's cartography facets). Existing
/// bodies keep their simulated position. (Was `seiche::Simulation::sync_with_graph`
/// before the seiche extraction.)
pub(crate) fn sync_sim_with_graph(sim: &mut Simulation, graph: &Graph) {
    sim.sync_nodes(graph.nodes().map(|(key, _node)| (key, Point2D::zero())));
}

/// Resolve a coupling against the graph into a [`CouplingForce`] — the canvas-side
/// bridge (was `seiche::CouplingForce::from_coupling`). Looks up the field
/// definition and the selector's matching nodes, seeding the registry from the
/// whole field layer so inter-field `Sample` references resolve. `None` if the
/// field id is unknown. The captured target set is a snapshot; rebuild on graph
/// mutation.
pub(crate) fn coupling_force_from_graph(
    coupling: &kernel::graph::Coupling,
    graph: &Graph,
) -> Option<CouplingForce> {
    let field = graph.field(coupling.field)?;
    let targets: Vec<NodeKey> = graph.nodes_matching(&coupling.selector).collect();
    let mut registry = FieldRegistry::new();
    for f in graph.fields() {
        registry.insert_with_id(f.id, f.definition.clone());
    }
    Some(
        CouplingForce::new(
            coupling.response.clone(),
            coupling.strength,
            targets,
            field.definition.clone(),
        )
        .with_registry(registry),
    )
}

/// The tight central spiral (golden-angle) seed for every node, so a ticked
/// settle visibly expands it into a readable layout. Returns the `(node,
/// position)` pairs; the caller applies them to the simulation (in-thread) or
/// sends them to the physics actor (offloaded), and mirrors them into the view.
pub(crate) fn seed_cluster(graph: &Graph) -> Vec<(NodeKey, Point2D<f32>)> {
    graph
        .nodes()
        .enumerate()
        .map(|(i, (key, _node))| {
            let r = 6.0 + i as f32 * 3.0;
            let theta = i as f32 * 2.399_963; // golden angle in radians
            (key, Point2D::new(r * theta.cos(), r * theta.sin()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::geometry::PortablePoint;
    use kernel::graph::apply::{GraphDelta, apply_graph_delta};
    use kernel::graph::fixtures::GraphFixtures;
    use kernel::graph::{
        COUPLING_VOCAB, Coupling, CouplingId, CouplingResponse, Field, FieldDefinition, FieldId,
        NodeSelector, ScalarField,
    };
    use uuid::Uuid;

    fn graph_with_nodes(n: u128) -> (Graph, Vec<NodeKey>) {
        let mut graph = Graph::new();
        let keys = (0..n)
            .map(|i| {
                graph.add_node_with_id(
                    Uuid::from_u128(i + 1),
                    format!("mere://{i}"),
                    PortablePoint::zero(),
                )
            })
            .collect();
        (graph, keys)
    }

    fn gaussian_field(graph: &mut Graph, id: u128) -> FieldId {
        let fid = FieldId::from_uuid(Uuid::from_u128(id));
        graph.add_field(Field::new(
            fid,
            FieldDefinition::Scalar(ScalarField::gaussian_at(0.0, 0.0, 50.0)),
        ));
        fid
    }

    fn force_coupling(fid: FieldId, selector: NodeSelector) -> Coupling {
        Coupling::new(
            CouplingId::from_uuid(Uuid::from_u128(0xC0)),
            fid,
            selector,
            CouplingResponse::open(format!("{COUPLING_VOCAB}force/attract")),
            1.0,
        )
    }

    // The deferred graph-resolution coverage (the three `from_coupling` tests
    // that left with seiche's kernel-bridge feature), rehomed on the canvas
    // side of the bridge.

    #[test]
    fn coupling_resolves_field_and_selector_targets() {
        let (mut graph, keys) = graph_with_nodes(3);
        let fid = gaussian_field(&mut graph, 0xF1);
        let coupling = force_coupling(fid, NodeSelector::All);
        let force = coupling_force_from_graph(&coupling, &graph).expect("field id resolves");
        assert_eq!(force.target_count(), keys.len(), "All selects every node");
    }

    #[test]
    fn coupling_with_unknown_field_resolves_to_none() {
        let (graph, _) = graph_with_nodes(2);
        let coupling = force_coupling(
            FieldId::from_uuid(Uuid::from_u128(0xDEAD)),
            NodeSelector::All,
        );
        assert!(
            coupling_force_from_graph(&coupling, &graph).is_none(),
            "an unknown field id yields no force"
        );
    }

    #[test]
    fn coupling_selector_narrows_the_target_snapshot() {
        let (mut graph, keys) = graph_with_nodes(3);
        let fid = gaussian_field(&mut graph, 0xF1);
        // Tag exactly one node through the public delta path; the Tagged
        // selector's snapshot then holds just that node.
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::InsertNodeTag {
                key: keys[1],
                tag: "well".to_string(),
            },
        );
        let coupling = force_coupling(fid, NodeSelector::Tagged("well".to_string()));
        let force = coupling_force_from_graph(&coupling, &graph).expect("field id resolves");
        assert_eq!(
            force.target_count(),
            1,
            "the selector's snapshot holds exactly the tagged node"
        );
    }

    #[test]
    fn build_simulation_wires_couplings_in() {
        let (mut graph, _) = graph_with_nodes(2);
        let fid = gaussian_field(&mut graph, 0xF1);
        graph.add_coupling(force_coupling(fid, NodeSelector::All));
        let sim = build_simulation(&graph);
        assert_eq!(
            sim.coupling_force_count(),
            1,
            "the placed coupling resolves into the built simulation"
        );
    }
}
