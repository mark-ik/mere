/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Persistence DTOs for the field layer (`Field` + `Coupling`), field-system
//! Phase 2.
//!
//! Siblings to the `Persisted*` types in [`crate::persistence`] (re-exported
//! there), kept in their own file so that module stays under the workspace's
//! per-file ceiling.
//!
//! The flat field-layer enums (extent, lifecycle, selector, response) archive
//! natively under rkyv. The one recursive piece — the scalar/vector AST
//! ([`FieldDefinition`](crate::graph::FieldDefinition), `Box`-nested) — rides as a
//! **serde-JSON string** ([`PersistedField::definition_json`]) rather than via
//! rkyv `omit_bounds`: the kernel AST is deliberately serde-only, the definition
//! is loaded once and evaluated in memory by `aether` (no zero-copy benefit), and
//! a blob keeps the archive flat and the round-trip robust. `serde_json` is
//! already a base kernel dependency.

use rkyv::{Archive, Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Field persistence types
// ---------------------------------------------------------------------------

/// Where a field applies (mirrors `graph::FieldExtent`). `AttachedToNode` stores
/// the node UUID as a string, matching `PersistedNode::node_id` / `PersistedEdge`.
#[derive(
    Archive, Serialize, Deserialize, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedFieldExtent {
    Global,
    Region {
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    },
    AttachedToNode(String),
}

impl Default for PersistedFieldExtent {
    fn default() -> Self {
        PersistedFieldExtent::Global
    }
}

/// Field lifecycle state (mirrors `graph::FieldLifecycle`).
#[derive(
    Archive,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedFieldLifecycle {
    #[default]
    Active,
    Retired,
}

/// Persisted field. Identity + a JSON-encoded definition AST + extent + lifecycle.
#[derive(
    Archive, Serialize, Deserialize, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[rkyv(derive(Debug))]
pub struct PersistedField {
    /// Stable field identity (UUID as string).
    pub id: String,
    /// Optional authoring name.
    #[serde(default)]
    pub name: Option<String>,
    /// The scalar/vector definition AST, serde-JSON-encoded (see module note).
    pub definition_json: String,
    #[serde(default)]
    pub extent: PersistedFieldExtent,
    #[serde(default)]
    pub lifecycle: PersistedFieldLifecycle,
}

// ---------------------------------------------------------------------------
// Coupling persistence types
// ---------------------------------------------------------------------------

/// Which nodes a coupling targets (mirrors `graph::NodeSelector`).
#[derive(
    Archive, Serialize, Deserialize, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedNodeSelector {
    All,
    Tagged(String),
    Kind(String),
    NotTagged(String),
}

/// How a coupling's targets respond to its field (mirrors `graph::CouplingResponse`).
/// The six force responses are the recognized core; `Open` carries the open-tail
/// families by IRI. Not `Copy` (the `Open` variant owns a `String`).
#[derive(
    Archive, Serialize, Deserialize, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum PersistedCouplingResponse {
    AttractToMin,
    RepelFromMax,
    AlignVelocity,
    FlowAdvect,
    DampenInside { factor: f32 },
    ContainmentWall,
    Open { predicate: String },
}

/// Persisted coupling. Identity + target field + selector + response + strength.
#[derive(
    Archive, Serialize, Deserialize, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[rkyv(derive(Debug))]
pub struct PersistedCoupling {
    /// Stable coupling identity (UUID as string).
    pub id: String,
    /// The field this coupling targets (UUID as string).
    pub field_id: String,
    pub selector: PersistedNodeSelector,
    pub response: PersistedCouplingResponse,
    pub strength: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_extent_and_lifecycle_serde_roundtrip() {
        let extents = [
            PersistedFieldExtent::Global,
            PersistedFieldExtent::Region {
                min_x: -1.0,
                min_y: -2.0,
                max_x: 3.0,
                max_y: 4.0,
            },
            PersistedFieldExtent::AttachedToNode("abc".into()),
        ];
        for e in &extents {
            let j = serde_json::to_string(e).unwrap();
            assert_eq!(
                *e,
                serde_json::from_str::<PersistedFieldExtent>(&j).unwrap()
            );
        }
        for l in [
            PersistedFieldLifecycle::Active,
            PersistedFieldLifecycle::Retired,
        ] {
            let j = serde_json::to_string(&l).unwrap();
            assert_eq!(
                l,
                serde_json::from_str::<PersistedFieldLifecycle>(&j).unwrap()
            );
        }
    }

    #[test]
    fn selector_and_response_serde_roundtrip() {
        let selectors = [
            PersistedNodeSelector::All,
            PersistedNodeSelector::Tagged("t".into()),
            PersistedNodeSelector::Kind("k".into()),
            PersistedNodeSelector::NotTagged("n".into()),
        ];
        for s in &selectors {
            let j = serde_json::to_string(s).unwrap();
            assert_eq!(
                *s,
                serde_json::from_str::<PersistedNodeSelector>(&j).unwrap()
            );
        }
        let responses = [
            PersistedCouplingResponse::AttractToMin,
            PersistedCouplingResponse::RepelFromMax,
            PersistedCouplingResponse::AlignVelocity,
            PersistedCouplingResponse::FlowAdvect,
            PersistedCouplingResponse::DampenInside { factor: 0.25 },
            PersistedCouplingResponse::ContainmentWall,
        ];
        for r in &responses {
            let j = serde_json::to_string(r).unwrap();
            assert_eq!(
                *r,
                serde_json::from_str::<PersistedCouplingResponse>(&j).unwrap()
            );
        }
    }

    #[test]
    fn field_and_coupling_rkyv_roundtrip() {
        // The flat DTOs archive + read back natively (the AST rides as the
        // definition_json string).
        let field = PersistedField {
            id: "field-1".into(),
            name: Some("focus".into()),
            definition_json: "{\"Scalar\":{\"Const\":1.0}}".into(),
            extent: PersistedFieldExtent::Region {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 10.0,
                max_y: 10.0,
            },
            lifecycle: PersistedFieldLifecycle::Retired,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&field).unwrap();
        let back: PersistedField =
            rkyv::from_bytes::<PersistedField, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.id, "field-1");
        assert_eq!(back.lifecycle, PersistedFieldLifecycle::Retired);
        assert_eq!(back.definition_json, field.definition_json);

        let coupling = PersistedCoupling {
            id: "c-1".into(),
            field_id: "field-1".into(),
            selector: PersistedNodeSelector::Tagged("paper".into()),
            response: PersistedCouplingResponse::DampenInside { factor: 0.3 },
            strength: 1.5,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&coupling).unwrap();
        let back: PersistedCoupling =
            rkyv::from_bytes::<PersistedCoupling, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.field_id, "field-1");
        assert_eq!(back.strength, 1.5);
    }
}
