//! Scenograph: the facade for the scenograph projection-engine family.
//!
//! A thin re-export of the three members, for consumers that want one
//! dependency:
//!
//! - [`sceno`] — core contracts: sources, scores, coordinate spaces,
//!   footprints, scenes, and per-item emphasis channels. Deliberately not
//!   intents; see that crate's docs for where the reverse path lives.
//! - [`scenomise`] — choreography: placement solvers that realize scores
//!   into arranged scenes.
//! - [`scenotime`] — runtime: stable epochs, incremental diffs, and picking
//!   the instance under a point.
//!
//! Products with tight dependency budgets depend on the members directly;
//! `sceno` alone is the pure-types option.
//!
//! The facade also owns the one thing no single member can: the [`registry`]
//! behind [`sceno::Arrangement::Custom`]. A registry needs the score contract
//! and the solver contract at once, and lives above both so `scenomise` never
//! learns what a registry is.

pub mod registry;

pub use registry::{
    ArrangementId, Disclosure, RegisterError, SolveError, Solver, SolverCapability, SolverRegistry,
    solve,
};
pub use sceno;
pub use scenomise;
pub use scenotime;
