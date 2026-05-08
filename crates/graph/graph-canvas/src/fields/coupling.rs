/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Coupling rules: how nodes and edges respond to fields.
//!
//! Today's [`crate::scene_region::SceneRegionEffect`] (Attractor / Repulsor /
//! Dampener / Wall) is a special case: a bounded-shape field with a fixed
//! response. This module generalises that to "any field × any response,
//! selectable by node tag/kind." Force-directed layout falls out as one
//! such coupling (a per-node-emitted repulsive scalar field).

use serde::{Deserialize, Serialize};

use crate::fields::registry::FieldId;

/// Selects which nodes (or edges) a coupling rule applies to.
///
/// Tag and kind names are opaque to graph-canvas — the host decides what
/// they mean in its own graph model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// How a node's motion responds to a field's value at its position.
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
    /// Hard pushout when the scalar field exceeds zero (replaces `Wall`).
    ContainmentWall,
}

/// One coupling rule: target nodes × field × response × strength.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coupling {
    pub selector: NodeSelector,
    pub field: FieldId,
    pub response: CouplingResponse,
    pub strength: f32,
}

impl Coupling {
    pub fn new(
        selector: NodeSelector,
        field: FieldId,
        response: CouplingResponse,
        strength: f32,
    ) -> Self {
        Self {
            selector,
            field,
            response,
            strength,
        }
    }
}

/// How an edge's path is generated between its two endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EdgePath {
    /// Straight line.
    Straight,
    /// Catmull-Rom-shaped spline with an authored tension.
    Spline { tension: f32 },
    /// Trace integral curves of a vector field from source toward target.
    /// `max_steps` caps the polyline length; `step_size` is in world units.
    FieldLine {
        field: FieldId,
        max_steps: u32,
        step_size: f32,
    },
}

/// Per edge-kind path-generation rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgePathRule {
    pub edge_kind: String,
    pub path: EdgePath,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupling_construction() {
        let c = Coupling::new(
            NodeSelector::Kind("paper".into()),
            FieldId(7),
            CouplingResponse::AttractToMin,
            1.0,
        );
        assert_eq!(c.selector, NodeSelector::Kind("paper".into()));
        assert_eq!(c.field, FieldId(7));
        assert_eq!(c.response, CouplingResponse::AttractToMin);
        assert_eq!(c.strength, 1.0);
    }

    #[test]
    fn node_selector_serde() {
        let selectors = [
            NodeSelector::All,
            NodeSelector::Tagged("important".into()),
            NodeSelector::Kind("paper".into()),
            NodeSelector::NotTagged("archived".into()),
        ];
        for s in &selectors {
            let json = serde_json::to_string(s).unwrap();
            let back: NodeSelector = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, back);
        }
    }

    #[test]
    fn coupling_response_serde() {
        let responses = [
            CouplingResponse::AttractToMin,
            CouplingResponse::RepelFromMax,
            CouplingResponse::AlignVelocity,
            CouplingResponse::FlowAdvect,
            CouplingResponse::DampenInside { factor: 0.3 },
            CouplingResponse::ContainmentWall,
        ];
        for r in &responses {
            let json = serde_json::to_string(r).unwrap();
            let back: CouplingResponse = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, back);
        }
    }

    #[test]
    fn edge_path_serde() {
        let paths = [
            EdgePath::Straight,
            EdgePath::Spline { tension: 0.5 },
            EdgePath::FieldLine {
                field: FieldId(3),
                max_steps: 64,
                step_size: 4.0,
            },
        ];
        for p in &paths {
            let json = serde_json::to_string(p).unwrap();
            let back: EdgePath = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, back);
        }
    }

    #[test]
    fn edge_path_rule_serde() {
        let rule = EdgePathRule {
            edge_kind: "cites".into(),
            path: EdgePath::FieldLine {
                field: FieldId(1),
                max_steps: 32,
                step_size: 2.0,
            },
        };
        let json = serde_json::to_string(&rule).unwrap();
        let back: EdgePathRule = serde_json::from_str(&json).unwrap();
        assert_eq!(rule, back);
    }
}
