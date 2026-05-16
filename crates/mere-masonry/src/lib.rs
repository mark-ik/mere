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
//!
//! ## Two-vello version skew (read before implementing scene-merge)
//!
//! This crate's dependency tree contains **two distinct `vello` versions**
//! that are *not* interoperable at the type level:
//!
//! - `vello = "0.9"` (direct dep, workspace-pinned) — the substrate-side
//!   scene the host paints into. Same version as `mere-renderer-registry`.
//! - `vello 0.8.x` (transitive via `masonry_imaging::imaging_vello`) — xilem
//!   hasn't bumped yet. This is what `masonry_imaging::vello::Renderer`
//!   produces; what `imaging_vello`'s `PaintSink` expects.
//!
//! For Rust's type system these are **different crates entirely**: a
//! `vello::Scene` from one cannot be passed to a `PaintSink` from the other.
//! A naive scene-merge implementation hits this with confusing errors —
//! "expected `vello::Scene`, found `vello::Scene`" — because the type names
//! match but the crate identities don't.
//!
//! **Three resolution paths**, in order of preference:
//!
//! 1. **Wait for xilem's 0.9 bump.** When upstream xilem catches up to vello
//!    0.9, this entire skew dissolves; one version, one Scene type, scene-merge
//!    becomes the trivial `target.append(&masonry_scene, transform)` Option A
//!    from the contract brief. Watch [xilem repo](https://github.com/linebender/xilem)
//!    for the bump.
//! 2. **Texture round-trip** (Option B in [`tile::MasonryTile::render`]'s
//!    doc comment). Render masonry's layers into a wgpu texture via
//!    `masonry_imaging::vello::Renderer` (xilem-side vello 0.8), then
//!    composite that texture into the host scene as a textured quad via
//!    vello 0.9. The two vellos never touch the same Scene type — the wgpu
//!    texture handle is the version-neutral bridge. Mechanical to write,
//!    pays per-frame texture round-trip cost.
//! 3. **Explicit 0.8 → 0.9 scene adapter** (Option A with a version
//!    converter). Walk the 0.8 `imaging::record::Scene` ops and re-emit them
//!    against the 0.9 `vello::Scene` API. Correct shape, but reinvents the
//!    interop that path 1 gets for free. Only worth it if path 1 stalls and
//!    path 2's texture round-trip becomes measurable in profiling.
//!
//! The current `MasonryTile::render` deliberately doesn't paint into the
//! target scene — it captures the `VisualLayerPlan` for inspection and
//! drains the AccessKit tree update, which are the next-most-important
//! consumers. The blank target scene is a load-bearing TODO; resolving it
//! requires picking one of the three paths above.

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
