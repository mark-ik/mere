//! # Platen
//!
//! Workbench composition surface for the
//! [`mere`](https://crates.io/crates/mere) browser — arranges canvas
//! "swatches" into frames and panes, presenting the composed arrangement
//! for [`verso-core`](https://crates.io/crates/verso-core) to render.
//!
//! In the printing-press metaphor: the platen is the press that pushes the
//! inked forme onto the verso to produce the impression. Here it is the
//! layer that holds the workbench arrangement — *which canvas swatch
//! goes in which frame pane* — and projects that arrangement into the
//! rendering-surface layer.
//!
//! ## Canvas swatches
//!
//! A *canvas* is a self-contained renderable unit that owns its own
//! within-tile layout. Each kind of content has its own canvas crate:
//!
//! - [`graph-canvas`](https://crates.io/crates/graph-canvas) — graph view
//!   layout (node positions, force-direct, projection, hit testing,
//!   render-packet derivation)
//! - `document-canvas` (planned) — document view layout (parley text +
//!   block stacking; consumes [`inker::EngineDocument`] and emits
//!   [`netrender::Scene`])
//!
//! Canvases are *swatches*: self-contained visual units that can be
//! embedded anywhere in the UI. Platen composes them into the workbench;
//! knots can embed them as fenced blocks; settings panels and sidebars
//! can drop them in as widgets. Each canvas owns its own internal layout
//! and emits the same shape of render packets regardless of where it
//! ends up sitting.
//!
//! ## What platen does NOT own
//!
//! - **Within-canvas layout** — a canvas crate's job (graph-canvas knows
//!   where each node goes; document-canvas will know where each paragraph
//!   goes). Platen sees canvases as opaque renderable units.
//! - **Rendering** — that's verso-core's surface lifecycle plus the host's
//!   netrender / gpui integration.
//! - **A11y projection** — that's mere-domain (`mere-orrery`,
//!   `frame`, `gloss`, `apparatus`) → uxtree.
//!
//! ## Status
//!
//! Pre-1.0. Workbench arrangement vocabulary in place; canvas-scene
//! wrapping for `graph-canvas` is implemented. `document-canvas` is
//! planned (deferred until the gpui host's bespoke layout starts hurting;
//! see `mere_docs/research/2026-05-09_netrender_for_engine_documents_brief.md`).

#![doc(html_root_url = "https://docs.rs/platen/0.0.1")]

/// Graph-canvas scene-input derivation. Translates a reducer-owned graph
/// view into the input shape `graph-canvas` expects.
pub mod canvas_scene;

/// Cartography projection-request derivation. Sibling of
/// [`canvas_scene`] that goes through cartography's strategy contract
/// instead of building a `CanvasSceneInput` directly. Hosts use this
/// when they want per-pane layout-strategy choice (force-directed,
/// radial, phyllotaxis, cluster-collapsed, etc.) rather than
/// hard-coded force-directed.
pub mod cartography_scene;

/// Render a cartography `Projection` into a `paint_list_api` paint list — the
/// orrery's host-agnostic scene underlay, consumed by netrender regardless of
/// host. Platen is the press; this is where a projection becomes paint.
pub mod scene_paint;

/// Visual couplings → paint overlays: the paint-side consumer of the field
/// system's open response tail (the aether→platen seam, mirroring aether→gyre).
pub mod coupling_paint;

/// The orrery scene producer: graph → a painted `CanvasPaintList` underlay
/// (host-agnostic; the serval-as-host orrery element's scene-paint layer).
pub mod orrery;

/// Document-canvas scene-input derivation. Wraps `document-canvas` for
/// composition-time use; the sibling of `canvas_scene` for the document
/// swatch. Hosts call this for any pane that holds a document tile (the
/// output of a nematic engine).
pub mod document_scene;

/// Portable workbench / frame model and selectors. Defines pane bindings,
/// frame arrangement snapshots, and the projection from arrangements into
/// hosted surface placements. Independent of which canvas kind sits in any
/// given pane.
pub mod workbench;

/// Tree projection — compiles a forme [`forme::Arrangement`] into a
/// [`tree_projection::WorkbenchPlan`] (splits of tab-stacks), platen's core
/// role under the composition spine. Sibling projections (cartography,
/// lattice) are added when a surface needs one.
pub mod tree_projection;

/// Between-tiles layout — assigns each [`tree_projection::WorkbenchPlan`] slot
/// a concrete rect inside a viewport via the [morphorm](https://crates.io/crates/morphorm)
/// engine. Platen owns the arrangement *between* tiles; the host owns content
/// *within* each tile.
pub mod layout;

/// Morphorm glue: platen's [`morphorm::Node`] / [`morphorm::Cache`]
/// implementation backing [`layout`]. Mechanical; not part of the public API.
mod layout_node;

pub use canvas_scene::{
    CanvasSceneOptions, HiddenRelationKey, build_canvas_scene_input, graph_view_id_to_canvas,
};
pub use cartography_scene::{
    CartographySceneOptions, build_projection_request, project_with, step_with,
};
pub use document_scene::build_document_scene;
pub use layout::{LaidOutPlan, LaidOutSlot, LayoutConfig, layout_plan};
pub use tree_projection::{PlanSlot, ProjectionKind, TilePlan, WorkbenchPlan, project_tree};
pub use workbench::{
    ArrangementContainer, ArrangementMember, ArrangementSnapshot, FrameId, FrameState, PaneBinding,
    ProjectedPane, TileSlot, WorkbenchProjection, assign_frame_pane, assign_view_and_frame_pane,
    clear_frame_pane, project_active_workbench, project_frame, remove_pane_binding,
    remove_view_and_frame_pane, select_active_frame, select_active_root_view,
    set_binding_surface_host, set_frame_root_view, set_view_and_frame_surface_host, slot_for_index,
    snapshot_active_arrangement, snapshot_frame_arrangement, upsert_pane_binding,
};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
