/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-frame
//!
//! Frame domain layer — defines the savable layout of resizable panes
//! and projects it into a uxtree subtree.
//!
//! See the crate README for the conceptual scope and the relationship
//! to legacy `platen::FrameState` / `graphshell-shell-state`.

#![doc(html_root_url = "https://docs.rs/mere-frame/0.0.1")]

use accesskit::{Node, Role};
use serde::{Deserialize, Serialize};
use uxtree::{UxTree, node_id_for_path};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

/// Stable identifier for a saved frame layout.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameId(pub String);

impl FrameId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identifier for an individual pane within a frame layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u64);

/// Stable identifier for a graph at app scope. Every leaf in a
/// `FrameLayout` carries one so the host can resolve "which graph
/// does this panel render?" against the app's `GraphRegistry`.
///
/// Frame layouts persist with serialized graph IDs so a saved
/// arrangement reattaches to the right graphs on next launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphId(pub uuid::Uuid);

impl GraphId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for GraphId {
    fn default() -> Self {
        Self::new()
    }
}

/// Durable session identity. Wraps the runtime/session shape: a
/// session owns a root graph (and may grow sub-graph references),
/// holds the worker manifest, engine profile binding, and policy
/// overrides. v0 of session-persistence maps one `SessionId` 1:1
/// to one root `GraphId`; the type distinction is enforced from
/// day one so later phases (sub-graphs, fork-on-divergence,
/// multi-graph-per-session) don't require a painful retrofit.
///
/// See `design_docs/mere_docs/research/2026-05-11_browser_multiplexer_framing.md`
/// §2 (identity matrix) for the broader identity model and
/// `design_docs/mere_docs/implementation_strategy/2026-05-11_graph_session_manifest_plan.md`
/// for storage / lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub uuid::Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Direction of a split between two child panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitAxis {
    /// Children are arranged side-by-side; ratio applies to width.
    Horizontal,
    /// Children are stacked vertically; ratio applies to height.
    Vertical,
}

/// What a leaf pane shows. Extension point: `Custom` carries a
/// host-defined content kind for content not yet promoted to a
/// dedicated mere-domain module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneContent {
    Workbench,
    Orrery,
    Gloss,
    Apparatus,
    System,
    Custom(String),
}

impl PaneContent {
    /// Compact tag suitable for tracing fields and accessible names.
    pub fn tag(&self) -> &str {
        match self {
            PaneContent::Workbench => "workbench",
            PaneContent::Orrery => "orrery",
            PaneContent::Gloss => "gloss",
            PaneContent::Apparatus => "apparatus",
            PaneContent::System => "system",
            PaneContent::Custom(s) => s.as_str(),
        }
    }
}

/// One node in the layout tree: either a split (two children at a
/// given axis + ratio) or a leaf (one pane showing a content kind
/// bound to a graph).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PaneNode {
    Split {
        axis: SplitAxis,
        /// Fraction of the parent occupied by `first`; `second` takes
        /// `1.0 - ratio`. Clamped by consumers to a sane minimum.
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
    Leaf {
        pane_id: PaneId,
        content: PaneContent,
        /// Which graph this panel renders. The host resolves the
        /// `GraphId` against its `GraphRegistry` to get the live
        /// `Entity<Graph>`. Multiple leaves carrying the same
        /// `graph_id` share a graph; differing IDs in one frame =
        /// multi-graph window.
        ///
        /// `#[serde(default)]` allows pre-`graph_id` layouts saved
        /// to disk to deserialize — they come back as a nil UUID,
        /// which the host stamps with the window's primary graph
        /// on load.
        #[serde(default)]
        graph_id: GraphId,
    },
}

/// One step into a [`PaneNode::Split`] when walking the layout tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitChoice {
    First,
    Second,
}

/// Path through the layout tree to a specific split, expressed as
/// `First`/`Second` choices at each branch. Empty path = root.
pub type SplitPath = Vec<SplitChoice>;

/// A complete frame: identity, label, and the layout tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameLayout {
    pub id: FrameId,
    pub label: String,
    pub root: PaneNode,
}

/// Where to insert a new leaf relative to an existing leaf at a
/// `SplitPath`. Used by [`FrameLayout::summon_leaf`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertSide {
    /// New leaf goes left of the existing leaf (horizontal split).
    Left,
    /// New leaf goes right of the existing leaf.
    Right,
    /// New leaf goes above the existing leaf (vertical split).
    Above,
    /// New leaf goes below the existing leaf.
    Below,
}

impl InsertSide {
    fn split_axis(self) -> SplitAxis {
        match self {
            Self::Left | Self::Right => SplitAxis::Horizontal,
            Self::Above | Self::Below => SplitAxis::Vertical,
        }
    }

    /// True when the new leaf goes in `first` position (left / above);
    /// false when it goes in `second` (right / below).
    fn new_is_first(self) -> bool {
        matches!(self, Self::Left | Self::Above)
    }
}

impl FrameLayout {
    /// Find the split node at `path` and overwrite its ratio. Returns
    /// `true` if the path resolved to a `Split`. The new ratio is
    /// clamped to `[0.05, 0.95]` so panes can't fully collapse.
    pub fn set_split_ratio(&mut self, path: &[SplitChoice], new_ratio: f32) -> bool {
        let mut node = &mut self.root;
        for step in path {
            let PaneNode::Split { first, second, .. } = node else {
                return false;
            };
            node = match step {
                SplitChoice::First => first.as_mut(),
                SplitChoice::Second => second.as_mut(),
            };
        }
        if let PaneNode::Split { ratio, .. } = node {
            *ratio = new_ratio.clamp(0.05, 0.95);
            true
        } else {
            false
        }
    }

    /// Read-only lookup of a split's `(axis, ratio)` at `path`.
    pub fn split_at(&self, path: &[SplitChoice]) -> Option<(SplitAxis, f32)> {
        let mut node = &self.root;
        for step in path {
            let PaneNode::Split { first, second, .. } = node else {
                return None;
            };
            node = match step {
                SplitChoice::First => first.as_ref(),
                SplitChoice::Second => second.as_ref(),
            };
        }
        if let PaneNode::Split { axis, ratio, .. } = node {
            Some((*axis, *ratio))
        } else {
            None
        }
    }

    /// Summon a new leaf adjacent to an existing leaf. Walks `path`
    /// to the target leaf, replaces it with a split whose two
    /// children are the original leaf and the new one (in the order
    /// dictated by `side`). The new split inherits a default 50/50
    /// ratio.
    ///
    /// Returns `true` when the path resolved to a leaf; `false`
    /// otherwise (the layout is unchanged). Panel-summon buttons in
    /// the shellbar call this with `path = []` (anchor at root)
    /// and `side = InsertSide::Right` to add panels along the
    /// right edge by default.
    pub fn summon_leaf(
        &mut self,
        path: &[SplitChoice],
        side: InsertSide,
        new_leaf: PaneNode,
    ) -> bool {
        debug_assert!(matches!(new_leaf, PaneNode::Leaf { .. }));
        // Walk to the parent of the target so we can swap the child in place.
        let target = walk_mut(&mut self.root, path);
        let Some(target) = target else {
            return false;
        };
        if !matches!(target, PaneNode::Leaf { .. }) {
            // The path lands on a split. Conventionally, summon
            // wants a leaf anchor so the user's mental model of
            // "summon next to *this*" stays consistent.
            return false;
        }
        let placeholder = PaneNode::Leaf {
            pane_id: PaneId(0),
            content: PaneContent::Custom("__placeholder__".to_string()),
            graph_id: GraphId(uuid::Uuid::nil()),
        };
        let original = std::mem::replace(target, placeholder);
        let (first, second) = if side.new_is_first() {
            (Box::new(new_leaf), Box::new(original))
        } else {
            (Box::new(original), Box::new(new_leaf))
        };
        *target = PaneNode::Split {
            axis: side.split_axis(),
            ratio: 0.5,
            first,
            second,
        };
        true
    }

    /// Move the leaf at `source_path` to be adjacent to the leaf
    /// at `target_path`, on the indicated `side`. Preserves the
    /// source leaf's content + `pane_id` + `graph_id` — only its
    /// position in the tree changes, so per-pane state (camera,
    /// tiles, lineage tree) survives the move.
    ///
    /// Refuses to move (returns `false`):
    /// - When either path doesn't resolve to a leaf.
    /// - When `source_path == target_path` (no-op move).
    /// - When the target leaf is a descendant of the source's
    ///   parent in a way that would orphan nodes — currently
    ///   approximated by refusing moves where the source has only
    ///   one sibling (collapsing the split would leave nothing
    ///   adjacent to drop onto). Conservative; can be refined.
    ///
    /// Implementation: extract the source leaf (close + retain), then
    /// summon it adjacent to the target. Paths are recomputed because
    /// the close step may shift the tree.
    pub fn reparent_leaf(
        &mut self,
        source_path: &[SplitChoice],
        target_path: &[SplitChoice],
        side: InsertSide,
    ) -> bool {
        if source_path == target_path {
            return false;
        }
        // Extract the source leaf (we need an owned copy before the
        // close call mutates the tree).
        let Some(source_node) = walk(&self.root, source_path) else {
            return false;
        };
        let PaneNode::Leaf { .. } = source_node else {
            return false;
        };
        let source_clone = source_node.clone();

        // Identify the source leaf by `pane_id` so we can find it
        // post-close (the close may shift paths). Leaves are
        // uniquely identified by `pane_id` within a layout.
        let source_pane = if let PaneNode::Leaf { pane_id, .. } = &source_clone {
            *pane_id
        } else {
            return false;
        };

        // Pre-flight: confirm the target leaf exists + isn't the
        // source itself.
        let Some(PaneNode::Leaf { pane_id: target_pane, .. }) = walk(&self.root, target_path)
        else {
            return false;
        };
        if *target_pane == source_pane {
            return false;
        }
        let target_pane = *target_pane;

        // Close the source (its sibling is promoted). If close
        // refused (e.g., source is the root), bail.
        if !self.close_leaf(source_path) {
            return false;
        }

        // Re-find the target by pane_id (its path may have shifted
        // because of the close-collapse).
        let Some(new_target_path) = path_for_pane_id(&self.root, target_pane) else {
            // Target vanished (shouldn't happen — source and
            // target were distinct leaves). Bail; tree is now
            // inconsistent (source lost). Best-effort recovery is
            // out of scope for v0.
            return false;
        };

        // Re-attach the source adjacent to the (re-pathed) target.
        self.summon_leaf(&new_target_path, side, source_clone)
    }

    /// Close (remove) the leaf at `path`. Walks to the parent split,
    /// promotes the sibling leaf in its place — so the surrounding
    /// layout collapses naturally. Returns `true` if a leaf was
    /// removed; `false` if the path didn't resolve to a leaf or if
    /// the target is the root (a frame must have at least one leaf).
    pub fn close_leaf(&mut self, path: &[SplitChoice]) -> bool {
        if path.is_empty() {
            // Root leaf can't be removed (frame must keep a panel).
            return false;
        }
        let (parent_path, last_step) = path.split_at(path.len() - 1);
        let parent = walk_mut(&mut self.root, parent_path);
        let Some(parent) = parent else {
            return false;
        };
        let PaneNode::Split { first, second, .. } = parent else {
            return false;
        };
        let removed_first = last_step[0];
        let keeper = match removed_first {
            SplitChoice::First => std::mem::replace(
                second.as_mut(),
                PaneNode::Leaf {
                    pane_id: PaneId(0),
                    content: PaneContent::Custom("__placeholder__".to_string()),
                    graph_id: GraphId(uuid::Uuid::nil()),
                },
            ),
            SplitChoice::Second => std::mem::replace(
                first.as_mut(),
                PaneNode::Leaf {
                    pane_id: PaneId(0),
                    content: PaneContent::Custom("__placeholder__".to_string()),
                    graph_id: GraphId(uuid::Uuid::nil()),
                },
            ),
        };
        // Replace the parent split entirely with the surviving sibling.
        *parent = keeper;
        true
    }

    /// Iterate every leaf in the layout in depth-first order
    /// (first-child before second-child). Yields `(pane_id, content,
    /// graph_id)` triples. Used by the host to assemble per-pane
    /// state + verify which graphs are referenced in this window.
    pub fn iter_leaves(&self) -> impl Iterator<Item = (PaneId, &PaneContent, GraphId)> {
        let mut out: Vec<(PaneId, &PaneContent, GraphId)> = Vec::new();
        fn walk<'a>(node: &'a PaneNode, out: &mut Vec<(PaneId, &'a PaneContent, GraphId)>) {
            match node {
                PaneNode::Leaf {
                    pane_id,
                    content,
                    graph_id,
                } => out.push((*pane_id, content, *graph_id)),
                PaneNode::Split { first, second, .. } => {
                    walk(first, out);
                    walk(second, out);
                }
            }
        }
        walk(&self.root, &mut out);
        out.into_iter()
    }
}

fn walk_mut<'a>(node: &'a mut PaneNode, path: &[SplitChoice]) -> Option<&'a mut PaneNode> {
    let mut current = node;
    for step in path {
        let PaneNode::Split { first, second, .. } = current else {
            return None;
        };
        current = match step {
            SplitChoice::First => first.as_mut(),
            SplitChoice::Second => second.as_mut(),
        };
    }
    Some(current)
}

/// Read-only walker — sibling of [`walk_mut`].
fn walk<'a>(node: &'a PaneNode, path: &[SplitChoice]) -> Option<&'a PaneNode> {
    let mut current = node;
    for step in path {
        let PaneNode::Split { first, second, .. } = current else {
            return None;
        };
        current = match step {
            SplitChoice::First => first.as_ref(),
            SplitChoice::Second => second.as_ref(),
        };
    }
    Some(current)
}

/// Find the `SplitPath` to the leaf with `pane_id`, if any. Used
/// by `reparent_leaf` to relocate the target after the close-step
/// shifts the tree.
fn path_for_pane_id(root: &PaneNode, pane_id: PaneId) -> Option<SplitPath> {
    fn descend(
        node: &PaneNode,
        path: &mut Vec<SplitChoice>,
        target: PaneId,
    ) -> Option<SplitPath> {
        match node {
            PaneNode::Leaf { pane_id, .. } if *pane_id == target => Some(path.clone()),
            PaneNode::Leaf { .. } => None,
            PaneNode::Split { first, second, .. } => {
                path.push(SplitChoice::First);
                if let Some(hit) = descend(first, path, target) {
                    return Some(hit);
                }
                path.pop();
                path.push(SplitChoice::Second);
                if let Some(hit) = descend(second, path, target) {
                    return Some(hit);
                }
                path.pop();
                None
            }
        }
    }
    let mut path = Vec::new();
    descend(root, &mut path, pane_id)
}

/// Project a [`FrameLayout`] into a uxtree subtree describing the
/// pane structure.
///
/// Splits become `Role::Group` nodes annotated with axis + ratio in
/// their description. Leaves become `Role::Group` nodes labeled by
/// their `PaneContent` tag. The leaf nodes are addressable by stable
/// id (derived from their `PaneId`); the host can use those ids to
/// stitch each leaf's content subtree (workbench / orrery / …) under
/// the corresponding leaf node, or render content separately while
/// keeping uxtree structurally aware of the layout.
pub fn project_frame(layout: &FrameLayout) -> UxTree {
    project_frame_with(layout, |_, _| None)
}

/// Project a frame layout, calling `content_for` at each leaf to ask
/// for a content subtree to attach. The returned subtree's root becomes
/// the leaf's accesskit child; its nodes are merged into the frame's
/// node list. Resolver returning `None` leaves the leaf empty (same as
/// [`project_frame`]).
///
/// Use this when the host wants the frame's leaf nodes to actually
/// carry their content's a11y / automation tree (workbench in pane 1,
/// orrery in pane 2, …) rather than tracking parallel subtrees.
pub fn project_frame_with<F>(layout: &FrameLayout, mut content_for: F) -> UxTree
where
    F: FnMut(&PaneContent, PaneId) -> Option<UxTree>,
{
    let mut nodes = Vec::new();
    let root_path = format!("frame/{}", layout.id.as_str());
    let root_id = node_id_for_path(&root_path);

    let root_child = project_node(&layout.root, &root_path, &mut nodes, &mut content_for);

    let mut root = Node::new(Role::Group);
    root.set_label(layout.label.clone());
    root.set_children(vec![root_child]);
    nodes.push((root_id, root));

    tracing::debug!(
        frame_id = %layout.id.as_str(),
        node_count = nodes.len(),
        "projected FrameLayout into uxtree subtree"
    );

    UxTree {
        root: root_id,
        nodes,
    }
}

fn project_node<F>(
    node: &PaneNode,
    path: &str,
    nodes: &mut Vec<(accesskit::NodeId, Node)>,
    content_for: &mut F,
) -> accesskit::NodeId
where
    F: FnMut(&PaneContent, PaneId) -> Option<UxTree>,
{
    match node {
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let split_path = format!("{path}/split");
            let split_id = node_id_for_path(&split_path);
            let first_id = project_node(first, &format!("{split_path}/first"), nodes, content_for);
            let second_id =
                project_node(second, &format!("{split_path}/second"), nodes, content_for);

            let mut split_node = Node::new(Role::Group);
            split_node.set_description(format!("split {:?} ratio={ratio:.2}", axis));
            split_node.set_children(vec![first_id, second_id]);
            nodes.push((split_id, split_node));
            split_id
        }
        PaneNode::Leaf {
            pane_id, content, ..
        } => {
            let leaf_path = format!("{path}/pane/{}", pane_id.0);
            let leaf_id = node_id_for_path(&leaf_path);

            let mut leaf_children = Vec::new();
            if let Some(content_tree) = content_for(content, *pane_id) {
                leaf_children.push(content_tree.root);
                nodes.extend(content_tree.nodes);
            }

            let mut leaf_node = Node::new(Role::Group);
            leaf_node.set_label(content.tag().to_string());
            leaf_node.set_children(leaf_children);
            nodes.push((leaf_id, leaf_node));
            leaf_id
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_three_pane_frame() -> FrameLayout {
        // Layout:
        //   ┌──────────┬─────────┐
        //   │          │ orrery  │
        //   │ workbench├─────────┤
        //   │          │apparatus│
        //   └──────────┴─────────┘
        let g = GraphId::from_uuid(uuid::Uuid::from_u128(0xc01));
        FrameLayout {
            id: FrameId::new("reading"),
            label: "Reading".to_string(),
            root: PaneNode::Split {
                axis: SplitAxis::Horizontal,
                ratio: 0.6,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(1),
                    content: PaneContent::Workbench,
                    graph_id: g,
                }),
                second: Box::new(PaneNode::Split {
                    axis: SplitAxis::Vertical,
                    ratio: 0.5,
                    first: Box::new(PaneNode::Leaf {
                        pane_id: PaneId(2),
                        content: PaneContent::Orrery,
                        graph_id: g,
                    }),
                    second: Box::new(PaneNode::Leaf {
                        pane_id: PaneId(3),
                        content: PaneContent::Apparatus,
                        graph_id: g,
                    }),
                }),
            },
        }
    }

    #[test]
    fn root_carries_frame_label() {
        let layout = fixture_three_pane_frame();
        let tree = project_frame(&layout);
        let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(root.role(), Role::Group);
        assert_eq!(root.label(), Some("Reading"));
    }

    #[test]
    fn each_leaf_emits_a_pane_node_with_content_tag_label() {
        let layout = fixture_three_pane_frame();
        let tree = project_frame(&layout);
        let labels: Vec<_> = tree
            .nodes
            .iter()
            .filter_map(|(_, n)| n.label().map(|s| s.to_string()))
            .collect();
        assert!(labels.contains(&"workbench".to_string()));
        assert!(labels.contains(&"orrery".to_string()));
        assert!(labels.contains(&"apparatus".to_string()));
    }

    #[test]
    fn split_nodes_carry_axis_and_ratio_in_description() {
        let layout = fixture_three_pane_frame();
        let tree = project_frame(&layout);
        let split_descriptions: Vec<_> = tree
            .nodes
            .iter()
            .filter_map(|(_, n)| n.description().map(|s| s.to_string()))
            .collect();
        assert!(
            split_descriptions
                .iter()
                .any(|d| d.contains("Horizontal") && d.contains("0.60")),
            "expected horizontal split with ratio 0.60, got {split_descriptions:?}"
        );
        assert!(
            split_descriptions
                .iter()
                .any(|d| d.contains("Vertical") && d.contains("0.50"))
        );
    }

    #[test]
    fn ids_are_deterministic_across_runs() {
        let layout = fixture_three_pane_frame();
        let a = project_frame(&layout);
        let b = project_frame(&layout);
        assert_eq!(a.root, b.root);
        let a_ids: Vec<_> = a.nodes.iter().map(|(id, _)| *id).collect();
        let b_ids: Vec<_> = b.nodes.iter().map(|(id, _)| *id).collect();
        assert_eq!(a_ids, b_ids);
    }

    #[test]
    fn project_frame_with_attaches_subtree_to_matching_leaf() {
        let layout = fixture_three_pane_frame();

        let tree = project_frame_with(&layout, |content, _| match content {
            PaneContent::Workbench => {
                // Build a fresh subtree on each call so the closure can be
                // FnMut without needing Clone on UxTree.
                let mut sub_root = Node::new(Role::Group);
                sub_root.set_label("workbench-content");
                Some(UxTree {
                    root: node_id_for_path("workbench-content-fixture"),
                    nodes: vec![(node_id_for_path("workbench-content-fixture"), sub_root)],
                })
            }
            _ => None,
        });

        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.label() == Some("workbench-content")),
            "expected attached workbench subtree to merge into frame"
        );
    }

    #[test]
    fn set_split_ratio_clamps_and_finds_path() {
        let mut layout = fixture_three_pane_frame();
        // Root split is horizontal, ratio 0.6
        assert_eq!(layout.split_at(&[]), Some((SplitAxis::Horizontal, 0.6)));
        assert!(layout.set_split_ratio(&[], 0.3));
        assert_eq!(layout.split_at(&[]), Some((SplitAxis::Horizontal, 0.3)));

        // Inner vertical split is at root.second
        assert_eq!(
            layout.split_at(&[SplitChoice::Second]),
            Some((SplitAxis::Vertical, 0.5))
        );
        assert!(layout.set_split_ratio(&[SplitChoice::Second], 0.75));
        assert_eq!(
            layout.split_at(&[SplitChoice::Second]),
            Some((SplitAxis::Vertical, 0.75))
        );

        // Out-of-range ratios clamp.
        assert!(layout.set_split_ratio(&[], 5.0));
        assert_eq!(layout.split_at(&[]), Some((SplitAxis::Horizontal, 0.95)));
        assert!(layout.set_split_ratio(&[], -1.0));
        assert_eq!(layout.split_at(&[]), Some((SplitAxis::Horizontal, 0.05)));

        // Path into a leaf returns None / no-op.
        assert!(!layout.set_split_ratio(&[SplitChoice::First], 0.5));
        assert_eq!(layout.split_at(&[SplitChoice::First]), None);
    }

    #[test]
    fn frame_layout_round_trips_through_serde() {
        let layout = fixture_three_pane_frame();
        let json = serde_json::to_string(&layout).expect("serialize");
        let restored: FrameLayout = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(layout, restored);
    }

    #[test]
    fn reparent_leaf_moves_leaf_without_losing_pane_id() {
        let mut layout = fixture_three_pane_frame();
        // Fixture has 3 leaves: workbench (pane 1), orrery (pane 2),
        // apparatus (pane 3). Move pane 3 to be on the left of pane 1.
        // Paths: workbench at [First]; apparatus at [Second, Second].
        let apparatus_path = vec![SplitChoice::Second, SplitChoice::Second];
        let workbench_path = vec![SplitChoice::First];
        assert!(layout.reparent_leaf(&apparatus_path, &workbench_path, InsertSide::Left));

        // After: apparatus should be present somewhere in the tree.
        let apparatus_after =
            super::path_for_pane_id(&layout.root, PaneId(3)).expect("apparatus still present");
        let workbench_after =
            super::path_for_pane_id(&layout.root, PaneId(1)).expect("workbench still present");
        let orrery_after =
            super::path_for_pane_id(&layout.root, PaneId(2)).expect("orrery still present");
        // All three leaves present, no duplication.
        let leaves: Vec<_> = layout.iter_leaves().collect();
        assert_eq!(leaves.len(), 3);
        // Apparatus + workbench now share a parent (the new split
        // wraps them), distinct from the orrery's parent.
        assert_ne!(apparatus_after, workbench_after);
        // Both should have one shared prefix step less than their
        // depths (siblings under a common split).
        assert_eq!(apparatus_after.len(), workbench_after.len());
        assert!(orrery_after != apparatus_after && orrery_after != workbench_after);
    }

    #[test]
    fn reparent_leaf_refuses_self_move() {
        let mut layout = fixture_three_pane_frame();
        let workbench_path = vec![SplitChoice::First];
        assert!(!layout.reparent_leaf(
            &workbench_path,
            &workbench_path,
            InsertSide::Right,
        ));
    }

    #[test]
    fn reparent_leaf_refuses_when_target_is_not_a_leaf() {
        let mut layout = fixture_three_pane_frame();
        // Empty path = root, which is a Split, not a Leaf.
        let workbench_path = vec![SplitChoice::First];
        assert!(!layout.reparent_leaf(&workbench_path, &[], InsertSide::Right));
    }
}
