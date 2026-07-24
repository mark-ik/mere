//! Product-free analytic score realization.

use sceno::{
    Arrangement, Board, Footprint, Geographic, Placement, ProjectedItem, Rect, Scene, Score,
    ScoreItem, Spiral, SpiralCurve, Transform2, Vec2,
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
        let position = match &score.arrangement {
            Arrangement::Spiral(spiral) => spiral_position(spiral, effective_spacing, rank),
            Arrangement::Board(board) => board_position(board, item, rank),
            Arrangement::Geographic(geographic) => geographic_position(geographic, item),
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
    scene.bounds = bounds.unwrap_or_default();
    scene
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

fn board_position(board: &Board, item: &ScoreItem, rank: usize) -> Vec2 {
    let columns = board.columns.max(1) as usize;
    let (column, row) = match item.placement {
        Placement::Cell { column, row } => (column as f32, row as f32),
        _ => ((rank % columns) as f32, (rank / columns) as f32),
    };
    Vec2::new(
        board.origin.x + column * (board.cell.x + board.gap),
        board.origin.y + row * (board.cell.y + board.gap),
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
    use sceno::{Representation, Size2, SourceRef};

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

    fn overlaps(a: &Rect, b: &Rect) -> bool {
        a.origin.x < b.origin.x + b.size.w
            && b.origin.x < a.origin.x + a.size.w
            && a.origin.y < b.origin.y + b.size.h
            && b.origin.y < a.origin.y + a.size.h
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
    fn board_keeps_authored_cells_and_geographic_keeps_coordinates() {
        let mut board_score = Score::new(Arrangement::Board(Board::default()));
        let mut placed = card(1, 0);
        placed.placement = Placement::Cell { column: 3, row: 2 };
        board_score.items.push(placed);
        let board = solve(&board_score);
        assert_eq!(board.items[0].transform.translate, Vec2::new(204.0, 136.0));

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
