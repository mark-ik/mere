//! Standing subscriptions: **when** a denizen runs.
//!
//! [`gate`](crate::gate) answers "may it write". This module answers the other
//! half: a **watch** is a scope whose committed changes wake a denizen's body,
//! so a helper can react to the world instead of waiting to be invoked.
//!
//! Three properties, each enforced here rather than left to a host:
//!
//! 1. **The containment law.** A watch must sit inside what its subject may
//!    already read: registration asks the [`AuthorityProvider`] and refuses
//!    otherwise. You cannot be woken by what you cannot read, so a watch can
//!    never become a side channel for observing a region a grant excludes.
//! 2. **No self-waking.** An entry authored by the watcher never matches that
//!    watcher's own watch. The trivial loop is unrepresentable rather than
//!    merely budgeted, which matters because the budget is the *last* line of
//!    defence and should only ever catch genuine mutual cascades.
//! 3. **Cursors, not replays.** Each watch remembers the last journal position
//!    it has seen. Waking is "what happened since", and a wake advances the
//!    cursor past everything considered, matched or not, so an unmatched entry
//!    is never rescanned.
//!
//! **The matcher is tier-agnostic on purpose.** It consumes [`WatchEvent`]s
//! (a sequence number, an author label, the scopes an entry touched) rather
//! than any one journal type, because the stack has two journals with two
//! different author conventions for the same identity:
//!
//! | journal | author label | set by |
//! |---|---|---|
//! | chartulary (a denizen's nested world) | `denizen:abcd1234` | [`Subject::to_author`] |
//! | mere's `GraphJournal` (the main graph) | the full 64-char hex | turnstone `remote_projection.rs` |
//!
//! Deriving the self-author here would therefore be right on one tier and
//! wrong on the other, and being wrong means the self-wake refusal silently
//! stops working. So [`WatchTable::register`] takes the label as an argument:
//! a caller cannot register a watch without stating which convention its
//! journal uses.
//!
//! **Scope shape is the caller's problem, and it is a real one.** Matching is
//! segment-prefix over [`ScopePath`], which fits a nested world (whose node
//! ids *are* scope paths, the same strings the gate scope-checks). Mere's main
//! graph keys nodes by `Uuid`, and a UUID is a single opaque segment, so
//! against that journal only an exact-node watch or the root scope can match
//! anything. Watching a *region* of the main graph needs a region vocabulary
//! that does not exist yet (container membership, address prefix, and tag are
//! the candidates); see the graph behaviors plan. Nothing here blocks it: a
//! main-graph adapter is another producer of [`WatchEvent`]s.

use crate::Subject;
use crate::cap::{Cap, CapError, ScopePath};
use crate::grant::{AuthorityProvider, Mode};

/// Why a watch was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchError {
    /// The subject holds no read capability covering the watched scope. The
    /// containment law: watch ⊆ read ⊆ grant.
    Unauthorized {
        /// The scope that was asked for.
        scope: String,
    },
    /// The persisted form did not parse.
    Malformed {
        /// The line as found.
        line: String,
    },
    /// The scope in a persisted form is not a valid scope.
    BadScope(CapError),
}

impl std::fmt::Display for WatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatchError::Unauthorized { scope } => {
                write!(f, "no read capability covers the watched scope `{scope}`")
            }
            WatchError::Malformed { line } => write!(f, "malformed watch record `{line}`"),
            WatchError::BadScope(err) => write!(f, "bad watch scope: {err}"),
        }
    }
}

impl std::error::Error for WatchError {}

/// One committed entry, as the matcher sees it.
///
/// Borrowed rather than owned: a host builds these per drain from whatever
/// journal it holds, and they live only for the call.
#[derive(Clone, Copy, Debug)]
pub struct WatchEvent<'a> {
    /// The entry's position in its journal. Monotonic within one journal.
    pub seq: u64,
    /// Who committed it, in that journal's own convention (see the module
    /// docs: the two journals label subjects differently).
    pub author: &'a str,
    /// The scopes this entry touched.
    pub scopes: &'a [ScopePath],
}

/// A standing subscription: this subject wakes when this scope changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watch {
    /// Whose watch it is.
    pub subject: Subject,
    /// The region whose changes wake it.
    pub scope: ScopePath,
    /// The author label this subject's own commits carry, in the journal this
    /// watch reads. Entries bearing it never wake this watch.
    pub self_author: String,
    /// The last journal position considered. Waking looks strictly past it.
    pub cursor: u64,
}

impl Watch {
    /// Whether `event` wakes this watch: past the cursor, not self-authored,
    /// and touching something the scope covers.
    pub fn matches(&self, event: &WatchEvent<'_>) -> bool {
        event.seq > self.cursor
            && event.author != self.self_author
            && event
                .scopes
                .iter()
                .any(|touched| self.scope.covers_scope(touched))
    }

    /// The persisted form: `<subject-hex> <cursor> <self-author> <scope>`.
    ///
    /// Space-separated with the scope last, because a scope segment may
    /// contain almost anything (including a colon, which is why the wire form
    /// of [`Cap`] cannot be reused here) but the first three fields cannot
    /// contain a space: hex is hex, a cursor is digits, and an author label is
    /// a journal's own identifier. Splitting three times leaves the remainder
    /// intact whatever it holds.
    pub fn to_wire(&self) -> String {
        format!(
            "{} {} {} {}",
            self.subject.to_hex(),
            self.cursor,
            self.self_author,
            self.scope
        )
    }

    /// Read back [`to_wire`](Self::to_wire).
    ///
    /// Deliberately does **not** re-check authority: a persisted watch is
    /// replayed as it was recorded, and a grant that has since narrowed is the
    /// authority layer's business at wake time, not a reason to lose the
    /// record silently on load.
    pub fn parse(line: &str) -> Result<Self, WatchError> {
        let malformed = || WatchError::Malformed {
            line: line.to_string(),
        };
        let mut parts = line.splitn(4, ' ');
        let hex = parts.next().ok_or_else(malformed)?;
        let cursor = parts.next().ok_or_else(malformed)?;
        let self_author = parts.next().ok_or_else(malformed)?;
        let scope = parts.next().ok_or_else(malformed)?;
        Ok(Self {
            subject: Subject::from_hex(hex).ok_or_else(malformed)?,
            cursor: cursor.parse().map_err(|_| malformed())?,
            self_author: self_author.to_string(),
            scope: ScopePath::parse(scope).map_err(WatchError::BadScope)?,
        })
    }
}

/// Which subjects a drain woke, and on what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wake {
    /// The denizen to run.
    pub subject: Subject,
    /// The journal positions that woke it, ascending. Handed to the body as
    /// its trigger context (the plan's W2).
    pub matched: Vec<u64>,
}

/// Every registered watch, and the wake decision over a drain.
#[derive(Clone, Debug, Default)]
pub struct WatchTable {
    watches: Vec<Watch>,
}

impl WatchTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a watch, enforcing the containment law.
    ///
    /// `self_author` is the label this subject's own commits carry in the
    /// journal this watch will read; entries bearing it never wake it. It is
    /// an argument rather than derived because the two journals in this stack
    /// label the same subject differently (see the module docs), so a derived
    /// label would disable the self-wake refusal on one of them without
    /// saying so.
    pub fn register(
        &mut self,
        provider: &impl AuthorityProvider,
        subject: Subject,
        scope: ScopePath,
        self_author: impl Into<String>,
    ) -> Result<&Watch, WatchError> {
        if !provider.covers(subject, &Cap::Scope(scope.clone()), Mode::Read) {
            return Err(WatchError::Unauthorized {
                scope: scope.to_string(),
            });
        }
        self.watches.push(Watch {
            subject,
            scope,
            self_author: self_author.into(),
            cursor: 0,
        });
        Ok(self.watches.last().expect("just pushed"))
    }

    /// Adopt a persisted watch as-is, without an authority check (see
    /// [`Watch::parse`]).
    pub fn adopt(&mut self, watch: Watch) {
        self.watches.push(watch);
    }

    /// Drop every watch held by `subject`. Uninstalling a denizen removes its
    /// watches with it; leaving one behind would wake a body that is gone.
    pub fn remove_subject(&mut self, subject: Subject) {
        self.watches.retain(|watch| watch.subject != subject);
    }

    /// The registered watches.
    pub fn watches(&self) -> &[Watch] {
        &self.watches
    }

    /// Whether anything is registered.
    pub fn is_empty(&self) -> bool {
        self.watches.is_empty()
    }

    /// Consider `events` and report who wakes, advancing every cursor past
    /// everything considered.
    ///
    /// Cursors advance whether or not an entry matched: an entry a watch has
    /// already declined is not a candidate again, so a quiet watch costs one
    /// comparison per entry per drain and never accumulates a backlog.
    ///
    /// Results come back in **stable subject order** (by key), which is what
    /// makes a cascade's execution order reproducible across runs.
    pub fn wake(&mut self, events: &[WatchEvent<'_>]) -> Vec<Wake> {
        let highest = events.iter().map(|event| event.seq).max();
        let mut wakes: Vec<Wake> = Vec::new();
        for watch in &mut self.watches {
            let matched: Vec<u64> = events
                .iter()
                .filter(|event| watch.matches(event))
                .map(|event| event.seq)
                .collect();
            if let Some(highest) = highest {
                watch.cursor = watch.cursor.max(highest);
            }
            if !matched.is_empty() {
                wakes.push(Wake {
                    subject: watch.subject,
                    matched,
                });
            }
        }
        wakes.sort_by_key(|wake| wake.subject.0);
        for wake in &mut wakes {
            wake.matched.sort_unstable();
        }
        wakes
    }

    /// Who `events` *would* wake, without advancing any cursor.
    ///
    /// The read-only twin of [`wake`](Self::wake), for naming the subjects a
    /// cascade ran out of budget before reaching. Peeking must not consume:
    /// the work is deferred to a later drain, not dropped, so the cursors have
    /// to stay where they are.
    pub fn would_wake(&self, events: &[WatchEvent<'_>]) -> Vec<Subject> {
        let mut subjects: Vec<Subject> = self
            .watches
            .iter()
            .filter(|watch| events.iter().any(|event| watch.matches(event)))
            .map(|watch| watch.subject)
            .collect();
        subjects.sort_by_key(|subject| subject.0);
        subjects.dedup();
        subjects
    }

    /// The whole table as persistable lines, one watch each.
    pub fn to_wire_lines(&self) -> Vec<String> {
        self.watches.iter().map(Watch::to_wire).collect()
    }

    /// Rebuild a table from [`to_wire_lines`](Self::to_wire_lines). Blank
    /// lines are skipped; a malformed one fails the load rather than being
    /// dropped, because a watch that vanishes quietly is a behavior that
    /// stops running for no stated reason.
    pub fn from_wire_lines<'a>(
        lines: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, WatchError> {
        let mut table = Self::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            table.watches.push(Watch::parse(line)?);
        }
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grant::{Grant, GrantTable};

    fn subject(seed: u8) -> Subject {
        Subject::new([seed; 32])
    }

    fn scope(raw: &str) -> ScopePath {
        ScopePath::parse(raw).expect("valid scope")
    }

    /// A subject holding read over `held`.
    fn reader(subject: Subject, held: &str) -> GrantTable {
        GrantTable::new().with_grant(Grant::new(subject, Cap::scope(held).unwrap(), Mode::Read))
    }

    fn event<'a>(seq: u64, author: &'a str, scopes: &'a [ScopePath]) -> WatchEvent<'a> {
        WatchEvent {
            seq,
            author,
            scopes,
        }
    }

    #[test]
    fn a_watch_must_sit_inside_what_its_subject_may_read() {
        let alice = subject(1);
        let authority = reader(alice, "trail");
        let mut table = WatchTable::new();

        assert!(
            table
                .register(&authority, alice, scope("trail/today"), "denizen:aa")
                .is_ok(),
            "narrower than the grant is inside it"
        );
        assert!(
            table
                .register(&authority, alice, scope("trail"), "denizen:aa")
                .is_ok(),
            "the grant's own scope is inside itself"
        );
        assert_eq!(
            table.register(&authority, alice, scope("vault"), "denizen:aa"),
            Err(WatchError::Unauthorized {
                scope: "vault".into()
            }),
            "you cannot be woken by what you cannot read"
        );
        assert_eq!(
            table.register(&authority, alice, ScopePath::root(), "denizen:aa"),
            Err(WatchError::Unauthorized { scope: "".into() }),
            "and the root scope is not a loophole"
        );
    }

    #[test]
    fn a_write_grant_covers_the_read_a_watch_needs() {
        let alice = subject(1);
        let authority = GrantTable::new().with_grant(Grant::new(
            alice,
            Cap::scope("trail").unwrap(),
            Mode::Write,
        ));
        let mut table = WatchTable::new();
        assert!(
            table
                .register(&authority, alice, scope("trail"), "denizen:aa")
                .is_ok()
        );
    }

    #[test]
    fn matching_is_by_segment_so_a_prefix_is_not_a_neighbour() {
        let alice = subject(1);
        let mut table = WatchTable::new();
        table
            .register(&reader(alice, "trail"), alice, scope("trail"), "denizen:aa")
            .unwrap();

        let inside = [scope("trail/today")];
        let neighbour = [scope("trailer/today")];
        assert_eq!(table.wake(&[event(1, "user", &inside)]).len(), 1);
        assert!(
            table.wake(&[event(2, "user", &neighbour)]).is_empty(),
            "`trail` does not cover `trailer`"
        );
    }

    #[test]
    fn a_denizen_never_wakes_itself() {
        // The structural half of cascade termination. The budget catches
        // mutual cascades; this catches the trivial one, so a body writing
        // inside its own watch cannot spin.
        let alice = subject(1);
        let bob = subject(2);
        let mut table = WatchTable::new();
        table
            .register(&reader(alice, "trail"), alice, scope("trail"), "denizen:aa")
            .unwrap();
        table
            .register(&reader(bob, "trail"), bob, scope("trail"), "denizen:bb")
            .unwrap();

        let touched = [scope("trail/today")];
        let wakes = table.wake(&[event(1, "denizen:aa", &touched)]);
        assert_eq!(
            wakes,
            vec![Wake {
                subject: bob,
                matched: vec![1]
            }],
            "alice's own commit wakes bob and not alice"
        );
    }

    #[test]
    fn the_self_author_label_is_whatever_the_journal_uses() {
        // The hazard the explicit label exists for: mere's main journal
        // attributes a denizen by full hex, chartulary's by `denizen:` plus
        // eight. A watch registered for one journal must refuse self-wake in
        // that journal's own terms.
        let alice = subject(1);
        let mut table = WatchTable::new();
        table
            .register(
                &reader(alice, "trail"),
                alice,
                scope("trail"),
                alice.to_hex(),
            )
            .unwrap();

        let touched = [scope("trail/today")];
        assert!(
            table
                .wake(&[event(1, &alice.to_hex(), &touched)])
                .is_empty(),
            "the full-hex convention refuses self-wake"
        );
        assert_eq!(
            table.wake(&[event(2, "user", &touched)]).len(),
            1,
            "and still wakes on somebody else's commit"
        );
    }

    #[test]
    fn a_cursor_advances_past_everything_considered() {
        let alice = subject(1);
        let mut table = WatchTable::new();
        table
            .register(&reader(alice, "trail"), alice, scope("trail"), "denizen:aa")
            .unwrap();

        let elsewhere = [scope("vault/key")];
        let inside = [scope("trail/today")];
        assert!(
            table.wake(&[event(7, "user", &elsewhere)]).is_empty(),
            "nothing matched"
        );
        assert_eq!(
            table.watches()[0].cursor,
            7,
            "but the unmatched entry is not a candidate again"
        );
        assert!(
            table.wake(&[event(7, "user", &inside)]).is_empty(),
            "an entry at or before the cursor cannot wake"
        );
        assert_eq!(table.wake(&[event(8, "user", &inside)]).len(), 1);
    }

    #[test]
    fn wakes_come_back_in_stable_subject_order() {
        // Cascade determinism rests on this: same inputs, same run order.
        let mut table = WatchTable::new();
        for seed in [9u8, 2, 5] {
            let who = subject(seed);
            table
                .register(
                    &reader(who, "trail"),
                    who,
                    scope("trail"),
                    format!("denizen:{seed:02x}"),
                )
                .unwrap();
        }
        let touched = [scope("trail/today")];
        let woken: Vec<u8> = table
            .wake(&[event(1, "user", &touched)])
            .iter()
            .map(|wake| wake.subject.0[0])
            .collect();
        assert_eq!(woken, vec![2, 5, 9]);
    }

    #[test]
    fn watches_and_their_cursors_survive_a_round_trip() {
        let alice = subject(1);
        let mut table = WatchTable::new();
        table
            .register(
                &reader(alice, "trail"),
                alice,
                scope("trail/today"),
                "denizen:aa",
            )
            .unwrap();
        let inside = [scope("trail/today/one")];
        table.wake(&[event(42, "user", &inside)]);

        let lines = table.to_wire_lines();
        let reloaded =
            WatchTable::from_wire_lines(lines.iter().map(String::as_str)).expect("round trip");
        assert_eq!(reloaded.watches(), table.watches());
        assert_eq!(reloaded.watches()[0].cursor, 42);
    }

    #[test]
    fn a_scope_holding_a_colon_survives_the_wire_form() {
        // Why the wire form is space-separated with the scope last: a scope
        // segment may contain a colon, so `Cap`'s `kind:value` form cannot
        // carry one unambiguously.
        let alice = subject(1);
        let awkward = scope("trail/urn:chart:rel:cites");
        let mut table = WatchTable::new();
        table
            .register(
                &reader(alice, "trail"),
                alice,
                awkward.clone(),
                "denizen:aa",
            )
            .unwrap();
        let line = table.watches()[0].to_wire();
        assert_eq!(Watch::parse(&line).unwrap().scope, awkward);
    }

    #[test]
    fn a_malformed_record_fails_the_load_rather_than_vanishing() {
        assert!(matches!(
            WatchTable::from_wire_lines(["not a watch"]),
            Err(WatchError::Malformed { .. })
        ));
        assert!(
            WatchTable::from_wire_lines(["", "   "])
                .expect("blank lines are skipped")
                .is_empty()
        );
    }

    #[test]
    fn uninstalling_a_denizen_takes_its_watches_with_it() {
        let alice = subject(1);
        let bob = subject(2);
        let mut table = WatchTable::new();
        table
            .register(&reader(alice, "trail"), alice, scope("trail"), "denizen:aa")
            .unwrap();
        table
            .register(&reader(bob, "trail"), bob, scope("trail"), "denizen:bb")
            .unwrap();

        table.remove_subject(alice);
        assert_eq!(table.watches().len(), 1);
        assert_eq!(table.watches()[0].subject, bob);
    }
}
