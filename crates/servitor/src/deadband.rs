//! Deadband at the actuation boundary.
//!
//! A cascade budget bounds how deep one drain may run. It cannot see a slow
//! loop that settles, wakes again later, and writes forever. A deadband bounds
//! that second failure mode before another attributed commit reaches the
//! journal.
//!
//! The policy is declared by a behavior, not inferred from arbitrary graph
//! edits. The behavior supplies one signed scalar output in its own stable
//! units, and [`DeadbandTable`] compares it with the last output that was
//! actually accepted. The host supplies the time. Servitor therefore invents
//! neither a fake distance between edit batches nor a wall clock that a replay
//! cannot reproduce.

use crate::Subject;

/// A behavior's two-dimensional actuation deadband.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deadband {
    minimum_change: u64,
    minimum_interval_ms: u64,
}

impl Deadband {
    /// Declare a real deadband. Both dimensions must be positive: zero in
    /// either position would advertise a bound that does not exist.
    pub fn new(minimum_change: u64, minimum_interval_ms: u64) -> Result<Self, DeadbandError> {
        if minimum_change == 0 {
            return Err(DeadbandError::ZeroMinimumChange);
        }
        if minimum_interval_ms == 0 {
            return Err(DeadbandError::ZeroMinimumInterval);
        }
        Ok(Self {
            minimum_change,
            minimum_interval_ms,
        })
    }

    /// Smallest accepted absolute change from the last accepted output.
    pub fn minimum_change(self) -> u64 {
        self.minimum_change
    }

    /// Smallest accepted elapsed interval from the last accepted actuation.
    pub fn minimum_interval_ms(self) -> u64 {
        self.minimum_interval_ms
    }
}

/// An invalid declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadbandError {
    /// The change dimension would be absent.
    ZeroMinimumChange,
    /// The frequency dimension would be absent.
    ZeroMinimumInterval,
}

impl std::fmt::Display for DeadbandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMinimumChange => f.write_str("minimum change must be positive"),
            Self::ZeroMinimumInterval => f.write_str("minimum interval must be positive"),
        }
    }
}

impl std::error::Error for DeadbandError {}

/// One proposed output, with time injected by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Actuation {
    /// The behavior's output in its own declared, stable units.
    pub output: i64,
    /// The host-supplied instant in milliseconds.
    pub at_ms: u64,
}

impl Actuation {
    /// Pair an output with its replayable host instant.
    pub fn new(output: i64, at_ms: u64) -> Self {
        Self { output, at_ms }
    }
}

/// The failed change dimension of a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeRefusal {
    /// What the declaration requires.
    pub minimum: u64,
    /// The absolute change from the last accepted output.
    pub actual: u64,
}

/// The failed frequency dimension of a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntervalRefusal {
    /// What the declaration requires.
    pub minimum_ms: u64,
    /// Time since the last accepted actuation. Zero when the host clock moved
    /// backwards, which fails closed rather than wrapping.
    pub elapsed_ms: u64,
}

/// A named behavior actuation that the deadband refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeadbandRefusal {
    /// Which behavior was refused.
    pub subject: Subject,
    /// Present when the proposed output moved too little.
    pub change: Option<ChangeRefusal>,
    /// Present when the proposed output arrived too soon.
    pub interval: Option<IntervalRefusal>,
}

impl std::fmt::Display for DeadbandRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "behavior {} refused by deadband", self.subject.to_hex())?;
        if let Some(change) = self.change {
            write!(f, ": change {} is below {}", change.actual, change.minimum)?;
        }
        if let Some(interval) = self.interval {
            write!(
                f,
                "{}interval {} ms is below {} ms",
                if self.change.is_some() { "; " } else { ": " },
                interval.elapsed_ms,
                interval.minimum_ms
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for DeadbandRefusal {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Accepted {
    output: i64,
    at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    subject: Subject,
    deadband: Deadband,
    accepted: Option<Accepted>,
}

/// Proof that an actuation passed the current declaration.
///
/// A host can check, perform its commit, then call
/// [`DeadbandTable::record`]. [`crate::gate::Gate::petition_behavior`] does
/// exactly that, so a failed revision check does not consume the interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeadbandAdmission {
    subject: Subject,
    deadband: Option<Deadband>,
    actuation: Actuation,
}

/// Every declared behavior deadband and its last accepted output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeadbandTable {
    entries: Vec<Entry>,
}

impl DeadbandTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace a behavior's declaration. Re-registering the same
    /// declaration preserves its accepted baseline; changing policy resets it.
    pub fn register(&mut self, subject: Subject, deadband: Deadband) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.subject == subject)
        {
            if entry.deadband != deadband {
                entry.deadband = deadband;
                entry.accepted = None;
            }
            return;
        }
        self.entries.push(Entry {
            subject,
            deadband,
            accepted: None,
        });
    }

    /// Drop a behavior's declaration and accepted baseline with its residency.
    pub fn remove_subject(&mut self, subject: Subject) {
        self.entries.retain(|entry| entry.subject != subject);
    }

    /// The declaration for `subject`, if it has one.
    pub fn get(&self, subject: Subject) -> Option<Deadband> {
        self.entries
            .iter()
            .find(|entry| entry.subject == subject)
            .map(|entry| entry.deadband)
    }

    /// Whether any behavior declares a deadband.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Check without moving the baseline. The first output is admitted to
    /// establish one. An undeclared subject is admitted but remains untracked,
    /// which lets a host use one actuation lane for behaviors with and without
    /// a declaration.
    pub fn check(
        &self,
        subject: Subject,
        actuation: Actuation,
    ) -> Result<DeadbandAdmission, DeadbandRefusal> {
        let Some(entry) = self.entries.iter().find(|entry| entry.subject == subject) else {
            return Ok(DeadbandAdmission {
                subject,
                deadband: None,
                actuation,
            });
        };
        let Some(accepted) = entry.accepted else {
            return Ok(DeadbandAdmission {
                subject,
                deadband: Some(entry.deadband),
                actuation,
            });
        };

        let actual_change = absolute_change(accepted.output, actuation.output);
        let elapsed_ms = actuation.at_ms.saturating_sub(accepted.at_ms);
        let change = (actual_change < entry.deadband.minimum_change).then_some(ChangeRefusal {
            minimum: entry.deadband.minimum_change,
            actual: actual_change,
        });
        let interval =
            (elapsed_ms < entry.deadband.minimum_interval_ms).then_some(IntervalRefusal {
                minimum_ms: entry.deadband.minimum_interval_ms,
                elapsed_ms,
            });
        if change.is_some() || interval.is_some() {
            return Err(DeadbandRefusal {
                subject,
                change,
                interval,
            });
        }
        Ok(DeadbandAdmission {
            subject,
            deadband: Some(entry.deadband),
            actuation,
        })
    }

    /// Move the accepted baseline after the host's actuation succeeds.
    /// Admissions for undeclared subjects are deliberately a no-op.
    pub fn record(&mut self, admission: DeadbandAdmission) {
        let Some(deadband) = admission.deadband else {
            return;
        };
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.subject == admission.subject && entry.deadband == deadband)
        {
            entry.accepted = Some(Accepted {
                output: admission.actuation.output,
                at_ms: admission.actuation.at_ms,
            });
        }
    }

    /// Check and immediately record an accepted actuation. Useful when the
    /// caller's actuation cannot fail; fallible commit paths should use
    /// [`check`](Self::check) and [`record`](Self::record) separately.
    pub fn admit(&mut self, subject: Subject, actuation: Actuation) -> Result<(), DeadbandRefusal> {
        let admission = self.check(subject, actuation)?;
        self.record(admission);
        Ok(())
    }

    /// The whole table as stable, persistable lines.
    ///
    /// Each line is
    /// `<subject> <minimum-change> <minimum-interval-ms> <output|-> <at-ms|->`.
    pub fn to_wire_lines(&self) -> Vec<String> {
        let mut entries: Vec<&Entry> = self.entries.iter().collect();
        entries.sort_by_key(|entry| entry.subject.0);
        entries
            .into_iter()
            .map(|entry| match entry.accepted {
                Some(accepted) => format!(
                    "{} {} {} {} {}",
                    entry.subject.to_hex(),
                    entry.deadband.minimum_change,
                    entry.deadband.minimum_interval_ms,
                    accepted.output,
                    accepted.at_ms
                ),
                None => format!(
                    "{} {} {} - -",
                    entry.subject.to_hex(),
                    entry.deadband.minimum_change,
                    entry.deadband.minimum_interval_ms
                ),
            })
            .collect()
    }

    /// Rebuild from [`to_wire_lines`](Self::to_wire_lines). One malformed line
    /// fails the whole load so a missing stability policy cannot hide inside a
    /// partially accepted file.
    pub fn from_wire_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut table = Self::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let mut parts = line.split(' ');
            let subject = Subject::from_hex(parts.next()?)?;
            let minimum_change = parts.next()?.parse().ok()?;
            let minimum_interval_ms = parts.next()?.parse().ok()?;
            let deadband = Deadband::new(minimum_change, minimum_interval_ms).ok()?;
            let output = parts.next()?;
            let at_ms = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let accepted = match (output, at_ms) {
                ("-", "-") => None,
                ("-", _) | (_, "-") => return None,
                (output, at_ms) => Some(Accepted {
                    output: output.parse().ok()?,
                    at_ms: at_ms.parse().ok()?,
                }),
            };
            if table.get(subject).is_some() {
                return None;
            }
            table.entries.push(Entry {
                subject,
                deadband,
                accepted,
            });
        }
        Some(table)
    }
}

fn absolute_change(before: i64, after: i64) -> u64 {
    let distance = (i128::from(after) - i128::from(before)).unsigned_abs();
    u64::try_from(distance).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(seed: u8) -> Subject {
        Subject::new([seed; 32])
    }

    fn band() -> Deadband {
        Deadband::new(5, 1_000).unwrap()
    }

    #[test]
    fn a_declaration_has_two_real_dimensions() {
        assert_eq!(Deadband::new(0, 1), Err(DeadbandError::ZeroMinimumChange));
        assert_eq!(Deadband::new(1, 0), Err(DeadbandError::ZeroMinimumInterval));
    }

    #[test]
    fn the_first_output_establishes_the_baseline() {
        let who = subject(1);
        let mut table = DeadbandTable::new();
        table.register(who, band());
        table.admit(who, Actuation::new(100, 10_000)).unwrap();
        assert_eq!(table.to_wire_lines().len(), 1);
    }

    #[test]
    fn too_small_and_too_soon_are_both_named() {
        let who = subject(1);
        let mut table = DeadbandTable::new();
        table.register(who, band());
        table.admit(who, Actuation::new(100, 10_000)).unwrap();
        let refusal = table.admit(who, Actuation::new(102, 10_100)).unwrap_err();
        assert_eq!(refusal.subject, who);
        assert_eq!(
            refusal.change,
            Some(ChangeRefusal {
                minimum: 5,
                actual: 2
            })
        );
        assert_eq!(
            refusal.interval,
            Some(IntervalRefusal {
                minimum_ms: 1_000,
                elapsed_ms: 100
            })
        );
    }

    #[test]
    fn both_boundaries_are_inclusive() {
        let who = subject(1);
        let mut table = DeadbandTable::new();
        table.register(who, band());
        table.admit(who, Actuation::new(100, 10_000)).unwrap();
        table.admit(who, Actuation::new(105, 11_000)).unwrap();
    }

    #[test]
    fn a_refusal_does_not_move_either_baseline() {
        let who = subject(1);
        let mut table = DeadbandTable::new();
        table.register(who, band());
        table.admit(who, Actuation::new(100, 10_000)).unwrap();
        assert!(table.admit(who, Actuation::new(200, 10_100)).is_err());
        table.admit(who, Actuation::new(105, 11_000)).unwrap();
    }

    #[test]
    fn a_clock_that_moves_backwards_fails_closed() {
        let who = subject(1);
        let mut table = DeadbandTable::new();
        table.register(who, band());
        table.admit(who, Actuation::new(0, 10_000)).unwrap();
        let refusal = table.admit(who, Actuation::new(100, 9_000)).unwrap_err();
        assert_eq!(refusal.change, None);
        assert_eq!(refusal.interval.unwrap().elapsed_ms, 0);
    }

    #[test]
    fn the_full_signed_range_has_a_saturating_distance() {
        let who = subject(1);
        let mut table = DeadbandTable::new();
        table.register(who, Deadband::new(u64::MAX, 1).unwrap());
        table.admit(who, Actuation::new(i64::MIN, 0)).unwrap();
        table.admit(who, Actuation::new(i64::MAX, 1)).unwrap();
    }

    #[test]
    fn state_round_trips_and_keeps_refusing_after_restart() {
        let who = subject(1);
        let mut table = DeadbandTable::new();
        table.register(who, band());
        table.admit(who, Actuation::new(100, 10_000)).unwrap();
        let lines = table.to_wire_lines();
        let mut restored =
            DeadbandTable::from_wire_lines(lines.iter().map(String::as_str)).unwrap();
        assert_eq!(restored, table);
        assert!(restored.admit(who, Actuation::new(102, 20_000)).is_err());
    }

    #[test]
    fn a_malformed_record_fails_the_whole_load() {
        let bad = ["not-a-subject 1 1000 - -"];
        assert!(DeadbandTable::from_wire_lines(bad).is_none());
    }
}
