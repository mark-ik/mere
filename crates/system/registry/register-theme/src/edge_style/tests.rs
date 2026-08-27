// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn family_styles_have_distinct_non_color_signatures() {
    let families = [
        EdgeStyleKey::Hyperlink,
        EdgeStyleKey::TraversalHistory,
        EdgeStyleKey::ContainmentUrlPath,
        EdgeStyleKey::ArrangementFrameMember,
        EdgeStyleKey::ImportedRelation,
    ];

    for (index, key) in families.iter().enumerate() {
        for other in families.iter().skip(index + 1) {
            assert_ne!(
                key.non_color_signature(),
                other.non_color_signature(),
                "{key:?} should not collide with {other:?} on pattern+marker",
            );
        }
    }
}

#[test]
fn monochrome_mode_preserves_pattern_identity() {
    let registry = EdgeStyleRegistry::new(EdgeAccessibilityMode::Monochrome);
    let hyperlink = registry.token_for(EdgeStyleKey::Hyperlink, 0.0);
    let arrangement = registry.token_for(EdgeStyleKey::ArrangementFrameMember, 0.0);

    assert_eq!(hyperlink.color.r(), hyperlink.color.g());
    assert_eq!(arrangement.color.r(), arrangement.color.g());
    assert_ne!(hyperlink.pattern, arrangement.pattern);
}

#[test]
fn agent_decay_reduces_opacity() {
    let registry = EdgeStyleRegistry::default();
    let fresh = registry.token_for(EdgeStyleKey::AgentDerived, 0.0);
    let stale = registry.token_for(EdgeStyleKey::AgentDerived, 1.0);

    assert!(fresh.opacity > stale.opacity);
}

#[test]
fn grayscale_luminance_separates_semantic_from_arrangement() {
    let registry = EdgeStyleRegistry::new(EdgeAccessibilityMode::Monochrome);
    let semantic = registry.token_for(EdgeStyleKey::UserGrouped, 0.0);
    let arrangement = registry.token_for(EdgeStyleKey::ArrangementTileGroup, 0.0);

    assert!((semantic.luminance() - arrangement.luminance()).abs() >= 20.0);
}

#[test]
fn theme_tokens_satisfy_default_contract() {
    let contract = ThemeContract {
        min_family_luminance_delta: 4.0,
        require_non_color_family_distinction: true,
        require_monochrome_preservation: true,
    };

    validate_theme_edge_tokens(&ThemeEdgeTokens::dark_theme(), &contract)
        .expect("dark theme edge tokens should satisfy contract");
    validate_theme_edge_tokens(&ThemeEdgeTokens::high_contrast_theme(), &contract)
        .expect("high contrast edge tokens should satisfy contract");
}
