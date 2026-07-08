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
use layout_dom_api::{LayoutDom, Namespace, NodeKind};
use serval_scripted_dom::NodeId;
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
        use crate::window_view::ShellListPane;
        match content {
            PaneContent::Orrery => self.orrery_a11y_tree(pane_id),
            PaneContent::Workbench => workbench_domain::project_workbench(&self.view.workbench),
            PaneContent::Apparatus => self.list_pane_a11y_tree(
                ShellListPane::Apparatus,
                "apparatus",
                "Apparatus",
                pane_id,
                action_routes,
            ),
            PaneContent::Roster => self.roster_a11y_tree(pane_id, action_routes),
            PaneContent::Gloss => self.gloss_a11y_tree(pane_id, action_routes),
            PaneContent::Comms => self.comms_a11y_tree(pane_id),
            PaneContent::Steward => self.list_pane_a11y_tree(
                ShellListPane::Steward,
                "steward",
                "Steward",
                pane_id,
                action_routes,
            ),
            PaneContent::Inspector => self.list_pane_a11y_tree(
                ShellListPane::Inspector,
                "inspector",
                "Inspector",
                pane_id,
                action_routes,
            ),
            PaneContent::Trail => self.list_pane_a11y_tree(
                ShellListPane::Trail,
                "trail",
                "Trail",
                pane_id,
                action_routes,
            ),
            PaneContent::Alembic => self.list_pane_a11y_tree(
                ShellListPane::Alembic,
                "alembic",
                "Alembic",
                pane_id,
                action_routes,
            ),
            // System is not folded into the shell document, so it keeps the skeleton.
            PaneContent::System | PaneContent::Tile(_) | PaneContent::Custom(_) => {
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
                // Taffy fragment locations are parent-relative; the roster rows sit
                // inside the positioned roster pane, so their absolute bounds need the
                // ancestor offsets summed in (the same accumulation the scrollbar
                // overlay uses), not the bare `location`. (Phase 1.)
                // Painted origins fold the roster pane's retained `element_scroll`, so a row's
                // bounds already account for the wheel scroll. (Host-scroll P2.)
                let origins = serval_layout::accumulate_painted_origins(
                    &*dom,
                    frags,
                    session.element_scroll(),
                );
                let mut rows = crate::all_with_class(&dom, root, "roster-row");
                rows.extend(crate::all_with_class(&dom, root, "roster-row-selected"));
                for node in rows {
                    if let (Some(member), Some(l), Some(p)) = (
                        crate::member_attr(&dom, node),
                        frags.rect_of(node),
                        origins.get(&node),
                    ) {
                        map.insert(member, [p.x, p.y, p.x + l.size.width, p.y + l.size.height]);
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

    /// The a11y subtree for the orrery pane: each graph node as a `Role::Link` carrying the
    /// node's URL as its value, so the post-stitch `attach_link_actions` pass makes it
    /// click/focus-actionable and routes it to `SelectNodeByUrl` (the select gyre's pointer
    /// hit-test also drives). Bounds come off the shell document's laid-out `.gnode` divs:
    /// each gnode's absolute origin (ancestor taffy offsets summed) plus its accumulated CSS
    /// `translate` (gyre's world position, which the fragments omit — the same offset the focus
    /// ring adds), keyed by the gnode's `data-member`, so the a11y rect tracks where the gnode
    /// actually paints, not the graph-space coordinate the retired `project_graph` reported. A
    /// node culled off-pane (riding the underlay demote-dots, no gnode) stays listed but
    /// bound-less. With this rich projection the chrome walk skips the `.orrery` subtree, so each
    /// node appears once. (Slice 4.)
    fn orrery_a11y_tree(&self, pane_id: PaneId) -> UxTree {
        let root_path = pane_content_root_path(&self.view.frame_layout, pane_id, "orrery");
        let root = node_id_for_path(&root_path);
        // Per-node rect off the shell layout: each gnode root's absolute origin plus
        // its accumulated CSS translate, keyed by `data-member` (the same scheme the roster +
        // workbench placeholders use). The gnodes are not in a scroll container (the orrery pans
        // by the per-gnode transform, not DOM scroll), so the unscrolled `accumulate_origins`
        // suffices, as it does for the roster rows. (Slice 4.)
        let gnode_bounds: HashMap<GraphMemberId, [f32; 4]> = {
            let mut map = HashMap::new();
            if let Some(session) = self.view.chrome_session.as_ref() {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let droot = dom.document();
                let origins = crate::serval_render::accumulate_origins(&dom, frags);
                for node in crate::all_with_class(&dom, droot, "gnode-root") {
                    if dom.attribute(node, &Namespace::from(""), &"data-parked".into())
                        == Some("true")
                    {
                        continue;
                    }
                    if let (Some(member), Some(l), Some(&(ox, oy))) = (
                        crate::member_attr(&dom, node),
                        frags.rect_of(node),
                        origins.get(&node),
                    ) {
                        let (tx, ty) = session.accumulated_translate(&dom, node);
                        let (x0, y0) = (ox + tx, oy + ty);
                        map.insert(member, [x0, y0, x0 + l.size.width, y0 + l.size.height]);
                    }
                }
            }
            map
        };
        let focused = self.orrery().focused_member();
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (_key, graph_node) in self.orrery().graph().nodes() {
            let id = node_id_for_path(&format!("{root_path}/node/{}", graph_node.id));
            let mut node = Node::new(Role::Link);
            let url = graph_node.primary_address().as_url_str().to_string();
            let label = if graph_node.title.is_empty() {
                url.clone()
            } else {
                graph_node.title.clone()
            };
            node.set_label(label);
            node.set_value(url);
            if focused == Some(graph_node.id) {
                node.set_description("focused");
            }
            if let Some(bounds) = gnode_bounds.get(&graph_node.id) {
                node.set_bounds(rect(*bounds));
            }
            nodes.push((id, node));
            children.push(id);
        }
        let mut root_node = Node::new(Role::Group);
        root_node.set_label("Orrery");
        root_node.set_children(children);
        nodes.push((root, root_node));
        UxTree { root, nodes }
    }

    /// The a11y subtree for a folded list pane (apparatus / steward / inspector / trail):
    /// its `ListPaneState` items as nodes. A button item (one carrying an activation key)
    /// becomes an actionable `Button` routed to its DOM node, so a screen reader fires the
    /// same click that drains its activation; a text item becomes a `Label`. Geometry + the
    /// dispatch targets come off the shell session's cached layout (the items are the inner
    /// root's element children, in order; taffy locations are parent-relative, so absolute
    /// bounds accumulate ancestor origins, then subtract the pane's scroll). With this rich
    /// projection the chrome walk skips the pane's subtree, so it appears once. (Phase 1, 3b.)
    fn list_pane_a11y_tree(
        &self,
        which: crate::window_view::ShellListPane,
        inner_class: &str,
        label: &str,
        pane_id: PaneId,
        action_routes: &mut HashMap<AccessNodeId, A11yHostAction>,
    ) -> UxTree {
        let root_path = pane_content_root_path(&self.view.frame_layout, pane_id, inner_class);
        let root = node_id_for_path(&root_path);
        // The item nodes (the inner root's element children, document order) paired with
        // their absolute bounds, for per-item geometry + the button dispatch target.
        let item_geom: Vec<(NodeId, Option<[f32; 4]>)> = {
            let mut out = Vec::new();
            if let Some(session) = self.view.chrome_session.as_ref() {
                let frags = session.fragments();
                let dom = self.view.dom.borrow();
                let droot = dom.document();
                // Painted origins fold the pane's retained `element_scroll`, so a row's bounds
                // already account for the wheel scroll — no host offset to subtract. (P2.)
                let origins = serval_layout::accumulate_painted_origins(
                    &*dom,
                    frags,
                    session.element_scroll(),
                );
                if let Some(inner) = crate::first_with_class(&dom, droot, inner_class) {
                    for child in dom.dom_children(inner) {
                        if dom.kind(child) == NodeKind::Element {
                            let bounds = frags
                                .rect_of(child)
                                .zip(origins.get(&child))
                                .map(|(l, p)| [p.x, p.y, p.x + l.size.width, p.y + l.size.height]);
                            out.push((child, bounds));
                        }
                    }
                }
            }
            out
        };
        let items = &self.multi.state().windows[self.view.projection_id.0].panes[which.idx()].items;
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let id = node_id_for_path(&format!("{root_path}/item/{i}"));
            let mut node = if item.key.is_some() {
                let mut n = Node::new(Role::Button);
                n.add_action(Action::Click);
                if let Some((dom_node, _)) = item_geom.get(i) {
                    action_routes.insert(id, A11yHostAction::ChromeNode(*dom_node));
                }
                n
            } else {
                Node::new(Role::Label)
            };
            node.set_label(item.text.clone());
            if let Some(Some(b)) = item_geom.get(i).map(|(_, b)| *b) {
                node.set_bounds(rect(b));
            }
            nodes.push((id, node));
            children.push(id);
        }
        let mut root_node = Node::new(Role::List);
        root_node.set_label(label.to_string());
        root_node.set_children(children);
        nodes.push((root, root_node));
        UxTree { root, nodes }
    }

    /// The gloss pane's a11y subtree: "Minimap", "Outline"
    /// ([`gloss_outline_a11y_tree`](Self::gloss_outline_a11y_tree)), and "Recent" groups —
    /// the same three sections the DOM renders, so this walks the live layout for bounds
    /// (via [`dom_member_bounds`](Self::dom_member_bounds)) rather than trusting a
    /// host-tracked rect cache (retired by the Scene-to-DOM migration's Phase 3 — the
    /// minimap's edges/rings backdrop is still a Scene, but its node squares and the
    /// recent list are DOM now, same as the outline). Minimap/Recent route
    /// `SelectNodeByUrl`, identical to their mouse path (`drain_gloss_minimap_intents` /
    /// `drain_gloss_recent_intents`). (gloss-outline P1a; Scene-to-DOM migration P3.)
    fn gloss_a11y_tree(
        &self,
        pane_id: PaneId,
        action_routes: &mut HashMap<AccessNodeId, A11yHostAction>,
    ) -> UxTree {
        let root_path = pane_content_root_path(&self.view.frame_layout, pane_id, "gloss");
        let root = node_id_for_path(&root_path);
        let focused = self.orrery().focused_member();
        let graph = self.orrery().graph();
        let mut nodes = Vec::new();

        let minimap_bounds = self.dom_member_bounds("gloss-minimap-node");
        let minimap_root = node_id_for_path(&format!("{root_path}/minimap"));
        let mut minimap_children = Vec::new();
        for (member, bounds) in &minimap_bounds {
            let Some((_, graph_node)) = graph.get_node_by_id(*member) else {
                continue;
            };
            let id = node_id_for_path(&format!("{root_path}/minimap/node/{member}"));
            let mut node = Node::new(Role::Link);
            let url = graph_node.primary_address().as_url_str().to_string();
            let label = if graph_node.title.is_empty() {
                url.clone()
            } else {
                graph_node.title.clone()
            };
            node.add_action(Action::Click);
            node.add_action(Action::Focus);
            action_routes.insert(id, A11yHostAction::SelectNodeByUrl(url.clone()));
            node.set_label(label);
            node.set_value(url);
            if focused == Some(*member) {
                node.set_description("focused");
            }
            node.set_bounds(rect(*bounds));
            nodes.push((id, node));
            minimap_children.push(id);
        }
        let mut minimap_node = Node::new(Role::Group);
        minimap_node.set_label("Minimap");
        minimap_node.set_children(minimap_children);
        nodes.push((minimap_root, minimap_node));

        let (outline_root, outline_nodes) = self.gloss_outline_a11y_tree(&root_path, action_routes);
        nodes.extend(outline_nodes);

        let recent_bounds = self.dom_member_bounds("gloss-recent-row");
        let recent_root = node_id_for_path(&format!("{root_path}/recent"));
        let mut recent_children = Vec::new();
        for (member, bounds) in &recent_bounds {
            let Some((_, graph_node)) = graph.get_node_by_id(*member) else {
                continue;
            };
            let id = node_id_for_path(&format!("{root_path}/recent/row/{member}"));
            let mut node = Node::new(Role::ListItem);
            let url = graph_node.primary_address().as_url_str().to_string();
            node.add_action(Action::Click);
            node.add_action(Action::Focus);
            action_routes.insert(id, A11yHostAction::SelectNodeByUrl(url.clone()));
            node.set_label(url);
            node.set_bounds(rect(*bounds));
            nodes.push((id, node));
            recent_children.push(id);
        }
        let mut recent_node = Node::new(Role::List);
        recent_node.set_label("Recent");
        recent_node.set_children(recent_children);
        nodes.push((recent_root, recent_node));

        let mut root_node = Node::new(Role::Group);
        root_node.set_label("Gloss");
        root_node.set_children(vec![minimap_root, outline_root, recent_root]);
        nodes.push((root, root_node));
        UxTree { root, nodes }
    }

    /// Every element carrying `class` under the chrome document, keyed by its
    /// `data-member` attribute and positioned via the session's retained layout
    /// (fragment rect + accumulated scroll/translate origin) — the same live-layout
    /// bounds lookup [`gloss_outline_a11y_tree`](Self::gloss_outline_a11y_tree)
    /// pioneered, factored out so the minimap and recent sections can reuse it instead
    /// of a host-tracked rect cache. Empty before the chrome session's first render.
    fn dom_member_bounds(&self, class: &str) -> HashMap<GraphMemberId, [f32; 4]> {
        let mut map = HashMap::new();
        let Some(session) = self.view.chrome_session.as_ref() else {
            return map;
        };
        let frags = session.fragments();
        let dom = self.view.dom.borrow();
        let droot = dom.document();
        let origins =
            serval_layout::accumulate_painted_origins(&*dom, frags, session.element_scroll());
        for node in crate::all_with_class(&dom, droot, class) {
            if let (Some(member), Some(l), Some(p)) = (
                crate::member_attr(&dom, node),
                frags.rect_of(node),
                origins.get(&node),
            ) {
                map.insert(member, [p.x, p.y, p.x + l.size.width, p.y + l.size.height]);
            }
        }
        map
    }

    /// The gloss outline lens's a11y nodes: bounds off the shell layout (keyed by
    /// `data-member`, the same scheme the roster rows use), rows from a fresh
    /// [`gloss_outline_snapshot`](Self::gloss_outline_snapshot) (mirroring how
    /// `roster_a11y_tree` recomputes rather than reading cached `ShellState`). A real-node
    /// row routes through `SelectNodeByUrl`, identical to the mouse path
    /// (`drain_gloss_outline_intents`); a structural row is a non-interactive label so the
    /// host/path hierarchy still reads for a screen-reader user, not just a flat leaf list.
    /// Returns `(outline_group_id, nodes)` for [`gloss_a11y_tree`](Self::gloss_a11y_tree) to
    /// fold into the pane's tree. (gloss-outline P1a.)
    fn gloss_outline_a11y_tree(
        &self,
        root_path: &str,
        action_routes: &mut HashMap<AccessNodeId, A11yHostAction>,
    ) -> (AccessNodeId, Vec<(AccessNodeId, Node)>) {
        let row_bounds = self.dom_member_bounds("gloss-outline-row");
        // The a11y tree should describe exactly what's visibly rendered (same cap), so
        // it reads the same live outline rect the last render's fold-in stored, rather
        // than an independent height — falling back generously (effectively uncapped)
        // before the first render has set one. (gloss-outline plan P2.)
        let available_height = self.gloss_outline_rect().map_or(10_000.0, |r| r[3] - r[1]);
        let snapshot = self.gloss_outline_snapshot(available_height);
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (i, row) in snapshot.rows.iter().enumerate() {
            let id = node_id_for_path(&format!("{root_path}/outline/row/{i}"));
            match &row.node {
                Some(n) => {
                    let mut node = Node::new(Role::ListItem);
                    node.add_action(Action::Click);
                    node.add_action(Action::Focus);
                    action_routes.insert(id, A11yHostAction::SelectNodeByUrl(n.url.clone()));
                    node.set_label(row.label.clone());
                    let desc = if n.selected {
                        format!("selected; {}", n.url)
                    } else {
                        n.url.clone()
                    };
                    node.set_description(desc);
                    if let Some(bounds) = row_bounds.get(&n.member) {
                        node.set_bounds(rect(*bounds));
                    }
                    nodes.push((id, node));
                }
                None => {
                    let mut node = Node::new(Role::Label);
                    node.set_label(row.label.clone());
                    nodes.push((id, node));
                }
            }
            children.push(id);
        }
        let outline_root = node_id_for_path(&format!("{root_path}/outline"));
        let mut outline_node = Node::new(Role::List);
        outline_node.set_label("Outline");
        outline_node.set_children(children);
        nodes.push((outline_root, outline_node));
        (outline_root, nodes)
    }

    fn comms_a11y_tree(&self, pane_id: PaneId) -> UxTree {
        let root_path = pane_content_root_path(&self.view.frame_layout, pane_id, "comms");
        let root = node_id_for_path(&root_path);
        let comms = &self.chrome().comms;
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
        draft.set_value(self.chrome().comms_draft.text().to_string());
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
