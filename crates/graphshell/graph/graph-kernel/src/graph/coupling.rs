/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `Coupling` — how nodes respond to a field. A coupling is
//! `field -> NodeSelector (a dynamic set) x response x strength`.
//!
//! This is the "seventh relation kind" of the
//! [field-system extraction](../../../../../design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md),
//! realized as a **tracked first-class field-layer primitive** (it has identity,
//! lifecycle, and persistence) rather than an `EdgeFamily` variant: a coupling
//! targets a *selector* over nodes, not a single node, so it cannot be an
//! `EdgePayload` sidecar on a node-to-node edge, and `EdgeFamily` (derived from
//! node-edge sidecars) stays six.
//!
//! Ported from `aether::coupling`. v1 responses are force-only (parity with
//! aether); the open response vocabulary (visual / navigational / selection /
//! semantic / trigger) is plan Phase 4, where the recognized-core-plus-open-tail
//! hybrid of the statements-over-schema stance applies. Derives are serde only
//! for now; rkyv lands at the `Persisted*` DTO layer (plan Phase 2). WASM-clean.

use serde::{Deserialize, Serialize};

use super::field::{CouplingId, FieldId};

/// Selects which nodes a coupling applies to. Tag and kind names are opaque
/// here — they resolve against the node's tags / classifications.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeSelector {
    /// Apply to every node.
    All,
    /// Apply to nodes carrying this tag.
    Tagged(String),
    /// Apply to nodes of this kind.
    Kind(String),
    /// Apply to nodes NOT carrying this tag.
    NotTagged(String),
}

/// How a node's motion responds to a field's value at its position. v1 is
/// force-only; see the module note for the deferred open vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CouplingResponse {
    /// Move along `-grad(scalar)` (gradient descent on a potential).
    AttractToMin,
    /// Move along `+grad(scalar)` (gradient ascent).
    RepelFromMax,
    /// Set velocity equal to the vector field at this position.
    AlignVelocity,
    /// Advect: `pos += dt * field(pos)`.
    FlowAdvect,
    /// Multiplicative damping when inside a positive scalar region.
    DampenInside { factor: f32 },
    /// Hard pushout when the scalar field exceeds zero.
    ContainmentWall,
}

/// One coupling rule: identity + target field + selector + response + strength.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coupling {
    pub id: CouplingId,
    pub field: FieldId,
    pub selector: NodeSelector,
    pub response: CouplingResponse,
    pub strength: f32,
}

impl Coupling {
    pub fn new(
        id: CouplingId,
        field: FieldId,
        selector: NodeSelector,
        response: CouplingResponse,
        strength: f32,
    ) -> Self {
        Self {
            id,
            field,
            selector,
            response,
            strength,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn coupling_serde_roundtrip() {
        let c = Coupling::new(
            CouplingId::from_uuid(uuid(1)),
            FieldId::from_uuid(uuid(2)),
            NodeSelector::Kind("paper".into()),
            CouplingResponse::AttractToMin,
            1.0,
        );
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(c, serde_json::from_str::<Coupling>(&s).unwrap());
    }

    #[test]
    fn selector_and_response_variants_roundtrip() {
        let selectors = [
            NodeSelector::All,
            NodeSelector::Tagged("important".into()),
            NodeSelector::Kind("paper".into()),
            NodeSelector::NotTagged("archived".into()),
        ];
        for s in &selectors {
            let j = serde_json::to_string(s).unwrap();
            assert_eq!(*s, serde_json::from_str::<NodeSelector>(&j).unwrap());
        }
        let responses = [
            CouplingResponse::AttractToMin,
            CouplingResponse::RepelFromMax,
            CouplingResponse::AlignVelocity,
            CouplingResponse::FlowAdvect,
            CouplingResponse::DampenInside { factor: 0.3 },
            CouplingResponse::ContainmentWall,
        ];
        for r in &responses {
            let j = serde_json::to_string(r).unwrap();
            assert_eq!(*r, serde_json::from_str::<CouplingResponse>(&j).unwrap());
        }
    }
}
