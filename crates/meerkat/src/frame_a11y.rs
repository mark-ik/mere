/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Accessibility projection orchestration for a window: building the AccessKit
//! tree from chrome + frame + per-pane content (delegating each pane's subtree
//! to [`frame_a11y_panes`](super::frame_a11y_panes)), routing screen-reader
//! actions back to the host, and the audit that backs Apparatus's a11y readout.
//! Factored out of `frame_ops.rs` to keep files under the 600-LOC ceiling.

use std::collections::HashMap;

use accesskit::{Action, Node, NodeId as AccessNodeId, Rect, Role, TreeUpdate};
use frame::{PaneContent, PaneId, PaneNode};
use uxtree::{UxTree, node_id_for_path};

use super::observability::A11ySnapshot;
use super::{A11yHostAction, WindowCtx};

impl WindowCtx<'_> {
    /// Refresh the shared a11y projection used by Apparatus and the OS bridge.
    pub(super) fn refresh_a11y_summary(&mut self) {
        let projection = self.build_a11y_projection();
        self.a11y_bridge.update(projection.tree_update());
        *self.a11y_action_routes = projection.action_routes;
        self.shared
            .observability
            .set_a11y_snapshot(projection.snapshot);
    }

    pub(super) fn drain_a11y_actions(&mut self) {
        for request in self.a11y_bridge.drain_actions() {
            self.apply_a11y_request(request);
        }
    }

    pub(super) fn apply_a11y_request(&mut self, request: super::a11y_bridge::A11yActionRequest) {
        let action_id = format!("{:?}", request.action).to_ascii_lowercase();
        match self.a11y_action_routes.get(&request.target_node).cloned() {
            Some(A11yHostAction::SelectNodeByUrl(url))
                if matches!(request.action, Action::Click | Action::Focus) =>
            {
                if self.orrery_mut().select_by_url(&url) {
                    self.view.active_content = super::ContentPane::Orrery;
                    self.sync_location();
                    self.shared.observability.record_diagnostic(
                        "meerkat.agent.action_applied",
                        super::observability::Severity::Info,
                        format!("accesskit.{action_id}: select {url}"),
                    );
                    self.refresh_a11y_summary();
                    self.view.request_redraw();
                } else {
                    self.shared.observability.record_diagnostic(
                        "meerkat.agent.intent_dropped",
                        super::observability::Severity::Warn,
                        format!("accesskit.{action_id}: missing node {url}"),
                    );
                }
            }
            Some(A11yHostAction::ChromeNode(node)) => match request.action {
                // Focus the chrome control directly — the same as a programmatic
                // `element.focus()` (the omnibar field, a palette row).
                Action::Focus => {
                    self.view.runner.set_focus(Some(node));
                    self.shared.observability.record_diagnostic(
                        "meerkat.agent.action_applied",
                        super::observability::Severity::Info,
                        format!("accesskit.{action_id}: focus chrome node"),
                    );
                    self.refresh_a11y_summary();
                    self.view.request_redraw();
                }
                // Activate it through the same path a pointer click drives: dispatch
                // to the node (element-local origin, which chrome controls ignore)
                // and drain whatever intents its handler queued.
                Action::Click => {
                    self.chrome_activate(node, (0.0, 0.0));
                    self.shared.observability.record_diagnostic(
                        "meerkat.agent.action_applied",
                        super::observability::Severity::Info,
                        format!("accesskit.{action_id}: activate chrome node"),
                    );
                    self.refresh_a11y_summary();
                }
                _ => {
                    self.shared.observability.record_diagnostic(
                        "meerkat.agent.intent_dropped",
                        super::observability::Severity::Warn,
                        format!("accesskit.{action_id}: unsupported action for chrome node"),
                    );
                }
            },
            Some(_) => {
                self.shared.observability.record_diagnostic(
                    "meerkat.agent.intent_dropped",
                    super::observability::Severity::Warn,
                    format!("accesskit.{action_id}: unsupported action for routed node"),
                );
            }
            None => {
                self.shared.observability.record_diagnostic(
                    "meerkat.agent.intent_dropped",
                    super::observability::Severity::Warn,
                    format!(
                        "accesskit.{action_id}: no route for target {:?}",
                        request.target_node
                    ),
                );
            }
        }
    }

    pub(super) fn update_a11y_window_focus(&mut self, focused: bool) {
        self.a11y_bridge.update_window_focus(focused);
        self.refresh_a11y_summary();
    }

    pub(super) fn build_a11y_projection(&self) -> A11yProjection {
        let leaves = self.laid_leaves();
        let surfaces = leaves.len() + 2; // host window + chrome root + content leaves
        // C4c: the chrome a11y subtree derives from the chrome DOM that renders
        // (the session's retained layout for bounds), so a screen reader navigates
        // the real toolbar / omnibar / buttons rather than one placeholder node.
        // Falls back to a single node covering the toolbar band before the first
        // render builds the session.
        let (chrome_tree, chrome_actionable) = match &self.view.chrome_session {
            Some(session) => {
                let dom = self.view.dom.borrow();
                crate::serval_a11y::chrome_a11y_tree(&dom, session.fragments())
            }
            None => {
                let mut chrome = Node::new(Role::Application);
                chrome.set_label("Chrome");
                chrome.set_bounds(Rect::new(
                    0.0,
                    0.0,
                    self.view.width as f64,
                    self.view.toolbar_h as f64,
                ));
                let chrome_root = node_id_for_path("meerkat/chrome");
                (
                    UxTree {
                        root: chrome_root,
                        nodes: vec![(chrome_root, chrome)],
                    },
                    Vec::new(),
                )
            }
        };
        let chrome_root = chrome_tree.root;
        let mut action_routes = HashMap::new();
        // Route each actionable chrome control to its whole DOM node, keyed by the
        // same salted id the projection gave that node, so a screen reader's request
        // resolves here in `apply_a11y_request`. Storing the node (not the salted id
        // reversed back to one) sidesteps the debug-broken doc-tag overlap. (G2.4.)
        for node in chrome_actionable {
            action_routes.insert(
                crate::serval_a11y::chrome_a11y_id(node),
                A11yHostAction::ChromeNode(node),
            );
        }
        let leaf_bounds: HashMap<PaneId, [f32; 4]> = leaves
            .iter()
            .map(|leaf| (leaf.pane_id, leaf.rect))
            .collect();
        let mut frame_tree =
            frame::project_frame_with(&self.view.frame_layout, |content, pane_id| {
                Some(self.a11y_content_tree(content, pane_id, &mut action_routes))
            });
        attach_frame_bounds(
            &mut frame_tree,
            &self.view.frame_layout,
            &leaf_bounds,
            self.content_band(),
        );
        let frame_root = frame_tree.root;
        let mut host = Node::new(Role::Window);
        host.set_label("Meerkat");
        host.set_bounds(Rect::new(
            0.0,
            0.0,
            self.view.width as f64,
            self.view.height as f64,
        ));
        let mut tree = uxtree::stitch("meerkat/window", host, vec![chrome_tree, frame_tree]);
        attach_link_actions(&mut tree, &mut action_routes);
        let (requested_focus, fallback_focus) = match self.view.runner.focus() {
            // The focused chrome DOM node (the omnibar field) when the DOM-derived
            // subtree is in use; the chrome subtree root in the placeholder fallback.
            Some(focused) if self.view.chrome_session.is_some() => {
                (crate::serval_a11y::chrome_a11y_id(focused), chrome_root)
            }
            Some(_) => (chrome_root, chrome_root),
            None => (
                self.active_frame_focus_node().unwrap_or(frame_root),
                frame_root,
            ),
        };
        let focus = valid_focus(&tree, requested_focus, fallback_focus);
        let audit = audit_a11y_tree(&tree, focus);
        let degraded = match self.a11y_bridge.status() {
            super::a11y_bridge::BridgeStatus::Installed => 0,
            super::a11y_bridge::BridgeStatus::Unavailable => surfaces,
        };
        let snapshot = A11ySnapshot {
            surfaces,
            degraded,
            nodes: tree.nodes.len(),
            missing_labels: audit.missing_labels,
            missing_bounds: audit.missing_bounds,
            duplicate_ids: audit.duplicate_ids,
            root: format_access_node(tree.root),
            focus: format_access_node(focus),
            audit: audit.findings,
        };
        A11yProjection {
            tree,
            focus,
            snapshot,
            action_routes,
        }
    }

    fn active_frame_focus_node(&self) -> Option<AccessNodeId> {
        let content = if self.workbench_active() {
            PaneContent::Workbench
        } else {
            PaneContent::Orrery
        };
        self.pane_of_content(&content)
            .and_then(|pane_id| frame_leaf_id(&self.view.frame_layout, pane_id))
    }
}

struct A11yAudit {
    missing_labels: usize,
    missing_bounds: usize,
    duplicate_ids: usize,
    findings: Vec<String>,
}

pub(super) struct A11yProjection {
    tree: UxTree,
    focus: AccessNodeId,
    snapshot: A11ySnapshot,
    action_routes: HashMap<AccessNodeId, A11yHostAction>,
}

impl A11yProjection {
    pub(super) fn tree_update(&self) -> TreeUpdate {
        self.tree.to_tree_update(Some(self.focus))
    }
}

fn audit_a11y_tree(tree: &UxTree, focus: AccessNodeId) -> A11yAudit {
    let mut seen = std::collections::HashSet::new();
    let mut focus_found = false;
    let mut missing_labels = 0usize;
    let mut missing_bounds = 0usize;
    let mut duplicate_ids = 0usize;
    let mut findings = Vec::new();
    for (id, node) in &tree.nodes {
        if !seen.insert(*id) {
            duplicate_ids += 1;
            findings.push(format!("duplicate id {}", format_access_node(*id)));
        }
        if *id == focus {
            focus_found = true;
        }
        let has_name = node.label().is_some_and(|label| !label.trim().is_empty())
            || node
                .description()
                .is_some_and(|description| !description.trim().is_empty());
        if !has_name {
            missing_labels += 1;
        }
        if node.bounds().is_none() {
            missing_bounds += 1;
        }
    }
    if !focus_found {
        findings.push(format!(
            "focused node {} is not in the current tree",
            format_access_node(focus)
        ));
    }
    if missing_labels > 0 {
        findings.push(format!("{missing_labels} nodes lack labels/descriptions"));
    }
    if missing_bounds > 0 {
        findings.push(format!("{missing_bounds} nodes lack bounds"));
    }
    A11yAudit {
        missing_labels,
        missing_bounds,
        duplicate_ids,
        findings,
    }
}

fn valid_focus(tree: &UxTree, requested: AccessNodeId, fallback: AccessNodeId) -> AccessNodeId {
    if tree_has_node(tree, requested) {
        requested
    } else if tree_has_node(tree, fallback) {
        fallback
    } else {
        tree.root
    }
}

fn tree_has_node(tree: &UxTree, node: AccessNodeId) -> bool {
    tree.nodes.iter().any(|(id, _)| *id == node)
}

fn attach_link_actions(
    tree: &mut UxTree,
    action_routes: &mut HashMap<AccessNodeId, A11yHostAction>,
) {
    for (id, node) in &mut tree.nodes {
        if node.role() != Role::Link {
            continue;
        }
        let Some(url) = node.value().map(str::to_string) else {
            continue;
        };
        node.add_action(Action::Click);
        node.add_action(Action::Focus);
        action_routes.insert(*id, A11yHostAction::SelectNodeByUrl(url));
    }
}

fn format_access_node(id: AccessNodeId) -> String {
    format!("node:{}", id.0)
}

fn attach_frame_bounds(
    tree: &mut UxTree,
    layout: &frame::FrameLayout,
    leaf_bounds: &HashMap<PaneId, [f32; 4]>,
    content_band: [f32; 4],
) {
    if let Some(root) = node_mut(tree, tree.root) {
        root.set_bounds(rect(content_band));
    }
    for (pane_id, bounds) in leaf_bounds {
        let Some(leaf_id) = frame_leaf_id(layout, *pane_id) else {
            continue;
        };
        let content_root =
            node_mut(tree, leaf_id).and_then(|node| node.children().first().copied());
        if let Some(node) = node_mut(tree, leaf_id) {
            node.set_bounds(rect(*bounds));
        }
        if let Some(content_root) = content_root {
            if let Some(node) = node_mut(tree, content_root) {
                node.set_bounds(rect(*bounds));
            }
        }
    }
}

fn frame_leaf_id(layout: &frame::FrameLayout, pane_id: PaneId) -> Option<AccessNodeId> {
    frame_leaf_id_at(
        &layout.root,
        pane_id,
        &format!("frame/{}", layout.id.as_str()),
    )
}

fn frame_leaf_id_at(node: &PaneNode, pane_id: PaneId, path: &str) -> Option<AccessNodeId> {
    match node {
        PaneNode::Leaf { pane_id: id, .. } if *id == pane_id => {
            Some(node_id_for_path(&format!("{path}/pane/{}", pane_id.0)))
        }
        PaneNode::Leaf { .. } => None,
        PaneNode::Split { first, second, .. } => {
            let split_path = format!("{path}/split");
            frame_leaf_id_at(first, pane_id, &format!("{split_path}/first"))
                .or_else(|| frame_leaf_id_at(second, pane_id, &format!("{split_path}/second")))
        }
    }
}

fn node_mut(tree: &mut UxTree, id: AccessNodeId) -> Option<&mut Node> {
    tree.nodes
        .iter_mut()
        .find(|(node_id, _)| *node_id == id)
        .map(|(_, node)| node)
}

pub(super) fn rect(bounds: [f32; 4]) -> Rect {
    Rect::new(
        bounds[0] as f64,
        bounds[1] as f64,
        bounds[2] as f64,
        bounds[3] as f64,
    )
}

#[cfg(test)]
mod a11y_tests {
    use super::*;
    use crate::frame_a11y_panes::generic_pane_content_tree;
    use frame::{GraphId, SplitAxis};

    fn layout_with_two_panes() -> frame::FrameLayout {
        frame::FrameLayout {
            id: frame::FrameId::new("content"),
            label: "content".to_string(),
            root: PaneNode::Split {
                axis: SplitAxis::Horizontal,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(0),
                    content: PaneContent::Orrery,
                    graph_id: GraphId::default(),
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(1),
                    content: PaneContent::Roster,
                    graph_id: GraphId::default(),
                }),
            },
        }
    }

    #[test]
    fn frame_leaf_ids_match_frame_projection_paths() {
        let layout = layout_with_two_panes();
        let tree = frame::project_frame(&layout);
        assert!(
            tree.nodes
                .iter()
                .any(|(id, _)| Some(*id) == frame_leaf_id(&layout, PaneId(0)))
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(id, _)| Some(*id) == frame_leaf_id(&layout, PaneId(1)))
        );
    }

    #[test]
    fn host_attaches_bounds_to_frame_leaves_and_content_roots() {
        let layout = layout_with_two_panes();
        let mut tree = frame::project_frame_with(&layout, |content, pane_id| {
            Some(generic_pane_content_tree(&layout, pane_id, content))
        });
        let bounds = HashMap::from([
            (PaneId(0), [0.0, 40.0, 400.0, 600.0]),
            (PaneId(1), [400.0, 40.0, 800.0, 600.0]),
        ]);
        attach_frame_bounds(&mut tree, &layout, &bounds, [0.0, 40.0, 800.0, 600.0]);

        for pane_id in [PaneId(0), PaneId(1)] {
            let leaf_id = frame_leaf_id(&layout, pane_id).expect("leaf id");
            let leaf = tree
                .nodes
                .iter()
                .find(|(id, _)| *id == leaf_id)
                .unwrap()
                .1
                .clone();
            assert!(leaf.bounds().is_some(), "leaf {pane_id:?} has bounds");
            let content_root = leaf.children().first().copied().expect("content root");
            let content = tree
                .nodes
                .iter()
                .find(|(id, _)| *id == content_root)
                .unwrap()
                .1
                .clone();
            assert!(content.bounds().is_some(), "content root has bounds");
        }
    }

    #[test]
    fn a11y_audit_reports_focus_membership_and_bound_gaps() {
        let layout = layout_with_two_panes();
        let mut tree = frame::project_frame(&layout);
        let bounds = HashMap::from([(PaneId(0), [0.0, 40.0, 400.0, 600.0])]);
        attach_frame_bounds(&mut tree, &layout, &bounds, [0.0, 40.0, 800.0, 600.0]);

        let audit = audit_a11y_tree(&tree, frame_leaf_id(&layout, PaneId(0)).expect("leaf id"));
        assert_eq!(audit.duplicate_ids, 0);
        assert!(
            audit.missing_bounds > 0,
            "unbounded split/second pane is reported"
        );

        let missing_focus = audit_a11y_tree(&tree, node_id_for_path("missing-focus"));
        assert!(
            missing_focus
                .findings
                .iter()
                .any(|finding| finding.contains("focused node"))
        );
    }

    #[test]
    fn valid_focus_never_returns_a_missing_node() {
        let root = node_id_for_path("app-root");
        let child = node_id_for_path("app-root/child");
        let missing = node_id_for_path("missing-focus");
        let mut root_node = Node::new(Role::Window);
        root_node.set_children(vec![child]);
        let child_node = Node::new(Role::Button);
        let tree = UxTree {
            root,
            nodes: vec![(root, root_node), (child, child_node)],
        };

        assert_eq!(valid_focus(&tree, child, root), child);
        assert_eq!(valid_focus(&tree, missing, child), child);
        assert_eq!(valid_focus(&tree, missing, missing), root);
    }
}
