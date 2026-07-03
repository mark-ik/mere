/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-session graphlet derivation and persistence for Mere's graph family.
//!
//! This crate owns the pure half of the graphlet seam: the per-session graphlet
//! index, linked-graphlet derivation/reconciliation, and the shape classifier in
//! [`classifier`]. It depends only on forme + kernel + serde.

#![doc(html_root_url = "https://docs.rs/graphlets/0.0.1")]

use std::collections::HashSet;
use std::path::Path;

use forme::{
    GraphMemberId, GraphletBinding, GraphletId, GraphletKind, GraphletMemberDelta, GraphletRef,
    GraphletSpec,
};
use kernel::graph::{EdgeFamily, Graph, RelationSelector};
use serde::{Deserialize, Serialize};

pub mod classifier;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Sidecar filename for a session's graphlet index, beside `graph.json`.
pub const GRAPHLETS_FILE: &str = "graphlets.json";

/// The graphlets belonging to one session's graph: named sub-structures (a tear-out
/// **branch**, later document-groups / relational-browse neighborhoods) over the same
/// kernel nodes the orrery + workbench render. One per session, persisted beside the
/// graph. (Graphlet-wiring Phase 1; plan recommendation B.)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionGraphlets {
    graphlets: Vec<GraphletRef<GraphMemberId>>,
    /// Monotonic id source. `GraphletId` is forme's lightweight `u32` index, unique
    /// within this session's index (not globally).
    next_id: GraphletId,
}

impl SessionGraphlets {
    /// An empty index. A freshly-created session seeds one default `Session`-kind
    /// graphlet via [`with_default_session`](Self::with_default_session).
    pub fn new() -> Self {
        Self::default()
    }

    /// The empty index plus the default whole-session graphlet (the grouping every
    /// session starts with, before any branch). The seed for a new session.
    pub fn with_default_session(mut self) -> Self {
        let id = self.mint_id();
        self.graphlets.push(GraphletRef::new_session(id));
        self
    }

    /// The graphlets in this session, in creation order.
    pub fn graphlets(&self) -> &[GraphletRef<GraphMemberId>] {
        &self.graphlets
    }

    /// Look a graphlet up by id.
    pub fn get(&self, id: GraphletId) -> Option<&GraphletRef<GraphMemberId>> {
        self.graphlets.iter().find(|g| g.id == id)
    }

    /// Preview a Linked graphlet's drift without mutating its roster. This powers
    /// the Roster Graphlet Card's dry diff; [`reconcile`](Self::reconcile) uses
    /// the same derivation and then applies the truth set.
    pub fn preview_reconcile(
        &self,
        graph: &Graph,
        id: GraphletId,
    ) -> Option<GraphletMemberDelta<GraphMemberId>> {
        self.reconcile_delta(graph, id).map(|(_, delta)| delta)
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
    pub fn record_branch(
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
    pub fn add_member(&mut self, id: GraphletId, node: GraphMemberId) -> bool {
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
    pub fn record_linked(&mut self, graph: &Graph, spec: GraphletSpec) -> GraphletId {
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

    /// Freeze a multi-selection as a **Session** graphlet (the 2026-06-13 crystallize default): an
    /// `UnlinkedSession` graphlet whose roster is exactly `members`, tagged with the classifier's
    /// `kind` for display. Unlike a Linked graphlet it does not derive or drift — it is the frozen
    /// selection, so it works for any shape (including the disconnected Loose / Session grab-bag the
    /// kind-derivation cannot). Returns the new id. (Swatch primitive — P3b crystallize.)
    pub fn record_session(
        &mut self,
        kind: GraphletKind,
        members: Vec<GraphMemberId>,
    ) -> GraphletId {
        let id = self.mint_id();
        let mut g = GraphletRef::new_session(id);
        g.kind = Some(kind);
        g.anchors = members;
        self.graphlets.push(g);
        id
    }

    /// Re-derive a **Linked** graphlet `id` from the current graph and reconcile its live
    /// roster to graph truth, returning the delta (added / removed members) or `None` when
    /// the graphlet is not Linked or nothing drifted. v0 **auto-applies** (the live set
    /// tracks truth); the user-choice proposal path (keep-linked / unlink / save-as-branch)
    /// is a later sub-slice. The diff is the harvested `compute_roster_delta` over the
    /// kernel-derived truth and the stored roster — no `GraphTree`. (Graphlet wiring P3.)
    pub fn reconcile(
        &mut self,
        graph: &Graph,
        id: GraphletId,
    ) -> Option<GraphletMemberDelta<GraphMemberId>> {
        let (truth, delta) = self.reconcile_delta(graph, id)?;
        if let Some(gm) = self.graphlets.iter_mut().find(|g| g.id == id) {
            gm.anchors = truth; // auto-apply: the live set tracks graph truth
        }
        Some(delta)
    }

    fn reconcile_delta(
        &self,
        graph: &Graph,
        id: GraphletId,
    ) -> Option<(Vec<GraphMemberId>, GraphletMemberDelta<GraphMemberId>)> {
        let g = self.graphlets.iter().find(|g| g.id == id)?;
        let spec = match &g.binding {
            GraphletBinding::Linked { spec } => spec,
            _ => return None,
        };
        let current = g.anchors.clone();
        let truth = derive_members(graph, spec);
        let truth_set: HashSet<&GraphMemberId> = truth.iter().collect();
        let cur_set: HashSet<&GraphMemberId> = current.iter().collect();
        let delta = GraphletMemberDelta {
            added: truth
                .iter()
                .filter(|m| !cur_set.contains(m))
                .copied()
                .collect(),
            removed: current
                .iter()
                .filter(|m| !truth_set.contains(m))
                .copied()
                .collect(),
            rebased_seeds: Vec::new(),
        };
        (!delta.is_empty()).then_some((truth, delta))
    }

    /// Convert a linked/branched graphlet to an unlinked session grouping without
    /// changing its current member roster.
    pub fn keep_as_session(&mut self, id: GraphletId) -> bool {
        let Some(g) = self.graphlets.iter_mut().find(|g| g.id == id) else {
            return false;
        };
        if matches!(g.binding, GraphletBinding::UnlinkedSession) {
            return false;
        }
        g.binding = GraphletBinding::UnlinkedSession;
        true
    }

    /// Toggle a relation-family selector on a **Linked** graphlet's spec (Graphlet
    /// Card selector/family editing): adds the family projection string if absent,
    /// removes it if present. Only `Linked` carries a spec whose selectors drive
    /// live re-derivation (`reconcile` / `preview_reconcile`); a no-op elsewhere
    /// (`Branched`'s `parent_spec` is lineage, not an active derivation rule).
    /// Returns whether the toggle applied.
    pub fn toggle_family_selector(&mut self, id: GraphletId, family: EdgeFamily) -> bool {
        let Some(g) = self.graphlets.iter_mut().find(|g| g.id == id) else {
            return false;
        };
        let GraphletBinding::Linked { spec } = &mut g.binding else {
            return false;
        };
        let name = edge_family_str(family);
        match spec
            .selectors
            .iter()
            .position(|s| s.eq_ignore_ascii_case(name))
        {
            Some(pos) => {
                spec.selectors.remove(pos);
            }
            None => spec.selectors.push(name.to_string()),
        }
        true
    }

    /// Branch an existing graphlet's current seed/roster into a new local graphlet.
    /// The parent spec is preserved when present; otherwise we derive a default spec
    /// from the first available member.
    pub fn branch_from_graphlet(&mut self, id: GraphletId) -> Option<GraphletId> {
        let parent = self.graphlets.iter().find(|g| g.id == id)?.clone();
        let anchor = parent
            .primary_anchor
            .or_else(|| parent.anchors.first().copied())?;
        let parent_spec = match parent.binding {
            GraphletBinding::Linked { spec } => spec,
            GraphletBinding::Branched { parent_spec, .. } => parent_spec,
            GraphletBinding::UnlinkedSession => default_spec_for(anchor),
        };
        Some(self.record_branch(anchor, parent_spec))
    }

    /// Whether any graphlet is `Linked` (so worth reconciling against graph truth). The
    /// host gates the per-save reconcile on this. (Graphlet wiring Phase 3 slice 2+.)
    pub fn has_linked(&self) -> bool {
        self.graphlets
            .iter()
            .any(|g| matches!(g.binding, GraphletBinding::Linked { .. }))
    }

    /// Reconcile every `Linked` graphlet against the current graph (re-derive + auto-apply
    /// each), returning whether any roster changed (so the caller persists). Drift at the
    /// data level: a Linked graphlet's persisted roster tracks the graph (the window
    /// already tracks it live via re-derive). (Graphlet wiring Phase 3 slice 2+.)
    pub fn reconcile_all(&mut self, graph: &Graph) -> bool {
        let ids: Vec<GraphletId> = self
            .graphlets
            .iter()
            .filter(|g| matches!(g.binding, GraphletBinding::Linked { .. }))
            .map(|g| g.id)
            .collect();
        let mut changed = false;
        for id in ids {
            if self.reconcile(graph, id).is_some() {
                changed = true;
            }
        }
        changed
    }

    /// Persist this index to `graphlets.json` in the session dir. Best-effort, like the
    /// other per-session sidecars (graph / workbench / cartography).
    pub fn save(&self, session_dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(session_dir.join(GRAPHLETS_FILE), json)
    }

    /// Load a session's graphlet index from its sidecar, falling back to a fresh
    /// default-session index on a missing or corrupt file (the same forgiving posture
    /// as sibling persisted graph sidecars.
    pub fn load(session_dir: &Path) -> Self {
        std::fs::read_to_string(session_dir.join(GRAPHLETS_FILE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Self::new().with_default_session())
    }
}

/// Derive a Linked graphlet's member set from the graph per its [`GraphletSpec`]
/// (Phase 3). The kind selects the kernel-graph algorithm; the seed is `primary_anchor`;
/// the spec's `selectors` are the **edge projection** (which relation families the walk
/// follows — empty = all). `GraphTree`-free — the derivation lives on the kernel graph
/// (where the primitives + the relation vocabulary are), forme supplies only the spec type.
/// Corridor / Loop / Frontier / Facet kinds are later sub-slices; they fall back to the
/// seed alone for now.
pub fn derive_members(graph: &Graph, spec: &GraphletSpec) -> Vec<GraphMemberId> {
    let Some(seed) = spec.primary_anchor.as_deref().and_then(|s| s.parse().ok()) else {
        return Vec::new();
    };
    let selectors = selectors_from_spec(spec);
    match spec.kind {
        GraphletKind::Component => graph.component_members(seed, &selectors),
        GraphletKind::Ego { radius } => graph.ego_members(seed, radius, &selectors),
        _ => vec![seed],
    }
}

/// Map a spec's opaque `selectors` strings to kernel [`RelationSelector`]s (the edge
/// projection). A family name (`Semantic` / `Traversal` / `Containment` / `Provenance` /
/// `Imported` / `Arrangement`, case-insensitive) becomes a `Family` selector; unknown
/// strings are skipped. Empty (or all-unknown) → no projection → the derivation follows
/// every family. Sub-kind selectors (e.g. only `Cites`) are a later refinement. (Graphlet
/// derivation — selectors.)
fn selectors_from_spec(spec: &GraphletSpec) -> Vec<RelationSelector> {
    spec.selectors
        .iter()
        .filter_map(|s| edge_family_from_str(s).map(RelationSelector::Family))
        .collect()
}

/// Parse a relation-family name to an [`EdgeFamily`] (the projection-toggle vocabulary).
fn edge_family_from_str(name: &str) -> Option<EdgeFamily> {
    Some(match name.to_ascii_lowercase().as_str() {
        "semantic" => EdgeFamily::Semantic,
        "traversal" => EdgeFamily::Traversal,
        "containment" => EdgeFamily::Containment,
        "arrangement" => EdgeFamily::Arrangement,
        "imported" => EdgeFamily::Imported,
        "provenance" => EdgeFamily::Provenance,
        _ => return None,
    })
}

/// Format a relation family to the opaque selector-string vocabulary
/// [`edge_family_from_str`] parses — its inverse.
fn edge_family_str(family: EdgeFamily) -> &'static str {
    match family {
        EdgeFamily::Semantic => "semantic",
        EdgeFamily::Traversal => "traversal",
        EdgeFamily::Containment => "containment",
        EdgeFamily::Arrangement => "arrangement",
        EdgeFamily::Imported => "imported",
        EdgeFamily::Provenance => "provenance",
    }
}

/// The relation families a Linked graphlet's spec can filter its derivation walk to
/// (Graphlet Card selector/family editing chips).
pub const EDGE_FAMILIES: [EdgeFamily; 6] = [
    EdgeFamily::Semantic,
    EdgeFamily::Traversal,
    EdgeFamily::Containment,
    EdgeFamily::Arrangement,
    EdgeFamily::Imported,
    EdgeFamily::Provenance,
];

/// Whether `family` is present in a graphlet spec's selector-string list (a Graphlet
/// Card selector chip's checked state).
pub fn spec_has_family(spec: &GraphletSpec, family: EdgeFamily) -> bool {
    let name = edge_family_str(family);
    spec.selectors.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// A minimal `GraphletSpec` for a branch whose donor carried no canonical spec: a
/// single-anchor `Session` grouping. Phase 2 enriches this from the donor's actual
/// graphlet once branches can carry one.
pub fn default_spec_for(anchor: GraphMemberId) -> GraphletSpec {
    GraphletSpec {
        kind: GraphletKind::Session,
        anchors: vec![anchor.to_string()],
        primary_anchor: Some(anchor.to_string()),
        selectors: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
