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
//! owner-scoped graph memory and framework-agnostic graph-tree layout. Host
//! adapters, GUI integration, and render bridge code move later.

#![doc(html_root_url = "https://docs.rs/graphshell/0.0.1")]

/// Portable reducer-owned app-state and service-boundary contracts.
pub mod app_state;

/// Portable identity, authority, and mutation kernel migrated from the
/// previous Graphshell tree.
pub use mere_kernel as core;

/// Portable runtime-boundary vocabulary migrated from the previous Graphshell
/// tree.
pub use mere_host_contract as runtime;

/// Owner-scoped graph memory model migrated from the previous Graphshell tree.
pub use graph_memory as memory;

/// Framework-agnostic graphlet-native tile tree migrated from the previous
/// Graphshell tree.
pub use graph_tree as tree;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
