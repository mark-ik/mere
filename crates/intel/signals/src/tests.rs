/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph-signal tests.

use super::*;
use kernel::graph::fixtures::GraphFixtures;
use kernel::geometry::PortablePoint;
use kernel::graph::Graph;

/// The cluster id that `set` assigns `key`, for partition-structure assertions.
fn community_of(set: &ClusterSet, key: NodeKey) -> Option<&str> {
    set.clusters
        .iter()
        .find(|c| c.members.contains(&key))
        .map(|c| c.id.as_str())
}

#[test]
fn degree_importance_normalizes_to_the_most_connected_node() {
    // A hub linked to two leaves: hub degree 2, each leaf degree 1.
    let mut graph = Graph::new();
    let hub = graph.add_node("https://hub.example".to_string(), PortablePoint::new(0.0, 0.0));
    let a = graph.add_node("https://a.example".to_string(), PortablePoint::new(1.0, 0.0));
    let b = graph.add_node("https://b.example".to_string(), PortablePoint::new(2.0, 0.0));
    graph.assert_semantic_predicate(hub, a, "links".to_string());
    graph.assert_semantic_predicate(hub, b, "links".to_string());

    let importance = degree_importance(&graph);
    assert_eq!(importance.lookup(hub), Some(1.0), "the most-connected node is 1.0");
    assert_eq!(importance.lookup(a), Some(0.5), "a degree-1 leaf is half the hub");
    assert_eq!(importance.lookup(b), Some(0.5));
}

#[test]
fn no_edges_yields_all_zero_importance() {
    let mut graph = Graph::new();
    let only = graph.add_node("https://one.example".to_string(), PortablePoint::new(0.0, 0.0));
    let importance = degree_importance(&graph);
    assert_eq!(importance.lookup(only), Some(0.0), "no edges => zero importance, not NaN");
}

#[test]
fn produce_cheap_signals_fills_only_importance() {
    let mut graph = Graph::new();
    graph.add_node("https://one.example".to_string(), PortablePoint::new(0.0, 0.0));
    let signals = produce_cheap_signals(&graph);
    assert!(signals.importance.is_some(), "importance is produced");
    assert!(signals.clusters.is_none(), "the un-produced signals stay None");
    assert!(signals.affinity.is_none());
    assert!(signals.bridges.is_none());
    assert!(signals.embeddings.is_none());
}

#[test]
fn betweenness_marks_the_broker_on_a_path() {
    // a-b-c: b is on the only a–c shortest path => betweenness max; the endpoints are 0.
    let mut graph = Graph::new();
    let a = graph.add_node("https://a.example".to_string(), PortablePoint::new(0.0, 0.0));
    let b = graph.add_node("https://b.example".to_string(), PortablePoint::new(1.0, 0.0));
    let c = graph.add_node("https://c.example".to_string(), PortablePoint::new(2.0, 0.0));
    graph.assert_semantic_predicate(a, b, "links".to_string());
    graph.assert_semantic_predicate(b, c, "links".to_string());
    let imp = betweenness_importance(&graph);
    assert_eq!(imp.lookup(b), Some(1.0), "the broker b is the normalized max");
    assert_eq!(imp.lookup(a), Some(0.0), "an endpoint has zero betweenness");
    assert_eq!(imp.lookup(c), Some(0.0));
}

#[test]
fn betweenness_rewards_the_bridge_node() {
    // A bowtie: triangles {0,1,2} and {2,3,4} share node 2. Every cross-triangle shortest path
    // routes through 2, so it scores the betweenness max — well above a peripheral node.
    let mut graph = Graph::new();
    let n: Vec<_> = (0..5)
        .map(|i| {
            graph.add_node(format!("https://{i}.example"), PortablePoint::new(i as f32, 0.0))
        })
        .collect();
    for &(a, b) in &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)] {
        graph.assert_semantic_predicate(n[a], n[b], "links".to_string());
    }
    let imp = betweenness_importance(&graph);
    let bridge = imp.lookup(n[2]).unwrap();
    let peripheral = imp.lookup(n[0]).unwrap();
    assert_eq!(bridge, 1.0, "the bridge node is the normalized max");
    assert!(bridge > peripheral, "the bridge outscores a peripheral node: {bridge} vs {peripheral}");
}

/// Build a graph of `n` nodes (urls `https://{i}.example`) wired by undirected edge pairs.
fn graph_from_edges(n: usize, edges: &[(usize, usize)]) -> (Graph, Vec<NodeKey>) {
    let mut graph = Graph::new();
    let keys: Vec<NodeKey> = (0..n)
        .map(|i| graph.add_node(format!("https://{i}.example"), PortablePoint::new(i as f32, 0.0)))
        .collect();
    for &(a, b) in edges {
        graph.assert_semantic_predicate(keys[a], keys[b], "links".to_string());
    }
    (graph, keys)
}

#[test]
fn community_splits_two_triangles_joined_by_a_bridge() {
    // Triangles {0,1,2} and {3,4,5} joined by the single edge 2-3: modularity keeps them
    // two communities (each triangle's 3 internal edges outweigh the lone bridge).
    let (graph, k) = graph_from_edges(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)]);
    let set = community_louvain(&graph);
    assert_eq!(set.clusters.len(), 2, "two triangles => two communities");
    assert_eq!(community_of(&set, k[0]), community_of(&set, k[1]), "triangle A coheres");
    assert_eq!(community_of(&set, k[0]), community_of(&set, k[2]));
    assert_eq!(community_of(&set, k[3]), community_of(&set, k[5]), "triangle B coheres");
    assert_ne!(community_of(&set, k[0]), community_of(&set, k[3]), "the bridge does not merge them");
}

#[test]
fn community_collapses_a_clique_to_one() {
    // A triangle is a single dense community.
    let (graph, k) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
    let set = community_louvain(&graph);
    assert_eq!(set.clusters.len(), 1, "a clique is one community");
    assert_eq!(set.clusters[0].members.len(), 3);
    assert_eq!(community_of(&set, k[0]), community_of(&set, k[2]));
}

#[test]
fn community_separates_disconnected_components() {
    // Two disjoint edges => two communities.
    let (graph, k) = graph_from_edges(4, &[(0, 1), (2, 3)]);
    let set = community_louvain(&graph);
    assert_eq!(set.clusters.len(), 2, "disjoint components are distinct communities");
    assert_eq!(community_of(&set, k[0]), community_of(&set, k[1]));
    assert_ne!(community_of(&set, k[0]), community_of(&set, k[2]));
}

#[test]
fn community_edgeless_graph_is_all_singletons() {
    let (graph, k) = graph_from_edges(3, &[]);
    let set = community_louvain(&graph);
    assert_eq!(set.clusters.len(), 3, "no edges => every node its own community");
    assert_ne!(community_of(&set, k[0]), community_of(&set, k[1]));
}

#[test]
fn community_three_triangles_in_a_line_are_three_communities() {
    // T1{0,1,2}-T2{3,4,5}-T3{6,7,8} joined by single bridges 2-3 and 5-6: the multi-level loop
    // keeps the three dense triangles apart (each bridge is too weak to merge across).
    let (graph, k) = graph_from_edges(
        9,
        &[
            (0, 1), (1, 2), (2, 0), // T1
            (3, 4), (4, 5), (5, 3), // T2
            (6, 7), (7, 8), (8, 6), // T3
            (2, 3), (5, 6), // bridges
        ],
    );
    let set = community_louvain(&graph);
    assert_eq!(set.clusters.len(), 3, "three triangles => three communities");
    assert_eq!(community_of(&set, k[0]), community_of(&set, k[1]), "T1 coheres");
    assert_eq!(community_of(&set, k[0]), community_of(&set, k[2]));
    assert_eq!(community_of(&set, k[3]), community_of(&set, k[5]), "T2 coheres");
    assert_eq!(community_of(&set, k[6]), community_of(&set, k[8]), "T3 coheres");
    assert_ne!(community_of(&set, k[0]), community_of(&set, k[3]), "T1 != T2");
    assert_ne!(community_of(&set, k[3]), community_of(&set, k[6]), "T2 != T3");
    // Every node lands in exactly one cluster.
    let total: usize = set.clusters.iter().map(|c| c.members.len()).sum();
    assert_eq!(total, 9, "the partition covers every node once");
}

#[test]
fn community_detection_is_deterministic() {
    // The same snapshot must yield the identical partition run to run (sorted tie-breaks +
    // first-seen compaction): the community-ring colours depend on it.
    let (graph, _) =
        graph_from_edges(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)]);
    let snapshot = CommunitySnapshot::from_graph(&graph);
    let a = community_louvain_on_snapshot(&snapshot);
    let b = community_louvain_on_snapshot(&snapshot);
    assert_eq!(a, b, "multi-level Louvain is deterministic");
}

#[test]
fn bridge_nodes_picks_the_high_betweenness_broker() {
    // Bowtie: triangles {0,1,2} and {2,3,4} share node 2 (betweenness 1.0); the rest are ~0.
    // At threshold 0.5 only the broker qualifies as a bridge.
    let (graph, k) = graph_from_edges(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]);
    assert_eq!(bridge_nodes(&graph, 0.5).bridges, vec![k[2]], "only the broker is a bridge");
}

#[test]
fn bridge_nodes_empty_for_a_clique() {
    // A triangle: every node's betweenness is 0, so there are no bridges.
    let (graph, _) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
    assert!(bridge_nodes(&graph, 0.5).bridges.is_empty(), "a clique has no bridges");
}

/// The articulation-point set as a sorted Vec, for order-independent comparison.
fn aps(graph: &Graph) -> Vec<NodeKey> {
    let mut v = articulation_points(graph).bridges;
    v.sort();
    v
}

#[test]
fn articulation_points_finds_a_paths_interior() {
    // Path 0-1-2-3: removing an interior node (1 or 2) splits the path; the endpoints do not.
    let (graph, k) = graph_from_edges(4, &[(0, 1), (1, 2), (2, 3)]);
    let mut expected = vec![k[1], k[2]];
    expected.sort();
    assert_eq!(aps(&graph), expected, "the path interior nodes are cut vertices");
}

#[test]
fn articulation_points_empty_for_a_clique() {
    // A triangle is 2-connected: removing any node leaves the other two connected.
    let (graph, _) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
    assert!(articulation_points(&graph).bridges.is_empty(), "a clique has no cut vertex");
}

#[test]
fn articulation_points_empty_for_a_cycle() {
    // A 4-cycle is 2-connected: removing one node leaves a path, still connected.
    let (graph, _) = graph_from_edges(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
    assert!(articulation_points(&graph).bridges.is_empty(), "a cycle has no cut vertex");
}

#[test]
fn articulation_points_finds_the_bowtie_hinge() {
    // Two triangles {0,1,2} and {2,3,4} sharing node 2: only 2 is a cut vertex (its removal
    // splits {0,1} from {3,4}).
    let (graph, k) = graph_from_edges(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]);
    assert_eq!(aps(&graph), vec![k[2]], "the shared hinge is the only cut vertex");
}

#[test]
fn bridges_dispatch_picks_the_metric() {
    // Bowtie: node 2 is both the betweenness broker and the cut vertex (here they agree). The
    // dispatcher routes to each producer by metric.
    let (graph, k) = graph_from_edges(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]);
    assert_eq!(bridges(&graph, BridgeMetric::Betweenness, 0.5).bridges, vec![k[2]]);
    assert_eq!(bridges(&graph, BridgeMetric::Articulation, 0.5).bridges, vec![k[2]]);
    // The codes round-trip for persistence.
    assert_eq!(BridgeMetric::from_code(BridgeMetric::Articulation.as_code()), BridgeMetric::Articulation);
    assert_eq!(BridgeMetric::from_code("nonsense"), BridgeMetric::Betweenness);
}

/// The affinity recorded for an unordered pair, or `None` if it was not emitted.
fn affinity_of(scores: &AffinityScores, a: NodeKey, b: NodeKey) -> Option<f32> {
    scores.lookup(a, b)
}

#[test]
fn affinity_empty_for_an_edgeless_graph() {
    let (graph, _) = graph_from_edges(3, &[]);
    assert!(
        structural_affinity(&graph, 0.0).pairs.is_empty(),
        "no edges => no shared neighbours => no affinity"
    );
}

#[test]
fn triangle_members_all_share_affinity() {
    // Every pair in a triangle shares the third node: J = 1/(2 + 2 − 1) = 1/3 for all three.
    let (graph, k) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
    let scores = structural_affinity(&graph, 0.0);
    assert_eq!(scores.pairs.len(), 3, "all three triangle pairs are similar");
    for &(a, b) in &[(0, 1), (1, 2), (2, 0)] {
        let j = affinity_of(&scores, k[a], k[b]).expect("pair present");
        assert!((j - 1.0 / 3.0).abs() < 1e-5, "triangle Jaccard is 1/3, got {j}");
    }
}

#[test]
fn affinity_clusters_within_triangles_not_across() {
    // Two triangles {0,1,2},{3,4,5} joined by the bridge 2-3. Same-triangle nodes share a
    // neighbour (affinity ~1/3); the far cross pairs share none (absent) — affinity reads as
    // the clusters, the property the affinity force exploits.
    let (graph, k) =
        graph_from_edges(6, &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (2, 3)]);
    let scores = structural_affinity(&graph, 0.0);
    let within_a = affinity_of(&scores, k[0], k[1]).expect("within triangle A");
    let within_b = affinity_of(&scores, k[4], k[5]).expect("within triangle B");
    assert!((within_a - 1.0 / 3.0).abs() < 1e-5, "a clean within-A pair is 1/3");
    assert!((within_b - 1.0 / 3.0).abs() < 1e-5, "a clean within-B pair is 1/3");
    assert_eq!(affinity_of(&scores, k[1], k[4]), None, "a far cross pair shares no neighbour");
    assert_eq!(affinity_of(&scores, k[0], k[5]), None, "another far cross pair is absent");
}

#[test]
fn clique_pairs_share_equal_affinity() {
    // A 4-clique: every pair shares the other two nodes, J = 2/(3 + 3 − 2) = 1/2, uniform.
    let (graph, k) = graph_from_edges(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]);
    let scores = structural_affinity(&graph, 0.0);
    assert_eq!(scores.pairs.len(), 6, "all six clique pairs are similar");
    for &(a, b) in &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
        let j = affinity_of(&scores, k[a], k[b]).expect("pair present");
        assert!((j - 0.5).abs() < 1e-5, "clique Jaccard is 1/2, got {j}");
    }
}

#[test]
fn affinity_threshold_prunes_weak_pairs() {
    // Triangle pairs sit at 1/3 ≈ 0.333: a 0.5 floor drops them all; a 0.1 floor keeps all three.
    let (graph, _) = graph_from_edges(3, &[(0, 1), (1, 2), (2, 0)]);
    assert!(
        structural_affinity(&graph, 0.5).pairs.is_empty(),
        "a 0.5 floor prunes the 1/3 pairs"
    );
    assert_eq!(
        structural_affinity(&graph, 0.1).pairs.len(),
        3,
        "a 0.1 floor keeps the triangle pairs"
    );
}
