// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::MemberId;
use crate::graphlet::{GraphletId, GraphletRef};
use crate::layout::LayoutMode;
use crate::lens::ProjectionLens;
use crate::member::MemberEntry;
use crate::topology::TreeTopology;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The core data structure. One per graph view.
///
/// Contains all members — active, warm, and cold — organized by graph
/// topology with multiple projection lenses. Framework-agnostic: no
/// egui, no iced, no winit, no wgpu.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct GraphTree<N: MemberId> {
    // --- Membership ---
    members: HashMap<N, MemberEntry<N>>,

    // --- Topology (graph-derived parent/child) ---
    topology: TreeTopology<N>,

    // --- Graphlet index (connected sub-structures) ---
    graphlets: Vec<GraphletRef<N>>,

    // --- Active projection lens ---
    active_lens: ProjectionLens,

    // --- Session state (not graph truth) ---
    active: Option<N>,
    expanded: HashSet<N>,
    scroll_anchor: Option<N>,

    // --- Layout ---
    layout_mode: LayoutMode,
}

impl<N: MemberId> GraphTree<N> {
    // ---------------------------------------------------------------
    // Construction
    // ---------------------------------------------------------------

    pub fn new(layout: LayoutMode, lens: ProjectionLens) -> Self {
        Self {
            members: HashMap::new(),
            topology: TreeTopology::new(),
            graphlets: Vec::new(),
            active_lens: lens,
            active: None,
            expanded: HashSet::new(),
            scroll_anchor: None,
            layout_mode: layout,
        }
    }

    pub fn from_members(
        members: Vec<(N, MemberEntry<N>)>,
        topology: TreeTopology<N>,
        graphlets: Vec<GraphletRef<N>>,
        layout: LayoutMode,
        lens: ProjectionLens,
    ) -> Self {
        Self {
            members: members.into_iter().collect(),
            topology,
            graphlets,
            active_lens: lens,
            active: None,
            expanded: HashSet::new(),
            scroll_anchor: None,
            layout_mode: layout,
        }
    }

    // ---------------------------------------------------------------
    // Membership queries
    // ---------------------------------------------------------------

    pub fn contains(&self, member: &N) -> bool {
        self.members.contains_key(member)
    }

    pub fn get(&self, member: &N) -> Option<&MemberEntry<N>> {
        self.members.get(member)
    }

    pub fn get_mut(&mut self, member: &N) -> Option<&mut MemberEntry<N>> {
        self.members.get_mut(member)
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    pub fn active_count(&self) -> usize {
        self.members.values().filter(|e| e.is_active()).count()
    }

    pub fn warm_count(&self) -> usize {
        self.members.values().filter(|e| e.is_warm()).count()
    }

    pub fn cold_count(&self) -> usize {
        self.members.values().filter(|e| e.is_cold()).count()
    }

    pub fn members(&self) -> impl Iterator<Item = (&N, &MemberEntry<N>)> {
        self.members.iter()
    }

    // ---------------------------------------------------------------
    // Topology delegation
    // ---------------------------------------------------------------

    pub fn topology(&self) -> &TreeTopology<N> {
        &self.topology
    }

    pub fn topology_mut(&mut self) -> &mut TreeTopology<N> {
        &mut self.topology
    }

    pub fn parent_of(&self, member: &N) -> Option<&N> {
        self.topology.parent_of(member)
    }

    pub fn children_of(&self, member: &N) -> &[N] {
        self.topology.children_of(member)
    }

    pub fn depth_of(&self, member: &N) -> usize {
        self.topology.depth_of(member)
    }

    // ---------------------------------------------------------------
    // Graphlets
    // ---------------------------------------------------------------

    pub fn graphlets(&self) -> &[GraphletRef<N>] {
        &self.graphlets
    }

    pub fn graphlets_mut(&mut self) -> &mut Vec<GraphletRef<N>> {
        &mut self.graphlets
    }

    pub fn add_graphlet(&mut self, graphlet: GraphletRef<N>) {
        self.graphlets.push(graphlet);
    }

    pub fn graphlet_of(&self, member: &N) -> Option<&GraphletRef<N>> {
        let entry = self.members.get(member)?;
        let gid = entry.graphlet_membership.first()?;
        self.graphlets.iter().find(|g| g.id == *gid)
    }

    pub fn graphlet_members(&self, id: GraphletId) -> Vec<&N> {
        self.members
            .iter()
            .filter(|(_, entry)| entry.graphlet_membership.contains(&id))
            .map(|(n, _)| n)
            .collect()
    }

    // ---------------------------------------------------------------
    // Lens & layout state
    // ---------------------------------------------------------------

    pub fn active_lens(&self) -> &ProjectionLens {
        &self.active_lens
    }

    pub fn layout_mode(&self) -> LayoutMode {
        self.layout_mode
    }

    pub fn active(&self) -> Option<&N> {
        self.active.as_ref()
    }

    pub fn is_expanded(&self, member: &N) -> bool {
        self.expanded.contains(member)
    }

    /// Iterate over all members currently in the expanded set.
    pub fn expanded_members(&self) -> impl Iterator<Item = &N> {
        self.expanded.iter()
    }

    pub fn scroll_anchor(&self) -> Option<&N> {
        self.scroll_anchor.as_ref()
    }
}

mod actions;
mod layout;
#[cfg(test)]
mod tests;
