// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Field-layer mutators + queries on `Graph` (field-system step 3, Phase 1).
//!
//! Fields and couplings are content truth, stored in parallel keyed maps beside
//! the node/edge petgraph (see the struct fields on [`Graph`]). These mutators
//! sit on the same single-write-path boundary as the node/edge mutators:
//! trusted writers (reducer + persistence replay) call them; other paths route
//! through reducer intents. `quint` reads this layer and evaluates it; nothing
//! writes back here except an explicit author/pin action.

use super::{Coupling, CouplingId, Field, FieldId, FieldLifecycle, Graph, NodeKey, NodeSelector};

impl Graph {
    // ── Fields ────────────────────────────────────────────────────────────────

    /// Add or replace a field by its id.
    pub(crate) fn add_field(&mut self, field: Field) {
        self.fields.insert(field.id, field);
    }

    /// Retire a field: mark it `Retired` (keeping its definition for history /
    /// federation) rather than dropping it. Returns whether the field existed.
    pub(crate) fn retire_field(&mut self, id: FieldId) -> bool {
        self.set_field_lifecycle(id, FieldLifecycle::Retired)
    }

    /// Reactivate a retired field, marking it `Active` again. The inverse of
    /// [`retire_field`](Self::retire_field); together they make the field
    /// lifecycle fully round-trippable from the kernel (the activate/retire UX
    /// drives these). Returns whether the field existed.
    pub(crate) fn activate_field(&mut self, id: FieldId) -> bool {
        self.set_field_lifecycle(id, FieldLifecycle::Active)
    }

    fn set_field_lifecycle(&mut self, id: FieldId, lifecycle: FieldLifecycle) -> bool {
        match self.fields.get_mut(&id) {
            Some(field) => {
                field.lifecycle = lifecycle;
                true
            }
            None => false,
        }
    }

    /// The field with this id, if any.
    pub fn field(&self, id: FieldId) -> Option<&Field> {
        self.fields.get(&id)
    }

    /// All fields, in unspecified order.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.values()
    }

    // ── Couplings ─────────────────────────────────────────────────────────────

    /// Add or replace a coupling by its id.
    pub(crate) fn add_coupling(&mut self, coupling: Coupling) {
        self.couplings.insert(coupling.id, coupling);
    }

    /// Remove a coupling. Returns whether it existed. (Couplings have no
    /// lifecycle state of their own; retracting drops the rule.)
    pub(crate) fn retract_coupling(&mut self, id: CouplingId) -> bool {
        self.couplings.remove(&id).is_some()
    }

    /// All couplings, in unspecified order.
    pub fn couplings(&self) -> impl Iterator<Item = &Coupling> {
        self.couplings.values()
    }

    /// Couplings whose target is the given field.
    pub fn couplings_for_field(&self, field: FieldId) -> impl Iterator<Item = &Coupling> + '_ {
        self.couplings.values().filter(move |c| c.field == field)
    }

    /// The `strength` of the first coupling targeting `field`, if any (a field
    /// carries one default coupling). For the roster's per-field strength readout.
    /// (Field regions — strength tuning.)
    pub fn field_coupling_strength(&self, field: FieldId) -> Option<f32> {
        self.couplings
            .values()
            .find(|c| c.field == field)
            .map(|c| c.strength)
    }

    /// Set the `strength` of every coupling targeting `field`, returning whether any
    /// was updated — the per-field force-well tuning the roster drives. (Field
    /// regions — strength tuning.)
    pub(crate) fn set_field_coupling_strength(&mut self, field: FieldId, strength: f32) -> bool {
        let mut changed = false;
        for coupling in self.couplings.values_mut() {
            if coupling.field == field {
                coupling.strength = strength;
                changed = true;
            }
        }
        changed
    }

    // ── Selector evaluation ───────────────────────────────────────────────────

    /// Resolve a coupling's [`NodeSelector`] to the matching nodes, against node
    /// tags and classifications. `Kind(k)` matches a node carrying any
    /// classification whose value is `k` (the v1 interpretation; a
    /// scheme-qualified match can refine it later).
    pub fn nodes_matching<'a>(
        &'a self,
        selector: &'a NodeSelector,
    ) -> impl Iterator<Item = NodeKey> + 'a {
        self.inner.inner().node_indices().filter(move |&key| {
            let Some(node) = self.inner.node(key) else {
                return false;
            };
            match selector {
                NodeSelector::All => true,
                NodeSelector::Tagged(tag) => node.tags.contains(tag),
                NodeSelector::NotTagged(tag) => !node.tags.contains(tag),
                NodeSelector::Kind(kind) => self
                    .node_classifications(key)
                    .is_some_and(|classes| classes.iter().any(|c| &c.value == kind)),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CouplingResponse, FieldDefinition, ScalarField};
    use super::*;
    use euclid::default::Point2D;
    use uuid::Uuid;

    fn fid(n: u8) -> FieldId {
        FieldId::from_uuid(Uuid::from_bytes([n; 16]))
    }
    fn cid(n: u8) -> CouplingId {
        CouplingId::from_uuid(Uuid::from_bytes([n; 16]))
    }
    fn scalar_field(id: FieldId) -> Field {
        Field::new(id, FieldDefinition::Scalar(ScalarField::Const(1.0)))
    }

    #[test]
    fn add_field_and_query_round_trip() {
        let mut g = Graph::new();
        let id = fid(1);
        g.add_field(scalar_field(id).with_name("focus"));
        assert!(g.field(id).is_some());
        assert_eq!(g.fields().count(), 1);
        assert_eq!(g.field(id).unwrap().name.as_deref(), Some("focus"));
    }

    #[test]
    fn retire_field_marks_retired_keeps_definition() {
        let mut g = Graph::new();
        let id = fid(2);
        g.add_field(scalar_field(id));
        assert!(g.field(id).unwrap().is_active());
        assert!(g.retire_field(id));
        assert!(!g.field(id).unwrap().is_active());
        // definition retained
        assert!(matches!(
            g.field(id).unwrap().definition,
            FieldDefinition::Scalar(_)
        ));
        assert!(!g.retire_field(fid(99)), "unknown field retires false");

        // reactivating is the inverse and round-trips the lifecycle
        assert!(g.activate_field(id));
        assert!(g.field(id).unwrap().is_active());
        assert!(!g.activate_field(fid(99)), "unknown field activates false");
    }

    #[test]
    fn couplings_add_query_and_retract() {
        let mut g = Graph::new();
        let field = fid(3);
        let c = Coupling::new(
            cid(3),
            field,
            NodeSelector::All,
            CouplingResponse::AttractToMin,
            1.0,
        );
        let other = Coupling::new(
            cid(4),
            fid(7),
            NodeSelector::All,
            CouplingResponse::RepelFromMax,
            1.0,
        );
        g.add_coupling(c);
        g.add_coupling(other);
        assert_eq!(g.couplings().count(), 2);
        assert_eq!(g.couplings_for_field(field).count(), 1);
        assert!(g.retract_coupling(cid(3)));
        assert_eq!(g.couplings().count(), 1);
        assert!(!g.retract_coupling(cid(3)), "second retract is false");
    }

    #[test]
    fn nodes_matching_resolves_selectors() {
        let mut g = Graph::new();
        let a = g.add_node_with_id(
            Uuid::from_u128(1),
            "mere://a".into(),
            Point2D::new(0.0, 0.0),
        );
        let b = g.add_node_with_id(
            Uuid::from_u128(2),
            "mere://b".into(),
            Point2D::new(10.0, 0.0),
        );
        if let Some(node) = g.inner.node_mut(a) {
            node.tags.insert("important".to_string());
        }

        let all: Vec<_> = g.nodes_matching(&NodeSelector::All).collect();
        assert_eq!(all.len(), 2);

        let tagged: Vec<_> = g
            .nodes_matching(&NodeSelector::Tagged("important".into()))
            .collect();
        assert_eq!(tagged, vec![a]);

        let not_tagged: Vec<_> = g
            .nodes_matching(&NodeSelector::NotTagged("important".into()))
            .collect();
        assert_eq!(not_tagged, vec![b]);

        let no_kind: Vec<_> = g
            .nodes_matching(&NodeSelector::Kind("paper".into()))
            .collect();
        assert!(no_kind.is_empty());
    }
}
