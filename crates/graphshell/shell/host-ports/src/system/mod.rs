/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! System layer — host-neutral runtime services that compose into
//! `GraphshellRuntime`'s tick boundary.
//!
//! Per the workspace architecture proposal §3.2 / Layer D, this is the home
//! for cross-cutting system concerns (signal bus, caches, protocol probes,
//! snapshot/tracing infrastructure) that aren't specific to any one host
//! adapter. Routing the signal bus through a sibling crate would invert the
//! architecture (the system would depend on a peer-of-the-system to do its
//! own routing); the proposal explicitly forbids that.
//!
//! Inaugural occupant: [`signal_bus`] (the Register-owned signal routing
//! layer, consolidated from the shell-side `signal_routing.rs` per Slice 51).
//! Future siblings (`caches`, `protocol_probe`, `snapshots`, `tracing`)
//! follow as they need to move out of the shell-side runtime.

pub mod signal_bus;
