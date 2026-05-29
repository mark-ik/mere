/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Protocol contract registry — the canonical late-binding map of
//! URI schemes to handler IDs, plus the provider hook that lets mods
//! register handlers during activation.
//!
//! This crate is the proof-of-concept for the per-registry split
//! described in
//! `design_docs/graphshell_docs/implementation_strategy/2026-05-01_workspace_architecture_proposal.md`.
//! `contract` was extracted from `registries/atomic/protocol.rs`
//! (Slice 50b); `provider` was folded in from
//! `registries/atomic/protocol_provider.rs` (Slice 51) as the natural
//! sibling — providers register handlers *into* the contract registry,
//! so they belong in the same crate.
//!
//! ## Visibility
//!
//! The original visibility (`pub(crate)` everywhere) was lifted to
//! `pub` on extraction so external callers (the rest of Graphshell, and
//! eventually third parties) can use the registry through the crate
//! boundary. This is the deliberate API-surface widening called out in
//! the proposal §6.

mod contract;
mod provider;

pub use contract::{
    ContentStream, ProtocolContractRegistry, ProtocolContractResolution, ProtocolError,
    ProtocolHandler,
};
pub use provider::{ProtocolHandlerProvider, ProtocolHandlerProviders};
