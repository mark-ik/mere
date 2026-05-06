/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `NavigationPolicy` — user-tunable navigation feel for a graph view.
//!
//! Every discretionary knob that the host would otherwise hardcode lives
//! here: zoom bounds, fit-padding, pan inertia, scroll-to-pan rate,
//! drag threshold, lasso modifier, etc. Callers in
//! `render::canvas_bridge` (egui host) and the future iced host both
//! read the same resolved `NavigationPolicy`, so user tuning carries
//! across framework backends.
//!
//! The policy is serde-ready so hosts can persist per-view overrides
//! and per-graph defaults. Resolution order at the host layer is:
//! view override → graph default → `NavigationPolicy::default()`
//! baseline.

use serde::{Deserialize, Serialize};

use crate::engine::InteractionConfig;

/// Modifier key that gates primary-drag-on-background into a lasso
/// marquee instead of a pan. `None` flips the default so plain primary
/// drag always lassoes (Figma/Sketch convention) and pan happens only
/// via middle-click / secondary-drag / scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LassoModifier {
    /// Plain primary-drag pans; `Shift`+primary-drag lassos.
    /// Miro / tldraw convention; Graphshell's baseline since 2026-04-19.
    #[default]
    Shift,
    /// Plain primary-drag pans; `Ctrl`/`Cmd`+primary-drag lassos.
    /// Keeps `Shift` free for keyboard multi-select adjacency.
    Ctrl,
    /// Plain primary-drag pans; `Alt`+primary-drag lassos.
    Alt,
    /// No modifier gating: plain primary-drag lassos. Pan happens only
    /// via middle-click, secondary-drag, or scroll (Figma convention).
    None,
}

/// Default zoom-min applied by `apply_zoom` and `fit_to_bounds` when
/// the host doesn't override. Matches the retired egui_graphs floor.
pub const DEFAULT_ZOOM_MIN: f32 = 0.1;
/// Default zoom-max applied by `apply_zoom` and `fit_to_bounds`. 10.0
/// is comfortably past legibility for the typical node radius; above
/// this the labels outgrow the viewport.
pub const DEFAULT_ZOOM_MAX: f32 = 10.0;
/// Default fit-padding: 1.08 → ~4 % margin per viewport edge, matching
/// the pre-retirement feel.
pub const DEFAULT_FIT_PADDING_RATIO: f32 = 1.08;
/// Default fit fallback zoom used when the fit bounds collapse to a
/// point (zero area).
pub const DEFAULT_FIT_FALLBACK_ZOOM: f32 = 1.0;
/// Default pan-inertia damping per second. See
/// [`crate::camera::DEFAULT_PAN_DAMPING_PER_SECOND`].
pub const DEFAULT_PAN_DAMPING_PER_SECOND: f32 = 0.003;
/// Default scroll pan rate: one mousewheel notch pans ~50 px, matching
/// typical infinite-canvas feel (Figma / Miro).
pub const DEFAULT_SCROLL_PAN_PIXELS_PER_UNIT: f32 = 50.0;
/// Default `Ctrl`+scroll zoom factor per scroll unit.
pub const DEFAULT_SCROLL_ZOOM_FACTOR: f32 = 0.1;
/// Default minimum drag distance in screen pixels before a drag gesture
/// starts — below this, the interaction is treated as a click.
pub const DEFAULT_DRAG_THRESHOLD_PX: f32 = 6.0;

/// Tunable navigation feel for a graph view. All host-facing
/// hardcoded constants used to live in `render::canvas_bridge` — they
/// now resolve through this struct so users can tune per-view feel and
/// the iced host picks up the same values without a parallel constants
/// table.
///
/// The policy is intentionally flat: a single struct of primitives +
/// the `LassoModifier` enum, so persistence, diff, and settings UI are
/// all trivial. Hosts can present a subset by omitting fields from
/// their settings surface; the full surface is always available.
///
/// Defaults trace the retired egui_graphs + Miro-style baseline that
/// landed on 2026-04-19 — identical feel to the pre-policy world when
/// no override is set.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NavigationPolicy {
    /// Lower bound the camera zoom is clamped to by both interactive
    /// zoom (`apply_zoom`) and automatic fit (`fit_to_bounds`).
    pub zoom_min: f32,
    /// Upper bound on the camera zoom.
    pub zoom_max: f32,
    /// Ratio ≥ 1.0 applied around a fit-bounds so the rendered content
    /// doesn't kiss the viewport edge. 1.0 = no padding; 1.1 = ~5 %
    /// margin per side.
    pub fit_padding_ratio: f32,
    /// Fallback zoom when the fit bounds have zero area (single-point
    /// target). Honors `zoom_min`/`zoom_max`.
    pub fit_fallback_zoom: f32,
    /// Pan-inertia damping per second. `pan_velocity *= damping^dt`
    /// each frame. Lower = more damping, faster stop; 1.0 = no damping.
    pub pan_damping_per_second: f32,
    /// Pan delta in screen pixels per unit of plain (no-modifier)
    /// scroll input.
    pub scroll_pan_pixels_per_unit: f32,
    /// Multiplicative zoom factor per unit of `Ctrl`/`Cmd`+scroll
    /// input. `factor = 1.0 + delta * scroll_zoom_factor`.
    pub scroll_zoom_factor: f32,
    /// Minimum drag distance (screen pixels) before a drag gesture
    /// starts. Below this, the interaction resolves as a click.
    pub drag_threshold_px: f32,
    /// Whether drag-release seeds a coast-inertia velocity.
    pub pan_inertia_enabled: bool,
    /// Whether lasso selection is enabled at all.
    pub lasso_enabled: bool,
    /// Whether dragging a node moves it. Disabling this locks nodes to
    /// their physics-driven positions.
    pub node_drag_enabled: bool,
    /// Which modifier routes primary-drag on empty background into a
    /// lasso marquee. See [`LassoModifier`] for the conventions.
    pub lasso_modifier: LassoModifier,
}

impl Default for NavigationPolicy {
    fn default() -> Self {
        Self {
            zoom_min: DEFAULT_ZOOM_MIN,
            zoom_max: DEFAULT_ZOOM_MAX,
            fit_padding_ratio: DEFAULT_FIT_PADDING_RATIO,
            fit_fallback_zoom: DEFAULT_FIT_FALLBACK_ZOOM,
            pan_damping_per_second: DEFAULT_PAN_DAMPING_PER_SECOND,
            scroll_pan_pixels_per_unit: DEFAULT_SCROLL_PAN_PIXELS_PER_UNIT,
            scroll_zoom_factor: DEFAULT_SCROLL_ZOOM_FACTOR,
            drag_threshold_px: DEFAULT_DRAG_THRESHOLD_PX,
            pan_inertia_enabled: true,
            lasso_enabled: true,
            node_drag_enabled: true,
            lasso_modifier: LassoModifier::default(),
        }
    }
}

impl NavigationPolicy {
    /// Project the subset of fields the interaction engine consumes
    /// into an [`InteractionConfig`].
    pub fn to_interaction_config(&self) -> InteractionConfig {
        InteractionConfig {
            drag_threshold_px: self.drag_threshold_px,
            scroll_zoom_factor: self.scroll_zoom_factor,
            scroll_pan_pixels_per_unit: self.scroll_pan_pixels_per_unit,
            node_drag_enabled: self.node_drag_enabled,
            lasso_enabled: self.lasso_enabled,
            pan_inertia_enabled: self.pan_inertia_enabled,
            lasso_modifier: self.lasso_modifier,
        }
    }

    /// Clamp a zoom factor to `[zoom_min, zoom_max]`.
    pub fn clamp_zoom(&self, zoom: f32) -> f32 {
        zoom.clamp(self.zoom_min, self.zoom_max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_exposed_constants() {
        let p = NavigationPolicy::default();
        assert_eq!(p.zoom_min, DEFAULT_ZOOM_MIN);
        assert_eq!(p.zoom_max, DEFAULT_ZOOM_MAX);
        assert_eq!(p.fit_padding_ratio, DEFAULT_FIT_PADDING_RATIO);
        assert_eq!(p.fit_fallback_zoom, DEFAULT_FIT_FALLBACK_ZOOM);
        assert_eq!(p.pan_damping_per_second, DEFAULT_PAN_DAMPING_PER_SECOND);
        assert_eq!(
            p.scroll_pan_pixels_per_unit,
            DEFAULT_SCROLL_PAN_PIXELS_PER_UNIT
        );
        assert_eq!(p.scroll_zoom_factor, DEFAULT_SCROLL_ZOOM_FACTOR);
        assert_eq!(p.drag_threshold_px, DEFAULT_DRAG_THRESHOLD_PX);
        assert!(p.pan_inertia_enabled);
        assert!(p.lasso_enabled);
        assert!(p.node_drag_enabled);
        assert_eq!(p.lasso_modifier, LassoModifier::Shift);
    }

    #[test]
    fn to_interaction_config_threads_knobs() {
        let policy = NavigationPolicy {
            drag_threshold_px: 12.0,
            scroll_zoom_factor: 0.2,
            scroll_pan_pixels_per_unit: 75.0,
            node_drag_enabled: false,
            lasso_enabled: false,
            pan_inertia_enabled: false,
            lasso_modifier: LassoModifier::Ctrl,
            ..NavigationPolicy::default()
        };
        let cfg = policy.to_interaction_config();
        assert_eq!(cfg.drag_threshold_px, 12.0);
        assert_eq!(cfg.scroll_zoom_factor, 0.2);
        assert_eq!(cfg.scroll_pan_pixels_per_unit, 75.0);
        assert!(!cfg.node_drag_enabled);
        assert!(!cfg.lasso_enabled);
        assert!(!cfg.pan_inertia_enabled);
        assert_eq!(cfg.lasso_modifier, LassoModifier::Ctrl);
    }

    #[test]
    fn clamp_zoom_respects_bounds() {
        let p = NavigationPolicy {
            zoom_min: 0.5,
            zoom_max: 4.0,
            ..NavigationPolicy::default()
        };
        assert_eq!(p.clamp_zoom(0.1), 0.5);
        assert_eq!(p.clamp_zoom(2.0), 2.0);
        assert_eq!(p.clamp_zoom(10.0), 4.0);
    }

    #[test]
    fn serde_roundtrip() {
        let policy = NavigationPolicy {
            zoom_min: 0.25,
            lasso_modifier: LassoModifier::Alt,
            pan_inertia_enabled: false,
            ..NavigationPolicy::default()
        };
        let json = serde_json::to_string(&policy).unwrap();
        let back: NavigationPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }
}
