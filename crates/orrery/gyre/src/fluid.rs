/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Position-Based Fluids (PBF) — our own small SPH liquid for the orrery, rapier-version-agnostic
//! (salva lags rapier badly, so we roll our own). PBF (Macklin-Müller 2013) enforces
//! incompressibility as a *positional density constraint* solved by a few Jacobi iterations rather
//! than a stiff pressure force, so it stays stable at the orrery's fixed 1/60s tick even when a
//! body is shoved straight through the pool — the same robustness reason the layout solver never
//! blows up. It folds into one tick with no sound-speed substep.
//!
//! This module is the standalone solver: a particle pool, the PBF step, and an analytic [`Basin`]
//! boundary (P4c-a — a settling pool). Wiring it into [`Simulation`](crate::Simulation) (so it
//! renders off-thread and a tangible graph can stir it, coupling to the rapier colliders) is the
//! next phase. The neighbour search is O(n²) for now — fine at the capped particle counts; a
//! uniform grid is the perf follow-on if a pool grows large.
//!
//! Math is euclid `Vector2D` / `Point2D` (gyre's public-surface convention), so positions feed the
//! render snapshot directly. `+y` is down, matching the rest of Mere. (Physics scenes P4c.)

use std::f32::consts::PI;

use euclid::default::{Point2D, Vector2D};

/// Tunable PBF parameters. Public per the configurability-over-defaults stance; the defaults give a
/// stable pool at Mere's `1 unit ≈ 1 px` scale.
#[derive(Clone, Copy, Debug)]
pub struct FluidParams {
    /// Target density the incompressibility constraint holds the fluid at. Auto-set from the spawn
    /// spacing by [`Fluid::spawn_block`] (the spawn defines "at rest"); override after if needed.
    pub rest_density: f32,
    /// Particle radius — the boundary keep-out and the paint size. Spacing is the spawn's concern.
    pub particle_radius: f32,
    /// Smoothing radius `h` (the kernel support). Want `h ≈ 2.5 ×` particle spacing so a particle
    /// sees ~2 rings of neighbours.
    pub smoothing_radius: f32,
    /// PBF constraint-solver iterations per step (2-4; more = stiffer/less compressible, costlier).
    pub solver_iters: u32,
    /// XSPH viscosity coefficient (0 = inviscid; small values smooth the velocity field).
    pub viscosity: f32,
    /// Velocity retained per step (a drag, `0.0`-`1.0`). `< 1.0` bleeds residual kinetic energy so
    /// a gravity-driven slosh settles to rest; `1.0` is drag-free. An interactive pool the graph
    /// stirs wants this near 1.0 to stay responsive.
    pub damping: f32,
    /// Constraint-force-mixing epsilon — relaxes the `λ` denominator so it never divides by ~0.
    pub relaxation_eps: f32,
    /// World gravity on the fluid (px/s²; `+y` down).
    pub gravity: Vector2D<f32>,
    /// Hard cap on live particles (the actor-budget valve).
    pub max_particles: usize,
}

impl Default for FluidParams {
    fn default() -> Self {
        Self {
            rest_density: 1.0,
            particle_radius: 7.0,
            smoothing_radius: 40.0,
            solver_iters: 3,
            viscosity: 0.05,
            damping: 0.99,
            relaxation_eps: 100.0,
            gravity: Vector2D::new(0.0, 520.0),
            max_particles: 600,
        }
    }
}

/// An analytic basin the particles stay within: a floor (largest `y`, since `+y` is down) and two
/// side walls, open at the top. The first, simplest boundary; coupling to arbitrary rapier
/// colliders (the bowl, a tangible node) is the next phase. (Physics scenes P4c-a.)
#[derive(Clone, Copy, Debug)]
pub struct Basin {
    pub min_x: f32,
    pub max_x: f32,
    pub floor_y: f32,
}

impl Basin {
    /// Push a point back inside the basin, keeping a particle of radius `r` clear of the walls/floor.
    fn clamp(&self, p: Point2D<f32>, r: f32) -> Point2D<f32> {
        let lo = self.min_x + r;
        let hi = self.max_x - r;
        // Guard against a basin narrower than the particle (lo > hi): centre it.
        let x = if lo <= hi { p.x.clamp(lo, hi) } else { 0.5 * (self.min_x + self.max_x) };
        Point2D::new(x, p.y.min(self.floor_y - r))
    }
}

/// A solid the fluid couples to, in world space: a circle or an oriented box. The host builds these
/// from the rapier bodies the fluid should bounce off (the fixed basin walls are the analytic
/// [`Basin`]; these are the node / scene bodies, gated by tangibility). A square / rounded-square body
/// couples at its true [`ContactShape::Obb`] face + rotation; a ball at its circle; a hull falls back
/// to its bounding circle. (Physics scenes P4c.)
#[derive(Clone, Copy, Debug)]
pub struct FluidContact {
    pub center: Point2D<f32>,
    pub shape: ContactShape,
}

/// The shape of a [`FluidContact`] the fluid pushes particles out of.
#[derive(Clone, Copy, Debug)]
pub enum ContactShape {
    /// A circle of `radius`.
    Circle { radius: f32 },
    /// An oriented box: half-extents `(hx, hy)` in the body's local frame, rotated by `angle` (rad).
    Obb { half: (f32, f32), angle: f32 },
}

impl FluidContact {
    /// A circle contact at `center` with `radius`.
    pub fn circle(center: Point2D<f32>, radius: f32) -> Self {
        Self { center, shape: ContactShape::Circle { radius } }
    }

    /// An oriented-box contact at `center` with local half-extents `half`, rotated by `angle` (rad).
    pub fn obb(center: Point2D<f32>, half: (f32, f32), angle: f32) -> Self {
        Self { center, shape: ContactShape::Obb { half, angle } }
    }
}

/// The PBF fluid: a particle pool plus its parameters and basin. Spawn particles, then [`step`] it
/// each tick. [`positions`] / [`velocities`] are the reads for rendering + tests.
///
/// [`step`]: Fluid::step
/// [`positions`]: Fluid::positions
/// [`velocities`]: Fluid::velocities
pub struct Fluid {
    pos: Vec<Point2D<f32>>,
    vel: Vec<Vector2D<f32>>,
    /// Predicted positions — PBF scratch, reused across steps to avoid reallocation.
    pred: Vec<Point2D<f32>>,
    /// Per-particle constraint multiplier — PBF scratch.
    lambda: Vec<f32>,
    params: FluidParams,
    basin: Basin,
}

impl Fluid {
    /// An empty fluid with the given params + basin.
    pub fn new(params: FluidParams, basin: Basin) -> Self {
        Self { pos: Vec::new(), vel: Vec::new(), pred: Vec::new(), lambda: Vec::new(), params, basin }
    }

    /// Seed a `cols × rows` grid of still particles at `spacing` from `origin` (top-left), capped at
    /// [`FluidParams::max_particles`]. Sets [`FluidParams::rest_density`] from `spacing` (the spawn
    /// defines the rest configuration), so the pool settles to this packing rather than collapsing
    /// or puffing up.
    pub fn spawn_block(&mut self, origin: Point2D<f32>, cols: usize, rows: usize, spacing: f32) {
        for row in 0..rows {
            for col in 0..cols {
                if self.pos.len() >= self.params.max_particles {
                    break;
                }
                let p = Point2D::new(origin.x + col as f32 * spacing, origin.y + row as f32 * spacing);
                self.pos.push(p);
                self.vel.push(Vector2D::zero());
                self.pred.push(p);
                self.lambda.push(0.0);
            }
        }
        self.params.rest_density =
            rest_density_for_spacing(spacing, self.params.smoothing_radius).max(1e-3);
    }

    /// Number of live particles.
    pub fn particle_count(&self) -> usize {
        self.pos.len()
    }

    /// The fluid's parameters (the rest density auto-set by [`spawn_block`] included).
    pub fn params(&self) -> &FluidParams {
        &self.params
    }

    /// Iterate every particle's current position — the render / test read.
    pub fn positions(&self) -> impl Iterator<Item = Point2D<f32>> + '_ {
        self.pos.iter().copied()
    }

    /// Iterate every particle's current velocity.
    pub fn velocities(&self) -> impl Iterator<Item = Vector2D<f32>> + '_ {
        self.vel.iter().copied()
    }

    /// The largest particle speed — an at-rest / stability probe.
    pub fn max_speed(&self) -> f32 {
        self.vel.iter().map(|v| v.length()).fold(0.0, f32::max)
    }

    /// Advance the fluid by `dt` seconds: predict under gravity, solve the incompressibility
    /// constraint over a few Jacobi iterations (projecting out of the basin each iteration), then
    /// derive velocities from the corrected positions and apply XSPH viscosity.
    pub fn step(&mut self, dt: f32) {
        let n = self.pos.len();
        if n == 0 || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let h = self.params.smoothing_radius;
        let rho0 = self.params.rest_density;
        let eps = self.params.relaxation_eps;
        let r = self.params.particle_radius;
        let g = self.params.gravity;

        // 1. Predict: integrate gravity into velocity, then a tentative position.
        for i in 0..n {
            self.vel[i] += g * dt;
            self.pred[i] = self.pos[i] + self.vel[i] * dt;
        }

        // 2. Constraint solve (Jacobi). O(n²) neighbour scan — fine at capped counts.
        let mut deltas = vec![Vector2D::zero(); n];
        for _ in 0..self.params.solver_iters {
            // 2a. Density and the per-particle multiplier λ.
            for i in 0..n {
                let mut rho = 0.0;
                let mut grad_i = Vector2D::zero();
                let mut sum_grad2 = 0.0;
                for j in 0..n {
                    let rvec = self.pred[i] - self.pred[j];
                    rho += poly6(rvec.square_length(), h);
                    if i != j {
                        let grad_j = spiky_grad(rvec, h) / rho0;
                        grad_i += grad_j;
                        sum_grad2 += grad_j.square_length();
                    }
                }
                sum_grad2 += grad_i.square_length();
                let c = rho / rho0 - 1.0;
                self.lambda[i] = -c / (sum_grad2 + eps);
            }
            // 2b. Position correction from the neighbours' multipliers.
            for i in 0..n {
                let mut dp = Vector2D::zero();
                for j in 0..n {
                    if i == j {
                        continue;
                    }
                    let rvec = self.pred[i] - self.pred[j];
                    dp += spiky_grad(rvec, h) * (self.lambda[i] + self.lambda[j]);
                }
                deltas[i] = dp / rho0;
            }
            // 2c. Apply, then project back into the basin.
            for i in 0..n {
                self.pred[i] = self.basin.clamp(self.pred[i] + deltas[i], r);
            }
        }

        // 3. Velocity from the corrected positions; commit.
        for i in 0..n {
            self.vel[i] = (self.pred[i] - self.pos[i]) / dt;
            self.pos[i] = self.pred[i];
        }

        // 4. XSPH viscosity — nudge each particle's velocity toward its neighbours' (a smoother,
        // more cohesive flow). Skipped when the coefficient is zero.
        if self.params.viscosity > 0.0 {
            let mut dv = vec![Vector2D::zero(); n];
            for i in 0..n {
                for j in 0..n {
                    if i == j {
                        continue;
                    }
                    let w = poly6((self.pos[i] - self.pos[j]).square_length(), h);
                    dv[i] += (self.vel[j] - self.vel[i]) * w;
                }
            }
            for i in 0..n {
                self.vel[i] += dv[i] * self.params.viscosity;
            }
        }

        // 5. Drag: bleed residual kinetic energy so a disturbed / gravity-sloshed pool settles to
        // rest. Tunable (an interactive pool wants this near 1.0).
        if self.params.damping < 1.0 {
            for i in 0..n {
                self.vel[i] *= self.params.damping;
            }
        }
    }

    /// Two-way coupling: push every particle out of the given solid circles (a tangible node / a
    /// scene body), zeroing the inward velocity, and return the net reaction the fluid exerts back
    /// on each contact (the sum of the displacements it absorbed). The caller turns each reaction
    /// into an impulse on the body. Call after [`step`]. (Physics scenes P4c.)
    ///
    /// [`step`]: Fluid::step
    pub fn resolve_contacts(&mut self, contacts: &[FluidContact]) -> Vec<Vector2D<f32>> {
        let mut reactions = vec![Vector2D::zero(); contacts.len()];
        if contacts.is_empty() {
            return reactions;
        }
        let pr = self.params.particle_radius;
        for i in 0..self.pos.len() {
            for (c, contact) in contacts.iter().enumerate() {
                // The displacement that pushes the particle just outside the (particle-inflated)
                // solid, per shape; `continue` when the particle is already clear.
                let push = match contact.shape {
                    ContactShape::Circle { radius } => {
                        let rmin = radius + pr;
                        let delta = self.pos[i] - contact.center;
                        let d = delta.length();
                        if d >= rmin {
                            continue;
                        }
                        // Outward normal (straight up if a particle sits dead-centre).
                        let n = if d > 1.0e-4 { delta / d } else { Vector2D::new(0.0, -1.0) };
                        n * (rmin - d)
                    }
                    ContactShape::Obb { half, angle } => {
                        // The particle in the box's local frame (rotate the offset by -angle).
                        let rel = self.pos[i] - contact.center;
                        let (s, co) = angle.sin_cos();
                        let lx = rel.x * co + rel.y * s;
                        let ly = -rel.x * s + rel.y * co;
                        // The box inflated by the particle radius (so the particle's edge is pushed
                        // to the surface, not its centre).
                        let (hx, hy) = (half.0 + pr, half.1 + pr);
                        if lx.abs() >= hx || ly.abs() >= hy {
                            continue;
                        }
                        // Push out the nearer local face, then rotate the push back to world.
                        let (px, py) = (hx - lx.abs(), hy - ly.abs());
                        let (plx, ply) = if px < py {
                            (px * lx.signum(), 0.0)
                        } else {
                            (0.0, py * ly.signum())
                        };
                        Vector2D::new(plx * co - ply * s, plx * s + ply * co)
                    }
                };
                self.pos[i] += push;
                // The particle can't move into the solid: kill its inward velocity component along
                // the push normal.
                let plen = push.length();
                if plen > 1.0e-6 {
                    let n = push / plen;
                    let vn = self.vel[i].dot(n);
                    if vn < 0.0 {
                        self.vel[i] -= n * vn;
                    }
                }
                // The fluid shoves the solid back by the opposite of the displacement it absorbed.
                reactions[c] -= push;
            }
        }
        reactions
    }
}

/// Poly6 smoothing kernel (2D), evaluated from the squared distance `r2`. Density falls smoothly to
/// zero at the support radius `h`. Normalisation `4 / (π h⁸)`.
fn poly6(r2: f32, h: f32) -> f32 {
    let h2 = h * h;
    if r2 >= h2 {
        return 0.0;
    }
    let c = 4.0 / (PI * h.powi(8));
    let d = h2 - r2;
    c * d * d * d
}

/// Spiky kernel gradient (2D): the pressure-direction gradient, steep near `r=0` so close particles
/// push apart firmly. Normalisation `-30 / (π h⁵)`; points along `rvec`. Zero past `h` or at ~0
/// separation (no singular self-force).
fn spiky_grad(rvec: Vector2D<f32>, h: f32) -> Vector2D<f32> {
    let r = rvec.length();
    if r >= h || r < 1.0e-6 {
        return Vector2D::zero();
    }
    let c = -30.0 / (PI * h.powi(5));
    let d = h - r;
    rvec * (c * d * d / r)
}

/// The rest density for a particle in an infinite regular grid at `spacing` (sampled within the
/// support `h`). Used to auto-set [`FluidParams::rest_density`] from the spawn so the pool holds the
/// packing it was seeded at.
fn rest_density_for_spacing(spacing: f32, h: f32) -> f32 {
    let spacing = spacing.max(1.0e-3);
    let reach = (h / spacing).ceil() as i32 + 1;
    let mut rho = 0.0;
    for gx in -reach..=reach {
        for gy in -reach..=reach {
            let r2 = (gx as f32 * spacing).powi(2) + (gy as f32 * spacing).powi(2);
            rho += poly6(r2, h);
        }
    }
    rho
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> (Fluid, Basin) {
        let basin = Basin { min_x: -200.0, max_x: 200.0, floor_y: 200.0 };
        let mut fluid = Fluid::new(FluidParams::default(), basin);
        // 14×10 block at spacing 16 (h=40 ≈ 2.5×), dropped above the basin floor.
        fluid.spawn_block(Point2D::new(-104.0, -150.0), 14, 10, 16.0);
        (fluid, basin)
    }

    #[test]
    fn fluid_settles_in_the_basin_and_stays_finite() {
        let (mut fluid, basin) = pool();
        let n = fluid.particle_count();
        assert_eq!(n, 140, "the block seeded 14×10 particles");
        assert!(fluid.params().rest_density > 0.0, "rest density auto-set from the spawn");

        for _ in 0..400 {
            fluid.step(1.0 / 60.0);
        }

        assert_eq!(fluid.particle_count(), n, "particle count is conserved");
        for p in fluid.positions() {
            assert!(p.x.is_finite() && p.y.is_finite(), "no NaN/inf: {p:?}");
            assert!(p.x >= basin.min_x - 1.0 && p.x <= basin.max_x + 1.0, "contained in x: {p:?}");
            assert!(p.y <= basin.floor_y + 1.0, "stayed above the floor: {p:?}");
        }
        // Settled: the pool came (nearly) to rest rather than churning or exploding.
        assert!(fluid.max_speed() < 40.0, "fluid settled (max speed {})", fluid.max_speed());
        // Sank: the pool fell from its spawn (mean y < -50) toward the floor (y=200).
        let mean_y: f32 = fluid.positions().map(|p| p.y).sum::<f32>() / n as f32;
        assert!(mean_y > 0.0, "the pool sank toward the floor (mean_y {mean_y})");
    }

    #[test]
    fn empty_fluid_step_is_a_noop() {
        let basin = Basin { min_x: -100.0, max_x: 100.0, floor_y: 100.0 };
        let mut fluid = Fluid::new(FluidParams::default(), basin);
        fluid.step(1.0 / 60.0);
        assert_eq!(fluid.particle_count(), 0);
    }

    #[test]
    fn contacts_push_particles_out_and_report_reaction() {
        let basin = Basin { min_x: -200.0, max_x: 200.0, floor_y: 200.0 };
        let mut fluid = Fluid::new(FluidParams::default(), basin);
        fluid.spawn_block(Point2D::new(-30.0, -30.0), 5, 5, 14.0); // a block straddling the origin
        let pr = fluid.params().particle_radius;
        let radius = 30.0;
        let center = Point2D::new(0.0, 0.0);
        let contact = FluidContact::circle(center, radius);

        let reactions = fluid.resolve_contacts(&[contact]);

        // Every particle now sits at or beyond the contact surface (radius + particle radius).
        for p in fluid.positions() {
            let d = (p - center).length();
            assert!(d >= radius + pr - 0.5, "particle pushed out of the contact (d {d})");
        }
        // The fluid reported a non-zero reaction back on the overlapped solid.
        assert!(reactions[0].length() > 0.0, "non-zero reaction on the overlapped contact");
        // An empty contact list is a no-op.
        assert!(fluid.resolve_contacts(&[]).is_empty());
    }

    #[test]
    fn obb_contact_pushes_particles_out_of_the_box() {
        // An axis-aligned box (local == world) clears every particle from its inflated interior - the
        // true-shape coupling, vs the circle approximation that would over-push at the box's edges.
        let basin = Basin { min_x: -300.0, max_x: 300.0, floor_y: 300.0 };
        let mut fluid = Fluid::new(FluidParams::default(), basin);
        fluid.spawn_block(Point2D::new(-40.0, -40.0), 6, 6, 14.0); // a block over the origin
        let pr = fluid.params().particle_radius;
        let half = (50.0, 30.0); // a wide box, so the nearer-face push is exercised
        let contact = FluidContact::obb(Point2D::new(0.0, 0.0), half, 0.0);

        let reactions = fluid.resolve_contacts(&[contact]);

        for p in fluid.positions() {
            let inside = p.x.abs() < half.0 + pr - 0.5 && p.y.abs() < half.1 + pr - 0.5;
            assert!(!inside, "particle pushed out of the box (was at {p:?})");
        }
        assert!(reactions[0].length() > 0.0, "non-zero reaction on the overlapped box");
    }
}
