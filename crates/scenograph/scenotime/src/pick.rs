//! Picking — turning a world-space point into the instance under it.
//!
//! This is the resolution half of the contract's gesture story. Sceno carries
//! the shapes ([`sceno::Footprint`], and `ProjectedItem::hit` when an item's
//! clickable area differs from the extent solvers clear); scenotime knows
//! which slots are live in this epoch and in what order they paint. A
//! consuming protocol turns the resulting [`InstanceId`] into an intent.
//!
//! The default exists because the viewer that most needs it has the least to
//! work with. A host that owns the source data usually has a better spatial
//! index already (mere resolves against its physics colliders), but a remote
//! client holding only a snapshot has no world to ask, and every such client
//! would otherwise reimplement this. Hosts with their own index should keep
//! using it; this is a floor, not a mandate.
//!
//! Cost is linear in live items. That is honest for the scene sizes the
//! contract targets and for a per-click query; a client with a large scene
//! and a hot pick path should build an index over the same tables rather
//! than call this in a loop.
//!
//! Backdrops are structurally pointer-transparent. A selectable map feature
//! is an item over a backdrop, preserving the protocol's `InstanceId` intent
//! path without giving environment paint a second, competing identity lane.

use sceno::{Footprint, InstanceId, ProjectedItem, SpaceId, Transform2, Vec2};

use crate::snapshot::{SceneSnapshot, SceneTables};

impl SceneSnapshot {
    /// The topmost live instance whose shape contains `world`, or `None`.
    ///
    /// See [`SceneTables::pick`]; this is the snapshot-level convenience.
    pub fn pick(&self, world: Vec2) -> Option<InstanceId> {
        self.tables.pick(world)
    }
}

impl SceneTables {
    /// The topmost live instance whose shape contains `world`, or `None`.
    ///
    /// Topmost means last painted: highest [`ProjectedItem::layer`] first,
    /// then latest explicit order, then highest stable slot. Invisible items
    /// and tombstoned slots never pick. An item's `hit` shape is used when
    /// present, otherwise its footprint, so an item drawn large can be
    /// clickable small and the reverse.
    ///
    /// A miss returns `None` rather than a nearest match: this answers "what
    /// is under the pointer", not "what did they probably mean".
    pub fn pick(&self, world: Vec2) -> Option<InstanceId> {
        let mut candidates: Vec<(i16, i32, u32)> = Vec::new();
        for (index, slot) in self.items.iter().enumerate() {
            let Some(item) = slot else { continue };
            if !item.visible {
                continue;
            }
            let Some(order) = self.item_order.get(index).copied().flatten() else {
                continue;
            };
            if self.item_contains(item, world) {
                candidates.push((item.layer, order, index as u32));
            }
        }
        candidates
            .into_iter()
            .max_by_key(|(layer, order, index)| (*layer, *order, *index))
            .map(|(_, _, index)| InstanceId(index))
    }

    /// Whether `world` falls inside one item's hit shape, carried back
    /// through the item's transform and its space chain.
    fn item_contains(&self, item: &ProjectedItem, world: Vec2) -> bool {
        let shape: &Footprint = item.hit.as_ref().unwrap_or(&item.footprint);
        let Some(space_to_world) = self.space_to_world(item.space) else {
            return false;
        };
        let Some(inverse) = space_to_world.then(&item.transform).inverse() else {
            return false;
        };
        shape.contains(inverse.apply(world))
    }

    /// Compose a space's transform chain to world. `None` on a dangling
    /// parent, a tombstoned ancestor, or a cycle, all of which
    /// [`SceneSnapshot::validate`] rejects at the boundary.
    pub fn space_to_world(&self, space: SpaceId) -> Option<Transform2> {
        let mut acc = Transform2::IDENTITY;
        let mut current = space;
        for _ in 0..=self.spaces.len() {
            let s = self.spaces.get(current.0 as usize)?.as_ref()?;
            acc = s.transform.then(&acc);
            match s.parent {
                Some(parent) => current = parent,
                None => return Some(acc),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use sceno::{Backdrop, Representation, Scene, SourceRef};

    use super::*;
    use crate::{Revision, SceneEpoch};

    fn card(scene: &mut Scene, id: &str, at: Vec2, layer: i16) -> InstanceId {
        let source = scene.intern_source(SourceRef::new("fixture", id));
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(at.x, at.y),
            footprint: Footprint::Rect {
                size: sceno::Size2::new(20.0, 20.0),
            },
            representation: Representation::Card,
            layer,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        InstanceId((scene.items.len() - 1) as u32)
    }

    fn snapshot(scene: Scene) -> SceneSnapshot {
        SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), scene).unwrap()
    }

    #[test]
    fn a_point_inside_one_card_picks_it() {
        let mut scene = Scene::new();
        let a = card(&mut scene, "a", Vec2::new(100.0, 100.0), 0);
        let snap = snapshot(scene);
        assert_eq!(snap.pick(Vec2::new(105.0, 95.0)), Some(a));
    }

    #[test]
    fn a_miss_returns_none_not_the_nearest() {
        let mut scene = Scene::new();
        card(&mut scene, "a", Vec2::new(100.0, 100.0), 0);
        let snap = snapshot(scene);
        assert_eq!(snap.pick(Vec2::new(400.0, 400.0)), None);
    }

    #[test]
    fn overlapping_items_pick_the_higher_layer() {
        let mut scene = Scene::new();
        let _under = card(&mut scene, "under", Vec2::ZERO, 0);
        let over = card(&mut scene, "over", Vec2::ZERO, 5);
        let snap = snapshot(scene);
        assert_eq!(snap.pick(Vec2::ZERO), Some(over));
    }

    #[test]
    fn within_one_layer_the_later_item_wins() {
        let mut scene = Scene::new();
        let _first = card(&mut scene, "first", Vec2::ZERO, 0);
        let second = card(&mut scene, "second", Vec2::ZERO, 0);
        let snap = snapshot(scene);
        assert_eq!(
            snap.pick(Vec2::ZERO),
            Some(second),
            "equal layers fall back to paint order"
        );
    }

    #[test]
    fn an_invisible_item_never_picks() {
        let mut scene = Scene::new();
        let _hidden = card(&mut scene, "hidden", Vec2::ZERO, 9);
        scene.items[0].visible = false;
        let under = card(&mut scene, "under", Vec2::ZERO, 0);
        let snap = snapshot(scene);
        assert_eq!(snap.pick(Vec2::ZERO), Some(under));
    }

    #[test]
    fn a_backdrop_never_intercepts_item_picking() {
        let mut scene = Scene::new();
        let backdrop_source = scene.intern_source(SourceRef::new("fixture", "floor"));
        scene.backdrops.push(Backdrop {
            source: backdrop_source,
            space: Scene::WORLD,
            transform: Transform2::IDENTITY,
            footprint: Footprint::Rect {
                size: sceno::Size2::new(200.0, 200.0),
            },
            kind: "fixture:floor".into(),
            visible: true,
            collidable: false,
        });
        let item = card(&mut scene, "item", Vec2::ZERO, 0);
        let snap = snapshot(scene);
        assert_eq!(snap.pick(Vec2::ZERO), Some(item));
    }

    #[test]
    fn a_hit_override_narrower_than_the_footprint_is_respected() {
        let mut scene = Scene::new();
        card(&mut scene, "a", Vec2::ZERO, 0);
        scene.items[0].hit = Some(Footprint::Rect {
            size: sceno::Size2::new(4.0, 4.0),
        });
        let snap = snapshot(scene);
        assert_eq!(snap.pick(Vec2::new(1.0, 1.0)), Some(InstanceId(0)));
        assert_eq!(
            snap.pick(Vec2::new(8.0, 8.0)),
            None,
            "inside the drawn footprint, outside the declared hit shape"
        );
    }

    #[test]
    fn a_point_footprint_is_unpickable_without_a_hit_shape() {
        let mut scene = Scene::new();
        card(&mut scene, "a", Vec2::ZERO, 0);
        scene.items[0].footprint = Footprint::Point;
        let snap = snapshot(scene);
        assert_eq!(snap.pick(Vec2::ZERO), None);
    }

    #[test]
    fn a_nested_space_picks_through_its_transform_chain() {
        let mut scene = Scene::new();
        let group = scene.push_space(Scene::WORLD, Transform2::translation(500.0, 0.0), None);
        let nested = scene.push_space(group, Transform2::translation(0.0, 300.0), None);
        let inside = card(&mut scene, "inside", Vec2::new(10.0, 10.0), 0);
        scene.items[0].space = nested;
        let snap = snapshot(scene);
        // Local (10, 10) under +500x then +300y lands at (510, 310).
        assert_eq!(snap.pick(Vec2::new(512.0, 308.0)), Some(inside));
        assert_eq!(snap.pick(Vec2::new(12.0, 8.0)), None);
    }

    #[test]
    fn a_scaled_space_scales_the_pickable_area() {
        let mut scene = Scene::new();
        let zoomed = scene.push_space(
            Scene::WORLD,
            Transform2 {
                translate: Vec2::ZERO,
                scale: 4.0,
                rotate: 0.0,
            },
            None,
        );
        let big = card(&mut scene, "big", Vec2::ZERO, 0);
        scene.items[0].space = zoomed;
        let snap = snapshot(scene);
        // The 20x20 card covers +-40 in world once the space scales it.
        assert_eq!(snap.pick(Vec2::new(35.0, 0.0)), Some(big));
        assert_eq!(snap.pick(Vec2::new(45.0, 0.0)), None);
    }

    #[test]
    fn a_tombstoned_slot_never_picks() {
        let mut scene = Scene::new();
        card(&mut scene, "a", Vec2::ZERO, 0);
        let mut snap = snapshot(scene);
        snap.tables.items[0] = None;
        snap.tables.item_order[0] = None;
        assert_eq!(snap.pick(Vec2::ZERO), None);
    }
}
