/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-pane accessibility subtree builders: each maps one pane's domain state
//! (the roster, gloss, comms, and the generic fallback) into an AccessKit
//! subtree, dispatched by [`a11y_content_tree`]. The orchestration that stitches
//! these into the window tree lives in [`frame_a11y`](super::frame_a11y).
//! Factored out of `frame_ops.rs` to keep files under the 600-LOC ceiling.

use std::collections::HashMap;

use accesskit::{Action, Node, NodeId as AccessNodeId, Role};
use forme::GraphMemberId;
use frame::{PaneContent, PaneId};
use layout_dom_api::LayoutDom;
use uxtree::{UxTree, node_id_for_path};

use super::frame_a11y::rect;
use super::{A11yHostAction, WindowCtx};

impl WindowCtx<'_> {
    pub(super) fn a11y_content_tree(
        &self,
        content: &PaneContent,
        pane_id: PaneId,
        action_routes: &mut HashMap<AccessNodeId, A11yHostAction>,
    ) -> UxTree {
        match content {
            PaneContent::Orrery => mere_orrery::project_graph(self.orrery().graph()),
            PaneContent::Workbench => workbench_domain::project_workbench(&self.view.workbench),
            PaneContent::Apparatus | PaneContent::System => apparatus_domain::project_skeleton(),
            PaneContent::Roster => self.roster_a11y_tree(pane_id, action_routes),
            PaneContent::Gloss => self.gloss_a11y_tree(pane_id),
            PaneContent::Comms => self.comms_a11y_tree(pane_id),
            PaneContent::Inspector
            | PaneContent::Trail
            | PaneContent::Steward
            | PaneContent::Tile(_)
            | PaneContent::Custom(_) => {
                generic_pane_content_tree(&self.view.frame_layout, pane_id, content)
            }
        }
    }

    fn roster_a11y_tree(
        &self,
        pane_id: PaneId,
        action_routes: &mut HashMap<AccessNodeId, A11yHostAction>,
    ) -> UxTree {
        let root_path = pane_content_root_path(&self.view.frame_layout, pane_id, "roster");
        let root = node_id_for_path(&root_path);
        // The roster is folded into the shell document, so its row geometry comes off the
        // shell session's cached layout (keyed by member), the rect-cache replacement now
        // sourced from the one shell layout rather than a separate roster pane. (Phase 1.)
        let row_bounds: HashMap<GraphMemberId, [f32; 4]> = {
            let mut map = HashMap::new();
            if let Some(session) = self.view.chrome_session.as_ref() {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let root = dom.document();
                let mut rows = crate::all_with_class(&dom, root, "roster-row");
                rows.extend(crate::all_with_class(&dom, root, "roster-row-selected"));
                let scroll = self.view.roster_scroll;
                for node in rows {
                    if let (Some(member), Some(l)) =
                        (crate::member_attr(&dom, node), frags.rect_of(node))
                    {
                        let x0 = l.location.x;
                        let y0 = l.location.y - scroll;
                        map.insert(member, [x0, y0, x0 + l.size.width, y0 + l.size.height]);
                    }
                }
            }
            map
        };
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for row in self.roster_rows() {
            let id = node_id_for_path(&format!("{root_path}/row/{}", row.member));
            let mut node = Node::new(Role::ListItem);
            node.add_action(Action::Click);
            node.add_action(Action::Focus);
            action_routes.insert(id, A11yHostAction::SelectNodeByUrl(row.title.clone()));
            node.set_label(row.title);
            let desc = if row.selected {
                format!("selected; {}", row.url)
            } else {
                row.url
            };
            node.set_description(desc);
            if let Some(bounds) = row_bounds.get(&row.member) {
                node.set_bounds(rect(*bounds));
            }
            nodes.push((id, node));
            children.push(id);
        }
        let mut root_node = Node::new(Role::List);
        root_node.set_label("Roster");
        root_node.set_children(children);
        nodes.push((root, root_node));
        UxTree { root, nodes }
    }

    fn gloss_a11y_tree(&self, pane_id: PaneId) -> UxTree {
        let root_path = pane_content_root_path(&self.view.frame_layout, pane_id, "gloss");
        let root = node_id_for_path(&root_path);
        let node_bounds: HashMap<GraphMemberId, [f32; 4]> =
            self.view.gloss_node_rects.iter().copied().collect();
        let focused = self.orrery().focused_member();
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (_key, graph_node) in self.orrery().graph().nodes() {
            let id = node_id_for_path(&format!("{root_path}/node/{}", graph_node.id));
            let mut node = Node::new(Role::Link);
            let label = if graph_node.title.is_empty() {
                graph_node.primary_address().as_url_str().to_string()
            } else {
                graph_node.title.clone()
            };
            node.set_label(label);
            node.set_value(graph_node.primary_address().as_url_str().to_string());
            if focused == Some(graph_node.id) {
                node.set_description("focused");
            }
            if let Some(bounds) = node_bounds.get(&graph_node.id) {
                node.set_bounds(rect(*bounds));
            }
            nodes.push((id, node));
            children.push(id);
        }
        let mut root_node = Node::new(Role::Group);
        root_node.set_label("Gloss");
        root_node.set_children(children);
        nodes.push((root, root_node));
        UxTree { root, nodes }
    }

    fn comms_a11y_tree(&self, pane_id: PaneId) -> UxTree {
        let root_path = pane_content_root_path(&self.view.frame_layout, pane_id, "comms");
        let root = node_id_for_path(&root_path);
        let comms = &self.view.chrome().comms;
        let mut nodes = Vec::new();
        let mut children = Vec::new();

        let inbox_root = node_id_for_path(&format!("{root_path}/inbox"));
        let mut inbox_children = Vec::new();
        for conversation in &comms.inbox {
            let id = node_id_for_path(&format!(
                "{root_path}/inbox/{:?}/{}",
                conversation.id.protocol, conversation.id.key
            ));
            let mut node = Node::new(Role::ListItem);
            node.set_label(conversation.title.clone());
            node.set_description(format!(
                "{:?}; unread={}",
                conversation.id.protocol, conversation.unread
            ));
            nodes.push((id, node));
            inbox_children.push(id);
        }
        let mut inbox = Node::new(Role::List);
        inbox.set_label("Conversations");
        inbox.set_children(inbox_children);
        nodes.push((inbox_root, inbox));
        children.push(inbox_root);

        let thread_root = node_id_for_path(&format!("{root_path}/thread"));
        let mut thread_children = Vec::new();
        for message in &comms.thread {
            let id = node_id_for_path(&format!("{root_path}/thread/{}", message.id.0));
            let mut node = Node::new(Role::Paragraph);
            node.set_label(message.author.label().to_string());
            node.set_value(message.body.text().to_string());
            node.set_description(format!("{:?}", message.direction));
            nodes.push((id, node));
            thread_children.push(id);
        }
        let mut thread = Node::new(Role::Group);
        thread.set_label("Thread");
        thread.set_children(thread_children);
        nodes.push((thread_root, thread));
        children.push(thread_root);

        let draft_root = node_id_for_path(&format!("{root_path}/draft"));
        let mut draft = Node::new(Role::TextInput);
        draft.set_label("Draft");
        draft.set_value(self.view.chrome().comms_draft.text().to_string());
        nodes.push((draft_root, draft));
        children.push(draft_root);

        let mut root_node = Node::new(Role::Group);
        root_node.set_label("Comms");
        root_node.set_children(children);
        nodes.push((root, root_node));
        UxTree { root, nodes }
    }
}

pub(super) fn generic_pane_content_tree(
    layout: &frame::FrameLayout,
    pane_id: PaneId,
    content: &PaneContent,
) -> UxTree {
    let root_path = pane_content_root_path(layout, pane_id, content.tag());
    let root = node_id_for_path(&root_path);
    let mut node = Node::new(Role::Group);
    node.set_label(content.tag().to_string());
    UxTree {
        root,
        nodes: vec![(root, node)],
    }
}

fn pane_content_root_path(layout: &frame::FrameLayout, pane_id: PaneId, tag: &str) -> String {
    format!(
        "meerkat/frame/{}/pane/{}/content/{tag}",
        layout.id.as_str(),
        pane_id.0
    )
}
