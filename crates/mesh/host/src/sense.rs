// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The two seams that touch the world.
//!
//! `mere-mesh` reads no clock and asks the OS nothing; that is what makes its
//! fold deterministic and its projection honest. Every such reading enters
//! here, behind a trait, so a receipt can drive a supervisor with a clock it
//! controls and a battery that never existed.
//!
//! A real host implements [`ConditionSource`] over whatever it can actually
//! observe — idle time, power, thermals, link class. Nothing below this crate
//! is allowed to care how.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mesh::DeviceConditions;

/// Where "now" comes from. Milliseconds since the Unix epoch, because that is
/// what the lease wire carries.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// The wall clock. Devices in a ring are expected to be roughly synced;
/// `LeasePolicy::max_skew_ms` is the slack for "roughly".
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// A clock a test advances by hand, so lease boundaries land where the test
/// says rather than where the machine happened to be.
#[derive(Debug, Default)]
pub struct ManualClock {
    now_ms: AtomicU64,
}

impl ManualClock {
    pub fn at(now_ms: u64) -> Self {
        Self {
            now_ms: AtomicU64::new(now_ms),
        }
    }

    pub fn set(&self, now_ms: u64) {
        self.now_ms.store(now_ms, Ordering::SeqCst);
    }

    pub fn advance(&self, by_ms: u64) {
        self.now_ms.fetch_add(by_ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

/// What the device is doing right now.
///
/// The supervisor overwrites [`DeviceConditions::running_jobs`] with its own
/// in-flight count before using the reading: it owns that number, and a source
/// that could get it wrong would let a device talk itself past its own
/// concurrency limit.
pub trait ConditionSource: Send + Sync {
    fn conditions(&self) -> DeviceConditions;
}

/// Conditions a caller sets directly — a background sampler writing what it
/// last observed, or a test deciding when the human comes back.
#[derive(Debug)]
pub struct ObservedConditions {
    current: Mutex<DeviceConditions>,
}

impl ObservedConditions {
    pub fn new(conditions: DeviceConditions) -> Self {
        Self {
            current: Mutex::new(conditions),
        }
    }

    /// An idle, plugged-in, cool device with nothing running.
    pub fn spare() -> Self {
        Self::new(DeviceConditions::spare())
    }

    pub fn set(&self, conditions: DeviceConditions) {
        *self.current.lock().expect("conditions lock") = conditions;
    }

    /// The human is back at the keyboard.
    pub fn in_use(&self) {
        let mut current = self.current.lock().expect("conditions lock");
        current.idle_ms = 0;
    }
}

impl Default for ObservedConditions {
    fn default() -> Self {
        Self::spare()
    }
}

impl ConditionSource for ObservedConditions {
    fn conditions(&self) -> DeviceConditions {
        *self.current.lock().expect("conditions lock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_told() {
        let clock = ManualClock::at(1_000);
        assert_eq!(clock.now_ms(), 1_000);
        clock.advance(500);
        assert_eq!(clock.now_ms(), 1_500);
        clock.set(9);
        assert_eq!(clock.now_ms(), 9);
    }

    #[test]
    fn the_system_clock_is_after_the_epoch() {
        assert!(SystemClock.now_ms() > 1_700_000_000_000);
    }

    #[test]
    fn observed_conditions_are_replaceable_under_a_shared_reference() {
        let conditions = ObservedConditions::spare();
        assert!(conditions.conditions().idle_ms > 0);
        conditions.in_use();
        assert_eq!(conditions.conditions().idle_ms, 0);
    }
}
