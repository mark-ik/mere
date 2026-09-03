// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Entry sequence numbers.

use serde::{Deserialize, Serialize};

/// The position of an entry in a [`Journal`](super::Journal). Zero-based: the
/// first appended entry is `Seq(0)`.
///
/// Monotonic and stable. Because entries are never removed or reordered, a
/// `Seq` refers to the same entry for the life of the log, so it is a durable
/// cursor a peer or a reader can hold across sessions.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Seq(pub u64);

impl Seq {
    /// The entry index this sequence addresses.
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The next sequence after this one.
    pub fn next(self) -> Seq {
        Seq(self.0 + 1)
    }
}
