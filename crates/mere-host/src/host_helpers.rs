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

/// Find an existing node by URL, or create one placed near `anchor`
/// (when supplied). New nodes are scattered into a free spot so
/// they don't overlap existing ones, and — when `anchor` is set —
/// connected to the anchor with a hyperlink edge so the graph
/// reflects the navigation path.
///
/// Returns `(node, was_created)` so the caller can decide whether to
/// run any "first-touch" logic (e.g. record the visit in history,
/// emit a creation telemetry event).
pub(crate) fn ensure_node_for_address_near(
    graph: &mut Graph,
    address: &str,
    anchor: Option<NodeKey>,
) -> (NodeKey, bool) {
    if let Some((key, _)) = graph.get_node_by_url(address) {
        return (key, false);
    }
    let position = next_free_position(graph, anchor);
    let key = create_node(graph, address, position);
    if let Some(anchor_key) = anchor {
        if anchor_key != key {
            // Per 2026-05-11 relation-taxonomy plan: callers use
            // `assert_relation(EdgeAssertion::Semantic { ... })`
            // instead of `add_edge(EdgeType::Hyperlink)`. Same
            // resulting Semantic(Hyperlink) sub-kind, typed write
            // contract.
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
    }
    (key, true)
}

/// Compatibility shim — same behaviour as `ensure_node_for_address_near`
/// without an anchor, returning only the `NodeKey`.
pub(crate) fn ensure_node_for_address(graph: &mut Graph, address: &str) -> NodeKey {
    ensure_node_for_address_near(graph, address, None).0
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
