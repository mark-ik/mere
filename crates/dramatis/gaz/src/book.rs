//! The persona-scoped collection of records.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contact::{Contact, ContactTier};
use crate::key::ContactKey;

/// Which persona a book belongs to.
///
/// An opaque label, not a parsed identity: mere passes the text of a
/// `personae::PersonaId`, and gaz never interprets it. That is what keeps this
/// crate free of a dependency on the identity stack it serves.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PersonaScope(String);

impl PersonaScope {
    /// Label a book with the persona that owns it.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The label as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PersonaScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A book was loaded for a persona it does not belong to.
///
/// Worth a distinct error rather than a silent accept: the whole point of
/// scoping contacts is that a burner persona cannot see the work persona's
/// people, and a mis-filed book would defeat it quietly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeMismatch {
    /// The persona the caller asked for.
    pub expected: PersonaScope,
    /// The persona the book actually carries.
    pub found: PersonaScope,
}

impl fmt::Display for ScopeMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "contact book belongs to persona {}, not {}",
            self.found, self.expected
        )
    }
}

impl core::error::Error for ScopeMismatch {}

/// One persona's contacts.
///
/// Records are filed under their anchor, the first key you ever knew someone
/// by, so a key rotation never moves a record and an old signature still finds
/// its owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactBook {
    scope: PersonaScope,
    contacts: BTreeMap<ContactKey, Contact>,
}

impl ContactBook {
    /// Open an empty book for a persona.
    pub fn new(scope: PersonaScope) -> Self {
        Self {
            scope,
            contacts: BTreeMap::new(),
        }
    }

    /// The persona this book belongs to.
    pub fn scope(&self) -> &PersonaScope {
        &self.scope
    }

    /// Check that a loaded book belongs where it was filed.
    ///
    /// Call this after loading, before showing anyone's contacts.
    pub fn verify_scope(&self, expected: &PersonaScope) -> Result<(), ScopeMismatch> {
        if &self.scope == expected {
            Ok(())
        } else {
            Err(ScopeMismatch {
                expected: expected.clone(),
                found: self.scope.clone(),
            })
        }
    }

    /// File a contact, replacing any record under the same anchor.
    pub fn insert(&mut self, contact: Contact) -> Option<Contact> {
        self.contacts.insert(contact.anchor(), contact)
    }

    /// Look a contact up by anchor.
    pub fn get(&self, anchor: &ContactKey) -> Option<&Contact> {
        self.contacts.get(anchor)
    }

    /// Borrow a contact mutably by anchor.
    pub fn get_mut(&mut self, anchor: &ContactKey) -> Option<&mut Contact> {
        self.contacts.get_mut(anchor)
    }

    /// Find a contact by any key they have ever used, current or retired.
    ///
    /// The anchor lookup is a map hit; a retired key costs a scan, which is the
    /// right trade for an address book and the reason a rotated-away key still
    /// resolves at all.
    pub fn by_key(&self, key: &ContactKey) -> Option<&Contact> {
        self.contacts
            .get(key)
            .or_else(|| self.contacts.values().find(|c| c.knows_key(key)))
    }

    /// Find a contact by a handle string a person typed.
    pub fn by_handle(&self, query: &str) -> Option<&Contact> {
        self.contacts
            .values()
            .find(|contact| contact.find_handle(query).is_some())
    }

    /// Find a contact by an exact endpoint address.
    pub fn by_endpoint(&self, address: &str) -> Option<&Contact> {
        self.contacts
            .values()
            .find(|contact| contact.find_endpoint(address).is_some())
    }

    /// Remove a contact by anchor.
    pub fn remove(&mut self, anchor: &ContactKey) -> Option<Contact> {
        self.contacts.remove(anchor)
    }

    /// The people you have actually exchanged something with, most recent
    /// first.
    ///
    /// Contacts you have never reached are left out entirely rather than
    /// padding the tail: a recently-contacted list that includes people you
    /// have never contacted is not answering the question.
    pub fn recent(&self, limit: usize) -> Vec<&Contact> {
        let mut contacted: Vec<&Contact> = self
            .contacts
            .values()
            .filter(|contact| contact.last_contact_ms.is_some())
            .collect();

        contacted.sort_by(|a, b| {
            b.last_contact_ms
                .cmp(&a.last_contact_ms)
                .then_with(|| a.petname.cmp(&b.petname))
        });
        contacted.truncate(limit);
        contacted
    }

    /// Everyone at a given tier.
    pub fn tier(&self, tier: ContactTier) -> impl Iterator<Item = &Contact> {
        self.contacts
            .values()
            .filter(move |contact| contact.tier == tier)
    }

    /// Everyone whose record needs a person to look at it.
    pub fn alarms(&self) -> impl Iterator<Item = &Contact> {
        self.contacts.values().filter(|contact| contact.has_alarm())
    }

    /// Note contact with whoever owns this key, current or retired.
    ///
    /// Returns whether anyone was found.
    pub fn mark_contacted(&mut self, key: &ContactKey, now_ms: u64) -> bool {
        let Some(anchor) = self.by_key(key).map(Contact::anchor) else {
            return false;
        };
        if let Some(contact) = self.contacts.get_mut(&anchor) {
            contact.mark_contacted(now_ms);
        }
        true
    }

    /// Every contact, ordered by anchor.
    pub fn iter(&self) -> impl Iterator<Item = &Contact> {
        self.contacts.values()
    }

    /// How many contacts the book holds.
    pub fn len(&self) -> usize {
        self.contacts.len()
    }

    /// Whether the book is empty.
    pub fn is_empty(&self) -> bool {
        self.contacts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::{Endpoint, EndpointKind};
    use crate::handle::Handle;
    use crate::trust::TrustState;

    fn key(seed: u8) -> ContactKey {
        ContactKey::from_bytes([seed; 32])
    }

    fn book() -> ContactBook {
        ContactBook::new(PersonaScope::new("work"))
    }

    #[test]
    fn a_fresh_book_is_empty_and_scoped() {
        let book = book();
        assert!(book.is_empty());
        assert_eq!(book.scope().as_str(), "work");
    }

    #[test]
    fn scope_verification_catches_a_misfiled_book() {
        let book = book();
        assert!(book.verify_scope(&PersonaScope::new("work")).is_ok());

        let error = book.verify_scope(&PersonaScope::new("burner")).unwrap_err();
        assert_eq!(error.found, PersonaScope::new("work"));
        assert_eq!(error.expected, PersonaScope::new("burner"));
    }

    #[test]
    fn a_rotated_key_still_finds_its_owner() {
        let mut book = book();
        let mut alice = Contact::new("Alice", key(1));
        alice.rotate_to(key(2));
        book.insert(alice);

        assert_eq!(book.by_key(&key(1)).unwrap().petname, "Alice");
        assert_eq!(book.by_key(&key(2)).unwrap().petname, "Alice");
        assert!(book.by_key(&key(9)).is_none());
    }

    #[test]
    fn rotation_does_not_move_the_record() {
        let mut book = book();
        book.insert(Contact::new("Alice", key(1)));

        book.get_mut(&key(1)).unwrap().rotate_to(key(2));

        assert_eq!(book.len(), 1, "rotation must not create a second record");
        assert!(book.get(&key(1)).is_some(), "still filed under the anchor");
    }

    #[test]
    fn lookup_by_handle_and_endpoint() {
        let mut book = book();
        book.insert(
            Contact::new("Alice", key(1))
                .with_handle(Handle::acct("acct:Alice@example.org"))
                .with_endpoint(Endpoint::new(EndpointKind::Misfin, "alice@example.org")),
        );

        assert!(book.by_handle("alice@example.org").is_some());
        assert!(book.by_endpoint("alice@example.org").is_some());
        assert!(book.by_handle("bob@example.org").is_none());
    }

    #[test]
    fn recent_is_most_recent_first_and_omits_the_never_contacted() {
        let mut book = book();
        book.insert(Contact::new("Alice", key(1)));
        book.insert(Contact::new("Bob", key(2)));
        book.insert(Contact::new("Carol", key(3)));

        book.mark_contacted(&key(1), 100);
        book.mark_contacted(&key(2), 300);

        let names: Vec<&str> = book
            .recent(10)
            .iter()
            .map(|c| c.petname.as_str())
            .collect();
        assert_eq!(names, vec!["Bob", "Alice"], "Carol was never contacted");
    }

    #[test]
    fn recent_respects_its_limit() {
        let mut book = book();
        for seed in 1..=5u8 {
            book.insert(Contact::new(format!("P{seed}"), key(seed)));
            book.mark_contacted(&key(seed), u64::from(seed) * 10);
        }
        assert_eq!(book.recent(2).len(), 2);
        assert_eq!(book.recent(2)[0].petname, "P5");
    }

    #[test]
    fn marking_through_a_retired_key_reaches_the_record() {
        let mut book = book();
        let mut alice = Contact::new("Alice", key(1));
        alice.rotate_to(key(2));
        book.insert(alice);

        assert!(book.mark_contacted(&key(1), 500));
        assert_eq!(book.get(&key(1)).unwrap().last_contact_ms, Some(500));
        assert!(!book.mark_contacted(&key(9), 500), "nobody owns that key");
    }

    #[test]
    fn tiers_and_alarms_filter() {
        let mut book = book();
        book.insert(Contact::new("Alice", key(1)).with_tier(ContactTier::Kin));
        book.insert(
            Contact::new("Bob", key(2)).with_endpoint(
                Endpoint::new(EndpointKind::Misfin, "b@x.org")
                    .with_trust(TrustState::Mismatched { noticed_ms: 4 }),
            ),
        );

        let kin: Vec<&str> = book.tier(ContactTier::Kin).map(|c| c.petname.as_str()).collect();
        assert_eq!(kin, vec!["Alice"]);

        let alarming: Vec<&str> = book.alarms().map(|c| c.petname.as_str()).collect();
        assert_eq!(alarming, vec!["Bob"]);
    }

    #[test]
    fn serde_round_trips_a_book() {
        let mut book = book();
        book.insert(Contact::new("Alice", key(1)).with_handle(Handle::acct("a@x.org")));
        book.mark_contacted(&key(1), 77);

        let json = serde_json::to_string(&book).unwrap();
        assert_eq!(serde_json::from_str::<ContactBook>(&json).unwrap(), book);
    }
}
