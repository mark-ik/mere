// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Per-canvas field registry.
//!
//! A [`FieldRegistry`] owns a set of named scalar and vector fields,
//! addressable by opaque [`FieldId`]s. Composition surfaces (Rhai,
//! programmatic builders, persisted scene snapshots) all funnel through
//! this type.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::ast::{ScalarField, VectorField};

// Field identity and the scalar/vector definition enum are kernel truth. The
// registry keys on the canonical UUID `FieldId` and stores `FieldDefinition`
// (re-exported here as `FieldDef` for call-site continuity). Registry-local ids
// are minted deterministically from a counter (`Uuid::from_u128`) so the type
// stays WASM-clean; once fields are sourced from the `Graph` (field-system
// Phase 3b) kernel ids flow in directly and this minting falls away.
pub use numen::{FieldDefinition as FieldDef, FieldId};

/// Per-canvas field registry.
///
/// Fields are inserted by name and looked up by name or id. Names are unique
/// within a registry. Inserting a duplicate name overwrites the previous
/// entry but **keeps the same id** so dependent expressions stay stable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldRegistry {
    fields: HashMap<FieldId, FieldDef>,
    names: HashMap<String, FieldId>,
    next_id: u64,
}

impl FieldRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or update a scalar field by name. Returns its id.
    pub fn insert_scalar(&mut self, name: impl Into<String>, field: ScalarField) -> FieldId {
        let name = name.into();
        let id = self.assign_id(&name);
        self.fields.insert(id, FieldDef::Scalar(field));
        id
    }

    /// Register or update a vector field by name. Returns its id.
    pub fn insert_vector(&mut self, name: impl Into<String>, field: VectorField) -> FieldId {
        let name = name.into();
        let id = self.assign_id(&name);
        self.fields.insert(id, FieldDef::Vector(field));
        id
    }

    /// Insert a field definition under an explicit, caller-supplied id — used to
    /// seed the registry from the graph's field layer, where the canonical
    /// (kernel UUID) [`FieldId`] is already known. Overwrites any existing entry
    /// for that id. No name is registered (these are id-addressed; `Sample`
    /// resolves by id, and `lookup`/`name_of` stay name-keyed).
    pub fn insert_with_id(&mut self, id: FieldId, def: FieldDef) {
        self.fields.insert(id, def);
    }

    fn assign_id(&mut self, name: &str) -> FieldId {
        if let Some(&id) = self.names.get(name) {
            return id;
        }
        let id = FieldId::from_uuid(Uuid::from_u128(self.next_id as u128));
        self.next_id += 1;
        self.names.insert(name.to_string(), id);
        id
    }

    pub fn get(&self, id: FieldId) -> Option<&FieldDef> {
        self.fields.get(&id)
    }

    pub fn lookup(&self, name: &str) -> Option<FieldId> {
        self.names.get(name).copied()
    }

    pub fn name_of(&self, id: FieldId) -> Option<&str> {
        self.names
            .iter()
            .find_map(|(n, &i)| (i == id).then_some(n.as_str()))
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (FieldId, &FieldDef)> {
        self.fields.iter().map(|(&id, def)| (id, def))
    }

    /// Remove a field by id. Returns whether anything was removed.
    pub fn remove(&mut self, id: FieldId) -> bool {
        let removed = self.fields.remove(&id).is_some();
        if removed {
            self.names.retain(|_, &mut i| i != id);
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Falloff;
    use uuid::Uuid;

    #[test]
    fn insert_and_lookup_scalar() {
        let mut reg = FieldRegistry::new();
        let id = reg.insert_scalar("focus", ScalarField::gaussian_at(0.0, 0.0, 10.0));
        assert_eq!(reg.lookup("focus"), Some(id));
        assert!(matches!(reg.get(id), Some(FieldDef::Scalar(_))));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn insert_and_lookup_vector() {
        let mut reg = FieldRegistry::new();
        let id = reg.insert_vector("flow", VectorField::Coord);
        assert_eq!(reg.lookup("flow"), Some(id));
        assert!(matches!(reg.get(id), Some(FieldDef::Vector(_))));
    }

    #[test]
    fn ids_are_distinct_across_kinds() {
        let mut reg = FieldRegistry::new();
        let a = reg.insert_scalar("a", ScalarField::Const(1.0));
        let b = reg.insert_vector("b", VectorField::Coord);
        assert_ne!(a, b);
    }

    #[test]
    fn duplicate_name_keeps_id_and_overwrites_value() {
        let mut reg = FieldRegistry::new();
        let id1 = reg.insert_scalar("focus", ScalarField::Const(1.0));
        let id2 = reg.insert_scalar("focus", ScalarField::Const(2.0));
        assert_eq!(id1, id2);
        assert_eq!(reg.len(), 1);
        match reg.get(id1) {
            Some(FieldDef::Scalar(ScalarField::Const(v))) => assert_eq!(*v, 2.0),
            _ => panic!("expected Const(2.0)"),
        }
    }

    #[test]
    fn remove_clears_both_maps() {
        let mut reg = FieldRegistry::new();
        let id = reg.insert_scalar("ephemeral", ScalarField::Const(1.0));
        assert!(reg.remove(id));
        assert!(reg.lookup("ephemeral").is_none());
        assert!(reg.get(id).is_none());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn remove_returns_false_for_unknown_id() {
        let mut reg = FieldRegistry::new();
        assert!(!reg.remove(FieldId::from_uuid(Uuid::from_u128(42))));
    }

    #[test]
    fn name_of_returns_registered_name() {
        let mut reg = FieldRegistry::new();
        let id = reg.insert_scalar("topic_focus", ScalarField::CoordX);
        assert_eq!(reg.name_of(id), Some("topic_focus"));
    }

    #[test]
    fn iter_yields_all_entries() {
        let mut reg = FieldRegistry::new();
        let a = reg.insert_scalar("a", ScalarField::Const(1.0));
        let b = reg.insert_vector("b", VectorField::Coord);
        let collected: Vec<_> = reg.iter().map(|(id, _)| id).collect();
        assert!(collected.contains(&a));
        assert!(collected.contains(&b));
    }

    #[test]
    fn serde_roundtrip_preserves_registry() {
        let mut reg = FieldRegistry::new();
        reg.insert_scalar(
            "disk",
            ScalarField::disk_at(0.0, 0.0, 50.0, Falloff::Smoothstep),
        );
        reg.insert_vector("flow", VectorField::Coord);
        let s = serde_json::to_string(&reg).unwrap();
        let back: FieldRegistry = serde_json::from_str(&s).unwrap();
        assert_eq!(reg, back);
    }
}
