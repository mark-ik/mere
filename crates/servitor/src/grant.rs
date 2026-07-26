//! Scoped structural capabilities and the authority seam.
//!
//! A [`Grant`] is a structural capability: a [`Subject`] may act under a
//! [`Cap`] with a [`Mode`]. This is the layer-1 structural cap of the designed
//! three-layer stack (structural cap + policy facts + group-key). Coverage is
//! the partial order the capability type owns (see [`crate::cap`]), never a
//! string test here. The [`AuthorityProvider`] is the replaceable read
//! boundary that answers coverage, so a meadowcap-shaped provider over
//! graph-cluster-derived namespaces can replace [`GrantTable`] without the
//! gate changing.

use crate::cap::{Cap, Capability};
use crate::Subject;

/// Access mode, in the meadowcap sense. Ordered by power: a `Write` grant
/// covers a `Read` need, and `Delegate` covers both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    /// May observe the covered area.
    Read,
    /// May mutate the covered area (implies [`Read`](Mode::Read)).
    Write,
    /// May grant covered capabilities to others (implies [`Write`](Mode::Write)).
    Delegate,
}

impl Mode {
    /// Whether a grant of `self` satisfies a `needed` mode.
    pub fn covers(self, needed: Mode) -> bool {
        self >= needed
    }
}

/// A structural capability: `subject` may act under `cap` with `mode`.
/// Coverage delegates to the capability's own order, so what "covers" means
/// is the capability type's business and cannot drift per call site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    /// The keyholder this capability is granted to.
    pub subject: Subject,
    /// The capability granted.
    pub cap: Cap,
    /// The access mode granted.
    pub mode: Mode,
    /// When this grant stops covering anything, in unix milliseconds. `None`
    /// is open-ended. Expiry is evaluated against the authority's host-set
    /// clock (see [`GrantTable::set_now`]), never a clock this crate reads:
    /// servitor stays portable, wasm-safe, and deterministic in tests.
    pub expires_at_ms: Option<u64>,
}

impl Grant {
    /// An open-ended grant of `mode` to `subject` over `cap`.
    pub fn new(subject: Subject, cap: Cap, mode: Mode) -> Self {
        Self {
            subject,
            cap,
            mode,
            expires_at_ms: None,
        }
    }

    /// The same grant, bounded: it stops covering at `expires_at_ms`.
    pub fn expiring_at(mut self, expires_at_ms: u64) -> Self {
        self.expires_at_ms = Some(expires_at_ms);
        self
    }

    /// Whether this grant has expired at `now_ms`. An open-ended grant never
    /// has; a bounded one expires AT its bound (the instant named is the
    /// first at which it no longer covers).
    pub fn expired_at(&self, now_ms: u64) -> bool {
        self.expires_at_ms.is_some_and(|at| now_ms >= at)
    }

    /// Whether this grant covers `needed` at `mode` as of `now_ms`: same
    /// subject, unexpired, the held capability covers the needed one, and the
    /// mode is sufficient.
    pub fn covers_at(&self, now_ms: u64, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        !self.expired_at(now_ms) && self.covers(subject, needed, mode)
    }

    /// Whether this grant covers `needed` at `mode`, ignoring expiry. Callers
    /// that have a clock want [`covers_at`](Self::covers_at).
    pub fn covers(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        self.subject == subject && self.mode.covers(mode) && self.cap.covers(needed)
    }
}

/// The replaceable authority read boundary: given a subject and the path a
/// petition claims to act under, does the subject hold a covering capability?
///
/// Mirrors `gemot::MootAuthorizationProvider`. The gate depends on this trait,
/// never on a concrete grant store, so authority can come from a local grant
/// table now and a meadowcap-shaped structural-cap provider (over
/// graph-cluster-derived namespaces, layered with tessera policy facts) later,
/// with no gate change.
pub trait AuthorityProvider {
    /// Whether `subject` may act under `needed` at `mode`.
    fn covers(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool;
}

/// The minimal provider: a flat table of grants, coverage answered by each
/// grant's capability order. The stand-in until the structural-cap layer is
/// built. (Was `PrefixAuthority` before capabilities became typed; the name
/// described the matching rule, which is no longer this type's business.)
#[derive(Clone, Debug, Default)]
pub struct GrantTable {
    grants: Vec<Grant>,
    /// The host-set clock expiry is judged against. Zero (the default) means
    /// "the epoch", under which no bounded grant has expired yet; a host that
    /// issues bounded grants must call [`set_now`](Self::set_now).
    now_ms: u64,
}

impl GrantTable {
    /// An empty authority table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a grant to the table, returning `self` for chaining.
    pub fn with_grant(mut self, grant: Grant) -> Self {
        self.grants.push(grant);
        self
    }

    /// Add a grant to the table in place.
    pub fn grant(&mut self, grant: Grant) {
        self.grants.push(grant);
    }

    /// The grants held (for projection into a denizen's nested graph).
    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }

    /// Set the clock expiry is judged against, in unix milliseconds. The host
    /// owns time; this crate never reads a clock of its own.
    ///
    /// Staleness window: a grant that expires between two calls keeps
    /// answering until the next one, so a host must tick this at every moment
    /// authority is consulted (turnstone does it per denizen run).
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// The clock currently in force.
    pub fn now(&self) -> u64 {
        self.now_ms
    }
}

impl AuthorityProvider for GrantTable {
    fn covers(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        self.grants
            .iter()
            .any(|grant| grant.covers_at(self.now_ms, subject, needed, mode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(tag: u8) -> Subject {
        Subject::new([tag; 32])
    }

    #[test]
    fn write_covers_read_but_not_the_reverse() {
        assert!(Mode::Write.covers(Mode::Read));
        assert!(Mode::Delegate.covers(Mode::Write));
        assert!(!Mode::Read.covers(Mode::Write));
    }

    fn scope(raw: &str) -> Cap {
        Cap::scope(raw).unwrap()
    }

    #[test]
    fn a_grant_covers_only_its_subject_capability_and_sufficient_mode() {
        let grant = Grant::new(subject(1), scope("trail"), Mode::Write);
        assert!(grant.covers(subject(1), &scope("trail/step1"), Mode::Write));
        assert!(grant.covers(subject(1), &scope("trail/step1"), Mode::Read));
        assert!(!grant.covers(subject(1), &scope("other/x"), Mode::Write), "outside the scope");
        assert!(!grant.covers(subject(2), &scope("trail/step1"), Mode::Write), "wrong subject");
        assert!(
            !grant.covers(subject(1), &scope("trail/step1"), Mode::Delegate),
            "insufficient mode"
        );
    }

    #[test]
    fn the_grant_table_answers_over_its_grants() {
        let authority = GrantTable::new()
            .with_grant(Grant::new(subject(1), scope("trail"), Mode::Write))
            .with_grant(Grant::new(subject(2), scope("notes"), Mode::Read));
        assert!(authority.covers(subject(1), &scope("trail/step1"), Mode::Write));
        assert!(authority.covers(subject(2), &scope("notes/a"), Mode::Read));
        assert!(!authority.covers(subject(2), &scope("notes/a"), Mode::Write), "read-only");
        assert!(!authority.covers(subject(1), &scope("notes/a"), Mode::Read), "no grant there");
    }

    #[test]
    fn a_bounded_grant_stops_covering_when_the_host_clock_passes_it() {
        let held = Grant::new(subject(1), scope("trail"), Mode::Write).expiring_at(1_000);
        let mut authority = GrantTable::new().with_grant(held);

        authority.set_now(999);
        assert!(authority.covers(subject(1), &scope("trail/step"), Mode::Write));
        // Expiry bites AT the named instant, and needs no mutation of the
        // store: the same table simply stops answering.
        authority.set_now(1_000);
        assert!(!authority.covers(subject(1), &scope("trail/step"), Mode::Write));
        authority.set_now(10_000);
        assert!(!authority.covers(subject(1), &scope("trail/step"), Mode::Write));
    }

    #[test]
    fn an_open_ended_grant_never_expires() {
        let mut authority =
            GrantTable::new().with_grant(Grant::new(subject(1), scope("trail"), Mode::Write));
        authority.set_now(u64::MAX);
        assert!(authority.covers(subject(1), &scope("trail/step"), Mode::Write));
    }

    #[test]
    fn a_power_grant_never_widens_when_a_new_power_appears() {
        // The hazard the typed capability exists to kill, at the authority
        // level: a denizen granted `navigate` gains nothing when a later build
        // adds a power whose name extends it.
        let authority = GrantTable::new()
            .with_grant(Grant::new(subject(1), Cap::power("navigate").unwrap(), Mode::Write));
        assert!(authority.covers(subject(1), &Cap::power("navigate").unwrap(), Mode::Write));
        assert!(!authority.covers(subject(1), &Cap::power("navigate-admin").unwrap(), Mode::Write));
        assert!(
            !authority.covers(subject(1), &scope("navigate"), Mode::Write),
            "a scope spelled like the power is not the power"
        );
    }
}
