//! K2: place members edit a document its holder keeps.
//!
//! Option A in one test. The holder owns the vault and every byte on disk; a
//! visitor never holds a replica, only a live projection it edits by sending
//! intents. Nothing here is stubbed: real `KnotEndpoint`s over a real
//! directory, admitted over a transport, driven by the blocking network
//! carrier through the ordinary `RetainedEndpointSession`.
//!
//! ## Why this serves sessions by hand rather than through the catalog
//!
//! `ResidentProjectionHost` spawns each session, so
//! `ResidentEndpointCatalog` requires `Send` endpoints. `KnotEndpoint` is not
//! `Send`, and for exactly one reason: `KnotEffectAuthority` holds
//! `BlockEvaluators`, a map of `Box<dyn BlockEvaluator>`, and the only
//! implementor is `RhaiEvaluator` wrapping a `rhai::Engine` that is not `Send`
//! without rhai's `sync` feature. Every other part of the endpoint, including
//! the directory source and its watcher, is fine.
//!
//! That is a live design question rather than a fact this test should route
//! around quietly, so it serves both sessions on one task with `join!`, which
//! needs no `Send` at all. What is proven below is the K2 mechanism; how a
//! resident host should schedule a non-`Send` endpoint is recorded in the Knot
//! plan and is not decided here.
//!
//! ## What the memory transport stands in for
//!
//! Each visitor gets its own paired transport, so each carries a distinct
//! authenticated subject with its own grant: these are two peers, not one peer
//! twice. The holder therefore holds two transports rather than one accepting
//! many, which is an artifact of a fixture that pairs exactly two nodes.

use std::fs;
use std::sync::{Arc, Barrier, RwLock};
use std::time::Duration;

use graphshell::admission::{CONNECT_ACTION, GRAPHSHELL_DOMAIN, PROJECTION_SERVICE, open_session};
use graphshell::carrier::{accept_projection_session, projection_policy};
use graphshell::client::{ResolvedContent, RetainedEndpointSession};
use graphshell::lifecycle::SessionAuthority;
use graphshell::network_carrier::{
    CarrierRuntime, NetworkCarrier, dial_projection_session, projection_binding,
};
use graphshell::session_notices::serve_admitted_session_notifying;
use graphshell_endpoint::ResumableProjectionSource;
use graphshell_protocol::{
    CapabilityProfile, IntentResult, PresentationCapability, ResumeRequest, SaveTextV1,
};
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

/// Everything one visitor does, start to finish, on one thread.
///
/// The session never leaves this thread and never has to: `Box<dyn Carrier>`
/// is not `Send` by deliberate choice, so a carrier belongs to the thread that
/// opened it. Only plain data comes back out.
struct Visit {
    client: MemoryTransport,
    holder_peer: PeerID,
    visitor_peer: PeerID,
    visitor: InMemoryProvider,
    nonce: [u8; 32],
    handle: tokio::runtime::Handle,
    mounted: Arc<Barrier>,
}

impl Visit {
    fn open(
        self,
    ) -> (
        RetainedEndpointSession,
        graphshell_protocol::ProjectionSession,
    ) {
        let subject = self.visitor.master_public_key().to_bytes();
        let hello = open_session(
            &self.visitor,
            NETWORK,
            profile_ref(),
            TrafficClass::Interactive,
            self.nonce,
            &projection_binding(self.visitor_peer),
            vec![grant(subject)],
        )
        .expect("hello");
        let stream = self
            .handle
            .block_on(dial_projection_session(
                &self.client,
                self.holder_peer,
                &hello,
                &policy().limits,
            ))
            .expect("dial")
            .expect("the holder admits this visitor");
        let carrier = NetworkCarrier::over(stream, CarrierRuntime::borrowed(self.handle.clone()));
        let mut retained = RetainedEndpointSession::over(Box::new(carrier), viewing_profile())
            .expect("discover the holder's endpoint");
        let session = retained.mount(0).expect("mount the projected vault");
        (retained, session)
    }
}

/// The document as this visitor currently sees it, with the token that makes a
/// save revision-checked.
fn read_document(
    retained: &mut RetainedEndpointSession,
    session: &graphshell_protocol::ProjectionSession,
) -> (
    sceno::InstanceId,
    String,
    Vec<u8>,
    graphshell_protocol::AdvertisedAction,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn two_place_members_edit_one_held_document_through_projection() {
    let vault = tempdir().unwrap();
    let path = vault.path().join(DOCUMENT);
    fs::write(&path, "# Field\n").unwrap();

    let ada = InMemoryProvider::from_seed([4; 32]);
    let bo = InMemoryProvider::from_seed([9; 32]);
    let (ada_server, ada_client, ada_holder_peer, ada_peer) = pairing(&ada, 0xa1);
    let (bo_server, bo_client, bo_holder_peer, bo_peer) = pairing(&bo, 0xb2);

    // Both visitors must be mounted before either edits, so the one who is not
    // editing has an acknowledgement to resume from.
    let mounted = Arc::new(Barrier::new(2));
    let handle = tokio::runtime::Handle::current();

    let ada_visit = Visit {
        client: ada_client,
        holder_peer: ada_holder_peer,
        visitor_peer: ada_peer,
        visitor: ada,
        nonce: [21; 32],
        handle: handle.clone(),
        mounted: Arc::clone(&mounted),
    };
    let ada_thread = tokio::task::spawn_blocking(move || {
        let barrier = Arc::clone(&ada_visit.mounted);
        let (mut retained, session) = ada_visit.open();
        let (target, source, token, action) = read_document(&mut retained, &session);
        barrier.wait();

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

    let bo_visit = Visit {
        client: bo_client,
        holder_peer: bo_holder_peer,
        visitor_peer: bo_peer,
        visitor: bo,
        nonce: [22; 32],
        handle: handle.clone(),
        mounted: Arc::clone(&mounted),
    };
    let bo_thread = tokio::task::spawn_blocking(move || {
        let barrier = Arc::clone(&bo_visit.mounted);
        let (mut retained, session) = bo_visit.open();
        let (_, before, _, _) = read_document(&mut retained, &session);
        barrier.wait();

        // Bo asked for nothing. The holder rings, and the ordinary resume path
        // brings Ada's edit to him.
        let heard = retained.wait_for_change().expect("Bo hears the bell");
        let (_, after, _, _) = read_document(&mut retained, &session);
        retained.close().expect("close Bo's session");
        (before, heard, after)
    });

    // Admitted before either can be served, while both visitors are blocked in
    // their handshakes.
    let revocations = RwLock::new(RevocationLedger::new());
    let mut ada_admitted = accept_projection_session(
        &ada_server,
        &policy(),
        &RevocationLedger::default(),
        NOW_MS,
        0,
    )
    .await
    .expect("accept Ada")
    .expect("Ada is admitted");
    let mut bo_admitted = accept_projection_session(
        &bo_server,
        &policy(),
        &RevocationLedger::default(),
        NOW_MS,
        1,
    )
    .await
    .expect("accept Bo")
    .expect("Bo is admitted");
    assert_ne!(
        ada_admitted.principal.subject, bo_admitted.principal.subject,
        "two peers, not one peer twice"
    );

    let ada_authority = SessionAuthority::retain_admitted(&ada_admitted);
    let bo_authority = SessionAuthority::retain_admitted(&bo_admitted);

    // One endpoint per session over one vault, which is what the catalog would
    // build. They converge through the holder's files, not shared memory.
    let mut ada_endpoint =
        knot::KnotEndpoint::open_writable(vault.path(), knot::KnotWriteGrant::new(4096))
            .expect("open the vault for Ada");
    let mut bo_endpoint =
        knot::KnotEndpoint::open_writable(vault.path(), knot::KnotWriteGrant::new(4096))
            .expect("open the vault for Bo");
    let mut ada_resume = |endpoint: &mut knot::KnotEndpoint, request: ResumeRequest| {
        ResumableProjectionSource::resume(endpoint, request).map_err(|error| error.to_string())
    };
    let mut bo_resume = |endpoint: &mut knot::KnotEndpoint, request: ResumeRequest| {
        ResumableProjectionSource::resume(endpoint, request).map_err(|error| error.to_string())
    };

    let (ada_out, bo_out, ada_served, bo_served) = tokio::join!(
        ada_thread,
        bo_thread,
        serve_admitted_session_notifying(
            &mut ada_admitted,
            &ada_authority,
            &revocations,
            &mut ada_endpoint,
            &mut ada_resume,
            || NOW_MS,
            Duration::from_millis(10),
        ),
        serve_admitted_session_notifying(
            &mut bo_admitted,
            &bo_authority,
            &revocations,
            &mut bo_endpoint,
            &mut bo_resume,
            || NOW_MS,
            Duration::from_millis(10),
        ),
    );
    ada_served.expect("Ada's session served");
    bo_served.expect("Bo's session served");

    let (ada_saw, saved) = ada_out.unwrap();
    let (bo_before, bo_heard, bo_after) = bo_out.unwrap();

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
