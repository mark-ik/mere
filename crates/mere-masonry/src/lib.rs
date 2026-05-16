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
//! ## Two-vello + wgpu version skew (read before implementing scene-merge or texture-fill)
//!
//! This crate's dependency tree contains **two distinct copies of *both*
//! `vello` and `wgpu`**, neither pair interoperable at the type level:
//!
//! | Crate  | mere-side (this crate, workspace pin)        | xilem-side (transitive via `masonry_imaging`) |
//! | ------ | -------------------------------------------- | --------------------------------------------- |
//! | `vello`| 0.9                                          | 0.8                                           |
//! | `wgpu` | 29                                           | 28                                            |
//!
//! For Rust's type system the two copies of each crate are **different
//! crates entirely**: a `vello::Scene` or `wgpu::Texture` from one cannot
//! cross to the other. Errors look like "expected `vello::Scene`, found
//! `vello::Scene`" — the names match, the crate identities don't.
//!
//! ### Why this defeats the texture round trip
//!
//! Earlier framing suggested the wgpu texture handle would be the
//! version-neutral bridge — render with masonry's vello 0.8 path into a
//! wgpu texture, hand that texture to the host's vello 0.9 to composite.
//! That assumed wgpu was a single version. **It isn't.**
//! `masonry_imaging::texture_render::Renderer::render_to_texture` takes a
//! `RenderTarget { device: &wgpu::Device, texture: &wgpu::Texture, ... }`
//! where `wgpu::Device` is wgpu 28's type. The host's `vello::Renderer`
//! lives in wgpu 29's world. There's no automatic conversion.
//!
//! ### Resolution paths
//!
//! 1. **Patch-bump xilem's local clone** (preferred). Change xilem's
//!    workspace pins from `wgpu = "28.0.0"` + `vello = "0.8"` to
//!    `wgpu = "29.0.3"` + `vello = "0.9"` and resolve any API breaks
//!    masonry_imaging tripped on. Dissolves *both* skews; mere-masonry
//!    then runs against a single vello + single wgpu, scene-merge via
//!    `Scene::append` becomes trivial, EmbeddedFrame texture round trip
//!    becomes straightforward. Fork debt: track xilem upstream for when
//!    they do the bump, then unwind.
//! 2. **Wait for xilem upstream to bump**. Same eventual outcome as (1)
//!    without the fork debt, but blocks on linebender's schedule.
//! 3. **CPU readback**. Render with masonry's wgpu 28, read pixels back
//!    to a `Vec<u8>`, upload to a wgpu 29 texture, register with
//!    vello 0.9. Works on any platform, terrible performance, but proves
//!    the architecture works.
//! 4. **Platform-specific cross-API shared handles**. Create a DX12
//!    shared texture / Metal IOSurface / dmabuf and import it on both
//!    wgpu 28 *and* wgpu 29. This is structurally what
//!    `scrying::native_frame` does for cross-*device* sharing — adapting
//!    it to cross-wgpu-*version* sharing is platform-specific work each
//!    OS. Significant.
//!
//! Until (1) or (2) lands, [`MasonryTile::render`] and
//! [`MasonryEmbeddedRenderer::next_frame`] are both effectively no-op
//! placeholders: the trait surfaces are wired, the texture is allocated,
//! the AccessKit tree update drains correctly, but no pixels reach the
//! host scene.

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

mod embedded_renderer;
mod input;
mod renderer;
mod signal;
mod tile;

pub use embedded_renderer::{MasonryEmbeddedRenderer, RootWidgetFactory};
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
