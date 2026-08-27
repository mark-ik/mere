// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Recipient-side client for one private Knot publication.
//!
//! The client consumes an out-of-band ticket and asks for exactly the named
//! publication. It never uses the candidate catalog request, so a reader
//! cannot turn a valid share into a catalog probe.

use base64::Engine as _;
use notochord::{
    FrameError, HandshakeError, IoHandshakeError, ProfileRef, RequestedAction, SessionHello,
    TrafficClass, initiate_session, read_frame, write_frame,
};
use personae::{Ed25519Keypair, InMemoryProvider};
use tokio::io::AsyncWriteExt;
use transport::noise::secure_initiator;
use transport::{PeerID, Transport, TransportError, initiator_binding};

use crate::{
    KNOT_PUBLISH_DOMAIN, KNOT_PUBLISH_READ_ACTION, KNOT_PUBLISH_SERVICE, KNOT_SHARE_TICKET_VERSION,
    KnotPublishRead, KnotShareTicket, PublishRequest, PublishWireError, PublishWireLimits,
    decode_response, encode_request, publication_path, publish_alpn,
};

/// Stable derivation label for the reader identity advertised to a publisher.
///
/// A product derives this key from its Personae root, uses it for the outer
/// carrier and inner Noise handshake, and gives only its public half to the
/// publisher. It is deliberately distinct from a device or root identity.
pub const KNOT_PUBLISH_READER_KEY_CONTEXT: &[u8] = b"mere/knot-publish/reader/v1";

/// Why a recipient could not import or read one private share.
#[derive(Debug, thiserror::Error)]
pub enum KnotPublishClientError {
    #[error("the handoff ticket is malformed: {0}")]
    Ticket(String),
    #[error("the ticket is not issued to this reader key")]
    WrongRecipient,
    #[error("the reader carrier identity differs from its reader key")]
    CarrierIdentity,
    #[error("the ticket's publisher key is invalid")]
    PublisherIdentity,
    #[error("the publishing carrier failed: {0}")]
    Transport(#[from] TransportError),
    #[error("the publishing Noise handshake failed: {0}")]
    Noise(TransportError),
    #[error(transparent)]
    Handshake(#[from] HandshakeError),
    #[error(transparent)]
    Session(#[from] IoHandshakeError),
    #[error("the publishing host refused the reader before disclosure")]
    Refused,
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error(transparent)]
    Wire(#[from] PublishWireError),
    #[error("the host returned a document outside this ticket's commitment")]
    TicketCommitment,
}

impl KnotPublishClientError {
    /// Whether trying the ticket's explicit endpoint is a sensible next hop.
    /// Admission, commitment, and decoding failures are final: a fallback
    /// address cannot make them legitimate.
    pub const fn allows_endpoint_fallback(&self) -> bool {
        matches!(self, Self::Transport(_))
    }
}

/// Decode a private handoff ticket without accepting a malformed shape.
pub fn decode_share_ticket(encoded: &str) -> Result<KnotShareTicket, KnotPublishClientError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.trim())
        .map_err(|_| KnotPublishClientError::Ticket("not URL-safe base64".into()))?;
    let ticket = serde_json::from_slice::<KnotShareTicket>(&bytes)
        .map_err(|_| KnotPublishClientError::Ticket("not a Knot share ticket".into()))?;
    validate_ticket(&ticket)?;
    Ok(ticket)
}

/// Encode a previously validated ticket for a private handoff channel.
pub fn encode_share_ticket(ticket: &KnotShareTicket) -> Result<String, KnotPublishClientError> {
    validate_ticket(ticket)?;
    let bytes = serde_json::to_vec(ticket)
        .map_err(|_| KnotPublishClientError::Ticket("could not encode ticket".into()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// Read the single current document named by `ticket`.
///
/// `reader` must be the protocol-scoped key whose public half the publisher
/// certified. The carrier must use that same key, binding the outer P2P peer,
/// Noise identity, and Notochord session subject together.
pub async fn fetch_published_document<T: Transport>(
    carrier: &T,
    reader: &Ed25519Keypair,
    profile: ProfileRef,
    ticket: &KnotShareTicket,
) -> Result<KnotPublishRead, KnotPublishClientError> {
    validate_ticket(ticket)?;
    let reader_peer = PeerID::from_bytes(&reader.public_key().to_bytes())
        .map_err(|_| KnotPublishClientError::CarrierIdentity)?;
    if carrier.local_peer_id().to_bytes() != reader_peer.to_bytes() {
        return Err(KnotPublishClientError::CarrierIdentity);
    }
    let recipient = ticket
        .delegations
        .last()
        .map(|certificate| certificate.certificate.subject)
        .ok_or_else(|| KnotPublishClientError::Ticket("has no delegation".into()))?;
    if recipient != reader.public_key().to_bytes() {
        return Err(KnotPublishClientError::WrongRecipient);
    }
    let publisher = PeerID::from_bytes(&ticket.publisher)
        .map_err(|_| KnotPublishClientError::PublisherIdentity)?;

    let outer = carrier.connect(publisher, publish_alpn()).await?;
    let (mut stream, noise_peer) = secure_initiator(reader, outer, &publish_alpn())
        .await
        .map_err(KnotPublishClientError::Noise)?;
    if noise_peer.to_bytes() != publisher.to_bytes() {
        let _ = stream.shutdown().await;
        return Err(KnotPublishClientError::PublisherIdentity);
    }

    // SessionHello is signed by a derived session key beneath this reader
    // protocol key. The temporary provider keeps that protocol key only for
    // this request and zeroizes its copy on drop.
    let provider = InMemoryProvider::from_seed(reader.to_seed());
    let hello = SessionHello::issue(
        &provider,
        ticket.network,
        profile,
        RequestedAction {
            domain: KNOT_PUBLISH_DOMAIN.into(),
            path: publication_path(ticket.publication),
            action: KNOT_PUBLISH_READ_ACTION.into(),
        },
        TrafficClass::Interactive,
        nonce(),
        &initiator_binding(&publish_alpn(), reader_peer),
        ticket.delegations.clone(),
    )?;
    let reply = initiate_session(&mut stream, &hello, &Default::default()).await?;
    if !reply.is_accept() {
        let _ = stream.shutdown().await;
        return Err(KnotPublishClientError::Refused);
    }

    let limits = PublishWireLimits::default();
    let request = encode_request(
        &PublishRequest::GetCurrent {
            publication: ticket.publication,
        },
        limits,
    )?;
    write_frame(&mut stream, &request, limits.max_request_bytes).await?;
    let response = read_frame(&mut stream, limits.max_response_bytes).await?;
    let read = decode_response(&response, limits)?.into_read()?;
    if let KnotPublishRead::Document(document) = &read
        && !ticket.accepts(document)
    {
        return Err(KnotPublishClientError::TicketCommitment);
    }
    Ok(read)
}

fn validate_ticket(ticket: &KnotShareTicket) -> Result<(), KnotPublishClientError> {
    if ticket.version != KNOT_SHARE_TICKET_VERSION {
        return Err(KnotPublishClientError::Ticket("unsupported version".into()));
    }
    if ticket.service_path != KNOT_PUBLISH_SERVICE {
        return Err(KnotPublishClientError::Ticket("wrong service path".into()));
    }
    if ticket.endpoint_ticket.trim().is_empty() {
        return Err(KnotPublishClientError::Ticket(
            "missing endpoint fallback".into(),
        ));
    }
    if ticket.delegations.is_empty() {
        return Err(KnotPublishClientError::Ticket("missing delegation".into()));
    }
    Ok(())
}

fn nonce() -> [u8; 32] {
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let mut nonce = [0_u8; 32];
    nonce[..16].copy_from_slice(first.as_bytes());
    nonce[16..].copy_from_slice(second.as_bytes());
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PublicationId;
    use notochord::NetworkId;

    #[test]
    fn handoff_encoding_rejects_a_ticket_without_a_delegation() {
        let ticket = KnotShareTicket::new(
            [1; 32],
            "endpoint-ticket",
            NetworkId([2; 32]),
            PublicationId::new(),
            Vec::new(),
            None,
        );
        assert!(matches!(
            encode_share_ticket(&ticket),
            Err(KnotPublishClientError::Ticket(_))
        ));
    }
}
