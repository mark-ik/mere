/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The per-frame render step for [`Orrery`](crate::Orrery). Factored from
//! `lib.rs` to keep files under the workspace 600-LOC ceiling.

use std::collections::{HashMap, HashSet};

use kernel::geometry::PortablePoint;
use layout_dom_api::LayoutDomMut;
use kernel::graph::NodeKey;
use netrender::Scene;
use paint_list_api::{
    AlphaType, ColorF, CommonPlacement, DeviceIntSize, IdNamespace, ImageItem, ImageKey,
    ImageRendering, ImageResource, LayoutPoint, LayoutRect, PaintCmd, PaintList,
};
use paint_list_render::{composite_paint_layers, CompositeLayer};
use platen::orrery::orrery_paint_list_demoted;
use serval_layout::{Applied, IncrementalLayout, ScrollOffsets};
use serval_scripted_dom::NodeId as DomNodeId;

use super::build::{
    background_cmds, field_overlay, marquee_rect_cmds, selected_edge_overlay, set_class, set_style,
    NODE_SHEET,
};
use super::{NodeShape, NodeState, Orrery, NODE_HALF, PAN_DECAY};

impl Orrery {
    /// Advance one frame at viewport `(w, h)` and return the composited content
    /// scene plus whether the host should request another frame (sim still
    /// settling, pan still gliding, or a node being dragged). Does not present.
    pub fn frame(&mut self, w: u32, h: u32) -> (Scene, bool) {
        let (w, h) = (w.max(1), h.max(1));
        self.view_w = w;
        self.view_h = h;
        let viewport = DeviceIntSize::new(w as i32, h as i32);

        // Advance physics (the in-thread tick, or the freshest actor snapshot)
        // into the read model, and learn whether the layout is still settling.
        // Everything below reprojects from the view — never the rapier world.
        let settling = self.physics.advance_frame(&mut self.view);
        // A dragged node tracks the cursor with zero round-trip: re-pin it locally
        // over whatever the backend just reported (the actor lags a frame behind,
        // and the in-thread snapshot already agrees, so this is a no-op there).
        let dragging = self.drag.is_some_and(|d| d.moved);
        if let Some(d) = self.drag {
            if d.moved {
                self.view.set_position(d.node, self.screen_to_world(self.cursor));
            }
        }
        // Pan inertia: glide + decay when not actively middle-dragging.
        let gliding = self.middle_drag.is_none()
            && (self.pan_velocity.0.abs() > 0.05 || self.pan_velocity.1.abs() > 0.05);
        if gliding {
            self.camera.offset.0 += self.pan_velocity.0;
            self.camera.offset.1 += self.pan_velocity.1;
            self.pan_velocity.0 *= PAN_DECAY;
            self.pan_velocity.1 *= PAN_DECAY;
        } else if self.middle_drag.is_none() {
            self.pan_velocity = (0.0, 0.0);
        }
        self.generation = self.generation.wrapping_add(1);

        // Reproject the underlay from the view positions (a
        // node with no body falls back to its committed position in the producer).
        let positions: HashMap<NodeKey, PortablePoint> = self
            .view
            .positions()
            .map(|(k, p)| (k, PortablePoint::new(p.x, p.y)))
            .collect();
        // On-screen nodes (cull against the world-space viewport) become DOM
        // children; the rest demote to underlay rects, so no node double-draws.
        let on_screen: HashSet<NodeKey> =
            self.view.cull_aabb(self.world_viewport()).into_iter().collect();

        let mut underlay = orrery_paint_list_demoted(
            &self.graph,
            |k| positions.get(&k).copied(),
            |k| !on_screen.contains(&k),
            // Skip relations whose undirected pair the user has hidden.
            |rel| {
                let pair =
                    if rel.from <= rel.to { (rel.from, rel.to) } else { (rel.to, rel.from) };
                !self.hidden_edges.contains(&pair)
            },
            viewport,
            self.camera,
            &self.style,
            self.generation,
        );
        // Placed field regions paint *under* the edges + demoted nodes (a background
        // the graph sits within), spliced at the bottom of the same camera transform.
        // The dashed extent box shows only for the hovered field (box-on-interaction);
        // hidden fields are skipped. (Field regions — disk-in-box + box-on-interaction.)
        underlay.splice_world_underlay(field_overlay(
            &self.graph,
            self.active_field(),
            self.hidden_field_ids(),
        ));
        // Highlight selected edges by splicing thicker strokes inside the
        // underlay's camera transform (world space — no transform replication).
        if !self.selected_edges.is_empty() {
            underlay.splice_world_overlays(selected_edge_overlay(&self.view, &self.selected_edges));
        }

        // The node-children layer — the pre-materialized pool. Ensure the
        // incremental layout exists at this viewport, then mutate the `.stage`
        // camera transform and each gnode's transform + selection class. These are
        // attribute-only (paint-tier), so `apply` stays on the RepaintOnly path —
        // no per-frame relayout or DOM rebuild.
        if self.node_layout.is_none() || self.pool_w != w || self.pool_h != h {
            let mut discard = Vec::new();
            self.node_dom.drain_mutations(&mut discard);
            self.node_layout =
                Some(IncrementalLayout::new(&self.node_dom, NODE_SHEET, w as f32, h as f32));
            self.pool_w = w;
            self.pool_h = h;
        }
        set_style(
            &mut self.node_dom,
            self.stage_node,
            &format!(
                "transform: translate({}px, {}px) scale({});",
                self.camera.offset.0, self.camera.offset.1, self.camera.zoom
            ),
        );
        let gnodes: Vec<(NodeKey, DomNodeId)> =
            self.gnode_of.iter().map(|(&k, &g)| (k, g)).collect();
        for (key, gnode) in gnodes {
            let pos = positions.get(&key).copied().unwrap_or_default();
            set_style(
                &mut self.node_dom,
                gnode,
                &format!("transform: translate({}px, {}px);", pos.x - NODE_HALF, pos.y - NODE_HALF),
            );
            // Selection wins (orange); otherwise color by activation state —
            // green open, red closed, blue idle (the default for an unset node).
            let state_class = if self.selected.contains(&key) {
                "gnode-selected"
            } else {
                match self.node_states.get(&key) {
                    Some(NodeState::Open) => "gnode-open",
                    Some(NodeState::Closed) => "gnode-closed",
                    _ => "gnode-idle",
                }
            };
            // Shape rides as a second class (border-radius only) so it merges with
            // the color class; square is the default (no shape class).
            let shape_class = match self.node_shapes.get(&key) {
                Some(NodeShape::Rounded) => " gnode-rounded",
                Some(NodeShape::Circle) => " gnode-circle",
                _ => "",
            };
            set_class(&mut self.node_dom, gnode, &format!("{state_class}{shape_class}"));
        }
        let mut muts = Vec::new();
        self.node_dom.drain_mutations(&mut muts);
        let applied = self.node_layout.as_mut().unwrap().apply(&self.node_dom, NODE_SHEET, &muts);
        if !matches!(applied, Applied::RepaintOnly | Applied::Unchanged) {
            tracing::warn!(?applied, "orrery pool: node layout left the RepaintOnly path");
        }
        let scroll = ScrollOffsets::<DomNodeId>::default();
        let nodes_plist =
            self.node_layout.as_ref().unwrap().emit_paint_list(&self.node_dom, &scroll, viewport);

        // Favicon layer: a textured quad over each on-screen tile that carries a
        // favicon. This layer is NOT under the `.stage` camera transform (it is a
        // bare command list, not the serval DOM), so the camera is applied here:
        // a world point maps to screen by `world * zoom + offset`. The favicon's
        // `favicon_rgba` is already the `ImageResource` shape (RGBA8, straight alpha),
        // so the host's existing rasterize uploads it with no GPU plumbing in the
        // orrery. It draws above the colored tile square and below the marquee.
        // (Favicon-on-tile.)
        let (cam_ox, cam_oy, cam_z) =
            (self.camera.offset.0, self.camera.offset.1, self.camera.zoom);
        let mut favicon_cmds: Vec<PaintCmd> = Vec::new();
        let mut favicon_images: Vec<ImageResource> = Vec::new();
        for &key in &on_screen {
            let Some(pos) = positions.get(&key) else { continue };
            let Some(node) = self.graph.get_node(key) else { continue };
            let Some(rgba) = node.favicon_rgba.as_ref() else { continue };
            if rgba.is_empty() || node.favicon_width == 0 || node.favicon_height == 0 {
                continue;
            }
            let img_key = ImageKey::new(IdNamespace(0), favicon_images.len() as u32);
            favicon_images.push(ImageResource {
                key: img_key,
                width: node.favicon_width,
                height: node.favicon_height,
                data: rgba.clone(),
            });
            let x0 = (pos.x - NODE_HALF) * cam_z + cam_ox;
            let y0 = (pos.y - NODE_HALF) * cam_z + cam_oy;
            let x1 = (pos.x + NODE_HALF) * cam_z + cam_ox;
            let y1 = (pos.y + NODE_HALF) * cam_z + cam_oy;
            favicon_cmds.push(PaintCmd::DrawImage(ImageItem {
                placement: CommonPlacement::new(LayoutRect::new(
                    LayoutPoint::new(x0, y0),
                    LayoutPoint::new(x1, y1),
                )),
                image_key: img_key,
                image_rendering: ImageRendering::Auto,
                alpha_type: AlphaType::Alpha,
                color: ColorF { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
            }));
        }

        // A screen-space layer for the marquee rubber-band, when active.
        let marquee_cmds = self.marquee.map(|origin| marquee_rect_cmds(origin, self.cursor));
        // The orrery's own opaque backdrop is the bottom layer (so the surface is
        // dark without depending on the host clear color); then the underlay edges
        // + demoted rects, then the on-screen node DOM, then any marquee on top.
        let bg_cmds = background_cmds(w, h, self.backdrop);
        let mut layers = vec![
            CompositeLayer::commands_only(&bg_cmds),
            CompositeLayer::commands_only(underlay.commands()),
            CompositeLayer {
                commands: nodes_plist.commands(),
                fonts: nodes_plist.fonts(),
                images: nodes_plist.images(),
            },
        ];
        if !favicon_cmds.is_empty() {
            layers.push(CompositeLayer {
                commands: &favicon_cmds,
                fonts: &[],
                images: &favicon_images,
            });
        }
        if let Some(cmds) = marquee_cmds.as_ref() {
            layers.push(CompositeLayer::commands_only(cmds));
        }
        let scene = composite_paint_layers(viewport, &layers).scene;

        let needs_redraw = settling || gliding || dragging;
        (scene, needs_redraw)
    }
}

#[cfg(test)]
mod tests {
    use crate::Orrery;
    use euclid::default::Point2D;
    use kernel::graph::Graph;

    fn graph_with_one_node(url: &str) -> (Graph, kernel::graph::NodeKey) {
        let mut graph = Graph::new();
        let key =
            graph.add_node_with_id(Graph::node_namespace_id(url), url.to_string(), Point2D::zero());
        (graph, key)
    }

    fn image_op_count(scene: &netrender::Scene) -> usize {
        scene.ops.iter().filter(|op| matches!(op, netrender::SceneOp::Image(_))).count()
    }

    /// A node carrying favicon RGBA emits an image op over its on-screen tile, so the
    /// host rasterizes a real favicon on the square. (Favicon-on-tile.)
    #[test]
    fn favicon_node_emits_an_image_op() {
        let (mut graph, key) = graph_with_one_node("https://ex.test/");
        // A tiny 2x2 opaque favicon (RGBA8, 16 bytes).
        assert!(graph.set_node_favicon(key, vec![255u8; 2 * 2 * 4], 2, 2));
        let mut orrery = Orrery::with_graph(graph);
        let (scene, _) = orrery.frame(800, 600);
        assert!(image_op_count(&scene) >= 1, "a favicon node emits at least one image op");
    }

    /// Without a favicon, no image op is emitted (the tile is just a colored square).
    #[test]
    fn node_without_favicon_emits_no_image_op() {
        let (graph, _key) = graph_with_one_node("https://ex.test/");
        let mut orrery = Orrery::with_graph(graph);
        let (scene, _) = orrery.frame(800, 600);
        assert_eq!(image_op_count(&scene), 0, "no favicon -> no image op");
    }
}
