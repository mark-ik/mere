// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use kernel::color::Color32;

use super::{LayoutMode, PhysicsProfile, THEME_ID_DEFAULT, ThemeData, resolve_theme_data};

pub const LENS_ID_DEFAULT: &str = "lens:default";
pub const LENS_ID_SEMANTIC_OVERLAY: &str = "lens:semantic_overlay";

#[derive(Debug, Clone)]
pub struct LensDefinition {
    pub display_name: String,
    pub physics: PhysicsProfile,
    pub layout: LayoutMode,
    pub layout_algorithm_id: String,
    pub theme: Option<ThemeData>,
    pub filters: Vec<String>,
    pub overlay_descriptor: Option<LensOverlayDescriptor>,
}

// `GlyphOverlay` + `GlyphAnchor` live in `kernel::overlay`. Re-exported
// here so lens call sites resolve unchanged.
pub use kernel::overlay::{GlyphAnchor, GlyphOverlay};

#[derive(Debug, Clone, PartialEq)]
pub struct LensOverlayDescriptor {
    pub border_tint: Option<Color32>,
    pub glyph_overlays: Vec<GlyphOverlay>,
    pub opacity_scale: f32,
    pub suppress_default_affordances: bool,
}

#[derive(Debug, Clone)]
pub struct LensDescriptor {
    pub applicable_mime_types: Vec<String>,
    pub priority: u8,
    pub requires_knowledge: bool,
    pub requires_graph_context: bool,
}

#[derive(Debug, Clone)]
struct RegisteredLens {
    descriptor: LensDescriptor,
    definition: LensDefinition,
}

#[derive(Debug, Clone)]
pub struct LensResolution {
    pub requested_id: String,
    pub resolved_id: String,
    pub matched: bool,
    pub fallback_used: bool,
    pub definition: LensDefinition,
}

pub struct LensRegistry {
    lenses: HashMap<String, RegisteredLens>,
    fallback_id: String,
}

impl LensRegistry {
    pub fn register(&mut self, lens_id: &str, definition: LensDefinition) {
        self.register_with_descriptor(lens_id, LensDescriptor::default(), definition);
    }

    pub fn register_with_descriptor(
        &mut self,
        lens_id: &str,
        descriptor: LensDescriptor,
        definition: LensDefinition,
    ) {
        self.lenses.insert(
            lens_id.to_ascii_lowercase(),
            RegisteredLens {
                descriptor,
                definition,
            },
        );
    }

    pub fn unregister(&mut self, lens_id: &str) -> bool {
        let normalized = lens_id.trim().to_ascii_lowercase();
        if normalized == self.fallback_id {
            return false;
        }
        self.lenses.remove(&normalized).is_some()
    }

    pub fn resolve(&self, lens_id: &str) -> LensResolution {
        let requested = lens_id.trim().to_ascii_lowercase();
        let fallback_lens = self
            .lenses
            .get(&self.fallback_id)
            .map(|registered| registered.definition.clone())
            .unwrap_or_else(default_lens_definition);

        if requested.is_empty() {
            return LensResolution {
                requested_id: requested,
                resolved_id: self.fallback_id.clone(),
                matched: false,
                fallback_used: true,
                definition: fallback_lens,
            };
        }

        if let Some(lens) = self.lenses.get(&requested) {
            return LensResolution {
                requested_id: requested.clone(),
                resolved_id: requested,
                matched: true,
                fallback_used: false,
                definition: lens.definition.clone(),
            };
        }

        LensResolution {
            requested_id: requested,
            resolved_id: self.fallback_id.clone(),
            matched: false,
            fallback_used: true,
            definition: fallback_lens,
        }
    }

    pub fn resolve_for_content(
        &self,
        mime_hint: Option<&str>,
        has_semantic_context: bool,
    ) -> Vec<String> {
        let normalized_mime = mime_hint.map(|value| value.trim().to_ascii_lowercase());
        let mut matches = self
            .lenses
            .iter()
            .filter_map(|(lens_id, registered)| {
                if registered.descriptor.requires_knowledge && !has_semantic_context {
                    return None;
                }

                let mime_matches = registered.descriptor.applicable_mime_types.is_empty()
                    || normalized_mime.as_deref().is_some_and(|mime| {
                        registered
                            .descriptor
                            .applicable_mime_types
                            .iter()
                            .any(|candidate| candidate == mime)
                    });
                if !mime_matches {
                    return None;
                }

                Some((registered.descriptor.priority, lens_id.clone()))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|(left_priority, left_id), (right_priority, right_id)| {
            right_priority
                .cmp(left_priority)
                .then_with(|| left_id.cmp(right_id))
        });

        if matches.is_empty() {
            return vec![self.fallback_id.clone()];
        }

        matches.into_iter().map(|(_, lens_id)| lens_id).collect()
    }

    pub fn compose(&self, lens_ids: &[String]) -> LensDefinition {
        let mut iter = lens_ids.iter();
        let base_id = iter
            .next()
            .cloned()
            .unwrap_or_else(|| self.fallback_id.clone());
        let mut composed = self.resolve(&base_id).definition;

        for lens_id in iter {
            let resolution = self.resolve(lens_id);
            for filter in resolution.definition.filters {
                if !composed.filters.contains(&filter) {
                    composed.filters.push(filter);
                }
            }
            if composed.theme.is_none() {
                composed.theme = resolution.definition.theme;
            }
            if composed.overlay_descriptor.is_none() {
                composed.overlay_descriptor = resolution.definition.overlay_descriptor;
            }
        }

        composed
    }
}

impl Default for LensRegistry {
    fn default() -> Self {
        let mut registry = Self {
            lenses: HashMap::new(),
            fallback_id: LENS_ID_DEFAULT.to_string(),
        };
        registry.register(LENS_ID_DEFAULT, default_lens_definition());
        registry.register_with_descriptor(
            LENS_ID_SEMANTIC_OVERLAY,
            LensDescriptor {
                applicable_mime_types: vec![
                    "text/html".to_string(),
                    "text/markdown".to_string(),
                    "application/pdf".to_string(),
                    "text/plain".to_string(),
                ],
                priority: 10,
                requires_knowledge: true,
                requires_graph_context: true,
            },
            LensDefinition {
                display_name: "Semantic Overlay".to_string(),
                physics: PhysicsProfile::default(),
                layout: LayoutMode::Free,
                layout_algorithm_id: "graph_layout:force_directed".to_string(),
                theme: Some(resolve_theme_data(THEME_ID_DEFAULT).theme),
                filters: vec!["semantic:overlay".to_string()],
                overlay_descriptor: Some(LensOverlayDescriptor {
                    border_tint: Some(Color32::from_rgb(120, 210, 255)),
                    glyph_overlays: vec![GlyphOverlay {
                        glyph_id: "semantic".to_string(),
                        anchor: GlyphAnchor::TopRight,
                    }],
                    opacity_scale: 1.1,
                    suppress_default_affordances: false,
                }),
            },
        );
        registry
    }
}

impl Default for LensDescriptor {
    fn default() -> Self {
        Self {
            applicable_mime_types: Vec::new(),
            priority: 0,
            requires_knowledge: false,
            requires_graph_context: false,
        }
    }
}

fn default_lens_definition() -> LensDefinition {
    LensDefinition {
        display_name: "Default".to_string(),
        physics: PhysicsProfile::default(),
        layout: LayoutMode::Free,
        layout_algorithm_id: "graph_layout:force_directed".to_string(),
        theme: Some(resolve_theme_data(THEME_ID_DEFAULT).theme),
        filters: Vec::new(),
        overlay_descriptor: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lens_registry_resolves_default_lens() {
        let registry = LensRegistry::default();
        let resolution = registry.resolve(LENS_ID_DEFAULT);

        assert!(resolution.matched);
        assert!(!resolution.fallback_used);
        assert_eq!(resolution.resolved_id, LENS_ID_DEFAULT);
        assert_eq!(resolution.definition.display_name, "Default");
    }

    #[test]
    fn lens_registry_falls_back_for_unknown_lens() {
        let registry = LensRegistry::default();
        let resolution = registry.resolve("lens:unknown");

        assert!(!resolution.matched);
        assert!(resolution.fallback_used);
        assert_eq!(resolution.resolved_id, LENS_ID_DEFAULT);
        assert_eq!(resolution.definition.display_name, "Default");
    }

    #[test]
    fn lens_registry_resolves_semantic_overlay_for_semantic_content() {
        let registry = LensRegistry::default();
        let resolved = registry.resolve_for_content(Some("text/markdown"), true);

        assert_eq!(
            resolved.first().map(String::as_str),
            Some(LENS_ID_SEMANTIC_OVERLAY)
        );
        let composed = registry.compose(&resolved);
        assert!(
            composed
                .filters
                .iter()
                .any(|filter| filter == "semantic:overlay")
        );
        assert!(composed.overlay_descriptor.is_some());
    }

    #[test]
    fn lens_registry_falls_back_to_default_without_semantic_context() {
        let registry = LensRegistry::default();
        let resolved = registry.resolve_for_content(Some("text/markdown"), false);

        assert_eq!(resolved, vec![LENS_ID_DEFAULT.to_string()]);
    }
}
