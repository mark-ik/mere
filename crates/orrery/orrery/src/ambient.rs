/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Ambient-sim backdrops: small standalone simulations painted behind the graph for liveliness the
//! rapier solver should not carry (the "ambient separate-sim backdrop" tier, physics scenes P5).
//! Non-rapier and host-side (cheap, no actor offload), advanced and painted each frame as the bottom
//! backdrop layer. They share the [`AmbientSim`] seam (advance + paint), so the orrery holds one
//! `Box<dyn AmbientSim>` and the catalog grows (Game of Life, n-body drift, particle-life) without
//! touching the host. Each is painted in a [`Tincture`] - a base colour it interprets as it likes.
//!
//! The first is Conway's [`GameOfLife`]. Its [`step`](GameOfLife::step) is pure Conway (B3/S23) so it
//! tests cleanly; the [`AmbientSim::advance`] impl paces generations and reseeds a thinning field so
//! the backdrop never freezes.

use paint_list_api::{ColorF, CommonPlacement, LayoutPoint, LayoutRect, PaintCmd, RectItem};

/// The base colour an ambient sim is painted in - a tint / wash. One colour the sim interprets as it
/// sees fit (Game of Life tints its cells; particle-life rotates the hue per species). Overridable
/// per backdrop via [`Orrery::set_ambient_tincture`](crate::Orrery::set_ambient_tincture). (P5.)
pub type Tincture = ColorF;

/// A non-rapier ambient backdrop simulation: advance it by `dt`, then paint it across the viewport in
/// a [`Tincture`]. The orrery holds one as `Box<dyn AmbientSim>`, advancing + painting it each frame
/// as the bottom layer and keep-redrawing while it is live. `Send` so a backdrop could later move to
/// an actor if one grows expensive. (Physics scenes P5.)
pub trait AmbientSim: Send {
    /// Advance the sim by `dt` seconds. Discrete sims (Game of Life) accumulate `dt` and step a
    /// generation at their own cadence; continuous sims (n-body) integrate by `dt`.
    fn advance(&mut self, dt: f32);
    /// Paint the sim across a `w` x `h` px viewport, tinted by `tincture`, as backdrop paint commands.
    fn paint(&self, w: f32, h: f32, tincture: Tincture) -> Vec<PaintCmd>;
    /// The sim's natural tincture (its default base colour), used unless the host overrides it.
    fn default_tincture(&self) -> Tincture;
}

/// Conway's Game of Life on a wrapped (toroidal) grid: the first ambient backdrop. The grid is
/// resolution-fixed (the host stretches it across the viewport at paint time); generations advance a
/// few a second behind the graph, and a thinning field reseeds so it never goes permanently dead.
pub struct GameOfLife {
    width: usize,
    height: usize,
    cells: Vec<bool>,
    /// Double-buffer scratch for a step (swapped in, not reallocated each generation).
    scratch: Vec<bool>,
    /// xorshift state for the random soup (deterministic from the seed, reproducible headless).
    rng: u32,
    /// Cached live-cell count (kept in step / reseed so the host can cheaply test for a dead field).
    alive: usize,
    /// Generations elapsed (drives the periodic reseed in [`Self::step_living`]).
    generation: u32,
    /// Seconds accumulated toward the next generation ([`AmbientSim::advance`] paces the rate).
    accum: f32,
}

impl GameOfLife {
    /// A `width` x `height` grid seeded with a deterministic random soup (~30% alive) from `seed`.
    pub fn seeded(width: usize, height: usize, seed: u32) -> Self {
        let mut gol = Self::empty(width, height);
        gol.rng = seed | 1;
        gol.reseed();
        gol
    }

    /// An empty (all-dead) `width` x `height` grid - the base for seeding explicit patterns (a
    /// glider gun, a test blinker). Use [`set_cell`](Self::set_cell) to populate it.
    pub fn empty(width: usize, height: usize) -> Self {
        let (w, h) = (width.max(1), height.max(1));
        Self {
            width: w,
            height: h,
            cells: vec![false; w * h],
            scratch: vec![false; w * h],
            rng: 1,
            alive: 0,
            generation: 0,
            accum: 0.0,
        }
    }

    /// Re-fill the grid with a random soup (~30% alive). Called at construction and by
    /// [`step_living`](Self::step_living) when the field thins out, so the backdrop stays lively.
    pub fn reseed(&mut self) {
        self.alive = 0;
        for cell in self.cells.iter_mut() {
            let alive = (xorshift(&mut self.rng) % 100) < 30;
            *cell = alive;
            self.alive += usize::from(alive);
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// The number of live cells (cached; updated each step / reseed).
    pub fn alive_count(&self) -> usize {
        self.alive
    }

    /// Whether the cell at `(col, row)` is alive. Out-of-range reads as dead.
    pub fn cell(&self, col: usize, row: usize) -> bool {
        if col >= self.width || row >= self.height {
            return false;
        }
        self.cells[row * self.width + col]
    }

    /// Set a cell alive / dead (for seeding explicit patterns). Out-of-range is ignored.
    pub fn set_cell(&mut self, col: usize, row: usize, alive: bool) {
        if col >= self.width || row >= self.height {
            return;
        }
        let idx = row * self.width + col;
        if self.cells[idx] != alive {
            self.cells[idx] = alive;
            if alive {
                self.alive += 1;
            } else {
                self.alive -= 1;
            }
        }
    }

    /// Advance one Conway generation (B3/S23) on the toroidal grid. Pure: no reseed, so a pattern
    /// (blinker, glider) evolves exactly. (Use [`step_living`](Self::step_living) for the backdrop.)
    pub fn step(&mut self) {
        let (w, h) = (self.width, self.height);
        let mut alive = 0;
        for row in 0..h {
            // Toroidal row neighbours via modular offsets (h - 1 == -1 mod h), avoiding signed math.
            let up = (row + h - 1) % h;
            let down = (row + 1) % h;
            for col in 0..w {
                let left = (col + w - 1) % w;
                let right = (col + 1) % w;
                let n = usize::from(self.cells[up * w + left])
                    + usize::from(self.cells[up * w + col])
                    + usize::from(self.cells[up * w + right])
                    + usize::from(self.cells[row * w + left])
                    + usize::from(self.cells[row * w + right])
                    + usize::from(self.cells[down * w + left])
                    + usize::from(self.cells[down * w + col])
                    + usize::from(self.cells[down * w + right]);
                let live = self.cells[row * w + col];
                let next = matches!((live, n), (true, 2) | (true, 3) | (false, 3));
                self.scratch[row * w + col] = next;
                alive += usize::from(next);
            }
        }
        std::mem::swap(&mut self.cells, &mut self.scratch);
        self.alive = alive;
    }

    /// Step one generation, then reseed if the field has died out (`alive == 0`) or every
    /// `reseed_every` generations (`0` disables the periodic reseed) - so the backdrop keeps living
    /// rather than freezing into still-lifes.
    pub fn step_living(&mut self, reseed_every: u32) {
        self.step();
        self.generation = self.generation.wrapping_add(1);
        if self.alive == 0 || (reseed_every > 0 && self.generation % reseed_every == 0) {
            self.reseed();
        }
    }
}

impl AmbientSim for GameOfLife {
    fn advance(&mut self, dt: f32) {
        // ~8 generations a second (a watchable pace), reseeding periodically for endless variety.
        const GEN_INTERVAL: f32 = 1.0 / 8.0;
        const RESEED_GENS: u32 = 600;
        self.accum += dt;
        while self.accum >= GEN_INTERVAL {
            self.accum -= GEN_INTERVAL;
            self.step_living(RESEED_GENS);
        }
    }

    fn paint(&self, w: f32, h: f32, tincture: Tincture) -> Vec<PaintCmd> {
        let mut cmds = Vec::new();
        if self.width == 0 || self.height == 0 {
            return cmds;
        }
        let cw = w / self.width as f32;
        let ch = h / self.height as f32;
        // Merge each row's live cells into runs (one rect per run) to keep the command count low.
        for row in 0..self.height {
            let mut col = 0;
            while col < self.width {
                if !self.cell(col, row) {
                    col += 1;
                    continue;
                }
                let start = col;
                while col < self.width && self.cell(col, row) {
                    col += 1;
                }
                cmds.push(PaintCmd::DrawRect(RectItem {
                    placement: CommonPlacement::new(LayoutRect::new(
                        LayoutPoint::new(start as f32 * cw, row as f32 * ch),
                        LayoutPoint::new(col as f32 * cw, (row + 1) as f32 * ch),
                    )),
                    color: tincture,
                }));
            }
        }
        cmds
    }

    fn default_tincture(&self) -> Tincture {
        // A phosphor green at a moderate alpha: vivid enough that the hue reads over the dark
        // backdrop (a low alpha washes any colour to grey), still translucent so the graph stays the
        // foreground. (Tincture pass.)
        ColorF::new(0.26, 0.92, 0.50, 0.40)
    }
}

/// An n-body orbital drift: a cloud of bodies orbiting a central well and tugging on one another,
/// for a slow galaxy-like swirl behind the graph. A harmonic central well keeps the cloud bound and
/// orbiting (stable, no fly-away / collapse), weak softened mutual gravity adds clumping and
/// structure. Continuous (integrated each frame). (Physics scenes P5.)
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
    /// A `count`-body disk around the centre, each on a near-circular orbit (tangential velocity
    /// `sqrt(K) * radius`), seeded deterministically from `seed`.
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
        Self { pos, vel, species, attract }
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
                let f = pl_force(d / PL_RADIUS, self.attract[self.species[i]][self.species[j]]);
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

/// The hue (in `[0, 1)`) of an RGB colour; grey returns 0. (For rotating a tincture into a palette.)
fn rgb_to_hue(c: ColorF) -> f32 {
    let (r, g, b) = (c.r, c.g, c.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    if d <= 1e-6 {
        return 0.0;
    }
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0).rem_euclid(1.0)
}

/// An RGBA colour from hue `h` in `[0, 1)`, saturation `s`, value `v`, alpha `a`.
fn hsv_to_rgb(h: f32, s: f32, v: f32, a: f32) -> ColorF {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ColorF::new(r, g, b, a)
}

/// A deterministic `[0, 1)` sample from the xorshift PRNG.
fn unit(state: &mut u32) -> f32 {
    xorshift(state) as f32 / u32::MAX as f32
}

/// xorshift32 - a tiny deterministic PRNG for the random soup (no `rand` dep, reproducible).
fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live cells as a sorted `(col, row)` set, for comparing generations.
    fn live_set(gol: &GameOfLife) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for row in 0..gol.height() {
            for col in 0..gol.width() {
                if gol.cell(col, row) {
                    v.push((col, row));
                }
            }
        }
        v
    }

    #[test]
    fn blinker_oscillates_with_period_two() {
        // A horizontal blinker (three in a row) flips to vertical and back. (B3/S23.)
        let mut gol = GameOfLife::empty(5, 5);
        gol.set_cell(1, 2, true);
        gol.set_cell(2, 2, true);
        gol.set_cell(3, 2, true);
        let start = live_set(&gol);
        assert_eq!(gol.alive_count(), 3);

        gol.step();
        assert_eq!(live_set(&gol), vec![(2, 1), (2, 2), (2, 3)], "blinker turned vertical");
        assert_eq!(gol.alive_count(), 3, "a blinker keeps three cells");

        gol.step();
        assert_eq!(live_set(&gol), start, "two steps return the blinker to its start");
    }

    #[test]
    fn block_is_a_still_life() {
        let mut gol = GameOfLife::empty(4, 4);
        for &(c, r) in &[(1, 1), (2, 1), (1, 2), (2, 2)] {
            gol.set_cell(c, r, true);
        }
        let before = live_set(&gol);
        gol.step();
        assert_eq!(live_set(&gol), before, "a block does not change");
    }

    #[test]
    fn empty_grid_stays_dead_and_seeded_grid_lives() {
        let mut empty = GameOfLife::empty(8, 8);
        assert_eq!(empty.alive_count(), 0);
        empty.step();
        assert_eq!(empty.alive_count(), 0, "nothing is born from an empty field");

        let gol = GameOfLife::seeded(40, 40, 0x1234_5678);
        let alive = gol.alive_count();
        assert!(alive > 0 && alive < 40 * 40, "a random soup is partly alive (was {alive})");
    }

    #[test]
    fn advance_paces_generations_and_reseeds_a_dead_field() {
        // A dead field revives once `advance` accumulates a generation's worth of time.
        let mut gol = GameOfLife::empty(20, 20);
        gol.advance(0.2); // > one 1/8s generation interval
        assert!(gol.alive_count() > 0, "advance steps a generation and revives a dead field");

        // A short dt under the interval accumulates without stepping (no panic, still empty pattern
        // semantics): start from a known blinker and confirm a tiny advance does not yet flip it.
        let mut blink = GameOfLife::empty(5, 5);
        blink.set_cell(1, 2, true);
        blink.set_cell(2, 2, true);
        blink.set_cell(3, 2, true);
        let before = live_set(&blink);
        blink.advance(0.01);
        assert_eq!(live_set(&blink), before, "a sub-interval advance does not step a generation");
    }

    #[test]
    fn nbody_stays_bounded_and_finite() {
        // The central well must keep the cloud bound (no fly-away / NaN blow-up) over a long run.
        let mut nb = NBody::seeded(80, 0xABCD_1234);
        for _ in 0..900 {
            nb.advance(1.0 / 60.0);
        }
        assert_eq!(nb.body_count(), 80);
        for &(x, y) in nb.pos.iter() {
            assert!(x.is_finite() && y.is_finite(), "positions stay finite ({x}, {y})");
            assert!(x > -600.0 && x < NBODY_SPAN + 600.0, "x stays near the well (was {x})");
            assert!(y > -600.0 && y < NBODY_SPAN + 600.0, "y stays near the well (was {y})");
        }
    }

    #[test]
    fn particle_life_stays_in_bounds_and_finite() {
        // The toroidal wrap + heavy friction must keep every particle finite and on the grid.
        let mut pl = ParticleLife::seeded(120, 0x515E_0001);
        for _ in 0..600 {
            pl.advance(1.0 / 60.0);
        }
        assert_eq!(pl.particle_count(), 120);
        for &(x, y) in pl.pos.iter() {
            assert!(x.is_finite() && y.is_finite(), "positions stay finite ({x}, {y})");
            assert!((0.0..=PL_SPAN).contains(&x), "wrapped x in bounds (was {x})");
            assert!((0.0..=PL_SPAN).contains(&y), "wrapped y in bounds (was {y})");
        }
    }

    #[test]
    fn hsv_round_trips_the_hue() {
        // A pure-green tincture should rotate cleanly: hue 1/3, recoverable from the built colour.
        let green = hsv_to_rgb(1.0 / 3.0, 0.8, 0.9, 1.0);
        assert!((rgb_to_hue(green) - 1.0 / 3.0).abs() < 0.02, "hue round-trips");
    }
}
