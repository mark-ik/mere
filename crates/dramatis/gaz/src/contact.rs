//! The record itself: one person, however many addresses.

use serde::{Deserialize, Serialize};

use crate::endpoint::Endpoint;
use crate::handle::Handle;
use crate::key::ContactKey;

/// How close a contact is.
///
/// Set by you, not derived from trust. Verification is a property of an
/// address; kith and kin describe a relationship, and folding one into the
/// other would mean a peer becomes close because their certificate checked
/// out. The two axes stay separate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContactTier {
    /// Known to you. The default for anyone you have merely met.
    #[default]
    Kith,
    /// Close: the people a permission can safely default to.
    Kin,
}

/// The local rollup that says these addresses are all the same person.
///
/// Rooted on keys, labelled with a petname you chose, carrying handles and
/// endpoints that each hold their own trust state. Local only: a contact
/// record is your view of someone and never goes onto a wire.
///
/// The key-rooted rule is enforced rather than documented. A contact cannot be
/// constructed without a key, and the key list can only grow, so there is no
/// state in which a record has drifted loose from its root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    /// Your name for them. Not their claim about themselves.
    pub petname: String,
    /// Every key you have known them by, oldest first. Never empty, on the way
    /// in as well as the way out: hand-written or corrupted state that has lost
    /// its root fails to load rather than deserializing into a contact rooted
    /// on nothing.
    #[serde(deserialize_with = "nonempty_keys")]
    keys: Vec<ContactKey>,
    /// Names they go by, each with its own binding state.
    pub handles: Vec<Handle>,
    /// Addresses they are reachable at, each with its own trust state.
    pub endpoints: Vec<Endpoint>,
    /// How close they are.
    pub tier: ContactTier,
    /// When you last exchanged anything, unix milliseconds.
    pub last_contact_ms: Option<u64>,
    /// A private note to yourself.
    pub note: Option<String>,
}

/// Reject a key list that has lost its root.
fn nonempty_keys<'de, D>(deserializer: D) -> Result<Vec<ContactKey>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let keys = Vec::<ContactKey>::deserialize(deserializer)?;
    if keys.is_empty() {
        return Err(serde::de::Error::custom(
            "a contact is key-rooted: its key list cannot be empty",
        ));
    }
    Ok(keys)
}

impl Contact {
    /// Start a record for someone you know a key for.
    pub fn new(petname: impl Into<String>, key: ContactKey) -> Self {
        Self {
            petname: petname.into(),
            keys: vec![key],
            handles: Vec::new(),
            endpoints: Vec::new(),
            tier: ContactTier::default(),
            last_contact_ms: None,
            note: None,
        }
    }

    /// The key you first knew them by.
    ///
    /// Stable for the life of the record, which is why a [`ContactBook`] files
    /// them under it: rotating a key never moves the record, and a message
    /// signed with a retired key still finds its way home.
    ///
    /// [`ContactBook`]: crate::ContactBook
    pub fn anchor(&self) -> ContactKey {
        self.keys[0]
    }

    /// The key they are using now.
    pub fn current_key(&self) -> ContactKey {
        *self.keys.last().expect("a contact always holds a key")
    }

    /// Every key, oldest first.
    pub fn keys(&self) -> &[ContactKey] {
        &self.keys
    }

    /// Whether this is one of their keys, current or retired.
    pub fn knows_key(&self, key: &ContactKey) -> bool {
        self.keys.contains(key)
    }

    /// Record that they have rotated to a new key.
    ///
    /// Returns whether anything changed. Rotating to a key already in the
    /// history is a no-op rather than an error, so replaying an event stream
    /// is safe.
    pub fn rotate_to(&mut self, key: ContactKey) -> bool {
        if self.keys.contains(&key) {
            return false;
        }
        self.keys.push(key);
        true
    }

    /// Add a handle, for chaining at construction.
    pub fn with_handle(mut self, handle: Handle) -> Self {
        self.handles.push(handle);
        self
    }

    /// Add an endpoint, for chaining at construction.
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    /// Set the tier, for chaining at construction.
    pub fn with_tier(mut self, tier: ContactTier) -> Self {
        self.tier = tier;
        self
    }

    /// Set the private note, for chaining at construction.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// The addresses still worth offering as a way to reach them.
    pub fn reachable(&self) -> impl Iterator<Item = &Endpoint> {
        self.endpoints
            .iter()
            .filter(|endpoint| endpoint.is_usable())
    }

    /// Whether anything about this record should reach a person.
    ///
    /// True when any endpoint or handle has gone mismatched or revoked. A
    /// contact list that hides this is worse than no contact list.
    pub fn has_alarm(&self) -> bool {
        self.endpoints
            .iter()
            .any(|endpoint| endpoint.trust.is_alarming())
            || self
                .handles
                .iter()
                .any(|handle| handle.binding.is_alarming())
    }

    /// Find a handle by the string a person typed.
    pub fn find_handle(&self, query: &str) -> Option<&Handle> {
        self.handles.iter().find(|handle| handle.matches(query))
    }

    /// Find an endpoint by exact address.
    pub fn find_endpoint(&self, address: &str) -> Option<&Endpoint> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.address == address)
    }

    /// Note that you exchanged something with them just now.
    ///
    /// Monotonic, like [`Endpoint::mark_used`]: a late or replayed event never
    /// moves recency backwards.
    pub fn mark_contacted(&mut self, now_ms: u64) {
        if self.last_contact_ms.is_none_or(|last| now_ms > last) {
            self.last_contact_ms = Some(now_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::EndpointKind;
    use crate::trust::TrustState;

    fn key(seed: u8) -> ContactKey {
        ContactKey::from_bytes([seed; 32])
    }

    fn alice() -> Contact {
        Contact::new("Alice", key(1))
    }

    #[test]
    fn a_new_contact_is_kith_and_anchored() {
        let contact = alice();
        assert_eq!(contact.tier, ContactTier::Kith);
        assert_eq!(contact.anchor(), key(1));
        assert_eq!(contact.current_key(), key(1));
        assert_eq!(contact.last_contact_ms, None);
    }

    #[test]
    fn rotation_keeps_the_anchor_and_moves_the_current_key() {
        let mut contact = alice();
        assert!(contact.rotate_to(key(2)));

        assert_eq!(contact.anchor(), key(1), "the anchor must never move");
        assert_eq!(contact.current_key(), key(2));
        assert!(contact.knows_key(&key(1)), "a retired key still resolves");
        assert!(contact.knows_key(&key(2)));
    }

    #[test]
    fn rotating_to_a_known_key_is_a_no_op() {
        let mut contact = alice();
        contact.rotate_to(key(2));
        assert!(!contact.rotate_to(key(2)));
        assert_eq!(contact.keys().len(), 2);
    }

    #[test]
    fn reachable_skips_revoked_addresses() {
        let contact = alice()
            .with_endpoint(Endpoint::new(EndpointKind::Misfin, "a@x.org"))
            .with_endpoint(
                Endpoint::new(EndpointKind::Gemini, "gemini://x.org/~a")
                    .with_trust(TrustState::Revoked { at_ms: 1 }),
            );

        let reachable: Vec<_> = contact.reachable().map(|e| e.address.as_str()).collect();
        assert_eq!(reachable, vec!["a@x.org"]);
    }

    #[test]
    fn a_mismatched_endpoint_raises_an_alarm() {
        assert!(!alice().has_alarm());

        let contact = alice().with_endpoint(
            Endpoint::new(EndpointKind::Misfin, "a@x.org")
                .with_trust(TrustState::Mismatched { noticed_ms: 7 }),
        );
        assert!(contact.has_alarm());
    }

    #[test]
    fn a_mismatched_handle_binding_also_raises_an_alarm() {
        let contact = alice()
            .with_handle(Handle::acct("a@x.org").with_binding(TrustState::Revoked { at_ms: 2 }));
        assert!(contact.has_alarm());
    }

    #[test]
    fn contact_marking_is_monotonic() {
        let mut contact = alice();
        contact.mark_contacted(500);
        contact.mark_contacted(200);
        assert_eq!(contact.last_contact_ms, Some(500));
    }

    #[test]
    fn handles_are_found_through_normalization() {
        let contact = alice().with_handle(Handle::acct("acct:Alice@Example.org"));
        assert!(contact.find_handle("alice@example.org").is_some());
        assert!(contact.find_handle("bob@example.org").is_none());
    }

    #[test]
    fn a_record_that_lost_its_root_fails_to_load() {
        let rootless = r#"{
            "petname": "Nobody",
            "keys": [],
            "handles": [],
            "endpoints": [],
            "tier": "Kith",
            "last_contact_ms": null,
            "note": null
        }"#;

        let error = serde_json::from_str::<Contact>(rootless).unwrap_err();
        assert!(
            error.to_string().contains("key-rooted"),
            "the failure must name the invariant, got: {error}"
        );
    }

    #[test]
    fn serde_round_trips_a_full_record() {
        let mut contact = alice()
            .with_tier(ContactTier::Kin)
            .with_handle(Handle::acct("alice@example.org"))
            .with_endpoint(Endpoint::new(EndpointKind::Murm, "ff00"))
            .with_note("met at the moot");
        contact.rotate_to(key(2));
        contact.mark_contacted(1234);

        let json = serde_json::to_string(&contact).unwrap();
        assert_eq!(serde_json::from_str::<Contact>(&json).unwrap(), contact);
    }
}
