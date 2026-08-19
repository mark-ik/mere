// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-frame render step for [`Canvas`](crate::Canvas). Factored from
//! `lib.rs` to keep files under the workspace 600-LOC ceiling.

use std::collections::{HashMap, HashSet};

use crate::underlay::{canvas_paint_list_demoted_from_arrangement, identity_arrangement};
use genet_layout::{Applied, IncrementalLayout, ScrollOffsets};
use genet_scripted_dom::NodeId as DomNodeId;
use kernel::geometry::PortablePoint;
use kernel::graph::NodeKey;
use layout_dom_api::LayoutDomMut;
use netrender::Scene;
use paint_list_api::{
    AlphaType, ColorF, CommonPlacement, DeviceIntSize, ExtendMode, GradientStop, IdNamespace,
    ImageItem, ImageKey, ImageRendering, ImageResource, LayoutPoint, LayoutRect, LayoutSize,
    LayoutTransform, PaintCmd, PaintList, PathCommand, PathData, PathItem, RadialGradientItem,
    RadialGradientPayload, RectItem, StrokeCap, StrokeItem, StrokeJoin, StrokeStyle, TransformKind,
    TransformSpec,
};
use paint_list_render::{CompositeLayer, composite_paint_layers};
use seiche::NodeCollider;

use super::build::{
    NODE_SHEET, background_cmds, bridge_ring_overlay, community_ring_overlay, field_overlay,
    marquee_rect_cmds, set_class, set_style,
};
use super::edge_cells::{
    edge_cell_for_relation, relation_cell_overlay, relation_family_color, selected_edge_overlay,
};
use super::fold_projection::{FOLD_SUMMARY_RADIUS, FoldProjection};
use super::{Canvas, NodeShape, NodeState, PAN_DECAY};

/// Paint a fold's boundary bundles and its synthetic summary body. These are
/// world-space commands only: neither the summary nor its boundary cells enter
/// the source graph, layout view, or physics simulation.
fn fold_summary_overlay(
    fold: &FoldProjection,
    positions: &HashMap<NodeKey, PortablePoint>,
    summary_color: ColorF,
) -> Vec<PaintCmd> {
    let Some(center) = fold.summary_center(|key| positions.get(&key).copied()) else {
        return Vec::new();
    };
    let mut commands = Vec::with_capacity(fold.boundary_bundles.len() + 1);
    for bundle in &fold.boundary_bundles {
        let Some(outside) = positions.get(&bundle.outside).copied() else {
            continue;
        };
        let dx = outside.x - center.x;
        let dy = outside.y - center.y;
        let length = (dx * dx + dy * dy).sqrt();
        if length <= f32::EPSILON {
            continue;
        }
        let ux = dx / length;
        let uy = dy / length;
        let from = LayoutPoint::new(
            center.x + ux * FOLD_SUMMARY_RADIUS,
            center.y + uy * FOLD_SUMMARY_RADIUS,
        );
        let to = LayoutPoint::new(outside.x - ux * 18.0, outside.y - uy * 18.0);
        let bounds = LayoutRect::new(
            LayoutPoint::new(from.x.min(to.x), from.y.min(to.y)),
            LayoutPoint::new(from.x.max(to.x), from.y.max(to.y)),
        );
        commands.push(PaintCmd::DrawStroke(StrokeItem {
            placement: CommonPlacement::new(bounds),
            path: PathData {
                commands: vec![PathCommand::MoveTo(from), PathCommand::LineTo(to)],
            },
            color: relation_family_color(bundle.family),
            width: 2.0 + (bundle.count.saturating_sub(1).min(4) as f32),
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            dash: None,
        }));
    }
    let radius = FOLD_SUMMARY_RADIUS;
    commands.push(PaintCmd::DrawRect(RectItem {
        placement: CommonPlacement::new(LayoutRect::new(
            LayoutPoint::new(center.x - radius, center.y - radius),
            LayoutPoint::new(center.x + radius, center.y + radius),
        )),
        color: summary_color,
    }));
    commands
}

impl Canvas {
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
        // Advance the ambient backdrop sim (it paces itself internally - GoL accumulates toward its
        // generation interval, a continuous sim integrates). A fixed ~frame dt is fine for a
        // backdrop. (Physics scenes P5.)
        if let Some(sim) = self.ambient.as_mut() {
            sim.advance(1.0 / 60.0);
        }
        // A non-seiche layout strategy overrides the physics snapshot: write its buffered
        // positions into the view before anything reads it. (Layout picker.)
        self.apply_strategy_to_view();
        // Pick up any finished off-thread community partition (a no-op when computing inline), so a
        // result dispatched on an earlier frame lands before the rings paint. (Graph signals — P3.)
        self.drain_community();
        // The community-ring overlay needs a fresh partition; recompute it only when the toggle is
        // on and the graph changed (generation-gated). (Graph signals — community to a ring.)
        if self.show_community_rings {
            self.ensure_community_fresh();
        }
        // The bridge-ring overlay needs the broker set, likewise revision-gated. (Graph signals.)
        if self.show_bridge_rings {
            self.ensure_bridges_fresh();
        }
        // Keep the affinity-clustering force in step with the toggle + the current affinity signal
        // (installs / rebuilds / clears once per real change, with a settle so it takes; a no-op when
        // the toggle is off and no force is installed). (Graph signals — P4.)
        self.sync_affinity_force();
        // The gloss size-by-importance encoding reads the importance cache; keep it fresh (the
        // recompute is dirty-gated, so this is cheap when nothing changed). (Graph signals — P6c.)
        if self.gloss_size_by_importance {
            self.recompute_importance();
        }
        // Refresh the revision-gated weighted-edge memo (cache generalization C), so a per-frame
        // gloss redraw reads the collapsed edge list instead of re-deduping every frame. (Query memos.)
        self.refresh_weighted_edges();
        // A dragged node tracks the cursor with zero round-trip: re-pin it locally
        // over whatever the backend just reported (the actor lags a frame behind,
        // and the in-thread snapshot already agrees, so this is a no-op there).
        let dragging = self.drag.is_some_and(|d| d.moved);
        if let Some(d) = self.drag {
            if d.moved {
                self.view
                    .set_position(d.node, self.screen_to_world(self.cursor));
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
        let mut on_screen: HashSet<NodeKey> = self
            .view
            .cull_aabb(self.world_viewport())
            .into_iter()
            .collect();
        // The scope lens (curated canvas): when set, render only the scoped nodes —
        // filter the on-screen DOM set here, project the underlay through a curated
        // arrangement of just the scope (below), and hide non-scoped DOM nodes (the
        // gnode loop). `None` shows the whole graph. (Curated canvas.)
        let scoped: Option<HashSet<NodeKey>> =
            self.scope.as_ref().map(|s| s.iter().copied().collect());
        let fold = self.active_fold_projection();
        let folded_members = fold
            .as_ref()
            .map(|projection| &projection.members)
            .cloned()
            .unwrap_or_default();
        let node_visible = |key: NodeKey| {
            scoped.as_ref().is_none_or(|scope| scope.contains(&key))
                && !folded_members.contains(&key)
        };
        on_screen.retain(|key| node_visible(*key));
        let visible_keys: Vec<NodeKey> = self
            .graph
            .nodes()
            .map(|(key, _)| key)
            .filter(|key| node_visible(*key))
            .collect();

        // Route the underlay through the canvas's forme arrangement — the full
        // read-through Identity arrangement, or a curated arrangement of just the
        // scope. Either way the canvas renders as a Cartography projection of an
        // arrangement (the spine's "two projections of one arrangement"); a scoped
        // arrangement is exactly the shape a stored/compare arrangement would take.
        let arrangement = match (&self.scope, &fold) {
            (None, None) => identity_arrangement(&self.graph),
            _ => crate::underlay::arrangement_of_keys(&self.graph, &visible_keys),
        };
        let mut underlay = canvas_paint_list_demoted_from_arrangement(
            &self.graph,
            &arrangement,
            |k| positions.get(&k).copied(),
            |k| !on_screen.contains(&k),
            // Skip relation cells the user has hidden. Platen still projects one pair-level
            // edge after filtering, so a parallel relation keeps the bundle visible.
            |rel| {
                !self
                    .hidden_edges
                    .contains(&edge_cell_for_relation(rel.from, rel.to, rel.kind))
            },
            // Each node's face radius (node_size / 2) so straight edges trim to the
            // face and demoted underlay rects draw at the node's true size. (Node-rep
            // Decision 5 — size drives the face geometry.)
            |k| Some(self.node_size(k) / 2.0),
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
        underlay.splice_world_overlays(relation_cell_overlay(
            &self.graph,
            &self.view,
            &self.hidden_edges,
            &node_visible,
        ));
        if let Some(fold) = &fold {
            underlay.splice_world_overlays(fold_summary_overlay(
                fold,
                &positions,
                self.style.node_color,
            ));
        }
        // Highlight selected edges by splicing thicker strokes inside the
        // underlay's camera transform (world space — no transform replication).
        if !self.selected_edges.is_empty() {
            underlay.splice_world_overlays(selected_edge_overlay(
                &self.graph,
                &self.view,
                &self.hidden_edges,
                &self.selected_edges,
                &node_visible,
            ));
        }
        // Community rings: a halo per node in its community's colour, spliced into the same
        // world-space transform. (Graph signals — community to a ring.)
        if self.show_community_rings
            && let Some(community) = self.community_cache.as_ref()
        {
            let rings = community_ring_overlay(&self.view, community, |k| self.node_size(k) / 2.0);
            underlay.splice_world_overlays(rings);
        }
        // Bridge rings: a bold ring on the high-betweenness brokers, over the community rings so the
        // connectors stand out. (Graph signals — bridges.)
        if self.show_bridge_rings
            && let Some(bridges) = self.bridge_cache.as_ref()
        {
            let rings = bridge_ring_overlay(&self.view, bridges, |k| self.node_size(k) / 2.0);
            underlay.splice_world_overlays(rings);
        }

        // The node-children layer — the pre-materialized pool. Ensure the
        // incremental layout exists at this viewport, then mutate the `.stage`
        // camera transform and each gnode's transform + selection class. These are
        // attribute-only (paint-tier), so `apply` stays on the RepaintOnly path —
        // no per-frame relayout or DOM rebuild.
        if self.node_layout.is_none() || self.pool_w != w || self.pool_h != h {
            let mut discard = Vec::new();
            self.node_dom.drain_mutations(&mut discard);
            self.node_layout = Some(IncrementalLayout::new(
                &self.node_dom,
                &NODE_SHEET,
                w as f32,
                h as f32,
            ));
            self.pool_w = w;
            self.pool_h = h;
        }
        // The gnode pool is positioned as upright billboards (each gnode carries its
        // own screen-space transform via `Camera::to_screen` below), so the `.stage`
        // container is identity: the camera's foreshorten lives in each node's anchor,
        // not a shear on the whole stage. (Isometric camera P1 — billboards.)
        set_style(
            &mut self.node_dom,
            self.stage_node,
            "transform: translate(0px, 0px) scale(1);",
        );
        // When the host renders these gnodes as chrome DOM elements (canvas-as-element)
        // the in-scene gnode layer is dropped below, so skip the per-gnode transform/class
        // updates: an empty set makes the loop a no-op, the costliest part of the frame on
        // a big graph. (Phase 2 — perf / cleaning.)
        let gnodes: Vec<(NodeKey, DomNodeId)> = if self.render_gnodes_as_dom {
            Vec::new()
        } else {
            self.gnode_of.iter().map(|(&k, &g)| (k, g)).collect()
        };
        for (key, gnode) in gnodes {
            // Scope lens: hide a non-scoped node's DOM child (the underlay already
            // excludes it), so a scoped canvas shows only its subset. (Curated canvas.)
            if !node_visible(key) {
                set_style(&mut self.node_dom, gnode, "display: none;");
                continue;
            }
            let pos = positions.get(&key).copied().unwrap_or_default();
            // Billboard: project the world center to its screen anchor and place the
            // upright gnode centered there, scaled by zoom. At top-down (tilt 1) the
            // anchor is exactly `pos*zoom + offset`, so the placement is unchanged.
            // (Isometric camera P1 — billboards.)
            let (ax, ay) = self.camera.to_screen(pos);
            let z = self.camera.zoom;
            // The face paints at the node's resolved footprint, so every size
            // channel (per-node override, size-by-degree / -importance /
            // -recency) is visible on the gnode itself — not only in the
            // collider, edge trim, and rings. The `.gnode` class keeps 36px as
            // its default; this inline width/height wins per node. `half`
            // generalizes the old `NODE_HALF * z` (identical at the default
            // size) so the billboard stays centred on its anchor.
            // (Projection proofs — P3, the visible size channel.)
            let face = self.node_size(key);
            let half = face * 0.5 * z;
            // P3 fake height: raise the gnode above its ground anchor (a stem, drawn under
            // the gnodes, drops back to the ground where the edges meet). Zero unless
            // height-by-degree is on. (Isometric camera P3 — fake height.)
            let lift = self.node_height(key) * z;
            // Depth-sort front-to-back by the projected ground depth (the post-yaw
            // "north" coordinate): a node lower on the reclined ground paints over one
            // behind it. At top-down this is just `y`, and separated nodes rarely
            // overlap, so it is a harmless no-op there. (Isometric camera P2 — depth.)
            let (s, c) = self.camera.yaw.sin_cos();
            let depth = (pos.x * s + pos.y * c).round() as i32;
            set_style(
                &mut self.node_dom,
                gnode,
                &format!(
                    "transform: translate({}px, {}px) scale({}); z-index: {}; width: {}px; height: {}px;",
                    ax - half,
                    (ay - lift) - half,
                    z,
                    depth,
                    face,
                    face
                ),
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
            // The representation rung changes how the same measured footprint
            // is realized, not how much space it occupies. Glyph suppresses
            // the caption and rounds the body; Card retains the ordinary Canvas
            // face; LivePane receives a framed focus treatment. It remains a
            // face request here, not a claim that interactive content has been
            // embedded. The open representation tail falls back to Card.
            // (Projection proofs — P3b renderer consumption.)
            let representation_class = match self.projection_representation(key) {
                Some(sceno::Representation::Glyph) => " gnode-representation-glyph",
                Some(sceno::Representation::LivePane) => " gnode-representation-live-pane",
                _ => " gnode-representation-card",
            };
            set_class(
                &mut self.node_dom,
                gnode,
                &format!("{state_class}{shape_class}{representation_class}"),
            );
        }
        let mut muts = Vec::new();
        self.node_dom.drain_mutations(&mut muts);
        let applied = self
            .node_layout
            .as_mut()
            .unwrap()
            .apply(&self.node_dom, &NODE_SHEET, &muts);
        if !matches!(applied, Applied::RepaintOnly | Applied::Unchanged) {
            tracing::warn!(
                ?applied,
                "canvas pool: node layout left the RepaintOnly path"
            );
        }
        let scroll = ScrollOffsets::<DomNodeId>::default();
        let nodes_plist =
            self.node_layout
                .as_ref()
                .unwrap()
                .emit_paint_list(&self.node_dom, &scroll, viewport);

        // Favicon layer: a textured quad over each on-screen tile that carries a
        // favicon. This layer is NOT under the `.stage` camera transform (it is a
        // bare command list, not the genet DOM), so the camera is applied here by
        // projecting each corner through `Camera::to_screen` (at the default camera
        // that is `world * zoom + offset`). The favicon's `favicon_rgba` is already
        // the `ImageResource` shape (RGBA8, straight alpha), so the host's existing
        // rasterize uploads it with no GPU plumbing in the canvas. It draws above the
        // colored tile square and below the marquee. (Favicon-on-tile.)
        let mut favicon_cmds: Vec<PaintCmd> = Vec::new();
        let mut favicon_images: Vec<ImageResource> = Vec::new();
        for &key in &on_screen {
            let Some(pos) = positions.get(&key) else {
                continue;
            };
            let Some(node) = self.graph.get_node(key) else {
                continue;
            };
            // The node carries a reference; the pixels come from the
            // host-registered cache. An unresolved reference does not paint.
            let Some(favicon) = node.favicon().copied() else {
                continue;
            };
            let Some((rgba, fav_w, fav_h)) = self.resolved_images.get(&favicon.digest) else {
                self.request_image(favicon);
                continue;
            };
            if rgba.is_empty() || fav_w == 0 || fav_h == 0 {
                continue;
            }
            let img_key = ImageKey::new(IdNamespace(0), favicon_images.len() as u32);
            favicon_images.push(ImageResource {
                key: img_key,
                width: fav_w,
                height: fav_h,
                data: rgba,
            });
            // Billboard the favicon too: an upright screen-space square centered on the
            // node's projected anchor (not two projected corners, which would foreshorten
            // it with the ground). Matches the gnode. (Isometric camera P1.)
            // Inset within the face so the accent frames the icon: state /
            // selection must stay readable at a glance once an icon lands
            // (representations carry node identity).
            let (cx, cy) = self.camera.to_screen(*pos);
            // Inset within the node's *resolved* face, so a resized node carries
            // its icon proportionally (identical at the default size).
            let half = self.node_size(key) * 0.5 * super::FAVICON_INSET * self.camera.zoom;
            let (x0, y0, x1, y1) = (cx - half, cy - half, cx + half, cy + half);
            favicon_cmds.push(PaintCmd::DrawImage(ImageItem {
                placement: CommonPlacement::new(LayoutRect::new(
                    LayoutPoint::new(x0, y0),
                    LayoutPoint::new(x1, y1),
                )),
                image_key: img_key,
                image_rendering: ImageRendering::Auto,
                alpha_type: AlphaType::Alpha,
                color: ColorF {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
            }));
        }

        // P3 fake height: a stem from each raised node's ground anchor up to its floating
        // gnode, so the gnode reads as standing above its ground spot (where its edges meet).
        // Composited before the gnode layer, so stems sit under the gnodes. Zero-height nodes
        // contribute nothing, so this is empty until height-by-degree is on. (Isometric camera P3.)
        let mut stem_cmds: Vec<PaintCmd> = Vec::new();
        for &key in &on_screen {
            let Some(pos) = positions.get(&key) else {
                continue;
            };
            let lift = self.node_height(key) * self.camera.zoom;
            if lift < 0.5 {
                continue;
            }
            let (gx, gy) = self.camera.to_screen(*pos);
            stem_cmds.push(PaintCmd::DrawRect(RectItem {
                placement: CommonPlacement::new(LayoutRect::new(
                    LayoutPoint::new(gx - 1.0, gy - lift),
                    LayoutPoint::new(gx + 1.0, gy),
                )),
                color: ColorF {
                    r: 0.5,
                    g: 0.55,
                    b: 0.66,
                    a: 0.5,
                },
            }));
        }

        // A screen-space layer for the marquee rubber-band, when active.
        let marquee_cmds = self
            .marquee
            .map(|origin| marquee_rect_cmds(origin, self.cursor));
        // The canvas's own opaque backdrop is the bottom layer (so the surface is
        // dark without depending on the host clear color); then the living-backdrop
        // scene orbs, then the underlay edges + demoted rects, then the on-screen node
        // DOM, then any marquee on top.
        let bg_cmds = background_cmds(w, h, self.backdrop);
        // Ambient backdrop: the sim painted as the bottom layer (above the bg fill, below the scene),
        // in its tincture, stretched across the viewport. The sim owns its look (GoL = run-merged
        // cell rects; a continuous sim = dots). (Physics scenes P5.)
        let ambient_cmds: Vec<PaintCmd> = self
            .ambient
            .as_ref()
            .map(|sim| sim.paint(w as f32, h as f32, self.ambient_tincture))
            .unwrap_or_default();
        // Living backdrop: drifting scene-decoration bodies as soft orbs behind the graph
        // (under the edges), projected through the camera so they recline with the iso
        // ground. (Physics scenes P1.)
        let mut scene_cmds: Vec<PaintCmd> = Vec::new();
        // Textured scene props (an opt-in sprite handle that resolves in the registry) billboard a
        // quad in their own layer above the abstract scene; the orb / polygon paints the rest.
        // (Scene-prop sprites.)
        let mut scene_sprite_cmds: Vec<PaintCmd> = Vec::new();
        let mut scene_sprite_images: Vec<ImageResource> = Vec::new();
        for body in self.view.scene_bodies() {
            // A prop wearing a registered sprite paints as a textured billboard over its footprint
            // (sized to its collider half-extent), oriented to the prop's rotation - upright when it
            // rests, tumbling when it tumbles - via a PushTransform spinning the quad about the prop's
            // projected anchor (identity at rest, so a resting prop is unrotated). Skips the abstract
            // paint; an unregistered handle falls through. (Scene-prop sprites - iso billboard.)
            if let Some(handle) = &body.sprite {
                if let Some((rgba, iw, ih)) = self.scene_sprite_textures.get(handle) {
                    if !rgba.is_empty() && *iw > 0 && *ih > 0 {
                        let (cx, cy) = self
                            .camera
                            .to_screen(PortablePoint::new(body.position.x, body.position.y));
                        let r = (scene_body_half(&body.collider) * self.camera.zoom).max(1.0);
                        let img_key =
                            ImageKey::new(IdNamespace(2), scene_sprite_images.len() as u32);
                        scene_sprite_images.push(ImageResource {
                            key: img_key,
                            width: *iw,
                            height: *ih,
                            data: rgba.clone(),
                        });
                        // Push a frame centred on the prop, rotated by its angle; draw the quad in
                        // that local frame (centred on the origin) so it spins with the prop.
                        scene_sprite_cmds.push(PaintCmd::PushTransform(TransformSpec {
                            origin: LayoutPoint::new(cx, cy),
                            transform: LayoutTransform::rotation(
                                0.0,
                                0.0,
                                1.0,
                                euclid::Angle::radians(body.rotation),
                            ),
                            kind: TransformKind::Standard,
                        }));
                        scene_sprite_cmds.push(PaintCmd::DrawImage(ImageItem {
                            placement: CommonPlacement::new(LayoutRect::new(
                                LayoutPoint::new(-r, -r),
                                LayoutPoint::new(r, r),
                            )),
                            image_key: img_key,
                            image_rendering: ImageRendering::Auto,
                            alpha_type: AlphaType::Alpha,
                            color: ColorF {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: 1.0,
                            },
                        }));
                        scene_sprite_cmds.push(PaintCmd::PopTransform);
                        continue;
                    }
                }
            }
            // A ball (or a degenerate hull) paints as a soft radial-gradient orb — the calm
            // backdrop look. A square / rounded-square / hull paints as a filled polygon of its
            // (rotated) corners, projected per-corner through the camera so the shape reclines
            // with the iso ground and shows the body's true orientation. (Physics scenes P4b.)
            let polygon: Option<Vec<(f32, f32)>> = match &body.collider {
                NodeCollider::Ball { .. } => None,
                NodeCollider::Square { half } | NodeCollider::RoundedSquare { half, .. } => {
                    Some(vec![
                        (-half, -half),
                        (*half, -half),
                        (*half, *half),
                        (-half, *half),
                    ])
                }
                NodeCollider::Hull { points, .. } if points.len() >= 3 => Some(points.clone()),
                NodeCollider::Hull { .. } => None,
            };
            match polygon {
                Some(corners) => {
                    let (s, c) = body.rotation.sin_cos();
                    let mut commands = Vec::with_capacity(corners.len() + 1);
                    let (mut min_x, mut min_y, mut max_x, mut max_y) =
                        (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                    for (i, &(lx, ly)) in corners.iter().enumerate() {
                        let wx = body.position.x + (lx * c - ly * s);
                        let wy = body.position.y + (lx * s + ly * c);
                        let (sx, sy) = self.camera.to_screen(PortablePoint::new(wx, wy));
                        min_x = min_x.min(sx);
                        min_y = min_y.min(sy);
                        max_x = max_x.max(sx);
                        max_y = max_y.max(sy);
                        let p = LayoutPoint::new(sx, sy);
                        commands.push(if i == 0 {
                            PathCommand::MoveTo(p)
                        } else {
                            PathCommand::LineTo(p)
                        });
                    }
                    commands.push(PathCommand::Close);
                    scene_cmds.push(PaintCmd::DrawPath(PathItem {
                        placement: CommonPlacement::new(LayoutRect::new(
                            LayoutPoint::new(min_x, min_y),
                            LayoutPoint::new(max_x, max_y),
                        )),
                        path: PathData { commands },
                        // A softened fill (so the big floor slabs no longer read as solid walls) plus
                        // a lighter lit edge, so a block / plank / domino has definition and a touch
                        // of dimension instead of blurring into the backdrop. (Scene polish.)
                        fill: Some(ColorF::new(0.42, 0.55, 0.85, 0.40)),
                        stroke: Some(StrokeStyle {
                            color: ColorF::new(0.64, 0.75, 0.98, 0.7),
                            width: 1.5,
                            cap: StrokeCap::Round,
                            join: StrokeJoin::Round,
                            dash: None,
                        }),
                    }));
                }
                None => {
                    let radius = match &body.collider {
                        NodeCollider::Ball { radius } => *radius,
                        NodeCollider::Hull { fallback, .. } => *fallback,
                        _ => continue,
                    };
                    let (cx, cy) = self
                        .camera
                        .to_screen(PortablePoint::new(body.position.x, body.position.y));
                    let r = (radius * self.camera.zoom).max(1.0);
                    let rect = LayoutRect::new(
                        LayoutPoint::new(cx - r, cy - r),
                        LayoutPoint::new(cx + r, cy + r),
                    );
                    scene_cmds.push(PaintCmd::DrawRadialGradient(RadialGradientItem {
                        placement: CommonPlacement::new(rect),
                        gradient: RadialGradientPayload {
                            center: LayoutPoint::new(cx, cy),
                            radius: LayoutSize::new(r, r),
                            extend_mode: ExtendMode::Clamp,
                            // A brighter core fading through a mid-stop to transparent, so a ball reads
                            // as a rounder, more present orb than a flat low-alpha disc. (Scene polish.)
                            stops: vec![
                                GradientStop {
                                    offset: 0.0,
                                    color: ColorF::new(0.54, 0.66, 0.94, 0.32),
                                },
                                GradientStop {
                                    offset: 0.55,
                                    color: ColorF::new(0.44, 0.57, 0.87, 0.18),
                                },
                                GradientStop {
                                    offset: 1.0,
                                    color: ColorF::new(0.42, 0.55, 0.85, 0.0),
                                },
                            ],
                        },
                        tile_size: LayoutSize::new(2.0 * r, 2.0 * r),
                        tile_spacing: LayoutSize::zero(),
                    }));
                }
            }
        }

        // Liquid pool: each PBF particle a soft watery orb; overlapping soft-alpha gradients read as
        // a connected blob (a true iso-surface threshold is later polish). Painted above the backdrop
        // scene, below the graph, and projected through the camera so it reclines with the iso ground.
        // (Physics scenes P4c.)
        let mut fluid_cmds: Vec<PaintCmd> = Vec::new();
        let fr = self.view.fluid_radius();
        for p in self.view.fluid_particles() {
            let (cx, cy) = self.camera.to_screen(PortablePoint::new(p.x, p.y));
            let r = (fr * 2.2 * self.camera.zoom).max(1.5);
            let rect = LayoutRect::new(
                LayoutPoint::new(cx - r, cy - r),
                LayoutPoint::new(cx + r, cy + r),
            );
            fluid_cmds.push(PaintCmd::DrawRadialGradient(RadialGradientItem {
                placement: CommonPlacement::new(rect),
                gradient: RadialGradientPayload {
                    center: LayoutPoint::new(cx, cy),
                    radius: LayoutSize::new(r, r),
                    extend_mode: ExtendMode::Clamp,
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: ColorF::new(0.30, 0.62, 0.95, 0.5),
                        },
                        GradientStop {
                            offset: 1.0,
                            color: ColorF::new(0.30, 0.62, 0.95, 0.0),
                        },
                    ],
                },
                tile_size: LayoutSize::new(2.0 * r, 2.0 * r),
                tile_spacing: LayoutSize::zero(),
            }));
        }

        let mut layers = vec![CompositeLayer::commands_only(&bg_cmds)];
        if !ambient_cmds.is_empty() {
            layers.push(CompositeLayer::commands_only(&ambient_cmds));
        }
        if !scene_cmds.is_empty() {
            layers.push(CompositeLayer::commands_only(&scene_cmds));
        }
        if !scene_sprite_cmds.is_empty() {
            layers.push(CompositeLayer {
                commands: &scene_sprite_cmds,
                fonts: &[],
                images: &scene_sprite_images,
            });
        }
        if !fluid_cmds.is_empty() {
            layers.push(CompositeLayer::commands_only(&fluid_cmds));
        }
        layers.push(CompositeLayer::commands_only(underlay.commands()));
        // The on-screen gnode + favicon layers, unless the host renders these gnodes as
        // chrome DOM elements instead (canvas-as-element); then only edges + demoted
        // dots remain as the underlay. (Canvas-as-element — Phase 2.)
        if !self.render_gnodes_as_dom {
            // Height stems under the gnodes (P3): the floating gnode paints over its stem.
            if !stem_cmds.is_empty() {
                layers.push(CompositeLayer::commands_only(&stem_cmds));
            }
            layers.push(CompositeLayer {
                commands: nodes_plist.commands(),
                fonts: nodes_plist.fonts(),
                images: nodes_plist.images(),
            });
            if !favicon_cmds.is_empty() {
                layers.push(CompositeLayer {
                    commands: &favicon_cmds,
                    fonts: &[],
                    images: &favicon_images,
                });
            }
        }
        if let Some(cmds) = marquee_cmds.as_ref() {
            layers.push(CompositeLayer::commands_only(cmds));
        }
        let scene = composite_paint_layers(viewport, &layers).scene;

        let needs_redraw = settling || gliding || dragging || self.ambient.is_some();
        (scene, needs_redraw)
    }
}

/// The half-extent (world units) of a scene prop's collider, for sizing a sprite billboard over its
/// footprint. A hull takes the largest corner offset (bounding its outline). (Scene-prop sprites.)
fn scene_body_half(collider: &NodeCollider) -> f32 {
    match collider {
        NodeCollider::Ball { radius } => *radius,
        NodeCollider::Square { half } | NodeCollider::RoundedSquare { half, .. } => *half,
        NodeCollider::Hull { points, fallback } => points
            .iter()
            .map(|&(x, y)| x.abs().max(y.abs()))
            .fold(*fallback, f32::max),
    }
}

#[cfg(test)]
mod tests {
    use crate::Canvas;
    use euclid::default::Point2D;
    use kernel::graph::Graph;
    use kernel::graph::fixtures::GraphFixtures;

    fn graph_with_one_node(url: &str) -> (Graph, kernel::graph::NodeKey) {
        let mut graph = Graph::new();
        let key = graph.add_node_with_id(
            Graph::node_namespace_id(url),
            url.to_string(),
            Point2D::zero(),
        );
        (graph, key)
    }

    fn image_op_count(scene: &netrender::Scene) -> usize {
        scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::Image(_)))
            .count()
    }

    /// A node carrying favicon RGBA emits an image op over its on-screen tile, so the
    /// host rasterizes a real favicon on the square. (Favicon-on-tile.)
    #[test]
    fn favicon_node_emits_an_image_op() {
        let (mut graph, key) = graph_with_one_node("https://ex.test/");
        // The node carries a reference; the host registers the decoded pixels.
        let favicon = kernel::types::ImageRef::new([42u8; 32], 2, 2);
        assert!(graph.set_node_image(key, kernel::types::ImageRole::Favicon, favicon));
        let mut canvas = Canvas::with_graph(graph);
        canvas.register_resolved_image([42u8; 32], vec![255u8; 2 * 2 * 4], 2, 2);
        let (scene, _) = canvas.frame(800, 600);
        assert!(
            image_op_count(&scene) >= 1,
            "a favicon node emits at least one image op"
        );
    }

    #[test]
    fn visible_cache_miss_requests_the_image_once() {
        let (mut graph, key) = graph_with_one_node("https://ex.test/");
        let favicon = kernel::types::ImageRef::new([42u8; 32], 2, 2);
        assert!(graph.set_node_image(key, kernel::types::ImageRole::Favicon, favicon));
        let mut canvas = Canvas::with_graph(graph);

        let (scene, _) = canvas.frame(800, 600);
        assert_eq!(
            image_op_count(&scene),
            0,
            "the cold frame has no pixels yet"
        );
        assert_eq!(canvas.take_image_requests(), vec![favicon]);

        let _ = canvas.frame(800, 600);
        assert!(
            canvas.take_image_requests().is_empty(),
            "an unresolved blob is not requested every frame"
        );
    }

    /// Without a favicon, no image op is emitted (the tile is just a colored square).
    #[test]
    fn node_without_favicon_emits_no_image_op() {
        let (graph, _key) = graph_with_one_node("https://ex.test/");
        let mut canvas = Canvas::with_graph(graph);
        let (scene, _) = canvas.frame(800, 600);
        assert_eq!(image_op_count(&scene), 0, "no favicon -> no image op");
    }

    /// A scene prop carrying a *registered* sprite handle paints as a textured billboard, so the
    /// frame emits an image op over the prop. (Scene-prop sprites.)
    #[test]
    fn registered_sprite_scene_prop_emits_an_image_op() {
        let (graph, _key) = graph_with_one_node("https://ex.test/");
        let mut canvas = Canvas::with_graph(graph);
        canvas.register_scene_sprite("crate", vec![255u8; 4 * 4 * 4], 4, 4);
        let spec = seiche::SceneSpec {
            bodies: vec![
                seiche::SceneBodySpec::dynamic(
                    seiche::NodeCollider::Square { half: 30.0 },
                    (0.0, 0.0),
                )
                .sprite("crate"),
            ],
            gravity: (0.0, 0.0),
            default_tangible: false,
            perpetual: false,
            joints: Vec::new(),
        };
        canvas.load_scene(spec);
        let (scene, _) = canvas.frame(800, 600);
        assert!(
            image_op_count(&scene) >= 1,
            "a registered sprite prop emits an image op"
        );
    }

    /// A scene prop whose sprite handle is *not* registered falls back to the abstract polygon, so
    /// no image op is emitted. (Scene-prop sprites.)
    #[test]
    fn unregistered_sprite_scene_prop_emits_no_image_op() {
        let (graph, _key) = graph_with_one_node("https://ex.test/");
        let mut canvas = Canvas::with_graph(graph);
        let spec = seiche::SceneSpec {
            bodies: vec![
                seiche::SceneBodySpec::dynamic(
                    seiche::NodeCollider::Square { half: 30.0 },
                    (0.0, 0.0),
                )
                .sprite("missing"),
            ],
            gravity: (0.0, 0.0),
            default_tangible: false,
            perpetual: false,
            joints: Vec::new(),
        };
        canvas.load_scene(spec);
        let (scene, _) = canvas.frame(800, 600);
        assert_eq!(
            image_op_count(&scene),
            0,
            "an unregistered handle falls back, no image op"
        );
    }

    /// The Game of Life ambient backdrop keeps the canvas redrawing (so it animates) while loaded,
    /// and parks again once cleared. (Physics scenes P5.)
    #[test]
    fn game_of_life_backdrop_keeps_redrawing_until_cleared() {
        let (graph, _key) = graph_with_one_node("https://ex.test/");
        let mut canvas = Canvas::with_graph(graph);
        // Settle the graph so the only thing that could request a redraw is the ambient sim.
        for _ in 0..400 {
            let _ = canvas.frame(800, 600);
        }
        let (_, settled_redraw) = canvas.frame(800, 600);
        assert!(!settled_redraw, "a settled graph with no ambient sim parks");

        canvas.load_game_of_life();
        let (_, with_gol) = canvas.frame(800, 600);
        assert!(with_gol, "the ambient backdrop keeps the canvas redrawing");

        canvas.clear_ambient();
        for _ in 0..400 {
            let _ = canvas.frame(800, 600);
        }
        let (_, after_clear) = canvas.frame(800, 600);
        assert!(
            !after_clear,
            "clearing the ambient backdrop lets the canvas park again"
        );
    }
}
