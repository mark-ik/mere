//! Shared replicated-space mechanics for signed peer domains.
//!
//! This crate owns the reusable p2panda receive drain and its join ceremony,
//! the muniment-backed operation store, and the policy-before-insert
//! processor. Direct exchange, Moot, mesh, and other domains keep their
//! operation grammar, authorization, and deterministic materialization.
//!
//! A domain supplies an [`OperationPolicy`] and, where it prunes history, a
//! [`CheckpointAuthority`]. Stickleback never infers authority from transport
//! access or visible membership: it supplies the erasure mechanics and calls
//! the domain's gate.

#![doc(html_root_url = "https://docs.rs/stickleback/0.1.0")]

mod authority;
mod causal;
pub mod drop;
mod drop_io;
mod epoch_retention;
mod group_crypto;
mod group_session;
mod joined_space;
mod processor;
#[cfg(test)]
mod prune_proof;
mod receipt;
mod store;
mod synced_space;
mod writer;

pub use authority::CheckpointAuthority;
pub use causal::{
    CausalEntry, CausalError, CausalLimits, CausalProjection, PendingCausalOperation, author_head,
    causal_projection, happens_before, observed_frontier, validate_causal_metadata,
};
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
pub use epoch_retention::{
    EpochCheckpointBasis, EpochHold, EpochHoldReason, EpochProposalBlocker, EpochPruningProposal,
    EpochRetentionFacts, EpochRetentionReason, RetainedEpoch, propose_epoch_pruning,
};
pub use group_crypto::{
    DataKeyring, GroupCiphertext, GroupCryptoError, GroupEncryptionMode, GroupEncryptionProfile,
};
pub use group_session::{
    GroupControlAction, GroupControlFrame, GroupControlId, GroupDirectFrame, GroupPrekeyBundle,
    GroupRecipientId, GroupSession, GroupSessionDispatch, GroupSessionError, GroupSessionId,
    GroupSessionProcess,
};
pub use joined_space::{JoinError, JoinedSpace};
pub use p2panda_encryption::data_scheme::GroupSecretId;
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
pub use writer::{WriterBindingError, stable_writer_subject};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
