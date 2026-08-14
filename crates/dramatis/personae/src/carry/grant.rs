// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Issuing a device grant as a delegation certificate.
//!
//! A device grant is a persona delegating a scoped, expiring capability to a
//! device. The two device modes differ in exactly one field:
//!
//! - [`DeviceMode::Copy`] is a **self grant**. The device holds the persona's
//!   own seed, so it *is* the persona and the certificate's subject is the
//!   master key.
//! - [`DeviceMode::RemoteAuth`] is a **delegated grant**. The device holds its
//!   own key and a narrower authority, so the subject is the device's public
//!   key.
//!
//! Everything else is common: the persona's own root authority as parent, the
//! device scope from [`device_capability_scope`], no subdelegation, and a
//! mandatory expiry. Keeping one constructor is what makes that difference
//! legible instead of two near-identical call sites drifting apart.
//!
//! Verification is deliberately not here. `notochord` evaluates chains
//! (signatures, root anchoring, attenuation, revocation, validity windows);
//! this module only issues.
//!
//! [`DeviceMode::Copy`]: super::DeviceMode::Copy
//! [`DeviceMode::RemoteAuth`]: super::DeviceMode::RemoteAuth

use std::collections::BTreeMap;

use crate::delegation::{
    DelegationCertificate, DelegationError, DelegationParent, SignedDelegationCertificate,
};
use crate::{IdentityProvider, InMemoryProvider, PersonaId};

use super::{DeviceId, DevicePublicKey, device_capability_scope};

/// Issuer-chosen uniqueness for one device grant.
///
/// Bound to the subject as well as the device and the clock, so a persona can
/// issue two grants for the same device to different holders within the same
/// millisecond without minting the same certificate id twice.
pub fn device_grant_nonce(device: DeviceId, subject: [u8; 32], now_ms: u64) -> [u8; 32] {
    *blake3::hash(
        &[
            device.as_uuid().as_bytes().as_slice(),
            &subject,
            &now_ms.to_le_bytes(),
        ]
        .concat(),
    )
    .as_bytes()
}

/// Issue one device grant certificate to an explicit subject.
///
/// Prefer [`issue_self_grant`] or [`issue_remote_auth_grant`], which name the
/// two device modes rather than leaving the subject to the caller.
pub fn issue_device_grant<P: IdentityProvider>(
    provider: &P,
    device: DeviceId,
    subject: [u8; 32],
    actions: &[&str],
    valid_for_ms: u64,
    now_ms: u64,
) -> Result<SignedDelegationCertificate, DelegationError> {
    let master = provider.master_public_key().to_bytes();
    SignedDelegationCertificate::issue(
        provider,
        DelegationCertificate::new(
            DelegationParent::Root(master),
            master,
            subject,
            device_capability_scope(device, actions.iter().copied()),
            now_ms,
            now_ms,
            Some(now_ms.saturating_add(valid_for_ms)),
            0,
            device_grant_nonce(device, subject, now_ms),
        ),
    )
}

/// Issue a `Copy`-mode grant: the device holds the seed and acts as the persona.
pub fn issue_self_grant<P: IdentityProvider>(
    provider: &P,
    device: DeviceId,
    actions: &[&str],
    valid_for_ms: u64,
    now_ms: u64,
) -> Result<SignedDelegationCertificate, DelegationError> {
    let master = provider.master_public_key().to_bytes();
    issue_device_grant(provider, device, master, actions, valid_for_ms, now_ms)
}

/// Issue a `RemoteAuth` grant: the device holds its own key and a narrower authority.
pub fn issue_remote_auth_grant<P: IdentityProvider>(
    provider: &P,
    device: DeviceId,
    holder: DevicePublicKey,
    actions: &[&str],
    valid_for_ms: u64,
    now_ms: u64,
) -> Result<SignedDelegationCertificate, DelegationError> {
    issue_device_grant(provider, device, holder.0, actions, valid_for_ms, now_ms)
}

/// Issue a device grant **under one persona's own authority**.
///
/// The old signed envelope carried `personas: Vec<PersonaId>` as a field, so
/// one signature spoke for every persona at once and withdrawing a device from
/// one persona meant re-issuing the authority of all of them. A certificate
/// has one issuer, so the persona set becomes one certificate per persona,
/// each anchored at that persona's own [`PersonaChainRoot`] and each revocable
/// without touching the others. That independence is the point; the extra
/// records are the price.
///
/// The signing identity is the persona's derived keypair, not the master.
/// [`SignedDelegationCertificate::issue`] requires the provider's master
/// public key to equal the certificate's issuer, so the persona is a provider
/// in its own right here, seeded from its derived key.
///
/// A verifier trusts these by carrying one `TrustedRoot` per persona, with
/// `authority` and `issuer` both the persona chain root.
///
/// [`PersonaChainRoot`]: super::PersonaChainRoot
pub fn issue_persona_device_grant(
    master_seed: [u8; 32],
    persona: PersonaId,
    device: DeviceId,
    holder: DevicePublicKey,
    actions: &[&str],
    valid_for_ms: u64,
    now_ms: u64,
) -> Result<SignedDelegationCertificate, DelegationError> {
    let master = InMemoryProvider::from_seed(master_seed);
    let persona_keypair = master
        .derive_keypair(&super::persona_wallet_salt(persona))
        .map_err(|_| DelegationError::Identity)?;
    let persona_provider = InMemoryProvider::from_seed(persona_keypair.to_seed());
    issue_remote_auth_grant(
        &persona_provider,
        device,
        holder,
        actions,
        valid_for_ms,
        now_ms,
    )
}

/// Every certificate one device grant comprises.
///
/// The old envelope was a single signature covering a device, an action list,
/// and a persona list at once. Splitting the issuer per persona splits the
/// grant into a set, and the action partition decides which certificate each
/// action lands on: device-scoped actions on one master-issued certificate,
/// persona-scoped actions on one certificate per persona.
///
/// Both halves are optional and a real grant often has only one. A sited radio
/// carries `device` alone; a grant that only lets a laptop act as two personae
/// carries two `personas` entries and no `device`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceGrantSet {
    /// Device-wide authority, issued by the master.
    pub device: Option<SignedDelegationCertificate>,
    /// Per-persona authority, each issued by that persona's own chain root.
    pub personas: BTreeMap<PersonaId, SignedDelegationCertificate>,
}

impl DeviceGrantSet {
    /// Every certificate in the set, device first.
    pub fn certificates(&self) -> impl Iterator<Item = &SignedDelegationCertificate> {
        self.device.iter().chain(self.personas.values())
    }

    /// Whether the set contains no authority at all.
    pub fn is_empty(&self) -> bool {
        self.device.is_none() && self.personas.is_empty()
    }
}

/// Issue the full certificate set for one device grant.
///
/// Returns [`DelegationError::MalformedCertificate`] when the requested
/// actions include persona-scoped ones but no persona is named: nothing could
/// issue such a capability, and silently dropping it would hand back a grant
/// narrower than the caller asked for without saying so.
pub fn issue_device_grant_set(
    master_seed: [u8; 32],
    device: DeviceId,
    holder: DevicePublicKey,
    actions: &[&str],
    personas: &[PersonaId],
    valid_for_ms: u64,
    now_ms: u64,
) -> Result<DeviceGrantSet, DelegationError> {
    let (device_actions, persona_actions) = super::partition_actions(actions.iter().copied());
    if !persona_actions.is_empty() && personas.is_empty() {
        return Err(DelegationError::MalformedCertificate);
    }

    let device_certificate = if device_actions.is_empty() {
        None
    } else {
        let master = InMemoryProvider::from_seed(master_seed);
        Some(issue_remote_auth_grant(
            &master,
            device,
            holder,
            &device_actions,
            valid_for_ms,
            now_ms,
        )?)
    };

    let mut persona_certificates = BTreeMap::new();
    if !persona_actions.is_empty() {
        for &persona in personas {
            persona_certificates.insert(
                persona,
                issue_persona_device_grant(
                    master_seed,
                    persona,
                    device,
                    holder,
                    &persona_actions,
                    valid_for_ms,
                    now_ms,
                )?,
            );
        }
    }

    Ok(DeviceGrantSet {
        device: device_certificate,
        personas: persona_certificates,
    })
}

#[cfg(test)]
mod tests {
    use crate::InMemoryProvider;
    use crate::carry::ACTION_TRANSPORT_EGRESS;

    use super::*;

    const NOW_MS: u64 = 1_770_000_000_000;

    fn provider() -> InMemoryProvider {
        InMemoryProvider::from_seed([0x4d; 32])
    }

    fn device() -> DeviceId {
        DeviceId::from_uuid(uuid::Uuid::from_u128(0x2026_0812_0002))
    }

    fn holder() -> DevicePublicKey {
        DevicePublicKey([0x5a; 32])
    }

    #[test]
    fn a_self_grant_names_the_persona_as_its_own_subject() {
        let provider = provider();
        let master = provider.master_public_key().to_bytes();
        let grant =
            issue_self_grant(&provider, device(), &[ACTION_TRANSPORT_EGRESS], 60_000, NOW_MS)
                .unwrap();

        assert_eq!(grant.certificate.issuer, master);
        assert_eq!(grant.certificate.subject, master);
        assert!(grant.verify());
    }

    #[test]
    fn a_remote_auth_grant_names_the_holder_as_subject() {
        let provider = provider();
        let master = provider.master_public_key().to_bytes();
        let grant = issue_remote_auth_grant(
            &provider,
            device(),
            holder(),
            &[ACTION_TRANSPORT_EGRESS],
            60_000,
            NOW_MS,
        )
        .unwrap();

        assert_eq!(grant.certificate.issuer, master);
        assert_eq!(grant.certificate.subject, holder().0);
        assert_ne!(grant.certificate.subject, master);
        assert!(grant.verify());
    }

    /// The whole point of `RemoteAuth`: a stolen station cannot hand its
    /// authority onward, and now the grammar enforces it rather than a
    /// string atom nobody reads.
    #[test]
    fn a_device_grant_forbids_subdelegation() {
        let grant = issue_remote_auth_grant(
            &provider(),
            device(),
            holder(),
            &[ACTION_TRANSPORT_EGRESS],
            60_000,
            NOW_MS,
        )
        .unwrap();

        assert_eq!(grant.certificate.remaining_delegation_depth, 0);
    }

    #[test]
    fn a_device_grant_always_expires() {
        let grant =
            issue_self_grant(&provider(), device(), &[ACTION_TRANSPORT_EGRESS], 60_000, NOW_MS)
                .unwrap();

        assert_eq!(grant.certificate.expires_at_ms, Some(NOW_MS + 60_000));
    }

    /// Binding the nonce to the subject is what keeps these distinct. The
    /// earlier derivation used only device and clock, so two holders granted
    /// the same device in one millisecond would have collided.
    #[test]
    fn two_holders_in_the_same_millisecond_get_distinct_certificates() {
        let provider = provider();
        let first = issue_remote_auth_grant(
            &provider,
            device(),
            DevicePublicKey([0x01; 32]),
            &[ACTION_TRANSPORT_EGRESS],
            60_000,
            NOW_MS,
        )
        .unwrap();
        let second = issue_remote_auth_grant(
            &provider,
            device(),
            DevicePublicKey([0x02; 32]),
            &[ACTION_TRANSPORT_EGRESS],
            60_000,
            NOW_MS,
        )
        .unwrap();

        assert_ne!(first.certificate.id(), second.certificate.id());
    }

    const MASTER_SEED: [u8; 32] = [0x4d; 32];

    fn persona(n: u128) -> PersonaId {
        PersonaId::from_uuid(uuid::Uuid::from_u128(n))
    }

    #[test]
    fn a_persona_grant_is_issued_by_that_persona_not_the_master() {
        let master = InMemoryProvider::from_seed(MASTER_SEED);
        let grant = issue_persona_device_grant(
            MASTER_SEED,
            persona(1),
            device(),
            holder(),
            &[ACTION_TRANSPORT_EGRESS],
            60_000,
            NOW_MS,
        )
        .unwrap();

        let expected = crate::carry::derive_persona_chain_root(MASTER_SEED, persona(1)).unwrap();
        assert_eq!(grant.certificate.issuer, expected.0);
        assert_ne!(grant.certificate.issuer, master.master_public_key().to_bytes());
        assert_eq!(grant.certificate.subject, holder().0);
        assert!(grant.verify());
    }

    /// The independence the per-persona split exists to buy: two personas
    /// granting the same device produce two certificates with two issuers,
    /// so revoking one leaves the other standing.
    #[test]
    fn two_personas_granting_one_device_are_independent_certificates() {
        let first = issue_persona_device_grant(
            MASTER_SEED,
            persona(1),
            device(),
            holder(),
            &[ACTION_TRANSPORT_EGRESS],
            60_000,
            NOW_MS,
        )
        .unwrap();
        let second = issue_persona_device_grant(
            MASTER_SEED,
            persona(2),
            device(),
            holder(),
            &[ACTION_TRANSPORT_EGRESS],
            60_000,
            NOW_MS,
        )
        .unwrap();

        assert_ne!(first.certificate.issuer, second.certificate.issuer);
        assert_ne!(first.certificate.id(), second.certificate.id());
        assert!(first.verify());
        assert!(second.verify());
    }

    /// A persona's authority is reproducible from the master seed, which is
    /// what lets M3 re-mint every grant on unlock without stored key material.
    #[test]
    fn persona_issuance_is_deterministic_for_one_seed() {
        let args = || {
            issue_persona_device_grant(
                MASTER_SEED,
                persona(1),
                device(),
                holder(),
                &[ACTION_TRANSPORT_EGRESS],
                60_000,
                NOW_MS,
            )
            .unwrap()
        };
        assert_eq!(args().certificate.id(), args().certificate.id());
    }

    fn set(actions: &[&str], personas: &[PersonaId]) -> DeviceGrantSet {
        issue_device_grant_set(
            MASTER_SEED,
            device(),
            holder(),
            actions,
            personas,
            60_000,
            NOW_MS,
        )
        .expect("issuing the grant set")
    }

    /// The sited-radio shape: device authority only, and no persona ever
    /// learns about it.
    #[test]
    fn a_station_grant_is_one_device_certificate_and_nothing_else() {
        let set = set(&[ACTION_TRANSPORT_EGRESS], &[]);

        let device_certificate = set.device.as_ref().expect("a device certificate");
        assert!(set.personas.is_empty());
        assert!(device_certificate.verify());
        assert!(
            device_certificate
                .certificate
                .scope
                .actions
                .contains(ACTION_TRANSPORT_EGRESS)
        );
    }

    #[test]
    fn actions_land_on_the_certificate_whose_authority_covers_them() {
        let set = set(
            &[ACTION_TRANSPORT_EGRESS, crate::carry::ACTION_IDENTITY_ACT],
            &[persona(1)],
        );

        let device_actions = &set.device.as_ref().unwrap().certificate.scope.actions;
        assert!(device_actions.contains(ACTION_TRANSPORT_EGRESS));
        assert!(!device_actions.contains(crate::carry::ACTION_IDENTITY_ACT));

        let persona_actions = &set.personas[&persona(1)].certificate.scope.actions;
        assert!(persona_actions.contains(crate::carry::ACTION_IDENTITY_ACT));
        assert!(!persona_actions.contains(ACTION_TRANSPORT_EGRESS));
    }

    /// Silently dropping the persona half would hand back a grant narrower
    /// than the caller asked for, without saying so.
    #[test]
    fn persona_actions_with_no_persona_named_are_refused() {
        let error = issue_device_grant_set(
            MASTER_SEED,
            device(),
            holder(),
            &[crate::carry::ACTION_IDENTITY_ACT],
            &[],
            60_000,
            NOW_MS,
        )
        .unwrap_err();

        assert!(matches!(error, DelegationError::MalformedCertificate));
    }

    #[test]
    fn each_persona_in_the_set_is_issued_by_its_own_authority() {
        let set = set(
            &[crate::carry::ACTION_IDENTITY_ACT],
            &[persona(1), persona(2)],
        );

        assert!(set.device.is_none());
        assert_eq!(set.personas.len(), 2);
        assert_ne!(
            set.personas[&persona(1)].certificate.issuer,
            set.personas[&persona(2)].certificate.issuer
        );
        assert!(set.certificates().all(|c| c.verify()));
    }
}
