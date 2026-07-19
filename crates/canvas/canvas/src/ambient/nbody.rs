// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! N-body orbital-drift ambient backdrop. (Physics scenes P5; see [`crate::ambient`].)

use paint_list_api::{ColorF, CommonPlacement, LayoutPoint, LayoutRect, PaintCmd, RectItem};

use super::{AmbientSim, Tincture, unit};

/// An n-body orbital drift: a cloud of bodies orbiting a central well and tugging on one another,
/// for a slow galaxy-like swirl behind the graph. A Keplerian central well keeps the cloud bound and
/// orbiting (stable, no fly-away / collapse) with differential rotation, weak softened mutual gravity
/// adds clumping and structure. Continuous (integrated each frame). (Physics scenes P5.)
pub struct NBody {
    /// Positions in a virtual `[0, SPAN] x [0, SPAN]` space, stretched to the viewport at paint.
    pos: Vec<(f32, f32)>,
    vel: Vec<(f32, f32)>,
}

/// The n-body virtual coordinate span (square; stretched to the viewport at paint).
const NBODY_SPAN: f32 = 1000.0;
/// Central mass x gravitational constant - a Keplerian well (inverse-square pull to the centre), so
/// inner bodies orbit faster than outer (differential rotation, which winds clumps into spiral arms
/// rather than a rigidly-rotating disk). Softened so a body near the centre never feels an unbounded
/// pull.
const NBODY_GM: f32 = 8_500_000.0;
/// Softening for the central well (added to squared distance).
const NBODY_CSOFT: f32 = 900.0;
/// Mutual-gravity strength (softened inverse-square), for clumping into arms.
const NBODY_G: f32 = 18_000.0;
/// Softening added to a pair's squared distance, so a close pair never produces an unbounded force.
const NBODY_SOFTEN: f32 = 700.0;

impl NBody {
    /// A `count`-body disk around the centre, each on a near-circular Keplerian orbit, seeded
    /// deterministically from `seed`.
    pub fn seeded(count: usize, seed: u32) -> Self {
        let mut rng = seed | 1;
        let c = NBODY_SPAN / 2.0;
        let mut pos = Vec::with_capacity(count);
        let mut vel = Vec::with_capacity(count);
        for _ in 0..count {
            let ang = unit(&mut rng) * std::f32::consts::TAU;
            let rad = 40.0 + unit(&mut rng) * 260.0;
            pos.push((c + rad * ang.cos(), c + rad * ang.sin()));
            // Keplerian circular-orbit speed v = sqrt(GM r^2 / (r^2 + soft)^1.5), tangential CCW, so
            // inner bodies orbit faster than outer (differential rotation).
            let r2 = rad * rad;
            let speed = (NBODY_GM * r2 / (r2 + NBODY_CSOFT).powf(1.5)).sqrt();
            vel.push((-ang.sin() * speed, ang.cos() * speed));
        }
        Self { pos, vel }
    }

    pub fn body_count(&self) -> usize {
        self.pos.len()
    }
}

impl AmbientSim for NBody {
    fn advance(&mut self, dt: f32) {
        let n = self.pos.len();
        let c = NBODY_SPAN / 2.0;
        let mut acc = vec![(0.0f32, 0.0f32); n];
        // Mutual gravity (softened inverse-square attraction), symmetric per pair.
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = self.pos[j].0 - self.pos[i].0;
                let dy = self.pos[j].1 - self.pos[i].1;
                let d2 = dx * dx + dy * dy + NBODY_SOFTEN;
                // G * r / |r|^3 — the inverse-square magnitude along the unit direction.
                let inv = NBODY_G / (d2 * d2.sqrt());
                acc[i].0 += dx * inv;
                acc[i].1 += dy * inv;
                acc[j].0 -= dx * inv;
                acc[j].1 -= dy * inv;
            }
        }
        // Central Keplerian well (softened inverse-square pull to the centre), then integrate
        // (semi-implicit Euler).
        for i in 0..n {
            let rx = self.pos[i].0 - c;
            let ry = self.pos[i].1 - c;
            let r2 = rx * rx + ry * ry + NBODY_CSOFT;
            let inv = NBODY_GM / (r2 * r2.sqrt());
            acc[i].0 -= rx * inv;
            acc[i].1 -= ry * inv;
            self.vel[i].0 += acc[i].0 * dt;
            self.vel[i].1 += acc[i].1 * dt;
            self.pos[i].0 += self.vel[i].0 * dt;
            self.pos[i].1 += self.vel[i].1 * dt;
        }
    }

    fn paint(&self, w: f32, h: f32, tincture: Tincture) -> Vec<PaintCmd> {
        let sx = w / NBODY_SPAN;
        let sy = h / NBODY_SPAN;
        let half = 1.6;
        self.pos
            .iter()
            .map(|&(px, py)| {
                let (cx, cy) = (px * sx, py * sy);
                PaintCmd::DrawRect(RectItem {
                    placement: CommonPlacement::new(LayoutRect::new(
                        LayoutPoint::new(cx - half, cy - half),
                        LayoutPoint::new(cx + half, cy + half),
                    )),
                    color: tincture,
                })
            })
            .collect()
    }

    fn default_tincture(&self) -> Tincture {
        // Warm gold starlight — bright points (high alpha; these are small dots, not a wash).
        ColorF::new(0.95, 0.85, 0.55, 0.85)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nbody_stays_bounded_and_finite() {
        // The central well must keep the cloud bound (no fly-away / NaN blow-up) over a long run.
        let mut nb = NBody::seeded(80, 0xABCD_1234);
        for _ in 0..900 {
            nb.advance(1.0 / 60.0);
        }
        assert_eq!(nb.body_count(), 80);
        for &(x, y) in nb.pos.iter() {
            assert!(
                x.is_finite() && y.is_finite(),
                "positions stay finite ({x}, {y})"
            );
            assert!(
                x > -600.0 && x < NBODY_SPAN + 600.0,
                "x stays near the well (was {x})"
            );
            assert!(
                y > -600.0 && y < NBODY_SPAN + 600.0,
                "y stays near the well (was {y})"
            );
        }
    }
}
