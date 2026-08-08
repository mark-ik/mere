//! Manual two-process receipt runner for private Knot publishing.
//!
//! ```text
//! # On the reader device, make a public key from its private local seed:
//! KNOT_PUBLISH_READER_SEED=<64 hex> cargo run -p knot --example knot_publish_peer -- reader-key
//!
//! # On the holder, issue a ticket only to that public key and print it:
//! KNOT_PUBLISH_HOLDER_SEED=<64 hex> KNOT_PUBLISH_READER_PUBLIC=<64 hex> \
//!   cargo run -p knot --example knot_publish_peer -- hold
//!
//! # Paste the printed ticket on the reader. The reader never receives the
//! # holder's seed, vault key, paired-writer key, or sync store:
//! KNOT_PUBLISH_READER_SEED=<same 64 hex> \
//!   cargo run -p knot --example knot_publish_peer -- visit <ticket>
//!
//! # On the same LAN, prove mDNS discovery without adding the endpoint ticket
//! # to the address book. The ticket still supplies the holder identity and
//! # signed publication delegation, but never a dial address:
//! KNOT_PUBLISH_READER_SEED=<same 64 hex> \
//!   cargo run -p knot --example knot_publish_peer -- visit-mdns <ticket>
//! ```
//!
//! `KNOT_PUBLISH_SOURCE` may set the holder's fixture source. The runner is a
//! receipt harness: it creates one in-memory retained source for its lifetime
//! and serves one request, rather than exposing a product UI or directory.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use knot::{
    KNOT_PUBLISH_DOMAIN, KNOT_PUBLISH_READ_ACTION, KNOT_PUBLISH_SERVICE, KnotPublishCatalog,
    KnotPublishHost, KnotPublishHostLimits, KnotPublishRead, KnotShareTicket, KnotSyncEvent,
    KnotSyncStore, KnotVault, PublicationId, PublishRequest, decode_response, encode_request,
    publication_path, publish_alpn, publish_policy,
};
use notochord::{
    NetworkId, ProfileRef, RequestedAction, SessionHello, TrafficClass, TrustedRoot,
    initiate_session, read_frame, write_frame,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
    SignedDelegationCertificate, SignedDelegationRevocation,
};
use personae::{IdentityProvider, InMemoryProvider};
use transport::p2panda_transport::{MdnsDiscoveryMode, P2pandaTransport};
use transport::{PeerID, Transport, initiator_binding};

const ROOT_AUTHORITY: [u8; 32] = [7; 32];
/// Bounded time for mDNS to populate the ticket-bound holder's address.
const MDNS_DIAL_DEADLINE: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PeerRoute {
    /// Explicit endpoint information from the share ticket. This is the
    /// off-LAN path and can also be used when discovery is unavailable.
    Ticket,
    /// mDNS names the known holder identity on the local network. No endpoint
    /// information is read from the ticket, and this runner registers no
    /// relay, so a successful path is direct LAN transport.
    Mdns,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("knot_publish_peer: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("reader-key") => {
            let reader = reader_identity()?;
            println!("{}", knot::hex32(&reader.master_public_key().to_bytes()));
            Ok(())
        }
        Some("hold") => hold(false).await,
        Some("hold-revocation") => hold(true).await,
        Some("visit") => {
            let ticket = args
                .next()
                .ok_or_else(|| "visit needs the ticket printed by hold".to_string())?;
            visit(&ticket, PeerRoute::Ticket).await
        }
        Some("visit-mdns") => {
            let ticket = args
                .next()
                .ok_or_else(|| "visit-mdns needs the ticket printed by hold".to_string())?;
            visit(&ticket, PeerRoute::Mdns).await
        }
        _ => Err(
            "use reader-key, hold, hold-revocation, visit <ticket>, or visit-mdns <ticket>".into(),
        ),
    }
}

async fn hold(revoke_after_first_fetch: bool) -> Result<(), String> {
    let seed = env_key("KNOT_PUBLISH_HOLDER_SEED")?;
    let reader = env_key("KNOT_PUBLISH_READER_PUBLIC")?;
    let holder = InMemoryProvider::from_seed(seed);
    let network = network()?;
    let carrier = P2pandaTransport::builder_from_seed(seed)
        .alpns(vec![publish_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|error| format!("bind holder carrier: {error}"))?;
    if carrier.local_peer_id().to_bytes() != holder.master_public_key().to_bytes() {
        return Err("holder carrier and Personae identity differ".into());
    }

    let source =
        std::env::var("KNOT_PUBLISH_SOURCE").unwrap_or_else(|_| "# Shared privately\n".into());
    let vault_root = tempfile::tempdir().map_err(|error| format!("fixture vault: {error}"))?;
    let vault = Arc::new(
        KnotVault::open(vault_root.path(), [0x44; 32])
            .map_err(|error| format!("fixture vault: {error}"))?,
    );
    let store = KnotSyncStore::in_memory(network.0, [holder.master_public_key().to_bytes()]);
    store
        .author(
            holder.master_keypair().to_seed(),
            &vault,
            &KnotSyncEvent::Put(knot::VaultDocument {
                id: "receipt-source".into(),
                title: "Private receipt source".into(),
                body: source.into_bytes(),
                media_type: "text/vnd.knot".into(),
            }),
        )
        .await
        .map_err(|error| format!("author source: {error}"))?;
    let mut catalog = KnotPublishCatalog::default();
    let publication = catalog.publish("receipt-source");
    let ticket = KnotShareTicket::new(
        carrier.local_peer_id().to_bytes(),
        carrier
            .ticket()
            .await
            .map_err(|error| format!("holder ticket: {error}"))?,
        network,
        publication,
        vec![grant(&holder, reader, network, publication)],
        None,
    );
    let encoded_ticket = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&ticket).map_err(|error| format!("encode share ticket: {error}"))?,
    );
    let policy = publish_policy(
        network,
        vec![TrustedRoot {
            authority: ROOT_AUTHORITY,
            issuer: holder.master_public_key().to_bytes(),
        }],
        vec![profile_ref()],
        Some(1),
    );
    let host = KnotPublishHost::new(
        holder.master_keypair().clone(),
        policy,
        store,
        vault,
        catalog,
        KnotPublishHostLimits {
            max_concurrent_sessions: 1,
            ..KnotPublishHostLimits::default()
        },
    );

    println!("knot_publish_peer hold");
    println!("  publication: {}", publication.as_uuid());
    println!("  ticket: {encoded_ticket}");
    println!(
        "  paste on the reader: cargo run -p knot --example knot_publish_peer -- visit <ticket>"
    );
    println!("  waiting for one distinct reader identity...");
    let outcome = host
        .accept_and_serve(&carrier)
        .await
        .map_err(|error| format!("serve: {error}"))?;
    println!("  holder outcome: {outcome:?}");
    if revoke_after_first_fetch {
        let certificate = ticket
            .delegations
            .first()
            .ok_or_else(|| "fixture ticket lost its delegation".to_string())?;
        let revocation = SignedDelegationRevocation::issue(
            &holder,
            DelegationRevocation::new(
                certificate.certificate.id(),
                holder.master_public_key().to_bytes(),
                certificate.certificate.scope.clone(),
                now_ms(),
                [0x56; 32],
            ),
        )
        .map_err(|error| format!("issue revocation: {error}"))?;
        if !host.revocations().write().await.fold(&revocation) {
            return Err("holder could not fold its own signed revocation".into());
        }
        println!("  reader delegation revoked; retry the same ticket for the refusal receipt...");
        let outcome = host
            .accept_and_serve(&carrier)
            .await
            .map_err(|error| format!("serve revoked reader: {error}"))?;
        println!("  holder post-revocation outcome: {outcome:?}");
    }
    Ok(())
}

async fn visit(encoded_ticket: &str, route: PeerRoute) -> Result<(), String> {
    let reader_seed = env_key("KNOT_PUBLISH_READER_SEED")?;
    let reader = InMemoryProvider::from_seed(reader_seed);
    let ticket: KnotShareTicket = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded_ticket)
            .map_err(|error| format!("ticket encoding: {error}"))?,
    )
    .map_err(|error| format!("ticket JSON: {error}"))?;
    if ticket.service_path != KNOT_PUBLISH_SERVICE || ticket.delegations.is_empty() {
        return Err("ticket does not carry a publishing delegation".into());
    }
    let carrier = P2pandaTransport::builder_from_seed(reader_seed)
        .alpns(vec![publish_alpn()])
        .mdns(MdnsDiscoveryMode::Active)
        .bind()
        .await
        .map_err(|error| format!("bind reader carrier: {error}"))?;
    if carrier.local_peer_id().to_bytes() != reader.master_public_key().to_bytes() {
        return Err("reader carrier and Personae identity differ".into());
    }
    let peer = PeerID::from_bytes(&ticket.publisher)
        .map_err(|error| format!("ticket publisher identity: {error}"))?;
    if route == PeerRoute::Ticket {
        let registered = carrier
            .add_peer_ticket(&ticket.endpoint_ticket)
            .await
            .map_err(|error| format!("add holder ticket: {error}"))?;
        if registered != peer {
            return Err("endpoint ticket identity does not match the share ticket".into());
        }
    } else {
        // mDNS starts asynchronously. Force the endpoint now, then retry the
        // identity-only dial while the discovery actor fills its address book.
        // The share ticket is still required below for Notochord delegation;
        // it simply contributes no carrier address in this branch.
        carrier
            .ticket()
            .await
            .map_err(|error| format!("start local discovery: {error}"))?;
        println!(
            "  waiting for mDNS to resolve holder {}",
            short(&ticket.publisher)
        );
    }
    let started = Instant::now();
    let outer = loop {
        match carrier.connect(peer, publish_alpn()).await {
            Ok(outer) => break outer,
            Err(error) if route == PeerRoute::Mdns && started.elapsed() < MDNS_DIAL_DEADLINE => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let _ = error;
            }
            Err(error) if route == PeerRoute::Mdns => {
                return Err(format!(
                    "mDNS did not resolve the holder before {} seconds: {error}",
                    MDNS_DIAL_DEADLINE.as_secs()
                ));
            }
            Err(error) => return Err(format!("dial holder from ticket: {error}")),
        }
    };
    let (mut stream, noise_peer) =
        transport::noise::secure_initiator(reader.master_keypair(), outer, &publish_alpn())
            .await
            .map_err(|error| format!("Noise: {error}"))?;
    if noise_peer.to_bytes() != ticket.publisher {
        return Err("Noise identity does not match the ticket publisher".into());
    }
    let hello = SessionHello::issue(
        &reader,
        ticket.network,
        profile_ref(),
        RequestedAction {
            domain: KNOT_PUBLISH_DOMAIN.into(),
            path: publication_path(ticket.publication),
            action: KNOT_PUBLISH_READ_ACTION.into(),
        },
        TrafficClass::Interactive,
        nonce(reader_seed),
        &initiator_binding(&publish_alpn(), carrier.local_peer_id()),
        ticket.delegations.clone(),
    )
    .map_err(|error| format!("sign session hello: {error}"))?;
    let reply = initiate_session(&mut stream, &hello, &Default::default())
        .await
        .map_err(|error| format!("Notochord admission: {error}"))?;
    if !reply.is_accept() {
        return Err(format!("holder refused reader: {reply:?}"));
    }
    let limits = KnotPublishHostLimits::default().wire;
    let request = encode_request(
        &PublishRequest::GetCurrent {
            publication: ticket.publication,
        },
        limits,
    )
    .map_err(|error| format!("encode request: {error}"))?;
    write_frame(&mut stream, &request, limits.max_request_bytes)
        .await
        .map_err(|error| format!("write request: {error}"))?;
    let response = read_frame(&mut stream, limits.max_response_bytes)
        .await
        .map_err(|error| format!("read response: {error}"))?;
    let read = decode_response(&response, limits)
        .and_then(|response| response.into_read())
        .map_err(|error| format!("decode response: {error}"))?;
    let KnotPublishRead::Document(document) = read else {
        return Err("holder made the selected publication unavailable".into());
    };
    if !ticket.accepts(&document) {
        return Err(
            "response did not satisfy the ticket's publication, pin, and digest checks".into(),
        );
    }
    println!("knot_publish_peer visit");
    println!(
        "  route:       {}",
        match route {
            PeerRoute::Ticket => "ticket endpoint",
            PeerRoute::Mdns => "mDNS direct LAN",
        }
    );
    println!("  holder:      {}", short(&ticket.publisher));
    println!("  publication: {}", document.publication.as_uuid());
    println!("  head:        {}", short(&document.operation));
    println!("  digest:      {}", short(&document.body_digest));
    println!("  media type:  {}", document.media_type);
    println!("  bytes:       {}", document.body.len());
    Ok(())
}

fn grant(
    holder: &InMemoryProvider,
    reader: [u8; 32],
    network: NetworkId,
    publication: PublicationId,
) -> SignedDelegationCertificate {
    let issued_at = now_ms().saturating_sub(60_000);
    SignedDelegationCertificate::issue(
        holder,
        DelegationCertificate::new(
            DelegationParent::Root(ROOT_AUTHORITY),
            holder.master_public_key().to_bytes(),
            reader,
            CapabilityScope {
                domain: KNOT_PUBLISH_DOMAIN.into(),
                resource: network.0.to_vec(),
                path_prefix: publication_path(publication),
                actions: [KNOT_PUBLISH_READ_ACTION.into()].into_iter().collect(),
            },
            issued_at,
            issued_at,
            Some(now_ms().saturating_add(3_600_000)),
            1,
            [0x55; 32],
        ),
    )
    .expect("holder issues its own receipt grant")
}

fn reader_identity() -> Result<InMemoryProvider, String> {
    Ok(InMemoryProvider::from_seed(env_key(
        "KNOT_PUBLISH_READER_SEED",
    )?))
}

fn env_key(name: &str) -> Result<[u8; 32], String> {
    let value = std::env::var(name).map_err(|_| format!("set {name} to 64 hex characters"))?;
    knot::parse_hex32(&value).map_err(|error| error.to_string())
}

fn network() -> Result<NetworkId, String> {
    let label =
        std::env::var("KNOT_PUBLISH_NETWORK").unwrap_or_else(|_| "knot-publish-receipt".into());
    if label.is_empty() {
        return Err("KNOT_PUBLISH_NETWORK must not be empty".into());
    }
    Ok(NetworkId(*blake3::hash(label.as_bytes()).as_bytes()))
}

fn profile_ref() -> ProfileRef {
    ProfileRef {
        id: "mere.base".into(),
        revision: 1,
    }
}

fn nonce(seed: [u8; 32]) -> [u8; 32] {
    let mut material = seed.to_vec();
    material.extend_from_slice(&now_ms().to_le_bytes());
    *blake3::hash(&material).as_bytes()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn short(bytes: &[u8; 32]) -> String {
    bytes
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
