/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The append-only log.

use serde::{Deserialize, Serialize};

use crate::seq::Seq;

/// A linear, append-only log of immutable entries.
///
/// Entries are appended and never edited or removed. Each is stamped with a
/// monotonic [`Seq`]. To recover the state the entries describe, [`replay`] folds
/// them oldest-first. This is the event-source and nondestructive-history
/// primitive: isometry session events, strophe edit history, a knowledge graph's
/// mutations.
///
/// Generic over the entry type `T`, so the log carries whatever an app's edits
/// are (a board event, a graph mutation, a text edit). This founding cut is
/// linear; a branching edit-tree (undo/redo) is a shape a consumer can layer on,
/// or a later extension.
///
/// `Codicil<T>` is `Serialize`/`Deserialize` when `T` is, so it persists whole
/// through a [`muniment`] slot (see the crate's persistence methods).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Codicil<T> {
    entries: Vec<T>,
}

impl<T> Default for Codicil<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T> Codicil<T> {
    /// A fresh, empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `entry`, returning the [`Seq`] it was stamped with.
    pub fn append(&mut self, entry: T) -> Seq {
        let seq = Seq(self.entries.len() as u64);
        self.entries.push(entry);
        seq
    }

    /// The entry at `seq`, or `None` if the log is shorter.
    pub fn get(&self, seq: Seq) -> Option<&T> {
        self.entries.get(seq.index())
    }

    /// Every entry, oldest first.
    pub fn entries(&self) -> &[T] {
        &self.entries
    }

    /// Entries from `seq` onward (inclusive). A reader holding everything before
    /// `next_seq` calls `from(next_seq)` to get only what is new, which is the
    /// incremental-catch-up path for replication and re-render.
    pub fn from(&self, seq: Seq) -> &[T] {
        let start = seq.index().min(self.entries.len());
        &self.entries[start..]
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The [`Seq`] the next appended entry will receive.
    pub fn next_seq(&self) -> Seq {
        Seq(self.entries.len() as u64)
    }

    /// Fold every entry into a state, oldest first, to reconstruct what the log
    /// describes.
    pub fn replay<S, F>(&self, init: S, mut f: F) -> S
    where
        F: FnMut(S, &T) -> S,
    {
        self.entries.iter().fold(init, |state, entry| f(state, entry))
    }

    /// Fold entries from `seq` onward into an existing state. The incremental
    /// twin of [`replay`], for advancing a materialized state by only the new
    /// entries.
    pub fn replay_from<S, F>(&self, seq: Seq, init: S, mut f: F) -> S
    where
        F: FnMut(S, &T) -> S,
    {
        self.from(seq).iter().fold(init, |state, entry| f(state, entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_stamps_monotonic_sequences() {
        let mut log = Codicil::new();
        assert_eq!(log.append("a"), Seq(0));
        assert_eq!(log.append("b"), Seq(1));
        assert_eq!(log.append("c"), Seq(2));
        assert_eq!(log.len(), 3);
        assert_eq!(log.next_seq(), Seq(3));
    }

    #[test]
    fn get_addresses_stable_entries() {
        let mut log = Codicil::new();
        log.append(10);
        log.append(20);
        assert_eq!(log.get(Seq(0)), Some(&10));
        assert_eq!(log.get(Seq(1)), Some(&20));
        assert_eq!(log.get(Seq(2)), None);
    }

    #[test]
    fn from_returns_only_newer_entries() {
        let mut log = Codicil::new();
        for n in 0..5 {
            log.append(n);
        }
        assert_eq!(log.from(Seq(3)), &[3, 4]);
        assert_eq!(log.from(Seq(5)), &[] as &[i32]);
        // Out-of-range cursors clamp rather than panic.
        assert_eq!(log.from(Seq(99)), &[] as &[i32]);
    }

    #[test]
    fn replay_folds_entries_into_state() {
        // Entries are edits; replay materializes the state they describe.
        let mut log: Codicil<i64> = Codicil::new();
        log.append(5);
        log.append(-2);
        log.append(10);
        let total = log.replay(0i64, |sum, delta| sum + delta);
        assert_eq!(total, 13);
    }

    #[test]
    fn replay_from_advances_an_existing_state() {
        let mut log: Codicil<i64> = Codicil::new();
        log.append(5);
        log.append(-2);
        let partial = log.replay(0, |s, d| s + d); // 3, holder is at next_seq = 2
        let at = log.next_seq();
        log.append(10);
        log.append(1);
        let advanced = log.replay_from(at, partial, |s, d| s + d);
        assert_eq!(advanced, 14, "only the entries after the cursor were applied");
    }
}
