/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `NodeStyle` — user-tunable appearance for the nodes themselves.
//!
//! Sibling to `NavigationPolicy`. Where that struct owns the *feel* of
//! camera, input, and inertia, this one owns the *look* of individual
//! nodes: default radius, selection fills, search-hit highlight. Lives
//! in graph-canvas so the iced host reads the same resolved style as
//! the current egui host without a parallel defaults table.
//!
//! The host computes its per-node state (selected / primary /
//! search-hit / neither) and projects it into a
//! [`crate::derive::NodeVisualOverride`] using this style. Anything
//! the host doesn't override falls back to the derive pipeline's
//! built-in defaults.

use serde::{Deserialize, Serialize};

use crate::packet::{Color, Stroke};

/// Visual treatment for a single node state (primary-selected,
/// secondary-selected, search-hit). Combines fill, stroke, and label
/// color in one struct so users can theme each state independently.
///
/// `stroke` is `Option<Stroke>` so a state can declare "no stroke"
/// (some themes prefer fill-only indicators for secondary selection).
/// `label_color` is `Option<Color>` so the state can either force a
/// specific label color or inherit whatever the derive pipeline picks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeStateStyle {
    pub fill: Color,
    pub stroke: Option<Stroke>,
    pub label_color: Option<Color>,
}

impl NodeStateStyle {
    pub const fn new(fill: Color, stroke: Option<Stroke>, label_color: Option<Color>) -> Self {
        Self {
            fill,
            stroke,
            label_color,
        }
    }
}

/// Default radius in world units for nodes that don't carry a
/// per-node radius override. Host `build_scene_input` seeds this as
/// the radius for each `CanvasNode` it constructs from the domain
/// graph.
pub const DEFAULT_NODE_RADIUS: f32 = 16.0;

/// Tunable node appearance — default radius + per-state visual styles.
/// Per-view and per-graph overrides are plumbed at the app layer
/// exactly like `NavigationPolicy`; see `resolve_node_style(view_id)`.
///
/// Defaults here reproduce the pre-policy hardcoded values that lived
/// in `render/canvas_bridge.rs` during the §4 host wiring — identical
/// feel to the pre-sweep world when no override is set.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeStyle {
    /// Default world-space radius for nodes. Threaded into
    /// `build_scene_input` so per-node overrides (future feature) can
    /// layer on top.
    pub default_radius: f32,
    /// Visual treatment for the primary selected node (most recently
    /// selected in a multi-select; the focus anchor).
    pub primary_selection: NodeStateStyle,
    /// Visual treatment for non-primary selected nodes.
    pub secondary_selection: NodeStateStyle,
    /// Visual treatment for nodes that match the active search query
    /// but are not selected.
    pub search_hit: NodeStateStyle,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            default_radius: DEFAULT_NODE_RADIUS,
            primary_selection: NodeStateStyle {
                fill: Color::new(0.3, 0.7, 1.0, 1.0),
                stroke: Some(Stroke {
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                    width: 2.5,
                }),
                label_color: Some(Color::WHITE),
            },
            secondary_selection: NodeStateStyle {
                fill: Color::new(0.3, 0.6, 0.9, 0.9),
                stroke: Some(Stroke {
                    color: Color::new(0.8, 0.9, 1.0, 0.8),
                    width: 1.5,
                }),
                label_color: None,
            },
            search_hit: NodeStateStyle {
                fill: Color::new(0.9, 0.8, 0.2, 1.0),
                stroke: None,
                label_color: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_pre_sweep_values() {
        let s = NodeStyle::default();
        assert_eq!(s.default_radius, DEFAULT_NODE_RADIUS);
        assert_eq!(s.primary_selection.fill, Color::new(0.3, 0.7, 1.0, 1.0));
        assert_eq!(
            s.primary_selection.stroke,
            Some(Stroke {
                color: Color::new(1.0, 1.0, 1.0, 1.0),
                width: 2.5,
            })
        );
        assert_eq!(s.primary_selection.label_color, Some(Color::WHITE));
        assert_eq!(s.secondary_selection.fill, Color::new(0.3, 0.6, 0.9, 0.9));
        assert_eq!(
            s.secondary_selection.stroke,
            Some(Stroke {
                color: Color::new(0.8, 0.9, 1.0, 0.8),
                width: 1.5,
            })
        );
        assert_eq!(s.secondary_selection.label_color, None);
        assert_eq!(s.search_hit.fill, Color::new(0.9, 0.8, 0.2, 1.0));
        assert_eq!(s.search_hit.stroke, None);
    }

    #[test]
    fn serde_roundtrip_preserves_all_fields() {
        let style = NodeStyle {
            default_radius: 24.0,
            primary_selection: NodeStateStyle {
                fill: Color::new(1.0, 0.2, 0.2, 1.0),
                stroke: None,
                label_color: None,
            },
            ..NodeStyle::default()
        };
        let json = serde_json::to_string(&style).unwrap();
        let back: NodeStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(style, back);
    }
}
