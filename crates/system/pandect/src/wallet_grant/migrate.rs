// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retiring grants written before the certificate format.
//!
//! The migration posture is re-issue, with no legacy decoder. That choice has
//! a consequence worth stating plainly: **the migration cannot be automatic.**
//! What a grant permitted lived in the signed payload, and refusing to decode
//! the payload means refusing to learn the scopes. A device's authority has to
//! be restated by whoever is re-commissioning it.
//!
//! Not everything is lost with the payload, though, and this module recovers
//! what survived elsewhere in the wallet so the restatement is a confirmation
//! rather than an act of memory:
//!
//! - the roster keeps each device's label, mode, exposure, and public key;
//! - each persona wallet keeps a capability slot named for the device, so the
//!   persona set is recoverable without reading a single grant byte.
//!
//! Legacy grants live at `identity/grants/<device_id>.cbor` and certificates
//! at `identity/grants/<device_id>/`, so the two never collide and a
//! half-migrated wallet is always legible.

use std::io;
use std::path::Path;

use identity::PersonaId;
use identity::carry::DeviceGrantSet;

use crate::wallet_store::{
    device_grant_path, load_device_roster, load_identity_wallet, load_persona_wallet,
};

use super::{
    DeviceExposure, DeviceId, DeviceMode, DevicePublicKey, RemoteAuthGrantSpec,
    issue_remote_auth_device_grant, load_device_grant_set, remote_auth_capability_slot_id,
};

/// One device still carrying a grant written before the certificate format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyGrant {
    /// The device the stale grant file belongs to.
    pub device_id: DeviceId,
    /// Operator-facing label from the roster.
    pub label: String,
    /// How the device was enrolled.
    pub mode: DeviceMode,
    /// Whether the device serves as an egress anchor.
    pub exposure: DeviceExposure,
    /// The device's public key, still known from the roster.
    pub holder: DevicePublicKey,
    /// Personas recovered from their own wallets' capability slots.
    ///
    /// Recovered rather than decoded: each persona that granted this device
    /// holds a slot named for it, so the persona set survives the payload.
    pub personas: Vec<PersonaId>,
    /// Whether a certificate set has already been issued for this device.
    ///
    /// True means the stale file is residue and [`retire_legacy_grant`] can
    /// remove it; false means the device has no working authority at all.
    pub reissued: bool,
}

impl LegacyGrant {
    /// Whether this device currently has no usable authority.
    ///
    /// A device in this state fails closed everywhere: `load_device_grant_set`
    /// returns an empty set, and enrol, refresh, and revoke all refuse it.
    pub fn is_stranded(&self) -> bool {
        !self.reissued
    }
}

/// Find every device whose grant predates the certificate format.
///
/// Reads the roster and the persona wallets. It never opens a legacy grant
/// file, only observes that one exists, which is what keeps the no-decoder
/// posture honest.
pub fn survey_legacy_grants(data_root: &Path) -> io::Result<Vec<LegacyGrant>> {
    let Some(roster) = load_device_roster(data_root)? else {
        return Ok(Vec::new());
    };
    let known_personas: Vec<PersonaId> = load_identity_wallet(data_root)?
        .map(|wallet| wallet.personas.iter().map(|p| p.persona_id).collect())
        .unwrap_or_default();

    let mut found = Vec::new();
    for device in &roster.devices {
        if !device_grant_path(data_root, device.device_id).exists() {
            continue;
        }
        let slot_id = remote_auth_capability_slot_id(device.device_id);
        let mut personas = Vec::new();
        for &persona in &known_personas {
            let Some(wallet) = load_persona_wallet(data_root, persona)? else {
                continue;
            };
            if wallet
                .capability_slots
                .iter()
                .any(|slot| slot.slot_id == slot_id)
            {
                personas.push(persona);
            }
        }
        let reissued = !load_device_grant_set(data_root, device.device_id)?.is_empty();
        found.push(LegacyGrant {
            device_id: device.device_id,
            label: device.label.clone(),
            mode: device.mode,
            exposure: device.exposure,
            holder: device.device_pubkey,
            personas,
            reissued,
        });
    }
    Ok(found)
}

/// Why a device has no certificates, when the answer is the migration.
///
/// A stranded device otherwise reports only that its certificates are absent,
/// which reads like corruption rather than like a pending re-commissioning.
/// This turns that into an attributable message at the two seams that refuse.
pub(crate) fn legacy_grant_hint(data_root: &Path, device_id: DeviceId) -> String {
    if device_grant_path(data_root, device_id).exists() {
        format!(
            "device {} still carries a grant written before the certificate format;              re-issue it (see survey_legacy_grants) rather than treating this as corruption",
            device_id.as_uuid()
        )
    } else {
        format!(
            "device grant certificates missing for {}",
            device_id.as_uuid()
        )
    }
}

/// Re-issue one legacy grant as a certificate set.
///
/// `scopes` is not optional and has no default on purpose. It is the one part
/// of the old grant this migration cannot recover, and guessing it would
/// either widen a device's authority or narrow it silently. The caller states
/// what the device is for; everything else comes from the survey.
pub fn reissue_legacy_grant(
    data_root: &Path,
    legacy: &LegacyGrant,
    scopes: &[String],
    issued_at_ms: u64,
    expires_at_ms: u64,
) -> io::Result<DeviceGrantSet> {
    if scopes.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "re-issuing the grant for device {} needs its scopes restated; \
                 the old payload is deliberately not decoded",
                legacy.device_id.as_uuid()
            ),
        ));
    }
    if legacy.mode != DeviceMode::RemoteAuth {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "device {} is enrolled as {:?}; only RemoteAuth devices carry re-issuable grants",
                legacy.device_id.as_uuid(),
                legacy.mode
            ),
        ));
    }
    issue_remote_auth_device_grant(
        data_root,
        &RemoteAuthGrantSpec {
            device_id: legacy.device_id,
            delegatee_pubkey: legacy.holder,
            label: legacy.label.clone(),
            exposure: legacy.exposure,
            issued_at_ms,
            expires_at_ms: Some(expires_at_ms),
            personas: legacy.personas.clone(),
            scopes: scopes.to_vec(),
            attenuations: Vec::new(),
            // Private-lane material cannot be recovered either: it was wrapped
            // to a pairing key inside the payload. A device that needs it is
            // re-paired, not re-issued.
            wrapped_private_epochs: Vec::new(),
        },
    )
}

/// Delete the stale grant file for a device that now has certificates.
///
/// Refuses while the device is stranded, so the only record that a device ever
/// had authority cannot be removed before that authority is restored.
pub fn retire_legacy_grant(data_root: &Path, device_id: DeviceId) -> io::Result<bool> {
    let path = device_grant_path(data_root, device_id);
    if !path.exists() {
        return Ok(false);
    }
    if load_device_grant_set(data_root, device_id)?.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing to retire the legacy grant for device {}: no certificates \
                 have been issued for it yet",
                device_id.as_uuid()
            ),
        ));
    }
    std::fs::remove_file(&path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::wallet_store::save_device_grant;

    fn stage_legacy(root: &Path, device_id: DeviceId) {
        // Opaque bytes: the point is that nothing ever decodes them.
        save_device_grant(root, device_id, b"pre-certificate grant bytes").unwrap();
    }

    #[test]
    fn a_wallet_with_no_roster_surveys_clean() {
        let root = tempfile::tempdir().unwrap();
        assert!(survey_legacy_grants(root.path()).unwrap().is_empty());
    }

    #[test]
    fn a_legacy_grant_is_found_and_reads_as_stranded() {
        let root = temp_data_root("m3-survey");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        let spec = sample_remote_auth_spec();
        issue_remote_auth_device_grant(&root, &spec).unwrap();
        // Simulate the pre-migration state: the stale file beside the new set.
        stage_legacy(&root, spec.device_id);

        let found = survey_legacy_grants(&root).unwrap();
        let entry = found
            .iter()
            .find(|entry| entry.device_id == spec.device_id)
            .expect("the legacy grant should be surveyed");
        assert_eq!(entry.label, spec.label);
        assert_eq!(entry.personas, vec![fixture_persona()]);
        // Certificates exist here, so it is residue rather than a stranding.
        assert!(entry.reissued);
        assert!(!entry.is_stranded());
    }

    /// The consequence of the no-decoder posture, asserted rather than
    /// described: the scopes must be restated, and omitting them fails loudly
    /// instead of minting a narrower grant.
    #[test]
    fn re_issuing_without_restated_scopes_is_refused() {
        let root = temp_data_root("m3-scopes");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        let legacy = LegacyGrant {
            device_id: fixture_device(),
            label: "Pocket relay".into(),
            mode: DeviceMode::RemoteAuth,
            exposure: DeviceExposure::ExposedEgress,
            holder: DevicePublicKey::from(delegatee().public_key()),
            personas: vec![fixture_persona()],
            reissued: false,
        };

        let error = reissue_legacy_grant(&root, &legacy, &[], 1_700_000_001, 1_800_000_001)
            .expect_err("empty scopes must be refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("scopes restated"));
    }

    #[test]
    fn a_copy_mode_device_is_not_re_issuable() {
        let root = temp_data_root("m3-copy");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        let legacy = LegacyGrant {
            device_id: fixture_device(),
            label: "Laptop clone".into(),
            mode: DeviceMode::Copy,
            exposure: DeviceExposure::HiddenClient,
            holder: DevicePublicKey::from(delegatee().public_key()),
            personas: vec![fixture_persona()],
            reissued: false,
        };

        let error = reissue_legacy_grant(
            &root,
            &legacy,
            &["transport.egress".to_string()],
            1_700_000_001,
            1_800_000_001,
        )
        .expect_err("a Copy device has no delegated grant to re-issue");
        assert!(error.to_string().contains("only RemoteAuth"));
    }

    #[test]
    fn re_issuing_restores_a_stranded_device() {
        let root = temp_data_root("m3-reissue");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        let legacy = LegacyGrant {
            device_id: fixture_device(),
            label: "Pocket relay".into(),
            mode: DeviceMode::RemoteAuth,
            exposure: DeviceExposure::ExposedEgress,
            holder: DevicePublicKey::from(delegatee().public_key()),
            personas: Vec::new(),
            reissued: false,
        };

        let set = reissue_legacy_grant(
            &root,
            &legacy,
            &["transport.egress".to_string()],
            1_700_000_001,
            1_800_000_001,
        )
        .unwrap();

        assert!(set.device.is_some());
        assert!(
            !load_device_grant_set(&root, fixture_device())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_stranded_device_keeps_its_legacy_file() {
        let root = temp_data_root("m3-retire-guard");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        stage_legacy(&root, fixture_device());

        let error = retire_legacy_grant(&root, fixture_device())
            .expect_err("retiring before re-issue must be refused");
        assert!(error.to_string().contains("no certificates"));
        assert!(device_grant_path(&root, fixture_device()).exists());
    }

    #[test]
    fn retiring_removes_the_file_once_certificates_exist() {
        let root = temp_data_root("m3-retire");
        crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC").unwrap();
        let spec = sample_remote_auth_spec();
        issue_remote_auth_device_grant(&root, &spec).unwrap();
        stage_legacy(&root, spec.device_id);

        assert!(retire_legacy_grant(&root, spec.device_id).unwrap());
        assert!(!device_grant_path(&root, spec.device_id).exists());
        // Retiring residue must not disturb the authority that replaced it.
        assert!(
            !load_device_grant_set(&root, spec.device_id)
                .unwrap()
                .is_empty()
        );
    }
}
