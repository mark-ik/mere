// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device grants as delegation certificates, and the epoch material that
//! stopped travelling inside them.
//!
//! The old envelope signed one payload carrying both the capability and the
//! wrapped private-lane keys, so appending an epoch re-signed a statement that
//! had not changed and churned the `grant_ref` every wallet tracked. Here the
//! two are separate records: the certificate says what the device may do, and
//! a [`WrappedEpochRecord`] keyed by the certificate's own id carries the key
//! material it needs to do it.
//!
//! Storage only. `personae::carry` issues certificates and `notochord`
//! evaluates them; this module writes them down and holds the one invariant
//! that used to live inside the payload validator.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use identity::carry::{DeviceGrantSet, WALLET_SCHEMA_VERSION};
use identity::delegation::{DelegationId, SignedDelegationCertificate};
use identity::PersonaId;
use identity::carry::ACTION_PRIVATE_READ;
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use serde::{Deserialize, Serialize};

use crate::wallet_store::identity_grants_dir;

use super::{CarryRef, DeviceGrantError, DeviceId, WrappedEpochMaterial};

/// The wrapped private-lane material one device grant carries.
///
/// Keyed by the certificate it serves rather than embedded in it, so a new
/// epoch appends here and leaves the capability statement, its signature, and
/// its content ref untouched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedEpochRecord {
    /// Schema version, stamped like every other carry record.
    pub schema_version: u32,
    /// The certificate this material serves.
    pub certificate: DelegationId,
    /// The wrapped epochs themselves, in issue order.
    pub epochs: Vec<WrappedEpochMaterial>,
}

impl WrappedEpochRecord {
    /// An empty record bound to one certificate.
    pub fn new(certificate: DelegationId) -> Self {
        Self {
            schema_version: WALLET_SCHEMA_VERSION,
            certificate,
            epochs: Vec::new(),
        }
    }

    /// Whether any wrapped epoch is present for `persona`.
    pub fn covers_persona(&self, persona: PersonaId) -> bool {
        self.epochs.iter().any(|e| e.persona_id == persona)
    }

    /// Stable content ref of the encoded record.
    pub fn content_ref(&self) -> Result<CarryRef, DeviceGrantError> {
        Ok(CarryRef::of(encode_epoch_record(self)?.as_slice()))
    }
}

/// Whether a certificate's actions oblige it to carry epoch material.
///
/// This is the rule that used to sit inside the payload validator, where it
/// could only exist because the statement and the carriage shared an envelope.
/// It is a ledger invariant now: the wallet checks that a `private.read`
/// certificate has a record, rather than a signed payload checking itself.
pub fn requires_epoch_material(certificate: &SignedDelegationCertificate) -> bool {
    certificate
        .certificate
        .scope
        .actions
        .iter()
        .any(|action| action == ACTION_PRIVATE_READ)
}

/// `<data_root>/identity/grants/<device_id>/<persona_id>.cert`
pub fn device_certificate_path(data_root: &Path, device: DeviceId, persona: PersonaId) -> PathBuf {
    identity_grants_dir(data_root)
        .join(device.as_uuid().to_string())
        .join(format!("{}.cert", persona.as_uuid()))
}

/// `<data_root>/identity/grants/epochs/<certificate_id>.cbor`
pub fn wrapped_epoch_record_path(data_root: &Path, certificate: DelegationId) -> PathBuf {
    identity_grants_dir(data_root)
        .join("epochs")
        .join(format!("{}.cbor", hex::encode(certificate.0)))
}

/// Canonical bytes of a signed device-grant certificate.
pub fn encode_certificate(
    certificate: &SignedDelegationCertificate,
) -> Result<Vec<u8>, DeviceGrantError> {
    encode_cbor(certificate).map_err(|_| DeviceGrantError::Encode)
}

/// Decode a signed device-grant certificate.
pub fn decode_certificate(bytes: &[u8]) -> Result<SignedDelegationCertificate, DeviceGrantError> {
    decode_cbor(bytes).map_err(|_| DeviceGrantError::Decode)
}

/// Canonical bytes of a wrapped-epoch record.
pub fn encode_epoch_record(record: &WrappedEpochRecord) -> Result<Vec<u8>, DeviceGrantError> {
    encode_cbor(record).map_err(|_| DeviceGrantError::Encode)
}

/// Decode a wrapped-epoch record.
pub fn decode_epoch_record(bytes: &[u8]) -> Result<WrappedEpochRecord, DeviceGrantError> {
    decode_cbor(bytes).map_err(|_| DeviceGrantError::Decode)
}

fn invalid_data(e: DeviceGrantError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

/// Persist one persona's device-grant certificate, returning its content ref.
pub fn save_device_certificate(
    data_root: &Path,
    device: DeviceId,
    persona: PersonaId,
    certificate: &SignedDelegationCertificate,
) -> io::Result<CarryRef> {
    let bytes = encode_certificate(certificate).map_err(invalid_data)?;
    let path = device_certificate_path(data_root, device, persona);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &bytes)?;
    Ok(CarryRef::of(bytes.as_slice()))
}

/// Load one persona's device-grant certificate, if it has been issued.
pub fn load_device_certificate(
    data_root: &Path,
    device: DeviceId,
    persona: PersonaId,
) -> io::Result<Option<SignedDelegationCertificate>> {
    let path = device_certificate_path(data_root, device, persona);
    match std::fs::read(&path) {
        Ok(bytes) => decode_certificate(&bytes).map(Some).map_err(invalid_data),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Persist the wrapped-epoch record for one certificate.
pub fn save_wrapped_epoch_record(
    data_root: &Path,
    record: &WrappedEpochRecord,
) -> io::Result<CarryRef> {
    let bytes = encode_epoch_record(record).map_err(invalid_data)?;
    let path = wrapped_epoch_record_path(data_root, record.certificate);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &bytes)?;
    Ok(CarryRef::of(bytes.as_slice()))
}

/// Load the wrapped-epoch record for one certificate, if any exists.
pub fn load_wrapped_epoch_record(
    data_root: &Path,
    certificate: DelegationId,
) -> io::Result<Option<WrappedEpochRecord>> {
    let path = wrapped_epoch_record_path(data_root, certificate);
    match std::fs::read(&path) {
        Ok(bytes) => decode_epoch_record(&bytes).map(Some).map_err(invalid_data),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// `<data_root>/identity/grants/<device_id>/device.cert`
///
/// The master-issued half of a grant set. Named rather than keyed by a
/// persona because it belongs to no persona; a station has only this file.
pub fn device_scope_certificate_path(data_root: &Path, device: DeviceId) -> PathBuf {
    identity_grants_dir(data_root)
        .join(device.as_uuid().to_string())
        .join("device.cert")
}

/// Content ref standing for a whole grant set.
///
/// The roster and the wallet index each hold one ref per device, and a grant
/// is now several certificates, so the ref covers the set: the device
/// certificate's id if present, then each persona's id in `BTreeMap` order.
/// Certificate ids already commit to their full contents, so hashing the ids
/// is enough to make this change when any member changes.
pub fn device_grant_set_ref(set: &DeviceGrantSet) -> CarryRef {
    let mut bytes = Vec::new();
    match &set.device {
        Some(certificate) => {
            bytes.push(1);
            bytes.extend_from_slice(&certificate.certificate.id().0);
        }
        None => bytes.push(0),
    }
    for (persona, certificate) in &set.personas {
        bytes.extend_from_slice(persona.as_uuid().as_bytes());
        bytes.extend_from_slice(&certificate.certificate.id().0);
    }
    CarryRef::of(bytes.as_slice())
}

/// Persist every certificate in a grant set, returning the set's content ref.
pub fn save_device_grant_set(
    data_root: &Path,
    device: DeviceId,
    set: &DeviceGrantSet,
) -> io::Result<CarryRef> {
    if let Some(certificate) = &set.device {
        let bytes = encode_certificate(certificate).map_err(invalid_data)?;
        let path = device_scope_certificate_path(data_root, device);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &bytes)?;
    }
    for (&persona, certificate) in &set.personas {
        save_device_certificate(data_root, device, persona, certificate)?;
    }
    Ok(device_grant_set_ref(set))
}

/// Load every certificate stored for one device.
///
/// Reads the directory rather than taking a persona list, so a caller that has
/// forgotten which personas granted a device still recovers the whole set.
pub fn load_device_grant_set(data_root: &Path, device: DeviceId) -> io::Result<DeviceGrantSet> {
    let mut set = DeviceGrantSet {
        device: None,
        personas: BTreeMap::new(),
    };

    match std::fs::read(device_scope_certificate_path(data_root, device)) {
        Ok(bytes) => set.device = Some(decode_certificate(&bytes).map_err(invalid_data)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    let dir = identity_grants_dir(data_root).join(device.as_uuid().to_string());
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(set),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("cert") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // `device.cert` is the master-issued half, already loaded above.
        let Ok(uuid) = stem.parse::<uuid::Uuid>() else {
            continue;
        };
        let bytes = std::fs::read(&path)?;
        set.personas.insert(
            PersonaId(uuid),
            decode_certificate(&bytes).map_err(invalid_data)?,
        );
    }
    Ok(set)
}

/// Canonical bytes of a whole grant set, for transport.
///
/// A station's set is one certificate, so this stays a single small record on
/// the wire; a laptop's carries its persona certificates alongside.
pub fn encode_device_grant_set(set: &DeviceGrantSet) -> Result<Vec<u8>, DeviceGrantError> {
    encode_cbor(set).map_err(|_| DeviceGrantError::Encode)
}

/// Decode a grant set from canonical bytes.
pub fn decode_device_grant_set(bytes: &[u8]) -> Result<DeviceGrantSet, DeviceGrantError> {
    decode_cbor(bytes).map_err(|_| DeviceGrantError::Decode)
}

/// The device a certificate addresses, read back out of its scope resource.
///
/// `device_capability_scope` puts the device uuid's 16 bytes there, so this is
/// the inverse. Returns `None` for a scope built by something else, which is
/// how a certificate from a foreign domain fails closed here.
pub fn certificate_device_id(certificate: &SignedDelegationCertificate) -> Option<DeviceId> {
    let bytes: [u8; 16] = certificate.certificate.scope.resource.as_slice().try_into().ok()?;
    Some(DeviceId::from_uuid(uuid::Uuid::from_bytes(bytes)))
}

/// Check the carriage invariant for one stored certificate.
///
/// A certificate carrying `private.read` must have a wrapped-epoch record with
/// material for `persona`; anything else is a grant that promises private
/// reading it cannot perform.
pub fn check_epoch_carriage(
    data_root: &Path,
    certificate: &SignedDelegationCertificate,
    persona: PersonaId,
) -> io::Result<()> {
    if !requires_epoch_material(certificate) {
        return Ok(());
    }
    let record = load_wrapped_epoch_record(data_root, certificate.certificate.id())?;
    match record {
        Some(record) if record.covers_persona(persona) => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "certificate carries {ACTION_PRIVATE_READ} but no wrapped epoch material \
                 is stored for persona {}",
                persona.as_uuid()
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use identity::carry::{DevicePublicKey, issue_persona_device_grant};
    use identity::carry::{ACTION_TRANSPORT_EGRESS, KeyEpochId};

    use super::*;

    const MASTER_SEED: [u8; 32] = [0x6d; 32];
    const NOW_MS: u64 = 1_770_000_000_000;

    fn persona() -> PersonaId {
        PersonaId::from_uuid(uuid::Uuid::from_u128(0x9001))
    }

    fn device() -> DeviceId {
        DeviceId::from_uuid(uuid::Uuid::from_u128(0x9002))
    }

    fn grant(actions: &[&str]) -> SignedDelegationCertificate {
        issue_persona_device_grant(
            MASTER_SEED,
            persona(),
            device(),
            DevicePublicKey([0x7a; 32]),
            actions,
            60_000,
            NOW_MS,
        )
        .expect("issuing a persona device grant")
    }

    fn material() -> WrappedEpochMaterial {
        WrappedEpochMaterial {
            persona_id: persona(),
            epoch_id: KeyEpochId(uuid::Uuid::from_u128(0x9003)),
            wrap_format: "test/v1".into(),
            wrapped_key: vec![1, 2, 3],
        }
    }

    #[test]
    fn a_certificate_round_trips_through_storage() {
        let root = tempfile::tempdir().unwrap();
        let certificate = grant(&[ACTION_TRANSPORT_EGRESS]);
        save_device_certificate(root.path(), device(), persona(), &certificate).unwrap();

        let back = load_device_certificate(root.path(), device(), persona())
            .unwrap()
            .expect("the certificate should be stored");
        assert_eq!(back, certificate);
        assert!(back.verify());
    }

    #[test]
    fn an_absent_certificate_reads_as_none() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            load_device_certificate(root.path(), device(), persona())
                .unwrap()
                .is_none()
        );
    }

    /// The reason the split exists. Under the old envelope this operation
    /// re-signed the capability and produced a new content ref for a
    /// statement that had not changed.
    #[test]
    fn appending_an_epoch_leaves_the_certificate_untouched() {
        let root = tempfile::tempdir().unwrap();
        let certificate = grant(&[ACTION_PRIVATE_READ]);
        let id = certificate.certificate.id();
        let cert_ref =
            save_device_certificate(root.path(), device(), persona(), &certificate).unwrap();

        let mut record = WrappedEpochRecord::new(id);
        record.epochs.push(material());
        save_wrapped_epoch_record(root.path(), &record).unwrap();

        let after = load_device_certificate(root.path(), device(), persona())
            .unwrap()
            .unwrap();
        assert_eq!(after.certificate.id(), id);
        assert_eq!(
            CarryRef::of(encode_certificate(&after).unwrap().as_slice()),
            cert_ref
        );
    }

    #[test]
    fn a_private_read_grant_without_epoch_material_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let certificate = grant(&[ACTION_PRIVATE_READ]);

        let error = check_epoch_carriage(root.path(), &certificate, persona()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains(ACTION_PRIVATE_READ));
    }

    #[test]
    fn a_private_read_grant_with_matching_material_passes() {
        let root = tempfile::tempdir().unwrap();
        let certificate = grant(&[ACTION_PRIVATE_READ]);
        let mut record = WrappedEpochRecord::new(certificate.certificate.id());
        record.epochs.push(material());
        save_wrapped_epoch_record(root.path(), &record).unwrap();

        check_epoch_carriage(root.path(), &certificate, persona()).unwrap();
    }

    /// A grant that never promised private reading owes no carriage, so the
    /// invariant must not fire on it.
    #[test]
    fn a_transport_only_grant_needs_no_epoch_material() {
        let root = tempfile::tempdir().unwrap();
        let certificate = grant(&[ACTION_TRANSPORT_EGRESS]);

        assert!(!requires_epoch_material(&certificate));
        check_epoch_carriage(root.path(), &certificate, persona()).unwrap();
    }

    fn issue_set(actions: &[&str], personas: &[PersonaId]) -> DeviceGrantSet {
        identity::carry::issue_device_grant_set(
            MASTER_SEED,
            device(),
            DevicePublicKey([0x7a; 32]),
            actions,
            personas,
            60_000,
            NOW_MS,
        )
        .expect("issuing the grant set")
    }

    /// The station shape end to end: one file on disk, and reading it back
    /// without being told which personas to look for still recovers the set.
    #[test]
    fn a_station_set_round_trips_through_storage() {
        let root = tempfile::tempdir().unwrap();
        let set = issue_set(&[ACTION_TRANSPORT_EGRESS], &[]);
        save_device_grant_set(root.path(), device(), &set).unwrap();

        let back = load_device_grant_set(root.path(), device()).unwrap();
        assert_eq!(back, set);
        assert!(back.device.is_some());
        assert!(back.personas.is_empty());
    }

    #[test]
    fn a_mixed_set_round_trips_with_every_member() {
        let root = tempfile::tempdir().unwrap();
        let other = PersonaId::from_uuid(uuid::Uuid::from_u128(0x9004));
        let set = issue_set(
            &[ACTION_TRANSPORT_EGRESS, identity::carry::ACTION_IDENTITY_ACT],
            &[persona(), other],
        );
        save_device_grant_set(root.path(), device(), &set).unwrap();

        let back = load_device_grant_set(root.path(), device()).unwrap();
        assert_eq!(back, set);
        assert_eq!(back.personas.len(), 2);
        assert!(back.certificates().all(|c| c.verify()));
    }

    #[test]
    fn an_unknown_device_loads_as_an_empty_set() {
        let root = tempfile::tempdir().unwrap();
        let set = load_device_grant_set(root.path(), device()).unwrap();

        assert!(set.is_empty());
    }

    /// The roster keeps one ref per device, so that ref has to move whenever
    /// any member of the set does.
    #[test]
    fn the_set_ref_tracks_every_member() {
        let station = issue_set(&[ACTION_TRANSPORT_EGRESS], &[]);
        let with_persona = issue_set(
            &[ACTION_TRANSPORT_EGRESS, identity::carry::ACTION_IDENTITY_ACT],
            &[persona()],
        );

        assert_ne!(
            device_grant_set_ref(&station),
            device_grant_set_ref(&with_persona)
        );
        assert_eq!(
            device_grant_set_ref(&station),
            device_grant_set_ref(&station.clone())
        );
    }
}
