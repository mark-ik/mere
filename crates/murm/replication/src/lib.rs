//! Shared replicated-space mechanics for the Murm peer-exchange family.
//!
//! This crate owns the reusable p2panda receive drain, the muniment-backed
//! operation store, and the policy-before-insert processor. Direct exchange,
//! Moot, mesh, and other domains keep their operation grammar, authorization,
//! and deterministic materialization.

#![doc(html_root_url = "https://docs.rs/murm-replication/0.0.1")]

mod authority;
pub mod drop;
mod drop_io;
mod processor;
#[cfg(test)]
mod prune_proof;
mod store;
mod synced_space;

pub use authority::CheckpointAuthority;
pub use drop::{
    DropId, DropLimits, DropManifest, DropProtector, DropReadReport, DropRecord, DropWriteReceipt,
    EvidenceKind, ManifestEntry, NativeDropError, read_plain_drop, read_protected_drop,
    visit_plain_drop, visit_protected_drop, write_plain_drop, write_protected_drop,
};
pub use drop_io::{
    DropExportProfile, DropImportReport, DropIoError, decode_operation_record,
    export_topic_operations, import_plain_drop, import_protected_drop, operation_record,
};
pub use processor::{
    Admission, HistoryAction, OperationPolicy, OperationProcessor, ProcessError, ProcessOutcome,
    Reject, StoreTarget,
};
pub use store::{BlobGcReport, MunimentStore};
pub use synced_space::{SyncRound, SyncStatus, SyncedSpace};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
