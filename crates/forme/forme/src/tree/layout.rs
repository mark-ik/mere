// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use crate::MemberId;
use crate::Rect;
use crate::layout::{LayoutMode, LayoutResult, OwnedTreeRow, SplitBoundary, TabEntry};
use crate::member::{Provenance, SplitDirection};

use super::GraphTree;

impl<N: MemberId> GraphTree<N> {
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
                        width: taffy::Dimension::length(available.w),
                        height: taffy::Dimension::length(available.h),
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
                (taffy::Dimension::percent(ratio), 0.0, 1.0)
            } else {
                // Default: equal flex distribution.
                (
                    taffy::Dimension::auto(),
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
                    .map(taffy::Dimension::length)
                    .unwrap_or(taffy::Dimension::auto()),
                height: lo
                    .and_then(|o| o.min_height)
                    .map(taffy::Dimension::length)
                    .unwrap_or(taffy::Dimension::auto()),
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
}
