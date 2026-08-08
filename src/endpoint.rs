//! Where a contact is reachable now.

use serde::{Deserialize, Serialize};

use crate::trust::TrustState;

/// Which protocol an address speaks.
///
/// The named variants are the endpoint types mere's gazetteer already
/// classifies WebFinger links into, plus murm. Deliberately not mere's
/// `comms::ProtocolKind`, which is a closed two-variant enum owned by the app;
/// gaz has to hold addresses for protocols it cannot message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointKind {
    /// A misfin mailbox, `mailbox@host`.
    Misfin,
    /// A murm author key.
    Murm,
    /// A gemini capsule.
    Gemini,
    /// A gopher resource.
    Gopher,
    /// An ActivityPub actor.
    ActivityPub,
    /// An ordinary web profile page.
    Http,
    /// Something else, named by the caller.
    Other(String),
}

/// One address a contact can be reached at, with its own trust state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Which protocol this address speaks.
    pub kind: EndpointKind,
    /// The address, in whatever form its protocol writes.
    pub address: String,
    /// What is known about this address belonging to the contact.
    pub trust: TrustState,
    /// When this address was last used, unix milliseconds. `None` if never.
    pub last_used_ms: Option<u64>,
}

impl Endpoint {
    /// Record an address with nothing yet checked and no use history.
    pub fn new(kind: EndpointKind, address: impl Into<String>) -> Self {
        Self {
            kind,
            address: address.into(),
            trust: TrustState::Unverified,
            last_used_ms: None,
        }
    }

    /// Set the trust state, for chaining at construction.
    pub fn with_trust(mut self, trust: TrustState) -> Self {
        self.trust = trust;
        self
    }

    /// Whether this address should still be offered as a way to reach someone.
    pub fn is_usable(&self) -> bool {
        self.trust.is_usable()
    }

    /// Note that this address was just used.
    ///
    /// Monotonic: an out-of-order or replayed timestamp never moves the record
    /// backwards, so a late-arriving event cannot rewrite recency.
    pub fn mark_used(&mut self, now_ms: u64) {
        if self.last_used_ms.is_none_or(|last| now_ms > last) {
            self.last_used_ms = Some(now_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_endpoint_is_unverified_and_unused() {
        let endpoint = Endpoint::new(EndpointKind::Misfin, "alice@example.org");
        assert_eq!(endpoint.trust, TrustState::Unverified);
        assert_eq!(endpoint.last_used_ms, None);
        assert!(endpoint.is_usable());
    }

    #[test]
    fn a_revoked_endpoint_is_not_usable() {
        let endpoint = Endpoint::new(EndpointKind::Misfin, "alice@example.org")
            .with_trust(TrustState::Revoked { at_ms: 3 });
        assert!(!endpoint.is_usable());
    }

    #[test]
    fn use_marking_is_monotonic() {
        let mut endpoint = Endpoint::new(EndpointKind::Murm, "ab12");
        endpoint.mark_used(100);
        assert_eq!(endpoint.last_used_ms, Some(100));

        endpoint.mark_used(50);
        assert_eq!(endpoint.last_used_ms, Some(100), "a late event must not rewind");

        endpoint.mark_used(150);
        assert_eq!(endpoint.last_used_ms, Some(150));
    }

    #[test]
    fn serde_round_trips_with_a_custom_kind() {
        let endpoint = Endpoint::new(EndpointKind::Other("matrix".into()), "@a:b.net");
        let json = serde_json::to_string(&endpoint).unwrap();
        assert_eq!(serde_json::from_str::<Endpoint>(&json).unwrap(), endpoint);
    }
}
