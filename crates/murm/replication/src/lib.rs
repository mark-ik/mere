//! Shared replicated-space mechanics for the Murm peer-exchange family.
//!
//! This crate owns the reusable p2panda receive drain and its join ceremony,
//! the muniment-backed operation store, and the policy-before-insert
//! processor. Direct exchange, Moot, mesh, and other domains keep their
//! operation grammar, authorization, and deterministic materialization.

#![doc(html_root_url = "https://docs.rs/murm-replication/0.0.1")]

mod authority;
pub mod drop;
mod drop_io;
mod joined_space;
mod processor;
#[cfg(test)]
mod prune_proof;
mod receipt;
mod store;
mod synced_space;

pub use authority::CheckpointAuthority;
pub use drop::{
    DropId, DropLimits, DropManifest, DropProtector, DropReadReport, DropRecord, DropWriteReceipt,
    EvidenceKind, ManifestEntry, NativeDropError, read_plain_drop, read_protected_drop,
    visit_plain_drop, visit_protected_drop, write_plain_drop, write_protected_drop,
};
pub use drop_io::{
    DropExportBudget, DropExportDecision, DropExportProfile, DropExportSelector, DropExportStats,
    DropFileExportReport, DropImportReport, DropIoError, StagedDrop, decode_operation_record,
    discard_peer_drop_receipts, discard_staged_drop, export_plain_topic_file,
    export_protected_topic_file, export_selected_plain_topic_file, export_topic_operations,
    export_topic_operations_selected, import_drop_records, import_plain_drop,
    import_plain_drop_file, import_protected_drop, import_protected_drop_file, list_staged_drops,
    local_drop_receipt, operation_record, peer_drop_receipt, resume_staged_drop,
    store_peer_drop_receipt,
};
pub use joined_space::{JoinError, JoinedSpace};
pub use processor::{
    Admission, HistoryAction, OperationPolicy, OperationProcessor, ProcessError, ProcessOutcome,
    Reject, StoreTarget,
};
pub use receipt::{
    DropReceipt, DropReceiptError, DropReceiptLimits, ReceiptPeer, read_drop_receipt,
    write_drop_receipt,
};
pub use store::{BlobGcReport, MunimentStore};
pub use synced_space::{SyncRound, SyncStatus, SyncedSpace};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
