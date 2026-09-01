// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! eidetic-search — lexical recall over your own trail (Phase 9, producer half).
//!
//! The [`TrailIndex`] is a tantivy index minted **from** `BrowsingTrace`
//! engrams: derived state, never the source of truth. The trace corpus in
//! the eidetic store is the authority; the index can always be re-minted
//! from it ([`TrailIndex::rebuild`]), which is exactly what happens when the
//! on-disk index's format no longer matches the linked tantivy
//! ([`SearchError::FormatMismatch`] — tantivy does not promise index
//! compatibility across releases, so the format version travels with the
//! index and recipients reject-or-re-mint, the workspace-pins doctrine
//! applied to a wire format).
//!
//! Three surfaces:
//!
//! - **Recall** — [`TrailIndex::search`]: BM25 over tokenized titles, page
//!   text, and URL components; a single absolute URL takes an exact canonical
//!   URL path. "Where did I read about X?"
//! - **Reports** — [`TrailIndex::top_domains`] /
//!   [`TrailIndex::visits_histogram`] over the reserved fast-field columns
//!   (domain / owner / time / transition are columnar from day one, so
//!   reports need no re-index).
//! - **Fusion** — [`fuse`]: the engine-agnostic seam that merges this
//!   crate's lexical ranking with a vector ranking (`intel/embed` or any
//!   other) by reciprocal-rank fusion. This crate deliberately does not
//!   depend on an embedding engine; the caller brings both rankings.
//!
//! The `SearchIndexSpec` engram ([`spec`]) is the hand-off contract: it
//! names the field set, tokenizer, and tantivy format version. Locally it
//! also rides a sidecar file in the index directory so `open` can check
//! compatibility before tantivy touches the segments; as an engram it is
//! what a moot's consume half (deferred) would verify before merging.

pub mod fusion;
pub mod index;
pub mod spec;

pub use fusion::{FusedHit, fuse};
pub use index::{Hit, TrailIndex};
pub use spec::{SEARCH_INDEX_SCHEMA_REF, SPEC_SIDECAR, SearchIndexSpec, bootstrap_search_schema};

/// Search-lane failures.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// The on-disk index was written by a different tantivy format than this
    /// build links. Re-mint from traces ([`TrailIndex::rebuild`]) — the
    /// corpus is the source of truth, the index is derived.
    #[error("index format mismatch: on disk {found}, this build {current} — re-mint from traces")]
    FormatMismatch { found: String, current: String },
    /// No index (or no spec sidecar) at the path. Build one from traces.
    #[error("no trail index at {0} — build one from traces first")]
    Missing(String),
    #[error("tantivy: {0}")]
    Tantivy(String),
    #[error("search io: {0}")]
    Io(String),
}

impl From<tantivy::TantivyError> for SearchError {
    fn from(e: tantivy::TantivyError) -> Self {
        SearchError::Tantivy(e.to_string())
    }
}

impl From<std::io::Error> for SearchError {
    fn from(e: std::io::Error) -> Self {
        SearchError::Io(e.to_string())
    }
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, SearchError>;
