//! # Graphshell
//!
//! Portable graph + shell chrome facade for the
//! [`mere`](https://crates.io/crates/mere) browser — graph authority,
//! host-independent shell state, and Navigator-facing contracts.
//!
//! Graphshell owns graph chrome and host shell chrome. Workbench crates own
//! engine routing, arrangement, composition, and tile/surface lifecycle;
//! Graphshell composes those contracts without becoming their owner.
//!
//! ## Status
//!
//! Pre-1.0. Package names still lag the physical topology in places; prefer
//! the directory families in `design_docs/DOC_README.md` for architecture
//! discussion until the bounded crate-name pass lands.

#![doc(html_root_url = "https://docs.rs/graphshell/0.0.1")]

/// Portable reducer-owned app-state and service-boundary contracts.
pub mod app_state;

/// Portable identity, authority, and mutation kernel migrated from the
/// previous Graphshell tree.
pub use mere_kernel as core;

/// Portable host-port vocabulary migrated from the previous Graphshell tree.
pub use mere_host_contract as runtime;

/// Owner-scoped navigation-lineage model migrated from the previous Graphshell
/// tree. Originally `graph_memory`; renamed because eidetic owns the "memory"
/// layer and this crate is really navigation lineage.
pub use node_lineage as lineage;

/// Per-graph-view workbench arrangement authority re-exported for existing
/// callers. Ownership lives in `workbench/forme`.
pub use forme;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
