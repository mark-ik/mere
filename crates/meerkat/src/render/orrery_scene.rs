/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Orrery-scene rendering for [`render`](super): the focused pane's per-frame node
//! state/shape sync, layout-strategy drive, scene frame + node-card snapshot, plus the
//! secondary graph panes. Split from `render.rs` to keep files under the workspace
//! 600-LOC ceiling.

use super::*;
use frame::GraphId;

impl crate::WindowCtx<'_> {
    /// Drive the focused Orrery pane for this frame: push node state/shape colours, resize
    /// + recenter / self-heal, mirror the workbench tiles, run any active layout strategy,
    /// produce the scene, snapshot the on-screen nodes as DOM cards (with the focused-node
    /// content card) into the shell view, and return `(orrery_scene, orrery_redraw)`.
    /// (Extracted from `render()`.)
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
        // The focused orrery renders its on-screen nodes as DOM cards in the shell (the
        // snapshot below), so drop its in-scene gnode layer. (Orrery-as-element.)
        self.pane_orrery_mut(orrery_gid).set_render_as_cards(true);
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
        let (orrery_scene, orrery_redraw) =
            self.pane_orrery_mut(orrery_gid).frame(orrery_w, orrery_h);

        // Orrery-as-element (i-2): snapshot the focused orrery's nodes through its
        // camera into the shell state, so the orrery element renders a DOM card per
        // node. The update + frame() above ran this frame, so the snapshot reads
        // this-frame positions/colors/scope and the cards align with the scene. (Phase 2.)
        // The cursor (window px) for the per-card hover test below, copied out so the card
        // closure does not re-borrow self while `orrery` is held. (P0 hover.)
        let hover_cursor = self.view.cursor;
        let orrery_cards = {
            let orrery = self.pane_orrery(orrery_gid);
            let cam = orrery.camera();
            // The focused pane box, for culling cards to it: serval does not clip
            // transformed overflow, so an off-screen node would otherwise escape the
            // orrery element up into the chrome (the toolbar-escape we saw).
            let (pw, ph) = (
                orrery_rect[2] - orrery_rect[0],
                orrery_rect[3] - orrery_rect[1],
            );
            let cards = orrery
                .graph()
                .nodes()
                .filter_map(|(key, node)| {
                    // Branch scope (Phase 2 slice 3): a scoped orrery — a branch window
                    // scoped to its graphlet — shows only its scoped members as cards;
                    // non-members are dropped, matching the scene's own scope filter.
                    if !orrery.node_in_scope(key) {
                        return None;
                    }
                    // Settings nodes render as normal nodes addressing their page (Mark,
                    // 2026-06-22): a `settings://` node is a first-class, visible graph node you
                    // can see / open / relate, not an invisible tile-only member. Opening it
                    // routes to the settings page like any node. (Settings lane — visible nodes.)
                    let w = orrery.node_position(key)?;
                    let x = w.x * cam.zoom + cam.offset.0;
                    let y = w.y * cam.zoom + cam.offset.1;
                    // Off-pane nodes ride the underlay demote-dots, not a card.
                    if !(0.0..=pw).contains(&x) || !(0.0..=ph).contains(&y) {
                        return None;
                    }
                    // Label: a real page title once the node has loaded one; otherwise the
                    // URL's last path segment (the readable slug, e.g. a wiki article name),
                    // which previews the eventual title and keeps same-site nodes distinct.
                    // node.title is seeded to the URL until a load completes, so a URL-shaped
                    // title is the un-loaded case. (node_display_label would collapse these to
                    // the bare host.) Capped with an ellipsis for the compact on-canvas card.
                    const CARD_LABEL_CAP: usize = 24;
                    let raw = node.title.trim_end_matches('/');
                    let base = if raw.contains("://") {
                        match raw.rsplit('/').next() {
                            Some(slug) if !slug.is_empty() => slug,
                            _ => raw,
                        }
                    } else {
                        raw
                    };
                    let label = if base.chars().count() <= CARD_LABEL_CAP {
                        base.to_string()
                    } else {
                        base.chars()
                            .take(CARD_LABEL_CAP - 1)
                            .chain(['\u{2026}'])
                            .collect()
                    };
                    // The node's footprint (per-node override / size-by-degree / default),
                    // used both as the card's face size and as the hover hit-box half. (P0.)
                    let node_size = orrery.node_size(key);
                    let face_half = node_size / 2.0;
                    Some(OrreryCard {
                        member: node.id,
                        label,
                        x,
                        y,
                        // State color only; selection shows as a ring + lift on the card face.
                        color: orrery.node_state_color(key).to_string(),
                        selected: orrery.node_selected(key),
                        // Hover: the cursor over this node's face box (window px). (P0 hover.)
                        hovered: {
                            let (wx, wy) = (orrery_rect[0] + x, orrery_rect[1] + y);
                            (hover_cursor.0 - wx).abs() <= face_half
                                && (hover_cursor.1 - wy).abs() <= face_half
                        },
                        size: node_size,
                        // Content-type silhouette as the face's border-radius.
                        radius: match orrery.node_shape(key) {
                            orrery::NodeShape::Square => "0",
                            orrery::NodeShape::Rounded => "9px",
                            orrery::NodeShape::Circle => "50%",
                        },
                        favicon: node.favicon_rgba.as_ref().and_then(|rgba| {
                            favicon_data_uri(rgba, node.favicon_width, node.favicon_height)
                        }),
                        // The custom sprite image (a data-URI), for a `Sprite` face. (P2.)
                        sprite: orrery.node_sprite(key).map(str::to_string),
                        face: orrery.node_face(key),
                        // The body hull, so the card face is clipped to the collider shape. (B&F.)
                        hull: orrery
                            .node_sprite_hull(key)
                            .map(<[(f32, f32)]>::to_vec)
                            .unwrap_or_default(),
                    })
                })
                .collect();
            cards
        };
        // The focused node's content card (snapshot / unvisited placeholder), placed after
        // the node cards in document order so it paints over them. (Layering fix.)
        let focus_card = self.compute_focus_card(orrery_rect, workbench_rect);
        let orrery_render = OrreryRender {
            rect: orrery_rect,
            cards: orrery_cards,
            focus_card,
        };
        // Only rebuild the shell view when the snapshot actually changed: a settled
        // orrery (no motion, selection, or camera change) produces an identical snapshot
        // each frame, so this skips the per-frame view re-run + diff entirely. (Perf.)
        if &orrery_render != self.view.orrery_render() {
            self.view.set_orrery(orrery_render);
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
                orrery.set_render_as_cards(false); // secondary panes keep their gnodes
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
