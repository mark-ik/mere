use super::*;

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
fn high_contrast_theme_passes_wcag_validation() {
    validate_theme_tokens(&high_contrast_theme_tokens())
        .expect("high contrast theme should satisfy validation");
}

#[test]
fn builtin_themes_include_edge_tokens() {
    let default = default_theme_tokens();
    let dark = dark_theme_tokens();

    assert!(default.edge_tokens.family_tokens.len() >= 5);
    assert!(dark.edge_tokens.kind_tokens.len() >= 5);
}
