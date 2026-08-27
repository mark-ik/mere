// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! End-to-end tests for the first-party door.
//!
//! Kept beside `app_broker` rather than inside it: the module is at its size
//! budget, and these tests drive a whole session rather than checking a part.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::RwLock;

use chirograph::{
    CapabilityProfile, CarrierRequest, CarrierRequestBody, CarrierResponseBody, ContentHash,
    PortableCardV1, ProtocolVersion, ResourceRequest, SessionOpen,
};
use personae::{Ed25519Keypair, IdentityVault, InMemoryStorage, Profile, ProfileId};

use crate::identity::VaultProtectionView;

use crate::browser_carrier::{read_native_message_async, write_native_message_async};
use crate::identity_endpoint::SupplementalCard;
use crate::native::app_admission::{AllowedAppRoutes, AppHello, AppId, AppRouteId};
use crate::native::app_broker::{
    APP_CONNECT_SCHEMA, AppEndpointCatalog, AppHostMessage, AppMessage, serve_app_broker,
    serve_app_connection_for_tests,
};
use crate::native::app_client::AppBrokerClient;
use crate::native::device_broker::DeviceSurface;
use crate::native::endpoint_catalog::ResidentEndpointCatalog;
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

    let allowed = AllowedAppRoutes::default();
    let host = serve_app_connection_for_tests(
        &mut host_reader,
        &mut host_writer,
        personae,
        &allowed,
        60_000,
        Some(Arc::new(RwLock::new(surface))),
        AppEndpointCatalog::default(),
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
                client_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0x92; 32]),
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
    let allowed = AllowedAppRoutes::default();
    let served = serve_app_connection_for_tests(
        &mut host_reader,
        &mut host_writer,
        personae,
        &allowed,
        60_000,
        Some(Arc::new(RwLock::new(DeviceSurface::default()))),
        AppEndpointCatalog::default(),
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

/// A known application cannot turn its name into product authority. Route
/// grants are checked before Graphshell admission and therefore before the
/// catalog is allowed to construct product state.
#[tokio::test]
async fn an_ungranted_route_never_opens_its_product_endpoint() {
    let personae = resident_host();
    let opened = Arc::new(AtomicUsize::new(0));
    let factory_opened = Arc::clone(&opened);
    let mut catalog = ResidentEndpointCatalog::new();
    catalog
        .register_erased("knot", "Knot fixture", move |_| {
            factory_opened.fetch_add(1, Ordering::SeqCst);
            Err("the ungranted factory was opened".to_string())
        })
        .unwrap();

    let (host_stream, app_stream) = tokio::io::duplex(16 * 1024);
    let (mut host_reader, mut host_writer) = tokio::io::split(host_stream);
    let (_app_reader, mut app_writer) = tokio::io::split(app_stream);
    write_native_message_async(
        &mut app_writer,
        &AppHello::for_route(AppId::new("turnstone"), AppRouteId::new("knot").unwrap()),
    )
    .await
    .unwrap();

    let served = serve_app_connection_for_tests(
        &mut host_reader,
        &mut host_writer,
        personae,
        &AllowedAppRoutes::default(),
        60_000,
        Some(Arc::new(RwLock::new(DeviceSurface::default()))),
        AppEndpointCatalog::new(catalog),
    )
    .await;
    assert!(served.is_err(), "the ungranted route is refused");
    assert_eq!(
        opened.load(Ordering::SeqCst),
        0,
        "route refusal happens before product state opens",
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

    let allowed = AllowedAppRoutes::default();
    let host = serve_app_connection_for_tests(
        &mut host_reader,
        &mut host_writer,
        personae,
        &allowed,
        60_000,
        Some(Arc::new(RwLock::new(DeviceSurface::default()))),
        AppEndpointCatalog::default(),
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
                client_nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0x93; 32]),
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

/// The client against the served endpoint, over the real transport: a named
/// pipe here, the socket path on Unix. This is the exact code turnstone runs.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_cards_over_the_served_endpoint() {
    let capture = b"capture bytes for the client test".to_vec();
    let (surface, hash) = receipt_surface(&capture);
    let personae = resident_host();
    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\graphshell-app-client-{}", uuid::Uuid::new_v4());
    #[cfg(not(windows))]
    let endpoint = std::env::temp_dir()
        .join(format!(
            "graphshell-app-client-{}.sock",
            uuid::Uuid::new_v4()
        ))
        .display()
        .to_string();

    let server_endpoint = endpoint.clone();
    let server = tokio::spawn(async move {
        let _ = serve_app_broker(
            &server_endpoint,
            personae,
            AllowedAppRoutes::default(),
            60_000,
            Some(Arc::new(RwLock::new(surface))),
            AppEndpointCatalog::default(),
        )
        .await;
    });

    // The server binds asynchronously; retry the connect briefly rather than
    // sleeping a guessed amount.
    let mut client = None;
    let mut last_error = String::new();
    for _ in 0..50 {
        match AppBrokerClient::open_at(&endpoint, AppId::new("turnstone")).await {
            Ok(open) => {
                client = Some(open);
                break;
            }
            Err(error) => {
                last_error = error.to_string();
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }
    }
    let mut client =
        client.unwrap_or_else(|| panic!("the client never connected, last: {last_error}"));

    // The endpoint offers the Personae cards beside the supplemental one;
    // the receipt card is found by title, the way a renderer groups them by
    // adapter rather than assuming it is served alone.
    let cards = client.read_cards().await.unwrap();
    let receipt = cards
        .iter()
        .find(|card| card.card.title == "Headed receipt")
        .expect("the receipt card is among the offers");
    assert_eq!(receipt.card.media, vec![hash], "the card names its capture");

    // Read the capture the card names, exactly as a renderer would.
    let opened = client.open_session().await.unwrap();
    let session =
        chirograph::ProjectionSession(opened.descriptor.projections[0].request.session.0.clone());
    let bytes = client.resource(session, hash).await.unwrap();
    assert_eq!(bytes, capture, "the capture arrives byte for byte");

    client.close().await.unwrap();
    server.abort();
}
