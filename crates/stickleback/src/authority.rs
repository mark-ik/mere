// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Domain-injected authority for retention trust-root transitions.

use proofs::Digest;

/// Authorization seam for accepting a retention checkpoint.
///
/// Mesh implements this with its owner key. Moot supplies a signer set and
/// revision derived from an accepted constitution. Replication does not infer
/// authority from transport access or visible membership.
pub trait CheckpointAuthority {
    fn authority_revision(&self) -> Digest;

    fn permits_checkpoint(&self, author: [u8; 32], named_revision: &Digest) -> bool;
}
