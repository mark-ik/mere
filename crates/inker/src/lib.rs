//! # Inker
//!
//! Modular engine/renderer controller for the
//! [`mere`](https://crates.io/crates/mere) browser — selects and orchestrates
//! content engines (Wry system webview, Serval/Servo-wgpu,
//! [`nematic`](https://crates.io/crates/nematic) smolweb, file/media viewers).
//!
//! In the printing-press metaphor that organizes Mere's architecture, the
//! Inker pairs each engine to its content and applies the engine's "ink" to
//! the [`platen`](https://crates.io/crates/platen) for the
//! [`verso-tile`](https://crates.io/crates/verso-tile) layer to receive.
//! Routing URI schemes to engines, lifecycle management, and engine-output
//! piping all live here.
//!
//! ## Status
//!
//! Pre-1.0. This 0.0.x release reserves the crate name and documents intent;
//! implementation is in progress within the
//! [Mere workspace](https://crates.io/crates/mere).

#![doc(html_root_url = "https://docs.rs/inker/0.0.1")]

/// Host-neutral engine routing contracts.
pub mod routing;

pub use routing::{
    EngineRouteDecision, EngineRoutePolicy, EngineRouteRequest, EngineRouteRule, SurfaceContract,
    SurfaceContractMode, SurfaceTargetId, WorkspaceRouteId,
};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
