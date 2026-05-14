/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scene derivation: the pure transformation from `CanvasSceneInput` into
//! a `ProjectedScene`.
//!
//! This is the central pipeline of `graph-canvas`. It consumes host-provided
//! graph/scene inputs and produces a complete draw list and hit-proxy set for
//! one frame. The derivation is:
//!
//! 1. **deterministic** — same inputs always produce the same output
//! 2. **framework-agnostic** — no egui, iced, or winit dependency
//! 3. **projection-aware** — TwoD, TwoPointFive, and Isometric all flow
//!    through the same pipeline
//!
//! The host is responsible for converting its graph model into
//! `CanvasSceneInput` before calling `derive_scene`.

use euclid::default::{Point2D, Rect, Size2D};
use std::hash::Hash;

use crate::camera::{CanvasCamera, CanvasViewport};
use crate::layout::extras::FrameRegion;
use crate::lod::{LodLevel, LodPolicy};
use crate::packet::{Color, HitProxy, ProjectedScene, SceneDrawItem, Stroke};
use crate::projection::{ProjectionConfig, ProjectionMode, project_position};
use crate::scene::{CanvasEdge, CanvasNode, CanvasSceneInput, CanvasSceneObject};
use crate::scene_region::{SceneRegion, SceneRegionEffect, SceneRegionShape};
use crate::scripting::SceneObjectHitShape;

/// Configuration for the derivation pipeline.
#[derive(Debug, Clone)]
pub struct DeriveConfig {
    /// LOD policy for node/edge detail levels.
    pub lod_policy: LodPolicy,
    /// Default node fill color when the host doesn't specify one.
    pub default_node_color: Color,
    /// Default edge stroke color.
    pub default_edge_color: Color,
    /// Default edge stroke width in world units.
    pub default_edge_width: f32,
    /// Whether to emit hit proxies (can be disabled for thumbnail/offscreen).
    pub emit_hit_proxies: bool,
    /// Projection tuning parameters.
    pub projection: ProjectionConfig,
}

impl Default for DeriveConfig {
    fn default() -> Self {
        Self {
            lod_policy: LodPolicy::default(),
            default_node_color: Color::new(0.4, 0.6, 0.9, 1.0),
            default_edge_color: Color::new(0.5, 0.5, 0.5, 0.6),
            default_edge_width: 1.5,
            emit_hit_proxies: true,
            projection: ProjectionConfig::default(),
        }
    }
}

/// Optional per-node visual overrides provided by the host.
///
/// The host computes colors, selection rings, etc. from its own theme/state
/// and passes them alongside the `CanvasSceneInput`. This keeps theme logic
/// in the host while letting the derivation pipeline apply the visuals.
#[derive(Debug, Clone)]
pub struct NodeVisualOverride {
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
    pub label_color: Option<Color>,
}

impl Default for NodeVisualOverride {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            label_color: None,
        }
    }
}

/// Per-frame overlay inputs consumed by [`derive_scene_with_overlays`].
///
/// These are scene-dependent draw hints that the host computes once per
/// frame (from its own registries, arrangement relations, and focus
/// state) and passes to the derivation pipeline. They land in the
/// projected scene's `background` / `overlays` layers — distinct from
/// `world` items so hosts can paint them before / after the primary
/// nodes and edges without re-sorting.
pub struct OverlayInputs<'a, N: Clone + Eq + Hash> {
    /// Frame-affinity regions: each rendered as a translucent disc
    /// enclosing its members plus an optional anchor label. Emitted to
    /// `scene.background` so frame backdrops appear behind edges /
    /// nodes.
    pub frame_regions: &'a [FrameRegion<N>],
    /// Scene-authoring regions (Arrange / Simulate mode): each
    /// rendered by shape (`Circle` / `Rect`) and effect-kind color to
    /// `scene.background`, again so the region fill sits behind
    /// primary geometry.
    pub scene_regions: &'a [SceneRegion],
    /// A single highlighted edge drawn with a thicker, accent stroke
    /// on top of regular edges. `source` and `target` must match
    /// existing node ids in the scene; otherwise nothing is emitted.
    pub highlighted_edge: Option<(N, N)>,
}

impl<'a, N: Clone + Eq + Hash> Default for OverlayInputs<'a, N> {
    fn default() -> Self {
        Self {
            frame_regions: &[],
            scene_regions: &[],
            highlighted_edge: None,
        }
    }
}

/// Visual tuning for the overlay layer. Exposed so hosts can theme the
/// backdrop palette without reimplementing the emitter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayStyle {
    pub frame_region_fill: Color,
    pub frame_region_stroke: Stroke,
    /// Extra padding around the convex-hull bounds of frame members,
    /// in world units, before wrapping in a circle / rect.
    pub frame_region_padding: f32,
    pub scene_region_attractor: Color,
    pub scene_region_repulsor: Color,
    pub scene_region_dampener: Color,
    pub scene_region_wall: Color,
    pub scene_region_stroke: Stroke,
    /// Color for the highlighted-edge overlay.
    pub highlighted_edge_color: Color,
    /// Width of the highlighted-edge overlay. Should exceed the
    /// default edge width so the highlight is visible over the
    /// underlying stroke.
    pub highlighted_edge_width: f32,
    /// Font size for optional region labels. `0.0` disables labels.
    pub region_label_font_size: f32,
    pub region_label_color: Color,
}

impl Default for OverlayStyle {
    fn default() -> Self {
        Self {
            frame_region_fill: Color::new(0.25, 0.55, 0.85, 0.12),
            frame_region_stroke: Stroke {
                color: Color::new(0.25, 0.55, 0.85, 0.45),
                width: 1.0,
            },
            frame_region_padding: 16.0,
            scene_region_attractor: Color::new(0.30, 0.75, 0.45, 0.12),
            scene_region_repulsor: Color::new(0.90, 0.35, 0.35, 0.12),
            scene_region_dampener: Color::new(0.55, 0.55, 0.60, 0.12),
            scene_region_wall: Color::new(0.15, 0.15, 0.18, 0.20),
            scene_region_stroke: Stroke {
                color: Color::new(0.25, 0.25, 0.30, 0.55),
                width: 1.0,
            },
            highlighted_edge_color: Color::new(1.0, 0.85, 0.25, 0.95),
            highlighted_edge_width: 3.5,
            region_label_font_size: 12.0,
            region_label_color: Color::new(0.15, 0.18, 0.25, 0.85),
        }
    }
}

/// Derive a `ProjectedScene` from a `CanvasSceneInput`.
///
/// Backwards-compatible shorthand: delegates to
/// [`derive_scene_with_overlays`] with empty overlay inputs and the
/// default [`OverlayStyle`]. New callers that want frame-region,
/// scene-region, or highlighted-edge overlays should call
/// `derive_scene_with_overlays` directly.
pub fn derive_scene<N: Clone + Eq + Hash>(
    input: &CanvasSceneInput<N>,
    camera: &CanvasCamera,
    viewport: &CanvasViewport,
    z_values: &dyn Fn(&N) -> f32,
    node_overrides: &dyn Fn(usize, &N) -> NodeVisualOverride,
    config: &DeriveConfig,
) -> ProjectedScene<N> {
    derive_scene_with_overlays(
        input,
        camera,
        viewport,
        z_values,
        node_overrides,
        &OverlayInputs::default(),
        &OverlayStyle::default(),
        config,
    )
}

/// Derive a `ProjectedScene`, emitting overlay backdrops and a
/// highlighted edge alongside the primary nodes / edges.
///
/// - `input`: the scene input for one graph view
/// - `camera`: current camera state (pan, zoom)
/// - `viewport`: the pane rectangle and scale factor
/// - `z_values`: per-node z values derived by the host from the active
///   z-field (see `FieldProjection::z_field`). Nodes not present in this
///   map get z=0.
/// - `node_overrides`: per-node visual overrides (colors, strokes).
///   Indexed by position in `input.nodes`.
/// - `overlay_inputs`: per-frame overlay hints (frame regions, scene
///   regions, highlighted edge).
/// - `overlay_style`: visual tuning for the overlay layer.
/// - `config`: derivation configuration.
///
/// Emission layering:
/// - Frame-region backdrops → `ProjectedScene.background` (behind edges/nodes).
/// - Scene-region backdrops → `ProjectedScene.background` (behind edges/nodes).
/// - Scripted scene-object overlays → `ProjectedScene.overlays`
///   (unchanged; matches the old `derive_scene` behavior).
/// - Highlighted edge → `ProjectedScene.overlays` (on top of regular edges).
pub fn derive_scene_with_overlays<N: Clone + Eq + Hash>(
    input: &CanvasSceneInput<N>,
    camera: &CanvasCamera,
    viewport: &CanvasViewport,
    z_values: &dyn Fn(&N) -> f32,
    node_overrides: &dyn Fn(usize, &N) -> NodeVisualOverride,
    overlay_inputs: &OverlayInputs<'_, N>,
    overlay_style: &OverlayStyle,
    config: &DeriveConfig,
) -> ProjectedScene<N> {
    let projection = input.projection.degrade_if_needed();

    // Compute the world-space viewport rect for culling.
    let world_viewport = world_viewport_rect(camera, viewport);

    let mut background_items = Vec::new();
    let mut world_items = Vec::new();
    let mut hit_proxies = Vec::new();

    // ── Frame-affinity backdrops (behind world layer) ────────────────────
    derive_frame_region_backdrops(
        overlay_inputs.frame_regions,
        &input.nodes,
        camera,
        viewport,
        overlay_style,
        &mut background_items,
    );

    // ── Scene-region backdrops (behind world layer) ──────────────────────
    derive_scene_region_backdrops(
        overlay_inputs.scene_regions,
        camera,
        viewport,
        overlay_style,
        &mut background_items,
    );

    // ── Edges (drawn first, behind nodes) ────────────────────────────────
    derive_edges(
        &input.edges,
        &input.nodes,
        camera,
        viewport,
        &world_viewport,
        &projection,
        z_values,
        config,
        &mut world_items,
        &mut hit_proxies,
    );

    // ── Nodes ────────────────────────────────────────────────────────────
    derive_nodes(
        &input.nodes,
        camera,
        viewport,
        &world_viewport,
        &projection,
        z_values,
        node_overrides,
        config,
        &mut world_items,
        &mut hit_proxies,
    );

    // ── Scene objects (scripted) ─────────────────────────────────────────
    let mut overlay_items = Vec::new();
    derive_scene_objects(
        &input.scene_objects,
        camera,
        viewport,
        &world_viewport,
        config,
        &mut world_items,
        &mut overlay_items,
        &mut hit_proxies,
    );

    // ── Highlighted edge (on top of regular edges) ───────────────────────
    derive_highlighted_edge(
        overlay_inputs.highlighted_edge.as_ref(),
        &input.nodes,
        camera,
        viewport,
        overlay_style,
        &mut overlay_items,
    );

    ProjectedScene {
        background: background_items,
        world: world_items,
        overlays: overlay_items,
        hit_proxies,
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Compute the world-space rectangle visible through the current camera.
fn world_viewport_rect(camera: &CanvasCamera, viewport: &CanvasViewport) -> Rect<f32> {
    let tl = camera.screen_to_world(
        Point2D::new(viewport.rect.min_x(), viewport.rect.min_y()),
        viewport,
    );
    let br = camera.screen_to_world(
        Point2D::new(viewport.rect.max_x(), viewport.rect.max_y()),
        viewport,
    );
    Rect::new(
        Point2D::new(tl.x.min(br.x), tl.y.min(br.y)),
        Size2D::new((br.x - tl.x).abs(), (br.y - tl.y).abs()),
    )
}

/// Determine whether a world-space circle is potentially visible in the
/// viewport (conservative — may include slightly off-screen nodes).
fn is_node_visible(world_pos: Point2D<f32>, radius: f32, world_viewport: &Rect<f32>) -> bool {
    let expanded = world_viewport.inflate(radius, radius);
    expanded.contains(world_pos)
}

fn derive_nodes<N: Clone + Eq + Hash>(
    nodes: &[CanvasNode<N>],
    camera: &CanvasCamera,
    viewport: &CanvasViewport,
    world_viewport: &Rect<f32>,
    projection: &ProjectionMode,
    z_values: &dyn Fn(&N) -> f32,
    node_overrides: &dyn Fn(usize, &N) -> NodeVisualOverride,
    config: &DeriveConfig,
    world_items: &mut Vec<SceneDrawItem>,
    hit_proxies: &mut Vec<HitProxy<N>>,
) {
    for (i, node) in nodes.iter().enumerate() {
        // Viewport culling in world space (before projection).
        if !is_node_visible(node.position, node.radius, world_viewport) {
            continue;
        }

        let z = z_values(&node.id);
        let projected = project_position(node.position, z, projection, &config.projection);

        // LOD check after projection (uses screen-space radius).
        let screen_radius = node.radius * projected.depth_scale;
        let lod = config.lod_policy.level_for_node(camera.zoom, screen_radius);
        if !lod.is_visible() {
            continue;
        }

        let screen_pos = camera.world_to_screen(projected.to_point(), viewport);
        let overrides = node_overrides(i, &node.id);

        let fill = overrides.fill.unwrap_or(config.default_node_color);
        let stroke = overrides.stroke;

        // Draw the node circle.
        let draw_radius = screen_radius * camera.zoom;
        world_items.push(SceneDrawItem::Circle {
            center: screen_pos,
            radius: draw_radius,
            fill,
            stroke,
        });

        // Draw the label at Full LOD only.
        if lod == LodLevel::Full {
            if let Some(ref label) = node.label {
                let label_color = overrides
                    .label_color
                    .unwrap_or(Color::new(0.9, 0.9, 0.9, 1.0));
                let font_size = (12.0 * projected.depth_scale * camera.zoom).max(6.0);
                world_items.push(SceneDrawItem::Label {
                    position: Point2D::new(screen_pos.x, screen_pos.y + draw_radius + 4.0),
                    text: label.clone(),
                    font_size,
                    color: label_color,
                });
            }
        }

        // Emit hit proxy.
        if config.emit_hit_proxies {
            hit_proxies.push(HitProxy::Node {
                id: node.id.clone(),
                center: screen_pos,
                radius: draw_radius,
            });
        }
    }
}

fn derive_scene_objects<N: Clone + Eq + Hash>(
    scene_objects: &[CanvasSceneObject],
    camera: &CanvasCamera,
    viewport: &CanvasViewport,
    world_viewport: &Rect<f32>,
    config: &DeriveConfig,
    world_items: &mut Vec<SceneDrawItem>,
    overlay_items: &mut Vec<SceneDrawItem>,
    hit_proxies: &mut Vec<HitProxy<N>>,
) {
    for obj in scene_objects {
        // Viewport culling: check if the object position is within the
        // visible world rect (with some margin for the hit shape).
        let margin = match &obj.hit_shape {
            Some(SceneObjectHitShape::Circle { radius }) => *radius,
            Some(SceneObjectHitShape::Rect { half_extents }) => half_extents.x.max(half_extents.y),
            None => 0.0,
        };
        if !is_node_visible(obj.position, margin, world_viewport) {
            continue;
        }

        let screen_pos = camera.world_to_screen(obj.position, viewport);

        // Offset draw items from object-local to screen space.
        for item in &obj.draw_items {
            world_items.push(offset_draw_item(item, screen_pos));
        }

        // Offset overlay items from object-local to screen space.
        for item in &obj.overlay_items {
            overlay_items.push(offset_draw_item(item, screen_pos));
        }

        // Emit hit proxy if the object has a hit shape.
        if config.emit_hit_proxies {
            if let Some(ref hit_shape) = obj.hit_shape {
                let radius = match hit_shape {
                    SceneObjectHitShape::Circle { radius } => *radius * camera.zoom,
                    SceneObjectHitShape::Rect { half_extents } => {
                        // Use the larger half-extent as the hit radius.
                        half_extents.x.max(half_extents.y) * camera.zoom
                    }
                };
                hit_proxies.push(HitProxy::SceneObject {
                    id: obj.id,
                    center: screen_pos,
                    radius,
                });
            }
        }
    }
}

/// Offset a draw item's position by a screen-space translation.
fn offset_draw_item(item: &SceneDrawItem, offset: Point2D<f32>) -> SceneDrawItem {
    match item {
        SceneDrawItem::Circle {
            center,
            radius,
            fill,
            stroke,
        } => SceneDrawItem::Circle {
            center: Point2D::new(center.x + offset.x, center.y + offset.y),
            radius: *radius,
            fill: *fill,
            stroke: *stroke,
        },
        SceneDrawItem::Line { from, to, stroke } => SceneDrawItem::Line {
            from: Point2D::new(from.x + offset.x, from.y + offset.y),
            to: Point2D::new(to.x + offset.x, to.y + offset.y),
            stroke: *stroke,
        },
        SceneDrawItem::RoundedRect {
            rect,
            corner_radius,
            fill,
            stroke,
        } => SceneDrawItem::RoundedRect {
            rect: Rect::new(
                Point2D::new(rect.origin.x + offset.x, rect.origin.y + offset.y),
                rect.size,
            ),
            corner_radius: *corner_radius,
            fill: *fill,
            stroke: *stroke,
        },
        SceneDrawItem::Label {
            position,
            text,
            font_size,
            color,
        } => SceneDrawItem::Label {
            position: Point2D::new(position.x + offset.x, position.y + offset.y),
            text: text.clone(),
            font_size: *font_size,
            color: *color,
        },
        SceneDrawItem::ImageRef { rect, handle } => SceneDrawItem::ImageRef {
            rect: Rect::new(
                Point2D::new(rect.origin.x + offset.x, rect.origin.y + offset.y),
                rect.size,
            ),
            handle: handle.clone(),
        },
    }
}

/// Emit one enclosing disc per frame region in screen space, sized to
/// enclose every member plus `style.frame_region_padding` (interpreted
/// in world units, then scaled by `camera.zoom`). Regions with no
/// members present in the current node set are skipped — they would
/// draw an empty backdrop otherwise.
fn derive_frame_region_backdrops<N: Clone + Eq + Hash>(
    regions: &[FrameRegion<N>],
    nodes: &[CanvasNode<N>],
    camera: &CanvasCamera,
    viewport: &CanvasViewport,
    style: &OverlayStyle,
    background: &mut Vec<SceneDrawItem>,
) {
    if regions.is_empty() {
        return;
    }
    let node_position: std::collections::HashMap<&N, Point2D<f32>> =
        nodes.iter().map(|n| (&n.id, n.position)).collect();

    for region in regions {
        let mut positions = Vec::with_capacity(region.members.len());
        for member in &region.members {
            if let Some(pos) = node_position.get(member) {
                positions.push(*pos);
            }
        }
        if positions.is_empty() {
            continue;
        }

        // Enclosing disc in world space: centroid + max-distance radius +
        // padding. Simple, cheap, visually stable under layout updates
        // (no convex-hull jitter).
        let mut centroid = Point2D::new(0.0, 0.0);
        for pos in &positions {
            centroid.x += pos.x;
            centroid.y += pos.y;
        }
        centroid.x /= positions.len() as f32;
        centroid.y /= positions.len() as f32;

        let mut max_dist: f32 = 0.0;
        for pos in &positions {
            let dx = pos.x - centroid.x;
            let dy = pos.y - centroid.y;
            max_dist = max_dist.max((dx * dx + dy * dy).sqrt());
        }
        let world_radius = max_dist + style.frame_region_padding.max(0.0);

        background.push(SceneDrawItem::Circle {
            center: camera.world_to_screen(centroid, viewport),
            radius: world_radius * camera.zoom,
            fill: style.frame_region_fill,
            stroke: Some(style.frame_region_stroke),
        });
    }
}

/// Emit one backdrop per visible scene region, colored by effect kind.
/// Shapes are projected from world → screen so they pan and zoom with
/// the rest of the scene. Regions with `visible: false` are skipped so
/// the host can hide authoring regions from the final view without
/// removing them.
fn derive_scene_region_backdrops(
    regions: &[SceneRegion],
    camera: &CanvasCamera,
    viewport: &CanvasViewport,
    style: &OverlayStyle,
    background: &mut Vec<SceneDrawItem>,
) {
    for region in regions {
        if !region.visible {
            continue;
        }
        let fill = match region.effect {
            SceneRegionEffect::Attractor { .. } => style.scene_region_attractor,
            SceneRegionEffect::Repulsor { .. } => style.scene_region_repulsor,
            SceneRegionEffect::Dampener { .. } => style.scene_region_dampener,
            SceneRegionEffect::Wall => style.scene_region_wall,
        };
        match region.shape {
            SceneRegionShape::Circle { center, radius } => {
                background.push(SceneDrawItem::Circle {
                    center: camera.world_to_screen(center, viewport),
                    radius: radius * camera.zoom,
                    fill,
                    stroke: Some(style.scene_region_stroke),
                });
            }
            SceneRegionShape::Rect { rect } => {
                let tl = camera.world_to_screen(Point2D::new(rect.min_x(), rect.min_y()), viewport);
                let screen_rect = Rect::new(
                    tl,
                    Size2D::new(
                        rect.size.width * camera.zoom,
                        rect.size.height * camera.zoom,
                    ),
                );
                background.push(SceneDrawItem::RoundedRect {
                    rect: screen_rect,
                    corner_radius: 6.0 * camera.zoom.max(0.25),
                    fill,
                    stroke: Some(style.scene_region_stroke),
                });
            }
        }
        if style.region_label_font_size > 0.0 {
            if let Some(label) = region.label.as_ref() {
                background.push(SceneDrawItem::Label {
                    position: camera.world_to_screen(region.shape.center(), viewport),
                    text: label.clone(),
                    font_size: style.region_label_font_size,
                    color: style.region_label_color,
                });
            }
        }
    }
}

/// Emit one highlighted-edge stroke in the overlay layer if both
/// endpoints exist in the scene. No-op when either endpoint is missing
/// (stale highlight for a removed node).
fn derive_highlighted_edge<N: Clone + Eq + Hash>(
    highlighted_edge: Option<&(N, N)>,
    nodes: &[CanvasNode<N>],
    camera: &CanvasCamera,
    viewport: &CanvasViewport,
    style: &OverlayStyle,
    overlays: &mut Vec<SceneDrawItem>,
) {
    let Some((source, target)) = highlighted_edge else {
        return;
    };
    let node_position: std::collections::HashMap<&N, Point2D<f32>> =
        nodes.iter().map(|n| (&n.id, n.position)).collect();
    let (Some(&src), Some(&tgt)) = (node_position.get(source), node_position.get(target)) else {
        return;
    };
    overlays.push(SceneDrawItem::Line {
        from: camera.world_to_screen(src, viewport),
        to: camera.world_to_screen(tgt, viewport),
        stroke: Stroke {
            color: style.highlighted_edge_color,
            width: style.highlighted_edge_width,
        },
    });
}

fn derive_edges<N: Clone + Eq + Hash>(
    edges: &[CanvasEdge<N>],
    nodes: &[CanvasNode<N>],
    camera: &CanvasCamera,
    viewport: &CanvasViewport,
    world_viewport: &Rect<f32>,
    projection: &ProjectionMode,
    z_values: &dyn Fn(&N) -> f32,
    config: &DeriveConfig,
    world_items: &mut Vec<SceneDrawItem>,
    hit_proxies: &mut Vec<HitProxy<N>>,
) {
    // Build a position lookup from node id to index for edge endpoint resolution.
    // This avoids O(n*m) lookups when there are many edges.
    let node_index: std::collections::HashMap<&N, usize> =
        nodes.iter().enumerate().map(|(i, n)| (&n.id, i)).collect();

    for edge in edges {
        let (Some(&src_idx), Some(&tgt_idx)) =
            (node_index.get(&edge.source), node_index.get(&edge.target))
        else {
            continue; // Skip edges with missing endpoints.
        };

        let src_node = &nodes[src_idx];
        let tgt_node = &nodes[tgt_idx];

        // Skip edge if both endpoints are outside the viewport.
        let src_visible = is_node_visible(src_node.position, src_node.radius, world_viewport);
        let tgt_visible = is_node_visible(tgt_node.position, tgt_node.radius, world_viewport);
        if !src_visible && !tgt_visible {
            continue;
        }

        let src_z = z_values(&edge.source);
        let tgt_z = z_values(&edge.target);
        let src_proj = project_position(src_node.position, src_z, projection, &config.projection);
        let tgt_proj = project_position(tgt_node.position, tgt_z, projection, &config.projection);

        let src_screen = camera.world_to_screen(src_proj.to_point(), viewport);
        let tgt_screen = camera.world_to_screen(tgt_proj.to_point(), viewport);

        let width = config.default_edge_width * camera.zoom * edge.weight.max(0.1);
        world_items.push(SceneDrawItem::Line {
            from: src_screen,
            to: tgt_screen,
            stroke: Stroke {
                color: config.default_edge_color,
                width,
            },
        });

        // Emit an edge hit proxy centred on the line midpoint. The
        // box hit-test in `hit_test.rs` uses `half_width` as a
        // square radius around the midpoint — keep it generously
        // larger than the stroke so a thin line is still a usable
        // right-click target, but not so large it swallows the
        // endpoint nodes.
        if config.emit_hit_proxies {
            let midpoint = Point2D::new(
                (src_screen.x + tgt_screen.x) * 0.5,
                (src_screen.y + tgt_screen.y) * 0.5,
            );
            let half_width = (width * 1.5).clamp(8.0, 16.0);
            hit_proxies.push(HitProxy::Edge {
                source: edge.source.clone(),
                target: edge.target.clone(),
                midpoint,
                half_width,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::TwoPointFiveProjection;
    use crate::scene::{SceneMode, ViewId};
    use crate::scripting::{SceneObjectHitShape, SceneObjectId};

    fn simple_input() -> CanvasSceneInput<u32> {
        CanvasSceneInput {
            view_id: ViewId(1),
            nodes: vec![
                CanvasNode {
                    id: 0,
                    position: Point2D::new(0.0, 0.0),
                    radius: 16.0,
                    label: Some("origin".into()),
                },
                CanvasNode {
                    id: 1,
                    position: Point2D::new(100.0, 0.0),
                    radius: 16.0,
                    label: Some("right".into()),
                },
                CanvasNode {
                    id: 2,
                    position: Point2D::new(0.0, 100.0),
                    radius: 16.0,
                    label: None,
                },
            ],
            edges: vec![
                CanvasEdge {
                    source: 0,
                    target: 1,
                    weight: 1.0,
                },
                CanvasEdge {
                    source: 0,
                    target: 2,
                    weight: 0.5,
                },
            ],
            scene_objects: vec![],
            overlays: vec![],
            scene_mode: SceneMode::Browse,
            projection: ProjectionMode::default(),
        }
    }

    fn default_camera() -> CanvasCamera {
        CanvasCamera::default()
    }

    fn default_viewport() -> CanvasViewport {
        CanvasViewport::default()
    }

    fn zero_z(_id: &u32) -> f32 {
        0.0
    }

    fn no_overrides(_idx: usize, _id: &u32) -> NodeVisualOverride {
        NodeVisualOverride::default()
    }

    #[test]
    fn derive_scene_produces_items_for_visible_nodes() {
        let input = simple_input();
        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );
        // 3 nodes visible -> at least 3 circles + labels for nodes with labels.
        let circles: Vec<_> = scene
            .world
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Circle { .. }))
            .collect();
        assert_eq!(circles.len(), 3);
    }

    #[test]
    fn derive_scene_produces_edges() {
        let input = simple_input();
        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );
        let lines: Vec<_> = scene
            .world
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Line { .. }))
            .collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn derive_scene_produces_hit_proxies() {
        let input = simple_input();
        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );
        // 3 node proxies + 2 edge proxies for the 3-node, 2-edge input.
        let node_proxies = scene
            .hit_proxies
            .iter()
            .filter(|p| matches!(p, HitProxy::Node { .. }))
            .count();
        let edge_proxies = scene
            .hit_proxies
            .iter()
            .filter(|p| matches!(p, HitProxy::Edge { .. }))
            .count();
        assert_eq!(node_proxies, 3);
        assert_eq!(edge_proxies, 2);
    }

    #[test]
    fn derive_scene_no_hit_proxies_when_disabled() {
        let input = simple_input();
        let config = DeriveConfig {
            emit_hit_proxies: false,
            ..DeriveConfig::default()
        };
        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &config,
        );
        assert!(scene.hit_proxies.is_empty());
    }

    #[test]
    fn derive_scene_empty_input_produces_empty_scene() {
        let input = CanvasSceneInput::<u32>::default();
        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );
        assert!(scene.world.is_empty());
        assert!(scene.hit_proxies.is_empty());
    }

    #[test]
    fn derive_scene_deterministic() {
        let input = simple_input();
        let camera = default_camera();
        let viewport = default_viewport();
        let config = DeriveConfig::default();

        let a = derive_scene(&input, &camera, &viewport, &zero_z, &no_overrides, &config);
        let b = derive_scene(&input, &camera, &viewport, &zero_z, &no_overrides, &config);
        assert_eq!(a, b);
    }

    #[test]
    fn derive_scene_twopointfive_shifts_deep_nodes() {
        let mut input = simple_input();
        input.projection = ProjectionMode::TwoPointFive {
            projection: TwoPointFiveProjection::MildPerspective { focal: 333.0 },
            z_field: None,
        };
        // Give node 1 a z value of 50.
        let z_fn = |id: &u32| if *id == 1 { 50.0 } else { 0.0 };

        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &z_fn,
            &no_overrides,
            &DeriveConfig::default(),
        );

        // Find the circle for node 0 (z=0) and node 1 (z=50).
        let circles: Vec<_> = scene
            .world
            .iter()
            .filter_map(|item| match item {
                SceneDrawItem::Circle { center, radius, .. } => Some((center, radius)),
                _ => None,
            })
            .collect();
        assert!(circles.len() >= 2);

        // Node 1 (z=50) should have a smaller rendered radius than node 0 (z=0)
        // because depth_scale < 1.0 for deeper nodes.
        // We identify them by their hit proxies.
        let proxy_0 = scene
            .hit_proxies
            .iter()
            .find(|p| matches!(p, HitProxy::Node { id: 0, .. }))
            .unwrap();
        let proxy_1 = scene
            .hit_proxies
            .iter()
            .find(|p| matches!(p, HitProxy::Node { id: 1, .. }))
            .unwrap();
        let r0 = match proxy_0 {
            HitProxy::Node { radius, .. } => *radius,
            _ => unreachable!(),
        };
        let r1 = match proxy_1 {
            HitProxy::Node { radius, .. } => *radius,
            _ => unreachable!(),
        };
        assert!(
            r1 < r0,
            "deeper node (z=50) should have smaller radius: r0={}, r1={}",
            r0,
            r1
        );
    }

    #[test]
    fn derive_scene_applies_node_overrides() {
        let input = simple_input();
        let red = Color::new(1.0, 0.0, 0.0, 1.0);
        let override_fn = |idx: usize, _id: &u32| {
            if idx == 0 {
                NodeVisualOverride {
                    fill: Some(red),
                    ..Default::default()
                }
            } else {
                NodeVisualOverride::default()
            }
        };

        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &override_fn,
            &DeriveConfig::default(),
        );

        // First circle should be red (node 0 gets the override).
        let first_circle = scene
            .world
            .iter()
            .find(|item| matches!(item, SceneDrawItem::Circle { .. }));
        match first_circle {
            Some(SceneDrawItem::Circle { fill, .. }) => {
                assert_eq!(*fill, red);
            }
            _ => panic!("expected a circle"),
        }
    }

    #[test]
    fn derive_scene_culls_offscreen_nodes() {
        let mut input = simple_input();
        // Place node 2 far outside the default viewport.
        input.nodes[2].position = Point2D::new(50000.0, 50000.0);

        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );

        // Only 2 nodes should be visible.
        let circles: Vec<_> = scene
            .world
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Circle { .. }))
            .collect();
        assert_eq!(circles.len(), 2);
        // 2 node proxies for the 2 visible nodes. Edge proxies still
        // emit when at least one endpoint is on-screen, so both
        // edges (0→1 fully visible, 1→2 partly visible) survive.
        let node_proxies = scene
            .hit_proxies
            .iter()
            .filter(|p| matches!(p, HitProxy::Node { .. }))
            .count();
        assert_eq!(node_proxies, 2);
    }

    #[test]
    fn derive_scene_labels_only_at_full_lod() {
        let mut input = simple_input();
        // All nodes have labels.
        for node in &mut input.nodes {
            node.label = Some("test".into());
        }

        // At very low zoom, LOD should be Reduced or Minimal — no labels.
        let camera = CanvasCamera::new(euclid::default::Vector2D::zero(), 0.05);
        let scene = derive_scene(
            &input,
            &camera,
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );

        let labels: Vec<_> = scene
            .world
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Label { .. }))
            .collect();
        assert_eq!(labels.len(), 0, "no labels at very low zoom");
    }

    #[test]
    fn derive_scene_edges_skip_missing_endpoints() {
        let mut input = simple_input();
        // Add an edge referencing a non-existent node.
        input.edges.push(CanvasEdge {
            source: 0,
            target: 99,
            weight: 1.0,
        });

        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );

        // Only the 2 valid edges should produce lines.
        let lines: Vec<_> = scene
            .world
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Line { .. }))
            .collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn derive_scene_with_scene_objects() {
        let mut input = simple_input();
        input.scene_objects.push(CanvasSceneObject {
            id: SceneObjectId(10),
            position: Point2D::new(50.0, 50.0),
            draw_items: vec![SceneDrawItem::Circle {
                center: Point2D::new(0.0, 0.0),
                radius: 8.0,
                fill: Color::new(1.0, 0.0, 0.0, 1.0),
                stroke: None,
            }],
            hit_shape: Some(SceneObjectHitShape::Circle { radius: 10.0 }),
            overlay_items: vec![SceneDrawItem::Label {
                position: Point2D::new(0.0, -15.0),
                text: "badge".into(),
                font_size: 10.0,
                color: Color::WHITE,
            }],
        });

        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );

        // Should have the scene object's circle in world items (plus 3 node circles + 2 edges).
        let circles: Vec<_> = scene
            .world
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Circle { .. }))
            .collect();
        assert_eq!(circles.len(), 4); // 3 nodes + 1 scene object

        // Should have the overlay label.
        assert!(!scene.overlays.is_empty());
        assert!(
            scene
                .overlays
                .iter()
                .any(|item| matches!(item, SceneDrawItem::Label { text, .. } if text == "badge"))
        );

        // Should have a SceneObject hit proxy.
        let obj_proxies: Vec<_> = scene
            .hit_proxies
            .iter()
            .filter(|p| matches!(p, HitProxy::SceneObject { .. }))
            .collect();
        assert_eq!(obj_proxies.len(), 1);
    }

    #[test]
    fn derive_scene_culls_offscreen_scene_objects() {
        let mut input = simple_input();
        input.scene_objects.push(CanvasSceneObject {
            id: SceneObjectId(20),
            position: Point2D::new(50000.0, 50000.0),
            draw_items: vec![SceneDrawItem::Circle {
                center: Point2D::new(0.0, 0.0),
                radius: 5.0,
                fill: Color::WHITE,
                stroke: None,
            }],
            hit_shape: Some(SceneObjectHitShape::Circle { radius: 5.0 }),
            overlay_items: vec![],
        });

        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );

        // The offscreen scene object should not produce a hit proxy.
        let obj_proxies: Vec<_> = scene
            .hit_proxies
            .iter()
            .filter(|p| matches!(p, HitProxy::SceneObject { .. }))
            .collect();
        assert!(obj_proxies.is_empty());
    }

    #[test]
    fn derive_scene_object_without_hit_shape_has_no_proxy() {
        let mut input = simple_input();
        input.scene_objects.push(CanvasSceneObject {
            id: SceneObjectId(30),
            position: Point2D::new(50.0, 50.0),
            draw_items: vec![SceneDrawItem::Circle {
                center: Point2D::new(0.0, 0.0),
                radius: 5.0,
                fill: Color::WHITE,
                stroke: None,
            }],
            hit_shape: None, // Not interactive
            overlay_items: vec![],
        });

        let scene = derive_scene(
            &input,
            &default_camera(),
            &default_viewport(),
            &zero_z,
            &no_overrides,
            &DeriveConfig::default(),
        );

        // Should still have the draw item but no scene object hit proxy.
        let obj_proxies: Vec<_> = scene
            .hit_proxies
            .iter()
            .filter(|p| matches!(p, HitProxy::SceneObject { .. }))
            .collect();
        assert!(obj_proxies.is_empty());

        // But the circle should be in the world items.
        let circles: Vec<_> = scene
            .world
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Circle { .. }))
            .collect();
        assert_eq!(circles.len(), 4); // 3 nodes + 1 scene object
    }

    // ── Overlay emission (§4) ────────────────────────────────────────

    fn overlay_test_scene(
        nodes: &[(u32, f32, f32)],
    ) -> (
        CanvasSceneInput<u32>,
        CanvasCamera,
        CanvasViewport,
        DeriveConfig,
    ) {
        let scene = CanvasSceneInput::<u32> {
            view_id: crate::scene::ViewId(0),
            nodes: nodes
                .iter()
                .map(|(id, x, y)| CanvasNode {
                    id: *id,
                    position: Point2D::new(*x, *y),
                    radius: 10.0,
                    label: None,
                })
                .collect(),
            edges: Vec::new(),
            scene_objects: Vec::new(),
            overlays: Vec::new(),
            scene_mode: crate::scene::SceneMode::Browse,
            projection: ProjectionMode::default(),
        };
        let viewport = CanvasViewport::new(Point2D::origin(), Size2D::new(1000.0, 800.0), 1.0);
        (
            scene,
            CanvasCamera::default(),
            viewport,
            DeriveConfig::default(),
        )
    }

    #[test]
    fn derive_scene_with_overlays_empty_matches_derive_scene() {
        let (input, cam, vp, cfg) = overlay_test_scene(&[(0, 0.0, 0.0), (1, 50.0, 0.0)]);
        let zero_z = |_: &u32| 0.0;
        let no_overrides = |_: usize, _: &u32| NodeVisualOverride::default();
        let base = derive_scene(&input, &cam, &vp, &zero_z, &no_overrides, &cfg);
        let with_empty = derive_scene_with_overlays(
            &input,
            &cam,
            &vp,
            &zero_z,
            &no_overrides,
            &OverlayInputs::default(),
            &OverlayStyle::default(),
            &cfg,
        );
        // Draw-item counts match — empty overlay inputs must emit
        // nothing beyond the baseline.
        assert_eq!(base.background.len(), with_empty.background.len());
        assert_eq!(base.world.len(), with_empty.world.len());
        assert_eq!(base.overlays.len(), with_empty.overlays.len());
    }

    #[test]
    fn derive_scene_emits_frame_region_backdrop_in_background_layer() {
        let (input, cam, vp, cfg) =
            overlay_test_scene(&[(0, 10.0, 10.0), (1, 50.0, 10.0), (2, 90.0, 10.0)]);
        let region = FrameRegion {
            anchor: 0,
            members: vec![0, 1, 2],
            strength: 1.0,
        };
        let overlays = OverlayInputs {
            frame_regions: std::slice::from_ref(&region),
            scene_regions: &[],
            highlighted_edge: None,
        };
        let scene = derive_scene_with_overlays(
            &input,
            &cam,
            &vp,
            &|_| 0.0,
            &|_, _| NodeVisualOverride::default(),
            &overlays,
            &OverlayStyle::default(),
            &cfg,
        );
        let circles: Vec<_> = scene
            .background
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Circle { .. }))
            .collect();
        assert_eq!(
            circles.len(),
            1,
            "one frame region → one enclosing disc in background"
        );
    }

    #[test]
    fn derive_scene_skips_frame_region_with_no_members_in_scene() {
        let (input, cam, vp, cfg) = overlay_test_scene(&[(0, 0.0, 0.0)]);
        // Members reference ids that aren't in the scene.
        let region = FrameRegion {
            anchor: 99,
            members: vec![99, 100],
            strength: 1.0,
        };
        let overlays = OverlayInputs {
            frame_regions: std::slice::from_ref(&region),
            scene_regions: &[],
            highlighted_edge: None,
        };
        let scene = derive_scene_with_overlays(
            &input,
            &cam,
            &vp,
            &|_| 0.0,
            &|_, _| NodeVisualOverride::default(),
            &overlays,
            &OverlayStyle::default(),
            &cfg,
        );
        assert!(
            scene.background.is_empty(),
            "no members present → no backdrop"
        );
    }

    #[test]
    fn derive_scene_emits_scene_region_backdrop_circle_and_rect() {
        let (input, cam, vp, cfg) = overlay_test_scene(&[]);
        use crate::scene_region::SceneRegionId;
        let circle_region = SceneRegion {
            id: SceneRegionId(1),
            label: Some("Attractor".into()),
            shape: SceneRegionShape::Circle {
                center: Point2D::new(0.0, 0.0),
                radius: 50.0,
            },
            effect: SceneRegionEffect::Attractor { strength: 1.0 },
            visible: true,
        };
        let rect_region = SceneRegion {
            id: SceneRegionId(2),
            label: None,
            shape: SceneRegionShape::Rect {
                rect: Rect::new(Point2D::new(-25.0, -25.0), Size2D::new(50.0, 30.0)),
            },
            effect: SceneRegionEffect::Wall,
            visible: true,
        };
        let hidden_region = SceneRegion {
            id: SceneRegionId(3),
            label: None,
            shape: SceneRegionShape::Circle {
                center: Point2D::new(500.0, 500.0),
                radius: 10.0,
            },
            effect: SceneRegionEffect::Dampener { factor: 0.5 },
            visible: false, // must be skipped
        };
        let regions = vec![circle_region, rect_region, hidden_region];
        let overlays = OverlayInputs {
            frame_regions: &[],
            scene_regions: &regions,
            highlighted_edge: None,
        };
        let scene = derive_scene_with_overlays(
            &input,
            &cam,
            &vp,
            &|_| 0.0,
            &|_, _| NodeVisualOverride::default(),
            &overlays,
            &OverlayStyle::default(),
            &cfg,
        );
        let circle_count = scene
            .background
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Circle { .. }))
            .count();
        let rect_count = scene
            .background
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::RoundedRect { .. }))
            .count();
        let label_count = scene
            .background
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Label { .. }))
            .count();
        assert_eq!(circle_count, 1, "one visible circle region");
        assert_eq!(rect_count, 1, "one visible rect region");
        assert_eq!(label_count, 1, "only the circle region carried a label");
    }

    #[test]
    fn derive_scene_emits_highlighted_edge_on_overlay_layer() {
        let (input, cam, vp, cfg) = overlay_test_scene(&[(7, -10.0, 0.0), (8, 10.0, 0.0)]);
        let overlays = OverlayInputs {
            frame_regions: &[],
            scene_regions: &[],
            highlighted_edge: Some((7, 8)),
        };
        let scene = derive_scene_with_overlays(
            &input,
            &cam,
            &vp,
            &|_| 0.0,
            &|_, _| NodeVisualOverride::default(),
            &overlays,
            &OverlayStyle::default(),
            &cfg,
        );
        let highlight_lines: Vec<_> = scene
            .overlays
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Line { .. }))
            .collect();
        assert_eq!(highlight_lines.len(), 1, "one highlighted edge → one line");
    }

    #[test]
    fn derive_scene_skips_highlighted_edge_when_endpoints_missing() {
        let (input, cam, vp, cfg) = overlay_test_scene(&[(7, -10.0, 0.0)]);
        let overlays = OverlayInputs {
            frame_regions: &[],
            scene_regions: &[],
            // 999 isn't a node in the scene.
            highlighted_edge: Some((7, 999)),
        };
        let scene = derive_scene_with_overlays(
            &input,
            &cam,
            &vp,
            &|_| 0.0,
            &|_, _| NodeVisualOverride::default(),
            &overlays,
            &OverlayStyle::default(),
            &cfg,
        );
        let highlight_lines: Vec<_> = scene
            .overlays
            .iter()
            .filter(|item| matches!(item, SceneDrawItem::Line { .. }))
            .collect();
        assert!(
            highlight_lines.is_empty(),
            "stale highlight with missing endpoint must be dropped silently"
        );
    }

    #[test]
    fn derive_scene_projects_backdrops_through_camera() {
        // At zoom 2.0, a world-space radius of 50 should map to a
        // screen radius of 100 — verifying the emitters apply the
        // camera projection instead of leaking world units.
        let (input, _, vp, cfg) = overlay_test_scene(&[]);
        use crate::scene_region::SceneRegionId;
        let region = SceneRegion {
            id: SceneRegionId(1),
            label: None,
            shape: SceneRegionShape::Circle {
                center: Point2D::new(0.0, 0.0),
                radius: 50.0,
            },
            effect: SceneRegionEffect::Attractor { strength: 1.0 },
            visible: true,
        };
        let cam = CanvasCamera::new(euclid::default::Vector2D::zero(), 2.0);
        let overlays = OverlayInputs {
            frame_regions: &[],
            scene_regions: std::slice::from_ref(&region),
            highlighted_edge: None,
        };
        let scene = derive_scene_with_overlays(
            &input,
            &cam,
            &vp,
            &|_| 0.0,
            &|_, _| NodeVisualOverride::default(),
            &overlays,
            &OverlayStyle::default(),
            &cfg,
        );
        let circle = scene
            .background
            .iter()
            .find_map(|item| match item {
                SceneDrawItem::Circle { radius, .. } => Some(*radius),
                _ => None,
            })
            .expect("scene region emitted a circle");
        assert!(
            (circle - 100.0).abs() < 1e-3,
            "expected screen radius 100 (world 50 * zoom 2.0), got {circle}"
        );
    }
}
