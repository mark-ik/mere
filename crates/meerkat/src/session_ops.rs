/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Session lifecycle for the shell: persisting a session (graph + camera +
//! frame), renaming sessions in the switcher, rebuilding switcher thumbnails and
//! labels, and the `Shell`-level create / switch / cycle / close ops that re-key
//! the focused orrery in the pool (the step a per-window `WindowCtx` cannot do).
//! The per-window pieces hang off `WindowCtx`; the pool re-keying off `Shell`.
//! Factored out of `frame_ops.rs` to keep files under the 600-LOC ceiling.

use frame::{GraphId, SessionId};
use kernel::graph::Graph;
use session_runtime::{
    SwitcherThumbnailOptions, ViewIntent, build_switcher_thumbnail, frame_layout_store,
    manifest::GraphSessionManifest, session_graph_store, view_intent_store,
};

use super::switcher::{SWITCHER_THUMB_H, SWITCHER_THUMB_W};
use super::{DEFAULT_FRAME, DEFAULT_PANE, WindowCtx};

impl WindowCtx<'_> {
    /// The session a `graph_id` belongs to: its [`SessionId`] and storage dir,
    /// resolved by the graph (manifests are keyed by session but each carries its
    /// `root_graph_id`). This is the pane-as-unit resolution — a pane pinned to a
    /// graph saves / renames / navigates through *this* session, not a global
    /// active one. Falls back to the conventional `sessions/<id>` dir when a
    /// manifest has no explicit `storage_path`. (Window composition — pane-as-unit.)
    pub(super) fn session_for_graph(
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
    pub(super) fn focus_pane_graph(&mut self, graph_id: GraphId) {
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
    pub(super) fn save_session(&mut self) {
        let Some((session_id, session_dir)) = self.session_for_graph(self.view.focused_graph)
        else {
            return; // the focused graph has no session manifest (should not happen)
        };
        let graph_file = session_dir.join(session_graph_store::GRAPH_FILE);
        if let Err(err) = session_graph_store::save(&graph_file, self.orrery().graph()) {
            tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
        }
        let intent = ViewIntent {
            camera: Some(super::camera_to_snapshot(self.orrery().camera())),
            focus: self.orrery().focused_url().map(str::to_string),
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
        if let Err(err) = frame_layout_store::save_frame_layout(&self.shared.session.mere_root, &self.view.frame_layout)
        {
            tracing::warn!(%err, dir = ?self.shared.session.mere_root, "failed to persist the frame layout");
        }
        // Record the save in this session's manifest (advances `updated_at`, the
        // switcher's recency key) and flush the registry. (Multi-graph MG1.)
        self.shared.session.manifests.update(session_id, |_| {});
        if let Err(err) = self.shared.session.manifests.flush_dirty() {
            tracing::warn!(%err, "failed to flush the session registry");
        }
        // Keep this session's switcher thumbnail live as its graph grows (cheap; no
        // disk read, unlike the full refresh on a session change).
        let opts = SwitcherThumbnailOptions {
            width: SWITCHER_THUMB_W,
            height: SWITCHER_THUMB_H,
            ..SwitcherThumbnailOptions::default()
        };
        let thumb = build_switcher_thumbnail(self.orrery().graph(), opts);
        self.shared.session.session_thumbnails.insert(session_id, thumb);
    }

    /// Begin renaming `id`: seed the switcher edit buffer from its current label, so
    /// editing starts from the shown name (display or derived). (Host text path.)
    pub(super) fn start_rename(&mut self, id: SessionId) {
        if self.shared.session.manifests.get(id).is_none() {
            return;
        }
        let seed = self.shared.session.session_labels.get(&id).cloned().unwrap_or_default();
        self.view.renaming = Some((id, seed));
        self.view.request_redraw();
    }

    /// Commit the in-progress rename: a non-empty name sets the session's display
    /// name; an empty one clears it (the label reverts to the derived one). Persists
    /// the manifest and refreshes the labels. (Host text path.)
    pub(super) fn commit_rename(&mut self) {
        let Some((id, name)) = self.view.renaming.take() else {
            return;
        };
        let trimmed = name.trim().to_string();
        let display = (!trimmed.is_empty()).then_some(trimmed);
        self.shared.session.manifests.update(id, |m| m.display_name = display);
        if let Err(err) = self.shared.session.manifests.flush_dirty() {
            tracing::warn!(%err, "failed to flush the renamed session manifest");
        }
        self.refresh_session_thumbnails();
        self.view.request_redraw();
    }

    /// Drop the in-progress rename without saving (Escape, or an interaction that
    /// moves on). A no-op when not renaming. (Host text path.)
    pub(super) fn cancel_rename(&mut self) {
        if self.view.renaming.take().is_some() {
            self.view.request_redraw();
        }
    }

    /// Append typed `ch` to the rename buffer. No-op when not renaming. (Host text.)
    pub(super) fn rename_push(&mut self, ch: &str) {
        if let Some((_, buf)) = self.view.renaming.as_mut() {
            buf.push_str(ch);
            self.view.request_redraw();
        }
    }

    /// Delete the last char of the rename buffer (Backspace). (Host text path.)
    pub(super) fn rename_backspace(&mut self) {
        if let Some((_, buf)) = self.view.renaming.as_mut() {
            buf.pop();
            self.view.request_redraw();
        }
    }

    /// Rebuild the per-session switcher thumbnails **and labels**: the active
    /// session from the **live** orrery graph, each inactive session from its cold
    /// `graph.json`. The label is the user's display name, else one derived from the
    /// graph. Drops entries for closed sessions. Called on every session or graph
    /// change (cheap small-graph walks; no per-frame disk reads). (Multi-graph MG4 /
    /// host text path.)
    pub(super) fn refresh_session_thumbnails(&mut self) {
        let opts = SwitcherThumbnailOptions {
            width: SWITCHER_THUMB_W,
            height: SWITCHER_THUMB_H,
            ..SwitcherThumbnailOptions::default()
        };
        let ids: Vec<SessionId> = self.shared.session.manifests.iter().map(|(id, _)| id).collect();
        let live: std::collections::HashSet<SessionId> = ids.iter().copied().collect();
        self.shared.session.session_thumbnails.retain(|id, _| live.contains(id));
        self.shared.session.session_labels.retain(|id, _| live.contains(id));
        for id in ids {
            // A user-set display name wins; otherwise derive a short label.
            let display_name = self.shared.session.manifests
                .get(id)
                .and_then(|m| m.display_name.clone())
                .filter(|n| !n.trim().is_empty());
            // A session whose graph is *pooled* (live in any pane, not just the
            // focused one) thumbnails off its live orrery; the rest cold-load from
            // disk. (Pane-as-unit — was "the active session only".)
            let pooled = self
                .shared
                .session
                .manifests
                .get(id)
                .map(|m| m.root_graph_id)
                .and_then(|gid| self.orreries.get(&gid));
            let (thumb, label) = if let Some(orrery) = pooled {
                let g = orrery.graph();
                let label = display_name.unwrap_or_else(|| derive_session_label(g));
                (build_switcher_thumbnail(g, opts), label)
            } else {
                let dir = self.shared.session.mere_root
                    .join("sessions")
                    .join(id.as_uuid().to_string());
                let graph = session_graph_store::load(&dir.join(session_graph_store::GRAPH_FILE))
                    .ok()
                    .flatten()
                    .unwrap_or_else(Graph::new);
                let label = display_name.unwrap_or_else(|| derive_session_label(&graph));
                (build_switcher_thumbnail(&graph, opts), label)
            };
            self.shared.session.session_thumbnails.insert(id, thumb);
            self.shared.session.session_labels.insert(id, label);
        }
    }
}

// Session ops live on `Shell`, not `WindowCtx`: switching a session re-keys the
// orrery pool, and a `WindowCtx` holds exactly one orrery borrowed *out* of the
// pool, so it cannot insert or re-key entries. Per-window input handlers request
// these by pushing a [`ShellCommand`]; `Shell::apply` runs them after the ctx
// borrow ends (the same seam as spawn/close). WindowCtx-shaped sub-steps
// (save_session, the cache reset, thumbnails) re-enter through `self.ctx()`, which
// resolves the focused view primary-or-pending and bundles its pooled orrery.
// (Window composition P1, multi-graph.)
impl super::Shell {
    /// Mint a fresh, empty session and make it active (persisting the current one
    /// first). The new session opens on the welcome node. Returns its id. (MG3.)
    pub(super) fn create_session(&mut self) -> SessionId {
        self.ctx().save_session();
        let session_id = SessionId::new();
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        let mut manifest = GraphSessionManifest::new(session_id, GraphId::new());
        manifest.storage_path = Some(session_dir.clone());
        self.shared.session.manifests.insert(manifest);
        let _ = self.shared.session.manifests.flush_dirty();
        self.load_active_session(session_id, session_dir, true);
        session_id
    }

    /// Switch the active session to `target`: persist the current one, then load the
    /// target's graph + camera + frame. No-op if already active or unknown. (MG2.)
    pub(super) fn switch_session(&mut self, target: SessionId) {
        if target == self.shared.session.active_session_id
            || self.shared.session.manifests.get(target).is_none()
        {
            return;
        }
        self.ctx().save_session();
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(target.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        self.load_active_session(target, session_dir, false);
    }

    /// Switch to the next (`forward`) or previous session in id order, wrapping.
    /// No-op with fewer than two sessions. (MG2.)
    pub(super) fn cycle_session(&mut self, forward: bool) {
        let mut ids: Vec<SessionId> =
            self.shared.session.manifests.iter().map(|(id, _)| id).collect();
        if ids.len() < 2 {
            return;
        }
        ids.sort_by_key(|id| *id.as_uuid());
        let Some(pos) = ids.iter().position(|id| *id == self.shared.session.active_session_id)
        else {
            return;
        };
        let next = if forward {
            (pos + 1) % ids.len()
        } else {
            (pos + ids.len() - 1) % ids.len()
        };
        self.switch_session(ids[next]);
    }

    /// Close (trash) the `target` session. Refuses to close the last one. If it was
    /// active, switches to the most-recently-updated survivor first. (MG3.)
    pub(super) fn close_session(&mut self, target: SessionId) {
        if self.shared.session.manifests.len() <= 1
            || self.shared.session.manifests.get(target).is_none()
        {
            return;
        }
        if target == self.shared.session.active_session_id {
            let survivor = self
                .shared
                .session
                .manifests
                .iter()
                .filter(|(id, _)| *id != target)
                .max_by_key(|(_, m)| m.updated_at)
                .map(|(id, _)| id);
            if let Some(next) = survivor {
                self.switch_session(next);
            }
        }
        if let Err(err) = self.shared.session.manifests.move_to_trash(target) {
            tracing::warn!(%err, "failed to trash the closed session");
        }
        self.focused_view_mut().renaming = None;
        self.ctx().refresh_session_thumbnails();
        self.focused_view_mut().request_redraw();
    }

    /// Make `id` (whose dir is `session_dir`) the active session: load its graph +
    /// camera + frame and reset the prior session's runtime caches. `fresh` marks a
    /// just-minted empty session. Re-keys the focused orrery to the target graph in
    /// the pool (the Shell-only step a WindowCtx cannot do); the rest runs on the
    /// re-entered ctx, which now resolves that re-keyed orrery. (MG2.)
    fn load_active_session(&mut self, id: SessionId, session_dir: std::path::PathBuf, fresh: bool) {
        // The target graph (empty for a fresh session, or a missing/corrupt file).
        let graph = if fresh {
            Graph::new()
        } else {
            session_graph_store::load(&session_dir.join(session_graph_store::GRAPH_FILE))
                .ok()
                .flatten()
                .unwrap_or_else(Graph::new)
        };
        let empty = graph.nodes().count() == 0;
        let target_graph = self
            .shared
            .session
            .manifests
            .get(id)
            .map(|m| m.root_graph_id)
            .unwrap_or_default();
        // Pool the target graph's orrery (Shell-level — a WindowCtx cannot insert
        // pool entries), keeping the *outgoing* graph's orrery live so two graphs
        // coexist. A graph shown for the first time mints a fresh orrery from its
        // loaded graph and offloads its own physics actor (like the seed did at
        // boot); switching back to a still-pooled graph reuses its live orrery —
        // its state was kept warm, so disk is not reloaded over it. The outgoing
        // graph is parked (its content actors reaped per-graph below); its orrery
        // stays in the pool.
        let old_gid = self.focused_view().focused_graph;
        self.pool_orrery(target_graph, graph, empty);
        self.focused_view_mut().focused_graph = target_graph;

        // Park the outgoing graph's physics (OQ2 park): a switched-away graph stays
        // warm in the pool, but stop it settling so it does not keep ticking and
        // waking the loop in the background — its actor then idles on its channel.
        // Switching back shows its settled positions; interaction resumes the settle.
        if old_gid != target_graph {
            if let Some(parked) = self.orreries.get_mut(&old_gid) {
                parked.park_physics();
            }
        }
        self.touch_and_evict(target_graph);

        // The remainder runs on the focused window's ctx (now resolving the re-keyed
        // orrery): restore camera + focus, retag the graph-bound leaves, reset the
        // prior session's runtime caches, and swap identity.
        let restored_view =
            view_intent_store::load_view_intent(&session_dir, DEFAULT_FRAME, DEFAULT_PANE)
                .ok()
                .flatten();
        let mut ctx = self.ctx();
        match restored_view.as_ref().and_then(|v| v.camera.as_ref()) {
            Some(snapshot) => {
                ctx.orrery_mut().set_camera(super::snapshot_to_camera(snapshot));
                ctx.view.centered = true;
            }
            None => ctx.view.centered = false,
        }
        if let Some(url) = restored_view.as_ref().and_then(|v| v.focus.as_deref()) {
            ctx.orrery_mut().select_by_url(url);
        }
        // Re-point only the panes that were on the outgoing graph, so a second
        // graph-pane pinned to a different graph survives the switch. (Pane-as-unit.)
        ctx.view.frame_layout.retag_graph_bound_from(old_gid, target_graph);
        // Switching toward a graph already shown beside would leave two panes on it;
        // collapse the duplicate so it doesn't render blank. (Pane-as-unit.)
        ctx.view.frame_layout.dedupe_graph_panes();
        ctx.shared.content.constellation.reap_graph(old_gid);
        ctx.view.scrying.clear();
        ctx.shared.content.engine_pins.clear();
        ctx.view.scrying_input_focus = None;
        ctx.view.scrying_rects.clear();
        ctx.shared.content.pages.clear();
        ctx.view.live_previews.clear();
        ctx.view.tile_textures.clear();
        ctx.view.snapshot_textures.clear();
        ctx.view.scroll.clear();
        ctx.view.focused_tile = None;
        ctx.view.shown_location = None;
        ctx.view.renaming = None;
        ctx.view.workbench = platen::Workbench::new();
        ctx.view.maximized_pane = None;
        ctx.view.active_content = super::ContentPane::Orrery;
        ctx.shared.session.session_dir = session_dir;
        ctx.shared.session.active_session_id = id;
        ctx.view.content_location =
            ctx.orrery().focused_url().unwrap_or("mere://welcome").to_string();
        ctx.refresh_session_thumbnails();
        ctx.view.request_redraw();
    }

    /// Pool `graph`'s orrery under `graph_id` if it isn't already pooled, minting
    /// it from the loaded graph with its own offloaded physics actor (an empty one
    /// opens on the welcome node, like first launch). A no-op when the graph is
    /// already live — its pooled orrery is authoritative, so disk is not reloaded
    /// over it. Touches no focus: both session-switch and open-graph-beside pool
    /// through here. (Window composition P2.)
    fn pool_orrery(&mut self, graph_id: GraphId, graph: Graph, empty: bool) {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.orreries.entry(graph_id) {
            let mut orrery = orrery::Orrery::with_graph(graph);
            if empty {
                orrery.visit("mere://welcome");
            }
            orrery.offload_physics(self.physics_wake.clone());
            slot.insert(orrery);
        }
    }

    /// Mark `graph_id` most-recently-used, then drop the stalest pooled orreries
    /// over the cap that **no pane in any window resolves to** — not just each
    /// window's focused graph, so a second graph-pane's orrery is not evicted out
    /// from under it. Dropping an orrery ends its physics actor and reaps its
    /// content actors; the graph was saved on its last switch-away, so eviction
    /// loses no data (switching / re-opening reloads it). (Window composition P2,
    /// OQ2 unload — now pane-aware.)
    fn touch_and_evict(&mut self, graph_id: GraphId) {
        self.orrery_lru.retain(|g| *g != graph_id);
        self.orrery_lru.push(graph_id);
        if self.orreries.len() <= super::MAX_POOLED_ORRERIES {
            return;
        }
        let live: std::collections::HashSet<GraphId> = self
            .windows
            .values()
            .chain(self.pending_view.iter())
            .flat_map(|v| {
                std::iter::once(v.focused_graph)
                    .chain(v.frame_layout.iter_leaves().map(|(_, _, gid)| gid))
            })
            .collect();
        while self.orreries.len() > super::MAX_POOLED_ORRERIES {
            let Some(stale) = self.orrery_lru.iter().copied().find(|g| !live.contains(g)) else {
                break;
            };
            self.orrery_lru.retain(|g| *g != stale);
            self.orreries.remove(&stale); // drop → physics actor thread ends
            self.shared.content.constellation.reap_graph(stale);
        }
    }

    /// Open session `id`'s graph in a second Orrery pane beside the current one
    /// **without switching focus**: pool its orrery (cold-loading from disk if it
    /// isn't live) and summon an Orrery leaf bound to its `graph_id`, split beside
    /// the primary graph pane. The two graphs then render side by side, each
    /// driving its own pooled orrery (the P2 per-pane render path). No-op if the
    /// session is unknown or its graph already shows in this window. (Window
    /// composition P2 — second graph-pane.)
    pub(super) fn open_graph_beside(&mut self, id: SessionId) {
        let Some(graph_id) = self.shared.session.manifests.get(id).map(|m| m.root_graph_id) else {
            return;
        };
        // Already shown beside in the focused window? Don't double-summon.
        if self
            .focused_view()
            .frame_layout
            .iter_leaves()
            .any(|(_, c, gid)| matches!(c, frame::PaneContent::Orrery) && gid == graph_id)
        {
            return;
        }
        let session_dir = self
            .shared
            .session
            .mere_root
            .join("sessions")
            .join(id.as_uuid().to_string());
        let graph = session_graph_store::load(&session_dir.join(session_graph_store::GRAPH_FILE))
            .ok()
            .flatten()
            .unwrap_or_else(Graph::new);
        let empty = graph.nodes().count() == 0;
        self.pool_orrery(graph_id, graph, empty);
        self.touch_and_evict(graph_id);
        // Summon a second Orrery leaf bound to this graph, split right of the
        // primary graph pane (an even split — two graphs share the band).
        let view = self.focused_view_mut();
        let pane_id = frame::PaneId(view.next_pane_id);
        view.next_pane_id += 1;
        let leaf = frame::PaneNode::Leaf {
            pane_id,
            content: frame::PaneContent::Orrery,
            graph_id,
        };
        let anchor =
            super::frame_view::pane_path(&view.frame_layout, super::GRAPH_PANE).unwrap_or_default();
        if view
            .frame_layout
            .summon_leaf(&anchor, frame::InsertSide::Right, leaf)
        {
            view.frame_layout.set_split_ratio(&anchor, 0.5);
        }
        view.maximized_pane = None;
        view.request_redraw();
        self.shared
            .observability
            .record_frame_layout_changed("second graph pane opened");
    }
}

/// A short switcher label for a session with no user-set display name: the first
/// non-intro node's cached host (else its title), or "New" for an empty /
/// welcome-only graph. (Host text path.)
fn derive_session_label(graph: &Graph) -> String {
    graph
        .nodes()
        .map(|(_, node)| node)
        .find(|node| node.url() != "mere://welcome")
        .and_then(|node| {
            node.cached_host
                .clone()
                .filter(|h| !h.trim().is_empty())
                .or_else(|| Some(node.title.clone()))
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "New".to_string())
}
