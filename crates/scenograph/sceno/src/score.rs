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
/// Version 4 absorbs the `arrangements` catalog: seven new [`Arrangement`]
/// families, the [`Arrangement::Custom`] escape, and the three per-item
/// disclosure fields they read ([`ScoreItem::axis`], [`ScoreItem::embedding`],
/// [`ScoreItem::weight`]). A version 3 reader must reject a version 4 score:
/// the families it does not know are not decorative, and an item whose
/// position depends on a disclosed axis value would be placed by ordinal
/// instead — silently, and somewhere else entirely.
///
/// Version 3 renames the regular-cell arrangement from `Board` to [`Grid`].
/// Version 2 added [`Score::holds`]. It differed from version 1 in nothing else,
/// but an adapter that only understands version 1 must reject a version 2
/// score rather than accept it, because the one thing it would drop is an
/// authored placement someone asked to be honored. A silently dropped pin is
/// the failure this field exists to prevent.
pub const SCORE_VERSION: u16 = 4;

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

/// The portable analytic arrangements.
///
/// Every family here is closed-form: a score in, placed instances out, no
/// iteration and no persistent solver state. Live force physics is `seiche`'s
/// domain and deliberately has no variant.
///
/// The named variants stay exhaustively matchable, which is what lets `solve`
/// prove it handles every family it claims to. [`Arrangement::Custom`] is the
/// escape for solvers registered at runtime — a consumer outside this
/// workspace can add an arrangement without a change here, at the cost of an
/// untyped config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Arrangement {
    Spiral(Spiral),
    Grid(Grid),
    Geographic(Geographic),
    Hulls(Hulls),
    Stack(Stack),
    Penrose(Penrose),
    LSystem(LSystem),
    Timeline(Timeline),
    Kanban(Kanban),
    Embedded(Embedded),
    Radial(Radial),
    /// A solver registered at runtime, named by id, configured opaquely.
    ///
    /// The id is matched against the `scenograph` solver registry. An
    /// unregistered id is a solve error, not a silent fallback: placing items
    /// by an arrangement nobody asked for is the same class of failure as
    /// dropping a pin.
    Custom {
        id: String,
        config: serde_json::Value,
    },
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

/// A layered stack. Layer index arrives per item on [`ScoreItem::axis`]; the
/// producer decides what a layer means (topological rank, dependency depth,
/// generation) and which direction it flows.
///
/// The flow choice — whether sources precede targets or the reverse — lives
/// with the producer that computed the ranks, not here. By the time a score
/// exists the ranks are already numbers, and reversing them is the producer's
/// arithmetic, not a placement policy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stack {
    /// Distance between successive layers.
    pub layer_gap: f32,
    /// Distance between items within one layer.
    pub row_gap: f32,
    pub center: Vec2,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            layer_gap: 180.0,
            row_gap: 110.0,
            center: Vec2::ZERO,
        }
    }
}

/// A Penrose aperiodic tiling; items take tiling vertices in ordinal order.
///
/// Assignment strategy is deliberately absent. The `arrangements` original
/// carried a five-variant `NodeAssignmentStrategy` (graphlet-aware,
/// domain-clustered, UDC-clustered, edge-affinity) of which four were
/// documented as falling back to the fifth and none were implemented. All five
/// describe an *ordering*, and [`ScoreItem::ordinal`] is already this
/// contract's ordering channel: a producer that wants graphlet-adjacent items
/// on adjacent vertices sorts them into adjacent ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Penrose {
    pub variant: PenroseVariant,
    pub subdivision_count: SubdivisionCount,
    pub unused_vertices: UnusedVertexPolicy,
    pub center: Vec2,
    /// World-unit scale of the initial tile ring.
    pub tile_scale: f32,
}

impl Default for Penrose {
    fn default() -> Self {
        Self {
            variant: PenroseVariant::default(),
            subdivision_count: SubdivisionCount::default(),
            unused_vertices: UnusedVertexPolicy::default(),
            center: Vec2::ZERO,
            tile_scale: 400.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PenroseVariant {
    /// P3 — thin + thick golden rhombi. Smoother, more uniform.
    #[default]
    Rhombus,
    /// P2 — kite + dart. Chunkier, more distinct local motifs.
    KiteDart,
}

/// How many deflation steps to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SubdivisionCount {
    /// Smallest depth such that vertex count ≥ item count.
    #[default]
    Auto,
    /// Explicit depth; useful for artistic control and regression tests.
    Explicit(u8),
}

/// What becomes of tiling vertices no item claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UnusedVertexPolicy {
    /// Unused vertices remain empty; the tiling's full extent is visible as
    /// gaps around placed items. Reveals aperiodic structure.
    #[default]
    LeaveEmpty,
    /// Clip the layout bounds to the convex hull of used vertices. Tighter
    /// result; hides tiling periphery.
    ClipToHull,
    /// Hide the tiling backdrop entirely where the visual cost is not worth it
    /// (mobile, low-power paths).
    HideTiling,
}

/// A fractal path walked by an L-system turtle; items take path positions in
/// ordinal order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LSystem {
    pub grammar: LSystemGrammar,
    pub iteration_depth: IterationDepth,
    /// Origin for the turtle's starting position.
    pub origin: Vec2,
    /// Bounding-box edge length in world units. The walked path is normalized
    /// to fit this extent.
    pub size: f32,
    /// Rotation of the whole path around `origin`, in radians.
    pub rotation: f32,
    /// Reverse the item-to-position assignment.
    pub reverse_order: bool,
}

impl Default for LSystem {
    fn default() -> Self {
        Self {
            grammar: LSystemGrammar::default(),
            iteration_depth: IterationDepth::default(),
            origin: Vec2::ZERO,
            size: 400.0,
            rotation: 0.0,
            reverse_order: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LSystemGrammar {
    #[default]
    Hilbert,
    Koch,
    Dragon,
    /// A grammar supplied by the host, named by id. Distinct from
    /// [`Arrangement::Custom`]: the arrangement is still this one, only the
    /// production rules come from elsewhere.
    Named(String),
}

/// Iteration depth selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IterationDepth {
    /// Smallest depth whose expansion yields at least as many positions as
    /// there are items.
    #[default]
    Auto,
    /// Explicit fractal depth; useful for deterministic comparisons or
    /// artistic control.
    Explicit(u8),
}

/// A numeric axis. Items place along it by [`ScoreItem::axis`]
/// ([`AxisValue::Numeric`]); items sharing a coordinate stack into rows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    /// World-space origin of the axis (its leftmost edge).
    pub origin: Vec2,
    /// World-unit length of the axis.
    pub axis_length: f32,
    /// Vertical spacing between rows when items share nearby coordinates.
    pub row_gap: f32,
    pub fallback: TimelineFallback,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            axis_length: 800.0,
            row_gap: 40.0,
            fallback: TimelineFallback::default(),
        }
    }
}

/// Treatment for items with no [`AxisValue::Numeric`] disclosure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TimelineFallback {
    /// Leave unassigned items where they are.
    #[default]
    LeaveInPlace,
    /// Stack unassigned items vertically below the axis origin.
    StackBelowOrigin,
    /// Stack unassigned items vertically past the axis end.
    StackPastEnd,
}

/// Categorical columns. Items place by [`ScoreItem::axis`]
/// ([`AxisValue::Categorical`]) into named buckets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kanban {
    pub origin: Vec2,
    /// Horizontal spacing between columns.
    pub column_gap: f32,
    /// Vertical spacing between entries within a column.
    pub row_gap: f32,
    /// Canonical ordering of columns, left to right. Items whose tag is absent
    /// here go to a trailing column.
    pub column_order: Vec<String>,
    /// Include the trailing column for unrecognized tags.
    pub include_other_column: bool,
}

impl Default for Kanban {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            column_gap: 240.0,
            row_gap: 80.0,
            column_order: Vec::new(),
            include_other_column: true,
        }
    }
}

/// Placement at coordinates someone else computed: the solver applies origin,
/// scale and rotation to each item's [`ScoreItem::embedding`] and places the
/// result.
///
/// Normalizing into a sane range is the producer's job, not the solver's. Only
/// the producer knows whether its coordinates are a unit square, a covariance
/// spread, or eigenvector components a few thousandths wide, so it is the only
/// side that can rescale without guessing.
///
/// One arrangement serves every producer of 2-D coordinates. A dimensionality
/// reduction (UMAP, t-SNE, PCA) and a spectral decomposition (the two smallest
/// non-trivial eigenvectors of a graph Laplacian) disagree about everything
/// except their output shape, and the output shape is all a placement needs.
/// These were two separate layouts in `arrangements`, in different files under
/// different names, differing only in a power-iteration count that was never a
/// placement parameter at all.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Embedded {
    pub origin: Vec2,
    /// World-unit scale applied to the disclosed coordinates, which producers
    /// typically supply in `[-1, 1]` or `[0, 1]`.
    pub scale: f32,
    /// Rotation applied to the scaled coordinates, in radians.
    pub rotation: f32,
    pub fallback: EmbeddingFallback,
}

impl Default for Embedded {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            scale: 400.0,
            rotation: 0.0,
            fallback: EmbeddingFallback::default(),
        }
    }
}

/// Treatment for items with no [`ScoreItem::embedding`] disclosure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EmbeddingFallback {
    /// Leave unembedded items where they are.
    #[default]
    LeaveInPlace,
    /// Place unembedded items at `origin`.
    CollapseToOrigin,
    /// Place unembedded items on a deterministic ring outside the embedded
    /// cluster, positions derived from a stable hash of the source reference.
    RingOutside,
}

/// Concentric rings. Ring index arrives per item on [`ScoreItem::axis`]
/// ([`AxisValue::Numeric`]); ring `n` sits at radius `n × ring_spacing`.
///
/// No focal item and no graph. The `arrangements` original walked a breadth
/// first search from a focus node, which is why its config was generic over a
/// node id — the only such generic in that crate. The walk is the producer's
/// work and its result is one number per item, so what reaches a score is the
/// ring, and the focus is simply whichever item the producer numbered zero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Radial {
    /// World-space position of ring zero.
    pub center: Vec2,
    /// Radial distance between successive rings.
    pub ring_spacing: f32,
    pub angular_policy: RadialAngularPolicy,
    /// Global rotation applied to every ring, in radians. Zero puts the first
    /// angular slot on the +x axis.
    pub rotation_offset: f32,
    pub unreachable_policy: RadialUnreachablePolicy,
}

impl Default for Radial {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            ring_spacing: 120.0,
            angular_policy: RadialAngularPolicy::default(),
            rotation_offset: 0.0,
            unreachable_policy: RadialUnreachablePolicy::default(),
        }
    }
}

/// How a ring distributes its items around the circle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RadialAngularPolicy {
    /// Every item on a ring gets an equal slot.
    #[default]
    Uniform,
    /// Slot width is proportional to [`ScoreItem::weight`], so a producer that
    /// discloses adjacency degree gets hub-and-satellite structure. Items with
    /// no weight are treated as uniform.
    Weighted,
    /// Stable order derived from a hash of the source reference —
    /// deterministic, and independent of both ordinal and weight.
    HashSorted,
}

/// Treatment for items the producer could not assign a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RadialUnreachablePolicy {
    /// Place them on an outer ring, one beyond the deepest assigned ring.
    #[default]
    OuterRing,
    /// Collapse them to `center`, hiding disconnected structure.
    Center,
    /// Leave them where they are.
    LeaveInPlace,
}

/// A per-item coordinate on an arrangement's axis.
///
/// Numeric for a continuous axis ([`Timeline`], [`Stack`]'s layer, [`Radial`]'s
/// ring); categorical for a bucketed one ([`Kanban`]'s column).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AxisValue {
    /// A position on a continuous axis, in the producer's own units.
    Numeric(f64),
    /// A bucket tag. Ordering across buckets comes from the arrangement's
    /// config, not from this value.
    Categorical(String),
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
    /// A coordinate the producer computed and disclosed, for arrangements that
    /// place along an axis ([`Timeline`], [`Kanban`], [`Stack`], [`Radial`]).
    ///
    /// This is the seam that keeps topology-derived layouts portable. A ring
    /// index, a topological rank and a timestamp are all one number per item by
    /// the time they matter to a placement; deriving them needs the source's
    /// native truth, and using them does not. The producer walks its own graph
    /// once and discloses the result, rather than serializing an adjacency list
    /// for a solver to walk again.
    #[serde(default)]
    pub axis: Option<AxisValue>,
    /// 2-D coordinates the producer computed, for [`Embedded`]. Any producer
    /// will do — a dimensionality reduction, a spectral decomposition, a hand
    /// placement exported from somewhere else.
    #[serde(default)]
    pub embedding: Option<Vec2>,
    /// A per-item scalar for arrangements that place proportionally, read today
    /// by [`RadialAngularPolicy::Weighted`]. Named to match
    /// [`RoutedRelation::weight`](crate::RoutedRelation::weight), which is the
    /// same idea on the scene side.
    #[serde(default)]
    pub weight: Option<f32>,
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
            axis: Some(AxisValue::Numeric(1861.0)),
            embedding: Some(Vec2::new(0.25, -0.5)),
            weight: Some(0.75),
        });
        let json = serde_json::to_string(&score).unwrap();
        assert_eq!(serde_json::from_str::<Score>(&json).unwrap(), score);
    }

    #[test]
    fn score_names_regular_cells_grid_on_the_wire() {
        let score = Score::new(Arrangement::Grid(Grid::default()));
        let json = serde_json::to_string(&score).unwrap();
        assert!(json.contains("\"Grid\""));
        assert!(!json.contains("\"Board\""));
    }

    #[test]
    fn score_stamps_v4() {
        assert_eq!(SCORE_VERSION, 4);
        assert_eq!(Score::new(Arrangement::Grid(Grid::default())).version, 4);
    }

    /// A v3 score predates the three disclosure fields entirely. It must still
    /// load — the fields are `#[serde(default)]` precisely so an older score
    /// stays readable — and every one of them must arrive absent rather than
    /// zeroed, because `Some(0.0)` and `None` mean different things to every
    /// arrangement that reads them.
    #[test]
    fn v3_score_loads_with_disclosure_fields_absent() {
        // Build the v3 wire by serializing a current score and removing exactly
        // what v3 did not have. Deriving the fixture rather than transcribing it
        // keeps this test honest about the real encoding: a rename anywhere in
        // `ScoreItem` changes both sides together, so what is asserted stays
        // "the three new fields are optional" and never decays into "this
        // hand-written blob still parses".
        let mut score = Score::new(Arrangement::Grid(Grid::default()));
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
            axis: Some(AxisValue::Numeric(1.0)),
            embedding: Some(Vec2::new(1.0, 1.0)),
            weight: Some(1.0),
        });

        let mut wire = serde_json::to_value(&score).unwrap();
        wire["version"] = serde_json::json!(3);
        let item = wire["items"][0].as_object_mut().unwrap();
        for field in ["axis", "embedding", "weight"] {
            assert!(
                item.remove(field).is_some(),
                "{field} must be on the v4 wire for this test to prove anything"
            );
        }

        let loaded: Score = serde_json::from_value(wire).expect("v3 score must still load");
        assert_eq!(loaded.version, 3);
        let item = &loaded.items[0];
        // Absent, not zeroed: `Some(0.0)` and `None` mean different things to
        // every arrangement that reads these.
        assert!(item.axis.is_none());
        assert!(item.embedding.is_none());
        assert!(item.weight.is_none());
        // The fields v3 did have must survive untouched.
        assert_eq!(item.source, SourceRef::new("fixture", "north"));
        assert_eq!(item.layer, 2);
    }

    /// Every family in the catalog has to survive the wire, including the
    /// untyped escape. A variant that serializes but cannot be read back is a
    /// score that silently loses its arrangement.
    #[test]
    fn every_arrangement_round_trips() {
        let arrangements = vec![
            Arrangement::Spiral(Spiral::default()),
            Arrangement::Grid(Grid::default()),
            Arrangement::Geographic(Geographic::default()),
            Arrangement::Hulls(Hulls::default()),
            Arrangement::Stack(Stack::default()),
            Arrangement::Penrose(Penrose::default()),
            Arrangement::LSystem(LSystem::default()),
            Arrangement::Timeline(Timeline::default()),
            Arrangement::Kanban(Kanban::default()),
            Arrangement::Embedded(Embedded::default()),
            Arrangement::Radial(Radial::default()),
            Arrangement::Custom {
                id: "isometry.campaign".to_string(),
                config: serde_json::json!({ "lanes": 3 }),
            },
        ];
        for arrangement in arrangements {
            let score = Score::new(arrangement.clone());
            let json = serde_json::to_string(&score).unwrap();
            let back: Score = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("{arrangement:?} failed to round trip: {e}"));
            assert_eq!(back.arrangement, arrangement);
        }
    }

    /// `AxisValue` carries two shapes that must stay distinguishable on the
    /// wire: a timeline coordinate and a kanban column tag are not the same
    /// disclosure, and an arrangement reading the wrong one places silently.
    #[test]
    fn axis_value_discriminates_numeric_from_categorical() {
        let numeric = AxisValue::Numeric(-3.5);
        let categorical = AxisValue::Categorical("in-review".to_string());
        for value in [numeric, categorical] {
            let json = serde_json::to_string(&value).unwrap();
            assert_eq!(serde_json::from_str::<AxisValue>(&json).unwrap(), value);
        }
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
