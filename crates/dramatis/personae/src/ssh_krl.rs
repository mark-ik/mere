//! Revocation: closing a machine's access without visiting it.
//!
//! Two layers, because they answer different questions.
//!
//! 1. **The ledger stops minting.** A revoked device gets no new
//!    certificates from this vault, so its reach ends when the last one it
//!    holds expires — at most [`crate::ssh_ca::MAX_CERT_TTL_MS`]. This needs
//!    no contact with any host at all, which is the point: the machines
//!    that must forget a stolen laptop are exactly the ones you may not be
//!    able to reach in the minutes that matter.
//! 2. **The KRL closes it now.** OpenSSH's Key Revocation List, named by
//!    `RevokedKeys` in `sshd_config`, refuses matching certificates at
//!    once. It costs a root-side deploy per host, so it is the second line
//!    rather than the first.
//!
//! Revocation is keyed by **device serial**, not by grant id. A self-grant
//! is re-issued on each mint and its id is new every time, so revoking an
//! id closes one certificate and nothing else; the serial is derived from
//! the device and is stable across every certificate that device carries.
//! Grant ids are still recorded, and still emitted as `id:` lines, because
//! they are what `sshd` logs and what an auditor reads back.
//!
//! This module renders a KRL *specification*: the text
//! `ssh-keygen -k -s <ca> -f <krl>` compiles into the binary format. It
//! does not shell out; the ceremony belongs to the CLI.
//!
//! Feature `ssh`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::IdentityError;
use crate::delegation::{DelegationId, SignedDelegationRevocation};
use crate::ssh_ca::{device_serial, key_id_for};
use crate::vault::{
    CredentialLineage, IdentitySlot, Profile, ProtocolKey, SecretBytes, UnlockTier,
};

/// The `mod_id` the revocation ledger is stored under.
pub const REVOCATION_MOD_ID: &str = "ssh-revocations";

/// One revoked device, and what is known about why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokedDevice {
    /// The certificate serial every certificate for this device carries.
    pub serial: u64,
    /// Human-readable name, for the operator reading a ledger back.
    pub label: String,
    /// When the revocation was authored.
    pub at_ms: u64,
    /// Grant ids revoked along the way, newest last.
    pub grants: Vec<DelegationId>,
}

/// The vault's record of what it will no longer certify.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationLedger {
    devices: BTreeMap<u64, RevokedDevice>,
}

impl RevocationLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a signed revocation, returning whether it was accepted.
    ///
    /// The signature is checked here rather than trusted: a ledger that
    /// folds unverified statements is a ledger anyone can write to.
    pub fn fold(&mut self, revocation: &SignedDelegationRevocation, label: &str) -> bool {
        if !revocation.verify() {
            return false;
        }
        let statement = &revocation.revocation;
        let serial = device_serial(&statement.scope.resource);
        let entry = self.devices.entry(serial).or_insert_with(|| RevokedDevice {
            serial,
            label: label.to_string(),
            at_ms: statement.at_ms,
            grants: Vec::new(),
        });
        if !entry.grants.contains(&statement.certificate) {
            entry.grants.push(statement.certificate);
        }
        entry.at_ms = entry.at_ms.min(statement.at_ms);
        true
    }

    /// Whether this device may still be certified.
    pub fn is_revoked(&self, serial: u64) -> bool {
        self.devices.contains_key(&serial)
    }

    /// Every revoked device, by serial.
    pub fn devices(&self) -> impl Iterator<Item = &RevokedDevice> {
        self.devices.values()
    }

    /// Whether anything is revoked at all.
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Render the KRL specification `ssh-keygen -k` compiles.
    ///
    /// One `serial:` line per revoked device closes every certificate that
    /// device carries; the `id:` lines beneath it name the specific grants,
    /// which is redundant for enforcement and useful for reading.
    pub fn krl_spec(&self) -> String {
        let mut out = String::from("# personae revocation list\n");
        for device in self.devices.values() {
            out.push_str(&format!(
                "# {} revoked at {}ms\n",
                device.label, device.at_ms
            ));
            out.push_str(&format!("serial: {}\n", device.serial));
            for grant in &device.grants {
                out.push_str(&format!("id: {}\n", key_id_for(grant)));
            }
        }
        out
    }
}

/// The protocol key the ledger stores under.
pub fn ledger_key() -> ProtocolKey {
    ProtocolKey::new(REVOCATION_MOD_ID, None)
}

/// Read a profile's revocation ledger, empty when it has none.
pub fn load_ledger(profile: &Profile) -> Result<RevocationLedger, IdentityError> {
    let Some(IdentitySlot::Direct { payload, .. }) = profile.slots.get(&ledger_key()) else {
        return Ok(RevocationLedger::new());
    };
    serde_json::from_slice(payload.as_slice())
        .map_err(|err| IdentityError::Backend(format!("decode revocation ledger: {err}")))
}

/// Write a profile's revocation ledger.
pub fn store_ledger(profile: &mut Profile, ledger: &RevocationLedger) -> Result<(), IdentityError> {
    let encoded = serde_json::to_vec(ledger)
        .map_err(|err| IdentityError::Backend(format!("encode revocation ledger: {err}")))?;
    profile.slots.insert(
        ledger_key(),
        IdentitySlot::Direct {
            kind: REVOCATION_MOD_ID.to_string(),
            payload: SecretBytes::new(encoded),
            lineage: CredentialLineage::LocallyDerived,
            unlock_tier: UnlockTier::Session,
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carry::{ACTION_SSH_LOGIN, DeviceId, device_capability_scope};
    use crate::delegation::{DelegationRevocation, SignedDelegationCertificate};
    use crate::ssh_ca::{self, serial_for_device};
    use crate::vault::ProfileId;
    use crate::{Ed25519Keypair, IdentityProvider, InMemoryProvider};

    const NOW_MS: u64 = 1_760_000_000_000;

    fn device(n: u128) -> DeviceId {
        DeviceId::from_uuid(uuid::Uuid::from_u128(n))
    }

    fn revocation(
        provider: &InMemoryProvider,
        device: DeviceId,
        grant: DelegationId,
    ) -> SignedDelegationRevocation {
        SignedDelegationRevocation::issue(
            provider,
            DelegationRevocation::new(
                grant,
                provider.master_public_key().to_bytes(),
                device_capability_scope(device, [ACTION_SSH_LOGIN]),
                NOW_MS,
                [1; 32],
            ),
        )
        .unwrap()
    }

    #[test]
    fn folding_records_the_device_serial_not_just_the_grant() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let mut ledger = RevocationLedger::new();
        assert!(ledger.fold(
            &revocation(&provider, device(1), DelegationId([9; 32])),
            "laptop"
        ));
        assert!(ledger.is_revoked(serial_for_device(device(1))));
        assert!(!ledger.is_revoked(serial_for_device(device(2))));
    }

    /// The whole reason revocation is keyed by device: a self-grant is
    /// re-issued per mint, so two mints from one machine carry two ids and
    /// one serial. Revoking either id must close both.
    #[test]
    fn two_grants_from_one_device_share_one_serial() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let first =
            ssh_ca::self_grant(&provider, device(1), &[ACTION_SSH_LOGIN], 60_000, NOW_MS).unwrap();
        let second = ssh_ca::self_grant(
            &provider,
            device(1),
            &[ACTION_SSH_LOGIN],
            60_000,
            NOW_MS + 1,
        )
        .unwrap();
        assert_ne!(
            first.certificate.id(),
            second.certificate.id(),
            "each mint issues its own grant"
        );

        let mut ledger = RevocationLedger::new();
        ledger.fold(
            &revocation(&provider, device(1), first.certificate.id()),
            "laptop",
        );
        // The second grant was never named, and is closed anyway.
        assert!(ledger.is_revoked(device_serial(&second.certificate.scope.resource)));
    }

    #[test]
    fn an_unverified_revocation_is_refused() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let mut tampered = revocation(&provider, device(1), DelegationId([9; 32]));
        tampered.revocation.certificate = DelegationId([8; 32]);
        let mut ledger = RevocationLedger::new();
        assert!(!ledger.fold(&tampered, "forged"));
        assert!(ledger.is_empty());
    }

    #[test]
    fn the_spec_is_what_ssh_keygen_reads() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let mut ledger = RevocationLedger::new();
        ledger.fold(
            &revocation(&provider, device(1), DelegationId([9; 32])),
            "thinkpad",
        );
        let spec = ledger.krl_spec();
        assert!(spec.contains(&format!("serial: {}", serial_for_device(device(1)))));
        assert!(spec.contains(&format!("id: {}", key_id_for(&DelegationId([9; 32])))));
        assert!(spec.contains("# thinkpad revoked at"));
    }

    #[test]
    fn a_ledger_round_trips_through_a_slot() {
        let provider = InMemoryProvider::from_seed([1; 32]);
        let mut ledger = RevocationLedger::new();
        ledger.fold(
            &revocation(&provider, device(3), DelegationId([2; 32])),
            "imac",
        );

        let mut profile = Profile::new(
            ProfileId("t".into()),
            "t",
            Ed25519Keypair::from_seed([5; 32]),
        );
        store_ledger(&mut profile, &ledger).unwrap();
        assert_eq!(load_ledger(&profile).unwrap(), ledger);
    }

    #[test]
    fn an_absent_ledger_reads_as_empty_rather_than_failing() {
        let profile = Profile::new(
            ProfileId("t".into()),
            "t",
            Ed25519Keypair::from_seed([5; 32]),
        );
        assert!(load_ledger(&profile).unwrap().is_empty());
    }
}
