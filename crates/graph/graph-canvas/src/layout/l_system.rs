/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Lindenmayer-system fractal path layouts.
//!
//! A grammar defines an axiom and production rules; iteration produces a
//! symbol string which a turtle walks to yield a sequence of positions.
//! Nodes are placed at successive turtle F-step positions, one per node.
//!
//! Three built-in grammars:
//!
//! - **Hilbert** — space-filling curve. Locality-preserving (close in
//!   index → close in space). Best for navigation-oriented layouts and
//!   scales cleanly to thousands of nodes. `4^n` points at depth `n`.
//! - **Koch** — snowflake-edge fractal. Decorative; boundary-style
//!   placement. `5^n` points at depth `n` (approximately — grammar
//!   length grows `5×` per deflation).
//! - **Dragon** — Heighway dragon curve. Self-avoiding spiral-fold.
//!   Visually striking; moderate point count (`2^n + 1`).
//!
//! Per the Step-5 plan, `LSystemGrammar::Custom(_)` is reserved for
//! future user-authored grammars tracked in the pluggable-mods lane;
//! this first-pass falls back to `Hilbert` if encountered.

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use super::{Layout, LayoutExtras, StaticLayoutState};
use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

/// Opaque handle to a future user-authored grammar. Current first-pass
/// implementations fall back to `Hilbert` when this variant is selected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomGrammarHandle(pub String);

/// Which built-in (or custom) L-system grammar to walk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LSystemGrammar {
    Hilbert,
    Koch,
    Dragon,
    /// Reserved for future user-authored grammars. Falls back to
    /// `Hilbert` in the current first-pass; implementation tracked in
    /// the pluggable-mods / WASM layout runtime lanes.
    Custom(CustomGrammarHandle),
}

impl Default for LSystemGrammar {
    fn default() -> Self {
        Self::Hilbert
    }
}

/// Iteration depth selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IterationDepth {
    /// Smallest depth `n` such that the expansion yields ≥ node_count
    /// positions. Default.
    Auto,
    /// Explicit fractal depth; useful for deterministic comparisons or
    /// artistic control.
    Explicit(u8),
}

impl Default for IterationDepth {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LSystemConfig {
    pub grammar: LSystemGrammar,
    pub iteration_depth: IterationDepth,
    /// Origin point for the turtle's starting position.
    pub origin: Point2D<f32>,
    /// Bounding-box edge length in world units. The walked path is
    /// normalized to fit this extent.
    pub size: f32,
    /// Rotation of the whole path around `origin`, in radians.
    pub rotation: f32,
    /// If true, the node-to-position assignment is reversed.
    pub reverse_order: bool,
}

impl Default for LSystemConfig {
    fn default() -> Self {
        Self {
            grammar: LSystemGrammar::default(),
            iteration_depth: IterationDepth::default(),
            origin: Point2D::new(0.0, 0.0),
            size: 400.0,
            rotation: 0.0,
            reverse_order: false,
        }
    }
}

/// L-system fractal path layout.
#[derive(Debug, Default)]
pub struct LSystem {
    pub config: LSystemConfig,
}

impl LSystem {
    pub fn new(config: LSystemConfig) -> Self {
        Self { config }
    }
}

impl<N> Layout<N> for LSystem
where
    N: Clone + Eq + Hash,
{
    type State = StaticLayoutState;

    fn step(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut Self::State,
        _dt: f32,
        _viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        state.step_count = state.step_count.saturating_add(1);
        if scene.nodes.is_empty() {
            return HashMap::new();
        }

        let grammar_def = resolve_grammar(&self.config.grammar);
        let depth = match self.config.iteration_depth {
            IterationDepth::Explicit(d) => d,
            IterationDepth::Auto => choose_auto_depth(&grammar_def, scene.nodes.len()),
        };

        let raw_positions = walk_grammar(&grammar_def, depth);
        if raw_positions.is_empty() {
            return HashMap::new();
        }

        let path = normalize_path(&raw_positions, &self.config);

        let damping = state.damping.clamp(0.0, 1.0);
        let n = scene.nodes.len();
        let path_len = path.len();
        let mut deltas = HashMap::with_capacity(n);
        for (idx, node) in scene.nodes.iter().enumerate() {
            if extras.pinned.contains(&node.id) {
                continue;
            }
            let path_idx = if self.config.reverse_order {
                path_len
                    .saturating_sub(1)
                    .saturating_sub(idx.min(path_len - 1))
            } else {
                idx.min(path_len - 1)
            };
            let target = path[path_idx];
            let delta = (target - node.position) * damping;
            if delta.length() > f32::EPSILON {
                deltas.insert(node.id.clone(), delta);
            }
        }
        deltas
    }
}

// ── Grammar definitions ──────────────────────────────────────────────────────

struct GrammarDef {
    /// Starting symbol string.
    axiom: &'static str,
    /// Production rules. Each key is a single symbol character; value is
    /// the replacement string.
    rules: &'static [(char, &'static str)],
    /// Turn angle in radians for `+` and `-` symbols.
    angle: f32,
}

const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

const HILBERT: GrammarDef = GrammarDef {
    axiom: "A",
    rules: &[('A', "-BF+AFA+FB-"), ('B', "+AF-BFB-FA+")],
    angle: 90.0 * DEG_TO_RAD,
};

const KOCH: GrammarDef = GrammarDef {
    axiom: "F",
    rules: &[('F', "F+F--F+F")],
    angle: 60.0 * DEG_TO_RAD,
};

const DRAGON: GrammarDef = GrammarDef {
    axiom: "FX",
    rules: &[('X', "X+YF+"), ('Y', "-FX-Y")],
    angle: 90.0 * DEG_TO_RAD,
};

fn resolve_grammar(g: &LSystemGrammar) -> &'static GrammarDef {
    match g {
        LSystemGrammar::Hilbert => &HILBERT,
        LSystemGrammar::Koch => &KOCH,
        LSystemGrammar::Dragon => &DRAGON,
        // Custom falls back to Hilbert per Step-5 plan first-pass.
        LSystemGrammar::Custom(_) => &HILBERT,
    }
}

// ── Grammar expansion + turtle walk ──────────────────────────────────────────

fn expand(grammar: &GrammarDef, depth: u8) -> String {
    let mut current = grammar.axiom.to_string();
    for _ in 0..depth {
        let mut next = String::with_capacity(current.len() * 4);
        for ch in current.chars() {
            if let Some((_, replacement)) = grammar.rules.iter().find(|(k, _)| *k == ch) {
                next.push_str(replacement);
            } else {
                next.push(ch);
            }
        }
        current = next;
    }
    current
}

fn walk_grammar(grammar: &GrammarDef, depth: u8) -> Vec<Point2D<f32>> {
    let symbols = expand(grammar, depth);
    let mut positions = Vec::new();
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut heading = 0.0f32; // 0 rad = +x
    let mut stack: Vec<(f32, f32, f32)> = Vec::new();
    let step = 1.0f32;

    positions.push(Point2D::new(x, y));
    for ch in symbols.chars() {
        match ch {
            'F' | 'A' | 'B' | 'X' | 'Y' => {
                // A/B in Hilbert are non-drawing; treat them as no-ops
                // per standard L-system convention. Same for X/Y in
                // Dragon. F always advances.
                if ch == 'F' {
                    x += step * heading.cos();
                    y += step * heading.sin();
                    positions.push(Point2D::new(x, y));
                }
            }
            '+' => heading -= grammar.angle,
            '-' => heading += grammar.angle,
            '[' => stack.push((x, y, heading)),
            ']' => {
                if let Some((sx, sy, sh)) = stack.pop() {
                    x = sx;
                    y = sy;
                    heading = sh;
                }
            }
            _ => {}
        }
    }
    positions
}

fn choose_auto_depth(grammar: &GrammarDef, node_count: usize) -> u8 {
    // Iterate depths until expansion yields ≥ node_count F-steps. Cap at
    // 10 to bound memory (4^10 = 1M for Hilbert).
    for depth in 0u8..=10 {
        let symbols = expand(grammar, depth);
        let f_count = symbols.chars().filter(|c| *c == 'F').count() + 1;
        if f_count >= node_count {
            return depth;
        }
    }
    10
}

fn normalize_path(raw: &[Point2D<f32>], config: &LSystemConfig) -> Vec<Point2D<f32>> {
    if raw.is_empty() {
        return Vec::new();
    }
    let mut min_x = raw[0].x;
    let mut max_x = raw[0].x;
    let mut min_y = raw[0].y;
    let mut max_y = raw[0].y;
    for p in raw.iter().skip(1) {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    let extent = (max_x - min_x).max(max_y - min_y).max(1.0);
    let scale = config.size / extent;
    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let (cos_r, sin_r) = (config.rotation.cos(), config.rotation.sin());
    raw.iter()
        .map(|p| {
            let dx = (p.x - center_x) * scale;
            let dy = (p.y - center_y) * scale;
            Point2D::new(
                config.origin.x + dx * cos_r - dy * sin_r,
                config.origin.y + dx * sin_r + dy * cos_r,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::ProjectionMode;
    use crate::scene::{CanvasEdge, CanvasNode, SceneMode, ViewId};
    use euclid::default::{Rect, Size2D};

    fn viewport() -> CanvasViewport {
        CanvasViewport {
            rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
            scale_factor: 1.0,
        }
    }

    fn scene(n: u32) -> CanvasSceneInput<u32> {
        CanvasSceneInput {
            view_id: ViewId(0),
            nodes: (0..n)
                .map(|id| CanvasNode {
                    id,
                    position: Point2D::new(0.0, 0.0),
                    radius: 16.0,
                    label: None,
                })
                .collect(),
            edges: Vec::<CanvasEdge<u32>>::new(),
            scene_objects: Vec::new(),
            overlays: Vec::new(),
            scene_mode: SceneMode::Browse,
            projection: ProjectionMode::default(),
        }
    }

    #[test]
    fn hilbert_grammar_expands_deterministically() {
        let d0 = expand(&HILBERT, 0);
        let d1 = expand(&HILBERT, 1);
        let d2 = expand(&HILBERT, 2);
        assert_eq!(d0, "A");
        assert_eq!(d1, "-BF+AFA+FB-");
        // Depth 2 must be longer than depth 1.
        assert!(d2.len() > d1.len());
    }

    #[test]
    fn auto_depth_picks_smallest_depth_fitting_node_count() {
        let n = 20;
        let depth = choose_auto_depth(&HILBERT, n);
        // Depth 1 gives 4 F's + 1 = 5 points; depth 2 gives ≥ 20.
        let symbols = expand(&HILBERT, depth);
        let f_count = symbols.chars().filter(|c| *c == 'F').count() + 1;
        assert!(f_count >= n);
    }

    #[test]
    fn walk_produces_unique_points_for_hilbert() {
        let path = walk_grammar(&HILBERT, 3);
        // Hilbert is self-avoiding; all points distinct.
        let mut seen = std::collections::HashSet::new();
        for p in &path {
            let key = ((p.x * 1000.0) as i32, (p.y * 1000.0) as i32);
            assert!(seen.insert(key), "duplicate point in Hilbert path");
        }
    }

    #[test]
    fn hilbert_places_nodes_within_configured_size() {
        let mut layout = LSystem::new(LSystemConfig {
            grammar: LSystemGrammar::Hilbert,
            iteration_depth: IterationDepth::Auto,
            origin: Point2D::new(0.0, 0.0),
            size: 100.0,
            rotation: 0.0,
            reverse_order: false,
        });
        let mut state = StaticLayoutState::default();
        let input = scene(20);
        let deltas = layout.step(
            &input,
            &mut state,
            0.0,
            &viewport(),
            &LayoutExtras::default(),
        );
        for node in &input.nodes {
            if let Some(delta) = deltas.get(&node.id) {
                let p = node.position + *delta;
                assert!(p.x.abs() <= 60.0, "x out of box: {}", p.x);
                assert!(p.y.abs() <= 60.0, "y out of box: {}", p.y);
            }
        }
    }

    #[test]
    fn all_three_grammars_produce_non_empty_paths() {
        for grammar in [
            LSystemGrammar::Hilbert,
            LSystemGrammar::Koch,
            LSystemGrammar::Dragon,
        ] {
            let def = resolve_grammar(&grammar);
            let path = walk_grammar(def, 3);
            assert!(!path.is_empty(), "{:?} produced empty path", grammar);
            assert!(path.len() >= 2, "{:?} produced trivial path", grammar);
        }
    }

    #[test]
    fn custom_grammar_falls_back_to_hilbert_in_first_pass() {
        let def_hilbert = resolve_grammar(&LSystemGrammar::Hilbert);
        let def_custom = resolve_grammar(&LSystemGrammar::Custom(CustomGrammarHandle(
            "unknown".into(),
        )));
        // Same memory address expected since both point to HILBERT.
        assert_eq!(def_hilbert.axiom, def_custom.axiom);
    }

    #[test]
    fn pinned_nodes_skipped() {
        let mut layout = LSystem::new(LSystemConfig::default());
        let input = scene(4);
        let mut extras: LayoutExtras<u32> = LayoutExtras::default();
        extras.pinned.insert(0);
        let deltas = layout.step(
            &input,
            &mut StaticLayoutState::default(),
            0.0,
            &viewport(),
            &extras,
        );
        assert!(!deltas.contains_key(&0));
    }
}
