//! Owner-editable policy state.
//!
//! Admission conclusions are live connection state. This document contains
//! only what the owner chose: networks, accepted profiles and roots, service
//! rules, handshake bounds, discovery, transit, and verified revocations.
//! Hosts may persist it; [`crate::SessionFacts`],
//! [`crate::AdmittedPrincipal`], and [`crate::AdmittedSession`] cannot enter
//! it by construction.

use personae::delegation::SignedDelegationRevocation;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
    DiscoveryPolicy, HandshakeLimits, LocalNetworkPolicy, NetworkId, POLICY_VERSION, ProfileRef,
    RevocationLedger, ServiceRule, TransitPolicy, TrustedRoot,
};

/// Serialized owner-policy document version.
pub const OWNER_POLICY_VERSION: u16 = 2;

/// Every network policy owned by one persona.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OwnerPolicySet {
    /// Serialized document version.
    pub version: u16,
    networks: Vec<OwnerNetworkPolicy>,
}

#[derive(Deserialize)]
struct OwnerPolicyDocument {
    version: u16,
    networks: Vec<OwnerNetworkPolicy>,
}

impl<'de> Deserialize<'de> for OwnerPolicySet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let document = OwnerPolicyDocument::deserialize(deserializer)?;
        let policies = Self {
            version: document.version,
            networks: document.networks,
        };
        policies.validate().map_err(D::Error::custom)?;
        Ok(policies)
    }
}

impl Default for OwnerPolicySet {
    fn default() -> Self {
        Self {
            version: OWNER_POLICY_VERSION,
            networks: Vec::new(),
        }
    }
}

impl OwnerPolicySet {
    /// An empty, current-version owner document.
    pub fn new() -> Self {
        Self::default()
    }

    /// All configured networks, in deterministic id order.
    pub fn networks(&self) -> &[OwnerNetworkPolicy] {
        &self.networks
    }

    /// Validate serialized versions and the deterministic network ordering.
    pub fn validate(&self) -> Result<(), OwnerPolicyValidationError> {
        if self.version != OWNER_POLICY_VERSION {
            return Err(OwnerPolicyValidationError::DocumentVersion(self.version));
        }
        if let Some(network) = self
            .networks
            .iter()
            .find(|entry| entry.policy.version != POLICY_VERSION)
        {
            return Err(OwnerPolicyValidationError::NetworkVersion(
                network.policy.version,
            ));
        }
        if self
            .networks
            .windows(2)
            .any(|pair| pair[0].policy.network >= pair[1].policy.network)
        {
            return Err(OwnerPolicyValidationError::NetworkOrder);
        }
        Ok(())
    }

    /// The settings for one network.
    pub fn network(&self, id: NetworkId) -> Option<&OwnerNetworkPolicy> {
        self.networks
            .binary_search_by_key(&id, |entry| entry.policy.network)
            .ok()
            .map(|index| &self.networks[index])
    }

    /// Mutable settings for one network.
    pub fn network_mut(&mut self, id: NetworkId) -> Option<&mut OwnerNetworkPolicy> {
        self.networks
            .binary_search_by_key(&id, |entry| entry.policy.network)
            .ok()
            .map(|index| &mut self.networks[index])
    }

    /// Insert or replace one network's owner settings.
    pub fn upsert(&mut self, settings: OwnerNetworkPolicy) {
        let id = settings.policy.network;
        match self
            .networks
            .binary_search_by_key(&id, |entry| entry.policy.network)
        {
            Ok(index) => self.networks[index] = settings,
            Err(index) => self.networks.insert(index, settings),
        }
    }

    /// Remove one network from the owner's accepted set.
    pub fn remove(&mut self, id: NetworkId) -> Option<OwnerNetworkPolicy> {
        self.networks
            .binary_search_by_key(&id, |entry| entry.policy.network)
            .ok()
            .map(|index| self.networks.remove(index))
    }
}

/// A persisted owner-policy document cannot be interpreted safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OwnerPolicyValidationError {
    /// The outer document uses an unsupported version.
    #[error("unsupported owner-policy document version {0}")]
    DocumentVersion(u16),
    /// A contained network policy uses an unsupported version.
    #[error("unsupported local-network policy version {0}")]
    NetworkVersion(u16),
    /// Network ids are duplicated or not in deterministic order.
    #[error("owner-policy networks are duplicated or out of order")]
    NetworkOrder,
}

/// Persistable owner settings and revocations for one network.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OwnerNetworkPolicy {
    /// The admission, discovery, transit, and resource bounds chosen by the
    /// owner.
    pub policy: LocalNetworkPolicy,
    /// Verified revocations folded for this network.
    pub revocations: RevocationLedger,
}

impl OwnerNetworkPolicy {
    /// Start with a closed network. Every axis opens through an explicit edit.
    pub fn closed(network: NetworkId) -> Self {
        Self {
            policy: LocalNetworkPolicy::closed(network),
            revocations: RevocationLedger::new(),
        }
    }

    /// Apply one independent owner setting.
    pub fn apply(&mut self, edit: OwnerPolicyEdit) {
        match edit {
            OwnerPolicyEdit::AcceptedProfiles(profiles) => {
                self.policy.accepted_profiles = profiles;
            }
            OwnerPolicyEdit::TrustedRoots(roots) => {
                self.policy.trusted_roots = roots;
            }
            OwnerPolicyEdit::Service { path, rule } => {
                self.policy.services.insert(path, rule);
            }
            OwnerPolicyEdit::RemoveService { path } => {
                self.policy.services.remove(&path);
            }
            OwnerPolicyEdit::HandshakeLimits(limits) => {
                self.policy.limits = limits.clamped();
            }
            OwnerPolicyEdit::Discovery(announce) => {
                self.policy.discovery = DiscoveryPolicy { announce };
            }
            OwnerPolicyEdit::Transit(enabled) => {
                self.policy.transit = TransitPolicy { enabled };
            }
        }
    }

    /// Verify and retain one revocation statement.
    pub fn fold_revocation(&mut self, statement: &SignedDelegationRevocation) -> bool {
        self.revocations.fold(statement)
    }
}

/// One host-originated edit to the owner document.
#[derive(Clone, Debug, PartialEq)]
pub enum OwnerPolicyEdit {
    /// Replace the accepted profile revisions for this network.
    AcceptedProfiles(Vec<ProfileRef>),
    /// Replace the trusted Personae roots for this network.
    TrustedRoots(Vec<TrustedRoot>),
    /// Insert or replace one service's independent rule.
    Service {
        /// Structural service path.
        path: String,
        /// Owner-selected access, transport requirement, and capacity.
        rule: ServiceRule,
    },
    /// Stop offering one service.
    RemoveService {
        /// Structural service path.
        path: String,
    },
    /// Replace the bounded handshake settings.
    HandshakeLimits(HandshakeLimits),
    /// Change discovery without changing service or transit policy.
    Discovery(bool),
    /// Change transit without changing discovery or service policy.
    Transit(bool),
}

#[cfg(test)]
mod tests {
    use personae::delegation::{
        CapabilityScope, DelegationId, DelegationRevocation, SignedDelegationRevocation,
    };
    use personae::{IdentityProvider, InMemoryProvider};

    use super::*;
    use crate::ServiceAccess;

    const NETWORK: NetworkId = NetworkId([7; 32]);
    const MURM: &str = "/services/murm";
    const GRAPHSHELL: &str = "/services/projection";

    fn rule(access: ServiceAccess, max_sessions: Option<u32>) -> ServiceRule {
        ServiceRule::new(access, "mere.network", ["connect"], false, max_sessions)
    }

    #[test]
    fn service_and_transit_edits_remain_independent() {
        let mut settings = OwnerNetworkPolicy::closed(NETWORK);
        settings.apply(OwnerPolicyEdit::Service {
            path: MURM.to_string(),
            rule: rule(ServiceAccess::Public, Some(4)),
        });
        settings.apply(OwnerPolicyEdit::Service {
            path: GRAPHSHELL.to_string(),
            rule: rule(ServiceAccess::MemberOnly, Some(2)),
        });
        settings.apply(OwnerPolicyEdit::Transit(true));

        let graphshell_before = settings.policy.services[GRAPHSHELL].clone();
        settings.apply(OwnerPolicyEdit::Service {
            path: MURM.to_string(),
            rule: rule(ServiceAccess::Disabled, Some(1)),
        });
        assert_eq!(settings.policy.services[GRAPHSHELL], graphshell_before);
        assert!(settings.policy.permits_transit());

        let services_before = settings.policy.services.clone();
        settings.apply(OwnerPolicyEdit::Transit(false));
        assert_eq!(settings.policy.services, services_before);
        assert!(!settings.policy.permits_transit());
    }

    #[test]
    fn handshake_limits_are_clamped_at_the_owner_boundary() {
        let mut settings = OwnerNetworkPolicy::closed(NETWORK);
        settings.apply(OwnerPolicyEdit::HandshakeLimits(HandshakeLimits {
            max_hello_bytes: u32::MAX,
            max_reply_bytes: u32::MAX,
            max_certificates: u16::MAX,
            max_certificate_bytes: u32::MAX,
            max_delegation_depth: u16::MAX,
            deadline_ms: u32::MAX,
        }));
        assert_eq!(
            settings.policy.limits,
            HandshakeLimits {
                max_hello_bytes: crate::limit_ceilings::HELLO_BYTES,
                max_reply_bytes: crate::limit_ceilings::REPLY_BYTES,
                max_certificates: crate::limit_ceilings::CERTIFICATES,
                max_certificate_bytes: crate::limit_ceilings::CERTIFICATE_BYTES,
                max_delegation_depth: crate::limit_ceilings::DELEGATION_DEPTH,
                deadline_ms: crate::limit_ceilings::DEADLINE_MS,
            }
        );
    }

    #[test]
    fn the_owner_document_contains_no_live_session_state() {
        let settings = OwnerPolicySet::new();
        let debug = format!("{settings:?}");
        assert!(!debug.contains("SessionFacts"));
        assert!(!debug.contains("AdmittedPrincipal"));
    }

    #[test]
    fn verified_revocations_round_trip_with_the_owner_document() {
        let issuer = InMemoryProvider::from_seed([3; 32]);
        let statement = SignedDelegationRevocation::issue(
            &issuer,
            DelegationRevocation::new(
                DelegationId([8; 32]),
                issuer.master_public_key().to_bytes(),
                CapabilityScope {
                    domain: "mere.network".to_string(),
                    resource: NETWORK.0.to_vec(),
                    path_prefix: MURM.to_string(),
                    actions: ["connect".to_string()].into_iter().collect(),
                },
                50,
                [5; 32],
            ),
        )
        .expect("revocation signs");
        let mut network = OwnerNetworkPolicy::closed(NETWORK);
        assert!(network.fold_revocation(&statement));
        let mut settings = OwnerPolicySet::new();
        settings.upsert(network);

        let bytes = postcard::to_allocvec(&settings).expect("owner policy encodes");
        let restored: OwnerPolicySet = postcard::from_bytes(&bytes).expect("owner policy decodes");
        assert_eq!(restored, settings);
        assert_eq!(restored.network(NETWORK).unwrap().revocations.len(), 1);
    }

    #[test]
    fn unsupported_or_nondeterministic_documents_are_rejected() {
        let mut future = OwnerPolicySet::new();
        future.version += 1;
        let bytes = postcard::to_allocvec(&future).expect("future policy encodes");
        assert!(postcard::from_bytes::<OwnerPolicySet>(&bytes).is_err());

        let mut unordered = OwnerPolicySet::new();
        unordered
            .networks
            .push(OwnerNetworkPolicy::closed(NetworkId([2; 32])));
        unordered
            .networks
            .push(OwnerNetworkPolicy::closed(NetworkId([1; 32])));
        let bytes = postcard::to_allocvec(&unordered).expect("unordered policy encodes");
        assert!(postcard::from_bytes::<OwnerPolicySet>(&bytes).is_err());
    }
}
