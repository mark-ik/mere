/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Adapters that bridge cartography's strategy contracts to existing
//! per-renderer layout implementations.
//!
//! Each adapter sits behind its own feature gate so cartography
//! consumers that just want the contract types don't pull in renderer
//! crates they don't need.
//!
//! ## Why adapters live in cartography (not in the renderer crates)
//!
//! The `mere-kernel` crate has a legacy dependency on `graph-canvas`
//! (for `graph_canvas::packet::Stroke` in `OverlayStrokePass`). That
//! makes `graph-canvas → cartography → mere-kernel → graph-canvas` a
//! dependency cycle. Until the kernel's `graph-canvas` dep is broken
//! (a separate refactor), the adapter has to live somewhere that
//! depends on both cartography and graph-canvas but isn't depended on
//! by either — cartography itself, behind a feature flag, is the
//! smallest place that fits.
//!
//! Long-term, when `graph-layout` is extracted as a sibling crate
//! (per the cartography brief's revised §9 step 4), it'll absorb
//! these adapters.

#[cfg(feature = "graph-canvas-adapters")]
pub mod graph_canvas;
