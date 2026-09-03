// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Cascades: how far one wake is allowed to travel.
//!
//! A woken body commits; its commits can wake other watches; those wake more.
//! That chain is a **cascade**, and left alone it is the failure mode of every
//! reactive system: two helpers that each react to the other, running until
//! something gives. This module is what gives, deliberately and out loud.
//!
//! Four properties hold it together, and only the last one is a limit:
//!
//! 1. **No self-waking**, from [`Watch`](crate::watch::Watch): the trivial
//!    one-body loop cannot even be expressed, so a budget is never spent on it.
//! 2. **Cursors advance**, so an entry considered in an earlier round is not a
//!    candidate in a later one. A cascade cannot re-chew its own history.
//! 3. **Stable order**, inherited from [`WatchTable::wake`]: rounds run their
//!    subjects in key order, so the same inputs produce the same run order and
//!    a recorded cascade replays.
//! 4. **A bounded round count**, the [`CascadeBudget`]. Reaching it is not a
//!    silent truncation: the outcome names the subjects still waking each
//!    other, so a host can say which behaviors are fighting rather than
//!    reporting that something vague went wrong.
//!
//! The budget is a **setting** (ruled 2026-08-13). There is deliberately no
//! unlimited value: an unbounded cascade is precisely the condition the budget
//! exists to name, so offering a way to switch the naming off would be
//! offering a way to hang the application quietly.
//!
//! This module runs no bodies. The host supplies a runner closure, because
//! what "run a denizen" means (a piccolo script under a step budget, a wasm
//! component, a scenario step) is the host's business, and because a headless
//! cascade is testable while an embedded one is not.

use crate::Subject;
use crate::cap::ScopePath;
use crate::watch::{Wake, WatchEvent, WatchTable};

/// An owned committed entry: what a round hands the next one.
///
/// [`WatchEvent`] borrows, which suits a single drain over a journal the host
/// already holds. A cascade's later rounds consume entries produced *during*
/// the cascade, so those have to be owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedEntry {
    /// The entry's position in its journal.
    pub seq: u64,
    /// Who committed it, in that journal's author convention.
    pub author: String,
    /// The scopes it touched.
    pub scopes: Vec<ScopePath>,
}

impl CommittedEntry {
    /// A committed entry.
    pub fn new(seq: u64, author: impl Into<String>, scopes: Vec<ScopePath>) -> Self {
        Self {
            seq,
            author: author.into(),
            scopes,
        }
    }

    /// Borrow it as a matcher input.
    pub fn as_event(&self) -> WatchEvent<'_> {
        WatchEvent {
            seq: self.seq,
            author: &self.author,
            scopes: &self.scopes,
        }
    }
}

/// How many rounds one cascade may run.
///
/// A setting, so it is constructed from a configured number rather than a
/// constant. The floor is 1: zero rounds would mean a commit wakes nothing,
/// which is not a smaller budget but a different (and silent) feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CascadeBudget(u32);

impl CascadeBudget {
    /// The default: enough for a behavior to answer a behavior that answered
    /// the user, with room to spare, and small enough that a genuine loop is
    /// reported in the same beat it starts.
    pub const DEFAULT: Self = Self(4);

    /// A budget of `rounds`, clamped up to the floor of 1.
    ///
    /// Clamped rather than refused: a setting that has drifted to zero should
    /// leave behaviors working, not disable them, and the floor is the
    /// smallest honest reading of "run cascades".
    pub fn new(rounds: u32) -> Self {
        Self(rounds.max(1))
    }

    /// The round count.
    pub fn rounds(self) -> u32 {
        self.0
    }
}

impl Default for CascadeBudget {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// How a cascade ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CascadeOutcome {
    /// A round woke nobody. The ordinary ending.
    Settled,
    /// The budget ran out with work still pending. The subjects named are the
    /// ones that would have woken next: the honest answer to "which behaviors
    /// are fighting", which is what a host puts on screen.
    BudgetExhausted {
        /// Who would have run in the round that was not allowed.
        still_waking: Vec<Subject>,
    },
}

impl CascadeOutcome {
    /// Whether the budget was reached.
    pub fn exhausted(&self) -> bool {
        matches!(self, CascadeOutcome::BudgetExhausted { .. })
    }
}

/// What one round did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Round {
    /// Who woke, in the stable order they were run in.
    pub wakes: Vec<Wake>,
}

/// A whole cascade: every round, and how it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cascade {
    /// Rounds in order. Empty when the triggering commits woke nobody.
    pub rounds: Vec<Round>,
    /// How it ended.
    pub outcome: CascadeOutcome,
}

impl Cascade {
    /// Every subject that ran, in first-woken order, without repeats. What a
    /// host shows when reporting a cascade.
    pub fn participants(&self) -> Vec<Subject> {
        let mut seen: Vec<Subject> = Vec::new();
        for round in &self.rounds {
            for wake in &round.wakes {
                if !seen.contains(&wake.subject) {
                    seen.push(wake.subject);
                }
            }
        }
        seen
    }
}

/// Run a cascade to settlement or to the budget.
///
/// `entries` are the commits that start it (a drain's worth of journal tail).
/// `run` is handed each round's wakes and returns whatever those bodies
/// committed; returning nothing settles the cascade. The runner is called once
/// per round rather than once per wake, so a host can commit a round's
/// petitions together and keep the journal's ordering its own business.
///
/// Cursors in `table` advance as rounds proceed, so this is the only place
/// that needs to be called: a second cascade over the same entries wakes
/// nobody.
pub fn run_cascade<R>(
    table: &mut WatchTable,
    budget: CascadeBudget,
    entries: Vec<CommittedEntry>,
    mut run: R,
) -> Cascade
where
    R: FnMut(&[Wake]) -> Vec<CommittedEntry>,
{
    let mut rounds: Vec<Round> = Vec::new();
    let mut pending = entries;

    for _ in 0..budget.rounds() {
        let events: Vec<WatchEvent<'_>> = pending.iter().map(CommittedEntry::as_event).collect();
        let wakes = table.wake(&events);
        if wakes.is_empty() {
            return Cascade {
                rounds,
                outcome: CascadeOutcome::Settled,
            };
        }
        let produced = run(&wakes);
        rounds.push(Round { wakes });
        pending = produced;
    }

    // The budget is spent. Anyone the next round would have woken is named,
    // without advancing a cursor: the work is deferred, not consumed, so a
    // later drain still sees it.
    let events: Vec<WatchEvent<'_>> = pending.iter().map(CommittedEntry::as_event).collect();
    let still_waking = table.would_wake(&events);
    let outcome = if still_waking.is_empty() {
        CascadeOutcome::Settled
    } else {
        CascadeOutcome::BudgetExhausted { still_waking }
    };
    Cascade { rounds, outcome }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cap::Cap;
    use crate::grant::{Grant, GrantTable, Mode};

    fn subject(seed: u8) -> Subject {
        Subject::new([seed; 32])
    }

    fn scope(raw: &str) -> ScopePath {
        ScopePath::parse(raw).expect("valid scope")
    }

    fn author(seed: u8) -> String {
        format!("denizen:{seed:02x}")
    }

    /// A table where each seed watches `trail` and authors as `denizen:<seed>`.
    fn watchers(seeds: &[u8]) -> WatchTable {
        let mut table = WatchTable::new();
        for &seed in seeds {
            watch_on(&mut table, seed, "trail");
        }
        table
    }

    /// Register `seed` on a scope of its own.
    fn watch_on(table: &mut WatchTable, seed: u8, watched: &str) {
        let who = subject(seed);
        let authority =
            GrantTable::new().with_grant(Grant::new(who, Cap::scope(watched).unwrap(), Mode::Read));
        table
            .register(&authority, who, scope(watched), author(seed))
            .expect("granted");
    }

    /// A commit inside `trail`, by `author`.
    fn commit(seq: u64, by: &str) -> CommittedEntry {
        CommittedEntry::new(seq, by, vec![scope("trail/today")])
    }

    #[test]
    fn a_chain_settles_when_nobody_answers() {
        // The ordinary shape: the user commits, one behavior answers, and its
        // answer wakes nobody.
        let mut table = watchers(&[1]);
        let mut seq = 100;
        let cascade = run_cascade(
            &mut table,
            CascadeBudget::DEFAULT,
            vec![commit(1, "user")],
            |wakes| {
                assert_eq!(wakes.len(), 1);
                seq += 1;
                vec![commit(seq, &author(1))]
            },
        );
        assert_eq!(cascade.outcome, CascadeOutcome::Settled);
        assert_eq!(cascade.rounds.len(), 1, "the answer woke nobody");
        assert_eq!(cascade.participants(), vec![subject(1)]);
    }

    #[test]
    fn a_relay_settles_when_the_last_link_writes_where_nobody_watches() {
        // A watches `trail` and answers into `notes`; B watches `notes` and
        // answers into `vault`, which no watch covers. Two rounds, then quiet.
        //
        // Worth stating because the first version of this test had both
        // subjects watching the same scope with a runner that always
        // committed, which is not a relay at all: it is two behaviors
        // answering each other, and it correctly exhausted the budget.
        let mut table = WatchTable::new();
        watch_on(&mut table, 1, "trail");
        watch_on(&mut table, 2, "notes");

        let mut round = 0;
        let cascade = run_cascade(
            &mut table,
            CascadeBudget::DEFAULT,
            vec![commit(1, "user")],
            |wakes| {
                round += 1;
                match round {
                    1 => {
                        assert_eq!(wakes[0].subject, subject(1), "A answers the user");
                        vec![CommittedEntry::new(
                            100,
                            author(1),
                            vec![scope("notes/summary")],
                        )]
                    }
                    _ => {
                        assert_eq!(wakes[0].subject, subject(2), "B answers A");
                        vec![CommittedEntry::new(
                            200,
                            author(2),
                            vec![scope("vault/key")],
                        )]
                    }
                }
            },
        );
        assert_eq!(cascade.outcome, CascadeOutcome::Settled);
        assert_eq!(cascade.rounds.len(), 2);
        assert_eq!(cascade.participants(), vec![subject(1), subject(2)]);
    }

    #[test]
    fn two_behaviors_answering_each_other_stop_at_the_budget_and_are_named() {
        // The failure mode the budget exists for. Each round, both wake and
        // both commit, so the next round wakes both again, forever.
        let mut table = watchers(&[1, 2]);
        let mut seq = 100;
        let budget = CascadeBudget::new(3);
        let cascade = run_cascade(&mut table, budget, vec![commit(1, "user")], |wakes| {
            wakes
                .iter()
                .map(|wake| {
                    seq += 1;
                    commit(seq, &author(wake.subject.0[0]))
                })
                .collect()
        });
        assert_eq!(cascade.rounds.len(), 3, "it ran exactly its budget");
        assert_eq!(
            cascade.outcome,
            CascadeOutcome::BudgetExhausted {
                still_waking: vec![subject(1), subject(2)]
            },
            "and says who is fighting rather than stopping quietly"
        );
    }

    #[test]
    fn exhaustion_defers_the_work_rather_than_consuming_it() {
        // The naming peek must not advance cursors: the entries that could not
        // be run are still candidates on the next drain, so a cascade cut
        // short loses nothing.
        let mut table = watchers(&[1, 2]);
        let mut seq = 100;
        let cascade = run_cascade(
            &mut table,
            CascadeBudget::new(1),
            vec![commit(1, "user")],
            |wakes| {
                wakes
                    .iter()
                    .map(|wake| {
                        seq += 1;
                        commit(seq, &author(wake.subject.0[0]))
                    })
                    .collect()
            },
        );
        assert!(cascade.outcome.exhausted());

        // The deferred entries wake their targets on a later pass.
        let deferred = vec![commit(101, &author(1)), commit(102, &author(2))];
        let next = run_cascade(&mut table, CascadeBudget::new(1), deferred, |_| Vec::new());
        assert_eq!(next.rounds.len(), 1, "the deferred work still ran");
    }

    #[test]
    fn a_budget_of_one_stops_after_a_single_round() {
        // The headless half of the live-setting condition: the number is read
        // per cascade, so changing it changes the very next one.
        let mut table = watchers(&[1, 2]);
        let cascade = run_cascade(
            &mut table,
            CascadeBudget::new(1),
            vec![commit(1, "user")],
            |_| vec![commit(2, "user")],
        );
        assert_eq!(cascade.rounds.len(), 1);
    }

    #[test]
    fn the_budget_floors_at_one_round() {
        assert_eq!(CascadeBudget::new(0).rounds(), 1);
        assert_eq!(CascadeBudget::DEFAULT.rounds(), 4);
    }

    #[test]
    fn nothing_to_wake_runs_no_bodies_at_all() {
        let mut table = watchers(&[1]);
        let mut called = false;
        let cascade = run_cascade(
            &mut table,
            CascadeBudget::DEFAULT,
            vec![CommittedEntry::new(1, "user", vec![scope("vault/key")])],
            |_| {
                called = true;
                Vec::new()
            },
        );
        assert!(!called, "the runner is not called for an empty wake set");
        assert_eq!(cascade.outcome, CascadeOutcome::Settled);
        assert!(cascade.rounds.is_empty());
    }

    #[test]
    fn a_cascade_runs_the_same_way_twice() {
        // What replay rests on: identical inputs, identical rounds.
        fn once() -> Cascade {
            let mut table = watchers(&[9, 2, 5]);
            let mut seq = 100;
            run_cascade(
                &mut table,
                CascadeBudget::new(2),
                vec![commit(1, "user")],
                |wakes| {
                    seq += 1;
                    vec![commit(seq, &author(wakes[0].subject.0[0]))]
                },
            )
        }
        let first = once();
        assert_eq!(first, once());
        assert_eq!(
            first.rounds[0]
                .wakes
                .iter()
                .map(|wake| wake.subject.0[0])
                .collect::<Vec<_>>(),
            vec![2, 5, 9],
            "and in key order, not registration order"
        );
    }
}
