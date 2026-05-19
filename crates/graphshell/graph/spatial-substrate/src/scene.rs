// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `SubstrateScene` — flat list of placed scene nodes + relation edges.
//!
//! v0a holds nodes and edges in insertion order; no spatial index. The IR
//! brief's full property set (placement / embeds / typed relations / LOD /
//! identity) is reflected for the bits this layer covers; sub-region
//! refinement / LOD promotion / persistence keys are renderer + cartography
//! concerns above this layer.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

use kurbo::{Point, Rect, Size};
use mere_renderer_registry::{
    LodLevel, NodeContentKind, NodeIdentity, Placement, RendererId, SceneNodeRef,
};
use vello::peniko::Color;

/// Stroke style for substrate-painted relation edges. Picked per
/// `EdgeKind` via `SubstrateHost::edge_style`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct EdgeStyle {
    pub color: Color,
    pub width_px: f64,
}

impl EdgeStyle {
    /// v0a default palette for `EdgeKind::Plain` — neutral gray.
    pub const PLAIN: Self = Self {
        color: Color::from_rgba8(200, 200, 210, 200),
        width_px: 2.0,
    };

    /// v0a default for `EdgeKind::Reference` — cool blue.
    pub const REFERENCE: Self = Self {
        color: Color::from_rgba8(110, 160, 230, 220),
        width_px: 2.0,
    };

    /// v0a default for `EdgeKind::Containment` — green, thicker.
    pub const CONTAINMENT: Self = Self {
        color: Color::from_rgba8(120, 200, 130, 220),
        width_px: 2.5,
    };

    /// v0a default for `EdgeKind::Annotation` — warm amber, thinner.
    pub const ANNOTATION: Self = Self {
        color: Color::from_rgba8(230, 180, 110, 200),
        width_px: 1.5,
    };

    /// The v0a default style for a given kind. Hosts override by
    /// calling `SubstrateHost::set_edge_style(kind, style)`.
    pub fn default_for_kind(kind: EdgeKind) -> Self {
        match kind {
            EdgeKind::Plain => Self::PLAIN,
            EdgeKind::Reference => Self::REFERENCE,
            EdgeKind::Containment => Self::CONTAINMENT,
            EdgeKind::Annotation => Self::ANNOTATION,
        }
    }
}

/// A single placed node in the substrate's spatial scene graph.
///
/// Not `Copy` because [`renderer_pin`](Self::renderer_pin) carries a
/// `RendererId`. `Clone` is cheap when no pin or static-string pin.
#[derive(Clone, Debug)]
pub struct SubstrateNode {
    pub identity: NodeIdentity,
    pub placement: Placement,
    pub size: Size,
    pub lod: LodLevel,
    pub content_kind: NodeContentKind,
    /// Per-node renderer pin. When set, the registry's selector
    /// honors this id over the default first-candidate pick if the
    /// pinned renderer is registered and handles `content_kind`. See
    /// `mere_renderer_registry::DefaultSelector` for the v0 chain.
    pub renderer_pin: Option<RendererId>,
}

impl SubstrateNode {
    /// Construct a node with a freshly-minted identity, `FullPane`
    /// LOD, and no renderer pin.
    pub fn new(content_kind: NodeContentKind, placement: Placement, size: Size) -> Self {
        Self {
            identity: NodeIdentity::next(),
            placement,
            size,
            lod: LodLevel::FullPane,
            content_kind,
            renderer_pin: None,
        }
    }

    /// Builder: attach a renderer pin to this node.
    pub fn with_renderer_pin(mut self, pin: RendererId) -> Self {
        self.renderer_pin = Some(pin);
        self
    }

    /// Borrowed view of this node for registry dispatch.
    pub fn as_ref(&self) -> SceneNodeRef {
        SceneNodeRef {
            identity: self.identity,
            placement: self.placement,
            lod: self.lod,
            size: self.size,
            content_kind: self.content_kind,
            renderer_pin: self.renderer_pin.clone(),
        }
    }
}

/// Stable per-edge identity. Disjoint from `NodeIdentity` namespace so
/// scene-level hit-test can distinguish at the type level.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EdgeIdentity(NonZeroU64);

impl EdgeIdentity {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(NonZeroU64::new(n).expect("edge identity counter never zero"))
    }

    pub fn as_u64(self) -> u64 {
        self.0.get()
    }
}

/// Relation kind tag. v0a names a small starter set — cartography
/// will own the full taxonomy (semantic / spatial / temporal /
/// causal / etc.) once that crate exists. The substrate paints each
/// kind with the host-configured `EdgeStyle`; default styles are
/// chosen for visual distinctness, not semantic correctness.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum EdgeKind {
    /// Generic uncategorized relation.
    Plain,
    /// "Points to" — link-like, directional reference. Default
    /// rendering: cool blue.
    Reference,
    /// "Contains" — hierarchical / parent-child. Default rendering:
    /// thicker green.
    Containment,
    /// "Annotates" — auxiliary note attached to its source. Default
    /// rendering: warm amber, thinner.
    Annotation,
}

/// A directed relation between two scene nodes.
///
/// v0a edges are pure data: identity + endpoints + kind + optional label.
/// Rendering is substrate-emitted (a straight line between endpoint
/// centers); hit-test uses line-segment proximity. The IR brief §10.1
/// leaves "edges as content vs paint" open — this is the paint
/// interpretation; a content interpretation (edges as
/// `NodeContentKind::EdgeRendering` nodes with their own renderer)
/// remains compatible with this data model since edges aren't
/// registered through the renderer registry.
#[derive(Clone, Debug)]
pub struct RelationEdge {
    pub identity: EdgeIdentity,
    pub from: NodeIdentity,
    pub to: NodeIdentity,
    pub kind: EdgeKind,
    /// Optional human-readable label. Rendering of the label string is
    /// deferred — v0a's substrate-side edge painter only draws the line
    /// geometry. When parley wiring lands in a downstream consumer,
    /// labels can be positioned at the edge midpoint.
    pub label: Option<String>,
}

impl RelationEdge {
    pub fn new(from: NodeIdentity, to: NodeIdentity) -> Self {
        Self {
            identity: EdgeIdentity::next(),
            from,
            to,
            kind: EdgeKind::Plain,
            label: None,
        }
    }

    pub fn with_kind(mut self, kind: EdgeKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// The thing a hit-test resolved to.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SceneHit {
    Node(NodeIdentity),
    Edge(EdgeIdentity),
}

/// Flat substrate scene. Nodes paint in insertion order (back-to-front);
/// edges paint on top of nodes in their own insertion order.
#[derive(Default)]
pub struct SubstrateScene {
    nodes: Vec<SubstrateNode>,
    edges: Vec<RelationEdge>,
}

impl SubstrateScene {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a node; returns its identity.
    pub fn insert(&mut self, node: SubstrateNode) -> NodeIdentity {
        let id = node.identity;
        self.nodes.push(node);
        id
    }

    /// Look up a node by identity.
    pub fn get(&self, identity: NodeIdentity) -> Option<&SubstrateNode> {
        self.nodes.iter().find(|n| n.identity == identity)
    }

    /// Iterate nodes in paint order (back-to-front).
    pub fn iter(&self) -> impl Iterator<Item = &SubstrateNode> {
        self.nodes.iter()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Insert a relation edge; returns its identity.
    pub fn insert_edge(&mut self, edge: RelationEdge) -> EdgeIdentity {
        let id = edge.identity;
        self.edges.push(edge);
        id
    }

    /// Look up an edge by identity.
    pub fn get_edge(&self, identity: EdgeIdentity) -> Option<&RelationEdge> {
        self.edges.iter().find(|e| e.identity == identity)
    }

    /// Iterate edges in paint order (insertion order).
    pub fn iter_edges(&self) -> impl Iterator<Item = &RelationEdge> {
        self.edges.iter()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Compute the scene-space endpoint center for an edge endpoint.
    /// Returns `None` if the node isn't in the scene.
    pub(crate) fn endpoint_center(&self, node_id: NodeIdentity) -> Option<Point> {
        let node = self.get(node_id)?;
        let local_center = Point::new(node.size.width / 2.0, node.size.height / 2.0);
        Some(node.placement.transform * local_center)
    }

    /// Topmost node containing `point`, or `None`. Renderer-registry
    /// spatial-input router behavior: walks nodes in reverse-paint
    /// order (front-to-back) and returns the first hit.
    ///
    /// Coarse hit-test only: a node is hit if `point`, mapped back into
    /// the node's local coordinates via the inverse of its placement
    /// transform, lies within `[0, size.width] × [0, size.height]`.
    /// Sub-region refinement (`RendererCapabilities::hit_testable_subregions`)
    /// happens inside the resolved renderer.
    pub fn hit_test_node(&self, point: Point) -> Option<NodeIdentity> {
        for node in self.nodes.iter().rev() {
            if node_contains_point(node, point) {
                return Some(node.identity);
            }
        }
        None
    }

    /// Topmost edge whose line geometry passes within `tolerance` of
    /// `point` (scene-space pixels), or `None`. Walks edges in
    /// reverse-paint order; ignores edges whose endpoints aren't in the
    /// scene.
    pub fn hit_test_edge(&self, point: Point, tolerance: f64) -> Option<EdgeIdentity> {
        for edge in self.edges.iter().rev() {
            let (Some(a), Some(b)) = (
                self.endpoint_center(edge.from),
                self.endpoint_center(edge.to),
            ) else {
                continue;
            };
            if distance_to_segment(point, a, b) <= tolerance {
                return Some(edge.identity);
            }
        }
        None
    }

    /// Unified hit-test: returns whatever was hit at `point` —
    /// `Edge(_)` if the point is within `edge_tolerance` of an edge's
    /// line geometry; otherwise `Node(_)` if the point is within a
    /// node; otherwise `None`. Edges win over nodes because they paint
    /// on top; consumers wanting node-first dispatch should call
    /// [`Self::hit_test_node`] and [`Self::hit_test_edge`] separately.
    pub fn hit_test(&self, point: Point) -> Option<SceneHit> {
        if let Some(edge) = self.hit_test_edge(point, DEFAULT_EDGE_HIT_TOLERANCE) {
            return Some(SceneHit::Edge(edge));
        }
        self.hit_test_node(point).map(SceneHit::Node)
    }

    /// Axis-aligned bounding rectangle of all node bounds (in scene
    /// coordinates), or `None` if the scene has no nodes.
    ///
    /// Each node contributes the AABB of its placement-transformed
    /// tile rect; the result is the union. Edges are not included —
    /// their geometry is always contained within the endpoint nodes'
    /// bounds (lines between endpoint centers). Useful for fitting
    /// the scene to a thumbnail target via [`crate::fit_camera`].
    pub fn content_bounds(&self) -> Option<Rect> {
        let mut iter = self.nodes.iter();
        let first = iter.next()?;
        let mut bounds = node_bounds(first);
        for node in iter {
            let b = node_bounds(node);
            bounds = bounds.union(b);
        }
        Some(bounds)
    }
}

/// Axis-aligned bounding rect of a single node's transformed tile.
fn node_bounds(node: &SubstrateNode) -> Rect {
    let t = node.placement.transform;
    let w = node.size.width;
    let h = node.size.height;
    let corners = [
        t * Point::new(0.0, 0.0),
        t * Point::new(w, 0.0),
        t * Point::new(0.0, h),
        t * Point::new(w, h),
    ];
    let mut x0 = corners[0].x;
    let mut x1 = corners[0].x;
    let mut y0 = corners[0].y;
    let mut y1 = corners[0].y;
    for c in corners.iter().skip(1) {
        if c.x < x0 {
            x0 = c.x;
        }
        if c.x > x1 {
            x1 = c.x;
        }
        if c.y < y0 {
            y0 = c.y;
        }
        if c.y > y1 {
            y1 = c.y;
        }
    }
    Rect::new(x0, y0, x1, y1)
}

/// Default pixel tolerance for edge hit-tests. Roughly 2× the typical
/// stroke width to give comfortable target acquisition on touch and
/// mouse alike.
pub const DEFAULT_EDGE_HIT_TOLERANCE: f64 = 6.0;

/// Distance from `p` to segment `[a, b]`.
fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab_x = b.x - a.x;
    let ab_y = b.y - a.y;
    let len_sq = ab_x * ab_x + ab_y * ab_y;
    if len_sq <= f64::EPSILON {
        // Degenerate segment: distance to the single point.
        let dx = p.x - a.x;
        let dy = p.y - a.y;
        return (dx * dx + dy * dy).sqrt();
    }
    let t = ((p.x - a.x) * ab_x + (p.y - a.y) * ab_y) / len_sq;
    let t_clamped = t.clamp(0.0, 1.0);
    let proj_x = a.x + t_clamped * ab_x;
    let proj_y = a.y + t_clamped * ab_y;
    let dx = p.x - proj_x;
    let dy = p.y - proj_y;
    (dx * dx + dy * dy).sqrt()
}

/// Test whether `point` (in scene coords) falls inside `node`'s
/// transformed bounding rectangle. Degenerate (zero-determinant)
/// transforms never contain any point.
fn node_contains_point(node: &SubstrateNode, point: Point) -> bool {
    let transform = node.placement.transform;
    if transform.determinant() == 0.0 {
        return false;
    }
    let local = transform.inverse() * point;
    local.x >= 0.0 && local.y >= 0.0 && local.x <= node.size.width && local.y <= node.size.height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_node_has_full_pane_lod() {
        let node = SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::IDENTITY,
            Size::new(100.0, 100.0),
        );
        assert_eq!(node.lod, LodLevel::FullPane);
    }

    #[test]
    fn nodes_iter_in_insertion_order() {
        let mut scene = SubstrateScene::new();
        let a = scene.insert(SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::IDENTITY,
            Size::new(10.0, 10.0),
        ));
        let b = scene.insert(SubstrateNode::new(
            NodeContentKind::GraphView,
            Placement::translate(20.0, 30.0),
            Size::new(50.0, 50.0),
        ));
        let collected: Vec<_> = scene.iter().map(|n| n.identity).collect();
        assert_eq!(collected, vec![a, b]);
    }

    #[test]
    fn as_ref_round_trips_fields() {
        let node = SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(7.5, 11.0),
            Size::new(80.0, 40.0),
        );
        let r = node.as_ref();
        assert_eq!(r.identity, node.identity);
        assert_eq!(r.size, node.size);
        assert_eq!(r.content_kind, node.content_kind);
        assert_eq!(r.placement.transform, node.placement.transform);
    }

    #[test]
    fn hit_test_empty_scene_returns_none() {
        let scene = SubstrateScene::new();
        assert_eq!(scene.hit_test_node(Point::new(10.0, 10.0)), None);
    }

    #[test]
    fn hit_test_inside_translated_rect_returns_identity() {
        let mut scene = SubstrateScene::new();
        let id = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(40.0, 60.0),
            Size::new(100.0, 80.0),
        ));
        // Inside the rect (45, 70) is at (5, 10) tile-local.
        assert_eq!(scene.hit_test_node(Point::new(45.0, 70.0)), Some(id));
        // Corner (139.99, 139.99) is inside.
        assert_eq!(scene.hit_test_node(Point::new(139.99, 139.99)), Some(id));
    }

    #[test]
    fn hit_test_outside_translated_rect_returns_none() {
        let mut scene = SubstrateScene::new();
        scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(40.0, 60.0),
            Size::new(100.0, 80.0),
        ));
        // Left of rect.
        assert_eq!(scene.hit_test_node(Point::new(20.0, 70.0)), None);
        // Above rect.
        assert_eq!(scene.hit_test_node(Point::new(50.0, 30.0)), None);
        // Below rect.
        assert_eq!(scene.hit_test_node(Point::new(50.0, 200.0)), None);
        // Right of rect.
        assert_eq!(scene.hit_test_node(Point::new(200.0, 70.0)), None);
    }

    #[test]
    fn hit_test_returns_topmost_when_overlapping() {
        let mut scene = SubstrateScene::new();
        let _under = scene.insert(SubstrateNode::new(
            NodeContentKind::CustomCanvas,
            Placement::IDENTITY,
            Size::new(200.0, 200.0),
        ));
        let over = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(50.0, 50.0),
            Size::new(100.0, 100.0),
        ));
        // (100, 100) is inside both — should return the topmost (last inserted).
        assert_eq!(scene.hit_test_node(Point::new(100.0, 100.0)), Some(over));
    }

    #[test]
    fn insert_edge_returns_unique_identity() {
        let mut scene = SubstrateScene::new();
        let a = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::IDENTITY,
            Size::new(20.0, 20.0),
        ));
        let b = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(100.0, 0.0),
            Size::new(20.0, 20.0),
        ));
        let e1 = scene.insert_edge(RelationEdge::new(a, b));
        let e2 = scene.insert_edge(RelationEdge::new(b, a));
        assert_ne!(e1, e2);
        assert_eq!(scene.edge_count(), 2);
    }

    #[test]
    fn hit_test_edge_finds_line_between_nodes() {
        let mut scene = SubstrateScene::new();
        let a = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(0.0, 0.0),
            Size::new(20.0, 20.0),
        ));
        let b = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(100.0, 0.0),
            Size::new(20.0, 20.0),
        ));
        // Endpoint centers: (10, 10) and (110, 10). Line lies along y=10.
        let e = scene.insert_edge(RelationEdge::new(a, b));
        // Point on the line, well between nodes.
        assert_eq!(scene.hit_test_edge(Point::new(60.0, 10.0), 1.0), Some(e));
        // Point just off the line (within tolerance).
        assert_eq!(scene.hit_test_edge(Point::new(60.0, 14.0), 5.0), Some(e));
        // Point far from the line.
        assert_eq!(scene.hit_test_edge(Point::new(60.0, 50.0), 5.0), None);
    }

    #[test]
    fn hit_test_edge_skips_when_endpoint_missing() {
        let mut scene = SubstrateScene::new();
        let a = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::IDENTITY,
            Size::new(20.0, 20.0),
        ));
        // Endpoint `b` is never inserted into the scene.
        let phantom = NodeIdentity::next();
        scene.insert_edge(RelationEdge::new(a, phantom));
        assert_eq!(scene.hit_test_edge(Point::new(50.0, 10.0), 100.0), None);
    }

    #[test]
    fn unified_hit_test_prefers_edge_over_node_when_within_tolerance() {
        let mut scene = SubstrateScene::new();
        let a = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(0.0, 0.0),
            Size::new(200.0, 200.0),
        ));
        let b = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(300.0, 0.0),
            Size::new(200.0, 200.0),
        ));
        let e = scene.insert_edge(RelationEdge::new(a, b));
        // (250, 100) is on the line between endpoint centers
        // ((100, 100) → (400, 100)) and also inside neither node's
        // bounds. Should hit the edge.
        let hit = scene.hit_test(Point::new(250.0, 100.0));
        assert_eq!(hit, Some(SceneHit::Edge(e)));
    }

    #[test]
    fn unified_hit_test_falls_back_to_node_when_no_edge() {
        let mut scene = SubstrateScene::new();
        let id = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(40.0, 40.0),
            Size::new(100.0, 100.0),
        ));
        assert_eq!(
            scene.hit_test(Point::new(50.0, 50.0)),
            Some(SceneHit::Node(id))
        );
    }

    #[test]
    fn edge_style_defaults_per_kind() {
        assert_eq!(
            EdgeStyle::default_for_kind(EdgeKind::Plain),
            EdgeStyle::PLAIN
        );
        assert_eq!(
            EdgeStyle::default_for_kind(EdgeKind::Reference),
            EdgeStyle::REFERENCE
        );
        assert_eq!(
            EdgeStyle::default_for_kind(EdgeKind::Containment),
            EdgeStyle::CONTAINMENT
        );
        assert_eq!(
            EdgeStyle::default_for_kind(EdgeKind::Annotation),
            EdgeStyle::ANNOTATION
        );
        // Each kind has a distinct default.
        assert_ne!(EdgeStyle::PLAIN, EdgeStyle::REFERENCE);
        assert_ne!(EdgeStyle::PLAIN, EdgeStyle::CONTAINMENT);
        assert_ne!(EdgeStyle::PLAIN, EdgeStyle::ANNOTATION);
        assert_ne!(EdgeStyle::REFERENCE, EdgeStyle::CONTAINMENT);
    }

    #[test]
    fn relation_edge_builder_methods() {
        let mut scene = SubstrateScene::new();
        let a = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::IDENTITY,
            Size::new(10.0, 10.0),
        ));
        let b = scene.insert(SubstrateNode::new(
            NodeContentKind::Panel,
            Placement::translate(50.0, 0.0),
            Size::new(10.0, 10.0),
        ));
        let edge = RelationEdge::new(a, b)
            .with_kind(EdgeKind::Plain)
            .with_label("connects");
        let id = scene.insert_edge(edge);
        let stored = scene.get_edge(id).unwrap();
        assert_eq!(stored.kind, EdgeKind::Plain);
        assert_eq!(stored.label.as_deref(), Some("connects"));
    }

    #[test]
    fn hit_test_through_scale_transform() {
        // Node placed at (50, 50) with 2× scale. The 80×40 source rect
        // covers (50..210, 50..130) in scene coords.
        let mut scene = SubstrateScene::new();
        let id = scene.insert(SubstrateNode {
            identity: NodeIdentity::next(),
            placement: Placement::new(
                kurbo::Affine::translate((50.0, 50.0)) * kurbo::Affine::scale(2.0),
            ),
            size: kurbo::Size::new(80.0, 40.0),
            lod: LodLevel::FullPane,
            content_kind: NodeContentKind::Panel,
            renderer_pin: None,
        });
        // (100, 70) → tile-local (25, 10) under inverse → inside.
        assert_eq!(scene.hit_test_node(Point::new(100.0, 70.0)), Some(id));
        // (250, 70) → tile-local (100, 10) → outside (width = 80).
        assert_eq!(scene.hit_test_node(Point::new(250.0, 70.0)), None);
    }
}
