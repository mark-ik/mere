// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom MODE calculators, declarative lane (theme-modes T5).
//!
//! A registered custom mode is a name plus a calculator that receives the
//! active theme's [`Seeds`] and produces the shell palette. This module is the
//! **declarative** calculator: a mapping table from each [`ChromeTheme`] role
//! to a small OKLCH transform of one seed colour (`seed` + optional lightness /
//! chroma / hue-rotation / alpha, or a contrast-picked `on` colour over the
//! result). It runs the same maths the built-in derivation uses (tinct's
//! public `oklch` module), so a mode file is a handful of numbers, tiny and
//! reviewable — the mod-distribution path, like theme files.
//!
//! If authors outgrow the table (conditionals, cross-role reads), the
//! graduation path is the rhai host-automation lane per the scripting
//! doctrine; the file shape here stays the compatibility floor.
//!
//! A custom mode is a **sheet swap by definition** (a different rule set, not
//! different media applicability), so hosts route it through the sheet-swap
//! path, never the baked scheme pair.

use std::collections::BTreeMap;

use kernel::color::Color32;
use tincture::oklch::Oklch;
use tincture::{Seeds, Srgb, best_on};

/// A custom mode definition — one file in the host's `modes/` directory.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CustomModeDef {
    /// Stable id; the presentation mode persists as `custom:<id>`.
    pub id: String,
    /// The picker label.
    pub name: String,
    /// Scheme direction the mode presents as — what the engine's
    /// `prefers-color-scheme` evaluates to while this mode is active, and the
    /// ladder direction for the lanes the calculator does not cover (orrery /
    /// document palettes derive canonically with this flag).
    #[serde(default)]
    pub dark: bool,
    /// Whether the uncovered lanes derive with the high-contrast profile.
    #[serde(default)]
    pub high_contrast: bool,
    /// The calculator: one entry per [`ChromeTheme`] role, keyed by the
    /// role's field name (`"toolbar_bg"`, `"body_text"`, …). Every role must
    /// be present — a partial table is rejected at load, not half-rendered.
    pub chrome: BTreeMap<String, RoleSpec>,
}

/// One role's derivation: a seed colour through optional OKLCH transforms.
/// Applied in order: seed → `l` → `c` → `rotate` → (`on`) → `alpha`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoleSpec {
    /// Which seed the role starts from.
    pub seed: SeedRef,
    /// OKLCH lightness override (0..1) — the surface-ladder primitive.
    #[serde(default)]
    pub l: Option<f64>,
    /// OKLCH chroma override (0..~0.4) — mute / saturate the tone.
    #[serde(default)]
    pub c: Option<f64>,
    /// Hue rotation in degrees (analogous / complementary shifts).
    #[serde(default)]
    pub rotate: Option<f64>,
    /// Replace the result with tinct's contrast-picked near-white/near-black
    /// **over** it — for text on a computed fill.
    #[serde(default)]
    pub on: bool,
    /// Straight alpha override (0..255; default opaque).
    #[serde(default)]
    pub alpha: Option<u8>,
}

/// The seed a [`RoleSpec`] starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedRef {
    Primary,
    Secondary,
    Tertiary,
    Neutral,
    Success,
    Danger,
}

impl SeedRef {
    fn resolve(self, seeds: &Seeds) -> Srgb {
        match self {
            SeedRef::Primary => seeds.primary,
            SeedRef::Secondary => seeds.secondary,
            SeedRef::Tertiary => seeds.tertiary,
            SeedRef::Neutral => seeds.neutral,
            SeedRef::Success => seeds.success,
            SeedRef::Danger => seeds.danger,
        }
    }
}

/// Evaluate one role spec against the seeds.
fn eval_role(spec: &RoleSpec, seeds: &Seeds) -> Color32 {
    let mut o = Oklch::from_srgb(spec.seed.resolve(seeds));
    if let Some(l) = spec.l {
        o = o.with_l(l.clamp(0.0, 1.0));
    }
    if let Some(c) = spec.c {
        o = o.with_c(c.max(0.0));
    }
    if let Some(deg) = spec.rotate {
        o = o.rotate_hue(deg);
    }
    let mut s = o.to_srgb();
    if spec.on {
        s = best_on(s);
    }
    if let Some(a) = spec.alpha {
        s = s.with_alpha(a);
    }
    Color32::from_rgba_unmultiplied(s.r, s.g, s.b, s.a)
}

/// The [`ChromeTheme`] role names the calculator must cover, in field order.
pub const CHROME_ROLES: [&str; 16] = [
    "toolbar_bg",
    "control_bg",
    "control_text",
    "field_bg",
    "field_text",
    "panel_bg",
    "surface_bg",
    "body_text",
    "strong_text",
    "muted_text",
    "active_bg",
    "disabled_text",
    "disabled_bg",
    "menu_bg",
    "error_text",
    "error_bg",
];

impl CustomModeDef {
    /// A complete starter definition — the "+ New custom mode" seed file. A
    /// neutral surface ladder in the declared direction, contrast-picked text
    /// over each computed fill, and primary/danger accents: the same shape the
    /// built-in derivation uses, expressed in the calculator vocabulary so the
    /// authored file is immediately valid AND immediately editable. `dark`
    /// flips the ladder; `high_contrast` pushes it to the extremes.
    pub fn template(id: &str, name: &str, dark: bool, high_contrast: bool) -> Self {
        let l = |dark_l: f64, light_l: f64| -> f64 {
            let (d, li) = if high_contrast {
                (dark_l * 0.4, 1.0 - (1.0 - light_l) * 0.4)
            } else {
                (dark_l, light_l)
            };
            if dark { d } else { li }
        };
        let surface = |dark_l: f64, light_l: f64| RoleSpec {
            seed: SeedRef::Neutral,
            l: Some(l(dark_l, light_l)),
            c: None,
            rotate: None,
            on: false,
            alpha: None,
        };
        let on_surface = |dark_l: f64, light_l: f64| RoleSpec {
            on: true,
            ..surface(dark_l, light_l)
        };
        let accent = |seed: SeedRef| RoleSpec {
            seed,
            l: None,
            c: None,
            rotate: None,
            on: false,
            alpha: None,
        };
        let mut chrome = BTreeMap::new();
        chrome.insert("toolbar_bg".into(), surface(0.205, 0.945));
        chrome.insert("control_bg".into(), surface(0.260, 0.900));
        chrome.insert("control_text".into(), on_surface(0.260, 0.900));
        chrome.insert("field_bg".into(), surface(0.185, 0.985));
        chrome.insert("field_text".into(), on_surface(0.185, 0.985));
        chrome.insert("panel_bg".into(), surface(0.190, 0.965));
        chrome.insert("surface_bg".into(), surface(0.225, 0.975));
        chrome.insert("body_text".into(), on_surface(0.225, 0.975));
        chrome.insert("strong_text".into(), on_surface(0.190, 0.965));
        chrome.insert(
            "muted_text".into(),
            RoleSpec {
                alpha: Some(200),
                ..on_surface(0.225, 0.975)
            },
        );
        chrome.insert("active_bg".into(), accent(SeedRef::Primary));
        chrome.insert(
            "disabled_text".into(),
            RoleSpec {
                alpha: Some(140),
                ..on_surface(0.225, 0.975)
            },
        );
        chrome.insert("disabled_bg".into(), surface(0.185, 0.915));
        chrome.insert("menu_bg".into(), surface(0.215, 0.935));
        chrome.insert("error_text".into(), accent(SeedRef::Danger));
        chrome.insert("error_bg".into(), surface(0.185, 0.915));
        debug_assert!(CHROME_ROLES.iter().all(|r| chrome.contains_key(*r)));
        Self {
            id: id.to_string(),
            name: name.to_string(),
            dark,
            high_contrast,
            chrome,
        }
    }
}

/// Run the calculator: every chrome role evaluated against `seeds`. Errors
/// (naming the missing / unknown roles) rather than half-rendering — a mode
/// file is rejected at load, and the host keeps the prior mode.
pub fn chrome_from_custom_mode(
    def: &CustomModeDef,
    seeds: &Seeds,
) -> Result<crate::chrome::ChromeTheme, String> {
    let missing: Vec<&str> = CHROME_ROLES
        .iter()
        .copied()
        .filter(|r| !def.chrome.contains_key(*r))
        .collect();
    if !missing.is_empty() {
        return Err(format!("mode '{}' missing roles: {}", def.id, missing.join(", ")));
    }
    let unknown: Vec<&String> = def
        .chrome
        .keys()
        .filter(|k| !CHROME_ROLES.contains(&k.as_str()))
        .collect();
    if !unknown.is_empty() {
        let names: Vec<&str> = unknown.iter().map(|s| s.as_str()).collect();
        return Err(format!("mode '{}' unknown roles: {}", def.id, names.join(", ")));
    }
    let role = |name: &str| eval_role(&def.chrome[name], seeds);
    Ok(crate::chrome::ChromeTheme {
        toolbar_bg: role("toolbar_bg"),
        control_bg: role("control_bg"),
        control_text: role("control_text"),
        field_bg: role("field_bg"),
        field_text: role("field_text"),
        panel_bg: role("panel_bg"),
        surface_bg: role("surface_bg"),
        body_text: role("body_text"),
        strong_text: role("strong_text"),
        muted_text: role("muted_text"),
        active_bg: role("active_bg"),
        disabled_text: role("disabled_text"),
        disabled_bg: role("disabled_bg"),
        menu_bg: role("menu_bg"),
        error_text: role("error_text"),
        error_bg: role("error_bg"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeds() -> Seeds {
        Seeds {
            primary: Srgb::rgb(0x33, 0x66, 0xC8),
            secondary: Srgb::rgb(0x2E, 0x9D, 0xA6),
            tertiary: Srgb::rgb(0xE0, 0xA8, 0x46),
            neutral: Srgb::rgb(0x10, 0x14, 0x22),
            text_header: None,
            text_body: None,
            success: Srgb::rgb(0x4F, 0xB3, 0x6E),
            danger: Srgb::rgb(0xD5, 0x4E, 0x4E),
            dark: true,
        }
    }

    fn spec(seed: SeedRef) -> RoleSpec {
        RoleSpec {
            seed,
            l: None,
            c: None,
            rotate: None,
            on: false,
            alpha: None,
        }
    }

    fn full_def() -> CustomModeDef {
        let mut chrome = BTreeMap::new();
        for role in CHROME_ROLES {
            let s = match role {
                r if r.ends_with("_text") => RoleSpec {
                    l: Some(0.92),
                    ..spec(SeedRef::Neutral)
                },
                "active_bg" => spec(SeedRef::Primary),
                _ => RoleSpec {
                    l: Some(0.20),
                    ..spec(SeedRef::Neutral)
                },
            };
            chrome.insert(role.to_string(), s);
        }
        CustomModeDef {
            id: "dusk".into(),
            name: "Dusk".into(),
            dark: true,
            high_contrast: false,
            chrome,
        }
    }

    #[test]
    fn template_is_complete_and_valid_for_all_flag_combos() {
        for (dark, hc) in [(false, false), (true, false), (false, true), (true, true)] {
            let def = CustomModeDef::template("t", "T", dark, hc);
            chrome_from_custom_mode(&def, &seeds())
                .unwrap_or_else(|e| panic!("template (dark={dark}, hc={hc}) invalid: {e}"));
            let json = serde_json::to_string(&def).expect("serializes");
            let back: CustomModeDef = serde_json::from_str(&json).expect("roundtrips");
            // Semantic roundtrip (f64 shortest-repr wobbles exact bits): the
            // reloaded def must still be a complete, valid calculator with the
            // same identity + flags + role set.
            assert_eq!((back.id.as_str(), back.name.as_str(), back.dark, back.high_contrast),
                       (def.id.as_str(), def.name.as_str(), def.dark, def.high_contrast));
            assert_eq!(back.chrome.len(), def.chrome.len());
            chrome_from_custom_mode(&back, &seeds()).expect("reloaded template still valid");
        }
    }

    #[test]
    fn calculator_covers_every_role_and_transforms_apply() {
        let def = full_def();
        let chrome = chrome_from_custom_mode(&def, &seeds()).expect("full table evaluates");
        // Text roles landed light, surfaces dark (the l overrides applied).
        assert!(chrome.body_text.r() > 180, "l: 0.92 lands a light text");
        assert!(chrome.toolbar_bg.r() < 90, "l: 0.20 lands a dark surface");
        // active_bg passed the primary seed through untouched (±rounding).
        assert!((chrome.active_bg.r() as i32 - 0x33).abs() <= 2);
        assert!((chrome.active_bg.b() as i32 - 0xC8).abs() <= 2);
    }

    #[test]
    fn on_picks_readable_text_over_the_computed_fill() {
        let mut def = full_def();
        def.chrome.insert(
            "control_text".into(),
            RoleSpec {
                on: true,
                ..spec(SeedRef::Primary)
            },
        );
        let chrome = chrome_from_custom_mode(&def, &seeds()).unwrap();
        let fill = chrome.active_bg; // primary, untouched
        let lum = |c: Color32| {
            0.2126 * (c.r() as f64) + 0.7152 * (c.g() as f64) + 0.0722 * (c.b() as f64)
        };
        // best_on over the mid-blue primary picks the near-white.
        assert!(lum(chrome.control_text) > lum(fill));
    }

    #[test]
    fn partial_or_unknown_tables_are_rejected_by_name() {
        let mut def = full_def();
        def.chrome.remove("menu_bg");
        let err = chrome_from_custom_mode(&def, &seeds()).unwrap_err();
        assert!(err.contains("menu_bg"), "missing role named: {err}");

        let mut def = full_def();
        def.chrome.insert("tool_bar".into(), spec(SeedRef::Neutral));
        let err = chrome_from_custom_mode(&def, &seeds()).unwrap_err();
        assert!(err.contains("tool_bar"), "unknown role named: {err}");
    }

    #[test]
    fn mode_file_json_roundtrips() {
        // The on-disk shape (`modes/<id>.json`): serde roundtrip + a spot
        // check that the JSON is the compact reviewable form intended.
        let def = full_def();
        let json = serde_json::to_string_pretty(&def).unwrap();
        let back: CustomModeDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
        // Optional fields serialize as null/absent-friendly and seeds are
        // snake_case keys.
        let parsed: CustomModeDef = serde_json::from_str(
            r#"{
                "id": "min", "name": "Min", "dark": true,
                "chrome": { "toolbar_bg": { "seed": "neutral", "l": 0.2 } }
            }"#,
        )
        .unwrap();
        assert_eq!(parsed.chrome["toolbar_bg"].seed, SeedRef::Neutral);
        assert!(chrome_from_custom_mode(&parsed, &seeds()).is_err());
    }
}
