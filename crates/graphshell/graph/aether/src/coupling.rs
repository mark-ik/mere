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

use crate::registry::FieldId;

// Coupling truth types — NodeSelector, CouplingResponse, Coupling — are owned by
// the kernel field layer (a coupling targets a selector, not a node→node edge, so
// it lives beside the graph as kernel truth rather than on the petgraph). aether
// re-exports them so `crate::coupling::Coupling` and `aether::Coupling` keep
// resolving onto the canonical kernel types. The kernel `Coupling` carries a
// stable `CouplingId`; construct via `Coupling::new(id, field, selector, response,
// strength)`.
pub use kernel::graph::{Coupling, CouplingResponse, NodeSelector};

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
    use uuid::Uuid;

    // Coupling/NodeSelector/CouplingResponse serde + construction are tested in
    // the kernel (`graph::coupling`, `graph::field_ops`). Here we only cover the
    // aether-local edge-path types, which embed the kernel UUID `FieldId`.
    fn fid(n: u8) -> FieldId {
        FieldId::from_uuid(Uuid::from_bytes([n; 16]))
    }

    #[test]
    fn edge_path_serde() {
        let paths = [
            EdgePath::Straight,
            EdgePath::Spline { tension: 0.5 },
            EdgePath::FieldLine {
                field: fid(3),
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
                field: fid(1),
                max_steps: 32,
                step_size: 2.0,
            },
        };
        let json = serde_json::to_string(&rule).unwrap();
        let back: EdgePathRule = serde_json::from_str(&json).unwrap();
        assert_eq!(rule, back);
    }
}
