//! Delegation: the one mechanism behind install, expiry, and revocation —
//! **personae's signed certificates, viewed through the typed capability**.
//!
//! The capability-model round's D3 ruling (2026-07-24): the user is not an
//! implicit infinite authority that "writes some grants" — the user is a
//! **root subject**, and everything else is delegation from it. Install is an
//! attenuating delegation; expiry is a delegation with a bound; revocation is
//! severing one, cascading to its subtree.
//!
//! **This module owns none of that machinery.** `personae::delegation` already
//! provides the whole signed grammar — [`DelegationCertificate`] with
//! `attenuates` / `covers`, chain parents, expiry bounds, delegation depth,
//! content-addressed ids, `DelegationRevocation`, and the derived-key identity
//! proof beneath it — and gemot's moot store already consumes it. Servitor
//! contributes exactly one thing personae lacks: the **typed capability**
//! ([`Cap`], with its closed-set powers and hierarchical scopes) as a view over
//! personae's `(path_prefix, actions)` pair. One delegation system, two tiers.
//!
//! ## How a `Cap` becomes a personae scope
//!
//! personae's [`CapabilityScope`] is two-dimensional — *where*
//! (`path_prefix`, matched with a slash boundary) and *what* (`actions`, a set
//! attenuated by subset). A [`Cap`] encodes into the path, and [`Mode`] into
//! the actions:
//!
//! | servitor | personae `path_prefix` | personae `actions` |
//! | --- | --- | --- |
//! | `Cap::Power("navigate")` at `Write` | `power/navigate` | `{read, write}` |
//! | `Cap::Scope("scenario/a")` at `Read` | `scope/scenario/a` | `{read}` |
//!
//! Both halves of the capability order survive the encoding:
//!
//! - **powers stay closed** — personae's `path_covers` requires a `/`
//!   boundary, so `power/nav` does not cover `power/navigate`, and no power
//!   has anything beneath it;
//! - **scopes stay hierarchical** — `scope/scenario` covers
//!   `scope/scenario/a`, which is what a scope is for;
//! - **modes stay ordered** — a `Write` grant carries `{read, write}`, so
//!   subset attenuation reproduces `Write` covering `Read` without personae
//!   knowing the ordering.

use std::collections::{BTreeSet, HashMap, HashSet};

use identity::delegation::{
    CapabilityScope, DelegationCertificate, DelegationId, DelegationParent,
    SignedDelegationCertificate,
};

use crate::Subject;
use crate::cap::Cap;
use crate::grant::{AuthorityProvider, Mode};

/// The application family servitor's denizen capabilities live under, in
/// personae's `domain` dimension. Keeps denizen certificates from ever being
/// confused with a moot's or a mesh's.
pub const DENIZEN_DOMAIN: &str = "mere.denizen";

const POWER_PATH: &str = "power";
const SCOPE_PATH: &str = "scope";

/// The `path_prefix` a capability encodes to.
///
/// A power becomes `power/<name>`; because personae matches paths on a slash
/// boundary, a power covers only itself (nothing sits beneath it), which is
/// the closed-set semantics [`Cap::Power`] exists for. A scope becomes
/// `scope/<path>`, keeping its hierarchy.
pub fn cap_path(cap: &Cap) -> String {
    match cap {
        Cap::Power(name) => format!("{POWER_PATH}/{name}"),
        Cap::Scope(path) => {
            let path = path.to_string();
            if path.is_empty() {
                // The root scope: the `scope` segment itself, which covers
                // every `scope/...` beneath it.
                SCOPE_PATH.to_string()
            } else {
                format!("{SCOPE_PATH}/{path}")
            }
        }
    }
}

/// The action one mode is checked as.
pub fn mode_action(mode: Mode) -> &'static str {
    match mode {
        Mode::Read => "read",
        Mode::Write => "write",
        Mode::Delegate => "delegate",
    }
}

/// The action SET a mode confers: every mode it implies, so personae's subset
/// attenuation reproduces servitor's `Read < Write < Delegate` ordering
/// without personae having to know it.
pub fn mode_actions(mode: Mode) -> BTreeSet<String> {
    let mut actions = BTreeSet::new();
    actions.insert("read".to_string());
    if mode >= Mode::Write {
        actions.insert("write".to_string());
    }
    if mode >= Mode::Delegate {
        actions.insert("delegate".to_string());
    }
    actions
}

/// Build the personae scope for `cap` at `mode` over `resource` (the opaque
/// id of the governed space — a session graph, a denizen's world).
pub fn scope_for(cap: &Cap, mode: Mode, resource: Vec<u8>) -> CapabilityScope {
    CapabilityScope {
        domain: DENIZEN_DOMAIN.to_string(),
        resource,
        path_prefix: cap_path(cap),
        actions: mode_actions(mode),
    }
}

/// Why a chain did not verify. Reported rather than silently denied, so a
/// broken delegation is attributable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainError {
    /// The certificate's own signature or identity proof did not verify.
    BadSignature(DelegationId),
    /// A parent certificate the chain names is not held.
    MissingParent(DelegationId),
    /// A link does not attenuate its parent (widening, depth, or expiry).
    NotAttenuating(DelegationId),
    /// A root link names an authority other than this table's root.
    WrongRoot(DelegationId),
    /// The certificate, or one of its ancestors, is revoked.
    Revoked(DelegationId),
    /// The chain loops.
    Cycle(DelegationId),
}

/// A set of signed delegation certificates rooted at one authority, answering
/// coverage by verified chain validity. Implements [`AuthorityProvider`], so it
/// drops into the gate exactly where [`GrantTable`](crate::GrantTable) does.
///
/// Validity is evaluated on read, so a revocation cascades to a whole subtree
/// with no marking pass (D4's lazy cascade): a descendant whose ancestor is
/// revoked or missing simply stops verifying.
#[derive(Clone, Debug, Default)]
pub struct DelegationTable {
    /// The root authority id: a root certificate is valid only if it names
    /// this. In turnstone this is the user's personae master public key.
    root: [u8; 32],
    /// The host-set clock (servitor reads no clock of its own).
    now_ms: u64,
    certificates: HashMap<DelegationId, SignedDelegationCertificate>,
    revoked: HashSet<DelegationId>,
}

impl DelegationTable {
    /// A table whose root certificates must name `root` — turnstone passes the
    /// active personae identity's master public key (OQ2).
    pub fn new(root: [u8; 32]) -> Self {
        Self {
            root,
            now_ms: 0,
            certificates: HashMap::new(),
            revoked: HashSet::new(),
        }
    }

    /// The root authority this table trusts.
    pub fn root(&self) -> [u8; 32] {
        self.root
    }

    /// Set the clock expiry is judged against. personae's `covers` takes the
    /// evaluation time explicitly, so this is simply what servitor passes it.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// The clock in force.
    pub fn now(&self) -> u64 {
        self.now_ms
    }

    /// Adopt a certificate (a fresh issue, or one replayed from a cold store).
    /// Validity is re-derived on read, never trusted from storage.
    pub fn adopt(&mut self, signed: SignedDelegationCertificate) {
        self.certificates.insert(signed.certificate.id(), signed);
    }

    /// The certificates held.
    pub fn certificates(&self) -> impl Iterator<Item = &SignedDelegationCertificate> {
        self.certificates.values()
    }

    /// Look one up by id.
    pub fn get(&self, id: &DelegationId) -> Option<&SignedDelegationCertificate> {
        self.certificates.get(id)
    }

    /// Record a revocation. The certificate stays in the table (the record of
    /// what once was), but neither it nor anything beneath it verifies again.
    pub fn revoke(&mut self, id: DelegationId) -> bool {
        self.revoked.insert(id)
    }

    /// Whether `id` is revoked directly (not counting an ancestor).
    pub fn is_revoked(&self, id: &DelegationId) -> bool {
        self.revoked.contains(id)
    }

    /// Revoke every certificate the ROOT issued directly to `subject`:
    /// uninstall, and the "replace" half of replace-and-cascade (OQ1).
    /// Everything `subject` delegated onward cascades. Returns how many root
    /// certificates were revoked.
    pub fn revoke_root_grants(&mut self, subject: Subject) -> usize {
        let ids: Vec<DelegationId> = self
            .certificates
            .values()
            .filter(|signed| {
                signed.certificate.subject == subject.0
                    && matches!(signed.certificate.parent, DelegationParent::Root(_))
            })
            .map(|signed| signed.certificate.id())
            .collect();
        let mut revoked = 0;
        for id in ids {
            if self.revoke(id) {
                revoked += 1;
            }
        }
        revoked
    }

    /// Verify one certificate's whole chain: its own signature and identity
    /// proof, that each link attenuates its parent, that nothing on the path
    /// is revoked, and that it terminates at this table's root.
    pub fn verify_chain(&self, signed: &SignedDelegationCertificate) -> Result<(), ChainError> {
        let mut seen = HashSet::new();
        self.verify_inner(signed, &mut seen)
    }

    fn verify_inner(
        &self,
        signed: &SignedDelegationCertificate,
        seen: &mut HashSet<DelegationId>,
    ) -> Result<(), ChainError> {
        let id = signed.certificate.id();
        if !seen.insert(id) {
            return Err(ChainError::Cycle(id));
        }
        if self.revoked.contains(&id) {
            return Err(ChainError::Revoked(id));
        }
        // personae owns the cryptography: signature, derived-key attestation,
        // and issuer binding all verify here, not in servitor.
        if !signed.verify() {
            return Err(ChainError::BadSignature(id));
        }
        match signed.certificate.parent {
            DelegationParent::Root(root) => {
                if root == self.root {
                    Ok(())
                } else {
                    Err(ChainError::WrongRoot(id))
                }
            }
            DelegationParent::Certificate(parent_id) => {
                let parent = self
                    .certificates
                    .get(&parent_id)
                    .ok_or(ChainError::MissingParent(parent_id))?;
                // personae owns attenuation too: scope narrowing, action
                // subset, expiry containment, and delegation depth.
                if !signed.certificate.attenuates(&parent.certificate) {
                    return Err(ChainError::NotAttenuating(id));
                }
                self.verify_inner(parent, seen)
            }
        }
    }

    /// Whether `subject` holds `needed` at `mode` right now, by a verified,
    /// unrevoked chain to the root.
    fn covers_inner(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        let path = cap_path(needed);
        let action = mode_action(mode);
        self.certificates.values().any(|signed| {
            signed.certificate.subject == subject.0
                && signed.certificate.covers(&path, action, self.now_ms)
                && self.verify_chain(signed).is_ok()
        })
    }
}

impl AuthorityProvider for DelegationTable {
    fn covers(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        self.covers_inner(subject, needed, mode)
    }
}

/// Build a root delegation certificate: the user (`root_provider`'s identity)
/// conferring `cap` at `mode` over `resource` to `subject`. Sign it with
/// [`SignedDelegationCertificate::issue`].
///
/// `depth` is how many further delegation steps the subject may take — `0`
/// forbids sub-delegation entirely, which is the right default for an
/// installed helper.
#[allow(clippy::too_many_arguments)]
pub fn root_certificate(
    issuer: [u8; 32],
    subject: Subject,
    cap: &Cap,
    mode: Mode,
    resource: Vec<u8>,
    issued_at_ms: u64,
    expires_at_ms: Option<u64>,
    depth: u16,
    nonce: [u8; 32],
) -> DelegationCertificate {
    DelegationCertificate::new(
        DelegationParent::Root(issuer),
        issuer,
        subject.0,
        scope_for(cap, mode, resource),
        issued_at_ms,
        issued_at_ms,
        expires_at_ms,
        depth,
        nonce,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::{IdentityProvider, InMemoryProvider};

    fn provider(seed: u8) -> InMemoryProvider {
        InMemoryProvider::from_seed([seed; 32])
    }

    fn root_key(provider: &InMemoryProvider) -> [u8; 32] {
        provider.master_public_key().to_bytes()
    }

    fn scope(raw: &str) -> Cap {
        Cap::scope(raw).unwrap()
    }

    fn subject_of(provider: &InMemoryProvider) -> Subject {
        Subject::new(provider.master_public_key().to_bytes())
    }

    /// The user delegates `trail` at `mode` to the helper, with `depth`
    /// further steps allowed.
    fn rooted(mode: Mode, depth: u16) -> (DelegationTable, InMemoryProvider, InMemoryProvider) {
        let user = provider(0);
        let helper = provider(1);
        let mut table = DelegationTable::new(root_key(&user));
        let cert = root_certificate(
            root_key(&user),
            subject_of(&helper),
            &scope("trail"),
            mode,
            b"session-1".to_vec(),
            1_000,
            None,
            depth,
            [7; 32],
        );
        table.adopt(SignedDelegationCertificate::issue(&user, cert).unwrap());
        table.set_now(2_000);
        (table, user, helper)
    }

    #[test]
    fn a_root_delegation_covers_within_its_scope() {
        let (table, _user, helper) = rooted(Mode::Write, 0);
        let subject = subject_of(&helper);
        assert!(table.covers(subject, &scope("trail/step"), Mode::Write));
        assert!(
            table.covers(subject, &scope("trail/step"), Mode::Read),
            "write implies read"
        );
        assert!(
            !table.covers(subject, &scope("notes"), Mode::Write),
            "outside the scope"
        );
        assert!(
            !table.covers(subject, &scope("trail/step"), Mode::Delegate),
            "a Write grant confers no delegate action"
        );
    }

    #[test]
    fn powers_stay_closed_through_the_personae_encoding() {
        // The F1 hazard, checked at the ENCODED layer: personae's path match
        // requires a slash boundary, so `power/nav` cannot cover
        // `power/navigate`.
        let user = provider(0);
        let helper = provider(1);
        let mut table = DelegationTable::new(root_key(&user));
        let cert = root_certificate(
            root_key(&user),
            subject_of(&helper),
            &Cap::power("nav").unwrap(),
            Mode::Write,
            b"app".to_vec(),
            1_000,
            None,
            0,
            [9; 32],
        );
        table.adopt(SignedDelegationCertificate::issue(&user, cert).unwrap());
        table.set_now(2_000);

        let subject = subject_of(&helper);
        assert!(table.covers(subject, &Cap::power("nav").unwrap(), Mode::Write));
        assert!(
            !table.covers(subject, &Cap::power("navigate").unwrap(), Mode::Write),
            "a longer power name is a different power"
        );
        assert!(
            !table.covers(subject, &scope("nav"), Mode::Write),
            "a scope spelled like the power is not the power"
        );
    }

    #[test]
    fn a_forged_root_is_refused() {
        // A certificate signed by someone who is not this table's root.
        let user = provider(0);
        let intruder = provider(5);
        let helper = provider(1);
        let mut table = DelegationTable::new(root_key(&user));
        let cert = root_certificate(
            root_key(&intruder),
            subject_of(&helper),
            &scope("trail"),
            Mode::Write,
            b"session-1".to_vec(),
            1_000,
            None,
            0,
            [3; 32],
        );
        let signed = SignedDelegationCertificate::issue(&intruder, cert).unwrap();
        assert!(signed.verify(), "it is a validly SIGNED certificate");
        table.adopt(signed);
        table.set_now(2_000);
        assert!(
            !table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write),
            "signed by the wrong authority: not this table's root"
        );
    }

    #[test]
    fn a_child_narrows_and_a_widening_child_never_verifies() {
        // The helper holds `trail` at Delegate with one step available.
        let (mut table, _user, helper) = rooted(Mode::Delegate, 1);
        let sub = provider(2);
        let parent_id = table.certificates().next().unwrap().certificate.id();

        // A strict narrowing verifies.
        let narrow = DelegationCertificate::new(
            DelegationParent::Certificate(parent_id),
            helper.master_public_key().to_bytes(),
            subject_of(&sub).0,
            scope_for(&scope("trail/step"), Mode::Write, b"session-1".to_vec()),
            1_000,
            1_000,
            None,
            0,
            [11; 32],
        );
        table.adopt(SignedDelegationCertificate::issue(&helper, narrow).unwrap());
        assert!(table.covers(subject_of(&sub), &scope("trail/step/a"), Mode::Write));
        assert!(!table.covers(subject_of(&sub), &scope("trail/other"), Mode::Write));

        // A widening child (the root scope) does not verify, however well signed.
        let wide = DelegationCertificate::new(
            DelegationParent::Certificate(parent_id),
            helper.master_public_key().to_bytes(),
            subject_of(&sub).0,
            scope_for(&Cap::root_scope(), Mode::Write, b"session-1".to_vec()),
            1_000,
            1_000,
            None,
            0,
            [12; 32],
        );
        let wide_signed = SignedDelegationCertificate::issue(&helper, wide).unwrap();
        assert!(
            wide_signed.verify(),
            "signed correctly, but still not authorized"
        );
        let err = table.verify_chain(&wide_signed).unwrap_err();
        assert!(matches!(err, ChainError::NotAttenuating(_)), "{err:?}");
    }

    #[test]
    fn a_holder_without_delegation_depth_cannot_delegate() {
        // depth 0: the helper may act, never grant.
        let (mut table, _user, helper) = rooted(Mode::Delegate, 0);
        let sub = provider(2);
        let parent_id = table.certificates().next().unwrap().certificate.id();
        let child = DelegationCertificate::new(
            DelegationParent::Certificate(parent_id),
            helper.master_public_key().to_bytes(),
            subject_of(&sub).0,
            scope_for(&scope("trail/step"), Mode::Read, b"session-1".to_vec()),
            1_000,
            1_000,
            None,
            0,
            [13; 32],
        );
        let signed = SignedDelegationCertificate::issue(&helper, child).unwrap();
        table.adopt(signed.clone());
        assert!(matches!(
            table.verify_chain(&signed).unwrap_err(),
            ChainError::NotAttenuating(_)
        ));
        assert!(!table.covers(subject_of(&sub), &scope("trail/step"), Mode::Read));
    }

    #[test]
    fn revoking_a_link_cascades_to_the_whole_subtree() {
        let (mut table, _user, helper) = rooted(Mode::Delegate, 1);
        let sub = provider(2);
        let parent_id = table.certificates().next().unwrap().certificate.id();
        let child = DelegationCertificate::new(
            DelegationParent::Certificate(parent_id),
            helper.master_public_key().to_bytes(),
            subject_of(&sub).0,
            scope_for(&scope("trail/step"), Mode::Write, b"session-1".to_vec()),
            1_000,
            1_000,
            None,
            0,
            [14; 32],
        );
        table.adopt(SignedDelegationCertificate::issue(&helper, child).unwrap());
        assert!(table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
        assert!(table.covers(subject_of(&sub), &scope("trail/step/y"), Mode::Write));

        // Revoke the ROOT link only.
        assert!(table.revoke(parent_id));
        assert!(!table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
        assert!(
            !table.covers(subject_of(&sub), &scope("trail/step/y"), Mode::Write),
            "the subtree cascades from one revocation, with no marking pass"
        );
    }

    #[test]
    fn revoke_root_grants_is_uninstall() {
        let (mut table, _user, helper) = rooted(Mode::Write, 0);
        assert_eq!(table.revoke_root_grants(subject_of(&helper)), 1);
        assert!(!table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
    }

    #[test]
    fn a_bounded_grant_stops_covering_when_the_clock_passes_it() {
        let user = provider(0);
        let helper = provider(1);
        let mut table = DelegationTable::new(root_key(&user));
        let cert = root_certificate(
            root_key(&user),
            subject_of(&helper),
            &scope("trail"),
            Mode::Write,
            b"session-1".to_vec(),
            1_000,
            Some(5_000),
            0,
            [15; 32],
        );
        table.adopt(SignedDelegationCertificate::issue(&user, cert).unwrap());

        table.set_now(4_999);
        assert!(table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write));
        table.set_now(5_001);
        assert!(
            !table.covers(subject_of(&helper), &scope("trail/x"), Mode::Write),
            "expiry needs no mutation of the store"
        );
    }

    #[test]
    fn a_chain_verifies_from_a_cold_store_in_any_order() {
        // Rebuild: certificates arrive out of order and validity is re-derived
        // from the signatures, not trusted because they were stored.
        let (table, user, helper) = rooted(Mode::Delegate, 1);
        let sub = provider(2);
        let parent_id = table.certificates().next().unwrap().certificate.id();
        let child = DelegationCertificate::new(
            DelegationParent::Certificate(parent_id),
            helper.master_public_key().to_bytes(),
            subject_of(&sub).0,
            scope_for(&scope("trail/step"), Mode::Write, b"session-1".to_vec()),
            1_000,
            1_000,
            None,
            0,
            [16; 32],
        );
        let child_signed = SignedDelegationCertificate::issue(&helper, child).unwrap();
        let root_signed = table.certificates().next().unwrap().clone();

        let mut cold = DelegationTable::new(root_key(&user));
        cold.adopt(child_signed); // child first
        cold.adopt(root_signed);
        cold.set_now(2_000);
        assert!(cold.covers(subject_of(&sub), &scope("trail/step/y"), Mode::Write));
    }

    #[test]
    fn a_child_whose_parent_is_absent_does_not_verify() {
        let (table, _user, helper) = rooted(Mode::Delegate, 1);
        let sub = provider(2);
        let orphan = DelegationCertificate::new(
            DelegationParent::Certificate(DelegationId([0xAB; 32])),
            helper.master_public_key().to_bytes(),
            subject_of(&sub).0,
            scope_for(&scope("trail/step"), Mode::Write, b"session-1".to_vec()),
            1_000,
            1_000,
            None,
            0,
            [17; 32],
        );
        let signed = SignedDelegationCertificate::issue(&helper, orphan).unwrap();
        assert!(matches!(
            table.verify_chain(&signed).unwrap_err(),
            ChainError::MissingParent(_)
        ));
    }
}
