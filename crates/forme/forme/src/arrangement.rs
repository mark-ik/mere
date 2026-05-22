// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `Arrangement` — forme's graph-capable workbench arrangement (v1).
//!
//! This is forme advanced to its own charter: an arrangement is "whatever
//! shape the typesetter locks up — not forced into a tree." It is a small
//! **graph** of arrangement nodes (members, tiles, groups, …) connected by
//! arrangement edges (stacked, split, compare, …). A *tree* is one
//! [projection](../../../../../design_docs/mere_docs/technical_architecture/2026-05-21_mere_composition_spine.md)
//! of it (platen's job), not its essence.
//!
//! **Geometry-free** by design: an arrangement holds *semantic* placement
//! (these two are stacked, that one is pinned) — never coordinates or split
//! ratios. Geometry lives in projection state, keyed `(FormeRef,
//! ProjectionKind)`; camera/focus lives in the pane view-intent.
//!
//! **Discipline (spine §3, §10):** graph *shape*, small vocabulary. The
//! graphlet-reconciliation / pressure / lens machinery from the graphshell era
//! is *parked* — `CollapsedGraphlet` is the only minimal hook kept for the
//! future derived-arrangement lane.
//!
//! forme stays framework-free: members are referenced by their stable graph id
//! ([`GraphMemberId`] = the kernel node UUID); forme holds the id, the host resolves
//! it against the kernel graph.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable id of a graph member (kernel `Node` UUID) an arrangement references.
pub type GraphMemberId = Uuid;

/// Local id of a node *within one arrangement* — distinct from a graph member
/// id (one member may appear as several arrangement nodes: mirrors, compares).
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ArrangementNodeId(Uuid);

impl ArrangementNodeId {
    /// Mint a fresh id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Construct from a known UUID (persistence / tests).
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for ArrangementNodeId {
    fn default() -> Self {
        Self::new()
    }
}

/// What an arrangement node *is*. The small v1 vocabulary; not every node is a
/// rendered tile (hence `forme`/`Arrangement`, not `TileGraph`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrangementNodeKind {
    /// The arrangement's root. Exactly one per arrangement.
    WorkbenchRoot,
    /// "Show this graph member" — the curated reference into graph truth.
    MemberIntent { member: GraphMemberId },
    /// A curated tile, optionally bound to a graph member (a tile may present
    /// non-member content, e.g. a tool).
    TileIntent { member: Option<GraphMemberId> },
    /// A grouping container (its members attach via `MemberOf`).
    Group,
    /// A mirror/portal of another arrangement node (the original lives
    /// elsewhere in the arrangement; the `MirrorOf` edge points to it).
    Portal,
    /// Minimal hook for the future derived-arrangement lane (a graphlet shown
    /// collapsed). Parked: no reconciliation machinery in v1.
    CollapsedGraphlet,
}

/// One arrangement node: a local id, a kind, an optional label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangementNode {
    pub id: ArrangementNodeId,
    pub kind: ArrangementNodeKind,
    pub label: Option<String>,
}

/// Semantic placement relations between arrangement nodes. These are the
/// arrangement *facts*; geometry (which side, what ratio) is a projection
/// concern.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrangementEdgeKind {
    /// Containment: `from` is a member of `to` (a tile in the root or a group).
    MemberOf,
    /// Tabbed together (one visible at a time in a tree projection).
    StackedWith,
    /// Side-by-side (a split in a tree projection).
    SplitWith,
    /// Loosely adjacent (spatial benches; ordering hint).
    AdjacentTo,
    /// Part of a comparison set (compare-fan projection).
    CompareWith,
    /// Ordered focus/navigation path (corridor projection; traversal order a
    /// tree gets for free, a graph must state).
    FocusPath,
    /// Pinned within a container (survives reflow).
    PinnedIn,
    /// `from` is a mirror/portal of `to`.
    MirrorOf,
}

/// A directed arrangement relation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangementEdge {
    pub from: ArrangementNodeId,
    pub to: ArrangementNodeId,
    pub kind: ArrangementEdgeKind,
}

/// A workbench arrangement: a small graph of nodes + relations, rooted at a
/// single [`ArrangementNodeKind::WorkbenchRoot`]. Geometry-free.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arrangement {
    root: ArrangementNodeId,
    nodes: BTreeMap<ArrangementNodeId, ArrangementNode>,
    edges: Vec<ArrangementEdge>,
}

impl Arrangement {
    /// A fresh arrangement with just its `WorkbenchRoot`.
    pub fn new() -> Self {
        let root = ArrangementNodeId::new();
        let mut nodes = BTreeMap::new();
        nodes.insert(
            root,
            ArrangementNode {
                id: root,
                kind: ArrangementNodeKind::WorkbenchRoot,
                label: None,
            },
        );
        Self {
            root,
            nodes,
            edges: Vec::new(),
        }
    }

    /// The root node's id.
    pub fn root(&self) -> ArrangementNodeId {
        self.root
    }

    /// Insert a node of `kind` with an optional label; returns its id. The
    /// node is *not* attached to anything — use [`attach`](Self::attach) or the
    /// `add_*` helpers, which attach to the root by default.
    pub fn insert(&mut self, kind: ArrangementNodeKind, label: Option<String>) -> ArrangementNodeId {
        let id = ArrangementNodeId::new();
        self.nodes.insert(id, ArrangementNode { id, kind, label });
        id
    }

    /// Add a `MemberIntent` for `member`, attached to the root.
    pub fn add_member_intent(&mut self, member: GraphMemberId) -> ArrangementNodeId {
        let id = self.insert(ArrangementNodeKind::MemberIntent { member }, None);
        self.attach(id, self.root);
        id
    }

    /// Add a `TileIntent` (optionally bound to a member), attached to the root.
    pub fn add_tile_intent(&mut self, member: Option<GraphMemberId>) -> ArrangementNodeId {
        let id = self.insert(ArrangementNodeKind::TileIntent { member }, None);
        self.attach(id, self.root);
        id
    }

    /// Add a `Group`, attached to the root.
    pub fn add_group(&mut self, label: Option<String>) -> ArrangementNodeId {
        let id = self.insert(ArrangementNodeKind::Group, label);
        self.attach(id, self.root);
        id
    }

    /// Make `child` a member of `parent` (a `MemberOf` edge). Re-parenting:
    /// drops any existing `MemberOf` out-edge from `child` first, so a node has
    /// exactly one container.
    pub fn attach(&mut self, child: ArrangementNodeId, parent: ArrangementNodeId) {
        self.edges
            .retain(|e| !(e.from == child && e.kind == ArrangementEdgeKind::MemberOf));
        self.relate(child, parent, ArrangementEdgeKind::MemberOf);
    }

    /// Add a relation edge (idempotent for the exact triple).
    pub fn relate(
        &mut self,
        from: ArrangementNodeId,
        to: ArrangementNodeId,
        kind: ArrangementEdgeKind,
    ) {
        let edge = ArrangementEdge { from, to, kind };
        if !self.edges.contains(&edge) {
            self.edges.push(edge);
        }
    }

    /// Tab two nodes together (`StackedWith`).
    pub fn stack(&mut self, a: ArrangementNodeId, b: ArrangementNodeId) {
        self.relate(a, b, ArrangementEdgeKind::StackedWith);
    }

    /// Split two nodes side-by-side (`SplitWith`).
    pub fn split(&mut self, a: ArrangementNodeId, b: ArrangementNodeId) {
        self.relate(a, b, ArrangementEdgeKind::SplitWith);
    }

    /// Look up a node.
    pub fn node(&self, id: ArrangementNodeId) -> Option<&ArrangementNode> {
        self.nodes.get(&id)
    }

    /// Iterate all nodes (deterministic order).
    pub fn nodes(&self) -> impl Iterator<Item = &ArrangementNode> {
        self.nodes.values()
    }

    /// Iterate all edges.
    pub fn edges(&self) -> impl Iterator<Item = &ArrangementEdge> {
        self.edges.iter()
    }

    /// Number of nodes (including the root).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        // The root always exists, so an arrangement is never truly empty;
        // "empty" here means "only the root".
        self.nodes.len() <= 1
    }

    /// Direct members of `container` (incoming `MemberOf` edges).
    pub fn members_of(&self, container: ArrangementNodeId) -> impl Iterator<Item = ArrangementNodeId> + '_ {
        self.edges
            .iter()
            .filter(move |e| e.to == container && e.kind == ArrangementEdgeKind::MemberOf)
            .map(|e| e.from)
    }

    /// The graph members referenced anywhere in the arrangement
    /// (`MemberIntent` + bound `TileIntent`). Deduped, deterministic.
    pub fn referenced_members(&self) -> Vec<GraphMemberId> {
        let mut out: Vec<GraphMemberId> = self
            .nodes
            .values()
            .filter_map(|n| match &n.kind {
                ArrangementNodeKind::MemberIntent { member } => Some(*member),
                ArrangementNodeKind::TileIntent { member: Some(m) } => Some(*m),
                _ => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

impl Default for Arrangement {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_arrangement_has_only_root() {
        let a = Arrangement::new();
        assert_eq!(a.len(), 1);
        assert!(a.is_empty());
        assert_eq!(
            a.node(a.root()).unwrap().kind,
            ArrangementNodeKind::WorkbenchRoot
        );
    }

    #[test]
    fn member_intents_attach_to_root() {
        let mut a = Arrangement::new();
        let m1 = a.add_member_intent(Uuid::from_u128(1));
        let m2 = a.add_member_intent(Uuid::from_u128(2));
        let members: Vec<_> = a.members_of(a.root()).collect();
        assert!(members.contains(&m1));
        assert!(members.contains(&m2));
        assert_eq!(a.len(), 3); // root + 2
    }

    #[test]
    fn referenced_members_dedupes_and_sorts() {
        let mut a = Arrangement::new();
        a.add_member_intent(Uuid::from_u128(2));
        a.add_tile_intent(Some(Uuid::from_u128(1)));
        a.add_member_intent(Uuid::from_u128(2)); // dup member, distinct node
        a.add_tile_intent(None); // unbound tile contributes nothing
        assert_eq!(
            a.referenced_members(),
            vec![Uuid::from_u128(1), Uuid::from_u128(2)]
        );
    }

    #[test]
    fn stack_and_split_are_relations_not_containment() {
        let mut a = Arrangement::new();
        let t1 = a.add_tile_intent(None);
        let t2 = a.add_tile_intent(None);
        a.stack(t1, t2);
        a.split(t1, t2);
        let kinds: Vec<_> = a
            .edges()
            .filter(|e| e.from == t1 && e.to == t2)
            .map(|e| e.kind)
            .collect();
        assert!(kinds.contains(&ArrangementEdgeKind::StackedWith));
        assert!(kinds.contains(&ArrangementEdgeKind::SplitWith));
    }

    #[test]
    fn attach_reparents_exactly_once() {
        let mut a = Arrangement::new();
        let group = a.add_group(Some("g".into()));
        let tile = a.add_tile_intent(None); // attached to root
        a.attach(tile, group); // reparent into the group
        // Exactly one MemberOf out-edge from `tile`, pointing at the group.
        let parents: Vec<_> = a
            .edges()
            .filter(|e| e.from == tile && e.kind == ArrangementEdgeKind::MemberOf)
            .map(|e| e.to)
            .collect();
        assert_eq!(parents, vec![group]);
    }

    #[test]
    fn relate_is_idempotent() {
        let mut a = Arrangement::new();
        let t1 = a.add_tile_intent(None);
        let t2 = a.add_tile_intent(None);
        a.stack(t1, t2);
        a.stack(t1, t2);
        let count = a
            .edges()
            .filter(|e| e.kind == ArrangementEdgeKind::StackedWith)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn round_trips_through_json() {
        let mut a = Arrangement::new();
        let g = a.add_group(Some("research".into()));
        let t = a.add_tile_intent(Some(Uuid::from_u128(7)));
        a.attach(t, g);
        let json = serde_json::to_string(&a).unwrap();
        let back: Arrangement = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }
}
