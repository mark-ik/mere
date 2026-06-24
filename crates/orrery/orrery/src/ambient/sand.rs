/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Falling-sand ambient backdrop. (Physics scenes P5; see [`crate::ambient`].)

use paint_list_api::{ColorF, CommonPlacement, LayoutPoint, LayoutRect, PaintCmd, RectItem};

use super::{AmbientSim, Tincture, xorshift};

/// Falling sand: a gravity-driven cellular automaton. Grains pour from a source at the top, fall,
/// and slide off slopes into a dune, and the grid resets once it fills - so the pour, pile, and
/// collapse cycle forever. A discrete CA like Game of Life, but gravity-shaped - it pours and piles
/// rather than oscillating in place. (Physics scenes P5.)
pub struct SandFall {
    width: usize,
    height: usize,
    /// Row-major cells: 0 empty, 1 a grain of sand.
    cells: Vec<u8>,
    /// xorshift state (which diagonal a blocked grain tries first - randomised to avoid a lean).
    rng: u32,
    /// Seconds accumulated toward the next step ([`AmbientSim::advance`] paces the fall rate).
    accum: f32,
}

impl SandFall {
    /// An empty `width` x `height` sand grid (grains arrive from the top source on the first step).
    pub fn new(width: usize, height: usize, seed: u32) -> Self {
        let (w, h) = (width.max(3), height.max(3));
        Self { width: w, height: h, cells: vec![0u8; w * h], rng: seed | 1, accum: 0.0 }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Whether `(col, row)` holds a grain. Out-of-range reads as empty.
    pub fn cell(&self, col: usize, row: usize) -> bool {
        if col >= self.width || row >= self.height {
            return false;
        }
        self.cells[row * self.width + col] != 0
    }

    /// Set / clear a grain (for tests). Out-of-range is ignored.
    pub fn set_cell(&mut self, col: usize, row: usize, grain: bool) {
        if col < self.width && row < self.height {
            self.cells[row * self.width + col] = u8::from(grain);
        }
    }

    /// The number of grains on the grid.
    pub fn grain_count(&self) -> usize {
        self.cells.iter().filter(|&&c| c != 0).count()
    }

    /// One sand step: emit a stream from the top-centre, settle every grain down one row (straight
    /// down, else a diagonal slide so it builds an angle-of-repose dune on the floor), and reset the
    /// grid once it fills - so the pour, pile, and collapse cycle forever.
    pub fn step(&mut self) {
        let (w, h) = (self.width, self.height);
        // Reset a full grid (the dune has grown to the cap) so the pour starts over - the collapse
        // that keeps the backdrop cycling rather than freezing once filled.
        let filled = self.cells.iter().filter(|&&c| c != 0).count();
        if filled > w * h * 2 / 5 {
            self.cells.iter_mut().for_each(|c| *c = 0);
        }
        // Source: a short stream from the top-centre (only into empty cells, so a backed-up source
        // throttles itself).
        let mid = w as i32 / 2;
        for d in -3..=3 {
            let c = mid + d;
            if c >= 0 && (c as usize) < w && self.cells[c as usize] == 0 {
                self.cells[c as usize] = 1;
            }
        }
        // Settle bottom-up (in place): a grain drops into the row below (already settled this step),
        // straight down if free, else sliding to a free diagonal (first side chosen at random). The
        // bottom row is never processed, so it is the floor grains pile on.
        for r in (0..h - 1).rev() {
            for c in 0..w {
                if self.cells[r * w + c] == 0 {
                    continue;
                }
                if self.cells[(r + 1) * w + c] == 0 {
                    self.cells[(r + 1) * w + c] = 1;
                    self.cells[r * w + c] = 0;
                    continue;
                }
                let dirs = if xorshift(&mut self.rng) & 1 == 0 { [-1i32, 1] } else { [1, -1] };
                for &dx in &dirs {
                    let nc = c as i32 + dx;
                    if nc < 0 || nc >= w as i32 {
                        continue;
                    }
                    let diag = (r + 1) * w + nc as usize;
                    if self.cells[diag] == 0 {
                        self.cells[diag] = 1;
                        self.cells[r * w + c] = 0;
                        break;
                    }
                }
            }
        }
    }
}

impl AmbientSim for SandFall {
    fn advance(&mut self, dt: f32) {
        // ~30 steps a second for a smooth fall; a small per-frame budget guards a dt spike.
        const STEP_INTERVAL: f32 = 1.0 / 30.0;
        self.accum += dt;
        let mut budget = 4;
        while self.accum >= STEP_INTERVAL && budget > 0 {
            self.accum -= STEP_INTERVAL;
            self.step();
            budget -= 1;
        }
    }

    fn paint(&self, w: f32, h: f32, tincture: Tincture) -> Vec<PaintCmd> {
        let mut cmds = Vec::new();
        if self.width == 0 || self.height == 0 {
            return cmds;
        }
        let cw = w / self.width as f32;
        let ch = h / self.height as f32;
        // Merge each row's grains into runs (one rect per run), like the Game of Life paint.
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
        // Warm sand tan, moderately opaque (grains read as a solid-ish pour, behind the graph).
        ColorF::new(0.84, 0.71, 0.42, 0.55)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sand_falls_and_stays_bounded() {
        let mut sand = SandFall::new(20, 20, 0x5A1D_0001);
        // A grain mid-grid drops exactly one row per step.
        sand.set_cell(5, 5, true);
        sand.step();
        assert!(!sand.cell(5, 5) && sand.cell(5, 6), "the grain fell one row");
        // Running the source + reset a while keeps grains flowing and the count bounded.
        for _ in 0..200 {
            sand.step();
        }
        let count = sand.grain_count();
        assert!(count > 0, "the source keeps grains on the grid (was {count})");
        assert!(count <= 20 * 20, "the grain count stays within the grid (was {count})");
    }
}
