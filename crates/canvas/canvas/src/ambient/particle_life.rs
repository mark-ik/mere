// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Particle-life ambient backdrop. (Physics scenes P5; see [`crate::ambient`].)

use paint_list_api::{ColorF, CommonPlacement, LayoutPoint, LayoutRect, PaintCmd, RectItem};

use super::{AmbientSim, Tincture, hsv_to_rgb, rgb_to_hue, unit, xorshift};

/// Particle-life: particles of a few species drifting under asymmetric per-species attraction /
/// repulsion, self-organising into the cells, chains, and chasing clusters the "Clusters" /
/// particle-life family is known for. Continuous and heavily damped (so it flows into structure
/// rather than flying apart); the space wraps (toroidal). Each species takes a hue rotated from the
/// tincture, so the backdrop is a coherent palette anchored at one colour. (Physics scenes P5.)
pub struct ParticleLife {
    pos: Vec<(f32, f32)>,
    vel: Vec<(f32, f32)>,
    species: Vec<usize>,
    /// `attract[a][b]` is how species `a` feels about species `b`, in `[-1, 1]` (asymmetric, so
    /// chase / flee dynamics emerge). Seeded random; the matrix is the scene's "rules".
    attract: Vec<Vec<f32>>,
}

/// The particle-life virtual coordinate span (square, toroidal; stretched to the viewport at paint).
const PL_SPAN: f32 = 1000.0;
const PL_SPECIES: usize = 5;
/// Interaction radius (no force beyond it).
const PL_RADIUS: f32 = 130.0;
/// Fraction of the radius that is the hard repulsion zone (keeps particles from overlapping).
const PL_BETA: f32 = 0.30;
/// Force scale, tuned with the heavy friction for flowing, stable structure.
const PL_FORCE: f32 = 135.0;
/// Per-step velocity retention - heavy friction, so the system is overdamped and self-organises.
const PL_FRICTION: f32 = 0.86;

impl ParticleLife {
    /// `count` particles at random positions, each a random species, under a random (asymmetric)
    /// attraction matrix - all seeded deterministically from `seed`.
    pub fn seeded(count: usize, seed: u32) -> Self {
        let mut rng = seed | 1;
        // Bias each species to attract its own kind (the diagonal strongly positive) so clear
        // per-species blobs always form; the off-diagonal stays random for chase / flee between them.
        let attract = (0..PL_SPECIES)
            .map(|i| {
                (0..PL_SPECIES)
                    .map(|j| {
                        if i == j {
                            0.6 + unit(&mut rng) * 0.4
                        } else {
                            unit(&mut rng) * 2.0 - 1.0
                        }
                    })
                    .collect()
            })
            .collect();
        let mut pos = Vec::with_capacity(count);
        let mut vel = Vec::with_capacity(count);
        let mut species = Vec::with_capacity(count);
        for _ in 0..count {
            pos.push((unit(&mut rng) * PL_SPAN, unit(&mut rng) * PL_SPAN));
            vel.push((0.0, 0.0));
            species.push(xorshift(&mut rng) as usize % PL_SPECIES);
        }
        Self {
            pos,
            vel,
            species,
            attract,
        }
    }

    pub fn particle_count(&self) -> usize {
        self.pos.len()
    }
}

/// The particle-life force at normalised distance `rn` in `[0, 1]` with attraction `a`: a hard
/// repulsion inside the beta zone (independent of `a`), then a triangular attraction / repulsion of
/// sign + scale `a` peaking mid-range, zero beyond the radius.
fn pl_force(rn: f32, a: f32) -> f32 {
    if rn < PL_BETA {
        rn / PL_BETA - 1.0
    } else if rn < 1.0 {
        a * (1.0 - (2.0 * rn - 1.0 - PL_BETA).abs() / (1.0 - PL_BETA))
    } else {
        0.0
    }
}

impl AmbientSim for ParticleLife {
    fn advance(&mut self, dt: f32) {
        let n = self.pos.len();
        let half = PL_SPAN / 2.0;
        let r2 = PL_RADIUS * PL_RADIUS;
        let mut acc = vec![(0.0f32, 0.0f32); n];
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                // Nearest-image displacement on the toroidal space.
                let mut dx = self.pos[j].0 - self.pos[i].0;
                let mut dy = self.pos[j].1 - self.pos[i].1;
                if dx > half {
                    dx -= PL_SPAN;
                } else if dx < -half {
                    dx += PL_SPAN;
                }
                if dy > half {
                    dy -= PL_SPAN;
                } else if dy < -half {
                    dy += PL_SPAN;
                }
                let d2 = dx * dx + dy * dy;
                if d2 >= r2 || d2 < 1e-4 {
                    continue;
                }
                let d = d2.sqrt();
                let f = pl_force(
                    d / PL_RADIUS,
                    self.attract[self.species[i]][self.species[j]],
                );
                acc[i].0 += (dx / d) * f;
                acc[i].1 += (dy / d) * f;
            }
        }
        for i in 0..n {
            self.vel[i].0 = (self.vel[i].0 + acc[i].0 * PL_FORCE * dt) * PL_FRICTION;
            self.vel[i].1 = (self.vel[i].1 + acc[i].1 * PL_FORCE * dt) * PL_FRICTION;
            self.pos[i].0 = (self.pos[i].0 + self.vel[i].0 * dt).rem_euclid(PL_SPAN);
            self.pos[i].1 = (self.pos[i].1 + self.vel[i].1 * dt).rem_euclid(PL_SPAN);
        }
    }

    fn paint(&self, w: f32, h: f32, tincture: Tincture) -> Vec<PaintCmd> {
        let sx = w / PL_SPAN;
        let sy = h / PL_SPAN;
        let half = 2.0;
        // One colour per species: the tincture's hue rotated around the wheel, kept vivid.
        let base_hue = rgb_to_hue(tincture);
        let colors: Vec<ColorF> = (0..PL_SPECIES)
            .map(|s| {
                let hue = (base_hue + s as f32 / PL_SPECIES as f32).rem_euclid(1.0);
                hsv_to_rgb(hue, 0.72, 0.95, tincture.a.max(0.85))
            })
            .collect();
        self.pos
            .iter()
            .zip(&self.species)
            .map(|(&(px, py), &s)| {
                let (cx, cy) = (px * sx, py * sy);
                PaintCmd::DrawRect(RectItem {
                    placement: CommonPlacement::new(LayoutRect::new(
                        LayoutPoint::new(cx - half, cy - half),
                        LayoutPoint::new(cx + half, cy + half),
                    )),
                    color: colors[s],
                })
            })
            .collect()
    }

    fn default_tincture(&self) -> Tincture {
        // A cyan base hue (the species rotate from here); alpha is the dot opacity.
        ColorF::new(0.25, 0.85, 0.95, 0.9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_life_stays_in_bounds_and_finite() {
        // The toroidal wrap + heavy friction must keep every particle finite and on the grid.
        let mut pl = ParticleLife::seeded(120, 0x515E_0001);
        for _ in 0..600 {
            pl.advance(1.0 / 60.0);
        }
        assert_eq!(pl.particle_count(), 120);
        for &(x, y) in pl.pos.iter() {
            assert!(
                x.is_finite() && y.is_finite(),
                "positions stay finite ({x}, {y})"
            );
            assert!(
                (0.0..=PL_SPAN).contains(&x),
                "wrapped x in bounds (was {x})"
            );
            assert!(
                (0.0..=PL_SPAN).contains(&y),
                "wrapped y in bounds (was {y})"
            );
        }
    }
}
