// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Waking on the clock rather than on a change.
//!
//! "Untouched for a month", "archive orphans weekly": behaviors whose trigger
//! is the passage of time. They are deliberately **not** [`Watch`]es, because a
//! watch is a cursor into a journal and time is not a journal. Forcing ticks
//! through the same matcher would mean minting synthetic entries just to have
//! something to point a cursor at.
//!
//! Two properties carry over from the rest of the crate, and one is new:
//!
//! - **The clock is the host's**, exactly as it is for grant expiry (see
//!   [`GrantTable::set_now`](crate::grant::GrantTable::set_now)). This module
//!   reads no clock, so it stays portable, wasm-safe, and deterministic: a
//!   replay that feeds the same instants fires the same behaviors at the same
//!   points, which is the whole reason time is injected rather than sampled.
//! - **No self-waking to worry about.** A tick is caused by the clock, not by
//!   anybody's commit, so there is no author to compare and no trivial loop.
//!   What a woken body then *writes* is still subject to the ordinary cascade
//!   rules on the journal side.
//! - **A tick needs no capability, and that is a deliberate asymmetry.**
//!   A [`Watch`] must sit inside what its subject may read, because being woken
//!   by a region reveals that the region changed. Being woken by the clock
//!   reveals nothing: the time is not anybody's secret. What a tick costs is
//!   *resource*, not disclosure, and the gate for that is the install review
//!   naming the period out loud, not a scope.
//!
//! [`Watch`]: crate::watch::Watch

use crate::Subject;

/// How often a behavior asks to run.
///
/// A closed set rather than a free number of milliseconds: a period is
/// something a person reads in a review row before granting it, and "every
/// 900000" is not a sentence. A finer period would also be a resource question
/// worth answering deliberately rather than by letting a pack pick any integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Period {
    /// Every minute.
    Minute,
    /// Every hour.
    Hour,
    /// Every day.
    Day,
}

impl Period {
    /// Every period, for a caller enumerating the choices.
    pub const ALL: [Self; 3] = [Self::Minute, Self::Hour, Self::Day];

    /// The wire and declaration name.
    pub fn as_str(self) -> &'static str {
        match self {
            Period::Minute => "minute",
            Period::Hour => "hour",
            Period::Day => "day",
        }
    }

    /// Parse a declaration name.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == raw)
    }

    /// The span in milliseconds.
    pub fn millis(self) -> u64 {
        match self {
            Period::Minute => 60_000,
            Period::Hour => 3_600_000,
            Period::Day => 86_400_000,
        }
    }
}

impl std::fmt::Display for Period {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One standing schedule: this subject asks to run every `period`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeWatch {
    /// Whose schedule it is.
    pub subject: Subject,
    /// How often.
    pub period: Period,
    /// When it last ran, in the host's milliseconds. Set at registration so a
    /// freshly installed behavior waits out a full period rather than firing
    /// the instant it is admitted, which would make install itself a trigger.
    pub last_fired_ms: u64,
}

impl TimeWatch {
    /// The persisted form: `<subject-hex> <last-fired> <period>`.
    pub fn to_wire(&self) -> String {
        format!(
            "{} {} {}",
            self.subject.to_hex(),
            self.last_fired_ms,
            self.period
        )
    }

    /// Read back [`to_wire`](Self::to_wire).
    pub fn parse(line: &str) -> Option<Self> {
        let mut parts = line.split(' ');
        let subject = Subject::from_hex(parts.next()?)?;
        let last_fired_ms = parts.next()?.parse().ok()?;
        let period = Period::parse(parts.next()?)?;
        Some(Self {
            subject,
            period,
            last_fired_ms,
        })
    }
}

/// Every schedule, and which of them are due.
#[derive(Clone, Debug, Default)]
pub struct TimeWatchTable {
    watches: Vec<TimeWatch>,
}

impl TimeWatchTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a schedule, starting its period at `now_ms`.
    pub fn register(&mut self, subject: Subject, period: Period, now_ms: u64) -> &TimeWatch {
        self.watches.push(TimeWatch {
            subject,
            period,
            last_fired_ms: now_ms,
        });
        self.watches.last().expect("just pushed")
    }

    /// Adopt a persisted schedule as it stands.
    pub fn adopt(&mut self, watch: TimeWatch) {
        self.watches.push(watch);
    }

    /// Drop every schedule held by `subject`.
    pub fn remove_subject(&mut self, subject: Subject) {
        self.watches.retain(|watch| watch.subject != subject);
    }

    /// The registered schedules.
    pub fn watches(&self) -> &[TimeWatch] {
        &self.watches
    }

    /// Whether anything is scheduled.
    pub fn is_empty(&self) -> bool {
        self.watches.is_empty()
    }

    /// Who is due at `now_ms`, marking them as fired.
    ///
    /// **A missed period is not made up.** A session closed for a week does not
    /// open with seven daily runs queued; it runs once and starts a fresh
    /// period. A behavior that files or archives should act on what it finds
    /// now, and firing six extra times to "catch up" would be six chances to
    /// do the same work again.
    ///
    /// Results come back in stable subject order, like [`WatchTable::wake`], so
    /// a recorded run replays.
    ///
    /// [`WatchTable::wake`]: crate::watch::WatchTable::wake
    pub fn due(&mut self, now_ms: u64) -> Vec<Subject> {
        let mut due: Vec<Subject> = Vec::new();
        for watch in &mut self.watches {
            // Saturating: a host clock that jumped backwards (a system time
            // change) must not wrap into a huge elapsed span and fire
            // everything at once.
            let elapsed = now_ms.saturating_sub(watch.last_fired_ms);
            if elapsed >= watch.period.millis() {
                watch.last_fired_ms = now_ms;
                due.push(watch.subject);
            }
        }
        due.sort_by_key(|subject| subject.0);
        due.dedup();
        due
    }

    /// The whole table as persistable lines.
    pub fn to_wire_lines(&self) -> Vec<String> {
        self.watches.iter().map(TimeWatch::to_wire).collect()
    }

    /// Rebuild from [`to_wire_lines`](Self::to_wire_lines). A malformed line
    /// fails the load rather than vanishing: a schedule that quietly stops
    /// existing is a behavior that stops running for no stated reason.
    pub fn from_wire_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut table = Self::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            table.watches.push(TimeWatch::parse(line)?);
        }
        Some(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(seed: u8) -> Subject {
        Subject::new([seed; 32])
    }

    const HOUR: u64 = 3_600_000;

    #[test]
    fn a_fresh_schedule_waits_out_its_first_period() {
        // Install is not a trigger: a behavior admitted at noon runs at one,
        // not at noon.
        let mut table = TimeWatchTable::new();
        table.register(subject(1), Period::Hour, 1_000);
        assert!(table.due(1_000).is_empty());
        assert!(table.due(1_000 + HOUR - 1).is_empty());
        assert_eq!(table.due(1_000 + HOUR), vec![subject(1)]);
    }

    #[test]
    fn firing_starts_the_next_period_from_the_moment_it_fired() {
        let mut table = TimeWatchTable::new();
        table.register(subject(1), Period::Hour, 0);
        assert_eq!(table.due(HOUR).len(), 1);
        assert!(table.due(HOUR + 1).is_empty(), "not twice for one period");
        assert_eq!(table.due(2 * HOUR).len(), 1);
    }

    #[test]
    fn a_long_absence_fires_once_rather_than_catching_up() {
        // A week closed does not open with seven days of queued runs.
        let mut table = TimeWatchTable::new();
        table.register(subject(1), Period::Day, 0);
        assert_eq!(table.due(7 * 86_400_000).len(), 1);
        assert!(
            table.due(7 * 86_400_000).is_empty(),
            "and the backlog is gone, not queued"
        );
    }

    #[test]
    fn a_clock_that_jumps_backwards_fires_nothing() {
        // A system time change must not wrap into an enormous elapsed span.
        let mut table = TimeWatchTable::new();
        table.register(subject(1), Period::Hour, 10 * HOUR);
        assert!(table.due(0).is_empty());
        assert!(table.due(HOUR).is_empty());
    }

    #[test]
    fn the_same_instants_fire_the_same_behaviors_in_the_same_order() {
        // What replay rests on: time is fed in, never sampled.
        fn run() -> Vec<Vec<u8>> {
            let mut table = TimeWatchTable::new();
            for seed in [9u8, 2, 5] {
                table.register(subject(seed), Period::Hour, 0);
            }
            [HOUR / 2, HOUR, HOUR + 1, 2 * HOUR]
                .into_iter()
                .map(|now| table.due(now).iter().map(|s| s.0[0]).collect())
                .collect()
        }
        let first = run();
        assert_eq!(first, run());
        assert_eq!(first, vec![vec![], vec![2, 5, 9], vec![], vec![2, 5, 9]]);
    }

    #[test]
    fn periods_round_trip_through_their_declaration_names() {
        for period in Period::ALL {
            assert_eq!(Period::parse(period.as_str()), Some(period));
        }
        assert_eq!(Period::parse("fortnight"), None);
    }

    #[test]
    fn a_schedule_and_its_phase_survive_a_round_trip() {
        let mut table = TimeWatchTable::new();
        table.register(subject(1), Period::Day, 12_345);
        let lines = table.to_wire_lines();
        let back = TimeWatchTable::from_wire_lines(lines.iter().map(String::as_str)).unwrap();
        assert_eq!(back.watches(), table.watches());
        assert_eq!(back.watches()[0].last_fired_ms, 12_345);
    }

    #[test]
    fn uninstalling_a_denizen_takes_its_schedule_with_it() {
        let mut table = TimeWatchTable::new();
        table.register(subject(1), Period::Hour, 0);
        table.register(subject(2), Period::Hour, 0);
        table.remove_subject(subject(1));
        assert_eq!(table.watches().len(), 1);
        assert_eq!(table.watches()[0].subject, subject(2));
    }
}
