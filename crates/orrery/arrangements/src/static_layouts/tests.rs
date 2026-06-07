use super::*;
use euclid::default::{Rect, Size2D};
use crate::scene::{CanvasEdge, CanvasNode};

fn viewport() -> CanvasViewport {
    CanvasViewport {
        rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
        scale_factor: 1.0,
    }
}

fn scene(nodes: Vec<(u32, f32, f32)>, edges: Vec<(u32, u32)>) -> CanvasSceneInput<u32> {
    CanvasSceneInput {
        nodes: nodes
            .into_iter()
            .map(|(id, x, y)| CanvasNode {
                id,
                position: Point2D::new(x, y),
                radius: 16.0,
                label: None,
            })
            .collect(),
        edges: edges
            .into_iter()
            .map(|(s, t)| CanvasEdge::untagged(s, t))
            .collect(),
    }
}

fn apply(
    deltas: &HashMap<u32, Vector2D<f32>>,
    scene: &CanvasSceneInput<u32>,
) -> HashMap<u32, Point2D<f32>> {
    let mut positions: HashMap<u32, Point2D<f32>> =
        scene.nodes.iter().map(|n| (n.id, n.position)).collect();
    for (id, d) in deltas {
        if let Some(p) = positions.get_mut(id) {
            *p = *p + *d;
        }
    }
    positions
}

#[test]
fn grid_places_nodes_in_row_major_order() {
    let mut layout = Grid::new(GridConfig {
        gap: 10.0,
        origin: Point2D::new(0.0, 0.0),
        ..Default::default()
    });
    let mut state = StaticLayoutState::default();
    let input = scene(
        vec![
            (0, 100.0, 100.0),
            (1, 100.0, 100.0),
            (2, 100.0, 100.0),
            (3, 100.0, 100.0),
        ],
        vec![],
    );
    let deltas = layout.step(
        &input,
        &mut state,
        0.0,
        &viewport(),
        &LayoutExtras::default(),
    );
    let positions = apply(&deltas, &input);
    // 4 nodes, columns=ceil(sqrt(4))=2, so layout is 2x2.
    assert_eq!(positions[&0], Point2D::new(0.0, 0.0));
    assert_eq!(positions[&1], Point2D::new(10.0, 0.0));
    assert_eq!(positions[&2], Point2D::new(0.0, 10.0));
    assert_eq!(positions[&3], Point2D::new(10.0, 10.0));
}

#[test]
fn radial_places_focus_at_center_and_direct_neighbors_on_ring_one() {
    let mut layout = Radial::new(RadialConfig {
        focus: Some(0u32),
        center: Point2D::new(500.0, 500.0),
        ring_spacing: 100.0,
        ..Default::default()
    });
    let mut state = StaticLayoutState::default();
    let input = scene(
        vec![(0, 0.0, 0.0), (1, 0.0, 0.0), (2, 0.0, 0.0)],
        vec![(0, 1), (0, 2)],
    );
    let deltas = layout.step(
        &input,
        &mut state,
        0.0,
        &viewport(),
        &LayoutExtras::default(),
    );
    let positions = apply(&deltas, &input);
    // Focus at center.
    assert!((positions[&0].x - 500.0).abs() < 0.01);
    assert!((positions[&0].y - 500.0).abs() < 0.01);
    // Two direct neighbors on ring 1 (radius 100).
    let r1 = (positions[&1] - Point2D::new(500.0, 500.0)).length();
    let r2 = (positions[&2] - Point2D::new(500.0, 500.0)).length();
    assert!((r1 - 100.0).abs() < 0.01);
    assert!((r2 - 100.0).abs() < 0.01);
}

#[test]
fn phyllotaxis_first_node_near_center_and_radius_grows_monotonically() {
    let mut layout = Phyllotaxis::new(PhyllotaxisConfig {
        center: Point2D::new(0.0, 0.0),
        scale: 10.0,
        orientation: SpiralOrientation::Outward,
        ..Default::default()
    });
    let mut state = StaticLayoutState::default();
    let input = scene(
        (0..10u32).map(|i| (i, 100.0 + i as f32, 0.0)).collect(),
        vec![],
    );
    let deltas = layout.step(
        &input,
        &mut state,
        0.0,
        &viewport(),
        &LayoutExtras::default(),
    );
    let positions = apply(&deltas, &input);

    // Node 0 is at the center.
    let d0 = (positions[&0] - Point2D::new(0.0, 0.0)).length();
    assert!(d0 < 0.5);

    // Radii grow roughly as sqrt(i).
    let mut last_r = 0.0;
    for i in 1..10u32 {
        let r = (positions[&i] - Point2D::new(0.0, 0.0)).length();
        assert!(r > last_r);
        last_r = r;
    }
}

#[test]
fn damping_fractional_only_applies_partial_delta() {
    let mut layout = Grid::new(GridConfig {
        gap: 10.0,
        origin: Point2D::new(0.0, 0.0),
        ..Default::default()
    });
    let mut state = StaticLayoutState {
        damping: 0.5,
        ..Default::default()
    };
    let input = scene(vec![(0, 100.0, 100.0)], vec![]);
    let deltas = layout.step(
        &input,
        &mut state,
        0.0,
        &viewport(),
        &LayoutExtras::default(),
    );
    // Target is (0,0); damped to half = (-50, -50).
    assert!((deltas[&0].x - (-50.0)).abs() < 0.01);
    assert!((deltas[&0].y - (-50.0)).abs() < 0.01);
}

#[test]
fn pinned_nodes_skipped() {
    let mut layout = Grid::new(GridConfig::default());
    let mut state = StaticLayoutState::default();
    let input = scene(vec![(0, 100.0, 100.0), (1, 100.0, 100.0)], vec![]);
    let mut extras = LayoutExtras::default();
    extras.pinned.insert(0);
    let deltas = layout.step(&input, &mut state, 0.0, &viewport(), &extras);
    assert!(!deltas.contains_key(&0));
    assert!(deltas.contains_key(&1));
}
