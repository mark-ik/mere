// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Source-time selection, distinct from projected-scene delivery revisions.
//!
//! A [`SourceTime`] owns immutable snapshots of its own truth. Its cursor is
//! deliberately opaque to projection and UI code: a journal uses a sequence,
//! while a Git-backed authority can use a commit identity. Hosts retain their
//! own labels, playback policy, and live-update subscription.

/// The selectable bounds of one source's history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceExtent<Cursor> {
    /// The oldest source snapshot this provider can reproduce.
    pub earliest: Cursor,
    /// The current source snapshot. This can advance while a host follows live.
    pub current: Cursor,
}

/// An ordered source of immutable snapshots.
///
/// This is intentionally storage- and clock-agnostic. A caller asks for a
/// compact, stable set of cursors, selects one, then projects the returned
/// snapshot with its existing arrangement. Selecting a historical cursor must
/// not mutate the provider's current truth.
pub trait SourceTime {
    /// An opaque source-owned identity for one snapshot.
    type Cursor: Clone + Eq;
    /// The source's truth at one cursor.
    type Snapshot;

    /// The earliest and current selectable cursors.
    fn source_extent(&self) -> SourceExtent<Self::Cursor>;

    /// Evenly distributed, source-ordered cursors for a bounded scrubber.
    ///
    /// `max_points == 0` may return no cursors. Providers include both extent
    /// ends when they differ and more than one point is requested.
    fn source_ticks(&self, max_points: usize) -> Vec<Self::Cursor>;

    /// Reproduce truth at exactly `cursor`, refusing stale or foreign cursors.
    fn source_snapshot(&self, cursor: &Self::Cursor) -> Option<Self::Snapshot>;
}
