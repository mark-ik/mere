//! Scenograph: the facade for the scenograph projection-engine family.
//!
//! A thin re-export of the three members, for consumers that want one
//! dependency:
//!
//! - [`sceno`] — core contracts: sources, scores, channels, coordinate
//!   spaces, footprints, scenes, intents.
//! - [`scenomise`] — choreography: placement solvers that realize scores
//!   into arranged scenes.
//! - [`scenotime`] — runtime: incremental evaluation and the inhabited
//!   scene.
//!
//! Products with tight dependency budgets depend on the members directly;
//! `sceno` alone is the pure-types option.

pub use sceno;
pub use scenomise;
pub use scenotime;
