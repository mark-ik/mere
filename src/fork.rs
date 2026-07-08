/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Log identity and fork provenance.
//!
//! A log can be forked: the fork copies the history so far and records where it
//! branched from, then diverges independently. This is git-style branching over an
//! append-only log. Two logs that share a fork point are related by their
//! provenance, not by deduplicating their entries.

use serde::{Deserialize, Serialize};

use crate::seq::Seq;

/// A stable identity for a log. Caller-chosen (a name, a URN, a hash you already
/// hold), so identity is deterministic and needs no randomness source.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LogId(pub String);

impl LogId {
    /// Name a log.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identity string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The record a forked log carries: where it branched from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The log this one forked from, if that log had an identity.
    pub source: Option<LogId>,
    /// The source's length at the fork point: entries `[0, at)` were copied in.
    pub at: Seq,
}
