// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared shape enums for force / distance / similarity curves.
//!
//! These are reused across multiple layouts so the user sees a consistent
//! set of options regardless of which layout they're configuring. For
//! example, `ProximityFalloff` is used by both `DegreeRepulsion` and
//! `HubPull`; picking `Exponential` in one has the same meaning as
//! picking it in the other.

use serde::{Deserialize, Serialize};

/// How force magnitude falls off with proximity within a bounded radius.
///
/// Input `t` is a normalized proximity value in `[0, 1]` where `0` means
/// the pair is exactly at the falloff radius (zero force) and `1` means
/// the pair is coincident (full force).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProximityFalloff {
    /// `force = t`. Traditional linear falloff.
    Linear,
    /// `force = 3t² − 2t³`. Smooth transitions at 0 and 1; better for
    /// animations.
    Smoothstep,
    /// `force = t² × exp(1 − 1/t)` (close approximation). Soft bell
    /// shape around medium range; near-zero at the edges.
    Exponential,
    /// `force = 0.5 × (1 − cos(π × t))`. Smooth like Smoothstep but
    /// symmetric about the midpoint.
    Cosine,
}

impl Default for ProximityFalloff {
    fn default() -> Self {
        Self::Linear
    }
}

impl ProximityFalloff {
    /// Evaluate the falloff at normalized proximity `t` (in `[0, 1]`).
    pub fn evaluate(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Smoothstep => t * t * (3.0 - 2.0 * t),
            Self::Exponential => {
                if t <= f32::EPSILON {
                    0.0
                } else {
                    t * t * (1.0 - 1.0 / t).exp()
                }
            }
            Self::Cosine => 0.5 * (1.0 - (std::f32::consts::PI * t).cos()),
        }
    }
}

/// How a node's adjacency degree scales forces that depend on degree
/// (e.g., hub-pull magnitude, degree-repulsion bonus).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DegreeWeighting {
    /// `ln(1 + degree)`. Default; soft diminishing returns.
    Logarithmic,
    /// `degree`. Linear — rare hubs dominate.
    Linear,
    /// `sqrt(degree)`. Mid-way between log and linear.
    SquareRoot,
    /// `degree^p` for user-specified `p`. Tighten or flatten
    /// arbitrarily.
    Polynomial(f32),
}

impl Default for DegreeWeighting {
    fn default() -> Self {
        Self::Logarithmic
    }
}

impl DegreeWeighting {
    /// Evaluate the weight for an integer degree.
    pub fn evaluate(self, degree: usize) -> f32 {
        let d = degree as f32;
        match self {
            Self::Logarithmic => d.ln_1p(),
            Self::Linear => d,
            Self::SquareRoot => d.sqrt(),
            Self::Polynomial(p) => d.powf(p),
        }
    }
}

/// How similarity (in `[0, 1]`) is mapped to an attraction coefficient.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SimilarityCurve {
    /// `force ∝ similarity`. Default.
    Linear,
    /// `force ∝ similarity²`. Emphasizes very-similar pairs, attenuates
    /// borderline ones.
    Quadratic,
    /// `force ∝ similarity³`. Stronger emphasis.
    Cubic,
    /// `force = 1.0` iff similarity ≥ floor, else `0.0`. Hard cutoff
    /// instead of graded attraction.
    Threshold(f32),
}

impl Default for SimilarityCurve {
    fn default() -> Self {
        Self::Linear
    }
}

impl SimilarityCurve {
    /// Evaluate the curve at `similarity` (in `[0, 1]`).
    pub fn evaluate(self, similarity: f32) -> f32 {
        match self {
            Self::Linear => similarity,
            Self::Quadratic => similarity * similarity,
            Self::Cubic => similarity * similarity * similarity,
            Self::Threshold(floor) => {
                if similarity >= floor {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// How force magnitude falls off with distance in an unbounded, physics-
/// style interaction (as opposed to the bounded [`ProximityFalloff`]).
///
/// Used by force-directed layouts (FR, Barnes-Hut)
/// to shape the repulsion and gravity curves without hardcoding
/// inverse-power laws.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Falloff {
    /// `force = 1 / distance`. FR default for attraction, `k²/d` for
    /// repulsion when combined with the canonical coefficients.
    Inverse,
    /// `force = 1 / distance²`. Coulomb/Newton-style. Sharper far-field
    /// decay.
    InverseSquare,
    /// `force = distance`. Linear — unusual but useful for "pull-harder-
    /// when-far" behaviors (spring-like).
    Linear,
    /// `force = exp(−distance × rate)`. Smooth exponential decay. Rate
    /// is encoded in the parameter.
    Exponential(f32),
}

impl Default for Falloff {
    fn default() -> Self {
        Self::Inverse
    }
}

impl Falloff {
    /// Evaluate the falloff shape at `distance` (positive, in world
    /// units). Callers multiply the result by their coefficient and,
    /// for FR-style repulsion, by `k²`.
    pub fn evaluate(self, distance: f32) -> f32 {
        let d = distance.max(f32::EPSILON);
        match self {
            Self::Inverse => 1.0 / d,
            Self::InverseSquare => 1.0 / (d * d),
            Self::Linear => d,
            Self::Exponential(rate) => (-d * rate).exp(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proximity_falloff_evaluates_at_endpoints() {
        for curve in [
            ProximityFalloff::Linear,
            ProximityFalloff::Smoothstep,
            ProximityFalloff::Exponential,
            ProximityFalloff::Cosine,
        ] {
            // t=0 → 0.
            let at_zero = curve.evaluate(0.0);
            assert!(
                at_zero.abs() < 1e-5,
                "{:?} at t=0 expected 0, got {}",
                curve,
                at_zero
            );
            // t=1 → 1 (or close).
            let at_one = curve.evaluate(1.0);
            assert!(
                (at_one - 1.0).abs() < 0.05,
                "{:?} at t=1 expected ~1, got {}",
                curve,
                at_one
            );
        }
    }

    #[test]
    fn degree_weighting_monotone_nondecreasing() {
        for weighting in [
            DegreeWeighting::Logarithmic,
            DegreeWeighting::Linear,
            DegreeWeighting::SquareRoot,
            DegreeWeighting::Polynomial(1.5),
        ] {
            let w1 = weighting.evaluate(1);
            let w3 = weighting.evaluate(3);
            let w10 = weighting.evaluate(10);
            assert!(w1 <= w3, "{:?} not monotone at 1→3", weighting);
            assert!(w3 <= w10, "{:?} not monotone at 3→10", weighting);
        }
    }

    #[test]
    fn similarity_curves_return_zero_at_zero() {
        for curve in [
            SimilarityCurve::Linear,
            SimilarityCurve::Quadratic,
            SimilarityCurve::Cubic,
        ] {
            assert_eq!(curve.evaluate(0.0), 0.0);
        }
    }

    #[test]
    fn similarity_threshold_is_binary() {
        let curve = SimilarityCurve::Threshold(0.5);
        assert_eq!(curve.evaluate(0.4), 0.0);
        assert_eq!(curve.evaluate(0.5), 1.0);
        assert_eq!(curve.evaluate(0.9), 1.0);
    }

    #[test]
    fn falloff_positive_for_positive_distance() {
        for f in [
            Falloff::Inverse,
            Falloff::InverseSquare,
            Falloff::Linear,
            Falloff::Exponential(0.1),
        ] {
            assert!(f.evaluate(1.0) > 0.0, "{:?} returned non-positive", f);
            assert!(f.evaluate(100.0) > 0.0, "{:?} returned non-positive", f);
        }
    }
}
