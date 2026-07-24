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

use sceno::{Footprint, Scene, Vec2};

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

/// Loosen `scene` in place: items push apart, related items pull together, and
/// each item is drawn back toward the slot its arrangement chose.
///
/// Deterministic — no randomness, so the same scene and settings always relax
/// the same way and a receipt can be compared frame to frame.
pub fn relax(scene: &mut Scene, settings: &Relaxation) {
    if settings.steps == 0 || scene.items.is_empty() {
        return;
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
            velocities[index].x = (velocities[index].x + forces[index].x * settings.dt)
                * settings.damping;
            velocities[index].y = (velocities[index].y + forces[index].y * settings.dt)
                * settings.damping;
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
    use sceno::{InstanceId, ProjectedItem, Representation, RoutedRelation, Size2, SourceRef, Transform2};

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
        relax(&mut scene, &Relaxation { steps: 0, ..Default::default() });
        assert_eq!(scene, before);
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
        relax(&mut tethered, &Relaxation { arrangement_pull: 4.0, ..Default::default() });
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
