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
use paint_list_api::{DeviceIntSize, PaintList};
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

        // A third (screen-space) layer for the marquee rubber-band, when active.
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
        if let Some(cmds) = marquee_cmds.as_ref() {
            layers.push(CompositeLayer::commands_only(cmds));
        }
        let scene = composite_paint_layers(viewport, &layers).scene;

        let needs_redraw = settling || gliding || dragging;
        (scene, needs_redraw)
    }
}
