/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Small functions that several host modules share — graph
//! find-or-create, tile labels, and the synthetic error document.

use euclid::default::Point2D;
use inker::EngineDocument;
use mere_kernel::graph::{Graph, NodeKey};

/// World-space radius of a rendered node. Mirrors
/// `platen::CanvasSceneOptions::default()`'s `default_node_radius`
/// so placement uses the same dimensions the renderer paints with.
pub(crate) const NODE_RADIUS_WORLD: f32 = 18.0;

/// Minimum center-to-center distance between two non-overlapping
/// nodes. A small gap (10% of diameter) keeps adjacent nodes
/// visually separable and clickable.
pub(crate) const NODE_PLACEMENT_PADDING: f32 = 4.0;
pub(crate) const NODE_MIN_SPACING: f32 = NODE_RADIUS_WORLD * 2.0 + NODE_PLACEMENT_PADDING;

/// Pure lookup: find an existing graph node whose address claims include
/// the given URL. No side effects. Returns the bare `NodeKey`; use
/// [`Graph::find_node_by_address`] or [`Graph::get_node_by_url`] when the
/// full `(NodeKey, &Node)` pair is needed.
///
/// Per the [node identity + duplicates brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/research/2026-05-18_node_identity_and_duplicates_brief.md):
/// the address is a *property* of the node, not its identity. Multiple
/// nodes may match. This helper returns *some* match (current impl: the
/// most-recently-added). Callers that need to enumerate siblings use
/// [`Graph::get_nodes_by_url`] instead.
pub(crate) fn find_node_by_address(graph: &Graph, address: &str) -> Option<NodeKey> {
    graph.get_node_by_url(address).map(|(key, _)| key)
}

/// Always create a fresh graph node carrying the given address as its
/// `Primary` claim. The kernel assigns a new `Uuid`; an existing node
/// pointing at the same address (if any) is **not** consulted.
///
/// Positioning:
/// - When `position` is `Some`, use it directly.
/// - When `position` is `None`, scatter into a free spot near `anchor`
///   (or near the origin when `anchor` is `None`) so the new node
///   doesn't overlap existing ones.
///
/// When `anchor` is supplied, a hyperlink edge from `anchor` → new
/// node is asserted so the graph reflects the navigation path.
///
/// Phase 2 of the node-identity rollout: replaces the old
/// `ensure_node_for_address_near` for the cases that wanted "always
/// create new" semantics. Sites that wanted "find or create" compose
/// [`find_node_by_address`] with this helper at the call site.
pub(crate) fn create_node_for_address(
    graph: &mut Graph,
    address: &str,
    position: Option<Point2D<f32>>,
    anchor: Option<NodeKey>,
) -> NodeKey {
    let position = position.unwrap_or_else(|| next_free_position(graph, anchor));
    let key = create_node(graph, address, position);
    if let Some(anchor_key) = anchor
        && anchor_key != key
    {
        let _ = graph.assert_relation(
            anchor_key,
            key,
            mere_kernel::graph::EdgeAssertion::Semantic {
                sub_kind: mere_kernel::graph::SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
        );
    }
    key
}

/// "Find or create" — looks up by address; falls back to creating a
/// fresh node positioned near `anchor` (with a hyperlink edge when
/// the anchor is set).
///
/// Convenience over composing [`find_node_by_address`] +
/// [`create_node_for_address`] at the call site for the common
/// default-navigation case. Sites that explicitly want one behaviour
/// or the other should call the underlying helpers directly.
///
/// Returns `(key, was_created)` for callers that need to run
/// "first-touch" logic on creation.
pub(crate) fn find_or_create_node_for_address(
    graph: &mut Graph,
    address: &str,
    anchor: Option<NodeKey>,
) -> (NodeKey, bool) {
    if let Some(key) = find_node_by_address(graph, address) {
        return (key, false);
    }
    let key = create_node_for_address(graph, address, None, anchor);
    (key, true)
}

fn create_node(graph: &mut Graph, address: &str, position: Point2D<f32>) -> NodeKey {
    #[cfg(not(target_arch = "wasm32"))]
    {
        graph.add_node(address.to_string(), position)
    }
    #[cfg(target_arch = "wasm32")]
    {
        graph.add_node_with_id(uuid::Uuid::new_v4(), address.to_string(), position)
    }
}

/// Pick a position near `anchor` (or near the origin when no anchor
/// is supplied) that doesn't overlap any existing node. Sweeps
/// outward in concentric rings, trying eight evenly-spaced angles
/// per ring until a free slot is found.
fn next_free_position(graph: &Graph, anchor: Option<NodeKey>) -> Point2D<f32> {
    let origin = anchor
        .and_then(|key| graph.get_node(key))
        .map(|node| node.projected_position())
        .unwrap_or(Point2D::new(0.0, 0.0));

    // Without an anchor, allow the origin itself if no node is
    // sitting there — it's the most natural placement for the first
    // few nodes in an empty graph.
    if anchor.is_none() && is_free(graph, origin) {
        return origin;
    }

    let ring_step = NODE_MIN_SPACING;
    let max_rings = 16;
    for ring in 1..=max_rings {
        let radius = ring as f32 * ring_step;
        // Eight directions per ring, starting from the right and
        // going counterclockwise. Eight is enough to find a slot
        // unless the graph is very dense, in which case the outer
        // ring sweep takes over.
        for step in 0..8 {
            let angle = step as f32 * std::f32::consts::FRAC_PI_4
                + (ring as f32) * 0.13; // tiny rotation per ring breaks alignment
            let candidate = Point2D::new(
                origin.x + radius * angle.cos(),
                origin.y + radius * angle.sin(),
            );
            if is_free(graph, candidate) {
                return candidate;
            }
        }
    }
    // Pathological case — graph is densely packed for many rings.
    // Drop the node at the outermost candidate and trust the user
    // (or a later force-layout pass) to sort it out.
    Point2D::new(
        origin.x + (max_rings as f32 + 1.0) * ring_step,
        origin.y,
    )
}

/// True when `point` is far enough from every live node that placing
/// a new node there won't overlap.
fn is_free(graph: &Graph, point: Point2D<f32>) -> bool {
    for (_key, node) in graph.nodes() {
        let p = node.projected_position();
        let dx = p.x - point.x;
        let dy = p.y - point.y;
        if dx * dx + dy * dy < NODE_MIN_SPACING * NODE_MIN_SPACING {
            return false;
        }
    }
    true
}

/// Title for a tile entry — prefer the document title, fall back to
/// the node's URL, and finally a generic placeholder.
pub(crate) fn tile_label(
    node: NodeKey,
    graph: &Graph,
    doc: Option<&EngineDocument>,
) -> String {
    if let Some(doc) = doc {
        if let Some(title) = doc.title.as_deref() {
            if !title.trim().is_empty() {
                return title.to_string();
            }
        }
    }
    if let Some(node_ref) = graph.get_node(node) {
        return node_ref.url().to_string();
    }
    format!("tile {node:?}")
}

/// Build a single-block "load failed" document so the workbench can
/// render something useful when a fetch / route / dispatch errored.
pub(crate) fn error_document(address: &str, error: &inker::EngineError) -> EngineDocument {
    use inker::{DocumentBlock, DocumentProvenance, DocumentTrustState, InlineSpan};
    EngineDocument {
        address: address.to_string(),
        title: Some(format!("Could not load {address}")),
        content_type: "text/plain".to_string(),
        lang: None,
        provenance: DocumentProvenance::for_engine("mere-host.error", address),
        trust: DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        blocks: vec![
            DocumentBlock::Heading {
                level: 1,
                spans: vec![InlineSpan::Text(format!("Could not load {address}"))],
            },
            DocumentBlock::Paragraph {
                spans: vec![InlineSpan::Text(error.to_string())],
            },
        ],
    }
}
