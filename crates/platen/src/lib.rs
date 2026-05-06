//! # Platen
//!
//! Graph-aware composition surface for the
//! [`mere`](https://crates.io/crates/mere) browser — composes node layout from
//! the graph store, presenting it for
//! [`verso-tile`](https://crates.io/crates/verso-tile) to render.
//!
//! In the printing-press metaphor: the platen is the press that pushes the
//! inked forme onto the verso to produce the impression. Here it is the
//! layer that knows graph semantics (where does this node go, how does it
//! relate, what's the layout?) and presses that knowledge into renderable form
//! for the rendering-surface layer to receive.
//!
//! ## Status
//!
//! Pre-1.0. This 0.0.x release reserves the crate name and documents intent;
//! implementation is in progress within the
//! [Mere workspace](https://crates.io/crates/mere).

#![doc(html_root_url = "https://docs.rs/platen/0.0.1")]

/// Graph-aware scene derivation for canvas-facing composition.
pub mod canvas_scene;

/// Portable workbench/frame model and selectors.
pub mod workbench;

pub use canvas_scene::{CanvasSceneOptions, build_canvas_scene_input, graph_view_id_to_canvas};
pub use workbench::{
    FrameId, FrameState, PaneBinding, select_active_frame, select_active_root_view,
};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
