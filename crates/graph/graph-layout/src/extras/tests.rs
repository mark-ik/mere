use super::*;
use euclid::default::{Rect, Size2D};
use canvas_ir::projection::ProjectionMode;
use canvas_ir::scene::{CanvasEdge, CanvasNode, SceneMode, ViewId};

fn viewport() -> CanvasViewport {
    CanvasViewport {
        rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
        scale_factor: 1.0,
    }
}

fn scene(nodes: Vec<(u32, f32, f32)>, edges: Vec<(u32, u32)>) -> CanvasSceneInput<u32> {
    CanvasSceneInput {
        view_id: ViewId(0),
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
        scene_objects: Vec::new(),
        overlays: Vec::new(),
        scene_mode: SceneMode::Browse,
        projection: ProjectionMode::default(),
    }
}

#[test]
fn degree_repulsion_pushes_hub_neighbors_apart() {
    // Hub=0 with edges to 1,2,3. Neighbors 1 and 2 should be pushed apart.
    let input = scene(
        vec![(0, 0.0, 0.0), (1, -5.0, 0.0), (2, 5.0, 0.0), (3, 0.0, 20.0)],
        vec![(0, 1), (0, 2), (0, 3)],
    );
    let mut layout = DegreeRepulsion::new(DegreeRepulsionConfig::medium());
    let mut state = StatelessPassState::default();
    let deltas = layout.step(
        &input,
        &mut state,
        0.0,
        &viewport(),
        &LayoutExtras::default(),
    );
    // 1 (left) pushed further left; 2 (right) pushed further right.
    assert!(deltas[&1].x < 0.0);
    assert!(deltas[&2].x > 0.0);
}

#[test]
fn domain_clustering_pulls_same_domain_members_together() {
    let input = scene(vec![(0, 0.0, 0.0), (1, 100.0, 0.0)], vec![]);
    let mut layout = DomainClustering::<u32>::new(DomainClusteringConfig {
        strength: 0.2,
        ..Default::default()
    });
    let mut extras: LayoutExtras<u32> = LayoutExtras::default();
    extras.domain_by_node.insert(0, "example.com".into());
    extras.domain_by_node.insert(1, "example.com".into());
    let deltas = layout.step(
        &input,
        &mut StatelessPassState::default(),
        0.0,
        &viewport(),
        &extras,
    );
    assert!(deltas[&0].x > 0.0); // pulled right toward centroid at 50
    assert!(deltas[&1].x < 0.0); // pulled left toward centroid at 50
}

#[test]
fn semantic_clustering_respects_similarity_floor() {
    let input = scene(vec![(0, 0.0, 0.0), (1, 100.0, 0.0)], vec![]);
    let mut layout = SemanticClustering::new(SemanticClusteringConfig {
        strength: 0.1,
        similarity_floor: 0.5,
        ..Default::default()
    });
    let mut extras: LayoutExtras<u32> = LayoutExtras::default();
    extras.semantic_similarity.insert((0, 1), 0.3); // below floor
    let deltas = layout.step(
        &input,
        &mut StatelessPassState::default(),
        0.0,
        &viewport(),
        &extras,
    );
    assert!(deltas.is_empty());

    extras.semantic_similarity.insert((0, 1), 0.8); // above floor
    let deltas = layout.step(
        &input,
        &mut StatelessPassState::default(),
        0.0,
        &viewport(),
        &extras,
    );
    assert!(deltas.contains_key(&0));
    assert!(deltas.contains_key(&1));
}

#[test]
fn hub_pull_moves_leaf_toward_hub() {
    let input = scene(
        vec![
            (0, 0.0, 0.0),  // hub
            (1, 80.0, 0.0), // leaf (will be pulled left toward hub)
            (2, -20.0, 20.0),
            (3, 20.0, 20.0),
        ],
        vec![(0, 1), (0, 2), (0, 3)],
    );
    let mut layout = HubPull::new(HubPullConfig::default());
    let deltas = layout.step(
        &input,
        &mut StatelessPassState::default(),
        0.0,
        &viewport(),
        &LayoutExtras::default(),
    );
    assert!(deltas[&1].x < 0.0, "leaf should be pulled toward hub");
}

#[test]
fn frame_affinity_pulls_members_to_centroid() {
    let input = scene(
        vec![(0, -50.0, 0.0), (1, 50.0, 0.0), (2, 0.0, 100.0)],
        vec![],
    );
    let mut layout = FrameAffinity::new(FrameAffinityConfig::default());
    let mut extras: LayoutExtras<u32> = LayoutExtras::default();
    extras.frame_regions.push(FrameRegion {
        anchor: 0,
        members: vec![0, 1, 2],
        strength: 0.5,
    });
    let deltas = layout.step(
        &input,
        &mut StatelessPassState::default(),
        0.0,
        &viewport(),
        &extras,
    );
    // Centroid is (0, 33.33). Node 0 pulled right+up; node 1 pulled left+up; node 2 pulled down.
    assert!(deltas[&0].x > 0.0 && deltas[&0].y > 0.0);
    assert!(deltas[&1].x < 0.0 && deltas[&1].y > 0.0);
    assert!(deltas[&2].y < 0.0);
}

#[test]
fn pinned_nodes_excluded_from_all_extras() {
    let input = scene(vec![(0, 0.0, 0.0), (1, 100.0, 0.0)], vec![]);
    let mut extras: LayoutExtras<u32> = LayoutExtras::default();
    extras.pinned.insert(0);
    extras.domain_by_node.insert(0, "x".into());
    extras.domain_by_node.insert(1, "x".into());
    let mut layout = DomainClustering::<u32>::new(DomainClusteringConfig {
        strength: 0.2,
        ..Default::default()
    });
    let deltas = layout.step(
        &input,
        &mut StatelessPassState::default(),
        0.0,
        &viewport(),
        &extras,
    );
    assert!(!deltas.contains_key(&0));
    assert!(deltas.contains_key(&1));
}
