// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Host-side view-intent presets. Each `ViewPreset` is a named bundle
//! that maps a pane's role (orrery, drift-browse, minimap, …) to a
//! [`cartography::ViewIntent`] shape — `FormFactor`, dimension,
//! target-size policy.
//!
//! Aligns with the cartography layer brief's "helper-era preset
//! portfolio recast as cartography view-intent presets" (drift /
//! scatter / settle / archipelago / resonance / constellation). v0a
//! ships the two minimum-viable members — `Orrery` (the t1 root view's
//! map-shaped form factor) and `Drift` (the default browse / canvas
//! form factor) — plus `Minimap` for the same-strategy thumbnail path
//! the brief §6 names. Additional presets join as their consumers
//! materialize.
//!
//! Strategy selection per preset is deliberately not wired here yet:
//! the v0 host runs `GridAdapter` for every preset (Path A in the
//! orrery-renderer brief). When the streaming-strategy slice (Path B
//! — force-directed with per-node substrate nodes) lands, the
//! `default_strategy_id()` method below grows from "always grid" to
//! per-preset.

use cartography::{FormFactor, ProjectionDimension, TargetSize, ViewIntent};
use frame::PaneContent;

/// Named view-intent preset. Each variant is a *role* a pane plays;
/// the preset picks the [`cartography::FormFactor`] and target-size
/// shape that role implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewPreset {
    /// The t1 root graph view — a "map" of the user's orrery. Strategies
    /// can specialize on `FormFactor::Orrery` to emphasize map-shape
    /// over network-shape.
    Orrery,
    /// Default browse / exploration view. Canvas form factor; gentle
    /// force-directed friend when its strategy lands.
    Drift,
    /// Thumbnail / overview projection. Same strategies as the source
    /// view; logical target size; strategies strip detail at this
    /// scale (cartography brief §6).
    Minimap,
}

impl ViewPreset {
    /// Build the [`ViewIntent`] this preset implies, sized for the
    /// given pane's pixel viewport. Pane size is the bound; the
    /// preset picks form-factor / dimension / target-size *shape*.
    pub fn intent_for(self, pane_size: kurbo::Size) -> ViewIntent {
        let width = pane_size.width.max(1.0) as u32;
        let height = pane_size.height.max(1.0) as u32;
        match self {
            ViewPreset::Orrery => ViewIntent {
                form_factor: FormFactor::Orrery,
                dimension: ProjectionDimension::TwoD,
                focus: None,
                filter: None,
                target_size: TargetSize::Pixels { width, height },
                axis_values: None,
            },
            ViewPreset::Drift => ViewIntent::canvas_pixels(width, height),
            ViewPreset::Minimap => ViewIntent::minimap(width as f32, height as f32),
        }
    }

    /// Default cartography strategy id for this preset. Returns the
    /// [`cartography::LayoutStrategy::projection_id`] string the host's
    /// [`StrategyRegistry`](crate::strategy_registry::StrategyRegistry)
    /// resolves to a boxed strategy.
    ///
    /// v0a routes through analytic adapters only (no streaming):
    /// Orrery → phyllotaxis (sunflower packing reads as a map of the
    /// user's flora); Drift and Minimap → grid (cheap, deterministic;
    /// streaming strategies replace these when Path B / per-node
    /// substrate nodes land).
    pub fn default_strategy_id(self) -> &'static str {
        match self {
            ViewPreset::Orrery => "phyllotaxis.default",
            ViewPreset::Drift | ViewPreset::Minimap => "grid.default",
        }
    }
}

/// Default preset for a [`PaneContent`] variant. Orrery panes get the
/// orrery preset; everything else with a graph (workbench, gloss, …)
/// defaults to drift. Called per-leaf inside the cartography
/// projection pass when the host hasn't installed a per-pane override.
pub fn default_preset_for(content: &PaneContent) -> ViewPreset {
    match content {
        PaneContent::Orrery => ViewPreset::Orrery,
        _ => ViewPreset::Drift,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orrery_preset_uses_orrery_form_factor() {
        let intent = ViewPreset::Orrery.intent_for(kurbo::Size::new(800.0, 600.0));
        assert_eq!(intent.form_factor, FormFactor::Orrery);
        assert_eq!(
            intent.target_size,
            TargetSize::Pixels {
                width: 800,
                height: 600
            }
        );
    }

    #[test]
    fn drift_preset_uses_canvas_form_factor() {
        let intent = ViewPreset::Drift.intent_for(kurbo::Size::new(400.0, 300.0));
        assert_eq!(intent.form_factor, FormFactor::Canvas);
        assert_eq!(
            intent.target_size,
            TargetSize::Pixels {
                width: 400,
                height: 300
            }
        );
    }

    #[test]
    fn minimap_preset_uses_logical_size() {
        let intent = ViewPreset::Minimap.intent_for(kurbo::Size::new(120.0, 90.0));
        assert_eq!(intent.form_factor, FormFactor::Minimap);
        assert_eq!(
            intent.target_size,
            TargetSize::Logical {
                width: 120.0,
                height: 90.0
            }
        );
    }

    #[test]
    fn default_preset_maps_orrery_content_to_orrery_preset() {
        assert_eq!(
            default_preset_for(&PaneContent::Orrery),
            ViewPreset::Orrery
        );
    }

    #[test]
    fn default_preset_maps_workbench_content_to_drift() {
        assert_eq!(
            default_preset_for(&PaneContent::Workbench),
            ViewPreset::Drift
        );
    }

    #[test]
    fn every_preset_resolves_to_a_strategy_id() {
        // The host wires this into a registry; an empty string would
        // silently route to the wrong strategy at projection time.
        for preset in [ViewPreset::Orrery, ViewPreset::Drift, ViewPreset::Minimap] {
            let id = preset.default_strategy_id();
            assert!(!id.is_empty(), "preset {preset:?} returned empty id");
        }
    }

    #[test]
    fn intent_handles_zero_pane_size_without_panic() {
        // Defensive: a freshly-resized pane can momentarily report
        // zero dimensions; clamping keeps the intent valid.
        let intent = ViewPreset::Drift.intent_for(kurbo::Size::new(0.0, 0.0));
        assert_eq!(
            intent.target_size,
            TargetSize::Pixels {
                width: 1,
                height: 1
            }
        );
    }
}
