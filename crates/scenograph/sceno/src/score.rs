//! Scores — persisted, product-free instructions for making a scene.
//!
//! A score names an arrangement and supplies the measured instances that the
//! arrangement places. It deliberately carries opaque [`SourceRef`]s rather
//! than source truth: a Mere graph adapter, an Isometry campaign adapter, and
//! a geographic fixture can serialize the same vocabulary without sharing a
//! model.

use serde::{Deserialize, Serialize};

use crate::{Footprint, Rect, Representation, Size2, SourceRef, Vec2};

/// The first persisted-score wire version.
pub const SCORE_VERSION: u16 = 1;

/// A complete, serializable projection request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// Allows an adapter to reject a score it cannot interpret.
    pub version: u16,
    /// The analytic placement family that will realize [`Self::items`].
    pub arrangement: Arrangement,
    /// Ordered, measured source instances. A source may appear more than
    /// once, which remains distinct from source truth by design.
    pub items: Vec<ScoreItem>,
    /// Adapter-stamped input generation, copied into the realized scene.
    pub generation: u64,
}

impl Score {
    pub fn new(arrangement: Arrangement) -> Self {
        Self {
            version: SCORE_VERSION,
            arrangement,
            items: Vec::new(),
            generation: 0,
        }
    }
}

/// The portable analytic arrangements exercised by the first proofs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Arrangement {
    Spiral(Spiral),
    Board(Board),
    Geographic(Geographic),
    Hulls(Hulls),
}

/// A golden-angle-family spiral. Spacing is a user-configurable lower bound;
/// solvers grow it when measured footprints require more clearance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spiral {
    pub center: Vec2,
    pub spacing: f32,
    pub angle_radians: f32,
    pub curve: SpiralCurve,
}

impl Default for Spiral {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            spacing: 22.0,
            angle_radians: 2.399_963_3,
            curve: SpiralCurve::SquareRoot,
        }
    }
}

/// Radius growth as a function of an item's ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SpiralCurve {
    #[default]
    SquareRoot,
    Linear,
    Quadratic,
    Logarithmic,
}

/// A regular tile board. Explicit cells preserve authored board coordinates;
/// items without one flow left-to-right from their ordinal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub origin: Vec2,
    pub cell: Vec2,
    pub columns: u32,
    pub gap: f32,
}

impl Default for Board {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            cell: Vec2::new(64.0, 64.0),
            columns: 8,
            gap: 4.0,
        }
    }
}

/// An affine-free geographic ground plane. Product adapters retain their
/// geocoding and fact selection; the score maps already-disclosed coordinates
/// into scene units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Geographic {
    pub origin: Vec2,
    pub units_per_coordinate: f32,
    pub invert_y: bool,
}

impl Default for Geographic {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            units_per_coordinate: 1.0,
            invert_y: true,
        }
    }
}

/// A bounded nearest-site partition: every coordinate-placed item is a site,
/// and each site's cell is the part of `bounds` nearer to it than to any other
/// site. The solver emits one scene [`Region`](crate::Region) per site, with
/// the cell as its polygon contour.
///
/// The regions *tile* the bounds: they cover it, they do not overlap, and a
/// point's cell is decided by the nearest-site rule rather than drawn. That is
/// what distinguishes Hulls from a derived cluster halo, and it is the same
/// rule Mesocosm's `Places::at` runs inside the simulation, so a hulls scene
/// over its places matches simulation truth exactly instead of approximating
/// it. What a cell *means* (a lineage's range, a faction's reach, a fertility
/// field) stays with the vessel; the contract carries only geometry and the
/// member it belongs to.
///
/// Coordinates map through origin/scale like [`Geographic`], so a product
/// adapter discloses positions in its own units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hulls {
    pub origin: Vec2,
    pub units_per_coordinate: f32,
    pub invert_y: bool,
    /// The outer boundary the cells tile, in scene units. Cells are clipped to
    /// it, so the partition is total over exactly this much world.
    pub bounds: Rect,
}

impl Default for Hulls {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            units_per_coordinate: 1.0,
            invert_y: true,
            bounds: Rect::new(Vec2::new(-256.0, -256.0), Size2::new(512.0, 512.0)),
        }
    }
}

/// Optional placement data an arrangement understands. The source adapter
/// decides whether an item has an authored cell/location; the solver never
/// looks behind the opaque source reference.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum Placement {
    /// Use the item's ordinal in the arrangement's deterministic order.
    #[default]
    Ordinal,
    /// An authored cell for [`Board`].
    Cell { column: i32, row: i32 },
    /// A disclosed geographic or local coordinate for [`Geographic`].
    Coordinate(Vec2),
}

/// One measured source instance in a score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreItem {
    pub source: SourceRef,
    /// Lower ordinal is earlier in an arrangement. An adapter maps native
    /// recency or priority to this scalar before persistence.
    pub ordinal: u32,
    /// What the host measured at the selected LOD.
    pub footprint: Footprint,
    /// The selected realization rung. A new score may select another rung for
    /// the same source when zoom, focus, or host capability changes.
    pub representation: Representation,
    pub placement: Placement,
    pub layer: i16,
    pub visible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Size2;

    #[test]
    fn score_round_trips_through_serde() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.generation = 7;
        score.items.push(ScoreItem {
            source: SourceRef::new("fixture", "north"),
            ordinal: 0,
            footprint: Footprint::Rect {
                size: Size2::new(40.0, 24.0),
            },
            representation: Representation::Card,
            placement: Placement::Ordinal,
            layer: 2,
            visible: true,
        });
        let json = serde_json::to_string(&score).unwrap();
        assert_eq!(serde_json::from_str::<Score>(&json).unwrap(), score);
    }
}
