//! Transfer offers: how a source tells a destination device that a transfer is
//! waiting, over the personal graph the two already share.
//!
//! An offer is a summary, not the transfer. Bytes stay in the source's blob
//! store; the authoritative description is the manifest blob the summary
//! names. The counts here are for display, so a destination shows what is
//! coming before committing to fetch it. `apply_transfer` reads the manifest.
//!
//! The offer rides on a synthetic node, because a facet needs a node to hang
//! on. That node is addressed `mere://transfer/<destination>/<source>/<id>`,
//! naming both endpoints so the source sees its outgoing offers on the same
//! footing as the destination sees its incoming ones.
//!
//! Addressing is presentation, not confidentiality. The personal lane carries
//! plaintext to every device the roster admits, so a device that filters an
//! offer out still receives and stores it. What bounds who can read a personal
//! graph is the roster, not this filter.

use chartulary::FacetId;
use eidetic::Hash;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::personal_sync::{PersonalGraphEvent, SyncProjection, SyntheticAddressRule};
use crate::product::decode_engram;
use crate::transfer::{TransferEndpointV1, TransferManifestV1, TransferOperation};

pub const TRANSFER_OFFER_FACET: &str = "graphshell.transfer-offer/v1";
pub const TRANSFER_ADDRESS_PREFIX: &str = "mere://transfer/";

/// One waiting transfer, as the destination sees it before fetching anything.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferOfferV1 {
    pub schema: String,
    pub transfer_id: Uuid,
    pub operation: TransferOperation,
    pub source: TransferEndpointV1,
    pub destination: TransferEndpointV1,
    /// The pairing this offer was made under, not the device. Unpairing mints
    /// a fresh id, so a re-pair cannot revive offers queued under the old one.
    pub pairing_id: String,
    /// Where the authoritative manifest is. A destination fetches this first,
    /// then the blobs the manifest names.
    pub manifest_blob: Hash,
    pub manifest_byte_len: u64,
    /// Advisory sizes, for deciding whether to accept. The manifest governs.
    pub nodes: u64,
    pub relations: u64,
    pub blobs: u64,
    pub blob_bytes: u64,
    pub offered_at_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum OfferError {
    #[error("transfer endpoint has no usable device segment: {0}")]
    DeviceSegment(String),
    #[error("transfer manifest selection could not be read: {0}")]
    Selection(String),
}

/// How the sync layer treats offer carriers: they follow the offer facet, and
/// they project only where the address names the local device.
pub fn transfer_offer_rule() -> SyntheticAddressRule {
    SyntheticAddressRule {
        prefix: TRANSFER_ADDRESS_PREFIX.to_string(),
        facet: TRANSFER_OFFER_FACET.to_string(),
        device_scoped: true,
    }
}

/// The device segment of an endpoint address, as it appears in an offer
/// address and in [`SyncSelection::with_local_device`].
///
/// Deliberately not hex-only: a device address is `personae://device/<key>`
/// and hosts use both raw keys and friendly names there.
///
/// [`SyncSelection::with_local_device`]: crate::personal_sync::SyncSelection::with_local_device
pub fn device_segment(device: &str) -> Option<&str> {
    let key = device.rsplit('/').next()?.trim();
    (!key.is_empty() && !key.contains(char::is_whitespace)).then_some(key)
}

pub fn offer_address(
    source_device: &str,
    destination_device: &str,
    transfer_id: Uuid,
) -> Result<String, OfferError> {
    let source = device_segment(source_device)
        .ok_or_else(|| OfferError::DeviceSegment(source_device.to_string()))?;
    let destination = device_segment(destination_device)
        .ok_or_else(|| OfferError::DeviceSegment(destination_device.to_string()))?;
    Ok(format!(
        "{TRANSFER_ADDRESS_PREFIX}{destination}/{source}/{transfer_id}"
    ))
}

/// Summarize a prepared manifest, given where its bytes were staged.
pub fn offer_for(
    manifest: &TransferManifestV1,
    manifest_blob: Hash,
    manifest_byte_len: u64,
    pairing_id: impl Into<String>,
    offered_at_ms: u64,
) -> Result<TransferOfferV1, OfferError> {
    let product = decode_engram(&manifest.selection.payload)
        .map_err(|error| OfferError::Selection(error.to_string()))?;
    Ok(TransferOfferV1 {
        schema: TRANSFER_OFFER_FACET.to_string(),
        transfer_id: manifest.transfer_id,
        operation: manifest.operation,
        source: manifest.source.clone(),
        destination: manifest.destination.clone(),
        pairing_id: pairing_id.into(),
        manifest_blob,
        manifest_byte_len,
        nodes: product.graph.nodes.len() as u64,
        relations: product.graph.edges.len() as u64,
        blobs: manifest.blobs.len() as u64,
        blob_bytes: manifest.blobs.iter().map(|blob| blob.byte_len).sum(),
        offered_at_ms,
    })
}

/// The events that place one offer on the personal graph.
pub fn offer_events(offer: &TransferOfferV1) -> Result<Vec<PersonalGraphEvent>, OfferError> {
    let address = offer_address(
        &offer.source.device,
        &offer.destination.device,
        offer.transfer_id,
    )?;
    let from = device_segment(&offer.source.device)
        .ok_or_else(|| OfferError::DeviceSegment(offer.source.device.clone()))?;
    Ok(vec![
        PersonalGraphEvent::AddNode {
            id: offer.transfer_id,
            address,
            title: format!("Transfer from {}", short(from)),
        },
        PersonalGraphEvent::SetFacet {
            node: offer.transfer_id,
            facet: TRANSFER_OFFER_FACET.to_string(),
            value: serde_json::to_value(offer).expect("a transfer offer always serializes"),
        },
    ])
}

/// Withdraw one offer. Devices that never projected the carrier replay this as
/// a no-op, so it is safe to author unconditionally.
pub fn withdraw_events(transfer_id: Uuid) -> Vec<PersonalGraphEvent> {
    vec![PersonalGraphEvent::RemoveNode { id: transfer_id }]
}

/// The offers this device projects, oldest first.
pub fn offers_in(projection: &SyncProjection) -> Vec<TransferOfferV1> {
    let facet = FacetId::new(TRANSFER_OFFER_FACET);
    let mut offers: Vec<TransferOfferV1> = projection
        .graph
        .nodes()
        .filter(|(_, node)| {
            node.primary_address()
                .as_url_str()
                .starts_with(TRANSFER_ADDRESS_PREFIX)
        })
        .filter_map(|(_, node)| projection.graph.facets().get(&node.id, &facet))
        .filter_map(|value| serde_json::from_value::<TransferOfferV1>(value.clone()).ok())
        .filter(|offer| offer.schema == TRANSFER_OFFER_FACET)
        .collect();
    offers.sort_by_key(|offer| (offer.offered_at_ms, offer.transfer_id));
    offers
}

fn short(device: &str) -> &str {
    let cut = device
        .char_indices()
        .nth(8)
        .map_or(device.len(), |(index, _)| index);
    &device[..cut]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personal_sync::{PersonalGraphReplica, SyncRoster, SyncSelection};
    use eidetic::PrivacyClass;
    use muniment::MemoryBackend;
    use p2panda_core::SigningKey;

    const GRAPH: [u8; 32] = [0x5c; 32];
    const SOURCE_SEED: [u8; 32] = [0x11; 32];
    const OFFER: Uuid = Uuid::from_u128(0x0ffe);

    fn endpoint(device: &str) -> TransferEndpointV1 {
        TransferEndpointV1 {
            graph: "graphshell://graph/personal".to_string(),
            persona: "personae://persona/alice".to_string(),
            device: format!("personae://device/{device}"),
        }
    }

    fn offer() -> TransferOfferV1 {
        TransferOfferV1 {
            schema: TRANSFER_OFFER_FACET.to_string(),
            transfer_id: OFFER,
            operation: TransferOperation::Copy,
            source: endpoint("laptop"),
            destination: endpoint("phone"),
            pairing_id: "pairing-1".to_string(),
            manifest_blob: Hash::of(b"manifest"),
            manifest_byte_len: 4096,
            nodes: 2,
            relations: 1,
            blobs: 1,
            blob_bytes: 90_000,
            offered_at_ms: 1_700_000_000_000,
        }
    }

    /// Every device in these tests shares one roster and one store contents.
    /// Only the selection differs, which is the point: filtering is local.
    fn selection(local_device: &str, offers_enabled: bool) -> SyncSelection {
        let facets: Vec<String> = if offers_enabled {
            vec![TRANSFER_OFFER_FACET.to_string()]
        } else {
            Vec::new()
        };
        SyncSelection::default()
            .with_facets(facets)
            .with_synthetic_addresses([transfer_offer_rule()])
            .with_local_device(local_device)
    }

    fn replica(selection: SyncSelection) -> PersonalGraphReplica<MemoryBackend> {
        let subject = *SigningKey::from_bytes(&SOURCE_SEED)
            .verifying_key()
            .as_bytes();
        PersonalGraphReplica::new(
            MemoryBackend::new(),
            GRAPH,
            SOURCE_SEED,
            SyncRoster::new([subject]),
            selection,
        )
    }

    #[tokio::test]
    async fn an_offer_reaches_its_destination_and_its_source_but_not_a_bystander() {
        let mut source = replica(selection("laptop", true));
        let operation = source
            .author(offer_events(&offer()).unwrap())
            .await
            .unwrap();

        let destination = replica(selection("phone", true));
        let bystander = replica(selection("tablet", true));
        for device in [&destination, &bystander] {
            assert!(
                device.accept(&operation).await.unwrap(),
                "every admitted device stores the operation; only projection differs"
            );
        }

        let addressed = offers_in(&destination.projection().await.unwrap());
        assert_eq!(addressed.len(), 1);
        assert_eq!(addressed[0], offer());

        assert_eq!(
            offers_in(&source.projection().await.unwrap()).len(),
            1,
            "a source projects the offers it sent"
        );
        let seen = bystander.projection().await.unwrap();
        assert!(offers_in(&seen).is_empty());
        assert!(
            seen.graph.get_node_by_id(OFFER).is_none(),
            "the carrier node is filtered with its facet, not left titled and empty"
        );
    }

    #[tokio::test]
    async fn a_device_without_the_offer_facet_projects_neither_carrier_nor_facet() {
        let mut source = replica(selection("laptop", true));
        let operation = source
            .author(offer_events(&offer()).unwrap())
            .await
            .unwrap();

        let opted_out = replica(selection("phone", false));
        opted_out.accept(&operation).await.unwrap();
        let projection = opted_out.projection().await.unwrap();

        assert!(offers_in(&projection).is_empty());
        assert!(projection.graph.get_node_by_id(OFFER).is_none());
        assert_eq!(
            projection
                .graph
                .facets()
                .get(&OFFER, &FacetId::new(TRANSFER_OFFER_FACET)),
            None,
            "a dropped carrier takes its facet with it, leaving no orphan"
        );
    }

    #[tokio::test]
    async fn a_device_that_does_not_know_its_own_key_projects_every_offer() {
        let mut source = replica(selection("laptop", true));
        let operation = source
            .author(offer_events(&offer()).unwrap())
            .await
            .unwrap();

        let unconfigured = replica(
            SyncSelection::default()
                .with_facets([TRANSFER_OFFER_FACET])
                .with_synthetic_addresses([transfer_offer_rule()]),
        );
        unconfigured.accept(&operation).await.unwrap();
        assert_eq!(
            offers_in(&unconfigured.projection().await.unwrap()).len(),
            1,
            "over-showing is the honest failure; silently hiding offers is not"
        );
    }

    #[test]
    fn an_offer_address_names_destination_then_source() {
        let address =
            offer_address("personae://device/laptop", "personae://device/phone", OFFER).unwrap();
        assert_eq!(address, format!("mere://transfer/phone/laptop/{OFFER}"));
        assert!(address.starts_with(TRANSFER_ADDRESS_PREFIX));
    }

    #[test]
    fn a_device_with_no_usable_segment_is_refused_rather_than_addressed_to_everyone() {
        for device in ["personae://device/", "personae://device/two words", "  "] {
            assert!(
                offer_address(device, "personae://device/phone", OFFER).is_err(),
                "{device} should not yield an offer address"
            );
        }
    }

    #[test]
    fn privacy_class_is_not_carried_by_an_offer() {
        // The offer names a manifest; the manifest carries the classification.
        // Kept as a reminder that this summary must stay secret-free, because
        // the lane it rides on is plaintext to the whole roster.
        let json = serde_json::to_string(&offer()).unwrap();
        assert!(!json.contains("privacy"));
        assert!(
            !json
                .to_lowercase()
                .contains(&format!("{:?}", PrivacyClass::LocalOnly).to_lowercase())
        );
    }
}
