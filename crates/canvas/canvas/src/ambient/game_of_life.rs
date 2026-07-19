// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Conway's Game of Life ambient backdrop. (Physics scenes P5; see [`crate::ambient`].)

use paint_list_api::{ColorF, CommonPlacement, LayoutPoint, LayoutRect, PaintCmd, RectItem};

use super::{AmbientSim, Tincture, xorshift};

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
        assert_eq!(
            live_set(&gol),
            vec![(2, 1), (2, 2), (2, 3)],
            "blinker turned vertical"
        );
        assert_eq!(gol.alive_count(), 3, "a blinker keeps three cells");

        gol.step();
        assert_eq!(
            live_set(&gol),
            start,
            "two steps return the blinker to its start"
        );
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
        assert_eq!(
            empty.alive_count(),
            0,
            "nothing is born from an empty field"
        );

        let gol = GameOfLife::seeded(40, 40, 0x1234_5678);
        let alive = gol.alive_count();
        assert!(
            alive > 0 && alive < 40 * 40,
            "a random soup is partly alive (was {alive})"
        );
    }

    #[test]
    fn advance_paces_generations_and_reseeds_a_dead_field() {
        // A dead field revives once `advance` accumulates a generation's worth of time.
        let mut gol = GameOfLife::empty(20, 20);
        gol.advance(0.2); // > one 1/8s generation interval
        assert!(
            gol.alive_count() > 0,
            "advance steps a generation and revives a dead field"
        );

        // A short dt under the interval accumulates without stepping (no panic, still empty pattern
        // semantics): start from a known blinker and confirm a tiny advance does not yet flip it.
        let mut blink = GameOfLife::empty(5, 5);
        blink.set_cell(1, 2, true);
        blink.set_cell(2, 2, true);
        blink.set_cell(3, 2, true);
        let before = live_set(&blink);
        blink.advance(0.01);
        assert_eq!(
            live_set(&blink),
            before,
            "a sub-interval advance does not step a generation"
        );
    }
}
