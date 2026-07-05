/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! WindowCtx session ops: save/restore, rename, thumbnails.

use kernel::geometry::PortablePoint;

use super::*;

impl WindowCtx<'_> {
    /// The session a `graph_id` belongs to: its [`SessionId`] and storage dir,
    /// resolved by the graph (manifests are keyed by session but each carries its
    /// `root_graph_id`). This is the pane-as-unit resolution — a pane pinned to a
    /// graph saves / renames / navigates through *this* session, not a global
    /// active one. Falls back to the conventional `sessions/<id>` dir when a
    /// manifest has no explicit `storage_path`. (Window composition — pane-as-unit.)
    pub(crate) fn session_for_graph(
        &self,
        graph_id: GraphId,
    ) -> Option<(SessionId, std::path::PathBuf)> {
        self.shared.session.manifests.iter().find_map(|(id, m)| {
            (m.root_graph_id == graph_id).then(|| {
                let dir = m.storage_path.clone().unwrap_or_else(|| {
                    self.shared
                        .session
                        .mere_root
                        .join("sessions")
                        .join(id.as_uuid().to_string())
                });
                (id, dir)
            })
        })
    }

    /// Move input focus to the graph-pane resolving to `graph_id`: point the
    /// window's `focused_graph` at it and re-key the active session (id + dir) to its
    /// session, so the context menu, selection, nav, the omnibar, and save all act on
    /// *this* pane. Focus is just a pointer — this does **not** reload the graph or
    /// clear the window's content caches (both graphs coexist), unlike a session
    /// switch. No-op if already focused or the graph has no session. (Window
    /// composition — pane-as-unit; focus-follows-click.)
    pub(crate) fn focus_pane_graph(&mut self, graph_id: GraphId) {
        if self.view.focused_graph == graph_id {
            return;
        }
        let Some((session_id, session_dir)) = self.session_for_graph(graph_id) else {
            return;
        };
        self.view.focused_graph = graph_id;
        self.shared.session.active_session_id = session_id;
        self.shared.session.session_dir = session_dir;
        self.view.request_redraw();
    }

    /// Persist the **focused pane's** session: its graph + camera view-intent under
    /// that session's dir, resolved from `focused_graph` (not a global active
    /// session), so two live graphs each persist to their own storage. The frame
    /// layout is window-scoped and stays at the shared root. Best-effort: a write
    /// failure is logged, not fatal. Called after each navigation and on close.
    /// (Window composition — pane-as-unit; per-pane save.)
    pub(crate) fn save_session(&mut self) {
        let Some((session_id, session_dir)) = self.session_for_graph(self.view.focused_graph)
        else {
            return; // the focused graph has no session manifest (should not happen)
        };
        let graph_file = session_dir.join(session_graph_store::GRAPH_FILE);
        if let Err(err) = session_graph_store::save(&graph_file, self.orrery().graph()) {
            tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
        }
        let intent = ViewIntent {
            hidden_relations: super::hidden_relation_records(self.orrery()),
            camera: Some(crate::camera_to_snapshot(
                self.orrery().camera(),
                self.orrery().yaw(),
                self.orrery().tilt(),
            )),
            focus: self.orrery().focused_url().map(str::to_string),
            strategy: self.orrery().layout_strategy().map(str::to_string),
            mirror_tiles: self.view.mirror_tiles,
            ..Default::default()
        };
        if let Err(err) =
            view_intent_store::save_view_intent(&session_dir, DEFAULT_FRAME, DEFAULT_PANE, &intent)
        {
            tracing::warn!(%err, dir = ?session_dir, "failed to persist the view intent");
        }
        // The content frame's pane layout (which panes are open + split ratios) is
        // **window-scoped** (Model B, MG5): it persists at the shared root and stays
        // put across session switches, so a graph swap re-sources the panes without
        // rearranging them.
        if let Err(err) = frame_layout_store::save_frame_layout(
            &self.shared.session.mere_root,
            &self.view.frame_layout,
        ) {
            tracing::warn!(%err, dir = ?self.shared.session.mere_root, "failed to persist the frame layout");
        }
        // The workbench tiling is the focused graph's, so it persists per-session
        // beside graph.json. Saved as the bridge's (arrangement, geometry) pair (the
        // Pane tree is the derived live model, not serde). Best-effort. (A3.)
        match self.view.workbench.to_persisted_json() {
            Ok(json) => {
                let path = session_dir.join(WORKBENCH_FILE);
                if let Err(err) = std::fs::write(&path, json) {
                    tracing::warn!(%err, path = ?path, "failed to persist the workbench tiling");
                }
            }
            Err(err) => tracing::warn!(%err, "failed to serialize the workbench tiling"),
        }
        // The orrery's settled layout: persist the live node positions as the
        // cartography sidecar (the live force-directed layout is never committed to
        // the kernel graph, so this is what survives a restart). Best-effort. (Position sidecar.)
        match self.orrery().cartography_geometry().to_persisted_json() {
            Ok(json) => {
                let path = session_dir.join(CARTOGRAPHY_FILE);
                if let Err(err) = std::fs::write(&path, json) {
                    tracing::warn!(%err, path = ?path, "failed to persist the cartography positions");
                }
            }
            Err(err) => tracing::warn!(%err, "failed to serialize the cartography positions"),
        }
        // Auto-reconcile this graph's Linked graphlets after the save, so their persisted
        // roster tracks graph drift (the scoped window already tracks it live via re-derive;
        // this keeps the on-disk set + other readers current). Deferred to Shell (needs
        // `&mut graphlets`); cheap + idempotent. (Graphlet wiring Phase 3 slice 2+.)
        let focused = self.view.focused_graph;
        if self
            .graphlets
            .get(&focused)
            .is_some_and(|idx| idx.has_linked())
        {
            self.commands
                .push(crate::ShellCommand::ReconcileGraphlets { graph: focused });
        }
        // Record the save in this session's manifest (advances `updated_at`, the
        // switcher's recency key) and flush the registry. (Multi-graph MG1.)
        self.shared.session.manifests.update(session_id, |_| {});
        if let Err(err) = self.shared.session.manifests.flush_dirty() {
            tracing::warn!(%err, "failed to flush the session registry");
        }
    }

    /// Restore the focused graph's persisted workbench tiling from its sidecar,
    /// pruned to the live graph's members. Thin wrapper over [`load_workbench`] for
    /// the session-switch path (which has a live `ctx`). (A3 persistence.)
    pub(crate) fn restore_workbench(&self, session_dir: &std::path::Path) -> platen::Workbench {
        let present = self
            .orrery()
            .graph()
            .nodes()
            .map(|(_, node)| node.id)
            .collect();
        load_workbench(session_dir, &present)
    }

    /// Begin renaming `id`: seed the switcher edit buffer from its current label, so
    /// editing starts from the shown name (display or derived). (Host text path.)
    pub(crate) fn start_rename(&mut self, id: SessionId) {
        if self.shared.session.manifests.get(id).is_none() {
            return;
        }
        let seed = self
            .shared
            .session
            .session_labels
            .get(&id)
            .cloned()
            .unwrap_or_default();
        self.view.renaming = Some((id, seed));
        self.view.request_redraw();
    }

    /// Commit the in-progress rename: a non-empty name sets the session's display
    /// name; an empty one clears it (the label reverts to the derived one). Persists
    /// the manifest and refreshes the labels. (Host text path.)
    pub(crate) fn commit_rename(&mut self) {
        let Some((id, name)) = self.view.renaming.take() else {
            return;
        };
        let trimmed = name.trim().to_string();
        let display = (!trimmed.is_empty()).then_some(trimmed);
        self.shared
            .session
            .manifests
            .update(id, |m| m.display_name = display);
        if let Err(err) = self.shared.session.manifests.flush_dirty() {
            tracing::warn!(%err, "failed to flush the renamed session manifest");
        }
        self.refresh_session_labels();
        self.view.request_redraw();
    }

    /// Drop the in-progress rename without saving (Escape, or an interaction that
    /// moves on). A no-op when not renaming. (Host text path.)
    pub(crate) fn cancel_rename(&mut self) {
        if self.view.renaming.take().is_some() {
            self.view.request_redraw();
        }
    }

    /// Append typed `ch` to the rename buffer. No-op when not renaming. (Host text.)
    pub(crate) fn rename_push(&mut self, ch: &str) {
        if let Some((_, buf)) = self.view.renaming.as_mut() {
            buf.push_str(ch);
            self.view.request_redraw();
        }
    }

    /// Delete the last char of the rename buffer (Backspace). (Host text path.)
    pub(crate) fn rename_backspace(&mut self) {
        if let Some((_, buf)) = self.view.renaming.as_mut() {
            buf.pop();
            self.view.request_redraw();
        }
    }

    /// Rebuild the per-session display **labels and chip thumbnails**: the
    /// active/pooled session from its live orrery graph, each cold session from its
    /// `graph.json` (positions from the cartography sidecar). A user-set display name
    /// wins the label; otherwise one derived from the graph. Drops entries for closed
    /// sessions. Called on every session or graph change — event-driven, so the chip
    /// `<img>` is never repainted per frame. (Chrome bar P4 labels; ui_polish S1
    /// revived the thumbnails into the chips.)
    pub(crate) fn refresh_session_labels(&mut self) {
        let ids: Vec<SessionId> = self
            .shared
            .session
            .manifests
            .iter()
            .map(|(id, _)| id)
            .collect();
        let live: std::collections::HashSet<SessionId> = ids.iter().copied().collect();
        self.shared
            .session
            .session_labels
            .retain(|id, _| live.contains(id));
        self.shared
            .session
            .session_thumbs
            .retain(|id, _| live.contains(id));
        let theme = self.shared.presentation.chrome_theme;
        let (thumb_bg, thumb_edge, thumb_node) = (
            theme.control_bg.to_array(),
            theme.muted_text.to_array(),
            theme.strong_text.to_array(),
        );
        let opts = session_runtime::SwitcherThumbnailOptions {
            width: crate::session_thumbs::THUMB_W,
            height: crate::session_thumbs::THUMB_H,
            node_radius: 2.5,
            ..Default::default()
        };
        for id in ids {
            // A user-set display name wins; otherwise derive a short label from the
            // graph — live orrery if pooled, else a cold `graph.json` load.
            let display_name = self
                .shared
                .session
                .manifests
                .get(id)
                .and_then(|m| m.display_name.clone())
                .filter(|n| !n.trim().is_empty());
            let pooled = self
                .shared
                .session
                .manifests
                .get(id)
                .map(|m| m.root_graph_id)
                .and_then(|gid| self.orreries.get(&gid));
            let (label, thumb) = if let Some(orrery) = pooled {
                let label = display_name
                    .unwrap_or_else(|| derive_session_label(orrery.graph()));
                let thumb = session_runtime::build_switcher_thumbnail_with(
                    orrery.graph(),
                    |k| orrery.node_position(k),
                    opts,
                );
                (label, thumb)
            } else {
                let dir = self
                    .shared
                    .session
                    .mere_root
                    .join("sessions")
                    .join(id.as_uuid().to_string());
                let graph = session_graph_store::load(&dir.join(session_graph_store::GRAPH_FILE))
                    .ok()
                    .flatten()
                    .unwrap_or_else(Graph::new);
                let label = display_name.unwrap_or_else(|| derive_session_label(&graph));
                // Positions are not graph truth; a cold session's thumbnail reads
                // them from its cartography sidecar (origin when absent).
                let present: std::collections::HashSet<forme::GraphMemberId> =
                    graph.nodes().map(|(_, n)| n.id).collect();
                let positions: std::collections::HashMap<forme::GraphMemberId, PortablePoint> =
                    super::load_cartography(&dir, &present)
                        .map(|g| {
                            g.iter()
                                .map(|(m, (x, y))| (m, PortablePoint::new(x, y)))
                                .collect()
                        })
                        .unwrap_or_default();
                let thumb = session_runtime::build_switcher_thumbnail_with(
                    &graph,
                    |k| {
                        graph
                            .nodes()
                            .find(|(key, _)| *key == k)
                            .and_then(|(_, n)| positions.get(&n.id).copied())
                    },
                    opts,
                );
                (label, thumb)
            };
            if let Some(uri) =
                crate::session_thumbs::thumb_data_uri(&thumb, thumb_bg, thumb_edge, thumb_node)
            {
                self.shared.session.session_thumbs.insert(id, uri);
            }
            self.shared.session.session_labels.insert(id, label);
        }
    }
}
