//! Owner-controlled session admission for Mere services.
//!
//! This crate evaluates one question: may this incoming session use this
//! service, right now, under the owner's local rules? It supplies the
//! versioned policy and request vocabulary, a personae delegation-chain
//! evaluator, and a local revocation ledger. It creates no membership token
//! of its own: personae remains the authority grammar, and Gemot remains
//! responsible for Moot membership (plan decision D5).
//!
//! Deliberately narrow (the low-power managed-network plan, V5): the first
//! supported action is `mere.network` / `/services/murm` / `connect`, and the
//! policy axes stay independent (D3). A node may publish a service without
//! carrying transit, carry transit without exposing services, and so on;
//! there is no global public/private mode. Transit admission is not a session
//! decision at all: anonymous Reticulum transit is enforced per interface and
//! budget (D7), so [`LocalNetworkPolicy::permits_transit`] is a separate axis
//! the endpoint consults, never a branch inside session evaluation.
//!
//! Transport facts are facts, not claims (D4), and since Notochord N0 the
//! types enforce it rather than asking callers to be careful:
//! [`SessionFacts`] is what the carrier observed and cannot be decoded from a
//! frame, [`SessionClaims`] is what a hello asserts and is worth only what its
//! proof is worth, and [`ProofBinding`] is the intersection both peers can
//! derive independently and therefore the only thing a signature can cover.

mod chain;
mod facts;
mod handshake;
#[cfg(feature = "tokio")]
mod io;
mod policy;
mod types;

pub use chain::{RevocationLedger, TrustedRoot, validate_chain};
pub use facts::{CarrierKind, IngressFacts, ProofBinding, SessionFacts};
pub use handshake::{
    AdmittedPrincipal, AdmittedSession, HandshakeError, SessionHello, SessionReply, admit, respond,
};
#[cfg(feature = "tokio")]
pub use io::{IoHandshakeError, accept_session, admit_session, initiate_session};
pub use policy::{
    DiscoveryPolicy, LocalNetworkPolicy, POLICY_VERSION, ServiceAccess, ServiceRule, TransitPolicy,
};
pub use types::{
    ChainFault, DenyReason, HandshakeLimits, NetworkId, ProfileRef, RequestedAction,
    SUPPORTED_WIRE_VERSION, SessionClaims, SessionDecision, TrafficClass, limit_ceilings,
};
