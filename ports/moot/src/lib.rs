// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! **Moot**, the Mere platform's places port.
//!
//! A moot is an assembly and the ground it is held on. This port is the
//! first-party application surface for the family's shared spaces, and it
//! carries two surfaces rather than one, because the workflows separate
//! cleanly and their consumers do not want them together:
//!
//! - **murmur** — the conversation surface. Direct and invitation-scoped
//!   conversations, store-and-forward mail, history, drafts, delivery,
//!   refusal and retry, attachments through shared content custody, and
//!   calls once the transport and media receipts support them. Its model is
//!   `mere-comms` (the WASM-clean inbox: `Conversation`, `Message`, `Draft`,
//!   and the `ProtocolAdapter` seam).
//! - **moot** — the community surface. Find, preview, join, leave and
//!   reconnect ceremony; membership and role inspection; proposals,
//!   decisions, moderation and appeals; storage and compute contributions;
//!   space health, replication and reachability.
//!
//! **murmur must mount alone.** Signalman wants messages and voice drops
//! without governance UI, and that constraint is what keeps the two surfaces
//! honestly separable rather than one screen with tabs.
//!
//! The boundaries are the point:
//!
//! - **Not the governance authority.** That is `gemot`, which owns a moot's
//!   lifecycle, membership, constitution, and trust facts. Tier 3 federation
//!   is `moothold`.
//! - **Not the exchange.** That is `murm`, which owns the post grammar, the
//!   signed per-author log, admission, and the sync lanes.
//! - **Not the shared graph.** That is the commons spine over `chartulary`,
//!   and not the replication mechanics either, which are `stickleback`'s.
//! - **Not a Turnstone feature.** The 2026-07-28 place-port plan ruled a
//!   place a Turnstone composition; the 2026-08-22 suite census reversed the
//!   application half of that ruling and left the authority half standing.
//!   Turnstone composes this port like any other host.
//!
//! The package is `mere-moot` because crates.io `moot` is held by an
//! unrelated crate with real code, and `murmur` is likewise taken. The
//! library keeps the product name.
//!
//! No implementation yet.

#![doc(html_no_source)]
