// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! # gaz
//!
//! The contact layer: your records about other people.
//!
//! A contact is the local rollup that says these addresses are all the same
//! person. Your petname for them, rooted on their stable keys, with handles
//! and endpoints that each carry their own trust state. Records are
//! persona-scoped, tiered kith or kin, and carry recency.
//!
//! ## What roots a record
//!
//! Keys, not names. Handles change and hosts move; a key does not, so a peer
//! who switches providers stays the same person rather than becoming a second
//! row. A record is filed under its **anchor**, the first key you ever knew
//! someone by, which means rotating a key never moves the record and a message
//! signed with a retired key still finds its owner.
//!
//! ## What this crate is not
//!
//! - **Not the resolver.** A gazetteer turns a name, handle, or key into
//!   reachable endpoints. gaz is where the ones you keep live. The two are
//!   siblings on the persona tier, and gaz is not short for gazetteer.
//! - **Not identity.** `personae` owns *me*, the key-bag and its carry. gaz
//!   owns *them*, your own records about other people's keys.
//! - **Not trust arithmetic.** Trust state is stored per endpoint; how it is
//!   earned belongs to the trust plane. gaz therefore depends on no
//!   cryptography, holding keys as bytes it compares but never verifies.
//!
//! ## Two habits worth knowing
//!
//! gaz never reads a clock. Every timestamp is a `now_ms` you pass in, unix
//! milliseconds, which keeps the crate deterministic under test and usable on
//! wasm. And every recency update is monotonic, so a replayed or late-arriving
//! event can never rewind a record.
//!
//! ## Quick start
//!
//! ```
//! use gaz::{Contact, ContactBook, ContactKey, Endpoint, EndpointKind, Handle, PersonaScope};
//!
//! let mut book = ContactBook::new(PersonaScope::new("work"));
//! let alice_key = ContactKey::from_bytes([1u8; 32]);
//!
//! book.insert(
//!     Contact::new("Alice", alice_key)
//!         .with_handle(Handle::acct("acct:Alice@example.org"))
//!         .with_endpoint(Endpoint::new(EndpointKind::Misfin, "alice@example.org")),
//! );
//!
//! // She writes back; you note it, supplying the clock yourself.
//! book.mark_contacted(&alice_key, 1_754_000_000_000);
//!
//! assert_eq!(book.recent(5)[0].petname, "Alice");
//! assert!(book.by_handle("alice@example.org").is_some());
//! ```
//!
//! ## Status
//!
//! Pre-1.0, and the data model is the part that exists. Persistence over
//! `muniment` and the adapters that turn resolver output into records are the
//! next lifts; see the founding plan in `design_docs/`.

#![warn(missing_docs)]

pub mod book;
pub mod contact;
pub mod endpoint;
pub mod handle;
pub mod key;
pub mod trust;

pub use book::{ContactBook, PersonaScope, ScopeMismatch};
pub use contact::{Contact, ContactTier};
pub use endpoint::{Endpoint, EndpointKind};
pub use handle::{Handle, HandleKind};
pub use key::{ContactKey, KeyParseError};
pub use trust::{ProofMethod, TrustState};

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
