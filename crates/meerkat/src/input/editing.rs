/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Object-card, swatch-vertex, and row-reorder direct-manipulation drags.

use super::*;

impl WindowCtx<'_> {
    /// Reconcile the object card: drop it once the focus moves off its member (a new or
    /// cleared selection), and dispatch the activation keys its widget controls queued.
    /// (Object card — P1.)
    pub(crate) fn drain_object_card(&mut self) {
        let keys = self.take_object_card_keys();
        if self.view.object_card.is_some() && self.view.object_card != self.focused_member() {
            self.view.object_card = None;
            self.view.request_redraw();
        }
        if let Some(member) = self.view.object_card {
            if !keys.is_empty() {
                let mut face_changed = false;
                for key in keys {
                    match key.as_str() {
                        "size:up" => {
                            self.orrery_mut().step_node_size_tier(member, 1);
                        }
                        "size:down" => {
                            self.orrery_mut().step_node_size_tier(member, -1);
                        }
                        "face:favicon" => {
                            self.orrery_mut()
                                .set_node_face(member, mere::canvas::Face::Favicon);
                            face_changed = true;
                        }
                        "face:bare" => {
                            self.orrery_mut().set_node_face(member, mere::canvas::Face::Bare);
                            face_changed = true;
                        }
                        _ => {}
                    }
                }
                // The face override persists in the cartography sidecar; save it now. (Body & face.)
                if face_changed {
                    self.save_session();
                }
                self.view.request_redraw();
            }
        }
    }

    /// Whether `(x, y)` lands on the **object card** specifically (not a gnode), so the
    /// double-click-to-open-in-pelt gesture can skip it — its − / + are tier steps, and a
    /// double-tap on + must step twice, never launch the node in pelt. (Object card P0.)
    pub(crate) fn point_over_object_card(&self, x: f32, y: f32) -> bool {
        let Some(session) = self.view.chrome_session.as_ref() else {
            return false;
        };
        let dom = self.view.dom.borrow();
        let offsets = ScrollOffsets::<NodeId>::default();
        let Some(mut node) = session.hit_test(&dom, x, y, &offsets) else {
            return false;
        };
        loop {
            if crate::has_class(&dom, node, "object-card") {
                return true;
            }
            match dom.parent(node) {
                Some(parent) => node = parent,
                None => return false,
            }
        }
    }

    /// Open the Roster Link Card for a relation-cell dot in the connections focus card.
    pub(crate) fn try_open_connection_relation_card(&mut self, x: f32, y: f32) -> bool {
        let Some((from, to, selector)) = self.connection_relation_at(x, y) else {
            return false;
        };
        self.set_roster_tab(crate::roster::RosterTab::Links);
        self.set_roster_subject(Some(crate::roster::RosterSubject::RelationCell {
            from,
            to,
            selector,
        }));
        self.view.request_redraw();
        true
    }

    fn connection_relation_at(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(uuid::Uuid, uuid::Uuid, mere::kernel::graph::RelationSelector)> {
        let session = self.view.chrome_session.as_ref()?;
        let dom = self.view.dom.borrow();
        let offsets = ScrollOffsets::<NodeId>::default();
        let mut node = session.hit_test(&dom, x, y, &offsets)?;
        loop {
            if attr_value(&dom, node, "data-element").as_deref() == Some("relation-cell") {
                let from = attr_value(&dom, node, "data-from")?.parse().ok()?;
                let to = attr_value(&dom, node, "data-to")?.parse().ok()?;
                let tag = attr_value(&dom, node, "data-relation-tag")?.parse().ok()?;
                let kind = mere::kernel::graph::RelationKind::from_tag(tag)?;
                return Some((from, to, selector_for_relation_kind(kind)));
            }
            node = dom.parent(node)?;
        }
    }

    /// Try to begin a swatch edit from a left press at `(x, y)`: grab the nearest hull vertex if
    /// the press is on one, else **insert** a new vertex where it lands on a hull edge and grab
    /// that — so the swatch is a full body designer (drag to move, click an edge to add a
    /// corner). Returns `true` if a drag started (the caller consumes the press), else `false`
    /// (it falls through to the normal pane click). The swatch is DOM in the chrome document, so
    /// this reuses the object-card press-gate pattern (hit-test the chrome session, walk up to
    /// the `node-swatch` container). (Swatch — node shape editor, Stage B / B3.)
    pub(crate) fn try_begin_swatch_drag(&mut self, x: f32, y: f32) -> bool {
        let Some((subject, hull, origin, edge, nx, ny)) = self.swatch_geometry_at(x, y) else {
            return false;
        };
        let grab = 16.0 / edge;
        let (vi, vdist2) = hull
            .iter()
            .enumerate()
            .map(|(i, &(vx, vy))| (i, (vx - nx).powi(2) + (vy - ny).powi(2)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("hull has >= 3 vertices");
        // On a vertex: drag it.
        if vdist2 <= grab * grab {
            self.view.swatch_drag = Some(crate::window_view::SwatchDrag {
                subject,
                vertex: vi,
                origin,
                edge,
            });
            self.view.request_redraw();
            return true;
        }
        // Not on a vertex but inside the swatch near an edge: insert a corner there and drag it.
        if nx.abs() <= 0.5 && ny.abs() <= 0.5 {
            if let Some(after) = nearest_hull_edge(&hull, nx, ny, 14.0 / edge) {
                let insert_at = after + 1;
                let mut new_hull = hull;
                new_hull.insert(insert_at, (nx.clamp(-0.5, 0.5), ny.clamp(-0.5, 0.5)));
                self.orrery_mut().set_node_sprite_hull(subject, new_hull);
                self.view.swatch_drag = Some(crate::window_view::SwatchDrag {
                    subject,
                    vertex: insert_at,
                    origin,
                    edge,
                });
                self.view.request_redraw();
                return true;
            }
        }
        false
    }

    /// Remove the hull vertex under a right press at `(x, y)`, if the press lands on one and the
    /// hull stays a polygon (>= 3 vertices). Returns whether a vertex was removed. (Swatch — B3.)
    pub(crate) fn try_remove_swatch_vertex(&mut self, x: f32, y: f32) -> bool {
        // Never reshape the hull from a right-click while a left-drag owns it: removing a vertex
        // would shift the dragged vertex's index out from under the in-flight gesture. (Swatch.)
        if self.view.swatch_drag.is_some() {
            return false;
        }
        let Some((subject, hull, _origin, edge, nx, ny)) = self.swatch_geometry_at(x, y) else {
            return false;
        };
        if hull.len() <= 3 {
            return false;
        }
        let grab = 16.0 / edge;
        let (vi, vdist2) = hull
            .iter()
            .enumerate()
            .map(|(i, &(vx, vy))| (i, (vx - nx).powi(2) + (vy - ny).powi(2)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("hull has >= 3 vertices");
        if vdist2 > grab * grab {
            return false;
        }
        let mut new_hull = hull;
        new_hull.remove(vi);
        self.orrery_mut().set_node_sprite_hull(subject, new_hull);
        // Persist the hull edit (graph-truth geometry) immediately. (Node body & face.)
        self.save_session();
        self.view.request_redraw();
        true
    }

    /// Resolve a press at `(x, y)` to a node swatch and the press in its normalized face space:
    /// the subject node, its current hull (cloned), the container's painted top-left (window px),
    /// the edge length, and the normalized press point `(nx, ny)` in `[-0.5, 0.5]`. `None` when
    /// the press is not over an editable swatch. (Swatch — Stage B / B3.)
    /// The submenu-parent row index under window `(x, y)`, if the press lands on a context-menu
    /// row that expands a submenu (one carrying `data-submenu=<index>`). The press gate uses this
    /// to open a submenu rather than dismiss the menu; deterministic (a hit-test, not the
    /// dispatch/drain path). (Nested submenus.)
    pub(crate) fn submenu_parent_at(&self, x: f32, y: f32) -> Option<usize> {
        let session = self.view.chrome_session.as_ref()?;
        let dom = self.view.dom.borrow();
        // The menu panel is a direct `.chrome` child, not one of the folded scrolling panes, so no
        // extra scroll offsets are needed (matches `chrome_click`, whose offset set omits the menu).
        let offsets = ScrollOffsets::<NodeId>::default();
        let mut node = session.hit_test(&dom, x, y, &offsets)?;
        loop {
            if let Some(idx) = dom
                .attributes(node)
                .find(|a| a.name.local.as_ref() == "data-submenu")
                .and_then(|a| a.value.parse::<usize>().ok())
            {
                return Some(idx);
            }
            node = dom.parent(node)?;
        }
    }

    pub(crate) fn swatch_geometry_at(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(uuid::Uuid, Vec<(f32, f32)>, (f32, f32), f32, f32, f32)> {
        let session = self.view.chrome_session.as_ref()?;
        let dom = self.view.dom.borrow();
        // The engine folds the settings-pane-body's retained `element_scroll` into hit-testing,
        // so the swatch resolves under the scroll without a mirrored host offset map. (P2.)
        let offsets = ScrollOffsets::<NodeId>::default();
        let root = dom.document();
        // Walk up from the hit node to the `node-swatch` container.
        let mut node = session.hit_test(&dom, x, y, &offsets)?;
        let container = loop {
            if crate::has_class(&dom, node, "node-swatch") {
                break node;
            }
            node = dom.parent(node)?;
        };
        // Whose hull this swatch edits (the container's `data-subject`), and its current hull.
        let subject: uuid::Uuid = dom
            .attributes(container)
            .find(|a| a.name.local.as_ref() == "data-subject")
            .and_then(|a| a.value.parse().ok())?;
        let key = self
            .orrery()
            .graph()
            .get_node_by_id(subject)
            .map(|(k, _)| k)?;
        let hull = self.orrery().node_sprite_hull(key)?.to_vec();
        if hull.len() < 3 {
            return None;
        }
        // The container's painted top-left: its absolute layout origin (via the engine's shared
        // parent-chain accumulation) minus the body's scroll offset. (Host-scroll P1.)
        let frags = session.fragments();
        let abs = serval_layout::absolute_origin(&*dom, frags, container)
            .map_or((0.0_f32, 0.0_f32), |p| (p.x, p.y));
        let edge = crate::swatch::swatch_edge_px();
        // The container's painted top-left subtracts the settings-pane-body's retained scroll
        // (the engine's `element_scroll`, keyed by that container node — one active settings
        // pane, so a single lookup). (Host-scroll P2.)
        let body_scroll = crate::first_with_class(&dom, root, "settings-pane-body")
            .and_then(|n| session.element_scroll().get(&n).copied())
            .map_or(0.0, |(_, sy)| sy);
        let origin = (abs.0, abs.1 - body_scroll);
        let nx = (x - origin.0) / edge - 0.5;
        let ny = (y - origin.1) / edge - 0.5;
        Some((subject, hull, origin, edge, nx, ny))
    }

    /// Advance an in-progress swatch vertex drag: map the cursor into the swatch's normalized
    /// face space and rewrite the dragged hull vertex, which rebuilds the node's collider live
    /// (`set_node_sprite_hull` pushes geometry). (Swatch — Stage B.)
    pub(crate) fn drag_swatch_vertex(&mut self, x: f32, y: f32) {
        let Some(drag) = self.view.swatch_drag else {
            return;
        };
        let nx = ((x - drag.origin.0) / drag.edge - 0.5).clamp(-0.5, 0.5);
        let ny = ((y - drag.origin.1) / drag.edge - 0.5).clamp(-0.5, 0.5);
        // Resolve the drag's target, requiring the vertex index to still be in range. If the
        // node or hull vanished, or a hull edit shortened it under the gesture, cancel the drag
        // rather than leave it armed in a zombie state. (Swatch — drag self-heal.)
        let target = self
            .orrery()
            .graph()
            .get_node_by_id(drag.subject)
            .map(|(k, _)| k)
            .and_then(|key| {
                self.orrery()
                    .node_sprite_hull(key)
                    .map(<[(f32, f32)]>::to_vec)
            })
            .filter(|hull| drag.vertex < hull.len());
        let Some(mut hull) = target else {
            self.view.swatch_drag = None;
            return;
        };
        hull[drag.vertex] = (nx, ny);
        self.orrery_mut().set_node_sprite_hull(drag.subject, hull);
        self.view.request_redraw();
    }

    /// The shell document's vertical scroll offsets, keyed by each scrolling pane container, so a
    /// hit-test lands on the visible row of a scrolled pane. Mirrors the offsets
    /// [`chrome_click`](Self::chrome_click) builds for its dispatch. (Row reorder B2 helper.)
    /// Try to begin a row-reorder drag from a left press at `(x, y)`: if the press landed on a
    /// reorderable row's **drag grip** (`app-reorder-grip`), arm the drag with that row's
    /// `data-reorder-id` and return `true` (the caller consumes the press); otherwise `false`
    /// (it falls through to the normal pane click, so the label / ▲ / ▼ controls still work).
    /// Serval has no native DOM pointer-drag, so the host drives it from the cursor — the swatch
    /// editor's "handle press → drag → mutate" pattern, generalized to a list row. (Command
    /// registry B2 — drag reorder.)
    pub(crate) fn try_begin_row_reorder(&mut self, x: f32, y: f32) -> bool {
        let Some(id) = self.row_reorder_grip_at(x, y) else {
            return false;
        };
        self.view.row_reorder_drag = Some(crate::window_view::RowReorderDrag {
            id: id.clone(),
            origin: (x, y),
            moved: false,
            target: Some(id),
        });
        self.view.request_redraw();
        true
    }

    /// The `data-reorder-id` of the reorderable row whose **drag grip** is under `(x, y)`, if any.
    /// Walks up the chrome hit chain: a grip (`app-reorder-grip`) must be in the chain, and the
    /// row above it carries the id. `None` when the press is not on a grip. (Row reorder B2.)
    pub(crate) fn row_reorder_grip_at(&self, x: f32, y: f32) -> Option<String> {
        let session = self.view.chrome_session.as_ref()?;
        let dom = self.view.dom.borrow();
        // The engine folds the panes' `element_scroll` into hit-testing. (Host-scroll P2.)
        let offsets = ScrollOffsets::<NodeId>::default();
        let mut node = session.hit_test(&dom, x, y, &offsets)?;
        let mut on_grip = false;
        loop {
            if crate::has_class(&dom, node, "app-reorder-grip") {
                on_grip = true;
            }
            if on_grip {
                if let Some(id) = dom
                    .attributes(node)
                    .find(|a| a.name.local.as_ref() == "data-reorder-id")
                    .map(|a| a.value.to_string())
                {
                    return Some(id);
                }
            }
            node = dom.parent(node)?;
        }
    }

    /// The `data-reorder-id` of the reorderable row under `(x, y)`, if any — the drop target
    /// while a row-reorder drag is in flight. Walks up to the first element carrying the id.
    /// (Row reorder B2.)
    pub(crate) fn row_reorder_id_at(&self, x: f32, y: f32) -> Option<String> {
        let session = self.view.chrome_session.as_ref()?;
        let dom = self.view.dom.borrow();
        // The engine folds the panes' `element_scroll` into hit-testing. (Host-scroll P2.)
        let offsets = ScrollOffsets::<NodeId>::default();
        let mut node = session.hit_test(&dom, x, y, &offsets)?;
        loop {
            if let Some(id) = dom
                .attributes(node)
                .find(|a| a.name.local.as_ref() == "data-reorder-id")
                .map(|a| a.value.to_string())
            {
                return Some(id);
            }
            node = dom.parent(node)?;
        }
    }

    /// Advance an in-progress row-reorder drag: update the drop target to the row under the
    /// cursor and arm the movement flag once the pointer leaves the press point. The view dims
    /// the dragged row and marks the drop target on the next frame. (Command registry B2.)
    pub(crate) fn drag_row_reorder(&mut self, x: f32, y: f32) {
        let target = self.row_reorder_id_at(x, y);
        let Some(drag) = self.view.row_reorder_drag.as_mut() else {
            return;
        };
        if !drag.moved && (x - drag.origin.0).hypot(y - drag.origin.1) > 4.0 {
            drag.moved = true;
        }
        drag.target = target;
        self.view.request_redraw();
    }
}

fn attr_value(dom: &serval_scripted_dom::ScriptedDom, node: NodeId, name: &str) -> Option<String> {
    dom.attributes(node)
        .find(|a| a.name.local.as_ref() == name)
        .map(|a| a.value.to_string())
}

fn selector_for_relation_kind(
    kind: mere::kernel::graph::RelationKind,
) -> mere::kernel::graph::RelationSelector {
    match kind {
        mere::kernel::graph::RelationKind::Semantic(sub) => {
            mere::kernel::graph::RelationSelector::Semantic(sub)
        }
        mere::kernel::graph::RelationKind::Traversal => {
            mere::kernel::graph::RelationSelector::Family(mere::kernel::graph::EdgeFamily::Traversal)
        }
        mere::kernel::graph::RelationKind::Containment(sub) => {
            mere::kernel::graph::RelationSelector::Containment(sub)
        }
        mere::kernel::graph::RelationKind::Arrangement(sub) => {
            mere::kernel::graph::RelationSelector::Arrangement(sub)
        }
        mere::kernel::graph::RelationKind::Imported(sub) => {
            mere::kernel::graph::RelationSelector::Imported(sub)
        }
        mere::kernel::graph::RelationKind::Provenance(sub) => {
            mere::kernel::graph::RelationSelector::Provenance(sub)
        }
    }
}
