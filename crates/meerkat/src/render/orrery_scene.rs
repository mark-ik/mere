/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Orrery-scene rendering for [`render`](super): the focused pane's per-frame node
//! state/shape sync, layout-strategy drive, scene frame + retained gnode-pool
//! reconcile, plus the secondary graph panes. Split from `render.rs` to keep files
//! under the workspace 600-LOC ceiling.

use super::*;
use crate::window_view::{GnodeBuildStats, GnodeHotRow, GnodeSnapshot, GnodeStableRow};
use frame::GraphId;
use orrery::Face;
use serval_scripted_dom::ScriptedDom;

impl crate::WindowCtx<'_> {
    /// Drive the focused Orrery pane for this frame: push node state/shape colours, resize
    /// + recenter / self-heal, mirror the workbench tiles, run any active layout strategy,
    /// produce the scene, reconcile the on-screen DOM gnodes through the window-local
    /// retained pool, and return `(orrery_scene, orrery_redraw)`. (Extracted from
    /// `render()`.)
    pub(super) fn render_orrery_scene(
        &mut self,
        orrery_gid: GraphId,
        orrery_w: u32,
        orrery_h: u32,
        orrery_rect: [f32; 4],
        workbench_rect: Option<[f32; 4]>,
    ) -> (netrender::Scene, bool) {
        // Color the orrery's nodes by activation state (green open / red closed /
        // blue new) so the graph shows at a glance what's live. (Visible in
        // Cartography; the orrery is hidden in the tiled view.)
        let states = self.node_states();
        self.pane_orrery_mut(orrery_gid).set_node_states(states);
        // Shape each node by its content type (square document / rounded menu /
        // circle feed), the same per-node-hint path as the color states.
        let shapes = self.node_shapes();
        self.pane_orrery_mut(orrery_gid).set_node_shapes(shapes);

        // The orrery always composites its own scene into its leaf (kept in sync,
        // centered once). The tiled workbench, when its pane is open, composites a
        // separate scene into its own leaf — the two coexist now, no longer toggled.
        self.pane_orrery_mut(orrery_gid).resize(orrery_w, orrery_h);
        // The focused orrery renders its on-screen nodes as DOM gnodes in the shell (the
        // snapshot below), so drop its in-scene gnode layer. (Orrery-as-element.)
        self.pane_orrery_mut(orrery_gid)
            .set_render_gnodes_as_dom(true);
        if !self.view.centered {
            self.pane_orrery_mut(orrery_gid).recenter();
            self.view.centered = true;
        } else if !self.view.healed && self.pane_orrery(orrery_gid).has_nodes() {
            // One-shot self-heal: a restored camera that frames nothing (a
            // degenerate saved pan/zoom) snaps back to the graph. Gated on
            // has_nodes() so it waits for the async session load — firing against
            // the still-empty graph would spend the one shot while graph_visible()
            // is trivially true, leaving the restored degenerate camera in place
            // once the nodes actually arrive. Checked once, so it never fights an
            // intentional pan into empty space.
            self.view.healed = true;
            if !self.pane_orrery(orrery_gid).graph_visible() {
                self.pane_orrery_mut(orrery_gid).recenter();
            }
        }
        // Live workbench mirror: re-scope the focused orrery to the workbench's open
        // tiles each frame, so the spatial map tracks the tile set as it changes (the
        // two surfaces stay in lockstep). (Curated orrery — workbench mirror.)
        if self.view.mirror_tiles {
            let members = self.view.workbench.open_members();
            self.pane_orrery_mut(orrery_gid).scope_to_members(members);
        }
        // Drive the pane's active layout strategy (if any): compute its node positions
        // through platen's cartography dispatch and push them in; the orrery overlays
        // them on the physics snapshot each frame. No-op under force-directed. (Layout picker.)
        if let Some(id) = self
            .pane_orrery(orrery_gid)
            .layout_strategy()
            .map(str::to_string)
        {
            // Focus-driven strategies (radial) center on the pane's single selection;
            // passing it each frame lets radial re-center live as the selection moves.
            // The graph-only strategies ignore it. (Layout picker.)
            // Skip the whole analytic recompute unless an input changed (strategy, graph revision,
            // viewport, or focus) — analytic layouts were recomputed every frame before. When we do
            // recompute, refresh the community cache first (the revision moved), then project against
            // it so Louvain is not re-run per frame either. (Arrangements cache + graph signals.)
            let focus = self.pane_orrery(orrery_gid).focused_key();
            if self
                .pane_orrery(orrery_gid)
                .needs_strategy_recompute(&id, orrery_w, orrery_h, focus)
            {
                self.pane_orrery_mut(orrery_gid)
                    .refresh_community_cache(&id);
                let pane = self.pane_orrery(orrery_gid);
                let positions = platen::project_orrery_strategy(
                    &id,
                    pane.graph(),
                    pane.focused_key(),
                    orrery_w,
                    orrery_h,
                    pane.community(),
                );
                self.pane_orrery_mut(orrery_gid)
                    .apply_strategy_positions(&positions);
                self.pane_orrery_mut(orrery_gid)
                    .note_strategy_computed(&id, orrery_w, orrery_h, focus);
            }
        }
        // Content-embedding arrangement (burn brief Lane 5, P4): while "cluster by affinity" is on,
        // derive the affinity signal from node *content* (embeddings) instead of structural Jaccard,
        // and inject it so this frame's `sync_affinity_force` installs it. Recompute is throttled +
        // revision-gated inside `maybe_recompute`, so a settled graph costs nothing. The arrangement
        // is `take`n out to break the self-borrow (it lives on `content`, the graph on the pane) —
        // the gnode-pool idiom. Focused pane only for now; secondaries keep structural. Burn stays
        // out of the orrery: it receives plain `(NodeKey, NodeKey, f32)` triples.
        #[cfg(feature = "content-affinity")]
        if self.pane_orrery(orrery_gid).cluster_by_affinity() {
            if let Some(mut arrangement) = self.shared.content.content_arrangement.take() {
                let pairs = arrangement.maybe_recompute(self.pane_orrery(orrery_gid).graph());
                self.shared.content.content_arrangement = Some(arrangement);
                if let Some(pairs) = pairs {
                    self.pane_orrery_mut(orrery_gid)
                        .set_content_affinity(Some(pairs));
                }
            }
        }
        let (orrery_scene, orrery_redraw) =
            self.pane_orrery_mut(orrery_gid).frame(orrery_w, orrery_h);

        // The cursor (window px) for the per-gnode hover test below, copied out so the
        // node loop does not re-borrow self while `orrery` is held. (P0 hover.)
        let hover_cursor = self.view.cursor;
        let profile_enabled = tracing::enabled!(target: "meerkat::profile", tracing::Level::DEBUG);
        let snapshot_t = std::time::Instant::now();
        let mut build_stats = GnodeBuildStats::default();
        let mut gnode_pool = std::mem::take(&mut self.view.gnode_pool);
        let gnodes = {
            let orrery = self.pane_orrery(orrery_gid);
            let cam = orrery.camera();
            // The focused pane box, for culling gnodes to it: serval does not clip
            // transformed overflow, so an off-screen node would otherwise escape the
            // orrery element up into the chrome (the toolbar-escape we saw).
            let (pw, ph) = (
                orrery_rect[2] - orrery_rect[0],
                orrery_rect[3] - orrery_rect[1],
            );
            let mut gnodes = Vec::new();
            for (key, node) in orrery.graph().nodes() {
                // Branch scope (Phase 2 slice 3): a scoped orrery — a branch window
                // scoped to its graphlet — shows only its scoped members as gnodes;
                // non-members are dropped, matching the scene's own scope filter.
                if !orrery.node_in_scope(key) {
                    continue;
                }
                // Settings nodes render as normal nodes addressing their page (Mark,
                // 2026-06-22): a `settings://` node is a first-class, visible graph node you
                // can see / open / relate, not an invisible tile-only member. Opening it
                // routes to the settings page like any node. (Settings lane — visible nodes.)
                let Some(w) = orrery.node_position(key) else {
                    continue;
                };
                let x = w.x * cam.zoom + cam.offset.0;
                let y = w.y * cam.zoom + cam.offset.1;
                // Off-pane nodes ride the underlay demote-dots, not a gnode.
                if !(0.0..=pw).contains(&x) || !(0.0..=ph).contains(&y) {
                    continue;
                }
                // The node's footprint (per-node override / size-by-degree / default),
                // used both as the gnode's face size and as the hover hit-box half. (P0.)
                let node_size = orrery.node_size(key);
                let face_half = node_size / 2.0;
                let face = orrery.node_face(key);
                let favicon = gnode_pool.cached_favicon(
                    node.id,
                    node.favicon_rgba.as_deref(),
                    node.favicon_width,
                    node.favicon_height,
                    &mut build_stats,
                );
                let sprite = gnode_pool.cached_sprite(node.id, orrery.node_sprite(key));
                gnodes.push(GnodeSnapshot {
                    member: node.id,
                    hot: GnodeHotRow {
                        x,
                        y,
                        // State color only; selection shows as a ring + lift on the gnode face.
                        color: orrery.node_state_color(key),
                        selected: orrery.node_selected(key),
                        // Hover: the cursor over this node's face box (window px). (P0 hover.)
                        hovered: {
                            let (wx, wy) = (orrery_rect[0] + x, orrery_rect[1] + y);
                            (hover_cursor.0 - wx).abs() <= face_half
                                && (hover_cursor.1 - wy).abs() <= face_half
                        },
                        size: node_size,
                    },
                    stable: GnodeStableRow {
                        label: gnode_pool.cached_label(node.id, &node.title),
                        // Content-type silhouette as the face's border-radius.
                        radius: match orrery.node_shape(key) {
                            orrery::NodeShape::Square => "0",
                            orrery::NodeShape::Rounded => "9px",
                            orrery::NodeShape::Circle => "50%",
                        },
                        image_uri: match face {
                            Face::Sprite => sprite,
                            Face::Favicon => favicon,
                            Face::Bare => None,
                        },
                        image_cover: matches!(face, Face::Sprite),
                        show_label: !matches!(face, Face::Bare),
                        // The body hull, so the gnode face is clipped to the collider shape. (B&F.)
                        hull: gnode_pool.cached_hull(node.id, orrery.node_sprite_hull(key)),
                    },
                });
            }
            gnodes
        };
        let gnode_count = gnodes.len();
        let snapshot_build_us = snapshot_t.elapsed().as_micros();
        let pool_stats = gnode_pool.reconcile(&self.view.dom, orrery_gid, gnodes);
        self.view.gnode_pool = gnode_pool;
        // The focused node's content card (snapshot / unvisited placeholder), placed after
        // the gnodes in document order so it paints over them. (Layering fix.)
        let focus_card = self.compute_focus_card(orrery_rect, workbench_rect);
        let orrery_render = OrreryRender {
            rect: orrery_rect,
            focus_card,
        };
        // Only rebuild the shell view when the snapshot actually changed: a settled
        // orrery (no motion, selection, or camera change) produces an identical snapshot
        // each frame, so this skips the per-frame view re-run + diff entirely. (Perf.)
        let mut view_rerun = false;
        let mut view_rerun_us = 0;
        if &orrery_render != self.view.orrery_render() {
            let view_rerun_t = std::time::Instant::now();
            self.view.set_orrery(orrery_render);
            view_rerun = true;
            view_rerun_us = view_rerun_t.elapsed().as_micros();
        }
        if profile_enabled {
            let shell_node_count = {
                let dom = self.view.dom.borrow();
                count_dom_nodes(&dom, dom.document())
            };
            tracing::debug!(
                target: "meerkat::profile",
                snapshot_build_us,
                view_rerun,
                view_rerun_us,
                gnode_count,
                shell_node_count,
                favicon_encodes = build_stats.favicon_encodes,
                pool_structural_inserts = pool_stats.structural_inserts,
                pool_structural_removes = pool_stats.structural_removes,
                pool_hot_attr_writes = pool_stats.hot_attr_writes,
                pool_stable_attr_writes = pool_stats.stable_attr_writes,
                "gnode pool profile"
            );
        }
        (orrery_scene, orrery_redraw)
    }

    /// Render every secondary graph pane (an Orrery leaf bound to a graph other than the
    /// focused one) into its own leaf: resize + recenter each, run its layout strategy,
    /// frame it, and collect `(scene, rect, w, h)` per pane for the composite below.
    /// (Extracted from `render()` — Window composition P2.)
    pub(super) fn render_secondary_orreries(
        &mut self,
        leaves: &[frame_view::LaidLeaf],
        orrery_gid: GraphId,
    ) -> Vec<(netrender::Scene, [f32; 4], u32, u32)> {
        let secondary_orreries: Vec<(netrender::Scene, [f32; 4], u32, u32)> = leaves
            .iter()
            .filter(|l| matches!(l.content, PaneContent::Orrery) && l.graph_id != orrery_gid)
            .map(|l| {
                let sw = (l.rect[2] - l.rect[0]).round().max(1.0) as u32;
                let sh = (l.rect[3] - l.rect[1]).round().max(1.0) as u32;
                let orrery = self.pane_orrery_mut(l.graph_id);
                orrery.resize(sw, sh);
                orrery.set_render_gnodes_as_dom(false); // secondary panes keep their gnodes
                if !orrery.graph_visible() {
                    orrery.recenter();
                }
                if let Some(id) = orrery.layout_strategy().map(str::to_string) {
                    let focus = orrery.focused_key();
                    if orrery.needs_strategy_recompute(&id, sw, sh, focus) {
                        orrery.refresh_community_cache(&id);
                        let positions = platen::project_orrery_strategy(
                            &id,
                            orrery.graph(),
                            orrery.focused_key(),
                            sw,
                            sh,
                            orrery.community(),
                        );
                        orrery.apply_strategy_positions(&positions);
                        orrery.note_strategy_computed(&id, sw, sh, focus);
                    }
                }
                let (scene, _) = orrery.frame(sw, sh);
                (scene, l.rect, sw, sh)
            })
            .collect();
        secondary_orreries
    }
}

fn count_dom_nodes(dom: &ScriptedDom, node: serval_scripted_dom::NodeId) -> usize {
    1 + dom
        .dom_children(node)
        .map(|child| count_dom_nodes(dom, child))
        .sum::<usize>()
}
