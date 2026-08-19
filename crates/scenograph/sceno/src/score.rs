//! Scores — persisted, product-free instructions for making a scene.
//!
//! A score names an arrangement and supplies the measured instances that the
//! arrangement places. It deliberately carries opaque [`SourceRef`]s rather
//! than source truth: a Mere graph adapter, an Isometry campaign adapter, and
//! a geographic fixture can serialize the same vocabulary without sharing a
//! model.

use serde::{Deserialize, Serialize};

use crate::{Footprint, Rect, Representation, Size2, SourceRef, Vec2};

/// The persisted-score wire version.
///
/// Version 3 renames the regular-cell arrangement from `Board` to [`Grid`].
/// Version 2 added [`Score::holds`]. It differed from version 1 in nothing else,
/// but an adapter that only understands version 1 must reject a version 2
/// score rather than accept it, because the one thing it would drop is an
/// authored placement someone asked to be honored. A silently dropped pin is
/// the failure this field exists to prevent.
pub const SCORE_VERSION: u16 = 3;

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
    /// Authored placements that outrank the arrangement, keyed by source.
    ///
    /// Sparse by construction: an unheld source has no entry, and the common
    /// case costs one empty vec. Keyed by [`SourceRef`] rather than by item
    /// index so a hold survives re-ordering, re-solving, and a changed
    /// authority, which is what lets a citation name a hold without shipping
    /// the score that contains it.
    #[serde(default)]
    pub holds: Vec<HeldPlacement>,
    /// Adapter-stamped input generation, copied into the realized scene.
    pub generation: u64,
}

impl Score {
    pub fn new(arrangement: Arrangement) -> Self {
        Self {
            version: SCORE_VERSION,
            arrangement,
            items: Vec::new(),
            holds: Vec::new(),
            generation: 0,
        }
    }

    /// The hold authored for `source`, if any. First entry wins; a score with
    /// two holds on one source is malformed and the later one is ignored.
    pub fn hold_for(&self, source: &SourceRef) -> Option<&HeldPlacement> {
        self.holds.iter().find(|held| &held.source == source)
    }
}

/// How firmly an authored placement must be honored.
///
/// The two classes are deliberately unequal, and naming them is the point: a
/// solver that treats both as suggestions produces the silent-soft failure
/// where a person pins something, the layout quietly moves it, and nothing
/// anywhere says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hold {
    /// Best effort. The arrangement seeds from here and relaxation may carry
    /// it away; moving an anchored item is correct behaviour, not a failure.
    Anchored,
    /// Must be honored. The arrangement does not get a vote, and a solver that
    /// cannot honor it reports rather than repositions.
    Pinned,
}

/// One authored placement, outranking whatever the arrangement would choose.
///
/// This record is also the unit a scene citation carries as its placement
/// delta: one serialization of a pin, not two.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeldPlacement {
    /// Which source this holds. Not an item index: indices move.
    pub source: SourceRef,
    /// Where, in the same coordinate space the arrangement places into.
    pub at: Vec2,
    pub hold: Hold,
}

/// An authored placement the solver *did* honor, bound to the instance that
/// received it.
///
/// The negative half of this record lives on [`crate::Scene::unmet_holds`]. The
/// positive half is here for two reasons that arrived together: a consumer that
/// wants to state "3 pins honored" should read it rather than recompute it, and
/// a scene that knows which of its instances are ensure-class can stop a
/// relaxation pass from quietly dragging one away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HonoredHold {
    /// Which placed instance received the hold.
    pub instance: crate::InstanceId,
    pub placement: HeldPlacement,
}

impl HeldPlacement {
    pub fn pinned(source: SourceRef, at: Vec2) -> Self {
        Self {
            source,
            at,
            hold: Hold::Pinned,
        }
    }

    pub fn anchored(source: SourceRef, at: Vec2) -> Self {
        Self {
            source,
            at,
            hold: Hold::Anchored,
        }
    }
}

/// The portable analytic arrangements exercised by the first proofs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Arrangement {
    Spiral(Spiral),
    Grid(Grid),
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

/// A regular cell grid. Explicit cells preserve authored grid coordinates;
/// items without one flow left-to-right from their ordinal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub origin: Vec2,
    pub cell: Vec2,
    pub columns: u32,
    pub gap: f32,
}

impl Default for Grid {
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
    /// An authored cell for [`Grid`].
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

    #[test]
    fn score_v3_names_regular_cells_grid_on_the_wire() {
        let score = Score::new(Arrangement::Grid(Grid::default()));
        let json = serde_json::to_string(&score).unwrap();
        assert_eq!(score.version, 3);
        assert!(json.contains("\"Grid\""));
        assert!(!json.contains("\"Board\""));
    }

    #[test]
    fn holds_round_trip_and_resolve_by_source() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        let pinned = SourceRef::new("fixture", "north");
        score
            .holds
            .push(HeldPlacement::pinned(pinned.clone(), Vec2::new(12.0, -4.0)));
        score.holds.push(HeldPlacement::anchored(
            SourceRef::new("fixture", "south"),
            Vec2::new(0.0, 9.0),
        ));

        let json = serde_json::to_string(&score).unwrap();
        assert_eq!(serde_json::from_str::<Score>(&json).unwrap(), score);

        let held = score.hold_for(&pinned).expect("north is held");
        assert_eq!(held.hold, Hold::Pinned);
        assert_eq!(held.at, Vec2::new(12.0, -4.0));
        assert!(score.hold_for(&SourceRef::new("fixture", "east")).is_none());
        // Same id, different adapter, is a different source.
        assert!(score.hold_for(&SourceRef::new("other", "north")).is_none());
    }

    #[test]
    fn a_version_1_score_still_reads_with_no_holds() {
        // The wire before holds existed. It must load, and it must load as
        // "nothing was held", never as "holds unknown".
        let json = r#"{
            "version": 1,
            "arrangement": {"Spiral": {"center": {"x": 0.0, "y": 0.0},
                "spacing": 40.0, "angle_radians": 2.399963, "curve": "SquareRoot"}},
            "items": [],
            "generation": 3
        }"#;
        let score: Score = serde_json::from_str(json).unwrap();
        assert_eq!(score.version, 1);
        assert_eq!(score.generation, 3);
        assert!(score.holds.is_empty());
    }
}
