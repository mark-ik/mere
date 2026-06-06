/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Theme registry — `register_theme` / `unregister_theme` / `resolve_theme`
//! over a `HashMap<String, ThemeTokenSet>` keyed by `theme:*` ids. Token
//! validation delegates to `crate::edge_style::validate_theme_edge_tokens`
//! (the bundled-in vocabulary, see [`crate`] for the bundling rationale).
//!
//! Cross-crate retargets vs. the original shell-side `theme.rs`:
//! - `crate::graph::edge_style_registry::*` → `crate::edge_style::*`
//! - `crate::registries::atomic::lens::*` → `register_lens::*`
//! - the dead `#[cfg(feature = "egui-host")] pub use egui::Color32;`
//!   alias was dropped per the "remove egui" half of the bundle decision —
//!   `egui-host` is an empty no-op feature in root `Cargo.toml:96` and egui
//!   is no longer in the dep graph
//! - `pub` → `pub` throughout (items are now this crate's public API)

use std::collections::HashMap;

pub use kernel::color::Color32;

use crate::edge_style::{
    EdgeAccessibilityMode, ThemeAccessibilitySupport, ThemeContract, ThemeEdgeTokens,
    validate_theme_edge_tokens,
};
use register_lens::{
    THEME_ID_DARK as LEGACY_THEME_ID_DARK, THEME_ID_DEFAULT as LEGACY_THEME_ID_DEFAULT, ThemeData,
};

/// Color tokens for graph-node chrome (badges, pinned fill, rings, default stroke).
///
/// Moved inline here from the retired `graph::egui_adapter` module; themes are
/// the sole consumer.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GraphNodeChromeTheme {
    pub workspace_badge_background: Color32,
    pub workspace_badge_text: Color32,
    pub semantic_badge_background: Color32,
    pub semantic_badge_text: Color32,
    pub semantic_badge_overflow_background: Color32,
    pub semantic_badge_orbit_background: Color32,
    pub pinned_fill: Color32,
    pub pinned_stroke: Color32,
    pub clip_ring: Color32,
    pub default_stroke: Color32,
}

impl Default for GraphNodeChromeTheme {
    fn default() -> Self {
        Self {
            workspace_badge_background: Color32::from_rgba_unmultiplied(20, 30, 46, 224),
            workspace_badge_text: Color32::from_gray(245),
            semantic_badge_background: Color32::from_rgba_unmultiplied(34, 44, 64, 224),
            semantic_badge_text: Color32::from_gray(245),
            semantic_badge_overflow_background: Color32::from_rgba_unmultiplied(24, 24, 24, 216),
            semantic_badge_orbit_background: Color32::from_rgba_unmultiplied(20, 28, 42, 230),
            pinned_fill: Color32::WHITE,
            pinned_stroke: Color32::from_gray(40),
            clip_ring: Color32::from_rgb(170, 210, 255),
            default_stroke: Color32::from_gray(90),
        }
    }
}

pub const THEME_ID_DEFAULT: &str = LEGACY_THEME_ID_DEFAULT;
pub const THEME_ID_LIGHT: &str = "theme:light";
pub const THEME_ID_DARK: &str = LEGACY_THEME_ID_DARK;
pub const THEME_ID_HIGH_CONTRAST: &str = "theme:high_contrast";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThemeTokenSet {
    pub theme_id: String,
    pub display_name: String,
    pub theme_data: ThemeData,
    pub accessibility: ThemeAccessibilitySupport,
    pub theme_contract: ThemeContract,
    pub edge_tokens: ThemeEdgeTokens,
    pub command_notice: Color32,
    pub radial_disabled_text: Color32,
    pub radial_hub_fill: Color32,
    pub radial_hub_stroke: Color32,
    pub radial_hub_text: Color32,
    pub radial_domain_active_fill: Color32,
    pub radial_domain_idle_fill: Color32,
    pub radial_command_active_fill: Color32,
    pub radial_command_hover_fill: Color32,
    pub radial_command_disabled_fill: Color32,
    pub radial_command_text: Color32,
    pub radial_chrome_text: Color32,
    pub radial_warning_text: Color32,
    pub hover_label_background: Color32,
    pub hover_label_stroke: Color32,
    pub hover_label_text: Color32,
    pub graph_node_search_match: Color32,
    pub graph_node_search_match_active: Color32,
    pub graph_node_hover: Color32,
    pub graph_node_selection: Color32,
    pub graph_node_focus_ring: Color32,
    pub graph_node_hover_ring: Color32,
    pub graph_node_chrome: GraphNodeChromeTheme,
    pub status_success: Color32,
    pub status_warning: Color32,
    pub status_error: Color32,
    pub status_neutral: Color32,
    pub workbench_panel_background: Color32,
    /// Highlight background for selection chrome over dense panels
    /// (dropdowns, toast strips). Paired with `selection_highlight_text`
    /// + `selection_highlight_stroke` to form a three-token trio that
    /// maintains contrast on any theme.
    pub selection_highlight_background: Color32,
    pub selection_highlight_text: Color32,
    pub selection_highlight_stroke: Color32,
    pub semantic_origin_manual: Color32,
    pub semantic_origin_semantic: Color32,
    pub semantic_origin_anchor: Color32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeCapability {
    pub requested_id: String,
    pub resolved_id: String,
    pub matched: bool,
    pub fallback_used: bool,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThemeResolution {
    pub requested_id: String,
    pub resolved_id: String,
    pub matched: bool,
    pub fallback_used: bool,
    pub tokens: ThemeTokenSet,
}

pub struct ThemeRegistry {
    themes: HashMap<String, ThemeTokenSet>,
    active: String,
    fallback_id: String,
}

impl Default for ThemeRegistry {
    fn default() -> Self {
        let mut registry = Self {
            themes: HashMap::new(),
            active: THEME_ID_DEFAULT.to_string(),
            fallback_id: THEME_ID_DEFAULT.to_string(),
        };
        registry
            .register_theme(default_theme_tokens())
            .expect("default theme must be valid");
        registry
            .register_theme(light_theme_tokens())
            .expect("light theme must be valid");
        registry
            .register_theme(dark_theme_tokens())
            .expect("dark theme must be valid");
        registry
            .register_theme(high_contrast_theme_tokens())
            .expect("high-contrast theme must be valid");
        registry
    }
}

impl ThemeRegistry {
    pub fn register_theme(&mut self, tokens: ThemeTokenSet) -> Result<(), String> {
        validate_theme_tokens(&tokens)?;
        self.themes
            .insert(tokens.theme_id.to_ascii_lowercase(), tokens);
        Ok(())
    }

    pub fn unregister_theme(&mut self, theme_id: &str) -> bool {
        let normalized = theme_id.trim().to_ascii_lowercase();
        if normalized == self.fallback_id {
            return false;
        }
        self.themes.remove(&normalized).is_some()
    }

    pub fn resolve_theme(&self, theme_id: Option<&str>) -> ThemeResolution {
        let requested = theme_id
            .unwrap_or(self.active.as_str())
            .trim()
            .to_ascii_lowercase();
        let fallback = self
            .themes
            .get(&self.fallback_id)
            .cloned()
            .unwrap_or_else(default_theme_tokens);

        if requested.is_empty() {
            return ThemeResolution {
                requested_id: requested,
                resolved_id: self.fallback_id.clone(),
                matched: false,
                fallback_used: true,
                tokens: fallback,
            };
        }

        if let Some(tokens) = self.themes.get(&requested).cloned() {
            return ThemeResolution {
                requested_id: requested.clone(),
                resolved_id: requested,
                matched: true,
                fallback_used: false,
                tokens,
            };
        }

        ThemeResolution {
            requested_id: requested,
            resolved_id: self.fallback_id.clone(),
            matched: false,
            fallback_used: true,
            tokens: fallback,
        }
    }

    pub fn describe_theme(&self, theme_id: Option<&str>) -> ThemeCapability {
        let resolution = self.resolve_theme(theme_id);
        ThemeCapability {
            requested_id: resolution.requested_id,
            resolved_id: resolution.resolved_id,
            matched: resolution.matched,
            fallback_used: resolution.fallback_used,
            display_name: resolution.tokens.display_name,
        }
    }

    pub fn set_active_theme(&mut self, theme_id: &str) -> ThemeResolution {
        let resolution = self.resolve_theme(Some(theme_id));
        self.active = resolution.resolved_id.clone();
        resolution
    }

    pub fn active_theme(&self) -> ThemeResolution {
        self.resolve_theme(None)
    }
}

fn validate_theme_tokens(tokens: &ThemeTokenSet) -> Result<(), String> {
    let minimum_ratio = if tokens.theme_id == THEME_ID_HIGH_CONTRAST {
        7.0
    } else {
        4.5
    };

    for (label, foreground, background) in [
        (
            "radial disabled text",
            tokens.radial_disabled_text,
            tokens.radial_command_disabled_fill,
        ),
        (
            "radial hub text",
            tokens.radial_hub_text,
            tokens.radial_hub_fill,
        ),
        (
            "hover label text",
            tokens.hover_label_text,
            tokens.hover_label_background,
        ),
        (
            "command notice",
            tokens.command_notice,
            tokens.hover_label_background,
        ),
    ] {
        let ratio = contrast_ratio(foreground, background);
        if ratio < minimum_ratio {
            return Err(format!(
                "{label} contrast {ratio:.2} below minimum {minimum_ratio:.2}"
            ));
        }
    }

    validate_theme_edge_tokens(&tokens.edge_tokens, &tokens.theme_contract)?;

    if matches!(
        tokens.accessibility.default_edge_mode,
        EdgeAccessibilityMode::Monochrome
    ) && !tokens.accessibility.supports_monochrome
    {
        return Err(
            "default edge mode cannot be monochrome when monochrome support is false".into(),
        );
    }

    Ok(())
}

fn contrast_ratio(foreground: Color32, background: Color32) -> f32 {
    let mut l1 = relative_luminance(foreground);
    let mut l2 = relative_luminance(background);
    if l2 > l1 {
        std::mem::swap(&mut l1, &mut l2);
    }
    (l1 + 0.05) / (l2 + 0.05)
}

fn relative_luminance(color: Color32) -> f32 {
    0.2126 * to_linear_component(color.r())
        + 0.7152 * to_linear_component(color.g())
        + 0.0722 * to_linear_component(color.b())
}

fn to_linear_component(component: u8) -> f32 {
    let value = component as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn default_theme_tokens() -> ThemeTokenSet {
    ThemeTokenSet {
        theme_id: THEME_ID_DEFAULT.to_string(),
        display_name: "Default".to_string(),
        theme_data: ThemeData {
            background_rgb: (20, 20, 25),
            accent_rgb: (80, 220, 255),
            font_scale: 1.0,
            stroke_width: 1.0,
        },
        accessibility: default_theme_accessibility(),
        theme_contract: default_theme_contract(),
        edge_tokens: ThemeEdgeTokens::default_theme(),
        command_notice: Color32::from_rgb(234, 200, 145),
        radial_disabled_text: Color32::from_rgb(165, 172, 178),
        radial_hub_fill: Color32::from_rgb(28, 32, 36),
        radial_hub_stroke: Color32::from_rgb(90, 110, 125),
        radial_hub_text: Color32::from_rgb(210, 230, 245),
        radial_domain_active_fill: Color32::from_rgb(70, 130, 170),
        radial_domain_idle_fill: Color32::from_rgb(50, 66, 80),
        radial_command_active_fill: Color32::from_rgb(80, 170, 215),
        radial_command_hover_fill: Color32::from_rgb(64, 82, 98),
        radial_command_disabled_fill: Color32::from_rgb(42, 48, 54),
        radial_command_text: Color32::from_rgb(230, 240, 248),
        radial_chrome_text: Color32::from_rgb(170, 190, 205),
        radial_warning_text: Color32::from_rgb(234, 200, 145),
        hover_label_background: Color32::from_rgba_unmultiplied(22, 28, 34, 235),
        hover_label_stroke: Color32::from_rgb(88, 110, 126),
        hover_label_text: Color32::from_rgb(220, 236, 248),
        graph_node_search_match: Color32::from_rgb(95, 220, 130),
        graph_node_search_match_active: Color32::from_rgb(140, 255, 140),
        graph_node_hover: Color32::from_rgb(255, 150, 80),
        graph_node_selection: Color32::from_rgb(255, 200, 100),
        graph_node_focus_ring: Color32::from_rgb(120, 200, 255),
        graph_node_hover_ring: Color32::from_rgba_unmultiplied(180, 180, 190, 180),
        graph_node_chrome: GraphNodeChromeTheme {
            workspace_badge_background: Color32::from_rgba_unmultiplied(20, 30, 46, 224),
            workspace_badge_text: Color32::from_gray(245),
            semantic_badge_background: Color32::from_rgba_unmultiplied(34, 44, 64, 224),
            semantic_badge_text: Color32::from_gray(245),
            semantic_badge_overflow_background: Color32::from_rgba_unmultiplied(24, 24, 24, 216),
            semantic_badge_orbit_background: Color32::from_rgba_unmultiplied(20, 28, 42, 230),
            pinned_fill: Color32::WHITE,
            pinned_stroke: Color32::from_gray(40),
            clip_ring: Color32::from_rgb(170, 210, 255),
            default_stroke: Color32::from_gray(90),
        },
        status_success: Color32::from_rgb(90, 200, 120),
        status_warning: Color32::from_rgb(210, 175, 70),
        status_error: Color32::from_rgb(180, 60, 60),
        status_neutral: Color32::from_rgb(140, 145, 150),
        workbench_panel_background: Color32::from_rgb(20, 20, 25),
        selection_highlight_background: Color32::from_rgb(255, 230, 0),
        selection_highlight_text: Color32::WHITE,
        selection_highlight_stroke: Color32::BLACK,
        semantic_origin_manual: Color32::from_rgb(120, 170, 255),
        semantic_origin_semantic: Color32::from_rgb(76, 175, 80),
        semantic_origin_anchor: Color32::from_rgb(255, 167, 38),
    }
}

fn light_theme_tokens() -> ThemeTokenSet {
    ThemeTokenSet {
        theme_id: THEME_ID_LIGHT.to_string(),
        display_name: "Light".to_string(),
        theme_data: ThemeData {
            background_rgb: (244, 246, 249),
            accent_rgb: (54, 120, 212),
            font_scale: 1.0,
            stroke_width: 1.0,
        },
        accessibility: default_theme_accessibility(),
        theme_contract: default_theme_contract(),
        edge_tokens: ThemeEdgeTokens::light_theme(),
        command_notice: Color32::from_rgb(144, 96, 22),
        radial_disabled_text: Color32::from_rgb(92, 98, 108),
        radial_hub_fill: Color32::from_rgb(248, 250, 252),
        radial_hub_stroke: Color32::from_rgb(176, 186, 198),
        radial_hub_text: Color32::from_rgb(26, 36, 48),
        radial_domain_active_fill: Color32::from_rgb(98, 152, 226),
        radial_domain_idle_fill: Color32::from_rgb(226, 232, 239),
        radial_command_active_fill: Color32::from_rgb(90, 150, 220),
        radial_command_hover_fill: Color32::from_rgb(214, 222, 232),
        radial_command_disabled_fill: Color32::from_rgb(236, 240, 244),
        radial_command_text: Color32::from_rgb(28, 36, 48),
        radial_chrome_text: Color32::from_rgb(92, 102, 118),
        radial_warning_text: Color32::from_rgb(144, 96, 22),
        hover_label_background: Color32::from_rgba_unmultiplied(250, 252, 255, 244),
        hover_label_stroke: Color32::from_rgb(178, 188, 202),
        hover_label_text: Color32::from_rgb(28, 36, 46),
        graph_node_search_match: Color32::from_rgb(50, 170, 94),
        graph_node_search_match_active: Color32::from_rgb(38, 146, 80),
        graph_node_hover: Color32::from_rgb(214, 120, 52),
        graph_node_selection: Color32::from_rgb(214, 160, 56),
        graph_node_focus_ring: Color32::from_rgb(54, 120, 212),
        graph_node_hover_ring: Color32::from_rgba_unmultiplied(152, 160, 172, 164),
        graph_node_chrome: GraphNodeChromeTheme {
            workspace_badge_background: Color32::from_rgba_unmultiplied(232, 238, 246, 236),
            workspace_badge_text: Color32::from_rgb(32, 40, 52),
            semantic_badge_background: Color32::from_rgba_unmultiplied(220, 228, 238, 236),
            semantic_badge_text: Color32::from_rgb(38, 46, 58),
            semantic_badge_overflow_background: Color32::from_rgba_unmultiplied(204, 212, 224, 232),
            semantic_badge_orbit_background: Color32::from_rgba_unmultiplied(238, 242, 248, 240),
            pinned_fill: Color32::from_rgb(255, 255, 255),
            pinned_stroke: Color32::from_rgb(110, 122, 136),
            clip_ring: Color32::from_rgb(92, 146, 214),
            default_stroke: Color32::from_rgb(132, 144, 158),
        },
        status_success: Color32::from_rgb(46, 140, 86),
        status_warning: Color32::from_rgb(160, 120, 32),
        status_error: Color32::from_rgb(170, 62, 62),
        status_neutral: Color32::from_rgb(120, 124, 130),
        workbench_panel_background: Color32::from_rgb(245, 246, 250),
        selection_highlight_background: Color32::from_rgb(255, 216, 48),
        selection_highlight_text: Color32::from_rgb(24, 28, 34),
        selection_highlight_stroke: Color32::from_rgb(48, 52, 60),
        semantic_origin_manual: Color32::from_rgb(90, 150, 220),
        semantic_origin_semantic: Color32::from_rgb(50, 150, 86),
        semantic_origin_anchor: Color32::from_rgb(214, 130, 36),
    }
}

fn dark_theme_tokens() -> ThemeTokenSet {
    ThemeTokenSet {
        theme_id: THEME_ID_DARK.to_string(),
        display_name: "Dark".to_string(),
        theme_data: ThemeData {
            background_rgb: (14, 14, 18),
            accent_rgb: (110, 170, 255),
            font_scale: 1.0,
            stroke_width: 1.0,
        },
        accessibility: default_theme_accessibility(),
        theme_contract: default_theme_contract(),
        edge_tokens: ThemeEdgeTokens::dark_theme(),
        command_notice: Color32::from_rgb(240, 214, 164),
        radial_disabled_text: Color32::from_rgb(176, 182, 190),
        radial_hub_fill: Color32::from_rgb(20, 24, 30),
        radial_hub_stroke: Color32::from_rgb(92, 116, 138),
        radial_hub_text: Color32::from_rgb(220, 234, 250),
        radial_domain_active_fill: Color32::from_rgb(86, 140, 186),
        radial_domain_idle_fill: Color32::from_rgb(44, 56, 72),
        radial_command_active_fill: Color32::from_rgb(94, 166, 224),
        radial_command_hover_fill: Color32::from_rgb(58, 74, 92),
        radial_command_disabled_fill: Color32::from_rgb(34, 40, 48),
        radial_command_text: Color32::from_rgb(232, 240, 248),
        radial_chrome_text: Color32::from_rgb(184, 198, 214),
        radial_warning_text: Color32::from_rgb(240, 214, 164),
        hover_label_background: Color32::from_rgba_unmultiplied(16, 20, 28, 240),
        hover_label_stroke: Color32::from_rgb(86, 110, 136),
        hover_label_text: Color32::from_rgb(226, 236, 248),
        graph_node_search_match: Color32::from_rgb(112, 214, 158),
        graph_node_search_match_active: Color32::from_rgb(162, 245, 188),
        graph_node_hover: Color32::from_rgb(255, 166, 104),
        graph_node_selection: Color32::from_rgb(255, 214, 134),
        graph_node_focus_ring: Color32::from_rgb(140, 182, 255),
        graph_node_hover_ring: Color32::from_rgba_unmultiplied(170, 176, 194, 190),
        graph_node_chrome: GraphNodeChromeTheme {
            workspace_badge_background: Color32::from_rgba_unmultiplied(26, 36, 50, 228),
            workspace_badge_text: Color32::from_rgb(242, 246, 250),
            semantic_badge_background: Color32::from_rgba_unmultiplied(38, 48, 66, 228),
            semantic_badge_text: Color32::from_rgb(242, 246, 250),
            semantic_badge_overflow_background: Color32::from_rgba_unmultiplied(28, 32, 40, 220),
            semantic_badge_orbit_background: Color32::from_rgba_unmultiplied(22, 30, 44, 234),
            pinned_fill: Color32::from_rgb(255, 255, 255),
            pinned_stroke: Color32::from_rgb(56, 66, 82),
            clip_ring: Color32::from_rgb(156, 198, 255),
            default_stroke: Color32::from_rgb(104, 116, 132),
        },
        status_success: Color32::from_rgb(110, 216, 146),
        status_warning: Color32::from_rgb(232, 188, 84),
        status_error: Color32::from_rgb(220, 102, 102),
        status_neutral: Color32::from_rgb(150, 156, 168),
        workbench_panel_background: Color32::from_rgb(20, 20, 25),
        selection_highlight_background: Color32::from_rgb(255, 230, 64),
        selection_highlight_text: Color32::WHITE,
        selection_highlight_stroke: Color32::BLACK,
        semantic_origin_manual: Color32::from_rgb(138, 186, 255),
        semantic_origin_semantic: Color32::from_rgb(98, 198, 112),
        semantic_origin_anchor: Color32::from_rgb(255, 182, 84),
    }
}

fn high_contrast_theme_tokens() -> ThemeTokenSet {
    ThemeTokenSet {
        theme_id: THEME_ID_HIGH_CONTRAST.to_string(),
        display_name: "High Contrast".to_string(),
        theme_data: ThemeData {
            background_rgb: (0, 0, 0),
            accent_rgb: (255, 230, 0),
            font_scale: 1.1,
            stroke_width: 1.5,
        },
        accessibility: ThemeAccessibilitySupport {
            supports_monochrome: true,
            supports_high_contrast: true,
            default_edge_mode: EdgeAccessibilityMode::ColorAndPattern,
        },
        theme_contract: ThemeContract {
            min_family_luminance_delta: 4.0,
            require_non_color_family_distinction: true,
            require_monochrome_preservation: true,
        },
        edge_tokens: ThemeEdgeTokens::high_contrast_theme(),
        command_notice: Color32::from_rgb(255, 230, 0),
        radial_disabled_text: Color32::from_rgb(255, 255, 255),
        radial_hub_fill: Color32::from_rgb(0, 0, 0),
        radial_hub_stroke: Color32::from_rgb(255, 255, 255),
        radial_hub_text: Color32::from_rgb(255, 255, 255),
        radial_domain_active_fill: Color32::from_rgb(255, 230, 0),
        radial_domain_idle_fill: Color32::from_rgb(0, 0, 0),
        radial_command_active_fill: Color32::from_rgb(255, 230, 0),
        radial_command_hover_fill: Color32::from_rgb(40, 40, 40),
        radial_command_disabled_fill: Color32::from_rgb(0, 0, 0),
        radial_command_text: Color32::from_rgb(255, 255, 255),
        radial_chrome_text: Color32::from_rgb(255, 255, 255),
        radial_warning_text: Color32::from_rgb(255, 230, 0),
        hover_label_background: Color32::from_rgba_unmultiplied(0, 0, 0, 255),
        hover_label_stroke: Color32::from_rgb(255, 255, 255),
        hover_label_text: Color32::from_rgb(255, 255, 255),
        graph_node_search_match: Color32::from_rgb(0, 255, 170),
        graph_node_search_match_active: Color32::from_rgb(255, 255, 255),
        graph_node_hover: Color32::from_rgb(255, 128, 0),
        graph_node_selection: Color32::from_rgb(255, 230, 0),
        graph_node_focus_ring: Color32::from_rgb(255, 255, 255),
        graph_node_hover_ring: Color32::from_rgba_unmultiplied(255, 255, 255, 196),
        graph_node_chrome: GraphNodeChromeTheme {
            workspace_badge_background: Color32::from_rgba_unmultiplied(0, 0, 0, 255),
            workspace_badge_text: Color32::from_rgb(255, 255, 255),
            semantic_badge_background: Color32::from_rgba_unmultiplied(0, 0, 0, 255),
            semantic_badge_text: Color32::from_rgb(255, 255, 255),
            semantic_badge_overflow_background: Color32::from_rgba_unmultiplied(255, 255, 255, 255),
            semantic_badge_orbit_background: Color32::from_rgba_unmultiplied(0, 0, 0, 255),
            pinned_fill: Color32::from_rgb(255, 230, 0),
            pinned_stroke: Color32::from_rgb(255, 255, 255),
            clip_ring: Color32::from_rgb(255, 255, 255),
            default_stroke: Color32::from_rgb(255, 255, 255),
        },
        status_success: Color32::from_rgb(0, 255, 170),
        status_warning: Color32::from_rgb(255, 230, 0),
        status_error: Color32::from_rgb(255, 64, 64),
        status_neutral: Color32::from_rgb(220, 220, 230),
        workbench_panel_background: Color32::BLACK,
        selection_highlight_background: Color32::from_rgb(255, 230, 0),
        selection_highlight_text: Color32::BLACK,
        selection_highlight_stroke: Color32::WHITE,
        semantic_origin_manual: Color32::from_rgb(0, 255, 255),
        semantic_origin_semantic: Color32::from_rgb(0, 255, 170),
        semantic_origin_anchor: Color32::from_rgb(255, 230, 0),
    }
}

fn default_theme_accessibility() -> ThemeAccessibilitySupport {
    ThemeAccessibilitySupport {
        supports_monochrome: true,
        supports_high_contrast: true,
        default_edge_mode: EdgeAccessibilityMode::ColorAndPattern,
    }
}

fn default_theme_contract() -> ThemeContract {
    ThemeContract {
        min_family_luminance_delta: 4.0,
        require_non_color_family_distinction: true,
        require_monochrome_preservation: true,
    }
}

#[cfg(test)]
mod tests;
