//! Knot: Mere's files-in-place authoring port.
//!
//! The first slice is deliberately read-only. [`DirectorySource`] discovers
//! files without storing their bytes in graph state, while [`KnotEndpoint`]
//! discloses those containers through Graphshell. The directory remains source
//! truth.

mod content_classes;
mod directory;
mod editor;
mod endpoint;
mod search;
mod sync;
mod vault;
mod watcher;
mod writer;

pub use content_classes::{
    FILE_CLASS, FILE_DOCUMENT_FACET, KnotContentClasses, NOTE_CLASS, NOTE_DOCUMENT_FACET,
};
pub use directory::{DirectorySource, DiskDocument, IgnorePolicy};
pub use editor::{EditOutcome, KnotEditor};
pub use endpoint::KnotEndpoint;
pub use search::{KnotSearch, SearchConfig, SearchHit, SearchLane};
pub use sync::{KnotSyncError, KnotSyncEvent, KnotSyncExt, KnotSyncFileStore, KnotSyncStore};
pub use vault::{KnotVault, VaultDocument};
pub use watcher::DirectoryWatcher;
pub use writer::{AuthoredFile, DocumentFormat, SaveOutcome};
