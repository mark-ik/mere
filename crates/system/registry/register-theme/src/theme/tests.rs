// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn builtin(id: &str) -> ThemeTokenSet {
    crate::seed::builtin_token_sets()
        .into_iter()
        .find(|t| t.theme_id == id)
        .expect("built-in present")
}

#[test]
fn theme_registry_resolves_builtin_themes_and_fallbacks() {
    let registry = ThemeRegistry::default();
    let dark = registry.resolve_theme(Some(THEME_ID_DARK));
    assert!(dark.matched);
    assert_eq!(dark.resolved_id, THEME_ID_DARK);

    let fallback = registry.resolve_theme(Some("theme:missing"));
    assert!(fallback.fallback_used);
    assert_eq!(fallback.resolved_id, THEME_ID_DEFAULT);
}

#[test]
fn all_seed_derived_builtins_pass_wcag_validation() {
    // The derivation must clear the contrast gate for every built-in — the
    // whole point of the seed engine being the single authoring path.
    for tokens in crate::seed::builtin_token_sets() {
        let id = tokens.theme_id.clone();
        validate_theme_tokens(&tokens)
            .unwrap_or_else(|e| panic!("built-in {id} failed validation: {e}"));
    }
}

#[test]
fn builtin_themes_include_edge_tokens() {
    let default = builtin(THEME_ID_DEFAULT);
    let dark = builtin(THEME_ID_DARK);

    assert!(default.edge_tokens.family_tokens.len() >= 5);
    assert!(dark.edge_tokens.kind_tokens.len() >= 5);
}

#[test]
fn registry_lists_built_ins_in_order() {
    let registry = ThemeRegistry::default();
    let ids: Vec<&str> = registry.list().iter().map(|d| d.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            THEME_ID_DEFAULT,
            THEME_ID_DARK,
            THEME_ID_LIGHT,
            THEME_ID_HIGH_CONTRAST
        ]
    );
}

#[test]
fn user_theme_crud_fork_rename_remove_add() {
    use tincture::{Seeds, Srgb};

    let mut reg = ThemeRegistry::default();
    let builtins = reg.list().len();

    // Fork a built-in into a user theme (the non-destructive edit path).
    let forked = reg
        .fork(THEME_ID_DEFAULT, "user:test", "My Theme")
        .expect("fork succeeds");
    assert_eq!(forked.source, ThemeSource::User);
    assert_eq!(reg.list().len(), builtins + 1);
    assert!(reg.resolve_theme(Some("user:test")).matched);
    // The built-in is untouched.
    assert!(reg.resolve_theme(Some(THEME_ID_DEFAULT)).matched);

    // Rename a user theme in place; built-ins can't be renamed or removed.
    assert!(reg.rename_user_theme("user:test", "Renamed"));
    assert_eq!(reg.theme_def("user:test").unwrap().name, "Renamed");
    assert!(!reg.rename_user_theme(THEME_ID_DEFAULT, "Nope"));
    assert!(!reg.remove_user_theme(THEME_ID_DEFAULT));

    // Remove the user theme; resolving it now falls back gracefully.
    assert!(reg.remove_user_theme("user:test"));
    assert_eq!(reg.list().len(), builtins);
    assert!(reg.resolve_theme(Some("user:test")).fallback_used);

    // Add a fresh user theme from seeds — derivation must pass validation.
    let def = ThemeDef {
        id: "user:green".into(),
        name: "Green".into(),
        source: ThemeSource::User,
        seeds: Seeds {
            primary: Srgb::rgb(0x4F, 0xB3, 0x6E),
            secondary: Srgb::rgb(0x2E, 0x9D, 0xA6),
            tertiary: Srgb::rgb(0xE0, 0xA8, 0x46),
            neutral: Srgb::rgb(0x10, 0x1E, 0x14),
            text_header: None,
            text_body: None,
            success: Srgb::rgb(0x4F, 0xB3, 0x6E),
            danger: Srgb::rgb(0xD5, 0x4E, 0x4E),
            dark: true,
        },
        high_contrast: false,
        harmony: Default::default(),
        mode_sheets: Default::default(),
    };
    reg.add_user_theme(def)
        .expect("user theme passes validation");
    assert!(reg.resolve_theme(Some("user:green")).matched);
}

#[test]
fn locked_harmony_rotates_accents_to_primary_plus_offset_and_validates() {
    use tincture::oklch::Oklch;
    use tincture::{Seeds, Srgb};
    // Saturated seeds so OKLCH hue is well-defined (low-chroma greys have no hue).
    let def = ThemeDef {
        id: "user:harmony".into(),
        name: "Harmony".into(),
        source: ThemeSource::User,
        seeds: Seeds {
            primary: Srgb::rgb(0xC0, 0x30, 0x30),
            secondary: Srgb::rgb(0x30, 0xC0, 0x30),
            tertiary: Srgb::rgb(0x30, 0x30, 0xC0),
            neutral: Srgb::rgb(0x12, 0x12, 0x16),
            text_header: None,
            text_body: None,
            success: Srgb::rgb(0x4F, 0xB3, 0x6E),
            danger: Srgb::rgb(0xD5, 0x4E, 0x4E),
            dark: true,
        },
        high_contrast: false,
        harmony: Harmony::Locked {
            secondary_deg: 120.0,
            tertiary_deg: 240.0,
        },
        mode_sheets: Default::default(),
    };

    // The accents' hue is the primary's hue plus the locked offset; each accent
    // keeps its own chroma + lightness (only the hue is tied).
    let eff = crate::seed::harmonized_seeds(&def);
    let norm = |d: f64| ((d % 360.0) + 360.0) % 360.0;
    let h = |c| Oklch::from_srgb(c).h.to_degrees();
    let base = h(eff.primary);
    assert!(
        (norm(h(eff.secondary) - base) - 120.0).abs() < 1.0,
        "secondary should be base + 120, got {}",
        norm(h(eff.secondary) - base)
    );
    assert!(
        (norm(h(eff.tertiary) - base) - 240.0).abs() < 1.0,
        "tertiary should be base + 240, got {}",
        norm(h(eff.tertiary) - base)
    );

    // A harmonized theme still derives + clears contrast validation.
    let mut reg = ThemeRegistry::default();
    reg.add_user_theme(def).expect("harmonized theme validates");
}

#[test]
fn theme_def_round_trips_through_serde() {
    // Theme files (T4) are serde ThemeDefs — confirm a def round-trips.
    let def = crate::seed::builtin_defs().swap_remove(0);
    let json = serde_json::to_string(&def).expect("serialize");
    let back: ThemeDef = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(def, back);
}

#[test]
fn user_theme_edit_helpers_only_mutate_user_defs() {
    let mut built_in = crate::seed::builtin_defs().swap_remove(0);
    assert!(!toggle_user_theme_mode(&mut built_in));
    assert!(!set_user_theme_seed_channel(
        &mut built_in,
        "primary",
        'h',
        0.25
    ));
    assert!(!set_user_theme_harmony(&mut built_in, "triadic"));

    built_in.source = ThemeSource::User;
    assert!(toggle_user_theme_mode(&mut built_in));
    assert!(set_user_theme_seed_channel(
        &mut built_in,
        "primary",
        's',
        0.5
    ));
    assert!(set_user_theme_harmony(&mut built_in, "triadic"));
}

#[test]
fn mode_key_roundtrips() {
    for mode in [
        Mode::Light,
        Mode::Dark,
        Mode::HcLight,
        Mode::HcDark,
        Mode::Custom("solar".to_string()),
    ] {
        assert_eq!(Mode::from_key(&mode.as_key()), Some(mode.clone()));
    }
    assert_eq!(Mode::from_key("nope"), None);
}

#[test]
fn mode_tokens_derive_four_distinct_palettes_from_one_theme() {
    // Theme-modes T1: one theme (one seed set), four canonical modes, four
    // distinct derivations; hc modes clear the 7:1 body-text gate and are
    // wider than their normal-contrast counterpart.
    let registry = ThemeRegistry::default();
    let tokens = |mode: &Mode| {
        registry
            .mode_tokens(THEME_ID_DEFAULT, mode)
            .expect("default theme resolves")
    };
    let light = tokens(&Mode::Light);
    let dark = tokens(&Mode::Dark);
    let hc_light = tokens(&Mode::HcLight);
    let hc_dark = tokens(&Mode::HcDark);

    let panels = [
        light.chrome.panel_bg,
        dark.chrome.panel_bg,
        hc_light.chrome.panel_bg,
        hc_dark.chrome.panel_bg,
    ];
    for i in 0..panels.len() {
        for j in (i + 1)..panels.len() {
            assert_ne!(panels[i], panels[j], "modes {i} and {j} share a panel bg");
        }
    }

    for (label, hc, normal) in [
        ("hc_light", &hc_light, &light),
        ("hc_dark", &hc_dark, &dark),
    ] {
        let hc_ratio = contrast_ratio(hc.chrome.body_text, hc.chrome.panel_bg);
        let normal_ratio = contrast_ratio(normal.chrome.body_text, normal.chrome.panel_bg);
        assert!(hc_ratio >= 7.0, "{label} body/panel {hc_ratio:.2} < 7.0");
        assert!(
            hc_ratio > normal_ratio,
            "{label} {hc_ratio:.2} not wider than {normal_ratio:.2}"
        );
    }
}

#[test]
fn default_mode_for_def_matches_legacy_builtins() {
    // Activating a legacy built-in re-seeds the mode from its def, so the
    // four pre-modes themes keep their meaning.
    let registry = ThemeRegistry::default();
    let mode_of =
        |id: &str| crate::seed::default_mode_for_def(registry.theme_def(id).expect("builtin def"));
    assert_eq!(mode_of(THEME_ID_DEFAULT), Mode::Dark);
    assert_eq!(mode_of(THEME_ID_DARK), Mode::Dark);
    assert_eq!(mode_of(THEME_ID_LIGHT), Mode::Light);
    assert_eq!(mode_of(THEME_ID_HIGH_CONTRAST), Mode::HcDark);
}

#[test]
fn mode_sheets_roundtrip_and_gate_on_non_empty() {
    // T4: per-mode custom sheets ride the ThemeDef serde (the theme-file
    // persistence path), old files without the field still parse, and an
    // empty rule list counts as no override.
    let mut def = crate::seed::builtin_defs().swap_remove(0);
    def.id = "user:t4-override".to_string();
    def.mode_sheets.insert(
        Mode::Dark.as_key(),
        vec![".toolbar { background-color: rgb(1, 2, 3); }".to_string()],
    );
    def.mode_sheets.insert(Mode::Light.as_key(), Vec::new());

    let json = serde_json::to_string(&def).unwrap();
    let back: ThemeDef = serde_json::from_str(&json).unwrap();
    assert_eq!(back, def, "mode_sheets survive the theme-file roundtrip");
    assert!(back.mode_sheet(&Mode::Dark).is_some());
    assert!(
        back.mode_sheet(&Mode::Light).is_none(),
        "an empty entry is treated as absent"
    );
    assert!(back.mode_sheet(&Mode::HcDark).is_none());

    // A pre-T4 file (no mode_sheets field) parses with no overrides.
    let legacy: ThemeDef = serde_json::from_str(
        &serde_json::to_string(&crate::seed::builtin_defs().swap_remove(0)).unwrap(),
    )
    .unwrap();
    assert!(legacy.mode_sheets.is_empty());

    // A fork carries the overrides.
    let mut registry = ThemeRegistry::default();
    registry.add_user_theme(def.clone()).unwrap();
    let fork = registry
        .fork(&def.id, "user:fork-t4", "Fork")
        .expect("fork succeeds");
    assert_eq!(fork.mode_sheets, def.mode_sheets);
}
