/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Seed-palette derivation — a few seed colours to the full [`ThemeTokenSet`].
//!
//! A theme is authored as a [`tincture::Seeds`] set (a primary/secondary/
//! tertiary brand triad + a neutral surface hue + a light/dark mode); this
//! module derives Mere's full token set from it. The base palette (surface
//! ladder, text hierarchy, contrast-picked `on_*`) comes from
//! [`tincture::derive_palette`]; the long-tail (chrome surface tiers, radial
//! menu, graph-node chrome) is refined from that base via the OKLCH
//! primitives. Role intent (from the shared Strophos theme doc):
//!
//! - **primary** — brand anchor: accent, primary actions, focus.
//! - **secondary** — sympathetic support hue (badge/nav tints).
//! - **tertiary** — "you are here": selection, hover markers, attention.
//!
//! The four built-ins are re-expressed as seeds here ([`builtin_token_sets`]),
//! so the derivation engine is the single authoring path and is proven against
//! them. Every derived set clears `validate_theme_tokens` (the contrast-gated
//! pairs use [`ensure_contrast`]).

use kernel::color::Color32;
use register_lens::ThemeData;
use tincture::oklch::Oklch;
use tincture::{best_on, contrast, derive_palette, mix, Seeds, Srgb};

use crate::chrome::ChromeTheme;
use crate::edge_style::{EdgeAccessibilityMode, ThemeAccessibilitySupport, ThemeContract, ThemeEdgeTokens};
use crate::theme::{
    GraphNodeChromeTheme, Harmony, ThemeDef, ThemeSource, ThemeTokenSet, THEME_ID_DARK,
    THEME_ID_DEFAULT, THEME_ID_HIGH_CONTRAST, THEME_ID_LIGHT,
};

// =============================================================================
// Colour conversion (Mere `Color32` <-> tincture `Srgb`)
// =============================================================================

fn srgb(c: Color32) -> Srgb {
    Srgb::rgba(c.r(), c.g(), c.b(), c.a())
}

fn c32(s: Srgb) -> Color32 {
    Color32::from_rgba_unmultiplied(s.r, s.g, s.b, s.a)
}

fn rgb_tuple(s: Srgb) -> (u8, u8, u8) {
    (s.r, s.g, s.b)
}

/// `fg` if it already clears `target` contrast against `bg`, else `fg` stepped
/// in OKLCH lightness away from `bg` until it does (or clamps to the extreme).
/// Keeps the 4 validator-gated text pairs legible on any seed set.
fn ensure_contrast(fg: Srgb, bg: Srgb, target: f64) -> Srgb {
    if contrast(fg, bg) >= target {
        return fg;
    }
    let o = Oklch::from_srgb(fg);
    // Move toward whichever extreme increases contrast against `bg`.
    let up = best_on(bg) == Srgb::rgb(0xF4, 0xF4, 0xF8); // bg is dark -> brighten fg
    for i in 1..=20 {
        let dl = 0.05 * i as f64;
        let cand = if up { o.lighten(dl) } else { o.darken(dl) }.to_srgb();
        if contrast(cand, bg) >= target {
            return cand;
        }
    }
    best_on(bg)
}

// =============================================================================
// Theme profile (the non-colour config a theme carries alongside its seeds)
// =============================================================================

/// Config a theme carries that is not derivable from colour seeds.
pub struct ThemeProfile {
    pub edge_tokens: ThemeEdgeTokens,
    pub font_scale: f32,
    pub stroke_width: f32,
    /// High-contrast mode: force surfaces to the pure extreme + strokes/text to
    /// max contrast, so the derived set clears the 7:1 gate and reads as the
    /// canonical black-surface / white-text high-contrast theme.
    pub high_contrast: bool,
}

fn standard_accessibility() -> ThemeAccessibilitySupport {
    ThemeAccessibilitySupport {
        supports_monochrome: true,
        supports_high_contrast: true,
        default_edge_mode: EdgeAccessibilityMode::ColorAndPattern,
    }
}

fn standard_contract() -> ThemeContract {
    ThemeContract {
        min_family_luminance_delta: 4.0,
        require_non_color_family_distinction: true,
        require_monochrome_preservation: true,
    }
}

// =============================================================================
// Derivation
// =============================================================================

/// Derive a full [`ThemeTokenSet`] from seeds + profile.
pub fn derive_token_set(
    theme_id: &str,
    display_name: &str,
    seeds: &Seeds,
    profile: &ThemeProfile,
) -> ThemeTokenSet {
    let p = derive_palette(seeds);
    let neutral = Oklch::from_srgb(seeds.neutral);
    let hc = profile.high_contrast;
    let dark = seeds.dark;

    // Surface ladder: a perceptual lightness step off the neutral hue. In
    // high-contrast mode every surface collapses to the pure extreme.
    let step = |dl: f64, ll: f64| -> Color32 {
        if hc {
            return c32(neutral.with_l(if dark { 0.0 } else { 1.0 }).to_srgb());
        }
        c32(neutral.with_l(if dark { dl } else { ll }).to_srgb())
    };
    // A stroke / divider tone: a mid-neutral, or pure-contrast in HC.
    let stroke = |dl: f64, ll: f64| -> Color32 {
        if hc {
            return c32(best_on(neutral.with_l(if dark { 0.0 } else { 1.0 }).to_srgb()));
        }
        c32(neutral.with_l(if dark { dl } else { ll }).to_srgb())
    };
    // Body / header / dim / disabled text, contrast-forced in HC.
    let surface = c32(p.surface);
    let text = if hc { c32(best_on(srgb(step(0.0, 1.0)))) } else { c32(p.text) };
    let header = if hc { text } else { c32(p.text_header) };
    let dim = if hc { text } else { c32(p.text_dim) };
    let disabled = if hc { stroke(0.0, 1.0) } else { c32(p.text_disabled) };

    // Brand triad (used directly; secondary only tints badges, via `p.secondary`).
    let primary = c32(p.primary);
    let tertiary = c32(p.tertiary);

    // A primary-tinted selection surface (active rows, focus fills).
    let active = if hc {
        primary
    } else {
        c32(mix(p.surface, p.primary, 0.40))
    };

    // --- Surface tiers -----------------------------------------------------
    let toolbar_bg = step(0.205, 0.945);
    let panel_bg = step(0.190, 0.965);
    let surface_bg = step(0.225, 0.975);
    let field_bg = step(0.185, 0.985);
    let control_bg = step(0.260, 0.900);
    let menu_bg = step(0.215, 0.935);
    let disabled_bg = step(0.185, 0.915);
    let hover_bg = step(0.300, 0.900);

    // --- Radial menu -------------------------------------------------------
    let hub_fill = step(0.175, 0.975);
    let hub_text = c32(ensure_contrast(srgb(text), srgb(hub_fill), 4.5));
    let command_disabled_fill = step(0.180, 0.930);
    let disabled_text = c32(ensure_contrast(srgb(dim), srgb(command_disabled_fill), 4.5));
    let idle_fill = step(0.260, 0.900);

    // --- Hover label (tooltip) --------------------------------------------
    let hover_label_bg = with_alpha(step(0.155, 0.985), 235);
    let hover_label_txt = c32(ensure_contrast(srgb(text), srgb(hover_label_bg), 4.5));
    let notice = c32(ensure_contrast(p.tertiary, srgb(hover_label_bg), 4.5));

    // --- Selection highlight (attention; theme-tinted but high-vis) -------
    let sel_bg = c32(Oklch::from_srgb(p.tertiary).with_l(if dark { 0.86 } else { 0.80 }).to_srgb());
    let sel_text = c32(best_on(srgb(sel_bg)));
    let sel_stroke = c32(best_on(srgb(sel_text)));

    // --- Status ------------------------------------------------------------
    let success = c32(p.success);
    let danger = c32(p.danger);
    let warning = tertiary;

    ThemeTokenSet {
        theme_id: theme_id.to_string(),
        display_name: display_name.to_string(),
        theme_data: ThemeData {
            background_rgb: rgb_tuple(srgb(step(0.160, 0.930))),
            accent_rgb: rgb_tuple(p.primary),
            font_scale: profile.font_scale,
            stroke_width: profile.stroke_width,
        },
        accessibility: standard_accessibility(),
        theme_contract: standard_contract(),
        edge_tokens: profile.edge_tokens.clone(),
        command_notice: notice,
        radial_disabled_text: disabled_text,
        radial_hub_fill: hub_fill,
        radial_hub_stroke: stroke(0.420, 0.620),
        radial_hub_text: hub_text,
        radial_domain_active_fill: if hc { primary } else { c32(mix(p.primary, p.surface, 0.30)) },
        radial_domain_idle_fill: idle_fill,
        radial_command_active_fill: primary,
        radial_command_hover_fill: hover_bg,
        radial_command_disabled_fill: command_disabled_fill,
        radial_command_text: text,
        radial_chrome_text: dim,
        radial_warning_text: warning,
        hover_label_background: hover_label_bg,
        hover_label_stroke: stroke(0.420, 0.620),
        hover_label_text: hover_label_txt,
        graph_node_search_match: success,
        graph_node_search_match_active: c32(Oklch::from_srgb(p.success).lighten(0.10).to_srgb()),
        graph_node_hover: tertiary,
        graph_node_selection: tertiary,
        graph_node_focus_ring: c32(Oklch::from_srgb(p.primary).lighten(0.18).to_srgb()),
        graph_node_hover_ring: with_alpha(stroke(0.560, 0.560), 190),
        graph_node_chrome: GraphNodeChromeTheme {
            workspace_badge_background: with_alpha(
                if hc { step(0.0, 1.0) } else { c32(mix(p.surface, p.primary, 0.15)) },
                224,
            ),
            workspace_badge_text: c32(best_on(srgb(surface))),
            semantic_badge_background: with_alpha(
                if hc { step(0.0, 1.0) } else { c32(mix(p.surface, p.secondary, 0.15)) },
                224,
            ),
            semantic_badge_text: c32(best_on(srgb(surface))),
            semantic_badge_overflow_background: with_alpha(step(0.140, 0.880), 220),
            semantic_badge_orbit_background: with_alpha(step(0.165, 0.940), 230),
            pinned_fill: stroke(0.980, 0.300),
            pinned_stroke: stroke(0.360, 0.500),
            clip_ring: c32(Oklch::from_srgb(p.primary).lighten(0.22).to_srgb()),
            default_stroke: stroke(0.420, 0.600),
        },
        chrome: ChromeTheme {
            toolbar_bg,
            control_bg,
            control_text: text,
            field_bg,
            field_text: text,
            panel_bg,
            surface_bg,
            body_text: text,
            strong_text: header,
            muted_text: dim,
            active_bg: active,
            disabled_text: disabled,
            disabled_bg,
            menu_bg,
            error_text: c32(mix(p.danger, p.text, 0.45)),
            error_bg: c32(mix(p.danger, p.bg, 0.75)),
        },
        status_success: success,
        status_warning: warning,
        status_error: danger,
        status_neutral: dim,
        workbench_panel_background: surface,
        selection_highlight_background: sel_bg,
        selection_highlight_text: sel_text,
        selection_highlight_stroke: sel_stroke,
        semantic_origin_manual: c32(Oklch::from_srgb(p.primary).lighten(0.12).to_srgb()),
        semantic_origin_semantic: success,
        semantic_origin_anchor: tertiary,
    }
}

/// `Color32` with replaced straight alpha.
fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

// =============================================================================
// Built-in themes (re-expressed as seeds). The derivation engine is the single
// authoring path; bring over Woodshed's curated triad values where they map.
// =============================================================================

fn rgb(r: u8, g: u8, b: u8) -> Srgb {
    Srgb::rgb(r, g, b)
}

/// Slate triad (Woodshed's faithful cool-dark) — used for Default + Dark.
fn slate_triad() -> (Srgb, Srgb, Srgb, Srgb, Srgb) {
    (
        rgb(0x33, 0x66, 0xC8), // primary
        rgb(0x2E, 0x9D, 0xA6), // secondary
        rgb(0xE0, 0xA8, 0x46), // tertiary
        rgb(0x4F, 0xB3, 0x6E), // success
        rgb(0xD5, 0x4E, 0x4E), // danger
    )
}

/// The non-colour profile for a theme, fully determined by its mode: edge
/// tokens + type/stroke scale follow `(dark, high_contrast)`.
fn profile_for(dark: bool, high_contrast: bool) -> ThemeProfile {
    let edge_tokens = if high_contrast {
        ThemeEdgeTokens::high_contrast_theme()
    } else if dark {
        ThemeEdgeTokens::dark_theme()
    } else {
        ThemeEdgeTokens::light_theme()
    };
    let (font_scale, stroke_width) = if high_contrast { (1.1, 1.5) } else { (1.0, 1.0) };
    ThemeProfile {
        edge_tokens,
        font_scale,
        stroke_width,
        high_contrast,
    }
}

/// Derive a [`ThemeTokenSet`] from a [`ThemeDef`] — the single path for both
/// built-ins and user themes. The profile is computed from the def's mode.
pub fn derive_from_def(def: &ThemeDef) -> ThemeTokenSet {
    let seeds = harmonized_seeds(def);
    derive_token_set(
        &def.id,
        &def.name,
        &seeds,
        &profile_for(seeds.dark, def.high_contrast),
    )
}

/// The def's seeds after applying its [`Harmony`] — the *effective* triad (the
/// accents possibly hue-locked to the primary). The derivation and the editor's
/// colour display both read this, so what you see matches what renders.
/// (Seed-palette harmony.)
pub fn harmonized_seeds(def: &ThemeDef) -> Seeds {
    let mut seeds = def.seeds;
    apply_harmony(&mut seeds, def.harmony);
    seeds
}

/// Apply a [`Harmony`] to seeds in place. `Locked` sets the secondary +
/// tertiary **hue** to the primary's hue plus the stored offset (in degrees),
/// keeping each accent's own chroma + lightness, so editing the base rotates the
/// whole triad. `Custom` leaves the accents untouched. (Seed-palette harmony.)
fn apply_harmony(seeds: &mut Seeds, harmony: Harmony) {
    let Harmony::Locked {
        secondary_deg,
        tertiary_deg,
    } = harmony
    else {
        return;
    };
    let base_h = Oklch::from_srgb(seeds.primary).h;
    let rot = |orig: Srgb, deg: f32| {
        Oklch::from_srgb(orig)
            .with_h(base_h + (deg as f64).to_radians())
            .to_srgb()
    };
    seeds.secondary = rot(seeds.secondary, secondary_deg);
    seeds.tertiary = rot(seeds.tertiary, tertiary_deg);
}

/// The Default built-in's derived token set — the registry's fallback when a
/// requested theme id is missing.
pub fn default_token_set() -> ThemeTokenSet {
    derive_from_def(&builtin_defs().swap_remove(0))
}

/// Convenience: every built-in's derived token set (built-ins re-expressed as
/// seeds, then derived). Used by tests + any consumer wanting the resolved sets
/// without the registry.
pub fn builtin_token_sets() -> Vec<ThemeTokenSet> {
    builtin_defs().iter().map(derive_from_def).collect()
}

/// The four built-in theme definitions, re-expressed as seeds. Default + Dark
/// share the Slate triad (Dark pulled toward black); Light is the triad shifted
/// for light surfaces; High Contrast is a yellow accent over forced-black
/// surfaces (high-contrast derivation mode).
pub fn builtin_defs() -> Vec<ThemeDef> {
    let (pr, se, te, ok, no) = slate_triad();
    vec![
        ThemeDef {
            id: THEME_ID_DEFAULT.to_string(),
            name: "Default".to_string(),
            source: ThemeSource::BuiltIn,
            seeds: Seeds {
                primary: pr,
                secondary: se,
                tertiary: te,
                neutral: rgb(0x10, 0x14, 0x22),
                text_header: None,
                text_body: None,
                success: ok,
                danger: no,
                dark: true,
            },
            high_contrast: false,
            harmony: Harmony::Custom,
        },
        ThemeDef {
            id: THEME_ID_DARK.to_string(),
            name: "Dark".to_string(),
            source: ThemeSource::BuiltIn,
            seeds: Seeds {
                primary: rgb(0x6E, 0xAA, 0xFF),
                secondary: se,
                tertiary: te,
                neutral: rgb(0x08, 0x09, 0x0D),
                text_header: None,
                text_body: None,
                success: ok,
                danger: no,
                dark: true,
            },
            high_contrast: false,
            harmony: Harmony::Custom,
        },
        ThemeDef {
            id: THEME_ID_LIGHT.to_string(),
            name: "Light".to_string(),
            source: ThemeSource::BuiltIn,
            seeds: Seeds {
                primary: rgb(0x2A, 0x55, 0xB4),
                secondary: rgb(0x1F, 0x77, 0x7F),
                tertiary: rgb(0xA8, 0x6C, 0x14),
                neutral: rgb(0xDF, 0xE3, 0xEE),
                text_header: None,
                text_body: None,
                success: rgb(0x2F, 0x8A, 0x4F),
                danger: rgb(0xB8, 0x33, 0x33),
                dark: false,
            },
            high_contrast: false,
            harmony: Harmony::Custom,
        },
        ThemeDef {
            id: THEME_ID_HIGH_CONTRAST.to_string(),
            name: "High Contrast".to_string(),
            source: ThemeSource::BuiltIn,
            seeds: Seeds {
                primary: rgb(0xFF, 0xE6, 0x00),
                secondary: rgb(0x00, 0xFF, 0xAA),
                tertiary: rgb(0xFF, 0xE6, 0x00),
                neutral: rgb(0x00, 0x00, 0x00),
                text_header: None,
                text_body: None,
                success: rgb(0x00, 0xFF, 0xAA),
                danger: rgb(0xFF, 0x40, 0x40),
                dark: true,
            },
            high_contrast: true,
            harmony: Harmony::Custom,
        },
    ]
}
