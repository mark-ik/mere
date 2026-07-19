// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Portable monotonic time.
//!
//! [`PortableInstant`] is a plain `u64` — milliseconds elapsed since an
//! implementation-chosen origin (typically application start). The
//! runtime never asks the platform "what time is it now"; instead, the
//! host emits a `PortableInstant` per frame (or per event) drawn from
//! whatever monotonic clock it prefers, and the runtime uses it for
//! comparisons / deadlines.
//!
//! Desktop hosts derive `PortableInstant` from `std::time::Instant`
//! (monotonic by construction). WASM hosts derive it from
//! `performance.now()` (also monotonic). Because the origin is
//! host-chosen, values from different hosts are not comparable — this
//! is a host-bounded monotonic clock, not a global wall clock. Use
//! [`UNIX_EPOCH`](std::time::UNIX_EPOCH)-relative timestamps for
//! anything user-visible or persisted; use `PortableInstant` for
//! ephemeral things like animation timers and request deadlines.
//!
//! Design choice: why a newtype instead of a plain `u64`?
//!
//! - Prevents accidental mixing of "ms since epoch" (a `u64`) with
//!   "ms-as-duration" (also `u64`). A deadline plus a duration yields
//!   a new deadline; a deadline minus another deadline yields a
//!   duration. The arithmetic is encoded in the method signatures.
//! - Makes the portability intent explicit at type-checking time:
//!   call sites that still reach for `std::time::Instant::now()` will
//!   fail to compile against a `PortableInstant` field, surfacing
//!   the host-boundary violation.
//!
//! WASM portability: no `std::time::Instant`, no `SystemTime::now()`,
//! no `std::thread`. Pure `u64` arithmetic.

use serde::{Deserialize, Serialize};

/// Host-provided monotonic timestamp, measured in milliseconds from a
/// host-chosen origin.
///
/// The origin is typically the host-process start time. The runtime
/// treats the origin as opaque: two `PortableInstant`s from the same
/// host are comparable, but values from different hosts (or from the
/// same host across restarts) are not.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct PortableInstant(pub u64);

impl PortableInstant {
    /// The zero-instant (origin). Used as a sentinel for "never" in
    /// some deadline fields; prefer `Option<PortableInstant>` for
    /// optional deadlines rather than relying on this value.
    pub const ORIGIN: Self = Self(0);

    /// Raw milliseconds from origin.
    pub fn millis(self) -> u64 {
        self.0
    }

    /// Saturating elapsed-duration (in ms) between two instants.
    /// If `earlier > self`, returns `0` rather than panicking — this
    /// matches the common pattern of "has enough time passed?" checks
    /// where a monotonic clock temporarily returning non-monotonic
    /// values (clock adjustment, host bug) should degrade to "yes,
    /// fire now" rather than underflow.
    pub fn saturating_elapsed_since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }

    /// Produce a deadline `ms` milliseconds in the future, saturating
    /// at `u64::MAX` (effectively "never").
    pub fn saturating_add_ms(self, ms: u64) -> Self {
        Self(self.0.saturating_add(ms))
    }

    /// `true` if `self` is at or after `deadline`. Equivalent to
    /// `self >= deadline` but reads more naturally at call sites
    /// that ask "has the deadline fired?".
    pub fn has_reached(self, deadline: Self) -> bool {
        self >= deadline
    }
}

impl std::ops::Sub for PortableInstant {
    type Output = u64;

    /// Saturating subtraction — mirrors [`saturating_elapsed_since`] so
    /// `later - earlier` reads as "how much later".
    ///
    /// [`saturating_elapsed_since`]: Self::saturating_elapsed_since
    fn sub(self, rhs: Self) -> u64 {
        self.saturating_elapsed_since(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_zero() {
        assert_eq!(PortableInstant::ORIGIN.millis(), 0);
        assert_eq!(PortableInstant::default(), PortableInstant::ORIGIN);
    }

    #[test]
    fn saturating_elapsed_since_handles_reverse_order() {
        // Defensive: if the host clock ever appears non-monotonic
        // (clock adjustment, recovered snapshot), we return 0 rather
        // than underflow. Callers comparing "has deadline elapsed?"
        // read 0 as "yes", which degrades safely.
        let earlier = PortableInstant(100);
        let later = PortableInstant(250);
        assert_eq!(later.saturating_elapsed_since(earlier), 150);
        assert_eq!(earlier.saturating_elapsed_since(later), 0);
    }

    #[test]
    fn saturating_add_ms_saturates_at_max() {
        let big = PortableInstant(u64::MAX - 5);
        assert_eq!(big.saturating_add_ms(10), PortableInstant(u64::MAX));
    }

    #[test]
    fn has_reached_is_true_at_and_after_deadline() {
        let deadline = PortableInstant(500);
        assert!(!PortableInstant(499).has_reached(deadline));
        assert!(PortableInstant(500).has_reached(deadline));
        assert!(PortableInstant(501).has_reached(deadline));
    }

    #[test]
    fn sub_operator_mirrors_saturating_elapsed_since() {
        let a = PortableInstant(300);
        let b = PortableInstant(100);
        assert_eq!(a - b, 200);
        // Reverse underflow clamps to 0 — same as saturating_elapsed_since.
        assert_eq!(b - a, 0);
    }

    #[test]
    fn ordering_is_consistent_with_u64() {
        assert!(PortableInstant(5) < PortableInstant(10));
        assert!(PortableInstant(10) == PortableInstant(10));
        assert!(PortableInstant(15) > PortableInstant(10));
    }

    #[test]
    fn deadline_pattern_round_trip() {
        // The canonical usage: arm a deadline, then ask "has now
        // reached the deadline?". Pin the pattern.
        let now = PortableInstant(1_000);
        let debounce_ms = 75;
        let deadline = now.saturating_add_ms(debounce_ms);

        // Before the debounce window elapses: not fired.
        let fifty_ms_later = PortableInstant(1_050);
        assert!(!fifty_ms_later.has_reached(deadline));

        // After: fired.
        let eighty_ms_later = PortableInstant(1_080);
        assert!(eighty_ms_later.has_reached(deadline));
    }

    #[test]
    fn serde_round_trip_preserves_value() {
        // Deadlines may be persisted in snapshots; pin the wire shape.
        let original = PortableInstant(1_234_567_890);
        let json = serde_json::to_string(&original).unwrap();
        // Serializes as a bare u64, not a struct wrapper.
        assert_eq!(json, "1234567890");
        let decoded: PortableInstant = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }
}
