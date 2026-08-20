//! K2: place members edit a document its holder keeps.
//!
//! Option A, end to end, with nothing stubbed. The holder owns the vault and
//! every byte on disk; a visitor never holds a replica, only a live projection
//! it edits by sending intents. Real `KnotEndpoint`s over a real directory,
//! registered in the resident host's catalog, admitted over a transport, and
//! driven by the blocking network carrier through the ordinary
//! `RetainedEndpointSession`.
//!
//! ## What the memory transport stands in for
//!
//! Each visitor gets its own paired transport, so each carries a distinct
//! authenticated subject with its own grant: these are two peers, not one peer
//! twice. The holder therefore holds two transports rather than one accepting
//! many, which is an artifact of a fixture that pairs exactly two nodes. The
//! host takes the transport per accept and is indifferent to how many it gets.

use std::fs;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use chirograph::{
    CapabilityProfile, IntentResult, PresentationCapability, ProjectionSession, ResumeRequest,
    SaveTextV1, SessionStatus,
};
use graphshell::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE, open_session};
use graphshell::carrier::{accept_projection_session, projection_policy};
use graphshell::client::{ResolvedContent, RetainedEndpointSession};
use graphshell::lifecycle::SessionAuthority;
use graphshell::native::endpoint_catalog::{ResidentEndpointCatalog, ResidentEndpointRoute};
use graphshell::native::projection_host::ResidentProjectionHost;
use graphshell::network_carrier::{
    CarrierRuntime, NetworkCarrier, dial_projection_session, projection_binding,
};
use graphshell::session_notices::serve_admitted_session_notifying;
use graphshell_endpoint::ResumableProjectionSource;
use notochord::{
    LocalNetworkPolicy, NetworkId, ProfileRef, RevocationLedger, TrafficClass, TrustedRoot,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::{IdentityProvider, InMemoryProvider};
use tempfile::tempdir;
use transport::PeerID;
use transport::memory::MemoryTransport;

const NETWORK: NetworkId = NetworkId([3; 32]);
const ROOT_AUTHORITY: [u8; 32] = [7; 32];
const NOW_MS: u64 = 50;
const DOCUMENT: &str = "field.knot";

/// The holder: owns the vault, issues the grants, answers for every byte.
fn holder() -> InMemoryProvider {
    InMemoryProvider::from_seed([1; 32])
}

fn profile_ref() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}

fn grant(subject: [u8; 32]) -> SignedDelegationCertificate {
    SignedDelegationCertificate::issue(
        &holder(),
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            holder().master_public_key().to_bytes(),
            subject,
            CapabilityScope {
                domain: GRAPHSHELL_DOMAIN.into(),
                resource: NETWORK.0.to_vec(),
                path_prefix: PROJECTION_SERVICE.into(),
                actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
            },
            5,
            10,
            Some(NOW_MS + 3_600_000),
            1,
            [1; 32],
        ),
    )
    .expect("issue certificate")
}

fn policy() -> LocalNetworkPolicy {
    projection_policy(
        NETWORK,
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: holder().master_public_key().to_bytes(),
        }],
        vec![profile_ref()],
        None,
    )
}

fn viewing_profile() -> CapabilityProfile {
    CapabilityProfile::new([
        PresentationCapability::EditableText,
        PresentationCapability::PortableCard,
    ])
}

/// One visitor's transport pair with the holder, keyed so the claimed subject
/// is the peer the carrier proved.
fn pairing(
    visitor: &InMemoryProvider,
    holder_tag: u8,
) -> (MemoryTransport, MemoryTransport, PeerID, PeerID) {
    let subject = visitor.master_public_key().to_bytes();
    let visitor_peer = PeerID::from_bytes(&subject).expect("visitor peer");
    let mut holder_bytes = holder().master_public_key().to_bytes();
    holder_bytes[0] = holder_tag;
    let holder_peer = PeerID::from_bytes(&holder_bytes).expect("holder peer");
    let (server, client) = MemoryTransport::pair(holder_peer, visitor_peer);
    (server, client, holder_peer, visitor_peer)
}

/// Dial the holder and mount the vault as this visitor sees it.
///
/// The session never leaves the thread that opens it and never has to:
/// `Box<dyn Carrier>` is not `Send` by deliberate choice, so a carrier belongs
/// to its own thread. Only plain data crosses back out.
fn mount(
    client: &MemoryTransport,
    holder_peer: PeerID,
    visitor_peer: PeerID,
    visitor: &InMemoryProvider,
    nonce: [u8; 32],
    handle: tokio::runtime::Handle,
) -> (RetainedEndpointSession, ProjectionSession) {
    let subject = visitor.master_public_key().to_bytes();
    let hello = open_session(
        visitor,
        NETWORK,
        profile_ref(),
        TrafficClass::Interactive,
        nonce,
        &projection_binding(visitor_peer),
        vec![grant(subject)],
    )
    .expect("hello");
    let stream = handle
        .block_on(dial_projection_session(
            client,
            holder_peer,
            &hello,
            &policy().limits,
        ))
        .expect("dial")
        .expect("the holder admits this visitor");
    let carrier = NetworkCarrier::over(stream, CarrierRuntime::borrowed(handle));
    let mut retained = RetainedEndpointSession::over(Box::new(carrier), viewing_profile())
        .expect("discover the holder's endpoint");
    let session = retained.mount(0).expect("mount the projected vault");
    (retained, session)
}

/// The document as this visitor currently sees it, with the token that makes a
/// save revision-checked.
fn read_document(
    retained: &mut RetainedEndpointSession,
    session: &ProjectionSession,
) -> (
    sceno::InstanceId,
    String,
    Vec<u8>,
    chirograph::AdvertisedAction,
) {
    retained
        .resolve_all(session)
        .expect("resolve the projection")
        .into_iter()
        .find_map(|(target, presentation)| match presentation.content {
            ResolvedContent::EditableText(editable) if editable.address.ends_with(DOCUMENT) => {
                Some((
                    target,
                    editable.source,
                    editable.base_token,
                    presentation.semantics.actions[0].clone(),
                ))
            }
            _ => None,
        })
        .expect("the holder disclosed the document as editable source")
}

/// A resident host serving one vault on one route.
fn vault_host(root: std::path::PathBuf) -> ResidentProjectionHost {
    let mut catalog = ResidentEndpointCatalog::new();
    catalog
        .register_resumable_notifying("knot", "Knot", move |_| {
            // The admitted context is deliberately unused: a Knot projection is
            // identified by the vault it serves, not by who is looking at it.
            // Authority is the holder's under Option A, so the write grant is
            // the holder's own rather than anything a visitor presented.
            knot::KnotEndpoint::open_writable(&root, knot::KnotWriteGrant::new(4096))
                .map_err(|error| error.to_string())
        })
        .expect("register the vault route");
    ResidentProjectionHost::new(
        policy(),
        ResidentEndpointRoute::new("knot", Duration::from_millis(10)).expect("route"),
        catalog,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn two_place_members_edit_one_held_document_through_projection() {
    let vault = tempdir().unwrap();
    let path = vault.path().join(DOCUMENT);
    fs::write(&path, "# Field\n").unwrap();

    let ada = InMemoryProvider::from_seed([4; 32]);
    let bo = InMemoryProvider::from_seed([9; 32]);
    let (ada_server, ada_client, ada_holder_peer, ada_peer) = pairing(&ada, 0xa1);
    let (bo_server, bo_client, bo_holder_peer, bo_peer) = pairing(&bo, 0xb2);
    let mut host = vault_host(vault.path().to_path_buf());

    // Both visitors mount before either edits, so the one who is not editing
    // has an acknowledgement to resume from.
    let mounted = Arc::new(Barrier::new(2));
    let handle = tokio::runtime::Handle::current();

    let ada_barrier = Arc::clone(&mounted);
    let ada_handle = handle.clone();
    let ada_thread = tokio::task::spawn_blocking(move || {
        let (mut retained, session) = mount(
            &ada_client,
            ada_holder_peer,
            ada_peer,
            &ada,
            [21; 32],
            ada_handle,
        );
        let (target, source, token, action) = read_document(&mut retained, &session);
        ada_barrier.wait();

        // Ada edits. She holds no replica: this is an intent the holder runs.
        let result = retained
            .invoke(
                &session,
                target,
                &action,
                &SaveTextV1 {
                    base_token: token,
                    source: "# Field\n\nAda was here.\n".into(),
                },
            )
            .expect("submit the save");
        retained.close().expect("close Ada's session");
        (source, result)
    });

    let bo_barrier = Arc::clone(&mounted);
    let bo_handle = handle.clone();
    let bo_thread = tokio::task::spawn_blocking(move || {
        let (mut retained, session) = mount(
            &bo_client,
            bo_holder_peer,
            bo_peer,
            &bo,
            [22; 32],
            bo_handle,
        );
        let (_, before, _, _) = read_document(&mut retained, &session);
        bo_barrier.wait();

        // Bo asked for nothing. The holder rings, and the ordinary resume path
        // brings Ada's edit to him.
        let heard = retained.wait_for_change().expect("Bo hears the bell");
        let (_, after, _, _) = read_document(&mut retained, &session);
        retained.close().expect("close Bo's session");
        (before, heard, after)
    });

    let ada_served = host
        .accept_one(&ada_server, || NOW_MS)
        .await
        .expect("accept Ada")
        .expect("Ada is admitted");
    let bo_served = host
        .accept_one(&bo_server, || NOW_MS)
        .await
        .expect("accept Bo")
        .expect("Bo is admitted");
    assert_ne!(
        ada_served.subject(),
        bo_served.subject(),
        "two peers, not one peer twice"
    );

    let (ada_saw, saved) = ada_thread.await.unwrap();
    let (bo_before, bo_heard, bo_after) = bo_thread.await.unwrap();
    ada_served.finished().await.expect("join").expect("served");
    bo_served.finished().await.expect("join").expect("served");

    assert_eq!(ada_saw, "# Field\n", "Ada opened the holder's document");
    assert_eq!(bo_before, "# Field\n", "so did Bo");
    assert_eq!(saved, IntentResult::Accepted, "the holder took Ada's edit");
    assert!(bo_heard, "the bell carried a revision Bo had not seen");
    assert_eq!(
        bo_after, "# Field\n\nAda was here.\n",
        "Bo sees what the holder holds"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# Field\n\nAda was here.\n",
        "and the holder's own file is the truth both were reading"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn a_visitor_whose_holder_goes_away_is_told_rather_than_shown_a_stale_copy() {
    // The done condition's third clause. A visitor holds no replica, so when
    // the holder is gone there is nothing to fall back to and the honest thing
    // is to say so. What must not happen is a scene that still reads Live,
    // which would offer a save that can never land.
    let vault = tempdir().unwrap();
    let path = vault.path().join(DOCUMENT);
    fs::write(&path, "# Field\n").unwrap();

    let ada = InMemoryProvider::from_seed([4; 32]);
    let (ada_server, ada_client, ada_holder_peer, ada_peer) = pairing(&ada, 0xa1);

    let (mounted_tx, mounted_rx) = std::sync::mpsc::channel();
    let (gone_tx, gone_rx) = std::sync::mpsc::channel();
    let handle = tokio::runtime::Handle::current();

    let visitor = tokio::task::spawn_blocking(move || {
        let (mut retained, session) = mount(
            &ada_client,
            ada_holder_peer,
            ada_peer,
            &ada,
            [21; 32],
            handle,
        );
        let (_, source, _, _) = read_document(&mut retained, &session);
        let live = retained.client().mounted(&session).unwrap().status;
        mounted_tx.send(()).unwrap();

        gone_rx.recv().unwrap();
        // Asking anything at all is enough to learn the holder is gone.
        let refused = retained.resnapshot(&session).unwrap_err();
        let after = retained.client().mounted(&session).unwrap().status;
        // The scene is kept, so a host can still show what was there.
        let still_there = retained
            .client()
            .mounted(&session)
            .map(|scene| scene.scene.active_item_count())
            .unwrap_or_default();
        (source, live, refused, after, still_there)
    });

    // Serve until the visitor has mounted, then stop being the holder.
    let mut admitted = accept_projection_session(
        &ada_server,
        &policy(),
        &RevocationLedger::default(),
        NOW_MS,
        0,
    )
    .await
    .expect("accept")
    .expect("admitted");
    let authority = SessionAuthority::retain_admitted(&admitted);
    let revocations = std::sync::RwLock::new(RevocationLedger::new());
    let mut endpoint =
        knot::KnotEndpoint::open_writable(vault.path(), knot::KnotWriteGrant::new(4096))
            .expect("open the vault");
    let mut resume = |endpoint: &mut knot::KnotEndpoint, request: ResumeRequest| {
        ResumableProjectionSource::resume(endpoint, request).map_err(|error| error.to_string())
    };
    let waiting = tokio::task::spawn_blocking(move || mounted_rx.recv().unwrap());
    tokio::select! {
        _ = serve_admitted_session_notifying(
            &mut admitted,
            &authority,
            &revocations,
            &mut endpoint,
            &mut resume,
            || NOW_MS,
            Duration::from_millis(10),
        ) => panic!("the visitor did not close this session"),
        _ = waiting => {}
    }
    // The holder goes away: its stream, its endpoint, and its transport.
    drop(admitted);
    drop(endpoint);
    drop(ada_server);
    gone_tx.send(()).unwrap();

    let (source, live, refused, after, still_there) = visitor.await.unwrap();
    assert_eq!(source, "# Field\n", "the visitor saw the holder's document");
    assert_eq!(live, SessionStatus::Live, "and it was live while served");
    assert!(
        refused.contains("no longer reachable"),
        "the refusal names the cause: {refused}"
    );
    assert_eq!(
        after,
        SessionStatus::Disconnected,
        "the visitor is told, rather than left holding a Live scene it cannot save"
    );
    assert!(
        still_there > 0,
        "the scene is kept so a host can show what was there, just not offer to save it"
    );
}
