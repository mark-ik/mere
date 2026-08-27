// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Falling-sand ambient backdrop, multi-material (sand + water). (Physics scenes P5; see
//! [`crate::ambient`].)

use paint_list_api::{ColorF, CommonPlacement, LayoutPoint, LayoutRect, PaintCmd, RectItem};

use super::{AmbientSim, Tincture, xorshift};

const EMPTY: u8 = 0;
const SAND: u8 = 1;
const WATER: u8 = 2;

/// Falling sand + water: a gravity-driven cellular automaton with two materials. Sand pours from one
/// top source and water from another; sand falls + slides into angle-of-repose dunes, water falls +
/// spreads sideways to find its level, and sand sinks through water (it is the denser of the two).
/// The grid resets once it fills, so the pour, pile, and collapse cycle forever. A discrete CA like
/// Game of Life, but gravity-shaped. (Physics scenes P5.)
pub struct SandFall {
    width: usize,
    height: usize,
    /// Row-major cells: [`EMPTY`] / [`SAND`] / [`WATER`].
    cells: Vec<u8>,
    /// Per-step scratch: a cell that already moved (or received material) this step, so the
    /// left-to-right scan doesn't cascade a grain across the row in a single step.
    moved: Vec<bool>,
    /// xorshift state (which side a grain tries first - randomised to avoid a lean).
    rng: u32,
    /// Seconds accumulated toward the next step ([`AmbientSim::advance`] paces the fall rate).
    accum: f32,
}

impl SandFall {
    /// An empty `width` x `height` grid (sand + water arrive from the top sources on the first step).
    pub fn new(width: usize, height: usize, seed: u32) -> Self {
        let (w, h) = (width.max(3), height.max(3));
        Self {
            width: w,
            height: h,
            cells: vec![EMPTY; w * h],
            moved: vec![false; w * h],
            rng: seed | 1,
            accum: 0.0,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Whether `(col, row)` holds any material. Out-of-range reads as empty.
    pub fn cell(&self, col: usize, row: usize) -> bool {
        self.material(col, row) != EMPTY
    }

    /// The material at `(col, row)` ([`EMPTY`] out of range).
    fn material(&self, col: usize, row: usize) -> u8 {
        if col >= self.width || row >= self.height {
            return EMPTY;
        }
        self.cells[row * self.width + col]
    }

    /// Set / clear a grain of sand (for tests). Out-of-range is ignored.
    pub fn set_cell(&mut self, col: usize, row: usize, grain: bool) {
        self.set_material(col, row, if grain { SAND } else { EMPTY });
    }

    /// Set a cell's material (for tests). Out-of-range is ignored.
    pub fn set_material(&mut self, col: usize, row: usize, material: u8) {
        if col < self.width && row < self.height {
            self.cells[row * self.width + col] = material;
        }
    }

    /// The number of filled cells (sand + water).
    pub fn grain_count(&self) -> usize {
        self.cells.iter().filter(|&&c| c != EMPTY).count()
    }

    /// One step: reset a full grid, emit the two sources, then settle every cell bottom-up - sand
    /// falls / sinks-in-water / slides, water falls / slides / spreads sideways. The bottom row is
    /// never processed, so it is the floor everything piles on.
    pub fn step(&mut self) {
        let (w, h) = (self.width, self.height);
        // Reset a full grid (the pile has grown to the cap) so the pour starts over.
        if self.grain_count() > w * h * 2 / 5 {
            self.cells.iter_mut().for_each(|c| *c = EMPTY);
        }
        self.moved.iter_mut().for_each(|m| *m = false);
        // Two top-centre sources: sand just left of centre, water just right (so they meet and the
        // sand sinks through the water). Only into empty cells, so a backed-up source throttles.
        let mid = w as i32 / 2;
        for d in -7..=-2 {
            self.emit(mid + d, SAND);
        }
        for d in 2..=7 {
            self.emit(mid + d, WATER);
        }
        // Settle bottom-up; a cell that already moved this step is skipped (no single-step cascade).
        for r in (0..h - 1).rev() {
            for c in 0..w {
                let idx = r * w + c;
                if self.moved[idx] {
                    continue;
                }
                match self.cells[idx] {
                    SAND => self.settle_sand(c, r),
                    WATER => self.settle_water(c, r),
                    _ => {}
                }
            }
        }
    }

    /// Emit material at `(col, 0)` (the top row) if the cell is empty.
    fn emit(&mut self, col: i32, material: u8) {
        if col >= 0 && (col as usize) < self.width && self.cells[col as usize] == EMPTY {
            self.cells[col as usize] = material;
        }
    }

    /// The two horizontal directions, the first chosen at random (so neither side gets a lean).
    fn rand_dirs(&mut self) -> [i32; 2] {
        if xorshift(&mut self.rng) & 1 == 0 {
            [-1, 1]
        } else {
            [1, -1]
        }
    }

    /// Move the material at `src` to `dst` (which must be empty), leaving `src` empty; marks `dst`
    /// moved so it is not reprocessed this step.
    fn relocate(&mut self, src: usize, dst: usize) {
        self.cells[dst] = self.cells[src];
        self.cells[src] = EMPTY;
        self.moved[dst] = true;
    }

    /// Swap the materials at `src` and `dst` (sand sinking into water); both are marked moved.
    fn swap_cells(&mut self, src: usize, dst: usize) {
        self.cells.swap(src, dst);
        self.moved[src] = true;
        self.moved[dst] = true;
    }

    /// Settle a sand grain at `(c, r)`: fall straight down (into empty, or sink by swapping with
    /// water), else slide to a free / waterlogged diagonal.
    fn settle_sand(&mut self, c: usize, r: usize) {
        let w = self.width;
        let src = r * w + c;
        let below = (r + 1) * w + c;
        match self.cells[below] {
            EMPTY => return self.relocate(src, below),
            WATER => return self.swap_cells(src, below),
            _ => {}
        }
        for dx in self.rand_dirs() {
            let nc = c as i32 + dx;
            if nc < 0 || nc >= w as i32 {
                continue;
            }
            let diag = (r + 1) * w + nc as usize;
            match self.cells[diag] {
                EMPTY => return self.relocate(src, diag),
                WATER => return self.swap_cells(src, diag),
                _ => {}
            }
        }
    }

    /// Settle water at `(c, r)`: fall straight down, else slide to a free diagonal, else spread to a
    /// free side (so it finds its level).
    fn settle_water(&mut self, c: usize, r: usize) {
        let w = self.width;
        let src = r * w + c;
        if self.cells[(r + 1) * w + c] == EMPTY {
            return self.relocate(src, (r + 1) * w + c);
        }
        for dx in self.rand_dirs() {
            let nc = c as i32 + dx;
            if nc < 0 || nc >= w as i32 {
                continue;
            }
            if self.cells[(r + 1) * w + nc as usize] == EMPTY {
                return self.relocate(src, (r + 1) * w + nc as usize);
            }
        }
        for dx in self.rand_dirs() {
            let nc = c as i32 + dx;
            if nc < 0 || nc >= w as i32 {
                continue;
            }
            if self.cells[r * w + nc as usize] == EMPTY {
                return self.relocate(src, r * w + nc as usize);
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
        // Water is a translucent blue; sand takes the tincture. Merge each row into per-material runs
        // (one rect per run) to keep the command count low.
        let water = ColorF::new(0.32, 0.56, 0.92, 0.55);
        for row in 0..self.height {
            let mut col = 0;
            while col < self.width {
                let mat = self.cells[row * self.width + col];
                if mat == EMPTY {
                    col += 1;
                    continue;
                }
                let start = col;
                while col < self.width && self.cells[row * self.width + col] == mat {
                    col += 1;
                }
                let color = if mat == WATER { water } else { tincture };
                cmds.push(PaintCmd::DrawRect(RectItem {
                    placement: CommonPlacement::new(LayoutRect::new(
                        LayoutPoint::new(start as f32 * cw, row as f32 * ch),
                        LayoutPoint::new(col as f32 * cw, (row + 1) as f32 * ch),
                    )),
                    color,
                }));
            }
        }
        cmds
    }

    fn default_tincture(&self) -> Tincture {
        // Warm sand tan, moderately opaque (grains read as a solid-ish pour, behind the graph).
        // Water paints its own blue (see `paint`).
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
        assert!(
            !sand.cell(5, 5) && sand.cell(5, 6),
            "the grain fell one row"
        );
        // Running the sources + reset a while keeps cells flowing and the count bounded.
        for _ in 0..200 {
            sand.step();
        }
        let count = sand.grain_count();
        assert!(
            count > 0,
            "the sources keep material on the grid (was {count})"
        );
        assert!(
            count <= 20 * 20,
            "the count stays within the grid (was {count})"
        );
    }

    #[test]
    fn water_spreads_sideways_where_sand_piles() {
        // A blocked sand grain stays put; a blocked water cell spreads to a free side. Build a small
        // floor patch under each so neither can fall, away from the top sources.
        let mut g = SandFall::new(40, 20, 0x9A7E_0001);
        let floor = 18; // a row just above the very bottom (h - 2)
        for &(c, r) in &[(5, floor + 1), (4, floor + 1), (6, floor + 1)] {
            g.set_material(c, r, SAND); // floor under the sand grain
        }
        for &(c, r) in &[(20, floor + 1), (19, floor + 1), (21, floor + 1)] {
            g.set_material(c, r, SAND); // floor under the water cell
        }
        g.set_material(5, floor, SAND);
        g.set_material(20, floor, WATER);
        g.step();
        assert!(
            g.cell(5, floor),
            "blocked sand stays put (no sideways flow)"
        );
        assert!(!g.cell(20, floor), "blocked water flowed off to a side");
        assert!(
            g.cell(19, floor) || g.cell(21, floor),
            "the water moved to a free side, not vanished"
        );
    }
}
