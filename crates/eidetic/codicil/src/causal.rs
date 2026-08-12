//! Causality inside a log: stored as a sequence, shaped as a graph.
//!
//! A log is usually a list because a list is the cheapest **total order**, not
//! because order requires a list. What replay actually needs is a
//! *deterministic* order, and a directed acyclic graph plus a tiebreak rule
//! supplies that just as well while being honest about events that genuinely
//! had no order between them.
//!
//! Git is the existence proof: append-only, content-addressed, multi-parent,
//! and nobody finds it exotic. CRDT causal histories are the same shape.
//! p2panda takes a third road, one linear chain per author with the concurrency
//! living *between* logs rather than inside one.
//!
//! # The insight this module is built on
//!
//! > **Store it as a sequence, shape it as a graph.**
//!
//! Entries stay in a `Vec` and keep their [`Seq`]. Causality is carried by
//! parent links beside them. So append stays O(1), replay stays a loop over a
//! slice, persistence is unchanged, and the causal structure is *also* there
//! for anything that wants to ask about it. Git does exactly this on disk:
//! objects in a flat store, the graph living in the pointers.
//!
//! # The property that makes it free
//!
//! An entry may only cite parents that already exist, which
//! [`Codicil::append_caused_by`] enforces rather than assumes. So a parent's
//! `Seq` is always lower than its child's, which means:
//!
//! > **The stored sequence is always a valid topological order of the graph.**
//!
//! Replaying in sequence order can therefore never apply an effect before its
//! cause, with no sorting step and no cycle check at read time. Cycles are not
//! detected because they are not representable.
//!
//! # What it costs
//!
//! Asymmetric, and worth knowing before you design a query around it. Walking
//! *backwards* to causes is cheap: follow parent links, bounded by the path.
//! Walking *forwards* to effects is a scan, because the log stores parents and
//! not children. Storing both would double the edges and add a redundant field
//! to every persisted log, so effects pay a linear pass instead.
//!
//! # Ordinary appends stay ordinary
//!
//! [`Codicil::append`] records no parents. That is not a chain by omission, it
//! is an honest absence: a log that never claimed causality does not acquire
//! any by being loaded into a newer build. Existing logs keep working, and
//! [`effects`](Codicil::effects) on them correctly returns nothing.

use std::collections::{BTreeSet, VecDeque};

use crate::log::Codicil;
use crate::seq::Seq;

/// Why an entry could not be appended with the causes it named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CausalError {
    /// A named parent does not exist yet.
    ///
    /// Refused rather than tolerated: an entry that could cite the future
    /// would let the stored sequence stop being a topological order, and the
    /// cheapness of this whole design rests on that never happening.
    UnknownParent(Seq),
}

impl<T> Codicil<T> {
    /// Appends an entry that was caused by existing ones.
    ///
    /// Parents are deduplicated and stored in ascending order, so two appends
    /// naming the same causes in different orders produce identical logs. That
    /// matters for replay equality and for hashing.
    pub fn append_caused_by(
        &mut self,
        causes: impl IntoIterator<Item = Seq>,
        entry: T,
    ) -> Result<Seq, CausalError> {
        let next = self.next_seq();
        let mut parents: BTreeSet<Seq> = BTreeSet::new();
        for cause in causes {
            if cause.index() >= next.index() {
                return Err(CausalError::UnknownParent(cause));
            }
            parents.insert(cause);
        }

        let seq = self.append(entry);
        self.set_parents(seq, parents.into_iter().collect());
        Ok(seq)
    }

    /// The entries this one names as its causes, ascending.
    pub fn parents(&self, seq: Seq) -> &[Seq] {
        self.parents_of(seq)
    }

    /// Entries that name no cause.
    ///
    /// Includes every entry of a log written before causality existed, which
    /// is correct: they claimed nothing.
    pub fn roots(&self) -> impl Iterator<Item = Seq> + '_ {
        (0..self.len())
            .map(|i| Seq(i as u64))
            .filter(|s| self.parents(*s).is_empty())
    }

    /// Everything that led to `seq`, nearest first, excluding `seq` itself.
    ///
    /// Cheap: this walks parent links and is bounded by the reachable set
    /// rather than by the log.
    pub fn causes(&self, seq: Seq) -> Vec<Seq> {
        let mut seen = BTreeSet::new();
        let mut queue: VecDeque<Seq> = self.parents(seq).iter().copied().collect();
        let mut order = Vec::new();

        while let Some(current) = queue.pop_front() {
            if !seen.insert(current) {
                continue;
            }
            order.push(current);
            queue.extend(self.parents(current).iter().copied());
        }
        order
    }

    /// Everything that followed from `seq`, ascending, excluding `seq` itself.
    ///
    /// **This is the query significance is made of**: an event matters because
    /// of what later depended on it. A flat log cannot answer it at all; here
    /// it costs one forward pass, because entries store their parents and not
    /// their children.
    pub fn effects(&self, seq: Seq) -> Vec<Seq> {
        let mut reached = BTreeSet::new();
        reached.insert(seq);

        // One ascending pass is enough. A parent always has a lower `Seq` than
        // its child, so by the time this reaches an entry, every entry that
        // could have caused it has already been decided.
        for index in seq.index() + 1..self.len() {
            let at = Seq(index as u64);
            if self.parents(at).iter().any(|p| reached.contains(p)) {
                reached.insert(at);
            }
        }

        reached.remove(&seq);
        reached.into_iter().collect()
    }

    /// Whether neither entry led to the other.
    ///
    /// The question a linear log cannot ask. Two organisms feeding in the same
    /// tick are concurrent, and a list would have to invent an order between
    /// them and then pretend it meant something.
    pub fn concurrent(&self, a: Seq, b: Seq) -> bool {
        a != b && !self.causes(b).contains(&a) && !self.causes(a).contains(&b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Codicil<&'static str> {
        let mut log = Codicil::new();
        let a = log.append("a");
        let b = log.append_caused_by([a], "b").unwrap();
        log.append_caused_by([b], "c").unwrap();
        log
    }

    #[test]
    fn an_ordinary_append_claims_no_cause() {
        // A log that never recorded causality does not acquire any by being
        // loaded into a newer build.
        let mut log: Codicil<u8> = Codicil::new();
        let a = log.append(1);
        let b = log.append(2);

        assert!(log.parents(a).is_empty());
        assert!(log.parents(b).is_empty());
        assert_eq!(log.roots().collect::<Vec<_>>(), vec![a, b]);
        assert!(
            log.effects(a).is_empty(),
            "nothing claimed to follow from it"
        );
    }

    #[test]
    fn causes_walk_backwards_and_effects_walk_forwards() {
        let log = chain();
        assert_eq!(log.causes(Seq(2)), vec![Seq(1), Seq(0)], "nearest first");
        assert_eq!(log.effects(Seq(0)), vec![Seq(1), Seq(2)], "ascending");
        assert!(log.causes(Seq(0)).is_empty());
        assert!(log.effects(Seq(2)).is_empty());
    }

    #[test]
    fn an_entry_may_have_several_causes() {
        // The thing a list cannot represent: a confluence.
        let mut log = Codicil::new();
        let a = log.append("a");
        let b = log.append("b");
        let merged = log.append_caused_by([a, b], "both").unwrap();

        assert_eq!(log.parents(merged), &[a, b]);
        assert_eq!(log.effects(a), vec![merged]);
        assert_eq!(log.effects(b), vec![merged]);
    }

    #[test]
    fn independent_entries_are_concurrent() {
        let mut log = Codicil::new();
        let a = log.append("a");
        let b = log.append("b");
        let after_a = log.append_caused_by([a], "after a").unwrap();

        assert!(log.concurrent(a, b), "neither led to the other");
        assert!(log.concurrent(after_a, b));
        assert!(!log.concurrent(a, after_a), "one led to the other");
        assert!(
            !log.concurrent(a, a),
            "an entry is not concurrent with itself"
        );
    }

    #[test]
    fn the_sequence_is_always_a_topological_order() {
        // The property the whole design rests on, and the reason replay needs
        // no sorting step: a parent is always stored before its child.
        let mut log = Codicil::new();
        let a = log.append("a");
        let b = log.append_caused_by([a], "b").unwrap();
        let c = log.append_caused_by([a, b], "c").unwrap();

        for entry in [a, b, c] {
            for parent in log.parents(entry) {
                assert!(
                    parent.index() < entry.index(),
                    "{parent:?} precedes {entry:?}"
                );
            }
        }
    }

    #[test]
    fn an_entry_cannot_cite_the_future() {
        // Refused rather than tolerated. If this were allowed the sequence
        // would stop being a topological order and cycles would become
        // representable, which is the one thing this design must not permit.
        let mut log: Codicil<u8> = Codicil::new();
        log.append(1);

        assert_eq!(
            log.append_caused_by([Seq(7)], 2),
            Err(CausalError::UnknownParent(Seq(7)))
        );
        assert_eq!(log.len(), 1, "and nothing was appended");
    }

    #[test]
    fn an_entry_cannot_cite_itself() {
        // The degenerate cycle, caught by the same rule: the entry being
        // appended does not exist yet.
        let mut log: Codicil<u8> = Codicil::new();
        assert_eq!(
            log.append_caused_by([Seq(0)], 1),
            Err(CausalError::UnknownParent(Seq(0)))
        );
    }

    #[test]
    fn repeated_causes_collapse() {
        // Two appends naming the same causes in different orders must produce
        // identical logs, or replay equality and hashing stop working.
        let mut left = Codicil::new();
        let a = left.append("a");
        let b = left.append("b");
        left.append_caused_by([a, b, a], "c").unwrap();

        let mut right = Codicil::new();
        right.append("a");
        right.append("b");
        right.append_caused_by([b, a], "c").unwrap();

        assert_eq!(left, right);
    }

    #[test]
    fn replaying_in_sequence_order_never_precedes_a_cause() {
        // The practical payoff: an ordinary replay is causally valid without
        // anybody sorting anything.
        let log = chain();
        let mut applied: Vec<Seq> = Vec::new();

        for index in 0..log.len() {
            let at = Seq(index as u64);
            for parent in log.parents(at) {
                assert!(
                    applied.contains(parent),
                    "{parent:?} was applied before {at:?}"
                );
            }
            applied.push(at);
        }
    }
}

#[cfg(test)]
mod compatibility {
    use super::*;

    /// A log serialized before causality existed, byte for byte.
    const OLD: &str = r#"{"id":null,"provenance":null,"entries":["a","b"]}"#;

    /// The same log as this build writes it. The added field is compatible in
    /// both directions for a self-describing codec: absent reads as empty, and
    /// a reader that predates it ignores what it does not know.
    const NOW: &str = r#"{"id":null,"provenance":null,"entries":["a","b"],"parents":[]}"#;

    #[test]
    fn a_log_written_before_causality_still_loads() {
        // Four crates persist these through muniment slots. A field that could
        // not be absent would have broken every stored log in the stack.
        let log: Codicil<String> = serde_json::from_str(OLD).expect("old logs load");

        assert_eq!(log.len(), 2);
        assert!(log.parents(Seq(0)).is_empty());
        assert!(log.parents(Seq(1)).is_empty());
        assert_eq!(log.roots().collect::<Vec<_>>(), vec![Seq(0), Seq(1)]);
    }

    #[test]
    fn a_log_that_claims_no_causes_stays_small() {
        // Adding the field must not bloat logs that do not use it. An empty
        // vec is a length prefix, which is the honest floor.
        let mut log: Codicil<String> = Codicil::new();
        log.append("a".into());
        log.append("b".into());

        assert_eq!(serde_json::to_string(&log).unwrap(), NOW);
    }

    #[test]
    fn a_causeless_log_survives_a_positional_codec() {
        // The bug this replaces. `skip_serializing_if` writes three fields and
        // reads four, which a self-describing codec forgives and postcard does
        // not. muniment's codec is pluggable, so this was live for anyone
        // persisting a causeless log as postcard.
        let mut log: Codicil<String> = Codicil::new();
        log.append("a".into());

        let bytes = postcard::to_allocvec(&log).expect("encodes");
        let back: Codicil<String> = postcard::from_bytes(&bytes).expect("and decodes");
        assert_eq!(back, log);
    }

    #[test]
    fn causality_survives_a_round_trip() {
        let mut log: Codicil<String> = Codicil::new();
        let a = log.append("a".into());
        log.append_caused_by([a], "b".into()).unwrap();

        let text = serde_json::to_string(&log).unwrap();
        let back: Codicil<String> = serde_json::from_str(&text).unwrap();

        assert_eq!(back, log);
        assert_eq!(back.parents(Seq(1)), &[a]);
    }
}
