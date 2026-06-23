/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `CouplingForce` — the aether→gyre seam (field-system Phase 3b).
//!
//! A [`kernel::graph::Coupling`] is field-algebra truth: a [`NodeSelector`] of
//! targets, a [`FieldId`](kernel::graph::FieldId), a [`CouplingResponse`], and a
//! strength. This adapter compiles one into a [`Force`] so the same rapier tick
//! that runs the built-in [`NodeExclusion`](crate::NodeExclusion) /
//! [`EdgeSpring`](crate::EdgeSpring) / [`Boundary`](crate::Boundary) forces also
//! runs scriptable couplings. `aether` evaluates the field (closed forms +
//! finite-difference gradients); gyre supplies bodies and integration.
//!
//! Build it against a [`Graph`] with [`CouplingForce::from_coupling`] — it
//! resolves the field definition and the selector's matching nodes once, at
//! build time — or from an already-resolved field + target set with
//! [`CouplingForce::new`]. The captured target set is a snapshot: rebuild the
//! force when the graph's nodes or the selector's matches change.
//!
//! Equivalence: `Boundary` is exactly `AttractToMin` on the radial paraboloid
//! `½(x² + y²)` (its gradient is `(x, y)`, so `-grad · strength = -pos · strength`).
//! The built-ins stay as the fast native path; couplings are the general one.

use aether::{FieldRegistry, ScalarField, VectorField, eval_scalar, eval_vector, grad_scalar};
use kernel::graph::{Coupling, CouplingResponse, FieldDefinition, Graph, NodeKey};
use rapier2d::prelude::*;

use crate::{Force, ForceContext};

/// A kernel [`Coupling`] compiled to a [`Force`]: evaluate the field at each
/// target node's position and apply the response.
pub struct CouplingForce {
    response: CouplingResponse,
    strength: f32,
    /// Nodes the coupling acts on, resolved from the selector at build time.
    targets: Vec<NodeKey>,
    /// The field to sample. Scalar responses (attract/repel/wall/dampen) need a
    /// scalar; flow responses (align/advect) need a vector. A mismatch is inert.
    field: FieldDefinition,
    /// Registry for the field's `Sample` references. Empty by default — a
    /// self-contained field needs none, and a missing sample evaluates to zero.
    registry: FieldRegistry,
}

impl CouplingForce {
    /// From an already-resolved field definition and target set. Needs no
    /// [`Graph`]; the field must be self-contained or carry its registry via
    /// [`Self::with_registry`].
    pub fn new(
        response: CouplingResponse,
        strength: f32,
        targets: impl IntoIterator<Item = NodeKey>,
        field: FieldDefinition,
    ) -> Self {
        Self {
            response,
            strength,
            targets: targets.into_iter().collect(),
            field,
            registry: FieldRegistry::new(),
        }
    }

    /// Supply a registry so the field's `Sample` references resolve.
    pub fn with_registry(mut self, registry: FieldRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// Resolve a coupling against the graph: look up its field definition and the
    /// nodes its selector matches (a snapshot — rebuild on graph mutation).
    /// Returns `None` if the field id is unknown.
    pub fn from_coupling(coupling: &Coupling, graph: &Graph) -> Option<Self> {
        let field = graph.field(coupling.field)?;
        let targets: Vec<NodeKey> = graph.nodes_matching(&coupling.selector).collect();
        // Seed the registry from the whole field layer so the coupling's field can
        // reference others by `Sample(FieldId)` and resolve them. Without this an
        // inter-field `Sample` evaluates to zero.
        let mut registry = FieldRegistry::new();
        for f in graph.fields() {
            registry.insert_with_id(f.id, f.definition.clone());
        }
        Some(
            Self::new(
                coupling.response.clone(),
                coupling.strength,
                targets,
                field.definition.clone(),
            )
            .with_registry(registry),
        )
    }

    /// Number of nodes this force currently acts on.
    pub fn target_count(&self) -> usize {
        self.targets.len()
    }

    fn scalar(&self) -> Option<&ScalarField> {
        match &self.field {
            FieldDefinition::Scalar(s) => Some(s),
            FieldDefinition::Vector(_) => None,
        }
    }

    fn vector(&self) -> Option<&VectorField> {
        match &self.field {
            FieldDefinition::Vector(v) => Some(v),
            FieldDefinition::Scalar(_) => None,
        }
    }
}

impl Force for CouplingForce {
    fn apply(&self, ctx: &mut ForceContext<'_>, dt: f32) {
        // Snapshot (handle, x, y) for each target immutably before mutating
        // forces / velocities (same discipline the built-in forces follow).
        let mut samples: Vec<(RigidBodyHandle, f32, f32)> = Vec::with_capacity(self.targets.len());
        for &node in &self.targets {
            if let Some(&handle) = ctx.bodies_by_node.get(&node) {
                if let Some(body) = ctx.bodies.get(handle) {
                    let t = body.translation();
                    samples.push((handle, t.x, t.y));
                }
            }
        }

        match &self.response {
            // Gradient descent / ascent on a scalar potential. Attract follows
            // -grad (toward minima); repel follows +grad (toward maxima).
            CouplingResponse::AttractToMin | CouplingResponse::RepelFromMax => {
                let Some(scalar) = self.scalar() else { return };
                let sign = if matches!(self.response, CouplingResponse::AttractToMin) {
                    -1.0
                } else {
                    1.0
                };
                for (handle, x, y) in samples {
                    let (gx, gy) = grad_scalar(scalar, &self.registry, x, y, 0.0);
                    if let Some(body) = ctx.bodies.get_mut(handle) {
                        let f = Vector::new(sign * gx * self.strength, sign * gy * self.strength);
                        body.add_force(f, true);
                    }
                }
            }
            // Hard pushout wherever the scalar is positive ("inside"): push along
            // +grad scaled by how far inside the body is.
            CouplingResponse::ContainmentWall => {
                let Some(scalar) = self.scalar() else { return };
                for (handle, x, y) in samples {
                    let depth = eval_scalar(scalar, &self.registry, x, y, 0.0);
                    if depth > 0.0 {
                        let (gx, gy) = grad_scalar(scalar, &self.registry, x, y, 0.0);
                        if let Some(body) = ctx.bodies.get_mut(handle) {
                            let f = Vector::new(gx, gy) * (self.strength * depth);
                            body.add_force(f, true);
                        }
                    }
                }
            }
            // Multiplicative velocity damping while inside a positive region.
            CouplingResponse::DampenInside { factor } => {
                let Some(scalar) = self.scalar() else { return };
                for (handle, x, y) in samples {
                    if eval_scalar(scalar, &self.registry, x, y, 0.0) > 0.0 {
                        if let Some(body) = ctx.bodies.get_mut(handle) {
                            let v = body.linvel();
                            body.set_linvel(v * *factor, true);
                        }
                    }
                }
            }
            // Set velocity to the vector field at this position (scaled).
            CouplingResponse::AlignVelocity => {
                let Some(field) = self.vector() else { return };
                for (handle, x, y) in samples {
                    let (vx, vy) = eval_vector(field, &self.registry, x, y, 0.0);
                    if let Some(body) = ctx.bodies.get_mut(handle) {
                        body.set_linvel(Vector::new(vx * self.strength, vy * self.strength), true);
                    }
                }
            }
            // Advect: nudge position along the field this step (pos += dt * field).
            CouplingResponse::FlowAdvect => {
                let Some(field) = self.vector() else { return };
                for (handle, x, y) in samples {
                    let (vx, vy) = eval_vector(field, &self.registry, x, y, 0.0);
                    if let Some(body) = ctx.bodies.get_mut(handle) {
                        let t = body.translation();
                        let next = t + Vector::new(vx, vy) * (self.strength * dt);
                        body.set_translation(next, true);
                    }
                }
            }
            // Open tail (visual / navigational / selection / semantic / trigger):
            // not a force response, so the integrator leaves these bodies untouched.
            // The consumer that owns the family acts on them elsewhere.
            CouplingResponse::Open { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Boundary, Simulation};
    use euclid::default::Point2D;
    use kernel::graph::{Field, FieldId};

    fn node_at(g: &mut Graph, id: u128, x: f32, y: f32) -> NodeKey {
        g.add_node_with_id(uuid::Uuid::from_u128(id), format!("mere://{id}"), Point2D::new(x, y))
    }

    fn radius(sim: &Simulation, k: NodeKey) -> f32 {
        sim.position_of(k).unwrap().to_vector().length()
    }

    /// `½(x² + y²)` — gradient `(x, y)`, the potential whose `AttractToMin`
    /// coupling reproduces `Boundary`.
    fn paraboloid() -> ScalarField {
        ScalarField::Scale(
            Box::new(ScalarField::Add(
                Box::new(ScalarField::Mul(
                    Box::new(ScalarField::CoordX),
                    Box::new(ScalarField::CoordX),
                )),
                Box::new(ScalarField::Mul(
                    Box::new(ScalarField::CoordY),
                    Box::new(ScalarField::CoordY),
                )),
            )),
            0.5,
        )
    }

    #[test]
    fn attract_to_min_centers_like_boundary() {
        // Two sims, same seed: Boundary vs an AttractToMin coupling on the
        // paraboloid. The paraboloid's gradient is (x, y), so the coupling force
        // is -pos·strength — Boundary exactly, up to aether's finite-difference
        // step. The bodies track each other across the whole settle (a ~580-unit
        // journey leaves them within a couple of units).
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, 500.0, -300.0);

        let mut boundary_sim = Simulation::new();
        boundary_sim.sync_with_graph(&g);
        boundary_sim.add_force(Boundary { strength: 1.5 });

        let mut coupling_sim = Simulation::new();
        coupling_sim.sync_with_graph(&g);
        coupling_sim.add_force(CouplingForce::new(
            CouplingResponse::AttractToMin,
            1.5,
            [a],
            FieldDefinition::Scalar(paraboloid()),
        ));

        for _ in 0..120 {
            boundary_sim.tick(1.0 / 60.0);
            coupling_sim.tick(1.0 / 60.0);
        }

        let pb = boundary_sim.position_of(a).unwrap();
        let pc = coupling_sim.position_of(a).unwrap();
        assert!(radius(&coupling_sim, a) < 500.0, "coupling should pull inward");
        assert!(
            (pb - pc).length() < 5.0,
            "coupling diverged from Boundary: {pb:?} vs {pc:?}"
        );
    }

    #[test]
    fn from_coupling_resolves_field_and_selector() {
        // Full Graph → Coupling → Force path: a field stored on the graph, a
        // coupling selecting all nodes, resolved into a force that centers them.
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, 400.0, 0.0);
        let b = node_at(&mut g, 2, 0.0, 400.0);

        let fid = FieldId::from_uuid(uuid::Uuid::from_u128(7));
        g.add_field(Field::new(fid, FieldDefinition::Scalar(paraboloid())));
        let coupling = Coupling::new(
            kernel::graph::CouplingId::from_uuid(uuid::Uuid::from_u128(1)),
            fid,
            kernel::graph::NodeSelector::All,
            CouplingResponse::AttractToMin,
            1.5,
        );

        let force = CouplingForce::from_coupling(&coupling, &g).expect("field resolves");
        assert_eq!(force.target_count(), 2, "All selector matches both nodes");

        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        let ra0 = radius(&sim, a);
        let rb0 = radius(&sim, b);
        sim.add_force(force);
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        assert!(radius(&sim, a) < ra0, "node a should center");
        assert!(radius(&sim, b) < rb0, "node b should center");
    }

    #[test]
    fn from_coupling_resolves_sample_through_registry() {
        // Field "referencing" is just Sample(base); base is the paraboloid. The
        // coupling targets "referencing", so it only produces motion if
        // from_coupling seeded the registry with base — exercises the follow-up.
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, 400.0, 0.0);

        let base = FieldId::from_uuid(uuid::Uuid::from_u128(10));
        g.add_field(Field::new(base, FieldDefinition::Scalar(paraboloid())));
        let referencing = FieldId::from_uuid(uuid::Uuid::from_u128(11));
        g.add_field(Field::new(
            referencing,
            FieldDefinition::Scalar(ScalarField::Sample(base)),
        ));

        let coupling = Coupling::new(
            kernel::graph::CouplingId::from_uuid(uuid::Uuid::from_u128(1)),
            referencing,
            kernel::graph::NodeSelector::All,
            CouplingResponse::AttractToMin,
            1.5,
        );
        let force = CouplingForce::from_coupling(&coupling, &g).expect("field resolves");

        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        let r0 = radius(&sim, a);
        sim.add_force(force);
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        assert!(
            radius(&sim, a) < r0,
            "a Sample-referenced field should resolve and center the node"
        );
    }

    #[test]
    fn from_coupling_returns_none_for_unknown_field() {
        let mut g = Graph::new();
        node_at(&mut g, 1, 10.0, 0.0);
        let coupling = Coupling::new(
            kernel::graph::CouplingId::from_uuid(uuid::Uuid::from_u128(1)),
            FieldId::from_uuid(uuid::Uuid::from_u128(99)),
            kernel::graph::NodeSelector::All,
            CouplingResponse::AttractToMin,
            1.0,
        );
        assert!(CouplingForce::from_coupling(&coupling, &g).is_none());
    }

    #[test]
    fn open_response_applies_no_force() {
        // An open-tail response (e.g. a visual coupling) is not a force: the
        // integrator must leave the body where it is, even on a field whose
        // AttractToMin response would otherwise pull it to the origin.
        let mut g = Graph::new();
        let a = node_at(&mut g, 1, 300.0, 0.0);

        let mut sim = Simulation::new();
        sim.sync_with_graph(&g);
        sim.add_force(CouplingForce::new(
            CouplingResponse::open(format!("{}visual/highlight", kernel::graph::COUPLING_VOCAB)),
            1.0,
            [a],
            FieldDefinition::Scalar(paraboloid()),
        ));
        let p0 = sim.position_of(a).unwrap();
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        assert!(
            (sim.position_of(a).unwrap() - p0).length() < 1.0e-3,
            "an open-tail response must apply no force"
        );
    }
}
