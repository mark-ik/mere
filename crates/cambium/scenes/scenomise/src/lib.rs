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
//! [`registry`] holds the solver catalog behind [`sceno::Arrangement::Custom`],
//! which arrived here when the `scenograph` facade dissolved: a registry needs
//! the score contract and the solver contract at once, and this crate already
//! owns both. Its types are re-exported at the root exactly as the facade
//! exported them, and its realizer is [`solve_via`]: `solve` realizes the
//! named families, `solve_with` takes a planner closure, `solve_via` takes
//! the catalog (renamed from the facade's `solve` on 2026-09-03, since that
//! name was already this crate's closed-form solve).
//!
//! Product adapters choose sources, translate native facts to a score, and
//! realize the resulting scene. Where an arrangement needs something only the
//! source knows — a ring index from a graph walk, coordinates from a
//! dimensionality reduction — the adapter computes it once and discloses it on
//! the item, rather than shipping source truth for a solver to re-derive.

mod families;
mod relax;
pub mod registry;
mod solve;

pub use registry::{
    ArrangementId, Disclosure, RegisterError, SolveError, Solver, SolverCapability, SolverRegistry,
    solve_via,
};
pub use relax::{Relaxation, relax, relax_holding};
pub use solve::{pinned_instances, solve, solve_with};
