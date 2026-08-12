//! The append-only log.

use serde::{Deserialize, Serialize};

use crate::fork::{LogId, Provenance};
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
    /// This log's stable identity, if it has one. Set by [`with_id`](Codicil::with_id)
    /// or [`fork`](Codicil::fork); `None` for an anonymous log.
    #[serde(default)]
    id: Option<LogId>,
    /// Where this log forked from, if it is a fork.
    #[serde(default)]
    provenance: Option<Provenance>,
    entries: Vec<T>,
    /// Causes, parallel to `entries` and indexed by [`Seq`].
    ///
    /// **Stored as a sequence, shaped as a graph.** Entries stay a flat `Vec`
    /// so append, replay, and persistence are unchanged; the causal structure
    /// rides alongside for anything that wants to ask about it. See
    /// [`crate::causal`] for why that is enough, and for the invariant that
    /// makes the stored order a topological one for free.
    ///
    /// Defaulted, so every log written before this existed still loads with no
    /// causes claimed. A log that records none keeps this empty, so it costs a
    /// length prefix and nothing more.
    ///
    /// **Not skipped when empty.** muniment's codec is pluggable and postcard
    /// is positional, so a field written conditionally cannot be read back:
    /// a causeless log would serialize three fields and fail to decode four.
    #[serde(default)]
    parents: Vec<Vec<Seq>>,
}

impl<T> Default for Codicil<T> {
    fn default() -> Self {
        Self {
            id: None,
            provenance: None,
            entries: Vec::new(),
            parents: Vec::new(),
        }
    }
}

impl<T> Codicil<T> {
    /// A fresh, empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fresh, empty log with a stable identity.
    pub fn with_id(id: LogId) -> Self {
        Self {
            id: Some(id),
            ..Self::default()
        }
    }

    /// The causes recorded for an entry, or nothing.
    pub(crate) fn parents_of(&self, seq: Seq) -> &[Seq] {
        self.parents
            .get(seq.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Records causes for an already-appended entry.
    pub(crate) fn set_parents(&mut self, seq: Seq, causes: Vec<Seq>) {
        if causes.is_empty() {
            return;
        }
        if self.parents.len() <= seq.index() {
            self.parents.resize_with(seq.index() + 1, Vec::new);
        }
        self.parents[seq.index()] = causes;
    }

    /// This log's identity, if any.
    pub fn id(&self) -> Option<&LogId> {
        self.id.as_ref()
    }

    /// Where this log forked from, if it is a fork.
    pub fn provenance(&self) -> Option<&Provenance> {
        self.provenance.as_ref()
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
    pub fn replay<S, F>(&self, init: S, f: F) -> S
    where
        F: FnMut(S, &T) -> S,
    {
        self.entries.iter().fold(init, f)
    }

    /// Fold entries from `seq` onward into an existing state. The incremental
    /// twin of [`replay`], for advancing a materialized state by only the new
    /// entries.
    pub fn replay_from<S, F>(&self, seq: Seq, init: S, f: F) -> S
    where
        F: FnMut(S, &T) -> S,
    {
        self.from(seq).iter().fold(init, f)
    }
}

impl<T: Clone> Codicil<T> {
    /// Fork this log under a new identity. The fork copies the current entries and
    /// records where it branched from (the source's id and the seq at the fork
    /// point), then diverges independently. Duplication across the fork is tracked
    /// by this provenance, never by deduplication.
    pub fn fork(&self, new_id: LogId) -> Self {
        Self {
            id: Some(new_id),
            provenance: Some(Provenance {
                source: self.id.clone(),
                at: self.next_seq(),
            }),
            entries: self.entries.clone(),
            // Causality forks with the entries. A fork inherits the graph it
            // branched from, then diverges; seqs stay valid because the copied
            // prefix keeps its positions.
            parents: self.parents.clone(),
        }
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
    fn fork_copies_history_and_records_provenance() {
        let mut source = Codicil::with_id(LogId::new("source"));
        source.append("a");
        source.append("b");

        let mut fork = source.fork(LogId::new("fork"));
        // The fork carries the source's history so far.
        assert_eq!(fork.entries(), source.entries());
        // And records where it branched from.
        let provenance = fork.provenance().expect("a fork has provenance");
        assert_eq!(provenance.source, Some(LogId::new("source")));
        assert_eq!(provenance.at, Seq(2), "forked at the source's length");
        assert_eq!(fork.id(), Some(&LogId::new("fork")));

        // Diverging the fork does not touch the source.
        fork.append("c");
        assert_eq!(source.len(), 2, "the source is unchanged");
        assert_eq!(fork.len(), 3);
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
        assert_eq!(
            advanced, 14,
            "only the entries after the cursor were applied"
        );
    }
}
