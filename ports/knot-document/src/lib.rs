// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Djot-first document authority and reusable Cambium presentation for Knot.
//!
//! The default dependency graph owns a single [`cambium::TextInput`] and native file
//! writes. Parsing, preview, and conversion are opt-in through [`engine`].

mod document_surface;
mod document_view;
mod editor;
mod writer;

pub use document_surface::{
    KnotDocumentIntentErrorV1, KnotDocumentIntentV1, KnotDocumentRefusalV1,
    KnotDocumentSaveFailureV1, KnotDocumentSaveOutcomeV1, KnotDocumentSession,
    KnotDocumentSnapshotV1, KnotDocumentSourceKindV1, KnotDocumentSourceV1,
    KnotDocumentWritePostureV1,
};
pub use document_view::{
    KNOT_DOCUMENT_CSS, KnotDocumentSurfaceState, KnotDocumentView, knot_document_descriptor,
    knot_document_surface, knot_document_view,
};
pub use editor::{EditOutcome, KnotEditor};
#[cfg(feature = "engine")]
pub use writer::AuthoredFile;
#[doc(hidden)]
pub use writer::write_if_distinct;
pub use writer::{DocumentFormat, SaveOutcome};
