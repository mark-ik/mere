//! The B5 physical receipt: a signed `mere.pack/v1` pack crosses the REAL
//! RF pair (two RNode radios on COM ports) as a retinue resource, and the
//! subscriber re-verifies the personae signature on arrival.
//!
//! Modeled on retinue's `tulle_headed` two-RNode acceptance (same LoRa
//! parameters and half-duplex pacing); trimmed to the resource path so the
//! airtime stays small. Usage: `rf_pack_pair [publisher_port] [subscriber_port]`
//! (defaults COM6 / COM5, the bench convention).

use std::sync::Arc;
use std::time::Duration;

use eidetic::pack::{PackManifest, PackPart, PackPartRole, PackVerdict, sign_pack, verify_pack};
use eidetic::schema::{ManifestId, ModerationState, TrustEnvelope, TrustLevel};
use identity::Ed25519Keypair;
use retinue::destination::DestinationName;
use retinue::endpoint::{Endpoint, ResourceTransferConfig};
use retinue::identity::PrivateIdentity;
use retinue::iface::tulle::drive;
use tulle::airtime::AirtimeBudget;
use tulle::lora::{CodingRate, LoRaParams};
use tulle::serial::{RNodeSerialLink, SerialPumpConfig};

fn params() -> LoRaParams {
    LoRaParams {
        spreading_factor: 8,
        bandwidth_hz: 125_000,
        coding_rate: CodingRate::Cr45,
        frequency_hz: 915_000_000,
        tx_power_dbm: 7,
        preamble_syms: 8,
        explicit_header: true,
        crc: true,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn signed_pack() -> (PackManifest, TrustEnvelope) {
    let author = Ed25519Keypair::from_seed([21u8; 32]);
    let source: &[u8] = b"mere.open('mere://kept/note')";
    let manifest = PackManifest {
        name: "trail-keeper".to_string(),
        version: "0.1.0".to_string(),
        author: hex(&author.public_key().to_bytes()),
        requested_scopes: vec!["scenario/".to_string(), "app/".to_string()],
        parts: vec![PackPart {
            name: "main.lua".to_string(),
            role: PackPartRole::ScenarioSource,
            blob: ManifestId::of_blob(source),
            bytes: source.len() as u64,
        }],
    };
    let envelope = TrustEnvelope {
        level: TrustLevel::SelfAsserted,
        signatures: vec![sign_pack(&manifest, &author)],
        moderation_state: ModerationState::Unreviewed,
    };
    (manifest, envelope)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let publisher_port = args.next().unwrap_or_else(|| "COM6".into());
    let subscriber_port = args.next().unwrap_or_else(|| "COM5".into());
    let serial = SerialPumpConfig {
        turnaround: Duration::from_millis(800),
        ..SerialPumpConfig::default()
    };
    let mut publisher_radio = RNodeSerialLink::open(
        &publisher_port,
        params(),
        AirtimeBudget::new(60_000, 60_000),
        serial.clone(),
    )?;
    let mut subscriber_radio = RNodeSerialLink::open(
        &subscriber_port,
        params(),
        AirtimeBudget::new(60_000, 60_000),
        serial,
    )?;
    let pub_fw =
        tokio::time::timeout(Duration::from_secs(25), publisher_radio.wait_online()).await??;
    let sub_fw =
        tokio::time::timeout(Duration::from_secs(25), subscriber_radio.wait_online()).await??;
    println!("radios online: {publisher_port}={pub_fw:?}, {subscriber_port}={sub_fw:?}");

    let publisher_id = PrivateIdentity::from_secret_bytes(&[0x11; 64]);
    let subscriber_id = PrivateIdentity::from_secret_bytes(&[0x22; 64]);
    let publisher = Endpoint::new(publisher_id);
    let subscriber = Arc::new(Endpoint::new(subscriber_id.clone()));
    publisher.set_reliable_initial_rtt(Duration::from_secs(5));
    subscriber.set_reliable_initial_rtt(Duration::from_secs(5));
    // Half-duplex pacing: window 1 so the two radios never talk over each
    // other (the tulle_headed acceptance's setting; without it the resource
    // exchange stalls to timeout).
    publisher.set_reliable_max_window(1);
    subscriber.set_reliable_max_window(1);
    publisher.set_link_mtu(255);
    subscriber.set_link_mtu(255);
    let _pub_driver = tokio::spawn(drive(publisher.attach_interface(), publisher_radio));
    let _sub_driver = tokio::spawn(drive(subscriber.attach_interface(), subscriber_radio));

    // The subscriber registers the pack destination and announces it over RF.
    let pack_name = DestinationName::new("retinue", ["pack"]);
    let pack_destination = pack_name.destination_hash(subscriber_id.public());
    subscriber.register_resource(pack_name.clone(), b"pack drop");
    let mut announce = None;
    for attempt in 0..3 {
        if attempt > 0 {
            subscriber.announce(&pack_name, b"pack drop");
        }
        match tokio::time::timeout(Duration::from_secs(20), publisher.next_announcement()).await {
            Ok(Ok(a)) => {
                announce = Some(a);
                break;
            }
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => eprintln!("announce attempt {} timed out", attempt + 1),
        }
    }
    let announce = announce.ok_or("pack destination announce did not cross RF")?;
    if announce.destination != pack_destination {
        return Err("received the wrong destination announce".into());
    }
    println!("discovery: pack destination announced over RF");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The wire form: signed manifest + envelope, one JSON document.
    let (manifest, envelope) = signed_pack();
    let wire = serde_json::to_vec(&(&manifest, &envelope))?;
    println!("publishing {} pack bytes over RF", wire.len());
    let config = ResourceTransferConfig {
        timeout: Duration::from_secs(120),
        retry_interval: Duration::from_secs(5),
        request_window: 1,
    };
    let fetcher = tokio::spawn({
        let subscriber = Arc::clone(&subscriber);
        async move {
            let mut accepted = subscriber.accept_resource().await?;
            accepted.session.set_config(config);
            accepted.session.fetch().await
        }
    });
    tokio::time::timeout(
        Duration::from_secs(150),
        publisher.publish_resource_with_config(
            pack_destination,
            *subscriber_id.public(),
            &wire,
            config,
        ),
    )
    .await??;
    let received = tokio::time::timeout(Duration::from_secs(150), fetcher).await???;
    println!("transfer: {} bytes received over RF", received.len());

    let (got_manifest, got_envelope): (PackManifest, TrustEnvelope) =
        serde_json::from_slice(&received)?;
    if got_manifest != manifest {
        println!("RESULT fail: manifest not bit-equal after RF transfer");
        return Err("manifest mismatch".into());
    }
    match verify_pack(&got_manifest, &got_envelope) {
        PackVerdict::Trusted => println!("verify: Trusted — the signature survives the air"),
        other => {
            println!("RESULT fail: expected Trusted, got {other:?}");
            return Err("verification failed".into());
        }
    }
    // The tamper check on the received copy (no second transfer: airtime is
    // precious; wire integrity is already proven by the bit-equal check).
    let mut widened = got_manifest.clone();
    widened.requested_scopes.push("wallet/".to_string());
    match verify_pack(&widened, &got_envelope) {
        PackVerdict::Broken => println!("verify: a widened ask reads Broken — refused"),
        other => {
            println!("RESULT fail: tampered copy verified {other:?}");
            return Err("tamper check failed".into());
        }
    }
    println!("RESULT ok: signed mere.pack/v1 crossed the RF pair and re-verified");
    Ok(())
}
