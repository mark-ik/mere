/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-owned remote-auth pairing artifacts for the omnibar admin seam.
//!
//! The wallet runtime already has typed ticket / response / grant helpers; this
//! module gives Meerkat a concrete on-disk handoff shape until the dedicated QR /
//! SAS pairing UI exists. The shell mints a ticket artifact set under
//! `<mere_root>/pairing/`, and the host later consumes a filled response artifact
//! from the same directory (or anywhere else on disk).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use identity::{IdentityProvider, InMemoryProvider};
use serde::{Deserialize, Serialize};
use session_runtime::{
    DeviceExposure, DeviceId, DevicePublicKey, RemoteAuthPairingResponse, RemoteAuthPairingTicket,
    RemoteAuthPairingTicketRequest, build_remote_auth_enrollment_bundle,
    decode_remote_auth_enrollment_bundle, decode_remote_auth_pairing_ticket,
    derive_remote_auth_pairing_material, encode_remote_auth_enrollment_bundle,
    encode_remote_auth_pairing_ticket, ensure_local_device_identity,
    format_remote_auth_pairing_code, install_remote_auth_enrollment_bundle,
    install_remote_auth_enrollment_bundle_with_wrapping_key, load_identity_seed,
    load_remote_auth_wrapping_key_bridge, mint_remote_auth_pairing_ticket,
};
use uuid::Uuid;

const PAIRING_DIR: &str = "pairing";
const OFFER_SUMMARY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RemoteAuthPairingOfferSummary {
    schema_version: u32,
    ticket_id: Uuid,
    issued_at_ms: u64,
    #[serde(default)]
    expires_at_ms: Option<u64>,
    personas: Vec<Uuid>,
    scopes: Vec<String>,
    attenuations: Vec<String>,
    delegator_pubkey_hex: String,
    pairing_code: String,
    ticket_cbor_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RemoteAuthPairingResponseArtifact {
    #[serde(default)]
    ticket_id: Option<Uuid>,
    device_id: String,
    delegatee_pubkey_hex: String,
    label: String,
    #[serde(default = "default_response_exposure")]
    exposure: String,
    #[serde(default)]
    short_auth_string: Option<String>,
}

fn default_response_exposure() -> String {
    "hidden".to_string()
}

/// Paths and display material emitted for one freshly minted pairing offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteAuthPairingOfferArtifacts {
    pub ticket: RemoteAuthPairingTicket,
    pub ticket_path: PathBuf,
    pub summary_path: PathBuf,
    pub response_template_path: PathBuf,
    pub pairing_code: String,
}

/// The response artifact a delegated device writes after consuming a pairing offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteAuthPairingResponseArtifacts {
    pub response_path: PathBuf,
    pub response: RemoteAuthPairingResponse,
    pub short_auth_string: String,
}

/// A read-only pairing preview both sides can compare before issuing a grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteAuthPairingPreview {
    pub response: RemoteAuthPairingResponse,
    pub short_auth_string: String,
}

/// Mint a remote-auth pairing ticket and persist the host-owned artifacts the
/// current shell seam exchanges.
pub(crate) fn mint_remote_auth_pairing_offer(
    data_root: &Path,
    request: &RemoteAuthPairingTicketRequest,
) -> io::Result<RemoteAuthPairingOfferArtifacts> {
    let ticket = mint_remote_auth_pairing_ticket(request);
    let ticket_bytes = encode_remote_auth_pairing_ticket(&ticket)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let pairing_code = format_remote_auth_pairing_code(ticket.pairing_secret);
    let seed = load_identity_seed(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "wallet root missing identity/master.seed; bootstrap the wallet first",
        )
    })?;
    let provider = InMemoryProvider::from_seed(seed);

    let dir = pairing_dir(data_root);
    fs::create_dir_all(&dir)?;
    let stem = ticket.ticket_id.to_string();
    let ticket_path = dir.join(format!("{stem}.ticket.cbor"));
    let summary_path = dir.join(format!("{stem}.ticket.json"));
    let response_template_path = dir.join(format!("{stem}.response.template.json"));

    fs::write(&ticket_path, &ticket_bytes)?;
    save_json_pretty(
        &summary_path,
        &RemoteAuthPairingOfferSummary {
            schema_version: OFFER_SUMMARY_SCHEMA_VERSION,
            ticket_id: ticket.ticket_id,
            issued_at_ms: ticket.issued_at_ms,
            expires_at_ms: ticket.expires_at_ms,
            personas: ticket.personas.iter().map(|id| *id.as_uuid()).collect(),
            scopes: ticket.scopes.clone(),
            attenuations: ticket.attenuations.clone(),
            delegator_pubkey_hex: hex::encode(provider.master_public_key().to_bytes()),
            pairing_code: pairing_code.clone(),
            ticket_cbor_hex: hex::encode(&ticket_bytes),
        },
    )?;
    save_json_pretty(
        &response_template_path,
        &RemoteAuthPairingResponseArtifact {
            ticket_id: Some(ticket.ticket_id),
            device_id: String::new(),
            delegatee_pubkey_hex: String::new(),
            label: String::new(),
            exposure: default_response_exposure(),
            short_auth_string: None,
        },
    )?;

    Ok(RemoteAuthPairingOfferArtifacts {
        ticket,
        ticket_path,
        summary_path,
        response_template_path,
        pairing_code,
    })
}

/// Prepare a remote-auth response artifact on the delegated device side.
pub(crate) fn prepare_remote_auth_pairing_response(
    data_root: &Path,
    ticket_path: &Path,
    device_label: &str,
) -> io::Result<RemoteAuthPairingResponseArtifacts> {
    let offer = load_remote_auth_pairing_offer(ticket_path)?;
    ensure_ticket_not_expired(&offer.ticket)?;
    let local = ensure_local_device_identity(data_root, device_label)?;
    let response = RemoteAuthPairingResponse {
        device_id: local.device_id,
        delegatee_pubkey: local.public_key(),
        label: local.label.clone(),
        exposure: DeviceExposure::HiddenClient,
    };
    let pairing = derive_remote_auth_pairing_material(
        &offer.ticket.pairing_secret,
        offer.delegator_pubkey,
        response.delegatee_pubkey,
        response.device_id,
    )
    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let dir = pairing_dir(data_root);
    fs::create_dir_all(&dir)?;
    fs::write(
        cached_pairing_ticket_path(data_root, offer.ticket.ticket_id),
        encode_remote_auth_pairing_ticket(&offer.ticket)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?,
    )?;
    let response_path = dir.join(format!(
        "{}.{}.response.json",
        offer.ticket.ticket_id,
        response.device_id.as_uuid()
    ));
    save_json_pretty(
        &response_path,
        &RemoteAuthPairingResponseArtifact {
            ticket_id: Some(offer.ticket.ticket_id),
            device_id: response.device_id.as_uuid().to_string(),
            delegatee_pubkey_hex: hex::encode(response.delegatee_pubkey.0),
            label: response.label.clone(),
            exposure: default_response_exposure(),
            short_auth_string: Some(pairing.short_auth_string.clone()),
        },
    )?;
    Ok(RemoteAuthPairingResponseArtifacts {
        response_path,
        response,
        short_auth_string: pairing.short_auth_string,
    })
}

/// Preview the short auth string on the delegator side without issuing the grant.
pub(crate) fn preview_remote_auth_pairing(
    data_root: &Path,
    ticket_path: &Path,
    response_path: &Path,
) -> io::Result<RemoteAuthPairingPreview> {
    let offer = load_remote_auth_pairing_offer(ticket_path)?;
    ensure_ticket_not_expired(&offer.ticket)?;
    let response =
        load_remote_auth_pairing_response_for_ticket(response_path, offer.ticket.ticket_id)?;
    let seed = load_identity_seed(data_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "wallet root missing identity/master.seed; bootstrap the wallet first",
        )
    })?;
    let provider = InMemoryProvider::from_seed(seed);
    let pairing = derive_remote_auth_pairing_material(
        &offer.ticket.pairing_secret,
        DevicePublicKey::from(provider.master_public_key()),
        response.delegatee_pubkey,
        response.device_id,
    )
    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(RemoteAuthPairingPreview {
        response,
        short_auth_string: pairing.short_auth_string,
    })
}

/// Export the post-acceptance enrollment bundle the delegator hands back to the
/// delegated device.
pub(crate) fn export_remote_auth_enrollment_bundle(
    data_root: &Path,
    ticket_id: Uuid,
    device_id: DeviceId,
) -> io::Result<PathBuf> {
    let mut bundle = build_remote_auth_enrollment_bundle(data_root, device_id)?;
    bundle.ticket_id = Some(ticket_id);
    let bytes = encode_remote_auth_enrollment_bundle(&bundle)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let dir = pairing_dir(data_root);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{ticket_id}.{}.enrollment.cbor",
        device_id.as_uuid()
    ));
    fs::write(&path, bytes)?;
    Ok(path)
}

/// Export a fresh enrollment bundle for an already-enrolled delegated device.
///
/// This is the manual/admin seam used after later grant refreshes, such as
/// private-epoch rotation on revocation. Pairing-backed `private.read` grants
/// need the original ticket id retained in the temporary wrapping-key bridge so
/// the delegatee can recover the same cached pairing ticket on install.
pub(crate) fn export_remote_auth_enrollment_bundle_for_device(
    data_root: &Path,
    device_id: DeviceId,
) -> io::Result<PathBuf> {
    let mut bundle = build_remote_auth_enrollment_bundle(data_root, device_id)?;
    if !bundle.grant.payload.wrapped_private_epochs.is_empty() {
        let ticket_id = load_remote_auth_wrapping_key_bridge(data_root)?
            .and_then(|bridge| {
                bridge
                    .keys
                    .into_iter()
                    .find(|known| known.device_id == device_id)
            })
            .and_then(|known| known.ticket_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "remote-auth enrollment export missing pairing ticket id for {}",
                        device_id.as_uuid()
                    ),
                )
            })?;
        bundle.ticket_id = Some(ticket_id);
    }
    let bytes = encode_remote_auth_enrollment_bundle(&bundle)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let dir = pairing_dir(data_root);
    fs::create_dir_all(&dir)?;
    let stem = bundle
        .ticket_id
        .map(|ticket_id| ticket_id.to_string())
        .unwrap_or_else(|| "manual".to_string());
    let path = dir.join(format!("{stem}.{}.enrollment.cbor", device_id.as_uuid()));
    fs::write(&path, bytes)?;
    Ok(path)
}

/// Install a delegator-produced enrollment bundle on the delegated device side.
///
/// When the bundle carries wrapped private epochs, this reloads the cached local
/// pairing ticket so the delegatee can recover the pairing-derived wrapping key
/// and seed its temporary plaintext epoch bridge.
pub(crate) fn install_remote_auth_enrollment_artifact(
    data_root: &Path,
    bundle_path: &Path,
) -> io::Result<()> {
    let bytes = fs::read(bundle_path)?;
    let bundle = decode_remote_auth_enrollment_bundle(&bytes)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if bundle.grant.payload.wrapped_private_epochs.is_empty() {
        return install_remote_auth_enrollment_bundle(data_root, &bundle);
    }
    let ticket_id = bundle.ticket_id.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "enrollment bundle carries wrapped private epochs but no pairing ticket id",
        )
    })?;
    let ticket = decode_remote_auth_pairing_ticket(&fs::read(cached_pairing_ticket_path(
        data_root, ticket_id,
    ))?)
    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let pairing = derive_remote_auth_pairing_material(
        &ticket.pairing_secret,
        bundle.grant.payload.delegator_pubkey,
        bundle.grant.payload.delegatee_pubkey,
        bundle.grant.payload.device_id,
    )
    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    install_remote_auth_enrollment_bundle_with_wrapping_key(
        data_root,
        &bundle,
        pairing.wrapping_key,
    )
}

/// Load a pairing ticket from either the raw `.ticket.cbor` artifact or the JSON
/// summary wrapper that carries the same CBOR bytes as hex.
pub(crate) fn load_remote_auth_pairing_ticket(path: &Path) -> io::Result<RemoteAuthPairingTicket> {
    Ok(load_remote_auth_pairing_offer(path)?.ticket)
}

/// Load and validate the ticket/response pair the host consumes during grant
/// issuance. This enforces ticket freshness and response-to-ticket coherence.
pub(crate) fn load_remote_auth_pairing_acceptance(
    ticket_path: &Path,
    response_path: &Path,
) -> io::Result<(RemoteAuthPairingTicket, RemoteAuthPairingResponse)> {
    let offer = load_remote_auth_pairing_offer(ticket_path)?;
    ensure_ticket_not_expired(&offer.ticket)?;
    let response =
        load_remote_auth_pairing_response_for_ticket(response_path, offer.ticket.ticket_id)?;
    Ok((offer.ticket, response))
}

/// Load a filled remote-auth response artifact from JSON and convert it to the
/// typed runtime response.
pub(crate) fn load_remote_auth_pairing_response(
    path: &Path,
) -> io::Result<RemoteAuthPairingResponse> {
    response_from_artifact(load_remote_auth_pairing_response_artifact(path)?)
}

fn load_remote_auth_pairing_response_for_ticket(
    path: &Path,
    ticket_id: Uuid,
) -> io::Result<RemoteAuthPairingResponse> {
    let artifact = load_remote_auth_pairing_response_artifact(path)?;
    match artifact.ticket_id {
        Some(found) if found == ticket_id => {}
        Some(found) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "pairing response ticket_id {} does not match ticket {}",
                    found, ticket_id
                ),
            ));
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("pairing response is missing ticket_id for ticket {ticket_id}"),
            ));
        }
    }
    response_from_artifact(artifact)
}

fn load_remote_auth_pairing_response_artifact(
    path: &Path,
) -> io::Result<RemoteAuthPairingResponseArtifact> {
    load_json(path)
}

fn response_from_artifact(
    artifact: RemoteAuthPairingResponseArtifact,
) -> io::Result<RemoteAuthPairingResponse> {
    let device_id = Uuid::parse_str(artifact.device_id.trim()).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("pairing response device_id is not a UUID: {err}"),
        )
    })?;
    let pubkey_bytes = hex::decode(artifact.delegatee_pubkey_hex.trim()).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("pairing response delegatee_pubkey_hex is not valid hex: {err}"),
        )
    })?;
    let pubkey_bytes: [u8; 32] = pubkey_bytes.as_slice().try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "pairing response delegatee_pubkey_hex must decode to 32 bytes",
        )
    })?;
    let label = artifact.label.trim();
    if label.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "pairing response label must not be empty",
        ));
    }
    Ok(RemoteAuthPairingResponse {
        device_id: DeviceId::from_uuid(device_id),
        delegatee_pubkey: DevicePublicKey(pubkey_bytes),
        label: label.to_string(),
        exposure: parse_device_exposure(&artifact.exposure)?,
    })
}

fn load_remote_auth_pairing_offer(path: &Path) -> io::Result<RemoteAuthPairingOffer> {
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        let summary: RemoteAuthPairingOfferSummary = load_json(path)?;
        let ticket_bytes = hex::decode(summary.ticket_cbor_hex).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("pairing ticket summary hex decode failed: {err}"),
            )
        })?;
        let delegator_pubkey = parse_pubkey_hex(
            &summary.delegator_pubkey_hex,
            "pairing ticket summary delegator_pubkey_hex",
        )?;
        let ticket = decode_remote_auth_pairing_ticket(&ticket_bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        return Ok(RemoteAuthPairingOffer {
            ticket,
            delegator_pubkey,
        });
    }
    let ticket = decode_remote_auth_pairing_ticket(&fs::read(path)?)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let summary_path = pairing_summary_path_for_ticket(path)?;
    let summary: RemoteAuthPairingOfferSummary = load_json(&summary_path)?;
    let delegator_pubkey = parse_pubkey_hex(
        &summary.delegator_pubkey_hex,
        "pairing ticket summary delegator_pubkey_hex",
    )?;
    Ok(RemoteAuthPairingOffer {
        ticket,
        delegator_pubkey,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteAuthPairingOffer {
    ticket: RemoteAuthPairingTicket,
    delegator_pubkey: DevicePublicKey,
}

fn ensure_ticket_not_expired(ticket: &RemoteAuthPairingTicket) -> io::Result<()> {
    let now_ms = unix_time_ms()?;
    if matches!(ticket.expires_at_ms, Some(expires_at_ms) if expires_at_ms <= now_ms) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("remote-auth pairing ticket {} is expired", ticket.ticket_id),
        ));
    }
    Ok(())
}

fn unix_time_ms() -> io::Result<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| io::Error::other(format!("system clock before unix epoch: {err}")))?;
    u64::try_from(now.as_millis()).map_err(|_| io::Error::other("unix time overflowed u64"))
}

fn pairing_dir(data_root: &Path) -> PathBuf {
    data_root.join(PAIRING_DIR)
}

fn cached_pairing_ticket_path(data_root: &Path, ticket_id: Uuid) -> PathBuf {
    pairing_dir(data_root).join(format!("{ticket_id}.ticket.cbor"))
}

fn pairing_summary_path_for_ticket(ticket_path: &Path) -> io::Result<PathBuf> {
    let parent = ticket_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ticket path has no parent directory: {ticket_path:?}"),
        )
    })?;
    let stem = ticket_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("ticket path has no valid file stem: {ticket_path:?}"),
            )
        })?;
    Ok(parent.join(format!("{stem}.json")))
}

fn parse_pubkey_hex(value: &str, context: &str) -> io::Result<DevicePublicKey> {
    let bytes = hex::decode(value.trim()).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{context} is not valid hex: {err}"),
        )
    })?;
    let bytes: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{context} must decode to 32 bytes"),
        )
    })?;
    Ok(DevicePublicKey(bytes))
}

fn parse_device_exposure(value: &str) -> io::Result<DeviceExposure> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "hidden" | "hidden_client" | "hidden-client" => Ok(DeviceExposure::HiddenClient),
        "egress" | "exposed" | "exposed_egress" | "exposed-egress" => {
            Ok(DeviceExposure::ExposedEgress)
        }
        other => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown device exposure: {other}"),
        )),
    }
}

fn save_json_pretty<T>(path: &Path, value: &T) -> io::Result<()>
where
    T: Serialize,
{
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path {path:?} has no parent directory"),
        )
    })?;
    fs::create_dir_all(parent)?;
    let json = serde_json::to_string_pretty(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    fs::write(path, json)
}

fn load_json<T>(path: &Path) -> io::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use identity::PersonaId;
    use session_runtime::{
        DeviceId, DeviceMode, ensure_wallet_state, issue_remote_auth_device_grant_from_ticket,
        load_current_private_epoch, load_identity_wallet, load_local_device_identity,
        load_persona_wallet, load_signed_device_grant,
    };
    use tempfile::tempdir;

    #[test]
    fn minted_offer_round_trips_from_cbor_and_summary_json() {
        let dir = tempdir().expect("tempdir");
        let request = RemoteAuthPairingTicketRequest {
            issued_at_ms: 1234,
            expires_at_ms: Some(5678),
            personas: vec![PersonaId::new()],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
        };

        let artifacts = mint_remote_auth_pairing_offer(dir.path(), &request).expect("offer");
        assert!(artifacts.ticket_path.is_file());
        assert!(artifacts.summary_path.is_file());
        assert!(artifacts.response_template_path.is_file());
        assert!(!artifacts.pairing_code.is_empty());

        let from_cbor = load_remote_auth_pairing_ticket(&artifacts.ticket_path).expect("cbor");
        let from_json = load_remote_auth_pairing_ticket(&artifacts.summary_path).expect("json");
        assert_eq!(from_cbor, artifacts.ticket);
        assert_eq!(from_json, artifacts.ticket);
    }

    #[test]
    fn response_artifact_parses_hex_pubkey_and_exposure_aliases() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("response.json");
        save_json_pretty(
            &path,
            &RemoteAuthPairingResponseArtifact {
                ticket_id: None,
                device_id: Uuid::nil().to_string(),
                delegatee_pubkey_hex: "11".repeat(32),
                label: "Test phone".into(),
                exposure: "egress".into(),
                short_auth_string: None,
            },
        )
        .expect("write");

        let response = load_remote_auth_pairing_response(&path).expect("response");
        assert_eq!(response.device_id, DeviceId::from_uuid(Uuid::nil()));
        assert_eq!(response.delegatee_pubkey, DevicePublicKey([0x11; 32]));
        assert_eq!(response.label, "Test phone");
        assert_eq!(response.exposure, DeviceExposure::ExposedEgress);
    }

    #[test]
    fn prepared_response_uses_the_local_device_identity_and_derives_sas() {
        let dir = tempdir().expect("tempdir");
        let persona = PersonaId::new();
        ensure_wallet_state(dir.path(), persona, "Delegator").expect("wallet");
        let request = RemoteAuthPairingTicketRequest {
            issued_at_ms: 1234,
            expires_at_ms: None,
            personas: vec![persona],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
        };
        let offer = mint_remote_auth_pairing_offer(dir.path(), &request).expect("offer");

        let response =
            prepare_remote_auth_pairing_response(dir.path(), &offer.summary_path, "Pocket Meerkat")
                .expect("response");
        let local = load_local_device_identity(dir.path())
            .expect("load")
            .expect("identity");
        assert_eq!(response.response.device_id, local.device_id);
        assert_eq!(response.response.delegatee_pubkey, local.public_key());
        assert_eq!(response.response.label, "Pocket Meerkat");
        assert_eq!(response.response.exposure, DeviceExposure::HiddenClient);
        assert_eq!(response.short_auth_string.len(), 6);
        assert!(response.response_path.is_file());
        assert!(cached_pairing_ticket_path(dir.path(), offer.ticket.ticket_id).is_file());
    }

    #[test]
    fn prepare_response_rejects_expired_ticket() {
        let dir = tempdir().expect("tempdir");
        let persona = PersonaId::new();
        ensure_wallet_state(dir.path(), persona, "Delegator").expect("wallet");
        let request = RemoteAuthPairingTicketRequest {
            issued_at_ms: 1234,
            expires_at_ms: Some(1),
            personas: vec![persona],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
        };
        let offer = mint_remote_auth_pairing_offer(dir.path(), &request).expect("offer");

        let err =
            prepare_remote_auth_pairing_response(dir.path(), &offer.summary_path, "Pocket Meerkat")
                .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn pairing_preview_matches_the_grant_issuance_sas() {
        let dir = tempdir().expect("tempdir");
        let persona = PersonaId::new();
        ensure_wallet_state(dir.path(), persona, "Delegator").expect("wallet");
        let request = RemoteAuthPairingTicketRequest {
            issued_at_ms: 1234,
            expires_at_ms: None,
            personas: vec![persona],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
        };
        let offer = mint_remote_auth_pairing_offer(dir.path(), &request).expect("offer");
        let response =
            prepare_remote_auth_pairing_response(dir.path(), &offer.summary_path, "Pocket Meerkat")
                .expect("response");

        let preview =
            preview_remote_auth_pairing(dir.path(), &offer.summary_path, &response.response_path)
                .expect("preview");
        let (_grant, pairing) = issue_remote_auth_device_grant_from_ticket(
            dir.path(),
            &offer.ticket,
            &response.response,
            Vec::new(),
        )
        .expect("grant");
        assert_eq!(preview.short_auth_string, response.short_auth_string);
        assert_eq!(preview.short_auth_string, pairing.short_auth_string);
    }

    #[test]
    fn pairing_acceptance_rejects_response_for_another_ticket() {
        let dir = tempdir().expect("tempdir");
        let persona = PersonaId::new();
        ensure_wallet_state(dir.path(), persona, "Delegator").expect("wallet");
        let request = RemoteAuthPairingTicketRequest {
            issued_at_ms: 1234,
            expires_at_ms: None,
            personas: vec![persona],
            scopes: vec!["identity.act".into()],
            attenuations: vec!["no-subdelegation".into()],
        };
        let offer = mint_remote_auth_pairing_offer(dir.path(), &request).expect("offer");
        let other_offer =
            mint_remote_auth_pairing_offer(dir.path(), &request).expect("other offer");
        let response = prepare_remote_auth_pairing_response(
            dir.path(),
            &other_offer.summary_path,
            "Pocket Meerkat",
        )
        .expect("response");

        let err = load_remote_auth_pairing_acceptance(&offer.summary_path, &response.response_path)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn enrollment_bundle_artifact_round_trips_between_delegator_and_delegatee_roots() {
        let delegator = tempdir().expect("delegator tempdir");
        let delegatee = tempdir().expect("delegatee tempdir");
        let persona = PersonaId::new();
        ensure_wallet_state(delegator.path(), persona, "Delegator").expect("wallet");
        let request = RemoteAuthPairingTicketRequest {
            issued_at_ms: 1234,
            expires_at_ms: None,
            personas: vec![persona],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
        };
        let offer = mint_remote_auth_pairing_offer(delegator.path(), &request).expect("offer");
        let response = prepare_remote_auth_pairing_response(
            delegatee.path(),
            &offer.summary_path,
            "Pocket Meerkat",
        )
        .expect("response");
        let current_epoch = load_current_private_epoch(delegator.path(), persona)
            .expect("epoch load")
            .expect("epoch bridge");

        issue_remote_auth_device_grant_from_ticket(
            delegator.path(),
            &offer.ticket,
            &response.response,
            vec![session_runtime::PrivateEpochPlaintext {
                persona_id: persona,
                epoch_id: current_epoch.epoch_id,
                epoch_secret: current_epoch.epoch_secret.clone(),
            }],
        )
        .expect("grant");
        let bundle_path = export_remote_auth_enrollment_bundle(
            delegator.path(),
            offer.ticket.ticket_id,
            response.response.device_id,
        )
        .expect("bundle");
        assert!(bundle_path.is_file());

        install_remote_auth_enrollment_artifact(delegatee.path(), &bundle_path).expect("install");

        let grant = load_signed_device_grant(delegatee.path(), response.response.device_id)
            .expect("grant load")
            .expect("grant file");
        assert_eq!(grant.payload.device_id, response.response.device_id);
        let persona_wallet = load_persona_wallet(delegatee.path(), persona)
            .expect("persona load")
            .expect("persona wallet");
        assert_eq!(persona_wallet.persona_id, persona);
        let restored_epoch = load_current_private_epoch(delegatee.path(), persona)
            .expect("delegatee epoch load")
            .expect("delegatee epoch bridge");
        assert_eq!(restored_epoch.epoch_id, current_epoch.epoch_id);
        assert_eq!(restored_epoch.epoch_secret, current_epoch.epoch_secret);
        let identity_wallet = load_identity_wallet(delegatee.path())
            .expect("identity load")
            .expect("identity wallet");
        assert!(
            identity_wallet
                .personas
                .iter()
                .any(|known| known.persona_id == persona)
        );
        let local = load_local_device_identity(delegatee.path())
            .expect("local identity load")
            .expect("local identity");
        let roster = session_runtime::load_device_roster(delegatee.path())
            .expect("roster load")
            .expect("roster");
        assert!(
            roster
                .devices
                .iter()
                .any(|record| record.device_id == local.device_id
                    && record.mode == DeviceMode::RemoteAuth)
        );
    }

    #[test]
    fn export_enrollment_bundle_for_device_reuses_the_original_pairing_ticket() {
        let delegator = tempdir().expect("delegator tempdir");
        let delegatee = tempdir().expect("delegatee tempdir");
        let persona = PersonaId::new();
        ensure_wallet_state(delegator.path(), persona, "Delegator").expect("wallet");
        let request = RemoteAuthPairingTicketRequest {
            issued_at_ms: 1234,
            expires_at_ms: None,
            personas: vec![persona],
            scopes: vec!["identity.act".into(), "private.read".into()],
            attenuations: vec!["no-subdelegation".into()],
        };
        let offer = mint_remote_auth_pairing_offer(delegator.path(), &request).expect("offer");
        let response = prepare_remote_auth_pairing_response(
            delegatee.path(),
            &offer.summary_path,
            "Pocket Meerkat",
        )
        .expect("response");
        let current_epoch = load_current_private_epoch(delegator.path(), persona)
            .expect("epoch load")
            .expect("epoch bridge");
        issue_remote_auth_device_grant_from_ticket(
            delegator.path(),
            &offer.ticket,
            &response.response,
            vec![session_runtime::PrivateEpochPlaintext {
                persona_id: persona,
                epoch_id: current_epoch.epoch_id,
                epoch_secret: current_epoch.epoch_secret.clone(),
            }],
        )
        .expect("grant");

        let bundle_path = export_remote_auth_enrollment_bundle_for_device(
            delegator.path(),
            response.response.device_id,
        )
        .expect("bundle");
        assert!(bundle_path.is_file());

        let bundle = decode_remote_auth_enrollment_bundle(&fs::read(&bundle_path).expect("read"))
            .expect("decode");
        assert_eq!(bundle.ticket_id, Some(offer.ticket.ticket_id));
        assert_eq!(bundle.grant.payload.device_id, response.response.device_id);
    }
}
