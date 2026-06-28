/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shell session load: activate, orrery pool/evict, open beside.

use super::*;

impl crate::Shell {
    /// Make `id` (whose dir is `session_dir`) the active session: load its graph +
    /// camera + frame and reset the prior session's runtime caches. `fresh` marks a
    /// just-minted empty session. Re-keys the focused orrery to the target graph in
    /// the pool (the Shell-only step a WindowCtx cannot do); the rest runs on the
    /// re-entered ctx, which now resolves that re-keyed orrery. (MG2.)
    pub(crate) fn load_active_session(&mut self, id: SessionId, session_dir: std::path::PathBuf, fresh: bool) {
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
        // Pool this session's graphlet index alongside its orrery (load-or-default from
        // the sidecar; a fresh session gets the default whole-session graphlet). Kept
        // warm like the orrery — a switch-back reuses the live index. Eviction parity
        // with the orrery LRU is a refinement (the index is tiny). (Graphlet wiring P1.)
        self.graphlets
            .entry(target_graph)
            .or_insert_with(|| crate::graphlets::SessionGraphlets::load(&session_dir));
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
                ctx.orrery_mut().set_camera(crate::snapshot_to_camera(snapshot));
                let (yaw, tilt) = crate::snapshot_yaw_tilt(snapshot);
                ctx.orrery_mut().set_yaw(yaw);
                ctx.orrery_mut().set_tilt(tilt);
                ctx.view.centered = true;
            }
            None => ctx.view.centered = false,
        }
        if let Some(url) = restored_view.as_ref().and_then(|v| v.focus.as_deref()) {
            ctx.orrery_mut().select_by_url(url);
        }
        // Restore the pane's layout strategy (None = force-directed). (Layout picker.)
        ctx.orrery_mut().set_layout_strategy(restored_view.as_ref().and_then(|v| v.strategy.clone()));
        // Restore live workbench-mirror mode; its scope re-derives from the open tiles
        // through the render loop, so the flag is all that needs to come back. (Mirror.)
        ctx.view.mirror_tiles = restored_view.as_ref().is_some_and(|v| v.mirror_tiles);
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
        ctx.view.tile_textures.clear();
        ctx.view.snapshot_data_uris.clear();
        ctx.view.scroll.clear();
        ctx.view.focused_tile = None;
        ctx.view.shown_location = None;
        ctx.view.renaming = None;
        // Restore this graph's persisted tiling instead of wiping to an empty
        // workbench (the split shape / tabs / active tab now survive a restart). (A3.)
        ctx.view.workbench = ctx.restore_workbench(&session_dir);
        // Restore the orrery's settled layout from the cartography sidecar, overriding
        // the graph's load-time seed so the spatial view comes back as it was left
        // rather than re-scrambling. (Position sidecar.)
        let present: std::collections::HashSet<forme::GraphMemberId> =
            ctx.orrery().graph().nodes().map(|(_, node)| node.id).collect();
        if let Some(geom) = load_cartography(&session_dir, &present) {
            ctx.orrery_mut().seed_cartography(geom.iter());
            // Restore the importance metric first, so the sizing restore recomputes with it.
            ctx.orrery_mut().apply_cartography_importance_metric(geom.importance_metric());
            // Restore the per-node sizes + the size-by-degree / size-by-importance flags. (Graph signals.)
            ctx.orrery_mut().apply_cartography_sizing(
                geom.size_iter(),
                geom.size_by_degree(),
                geom.size_by_importance(),
            );
            // Restore the custom sprite faces, so a textured node re-opens textured. (Node-rep.)
            ctx.orrery_mut().apply_cartography_sprites(geom.sprite_iter());
            // ...and their collider hulls, so the traced-to-image collider survives too. (Node-rep.)
            ctx.orrery_mut().apply_cartography_sprite_hulls(geom.sprite_hull_iter());
            // ...and the per-node physical materials, so a tuned node re-opens tuned. (Body & face.)
            ctx.orrery_mut().apply_cartography_materials(geom.material_iter());
            // ...and the face overrides LAST (after sprites), so the chosen face wins. (Body & face.)
            ctx.orrery_mut().apply_cartography_faces(geom.face_iter());
        }
        ctx.view.maximized_pane = None;
        ctx.view.active_content = crate::ContentPane::Orrery;
        ctx.shared.session.session_dir = session_dir;
        ctx.shared.session.active_session_id = id;
        ctx.view.content_location =
            ctx.orrery().focused_url().unwrap_or("mere://welcome").to_string();
        ctx.refresh_session_labels();
        ctx.view.request_redraw();
    }

    /// Pool `graph`'s orrery under `graph_id` if it isn't already pooled, minting
    /// it from the loaded graph with its own offloaded physics actor (an empty one
    /// opens on the welcome node, like first launch). A no-op when the graph is
    /// already live — its pooled orrery is authoritative, so disk is not reloaded
    /// over it. Touches no focus: both session-switch and open-graph-beside pool
    /// through here. (Window composition P2.)
    pub(crate) fn pool_orrery(&mut self, graph_id: GraphId, graph: Graph, empty: bool) {
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
    pub(crate) fn touch_and_evict(&mut self, graph_id: GraphId) {
        self.orrery_lru.retain(|g| *g != graph_id);
        self.orrery_lru.push(graph_id);
        if self.orreries.len() <= crate::MAX_POOLED_ORRERIES {
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
        while self.orreries.len() > crate::MAX_POOLED_ORRERIES {
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
    pub(crate) fn open_graph_beside(&mut self, id: SessionId) {
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
            crate::frame_view::pane_path(&view.frame_layout, crate::GRAPH_PANE).unwrap_or_default();
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

    /// Thaw a graph engram (by its manifest id, as the Alembic Engrams row carries it) into a
    /// fresh, ephemeral Orrery pane beside the current one, read-only: browsing an engram does
    /// not mutate it, and the thawed graph pools under its own `GraphId` with no session manifest,
    /// so it is not persisted unless re-saved (editing forks a thaw — the immutable-engram model).
    /// No-op if the id is unparseable, the store is absent, or no engram is stored under it.
    /// (Alembic memory pane — open an engram, slice B2.)
    pub(crate) fn open_engram_beside(&mut self, id_str: &str) {
        let Some(id) = eidetic::Hash::parse(id_str)
            .ok()
            .map(eidetic::ManifestId::from_hash)
        else {
            return;
        };
        // Thaw off the private store; the borrow ends here, before the orrery-pool borrow.
        let Some(store) = self.shared.content.store.as_mut() else {
            return;
        };
        let Some(graph) =
            pollster::block_on(session_runtime::graph_engram::open_engram_as_session(store, id))
                .ok()
                .flatten()
        else {
            return;
        };
        // An engram is not a session, so it pools under a fresh, ephemeral graph id.
        let graph_id = GraphId::new();
        let empty = graph.nodes().count() == 0;
        self.pool_orrery(graph_id, graph, empty);
        self.touch_and_evict(graph_id);
        // Summon an Orrery leaf bound to the thawed graph, split right of the primary graph pane
        // (an even split), without switching focus — the same shape as `open_graph_beside`.
        let view = self.focused_view_mut();
        let pane_id = frame::PaneId(view.next_pane_id);
        view.next_pane_id += 1;
        let leaf = frame::PaneNode::Leaf {
            pane_id,
            content: frame::PaneContent::Orrery,
            graph_id,
        };
        let anchor =
            crate::frame_view::pane_path(&view.frame_layout, crate::GRAPH_PANE).unwrap_or_default();
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
            .record_frame_layout_changed("engram opened beside");
    }
}
