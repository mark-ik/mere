//! Versioned, serializable vocabulary shared by policy owners and evaluators.

use personae::delegation::SignedDelegationCertificate;
use serde::{Deserialize, Serialize};

/// The session-admission wire version this evaluator understands.
pub const SUPPORTED_WIRE_VERSION: u16 = 1;

/// Opaque identity of one network (a community, mesh, or household).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NetworkId(pub [u8; 32]);

/// Reference to a shared vocabulary profile.
///
/// A profile names conventions (actions, classes, offer fields) that several
/// nodes agree to speak. Referencing one never makes it locally
/// authoritative: the owner's [`crate::LocalNetworkPolicy`] decides which
/// profiles are accepted, and `revision` in an accepted entry is the minimum
/// revision the owner will speak.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileRef {
    /// Stable profile identifier, e.g. `mere.base`.
    pub id: String,
    /// Revision of the profile the referrer speaks.
    pub revision: u32,
}

/// Traffic class a session asks to run under.
///
/// Mirrors the transit scheduler's outbound classes (plan V8) so admission
/// and scheduling share one vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrafficClass {
    /// Protocol-internal control traffic, bounded and rate-limited.
    Control,
    /// Foreground request/response traffic.
    Interactive,
    /// Bulk and deferrable traffic.
    Background,
    /// Forwarded traffic. Never a session class: transit is an interface
    /// policy (plan D7), and requesting it here is refused.
    Transit,
}

/// One requested capability, in personae scope terms.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestedAction {
    /// Application domain owning the action vocabulary, e.g. `mere.network`.
    pub domain: String,
    /// Structural path of the service, e.g. `/services/murm`.
    pub path: String,
    /// The action itself, e.g. `connect`.
    pub action: String,
}

/// Everything the evaluator may consider for one incoming session.
///
/// `transport_peer` is a transport fact, not a claim (plan D4): callers fill
/// it only when the transport authenticated the peer. A subject named by
/// application bytes never goes there; Reticulum best-effort acceptance
/// passes `None` and proves its subject through the session handshake
/// instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRequest {
    /// Admission wire version the initiator speaks.
    pub wire_version: u16,
    /// Network this session claims to belong to.
    pub network: NetworkId,
    /// Profile the initiator speaks.
    pub profile: ProfileRef,
    /// The capability being requested.
    pub action: RequestedAction,
    /// Traffic class requested for the session.
    pub class: TrafficClass,
    /// Personae master public key of the claimed subject.
    pub subject: [u8; 32],
    /// Transport-authenticated peer key, when the transport proved one.
    pub transport_peer: Option<[u8; 32]>,
    /// Delegation chain backing the request, root grant first, subject last.
    pub delegations: Vec<SignedDelegationCertificate>,
}

/// The evaluator's answer for one session request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionDecision {
    /// Admit the session under the granted class.
    Accept {
        /// Class the session actually runs under.
        class: TrafficClass,
    },
    /// Refuse the session.
    Deny {
        /// Why the session was refused.
        reason: DenyReason,
    },
}

impl SessionDecision {
    /// Whether this decision admits the session.
    pub fn is_accept(&self) -> bool {
        matches!(self, SessionDecision::Accept { .. })
    }
}

/// Why a session was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum DenyReason {
    /// The initiator speaks an admission wire version this node does not.
    #[error("unsupported wire version {requested} (supported {supported})")]
    UnsupportedWireVersion {
        /// Version the initiator asked for.
        requested: u16,
        /// Version this evaluator supports.
        supported: u16,
    },
    /// The request names a network this policy does not govern.
    #[error("unknown network")]
    UnknownNetwork,
    /// No accepted profile matches the request.
    #[error("profile not accepted")]
    ProfileNotAccepted,
    /// Transit is not a session service (plan D7).
    #[error("transit is not admitted as a session")]
    TransitNotASession,
    /// The named service is not offered, or its rule disables it.
    #[error("service not offered")]
    ServiceNotOffered,
    /// The service rule requires a transport-authenticated peer.
    #[error("transport identity required")]
    TransportIdentityRequired,
    /// The transport authenticated a peer, and the claimed subject is not it
    /// (plan D6). Presenting someone else's authority over your own
    /// authenticated connection is exactly what this refuses.
    #[error("claimed subject is not the transport-authenticated peer")]
    SubjectNotTransportPeer,
    /// The hello frame was oversized, truncated, or not decodable.
    #[error("malformed session hello")]
    MalformedHello,
    /// The session proof did not verify against the claimed subject and the
    /// responder's own view of the connection.
    #[error("session proof invalid")]
    SessionProofInvalid,
    /// The delegation chain failed validation.
    #[error("delegation rejected: {0}")]
    Delegation(#[source] ChainFault),
    /// The chain is valid but does not cover the requested action now.
    #[error("action not covered by the presented authority")]
    ActionNotCovered,
    /// The session was admitted, but for a different action than this listener
    /// serves. Distinct from [`DenyReason::ServiceNotOffered`], which is the
    /// owner declining to offer a service at all, and from
    /// [`DenyReason::ActionNotCovered`], which is the authority falling short:
    /// this is a listener refusing to serve one grant under another service.
    #[error("this listener does not serve the admitted action")]
    ActionNotOffered,
    /// The service is at its configured session capacity.
    #[error("service capacity exhausted")]
    CapacityExhausted,
}

/// How a delegation chain failed validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ChainFault {
    /// The request carried no certificates where authority was required.
    #[error("no delegation presented")]
    Empty,
    /// The chain exceeds the configured depth or certificate budget.
    #[error("delegation depth exceeded")]
    DepthExceeded,
    /// A certificate's signature or signer attestation failed.
    #[error("certificate signature invalid")]
    BadSignature,
    /// The chain does not terminate at a locally accepted root.
    #[error("no locally accepted root")]
    UntrustedRoot,
    /// A link's parent reference or issuer does not match its predecessor.
    #[error("broken delegation link")]
    BrokenLink,
    /// A child certificate fails to narrow its parent.
    #[error("child does not attenuate its parent")]
    NotAttenuated,
    /// A certificate in the chain is revoked by its issuer.
    #[error("certificate revoked")]
    Revoked,
    /// A certificate is not yet valid at the evaluation time.
    #[error("certificate not yet valid")]
    NotYetValid,
    /// A certificate is expired at the evaluation time.
    #[error("certificate expired")]
    Expired,
    /// The leaf certificate names a different subject than the session.
    #[error("leaf subject does not match the session subject")]
    SubjectMismatch,
}

/// Compile-time safety ceilings for [`HandshakeLimits`].
///
/// Owner configuration may tighten below these but never exceed them; the
/// ceilings bound what a hostile hello can make this node parse or verify.
pub mod limit_ceilings {
    /// Largest encoded hello frame.
    pub const HELLO_BYTES: u32 = 8_192;
    /// Largest encoded reply frame.
    pub const REPLY_BYTES: u32 = 2_048;
    /// Most certificates one hello may carry.
    pub const CERTIFICATES: u16 = 8;
    /// Largest single encoded certificate.
    pub const CERTIFICATE_BYTES: u32 = 4_096;
    /// Deepest delegation chain the evaluator will walk.
    pub const DELEGATION_DEPTH: u16 = 8;
    /// Longest handshake deadline.
    pub const DEADLINE_MS: u32 = 30_000;
}

/// Owner-configurable handshake bounds, always within [`limit_ceilings`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeLimits {
    /// Largest encoded hello this node will read.
    pub max_hello_bytes: u32,
    /// Largest encoded reply this node will send.
    pub max_reply_bytes: u32,
    /// Most certificates accepted in one hello.
    pub max_certificates: u16,
    /// Largest single encoded certificate accepted.
    pub max_certificate_bytes: u32,
    /// Deepest delegation chain accepted.
    pub max_delegation_depth: u16,
    /// Handshake deadline in milliseconds.
    pub deadline_ms: u32,
}

impl HandshakeLimits {
    /// Clamp every field to its compile-time ceiling.
    pub fn clamped(self) -> Self {
        Self {
            max_hello_bytes: self.max_hello_bytes.min(limit_ceilings::HELLO_BYTES),
            max_reply_bytes: self.max_reply_bytes.min(limit_ceilings::REPLY_BYTES),
            max_certificates: self.max_certificates.min(limit_ceilings::CERTIFICATES),
            max_certificate_bytes: self
                .max_certificate_bytes
                .min(limit_ceilings::CERTIFICATE_BYTES),
            max_delegation_depth: self
                .max_delegation_depth
                .min(limit_ceilings::DELEGATION_DEPTH),
            deadline_ms: self.deadline_ms.min(limit_ceilings::DEADLINE_MS),
        }
    }
}

impl Default for HandshakeLimits {
    fn default() -> Self {
        Self {
            max_hello_bytes: 2_048,
            max_reply_bytes: 512,
            max_certificates: 4,
            max_certificate_bytes: 2_048,
            max_delegation_depth: 4,
            deadline_ms: 10_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_clamp_to_the_ceilings() {
        let greedy = HandshakeLimits {
            max_hello_bytes: u32::MAX,
            max_reply_bytes: u32::MAX,
            max_certificates: u16::MAX,
            max_certificate_bytes: u32::MAX,
            max_delegation_depth: u16::MAX,
            deadline_ms: u32::MAX,
        }
        .clamped();
        assert_eq!(greedy.max_hello_bytes, limit_ceilings::HELLO_BYTES);
        assert_eq!(greedy.max_reply_bytes, limit_ceilings::REPLY_BYTES);
        assert_eq!(greedy.max_certificates, limit_ceilings::CERTIFICATES);
        assert_eq!(
            greedy.max_certificate_bytes,
            limit_ceilings::CERTIFICATE_BYTES
        );
        assert_eq!(
            greedy.max_delegation_depth,
            limit_ceilings::DELEGATION_DEPTH
        );
        assert_eq!(greedy.deadline_ms, limit_ceilings::DEADLINE_MS);
    }

    #[test]
    fn defaults_sit_inside_the_ceilings() {
        let defaults = HandshakeLimits::default();
        assert_eq!(defaults, defaults.clamped());
    }
}
