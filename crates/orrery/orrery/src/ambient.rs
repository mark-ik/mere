/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Ambient-sim backdrops: small standalone simulations painted behind the graph for liveliness the
//! rapier solver should not carry (the "ambient separate-sim backdrop" tier, physics scenes P5).
//! Non-rapier and host-side (cheap, no actor offload), stepped slowly and painted as the bottom
//! backdrop layer. The first is Conway's [`GameOfLife`]; n-body drift / particle-life are siblings.
//!
//! [`GameOfLife::step`] is pure Conway (B3/S23) so it tests cleanly; [`GameOfLife::step_living`] is
//! the host-facing wrapper that reseeds a thinning field so the backdrop never goes permanently dead.

/// Conway's Game of Life on a wrapped (toroidal) grid: the first ambient backdrop. The grid is
/// resolution-fixed (the host stretches it across the viewport at paint time); the host steps it a
/// few generations a second and paints the live cells as a muted backdrop behind the graph. (P5.)
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
}

impl GameOfLife {
    /// A `width` x `height` grid seeded with a deterministic random soup (~30% alive) from `seed`.
    pub fn seeded(width: usize, height: usize, seed: u32) -> Self {
        let (w, h) = (width.max(1), height.max(1));
        let mut gol = Self {
            width: w,
            height: h,
            cells: vec![false; w * h],
            scratch: vec![false; w * h],
            rng: seed | 1,
            alive: 0,
            generation: 0,
        };
        gol.reseed();
        gol
    }

    /// An empty (all-dead) `width` x `height` grid — the base for seeding explicit patterns (a
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
    /// `reseed_every` generations (`0` disables the periodic reseed) — so the backdrop keeps living
    /// rather than freezing into still-lifes. The host-facing step. (P5.)
    pub fn step_living(&mut self, reseed_every: u32) {
        self.step();
        self.generation = self.generation.wrapping_add(1);
        if self.alive == 0 || (reseed_every > 0 && self.generation % reseed_every == 0) {
            self.reseed();
        }
    }
}

/// xorshift32 — a tiny deterministic PRNG for the random soup (no `rand` dep, reproducible).
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
        // Now vertical: the centre column, rows 1..=3.
        assert_eq!(live_set(&gol), vec![(2, 1), (2, 2), (2, 3)], "blinker turned vertical");
        assert_eq!(gol.alive_count(), 3, "a blinker keeps three cells");

        gol.step();
        assert_eq!(live_set(&gol), start, "two steps return the blinker to its start");
    }

    #[test]
    fn block_is_a_still_life() {
        // A 2x2 block is stable under Conway's rules.
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
    fn step_living_reseeds_a_dead_field() {
        // An empty field is dead; the living step reseeds it back to life.
        let mut gol = GameOfLife::empty(20, 20);
        gol.step_living(0);
        assert!(gol.alive_count() > 0, "step_living revives a dead field");
    }
}
