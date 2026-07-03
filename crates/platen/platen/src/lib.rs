//! # Platen
//!
//! Workbench composition surface for the
//! [`mere`](https://crates.io/crates/mere) browser — arranges canvas
//! "swatches" into frames and panes, presenting the composed arrangement
//! for the host to render (`platen-view` emits it as flex DOM that serval
//! lays out).
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
//! - **Rendering** — that's the host's job: `platen-view` flex DOM through
//!   serval's layout, presented by netrender.
//! - **A11y projection** — that's mere-domain (`frame`, `gloss`, `apparatus`)
//!   → uxtree; the orrery's a11y is host-side now (meerkat `orrery_a11y_tree`).
//!
//! ## Status
//!
//! Pre-1.0. Workbench arrangement vocabulary in place; canvas-scene
//! wrapping for `graph-canvas` is implemented. `document-canvas` is
//! planned (deferred until the gpui host's bespoke layout starts hurting;
//! see `mere_docs/research/2026-05-09_netrender_for_engine_documents_brief.md`).

#![doc(html_root_url = "https://docs.rs/platen/0.0.1")]

/// Cartography projection-request derivation: builds a
/// [`cartography::request::ProjectionRequest`] and dispatches it through a
/// chosen [`cartography::LayoutStrategy`]. The integration seam for
/// per-pane layout-strategy choice (the `arrangements` catalog: radial,
/// phyllotaxis, penrose, kanban, etc.) once a picker wires it up.
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

/// The tiled-workbench model: slots of tab-stacks over a forme [`forme::Arrangement`],
/// the active tab per stack, and the projection mode. platen's canonical tiling state
/// (it replaces the legacy `FrameState` / `PaneBinding` frame model), projected to
/// side-by-side placed slots via [`tree_projection`] (concrete rects come from
/// `platen-view`'s flex DOM under serval).
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
pub mod projection_geometry;

pub use cartography_scene::{
    CartographySceneOptions, ORRERY_LAYOUT_STRATEGIES, build_projection_request,
    project_orrery_lens, project_orrery_strategy, project_orrery_subgraph, project_with,
    signal_overlays,
};
pub use document_scene::build_document_scene;
pub use projection_geometry::{Axis, CartographyGeometry, TreeBranch, TreeGeometry};
pub use tree_projection::{PlanSlot, ProjectionKind, TilePlan, WorkbenchPlan, project_tree};
pub use workbench::{SlotView, Workbench};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
