// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The physics catalog: every id builds, every label is plain, a switch does
//! not move a body, and the choice survives a graph reconcile.

use super::*;
use crate::physics_catalog::{
    CANVAS_PHYSICS_DEPTH_SOURCES, CANVAS_PHYSICS_KIND_SOURCES, CANVAS_PHYSICS_LAWS,
    CANVAS_PHYSICS_MASS_SOURCES, CANVAS_PHYSICS_OVERLAYS, CANVAS_PHYSICS_PROFILES,
    PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource, PhysicsOverlay,
};

/// A canvas with named nodes joined by directed semantic relations, for the
/// petgraph-backed sources. A pair listed more than once gets a different
/// relation kind each time (asserting one kind twice is idempotent), so it
/// carries that many relation cells.
fn wired(urls: &[&str], edges: &[(usize, usize)]) -> (Canvas, Vec<NodeKey>) {
    const KINDS: [SemanticSubKind; 4] = [
        SemanticSubKind::Cites,
        SemanticSubKind::Quotes,
        SemanticSubKind::Summarizes,
        SemanticSubKind::Elaborates,
    ];
    let mut canvas = Canvas::new();
    let ids: Vec<uuid::Uuid> = urls
        .iter()
        .map(|url| canvas.open_member_as_new_node(None, url))
        .collect();
    let mut seen: HashMap<(usize, usize), usize> = HashMap::new();
    for &(a, b) in edges {
        let nth = seen.entry((a, b)).or_default();
        assert!(canvas.assert_relation_between_members(ids[a], ids[b], KINDS[*nth]));
        *nth += 1;
    }
    let keys = ids
        .iter()
        .map(|id| canvas.graph().get_node_by_id(*id).unwrap().0)
        .collect();
    (canvas, keys)
}

fn lookup<T: Copy>(table: &[(NodeKey, T)], key: NodeKey) -> T {
    table
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap()
}

/// A label in the plain register: a short capitalised phrase, no ids or
/// technical punctuation.
fn is_plain(label: &str) -> bool {
    !label.is_empty()
        && label.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && label.len() <= 16
        && !label.contains(['.', '_', '-', '(', ')'])
}

fn positions(canvas: &Canvas) -> Vec<(NodeKey, Point2D<f32>)> {
    let mut all: Vec<_> = canvas
        .graph()
        .nodes()
        .map(|(key, _)| (key, canvas.view.position_of(key).unwrap_or_default()))
        .collect();
    all.sort_by_key(|(key, _)| key.index());
    all
}

#[test]
fn every_law_id_round_trips_and_its_label_is_plain() {
    assert_eq!(CANVAS_PHYSICS_LAWS.len(), PhysicsLaw::ALL.len());
    for (id, label) in CANVAS_PHYSICS_LAWS {
        let law = PhysicsLaw::parse(id).unwrap_or_else(|| panic!("{id} parses"));
        assert_eq!(law.id(), *id);
        assert_eq!(law.label(), *label);
        assert!(is_plain(label), "{label} is plain");
    }
    assert_eq!(CANVAS_PHYSICS_OVERLAYS.len(), PhysicsOverlay::ALL.len());
    for (id, label) in CANVAS_PHYSICS_OVERLAYS {
        let overlay = PhysicsOverlay::parse(id).unwrap_or_else(|| panic!("{id} parses"));
        assert_eq!(overlay.id(), *id);
        assert_eq!(overlay.label(), *label);
        assert!(is_plain(label), "{label} is plain");
    }
    assert_eq!(
        CANVAS_PHYSICS_KIND_SOURCES.len(),
        PhysicsKindSource::ALL.len()
    );
    for (id, label) in CANVAS_PHYSICS_KIND_SOURCES {
        let source = PhysicsKindSource::parse(id).unwrap_or_else(|| panic!("{id} parses"));
        assert_eq!(source.id(), *id);
        assert_eq!(source.label(), *label);
        assert!(is_plain(label), "{label} is plain");
    }
    assert_eq!(
        CANVAS_PHYSICS_MASS_SOURCES.len(),
        PhysicsMassSource::ALL.len()
    );
    for (id, label) in CANVAS_PHYSICS_MASS_SOURCES {
        let source = PhysicsMassSource::parse(id).unwrap_or_else(|| panic!("{id} parses"));
        assert_eq!(source.id(), *id);
        assert_eq!(source.label(), *label);
        assert!(is_plain(label), "{label} is plain");
    }
    assert_eq!(
        CANVAS_PHYSICS_DEPTH_SOURCES.len(),
        PhysicsDepthSource::ALL.len()
    );
    for (id, label) in CANVAS_PHYSICS_DEPTH_SOURCES {
        let source = PhysicsDepthSource::parse(id).unwrap_or_else(|| panic!("{id} parses"));
        assert_eq!(source.id(), *id);
        assert_eq!(source.label(), *label);
        assert!(is_plain(label), "{label} is plain");
    }
    assert!(PhysicsLaw::parse("spring").is_none());
    assert!(PhysicsOverlay::parse("").is_none());
}

/// A proper colouring never puts a kind beside itself; islands each get one.
#[test]
fn coloring_and_island_kinds_read_the_topology() {
    // A path a-b-c, and an island d.
    let (canvas, keys) = wired(
        &[
            "https://a.test",
            "https://b.test",
            "https://c.test",
            "https://d.test",
        ],
        &[(0, 1), (1, 2)],
    );
    let inputs = canvas.law_inputs();
    let coloring = inputs.coloring_groups();
    assert_ne!(lookup(&coloring, keys[0]), lookup(&coloring, keys[1]));
    assert_ne!(lookup(&coloring, keys[1]), lookup(&coloring, keys[2]));
    assert_eq!(
        lookup(&coloring, keys[0]),
        lookup(&coloring, keys[2]),
        "a path is two-colourable"
    );
    let islands = inputs.component_groups();
    assert_eq!(lookup(&islands, keys[0]), lookup(&islands, keys[2]));
    assert_ne!(lookup(&islands, keys[0]), lookup(&islands, keys[3]));
    let (kinds, count) = inputs.kinds(PhysicsKindSource::Coloring);
    assert_eq!(count, 2);
    assert_ne!(lookup(&kinds, keys[0]), lookup(&kinds, keys[1]));
}

/// The most linked-to node outranks its linkers, and the weights average one.
#[test]
fn page_rank_weights_favour_the_linked_to() {
    let (canvas, keys) = wired(
        &[
            "https://a.test",
            "https://b.test",
            "https://c.test",
            "https://hub.test",
        ],
        &[(0, 3), (1, 3), (2, 3)],
    );
    let inputs = canvas.law_inputs();
    let weights = inputs.page_rank_weights();
    let hub = lookup(&weights, keys[3]);
    assert!(
        hub > lookup(&weights, keys[0]) * 2.0,
        "the hub outranks a linker: {hub}"
    );
    let mean = weights.iter().map(|(_, w)| w).sum::<f32>() / weights.len() as f32;
    assert!((mean - 1.0).abs() < 1e-3, "weights average one, got {mean}");
    // And it is the mass source Orbit and the hub overlays read.
    let mut canvas = canvas;
    canvas.set_physics_overlays(vec![PhysicsOverlay::HubGravity]);
    canvas.set_physics_mass_source(PhysicsMassSource::PageRank);
    assert!(
        canvas.physics_forces_are_graph_bound(),
        "ranked hub weights follow the topology"
    );
    canvas.set_physics_law(PhysicsLaw::Orbit);
    assert!(canvas.law_force_count() >= 3);
}

/// Layers survive a cycle and order a chain; dominators count from the focus.
#[test]
fn layer_and_focus_depths_order_the_graph() {
    // a -> b -> c -> a (a cycle), c -> d, and e unreachable.
    let (mut canvas, keys) = wired(
        &[
            "https://a.test",
            "https://b.test",
            "https://c.test",
            "https://d.test",
            "https://e.test",
        ],
        &[(0, 1), (1, 2), (2, 0), (2, 3)],
    );
    let inputs = canvas.law_inputs();
    let layers = inputs.layer_depths();
    let (a, b, c, d) = (
        lookup(&layers, keys[0]),
        lookup(&layers, keys[1]),
        lookup(&layers, keys[2]),
        lookup(&layers, keys[3]),
    );
    assert!(d > c, "d sits below c: {d} > {c}");
    // One arc of the cycle is cut; the other two still order their ends.
    let ordered = [(a, b), (b, c), (c, a)]
        .iter()
        .filter(|(x, y)| y > x)
        .count();
    assert_eq!(
        ordered, 2,
        "two of the three cycle arcs keep their order: {a} {b} {c}"
    );
    // Depth from the focus counts dominators; the unreachable node sits below all.
    let focus = inputs.focus_depths(Some(keys[0]));
    assert_eq!(lookup(&focus, keys[0]), 0);
    assert_eq!(lookup(&focus, keys[1]), 1);
    assert_eq!(lookup(&focus, keys[2]), 2);
    assert_eq!(lookup(&focus, keys[3]), 3);
    assert_eq!(
        lookup(&focus, keys[4]),
        4,
        "the unreachable node is one below the deepest"
    );
    // Without a focus the Focus source is the Roots source.
    assert_eq!(inputs.focus_depths(None), inputs.root_depths());
    drop(inputs);
    canvas.set_physics_overlays(vec![PhysicsOverlay::DepthGravity]);
    canvas.set_physics_depth_source(PhysicsDepthSource::Layers);
    assert_eq!(canvas.law_force_count(), 4, "springs and the depth overlay");
}

/// The skeleton is a spanning tree that prefers the pairs with more relations,
/// and Stress reads those pairs as shorter.
#[test]
fn skeleton_and_weighted_stress_read_multiplicity() {
    // A triangle a-b-c, with b-c joined three times.
    let (mut canvas, keys) = wired(
        &["https://a.test", "https://b.test", "https://c.test"],
        &[(0, 1), (0, 2), (1, 2), (1, 2), (1, 2)],
    );
    let inputs = canvas.law_inputs();
    let tree = inputs.skeleton_edges();
    assert_eq!(
        tree.len(),
        2,
        "a spanning tree of three nodes has two edges"
    );
    let joins = |x: NodeKey, y: NodeKey| {
        tree.iter()
            .any(|(p, q)| (*p == x && *q == y) || (*p == y && *q == x))
    };
    assert!(
        joins(keys[1], keys[2]),
        "the thrice-joined pair is in the tree"
    );
    let distances = inputs.weighted_distances();
    let dist = |x: NodeKey, y: NodeKey| {
        distances
            .iter()
            .find(|(p, q, _)| (*p == x && *q == y) || (*p == y && *q == x))
            .map(|(_, _, d)| *d)
            .unwrap()
    };
    assert!(
        (dist(keys[1], keys[2]) - 1.0 / 3.0).abs() < 1e-5,
        "three relations are a third of a hop"
    );
    assert!((dist(keys[0], keys[1]) - 1.0).abs() < 1e-5);
    drop(inputs);
    canvas.set_physics_law(PhysicsLaw::Still);
    canvas.set_physics_overlays(vec![PhysicsOverlay::Skeleton]);
    assert_eq!(canvas.law_force_count(), 1);
    assert!(
        canvas.physics_forces_are_graph_bound(),
        "the tree follows the topology"
    );
    assert!(canvas.apply_physics_profile("skeleton"));
    assert_eq!(canvas.physics_profile_id(), Some("skeleton"));
}

#[test]
fn every_law_and_overlay_builds_on_the_sample_graph_without_moving_a_body() {
    let mut canvas = Canvas::with_sample_graph();
    canvas.set_physics_paused(true);
    let before = positions(&canvas);
    for law in PhysicsLaw::ALL {
        canvas.set_physics_law(law);
        assert_eq!(canvas.physics_law(), law);
        let expected_min = if law == PhysicsLaw::Still { 0 } else { 1 };
        assert!(
            canvas.law_force_count() >= expected_min,
            "{} builds its forces",
            law.id()
        );
        assert_eq!(
            positions(&canvas),
            before,
            "{} switch moved a body",
            law.id()
        );
    }
    for overlay in PhysicsOverlay::ALL {
        assert!(
            canvas.toggle_physics_overlay(overlay),
            "{} toggles on",
            overlay.id()
        );
        assert_eq!(
            positions(&canvas),
            before,
            "{} switch moved a body",
            overlay.id()
        );
    }
    assert_eq!(canvas.physics_overlays().len(), PhysicsOverlay::ALL.len());
    // Still + every overlay: exactly one force per overlay.
    canvas.set_physics_law(PhysicsLaw::Still);
    assert_eq!(canvas.law_force_count(), PhysicsOverlay::ALL.len());
    for overlay in PhysicsOverlay::ALL {
        assert!(
            !canvas.toggle_physics_overlay(overlay),
            "{} toggles off",
            overlay.id()
        );
    }
    assert_eq!(canvas.law_force_count(), 0);
    for source in PhysicsKindSource::ALL {
        canvas.set_physics_law(PhysicsLaw::Kinds);
        canvas.set_physics_kind_source(source);
        assert_eq!(canvas.physics_kind_source(), source);
        assert!(
            canvas.law_force_count() >= 2,
            "kinds by {} builds",
            source.id()
        );
    }
}

#[test]
fn every_profile_applies_and_names_itself_back() {
    let mut canvas = Canvas::with_sample_graph();
    assert!(
        !canvas.apply_physics_profile("plasma"),
        "an unknown profile is refused"
    );
    for profile in CANVAS_PHYSICS_PROFILES {
        assert!(is_plain(profile.label), "{} is plain", profile.label);
        assert!(
            canvas.apply_physics_profile(profile.id),
            "{} applies",
            profile.id
        );
        assert_eq!(canvas.physics_law(), profile.law);
        assert_eq!(canvas.physics_overlays(), profile.overlays);
        assert_eq!(
            canvas.physics_profile_id(),
            Some(profile.id),
            "{} names itself back",
            profile.id
        );
    }
    // The donor's ten come first, under their own names.
    let donor: Vec<&str> = CANVAS_PHYSICS_PROFILES
        .iter()
        .take(10)
        .map(|p| p.id)
        .collect();
    assert_eq!(
        donor,
        [
            "liquid",
            "gas",
            "solid",
            "archipelago",
            "constellation",
            "crystal",
            "tide",
            "sediment",
            "magnet",
            "void"
        ]
    );
    // And no two profiles share a (law, overlays) pair, or the picker could not name the live one.
    for (i, a) in CANVAS_PHYSICS_PROFILES.iter().enumerate() {
        for b in &CANVAS_PHYSICS_PROFILES[i + 1..] {
            assert!(
                !(a.law == b.law && a.overlays == b.overlays),
                "{} and {} coincide",
                a.id,
                b.id
            );
        }
    }
    // Every law is one pick away: bare, under its own name or a donor's.
    for law in PhysicsLaw::ALL {
        assert!(
            CANVAS_PHYSICS_PROFILES
                .iter()
                .any(|p| p.law == law && p.overlays.is_empty()),
            "{} has a bare profile",
            law.id()
        );
    }
}

#[test]
fn a_living_law_runs_until_paused_and_a_graph_bound_law_survives_a_reconcile() {
    let mut canvas = Canvas::with_sample_graph();
    canvas.set_physics_law(PhysicsLaw::Orbit);
    assert!(canvas.physics_never_rests());
    assert!(canvas.is_settling(), "orbit keeps ticking");
    canvas.set_physics_law(PhysicsLaw::Stress);
    assert!(!canvas.physics_never_rests());
    let count = canvas.law_force_count();
    canvas.visit("https://a-new-node.example");
    assert_eq!(
        canvas.physics_law(),
        PhysicsLaw::Stress,
        "the law survives a topology change"
    );
    assert_eq!(
        canvas.law_force_count(),
        count,
        "stress rebuilt against the new topology"
    );
    // A graph swap keeps the choice too: the scene restore re-applies it afterwards anyway.
    canvas.set_graph(Graph::new());
    assert_eq!(canvas.physics_law(), PhysicsLaw::Stress);
}
