//! **Distillery**, the model-works port of the Mere platform.
//!
//! A distillery takes a raw mash and runs it, batch by batch, through stills
//! into something concentrated. This port is that works for models: the
//! harness where a ring of devices runs inference, embedding, and eventually
//! training jobs. It splits in two, in the castellan mold:
//!
//! - an **embeddable half** any host app composes: the job board, lease and
//!   heartbeat tiles, the device roster, retention and device-policy panes,
//!   model-manifest browsing, a minimal streaming console. These views render
//!   what the supervisor reports, never a placebo.
//! - an **authority half** that lives with the device: the supervisor loop
//!   that owns mesh host ticks, device conditions, the resource registry, and
//!   blob custody. It answers to the owner; reclaim always wins.
//!
//! The boundaries are the point:
//!
//! - **Not esp.** The inference/embedding seam crate stays the portable burn
//!   boundary; distillery drives it.
//! - **Not mere-mesh or mere-mesh-host.** Job grammar, leases, and the
//!   supervisor are substrate; distillery embeds and renders them.
//! - **Not servitor.** Whether an admitted denizen may petition at all stays
//!   the gate's office.
//! - **Not turnstone.** The flagship embeds the same views; distillery is the
//!   standalone works.
//!
//! The trainer lane (Distillery-as-trainer, per the geist brief) lands here
//! later, behind its own plan: training is one more job the works runs.
//!
//! [`Distillery`] is the first real consumer of `mere-mesh-host`. Its D0
//! authority drives the non-blocking supervisor and owns checkpoint/collection.
//! [`ResidentAuthority`] is D1's long-lived process body: it owns persistent
//! collecting storage, the transport shutdown order, configurable tick and
//! maintenance cadence, and an ordered receipt stream. Views remain a later
//! slice; when they arrive they render those receipts rather than reconstructing
//! authority state.

#![doc(html_no_source)]
#![warn(missing_docs)]

mod authority;
mod resident;

pub use authority::{
    BlobCustody, Distillery, DistilleryError, MaintenanceReport, RetentionSettings,
};
pub use resident::{
    ResidentAuthority, ResidentError, ResidentReceipt, ResidentSettings, ResidentStorage,
};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
