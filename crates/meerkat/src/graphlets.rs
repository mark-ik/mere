/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-session graphlet index — the seam for wiring forme's graphlet layer into the
//! live shell.
//!
//! **STUB / scaffold.** Landed at a checkpoint, not yet wired into the session
//! lifecycle. The wiring (construct on session load, persist beside `graph.json`, the
//! window carries a `GraphletId`, the branch op writes here) is Phase 1 of the
//! [graphlet-wiring plan](../../../design_docs/mere_docs/implementation_strategy/2026-06-25_graphlet_wiring_plan.md).
//!
//! Design (plan recommendation **B**): forme's `GraphletRef` type is first-class and
//! tested, but its `GraphTree` container (members + topology + lens + layout) is
//! superseded by the live arrangement — the kernel graph is truth, the orrery is its
//! cartography projection, platen's `Workbench` its tree projection. So we reuse only
//! `GraphletRef`, held here as a per-session index keyed by `GraphMemberId` (the kernel
//! node uuid the orrery + workbench already use), rather than resurrecting `GraphTree`.

// Some accessors (`get`, future kinds) land ahead of their Phase 2 consumers.
#![allow(dead_code)]

use std::collections::HashSet;
use std::path::Path;

use forme::{
    GraphMemberId, GraphletBinding, GraphletId, GraphletKind, GraphletMemberDelta, GraphletRef,
    GraphletSpec,
};
use kernel::graph::Graph;
use serde::{Deserialize, Serialize};

/// Sidecar filename for a session's graphlet index, beside `graph.json`.
pub(crate) const GRAPHLETS_FILE: &str = "graphlets.json";

/// The graphlets belonging to one session's graph: named sub-structures (a tear-out
/// **branch**, later document-groups / relational-browse neighborhoods) over the same
/// kernel nodes the orrery + workbench render. One per session, persisted beside the
/// graph. (Graphlet-wiring Phase 1; plan recommendation B.)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct SessionGraphlets {
    graphlets: Vec<GraphletRef<GraphMemberId>>,
    /// Monotonic id source. `GraphletId` is forme's lightweight `u32` index, unique
    /// within this session's index (not globally).
    next_id: GraphletId,
}

impl SessionGraphlets {
    /// An empty index. A freshly-created session seeds one default `Session`-kind
    /// graphlet via [`with_default_session`](Self::with_default_session).
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The empty index plus the default whole-session graphlet (the grouping every
    /// session starts with, before any branch). The seed for a new session.
    pub(crate) fn with_default_session(mut self) -> Self {
        let id = self.mint_id();
        self.graphlets.push(GraphletRef::new_session(id));
        self
    }

    /// The graphlets in this session, in creation order.
    pub(crate) fn graphlets(&self) -> &[GraphletRef<GraphMemberId>] {
        &self.graphlets
    }

    /// Look a graphlet up by id.
    pub(crate) fn get(&self, id: GraphletId) -> Option<&GraphletRef<GraphMemberId>> {
        self.graphlets.iter().find(|g| g.id == id)
    }

    fn mint_id(&mut self) -> GraphletId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Record a tear-out **branch** (G3): a new graphlet anchored on the torn node,
    /// bound `Branched` back to the donor's spec. Returns the new `GraphletId` the torn
    /// window carries. The branch and the donor share kernel nodes and diverge in this
    /// graphlet's lineage (brief §4.2).
    ///
    /// `parent_spec` is the donor graphlet's spec if it had one, else a default derived
    /// from the anchor (see [`default_spec_for`]).
    pub(crate) fn record_branch(
        &mut self,
        anchor: GraphMemberId,
        parent_spec: GraphletSpec,
    ) -> GraphletId {
        let id = self.mint_id();
        let mut graphlet = GraphletRef::new_session(id).with_anchor(anchor);
        graphlet.binding = GraphletBinding::Branched {
            parent_spec,
            reason: "tearout-branch".to_string(),
        };
        self.graphlets.push(graphlet);
        id
    }

    /// Add `node` to graphlet `id`'s roster, returning whether it was newly added (not
    /// already present). We reuse forme's `anchors` list as the live member set, since
    /// the superseded `GraphTree` member map is not in play (Phase 3 may distinguish
    /// seed anchors from a derived roster). A tear-out **branch** grows this as its
    /// window navigates, so it diverges from the donor. (Graphlet wiring Phase 2.)
    pub(crate) fn add_member(&mut self, id: GraphletId, node: GraphMemberId) -> bool {
        if let Some(g) = self.graphlets.iter_mut().find(|g| g.id == id) {
            if !g.anchors.contains(&node) {
                g.anchors.push(node);
                return true;
            }
        }
        false
    }

    /// Record a **Linked** graphlet (Phase 3): one whose roster is *derived* from the
    /// graph by its [`GraphletSpec`] (kind + seed), not hand-built like a branch. The
    /// initial roster is derived now; [`reconcile`](Self::reconcile) re-derives it when the
    /// graph drifts. `anchors` holds the live derived set; `spec.primary_anchor` holds the
    /// seed (the anchors-vs-members split). Returns the new id. (Graphlet wiring Phase 3.)
    pub(crate) fn record_linked(&mut self, graph: &Graph, spec: GraphletSpec) -> GraphletId {
        let id = self.mint_id();
        let members = derive_members(graph, &spec);
        let seed = spec.primary_anchor.as_deref().and_then(|s| s.parse().ok());
        let mut g = GraphletRef::new_session(id);
        g.kind = Some(spec.kind.clone());
        g.anchors = members;
        g.primary_anchor = seed;
        g.binding = GraphletBinding::Linked { spec };
        self.graphlets.push(g);
        id
    }

    /// Re-derive a **Linked** graphlet `id` from the current graph and reconcile its live
    /// roster to graph truth, returning the delta (added / removed members) or `None` when
    /// the graphlet is not Linked or nothing drifted. v0 **auto-applies** (the live set
    /// tracks truth); the user-choice proposal path (keep-linked / unlink / save-as-branch)
    /// is a later sub-slice. The diff is the harvested `compute_roster_delta` over the
    /// kernel-derived truth and the stored roster — no `GraphTree`. (Graphlet wiring P3.)
    pub(crate) fn reconcile(
        &mut self,
        graph: &Graph,
        id: GraphletId,
    ) -> Option<GraphletMemberDelta<GraphMemberId>> {
        let g = self.graphlets.iter().find(|g| g.id == id)?;
        let spec = match &g.binding {
            GraphletBinding::Linked { spec } => spec.clone(),
            _ => return None, // only Linked graphlets reconcile against graph truth
        };
        let current = g.anchors.clone();
        let truth = derive_members(graph, &spec);
        let truth_set: HashSet<&GraphMemberId> = truth.iter().collect();
        let cur_set: HashSet<&GraphMemberId> = current.iter().collect();
        let added: Vec<GraphMemberId> =
            truth.iter().filter(|m| !cur_set.contains(m)).copied().collect();
        let removed: Vec<GraphMemberId> =
            current.iter().filter(|m| !truth_set.contains(m)).copied().collect();
        let delta = GraphletMemberDelta {
            added,
            removed,
            rebased_seeds: Vec::new(),
        };
        if delta.is_empty() {
            return None;
        }
        if let Some(gm) = self.graphlets.iter_mut().find(|g| g.id == id) {
            gm.anchors = truth; // auto-apply: the live set tracks graph truth
        }
        Some(delta)
    }

    /// Persist this index to `graphlets.json` in the session dir. Best-effort, like the
    /// other per-session sidecars (graph / workbench / cartography).
    pub(crate) fn save(&self, session_dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(session_dir.join(GRAPHLETS_FILE), json)
    }

    /// Load a session's graphlet index from its sidecar, falling back to a fresh
    /// default-session index on a missing or corrupt file (the same forgiving posture
    /// as [`load_workbench`](crate::session_ops::load_workbench)).
    pub(crate) fn load(session_dir: &Path) -> Self {
        std::fs::read_to_string(session_dir.join(GRAPHLETS_FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Self::new().with_default_session())
    }
}

/// Derive a Linked graphlet's member set from the graph per its [`GraphletSpec`]
/// (Phase 3). The kind selects the kernel-graph algorithm; the seed is `primary_anchor`.
/// `GraphTree`-free — the derivation lives on the kernel graph (where the primitives + the
/// relation vocabulary are), forme supplies only the spec type. Selector / Corridor /
/// Loop kinds are later sub-slices; they fall back to the seed alone for now.
pub(crate) fn derive_members(graph: &Graph, spec: &GraphletSpec) -> Vec<GraphMemberId> {
    let Some(seed) = spec.primary_anchor.as_deref().and_then(|s| s.parse().ok()) else {
        return Vec::new();
    };
    match spec.kind {
        GraphletKind::Component => graph.component_members(seed),
        GraphletKind::Ego { radius } => graph.ego_members(seed, radius),
        _ => vec![seed],
    }
}

/// A minimal `GraphletSpec` for a branch whose donor carried no canonical spec: a
/// single-anchor `Session` grouping. Phase 2 enriches this from the donor's actual
/// graphlet once branches can carry one.
pub(crate) fn default_spec_for(anchor: GraphMemberId) -> GraphletSpec {
    GraphletSpec {
        kind: GraphletKind::Session,
        anchors: vec![anchor.to_string()],
        primary_anchor: Some(anchor.to_string()),
        selectors: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_session_seeds_one_graphlet() {
        let g = SessionGraphlets::new().with_default_session();
        assert_eq!(g.graphlets().len(), 1);
        assert!(matches!(
            g.graphlets()[0].binding,
            GraphletBinding::UnlinkedSession
        ));
    }

    #[test]
    fn record_branch_mints_a_branched_graphlet_anchored_on_the_node() {
        let anchor = uuid::Uuid::from_u128(0x42);
        let mut g = SessionGraphlets::new().with_default_session();
        let id = g.record_branch(anchor, default_spec_for(anchor));

        let branch = g.get(id).expect("the branch graphlet exists");
        assert_eq!(branch.primary_anchor, Some(anchor));
        match &branch.binding {
            GraphletBinding::Branched { reason, .. } => assert_eq!(reason, "tearout-branch"),
            other => panic!("expected a Branched binding, got {other:?}"),
        }
        // It is distinct from the default session graphlet.
        assert_eq!(g.graphlets().len(), 2);
    }

    #[test]
    fn add_member_grows_the_roster_and_dedups() {
        let anchor = uuid::Uuid::from_u128(0x42);
        let mut g = SessionGraphlets::new().with_default_session();
        let id = g.record_branch(anchor, default_spec_for(anchor));
        assert!(g.get(id).unwrap().anchors.contains(&anchor), "seeded with its anchor");

        let visited = uuid::Uuid::from_u128(0x99);
        assert!(g.add_member(id, visited), "a newly-navigated node is added");
        assert!(!g.add_member(id, visited), "a revisit is a dedup'd no-op");
        assert!(!g.add_member(id, anchor), "the anchor is already present");
        assert_eq!(g.get(id).unwrap().anchors.len(), 2, "anchor + the one visited node");
    }

    #[test]
    fn linked_component_graphlet_derives_and_reconciles_on_drift() {
        use euclid::default::Point2D;
        use kernel::graph::{EdgeAssertion, SemanticSubKind};
        let mut graph = Graph::new();
        let a = graph.add_node("https://a".to_string(), Point2D::new(0.0, 0.0));
        let b = graph.add_node("https://b".to_string(), Point2D::new(1.0, 0.0));
        let c = graph.add_node("https://c".to_string(), Point2D::new(2.0, 0.0));
        let link = |g: &mut Graph, x, y| {
            g.assert_relation(
                x,
                y,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Hyperlink,
                    label: None,
                    decay_progress: None,
                },
            );
        };
        link(&mut graph, a, b); // A–B connected; C isolated
        let a_id = graph.get_node(a).unwrap().id;
        let c_id = graph.get_node(c).unwrap().id;

        // A Linked Component graphlet seeded on A derives {A, B}.
        let mut idx = SessionGraphlets::new();
        let spec = GraphletSpec {
            kind: GraphletKind::Component,
            anchors: vec![a_id.to_string()],
            primary_anchor: Some(a_id.to_string()),
            selectors: Vec::new(),
        };
        let id = idx.record_linked(&graph, spec);
        assert_eq!(idx.get(id).unwrap().anchors.len(), 2, "component of A is A plus B");
        assert!(idx.reconcile(&graph, id).is_none(), "no drift yet, reconcile is a no-op");

        // Connect C into the component; reconcile re-derives + auto-applies.
        link(&mut graph, b, c);
        let delta = idx.reconcile(&graph, id).expect("the component grew");
        assert_eq!(delta.added, vec![c_id], "C joined A's component");
        assert!(delta.removed.is_empty());
        assert_eq!(idx.get(id).unwrap().anchors.len(), 3, "live roster is now A, B, C");
    }

    #[test]
    fn linked_ego_graphlet_is_radius_bounded() {
        use euclid::default::Point2D;
        use kernel::graph::{EdgeAssertion, SemanticSubKind};
        let mut graph = Graph::new();
        let a = graph.add_node("https://a".to_string(), Point2D::new(0.0, 0.0));
        let b = graph.add_node("https://b".to_string(), Point2D::new(1.0, 0.0));
        let c = graph.add_node("https://c".to_string(), Point2D::new(2.0, 0.0));
        let link = |g: &mut Graph, x, y| {
            g.assert_relation(
                x,
                y,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Hyperlink,
                    label: None,
                    decay_progress: None,
                },
            );
        };
        link(&mut graph, a, b);
        link(&mut graph, b, c); // A–B–C chain
        let a_id = graph.get_node(a).unwrap().id;

        let mut idx = SessionGraphlets::new();
        let spec = GraphletSpec {
            kind: GraphletKind::Ego { radius: 1 },
            anchors: vec![a_id.to_string()],
            primary_anchor: Some(a_id.to_string()),
            selectors: Vec::new(),
        };
        let id = idx.record_linked(&graph, spec);
        assert_eq!(
            idx.get(id).unwrap().anchors.len(),
            2,
            "ego radius 1 from A is A plus B; C is two hops away"
        );
    }
}
