// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The wallet's revocation ledger.
//!
//! `personae::carry` mints the signed statements and `notochord` owns the
//! ledger that folds them; this module is where the wallet keeps its copy on
//! disk. That split is the reconciliation ruling in miniature: the grammar and
//! its evaluator are shared, and the ledger is the application's own.
//!
//! The roster's `revoked` list survives, demoted to what it always should have
//! been. It is a projection of the fold, kept because asking "is this device
//! revoked?" should not mean re-reading every certificate; the statements are
//! the record, and the list is the index.

use std::io;
use std::path::{Path, PathBuf};

use identity::carry::{DeviceGrantSet, revoke_device_grant_set};
use identity::delegation::SignedDelegationRevocation;
use notochord::RevocationLedger;
use p2panda_core::cbor::{decode_cbor, encode_cbor};

use crate::wallet_store::identity_grants_dir;

use super::{CarryRef, DeviceGrantError, DeviceId, load_device_grant_set};

/// `<data_root>/identity/grants/revocations.cbor`
pub fn revocation_ledger_path(data_root: &Path) -> PathBuf {
    identity_grants_dir(data_root).join("revocations.cbor")
}

/// Load the wallet's revocation ledger, empty when none has been written.
pub fn load_revocation_ledger(data_root: &Path) -> io::Result<RevocationLedger> {
    match std::fs::read(revocation_ledger_path(data_root)) {
        Ok(bytes) => decode_cbor(bytes.as_slice())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, DeviceGrantError::Decode)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(RevocationLedger::new()),
        Err(e) => Err(e),
    }
}

/// Persist the revocation ledger, returning its content ref.
pub fn save_revocation_ledger(
    data_root: &Path,
    ledger: &RevocationLedger,
) -> io::Result<CarryRef> {
    let bytes = encode_cbor(ledger)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, DeviceGrantError::Encode))?;
    let path = revocation_ledger_path(data_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &bytes)?;
    Ok(CarryRef::of(bytes.as_slice()))
}

/// Fold verified revocation statements into the wallet's ledger.
///
/// Returns how many were accepted. `RevocationLedger::fold` verifies each
/// signature and records nothing for a statement that fails, so a rejected
/// count is a real signal rather than a rounding error: statements arriving
/// from a peer are exactly as trustworthy as their signatures.
pub fn fold_revocations(
    data_root: &Path,
    statements: &[SignedDelegationRevocation],
) -> io::Result<usize> {
    let mut ledger = load_revocation_ledger(data_root)?;
    let accepted = statements
        .iter()
        .filter(|statement| ledger.fold(statement))
        .count();
    if accepted > 0 {
        save_revocation_ledger(data_root, &ledger)?;
    }
    Ok(accepted)
}

/// Mint and fold the statements withdrawing one device's whole grant set.
pub fn revoke_device_certificates(
    data_root: &Path,
    master_seed: [u8; 32],
    device_id: DeviceId,
    at_ms: u64,
) -> io::Result<Vec<SignedDelegationRevocation>> {
    let set = load_device_grant_set(data_root, device_id)?;
    if set.is_empty() {
        return Ok(Vec::new());
    }
    let statements = revoke_device_grant_set(master_seed, &set, at_ms)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fold_revocations(data_root, &statements)?;
    Ok(statements)
}

/// Whether the ledger withdraws every certificate a device holds.
///
/// Asks the statements rather than the roster list. Partial revocation is
/// visible here rather than flattened: a device whose persona authority was
/// withdrawn while its transport authority stands is not revoked, and saying
/// otherwise would be the flattening the split exists to remove.
pub fn device_is_fully_revoked(data_root: &Path, device_id: DeviceId) -> io::Result<bool> {
    let set = load_device_grant_set(data_root, device_id)?;
    if set.is_empty() {
        return Ok(false);
    }
    let ledger = load_revocation_ledger(data_root)?;
    Ok(set
        .certificates()
        .all(|certificate| ledger.revokes(&certificate.certificate)))
}

/// Which certificates in a set the ledger has withdrawn.
pub fn revoked_certificate_count(data_root: &Path, set: &DeviceGrantSet) -> io::Result<usize> {
    let ledger = load_revocation_ledger(data_root)?;
    Ok(set
        .certificates()
        .filter(|certificate| ledger.revokes(&certificate.certificate))
        .count())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use super::super::issue_remote_auth_device_grant;

    const AT_MS: u64 = 1_750_000_000_000;

    fn seeded_root(tag: &str) -> (std::path::PathBuf, [u8; 32]) {
        let root = temp_data_root(tag);
        let seed = crate::wallet_store::ensure_wallet_state(&root, fixture_persona(), "Studio PC")
            .unwrap();
        (root, seed)
    }

    #[test]
    fn an_unwritten_ledger_loads_empty() {
        let root = tempfile::tempdir().unwrap();
        assert!(load_revocation_ledger(root.path()).unwrap().is_empty());
    }

    #[test]
    fn revoking_a_device_withdraws_every_certificate_it_held() {
        let (root, seed) = seeded_root("m4-revoke");
        let spec = sample_remote_auth_spec();
        let set = issue_remote_auth_device_grant(&root, &spec).unwrap();

        let statements = revoke_device_certificates(&root, seed, spec.device_id, AT_MS).unwrap();
        assert_eq!(statements.len(), set.certificates().count());
        assert!(device_is_fully_revoked(&root, spec.device_id).unwrap());
    }

    #[test]
    fn the_ledger_survives_a_round_trip_through_disk() {
        let (root, seed) = seeded_root("m4-persist");
        let spec = sample_remote_auth_spec();
        issue_remote_auth_device_grant(&root, &spec).unwrap();
        revoke_device_certificates(&root, seed, spec.device_id, AT_MS).unwrap();

        let reloaded = load_revocation_ledger(&root).unwrap();
        assert!(!reloaded.is_empty());
        assert!(device_is_fully_revoked(&root, spec.device_id).unwrap());
    }

    /// A statement is worth its signature and nothing else. This is what lets
    /// revocations travel: a peer folds what verifies and ignores the rest.
    #[test]
    fn a_tampered_statement_is_refused_by_the_fold() {
        let (root, seed) = seeded_root("m4-tamper");
        let spec = sample_remote_auth_spec();
        issue_remote_auth_device_grant(&root, &spec).unwrap();
        let set = load_device_grant_set(&root, spec.device_id).unwrap();
        let mut statements = identity::carry::revoke_device_grant_set(seed, &set, AT_MS).unwrap();

        statements[0].revocation.at_ms = AT_MS + 1;
        let accepted = fold_revocations(&root, &statements[..1]).unwrap();

        assert_eq!(accepted, 0, "an altered statement must not fold");
        assert!(!device_is_fully_revoked(&root, spec.device_id).unwrap());
    }

    /// Partial revocation must stay visible. Withdrawing one persona's
    /// authority is exactly the operation the old single-signature grant
    /// could not express.
    #[test]
    fn withdrawing_one_certificate_does_not_revoke_the_device() {
        let (root, seed) = seeded_root("m4-partial");
        let mut spec = sample_remote_auth_spec();
        spec.scopes = vec!["transport.egress".into(), "identity.act".into()];
        issue_remote_auth_device_grant(&root, &spec).unwrap();
        let set = load_device_grant_set(&root, spec.device_id).unwrap();
        assert!(set.device.is_some() && !set.personas.is_empty());

        let statements = identity::carry::revoke_device_grant_set(seed, &set, AT_MS).unwrap();
        let persona_only: Vec<_> = statements
            .into_iter()
            .filter(|statement| {
                set.personas
                    .values()
                    .any(|c| c.certificate.id() == statement.revocation.certificate)
            })
            .collect();
        assert_eq!(fold_revocations(&root, &persona_only).unwrap(), 1);

        assert_eq!(revoked_certificate_count(&root, &set).unwrap(), 1);
        assert!(
            !device_is_fully_revoked(&root, spec.device_id).unwrap(),
            "transport authority still stands, so the device is not revoked"
        );
    }
}
