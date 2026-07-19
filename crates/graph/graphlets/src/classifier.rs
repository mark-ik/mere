// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shape classifier (swatch-primitive plan P3): given a selection's induced subgraph (node
//! count + undirected edge pairs), rank which canonical graphlet shapes it best fits. Pure
//! topology — no graph handle — so it is unit-tested in isolation. This is the 2026-06-13 doc's
//! named "one real gap"; it feeds the connections swatch's shape chip. Structural detectors only
//! for now (Ego / Corridor / Loop / Component / Loose); Bridge / Facet / Frontier /
//! WorkbenchCorrespondence are contextual and land as later sub-slices.

use std::collections::HashSet;

use forme::GraphletKind;
use kernel::graph::Graph;

/// One ranked shape candidate: the kind, a fit score in `[0, 1]`, and a short chip label.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeRank {
    pub kind: GraphletKind,
    pub fit: f32,
    pub label: String,
}

/// Classify a selection's induced subgraph: `n` nodes (ids `0..n`) and `edges` (undirected pairs,
/// `a != b`; duplicates and out-of-range ignored). Returns the candidate shapes ranked by fit,
/// dominant first. Always non-empty — the floor is Loose / Session. (Swatch primitive — P3.)
pub fn classify(n: usize, edges: &[(usize, usize)]) -> Vec<ShapeRank> {
    let mut deg = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut seen = HashSet::new();
    let mut m = 0usize;
    for &(a, b) in edges {
        if a == b || a >= n || b >= n {
            continue;
        }
        let key = if a < b { (a, b) } else { (b, a) };
        if !seen.insert(key) {
            continue;
        }
        deg[a] += 1;
        deg[b] += 1;
        adj[a].push(b);
        adj[b].push(a);
        m += 1;
    }
    let connected = is_connected(n, &adj);
    let max_deg = deg.iter().copied().max().unwrap_or(0);
    let deg1 = deg.iter().filter(|&&d| d == 1).count();

    let mut ranks: Vec<ShapeRank> = Vec::new();

    // Loop (cycle): connected, every node degree 2, one edge per node.
    if connected && n >= 3 && m == n && deg.iter().all(|&d| d == 2) {
        ranks.push(rank(GraphletKind::Loop, 1.0, format!("Loop · {n}")));
    }
    // Ego (star): a hub adjacent to every other node, all leaves degree 1, tree-sparse.
    if n >= 3 && m == n - 1 && max_deg == n - 1 && deg1 == n - 1 {
        ranks.push(rank(
            GraphletKind::Ego { radius: 1 },
            1.0,
            format!("Ego · hub + {}", n - 1),
        ));
    } else if n >= 4 && connected && m <= n && max_deg >= n - 2 {
        ranks.push(rank(
            GraphletKind::Ego { radius: 1 },
            0.6,
            "Ego · near-hub".into(),
        ));
    }
    // Corridor (simple path): connected, tree, max degree 2, exactly two endpoints.
    if connected && n >= 2 && m == n - 1 && max_deg <= 2 && deg1 == 2 {
        ranks.push(rank(GraphletKind::Corridor, 1.0, format!("Corridor · {n}")));
    } else if connected && n >= 3 && max_deg <= 2 && m <= n {
        ranks.push(rank(
            GraphletKind::Corridor,
            0.5,
            "Corridor · path-ish".into(),
        ));
    }
    // Component: the connected-baseline descriptor (a weak fit whenever everything links up).
    if connected && n >= 2 {
        ranks.push(rank(
            GraphletKind::Component,
            0.55,
            format!("Component · {n}"),
        ));
    }
    // Loose / Session: a disconnected grab-bag (the floor, e.g. a field-scoped set).
    if !connected && n >= 2 {
        ranks.push(rank(GraphletKind::Session, 0.7, format!("Loose · {n}")));
    }
    if ranks.is_empty() {
        ranks.push(rank(GraphletKind::Session, 0.6, format!("Loose · {n}")));
    }
    ranks.sort_by(|a, b| {
        b.fit
            .partial_cmp(&a.fit)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranks
}

/// Classify a selection against a kernel graph: index the resolvable `members`, gather the
/// inter-edges among them (the induced subgraph), and rank the shapes. The graph-aware convenience
/// over [`classify`] for callers holding a `Graph` + a selection (the connections swatch, and
/// crystallize). (Swatch primitive — P3.)
pub fn classify_selection(graph: &Graph, members: &[uuid::Uuid]) -> Vec<ShapeRank> {
    let mut index = std::collections::HashMap::new();
    for &m in members {
        if let Some((key, _)) = graph.get_node_by_id(m) {
            let next = index.len();
            index.entry(key).or_insert(next);
        }
    }
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for r in graph.relations() {
        if let (Some(&a), Some(&b)) = (index.get(&r.from), index.get(&r.to)) {
            if a != b {
                edges.push((a, b));
            }
        }
    }
    classify(index.len(), &edges)
}

fn rank(kind: GraphletKind, fit: f32, label: String) -> ShapeRank {
    ShapeRank { kind, fit, label }
}

/// A stable string tag for a shape kind, carried on the swatch chip strip's DOM
/// (`data-shape-kind`) so a chip click can name the shape to crystallize as. The inverse is
/// [`shape_kind_from_tag`]. (Swatch primitive — P3, chip-click crystallize.)
pub fn shape_kind_tag(kind: &GraphletKind) -> String {
    match kind {
        GraphletKind::Ego { radius } => format!("ego:{radius}"),
        GraphletKind::Corridor => "corridor".into(),
        GraphletKind::Component => "component".into(),
        GraphletKind::Loop => "loop".into(),
        GraphletKind::Frontier => "frontier".into(),
        GraphletKind::Facet => "facet".into(),
        GraphletKind::Session => "session".into(),
        GraphletKind::Bridge => "bridge".into(),
        GraphletKind::WorkbenchCorrespondence => "workbench-correspondence".into(),
    }
}

/// Parse a [`shape_kind_tag`] back to its kind; `None` for an unknown tag (a stale DOM
/// attribute never panics the input path).
pub fn shape_kind_from_tag(tag: &str) -> Option<GraphletKind> {
    Some(match tag {
        "corridor" => GraphletKind::Corridor,
        "component" => GraphletKind::Component,
        "loop" => GraphletKind::Loop,
        "frontier" => GraphletKind::Frontier,
        "facet" => GraphletKind::Facet,
        "session" => GraphletKind::Session,
        "bridge" => GraphletKind::Bridge,
        "workbench-correspondence" => GraphletKind::WorkbenchCorrespondence,
        _ => {
            let radius = tag.strip_prefix("ego:")?.parse().ok()?;
            GraphletKind::Ego { radius }
        }
    })
}

/// Whether the `n`-node graph (by adjacency) is a single connected component.
fn is_connected(n: usize, adj: &[Vec<usize>]) -> bool {
    if n <= 1 {
        return true;
    }
    let mut seen = vec![false; n];
    let mut stack = vec![0usize];
    seen[0] = true;
    let mut count = 1;
    while let Some(u) = stack.pop() {
        for &v in &adj[u] {
            if !seen[v] {
                seen[v] = true;
                count += 1;
                stack.push(v);
            }
        }
    }
    count == n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(n: usize, edges: &[(usize, usize)]) -> GraphletKind {
        classify(n, edges).first().unwrap().kind.clone()
    }

    #[test]
    fn path_is_a_corridor() {
        // 0-1-2-3
        assert_eq!(top(4, &[(0, 1), (1, 2), (2, 3)]), GraphletKind::Corridor);
    }

    #[test]
    fn star_is_an_ego() {
        // hub 0 -> 1, 2, 3
        assert_eq!(
            top(4, &[(0, 1), (0, 2), (0, 3)]),
            GraphletKind::Ego { radius: 1 }
        );
    }

    #[test]
    fn triangle_is_a_loop() {
        assert_eq!(top(3, &[(0, 1), (1, 2), (2, 0)]), GraphletKind::Loop);
    }

    #[test]
    fn disconnected_is_loose_session() {
        // two separate edges 0-1 and 2-3 — a grab-bag, not one shape.
        assert_eq!(top(4, &[(0, 1), (2, 3)]), GraphletKind::Session);
    }

    #[test]
    fn clique_is_a_component_not_an_ego() {
        // K4: dense, every node degree 3 — not a star, not a path; the Component baseline wins.
        assert_eq!(
            top(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]),
            GraphletKind::Component
        );
    }

    #[test]
    fn two_connected_nodes_are_a_trivial_corridor() {
        assert_eq!(top(2, &[(0, 1)]), GraphletKind::Corridor);
    }

    #[test]
    fn shape_kind_tags_round_trip_every_kind() {
        let kinds = [
            GraphletKind::Ego { radius: 1 },
            GraphletKind::Ego { radius: 2 },
            GraphletKind::Corridor,
            GraphletKind::Component,
            GraphletKind::Loop,
            GraphletKind::Frontier,
            GraphletKind::Facet,
            GraphletKind::Session,
            GraphletKind::Bridge,
            GraphletKind::WorkbenchCorrespondence,
        ];
        for kind in kinds {
            assert_eq!(
                shape_kind_from_tag(&shape_kind_tag(&kind)),
                Some(kind.clone()),
                "tag round-trip failed for {kind:?}",
            );
        }
        assert_eq!(shape_kind_from_tag("nonsense"), None);
        assert_eq!(shape_kind_from_tag("ego:x"), None);
    }
}
