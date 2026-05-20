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
//! ## Version alignment (post-2026-05-16 bump)
//!
//! This crate's dependency tree previously contained two distinct vello
//! versions (0.8 via xilem, 0.9 via mere) and two distinct wgpu versions
//! (28 via xilem, 29 via mere). Those skews are now **dissolved** by the
//! local fork at `repos/imaging` (branch `mere-wgpu-29-vello-0-9`) plus
//! xilem's workspace pins bumped to match mere's:
//!
//! | Crate  | mere workspace | xilem workspace (bumped) |
//! | ------ | -------------- | ------------------------ |
//! | vello  | 0.9            | 0.9                      |
//! | wgpu   | 29             | 29                       |
//! | parley | 0.9            | 0.9                      |
//!
//! With versions aligned, both halves of this crate operate in the same
//! wgpu 29 / vello 0.9 universe — [`MasonryEmbeddedRenderer::next_frame`]
//! returns a `wgpu::Texture` the host's vello directly samples through
//! `Renderer::register_texture`.
//!
//! ### Fork tracking
//!
//! The imaging fork's additions are purely additive (new `wgpu-29` +
//! `vello-0-9` features alongside the existing 27/28 + 0.7/0.8 versions)
//! and a clean PR candidate. When upstream forest-rs/imaging accepts the
//! features, xilem's workspace can drop the path-dep override and return
//! to a git-rev pin. When upstream xilem in turn bumps to vello 0.9 +
//! wgpu 29, the local xilem clone's pin overrides can be dropped too.
//!
//! ### Why the [`MasonryTile::render`] scene-merge TODO is still a TODO
//!
//! Now that the vello version skew is gone, scene-merge via
//! `Scene::append(&masonry_scene, transform)` is the trivial path it was
//! always meant to be. The TODO simply hasn't been implemented yet —
//! [`MasonryEmbeddedRenderer`] took the EmbeddedFrame path (offscreen
//! texture + host registers as image source) which works fine for the
//! current consumer set and doesn't need the in-scene paint API. The
//! in-scene path will land if/when a consumer needs to interleave
//! masonry content with host-emitted vello ops at fine grain.

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

mod embedded_renderer;
mod input;
mod panel_tile;
mod renderer;
mod signal;
mod tile;
mod xilem_panel;

pub use embedded_renderer::{MasonryEmbeddedRenderer, PanelFactory, RootWidgetFactory};
pub use panel_tile::PanelTile;
pub use signal::TileSignal;
pub use tile::{MasonryTile, TileSize};
pub use xilem_panel::{PanelLogic, XilemPanel};

// Re-export the xilem masonry binding so host-side panel logic can
// name views (label / flex_col / etc.), `AnyWidgetView`, and the
// `WidgetView::boxed` helper without a separate dep.
pub use xilem_masonry;

// Re-export the surfaces callers need so they don't have to add the matching
// dep version themselves. Every load-bearing crate is a direct dep of this
// crate (see Cargo.toml) — no transitive resolution through registry's
// re-exports.
pub use accesskit;
pub use kurbo;
pub use masonry_core;
pub use register_renderer;
pub use ui_events;
pub use vello;

// `masonry_imaging` provides the `imaging_vello_cpu` backend +
// PreparedFrame type that `MasonryEmbeddedRenderer::next_frame` uses
// directly. Re-exported so downstream callers can name the imaging
// types if they need to.
pub use masonry_imaging;
