//! Workbench tile lifecycle state for Verso.
//!
//! `verso-core` owns surface identity and command schedules. This crate owns
//! the mutable tile state that depends on `inker` engine output: within-tile
//! navigation history, cached document tiles, and long-lived
//! surface-producer-backed tiles.

#![doc(html_root_url = "https://docs.rs/tile-state/0.0.1")]

/// Long-lived surface-producer tile state.
pub mod surface_tile;
/// Workbench tile state and within-tile navigation history.
pub mod tiles;

pub use surface_tile::{SurfaceTileState, SurfaceTileStep};
pub use tiles::{HistoryEntry, NavigateMode, TileManager, TileState};
