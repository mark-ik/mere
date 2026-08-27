// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! codicil — an append-only, replayable log.
//!
//! A codicil is an amendment appended to a document, never a rewrite of it. This
//! crate is that discipline as a data structure: a [`Codicil<T>`] is a linear log
//! of immutable entries you [`append`](Codicil::append) and [`replay`](Codicil::replay)
//! to reconstruct the state they describe. Edits are never destroyed; a change is
//! a new entry.
//!
//! It is the event-source and nondestructive-history primitive shared across the
//! Merely apps: isometry's session events, strophe's edit history, mere's graph
//! mutations. Each stamps entries with a monotonic [`Seq`] that stays valid for
//! the life of the log, so a reader or peer can hold one as a durable cursor and
//! catch up with [`from`](Codicil::from) / [`replay_from`](Codicil::replay_from).
//!
//! codicil is the versioning half over its sibling [`muniment`], the store: a log
//! persists through a muniment slot. It is transport-neutral: it produces a
//! replayable sequence, and shipping it to peers is the consumer's job.
//!
//! # Storage is a sequence; shape is a graph
//!
//! Entries are a flat, monotonic list, which is what makes append, replay, and
//! persistence cheap. Causality rides beside them as parent links, so a log can
//! also answer what led to an entry, what followed from it, and which entries
//! were genuinely concurrent. Opt in with
//! [`append_caused_by`](Codicil::append_caused_by); plain
//! [`append`](Codicil::append) claims no causes and behaves exactly as before.
//! See [`causal`] for the invariant that makes this nearly free.
//!
//! This replaces the crate's earlier scope note that a branching edit-tree was
//! a later shape. It is this shape, and it arrived without disturbing the
//! linear one.

pub mod causal;
pub mod fork;
pub mod log;
pub mod persist;
pub mod seq;

pub use fork::{LogId, Provenance};
pub use log::Codicil;
pub use seq::Seq;
