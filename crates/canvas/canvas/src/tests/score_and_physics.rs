// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Projection scores: rebinding a representation, what `frame` then paints, and
//! how a playing arrangement composes with the global physics.

use super::*;

fn one_item_score(id: uuid::Uuid, representation: sceno::Representation) -> sceno::Score {
    let mut score = sceno::Score::new(sceno::Arrangement::Spiral(sceno::Spiral::default()));
    score.items.push(sceno::ScoreItem {
        source: sceno::SourceRef::new(::cartography::MERE_GRAPH_ADAPTER, id.to_string()),
        ordinal: 0,
        footprint: sceno::Footprint::Circle { radius: 18.0 },
        representation,
        placement: sceno::Placement::Ordinal,
        layer: 0,
        visible: true,
        axis: None,
        embedding: None,
        weight: None,
    });
    score
}

#[test]
fn projection_score_rebinds_representation_and_graph_swap_clears_it() {
    let mut canvas = Canvas::new();
    let key = canvas.visit("https://representation.example");
    let id = canvas.graph().get_node(key).unwrap().id;
    canvas.set_projection_score(Some(one_item_score(id, sceno::Representation::LivePane)));
    assert_eq!(
        canvas.projection_representation(key),
        Some(&sceno::Representation::LivePane),
        "the opaque score source is rebound at the product boundary",
    );

    canvas.set_graph(Graph::new());
    assert!(canvas.projection_score().is_none());
    assert!(canvas.projection_representations.is_empty());
}

#[test]
fn score_representation_changes_the_class_painted_by_frame() {
    let mut canvas = Canvas::new();
    let key = canvas.visit("https://face.example");
    let id = canvas.graph().get_node(key).unwrap().id;
    let gnode = canvas.gnode_of[&key];
    let ns = Namespace::from("");
    let class = LocalName::from("class");

    for (representation, expected) in [
        (sceno::Representation::Glyph, "gnode-representation-glyph"),
        (sceno::Representation::Card, "gnode-representation-card"),
        (
            sceno::Representation::LivePane,
            "gnode-representation-live-pane",
        ),
    ] {
        canvas.set_projection_score(Some(one_item_score(id, representation)));
        canvas.frame(800, 600);
        let painted = canvas
            .node_document
            .dom()
            .attribute(gnode, &ns, &class)
            .expect("frame assigns a gnode class");
        assert!(
            painted
                .split_ascii_whitespace()
                .any(|class| class == expected),
            "the score rung must reach the painted DOM class: {painted}",
        );
    }
}

#[test]
fn a_playing_arrangement_pulls_as_a_field_not_an_override() {
    // The arrangement stops being an authority and becomes a participant: while
    // playing, its slots are anchor springs the graph's own forces argue with.
    let mut canvas = Canvas::new();
    canvas.visit("https://pull-a.example");
    canvas.visit("https://pull-b.example");
    let keys: Vec<_> = canvas.graph().nodes().map(|(k, _)| k).collect();
    let slots: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (*k, PortablePoint::new(i as f32 * 40.0, 0.0)))
        .collect();

    canvas.set_layout_strategy(Some("phyllotaxis.default".to_string()));
    canvas.apply_strategy_positions(&slots);
    // Paused: the placement is asserted directly, so no anchor force is needed.
    assert!(canvas.physics_paused());
    assert_eq!(canvas.physics.anchor_count(), 0, "paused needs no pull");

    // Playing: the same slots become springs.
    canvas.set_physics_paused(false);
    assert_eq!(
        canvas.physics.anchor_count(),
        slots.len(),
        "a playing arrangement anchors its slots"
    );

    // Zero pull is the seed-only reading: the arrangement is an initial
    // condition and the graph's own forces take over entirely.
    canvas.set_arrangement_pull(0.0);
    assert_eq!(canvas.physics.anchor_count(), 0, "no pull, no anchors");
    assert_eq!(
        canvas.layout_strategy(),
        Some("phyllotaxis.default"),
        "the arrangement stays selected either way"
    );
}

#[test]
fn physics_is_global_and_composes_with_any_arrangement() {
    // Physics is a capability of the graph, not a property of one layout mode:
    // every arrangement composes with either state, and "force-directed" is
    // simply no analytic arrangement with physics running.
    let mut canvas = Canvas::new();
    canvas.visit("https://phys-a.example");
    canvas.visit("https://phys-b.example");
    assert!(!canvas.physics_paused(), "runs by default");

    // Picking an arrangement pauses *visibly* (the same flag the user drives),
    // so the analytic placement reads crisply on selection.
    canvas.set_layout_strategy(Some("phyllotaxis.default".to_string()));
    assert!(canvas.physics_paused(), "an arrangement pauses by default");

    // ...but the pause is ordinary state: play it and the arrangement stays
    // selected while the sim takes over from its seeded placement.
    canvas.set_physics_paused(false);
    assert!(!canvas.physics_paused());
    assert_eq!(
        canvas.layout_strategy(),
        Some("phyllotaxis.default"),
        "playing physics does not deselect the arrangement"
    );

    // And a graph with no arrangement can be frozen — the other half of the
    // matrix, impossible while physics was welded to the mode.
    canvas.set_layout_strategy(None);
    assert!(!canvas.physics_paused(), "reverting resumes");
    canvas.set_physics_paused(true);
    assert!(canvas.physics_paused() && canvas.layout_strategy().is_none());
}

#[test]
fn a_restored_score_survives_the_hosts_next_recompute_check() {
    // The host gates its per-frame projection on `needs_strategy_recompute`.
    // A restore leaves the input cache empty, so without the restored-score
    // claim the very next frame recomputes the arrangement from *live* inputs
    // and the saved score never paints — the restore looks green while showing
    // a different layout.
    let mut canvas = Canvas::new();
    let first = canvas.visit("https://hold-first.example");
    let second = canvas.visit("https://hold-second.example");
    let ids = [
        canvas.graph().get_node(first).unwrap().id,
        canvas.graph().get_node(second).unwrap().id,
    ];
    let mut score = sceno::Score::new(sceno::Arrangement::Spiral(sceno::Spiral::default()));
    score.items = ids
        .into_iter()
        .enumerate()
        .map(|(ordinal, id)| sceno::ScoreItem {
            source: sceno::SourceRef::new(::cartography::MERE_GRAPH_ADAPTER, id.to_string()),
            ordinal: ordinal as u32,
            footprint: sceno::Footprint::Circle { radius: 18.0 },
            representation: sceno::Representation::Glyph,
            placement: sceno::Placement::Ordinal,
            layer: 0,
            visible: true,
            axis: None,
            embedding: None,
            weight: None,
        })
        .collect();
    assert!(canvas.restore_projection_score(score));

    assert!(
        !canvas.needs_strategy_recompute("phyllotaxis.default", 800, 600, None),
        "the restored score holds the layout through the host's next check"
    );

    // A real input change releases the claim, so new content still re-lays out.
    canvas.visit("https://hold-third.example");
    assert!(
        canvas.needs_strategy_recompute("phyllotaxis.default", 800, 600, None),
        "a graph change releases the restored score's claim"
    );
}
