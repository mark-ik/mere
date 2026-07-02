/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `FieldProjection` — per-canvas bundle of fields, couplings, and edge-path
//! rules.
//!
//! Generalises the legacy z-derivation enum: that enum produced one scalar
//! per node for z; a `FieldProjection` produces an arbitrary collection of
//! fields for any consumer (z, layout forces, edge paths, background
//! visualisation, …) and names which one drives z when in 2.5D mode via
//! [`crate::projection::ViewDimension::TwoPointFive::z_field`].

use serde::{Deserialize, Serialize};

use kernel::graph::{Field, Graph};

use crate::ast::{ScalarField, VectorField};
use crate::coupling::{Coupling, EdgePathRule};
use crate::registry::{FieldId, FieldRegistry};

/// Per-canvas field-driven projection state.
///
/// Persisted as part of the per-view scene composition state. Carries the
/// registry, the active couplings, the edge-path rules, and the optional
/// z-driving field id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldProjection {
    pub registry: FieldRegistry,
    pub couplings: Vec<Coupling>,
    pub edge_path_rules: Vec<EdgePathRule>,
    /// If `Some`, this scalar field drives per-node z in 2.5D / Isometric.
    pub z_field: Option<FieldId>,
}

impl FieldProjection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience: register a scalar field by name.
    pub fn add_scalar(&mut self, name: impl Into<String>, field: ScalarField) -> FieldId {
        self.registry.insert_scalar(name, field)
    }

    /// Convenience: register a vector field by name.
    pub fn add_vector(&mut self, name: impl Into<String>, field: VectorField) -> FieldId {
        self.registry.insert_vector(name, field)
    }

    /// Convenience: append a coupling rule.
    pub fn add_coupling(&mut self, coupling: Coupling) {
        self.couplings.push(coupling);
    }

    /// Convenience: append an edge-path rule.
    pub fn add_edge_path_rule(&mut self, rule: EdgePathRule) {
        self.edge_path_rules.push(rule);
    }

    /// Commit this projection's fields and couplings into a kernel [`Graph`]'s
    /// field layer — the inverse of the read path `gyre` and `platen` use, and the
    /// bridge that lets an authored projection (e.g. one built by the Rhai surface)
    /// reach the truth those consumers read from.
    ///
    /// Fields come from the registry, added at `Global` extent and `Active` with
    /// their names carried over; couplings are added as-is (they already reference
    /// the registry `FieldId`s the committed fields use, so the references stay
    /// intact). `edge_path_rules` and `z_field` are projection-level view state
    /// with no `Graph` home yet, so they are not committed.
    ///
    /// Ids are the registry's (`Uuid::from_u128` of a per-registry counter): one
    /// authored projection lands cleanly, but committing two independently built
    /// projections into the same graph can collide on those ids (a later
    /// `add_field` replaces the earlier by id). Assign stable ids at the host when
    /// composing multiple projections matters. Returns `(fields, couplings)` added.
    pub fn commit_to_graph(&self, graph: &mut Graph) -> (usize, usize) {
        let mut fields = 0;
        for (id, def) in self.registry.iter() {
            let mut field = Field::new(id, def.clone());
            if let Some(name) = self.registry.name_of(id) {
                field = field.with_name(name);
            }
            let _ = kernel::graph::apply::apply_graph_delta(
                graph,
                kernel::graph::apply::GraphDelta::AddField { field },
            );
            fields += 1;
        }
        for coupling in &self.couplings {
            let _ = kernel::graph::apply::apply_graph_delta(
                graph,
                kernel::graph::apply::GraphDelta::AddCoupling { coupling: coupling.clone() },
            );
        }
        (fields, self.couplings.len())
    }
}

/// Builder helpers retained as a hook for future shapes (e.g.
/// `FieldProjectionBuilder::standard_force_directed()` etc.). Today this is
/// a unit struct namespace.
pub struct FieldProjectionBuilder;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coupling::{CouplingResponse, EdgePath, NodeSelector};

    #[test]
    fn new_projection_is_empty() {
        let p = FieldProjection::new();
        assert!(p.registry.is_empty());
        assert!(p.couplings.is_empty());
        assert!(p.edge_path_rules.is_empty());
        assert!(p.z_field.is_none());
    }

    #[test]
    fn add_scalar_returns_lookup_id() {
        let mut p = FieldProjection::new();
        let id = p.add_scalar("focus", ScalarField::gaussian_at(0.0, 0.0, 10.0));
        assert_eq!(p.registry.lookup("focus"), Some(id));
    }

    #[test]
    fn add_coupling_appends() {
        let mut p = FieldProjection::new();
        let id = p.add_scalar("f", ScalarField::Const(1.0));
        p.add_coupling(Coupling::new(
            kernel::graph::CouplingId::from_uuid(uuid::Uuid::from_u128(0)),
            id,
            NodeSelector::All,
            CouplingResponse::AttractToMin,
            1.0,
        ));
        assert_eq!(p.couplings.len(), 1);
    }

    #[test]
    fn add_edge_path_rule_appends() {
        let mut p = FieldProjection::new();
        p.add_edge_path_rule(EdgePathRule {
            edge_kind: "cites".into(),
            path: EdgePath::Straight,
        });
        assert_eq!(p.edge_path_rules.len(), 1);
    }

    #[test]
    fn serde_roundtrip_preserves_projection() {
        let mut p = FieldProjection::new();
        let id = p.add_scalar("focus", ScalarField::gaussian_at(0.0, 0.0, 10.0));
        p.z_field = Some(id);
        p.add_coupling(Coupling::new(
            kernel::graph::CouplingId::from_uuid(uuid::Uuid::from_u128(1)),
            id,
            NodeSelector::Kind("paper".into()),
            CouplingResponse::AttractToMin,
            0.8,
        ));
        p.add_edge_path_rule(EdgePathRule {
            edge_kind: "cites".into(),
            path: EdgePath::FieldLine {
                field: id,
                max_steps: 32,
                step_size: 4.0,
            },
        });
        let s = serde_json::to_string(&p).unwrap();
        let back: FieldProjection = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn commit_to_graph_writes_fields_and_couplings() {
        let mut p = FieldProjection::new();
        let fid = p.add_scalar("focus", ScalarField::gaussian_at(0.0, 0.0, 10.0));
        p.add_coupling(Coupling::new(
            kernel::graph::CouplingId::from_uuid(uuid::Uuid::from_u128(1)),
            fid,
            NodeSelector::All,
            CouplingResponse::AttractToMin,
            1.0,
        ));

        let mut g = Graph::new();
        let (fields, couplings) = p.commit_to_graph(&mut g);
        assert_eq!((fields, couplings), (1, 1));

        // The field landed under its registry id, named.
        let f = g.field(fid).expect("field committed");
        assert_eq!(f.name.as_deref(), Some("focus"));
        // The coupling landed, still referencing that field.
        let c = g
            .couplings_for_field(fid)
            .next()
            .expect("coupling committed");
        assert_eq!(c.selector, NodeSelector::All);
        assert_eq!(c.response, CouplingResponse::AttractToMin);
    }
}
