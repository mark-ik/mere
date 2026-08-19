//! Product-free analytic score realization.

use sceno::{
    Arrangement, Footprint, Geographic, Grid, Hold, HonoredHold, Hulls, InstanceId, Placement,
    ProjectedItem, Rect, Region, Scene, Score, ScoreItem, Spiral, SpiralCurve, Transform2, Vec2,
};

/// The golden-angle spiral's nearest neighbours are roughly 0.9 of the scale
/// constant apart. `sqrt(2) / 0.9` clears equal axis-aligned squares at every
/// angle; a little headroom keeps the receipt legible.
const SPACING_PER_EXTENT: f32 = 1.6;

/// Realize a persisted score into a scene. The solver only sees the score's
/// opaque refs, footprints, placements, and selected representations.
pub fn solve(score: &Score) -> Scene {
    let mut scene = Scene::new();
    scene.generation = score.generation;

    let mut order: Vec<(usize, &ScoreItem)> = score.items.iter().enumerate().collect();
    order.sort_by_key(|(index, item)| (item.ordinal, *index));

    let effective_spacing = match &score.arrangement {
        Arrangement::Spiral(spiral) => spiral.spacing.max(
            order
                .iter()
                .filter_map(|(_, item)| footprint_side(&item.footprint))
                .max_by(f32::total_cmp)
                .unwrap_or(0.0)
                * SPACING_PER_EXTENT,
        ),
        _ => 0.0,
    };

    let mut bounds: Option<Rect> = None;
    for (rank, (_, item)) in order.into_iter().enumerate() {
        // An authored hold outranks the arrangement, in every family. Without
        // this, a Coordinate placement was honored by Geographic and Hulls and
        // silently discarded by Spiral and Grid, which is the same bug as
        // moving a pin: the person said where, and the layout answered
        // somewhere else without saying so.
        let position = match score.hold_for(&item.source) {
            Some(held) => held.at,
            None => match &score.arrangement {
                Arrangement::Spiral(spiral) => spiral_position(spiral, effective_spacing, rank),
                Arrangement::Grid(grid) => grid_position(grid, item, rank),
                Arrangement::Geographic(geographic) => geographic_position(geographic, item),
                Arrangement::Hulls(hulls) => hulls_position(hulls, item),
            },
        };
        let source = scene.intern_source(item.source.clone());
        let instance = ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(position.x, position.y),
            footprint: item.footprint.clone(),
            representation: item.representation.clone(),
            layer: item.layer,
            visible: item.visible,
            hit: None,
            channels: Vec::new(),
        };
        if let Some(item_bounds) = placed_bounds(position, &instance.footprint) {
            bounds = Some(match bounds {
                Some(old) => old.union(item_bounds),
                None => item_bounds,
            });
        }
        scene.items.push(instance);
    }

    // Hulls emits the partition after every site is placed, because a cell is
    // defined against all the others: the part of the bounds nearer this site
    // than any of them.
    if let Arrangement::Hulls(hulls) = &score.arrangement {
        partition(hulls, &mut scene);
        bounds = Some(match bounds {
            Some(old) => old.union(hulls.bounds),
            None => hulls.bounds,
        });
    }

    // A hold naming a source this score never placed is an unmet pin. Before
    // this it was dropped in silence, which is the same failure as moving a pin
    // and saying nothing. Encourage-class holds are excluded on purpose: an
    // anchored home that goes unplaced is best effort behaving as designed.
    // The positive half, bound to instances. Recorded for every instance a
    // held source reached, because a source may be placed more than once and
    // each placement is equally held.
    scene.honored_holds = scene
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let source = scene.sources.get(item.source.0 as usize)?;
            let held = score.hold_for(source)?;
            matches!(held.hold, Hold::Pinned).then(|| HonoredHold {
                instance: InstanceId(index as u32),
                placement: held.clone(),
            })
        })
        .collect();

    scene.unmet_holds = score
        .holds
        .iter()
        .filter(|held| matches!(held.hold, Hold::Pinned))
        .filter(|held| !score.items.iter().any(|item| item.source == held.source))
        .cloned()
        .collect();

    scene.bounds = bounds.unwrap_or_default();
    scene
}

/// The instances a score pins, ready to hand to [`crate::relax_holding`].
///
/// Resolves through the scene's interned sources, so it stays correct when a
/// source appears more than once: every instance of a pinned source is pinned.
pub fn pinned_instances(score: &Score, scene: &Scene) -> Vec<InstanceId> {
    scene
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let source = scene.sources.get(item.source.0 as usize)?;
            matches!(score.hold_for(source)?.hold, Hold::Pinned)
                .then_some(InstanceId(index as u32))
        })
        .collect()
}

/// One scene [`Region`] per placed site: the bounds clipped by the
/// perpendicular-bisector half-plane against every other site. Exact rather
/// than sampled, and quadratic in sites, which place-graph counts never
/// stress.
fn partition(hulls: &Hulls, scene: &mut Scene) {
    let sites: Vec<(sceno::InstanceId, Vec2)> = scene
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.visible)
        .map(|(index, item)| {
            (
                sceno::InstanceId(index as u32),
                Vec2::new(item.transform.translate.x, item.transform.translate.y),
            )
        })
        .collect();

    for (id, site) in &sites {
        let mut cell = rect_polygon(hulls.bounds);
        for (other_id, other) in &sites {
            if other_id == id {
                continue;
            }
            cell = clip_nearer(cell, *site, *other);
            if cell.len() < 3 {
                break;
            }
        }
        if cell.len() < 3 {
            // Coincident sites can starve a cell to nothing. An empty region
            // would be a lie about coverage, so it is simply not emitted.
            continue;
        }
        scene.regions.push(Region {
            members: vec![*id],
            space: Scene::WORLD,
            contour: Some(Footprint::Polygon { points: cell }),
            label: None,
            confidence: 1.0,
        });
    }
}

fn rect_polygon(rect: Rect) -> Vec<Vec2> {
    let (x, y) = (rect.origin.x, rect.origin.y);
    let (w, h) = (rect.size.w, rect.size.h);
    vec![
        Vec2::new(x, y),
        Vec2::new(x + w, y),
        Vec2::new(x + w, y + h),
        Vec2::new(x, y + h),
    ]
}

/// Sutherland-Hodgman against the half-plane of points nearer `site` than
/// `other`: keep everything on `site`'s side of their perpendicular bisector.
fn clip_nearer(polygon: Vec<Vec2>, site: Vec2, other: Vec2) -> Vec<Vec2> {
    let mid = Vec2::new((site.x + other.x) / 2.0, (site.y + other.y) / 2.0);
    let away = Vec2::new(other.x - site.x, other.y - site.y);
    // Signed distance along `away`: negative is nearer `site`.
    let side = |p: Vec2| (p.x - mid.x) * away.x + (p.y - mid.y) * away.y;

    let mut out = Vec::with_capacity(polygon.len() + 1);
    for (index, &current) in polygon.iter().enumerate() {
        let previous = polygon[(index + polygon.len() - 1) % polygon.len()];
        let (d_prev, d_cur) = (side(previous), side(current));
        if d_prev <= 0.0 && d_cur > 0.0 || d_prev > 0.0 && d_cur <= 0.0 {
            // The edge crosses the bisector; keep the crossing point.
            let t = d_prev / (d_prev - d_cur);
            out.push(Vec2::new(
                previous.x + (current.x - previous.x) * t,
                previous.y + (current.y - previous.y) * t,
            ));
        }
        if d_cur <= 0.0 {
            out.push(current);
        }
    }
    out
}

fn hulls_position(hulls: &Hulls, item: &ScoreItem) -> Vec2 {
    let coordinate = match item.placement {
        Placement::Coordinate(coordinate) => coordinate,
        // A site is somewhere by definition; anything undisclosed sits at the
        // origin rather than being invented a position.
        _ => Vec2::ZERO,
    };
    Vec2::new(
        hulls.origin.x + coordinate.x * hulls.units_per_coordinate,
        hulls.origin.y
            + coordinate.y * hulls.units_per_coordinate * if hulls.invert_y { -1.0 } else { 1.0 },
    )
}

fn spiral_position(spiral: &Spiral, spacing: f32, ordinal: usize) -> Vec2 {
    let n = ordinal as f32;
    let radius = spacing
        * match spiral.curve {
            SpiralCurve::SquareRoot => n.sqrt(),
            SpiralCurve::Linear => n,
            SpiralCurve::Quadratic => n * n,
            SpiralCurve::Logarithmic => (1.0 + n).ln(),
        };
    let angle = n * spiral.angle_radians;
    Vec2::new(
        spiral.center.x + radius * angle.cos(),
        spiral.center.y + radius * angle.sin(),
    )
}

fn grid_position(grid: &Grid, item: &ScoreItem, rank: usize) -> Vec2 {
    let columns = grid.columns.max(1) as usize;
    let (column, row) = match item.placement {
        Placement::Cell { column, row } => (column as f32, row as f32),
        _ => ((rank % columns) as f32, (rank / columns) as f32),
    };
    Vec2::new(
        grid.origin.x + column * (grid.cell.x + grid.gap),
        grid.origin.y + row * (grid.cell.y + grid.gap),
    )
}

fn geographic_position(geographic: &Geographic, item: &ScoreItem) -> Vec2 {
    let Placement::Coordinate(coordinate) = item.placement else {
        return geographic.origin;
    };
    Vec2::new(
        geographic.origin.x + coordinate.x * geographic.units_per_coordinate,
        geographic.origin.y
            + coordinate.y
                * geographic.units_per_coordinate
                * if geographic.invert_y { -1.0 } else { 1.0 },
    )
}

fn footprint_side(footprint: &Footprint) -> Option<f32> {
    footprint
        .bounds()
        .map(|bounds| bounds.size.w.max(bounds.size.h))
}

fn placed_bounds(position: Vec2, footprint: &Footprint) -> Option<Rect> {
    footprint.bounds().map(|local| {
        Rect::new(
            Vec2::new(position.x + local.origin.x, position.y + local.origin.y),
            local.size,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::{HeldPlacement, Representation, Size2, SourceRef};

    fn card(id: u32, ordinal: u32) -> ScoreItem {
        ScoreItem {
            source: SourceRef::new("fixture", id.to_string()),
            ordinal,
            footprint: Footprint::Rect {
                size: Size2::new(36.0, 36.0),
            },
            representation: Representation::Card,
            placement: Placement::Ordinal,
            layer: 0,
            visible: true,
        }
    }

    #[test]
    fn a_hold_outranks_the_arrangement_in_every_family() {
        // The audit that opened A6: Coordinate was honored by Geographic and
        // Hulls and silently dropped by Spiral and Grid. A hold is honored by
        // all four or it is not a hold.
        let at = Vec2::new(140.0, -60.0);
        for arrangement in [
            Arrangement::Spiral(Spiral::default()),
            Arrangement::Grid(Grid::default()),
            Arrangement::Geographic(Geographic::default()),
            Arrangement::Hulls(Hulls::default()),
        ] {
            let mut score = Score::new(arrangement.clone());
            score.items.push(card(0, 0));
            score.items.push(card(1, 1));
            score
                .holds
                .push(HeldPlacement::pinned(SourceRef::new("fixture", "1"), at));

            let scene = solve(&score);
            let held = scene
                .items
                .iter()
                .find(|item| scene.sources[item.source.0 as usize].id == "1")
                .expect("the held item is placed");
            assert_eq!(
                held.transform.translate, at,
                "{arrangement:?} ignored an authored hold"
            );
        }
    }

    #[test]
    fn an_unheld_score_places_exactly_as_before() {
        // Holds are additive: the empty case must not perturb any receipt.
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        for id in 0..5 {
            score.items.push(card(id, id));
        }
        let before = solve(&score);
        score.holds.clear();
        let after = solve(&score);
        assert_eq!(before.items.len(), after.items.len());
        for (a, b) in before.items.iter().zip(&after.items) {
            assert_eq!(a.transform.translate, b.transform.translate);
        }
    }

    #[test]
    fn pinned_instances_finds_every_instance_of_a_pinned_source() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        // One source, twice: legal by design, and both instances are pinned.
        score.items.push(card(0, 0));
        score.items.push(card(0, 1));
        score.items.push(card(1, 2));
        score.holds.push(HeldPlacement::pinned(
            SourceRef::new("fixture", "0"),
            Vec2::new(5.0, 5.0),
        ));
        score.holds.push(HeldPlacement::anchored(
            SourceRef::new("fixture", "1"),
            Vec2::new(9.0, 9.0),
        ));

        let scene = solve(&score);
        let pinned = pinned_instances(&score, &scene);
        assert_eq!(pinned.len(), 2, "both instances of source 0 are pinned");
        // Anchored is best effort, so it is not immovable.
        for instance in &pinned {
            assert_eq!(scene.sources[scene.items[instance.0 as usize].source.0 as usize].id, "0");
        }
    }

    #[test]
    fn a_satisfied_pin_is_recorded_against_its_instance() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.items.push(card(0, 0));
        score.items.push(card(1, 1));
        score.holds.push(HeldPlacement::pinned(
            SourceRef::new("fixture", "1"),
            Vec2::new(9.0, -3.0),
        ));
        let scene = solve(&score);

        assert_eq!(scene.honored_holds.len(), 1);
        let honored = &scene.honored_holds[0];
        assert_eq!(honored.placement.source.id, "1");
        // Bound to the instance, so a consumer never re-derives the mapping.
        let placed = scene.items[honored.instance.0 as usize].transform.translate;
        assert_eq!(placed, Vec2::new(9.0, -3.0));
        assert!(scene.unmet_holds.is_empty());
    }

    #[test]
    fn an_anchored_hold_is_not_recorded_as_honored() {
        // Encourage-class is a suggestion. Recording it as honored would invite
        // a later pass to treat it as binding, which is the opposite of what
        // anchored means.
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.items.push(card(0, 0));
        score.holds.push(HeldPlacement::anchored(
            SourceRef::new("fixture", "0"),
            Vec2::new(1.0, 1.0),
        ));
        assert!(solve(&score).honored_holds.is_empty());
    }

    #[test]
    fn every_instance_of_a_held_source_is_recorded() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.items.push(card(0, 0));
        score.items.push(card(0, 1));
        score.holds.push(HeldPlacement::pinned(
            SourceRef::new("fixture", "0"),
            Vec2::new(2.0, 2.0),
        ));
        assert_eq!(solve(&score).honored_holds.len(), 2);
    }

    #[test]
    fn an_unsatisfiable_pin_is_reported_on_the_scene() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.items.push(card(0, 0));
        score.holds.push(HeldPlacement::pinned(
            SourceRef::new("fixture", "ghost"),
            Vec2::new(7.0, 7.0),
        ));

        let scene = solve(&score);
        assert_eq!(scene.items.len(), 1, "the ghost is not invented as an item");
        assert_eq!(scene.unmet_holds.len(), 1, "the violation is carried");
        assert_eq!(scene.unmet_holds[0].source.id, "ghost");
        // The coordinate travels too, so a viewer without the score can say
        // what was asked for, not merely that something failed.
        assert_eq!(scene.unmet_holds[0].at, Vec2::new(7.0, 7.0));
    }

    #[test]
    fn a_satisfied_pin_reports_nothing() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.items.push(card(0, 0));
        score.holds.push(HeldPlacement::pinned(
            SourceRef::new("fixture", "0"),
            Vec2::new(4.0, 4.0),
        ));
        let scene = solve(&score);
        assert!(scene.unmet_holds.is_empty());
        assert_eq!(scene.items[0].transform.translate, Vec2::new(4.0, 4.0));
    }

    #[test]
    fn an_unplaced_anchor_is_not_a_violation() {
        // Encourage-class is best effort by definition, so its absence is not
        // reported; only ensure-class earns a violation.
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.items.push(card(0, 0));
        score.holds.push(HeldPlacement::anchored(
            SourceRef::new("fixture", "ghost"),
            Vec2::new(7.0, 7.0),
        ));
        assert!(solve(&score).unmet_holds.is_empty());
    }

    fn overlaps(a: &Rect, b: &Rect) -> bool {
        a.origin.x < b.origin.x + b.size.w
            && b.origin.x < a.origin.x + a.size.w
            && a.origin.y < b.origin.y + b.size.h
            && b.origin.y < a.origin.y + a.size.h
    }

    fn site(id: u32, x: f32, y: f32) -> ScoreItem {
        ScoreItem {
            source: SourceRef::new("fixture", id.to_string()),
            ordinal: id,
            footprint: Footprint::Point,
            representation: Representation::Glyph,
            placement: Placement::Coordinate(Vec2::new(x, y)),
            layer: 0,
            visible: true,
        }
    }

    fn hulls_score(sites: &[(f32, f32)]) -> Score {
        let mut score = Score::new(Arrangement::Hulls(Hulls {
            origin: Vec2::ZERO,
            units_per_coordinate: 1.0,
            invert_y: false,
            bounds: Rect::new(Vec2::new(-100.0, -100.0), Size2::new(200.0, 200.0)),
        }));
        for (index, (x, y)) in sites.iter().enumerate() {
            score.items.push(site(index as u32, *x, *y));
        }
        score
    }

    fn polygon_area(points: &[Vec2]) -> f32 {
        let mut doubled = 0.0;
        for (index, a) in points.iter().enumerate() {
            let b = points[(index + 1) % points.len()];
            doubled += a.x * b.y - b.x * a.y;
        }
        (doubled / 2.0).abs()
    }

    fn contains(points: &[Vec2], p: Vec2) -> bool {
        // Even-odd ray cast, good enough for convex cells in a test.
        let mut inside = false;
        for (index, a) in points.iter().enumerate() {
            let b = points[(index + 1) % points.len()];
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y) {
                inside = !inside;
            }
        }
        inside
    }

    #[test]
    fn one_site_owns_the_whole_bounds() {
        let scene = solve(&hulls_score(&[(10.0, -5.0)]));
        assert_eq!(scene.regions.len(), 1);
        let Some(Footprint::Polygon { points }) = &scene.regions[0].contour else {
            panic!("a cell is a polygon");
        };
        assert!((polygon_area(points) - 200.0 * 200.0).abs() < 1.0);
    }

    #[test]
    fn cells_tile_the_bounds() {
        // Cover without overlap: the areas of the cells sum to the area of the
        // bounds, which a halo arrangement could never promise.
        let scene = solve(&hulls_score(&[
            (-40.0, -40.0),
            (60.0, 10.0),
            (0.0, 55.0),
            (-20.0, 80.0),
        ]));
        assert_eq!(scene.regions.len(), 4);

        let total: f32 = scene
            .regions
            .iter()
            .map(|region| match &region.contour {
                Some(Footprint::Polygon { points }) => polygon_area(points),
                _ => panic!("every cell is a polygon"),
            })
            .sum();
        assert!((total - 200.0 * 200.0).abs() < 1.0, "cells sum to {total}");
    }

    #[test]
    fn a_cell_is_the_nearest_site_rule_made_visible() {
        // The property that makes hulls over Mesocosm's places exact: any
        // sampled point lies in the cell of the site it is nearest.
        let sites = [(-40.0, -40.0), (60.0, 10.0), (0.0, 55.0)];
        let scene = solve(&hulls_score(&sites));

        for sample in [
            Vec2::new(-70.0, -70.0),
            Vec2::new(80.0, 20.0),
            Vec2::new(5.0, 90.0),
            Vec2::new(-10.0, -3.0),
        ] {
            let nearest = (0..sites.len())
                .min_by(|a, b| {
                    let d = |i: usize| {
                        (sample.x - sites[i].0).powi(2) + (sample.y - sites[i].1).powi(2)
                    };
                    d(*a).total_cmp(&d(*b))
                })
                .unwrap();
            let holder = scene
                .regions
                .iter()
                .position(|region| match &region.contour {
                    Some(Footprint::Polygon { points }) => contains(points, sample),
                    _ => false,
                })
                .expect("every point in bounds is in some cell");
            assert_eq!(
                scene.regions[holder].members,
                vec![sceno::InstanceId(nearest as u32)],
                "the cell holding {sample:?} belongs to the nearest site"
            );
        }
    }

    #[test]
    fn each_cell_contains_its_own_site() {
        let sites = [(-40.0, -40.0), (60.0, 10.0), (0.0, 55.0), (-20.0, 80.0)];
        let scene = solve(&hulls_score(&sites));
        for region in &scene.regions {
            let Some(Footprint::Polygon { points }) = &region.contour else {
                panic!("polygon");
            };
            let site = &scene.items[region.members[0].0 as usize];
            assert!(contains(
                points,
                Vec2::new(site.transform.translate.x, site.transform.translate.y)
            ));
        }
    }

    #[test]
    fn an_invisible_item_is_not_a_site() {
        let mut score = hulls_score(&[(-40.0, 0.0), (40.0, 0.0)]);
        score.items[1].visible = false;
        let scene = solve(&score);
        assert_eq!(scene.regions.len(), 1, "only the visible site got a cell");
    }

    #[test]
    fn a_hulls_score_round_trips() {
        let score = hulls_score(&[(1.0, 2.0)]);
        let json = serde_json::to_string(&score).unwrap();
        assert_eq!(serde_json::from_str::<Score>(&json).unwrap(), score);
    }

    #[test]
    fn spiral_grows_spacing_to_clear_measured_cards() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.items = (0..15).map(|i| card(i, i)).collect();
        let scene = solve(&score);
        let bounds: Vec<_> = scene
            .items
            .iter()
            .map(|item| placed_bounds(item.transform.translate, &item.footprint).unwrap())
            .collect();
        for (index, a) in bounds.iter().enumerate() {
            for b in &bounds[index + 1..] {
                assert!(!overlaps(a, b), "measured cards must not overlap");
            }
        }
    }

    #[test]
    fn grid_keeps_authored_cells_and_geographic_keeps_coordinates() {
        let mut grid_score = Score::new(Arrangement::Grid(Grid::default()));
        let mut placed = card(1, 0);
        placed.placement = Placement::Cell { column: 3, row: 2 };
        grid_score.items.push(placed);
        let grid = solve(&grid_score);
        assert_eq!(grid.items[0].transform.translate, Vec2::new(204.0, 136.0));

        let mut geo_score = Score::new(Arrangement::Geographic(Geographic::default()));
        let mut point = card(2, 0);
        point.placement = Placement::Coordinate(Vec2::new(12.0, 4.0));
        geo_score.items.push(point);
        let geographic = solve(&geo_score);
        assert_eq!(
            geographic.items[0].transform.translate,
            Vec2::new(12.0, -4.0)
        );
    }

    #[test]
    fn score_order_drives_spiral_not_source_identity() {
        let mut score = Score::new(Arrangement::Spiral(Spiral::default()));
        score.items.push(card(9, 10));
        score.items.push(card(3, 0));
        let scene = solve(&score);
        assert_eq!(scene.sources[scene.items[0].source.0 as usize].id, "3");
        assert_eq!(scene.items[0].transform.translate, Vec2::ZERO);
    }

    #[test]
    fn geographic_map_fixture_keeps_disclosed_coordinates_and_lod() {
        let score: Score = serde_json::from_str(include_str!("../fixtures/coastal_map.json"))
            .expect("the portable geographic fixture parses");
        let scene = solve(&score);
        assert_eq!(scene.generation, 17);
        let position_for = |id: &str| {
            scene
                .items
                .iter()
                .find(|item| scene.sources[item.source.0 as usize].id == id)
                .map(|item| item.transform.translate)
                .expect("fixture source is realized")
        };
        assert_eq!(position_for("harbor"), Vec2::new(90.0, 194.0));
        assert_eq!(position_for("beacon"), Vec2::new(108.0, 204.0));
        assert_eq!(position_for("ridge"), Vec2::new(118.0, 184.0));
        let ridge = scene
            .items
            .iter()
            .find(|item| scene.sources[item.source.0 as usize].id == "ridge")
            .unwrap();
        assert_eq!(ridge.representation, Representation::Snapshot);
        assert!(
            scene.items.iter().any(|item| item.layer == -10
                && scene.sources[item.source.0 as usize].adapter == "fixture.map-underlay"),
            "the map underlay is an ordinary low-layer scene item"
        );
    }
}
