// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Name reservation for **dramatis**, the cast-list tier of the Mere platform.
//!
//! *Dramatis personae*: the persons of the drama. The tier holds both sides of
//! identity, your faces and the other players, which is why the name is the
//! full cast list rather than any one role:
//!
//! - **personae** — the trust-plane spine: master keypair, per-protocol
//!   derivation, vault, sealed records, carry.
//! - **gaz** — stored contacts: key-rooted records, petnames, per-endpoint
//!   trust, kith/kin tiers.
//! - **gazette** — handle resolution: turning a name into reachable,
//!   trust-stated endpoints.
//!
//! The boundaries are the point:
//!
//! - **Not the data plane.** Persistence is the eidetic family (muniment,
//!   codicil, chartulary). The planes bond at the seal seam and the sync gate;
//!   dramatis holds keys and trust, never the bytes they seal.
//! - **Not a product.** *Persona* is an in-product term for a face; dramatis
//!   names the tier so the term stays free.
//!
//! If a facade over the member crates ever earns its existence, it lives here.
//! No implementation yet.

#![doc(html_no_source)]
