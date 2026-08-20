//! H6 destination: fetch, reconnect, resume, apply, then test revocation.

use std::path::Path;
use std::time::Instant;

use chirograph::{
    CapabilityProfile, CarrierRequest, CarrierRequestBody, CarrierResponse, CarrierResponseBody,
    ContentHash, IntentInvocation, IntentResult, PortableCardV1, ProtocolVersion, ResourceRequest,
    ResourceResponse, ResumeReply, ResumeRequest, SessionOpen,
};
use eidetic::PrivacyClass;
use graphshell::access::AccessContext;
use graphshell::admission::open_session;
use graphshell::carrier::projection_alpn;
use graphshell::mere_host::{MereHost, SelectedPersonaRef, fixture_handlers};
use graphshell::transfer::{
    ApplyTransferContext, TransferAuthorization, TransferManifestV1, TransferOperation,
    apply_transfer,
};
use graphshell::transfer_endpoint::{TRANSFER_BEGIN_INTENT, TransferBeginV1};
use muniment::{BlobStore, MemoryBackend};
use notochord::{NetworkId, SessionReply, TrafficClass, initiate_session};
use personae::{IdentityProvider, InMemoryProvider};
use sceno::InstanceId;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, ReadHalf, WriteHalf};
use tokio::time::sleep;
use transport::p2panda_transport::{MdnsDiscoveryMode, P2pandaStream, P2pandaTransport};
use transport::{Transport, initiator_binding};

use crate::DIAL_DEADLINE;
use crate::identity::{assert_same_key, device, grant, hex, now_ms, profile, short};

struct AdmittedIo {
    lines: Lines<BufReader<ReadHalf<P2pandaStream>>>,
    writer: WriteHalf<P2pandaStream>,
    grant_id: String,
}

impl AdmittedIo {
    async fn request(
        &mut self,
        id: u64,
        body: CarrierRequestBody,
    ) -> Result<CarrierResponse, String> {
        let mut line = serde_json::to_vec(&CarrierRequest { id, body })
            .map_err(|error| format!("encode request: {error}"))?;
        line.push(b'\n');
        self.writer
            .write_all(&line)
            .await
            .map_err(|error| format!("write request: {error}"))?;
        self.writer
            .flush()
            .await
            .map_err(|error| format!("flush request: {error}"))?;
        let line = self
            .lines
            .next_line()
            .await
            .map_err(|error| format!("read response: {error}"))?
            .ok_or("the source closed without answering")?;
        let response: CarrierResponse =
            serde_json::from_str(&line).map_err(|error| format!("decode response: {error}"))?;
        if response.id != id {
            return Err(format!(
                "response id {} did not match request {id}",
                response.id
            ));
        }
        Ok(response)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn connect(
    owner: InMemoryProvider,
    me: InMemoryProvider,
    seed: [u8; 32],
    source_key: [u8; 32],
    network: NetworkId,
    ticket: &str,
    receipt_path: Option<&Path>,
    expect_revoked: bool,
) -> Result<(), String> {
    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![projection_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|error| format!("bind: {error}"))?;
    assert_same_key(&carrier, &me)?;
    let peer = carrier
        .add_peer_ticket(ticket)
        .await
        .map_err(|error| format!("ticket: {error}"))?;
    if peer.to_bytes() != source_key {
        return Err("ticket peer does not match H6_PEER".to_string());
    }

    println!("h6_transfer_peer connect");
    println!("  source: {}", short(&source_key));

    println!("  --- session 1: manifest ---");
    let mut first = dial(&carrier, peer, &me, &owner, network).await?;
    let first_opened = opened(ok(first.request(1, open_body()).await?)?)?;
    let projection = first_opened
        .descriptor
        .projections
        .first()
        .ok_or("source advertised no transfer projection")?
        .request
        .clone();
    let snapshot = snapshot(ok(first
        .request(2, CarrierRequestBody::Snapshot(projection.clone()))
        .await?)?)?;
    let transfer_id_text = projection
        .session
        .0
        .rsplit(':')
        .next()
        .ok_or("transfer session has no id")?;
    let transfer_id = uuid::Uuid::parse_str(transfer_id_text).map_err(|error| error.to_string())?;
    let begin = IntentInvocation {
        session: projection.session.clone(),
        target: InstanceId(0),
        observed_epoch: snapshot.scene.epoch,
        observed_revision: snapshot.scene.revision,
        intent: TRANSFER_BEGIN_INTENT.to_string(),
        payload: serde_json::to_vec(&TransferBeginV1 { transfer_id })
            .map_err(|error| error.to_string())?,
    };
    match intent(ok(first
        .request(3, CarrierRequestBody::Intent(begin.clone()))
        .await?)?)?
    {
        IntentResult::Accepted => println!("  transfer disclosure accepted"),
        other => return Err(format!("transfer disclosure was not accepted: {other:?}")),
    }

    let card_offer = snapshot
        .presentation
        .offers_for(InstanceId(0))
        .and_then(|offers| offers.first())
        .ok_or("transfer snapshot advertised no card")?;
    let card_response = resource(ok(first
        .request(
            4,
            CarrierRequestBody::Resource(ResourceRequest {
                session: projection.session.clone(),
                resource: card_offer.resource,
            }),
        )
        .await?)?)?;
    verify_address(&card_response)?;
    let card: PortableCardV1 =
        serde_json::from_slice(&card_response.bytes).map_err(|error| error.to_string())?;
    let manifest_address = *card.media.first().ok_or("transfer card has no manifest")?;
    let manifest_response = resource(ok(first
        .request(
            5,
            CarrierRequestBody::Resource(ResourceRequest {
                session: projection.session.clone(),
                resource: manifest_address,
            }),
        )
        .await?)?)?;
    verify_address(&manifest_response)?;
    let manifest: TransferManifestV1 =
        serde_json::from_slice(&manifest_response.bytes).map_err(|error| error.to_string())?;
    if manifest.transfer_id != transfer_id {
        return Err("card session and manifest name different transfers".to_string());
    }
    if manifest.operation != TransferOperation::Replicate {
        return Err("the physical H6 proof expects replicate".to_string());
    }
    suspended(ok(first.request(6, CarrierRequestBody::Suspend).await?)?)?;
    println!(
        "  cached manifest {}; connection suspended before {} blob(s)",
        manifest.transfer_id,
        manifest.blobs.len()
    );
    drop(first);

    println!("  --- interruption: new carrier admission ---");
    println!("  --- session 2: resume and blobs ---");
    let mut second = dial(&carrier, peer, &me, &owner, network).await?;
    let authorization_grant = second.grant_id.clone();
    opened(ok(second.request(7, open_body()).await?)?)?;
    match resume(ok(second
        .request(
            8,
            CarrierRequestBody::Resume(ResumeRequest {
                session: projection.session.clone(),
                epoch: snapshot.scene.epoch,
                revision: snapshot.scene.revision,
            }),
        )
        .await?)?)?
    {
        ResumeReply::Current(ack)
            if ack.epoch == snapshot.scene.epoch && ack.revision == snapshot.scene.revision =>
        {
            println!("  resumed current projection without a fresh snapshot")
        }
        other => return Err(format!("unexpected resume answer: {other:?}")),
    }

    let staging_backend = MemoryBackend::new();
    let staging_blobs = BlobStore::new(staging_backend.clone());
    for (index, descriptor) in manifest.blobs.iter().enumerate() {
        let response = resource(ok(second
            .request(
                9 + index as u64,
                CarrierRequestBody::Resource(ResourceRequest {
                    session: projection.session.clone(),
                    resource: ContentHash(*descriptor.content_hash.as_bytes()),
                }),
            )
            .await?)?)?;
        verify_address(&response)?;
        if eidetic::Hash::of(&response.bytes) != descriptor.content_hash
            || response.bytes.len() as u64 != descriptor.byte_len
        {
            return Err(format!(
                "blob {} failed its manifest address",
                descriptor.content_hash
            ));
        }
        let stored = staging_blobs
            .put(&response.bytes)
            .await
            .map_err(|error| error.to_string())?;
        if stored.to_hex() != descriptor.content_hash.to_hex() {
            return Err("staging store changed the blob address".to_string());
        }
        println!(
            "  fetched blob {} ({} bytes)",
            descriptor.content_hash, descriptor.byte_len
        );
    }
    let close_id = 9 + manifest.blobs.len() as u64;
    closed(ok(second
        .request(close_id, CarrierRequestBody::Close)
        .await?)?)?;
    drop(second);

    let destination_key = me.master_public_key().to_bytes();
    if manifest.destination.device != device(&destination_key) {
        return Err("manifest destination is not this device".to_string());
    }
    let destination_backend = MemoryBackend::new();
    let mut destination_host = MereHost::empty(
        destination_backend.clone(),
        SelectedPersonaRef {
            persona: manifest.destination.persona.clone(),
            profile: "mere.base".to_string(),
        },
        fixture_handlers(),
        AccessContext {
            persona: manifest.destination.persona.clone(),
            device: manifest.destination.device.clone(),
            at_ms: now_ms(),
        },
    );
    let destination_blobs = BlobStore::new(destination_backend.clone());
    let mut destination_authority = destination_backend.clone();
    let receipt = apply_transfer(
        &mut destination_host,
        &staging_blobs,
        &destination_blobs,
        &mut destination_authority,
        &manifest,
        &ApplyTransferContext {
            authorization: TransferAuthorization {
                grant_id: authorization_grant,
                revoked: false,
            },
            application: "graphshell".to_string(),
            handler: "graphshell.transfer/v1".to_string(),
            completed_at_ms: now_ms(),
            access_privacy: PrivacyClass::LocalOnly,
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    verify_destination(&destination_host, &destination_blobs, &manifest, &receipt).await?;
    if let Some(path) = receipt_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?;
        std::fs::write(path, bytes)
            .map_err(|error| format!("write {}: {error}", path.display()))?;
        println!("  receipt: {}", path.display());
    }
    println!(
        "  applied: {} objects, {} relation(s), {} destination access record(s)",
        receipt.nodes,
        receipt.relations,
        receipt.destination_access_records.len()
    );

    println!("  --- session 3: transfer intent first ---");
    let mut third = dial(&carrier, peer, &me, &owner, network).await?;
    let response = third
        .request(close_id + 1, CarrierRequestBody::Intent(begin))
        .await?;
    match response.body {
        Err(failure) if expect_revoked && failure.message.contains("revoked") => {
            println!("  revoked transfer intent refused before endpoint dispatch");
        }
        Ok(CarrierResponseBody::Intent(IntentResult::Accepted)) if !expect_revoked => {
            println!("  granted transfer intent accepted");
            closed(ok(third
                .request(close_id + 2, CarrierRequestBody::Close)
                .await?)?)?;
        }
        Err(failure) => return Err(format!("unexpected transfer refusal: {}", failure.message)),
        Ok(body) => return Err(format!("unexpected transfer-intent answer: {body:?}")),
    }

    println!("H6 physical transfer proof passed");
    Ok(())
}

async fn dial(
    carrier: &P2pandaTransport,
    peer: transport::PeerID,
    me: &InMemoryProvider,
    owner: &InMemoryProvider,
    network: NetworkId,
) -> Result<AdmittedIo, String> {
    let started = Instant::now();
    let mut stream = loop {
        match carrier.connect(peer, projection_alpn()).await {
            Ok(stream) => break stream,
            Err(error) => {
                if started.elapsed() >= DIAL_DEADLINE {
                    return Err(format!("connect: {error}"));
                }
                sleep(std::time::Duration::from_millis(250)).await;
            }
        }
    };
    let subject = me.master_public_key().to_bytes();
    let local = transport::PeerID::from_bytes(&subject).map_err(|error| error.to_string())?;
    let binding = initiator_binding(&projection_alpn(), local);
    let certificate = grant(owner, subject, network, now_ms() + 3_600_000);
    let grant_id = hex(&certificate.certificate.id().0);
    let hello = open_session(
        me,
        network,
        profile(),
        TrafficClass::Interactive,
        [5; 32],
        &binding,
        vec![certificate],
    )
    .map_err(|error| format!("hello: {error}"))?;
    match initiate_session(&mut stream, &hello, &Default::default())
        .await
        .map_err(|error| format!("handshake: {error}"))?
    {
        SessionReply::Reject { reason } => return Err(format!("admission refused: {reason:?}")),
        SessionReply::Accept { .. } => println!("  admitted"),
    }
    let (reader, writer) = tokio::io::split(stream);
    Ok(AdmittedIo {
        lines: BufReader::new(reader).lines(),
        writer,
        grant_id,
    })
}

fn open_body() -> CarrierRequestBody {
    CarrierRequestBody::Open(Box::new(SessionOpen {
        version: ProtocolVersion::V1,
        capabilities: CapabilityProfile::default(),
    }))
}

fn ok(response: CarrierResponse) -> Result<CarrierResponseBody, String> {
    response.body.map_err(|failure| failure.message)
}

fn opened(body: CarrierResponseBody) -> Result<Box<chirograph::SessionOpened>, String> {
    match body {
        CarrierResponseBody::Opened(opened) => Ok(opened),
        other => Err(format!("expected opened, found {other:?}")),
    }
}

fn snapshot(body: CarrierResponseBody) -> Result<Box<chirograph::ProjectionSnapshot>, String> {
    match body {
        CarrierResponseBody::Snapshot(snapshot) => Ok(snapshot),
        other => Err(format!("expected snapshot, found {other:?}")),
    }
}

fn resource(body: CarrierResponseBody) -> Result<ResourceResponse, String> {
    match body {
        CarrierResponseBody::Resource(resource) => Ok(resource),
        other => Err(format!("expected resource, found {other:?}")),
    }
}

fn intent(body: CarrierResponseBody) -> Result<IntentResult, String> {
    match body {
        CarrierResponseBody::Intent(intent) => Ok(intent),
        other => Err(format!("expected intent result, found {other:?}")),
    }
}

fn resume(body: CarrierResponseBody) -> Result<ResumeReply, String> {
    match body {
        CarrierResponseBody::Resume(reply) => Ok(reply),
        other => Err(format!("expected resume, found {other:?}")),
    }
}

fn suspended(body: CarrierResponseBody) -> Result<(), String> {
    match body {
        CarrierResponseBody::Suspended => Ok(()),
        other => Err(format!("expected suspended, found {other:?}")),
    }
}

fn closed(body: CarrierResponseBody) -> Result<(), String> {
    match body {
        CarrierResponseBody::Closed => Ok(()),
        other => Err(format!("expected closed, found {other:?}")),
    }
}

fn verify_address(response: &ResourceResponse) -> Result<(), String> {
    if !response.has_valid_address() {
        return Err(format!("resource {} failed its address", response.resource));
    }
    Ok(())
}

async fn verify_destination(
    host: &MereHost<MemoryBackend>,
    blobs: &BlobStore<MemoryBackend>,
    manifest: &TransferManifestV1,
    receipt: &graphshell::transfer::TransferReceiptV1,
) -> Result<(), String> {
    if receipt.nodes != 2
        || receipt.relations != 1
        || receipt.destination_access_records.len() != 2
        || receipt
            .id_map
            .iter()
            .any(|mapping| mapping.source != mapping.destination)
    {
        return Err(format!("destination receipt is incomplete: {receipt:?}"));
    }
    for mapping in &receipt.id_map {
        let (_, node) = host
            .graph()
            .get_node_by_id(mapping.destination)
            .ok_or_else(|| format!("destination is missing {}", mapping.destination))?;
        if !node.tags.contains("h6") || !node.tags.contains("physical") {
            return Err(format!("{} lost its transfer tags", mapping.destination));
        }
    }
    if host.graph().relations().count() != 1 {
        return Err("destination did not preserve the Cites relation".to_string());
    }
    for descriptor in &manifest.blobs {
        let hash = muniment::Hash::from_hex(&descriptor.content_hash.to_hex())
            .ok_or("invalid blob hash")?;
        if !blobs.has(&hash).await.map_err(|error| error.to_string())? {
            return Err(format!(
                "destination is missing blob {}",
                descriptor.content_hash
            ));
        }
    }
    Ok(())
}
