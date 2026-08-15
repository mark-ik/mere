//! End-to-end tests for the first-party door.
//!
//! Kept beside `app_broker` rather than inside it: the module is at its size
//! budget, and these tests drive a whole session rather than checking a part.

use std::sync::Arc;

use tokio::sync::RwLock;

use chirograph::{
    CapabilityProfile, CarrierRequest, CarrierRequestBody, CarrierResponseBody, ContentHash,
    PortableCardV1, ProtocolVersion, ResourceRequest, SessionOpen,
};
use personae::{Ed25519Keypair, IdentityVault, InMemoryStorage, Profile, ProfileId};

use crate::identity::VaultProtectionView;

use crate::browser_carrier::{read_native_message_async, write_native_message_async};
use crate::identity_endpoint::SupplementalCard;
use crate::native::app_admission::{AllowedApps, AppHello, AppId};
use crate::native::app_broker::{
    APP_CONNECT_SCHEMA, AppHostMessage, AppMessage, serve_app_connection_for_tests,
};
use crate::native::device_broker::DeviceSurface;
use crate::native::personae_host::PersonaeHost;

fn resident_host() -> Arc<PersonaeHost<InMemoryStorage>> {
    let profile = Profile::new(
        ProfileId("default".into()),
        "Default",
        Ed25519Keypair::from_seed([0x91; 32]),
    );
    Arc::new(PersonaeHost::new(
        IdentityVault::with_profile(InMemoryStorage::new(), profile),
        None,
        VaultProtectionView::Ephemeral,
    ))
}

/// A surface shaped like the one the receipts lane composes: a card that names
/// a capture, and a reader that can reach that capture in the store.
fn receipt_surface(capture: &[u8]) -> (DeviceSurface, ContentHash) {
    let hash = ContentHash::of(capture);
    let bytes = capture.to_vec();
    let card = SupplementalCard::read_only(
        "graphshell.receipts",
        "receipt-run",
        PortableCardV1 {
            title: "Headed receipt".into(),
            values: Vec::new(),
            badges: vec!["Verified".into()],
            media: vec![hash],
        },
    );
    (
        DeviceSurface {
            cards: vec![card],
            released_blobs: Vec::new(),
            decisions: Default::default(),
            blob_reader: Some(Arc::new(move |wanted: &ContentHash| {
                (*wanted == hash).then(|| bytes.clone())
            })),
        },
        hash,
    )
}

/// The whole point of the door: turnstone opens a session and reads a
/// receipt's capture bytes out of the resident store, through the same
/// endpoint the browser is served.
#[tokio::test]
async fn turnstone_opens_a_session_and_reads_a_capture() {
    let capture = b"a headed screenshot, or near enough".to_vec();
    let (surface, hash) = receipt_surface(&capture);
    let personae = resident_host();

    let (host_stream, app_stream) = tokio::io::duplex(64 * 1024);
    let (mut host_reader, mut host_writer) = tokio::io::split(host_stream);
    let (mut app_reader, mut app_writer) = tokio::io::split(app_stream);

    let allowed = AllowedApps::default();
    let host = serve_app_connection_for_tests(
        &mut host_reader,
        &mut host_writer,
        personae,
        &allowed,
        60_000,
        Some(Arc::new(RwLock::new(surface))),
    );

    let app = async {
        write_native_message_async(&mut app_writer, &AppHello::new(AppId::new("turnstone")))
            .await
            .unwrap();
        let challenge = match read_native_message_async::<_, AppHostMessage>(&mut app_reader)
            .await
            .unwrap()
            .unwrap()
        {
            AppHostMessage::Challenge { challenge } => challenge,
            other => panic!("expected a challenge, got {other:?}"),
        };
        write_native_message_async(
            &mut app_writer,
            &AppMessage::Connect {
                schema: APP_CONNECT_SCHEMA.into(),
                host_nonce: challenge.host_nonce,
                client_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode([0x92; 32]),
            },
        )
        .await
        .unwrap();
        match read_native_message_async::<_, AppHostMessage>(&mut app_reader)
            .await
            .unwrap()
            .unwrap()
        {
            AppHostMessage::Connected { app, .. } => {
                assert_eq!(app, AppId::new("turnstone"), "the door names who connected")
            }
            other => panic!("expected connected, got {other:?}"),
        }

        write_native_message_async(
            &mut app_writer,
            &AppMessage::Request {
                request: CarrierRequest {
                    id: 1,
                    body: CarrierRequestBody::Open(Box::new(SessionOpen {
                        version: ProtocolVersion { major: 1, minor: 0 },
                        capabilities: CapabilityProfile::default(),
                    })),
                },
            },
        )
        .await
        .unwrap();
        let projection = match response(&mut app_reader).await {
            CarrierResponseBody::Opened(opened) => opened.descriptor.projections[0].request.clone(),
            other => panic!("expected opened, got {other:?}"),
        };

        write_native_message_async(
            &mut app_writer,
            &AppMessage::Request {
                request: CarrierRequest {
                    id: 2,
                    body: CarrierRequestBody::Snapshot(projection),
                },
            },
        )
        .await
        .unwrap();
        let snapshot = match response(&mut app_reader).await {
            CarrierResponseBody::Snapshot(snapshot) => snapshot,
            other => panic!("expected a snapshot, got {other:?}"),
        };
        assert!(
            snapshot
                .presentation
                .offers
                .values()
                .flatten()
                .any(|offer| offer.semantics.label == "Headed receipt"),
            "the admitted application sees the receipt card",
        );

        // The capture itself: named by the card, held in the store, never
        // staged. This is the read-through path, exercised end to end.
        write_native_message_async(
            &mut app_writer,
            &AppMessage::Request {
                request: CarrierRequest {
                    id: 3,
                    body: CarrierRequestBody::Resource(ResourceRequest {
                        session: snapshot.session.clone(),
                        resource: hash,
                    }),
                },
            },
        )
        .await
        .unwrap();
        match response(&mut app_reader).await {
            CarrierResponseBody::Resource(resource) => {
                assert_eq!(resource.bytes, capture, "the capture arrives byte for byte")
            }
            other => panic!("expected the resource, got {other:?}"),
        }

        write_native_message_async(
            &mut app_writer,
            &AppMessage::Request {
                request: CarrierRequest {
                    id: 4,
                    body: CarrierRequestBody::Close,
                },
            },
        )
        .await
        .unwrap();
        let _ = response(&mut app_reader).await;
    };

    let (served, ()) = tokio::join!(host, app);
    assert!(served.unwrap().is_some(), "the session ran to a summary");
}

/// An application this device does not admit is turned away at the hello,
/// before a challenge is ever written. A refused client learns nothing about
/// the host beyond the refusal.
#[tokio::test]
async fn an_unadmitted_application_never_sees_a_challenge() {
    let personae = resident_host();
    let (host_stream, app_stream) = tokio::io::duplex(16 * 1024);
    let (mut host_reader, mut host_writer) = tokio::io::split(host_stream);
    let (mut app_reader, mut app_writer) = tokio::io::split(app_stream);

    write_native_message_async(&mut app_writer, &AppHello::new(AppId::new("not-ours")))
        .await
        .unwrap();
    let allowed = AllowedApps::default();
    let served = serve_app_connection_for_tests(
        &mut host_reader,
        &mut host_writer,
        personae,
        &allowed,
        60_000,
        Some(Arc::new(RwLock::new(DeviceSurface::default()))),
    )
    .await;
    assert!(served.is_err(), "an unadmitted application is refused");
    // Both halves: `tokio::io::split` keeps the duplex open while either one
    // lives, so dropping only the writer would leave the read below waiting
    // on an end that never comes.
    drop(host_writer);
    drop(host_reader);
    let refused = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        read_native_message_async::<_, AppHostMessage>(&mut app_reader),
    )
    .await
    .expect("the refused application reaches an end rather than hanging")
    .unwrap();
    assert!(
        refused.is_none(),
        "no challenge was written to a refused application, got {refused:?}",
    );
}

/// The browser's connect frame does not open an application session. The two
/// doors do not accept each other's wire, in either direction.
#[tokio::test]
async fn the_browser_connect_frame_does_not_open_an_application_session() {
    let personae = resident_host();
    let (host_stream, app_stream) = tokio::io::duplex(16 * 1024);
    let (mut host_reader, mut host_writer) = tokio::io::split(host_stream);
    let (mut app_reader, mut app_writer) = tokio::io::split(app_stream);

    let allowed = AllowedApps::default();
    let host = serve_app_connection_for_tests(
        &mut host_reader,
        &mut host_writer,
        personae,
        &allowed,
        60_000,
        Some(Arc::new(RwLock::new(DeviceSurface::default()))),
    );
    let app = async {
        write_native_message_async(&mut app_writer, &AppHello::new(AppId::new("turnstone")))
            .await
            .unwrap();
        let challenge = match read_native_message_async::<_, AppHostMessage>(&mut app_reader)
            .await
            .unwrap()
            .unwrap()
        {
            AppHostMessage::Challenge { challenge } => challenge,
            other => panic!("expected a challenge, got {other:?}"),
        };
        write_native_message_async(
            &mut app_writer,
            &AppMessage::Connect {
                schema: "mere.graphshell/browser-connect/v1".into(),
                host_nonce: challenge.host_nonce,
                client_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode([0x93; 32]),
            },
        )
        .await
        .unwrap();
    };
    let (served, ()) = tokio::join!(host, app);
    assert!(
        served.is_err(),
        "the browser connect schema does not open this door",
    );
}

async fn response<R: tokio::io::AsyncRead + Unpin>(reader: &mut R) -> CarrierResponseBody {
    match read_native_message_async::<_, AppHostMessage>(reader)
        .await
        .unwrap()
        .unwrap()
    {
        AppHostMessage::Response { response } => response.body.unwrap(),
        other => panic!("expected a carrier response, got {other:?}"),
    }
}

use base64::Engine as _;
