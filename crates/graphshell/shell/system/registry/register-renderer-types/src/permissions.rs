// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Capability-gate seam — the substrate-side trait + decision type
//! the renderer registry consults at policy-relevant choice points.
//!
//! Per renderer-registry contract brief §7 and the capability-gate
//! catalogue brief: registry-level policy decisions (per-node route
//! override, profile escalation) go through a host-installed
//! `PermissionGate`. The substrate defines the seam + decision
//! vocabulary; the host implements the actual 4-layer policy chain
//! (action → session → persona → app) elsewhere.
//!
//! ## v0a scope
//!
//! - `CapabilityAction::RouteOverride { node, target_renderer }`
//!   fires when the substrate is about to honor a per-node renderer
//!   pin. The host can deny — the chain falls through to the next
//!   step (named default policy, then first-candidate).
//! - `PermissionDecision::{Allow, Deny}` only. The catalogue brief's
//!   `RequireConsent` (third decision variant that pops a confirm
//!   modal) waits on host UX for the modal flow; substrate-side it
//!   would degrade to `Deny` until the modal lands.

use crate::identity::RendererId;
use crate::scene::NodeIdentity;

/// What the substrate is asking permission for. Non-exhaustive — new
/// actions will land as more capability gates wire through the
/// registry (e.g. `EngineProfileEscalation`).
#[derive(Debug)]
#[non_exhaustive]
pub enum CapabilityAction<'a> {
    /// Per-node renderer pin override. Substrate is about to use
    /// `target_renderer` for `node` because the node's `renderer_pin`
    /// names it; gate `Deny` causes the chain to fall through to the
    /// next step (named default policy, then first-candidate
    /// tie-break).
    RouteOverride {
        node: NodeIdentity,
        target_renderer: &'a RendererId,
    },
}

/// Host's verdict on a `CapabilityAction`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PermissionDecision {
    /// Host permits the action — substrate proceeds.
    Allow,
    /// Host denies the action — substrate fails open to the next
    /// chain step (no panic, no abort).
    Deny,
}

/// Trait the registry consults at policy choice points. Substrate-
/// side: the trait. Host-side: the impl that walks the 4-layer
/// policy chain (per-action → per-session → per-persona → per-app
/// fallback).
///
/// Methods are `&self` so the gate can be queried during registry
/// dispatch without `&mut self` plumbing. Hosts needing mutable
/// state (caches, denial logs) use interior mutability.
pub trait PermissionGate {
    /// Default impl: allow everything. Hosts override to implement
    /// real policy.
    fn check(&self, action: &CapabilityAction<'_>) -> PermissionDecision {
        let _ = action;
        PermissionDecision::Allow
    }
}

/// Permit-everything gate. The registry's default — substrate ships
/// with no policy until the host installs one. Matches the contract
/// brief's "v0 ships with a permissive gate; host swaps in real
/// policy when ready" framing.
#[derive(Debug, Default, Clone, Copy)]
pub struct PermitEverythingGate;

impl PermissionGate for PermitEverythingGate {}

/// Deny-everything gate — primarily for tests verifying that the
/// chain falls through correctly when the gate denies. Production
/// hosts would install a real policy gate, not this.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyEverythingGate;

impl PermissionGate for DenyEverythingGate {
    fn check(&self, _action: &CapabilityAction<'_>) -> PermissionDecision {
        PermissionDecision::Deny
    }
}
