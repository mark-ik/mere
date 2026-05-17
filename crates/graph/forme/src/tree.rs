// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::MemberId;
use crate::Rect;
use crate::graphlet::{GraphletId, GraphletRef};
use crate::layout::{LayoutMode, LayoutResult, OwnedTreeRow, SplitBoundary, TabEntry};
use crate::lens::ProjectionLens;
use crate::member::{LayoutOverride, Lifecycle, MemberEntry, Provenance, SplitDirection};
use crate::nav::{FocusCycleRegion, FocusDirection, NavAction, NavResult, TreeIntent};
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

    // ---------------------------------------------------------------
    // Layout computation
    // ---------------------------------------------------------------

    /// Compute visible tree rows (sidebar in every mode).
    pub fn visible_rows(&self) -> Vec<OwnedTreeRow<N>> {
        self.topology
            .visible_walk(&self.expanded, &self.active_lens)
            .into_iter()
            .map(|row| {
                let mut owned = OwnedTreeRow::from(row);
                // Fill in graphlet_id from membership
                if let Some(entry) = self.members.get(&owned.member) {
                    owned.graphlet_id = entry.graphlet_membership.first().copied();
                }
                owned
            })
            .collect()
    }

    /// Compute full layout result for a given available rect.
    ///
    /// - **TreeStyleTabs / FlatTabs**: active member gets the full rect.
    /// - **SplitPanes**: visible (Active/Warm) members are laid out via taffy
    ///   flexbox. The topology's parent-child structure maps to nested flex
    ///   containers with alternating H/V direction.
    pub fn compute_layout(&self, available: Rect) -> LayoutResult<N> {
        let tree_rows = self.visible_rows();
        let tab_order = self.build_tab_order();

        let (pane_rects, split_boundaries) = match self.layout_mode {
            LayoutMode::TreeStyleTabs | LayoutMode::FlatTabs => {
                (self.layout_single_pane(&available), Vec::new())
            }
            LayoutMode::SplitPanes => self.layout_split_panes(&available),
        };

        LayoutResult {
            pane_rects,
            split_boundaries,
            tab_order,
            tree_rows,
            active: self.active.clone(),
        }
    }

    fn build_tab_order(&self) -> Vec<TabEntry<N>> {
        // Collect visible members in topology insertion order for stable ordering
        let insertion_order = self.topology.insertion_order();
        let mut tabs = Vec::new();
        for id in insertion_order {
            if let Some(entry) = self.members.get(id) {
                if entry.is_visible_in_pane() {
                    tabs.push(TabEntry {
                        member: id.clone(),
                        lifecycle: entry.lifecycle,
                        is_anchor: matches!(entry.provenance, Provenance::Anchor),
                        depth: self.topology.depth_of(id),
                        graphlet_id: entry.graphlet_membership.first().copied(),
                    });
                }
            }
        }
        tabs
    }

    /// Single-pane layout: the active member gets the full available rect.
    fn layout_single_pane(&self, available: &Rect) -> HashMap<N, Rect> {
        let mut rects = HashMap::new();
        if let Some(active) = &self.active {
            if self
                .members
                .get(active)
                .is_some_and(|e| e.is_visible_in_pane())
            {
                rects.insert(active.clone(), *available);
            }
        }
        rects
    }

    /// Split-pane layout: visible members get taffy-computed rects.
    ///
    /// Walks the topology recursively. A member with visible children becomes
    /// a flex container whose first child is the member's own leaf (it retains
    /// its pane rect), followed by recursive subtrees for each visible child.
    /// Direction alternates H→V→H by default; `preferred_split` overrides.
    fn layout_split_panes(&self, available: &Rect) -> (HashMap<N, Rect>, Vec<SplitBoundary<N>>) {
        let visible_roots: Vec<N> = self
            .topology
            .roots()
            .iter()
            .filter(|id| self.is_visible_in_pane(id))
            .cloned()
            .collect();

        if visible_roots.is_empty() {
            return (HashMap::new(), Vec::new());
        }

        // Single visible member across the entire tree → full rect, no taffy.
        if visible_roots.len() == 1 && self.visible_children_of(&visible_roots[0]).is_empty() {
            let mut rects = HashMap::new();
            rects.insert(visible_roots[0].clone(), *available);
            return (rects, Vec::new());
        }

        let mut taffy = taffy::TaffyTree::<()>::new();
        let mut taffy_to_member: HashMap<taffy::NodeId, N> = HashMap::new();
        let mut container_directions: HashMap<taffy::NodeId, SplitDirection> = HashMap::new();

        let root_direction = SplitDirection::Horizontal;

        let root_children: Vec<taffy::NodeId> = visible_roots
            .iter()
            .map(|id| {
                self.build_subtree(
                    id,
                    root_direction,
                    &mut taffy,
                    &mut taffy_to_member,
                    &mut container_directions,
                )
            })
            .collect();

        let root = taffy
            .new_with_children(
                taffy::Style {
                    size: taffy::Size {
                        width: taffy::Dimension::Length(available.w),
                        height: taffy::Dimension::Length(available.h),
                    },
                    flex_direction: Self::taffy_direction(root_direction),
                    ..Default::default()
                },
                &root_children,
            )
            .expect("taffy root");
        container_directions.insert(root, root_direction);

        taffy
            .compute_layout(
                root,
                taffy::Size {
                    width: taffy::AvailableSpace::Definite(available.w),
                    height: taffy::AvailableSpace::Definite(available.h),
                },
            )
            .expect("taffy compute");

        // Walk the taffy tree to extract absolute rects and split boundaries.
        let mut rects = HashMap::new();
        let mut boundaries = Vec::new();
        self.extract_layout_results(
            &taffy,
            root,
            available.x,
            available.y,
            &taffy_to_member,
            &container_directions,
            &mut rects,
            &mut boundaries,
        );
        (rects, boundaries)
    }

    /// Recursively build a taffy subtree for a member.
    ///
    /// If the member has no visible children it becomes a leaf.
    /// Otherwise it becomes a flex container: [self-leaf, child₀, child₁, …].
    fn build_subtree(
        &self,
        member: &N,
        parent_direction: SplitDirection,
        taffy: &mut taffy::TaffyTree<()>,
        taffy_to_member: &mut HashMap<taffy::NodeId, N>,
        container_directions: &mut HashMap<taffy::NodeId, SplitDirection>,
    ) -> taffy::NodeId {
        let visible_children = self.visible_children_of(member);

        if visible_children.is_empty() {
            // Leaf — member gets its own pane rect.
            let leaf = taffy
                .new_leaf(self.leaf_style_for(member))
                .expect("taffy leaf");
            taffy_to_member.insert(leaf, member.clone());
            return leaf;
        }

        // Container: the member itself is the first child (retains a pane rect),
        // followed by recursive subtrees for each visible child.
        let child_direction = self
            .members
            .get(member)
            .and_then(|e| e.layout_override.as_ref())
            .and_then(|lo| lo.preferred_split)
            .unwrap_or_else(|| Self::toggle_direction(parent_direction));

        let self_leaf = taffy
            .new_leaf(self.leaf_style_for(member))
            .expect("taffy self-leaf");
        taffy_to_member.insert(self_leaf, member.clone());

        let mut children = vec![self_leaf];
        for child_id in &visible_children {
            children.push(self.build_subtree(
                child_id,
                child_direction,
                taffy,
                taffy_to_member,
                container_directions,
            ));
        }

        let container = taffy
            .new_with_children(
                taffy::Style {
                    flex_direction: Self::taffy_direction(child_direction),
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    ..Default::default()
                },
                &children,
            )
            .expect("taffy container");
        container_directions.insert(container, child_direction);
        container
    }

    /// Build the taffy leaf style for a member, respecting layout overrides.
    fn leaf_style_for(&self, member: &N) -> taffy::Style {
        let lo = self
            .members
            .get(member)
            .and_then(|e| e.layout_override.as_ref());

        let (flex_basis, flex_grow, flex_shrink) =
            if let Some(ratio) = lo.and_then(|o| o.split_ratio) {
                // Explicit user-set ratio: use flex_basis percentage.
                (taffy::Dimension::Percent(ratio), 0.0, 1.0)
            } else {
                // Default: equal flex distribution.
                (
                    taffy::Dimension::Auto,
                    lo.and_then(|o| o.flex_grow).unwrap_or(1.0),
                    lo.and_then(|o| o.flex_shrink).unwrap_or(1.0),
                )
            };

        taffy::Style {
            flex_basis,
            flex_grow,
            flex_shrink,
            min_size: taffy::Size {
                width: lo
                    .and_then(|o| o.min_width)
                    .map(taffy::Dimension::Length)
                    .unwrap_or(taffy::Dimension::Auto),
                height: lo
                    .and_then(|o| o.min_height)
                    .map(taffy::Dimension::Length)
                    .unwrap_or(taffy::Dimension::Auto),
            },
            ..Default::default()
        }
    }

    /// Walk the taffy tree, collecting leaf rects and split boundaries.
    fn extract_layout_results(
        &self,
        taffy: &taffy::TaffyTree<()>,
        node: taffy::NodeId,
        parent_x: f32,
        parent_y: f32,
        taffy_to_member: &HashMap<taffy::NodeId, N>,
        container_directions: &HashMap<taffy::NodeId, SplitDirection>,
        rects: &mut HashMap<N, Rect>,
        boundaries: &mut Vec<SplitBoundary<N>>,
    ) {
        use crate::layout::SplitBoundary;

        let layout = taffy.layout(node).expect("taffy layout");
        let abs_x = parent_x + layout.location.x;
        let abs_y = parent_y + layout.location.y;

        if let Some(member) = taffy_to_member.get(&node) {
            // This is a leaf — record its rect.
            rects.insert(
                member.clone(),
                Rect {
                    x: abs_x,
                    y: abs_y,
                    w: layout.size.width,
                    h: layout.size.height,
                },
            );
        }

        let children = taffy.children(node).unwrap_or_default();

        // Recurse into children first to populate rects.
        for child in &children {
            self.extract_layout_results(
                taffy,
                *child,
                abs_x,
                abs_y,
                taffy_to_member,
                container_directions,
                rects,
                boundaries,
            );
        }

        // Derive split boundaries between consecutive leaf-bearing children.
        if let Some(&direction) = container_directions.get(&node) {
            let container_extent = match direction {
                SplitDirection::Horizontal => layout.size.width,
                SplitDirection::Vertical => layout.size.height,
            };

            for pair in children.windows(2) {
                let before_node = pair[0];
                let after_node = pair[1];

                // Resolve each child to its "representative" leaf member.
                // For a leaf, that's the member itself. For a container,
                // it's the last leaf in the before subtree or first leaf
                // in the after subtree — but for boundary identity we want
                // the direct child's representative member.
                let Some(before_member) =
                    self.first_leaf_member(taffy, before_node, taffy_to_member)
                else {
                    continue;
                };
                let Some(after_member) = self.first_leaf_member(taffy, after_node, taffy_to_member)
                else {
                    continue;
                };

                let before_rect = rects.get(&before_member);
                let after_rect = rects.get(&after_member);
                let (Some(br), Some(ar)) = (before_rect, after_rect) else {
                    continue;
                };

                let (axis_position, cross_start, cross_end) = match direction {
                    SplitDirection::Horizontal => {
                        // Boundary is a vertical line between before's right edge and after's left edge.
                        let x = (br.x + br.w + ar.x) / 2.0;
                        (x, abs_y, abs_y + layout.size.height)
                    }
                    SplitDirection::Vertical => {
                        // Boundary is a horizontal line between before's bottom and after's top.
                        let y = (br.y + br.h + ar.y) / 2.0;
                        (y, abs_x, abs_x + layout.size.width)
                    }
                };

                boundaries.push(SplitBoundary {
                    before: before_member,
                    after: after_member,
                    direction,
                    axis_position,
                    cross_start,
                    cross_end,
                    container_extent,
                });
            }
        }
    }

    /// Find the first leaf member in a taffy subtree (depth-first).
    fn first_leaf_member(
        &self,
        taffy: &taffy::TaffyTree<()>,
        node: taffy::NodeId,
        taffy_to_member: &HashMap<taffy::NodeId, N>,
    ) -> Option<N> {
        if let Some(member) = taffy_to_member.get(&node) {
            return Some(member.clone());
        }
        for child in taffy.children(node).unwrap_or_default() {
            if let Some(m) = self.first_leaf_member(taffy, child, taffy_to_member) {
                return Some(m);
            }
        }
        None
    }

    fn taffy_direction(dir: SplitDirection) -> taffy::FlexDirection {
        match dir {
            SplitDirection::Horizontal => taffy::FlexDirection::Row,
            SplitDirection::Vertical => taffy::FlexDirection::Column,
        }
    }

    fn toggle_direction(dir: SplitDirection) -> SplitDirection {
        match dir {
            SplitDirection::Horizontal => SplitDirection::Vertical,
            SplitDirection::Vertical => SplitDirection::Horizontal,
        }
    }

    fn is_visible_in_pane(&self, member: &N) -> bool {
        self.members
            .get(member)
            .is_some_and(|e| e.is_visible_in_pane())
    }

    fn visible_children_of(&self, member: &N) -> Vec<N> {
        self.topology
            .children_of(member)
            .iter()
            .filter(|c| self.is_visible_in_pane(c))
            .cloned()
            .collect()
    }

    // ---------------------------------------------------------------
    // Navigation — apply()
    // ---------------------------------------------------------------

    /// Apply a navigation action. Returns intents for the host.
    pub fn apply(&mut self, action: NavAction<N>) -> NavResult<N> {
        match action {
            NavAction::Select(member) => self.apply_select(member),
            NavAction::Activate(member) => self.apply_activate(member),
            NavAction::Dismiss(member) => self.apply_dismiss(member),
            NavAction::ToggleExpand(member) => self.apply_toggle_expand(member),
            NavAction::Reveal(member) => self.apply_reveal(member),
            NavAction::Attach { member, provenance } => self.apply_attach(member, provenance),
            NavAction::Detach { member, recursive } => self.apply_detach(member, recursive),
            NavAction::Reparent { member, new_parent } => self.apply_reparent(member, new_parent),
            NavAction::Reorder { parent, new_order } => self.apply_reorder(parent, new_order),
            NavAction::SetLifecycle(member, lifecycle) => {
                self.apply_set_lifecycle(member, lifecycle)
            }
            NavAction::SetLayoutMode(mode) => self.apply_set_layout_mode(mode),
            NavAction::SetLens(lens) => self.apply_set_lens(lens),
            NavAction::CycleFocus(direction) => self.apply_cycle_focus(direction),
            NavAction::CycleFocusRegion(region) => self.apply_cycle_focus_region(region),
            NavAction::SetLayoutOverride(member, layout_override) => {
                self.apply_set_layout_override(member, layout_override)
            }
        }
    }

    // ---------------------------------------------------------------
    // Action implementations
    // ---------------------------------------------------------------

    fn apply_select(&mut self, member: N) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }
        self.active = Some(member.clone());
        NavResult::session(vec![TreeIntent::SelectionChanged(member)])
    }

    fn apply_activate(&mut self, member: N) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }
        if let Some(entry) = self.members.get_mut(&member) {
            entry.lifecycle = Lifecycle::Active;
        }
        self.active = Some(member.clone());
        NavResult::session(vec![
            TreeIntent::RequestActivation(member.clone()),
            TreeIntent::SelectionChanged(member),
        ])
    }

    fn apply_dismiss(&mut self, member: N) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }
        if let Some(entry) = self.members.get_mut(&member) {
            entry.lifecycle = Lifecycle::Cold;
        }
        // If the dismissed member was active, clear selection
        if self.active.as_ref() == Some(&member) {
            self.active = None;
        }
        NavResult::session(vec![TreeIntent::RequestDismissal(member)])
    }

    fn apply_toggle_expand(&mut self, member: N) -> NavResult<N> {
        if self.expanded.contains(&member) {
            self.expanded.remove(&member);
        } else {
            self.expanded.insert(member);
        }
        NavResult::session(Vec::new())
    }

    fn apply_reveal(&mut self, member: N) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }
        // Expand all ancestors
        let ancestors = self.topology.ancestors(&member);
        for ancestor in ancestors {
            self.expanded.insert(ancestor);
        }
        self.scroll_anchor = Some(member);
        NavResult::session(Vec::new())
    }

    fn apply_attach(&mut self, member: N, provenance: Provenance<N>) -> NavResult<N> {
        if self.contains(&member) {
            return NavResult::empty();
        }

        // Determine placement from provenance.
        // If the requested parent/sibling doesn't exist in the topology,
        // the topology method returns false and we fall back to root placement.
        let placed = match &provenance {
            Provenance::Traversal { source, .. } => {
                self.topology.attach_child(member.clone(), source)
            }
            Provenance::Manual {
                source: Some(source),
                ..
            } => self.topology.attach_sibling(member.clone(), source),
            Provenance::Derived {
                connection: Some(conn),
                ..
            } => self.topology.attach_sibling(member.clone(), conn),
            Provenance::AgentDerived {
                source: Some(source),
                ..
            } => self.topology.attach_sibling(member.clone(), source),
            _ => {
                // Anchor, Restored, Manual without source, Derived without connection
                self.topology.attach_root(member.clone())
            }
        };

        // If provenance-guided placement failed (e.g. source not in topology),
        // fall back to root placement so the member is always reachable.
        if !placed {
            self.topology.attach_root(member.clone());
        }

        let entry = MemberEntry::new(Lifecycle::Cold, provenance);
        self.members.insert(member.clone(), entry);

        NavResult::structural(vec![TreeIntent::MemberAttached(member)])
    }

    fn apply_detach(&mut self, member: N, recursive: bool) -> NavResult<N> {
        if !self.contains(&member) {
            return NavResult::empty();
        }

        let mut intents = Vec::new();

        if recursive {
            let detached = self.topology.detach(&member);
            for node in &detached {
                self.members.remove(node);
                self.expanded.remove(node);
                intents.push(TreeIntent::MemberDetached(node.clone()));
            }
        } else {
            // Non-recursive: remove only this member, reparenting its children
            // to its parent (or promoting them to roots).
            let children: Vec<N> = self.topology.children_of(&member).to_vec();
            let parent = self.topology.parent_of(&member).cloned();

            // Remove only this single node from the topology. We can't use
            // topology.detach() here because it removes the entire subtree,
            // which would orphan grandchildren still in the members map.
            self.topology.detach_single(&member);
            self.members.remove(&member);
            self.expanded.remove(&member);

            // Re-attach children (with their full subtrees intact) to the
            // detached member's parent, or promote them to roots.
            // After detach_single, the children have no parent pointer and
            // aren't in insertion_order as children — we need to re-link them.
            for child in children {
                if let Some(ref p) = parent {
                    // Re-establish parent/child link directly.
                    self.topology.reattach_child(child, p);
                } else {
                    // Promote to root.
                    self.topology.promote_to_root(&child);
                }
            }

            intents.push(TreeIntent::MemberDetached(member.clone()));
        }

        // Clear active if it was detached
        if let Some(ref active) = self.active {
            if !self.contains(active) {
                self.active = None;
            }
        }

        NavResult::structural(intents)
    }

    fn apply_reparent(&mut self, member: N, new_parent: N) -> NavResult<N> {
        if !self.contains(&member) || !self.contains(&new_parent) {
            return NavResult::empty();
        }
        if self.topology.reparent(&member, &new_parent) {
            NavResult::structural(Vec::new())
        } else {
            // Rejected (cycle or self-reparent)
            NavResult::empty()
        }
    }

    fn apply_reorder(&mut self, parent: N, new_order: Vec<N>) -> NavResult<N> {
        if !self.contains(&parent) {
            return NavResult::empty();
        }
        self.topology.reorder_children(&parent, new_order);
        NavResult::structural(Vec::new())
    }

    fn apply_set_lifecycle(&mut self, member: N, lifecycle: Lifecycle) -> NavResult<N> {
        if let Some(entry) = self.members.get_mut(&member) {
            entry.lifecycle = lifecycle;
            NavResult::session(Vec::new())
        } else {
            NavResult::empty()
        }
    }

    fn apply_set_layout_override(
        &mut self,
        member: N,
        layout_override: LayoutOverride,
    ) -> NavResult<N> {
        if let Some(entry) = self.members.get_mut(&member) {
            entry.layout_override = Some(layout_override);
            NavResult::session(Vec::new())
        } else {
            NavResult::empty()
        }
    }

    fn apply_set_layout_mode(&mut self, mode: LayoutMode) -> NavResult<N> {
        self.layout_mode = mode;
        NavResult::session(vec![TreeIntent::LayoutModeChanged(mode)])
    }

    fn apply_set_lens(&mut self, lens: ProjectionLens) -> NavResult<N> {
        self.active_lens = lens.clone();
        NavResult::session(vec![TreeIntent::LensChanged(lens)])
    }

    fn apply_cycle_focus(&mut self, direction: FocusDirection) -> NavResult<N> {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return NavResult::empty();
        }

        let current_idx = self
            .active
            .as_ref()
            .and_then(|a| rows.iter().position(|r| r.member == *a));

        let next_idx = match (current_idx, direction) {
            (Some(idx), FocusDirection::Next) => (idx + 1) % rows.len(),
            (Some(idx), FocusDirection::Previous) => {
                if idx == 0 {
                    rows.len() - 1
                } else {
                    idx - 1
                }
            }
            (None, _) => 0,
        };

        let member = rows[next_idx].member.clone();
        self.active = Some(member.clone());
        NavResult::session(vec![TreeIntent::SelectionChanged(member)])
    }

    fn apply_cycle_focus_region(&mut self, region: FocusCycleRegion) -> NavResult<N> {
        let candidates: Vec<N> = match region {
            FocusCycleRegion::Roots => self.topology.roots().to_vec(),
            FocusCycleRegion::Branches => self
                .members
                .keys()
                .filter(|m| self.topology.has_children(m))
                .cloned()
                .collect(),
            FocusCycleRegion::Leaves => self
                .members
                .keys()
                .filter(|m| !self.topology.has_children(m))
                .cloned()
                .collect(),
        };

        if candidates.is_empty() {
            return NavResult::empty();
        }

        let current_idx = self
            .active
            .as_ref()
            .and_then(|a| candidates.iter().position(|c| c == a));

        let next_idx = match current_idx {
            Some(idx) => (idx + 1) % candidates.len(),
            None => 0,
        };

        let member = candidates[next_idx].clone();
        self.active = Some(member.clone());
        NavResult::session(vec![TreeIntent::SelectionChanged(member)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphlet::{GraphletKind, GraphletRef};

    #[test]
    fn new_tree_is_empty() {
        let tree = GraphTree::<u64>::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);
        assert_eq!(tree.member_count(), 0);
        assert!(tree.active().is_none());
        assert_eq!(tree.layout_mode(), LayoutMode::TreeStyleTabs);
    }

    #[test]
    fn attach_traversal_creates_child() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });

        assert_eq!(tree.member_count(), 2);
        assert_eq!(tree.parent_of(&2), Some(&1));
        assert_eq!(tree.children_of(&1), &[2]);
    }

    #[test]
    fn attach_manual_creates_sibling() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Manual {
                source: Some(2),
                context: None,
            },
        });

        // 3 should be sibling of 2 (child of 1)
        assert_eq!(tree.parent_of(&3), Some(&1));
    }

    #[test]
    fn activate_and_dismiss() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });

        let result = tree.apply(NavAction::Activate(1));
        assert!(result.session_changed);
        assert_eq!(tree.active(), Some(&1));
        assert!(tree.get(&1).unwrap().is_active());

        let result = tree.apply(NavAction::Dismiss(1));
        assert!(result.session_changed);
        assert!(tree.active().is_none());
        assert!(tree.get(&1).unwrap().is_cold());
    }

    #[test]
    fn toggle_expand() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });

        assert!(!tree.is_expanded(&1));
        tree.apply(NavAction::ToggleExpand(1));
        assert!(tree.is_expanded(&1));
        tree.apply(NavAction::ToggleExpand(1));
        assert!(!tree.is_expanded(&1));
    }

    #[test]
    fn reveal_expands_ancestors() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 2,
                edge_kind: None,
            },
        });

        assert!(!tree.is_expanded(&1));
        assert!(!tree.is_expanded(&2));

        tree.apply(NavAction::Reveal(3));

        assert!(tree.is_expanded(&1));
        assert!(tree.is_expanded(&2));
    }

    #[test]
    fn detach_recursive() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 2,
                edge_kind: None,
            },
        });

        let result = tree.apply(NavAction::Detach {
            member: 2,
            recursive: true,
        });
        assert!(result.structure_changed);
        assert_eq!(tree.member_count(), 1);
        assert!(!tree.contains(&2));
        assert!(!tree.contains(&3));
    }

    #[test]
    fn detach_non_recursive_reparents_children() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 2,
                edge_kind: None,
            },
        });

        tree.apply(NavAction::Detach {
            member: 2,
            recursive: false,
        });

        assert_eq!(tree.member_count(), 2);
        assert!(!tree.contains(&2));
        // 3 should have been reparented to 1
        assert!(tree.contains(&3));
    }

    #[test]
    fn set_lens_emits_intent() {
        let mut tree = GraphTree::<u64>::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        let result = tree.apply(NavAction::SetLens(ProjectionLens::Containment));
        assert!(result.session_changed);
        assert_eq!(tree.active_lens(), &ProjectionLens::Containment);
        assert!(
            result
                .intents
                .iter()
                .any(|i| matches!(i, TreeIntent::LensChanged(ProjectionLens::Containment)))
        );
    }

    #[test]
    fn cycle_focus_wraps() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::ToggleExpand(1));
        tree.apply(NavAction::ToggleExpand(2));

        // Select first
        tree.apply(NavAction::Select(1));
        assert_eq!(tree.active(), Some(&1));

        // Cycle next
        tree.apply(NavAction::CycleFocus(FocusDirection::Next));
        assert_eq!(tree.active(), Some(&2));

        // Cycle next wraps to first
        tree.apply(NavAction::CycleFocus(FocusDirection::Next));
        assert_eq!(tree.active(), Some(&1));
    }

    #[test]
    fn graphlet_membership() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });

        let graphlet = GraphletRef::new_session(0).with_kind(GraphletKind::Session);
        tree.add_graphlet(graphlet);

        tree.get_mut(&1).unwrap().graphlet_membership.push(0);
        tree.get_mut(&2).unwrap().graphlet_membership.push(0);

        let members = tree.graphlet_members(0);
        assert_eq!(members.len(), 2);
        assert!(tree.graphlet_of(&1).is_some());
    }

    #[test]
    fn serialization_roundtrip() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));

        let json = serde_json::to_string(&tree).expect("serialize");
        let restored: GraphTree<u64> = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.member_count(), 2);
        assert_eq!(restored.parent_of(&2), Some(&1));
        assert!(restored.get(&1).unwrap().is_active());
    }

    #[test]
    fn reparent_rejects_cycle_via_nav() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 2,
                edge_kind: None,
            },
        });

        // Try to reparent 1 under 3 — would create 1→2→3→1 cycle
        let result = tree.apply(NavAction::Reparent {
            member: 1,
            new_parent: 3,
        });
        assert!(!result.structure_changed);

        // Tree should be unchanged
        assert_eq!(tree.topology().roots(), &[1]);
        assert_eq!(tree.parent_of(&2), Some(&1));
        assert_eq!(tree.parent_of(&3), Some(&2));
        tree.topology().assert_invariants();
    }

    #[test]
    fn duplicate_attach_is_noop() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        let result = tree.apply(NavAction::Attach {
            member: 1,
            provenance: Provenance::Anchor,
        });

        // Should be a no-op
        assert!(!result.structure_changed);
        assert_eq!(tree.member_count(), 1);
        tree.topology().assert_invariants();
    }

    #[test]
    fn actions_on_nonexistent_member_are_noop() {
        let mut tree = GraphTree::<u64>::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        // All of these should be no-ops on an empty tree
        let r = tree.apply(NavAction::Select(99));
        assert!(!r.session_changed);
        let r = tree.apply(NavAction::Activate(99));
        assert!(!r.session_changed);
        let r = tree.apply(NavAction::Dismiss(99));
        assert!(!r.session_changed);
        let r = tree.apply(NavAction::Reveal(99));
        assert!(!r.session_changed);
        let r = tree.apply(NavAction::Detach {
            member: 99,
            recursive: true,
        });
        assert!(!r.structure_changed);
        let r = tree.apply(NavAction::Reparent {
            member: 99,
            new_parent: 100,
        });
        assert!(!r.structure_changed);
    }

    // --- Layout tests ---

    fn make_layout_tree(mode: LayoutMode) -> GraphTree<u64> {
        let mut tree = GraphTree::new(mode, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(3, Lifecycle::Warm));
        tree.apply(NavAction::Activate(1));
        tree.apply(NavAction::ToggleExpand(1));
        tree
    }

    #[test]
    fn tree_style_tabs_single_pane() {
        let tree = make_layout_tree(LayoutMode::TreeStyleTabs);
        let rect = Rect::new(0.0, 0.0, 800.0, 600.0);
        let result = tree.compute_layout(rect);

        // Active member gets the full rect
        assert_eq!(result.pane_rects.len(), 1);
        let pane = result.pane_rects.get(&1).expect("active member rect");
        assert_eq!(pane.w, 800.0);
        assert_eq!(pane.h, 600.0);

        // Tree rows populated (sidebar)
        assert!(!result.tree_rows.is_empty());
    }

    #[test]
    fn flat_tabs_single_pane() {
        let tree = make_layout_tree(LayoutMode::FlatTabs);
        let rect = Rect::new(0.0, 0.0, 800.0, 600.0);
        let result = tree.compute_layout(rect);

        // Active member gets the full rect
        assert_eq!(result.pane_rects.len(), 1);
        assert!(result.pane_rects.contains_key(&1));

        // Tab order includes visible members (Active + Warm)
        assert_eq!(result.tab_order.len(), 3);
    }

    #[test]
    fn flat_tabs_no_active_empty_rects() {
        let mut tree = GraphTree::new(LayoutMode::FlatTabs, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        // Member is Cold (default), no active set
        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert!(result.pane_rects.is_empty());
    }

    #[test]
    fn split_panes_divides_space() {
        let tree = make_layout_tree(LayoutMode::SplitPanes);
        let rect = Rect::new(0.0, 0.0, 900.0, 600.0);
        let result = tree.compute_layout(rect);

        // 3 visible members (1=Active, 2=Active, 3=Warm) get rects.
        // Topology: 1 is root with children [2, 3].
        // Layout: root(H) → container(V for member 1's subtree) → [leaf(1), leaf(2), leaf(3)]
        // All 3 share the vertical space; each gets full width.
        assert_eq!(result.pane_rects.len(), 3);

        // All rects are within the available area
        for (_, r) in &result.pane_rects {
            assert!(r.x >= 0.0);
            assert!(r.y >= 0.0);
            assert!(r.x + r.w <= 900.1); // small epsilon for float
            assert!(r.y + r.h <= 600.1);
            assert!(r.w > 0.0);
            assert!(r.h > 0.0);
        }

        // With nested layout, member 1's subtree is a V container.
        // All 3 leaves get full width, split height equally.
        let total_height: f32 = result.pane_rects.values().map(|r| r.h).sum();
        assert!(
            (total_height - 600.0).abs() < 1.0,
            "expected total height ~600, got {}",
            total_height
        );
        for (_, r) in &result.pane_rects {
            assert!(
                (r.w - 900.0).abs() < 1.0,
                "expected full width ~900, got {}",
                r.w
            );
        }
    }

    #[test]
    fn split_panes_non_overlapping() {
        let tree = make_layout_tree(LayoutMode::SplitPanes);
        let result = tree.compute_layout(Rect::new(0.0, 0.0, 900.0, 600.0));

        let rects: Vec<&Rect> = result.pane_rects.values().collect();
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let a = rects[i];
                let b = rects[j];
                // No overlap: one must be fully left, right, above, or below the other
                let no_overlap = a.x + a.w <= b.x + 0.1
                    || b.x + b.w <= a.x + 0.1
                    || a.y + a.h <= b.y + 0.1
                    || b.y + b.h <= a.y + 0.1;
                assert!(no_overlap, "panes overlap: {:?} and {:?}", a, b);
            }
        }
    }

    #[test]
    fn split_panes_respects_min_width() {
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

        // Set min_width on member 1
        tree.get_mut(&1).unwrap().layout_override = Some(crate::member::LayoutOverride {
            min_width: Some(400.0),
            min_height: None,
            flex_grow: Some(1.0),
            flex_shrink: Some(0.0), // don't shrink below min
            preferred_split: None,
            split_ratio: None,
        });

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        let r1 = result.pane_rects.get(&1).expect("member 1 rect");
        assert!(r1.w >= 399.0, "min_width not respected: {}", r1.w);
    }

    #[test]
    fn split_panes_vertical_direction() {
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

        // Set vertical split
        tree.get_mut(&1).unwrap().layout_override = Some(crate::member::LayoutOverride {
            min_width: None,
            min_height: None,
            flex_grow: None,
            flex_shrink: None,
            preferred_split: Some(crate::member::SplitDirection::Vertical),
            split_ratio: None,
        });

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        // Vertical split: both panes should have full width, split height
        let r1 = result.pane_rects.get(&1).expect("member 1 rect");
        let r2 = result.pane_rects.get(&2).expect("member 2 rect");
        assert!(
            (r1.w - 800.0).abs() < 1.0,
            "expected full width, got {}",
            r1.w
        );
        assert!(
            (r2.w - 800.0).abs() < 1.0,
            "expected full width, got {}",
            r2.w
        );
        assert!((r1.h + r2.h - 600.0).abs() < 1.0);
    }

    #[test]
    fn layout_tab_order_stable() {
        let tree = make_layout_tree(LayoutMode::FlatTabs);
        let result1 = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        let result2 = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));

        let ids1: Vec<u64> = result1.tab_order.iter().map(|t| t.member).collect();
        let ids2: Vec<u64> = result2.tab_order.iter().map(|t| t.member).collect();
        assert_eq!(ids1, ids2, "tab order should be stable across calls");
    }

    #[test]
    fn tree_rows_respect_expansion() {
        let mut tree = make_layout_tree(LayoutMode::TreeStyleTabs);
        // Initially root is expanded, children visible
        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(result.tree_rows.len(), 3); // root + 2 children

        // Collapse root
        tree.apply(NavAction::ToggleExpand(1));
        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(result.tree_rows.len(), 1); // just root
    }

    // --- Orphan prevention tests (Phase A correctness hardening) ---

    #[test]
    fn traversal_attach_with_missing_source_falls_back_to_root() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        // Attach member 2 with traversal from source 99 which doesn't exist
        tree.apply(NavAction::Attach {
            member: 2u64,
            provenance: Provenance::Traversal {
                source: 99,
                edge_kind: None,
            },
        });

        // Member should still be attached — as a root, not orphaned
        assert!(tree.contains(&2));
        assert!(tree.topology().roots().contains(&2));
        assert!(tree.topology().parent_of(&2).is_none());
        tree.topology().assert_invariants();
    }

    #[test]
    fn manual_attach_with_missing_source_falls_back_to_root() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Manual {
                source: Some(99), // doesn't exist
                context: None,
            },
        });

        assert!(tree.contains(&1));
        assert!(tree.topology().roots().contains(&1));
        tree.topology().assert_invariants();
    }

    #[test]
    fn derived_attach_with_missing_connection_falls_back_to_root() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        tree.apply(NavAction::Attach {
            member: 5u64,
            provenance: Provenance::Derived {
                connection: Some(42), // doesn't exist
                derivation: "test".to_string(),
            },
        });

        assert!(tree.contains(&5));
        assert!(tree.topology().roots().contains(&5));
        tree.topology().assert_invariants();
    }

    #[test]
    fn all_members_reachable_after_mixed_attaches() {
        let mut tree = GraphTree::new(LayoutMode::TreeStyleTabs, ProjectionLens::Traversal);

        // Valid chain
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });

        // Attach with missing source — should become root
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 99,
                edge_kind: None,
            },
        });

        // Every member must appear in visible_rows when all are expanded
        tree.apply(NavAction::ToggleExpand(1));
        tree.apply(NavAction::ToggleExpand(2));
        tree.apply(NavAction::ToggleExpand(3));

        let rows = tree.visible_rows();
        let row_ids: Vec<u64> = rows.iter().map(|r| r.member).collect();
        assert!(row_ids.contains(&1));
        assert!(row_ids.contains(&2));
        assert!(row_ids.contains(&3));

        tree.topology().assert_invariants();
    }

    // --- Nested layout tests (Phase E/G) ---

    #[test]
    fn two_roots_no_children_flat_horizontal_split() {
        // Two independent roots → flat horizontal row.
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(result.pane_rects.len(), 2);

        let r1 = result.pane_rects.get(&1).unwrap();
        let r2 = result.pane_rects.get(&2).unwrap();
        // Horizontal row: each gets ~400 width, full 600 height.
        assert!((r1.w - 400.0).abs() < 1.0, "expected ~400, got {}", r1.w);
        assert!((r2.w - 400.0).abs() < 1.0, "expected ~400, got {}", r2.w);
        assert!((r1.h - 600.0).abs() < 1.0);
        assert!((r2.h - 600.0).abs() < 1.0);
        // Non-overlapping: r1 ends where r2 starts.
        assert!((r1.x + r1.w - r2.x).abs() < 1.0);
    }

    #[test]
    fn root_with_children_nested_split() {
        // Root A with children [B, C] → A gets pane, B and C get nested sub-panes.
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 10u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 20,
            provenance: Provenance::Traversal {
                source: 10,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 30,
            provenance: Provenance::Traversal {
                source: 10,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(10, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(20, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(30, Lifecycle::Active));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 900.0, 600.0));
        assert_eq!(result.pane_rects.len(), 3);

        // Topology: root(H) → subtree_container(V) → [leaf(10), leaf(20), leaf(30)]
        // All three share vertical space, each gets full width.
        for (_, r) in &result.pane_rects {
            assert!(
                (r.w - 900.0).abs() < 1.0,
                "nested children should get full width, got {}",
                r.w
            );
        }
        let total_h: f32 = result.pane_rects.values().map(|r| r.h).sum();
        assert!(
            (total_h - 600.0).abs() < 1.0,
            "total height should be ~600, got {}",
            total_h
        );
    }

    #[test]
    fn three_levels_deep_nesting() {
        // Root → child → grandchild. Direction alternates H→V→H.
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 2,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(3, Lifecycle::Active));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(result.pane_rects.len(), 3);

        let r1 = result.pane_rects.get(&1).unwrap();
        let r2 = result.pane_rects.get(&2).unwrap();
        let r3 = result.pane_rects.get(&3).unwrap();

        // Level 0: root(H) → subtree_1(V direction, since toggle from H)
        //   subtree_1: [leaf(1), subtree_2(H direction, toggle from V)]
        //     subtree_2: [leaf(2), leaf(3)]
        //
        // So 1 and container_2 split V (height). Within container_2, 2 and 3 split H (width).

        // Member 1 gets full width, ~half height.
        assert!((r1.w - 800.0).abs() < 1.0, "r1 full width, got {}", r1.w);
        assert!(
            r1.h > 100.0 && r1.h < 500.0,
            "r1 partial height, got {}",
            r1.h
        );

        // Members 2 and 3 each get ~half width, same height as each other.
        assert!((r2.h - r3.h).abs() < 1.0, "r2 and r3 same height");
        assert!(
            r2.w > 100.0 && r2.w < 700.0,
            "r2 partial width, got {}",
            r2.w
        );
        assert!(
            r3.w > 100.0 && r3.w < 700.0,
            "r3 partial width, got {}",
            r3.w
        );
        assert!(
            (r2.w + r3.w - 800.0).abs() < 1.0,
            "r2+r3 widths should sum to 800, got {}",
            r2.w + r3.w
        );
    }

    #[test]
    fn split_ratio_respected() {
        // Two roots, first has split_ratio 0.7 → gets ~70% of width.
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

        tree.apply(NavAction::SetLayoutOverride(
            1,
            LayoutOverride {
                min_width: None,
                min_height: None,
                flex_grow: None,
                flex_shrink: None,
                preferred_split: None,
                split_ratio: Some(0.7),
            },
        ));
        tree.apply(NavAction::SetLayoutOverride(
            2,
            LayoutOverride {
                min_width: None,
                min_height: None,
                flex_grow: None,
                flex_shrink: None,
                preferred_split: None,
                split_ratio: Some(0.3),
            },
        ));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 1000.0, 600.0));
        let r1 = result.pane_rects.get(&1).unwrap();
        let r2 = result.pane_rects.get(&2).unwrap();

        // r1 should be ~700, r2 ~300.
        assert!((r1.w - 700.0).abs() < 20.0, "expected ~700, got {}", r1.w);
        assert!((r2.w - 300.0).abs() < 20.0, "expected ~300, got {}", r2.w);
    }

    #[test]
    fn direction_alternation() {
        // Root H → children V → grandchildren H.
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Anchor,
        });
        // Make member 1 a parent with child 3
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(3, Lifecycle::Active));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(result.pane_rects.len(), 3);

        let r1 = result.pane_rects.get(&1).unwrap();
        let r2 = result.pane_rects.get(&2).unwrap();
        let r3 = result.pane_rects.get(&3).unwrap();

        // Root container is H → member 1's subtree and member 2 split horizontally.
        // Member 2 (leaf root) gets ~half width, full height.
        assert!((r2.h - 600.0).abs() < 1.0, "r2 full height, got {}", r2.h);
        assert!(
            r2.w > 100.0 && r2.w < 700.0,
            "r2 partial width, got {}",
            r2.w
        );

        // Member 1's subtree is V (alternated from H) → leaf(1) and leaf(3) split vertically.
        assert!(
            (r1.h + r3.h - 600.0).abs() < 1.0,
            "r1+r3 heights should sum to 600, got {}",
            r1.h + r3.h
        );
        // r1 and r3 have the same width (the subtree container's width).
        assert!(
            (r1.w - r3.w).abs() < 1.0,
            "r1 and r3 should have same width, got {} vs {}",
            r1.w,
            r3.w
        );
    }

    #[test]
    fn preferred_split_overrides_alternation() {
        // Root(H) → child normally gets V, but preferred_split=Horizontal overrides.
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

        // Override: member 1's children split H instead of the default V.
        tree.apply(NavAction::SetLayoutOverride(
            1,
            LayoutOverride {
                min_width: None,
                min_height: None,
                flex_grow: None,
                flex_shrink: None,
                preferred_split: Some(SplitDirection::Horizontal),
                split_ratio: None,
            },
        ));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        let r1 = result.pane_rects.get(&1).unwrap();
        let r2 = result.pane_rects.get(&2).unwrap();

        // With H override, both leaf(1) and leaf(2) split horizontally (row).
        // Both should have full height, each ~half width.
        assert!((r1.h - 600.0).abs() < 1.0, "r1 full height, got {}", r1.h);
        assert!((r2.h - 600.0).abs() < 1.0, "r2 full height, got {}", r2.h);
        assert!(
            (r1.w + r2.w - 800.0).abs() < 1.0,
            "widths should sum to 800, got {}",
            r1.w + r2.w
        );
    }

    #[test]
    fn split_boundaries_between_roots() {
        // Two roots → one horizontal split boundary between them.
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(result.split_boundaries.len(), 1);

        let boundary = &result.split_boundaries[0];
        assert_eq!(boundary.direction, SplitDirection::Horizontal);
        // Boundary should be at ~400 (midpoint between the two panes).
        assert!(
            (boundary.axis_position - 400.0).abs() < 2.0,
            "expected boundary at ~400, got {}",
            boundary.axis_position
        );
        // Cross-axis spans full height.
        assert!((boundary.cross_start - 0.0).abs() < 1.0);
        assert!((boundary.cross_end - 600.0).abs() < 1.0);
        assert!((boundary.container_extent - 800.0).abs() < 1.0);
    }

    #[test]
    fn split_boundaries_nested() {
        // Root with two children → boundaries at both levels.
        // root(H) → subtree(V) → [leaf(1), leaf(2), leaf(3)]
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::Attach {
            member: 2,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::Attach {
            member: 3,
            provenance: Provenance::Traversal {
                source: 1,
                edge_kind: None,
            },
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(2, Lifecycle::Active));
        tree.apply(NavAction::SetLifecycle(3, Lifecycle::Active));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 900.0, 600.0));
        // Subtree container(V) has 3 children [leaf(1), leaf(2), leaf(3)].
        // That gives 2 boundaries between consecutive pairs.
        assert_eq!(
            result.split_boundaries.len(),
            2,
            "expected 2 boundaries, got {}: {:?}",
            result.split_boundaries.len(),
            result.split_boundaries
        );

        for boundary in &result.split_boundaries {
            assert_eq!(
                boundary.direction,
                SplitDirection::Vertical,
                "nested container should split vertically"
            );
        }
    }

    #[test]
    fn single_pane_no_boundaries() {
        let mut tree = GraphTree::new(LayoutMode::SplitPanes, ProjectionLens::Traversal);
        tree.apply(NavAction::Attach {
            member: 1u64,
            provenance: Provenance::Anchor,
        });
        tree.apply(NavAction::SetLifecycle(1, Lifecycle::Active));

        let result = tree.compute_layout(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert!(result.split_boundaries.is_empty());
    }
}
