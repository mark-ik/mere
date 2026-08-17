//! The projection scene — the output side of the contract.
//!
//! Identity is an index (the data-oriented doctrine): sources, spaces,
//! items, relations, and regions are dense vectors; ids are indexes into
//! them. The source/instance separation is structural: one [`SourceRef`]
//! entry may be pointed at by many [`ProjectedItem`]s, which is how one
//! phrase appears in the loop table, the history branch, and the similarity
//! map without duplicating truth.
//!
//! The representation measures content; the projection places it. A scene
//! carries a representation *slot* and an assigned extent per item, never
//! rendered content.

use serde::{Deserialize, Serialize};

use crate::footprint::Footprint;
use crate::geometry::{Rect, Transform2};

/// A stable reference into a source's own truth. `adapter` names the source
/// lane (e.g. `"mere.graph"`, `"isometry.overmap"`); `id` is the source's
/// stable identity for the object, in the source's own scheme. The engine
/// never interprets `id`; it only carries it back in intents.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceRef {
    pub adapter: String,
    pub id: String,
}

impl SourceRef {
    pub fn new(adapter: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            adapter: adapter.into(),
            id: id.into(),
        }
    }
}

/// Index into [`Scene::sources`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceIx(pub u32);

/// Index into [`Scene::spaces`]. `SpaceId(0)` is the world space by
/// convention (see [`Scene::WORLD`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpaceId(pub u32);

/// Index into [`Scene::items`] — the *instance* identity, distinct from the
/// source identity by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(pub u32);

/// A coordinate space: a transform within an optional parent. Frames, hull
/// groups, a geographic projection's ground plane, and a nested map are all
/// spaces; dragging a group is one transform edit here, not N item edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    /// `None` only for the world space (index 0).
    pub parent: Option<SpaceId>,
    pub transform: Transform2,
    /// Optional authoring label ("hull: rust papers", "fretboard").
    pub name: Option<String>,
}

/// The representation slot an item asks its host to fill. Recognized core
/// is the LOD ladder; the open tail carries anything a host recognizes by
/// name. The scene never carries rendered content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Representation {
    /// Smallest rung: a themed dot/icon.
    Glyph,
    /// A compact summary card.
    Card,
    /// An image/sprite face.
    Sprite,
    /// A static capture of live content.
    Snapshot,
    /// Live, interactive content (a web session, an editor).
    LivePane,
    /// Open tail: host-recognized representation by name.
    Open { kind: String },
}

/// One placed instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectedItem {
    pub source: SourceIx,
    pub space: SpaceId,
    /// Placement within `space`: the local origin (and any rotation/scale
    /// the solver assigned).
    pub transform: Transform2,
    /// The extent the item occupies in its local space (what solvers
    /// clear; what hit-testing falls back to).
    pub footprint: Footprint,
    pub representation: Representation,
    /// Paint order within the scene (higher paints later/on top).
    pub layer: i16,
    pub visible: bool,
    /// Hit-test shape override; `None` = use `footprint`.
    pub hit: Option<Footprint>,
    /// Per-item emphasis, as an open map of named scalars (conventionally
    /// `0..=1`). Recognized names carry no special meaning here; a host
    /// renders the ones it knows and ignores the rest, the same
    /// recognized-core-plus-open-tail shape [`Representation`] uses.
    ///
    /// Emphasis belongs *in the scene* because a scene may be realized by a
    /// viewer with no access to the source: a remote client cannot read a
    /// host-side signal to shade a node it only knows as an instance. Names
    /// in use today are `"heat"` (activity or freshness) and `"bridge"`
    /// (community-spanning emphasis); both come from mere's cartography
    /// overlays.
    ///
    /// Relations deliberately have no equivalent. An item carries several
    /// simultaneous emphases, but a pair of instances related for several
    /// reasons is several relations, not one relation with a channel list.
    pub channels: Vec<(String, f32)>,
}

/// A routed relationship between two instances: a visible stroke, a route,
/// a shared border — the projection decides. Points are a full polyline in
/// `space` (including endpoints), so routed and straight edges are one
/// shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutedRelation {
    pub from: InstanceId,
    pub to: InstanceId,
    pub space: SpaceId,
    pub points: Vec<crate::geometry::Vec2>,
    /// Open relation kind (IRI or short name); `None` = unspecified.
    pub kind: Option<String>,
    /// Emphasis weight in `0..=1` (the cartography `EdgeWeight` overlay
    /// migrates here).
    pub weight: Option<f32>,
}

/// A region over member instances: a drawn hull, a derived cluster halo, a
/// map zone. Regions absorb the cartography `ClusterHalo` overlay and are
/// the scene face of the hulls lane (group + region compose with a space
/// for the draggable-hull gesture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub members: Vec<InstanceId>,
    /// The region's own space (a hull group's frame lives here).
    pub space: SpaceId,
    /// Contour in `space`; `None` = derived (host draws a hull around the
    /// members' footprints).
    pub contour: Option<Footprint>,
    pub label: Option<String>,
    /// Derivation confidence in `0..=1`; `1.0` for authored regions.
    pub confidence: f32,
}

/// A complete projected scene: the output of choreography, the thing
/// scenotime diffs, the input a host realizes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Scene {
    pub sources: Vec<SourceRef>,
    /// `spaces[0]` is the world space (no parent, identity transform, by
    /// construction via [`Scene::new`]).
    pub spaces: Vec<Space>,
    pub items: Vec<ProjectedItem>,
    pub relations: Vec<RoutedRelation>,
    pub regions: Vec<Region>,
    /// Content bounds in world space (what a camera frames).
    pub bounds: Rect,
    /// The generation of inputs this scene was computed from (score +
    /// signal generations; stamped by the runtime).
    pub generation: u64,
    /// Ensure-class placements the solver could not honor, carried with the
    /// coordinate that was asked for.
    ///
    /// Empty is the common and correct case. A non-empty entry is the scene
    /// saying so out loud instead of quietly placing the item somewhere else,
    /// which is the silent-soft failure this record exists to prevent. It
    /// carries [`HeldPlacement`] rather than a bare reference so a viewer with
    /// no access to the score can still report what was requested and where.
    ///
    /// Encourage-class holds never appear here: an anchored home that drifts is
    /// working as designed.
    #[serde(default)]
    pub unmet_holds: Vec<crate::HeldPlacement>,
}

impl Scene {
    /// The world space id.
    pub const WORLD: SpaceId = SpaceId(0);

    /// An empty scene with the world space installed.
    pub fn new() -> Self {
        Self {
            spaces: vec![Space {
                parent: None,
                transform: Transform2::IDENTITY,
                name: Some("world".into()),
            }],
            ..Default::default()
        }
    }

    /// Intern a source ref, returning its index (existing entry reused).
    pub fn intern_source(&mut self, source: SourceRef) -> SourceIx {
        if let Some(i) = self.sources.iter().position(|s| *s == source) {
            return SourceIx(i as u32);
        }
        self.sources.push(source);
        SourceIx((self.sources.len() - 1) as u32)
    }

    /// Add a space under `parent`, returning its id.
    pub fn push_space(
        &mut self,
        parent: SpaceId,
        transform: Transform2,
        name: Option<String>,
    ) -> SpaceId {
        self.spaces.push(Space {
            parent: Some(parent),
            transform,
            name,
        });
        SpaceId((self.spaces.len() - 1) as u32)
    }

    /// Resolve a space's transform to world by composing its parent chain.
    /// Returns `None` on a dangling parent or a cycle (both are contract
    /// violations a debug host should surface loudly).
    pub fn to_world(&self, space: SpaceId) -> Option<Transform2> {
        let mut acc = Transform2::IDENTITY;
        let mut current = space;
        // A well-formed chain is shorter than the space count; more hops
        // than that proves a cycle.
        for _ in 0..=self.spaces.len() {
            let s = self.spaces.get(current.0 as usize)?;
            acc = s.transform.then(&acc);
            match s.parent {
                Some(p) => current = p,
                None => return Some(acc),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Vec2;

    #[test]
    fn one_source_many_instances() {
        let mut scene = Scene::new();
        let phrase = scene.intern_source(SourceRef::new("hocket.session", "phrase:7"));
        let again = scene.intern_source(SourceRef::new("hocket.session", "phrase:7"));
        assert_eq!(phrase, again, "interning dedups");
        for _ in 0..2 {
            scene.items.push(ProjectedItem {
                source: phrase,
                space: Scene::WORLD,
                transform: Transform2::IDENTITY,
                footprint: Footprint::Point,
                representation: Representation::Glyph,
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });
        }
        assert_eq!(scene.sources.len(), 1);
        assert_eq!(scene.items.len(), 2, "one source, two instances");
    }

    #[test]
    fn space_chain_resolves_to_world() {
        let mut scene = Scene::new();
        let hull = scene.push_space(Scene::WORLD, Transform2::translation(100.0, 0.0), None);
        let nested = scene.push_space(hull, Transform2::translation(0.0, 50.0), None);
        let t = scene.to_world(nested).unwrap();
        let p = t.apply(Vec2::ZERO);
        assert_eq!((p.x, p.y), (100.0, 50.0));
    }

    #[test]
    fn space_cycle_is_detected() {
        let mut scene = Scene::new();
        let a = scene.push_space(Scene::WORLD, Transform2::IDENTITY, None);
        let b = scene.push_space(a, Transform2::IDENTITY, None);
        // Corrupt: make `a` a child of `b`.
        scene.spaces[a.0 as usize].parent = Some(b);
        assert_eq!(
            scene.to_world(b),
            None,
            "cycle resolves to None, not a hang"
        );
    }

    #[test]
    fn scene_round_trips_through_serde() {
        let mut scene = Scene::new();
        let src = scene.intern_source(SourceRef::new("mere.graph", "uuid:abc"));
        scene.items.push(ProjectedItem {
            source: src,
            space: Scene::WORLD,
            transform: Transform2::translation(3.0, 4.0),
            footprint: Footprint::Rect {
                size: crate::geometry::Size2::new(40.0, 40.0),
            },
            representation: Representation::Card,
            layer: 1,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        scene.relations.push(RoutedRelation {
            from: InstanceId(0),
            to: InstanceId(0),
            space: Scene::WORLD,
            points: vec![Vec2::ZERO, Vec2::new(3.0, 4.0)],
            kind: Some("traversal".into()),
            weight: Some(0.5),
        });
        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        assert_eq!(scene, back);
    }
}
