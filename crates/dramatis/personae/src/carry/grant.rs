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

use crate::IdentityProvider;
use crate::delegation::{
    DelegationCertificate, DelegationError, DelegationParent, SignedDelegationCertificate,
};

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
}
