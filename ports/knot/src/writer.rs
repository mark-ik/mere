//! Compatibility names for the document authority now packaged separately.

pub use knot_document::{AuthoredFile, DocumentFormat, SaveOutcome};

#[doc(hidden)]
pub(crate) use knot_document::write_if_distinct;
