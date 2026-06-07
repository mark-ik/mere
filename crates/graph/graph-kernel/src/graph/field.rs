/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The `Field` truth primitive — fields as a third graph element beside nodes
//! and edges (the [field-system extraction](../../../../../design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md)
//! decision; this is its step-3 kernel slice).
//!
//! A [`Field`] is identity + a portable [`FieldDefinition`] (the scalar/vector
//! AST as data) + a [`FieldExtent`] + a [`FieldLifecycle`]. The definition is
//! truth the kernel owns and persists; `aether` evaluates it. Identity is a
//! stable, federatable UUID ([`FieldId`]), unlike `aether`'s registry-local
//! `FieldId(u64)` — the kernel id is canonical once fields are kernel truth.
//!
//! Storage (plan Phase 1) is a parallel keyed store on `Graph`, **not** a
//! petgraph node weight and **not** an `EdgePayload` sidecar: a coupling is
//! field-to-selector, never node-to-node, so it cannot ride the node/edge graph.
//!
//! Derives are serde only for now; rkyv archiving lands at the `Persisted*` DTO
//! layer (plan Phase 2). WASM-clean.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::field_ast::{ScalarField, VectorField};

/// Stable, federatable field identity. UUID-backed (vs `aether`'s
/// registry-local `FieldId(u64)`); this is the canonical id for kernel truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldId(pub Uuid);

impl FieldId {
    /// Fresh random identity. Non-WASM only (`Uuid::new_v4` needs an OS
    /// randomness source absent on `wasm32-unknown-unknown`); WASM hosts
    /// construct via [`FieldId::from_uuid`] with a host-supplied UUID.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Stable identity for a coupling rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CouplingId(pub Uuid);

impl CouplingId {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// A field's portable definition: scalar- or vector-valued. The AST is data;
/// `aether` evaluates it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldDefinition {
    Scalar(ScalarField),
    Vector(VectorField),
}

/// Where a field applies. (`Region` is an axis-aligned world-space box, kept as
/// four scalars to stay rkyv-friendly at the DTO layer without a `Box2D`
/// archive helper.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldExtent {
    /// Applies everywhere.
    Global,
    /// Applies within an axis-aligned world-space box.
    Region {
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    },
    /// Travels with a node (its `Node.id`).
    AttachedToNode(Uuid),
}

/// Field lifecycle state. Mirrors the spirit of `NodeLifecycle`: a field is
/// authored (Active) and can be retired without losing its definition for
/// history/federation. Richer states (Warm/Cold equivalents) are deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FieldLifecycle {
    Active,
    Retired,
}

/// A field as kernel truth: identity + portable definition + extent + lifecycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub id: FieldId,
    /// Optional authoring name (parity with `aether`'s named registry; lets
    /// definitions reference each other by a human label before id resolution).
    pub name: Option<String>,
    pub definition: FieldDefinition,
    pub extent: FieldExtent,
    pub lifecycle: FieldLifecycle,
}

impl Field {
    /// A global, active field with no authoring name.
    pub fn new(id: FieldId, definition: FieldDefinition) -> Self {
        Self {
            id,
            name: None,
            definition,
            extent: FieldExtent::Global,
            lifecycle: FieldLifecycle::Active,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_extent(mut self, extent: FieldExtent) -> Self {
        self.extent = extent;
        self
    }

    pub fn is_active(&self) -> bool {
        self.lifecycle == FieldLifecycle::Active
    }
}

#[cfg(test)]
mod tests {
    use super::super::field_ast::Falloff;
    use super::*;

    fn sample_uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn field_serde_roundtrip() {
        let field = Field::new(
            FieldId::from_uuid(sample_uuid(1)),
            FieldDefinition::Scalar(ScalarField::disk_at(0.0, 0.0, 50.0, Falloff::Smoothstep)),
        )
        .with_name("focus")
        .with_extent(FieldExtent::AttachedToNode(sample_uuid(2)));
        let s = serde_json::to_string(&field).unwrap();
        let back: Field = serde_json::from_str(&s).unwrap();
        assert_eq!(field, back);
    }

    #[test]
    fn defaults_are_global_active() {
        let f = Field::new(
            FieldId::from_uuid(sample_uuid(3)),
            FieldDefinition::Vector(VectorField::Coord),
        );
        assert_eq!(f.extent, FieldExtent::Global);
        assert!(f.is_active());
        assert!(f.name.is_none());
    }

    #[test]
    fn ids_are_distinct_types_but_uuid_backed() {
        let fid = FieldId::from_uuid(sample_uuid(4));
        let cid = CouplingId::from_uuid(sample_uuid(4));
        assert_eq!(fid.as_uuid(), cid.as_uuid());
    }
}
