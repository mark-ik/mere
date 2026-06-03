/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Generation counters for backpressure.
//!
//! Async work can arrive stale: a scene built for an old URL or an old size lands
//! after the tile navigated or resized. The kernel stamps outgoing work with the
//! current [`Generations`] and drops returning work whose stamp no longer matches.
//! See the plan's "Backpressure and generations".

/// A monotonic per-tile **navigation** generation. The kernel bumps it on every
/// navigate; a scene or input tagged with an older value is from a page the tile
/// already left, and is dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NavGeneration(pub u64);

/// A monotonic per-tile **viewport** generation. Bumped on resize; work built at
/// an old size is dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewportGeneration(pub u64);

impl NavGeneration {
    /// Advance to the next generation and return it.
    pub fn bump(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

impl ViewportGeneration {
    /// Advance to the next generation and return it.
    pub fn bump(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

/// The current `(nav, viewport)` pair the kernel holds per tile. Stamp outgoing
/// commands with it; check returning scenes/input against it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Generations {
    pub nav: NavGeneration,
    pub viewport: ViewportGeneration,
}

impl Generations {
    /// Whether work stamped `stamp` is still current (neither generation has moved
    /// on). Stale work is dropped by the kernel rather than composited or
    /// delivered.
    pub fn accepts(&self, stamp: Generations) -> bool {
        stamp.nav == self.nav && stamp.viewport == self.viewport
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_advances_monotonically_and_returns_the_new_value() {
        let mut nav = NavGeneration::default();
        assert_eq!(nav, NavGeneration(0));
        assert_eq!(nav.bump(), NavGeneration(1));
        assert_eq!(nav.bump(), NavGeneration(2));
        assert_eq!(nav, NavGeneration(2), "bump mutates in place");
    }

    #[test]
    fn generations_reject_stale_work() {
        let mut current = Generations::default();
        let stamp = current; // work stamped at the current pair
        assert!(current.accepts(stamp), "fresh work is accepted");

        current.nav.bump(); // the tile navigated
        assert!(!current.accepts(stamp), "a scene from before the navigation is stale");

        let after_nav = current;
        assert!(current.accepts(after_nav));
        current.viewport.bump(); // and then it resized
        assert!(!current.accepts(after_nav), "a scene from before the resize is stale");
    }
}
