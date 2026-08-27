// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Scenomise: choreography for the scenograph projection engine.
//!
//! To scenomise is to arrange into a scene (mise-en-scÃ¨ne lives in the name).
//! This crate holds the placement solvers that realize a score into an
//! arranged scene: spirals, grids, concentric rings, aperiodic tilings,
//! fractal paths, layered stacks, axial boards, embedded coordinates,
//! adjacency-preserving tiling, and geographic transforms. Solvers read
//! [`sceno`]'s contracts and emit placed instances with footprints; they never
//! render, and they never learn a source's native truth.
//!
//! Every solver here is closed-form: a score in, placed instances out, in one
//! pass with no state carried between calls. Live force physics is `seiche`'s
//! domain and has no solver here.
//!
//! Product adapters choose sources, translate native facts to a score, and
//! realize the resulting scene. Where an arrangement needs something only the
//! source knows — a ring index from a graph walk, coordinates from a
//! dimensionality reduction — the adapter computes it once and discloses it on
//! the item, rather than shipping source truth for a solver to re-derive.

mod families;
mod relax;
mod solve;

pub use relax::{Relaxation, relax, relax_holding};
pub use solve::{pinned_instances, solve, solve_with};
