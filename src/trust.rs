//! Per-endpoint trust state.
//!
//! Resolution and trust are separate layers. A resolver says where to reach
//! someone; it never says the binding is real. So trust is recorded here, per
//! address, rather than as one verdict per person: a peer's misfin mailbox can
//! be pinned while their freshly scraped ActivityPub actor is still unchecked.

use serde::{Deserialize, Serialize};

/// How a binding came to be believed.
///
/// Open-ended on purpose. The variants name the methods mere already has or
/// has charted; anything else rides [`ProofMethod::Other`] rather than forcing
/// a breaking change on this enum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProofMethod {
    /// A signature by the contact's root key.
    Signature,
    /// A DID document that self-authenticates.
    DidAuth,
    /// The back-claim proof: the handle's own well-known document names the key.
    BackClaim,
    /// Confirmed out of band, by a human, through a channel gaz cannot see.
    OutOfBand,
    /// Some other method, named by the caller.
    Other(String),
}

/// What is known about one endpoint's claim to belong to this contact.
///
/// The states are ordered by how much they let you assume, and
/// [`TrustState::is_alarming`] marks the two that should reach a person.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustState {
    /// Recorded, nothing checked. The honest default for anything a resolver
    /// handed over.
    #[default]
    Unverified,
    /// Pinned on first sight and unchanged since: trust on first use, which is
    /// how misfin certificate fingerprints work.
    Pinned {
        /// When the pin was taken, unix milliseconds.
        first_seen_ms: u64,
    },
    /// Checked against the contact's root key.
    Verified {
        /// How it was checked.
        method: ProofMethod,
        /// When, unix milliseconds.
        at_ms: u64,
    },
    /// A pin that stopped matching. Either the peer rotated a key without
    /// telling anyone, or someone is standing in the middle. gaz cannot tell
    /// which, and must not silently pick the friendlier reading.
    Mismatched {
        /// When the change was noticed, unix milliseconds.
        noticed_ms: u64,
    },
    /// Withdrawn: the binding was believed and no longer is.
    Revoked {
        /// When it was withdrawn, unix milliseconds.
        at_ms: u64,
    },
}

impl TrustState {
    /// Whether the binding has been positively established, by pin or by proof.
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::Pinned { .. } | Self::Verified { .. })
    }

    /// Whether this state should be surfaced to a person rather than absorbed.
    ///
    /// A mismatch may be an attack and a revocation may be news, so neither is
    /// something to render as a quiet grey dot.
    pub fn is_alarming(&self) -> bool {
        matches!(self, Self::Mismatched { .. } | Self::Revoked { .. })
    }

    /// Whether the endpoint should still be offered as a way to reach someone.
    pub fn is_usable(&self) -> bool {
        !self.is_alarming()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_honest() {
        assert_eq!(TrustState::default(), TrustState::Unverified);
        assert!(!TrustState::default().is_trusted());
        assert!(TrustState::default().is_usable());
    }

    #[test]
    fn pinned_and_verified_are_trusted() {
        assert!(TrustState::Pinned { first_seen_ms: 1 }.is_trusted());
        assert!(
            TrustState::Verified {
                method: ProofMethod::BackClaim,
                at_ms: 1,
            }
            .is_trusted()
        );
    }

    #[test]
    fn mismatch_and_revocation_are_alarming_and_unusable() {
        let mismatched = TrustState::Mismatched { noticed_ms: 5 };
        let revoked = TrustState::Revoked { at_ms: 5 };
        for state in [mismatched, revoked] {
            assert!(state.is_alarming());
            assert!(!state.is_usable());
            assert!(!state.is_trusted());
        }
    }

    #[test]
    fn serde_round_trips_with_a_custom_method() {
        let state = TrustState::Verified {
            method: ProofMethod::Other("tessera".into()),
            at_ms: 42,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<TrustState>(&json).unwrap(), state);
    }
}
