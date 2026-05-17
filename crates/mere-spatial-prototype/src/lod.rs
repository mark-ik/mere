// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! LOD-promotion — the substrate's per-frame resolution of which
//! `LodLevel` a node should render at, given the current camera.
//!
//! Per spatial-chrome IR brief §2.4 ("LOD is per-node") and §5
//! (`lod-promotion` system), the substrate computes effective LOD
//! before dispatch and passes the result to renderers via
//! `SceneNodeRef.lod`. Renderers may use the hint to choose content
//! representation — a `Panel` at `Thumbnail` LOD might render as an
//! icon; at `DeepZoom` it might unfurl into a high-detail editor.
//! Renderers that don't care can ignore the field.
//!
//! ## v0a heuristic
//!
//! Effective LOD is decided by the node's smaller apparent dimension
//! in host pixels — i.e. the minimum of width × camera.x-scale and
//! height × camera.y-scale. Bands:
//!
//! - `< Thumbnail`: too small to render meaningfully; map to
//!   `LodLevel::Thumbnail` (renderer expected to skip or emit icon).
//! - `< Card`: `LodLevel::Thumbnail`.
//! - `< FullPane`: `LodLevel::Card`.
//! - `< DeepZoom`: `LodLevel::FullPane`.
//! - `>= DeepZoom`: `LodLevel::DeepZoom`.
//!
//! Defaults: thumbnail 32, card 128, full_pane 768, deep_zoom 2048
//! (all host-pixels). Hosts wanting different thresholds set them on
//! `SubstrateHost`.
//!
//! Heuristic limits: rotation-only cameras don't change apparent size
//! and stay at the source LOD. Non-uniform scale uses the smaller
//! axis (a panel sheared narrow correctly demotes).

use kurbo::Affine;
use mere_renderer_registry::{LodLevel, SceneNodeRef};

/// Tunable pixel thresholds for LOD promotion. Each value is the
/// minimum host-pixel size at which the corresponding LOD becomes the
/// chosen tier (so `card = 128` means "nodes whose apparent size is
/// at least 128 px render at Card LOD or higher").
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LodThresholds {
    pub thumbnail_min_px: f64,
    pub card_min_px: f64,
    pub full_pane_min_px: f64,
    pub deep_zoom_min_px: f64,
}

impl LodThresholds {
    /// v0a defaults: thumbnail 32, card 128, full pane 768, deep zoom 2048.
    pub const DEFAULT: Self = Self {
        thumbnail_min_px: 32.0,
        card_min_px: 128.0,
        full_pane_min_px: 768.0,
        deep_zoom_min_px: 2048.0,
    };
}

impl Default for LodThresholds {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Compute the effective LOD for `node` under `camera`, using
/// `thresholds`. Returns the node's stored LOD as a floor — promotion
/// can only go to the same level or higher (a node tagged `DeepZoom`
/// never demotes to `Card` even if zoomed out, since the source
/// content is presumed to mandate that level).
pub fn compute_lod_for_node(
    node: &SceneNodeRef,
    camera: Affine,
    thresholds: &LodThresholds,
) -> LodLevel {
    let apparent = apparent_min_pixels(node, camera);
    let band = band_for(apparent, thresholds);
    max_lod(node.lod, band)
}

/// Smaller of width / height of the node's bounding rect after
/// `camera * placement`. Conservative — uses the smaller axis so
/// non-uniform scale demotes correctly.
fn apparent_min_pixels(node: &SceneNodeRef, camera: Affine) -> f64 {
    let effective = camera * node.placement.transform;
    // Transform the corners of the local tile rect; take the AABB.
    let w = node.size.width;
    let h = node.size.height;
    let corners = [
        effective * kurbo::Point::new(0.0, 0.0),
        effective * kurbo::Point::new(w, 0.0),
        effective * kurbo::Point::new(0.0, h),
        effective * kurbo::Point::new(w, h),
    ];
    let mut x0 = corners[0].x;
    let mut x1 = corners[0].x;
    let mut y0 = corners[0].y;
    let mut y1 = corners[0].y;
    for c in corners.iter().skip(1) {
        if c.x < x0 {
            x0 = c.x;
        }
        if c.x > x1 {
            x1 = c.x;
        }
        if c.y < y0 {
            y0 = c.y;
        }
        if c.y > y1 {
            y1 = c.y;
        }
    }
    (x1 - x0).min(y1 - y0)
}

fn band_for(apparent: f64, t: &LodThresholds) -> LodLevel {
    if apparent >= t.deep_zoom_min_px {
        LodLevel::DeepZoom
    } else if apparent >= t.full_pane_min_px {
        LodLevel::FullPane
    } else if apparent >= t.card_min_px {
        LodLevel::Card
    } else {
        // Below `thumbnail_min_px` is still Thumbnail — that's the
        // most-degraded tier. Hosts wanting "don't render at all"
        // behavior can extend the band scheme.
        let _ = t.thumbnail_min_px;
        LodLevel::Thumbnail
    }
}

/// Return whichever of `a` and `b` is the higher-detail LOD
/// (DeepZoom > FullPane > Card > Thumbnail).
fn max_lod(a: LodLevel, b: LodLevel) -> LodLevel {
    if lod_rank(a) >= lod_rank(b) { a } else { b }
}

fn lod_rank(lod: LodLevel) -> u8 {
    match lod {
        LodLevel::Thumbnail => 0,
        LodLevel::Card => 1,
        LodLevel::FullPane => 2,
        LodLevel::DeepZoom => 3,
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Size;
    use mere_renderer_registry::{NodeContentKind, NodeIdentity, Placement};

    use super::*;

    fn node(size: Size, placement: Placement, stored_lod: LodLevel) -> SceneNodeRef {
        SceneNodeRef {
            identity: NodeIdentity::next(),
            placement,
            lod: stored_lod,
            size,
            content_kind: NodeContentKind::Panel,
            renderer_pin: None,
        }
    }

    #[test]
    fn full_pane_node_at_identity_camera_yields_full_pane_band() {
        let n = node(
            Size::new(400.0, 300.0),
            Placement::IDENTITY,
            LodLevel::FullPane,
        );
        // Apparent min = min(400, 300) = 300 → in [128, 768) → Card
        // band. But node.lod is FullPane (a floor) → result FullPane.
        assert_eq!(
            compute_lod_for_node(&n, Affine::IDENTITY, &LodThresholds::DEFAULT),
            LodLevel::FullPane
        );
    }

    #[test]
    fn zoom_in_promotes_to_deep_zoom() {
        let n = node(
            Size::new(400.0, 300.0),
            Placement::IDENTITY,
            LodLevel::FullPane,
        );
        let cam = Affine::scale(8.0);
        // Apparent min = 8 × 300 = 2400 → ≥ deep_zoom_min_px (2048).
        assert_eq!(
            compute_lod_for_node(&n, cam, &LodThresholds::DEFAULT),
            LodLevel::DeepZoom
        );
    }

    #[test]
    fn zoom_out_floors_at_stored_lod() {
        let n = node(
            Size::new(400.0, 300.0),
            Placement::IDENTITY,
            LodLevel::FullPane,
        );
        let cam = Affine::scale(0.1);
        // Apparent min = 30 → below all bands → Thumbnail band. But
        // stored LOD is FullPane → result floored at FullPane.
        assert_eq!(
            compute_lod_for_node(&n, cam, &LodThresholds::DEFAULT),
            LodLevel::FullPane
        );
    }

    #[test]
    fn small_node_at_identity_camera_yields_card_band() {
        let n = node(
            Size::new(200.0, 150.0),
            Placement::IDENTITY,
            LodLevel::Thumbnail,
        );
        // Apparent min = 150 → in [128, 768) → Card band. Stored is
        // Thumbnail (rank 0) so promotion to Card wins.
        assert_eq!(
            compute_lod_for_node(&n, Affine::IDENTITY, &LodThresholds::DEFAULT),
            LodLevel::Card
        );
    }

    #[test]
    fn non_uniform_camera_uses_smaller_axis() {
        let n = node(
            Size::new(400.0, 100.0),
            Placement::IDENTITY,
            LodLevel::Thumbnail,
        );
        // Non-uniform: 2× width, 0.5× height → apparent (800, 50).
        // Min = 50 → below 128 → Thumbnail band. Stored is Thumbnail
        // so the result is Thumbnail.
        let cam = Affine::scale_non_uniform(2.0, 0.5);
        assert_eq!(
            compute_lod_for_node(&n, cam, &LodThresholds::DEFAULT),
            LodLevel::Thumbnail
        );
    }

    #[test]
    fn rotation_doesnt_change_apparent_size_meaningfully() {
        let n = node(
            Size::new(200.0, 200.0),
            Placement::IDENTITY,
            LodLevel::Thumbnail,
        );
        // 45° rotation around origin. AABB grows by sqrt(2); min
        // dimension grows from 200 to ~282.84, still in Card band.
        let cam = Affine::rotate(std::f64::consts::FRAC_PI_4);
        assert_eq!(
            compute_lod_for_node(&n, cam, &LodThresholds::DEFAULT),
            LodLevel::Card
        );
    }
}
