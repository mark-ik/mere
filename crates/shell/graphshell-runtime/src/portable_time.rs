/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Process-monotonic clock adapter producing
//! [`graphshell_core::time::PortableInstant`] values.
//!
//! 2026-04-25 graphshell-runtime extraction: relocated from
//! `shell/desktop/ui/portable_time.rs`. Both host adapters (egui,
//! iced) on desktop want the same monotonic-clock anchoring, so the
//! shim moved into the runtime crate where it can be shared without
//! either host owning it. Future wasm/mobile hosts that need a
//! different time source can introduce a feature-gated alternative
//! here without touching call sites.
//!
//! Runtime state (omnibar mailbox deadlines, focus-ring animation
//! anchors, etc.) stores times as `PortableInstant` — milliseconds
//! from an app-chosen origin. On desktop we materialise that value
//! from `std::time::Instant`, anchored to the first call so elapsed
//! duration is measured against application start.
//!
//! The anchor is per-process, wrapped in a `OnceLock<Instant>` so
//! first-caller semantics are well-defined without requiring explicit
//! initialisation. Tests that care about absolute values can freeze a
//! starting anchor by calling [`portable_now()`] once before the code
//! under test.

use std::sync::OnceLock;
use std::time::Instant;

use graphshell_core::time::PortableInstant;

static APP_START: OnceLock<Instant> = OnceLock::new();

/// Return the current monotonic time as a [`PortableInstant`],
/// measured in milliseconds from the first call to this function in
/// the process lifetime.
pub fn portable_now() -> PortableInstant {
    let anchor = APP_START.get_or_init(Instant::now);
    let elapsed_ms = anchor.elapsed().as_millis();
    // Saturate at u64::MAX — a process running for 584+ million years
    // is not a supported configuration. The saturation keeps the
    // ordering comparison `now >= deadline` well-defined even under
    // pathological clocks rather than wrapping silently.
    PortableInstant(u64::try_from(elapsed_ms).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_now_is_monotonic_non_decreasing() {
        let a = portable_now();
        let b = portable_now();
        assert!(b.0 >= a.0);
    }

    #[test]
    fn portable_now_advances_with_time() {
        let a = portable_now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = portable_now();
        assert!(b.0 >= a.0);
    }
}
