// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use kernel::color::Color32;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum EdgeStyleFamily {
    Semantic,
    Traversal,
    Containment,
    Arrangement,
    Imported,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum EdgeStrokePattern {
    Solid,
    Dotted,
    DashedShort,
    DashedLong,
    DashDot,
    DoubleStroke,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum EdgeEndpointMarker {
    None,
    Arrow,
    Bracket,
    Square,
    Diamond,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum EdgeAccessibilityMode {
    ColorAndPattern,
    Monochrome,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum EdgeStyleKey {
    Hidden,
    Hyperlink,
    UserGrouped,
    AgentDerived,
    TraversalHistory,
    ContainmentUrlPath,
    ContainmentDomain,
    ArrangementFrameMember,
    ArrangementTileGroup,
    ArrangementSplitPair,
    ImportedRelation,
}

impl EdgeStyleKey {
    pub fn family(self) -> Option<EdgeStyleFamily> {
        match self {
            Self::Hidden => None,
            Self::Hyperlink | Self::UserGrouped | Self::AgentDerived => {
                Some(EdgeStyleFamily::Semantic)
            }
            Self::TraversalHistory => Some(EdgeStyleFamily::Traversal),
            Self::ContainmentUrlPath | Self::ContainmentDomain => {
                Some(EdgeStyleFamily::Containment)
            }
            Self::ArrangementFrameMember
            | Self::ArrangementTileGroup
            | Self::ArrangementSplitPair => Some(EdgeStyleFamily::Arrangement),
            Self::ImportedRelation => Some(EdgeStyleFamily::Imported),
        }
    }

    pub fn non_color_signature(self) -> Option<(EdgeStrokePattern, EdgeEndpointMarker)> {
        self.family().map(|_| {
            let token = EdgeStyleRegistry::default().token_for(self, 0.0);
            (token.pattern, token.end_marker)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EdgeStyleToken {
    pub color: Color32,
    pub width: f32,
    pub pattern: EdgeStrokePattern,
    pub opacity: f32,
    pub end_marker: EdgeEndpointMarker,
    pub halo_color: Option<Color32>,
    pub halo_width: f32,
}

impl EdgeStyleToken {
    pub fn resolved_color(self) -> Color32 {
        let alpha = ((self.opacity.clamp(0.0, 1.0)) * 255.0).round() as u8;
        Color32::from_rgba_unmultiplied(self.color.r(), self.color.g(), self.color.b(), alpha)
    }

    pub fn luminance(self) -> f32 {
        let [r, g, b, _] = self.resolved_color().to_array();
        0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThemeEdgeFamilyToken {
    pub color: Color32,
    pub pattern: EdgeStrokePattern,
    pub end_marker: EdgeEndpointMarker,
    pub width: f32,
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThemeEdgeKindToken {
    pub color_override: Option<Color32>,
    pub pattern_override: Option<EdgeStrokePattern>,
    pub end_marker_override: Option<EdgeEndpointMarker>,
    pub width_delta: f32,
    pub opacity_multiplier: f32,
    pub halo_color: Option<Color32>,
    pub halo_width: f32,
}

impl Default for ThemeEdgeKindToken {
    fn default() -> Self {
        Self {
            color_override: None,
            pattern_override: None,
            end_marker_override: None,
            width_delta: 0.0,
            opacity_multiplier: 1.0,
            halo_color: None,
            halo_width: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThemeEdgeEmphasisToken {
    pub foreground_color: Color32,
    pub halo_color: Color32,
    pub halo_width: f32,
    pub width_delta: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThemeAccessibilitySupport {
    pub supports_monochrome: bool,
    pub supports_high_contrast: bool,
    pub default_edge_mode: EdgeAccessibilityMode,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThemeContract {
    pub min_family_luminance_delta: f32,
    pub require_non_color_family_distinction: bool,
    pub require_monochrome_preservation: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThemeEdgeTokens {
    pub family_tokens: BTreeMap<EdgeStyleFamily, ThemeEdgeFamilyToken>,
    pub kind_tokens: BTreeMap<EdgeStyleKey, ThemeEdgeKindToken>,
    pub hover: ThemeEdgeEmphasisToken,
    pub selection: ThemeEdgeEmphasisToken,
}

impl Default for ThemeEdgeTokens {
    fn default() -> Self {
        Self::default_theme()
    }
}

impl ThemeEdgeTokens {
    pub fn default_theme() -> Self {
        Self::with_palette(EdgeThemePalette {
            hyperlink: Color32::from_rgb(150, 150, 155),
            grouped: Color32::from_rgb(236, 171, 64),
            agent: Color32::from_rgb(180, 140, 220),
            traversal: Color32::from_rgb(120, 180, 210),
            containment: Color32::from_rgb(42, 168, 132),
            arrangement: Color32::from_rgb(130, 110, 220),
            imported: Color32::from_rgb(126, 112, 100),
            hover_halo: Color32::from_rgba_unmultiplied(236, 244, 255, 84),
            selection_halo: Color32::from_rgba_unmultiplied(255, 230, 164, 128),
        })
    }

    pub fn light_theme() -> Self {
        Self::with_palette(EdgeThemePalette {
            hyperlink: Color32::from_rgb(108, 112, 122),
            grouped: Color32::from_rgb(196, 136, 36),
            agent: Color32::from_rgb(148, 112, 196),
            traversal: Color32::from_rgb(66, 128, 170),
            containment: Color32::from_rgb(10, 130, 92),
            arrangement: Color32::from_rgb(94, 88, 182),
            imported: Color32::from_rgb(80, 72, 68),
            hover_halo: Color32::from_rgba_unmultiplied(40, 78, 114, 54),
            selection_halo: Color32::from_rgba_unmultiplied(214, 160, 56, 108),
        })
    }

    pub fn dark_theme() -> Self {
        Self::with_palette(EdgeThemePalette {
            hyperlink: Color32::from_rgb(164, 168, 178),
            grouped: Color32::from_rgb(236, 171, 64),
            agent: Color32::from_rgb(188, 148, 232),
            traversal: Color32::from_rgb(124, 186, 220),
            containment: Color32::from_rgb(52, 176, 140),
            arrangement: Color32::from_rgb(142, 124, 236),
            imported: Color32::from_rgb(124, 112, 104),
            hover_halo: Color32::from_rgba_unmultiplied(214, 230, 255, 72),
            selection_halo: Color32::from_rgba_unmultiplied(255, 220, 144, 132),
        })
    }

    pub fn high_contrast_theme() -> Self {
        Self::with_palette(EdgeThemePalette {
            hyperlink: Color32::from_rgb(255, 255, 255),
            grouped: Color32::from_rgb(255, 230, 0),
            agent: Color32::from_rgb(255, 255, 255),
            traversal: Color32::from_rgb(0, 255, 255),
            containment: Color32::from_rgb(0, 255, 170),
            arrangement: Color32::from_rgb(255, 128, 0),
            imported: Color32::from_rgb(188, 188, 188),
            hover_halo: Color32::from_rgba_unmultiplied(255, 255, 255, 120),
            selection_halo: Color32::from_rgba_unmultiplied(255, 230, 0, 156),
        })
    }

    fn with_palette(palette: EdgeThemePalette) -> Self {
        let family_tokens = BTreeMap::from([
            (
                EdgeStyleFamily::Semantic,
                ThemeEdgeFamilyToken {
                    color: palette.hyperlink,
                    pattern: EdgeStrokePattern::Solid,
                    end_marker: EdgeEndpointMarker::Arrow,
                    width: 1.4,
                    opacity: 0.85,
                },
            ),
            (
                EdgeStyleFamily::Traversal,
                ThemeEdgeFamilyToken {
                    color: palette.traversal,
                    pattern: EdgeStrokePattern::DashedLong,
                    end_marker: EdgeEndpointMarker::Arrow,
                    width: 1.8,
                    opacity: 0.7,
                },
            ),
            (
                EdgeStyleFamily::Containment,
                ThemeEdgeFamilyToken {
                    color: palette.containment,
                    pattern: EdgeStrokePattern::Dotted,
                    end_marker: EdgeEndpointMarker::Bracket,
                    width: 1.0,
                    opacity: 0.6,
                },
            ),
            (
                EdgeStyleFamily::Arrangement,
                ThemeEdgeFamilyToken {
                    color: palette.arrangement,
                    pattern: EdgeStrokePattern::DoubleStroke,
                    end_marker: EdgeEndpointMarker::Square,
                    width: 2.0,
                    opacity: 0.5,
                },
            ),
            (
                EdgeStyleFamily::Imported,
                ThemeEdgeFamilyToken {
                    color: palette.imported,
                    pattern: EdgeStrokePattern::DashDot,
                    end_marker: EdgeEndpointMarker::Diamond,
                    width: 0.8,
                    opacity: 0.35,
                },
            ),
        ]);
        let kind_tokens = BTreeMap::from([
            (
                EdgeStyleKey::Hyperlink,
                ThemeEdgeKindToken {
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::UserGrouped,
                ThemeEdgeKindToken {
                    color_override: Some(palette.grouped),
                    end_marker_override: Some(EdgeEndpointMarker::None),
                    width_delta: 1.6,
                    opacity_multiplier: 1.0 / 0.85,
                    halo_color: Some(Color32::from_rgba_unmultiplied(
                        palette.grouped.r(),
                        palette.grouped.g(),
                        palette.grouped.b(),
                        96,
                    )),
                    halo_width: 1.0,
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::AgentDerived,
                ThemeEdgeKindToken {
                    color_override: Some(palette.agent),
                    pattern_override: Some(EdgeStrokePattern::DashDot),
                    end_marker_override: Some(EdgeEndpointMarker::Diamond),
                    width_delta: -0.2,
                    opacity_multiplier: 1.0,
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::TraversalHistory,
                ThemeEdgeKindToken {
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::ContainmentUrlPath,
                ThemeEdgeKindToken {
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::ContainmentDomain,
                ThemeEdgeKindToken {
                    pattern_override: Some(EdgeStrokePattern::DashedShort),
                    width_delta: -0.2,
                    opacity_multiplier: 0.4 / 0.6,
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::ArrangementFrameMember,
                ThemeEdgeKindToken {
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::ArrangementTileGroup,
                ThemeEdgeKindToken {
                    pattern_override: Some(EdgeStrokePattern::Dotted),
                    width_delta: -1.2,
                    opacity_multiplier: 0.4 / 0.5,
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::ArrangementSplitPair,
                ThemeEdgeKindToken {
                    pattern_override: Some(EdgeStrokePattern::DashedShort),
                    width_delta: -1.0,
                    opacity_multiplier: 0.45 / 0.5,
                    ..ThemeEdgeKindToken::default()
                },
            ),
            (
                EdgeStyleKey::ImportedRelation,
                ThemeEdgeKindToken {
                    ..ThemeEdgeKindToken::default()
                },
            ),
        ]);
        Self {
            family_tokens,
            kind_tokens,
            hover: ThemeEdgeEmphasisToken {
                foreground_color: Color32::from_rgb(210, 226, 242),
                halo_color: palette.hover_halo,
                halo_width: 1.0,
                width_delta: 0.5,
            },
            selection: ThemeEdgeEmphasisToken {
                foreground_color: palette.grouped,
                halo_color: palette.selection_halo,
                halo_width: 1.2,
                width_delta: 1.0,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct EdgeStyleRegistry {
    accessibility_mode: EdgeAccessibilityMode,
    theme_tokens: ThemeEdgeTokens,
}

impl Default for EdgeStyleRegistry {
    fn default() -> Self {
        Self {
            accessibility_mode: EdgeAccessibilityMode::ColorAndPattern,
            theme_tokens: ThemeEdgeTokens::default_theme(),
        }
    }
}

impl EdgeStyleRegistry {
    pub fn new(accessibility_mode: EdgeAccessibilityMode) -> Self {
        Self {
            accessibility_mode,
            ..Self::default()
        }
    }

    pub fn from_theme_tokens(
        theme_tokens: ThemeEdgeTokens,
        accessibility_mode: EdgeAccessibilityMode,
    ) -> Self {
        Self {
            accessibility_mode,
            theme_tokens,
        }
    }

    pub fn token_for(&self, key: EdgeStyleKey, decay_progress: f32) -> EdgeStyleToken {
        if key == EdgeStyleKey::Hidden {
            return EdgeStyleToken {
                color: Color32::TRANSPARENT,
                width: 0.0,
                pattern: EdgeStrokePattern::Solid,
                opacity: 0.0,
                end_marker: EdgeEndpointMarker::None,
                halo_color: None,
                halo_width: 0.0,
            };
        }

        let family = key
            .family()
            .expect("non-hidden edge styles must resolve to a family");
        let family_token = self.theme_tokens.family_tokens[&family];
        let kind_token = self
            .theme_tokens
            .kind_tokens
            .get(&key)
            .copied()
            .unwrap_or_default();

        let token = EdgeStyleToken {
            color: kind_token.color_override.unwrap_or(family_token.color),
            width: (family_token.width + kind_token.width_delta).max(0.6),
            pattern: kind_token.pattern_override.unwrap_or(family_token.pattern),
            opacity: match key {
                EdgeStyleKey::AgentDerived => lerp(0.55, 0.15, decay_progress.clamp(0.0, 1.0)),
                _ => {
                    (family_token.opacity * kind_token.opacity_multiplier.max(0.0)).clamp(0.0, 1.0)
                }
            },
            end_marker: kind_token
                .end_marker_override
                .unwrap_or(family_token.end_marker),
            halo_color: kind_token.halo_color,
            halo_width: kind_token.halo_width,
        };

        match self.accessibility_mode {
            EdgeAccessibilityMode::ColorAndPattern => token,
            EdgeAccessibilityMode::Monochrome => EdgeStyleToken {
                color: Color32::from_gray(monochrome_value(key)),
                ..token
            },
        }
    }
}

pub fn validate_theme_edge_tokens(
    tokens: &ThemeEdgeTokens,
    contract: &ThemeContract,
) -> Result<(), String> {
    for family in [
        EdgeStyleFamily::Semantic,
        EdgeStyleFamily::Traversal,
        EdgeStyleFamily::Containment,
        EdgeStyleFamily::Arrangement,
        EdgeStyleFamily::Imported,
    ] {
        if !tokens.family_tokens.contains_key(&family) {
            return Err(format!("missing family edge token for {family:?}"));
        }
    }

    if contract.require_non_color_family_distinction {
        let mut signatures = BTreeSet::new();
        for family in [
            EdgeStyleFamily::Semantic,
            EdgeStyleFamily::Traversal,
            EdgeStyleFamily::Containment,
            EdgeStyleFamily::Arrangement,
            EdgeStyleFamily::Imported,
        ] {
            let token = tokens.family_tokens[&family];
            if !signatures.insert((token.pattern, token.end_marker)) {
                return Err(format!(
                    "family edge signature collision for {family:?}; pattern+marker must remain unique"
                ));
            }
        }
    }

    let families = [
        EdgeStyleFamily::Semantic,
        EdgeStyleFamily::Traversal,
        EdgeStyleFamily::Containment,
        EdgeStyleFamily::Arrangement,
        EdgeStyleFamily::Imported,
    ];
    for (index, family) in families.iter().enumerate() {
        let a = edge_family_luminance(tokens.family_tokens[family]);
        for other in families.iter().skip(index + 1) {
            let b = edge_family_luminance(tokens.family_tokens[other]);
            if (a - b).abs() < contract.min_family_luminance_delta {
                return Err(format!(
                    "family luminance delta for {family:?} and {other:?} below {:.1}",
                    contract.min_family_luminance_delta
                ));
            }
        }
    }

    if contract.require_monochrome_preservation {
        let mut signatures = BTreeSet::new();
        for key in [
            EdgeStyleKey::Hyperlink,
            EdgeStyleKey::TraversalHistory,
            EdgeStyleKey::ContainmentUrlPath,
            EdgeStyleKey::ArrangementFrameMember,
            EdgeStyleKey::ImportedRelation,
        ] {
            let token = EdgeStyleRegistry::from_theme_tokens(
                tokens.clone(),
                EdgeAccessibilityMode::Monochrome,
            )
            .token_for(key, 0.0);
            if !signatures.insert((token.pattern, token.end_marker)) {
                return Err(format!(
                    "monochrome preservation failed for {key:?}; non-color signature collision detected"
                ));
            }
        }
    }

    Ok(())
}

fn edge_family_luminance(token: ThemeEdgeFamilyToken) -> f32 {
    let [r, g, b, _] = token.color.to_array();
    0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)
}

fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

fn monochrome_value(key: EdgeStyleKey) -> u8 {
    match key.family() {
        None => 0,
        Some(EdgeStyleFamily::Semantic) => 232,
        Some(EdgeStyleFamily::Traversal) => 196,
        Some(EdgeStyleFamily::Containment) => 164,
        Some(EdgeStyleFamily::Arrangement) => 128,
        Some(EdgeStyleFamily::Imported) => 96,
    }
}

#[derive(Debug, Clone, Copy)]
struct EdgeThemePalette {
    hyperlink: Color32,
    grouped: Color32,
    agent: Color32,
    traversal: Color32,
    containment: Color32,
    arrangement: Color32,
    imported: Color32,
    hover_halo: Color32,
    selection_halo: Color32,
}

#[cfg(test)]
mod tests;
