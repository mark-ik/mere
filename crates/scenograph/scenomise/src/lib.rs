//! Scenomise: choreography for the scenograph projection engine.
//!
//! To scenomise is to arrange into a scene (mise-en-scène lives in the name).
//! This crate holds the placement solvers that realize a score into an
//! arranged scene: analytic layouts (spirals, grids, radial, aperiodic
//! tilings, axial boards), rectangular subdivision, adjacency-preserving
//! tiling, and geographic transforms. Solvers read [`sceno`]'s contracts and
//! emit placed instances with footprints; they never render, and they never
//! learn a source's native truth.
//!
//! The first consumer proof moves the generic Spiral, board, and geographic
//! solvers here. Product adapters choose sources, translate native facts to a
//! score, and realize the resulting scene.

mod relax;
mod solve;

pub use relax::{Relaxation, relax};
pub use solve::solve;
