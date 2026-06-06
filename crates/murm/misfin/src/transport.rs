/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::path::Path;

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::pki_types::CertificateDer;

use super::*;
use super::helpers::*;

pub(super) fn load_or_create_identity(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinClientIdentity, String> {
    let Some(identity_root) = identity_root else {
        return generate_identity(spec);
    };

    fs::create_dir_all(identity_root)
        .map_err(|error| format!("Failed to create Misfin identity directory: {error}"))?;
    let path = identity_root.join(format!(
        "{}.json",
        sanitize_filename(&spec.address.as_addr_spec())
    ));

    if path.exists() {
        let content = fs::read_to_string(&path).map_err(|error| {
            format!(
                "Failed to read Misfin identity '{}': {error}",
                path.display()
            )
        })?;
        let persisted: PersistedMisfinIdentity =
            serde_json::from_str(&content).map_err(|error| {
                format!(
                    "Failed to parse Misfin identity '{}': {error}",
                    path.display()
                )
            })?;
        return Ok(MisfinClientIdentity {
            certificate_chain: vec![CertificateDer::from(decode_hex(
                &persisted.certificate_der_hex,
            )?)],
            private_key_der: decode_hex(&persisted.private_key_der_hex)?,
        });
    }

    let identity = generate_identity(spec)?;
    let persisted = PersistedMisfinIdentity {
        address: spec.address.as_addr_spec(),
        blurb: spec.blurb.clone(),
        certificate_der_hex: encode_hex(identity.certificate_chain[0].as_ref()),
        private_key_der_hex: encode_hex(&identity.private_key_der),
    };
    let content = serde_json::to_string_pretty(&persisted).map_err(|error| {
        format!(
            "Failed to serialize Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    fs::write(&path, content).map_err(|error| {
        format!(
            "Failed to persist Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    Ok(identity)
}

pub(super) fn identity_status_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinIdentityStatus, String> {
    let path = identity_root.map(|root| identity_path_for_spec(spec, root));
    let Some(path) = path else {
        return Ok(MisfinIdentityStatus {
            address: spec.address.as_addr_spec(),
            path: None,
            exists: false,
            blurb: spec.blurb.clone(),
            certificate_fingerprint: None,
        });
    };

    if !path.exists() {
        return Ok(MisfinIdentityStatus {
            address: spec.address.as_addr_spec(),
            path: Some(path),
            exists: false,
            blurb: spec.blurb.clone(),
            certificate_fingerprint: None,
        });
    }

    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed to read Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    let persisted: PersistedMisfinIdentity = serde_json::from_str(&content).map_err(|error| {
        format!(
            "Failed to parse Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    let certificate_der = decode_hex(&persisted.certificate_der_hex)?;

    Ok(MisfinIdentityStatus {
        address: persisted.address,
        path: Some(path),
        exists: true,
        blurb: persisted.blurb,
        certificate_fingerprint: Some(sha256_hex(&certificate_der)),
    })
}

pub(super) fn ensure_identity_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinIdentityStatus, String> {
    let _ = load_or_create_identity(spec, identity_root)?;
    identity_status_with_root(spec, identity_root)
}

pub(super) fn rotate_identity_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<MisfinIdentityStatus, String> {
    let _ = forget_identity_with_root(spec, identity_root)?;
    ensure_identity_with_root(spec, identity_root)
}

pub(super) fn forget_identity_with_root(
    spec: &MisfinIdentitySpec,
    identity_root: Option<&Path>,
) -> Result<bool, String> {
    let Some(identity_root) = identity_root else {
        return Ok(false);
    };
    let path = identity_path_for_spec(spec, identity_root);
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(&path).map_err(|error| {
        format!(
            "Failed to remove Misfin identity '{}': {error}",
            path.display()
        )
    })?;
    Ok(true)
}

pub(super) fn generate_identity(spec: &MisfinIdentitySpec) -> Result<MisfinClientIdentity, String> {
    let key_pair =
        KeyPair::generate().map_err(|error| format!("Misfin key generation failed: {error}"))?;
    let mut params = CertificateParams::new(vec![spec.address.host.clone()])
        .map_err(|error| format!("Misfin certificate params failed: {error}"))?;
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(
        DnType::CustomDnType(MISFIN_USER_ID_OID.to_vec()),
        spec.address.mailbox.clone(),
    );
    distinguished_name.push(
        DnType::CommonName,
        spec.blurb
            .clone()
            .unwrap_or_else(|| spec.address.as_addr_spec()),
    );
    params.distinguished_name = distinguished_name;
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(2099, 12, 31);

    let cert = params
        .self_signed(&key_pair)
        .map_err(|error| format!("Misfin identity certificate generation failed: {error}"))?;

    Ok(MisfinClientIdentity {
        certificate_chain: vec![CertificateDer::from(cert.der().to_vec())],
        private_key_der: key_pair.serialize_der(),
    })
}

/// Deterministically mint a Misfin client identity from a 32-byte Ed25519 `seed`.
///
/// Unlike [`generate_identity`] (a random ECDSA P-256 key that must be persisted),
/// this derives the whole identity from `seed` + `spec`: an Ed25519 key from the
/// seed, then a self-signed cert with a **fixed serial** over the same fixed
/// validity + DN. Ed25519 signs deterministically (RFC 8032), so the same seed +
/// address reproduce a byte-identical certificate and SHA-256 fingerprint.
pub(super) fn deterministic_identity(
    seed: &[u8; 32],
    spec: &MisfinIdentitySpec,
) -> Result<MisfinClientIdentity, String> {
    // Import the Ed25519 key from its PKCS#8 wrapper; rcgen infers Ed25519 from
    // the embedded algorithm OID.
    let key_pair = KeyPair::try_from(ed25519_pkcs8_der(seed).as_slice())
        .map_err(|error| format!("Misfin Ed25519 key import failed: {error}"))?;

    let mut params = CertificateParams::new(vec![spec.address.host.clone()])
        .map_err(|error| format!("Misfin certificate params failed: {error}"))?;
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(
        DnType::CustomDnType(MISFIN_USER_ID_OID.to_vec()),
        spec.address.mailbox.clone(),
    );
    distinguished_name.push(
        DnType::CommonName,
        spec.blurb
            .clone()
            .unwrap_or_else(|| spec.address.as_addr_spec()),
    );
    params.distinguished_name = distinguished_name;
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(2099, 12, 31);
    // Fix the serial: rcgen randomises it by default, which would churn the
    // fingerprint. The key + DN already differ per identity, so a constant is fine.
    params.serial_number = Some(rcgen::SerialNumber::from(1u64));

    let cert = params
        .self_signed(&key_pair)
        .map_err(|error| format!("Misfin identity certificate generation failed: {error}"))?;

    Ok(MisfinClientIdentity {
        certificate_chain: vec![CertificateDer::from(cert.der().to_vec())],
        private_key_der: key_pair.serialize_der(),
    })
}

/// The PKCS#8 v1 (RFC 8410) DER encoding of an Ed25519 private key from its
/// 32-byte `seed`: a fixed 16-byte prefix followed by the seed.
fn ed25519_pkcs8_der(seed: &[u8; 32]) -> Vec<u8> {
    const PREFIX: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    let mut der = Vec::with_capacity(PREFIX.len() + seed.len());
    der.extend_from_slice(&PREFIX);
    der.extend_from_slice(seed);
    der
}

