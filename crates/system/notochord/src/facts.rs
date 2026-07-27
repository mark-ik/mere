//! Carrier-observed facts, and the subset of them a proof may bind.
//!
//! The split this module exists for (Notochord plan, N0): what the *carrier
//! observed* and what the *initiator claimed* are different kinds of thing,
//! and mixing them into one struct makes the D4 rule ("transport facts are
//! facts, not claims") a discipline rather than a property of the types.
//!
//! - [`SessionFacts`] is built by the accepting carrier adapter and is
//!   deliberately **not** serializable: nothing decoded from application bytes
//!   can construct one, so a hello cannot smuggle in an authenticated peer or
//!   a local interface.
//! - [`crate::SessionClaims`] is what a hello asserts, and is worth exactly as
//!   much as the proof over it.
//! - [`ProofBinding`] is the intersection: the facts *both* peers can derive
//!   independently, and therefore the only ones a signature can cover.

/// Which carrier delivered a session.
///
/// Mirrors `mere-transport::TransportKind`. It is redeclared here rather than
/// imported because this crate must not depend on the transport stack (and
/// therefore on iroh, p2panda, and QUIC); the carrier adapter maps between
/// them. Named `CarrierKind` so the two can be imported side by side in that
/// adapter without a rename.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CarrierKind {
    /// In-process paired fixture.
    Memory,
    /// p2panda / iroh.
    P2panda,
    /// Reticulum, over any retinue interface.
    Reticulum,
    /// A carrier this crate has no specific knowledge of.
    Other,
}

/// Where a session physically arrived.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IngressFacts {
    /// The local interface it arrived on, when the carrier tracks one.
    ///
    /// Opaque and local: meaningful only to the node that assigned it, not
    /// stable across restarts, and never on the wire. It may inform the
    /// owner's policy; it can never enter a transcript, because the far end
    /// cannot reconstruct it. See [`ProofBinding`].
    pub local_interface: Option<u64>,
    /// The link identifier, when the carrier has link identity.
    ///
    /// Shared, unlike the interface: both ends of a retinue link compute the
    /// same value, which is what lets a proof be pinned to one link.
    pub shared_link: Option<[u8; 16]>,
}

/// What the carrier observed about one inbound session.
///
/// Constructed by the accepting adapter, never decoded. `authenticated_initiator`
/// is `Some` only when the carrier itself proved the initiator: p2panda
/// authenticates its connections, Memory knows its counterparty by
/// construction, and Reticulum best-effort acceptance honestly reports `None`
/// and lets the session proof establish a subject instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFacts {
    /// The protocol (ALPN) the session was accepted for.
    pub protocol: Vec<u8>,
    /// The carrier that delivered it.
    pub transport: CarrierKind,
    /// The initiator's identity, when the carrier authenticated it.
    pub authenticated_initiator: Option<[u8; 32]>,
    /// Where it arrived.
    pub ingress: IngressFacts,
}

impl SessionFacts {
    /// Facts for a carrier with no peer authentication and no bearer detail.
    pub fn new(protocol: impl Into<Vec<u8>>, transport: CarrierKind) -> Self {
        Self {
            protocol: protocol.into(),
            transport,
            authenticated_initiator: None,
            ingress: IngressFacts::default(),
        }
    }

    /// Facts for a carrier that authenticated the initiator.
    pub fn authenticated(
        protocol: impl Into<Vec<u8>>,
        transport: CarrierKind,
        initiator: [u8; 32],
    ) -> Self {
        Self {
            authenticated_initiator: Some(initiator),
            ..Self::new(protocol, transport)
        }
    }

    /// Add the bearer detail a link-oriented carrier observed.
    pub fn with_ingress(
        mut self,
        local_interface: Option<u64>,
        shared_link: Option<[u8; 16]>,
    ) -> Self {
        self.ingress = IngressFacts {
            local_interface,
            shared_link,
        };
        self
    }

    /// The binding a *responder* signs against: only what the far end can also
    /// derive. The local interface is dropped here, deliberately.
    pub fn proof_binding(&self) -> ProofBinding {
        ProofBinding {
            protocol: self.protocol.clone(),
            initiator_transport_identity: self.authenticated_initiator,
            shared_link: self.ingress.shared_link,
        }
    }
}

/// The facts a session proof covers.
///
/// Every field must be independently derivable by both peers, or an honest
/// session cannot be admitted. Note what
/// `initiator_transport_identity` means: it is **the initiator's** transport
/// identity in both roles. The responder fills it from the peer the carrier
/// authenticated; the initiator fills it from its own local transport
/// identity. It must never be read as "the other end" on both sides, or the
/// two would sign different bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProofBinding {
    /// The protocol the session was accepted for.
    pub protocol: Vec<u8>,
    /// The initiator's carrier-proved identity, when there is one.
    pub initiator_transport_identity: Option<[u8; 32]>,
    /// The shared link identifier, when the carrier has link identity.
    pub shared_link: Option<[u8; 16]>,
}

impl ProofBinding {
    /// The binding an *initiator* signs, from its own local transport
    /// identity and the link it is about to speak on.
    ///
    /// `local_transport_identity` is this node's own identity on an
    /// authenticating carrier, and `None` on one that cannot prove peers.
    pub fn initiator(
        protocol: impl Into<Vec<u8>>,
        local_transport_identity: Option<[u8; 32]>,
        shared_link: Option<[u8; 16]>,
    ) -> Self {
        Self {
            protocol: protocol.into(),
            initiator_transport_identity: local_transport_identity,
            shared_link,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_roles_derive_the_same_binding_on_an_authenticated_carrier() {
        let initiator_key = [7u8; 32];
        let link = [3u8; 16];

        // Responder: reads the initiator identity off its own acceptance.
        let responder =
            SessionFacts::authenticated(b"p".to_vec(), CarrierKind::P2panda, initiator_key)
                .with_ingress(Some(41), Some(link))
                .proof_binding();
        // Initiator: uses its own transport identity.
        let initiator = ProofBinding::initiator(b"p".to_vec(), Some(initiator_key), Some(link));

        assert_eq!(responder, initiator);
    }

    #[test]
    fn the_local_interface_never_reaches_the_binding() {
        let facts = SessionFacts::new(b"p".to_vec(), CarrierKind::Reticulum)
            .with_ingress(Some(9), Some([1; 16]));
        let other_interface = SessionFacts::new(b"p".to_vec(), CarrierKind::Reticulum)
            .with_ingress(Some(1_000), Some([1; 16]));

        assert_ne!(facts.ingress, other_interface.ingress);
        assert_eq!(
            facts.proof_binding(),
            other_interface.proof_binding(),
            "a purely local number cannot change what gets signed"
        );
    }
}
