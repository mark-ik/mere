// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What this wallet trusts to issue a device grant, and how a grant stands.
//!
//! This is the verifying half of the persona split, and the part with a real
//! architectural consequence. A verifier used to need one thing: the master
//! public key. It now needs **one root per persona**, because each persona
//! issues its own device certificates under its own chain root. That is what
//! buys independent revocation, and it is why the trusted set is a list.
//!
//! The roots are readable without unlocking anything: each persona wallet
//! already stores its `chain_root`. Only the master root has to be supplied by
//! a caller holding it, since the wallet does not keep the master public key
//! beside its manifests.
//!
//! Nothing here evaluates a chain itself. `notochord::validate_chain` does
//! that, and it checks signatures, signer attestation, root anchoring, link
//! integrity, attenuation, revocation, and validity windows. This module
//! assembles its inputs and reports what it said.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use identity::PersonaId;
use identity::carry::DevicePublicKey;
use notochord::{ChainFault, TrustedRoot, validate_chain};

use crate::wallet_store::{load_identity_wallet, load_persona_wallet};

use super::{DeviceId, load_device_grant_set, load_revocation_ledger};

/// A device grant is one certificate deep, always. Subdelegation is forbidden
/// at issuance, so a longer chain is a chain this wallet did not write.
const DEVICE_GRANT_DEPTH: u16 = 1;

/// Every root this wallet accepts as an issuer of device grants.
///
/// The master root anchors device-scoped certificates; one persona root
/// anchors each persona's own. `master_public_key` is a parameter because the
/// wallet stores persona chain roots but not its own master key, so a caller
/// that has unlocked the seed supplies it.
pub fn wallet_trusted_roots(
    data_root: &Path,
    master_public_key: [u8; 32],
) -> io::Result<Vec<TrustedRoot>> {
    let mut roots = vec![TrustedRoot {
        authority: master_public_key,
        issuer: master_public_key,
    }];
    let Some(wallet) = load_identity_wallet(data_root)? else {
        return Ok(roots);
    };
    for persona in &wallet.personas {
        let Some(manifest) = load_persona_wallet(data_root, persona.persona_id)? else {
            continue;
        };
        // A persona is its own authority here: it anchors the chain it issues,
        // so `authority` and `issuer` are the same key.
        roots.push(TrustedRoot {
            authority: manifest.chain_root.0,
            issuer: manifest.chain_root.0,
        });
    }
    Ok(roots)
}

/// How every certificate in one device's grant set stands, right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantStanding {
    /// The device-scoped certificate's verdict, when the device holds one.
    pub device: Option<Result<(), ChainFault>>,
    /// Each persona certificate's verdict, keyed by the persona that issued it.
    pub personas: BTreeMap<PersonaId, Result<(), ChainFault>>,
}

impl GrantStanding {
    /// Whether the device holds any authority at all that still stands.
    ///
    /// Deliberately not "all of them". A device whose persona authority was
    /// withdrawn while its transport authority stands is still carrying
    /// traffic, and reporting that as wholly invalid would flatten exactly
    /// what the split exists to keep visible.
    pub fn holds_any_authority(&self) -> bool {
        self.device.as_ref().is_some_and(|v| v.is_ok())
            || self.personas.values().any(|verdict| verdict.is_ok())
    }

    /// Whether every certificate the device holds still stands.
    pub fn is_wholly_valid(&self) -> bool {
        !self.is_empty()
            && self.device.as_ref().is_none_or(|v| v.is_ok())
            && self.personas.values().all(|verdict| verdict.is_ok())
    }

    /// Whether the device holds no certificates at all.
    pub fn is_empty(&self) -> bool {
        self.device.is_none() && self.personas.is_empty()
    }

    /// The actions the device may still exercise, across standing certificates.
    pub fn standing_faults(&self) -> Vec<ChainFault> {
        let mut faults: Vec<ChainFault> = self
            .device
            .iter()
            .chain(self.personas.values())
            .filter_map(|verdict| verdict.as_ref().err().copied())
            .collect();
        faults.sort_by_key(|fault| format!("{fault:?}"));
        faults.dedup();
        faults
    }
}

/// Evaluate one device's grant set against this wallet's roots and ledger.
///
/// `holder` is the device key the certificates must name as subject, so a
/// certificate lifted from another device fails here rather than passing on
/// its signature alone.
pub fn assess_device_grant(
    data_root: &Path,
    device_id: DeviceId,
    holder: DevicePublicKey,
    master_public_key: [u8; 32],
    now_ms: u64,
) -> io::Result<GrantStanding> {
    let set = load_device_grant_set(data_root, device_id)?;
    let roots = wallet_trusted_roots(data_root, master_public_key)?;
    let ledger = load_revocation_ledger(data_root)?;

    let assess = |certificate: &identity::delegation::SignedDelegationCertificate| {
        validate_chain(
            std::slice::from_ref(certificate),
            holder.0,
            &roots,
            &ledger,
            DEVICE_GRANT_DEPTH,
            now_ms,
        )
    };

    Ok(GrantStanding {
        device: set.device.as_ref().map(assess),
        personas: set
            .personas
            .iter()
            .map(|(&persona, certificate)| (persona, assess(certificate)))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use identity::IdentityProvider;

    use super::super::test_support::*;
    use super::super::{issue_remote_auth_device_grant, revoke_device_certificates};
    use super::*;

    const NOW_MS: u64 = 1_750_000_000_000;

    fn seeded(tag: &str) -> (std::path::PathBuf, [u8; 32], [u8; 32]) {
        let root = temp_data_root(tag);
        let seed = crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC")
            .unwrap();
        let master = identity::InMemoryProvider::from_seed(seed)
            .master_public_key()
            .to_bytes();
        (root, seed, master)
    }

    fn holder() -> DevicePublicKey {
        DevicePublicKey::from(delegatee().public_key())
    }

    /// The headline consequence: a verifier needs one root per persona, not
    /// one master key. This asserts the set actually grows with them.
    #[test]
    fn the_trusted_set_carries_one_root_per_persona() {
        let (root, _seed, master) = seeded("m5-roots");
        let roots = wallet_trusted_roots(&root, master).unwrap();

        assert!(roots.iter().any(|r| r.authority == master));
        assert_eq!(roots.len(), 2, "the master root plus one persona");

        crate::wallet_store::ensure_wallet_state(&root, second_persona(), "Studio PC").unwrap();
        let roots = wallet_trusted_roots(&root, master).unwrap();
        assert_eq!(roots.len(), 3, "a second persona adds a third root");
    }

    #[test]
    fn a_fresh_grant_stands_on_every_certificate() {
        let (root, _seed, master) = seeded("m5-fresh");
        let mut spec = sample_remote_auth_spec();
        spec.scopes = vec!["transport.egress".into(), "identity.act".into()];
        spec.issued_at_ms = NOW_MS;
        spec.expires_at_ms = Some(NOW_MS + 3_600_000);
        issue_remote_auth_device_grant(&root, &spec).unwrap();

        let standing =
            assess_device_grant(&root, spec.device_id, holder(), master, NOW_MS + 1_000).unwrap();

        assert!(standing.is_wholly_valid(), "{standing:?}");
        assert!(standing.device.is_some());
        assert_eq!(standing.personas.len(), 1);
    }

    #[test]
    fn a_revoked_grant_reports_revoked_rather_than_merely_failing() {
        let (root, seed, master) = seeded("m5-revoked");
        let mut spec = sample_remote_auth_spec();
        spec.issued_at_ms = NOW_MS;
        spec.expires_at_ms = Some(NOW_MS + 3_600_000);
        issue_remote_auth_device_grant(&root, &spec).unwrap();
        revoke_device_certificates(&root, seed, spec.device_id, NOW_MS + 10).unwrap();

        let standing =
            assess_device_grant(&root, spec.device_id, holder(), master, NOW_MS + 1_000).unwrap();

        assert!(!standing.holds_any_authority());
        assert_eq!(standing.standing_faults(), vec![ChainFault::Revoked]);
    }

    #[test]
    fn an_expired_grant_does_not_stand() {
        let (root, _seed, master) = seeded("m5-expired");
        let mut spec = sample_remote_auth_spec();
        spec.issued_at_ms = NOW_MS;
        spec.expires_at_ms = Some(NOW_MS + 1_000);
        issue_remote_auth_device_grant(&root, &spec).unwrap();

        let standing =
            assess_device_grant(&root, spec.device_id, holder(), master, NOW_MS + 5_000).unwrap();

        assert!(!standing.holds_any_authority());
    }

    /// A certificate is bound to the device that holds it. Presenting one from
    /// a different holder must fail even though its signature is perfectly
    /// good.
    #[test]
    fn a_certificate_does_not_travel_to_another_holder() {
        let (root, _seed, master) = seeded("m5-holder");
        let mut spec = sample_remote_auth_spec();
        spec.issued_at_ms = NOW_MS;
        spec.expires_at_ms = Some(NOW_MS + 3_600_000);
        issue_remote_auth_device_grant(&root, &spec).unwrap();

        let standing = assess_device_grant(
            &root,
            spec.device_id,
            DevicePublicKey([0x99; 32]),
            master,
            NOW_MS + 1_000,
        )
        .unwrap();

        assert!(!standing.holds_any_authority());
    }

    /// Without the persona's root in the trusted set, its certificate is a
    /// well-signed statement from a stranger. This is the failure mode the
    /// per-persona root exists to make possible.
    #[test]
    fn a_persona_certificate_needs_its_own_root_to_be_trusted() {
        let (root, _seed, master) = seeded("m5-untrusted");
        let mut spec = sample_remote_auth_spec();
        spec.scopes = vec!["identity.act".into()];
        spec.issued_at_ms = NOW_MS;
        spec.expires_at_ms = Some(NOW_MS + 3_600_000);
        issue_remote_auth_device_grant(&root, &spec).unwrap();

        let set = load_device_grant_set(&root, spec.device_id).unwrap();
        let certificate = set.personas.values().next().expect("a persona certificate");
        let ledger = load_revocation_ledger(&root).unwrap();
        let master_only = [TrustedRoot {
            authority: master,
            issuer: master,
        }];

        let verdict = validate_chain(
            std::slice::from_ref(certificate),
            holder().0,
            &master_only,
            &ledger,
            DEVICE_GRANT_DEPTH,
            NOW_MS + 1_000,
        );

        assert_eq!(verdict, Err(ChainFault::UntrustedRoot));
    }
}
