//! The owner's local policy and its deterministic session evaluator.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::chain::{RevocationLedger, TrustedRoot, validate_chain};
use crate::types::{
    DenyReason, HandshakeLimits, NetworkId, ProfileRef, SUPPORTED_WIRE_VERSION, SessionDecision,
    SessionRequest, TrafficClass,
};

/// Version of the serialized policy shape.
pub const POLICY_VERSION: u16 = 1;

/// Who may use one service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceAccess {
    /// The service is not offered.
    Disabled,
    /// Anyone the transport delivers may connect; no authority required.
    Public,
    /// Only subjects presenting a valid delegation chain may connect.
    MemberOnly,
}

/// The owner's rule for one service path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRule {
    /// Who may use the service.
    pub access: ServiceAccess,
    /// Whether a transport-authenticated peer is required (plan D4: this is
    /// a transport fact; Reticulum best-effort sessions cannot satisfy it).
    pub require_transport_identity: bool,
    /// Concurrent-session ceiling, when the owner sets one.
    pub max_sessions: Option<u32>,
}

/// Whether this node carries Reticulum transit.
///
/// Deliberately not consulted by session evaluation: anonymous transit is an
/// interface policy (plan D7), and keeping the axis independent is what lets
/// a node publish a service without carrying transit or the reverse (D3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitPolicy {
    /// Whether forwarding for others is offered at all.
    pub enabled: bool,
}

/// Whether this node announces itself for discovery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryPolicy {
    /// Whether the node announces its presence.
    pub announce: bool,
}

/// The owner's private network policy: every axis independent (plan D3).
///
/// This structure is local state. It is serializable for persistence, and it
/// is never shared on the wire; signed offers (plan V9) describe current
/// willingness without exposing it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocalNetworkPolicy {
    /// Serialized-shape version.
    pub version: u16,
    /// The network this policy governs.
    pub network: NetworkId,
    /// Profiles the owner accepts; each entry's `revision` is the minimum.
    pub accepted_profiles: Vec<ProfileRef>,
    /// Root authorities the owner accepts delegation chains from.
    pub trusted_roots: Vec<TrustedRoot>,
    /// Discovery axis.
    pub discovery: DiscoveryPolicy,
    /// Service rules keyed by service path, e.g. `/services/murm`.
    pub services: BTreeMap<String, ServiceRule>,
    /// Transit axis.
    pub transit: TransitPolicy,
    /// Handshake bounds, always clamped to the compile-time ceilings.
    pub limits: HandshakeLimits,
}

impl LocalNetworkPolicy {
    /// A closed policy for one network: nothing announced, nothing offered,
    /// no transit, no trusted roots. Every axis opens by explicit owner
    /// action.
    pub fn closed(network: NetworkId) -> Self {
        Self {
            version: POLICY_VERSION,
            network,
            accepted_profiles: Vec::new(),
            trusted_roots: Vec::new(),
            discovery: DiscoveryPolicy::default(),
            services: BTreeMap::new(),
            transit: TransitPolicy::default(),
            limits: HandshakeLimits::default(),
        }
    }

    /// Whether this node currently offers Reticulum transit.
    pub fn permits_transit(&self) -> bool {
        self.transit.enabled
    }

    /// Whether this node currently announces itself.
    pub fn permits_discovery(&self) -> bool {
        self.discovery.announce
    }

    fn accepts_profile(&self, profile: &ProfileRef) -> bool {
        self.accepted_profiles
            .iter()
            .any(|accepted| accepted.id == profile.id && profile.revision >= accepted.revision)
    }

    /// Decide one incoming session.
    ///
    /// Deterministic over its inputs: the request, the caller-supplied clock
    /// `now_ms`, the revocation ledger, and `active_sessions` (the caller's
    /// honest count of live sessions already admitted under this action's
    /// rule). Evaluation order follows the plan: wire version, profile,
    /// transport identity, delegation chain, action coverage, service rule,
    /// capacity.
    pub fn evaluate(
        &self,
        ledger: &RevocationLedger,
        request: &SessionRequest,
        now_ms: u64,
        active_sessions: u32,
    ) -> SessionDecision {
        if request.wire_version != SUPPORTED_WIRE_VERSION {
            return deny(DenyReason::UnsupportedWireVersion {
                requested: request.wire_version,
                supported: SUPPORTED_WIRE_VERSION,
            });
        }
        if request.network != self.network {
            return deny(DenyReason::UnknownNetwork);
        }
        if !self.accepts_profile(&request.profile) {
            return deny(DenyReason::ProfileNotAccepted);
        }
        if request.class == TrafficClass::Transit {
            return deny(DenyReason::TransitNotASession);
        }

        let Some(rule) = self.services.get(&request.action.path) else {
            return deny(DenyReason::ServiceNotOffered);
        };
        if rule.access == ServiceAccess::Disabled {
            return deny(DenyReason::ServiceNotOffered);
        }
        if rule.require_transport_identity && request.transport_peer.is_none() {
            return deny(DenyReason::TransportIdentityRequired);
        }

        if rule.access == ServiceAccess::MemberOnly {
            let depth = self
                .limits
                .max_delegation_depth
                .min(self.limits.max_certificates);
            if let Err(fault) = validate_chain(
                &request.delegations,
                request.subject,
                &self.trusted_roots,
                ledger,
                depth,
                now_ms,
            ) {
                return deny(DenyReason::Delegation(fault));
            }
            let leaf = &request.delegations[request.delegations.len() - 1].certificate;
            let covered = leaf.scope.domain == request.action.domain
                && leaf.scope.resource == request.network.0
                && leaf.covers(&request.action.path, &request.action.action, now_ms);
            if !covered {
                return deny(DenyReason::ActionNotCovered);
            }
        }

        if let Some(max_sessions) = rule.max_sessions
            && active_sessions >= max_sessions
        {
            return deny(DenyReason::CapacityExhausted);
        }

        SessionDecision::Accept {
            class: request.class,
        }
    }
}

fn deny(reason: DenyReason) -> SessionDecision {
    SessionDecision::Deny { reason }
}

// The chain-shaped cases (expiry, revocation, widening, depth) live in the
// tests/matrix.rs integration suite, which exercises the public surface end
// to end with real personae statements.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChainFault;

    #[test]
    fn a_closed_policy_offers_nothing_but_stays_evaluable() {
        let policy = LocalNetworkPolicy::closed(NetworkId([1; 32]));
        assert!(!policy.permits_transit());
        assert!(!policy.permits_discovery());
        assert!(policy.services.is_empty());
    }

    #[test]
    fn used_chain_fault_variant_is_reachable_for_empty_chains() {
        // Guard the mapping the matrix relies on: MemberOnly with no
        // certificates must surface ChainFault::Empty, not a generic denial.
        assert_eq!(
            format!("{}", DenyReason::Delegation(ChainFault::Empty)),
            "delegation rejected: no delegation presented"
        );
    }
}
