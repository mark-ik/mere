//! # Graphshell
//!
//! Portable shell layer for the [`mere`](https://crates.io/crates/mere) browser
//! — host GUI integration (iced, gpui, html/css, or other) and Navigator surface
//! (the single configurable-scope surface where browsing happens).
//!
//! Graphshell owns the workbench, the tile tree, the Navigator, and the
//! interface contracts to whichever GUI framework hosts the app on a given
//! platform. It is intentionally framework-agnostic so Mere can ship across
//! native desktop, browser-extension, browser-tab/PWA, and mobile envelopes
//! with the same shell semantics.
//!
//! ## Status
//!
//! Pre-1.0. The first migrated surface is the portable graph authority:
//! owner-scoped node lineage and the framework-agnostic forme (per-graph-view
//! workbench arrangement authority). Host adapters, GUI integration, and
//! render bridge code move later.

#![doc(html_root_url = "https://docs.rs/graphshell/0.0.1")]

/// Portable reducer-owned app-state and service-boundary contracts.
pub mod app_state;

/// Portable identity, authority, and mutation kernel migrated from the
/// previous Graphshell tree.
pub use mere_kernel as core;

/// Portable runtime-boundary vocabulary migrated from the previous Graphshell
/// tree.
pub use mere_host_contract as runtime;

/// Owner-scoped navigation-lineage model migrated from the previous Graphshell
/// tree. Originally `graph_memory`; renamed because eidetic owns the "memory"
/// layer and this crate is really navigation lineage.
pub use node_lineage as lineage;

/// Per-graph-view workbench arrangement authority — projects graph members +
/// edges into the workbench's tile arrangement (which may or may not be
/// tree-shaped). Originally `graph_tree`; renamed because "tree" undersold
/// the role.
pub use forme;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
