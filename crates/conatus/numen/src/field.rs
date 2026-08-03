//! The `Field` truth primitive — fields as a third graph element beside nodes
//! and edges (the field-system extraction
//! decision; this is its step-3 kernel slice).
//!
//! A [`Field`] is identity + a portable [`FieldDefinition`] (the scalar/vector
//! AST as data) + a [`FieldExtent`] + a [`FieldLifecycle`]. The definition is
//! truth the kernel owns and persists; `quint` evaluates it. Identity is a
//! stable, federatable UUID ([`FieldId`]), unlike `quint`'s original
//! registry-local `FieldId(u64)` — the kernel id is canonical once fields are
//! kernel truth.
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

/// Stable, federatable field identity. UUID-backed (vs `quint`'s original
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
/// `quint` evaluates it.
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
    /// Applies within a closed world-space polygon.
    ///
    /// The hulls extent: a place-graph cell, a drawn hull, a map region. Kept
    /// as bare point pairs for the same rkyv-friendliness reason `Region` is
    /// four scalars. Fewer than three points is a degenerate extent that
    /// contains nothing.
    Polygon { points: Vec<(f32, f32)> },
}

impl FieldExtent {
    /// Whether a world-space point falls inside this extent.
    ///
    /// `None` for [`AttachedToNode`](FieldExtent::AttachedToNode), which only
    /// a graph can answer: the extent travels with a node this type cannot
    /// see. Callers with a graph resolve that arm themselves.
    pub fn contains(&self, x: f32, y: f32) -> Option<bool> {
        match self {
            FieldExtent::Global => Some(true),
            FieldExtent::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } => Some(x >= *min_x && x <= *max_x && y >= *min_y && y <= *max_y),
            FieldExtent::AttachedToNode(_) => None,
            FieldExtent::Polygon { points } => Some(polygon_contains(points, x, y)),
        }
    }

    /// Distance from a point to the extent's boundary, for falloff shaping.
    ///
    /// Positive inside, negative outside, `None` where boundaries make no
    /// sense (`Global` has none; `AttachedToNode` needs a graph).
    pub fn boundary_distance(&self, x: f32, y: f32) -> Option<f32> {
        match self {
            FieldExtent::Global => None,
            FieldExtent::AttachedToNode(_) => None,
            FieldExtent::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                let inside =
                    (x - min_x).min(max_x - x).min(y - min_y).min(max_y - y);
                Some(inside)
            }
            FieldExtent::Polygon { points } => {
                if points.len() < 3 {
                    return None;
                }
                let nearest = points
                    .iter()
                    .enumerate()
                    .map(|(index, a)| {
                        let b = points[(index + 1) % points.len()];
                        segment_distance(*a, b, (x, y))
                    })
                    .fold(f32::INFINITY, f32::min);
                Some(if polygon_contains(points, x, y) {
                    nearest
                } else {
                    -nearest
                })
            }
        }
    }
}

/// Even-odd ray cast. Winding is not significant, matching the scene
/// contract's polygon footprint.
fn polygon_contains(points: &[(f32, f32)], x: f32, y: f32) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (index, a) in points.iter().enumerate() {
        let b = points[(index + 1) % points.len()];
        if (a.1 > y) != (b.1 > y) && x < a.0 + (b.0 - a.0) * (y - a.1) / (b.1 - a.1) {
            inside = !inside;
        }
    }
    inside
}

fn segment_distance(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let (apx, apy) = (p.0 - a.0, p.1 - a.1);
    let length_sq = abx * abx + aby * aby;
    let t = if length_sq <= f32::EPSILON {
        0.0
    } else {
        ((apx * abx + apy * aby) / length_sq).clamp(0.0, 1.0)
    };
    let (dx, dy) = (apx - abx * t, apy - aby * t);
    (dx * dx + dy * dy).sqrt()
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
    /// Optional authoring name (parity with `quint`'s named registry; lets
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
    fn a_polygon_extent_contains_and_measures() {
        // A triangle. Containment is even-odd; boundary distance is signed,
        // positive inside, so falloffs can shape against the edge.
        let extent = FieldExtent::Polygon {
            points: vec![(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)],
        };

        assert_eq!(extent.contains(2.0, 2.0), Some(true));
        assert_eq!(extent.contains(9.0, 9.0), Some(false));
        assert!(extent.boundary_distance(2.0, 2.0).unwrap() > 0.0);
        assert!(extent.boundary_distance(20.0, 20.0).unwrap() < 0.0);
        // On an axis edge, distance to the nearest boundary is the y height.
        let inside = extent.boundary_distance(3.0, 1.0).unwrap();
        assert!((inside - 1.0).abs() < 1e-4, "got {inside}");
    }

    #[test]
    fn a_degenerate_polygon_contains_nothing() {
        let extent = FieldExtent::Polygon {
            points: vec![(0.0, 0.0), (10.0, 0.0)],
        };
        assert_eq!(extent.contains(5.0, 0.0), Some(false));
        assert_eq!(extent.boundary_distance(5.0, 0.0), None);
    }

    #[test]
    fn attachment_is_a_question_for_a_graph() {
        // The honest None: this type cannot know where a node is.
        let extent = FieldExtent::AttachedToNode(sample_uuid(9));
        assert_eq!(extent.contains(0.0, 0.0), None);
        assert_eq!(extent.boundary_distance(0.0, 0.0), None);
    }

    #[test]
    fn a_polygon_extent_round_trips() {
        let field = Field::new(
            FieldId::from_uuid(sample_uuid(7)),
            FieldDefinition::Scalar(ScalarField::Const(1.0)),
        )
        .with_extent(FieldExtent::Polygon {
            points: vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)],
        });
        let json = serde_json::to_string(&field).unwrap();
        assert_eq!(serde_json::from_str::<Field>(&json).unwrap(), field);
    }

    #[test]
    fn ids_are_distinct_types_but_uuid_backed() {
        let fid = FieldId::from_uuid(sample_uuid(4));
        let cid = CouplingId::from_uuid(sample_uuid(4));
        assert_eq!(fid.as_uuid(), cid.as_uuid());
    }
}
