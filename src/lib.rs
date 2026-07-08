/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! codicil — an append-only, replayable log.
//!
//! A codicil is an amendment appended to a document, never a rewrite of it. This
//! crate is that discipline as a data structure: a [`Codicil<T>`] is a linear log
//! of immutable entries you [`append`](Codicil::append) and [`replay`](Codicil::replay)
//! to reconstruct the state they describe. Edits are never destroyed; a change is
//! a new entry.
//!
//! It is the event-source and nondestructive-history primitive shared across the
//! Strophos apps: isometry's session events, strophe's edit history, mere's graph
//! mutations. Each stamps entries with a monotonic [`Seq`] that stays valid for
//! the life of the log, so a reader or peer can hold one as a durable cursor and
//! catch up with [`from`](Codicil::from) / [`replay_from`](Codicil::replay_from).
//!
//! codicil is the versioning half over its sibling [`muniment`], the store: a log
//! persists through a muniment slot. It is transport-neutral (it produces a
//! replayable sequence; shipping it to peers is the consumer's job) and linear
//! (a branching edit-tree is a later shape), by deliberate scope.

pub mod log;
pub mod persist;
pub mod seq;

pub use log::Codicil;
pub use seq::Seq;
