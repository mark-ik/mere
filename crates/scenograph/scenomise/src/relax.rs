//! Relaxation — physics as a capability of *any* scene, not of one big canvas.
//!
//! A full rigid-body sim is the right tool for a room-sized graph canvas, but
//! it is far too much machinery for a swatch: a card-sized graph in a side
//! panel, an overmap, a set list. Those surfaces still deserve the capability —
//! a graph should be able to loosen up and settle wherever it is drawn, not
//! only in the one surface that happens to own a rapier world.
//!
//! So relaxation is deliberately dependency-free and deterministic: repulsion
//! between placed items, springs along routed relations, and a pull back toward
//! the arrangement's own slots. That last term is the same idea as an anchor
//! spring — the arrangement participates rather than dictating — so a swatch
//! reads with the identical vocabulary as the canvas, at a fraction of the cost.
//!
//! Cost is `O(steps · n²)` in placed items, which is the right trade at swatch
//! scale (tens of nodes) and the wrong one at canvas scale (thousands). A
//! surface with a real sim should keep using it.

use sceno::{Footprint, InstanceId, Rect, Scene, Transform2, Vec2};

/// How a scene loosens up. All terms are optional: zero any of them out and it
/// simply stops contributing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Relaxation {
    /// Iterations to run. `0` leaves the scene untouched.
    pub steps: u32,
    /// Push between every pair of placed items, scaled by their separation.
    pub repulsion: f32,
    /// Pull along each routed relation toward `rest_length`.
    pub spring: f32,
    /// The separation a related pair settles toward.
    pub rest_length: f32,
    /// Pull back toward where the arrangement placed each item. High holds the
    /// arrangement's shape; `0.0` lets the graph's own forces win entirely,
    /// making the arrangement a pure initial condition.
    pub arrangement_pull: f32,
    /// Velocity retained per step (`0.0`..=`1.0`).
    pub damping: f32,
    /// Step size.
    pub dt: f32,
}

impl Default for Relaxation {
    fn default() -> Self {
        Self {
            steps: 60,
            repulsion: 900.0,
            spring: 0.6,
            rest_length: 40.0,
            arrangement_pull: 0.25,
            damping: 0.82,
            dt: 0.1,
        }
    }
}

impl Relaxation {
    /// A pure loosening: the arrangement seeds the scene and then stops acting.
    pub fn untethered(mut self) -> Self {
        self.arrangement_pull = 0.0;
        self
    }
}

/// The separation an item wants kept clear around its centre.
fn item_clearance(footprint: &Footprint) -> f32 {
    footprint
        .bounds()
        .map(|b| b.size.w.max(b.size.h) * 0.5)
        .unwrap_or(0.0)
}

/// Conservative axis-aligned bounds for a placed footprint. Rotation is
/// retained by transforming all four local bounds corners.
fn placed_bounds(footprint: &Footprint, transform: Transform2) -> Option<Rect> {
    let local = footprint.bounds()?;
    let min = local.origin;
    let max = Vec2::new(local.origin.x + local.size.w, local.origin.y + local.size.h);
    let corners = [
        transform.apply(min),
        transform.apply(Vec2::new(max.x, min.y)),
        transform.apply(max),
        transform.apply(Vec2::new(min.x, max.y)),
    ];
    let (mut min_x, mut min_y, mut max_x, mut max_y) =
        (corners[0].x, corners[0].y, corners[0].x, corners[0].y);
    for corner in &corners[1..] {
        min_x = min_x.min(corner.x);
        min_y = min_y.min(corner.y);
        max_x = max_x.max(corner.x);
        max_y = max_y.max(corner.y);
    }
    Some(Rect::new(
        Vec2::new(min_x, min_y),
        sceno::Size2::new(max_x - min_x, max_y - min_y),
    ))
}

/// Smallest deterministic translation that carries `item` outside `obstacle`.
fn separating_shift(item: Rect, obstacle: Rect) -> Option<Vec2> {
    let item_max = Vec2::new(item.origin.x + item.size.w, item.origin.y + item.size.h);
    let obstacle_max = Vec2::new(
        obstacle.origin.x + obstacle.size.w,
        obstacle.origin.y + obstacle.size.h,
    );
    if item_max.x <= obstacle.origin.x
        || obstacle_max.x <= item.origin.x
        || item_max.y <= obstacle.origin.y
        || obstacle_max.y <= item.origin.y
    {
        return None;
    }
    let candidates = [
        Vec2::new(obstacle.origin.x - item_max.x, 0.0),
        Vec2::new(obstacle_max.x - item.origin.x, 0.0),
        Vec2::new(0.0, obstacle.origin.y - item_max.y),
        Vec2::new(0.0, obstacle_max.y - item.origin.y),
    ];
    candidates.into_iter().min_by(|left, right| {
        let left_len = left.x.abs() + left.y.abs();
        let right_len = right.x.abs() + right.y.abs();
        left_len.total_cmp(&right_len)
    })
}

/// Loosen `scene` in place: items push apart, related items pull together, and
/// each item is drawn back toward the slot its arrangement chose.
///
/// Deterministic — no randomness, so the same scene and settings always relax
/// the same way and a receipt can be compared frame to frame.
pub fn relax(scene: &mut Scene, settings: &Relaxation) {
    relax_holding(scene, settings, &[]);
}

/// Loosen `scene`, leaving `immovable` where it stands.
///
/// An immovable item still pushes and pulls its neighbours; it simply does not
/// integrate. That asymmetry is the whole meaning of a hard hold: the pin
/// wins, and the rest of the scene accommodates it, rather than the pin being
/// averaged away into a position nobody asked for.
///
/// Anchored holds are deliberately absent here. Anchored means best effort, so
/// it relaxes like anything else and the arrangement pull carries it home.
pub fn relax_holding(scene: &mut Scene, settings: &Relaxation, immovable: &[InstanceId]) {
    if settings.steps == 0 || scene.items.is_empty() {
        return;
    }
    let mut held = vec![false; scene.items.len()];
    // A scene that records its own honored pins does not need a caller to
    // remember them. This is the difference between an invariant and a
    // convention: before it, calling `relax` instead of `relax_holding` dragged
    // an ensure-class placement away in silence, and nothing in the types said
    // so. The explicit list still adds to this; it never subtracts.
    for honored in &scene.honored_holds {
        if let Some(slot) = held.get_mut(honored.instance.0 as usize) {
            *slot = true;
        }
    }
    for instance in immovable {
        if let Some(slot) = held.get_mut(instance.0 as usize) {
            *slot = true;
        }
    }
    let anchors: Vec<Vec2> = scene
        .items
        .iter()
        .map(|item| item.transform.translate)
        .collect();
    let clearances: Vec<f32> = scene
        .items
        .iter()
        .map(|item| item_clearance(&item.footprint))
        .collect();
    let mut velocities = vec![Vec2::ZERO; scene.items.len()];

    for _ in 0..settings.steps {
        let positions: Vec<Vec2> = scene
            .items
            .iter()
            .map(|item| item.transform.translate)
            .collect();
        let mut forces = vec![Vec2::ZERO; positions.len()];

        // Pairwise push. Items that overlap by footprint push hardest, so the
        // measured extents that keep an arrangement legible keep it legible
        // here too.
        if settings.repulsion > 0.0 {
            for a in 0..positions.len() {
                for b in (a + 1)..positions.len() {
                    let delta = Vec2::new(
                        positions[b].x - positions[a].x,
                        positions[b].y - positions[a].y,
                    );
                    let touching = (clearances[a] + clearances[b]).max(1.0);
                    let distance = (delta.x * delta.x + delta.y * delta.y).sqrt();
                    if distance < 1e-4 {
                        // Coincident: separate along a fixed axis rather than a
                        // random one, so the result stays reproducible.
                        forces[a].x -= settings.repulsion;
                        forces[b].x += settings.repulsion;
                        continue;
                    }
                    let push = settings.repulsion * touching / (distance * distance);
                    let (ux, uy) = (delta.x / distance, delta.y / distance);
                    forces[a].x -= ux * push;
                    forces[a].y -= uy * push;
                    forces[b].x += ux * push;
                    forces[b].y += uy * push;
                }
            }

            // Backdrops are static environment geometry. A collidable one
            // excludes item placement in its own coordinate space, while its
            // visibility has no bearing on collision. The solver deliberately
            // leaves cross-space collision to hosts with a full physics world;
            // this small relaxer already assumes its item forces share a space.
            for (index, item) in scene.items.iter().enumerate() {
                let Some(item_bounds) = placed_bounds(&item.footprint, item.transform) else {
                    continue;
                };
                for backdrop in scene
                    .backdrops
                    .iter()
                    .filter(|backdrop| backdrop.collidable && backdrop.space == item.space)
                {
                    let Some(obstacle) = placed_bounds(&backdrop.footprint, backdrop.transform)
                    else {
                        continue;
                    };
                    let Some(shift) = separating_shift(item_bounds, obstacle) else {
                        continue;
                    };
                    let extent = obstacle.size.w.max(obstacle.size.h).max(1.0);
                    let strength = settings.repulsion / extent;
                    forces[index].x += shift.x * strength;
                    forces[index].y += shift.y * strength;
                }
            }
        }

        // Springs along the scene's own relations.
        if settings.spring > 0.0 {
            for relation in &scene.relations {
                let (a, b) = (relation.from.0 as usize, relation.to.0 as usize);
                if a == b || a >= positions.len() || b >= positions.len() {
                    continue;
                }
                let delta = Vec2::new(
                    positions[b].x - positions[a].x,
                    positions[b].y - positions[a].y,
                );
                let distance = (delta.x * delta.x + delta.y * delta.y).sqrt();
                if distance < 1e-4 {
                    continue;
                }
                let weight = relation.weight.unwrap_or(1.0).clamp(0.0, 1.0).max(0.05);
                let pull = settings.spring * weight * (distance - settings.rest_length);
                let (ux, uy) = (delta.x / distance, delta.y / distance);
                forces[a].x += ux * pull;
                forces[a].y += uy * pull;
                forces[b].x -= ux * pull;
                forces[b].y -= uy * pull;
            }
        }

        // The arrangement's own pull — the swatch-scale anchor spring.
        if settings.arrangement_pull > 0.0 {
            for (index, anchor) in anchors.iter().enumerate() {
                forces[index].x += (anchor.x - positions[index].x) * settings.arrangement_pull;
                forces[index].y += (anchor.y - positions[index].y) * settings.arrangement_pull;
            }
        }

        for (index, item) in scene.items.iter_mut().enumerate() {
            if held[index] {
                continue;
            }
            velocities[index].x =
                (velocities[index].x + forces[index].x * settings.dt) * settings.damping;
            velocities[index].y =
                (velocities[index].y + forces[index].y * settings.dt) * settings.damping;
            item.transform.translate.x += velocities[index].x * settings.dt;
            item.transform.translate.y += velocities[index].y * settings.dt;
        }
    }

    // Content bounds moved with the items.
    if let Some(first) = scene.items.first() {
        let mut bounds = sceno::Rect::new(first.transform.translate, sceno::Size2::new(0.0, 0.0));
        for item in &scene.items {
            bounds = bounds.union(sceno::Rect::new(
                item.transform.translate,
                sceno::Size2::new(0.0, 0.0),
            ));
        }
        scene.bounds = bounds;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::{
        Backdrop, InstanceId, ProjectedItem, Representation, RoutedRelation, Size2, SourceRef,
        Transform2,
    };

    fn scene_with(points: &[(f32, f32)]) -> Scene {
        let mut scene = Scene::new();
        for (i, (x, y)) in points.iter().enumerate() {
            let source = scene.intern_source(SourceRef::new("test", i.to_string()));
            scene.items.push(ProjectedItem {
                source,
                space: Scene::WORLD,
                transform: Transform2::translation(*x, *y),
                footprint: Footprint::Rect {
                    size: Size2::new(20.0, 20.0),
                },
                representation: Representation::Glyph,
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });
        }
        scene
    }

    fn separation(scene: &Scene, a: usize, b: usize) -> f32 {
        let p = scene.items[a].transform.translate;
        let q = scene.items[b].transform.translate;
        ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt()
    }

    #[test]
    fn zero_steps_leaves_the_scene_untouched() {
        let mut scene = scene_with(&[(0.0, 0.0), (1.0, 0.0)]);
        let before = scene.clone();
        relax(
            &mut scene,
            &Relaxation {
                steps: 0,
                ..Default::default()
            },
        );
        assert_eq!(scene, before);
    }

    #[test]
    fn plain_relax_no_longer_drags_a_recorded_pin() {
        // The footgun this closes: a caller who reaches for `relax` rather than
        // `relax_holding` used to move an ensure-class placement in silence,
        // and nothing in the types objected.
        let mut scene = scene_with(&[(0.0, 0.0), (1.0, 0.0), (-1.0, 0.5)]);
        scene.honored_holds.push(sceno::HonoredHold {
            instance: InstanceId(0),
            placement: sceno::HeldPlacement::pinned(
                SourceRef::new("test", "0"),
                Vec2::new(0.0, 0.0),
            ),
        });
        let pinned_at = scene.items[0].transform.translate;

        relax(&mut scene, &Relaxation::default());

        assert_eq!(
            scene.items[0].transform.translate, pinned_at,
            "a recorded pin moved under plain relax"
        );
        assert_ne!(
            scene.items[1].transform.translate,
            Vec2::new(1.0, 0.0),
            "and its neighbours still relaxed around it"
        );
    }

    #[test]
    fn an_anchored_hold_still_relaxes() {
        // Anchored is best effort by design, so it is not in honored_holds and
        // must keep moving. A scene where anchoring silently pinned would be
        // the same silent-soft failure wearing the other face.
        let mut scene = scene_with(&[(0.0, 0.0), (1.0, 0.0)]);
        let before = scene.items[0].transform.translate;
        relax(&mut scene, &Relaxation::default());
        assert_ne!(scene.items[0].transform.translate, before);
    }

    #[test]
    fn a_held_item_does_not_move_however_crowded() {
        // Three items on top of each other: maximum pressure to displace.
        let mut scene = scene_with(&[(0.0, 0.0), (1.0, 0.0), (-1.0, 0.5)]);
        let anchored = scene.items[0].transform.translate;
        relax_holding(&mut scene, &Relaxation::default(), &[InstanceId(0)]);
        assert_eq!(
            scene.items[0].transform.translate, anchored,
            "a hold was relaxed away, which is the silent-soft failure"
        );
    }

    #[test]
    fn a_held_item_still_pushes_its_neighbours() {
        // The asymmetry that makes a hold useful: the scene accommodates the
        // pin rather than the pin being averaged into the crowd.
        let mut scene = scene_with(&[(0.0, 0.0), (1.0, 0.0)]);
        let before = separation(&scene, 0, 1);
        relax_holding(&mut scene, &Relaxation::default(), &[InstanceId(0)]);
        assert!(
            separation(&scene, 0, 1) > before,
            "the free neighbour should have been pushed clear"
        );
    }

    #[test]
    fn relax_is_relax_holding_with_nothing_held() {
        let mut plain = scene_with(&[(0.0, 0.0), (1.0, 0.0), (-1.0, 0.5)]);
        let mut empty_holds = plain.clone();
        relax(&mut plain, &Relaxation::default());
        relax_holding(&mut empty_holds, &Relaxation::default(), &[]);
        assert_eq!(plain, empty_holds);
    }

    #[test]
    fn crowded_items_push_apart() {
        let mut scene = scene_with(&[(0.0, 0.0), (2.0, 0.0), (-2.0, 1.0)]);
        let before = separation(&scene, 0, 1);
        relax(&mut scene, &Relaxation::default().untethered());
        assert!(
            separation(&scene, 0, 1) > before,
            "a crowded pair should loosen, got {} from {before}",
            separation(&scene, 0, 1)
        );
    }

    #[test]
    fn a_collidable_backdrop_excludes_item_placement_even_when_invisible() {
        let mut scene = scene_with(&[(0.0, 0.0)]);
        let source = scene.intern_source(SourceRef::new("test", "wall"));
        scene.backdrops.push(Backdrop {
            source,
            space: Scene::WORLD,
            transform: Transform2::IDENTITY,
            footprint: Footprint::Rect {
                size: Size2::new(80.0, 80.0),
            },
            kind: "test:wall".into(),
            visible: false,
            collidable: true,
        });

        relax(&mut scene, &Relaxation::default().untethered());

        let item = placed_bounds(&scene.items[0].footprint, scene.items[0].transform).unwrap();
        let obstacle =
            placed_bounds(&scene.backdrops[0].footprint, scene.backdrops[0].transform).unwrap();
        assert_eq!(separating_shift(item, obstacle), None);
    }

    #[test]
    fn a_noncollidable_backdrop_does_not_move_an_item() {
        let mut scene = scene_with(&[(0.0, 0.0)]);
        let source = scene.intern_source(SourceRef::new("test", "floor"));
        scene.backdrops.push(Backdrop {
            source,
            space: Scene::WORLD,
            transform: Transform2::IDENTITY,
            footprint: Footprint::Rect {
                size: Size2::new(80.0, 80.0),
            },
            kind: "test:floor".into(),
            visible: true,
            collidable: false,
        });
        let before = scene.items[0].transform;
        relax(&mut scene, &Relaxation::default().untethered());
        assert_eq!(scene.items[0].transform, before);
    }

    #[test]
    fn coincident_items_separate_reproducibly() {
        let mut a = scene_with(&[(5.0, 5.0), (5.0, 5.0)]);
        let mut b = a.clone();
        let settings = Relaxation::default().untethered();
        relax(&mut a, &settings);
        relax(&mut b, &settings);
        assert!(separation(&a, 0, 1) > 1.0, "coincident items must separate");
        assert_eq!(a, b, "relaxation is deterministic");
    }

    #[test]
    fn the_arrangement_pull_holds_its_shape() {
        let points = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)];
        let mut tethered = scene_with(&points);
        let mut free = scene_with(&points);
        relax(
            &mut tethered,
            &Relaxation {
                arrangement_pull: 4.0,
                ..Default::default()
            },
        );
        relax(&mut free, &Relaxation::default().untethered());

        let drift = |scene: &Scene| -> f32 {
            scene
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let (ax, ay) = points[i];
                    ((item.transform.translate.x - ax).powi(2)
                        + (item.transform.translate.y - ay).powi(2))
                    .sqrt()
                })
                .sum()
        };
        assert!(
            drift(&tethered) < drift(&free),
            "a pulled arrangement stays nearer its slots: {} vs {}",
            drift(&tethered),
            drift(&free)
        );
    }

    #[test]
    fn related_items_are_drawn_toward_the_rest_length() {
        let mut scene = scene_with(&[(0.0, 0.0), (400.0, 0.0)]);
        scene.relations.push(RoutedRelation {
            from: InstanceId(0),
            to: InstanceId(1),
            space: Scene::WORLD,
            points: Vec::new(),
            kind: None,
            weight: Some(1.0),
        });
        let before = separation(&scene, 0, 1);
        relax(&mut scene, &Relaxation::default().untethered());
        assert!(
            separation(&scene, 0, 1) < before,
            "a stretched relation should pull in"
        );
    }
}
