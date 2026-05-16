// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! # mere-masonry
//!
//! First-cut sketch (2026-05-15) of a Mere-substrate driver for the Masonry
//! widget engine. See [`README.md`](../README.md) for the architectural framing
//! and which parts are wired vs. TODO.
//!
//! The public surface this crate exposes is small on purpose:
//!
//! - [`MasonryTile`] — one masonry composition root, scoped to a single tile.
//!   The host constructs one per panel-shaped `SceneNode`, drives it per
//!   frame, and disposes it when the node is removed.
//! - [`TileSignal`] — the substrate-shaped projection of masonry's
//!   `RenderRootSignal`. The host drains these per frame and routes each
//!   to the appropriate substrate sink (action bus, IME bridge, cursor
//!   manager, etc.).
//! - [`TileSize`] — newtype for tile-local physical size (the rect masonry
//!   should lay out for).
//!
//! ## Composition with xilem (opt-in)
//!
//! Callers who want declarative reactive panels compose xilem on top:
//!
//! ```ignore
//! // Caller side (sketch, not in this crate):
//! //
//! //   1. Construct a `MasonryTile` for the panel.
//! //   2. Use `xilem_masonry::Xilem` (or equivalent) to drive a
//! //      `View<PanelState, PanelAction>` against the tile's RenderRoot.
//! //   3. Each frame: hand `MasonryTile::render` the substrate's vello
//! //      scene and the tile's placement transform.
//! ```
//!
//! Callers who want raw masonry widgets get the same `MasonryTile` API;
//! the xilem layer is not bolted into this crate.

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

mod input;
mod renderer;
mod signal;
mod tile;

pub use signal::TileSignal;
pub use tile::{MasonryTile, TileSize};

// Re-export the surfaces callers need so they don't have to add the matching
// dep version themselves. Every load-bearing crate is a direct dep of this
// crate (see Cargo.toml) — no transitive resolution through registry's
// re-exports.
pub use accesskit;
pub use kurbo;
pub use masonry_core;
pub use mere_renderer_registry;
pub use ui_events;
pub use vello;

// `masonry_imaging` is a runtime backend dep (provides `imaging_vello`
// against masonry_core's imaging trait surface); nothing in this crate
// imports it directly, but it must link to satisfy masonry's backend.
use masonry_imaging as _;
