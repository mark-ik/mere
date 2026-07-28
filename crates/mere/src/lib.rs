//! Mere's composed graph surface.
//!
//! Applications depend on this crate for graph truth and its portable
//! projections. It deliberately does not expose browser-window, input, render,
//! or session-host concerns; those belong to a consumer such as Turnstone.
//!
//! Capability features keep that facade honest:
//!
//! - `graph`: portable graph and domain vocabulary;
//! - `linked-data`: semantic import/export;
//! - `canvas`: Mere's graph-aware visual surface;
//! - `workbench`: app-host composition and routing.

/// Mere's routing vocabulary over inker's host-neutral policy: the
/// app-flavored host-handled ids + [`routing::route_policy`].
#[cfg(feature = "workbench")]
pub mod routing;

#[cfg(feature = "workbench")]
pub use apparatus;
#[cfg(feature = "canvas")]
pub use canvas;
#[cfg(feature = "graph")]
pub use forme;
#[cfg(feature = "canvas")]
pub use gloss;
#[cfg(feature = "graph")]
pub use glossary;
#[cfg(feature = "graph")]
pub use graphlets;
#[cfg(feature = "graph")]
pub use kernel;
#[cfg(feature = "linked-data")]
pub use linked_data;
#[cfg(feature = "workbench")]
pub use platen;
#[cfg(feature = "graph")]
pub use roster;
#[cfg(feature = "graph")]
pub use trail;
#[cfg(feature = "workbench")]
pub use workbench;
