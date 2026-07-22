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
//! Name reservation: the algorithms migrate in with the engine's first
//! consumers.
