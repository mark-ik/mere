// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The community cache and the affinity-clustering force: structural vs content
//! signals, their blend, and the install / clear / refresh lifecycle.

use super::*;

#[test]
fn community_cache_fills_and_invalidates_on_topology_change() {
    // Two triangles {0,1,2} and {3,4,5} joined by a bridge => two Louvain communities.
    let mut graph = Graph::new();
    let n: Vec<NodeKey> = (0..6)
        .map(|i| {
            graph.add_node(
                format!("https://{i}.example"),
                PortablePoint::new(i as f32, 0.0),
            )
        })
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let mut canvas = Canvas::with_graph(graph);

    // No partition until a cluster strategy refreshes it; a non-cluster strategy is a no-op.
    assert!(canvas.community().is_none(), "no partition computed yet");
    canvas.refresh_community_cache("phyllotaxis.default");
    assert!(
        canvas.community().is_none(),
        "a non-cluster strategy does not compute community"
    );

    // Cluster-kanban fills the generation-gated cache: two communities.
    canvas.refresh_community_cache("kanban.community");
    assert_eq!(
        canvas.community().unwrap().clusters.len(),
        2,
        "two triangles => two communities"
    );

    // A topology change bumps the generation; the next refresh recomputes against the new graph,
    // so the added node appears in the partition (the cache invalidated, not stale).
    canvas.ingest_graph(|g| {
        let extra = g.add_node(
            "https://6.example".to_string(),
            PortablePoint::new(6.0, 0.0),
        );
        let a = g.get_node_by_url("https://0.example").unwrap().0;
        g.assert_semantic_predicate(a, extra, "links".to_string());
        true
    });
    canvas.refresh_community_cache("kanban.community");
    let total_members: usize = canvas
        .community()
        .unwrap()
        .clusters
        .iter()
        .map(|c| c.members.len())
        .sum();
    assert_eq!(
        total_members, 7,
        "the new node joined the recomputed partition"
    );
}

#[test]
fn cluster_by_affinity_installs_and_clears_the_affinity_force() {
    // Two triangles joined by a bridge: structural affinity is high within each triangle, so the
    // signal yields pairs. The toggle should install the force on the next frame and clear it on
    // the frame after the toggle goes off. (Graph signals — P4.)
    let mut graph = Graph::new();
    let n: Vec<NodeKey> = (0..6)
        .map(|i| {
            graph.add_node(
                format!("https://{i}.example"),
                PortablePoint::new(i as f32, 0.0),
            )
        })
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let mut canvas = Canvas::with_graph(graph);

    // Off by default: no force even after a frame.
    let _ = canvas.frame(800, 600);
    assert_eq!(canvas.affinity_pair_count(), 0, "off => no affinity force");

    // Toggle on: the next frame installs the force with the clustered pairs.
    canvas.set_cluster_by_affinity(true);
    assert!(canvas.cluster_by_affinity(), "the toggle reads on");
    let _ = canvas.frame(800, 600);
    assert!(
        canvas.affinity_pair_count() > 0,
        "the affinity force is installed with the signal's pairs"
    );

    // Toggle off: the next frame clears it.
    canvas.set_cluster_by_affinity(false);
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        0,
        "off => the force is cleared"
    );
}

#[test]
fn content_affinity_supersedes_structural_and_reverts() {
    // Two triangles + a bridge: structural affinity yields several intra-triangle pairs. Under the
    // `ContentOnly` blend mode, a host content-embedding signal (one cross-triangle pair here)
    // supersedes structural while set; clearing it (None) reverts to structural. (burn brief Lane 5
    // — P4/P6, content source under ContentOnly.)
    let mut graph = Graph::new();
    let n: Vec<NodeKey> = (0..6)
        .map(|i| {
            graph.add_node(
                format!("https://{i}.example"),
                PortablePoint::new(i as f32, 0.0),
            )
        })
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let mut canvas = Canvas::with_graph(graph);
    canvas.set_affinity_blend(AffinityBlend::ContentOnly);
    canvas.set_cluster_by_affinity(true);
    let _ = canvas.frame(800, 600);
    let structural = canvas.affinity_pair_count();
    assert!(
        structural >= 2,
        "the structural signal installs several pairs, got {structural}"
    );

    // Inject a one-pair content signal: it supersedes structural (the live count is exactly the
    // injected 1, not the several structural pairs).
    canvas.set_content_affinity(Some(vec![(n[0], n[3], 0.9)]));
    assert!(canvas.has_content_affinity(), "content is now the source");
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        1,
        "content (1 pair) supersedes structural ({structural} pairs)"
    );

    // Clear it: structural returns on the next frame.
    canvas.set_content_affinity(None);
    assert!(
        !canvas.has_content_affinity(),
        "back to the structural source"
    );
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        structural,
        "clearing content reverts to the structural signal"
    );
}

#[test]
fn empty_content_affinity_is_authoritative_but_inert() {
    // Under `ContentOnly`, a host that ran embeddings and found no pairs above its threshold injects
    // `Some(empty)`: that is authoritative — it clears the force rather than falling back to
    // structural (pass `None` for that). (burn brief Lane 5 — P4/P6.)
    let mut graph = Graph::new();
    let n: Vec<NodeKey> = (0..3)
        .map(|i| {
            graph.add_node(
                format!("https://{i}.example"),
                PortablePoint::new(i as f32, 0.0),
            )
        })
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 0)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let mut canvas = Canvas::with_graph(graph);
    canvas.set_affinity_blend(AffinityBlend::ContentOnly);
    canvas.set_cluster_by_affinity(true);
    let _ = canvas.frame(800, 600);
    assert!(
        canvas.affinity_pair_count() > 0,
        "the structural triangle installs pairs"
    );

    canvas.set_content_affinity(Some(Vec::new()));
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        0,
        "empty content clears the force — no structural fallback"
    );
}

#[test]
fn content_affinity_reinstalls_after_a_toggle_cycle() {
    // The dirty flag is consumed at install, so a toggle off→on must still reinstall the persisted
    // content signal (the off branch re-arms it). Two edgeless nodes: structural is empty, so the
    // count `1` can only come from the content signal. (burn brief Lane 5 — P4.)
    let mut graph = Graph::new();
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let b = graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let mut canvas = Canvas::with_graph(graph);
    canvas.set_content_affinity(Some(vec![(a, b, 0.8)]));
    canvas.set_cluster_by_affinity(true);
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        1,
        "content installed on enable"
    );

    canvas.set_cluster_by_affinity(false);
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        0,
        "toggle off clears the force"
    );

    canvas.set_cluster_by_affinity(true);
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        1,
        "toggle back on reinstalls the persisted content signal"
    );
}

#[test]
fn blend_unions_structural_and_content_pairs() {
    // Default mode is Blend. Structural yields the triangle's pairs; a content signal adds one pair
    // between an otherwise-unconnected node and the triangle. The live force is the *union*
    // (structural pairs + the new content pair), not one superseding the other. (burn brief Lane 5
    // — P6, blended affinity.)
    let mut graph = Graph::new();
    let n: Vec<NodeKey> = (0..4)
        .map(|i| {
            graph.add_node(
                format!("https://{i}.example"),
                PortablePoint::new(i as f32, 0.0),
            )
        })
        .collect();
    // A triangle over 0,1,2 (structural pairs); node 3 shares no edge (no structural pair).
    for &(a, b) in &[(0, 1), (1, 2), (2, 0)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let mut canvas = Canvas::with_graph(graph);
    assert_eq!(
        canvas.affinity_blend(),
        AffinityBlend::Blend,
        "Blend is the default"
    );
    canvas.set_cluster_by_affinity(true);

    // Structural-only baseline (no content injected yet): the triangle's pairs.
    let _ = canvas.frame(800, 600);
    let structural = canvas.affinity_pair_count();
    assert!(
        structural >= 3,
        "the triangle's structural pairs, got {structural}"
    );

    // Inject one content pair structural does not have (node 3 → node 0, no shared edge).
    canvas.set_content_affinity(Some(vec![(n[3], n[0], 0.9)]));
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        structural + 1,
        "Blend unions the structural pairs with the new content pair"
    );
}

#[test]
fn blend_affinity_pairs_noisy_ors_shared_weights() {
    // The merge math directly: a pair in only one signal passes through at its weight; a pair in
    // both is noisy-OR'd (0.8 with 0.8 → 0.96 = 1 − 0.2·0.2). (burn brief Lane 5 — P6.)
    let mut graph = Graph::new();
    let a = graph.add_node(
        "https://a.example".to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    let b = graph.add_node(
        "https://b.example".to_string(),
        PortablePoint::new(1.0, 0.0),
    );
    let c = graph.add_node(
        "https://c.example".to_string(),
        PortablePoint::new(2.0, 0.0),
    );
    let structural = signals::AffinityScores {
        pairs: vec![((a, b), 0.8)],
    };
    let content = vec![(a, b, 0.8), (a, c, 0.5)];
    let blended = blend_affinity_pairs(Some(&structural), Some(&content));
    let weight = |x: NodeKey, y: NodeKey| {
        blended
            .iter()
            .find(|&&(p, q, _)| (p == x && q == y) || (p == y && q == x))
            .map(|&(_, _, w)| w)
    };
    let ab = weight(a, b).expect("a-b present");
    assert!(
        (ab - 0.96).abs() < 1e-5,
        "noisy-or 0.8,0.8 -> 0.96, got {ab}"
    );
    let ac = weight(a, c).expect("a-c present");
    assert!(
        (ac - 0.5).abs() < 1e-5,
        "content-only pair passes through, got {ac}"
    );
    assert_eq!(blended.len(), 2, "two distinct unordered pairs");
}

#[test]
fn affinity_force_refreshes_on_a_topology_change() {
    // With clustering on, a structural mutation must refresh the installed signal (the cache is
    // revision-gated, not frozen at first compute). An edgeless start has no affinity pairs; wiring
    // a triangle in creates them. (Graph signals — P4.)
    let mut graph = Graph::new();
    for i in 0..3 {
        graph.add_node(
            format!("https://{i}.example"),
            PortablePoint::new(i as f32, 0.0),
        );
    }
    let mut canvas = Canvas::with_graph(graph);
    canvas.set_cluster_by_affinity(true);
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        0,
        "an edgeless graph has no affinity pairs"
    );

    // Wire the three nodes into a triangle: every pair now shares the third node.
    canvas.ingest_graph(|g| {
        let a = g.get_node_by_url("https://0.example").unwrap().0;
        let b = g.get_node_by_url("https://1.example").unwrap().0;
        let c = g.get_node_by_url("https://2.example").unwrap().0;
        g.assert_semantic_predicate(a, b, "links".to_string());
        g.assert_semantic_predicate(b, c, "links".to_string());
        g.assert_semantic_predicate(c, a, "links".to_string());
        true
    });
    let _ = canvas.frame(800, 600);
    assert_eq!(
        canvas.affinity_pair_count(),
        3,
        "the triangle's three pairs refresh into the live force"
    );
}
