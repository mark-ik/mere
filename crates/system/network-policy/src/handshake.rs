//! The bounded session handshake (plan V6, decision D6).
//!
//! Sans-io on purpose: this module turns bytes into decisions and decisions
//! into bytes, and never touches a socket. The transport adapter above it
//! reads one length-bounded frame, calls [`respond`], writes the reply, and
//! only then hands the stream to the application. Keeping it io-free is what
//! lets the whole admission matrix run as ordinary unit tests, and what keeps
//! this crate free of the QUIC and mesh dependencies its callers carry.
//!
//! ## What the signature covers
//!
//! The initiator signs a canonical transcript with a personae-derived key,
//! attested to its master identity. The transcript binds every field of the
//! hello *and* the responder's own view of the connection ([`SessionBinding`]):
//! the protocol, the transport-authenticated peer if there is one, and the
//! ingress interface and link if the bearer has them. The responder rebuilds
//! the transcript from what it independently observed, so a proof minted for
//! one connection does not verify on another. That is what makes a captured
//! hello useless when replayed over a different link.

use personae::delegation::SignedDelegationCertificate;
use personae::{DerivedKeyAttestation, Ed25519PublicKey, Ed25519Signature, IdentityProvider};
use serde::{Deserialize, Serialize};

use crate::chain::RevocationLedger;
use crate::policy::LocalNetworkPolicy;
use crate::types::{
    DenyReason, HandshakeLimits, NetworkId, ProfileRef, RequestedAction, SUPPORTED_WIRE_VERSION,
    SessionDecision, SessionRequest, TrafficClass,
};

/// Derivation salt for the key that signs session transcripts.
const SESSION_SIGNING_SALT: &[u8] = b"mere/network-policy/session-signer/v1";
/// Domain separator for the signed transcript.
const TRANSCRIPT_DOMAIN: &[u8] = b"mere/network-policy/session-transcript/v1";

/// The responder's own view of the connection a hello arrived on.
///
/// Every field here is a local observation, never something the initiator
/// asserted. Both sides construct it independently and the signature only
/// verifies if they agree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionBinding {
    /// The protocol (ALPN) the session was accepted for.
    pub protocol: Vec<u8>,
    /// The transport-authenticated peer, when the transport proved one.
    pub transport_peer: Option<[u8; 32]>,
    /// Opaque local interface identifier, when the bearer tracks one.
    pub interface: Option<u64>,
    /// Link identifier, when the bearer has link identity.
    pub link: Option<[u8; 16]>,
}

impl SessionBinding {
    /// Binding for a transport with no peer authentication or bearer detail.
    pub fn protocol_only(protocol: impl Into<Vec<u8>>) -> Self {
        Self {
            protocol: protocol.into(),
            ..Self::default()
        }
    }

    /// Binding for a transport that authenticated its peer.
    pub fn authenticated(protocol: impl Into<Vec<u8>>, peer: [u8; 32]) -> Self {
        Self {
            protocol: protocol.into(),
            transport_peer: Some(peer),
            ..Self::default()
        }
    }
}

/// The initiator's opening frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHello {
    /// Admission wire version the initiator speaks.
    pub version: u16,
    /// Network the session claims to belong to.
    pub network: NetworkId,
    /// Profile the initiator speaks.
    pub profile: ProfileRef,
    /// Capability being requested.
    pub action: RequestedAction,
    /// Traffic class requested.
    pub class: TrafficClass,
    /// Freshness, chosen by the initiator.
    pub nonce: [u8; 32],
    /// Personae master public key of the claimed subject.
    pub subject: [u8; 32],
    /// Master-signed proof of the derived key that signed the transcript.
    pub session_signer: DerivedKeyAttestation,
    /// Delegation chain backing the request, root grant first.
    pub delegations: Vec<SignedDelegationCertificate>,
    /// Signature over the canonical transcript.
    pub transcript_signature: Vec<u8>,
}

/// The responder's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionReply {
    /// The session is admitted.
    Accept {
        /// Identifier for the admitted session, derived from the transcript.
        session_id: [u8; 32],
        /// Class the session runs under.
        class: TrafficClass,
        /// Bounds the responder will hold the session to.
        limits: HandshakeLimits,
        /// Profile revision the responder speaks.
        profile_revision: u32,
    },
    /// The session is refused.
    Reject {
        /// Why.
        reason: DenyReason,
    },
}

/// Failure while building or reading a handshake frame.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HandshakeError {
    /// The identity provider could not produce the session signing key.
    #[error("identity provider could not derive the session signer")]
    Identity,
    /// The frame did not encode or decode.
    #[error("handshake frame could not be encoded or decoded")]
    Codec,
    /// The frame exceeded a configured bound.
    #[error("handshake frame exceeds its configured bound")]
    TooLarge,
}

/// Canonical transcript bytes. Every field the decision depends on is here,
/// length-prefixed so no two different field sets can collide.
fn transcript(hello: &SessionHello, binding: &SessionBinding) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_bytes(&mut bytes, TRANSCRIPT_DOMAIN);
    bytes.extend_from_slice(&hello.version.to_le_bytes());
    bytes.extend_from_slice(&hello.network.0);
    push_bytes(&mut bytes, hello.profile.id.as_bytes());
    bytes.extend_from_slice(&hello.profile.revision.to_le_bytes());
    push_bytes(&mut bytes, &binding.protocol);
    push_bytes(&mut bytes, hello.action.domain.as_bytes());
    push_bytes(&mut bytes, hello.action.path.as_bytes());
    push_bytes(&mut bytes, hello.action.action.as_bytes());
    bytes.push(class_tag(hello.class));
    bytes.extend_from_slice(&hello.nonce);
    bytes.extend_from_slice(&hello.subject);
    push_option(&mut bytes, binding.transport_peer.as_ref().map(|p| &p[..]));
    push_option(
        &mut bytes,
        binding
            .interface
            .map(|i| i.to_le_bytes())
            .as_ref()
            .map(|b| &b[..]),
    );
    push_option(&mut bytes, binding.link.as_ref().map(|l| &l[..]));
    bytes.extend_from_slice(&(hello.delegations.len() as u64).to_le_bytes());
    for signed in &hello.delegations {
        bytes.extend_from_slice(&signed.certificate.id().0);
    }
    bytes
}

fn class_tag(class: TrafficClass) -> u8 {
    match class {
        TrafficClass::Control => 0,
        TrafficClass::Interactive => 1,
        TrafficClass::Background => 2,
        TrafficClass::Transit => 3,
    }
}

fn push_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value);
}

fn push_option(bytes: &mut Vec<u8>, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            bytes.push(1);
            push_bytes(bytes, value);
        }
        None => bytes.push(0),
    }
}

impl SessionHello {
    /// Build and sign a hello for one connection.
    ///
    /// `binding` must describe the connection this hello will actually travel
    /// on; the responder rebuilds it independently and the proof fails if the
    /// two disagree.
    #[allow(clippy::too_many_arguments)]
    pub fn issue<P: IdentityProvider>(
        provider: &P,
        network: NetworkId,
        profile: ProfileRef,
        action: RequestedAction,
        class: TrafficClass,
        nonce: [u8; 32],
        binding: &SessionBinding,
        delegations: Vec<SignedDelegationCertificate>,
    ) -> Result<Self, HandshakeError> {
        let session_signer = provider
            .attest_derived_key(SESSION_SIGNING_SALT)
            .map_err(|_| HandshakeError::Identity)?;
        let keypair = provider
            .derive_keypair(SESSION_SIGNING_SALT)
            .map_err(|_| HandshakeError::Identity)?;
        let mut hello = Self {
            version: SUPPORTED_WIRE_VERSION,
            network,
            profile,
            action,
            class,
            nonce,
            subject: provider.master_public_key().to_bytes(),
            session_signer,
            delegations,
            transcript_signature: Vec::new(),
        };
        hello.transcript_signature = keypair
            .sign(&transcript(&hello, binding))
            .to_bytes()
            .to_vec();
        Ok(hello)
    }

    /// Encode to a bounded frame.
    pub fn encode(&self, limits: &HandshakeLimits) -> Result<Vec<u8>, HandshakeError> {
        let limits = limits.clamped();
        if self.delegations.len() > usize::from(limits.max_certificates) {
            return Err(HandshakeError::TooLarge);
        }
        let bytes = postcard::to_allocvec(self).map_err(|_| HandshakeError::Codec)?;
        if bytes.len() > limits.max_hello_bytes as usize {
            return Err(HandshakeError::TooLarge);
        }
        Ok(bytes)
    }

    /// Decode a frame, refusing anything past the configured bounds.
    ///
    /// The size check happens before parsing, so an oversized frame is never
    /// deserialized, and the certificate bounds are checked before any
    /// signature work is attempted.
    pub fn decode(bytes: &[u8], limits: &HandshakeLimits) -> Result<Self, HandshakeError> {
        let limits = limits.clamped();
        if bytes.len() > limits.max_hello_bytes as usize {
            return Err(HandshakeError::TooLarge);
        }
        let hello: Self = postcard::from_bytes(bytes).map_err(|_| HandshakeError::Codec)?;
        if hello.delegations.len() > usize::from(limits.max_certificates) {
            return Err(HandshakeError::TooLarge);
        }
        for signed in &hello.delegations {
            let encoded = postcard::to_allocvec(signed).map_err(|_| HandshakeError::Codec)?;
            if encoded.len() > limits.max_certificate_bytes as usize {
                return Err(HandshakeError::TooLarge);
            }
        }
        Ok(hello)
    }

    /// Verify the session proof against the responder's own view.
    ///
    /// Checks the derived-key attestation, that the attested master is the
    /// claimed subject, and the transcript signature over `binding`.
    pub fn verify_proof(&self, binding: &SessionBinding) -> bool {
        if !self.session_signer.verify(SESSION_SIGNING_SALT) {
            return false;
        }
        let Ok(master) = self.session_signer.master_public_key() else {
            return false;
        };
        if master.to_bytes() != self.subject {
            return false;
        }
        let Ok(derived) = self.session_signer.derived_public_key() else {
            return false;
        };
        verify_signature(
            derived,
            &self.transcript_signature,
            &transcript(self, binding),
        )
    }

    /// The policy request this hello represents, as seen from `binding`.
    ///
    /// The transport peer comes from the responder's observation, never from
    /// the frame, which is what keeps [`SessionRequest::transport_peer`] a
    /// transport fact (plan D4).
    pub fn to_request(&self, binding: &SessionBinding) -> SessionRequest {
        SessionRequest {
            wire_version: self.version,
            network: self.network,
            profile: self.profile.clone(),
            action: self.action.clone(),
            class: self.class,
            subject: self.subject,
            transport_peer: binding.transport_peer,
            delegations: self.delegations.clone(),
        }
    }

    /// Identifier for the admitted session: a digest of the bound transcript.
    pub fn session_id(&self, binding: &SessionBinding) -> [u8; 32] {
        *blake3::hash(&transcript(self, binding)).as_bytes()
    }
}

fn verify_signature(public_key: Ed25519PublicKey, signature: &[u8], message: &[u8]) -> bool {
    let Ok(signature) = <[u8; 64]>::try_from(signature) else {
        return false;
    };
    public_key.verify(message, &Ed25519Signature::from_bytes(&signature))
}

/// Run the responder half: decode, verify, evaluate, and answer.
///
/// Returns the encoded reply and the decision behind it. A refusal is still a
/// well-formed reply: the caller writes it, then closes, and the application
/// above never sees the stream.
pub fn respond(
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    hello_bytes: &[u8],
    binding: &SessionBinding,
    now_ms: u64,
    active_sessions: u32,
) -> (Vec<u8>, SessionDecision) {
    let limits = policy.limits.clamped();
    let decision = match SessionHello::decode(hello_bytes, &limits) {
        Err(_) => SessionDecision::Deny {
            reason: DenyReason::MalformedHello,
        },
        Ok(hello) => {
            if !hello.verify_proof(binding) {
                SessionDecision::Deny {
                    reason: DenyReason::SessionProofInvalid,
                }
            } else {
                let request = hello.to_request(binding);
                match policy.evaluate(ledger, &request, now_ms, active_sessions) {
                    SessionDecision::Accept { class } => {
                        return (
                            encode_reply(&SessionReply::Accept {
                                session_id: hello.session_id(binding),
                                class,
                                limits,
                                profile_revision: policy
                                    .accepted_profiles
                                    .iter()
                                    .find(|accepted| accepted.id == hello.profile.id)
                                    .map(|accepted| accepted.revision)
                                    .unwrap_or(hello.profile.revision),
                            }),
                            SessionDecision::Accept { class },
                        );
                    }
                    denial => denial,
                }
            }
        }
    };
    let reason = match &decision {
        SessionDecision::Deny { reason } => reason.clone(),
        SessionDecision::Accept { .. } => unreachable!("accept returns above"),
    };
    (encode_reply(&SessionReply::Reject { reason }), decision)
}

/// Encode a reply, falling back to a minimal refusal if encoding fails so a
/// responder always has something to write.
fn encode_reply(reply: &SessionReply) -> Vec<u8> {
    postcard::to_allocvec(reply).unwrap_or_default()
}

impl SessionReply {
    /// Decode a reply frame, refusing anything past the configured bound.
    pub fn decode(bytes: &[u8], limits: &HandshakeLimits) -> Result<Self, HandshakeError> {
        if bytes.len() > limits.clamped().max_reply_bytes as usize {
            return Err(HandshakeError::TooLarge);
        }
        postcard::from_bytes(bytes).map_err(|_| HandshakeError::Codec)
    }

    /// Whether this reply admits the session.
    pub fn is_accept(&self) -> bool {
        matches!(self, SessionReply::Accept { .. })
    }
}
