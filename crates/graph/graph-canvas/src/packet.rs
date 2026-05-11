/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Projected scene packets and draw items.
//!
//! A `ProjectedScene` is the output of scene derivation — the complete set of
//! draw instructions and hit proxies for one frame of one graph view. Backends
//! consume this; they do not define graph semantics.

use euclid::default::{Point2D, Rect};
use serde::{Deserialize, Serialize};

use crate::scripting::SceneObjectId;

// `Color` and `Stroke` moved into mere-kernel's `paint` module
// (2026-05-11) so the kernel no longer depends on graph-canvas — the
// reversed dependency direction unlocks the cartography layer's
// graph-layout extraction (see the cartography brief's revised §9
// step 4). Re-exported here to keep graph-canvas's public API stable
// for downstream consumers.
pub use mere_kernel::paint::{Color, Stroke};

/// A single drawable primitive in screen space.
///
/// These are intentionally simple — enough to represent the current
/// `egui_graphs` rendering vocabulary. Future renderer adapters may extend
/// this or introduce a richer scene graph, but the packet contract remains
/// the backend seam.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneDrawItem {
    /// Filled circle.
    Circle {
        center: Point2D<f32>,
        radius: f32,
        fill: Color,
        stroke: Option<Stroke>,
    },
    /// Line segment.
    Line {
        from: Point2D<f32>,
        to: Point2D<f32>,
        stroke: Stroke,
    },
    /// Filled rounded rectangle.
    RoundedRect {
        rect: Rect<f32>,
        corner_radius: f32,
        fill: Color,
        stroke: Option<Stroke>,
    },
    /// Text label.
    Label {
        position: Point2D<f32>,
        text: String,
        font_size: f32,
        color: Color,
    },
    /// Reference to an image/texture by opaque handle. The backend resolves
    /// the handle to an actual texture.
    ImageRef {
        rect: Rect<f32>,
        handle: ImageHandle,
    },
}

/// Opaque image/texture handle. The host assigns these and the backend
/// resolves them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImageHandle(pub u64);

/// A hit proxy: a clickable/hoverable region associated with a node, edge,
/// or scene object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HitProxy<N> {
    Node {
        id: N,
        center: Point2D<f32>,
        radius: f32,
    },
    Edge {
        source: N,
        target: N,
        midpoint: Point2D<f32>,
        half_width: f32,
    },
    /// A scripted scene object's interaction surface.
    SceneObject {
        id: SceneObjectId,
        center: Point2D<f32>,
        radius: f32,
    },
}

/// The complete projected scene for one graph view, one frame.
///
/// Layers are ordered: `background` is drawn first, then `world` (nodes,
/// edges), then `overlays` (selection rects, labels, debug). Hit proxies
/// are not drawn — they are the interaction surface for hit testing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectedScene<N> {
    pub background: Vec<SceneDrawItem>,
    pub world: Vec<SceneDrawItem>,
    pub overlays: Vec<SceneDrawItem>,
    pub hit_proxies: Vec<HitProxy<N>>,
}

impl<N> Default for ProjectedScene<N> {
    fn default() -> Self {
        Self {
            background: Vec::new(),
            world: Vec::new(),
            overlays: Vec::new(),
            hit_proxies: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::default::Size2D;

    #[test]
    fn color_constants() {
        assert_eq!(Color::BLACK.r, 0.0);
        assert_eq!(Color::WHITE.a, 1.0);
        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }

    #[test]
    fn projected_scene_default_is_empty() {
        let scene: ProjectedScene<u32> = ProjectedScene::default();
        assert!(scene.background.is_empty());
        assert!(scene.world.is_empty());
        assert!(scene.overlays.is_empty());
        assert!(scene.hit_proxies.is_empty());
    }

    #[test]
    fn serde_roundtrip_draw_items() {
        let items = vec![
            SceneDrawItem::Circle {
                center: Point2D::new(10.0, 20.0),
                radius: 8.0,
                fill: Color::new(0.5, 0.3, 0.1, 1.0),
                stroke: Some(Stroke {
                    color: Color::BLACK,
                    width: 1.0,
                }),
            },
            SceneDrawItem::Line {
                from: Point2D::new(0.0, 0.0),
                to: Point2D::new(100.0, 100.0),
                stroke: Stroke {
                    color: Color::new(0.0, 0.0, 1.0, 0.8),
                    width: 2.0,
                },
            },
            SceneDrawItem::RoundedRect {
                rect: Rect::new(Point2D::new(5.0, 5.0), Size2D::new(50.0, 30.0)),
                corner_radius: 4.0,
                fill: Color::WHITE,
                stroke: None,
            },
            SceneDrawItem::Label {
                position: Point2D::new(25.0, 15.0),
                text: "hello".into(),
                font_size: 14.0,
                color: Color::BLACK,
            },
            SceneDrawItem::ImageRef {
                rect: Rect::new(Point2D::origin(), Size2D::new(64.0, 64.0)),
                handle: ImageHandle(42),
            },
        ];
        let json = serde_json::to_string(&items).unwrap();
        let back: Vec<SceneDrawItem> = serde_json::from_str(&json).unwrap();
        assert_eq!(items, back);
    }

    #[test]
    fn serde_roundtrip_hit_proxy_scene_object() {
        let proxy = HitProxy::<u32>::SceneObject {
            id: SceneObjectId(99),
            center: Point2D::new(50.0, 75.0),
            radius: 20.0,
        };
        let json = serde_json::to_string(&proxy).unwrap();
        let back: HitProxy<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(proxy, back);
    }

    #[test]
    fn serde_roundtrip_projected_scene() {
        let scene = ProjectedScene {
            background: vec![SceneDrawItem::RoundedRect {
                rect: Rect::new(Point2D::origin(), Size2D::new(800.0, 600.0)),
                corner_radius: 0.0,
                fill: Color::new(0.1, 0.1, 0.1, 1.0),
                stroke: None,
            }],
            world: vec![SceneDrawItem::Circle {
                center: Point2D::new(100.0, 200.0),
                radius: 16.0,
                fill: Color::new(0.2, 0.6, 0.9, 1.0),
                stroke: None,
            }],
            overlays: vec![],
            hit_proxies: vec![HitProxy::Node {
                id: 0u32,
                center: Point2D::new(100.0, 200.0),
                radius: 16.0,
            }],
        };
        let json = serde_json::to_string(&scene).unwrap();
        let back: ProjectedScene<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(scene, back);
    }
}
