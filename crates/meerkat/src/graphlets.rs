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

use std::path::Path;

use forme::{GraphMemberId, GraphletBinding, GraphletId, GraphletKind, GraphletRef, GraphletSpec};
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
}
