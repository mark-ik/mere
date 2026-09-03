// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Mutable names that hang off a key.

use serde::{Deserialize, Serialize};

use crate::trust::TrustState;

/// What kind of name a handle is.
///
/// Open-ended: a scheme gaz has not heard of rides [`HandleKind::Other`]
/// instead of forcing a breaking change here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandleKind {
    /// A WebFinger account, `acct:user@host`, the host-rooted family.
    Acct,
    /// A decentralized identifier, `did:web:…` or `did:plc:…`.
    Did,
    /// A Nostr `npub…`.
    Nostr,
    /// Something else, named by the caller.
    Other(String),
}

/// A name a contact goes by, and how well it is bound to their key.
///
/// Handles change; keys do not. A handle is therefore a label on the record
/// rather than the thing the record is filed under, which is what lets a peer
/// move hosts without becoming a stranger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handle {
    /// Which scheme the handle belongs to.
    pub kind: HandleKind,
    /// The handle itself, as written.
    pub value: String,
    /// What is known about this handle actually belonging to the contact's key.
    /// Unverified until something proves otherwise.
    pub binding: TrustState,
}

impl Handle {
    /// Record a handle with nothing yet checked.
    pub fn new(kind: HandleKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
            binding: TrustState::Unverified,
        }
    }

    /// Record a WebFinger account handle.
    pub fn acct(value: impl Into<String>) -> Self {
        Self::new(HandleKind::Acct, value)
    }

    /// Set the binding state, for chaining at construction.
    pub fn with_binding(mut self, binding: TrustState) -> Self {
        self.binding = binding;
        self
    }

    /// The form to compare against when looking a handle up.
    ///
    /// Lowercased, with a leading `acct:` dropped, so `acct:Alice@Host` and
    /// `alice@host` are the same handle. Everything else is left alone,
    /// because gaz does not know the comparison rules of a scheme it has
    /// never met.
    pub fn normalized(&self) -> String {
        let lowered = self.value.trim().to_lowercase();
        lowered
            .strip_prefix("acct:")
            .unwrap_or(&lowered)
            .to_string()
    }

    /// Whether this handle matches a user-supplied string.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        let query = query.strip_prefix("acct:").unwrap_or(&query);
        self.normalized() == query
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_folds_case_and_the_acct_scheme() {
        let handle = Handle::acct("acct:Alice@Example.Org");
        assert_eq!(handle.normalized(), "alice@example.org");
    }

    #[test]
    fn matching_works_from_either_side_of_the_scheme() {
        let handle = Handle::acct("acct:alice@example.org");
        assert!(handle.matches("alice@example.org"));
        assert!(handle.matches("ACCT:Alice@Example.org"));
        assert!(!handle.matches("bob@example.org"));
    }

    #[test]
    fn unknown_schemes_are_left_alone_apart_from_case() {
        let handle = Handle::new(HandleKind::Other("matrix".into()), "@alice:Server.Net");
        assert_eq!(handle.normalized(), "@alice:server.net");
    }

    #[test]
    fn a_fresh_handle_is_unverified() {
        assert_eq!(
            Handle::acct("alice@example.org").binding,
            TrustState::Unverified
        );
    }

    #[test]
    fn serde_round_trips() {
        let handle = Handle::new(HandleKind::Did, "did:web:example.org")
            .with_binding(TrustState::Pinned { first_seen_ms: 9 });
        let json = serde_json::to_string(&handle).unwrap();
        assert_eq!(serde_json::from_str::<Handle>(&json).unwrap(), handle);
    }
}
