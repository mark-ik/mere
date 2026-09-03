// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! # Platen
//!
//! The **pane home** for the [`mere`](https://crates.io/crates/mere)
//! browser: platen compiles a forme [`forme::Arrangement`] into the
//! presentation plans a host renders panes from — the tiled-layout
//! model, its tree projection, the pane geometry sidecar, and the
//! document-canvas scene input for a pane that holds a document tile.
//!
//! In the printing-press metaphor: the platen is the press that pushes the
//! inked forme onto the page to produce the impression. Here it is the
//! layer that holds the pane arrangement — *which content goes in which
//! frame pane* — and projects that arrangement into the rendering-surface
//! layer (Genet's host tile tree; `platen-view` flex DOM under Genet).
//!
//! ## Decomposed 2026-07-09
//!
//! Platen used to also carry the graph-scene paint lane (scene_paint,
//! cartography projection dispatch, coupling paint, the canvas underlay,
//! cartography geometry). That lane merged into the `canvas` crate — the
//! graph-truth presentation library that absorbed the old `orrery`
//! content-root — leaving platen the pane composition home. See the
//! mere/turnstone boundary pass plan's amendments.
//!
//! ## What platen does NOT own
//!
//! - **Within-pane content layout** — a canvas crate's job (the graph
//!   canvas knows where each node goes; document-canvas knows where each
//!   paragraph goes). Platen sees pane content as opaque renderable units.
//! - **Rendering** — that's the host's job: `platen-view` flex DOM through
//!   genet's layout, presented by netrender.
//! - **Canvas a11y** — host-side. Platen projects its own tiled-layout structure
//!   through [`accessibility`]; other domain projections remain domain-owned.

#![doc(html_root_url = "https://docs.rs/platen/0.0.1")]

/// Document-canvas scene-input derivation. Wraps `document-canvas` for
/// composition-time use. Hosts call this for any pane that holds a document
/// tile (the output of a nematic engine).
pub mod document_scene;

/// AccessKit/uxtree projection of Platen-owned structure.
pub mod accessibility;

/// The tiled-workbench model: slots of tab-stacks over a forme [`forme::Arrangement`],
/// the active tab per stack, and the projection mode. platen's canonical tiling state
/// (it replaces the legacy `FrameState` / `PaneBinding` frame model), projected to
/// side-by-side placed slots via [`tree_projection`] (concrete rects come from
/// `platen-view`'s flex DOM under genet).
pub mod workbench;

/// Tree projection — compiles a forme [`forme::Arrangement`] into a
/// [`tree_projection::WorkbenchPlan`] (splits of tab-stacks), platen's core
/// role under the composition spine. Sibling projections (cartography,
/// lattice) are added when a surface needs one.
pub mod tree_projection;

/// Projection geometry — the geometry sidecar for a forme's Tree projection
/// (split skeleton, fractions, active tab), keyed `(FormeRef, ProjectionKind)`.
/// The layout refinement over [`tree_projection::project_tree`]'s default flat
/// projection; the [`workbench`] bridge derives/rebuilds it losslessly.
/// (Cartography geometry lives with the canvas since the decomposition.)
pub mod projection_geometry;

pub use accessibility::project_tile_layout;
pub use document_scene::build_document_scene;
pub use projection_geometry::{Axis, TreeBranch, TreeGeometry};
pub use tree_projection::{PlanSlot, ProjectionKind, TilePlan, WorkbenchPlan, project_tree};
pub use workbench::{SlotView, TileLayout, Workbench};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
