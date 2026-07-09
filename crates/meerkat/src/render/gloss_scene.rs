/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The gloss minimap's per-frame build: node positions (mirroring the main view or
//! the gloss's own lens projection — unchanged from the pre-migration Scene block),
//! the shared fit transform, the DOM node snapshot (folded into the shell document
//! like the outline/recent lenses), and the backdrop Scene (edges/rings, embedded
//! via `<external-texture>`). Moved out of `paint.rs`'s old all-Scene block by the
//! Scene-to-DOM migration's Phase 2 — the same move `render_orrery_scene` made for
//! the orrery. Split out to keep files under the workspace 600-LOC ceiling.

use super::*;
use crate::gloss_view::{GlossMinimapNode, GlossMinimapSnapshot};
use mere::gloss::MinimapFit;

impl WindowCtx<'_> {
    /// Build this frame's gloss minimap: fold its DOM node snapshot into the shell
    /// document and return the backdrop Scene (edges + signal rings) for the caller
    /// to rasterize + composite at `GLOSS_MINIMAP_SCENE_KEY`. `None` when the gloss
    /// pane is closed (closing the DOM fold-in too) or when there are no nodes to
    /// show. (Scene-to-DOM migration P2.)
    pub(super) fn render_gloss_minimap(
        &mut self,
        gloss_rect: Option<[f32; 4]>,
    ) -> Option<(netrender::Scene, u32, u32)> {
        let Some(grect) = gloss_rect else {
            if self.gloss_minimap_open() {
                self.set_gloss_minimap(GlossMinimapSnapshot::default(), None);
            }
            return None;
        };
        let (minimap_rect, _, _) = mere::gloss::gloss_sections(grect);
        let mw = (minimap_rect[2] - minimap_rect[0]).round().max(1.0) as u32;
        let mh = (minimap_rect[3] - minimap_rect[1]).round().max(1.0) as u32;

        // With a gloss lens set, the gloss shows its OWN arrangement (recomputed only
        // when its inputs change, since it may be an expensive layout); otherwise it
        // mirrors the main view. Unchanged from the pre-migration Scene block.
        // (Graph signals — P6, the independent gloss projection.)
        let (nodes, edges, rings) =
            if let Some(id) = self.orrery().gloss_strategy().map(str::to_string) {
                if self.orrery().gloss_needs_recompute(mw, mh) {
                    let pane = self.orrery();
                    let clusters = pane
                        .show_community_rings()
                        .then(|| pane.community())
                        .flatten();
                    let bridges = pane.show_bridge_rings().then(|| pane.bridges()).flatten();
                    let (positions, overlays): (Vec<_>, _) = match pane.gloss_scope_keys() {
                        Some(scope) => (
                            mere::platen::project_orrery_subgraph(
                                pane.graph(),
                                &scope,
                                &id,
                                pane.focused_key(),
                                mw,
                                mh,
                            ),
                            mere::platen::signal_overlays(clusters, bridges),
                        ),
                        None => {
                            let projection = mere::platen::project_orrery_lens(
                                &id,
                                pane.graph(),
                                pane.focused_key(),
                                mw,
                                mh,
                                clusters,
                                bridges,
                            );
                            let pos = projection
                                .nodes
                                .iter()
                                .map(|n| (n.node, n.position))
                                .collect();
                            (pos, projection.overlays)
                        }
                    };
                    self.orrery_mut()
                        .set_gloss_positions(positions, overlays, mw, mh);
                }
                self.orrery().gloss_geometry_cached()
            } else {
                let (n, e) = self.orrery().minimap_geometry();
                (n, e, Vec::new())
            };

        let Some(fit) = MinimapFit::compute(
            &nodes.iter().map(|(_, pos, _, _)| *pos).collect::<Vec<_>>(),
            mw,
            mh,
        ) else {
            self.set_gloss_minimap(GlossMinimapSnapshot::default(), Some(minimap_rect));
            return None;
        };

        let theme = self.shared.presentation.chrome_theme.clone();
        let node_color = mere::gloss::theme_rgb_css(theme.body_text);
        let selected_color = mere::gloss::theme_rgb_css(theme.strong_text);
        let graph = self.orrery().graph();
        let dom_nodes: Vec<GlossMinimapNode> = nodes
            .iter()
            .map(|(member, pos, selected, size_factor)| {
                let (x, y) = fit.apply(*pos);
                let url = graph
                    .get_node_by_id(*member)
                    .map(|(_, n)| n.url().to_string())
                    .unwrap_or_default();
                GlossMinimapNode {
                    member: *member,
                    url,
                    x,
                    y,
                    size: mere::gloss::minimap_node_size(*selected, *size_factor),
                    color: if *selected {
                        selected_color.clone()
                    } else {
                        node_color.clone()
                    },
                }
            })
            .collect();
        self.set_gloss_minimap(
            GlossMinimapSnapshot {
                nodes: dom_nodes,
                w: mw,
                h: mh,
            },
            Some(minimap_rect),
        );

        let mapped_edges: Vec<((f32, f32), (f32, f32), f32)> = edges
            .iter()
            .map(|(a, b, weight)| (fit.apply(*a), fit.apply(*b), *weight))
            .collect();
        let mapped_rings: Vec<((f32, f32), f32, [f32; 4])> = rings
            .iter()
            .map(|(center, factor, color)| (fit.apply(*center), *factor, *color))
            .collect();
        let backdrop = mere::gloss::minimap_backdrop_scene(&mapped_edges, &mapped_rings, mw, mh, &theme);
        Some((backdrop, mw, mh))
    }
}
