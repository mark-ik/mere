use std::sync::Arc;

use euclid::default::{Point2D, Rect, Size2D};

use canvas_ir::camera::CanvasViewport;
use canvas_ir::projection::ProjectionMode;
use canvas_ir::scene::{CanvasNode, CanvasSceneInput, SceneMode, ViewId};

use crate::LayoutExtras;
use super::{
    BuiltinProvider, DynLayout, LayoutCapability, LayoutCategory, LayoutProvider,
    LayoutProvenance, LayoutRegistry, RegisterError,
};
use super::force_directed_capability;

type Id = u32;

#[test]
fn default_registry_includes_force_directed() {
    let registry = LayoutRegistry::<Id>::default();
    let resolved = registry
        .resolve("graph_layout:force_directed")
        .expect("FR must be present in default registry");
    let capability = resolved.capability();
    assert_eq!(capability.category, LayoutCategory::Force);
    assert_eq!(capability.provenance, LayoutProvenance::Builtin);
}

#[test]
fn default_registry_surfaces_all_builtins() {
    let registry = LayoutRegistry::<Id>::default();
    // 16 built-ins: 11 layouts + 5 extras.
    assert_eq!(registry.len(), 16);
}

#[test]
fn filter_by_category_partitions_cleanly() {
    let registry = LayoutRegistry::<Id>::default();
    let force = registry.filter_by_category(LayoutCategory::Force);
    let positional = registry.filter_by_category(LayoutCategory::Positional);
    let extras = registry.filter_by_category(LayoutCategory::Extras);
    let projection = registry.filter_by_category(LayoutCategory::Projection);
    // 2 Force (FR, Barnes-Hut), 8 Positional, 5 Extras, 1 Projection.
    assert!(force.len() >= 2);
    assert_eq!(positional.len(), 8);
    assert_eq!(extras.len(), 5);
    assert_eq!(projection.len(), 1);
}

#[test]
fn filter_by_tag_matches_expected_members() {
    let registry = LayoutRegistry::<Id>::default();
    let organic = registry.filter_by_tag("organic");
    // FR, Barnes-Hut, Phyllotaxis carry the `organic` tag.
    assert!(organic.len() >= 3);
    let fractal = registry.filter_by_tag("fractal");
    // Penrose + L-system.
    assert_eq!(fractal.len(), 2);
}

#[test]
fn filter_by_provenance_is_all_builtin_by_default() {
    let registry = LayoutRegistry::<Id>::default();
    let builtins = registry.filter_by_provenance(LayoutProvenance::Builtin);
    assert_eq!(builtins.len(), registry.len());
    let native_mods = registry.filter_by_provenance(LayoutProvenance::NativeMod);
    assert!(native_mods.is_empty());
}

#[test]
fn register_rejects_empty_id() {
    let mut registry = LayoutRegistry::<Id>::empty();
    struct BadProvider;
    impl LayoutProvider<Id> for BadProvider {
        fn capability(&self) -> LayoutCapability {
            LayoutCapability {
                id: "".into(),
                display_name: "x".into(),
                description: None,
                category: LayoutCategory::Force,
                is_deterministic: true,
                is_topology_sensitive: false,
                supports_3d: false,
                recommended_max_node_count: None,
                provenance: LayoutProvenance::Builtin,
                capability_tags: vec![],
            }
        }
        fn create_default(&self) -> Box<dyn DynLayout<Id>> {
            Box::new(crate::ForceDirected::default())
        }
    }
    let err = registry
        .register(Arc::new(BadProvider))
        .expect_err("empty id must be rejected");
    assert!(matches!(err, RegisterError::InvalidId(_)));
}

#[test]
fn register_rejects_duplicate_id() {
    let mut registry = LayoutRegistry::<Id>::default();
    let provider = Arc::new(BuiltinProvider::<crate::ForceDirected, Id>::new(
        force_directed_capability,
    ));
    let err = registry
        .register(provider)
        .expect_err("duplicate FR registration must fail");
    assert!(matches!(err, RegisterError::DuplicateId(_)));
}

#[test]
fn unregister_removes_provider() {
    let mut registry = LayoutRegistry::<Id>::default();
    assert!(registry.unregister("graph_layout:force_directed"));
    assert!(registry.resolve("graph_layout:force_directed").is_none());
}

#[test]
fn resolved_provider_creates_usable_layout() {
    let registry = LayoutRegistry::<Id>::default();
    let provider = registry
        .resolve("graph_layout:grid")
        .expect("grid must be registered");

    let capability = provider.capability();
    assert_eq!(capability.category, LayoutCategory::Positional);

    let mut layout = provider.create_default();
    let mut state = layout.default_state_erased();
    let viewport = CanvasViewport {
        rect: Rect::new(Point2D::new(0.0, 0.0), Size2D::new(1000.0, 1000.0)),
        scale_factor: 1.0,
    };
    let scene = CanvasSceneInput::<Id> {
        view_id: ViewId(0),
        nodes: (0..4u32)
            .map(|id| CanvasNode {
                id,
                position: Point2D::new(500.0 + id as f32 * 10.0, 500.0),
                radius: 16.0,
                label: None,
            })
            .collect(),
        edges: vec![],
        scene_objects: vec![],
        overlays: vec![],
        scene_mode: SceneMode::Browse,
        projection: ProjectionMode::default(),
    };
    let deltas = layout.step_dyn(&scene, &mut state, 0.0, &viewport, &LayoutExtras::default());
    assert!(!deltas.is_empty(), "grid should produce deltas");
}
