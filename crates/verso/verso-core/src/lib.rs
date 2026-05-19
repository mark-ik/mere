//! # Verso-Tile
//!
//! Tile-rendering-surface management for the
//! [`mere`](https://crates.io/crates/mere) browser. *Verso* is the brand-level
//! name (Latin *verso*: the back side of a printed leaf, the page that catches
//! the impression); `verso-tile` is the crate that owns rendering-surface
//! identity and command schedules. Mutable tile lifecycle state that depends on
//! `inker` lives in the sibling `verso-tile-state` crate.
//!
//! In the printing-press metaphor that organizes Mere's architecture:
//! engines produce ink, the inker pairs each ink to its content, the
//! [`platen`](https://crates.io/crates/platen) presses the inked content,
//! and `verso-tile` is the surface that receives the impression.
//!
//! ## Status
//!
//! Pre-1.0. This 0.0.x release reserves the crate name and documents intent;
//! implementation is in progress within the
//! [Mere workspace](https://crates.io/crates/mere).

#![doc(html_root_url = "https://docs.rs/verso-tile/0.0.1")]

/// Apply algorithm for `SurfaceCommandSchedule`.
pub mod apply;
/// Portable rendering-surface identity types.
pub mod surface;

pub use apply::{
    SurfaceScheduleApplyError, SurfaceScheduleApplyReport, apply_viewer_surface_schedule,
};
pub use surface::{
    SurfaceCommand, SurfaceCommandBacklog, SurfaceCommandOutcome, SurfaceCommandSchedule,
    SurfaceCommandSink, SurfaceCommandStatus, SurfaceEffect, SurfaceHostId, SurfaceLifecycleState,
    SurfacePlacementPlan, SurfaceRequest, SurfaceSlotPlacement, SurfaceTargetId, TileSlot,
};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
