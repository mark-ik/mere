//! `EdgePath` / `EdgePathRule` — how an edge's path is generated between its two
//! endpoints, optionally traced through a vector field.
//!
//! Edge-geometry siblings to [`Coupling`](super::coupling::Coupling): both are
//! field-layer truth the kernel owns and `quint` reads, and neither is a
//! node→node `EdgePayload` sidecar. Where a coupling says *how nodes respond to a
//! field*, an edge-path rule says *how an edge's curve is drawn* (and a
//! `FieldLine` path is field-driven, hence the [`FieldId`] reference).
//!
//! Ported from `quint::coupling` (field-system extraction Phase 4) so the kernel
//! owns every field-layer definition type; `quint` re-exports these. Derives are
//! serde only (parity with [`coupling`](super::coupling) / [`field_ast`](super::field_ast));
//! rkyv lands at the `Persisted*` DTO layer if these become persisted. WASM-clean.

use serde::{Deserialize, Serialize};

use super::field::FieldId;

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
            assert_eq!(*p, serde_json::from_str::<EdgePath>(&json).unwrap());
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
        assert_eq!(rule, serde_json::from_str::<EdgePathRule>(&json).unwrap());
    }
}
