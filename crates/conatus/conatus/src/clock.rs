// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::{error::Error, fmt};

const ONE_SECOND_US: u128 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockAdvance {
    pub steps: u64,
    pub deferred: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockError;

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("fixed-step tick rate must be greater than zero")
    }
}

impl Error for ClockError {}

/// Drift-free conversion from uneven host time into whole simulation steps.
///
/// The accumulator keeps elapsed microseconds multiplied by the tick rate.
/// This avoids rounding a duration such as 1/60 second to an integer interval.
/// A step cap defers excess work instead of silently dropping simulation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedClock {
    ticks_per_second: u32,
    scaled_remainder: u128,
    steps_taken: u64,
}

impl FixedClock {
    pub fn new(ticks_per_second: u32) -> Result<Self, ClockError> {
        if ticks_per_second == 0 {
            return Err(ClockError);
        }
        Ok(Self {
            ticks_per_second,
            scaled_remainder: 0,
            steps_taken: 0,
        })
    }

    pub const fn ticks_per_second(&self) -> u32 {
        self.ticks_per_second
    }

    pub const fn steps_taken(&self) -> u64 {
        self.steps_taken
    }

    pub fn step_seconds(&self) -> f32 {
        1.0 / self.ticks_per_second as f32
    }

    pub fn pending_steps(&self) -> u64 {
        (self.scaled_remainder / ONE_SECOND_US).min(u64::MAX as u128) as u64
    }

    /// Fractional progress toward the next step after any whole-step backlog.
    pub fn interpolation_alpha(&self) -> f32 {
        (self.scaled_remainder % ONE_SECOND_US) as f32 / ONE_SECOND_US as f32
    }

    pub fn advance(&mut self, elapsed_us: u64, max_steps: u64) -> ClockAdvance {
        self.scaled_remainder = self
            .scaled_remainder
            .saturating_add(elapsed_us as u128 * self.ticks_per_second as u128);

        let due = self.scaled_remainder / ONE_SECOND_US;
        let steps = due.min(max_steps as u128).min(u64::MAX as u128) as u64;
        self.scaled_remainder -= steps as u128 * ONE_SECOND_US;
        self.steps_taken = self.steps_taken.saturating_add(steps);

        ClockAdvance {
            steps,
            deferred: (due - steps as u128).min(u64::MAX as u128) as u64,
        }
    }

    /// Explicitly drop accumulated whole steps while retaining the fractional
    /// interpolation remainder. Useful after an application resumes from a
    /// deliberate pause; ordinary frame stalls should use deferred catch-up.
    pub fn discard_backlog(&mut self) -> u64 {
        let discarded = self.pending_steps();
        self.scaled_remainder %= ONE_SECOND_US;
        discarded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_hertz_does_not_drift() {
        let mut clock = FixedClock::new(60).unwrap();
        for _ in 0..60 {
            clock.advance(1_000_000, u64::MAX);
        }
        assert_eq!(clock.steps_taken(), 3_600);
    }

    #[test]
    fn cap_defers_without_dropping_steps() {
        let mut clock = FixedClock::new(60).unwrap();
        let first = clock.advance(1_000_000, 4);
        assert_eq!(
            first,
            ClockAdvance {
                steps: 4,
                deferred: 56
            }
        );

        let mut recovered = first.steps;
        for _ in 0..14 {
            recovered += clock.advance(0, 4).steps;
        }
        assert_eq!(recovered, 60);
        assert_eq!(clock.pending_steps(), 0);
    }

    #[test]
    fn interpolation_ignores_whole_step_backlog() {
        let mut clock = FixedClock::new(10).unwrap();
        clock.advance(250_000, 0);
        assert_eq!(clock.pending_steps(), 2);
        assert!((clock.interpolation_alpha() - 0.5).abs() < f32::EPSILON);
    }
}
