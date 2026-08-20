//! Receipts: scenario runs as replicated, provenance-bearing graph facts.
//!
//! A capture without provenance is an image nobody can place six months
//! later. The remote-receipt lane (`genet/scripts/remote-receipt.ps1`) binds
//! every artifact to a commit, a machine, a session type and a scenario in a
//! `manifest.json`; these modules turn that into the personal graph's own
//! vocabulary, so the binding replicates instead of sitting in an unversioned
//! directory on one laptop.
//!
//! Three pieces, in the order a receipt moves through them:
//!
//! - [`ingest`] reads a receipt directory, stores its artifacts as
//!   content-addressed blobs, and produces the graph events that record the
//!   run. Idempotent and clock-free.
//! - The **sync lane** below names what a replica must select for those events
//!   to arrive whole.
//! - [`intake`] is the resident host's side: pending event files are picked
//!   up, authored as one signed turn, and moved aside.
//!
//! The split between ingest and intake is deliberate. Ingest can run anywhere
//! (a CLI, a test, a future importer); **authoring is the resident host's
//! alone**, because it holds the signing identity and the log, and a second
//! writer for one graph is a bug waiting to happen.

pub mod card;
pub mod ingest;
pub mod intake;
pub mod manifest;

pub use card::{CARD_FACETS, is_receipt, receipt_card};
pub use ingest::{IngestedReceipt, ingest_directory, ingest_manifest};
pub use intake::{
    InboxEntry, PendingReceipt, captures_in, inbox_dir, mark_applied, pending, write_to_inbox,
};
pub use manifest::{
    ADDRESS_PREFIX, FACET_ARTIFACTS, FACET_RUN, ManifestArtifact, ReceiptError, ReceiptManifest,
};

use crate::personal_sync::SyntheticAddressRule;

/// The facets a replica must select for receipts to arrive with their
/// provenance.
///
/// Named here rather than spelled into the sync wiring, because a facet the
/// selection does not list is silently not projected: receipts would
/// replicate as bare titled nodes carrying none of the context that makes
/// them evidence.
pub fn sync_facets() -> [&'static str; 2] {
    [FACET_RUN, FACET_ARTIFACTS]
}

/// The projection rule for receipt nodes.
///
/// Receipt addresses are synthetic (a run is not a navigable location), so
/// they follow their facet the way the transfer carrier does: a device that
/// does not select [`FACET_RUN`] materializes no receipt nodes at all, rather
/// than a list of titles with nothing behind them. Not device-scoped — a
/// receipt from the ThinkPad is exactly what the laptop wants to see.
pub fn sync_address_rule() -> SyntheticAddressRule {
    SyntheticAddressRule {
        prefix: ADDRESS_PREFIX.to_string(),
        facet: FACET_RUN.to_string(),
        device_scoped: false,
    }
}
