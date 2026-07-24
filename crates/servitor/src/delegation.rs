//! Delegation: the one mechanism behind install, expiry, and revocation.
//!
//! The capability-model round's D3 ruling (2026-07-24): the user is not an
//! implicit infinite authority that "writes some grants" — the user is a
//! **root subject**, and everything else is delegation from it.
//!
//! - The visible install review is an **attenuating delegation** (user →
//!   denizen): the denizen receives a strict subset of what the user holds.
//! - **Expiry** is a delegation with a bound ([`Grant::expires_at_ms`]).
//! - **Revocation** is severing a delegation, and a sever cascades: a
//!   descendant whose ancestor is gone is invalid by construction, evaluated
//!   lazily on read, with no marking pass.
//!
//! A [`Delegation`] confers one [`Grant`] from `from` to the grant's subject.
//! Validity is an unbroken chain of attenuating links to a root delegation
//! issued by the table's root subject. The three invariants (D3) are checked
//! at issue AND re-checked at verify, because a stored chain is never trusted:
//!
//! 1. **attenuation** — the parent's capability covers the child's;
//! 2. **delegability** — the parent held [`Mode::Delegate`] (only a delegable
//!    grant can be delegated onward);
//! 3. **expiry** — a child cannot outlive its parent (falls out of chain
//!    validity: an expired ancestor invalidates the whole subtree).
//!
//! Servitor stays identity-agnostic: the root is just a [`Subject`]. Whether
//! it is a test key or the active personae identity is the host's fact
//! (merecat roots the table on the user's personae public key, OQ2).

use std::collections::HashSet;

use crate::cap::{Cap, Capability};
use crate::grant::{AuthorityProvider, Grant, Mode};
use crate::Subject;

/// A delegation's stable identity. Host-minted (servitor mints nothing, to
/// stay deterministic): merecat derives it from the delegation's content, a
/// test supplies a literal.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DelegationId(pub String);

impl DelegationId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One link in a delegation chain: `from` confers `grant` (whose subject is
/// the delegate). A root link has no `parent` and must be issued by the
/// table's root subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delegation {
    pub id: DelegationId,
    /// The delegation this one narrows, or `None` for a root delegation.
    pub parent: Option<DelegationId>,
    /// Who issued this delegation.
    pub from: Subject,
    /// The conferred capability; `grant.subject` is the delegate.
    pub grant: Grant,
}

impl Delegation {
    /// A root delegation: the user (`from`, the table's root) confers `grant`
    /// directly to a denizen.
    pub fn root(id: impl Into<String>, from: Subject, grant: Grant) -> Self {
        Self {
            id: DelegationId::new(id),
            parent: None,
            from,
            grant,
        }
    }

    /// A child delegation: `from` (who must hold `parent` at
    /// [`Mode::Delegate`]) confers a narrower `grant` onward.
    pub fn child(
        id: impl Into<String>,
        parent: &DelegationId,
        from: Subject,
        grant: Grant,
    ) -> Self {
        Self {
            id: DelegationId::new(id),
            parent: Some(parent.clone()),
            from,
            grant,
        }
    }

    /// The delegate this confers to.
    pub fn to(&self) -> Subject {
        self.grant.subject
    }
}

/// Why a delegation could not be issued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DelegationError {
    /// A child named a parent the table does not hold.
    UnknownParent(DelegationId),
    /// A root delegation was issued by a subject other than the table's root.
    NotRoot,
    /// The child's capability is not covered by the parent's (widening).
    NotAttenuating {
        /// The parent's capability.
        parent: Cap,
        /// The child's (wider) capability.
        child: Cap,
    },
    /// The parent's holder does not hold [`Mode::Delegate`], so it cannot
    /// delegate at all.
    NotDelegable,
    /// The child's mode exceeds the parent's.
    ModeExceedsParent,
    /// The child would outlive its parent (a later or absent expiry).
    OutlivesParent,
    /// `from` on a child does not match whom the parent delegated to.
    WrongDelegator,
    /// An id already in the table.
    DuplicateId(DelegationId),
}

/// A set of delegations rooted at one subject, answering coverage by chain
/// validity. Implements [`AuthorityProvider`], so it drops into the gate
/// exactly where [`GrantTable`](crate::GrantTable) does.
#[derive(Clone, Debug)]
pub struct DelegationTable {
    /// The ceiling: a root delegation is valid only if issued by this subject.
    root: Subject,
    /// The host-set clock (servitor reads no clock of its own).
    now_ms: u64,
    delegations: Vec<Delegation>,
}

impl DelegationTable {
    /// A table whose root delegations must come from `root` (merecat: the
    /// user's personae subject).
    pub fn new(root: Subject) -> Self {
        Self {
            root,
            now_ms: 0,
            delegations: Vec::new(),
        }
    }

    /// The root subject this table trusts.
    pub fn root(&self) -> Subject {
        self.root
    }

    /// Set the clock expiry is judged against (see [`GrantTable::set_now`]).
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// The clock in force.
    pub fn now(&self) -> u64 {
        self.now_ms
    }

    /// The delegations held, in issue order.
    pub fn delegations(&self) -> &[Delegation] {
        &self.delegations
    }

    fn get(&self, id: &DelegationId) -> Option<&Delegation> {
        self.delegations.iter().find(|d| &d.id == id)
    }

    /// Check that `delegation` attenuates its parent (D3's issue-time rules).
    /// Pure, so [`issue`](Self::issue) and a rebuild-time verify share it.
    fn check_attenuation(&self, delegation: &Delegation) -> Result<(), DelegationError> {
        match &delegation.parent {
            None => {
                if delegation.from != self.root {
                    return Err(DelegationError::NotRoot);
                }
            }
            Some(parent_id) => {
                let parent = self
                    .get(parent_id)
                    .ok_or_else(|| DelegationError::UnknownParent(parent_id.clone()))?;
                if parent.to() != delegation.from {
                    return Err(DelegationError::WrongDelegator);
                }
                if parent.grant.mode != Mode::Delegate {
                    return Err(DelegationError::NotDelegable);
                }
                if !parent.grant.cap.covers(&delegation.grant.cap) {
                    return Err(DelegationError::NotAttenuating {
                        parent: parent.grant.cap.clone(),
                        child: delegation.grant.cap.clone(),
                    });
                }
                if !parent.grant.mode.covers(delegation.grant.mode) {
                    return Err(DelegationError::ModeExceedsParent);
                }
                if !expiry_within(delegation.grant.expires_at_ms, parent.grant.expires_at_ms) {
                    return Err(DelegationError::OutlivesParent);
                }
            }
        }
        Ok(())
    }

    /// Issue a delegation, refusing any widening (D3). The parent, if named,
    /// must already be in the table.
    pub fn issue(&mut self, delegation: Delegation) -> Result<(), DelegationError> {
        if self.get(&delegation.id).is_some() {
            return Err(DelegationError::DuplicateId(delegation.id.clone()));
        }
        self.check_attenuation(&delegation)?;
        self.delegations.push(delegation);
        Ok(())
    }

    /// Insert a delegation read back from storage WITHOUT the issue-time
    /// attenuation check — a cold rebuild replays what was already validated,
    /// and [`is_valid`](Self::is_valid) re-checks the chain at read time
    /// regardless. Order matters only in that a parent should precede its
    /// child for the eventual lookups; validity does not depend on order.
    pub fn adopt(&mut self, delegation: Delegation) {
        self.delegations.push(delegation);
    }

    /// Whether `delegation` is valid now: unexpired, attenuating, and chained
    /// to a root the table trusts. Lazily evaluated, so a severed ancestor
    /// invalidates the whole subtree with no marking pass. Cycle-guarded.
    pub fn is_valid(&self, delegation: &Delegation) -> bool {
        let mut seen = HashSet::new();
        self.valid_inner(delegation, &mut seen)
    }

    fn valid_inner<'a>(
        &'a self,
        delegation: &'a Delegation,
        seen: &mut HashSet<&'a DelegationId>,
    ) -> bool {
        if !seen.insert(&delegation.id) {
            return false; // a cycle in stored data is not a valid chain
        }
        if delegation.grant.expired_at(self.now_ms) {
            return false;
        }
        match &delegation.parent {
            None => delegation.from == self.root,
            Some(parent_id) => {
                let Some(parent) = self.get(parent_id) else {
                    return false; // severed / absent -> cascade
                };
                parent.to() == delegation.from
                    && parent.grant.mode == Mode::Delegate
                    && parent.grant.cap.covers(&delegation.grant.cap)
                    && parent.grant.mode.covers(delegation.grant.mode)
                    && expiry_within(delegation.grant.expires_at_ms, parent.grant.expires_at_ms)
                    && self.valid_inner(parent, seen)
            }
        }
    }

    /// Sever one delegation by id, returning whether it was present. Its
    /// descendants are not touched — they simply stop being valid, because
    /// their chain to the root is now broken (lazy cascade).
    pub fn sever(&mut self, id: &DelegationId) -> bool {
        let before = self.delegations.len();
        self.delegations.retain(|d| &d.id != id);
        self.delegations.len() != before
    }

    /// Revoke everything the ROOT granted `subject` directly: sever every root
    /// delegation conferring to `subject`. Onward delegations `subject` made
    /// cascade. Returns how many root delegations were severed. This is
    /// uninstall, and the "replace" half of replace-and-cascade (OQ1).
    pub fn revoke_root_grants(&mut self, subject: Subject) -> usize {
        let before = self.delegations.len();
        self.delegations
            .retain(|d| !(d.parent.is_none() && d.to() == subject));
        before - self.delegations.len()
    }
}

impl AuthorityProvider for DelegationTable {
    fn covers(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        self.delegations.iter().any(|d| {
            d.to() == subject
                && d.grant.cap.covers(needed)
                && d.grant.mode.covers(mode)
                && self.is_valid(d)
        })
    }
}

/// Whether `child` expiry is no later than `parent` expiry. `None` is
/// open-ended (the latest possible), so a child may not be open-ended unless
/// its parent is, and a bounded child must end at or before its parent.
fn expiry_within(child: Option<u64>, parent: Option<u64>) -> bool {
    match (child, parent) {
        (_, None) => true,             // an open-ended parent bounds nothing
        (None, Some(_)) => false,      // an open-ended child outlives a bounded parent
        (Some(c), Some(p)) => c <= p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(tag: u8) -> Subject {
        Subject::new([tag; 32])
    }

    const ROOT: u8 = 0;
    const HELPER: u8 = 1;
    const SUB: u8 = 2;

    fn scope(raw: &str) -> Cap {
        Cap::scope(raw).unwrap()
    }

    /// A table rooted at ROOT, with the user delegating `trail` at `mode` to
    /// the helper.
    fn table_with_root_grant(mode: Mode) -> DelegationTable {
        let mut table = DelegationTable::new(subject(ROOT));
        table
            .issue(Delegation::root(
                "d-helper",
                subject(ROOT),
                Grant::new(subject(HELPER), scope("trail"), mode),
            ))
            .unwrap();
        table
    }

    #[test]
    fn a_root_delegation_covers_within_its_scope() {
        let table = table_with_root_grant(Mode::Write);
        assert!(table.covers(subject(HELPER), &scope("trail/step"), Mode::Write));
        assert!(!table.covers(subject(HELPER), &scope("notes"), Mode::Write));
        assert!(!table.covers(subject(SUB), &scope("trail/step"), Mode::Write), "not the delegate");
    }

    #[test]
    fn only_the_root_may_issue_a_root_delegation() {
        let mut table = DelegationTable::new(subject(ROOT));
        let err = table
            .issue(Delegation::root(
                "forged",
                subject(HELPER), // not the root
                Grant::new(subject(SUB), scope("trail"), Mode::Write),
            ))
            .unwrap_err();
        assert_eq!(err, DelegationError::NotRoot);
    }

    #[test]
    fn a_child_narrows_to_a_sub_scope() {
        // The user gives the helper `trail` at Delegate, so it can sub-delegate.
        let mut table = table_with_root_grant(Mode::Delegate);
        table
            .issue(Delegation::child(
                "d-sub",
                &DelegationId::new("d-helper"),
                subject(HELPER),
                Grant::new(subject(SUB), scope("trail/step"), Mode::Write),
            ))
            .unwrap();
        assert!(table.covers(subject(SUB), &scope("trail/step/a"), Mode::Write));
        assert!(
            !table.covers(subject(SUB), &scope("trail/other"), Mode::Write),
            "outside the narrower scope"
        );
    }

    #[test]
    fn widening_the_capability_is_refused() {
        let mut table = table_with_root_grant(Mode::Delegate);
        let err = table
            .issue(Delegation::child(
                "d-wide",
                &DelegationId::new("d-helper"),
                subject(HELPER),
                // `trail` covers `trail/step`, not the other way: the root
                // scope would be wider than what the helper holds.
                Grant::new(subject(SUB), Cap::root_scope(), Mode::Write),
            ))
            .unwrap_err();
        assert!(matches!(err, DelegationError::NotAttenuating { .. }), "{err:?}");
    }

    #[test]
    fn a_write_only_holder_cannot_delegate_at_all() {
        // The helper holds Write, not Delegate: it may act, never grant.
        let mut table = table_with_root_grant(Mode::Write);
        let err = table
            .issue(Delegation::child(
                "d-sub",
                &DelegationId::new("d-helper"),
                subject(HELPER),
                Grant::new(subject(SUB), scope("trail/step"), Mode::Read),
            ))
            .unwrap_err();
        assert_eq!(err, DelegationError::NotDelegable);
    }

    #[test]
    fn a_child_may_not_outlive_its_parent() {
        let mut table = DelegationTable::new(subject(ROOT));
        table
            .issue(Delegation::root(
                "d-helper",
                subject(ROOT),
                Grant::new(subject(HELPER), scope("trail"), Mode::Delegate).expiring_at(1_000),
            ))
            .unwrap();
        // Open-ended child under a bounded parent: refused.
        let err = table
            .issue(Delegation::child(
                "d-open",
                &DelegationId::new("d-helper"),
                subject(HELPER),
                Grant::new(subject(SUB), scope("trail/step"), Mode::Read),
            ))
            .unwrap_err();
        assert_eq!(err, DelegationError::OutlivesParent);
        // A child ending at or before the parent is fine.
        table
            .issue(Delegation::child(
                "d-bounded",
                &DelegationId::new("d-helper"),
                subject(HELPER),
                Grant::new(subject(SUB), scope("trail/step"), Mode::Read).expiring_at(900),
            ))
            .unwrap();
    }

    #[test]
    fn severing_a_link_cascades_to_the_whole_subtree() {
        let mut table = table_with_root_grant(Mode::Delegate);
        table
            .issue(Delegation::child(
                "d-sub",
                &DelegationId::new("d-helper"),
                subject(HELPER),
                Grant::new(subject(SUB), scope("trail/step"), Mode::Write),
            ))
            .unwrap();
        assert!(table.covers(subject(HELPER), &scope("trail/x"), Mode::Write));
        assert!(table.covers(subject(SUB), &scope("trail/step/y"), Mode::Write));

        // Sever the ROOT link: both the helper AND its sub-delegate lose
        // authority, though only one link was removed.
        assert!(table.sever(&DelegationId::new("d-helper")));
        assert!(!table.covers(subject(HELPER), &scope("trail/x"), Mode::Write), "the helper is revoked");
        assert!(
            !table.covers(subject(SUB), &scope("trail/step/y"), Mode::Write),
            "and the whole subtree cascades, with no second sever"
        );
    }

    #[test]
    fn revoke_root_grants_is_uninstall_and_cascades() {
        let mut table = table_with_root_grant(Mode::Delegate);
        table
            .issue(Delegation::child(
                "d-sub",
                &DelegationId::new("d-helper"),
                subject(HELPER),
                Grant::new(subject(SUB), scope("trail/step"), Mode::Write),
            ))
            .unwrap();

        let severed = table.revoke_root_grants(subject(HELPER));
        assert_eq!(severed, 1, "one root delegation removed");
        assert!(!table.covers(subject(HELPER), &scope("trail/x"), Mode::Write));
        assert!(!table.covers(subject(SUB), &scope("trail/step/y"), Mode::Write), "the sub cascaded");
    }

    #[test]
    fn a_bounded_chain_expires_at_its_earliest_link() {
        let mut table = DelegationTable::new(subject(ROOT));
        table
            .issue(Delegation::root(
                "d-helper",
                subject(ROOT),
                Grant::new(subject(HELPER), scope("trail"), Mode::Delegate).expiring_at(1_000),
            ))
            .unwrap();
        table
            .issue(Delegation::child(
                "d-sub",
                &DelegationId::new("d-helper"),
                subject(HELPER),
                Grant::new(subject(SUB), scope("trail/step"), Mode::Read).expiring_at(900),
            ))
            .unwrap();

        table.set_now(899);
        assert!(table.covers(subject(SUB), &scope("trail/step"), Mode::Read));
        // The child's own bound bites first.
        table.set_now(900);
        assert!(!table.covers(subject(SUB), &scope("trail/step"), Mode::Read));
        // The helper still stands until the parent bound.
        assert!(table.covers(subject(HELPER), &scope("trail/x"), Mode::Read));
        // Past the parent bound, the parent falls and the child would too.
        table.set_now(1_000);
        assert!(!table.covers(subject(HELPER), &scope("trail/x"), Mode::Read));
    }

    #[test]
    fn a_chain_verifies_from_a_cold_store_via_adopt() {
        // Rebuild: delegations arrive already-validated, out of issue order,
        // and validity is re-derived without the issue-time gate.
        let mut table = DelegationTable::new(subject(ROOT));
        table.adopt(Delegation::child(
            "d-sub",
            &DelegationId::new("d-helper"),
            subject(HELPER),
            Grant::new(subject(SUB), scope("trail/step"), Mode::Write),
        ));
        table.adopt(Delegation::root(
            "d-helper",
            subject(ROOT),
            Grant::new(subject(HELPER), scope("trail"), Mode::Delegate),
        ));
        assert!(table.covers(subject(SUB), &scope("trail/step/y"), Mode::Write));

        // A forged cold record (child claiming a parent that grants less) is
        // rejected by the read-time re-check, not trusted because it is stored.
        let mut forged = DelegationTable::new(subject(ROOT));
        forged.adopt(Delegation::root(
            "d-narrow",
            subject(ROOT),
            Grant::new(subject(HELPER), scope("trail/step"), Mode::Delegate),
        ));
        forged.adopt(Delegation::child(
            "d-forged",
            &DelegationId::new("d-narrow"),
            subject(HELPER),
            Grant::new(subject(SUB), scope("trail"), Mode::Write), // wider than the parent
        ));
        assert!(
            !forged.covers(subject(SUB), &scope("trail/other"), Mode::Write),
            "a stored chain is re-verified, never trusted"
        );
    }

    #[test]
    fn a_cycle_in_stored_data_is_not_a_valid_chain() {
        let mut table = DelegationTable::new(subject(ROOT));
        // Two children naming each other as parent: no root, so no validity.
        table.adopt(Delegation {
            id: DelegationId::new("a"),
            parent: Some(DelegationId::new("b")),
            from: subject(HELPER),
            grant: Grant::new(subject(HELPER), scope("trail"), Mode::Delegate),
        });
        table.adopt(Delegation {
            id: DelegationId::new("b"),
            parent: Some(DelegationId::new("a")),
            from: subject(HELPER),
            grant: Grant::new(subject(HELPER), scope("trail"), Mode::Delegate),
        });
        assert!(!table.covers(subject(HELPER), &scope("trail"), Mode::Write), "a cycle terminates false");
    }
}
