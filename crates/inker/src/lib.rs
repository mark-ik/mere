//! # Inker
//!
//! Modular engine/renderer controller for the
//! [`mere`](https://crates.io/crates/mere) browser — selects and orchestrates
//! content engines (Wry system webview, Serval,
//! [`nematic`](https://crates.io/crates/nematic) smolweb, file/media viewers).
//!
//! In the printing-press metaphor that organizes Mere's architecture, the
//! Inker pairs each engine to its content and applies the engine's "ink" to
//! the [`platen`](https://crates.io/crates/platen) press. Routing URI schemes
//! to engines, lifecycle management, and engine-output piping all live here.
//! (*Verso* names the engine-flip / compatibility-view seam — see
//! `design_docs/verso_docs/` — not a pipeline stage below platen.)
//!
//! ## Status
//!
//! Pre-1.0. This 0.0.x release reserves the crate name and documents intent;
//! implementation is in progress within the
//! [Mere workspace](https://crates.io/crates/mere).

#![doc(html_root_url = "https://docs.rs/inker/0.0.1")]

/// Accessibility capability contract (R0 invariant — every surface declares
/// what it can expose to the a11y tree; degradation is declared, never silent).
pub mod a11y;

/// Host-neutral engine routing contracts.
pub mod routing;

/// Portable document model — what engines produce.
pub mod document;

/// Engine trait and registry.
pub mod engine;

/// Surface-engine traits and registry — parallel dispatch path for
/// long-lived, frame-streaming engines (e.g. `scrying.web`).
pub mod surface_engine;

/// Content-type sniffing for unlabelled byte streams.
pub mod sniff;

/// Statements-over-schema ingest — knot `rel` links → kernel `Semantic` edges.
pub mod statements;

pub use a11y::A11yCapability;
pub use document::{
    BlockProvenance, BlockProvenanceMap, DocumentBlock, DocumentDiagnostic, DocumentProvenance,
    BlockEvaluator, BlockEvaluators, DocumentTrustState, EngineDocument, EvalOutcome, EvalOutput,
    EvaluationPolicy, Fetched, GophermapContext, InlineSpan, ResolvedProvenance, TableAlignment,
    TranscludeOutcome,
    TransclusionPolicy, evaluate_blocks, inline_text, parse_eval, parse_include,
    resolve_transclusions,
};
pub use engine::{Engine, EngineError, EngineInput, EngineRegistry};
pub use routing::{
    EngineRouteDecision, EngineRoutePolicy, EngineRouteRequest, EngineRouteRule, SurfaceContract,
    SurfaceContractMode, SurfaceTargetId, WorkspaceRouteId,
};
pub use sniff::sniff_content_type;
pub use statements::{
    LinkStatement, StatementOutcome, apply_link_statements, link_statements, resolve_rel,
};
pub use surface_engine::{
    CursorShape, EngineProfileBinding, FocusReason, KeyboardEvent, KeyboardModifiers, MouseButton,
    MouseEvent, MouseEventKind, NativeTextureHandle, NavigationEvent, PhysicalPosition,
    PointerEvent, SurfaceEngine, SurfaceEngineRegistry, SurfaceError, SurfaceFrame,
    SurfaceProducer, SurfaceSettings, SurfaceSpawnRequest, SurfaceSyncHandle, WebMessage,
};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
