//! Generation counters for backpressure.
//!
//! Async work can arrive stale: a scene built for an old destination or an old
//! size lands after the surface navigated or resized. The kernel stamps outgoing
//! work with the current [`Generations`] and drops returning work whose stamp no
//! longer matches.

/// A process-local identity for one command or action request.
///
/// The host stamps work before it crosses an actor boundary and copies the ID
/// onto every progress or terminal update. Interpretation stays with the host:
/// Armillary correlates messages but does not define applied/rejected states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub u64);

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Monotonic request-ID source owned by a host boundary.
#[derive(Clone, Debug, Default)]
pub struct RequestIds {
    next: u64,
}

impl RequestIds {
    /// Issue the next non-zero request ID.
    pub fn issue(&mut self) -> RequestId {
        self.next = self.next.wrapping_add(1);
        if self.next == 0 {
            self.next = 1;
        }
        RequestId(self.next)
    }
}

/// A command update paired with the request that caused it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correlated<T> {
    pub request: RequestId,
    pub value: T,
}

impl<T> Correlated<T> {
    pub fn new(request: RequestId, value: T) -> Self {
        Self { request, value }
    }

    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Correlated<U> {
        Correlated {
            request: self.request,
            value: map(self.value),
        }
    }
}

/// A monotonic per-surface **navigation** generation. The kernel bumps it on every
/// navigate; a scene or input tagged with an older value is from a page the surface
/// already left, and is dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NavGeneration(pub u64);

/// A monotonic per-surface **viewport** generation. Bumped on resize; work built at
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

/// The current `(nav, viewport)` pair the kernel holds per surface. Stamp outgoing
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

        current.nav.bump(); // the surface navigated
        assert!(
            !current.accepts(stamp),
            "a scene from before the navigation is stale"
        );

        let after_nav = current;
        assert!(current.accepts(after_nav));
        current.viewport.bump(); // and then it resized
        assert!(
            !current.accepts(after_nav),
            "a scene from before the resize is stale"
        );
    }

    #[test]
    fn request_ids_correlate_updates_without_defining_their_outcome() {
        let mut ids = RequestIds::default();
        let first = ids.issue();
        let second = ids.issue();
        assert_eq!(first, RequestId(1));
        assert_eq!(second, RequestId(2));

        let update = Correlated::new(second, Ok::<_, &'static str>("saved"));
        let label = update.map(|result| result.unwrap().len());
        assert_eq!(label.request, second);
        assert_eq!(label.value, 5);
    }
}
