// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::hash::Hash;

use euclid::default::{Point2D, Vector2D};
use serde::{Deserialize, Serialize};

use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

use super::{StaticLayoutState, emit};
use crate::{Layout, LayoutExtras};

// ── Phyllotaxis ───────────────────────────────────────────────────────────────

/// Divergence angle (radians) between successive spiral steps. Standard
/// phyllotaxis uses the golden angle, but other angles produce dramatically
/// different patterns: 120° → three-arm spiral, 180° → alternating line,
/// 90° → cross-grid. Exposed as a knob so users can explore.
pub mod angles {
    use std::f32::consts::PI;
    /// Golden angle: `π × (3 − √5)`. Fibonacci phyllotaxis default.
    pub const GOLDEN: f32 = 2.399_963_3;
    /// 90° — cross-grid / four-arm spiral.
    pub const QUARTER_TURN: f32 = PI * 0.5;
    /// 120° — three-arm spiral.
    pub const THIRD_TURN: f32 = PI * 2.0 / 3.0;
    /// 180° — alternating line.
    pub const HALF_TURN: f32 = PI;
}

/// How the per-step radius scales with ordinal index. Changes the
/// visual "density profile" of the spiral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhyllotaxisRadiusCurve {
    /// `r = scale × sqrt(n)`. Classic sunflower — packs nodes evenly
    /// by area.
    SquareRoot,
    /// `r = scale × n`. Tightens center, spreads outer ring.
    Linear,
    /// `r = scale × n²`. Very tight center, rapidly expanding periphery.
    Quadratic,
    /// `r = scale × ln(1 + n)`. Near-center densities like a disk but
    /// compressed far edges.
    Logarithmic,
}

impl Default for PhyllotaxisRadiusCurve {
    fn default() -> Self {
        Self::SquareRoot
    }
}

/// Fibonacci-family spiral placement. Each node `n` is placed at angle
/// `n × angle_radians` and radius `scale × curve(n)`. With defaults
/// (golden angle, square-root curve) this is classic phyllotaxis; other
/// combinations produce three-arm spirals, cross-grids, or flower-like
/// packings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhyllotaxisConfig {
    pub center: Point2D<f32>,
    /// Scale applied to the radius curve.
    pub scale: f32,
    /// Divergence angle between successive steps, in radians. See
    /// [`angles`] module for named constants.
    pub angle_radians: f32,
    /// How radius grows with ordinal index.
    pub radius_curve: PhyllotaxisRadiusCurve,
    /// `Inward` = most-recent/priority-0 at center; `Outward` =
    /// oldest/index-0 at center.
    pub orientation: SpiralOrientation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpiralOrientation {
    Inward,
    Outward,
}

impl Default for PhyllotaxisConfig {
    fn default() -> Self {
        Self {
            center: Point2D::new(0.0, 0.0),
            scale: 22.0,
            angle_radians: angles::GOLDEN,
            radius_curve: PhyllotaxisRadiusCurve::default(),
            orientation: SpiralOrientation::Outward,
        }
    }
}

#[derive(Debug, Default)]
pub struct Phyllotaxis {
    pub config: PhyllotaxisConfig,
}

impl Phyllotaxis {
    pub fn new(config: PhyllotaxisConfig) -> Self {
        Self { config }
    }
}

fn radius_from_ordinal(curve: PhyllotaxisRadiusCurve, scale: f32, ordinal: usize) -> f32 {
    let n = ordinal as f32;
    scale
        * match curve {
            PhyllotaxisRadiusCurve::SquareRoot => n.sqrt(),
            PhyllotaxisRadiusCurve::Linear => n,
            PhyllotaxisRadiusCurve::Quadratic => n * n,
            PhyllotaxisRadiusCurve::Logarithmic => (1.0 + n).ln(),
        }
}

impl<N> Layout<N> for Phyllotaxis
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
        let n = scene.nodes.len();
        if n == 0 {
            state.step_count = state.step_count.saturating_add(1);
            return HashMap::new();
        }
        let mut targets: HashMap<N, Point2D<f32>> = HashMap::with_capacity(n);
        for (idx, node) in scene.nodes.iter().enumerate() {
            let ordinal = match self.config.orientation {
                SpiralOrientation::Outward => idx,
                SpiralOrientation::Inward => n - 1 - idx,
            };
            let radius = radius_from_ordinal(self.config.radius_curve, self.config.scale, ordinal);
            let angle = ordinal as f32 * self.config.angle_radians;
            targets.insert(
                node.id.clone(),
                Point2D::new(
                    self.config.center.x + radius * angle.cos(),
                    self.config.center.y + radius * angle.sin(),
                ),
            );
        }
        emit(scene, targets, state, extras)
    }
}
