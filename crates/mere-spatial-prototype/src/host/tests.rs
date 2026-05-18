// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Tests for [`super::SubstrateHost`]. Split out of `host.rs` to keep
//! the parent module under the workspace's 600-LOC ceiling.

use kurbo::{Affine, Size};
use mere_renderer_registry::{
    CompositionMode, InputDisposition, InputEvent, NodeContentKind, NodeContentKindSet,
    NodeRenderer, Placement, RendererCapabilities, RendererId, RendererRegistry,
};

use super::*;
use crate::recording_renderer::RecordingRenderer;
use crate::scene::{EdgeKind, EdgeStyle, SubstrateNode, SubstrateScene};

fn build_host_with_recording(kind: NodeContentKind) -> (SubstrateHost, RendererId) {
    let renderer = RecordingRenderer::for_kind("recording", kind);
    let id = renderer.id();
    let mut registry = RendererRegistry::with_default_selector();
    registry.register(Box::new(renderer)).unwrap();
    (SubstrateHost::new(registry), id)
}

#[test]
fn empty_scene_paints_nothing() {
    let mut host = SubstrateHost::with_default_registry();
    let scene = SubstrateScene::new();
    let mut target = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target, 1.0);
    assert_eq!(report.painted, 0);
    assert!(report.is_clean());
}

#[test]
fn dispatch_calls_matching_renderer() {
    let (mut host, _) = build_host_with_recording(NodeContentKind::DocumentTile);
    let mut scene = SubstrateScene::new();
    scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(10.0, 20.0),
        Size::new(80.0, 40.0),
    ));
    let mut target = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target, 1.0);
    assert_eq!(report.painted, 1);
    assert!(report.is_clean());
}

#[test]
fn dispatch_reports_missing_renderer_for_unknown_kind() {
    let (mut host, _) = build_host_with_recording(NodeContentKind::DocumentTile);
    let mut scene = SubstrateScene::new();
    scene.insert(SubstrateNode::new(
        NodeContentKind::GraphView, // unregistered
        Placement::IDENTITY,
        Size::new(10.0, 10.0),
    ));
    let mut target = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target, 1.0);
    assert_eq!(report.painted, 0);
    assert_eq!(report.missing_renderer, 1);
}

#[test]
fn dispatch_passes_placement_to_renderer() {
    let renderer = RecordingRenderer::for_kind("recording", NodeContentKind::Panel);
    let log = renderer.shared_paint_log();
    let mut registry = RendererRegistry::with_default_selector();
    registry.register(Box::new(renderer)).unwrap();
    let mut host = SubstrateHost::new(registry);

    let mut scene = SubstrateScene::new();
    scene.insert(SubstrateNode::new(
        NodeContentKind::Panel,
        Placement::translate(33.0, 77.0),
        Size::new(50.0, 50.0),
    ));
    let mut target = vello::Scene::new();
    let _ = host.paint_scene(&scene, &mut target, 1.5);

    let records = log.borrow();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].node_transform, Affine::translate((33.0, 77.0)));
    assert_eq!(records[0].scale_factor, 1.5);
}

#[test]
fn dispatch_reports_wrong_composition_mode() {
    // A renderer that advertises InScenePaint via composition_mode()
    // but returns None from as_in_scene_paint(). Pathological — but
    // the dispatch helper has to report it cleanly.
    struct MislabelledRenderer {
        id: RendererId,
    }
    impl NodeRenderer for MislabelledRenderer {
        fn renderer_id(&self) -> RendererId {
            self.id.clone()
        }
        fn handles(&self) -> NodeContentKindSet {
            NodeContentKindSet::from_one(NodeContentKind::WebPage)
        }
        fn composition_mode(&self) -> CompositionMode {
            CompositionMode::InScenePaint
        }
        fn capabilities(&self) -> RendererCapabilities {
            RendererCapabilities::NONE
        }
        // Deliberately doesn't override as_in_scene_paint — default
        // None — which is the bug the dispatch helper must surface.
    }

    let mut registry = RendererRegistry::with_default_selector();
    let id = RendererId::from_static("mislabelled");
    registry
        .register(Box::new(MislabelledRenderer { id: id.clone() }))
        .unwrap();
    let mut host = SubstrateHost::new(registry);

    let mut scene = SubstrateScene::new();
    scene.insert(SubstrateNode::new(
        NodeContentKind::WebPage,
        Placement::IDENTITY,
        Size::new(10.0, 10.0),
    ));
    let mut target = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target, 1.0);
    assert_eq!(report.painted, 0);
    assert_eq!(report.wrong_mode, vec![id]);
}

#[test]
fn deliver_input_at_routes_via_hit_test() {
    use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

    let renderer = RecordingRenderer::for_kind("recording", NodeContentKind::DocumentTile);
    let log = renderer.shared_input_log();
    let mut registry = RendererRegistry::with_default_selector();
    registry.register(Box::new(renderer)).unwrap();
    let mut host = SubstrateHost::new(registry);

    let mut scene = SubstrateScene::new();
    let id = scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(100.0, 200.0),
        Size::new(80.0, 40.0),
    ));

    let event = InputEvent::Pointer {
        position: kurbo::Point::new(0.0, 0.0), // overwritten by deliver_input_at
        kind: PointerEventKind::Down(PointerButton::Primary),
        modifiers: ModifiersState::default(),
    };
    // Click at (120, 220) — inside the rect at tile-local (20, 20).
    let disposition = host
        .deliver_input_at(&scene, kurbo::Point::new(120.0, 220.0), &event)
        .expect("dispatch ok");
    assert_eq!(disposition, Some(InputDisposition::Consumed));

    // Recording renderer captured the dispatched event with the
    // translated position.
    let records = log.borrow();
    assert_eq!(records.len(), 1);
    let (recorded_id, recorded_event) = &records[0];
    assert_eq!(*recorded_id, id);
    match recorded_event {
        InputEvent::Pointer { position, .. } => {
            assert_eq!(position.x, 20.0, "x translated to tile-local");
            assert_eq!(position.y, 20.0, "y translated to tile-local");
        }
        _ => panic!("expected Pointer event"),
    }
}

#[test]
fn deliver_input_at_returns_none_when_no_hit() {
    use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

    let (mut host, _) = build_host_with_recording(NodeContentKind::DocumentTile);
    let mut scene = SubstrateScene::new();
    scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(100.0, 200.0),
        Size::new(80.0, 40.0),
    ));

    let event = InputEvent::Pointer {
        position: kurbo::Point::new(0.0, 0.0),
        kind: PointerEventKind::Down(PointerButton::Primary),
        modifiers: ModifiersState::default(),
    };
    // Click far from the rect.
    let disposition = host
        .deliver_input_at(&scene, kurbo::Point::new(5.0, 5.0), &event)
        .expect("dispatch ok");
    assert_eq!(disposition, None);
}

#[test]
fn diagnostics_fire_for_register_unregister_and_misroute() {
    use mere_renderer_registry::{DiagnosticEvent, RecordingSink, RouteDegradedReason};
    use std::sync::Arc;

    let sink = Arc::new(RecordingSink::new());

    // Sink stored once on the registry. We borrow through `sink_view`
    // for assertions; the registry holds its own Box<dyn DiagnosticSink>
    // pointing at the same RecordingSink via shared ownership.
    struct SharedSink(Arc<RecordingSink>);
    impl mere_renderer_registry::DiagnosticSink for SharedSink {
        fn record(&self, event: DiagnosticEvent) {
            self.0.record(event);
        }
    }

    let mut registry =
        RendererRegistry::with_default_selector().with_sink(Box::new(SharedSink(sink.clone())));
    let renderer_id = RendererId::from_static("test");
    registry
        .register(Box::new(RecordingRenderer::for_kind(
            "test",
            NodeContentKind::DocumentTile,
        )))
        .unwrap();

    // First event: RendererRegistered with the right kinds.
    let events = sink.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        DiagnosticEvent::RendererRegistered { id, kinds } => {
            assert_eq!(*id, renderer_id);
            assert!(kinds.contains(&NodeContentKind::DocumentTile));
        }
        other => panic!("expected RendererRegistered, got {:?}", other),
    }

    // Now exercise a misroute: register a node with a content kind
    // no renderer handles.
    let mut host = SubstrateHost::new(registry);
    let mut scene = SubstrateScene::new();
    let unrouted = scene.insert(SubstrateNode::new(
        NodeContentKind::WebPage, // no renderer for this kind
        Placement::IDENTITY,
        Size::new(50.0, 50.0),
    ));
    let mut target = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target, 1.0);
    assert_eq!(report.missing_renderer, 1);

    // Second event: RouteDegraded { NoCandidates } for the unrouted node.
    let events = sink.events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            DiagnosticEvent::RouteDegraded {
                node,
                reason: RouteDegradedReason::NoCandidates,
            } if *node == unrouted
        )),
        "expected RouteDegraded NoCandidates for {:?}; got {:?}",
        unrouted,
        events
    );

    // Unregister + verify the corresponding event.
    host.registry_mut().unregister(&renderer_id);
    let events = sink.events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            DiagnosticEvent::RendererUnregistered { id } if *id == renderer_id
        )),
        "expected RendererUnregistered for {:?}; got {:?}",
        renderer_id,
        events
    );
}

#[test]
fn renderer_pin_overrides_default_selector() {
    // Register two renderers for the same NodeContentKind; the
    // default selector would pick the first-registered one. A
    // pinned node should route to the second.
    let first = RecordingRenderer::for_kind("first", NodeContentKind::DocumentTile);
    let second = RecordingRenderer::for_kind("second", NodeContentKind::DocumentTile);
    let first_log = first.shared_paint_log();
    let second_log = second.shared_paint_log();
    let mut registry = RendererRegistry::with_default_selector();
    registry.register(Box::new(first)).unwrap();
    registry.register(Box::new(second)).unwrap();
    let mut host = SubstrateHost::new(registry);

    let mut scene = SubstrateScene::new();
    // Unpinned node: should go to "first".
    scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::translate(10.0, 10.0),
        Size::new(50.0, 50.0),
    ));
    // Pinned to "second".
    scene.insert(
        SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::translate(100.0, 10.0),
            Size::new(50.0, 50.0),
        )
        .with_renderer_pin(RendererId::from_static("second")),
    );

    let mut target = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target, 1.0);
    assert_eq!(report.painted, 2);

    assert_eq!(first_log.borrow().len(), 1, "first received the unpinned node");
    assert_eq!(second_log.borrow().len(), 1, "second received the pinned node");
}

#[test]
fn renderer_pin_falls_through_when_pin_not_registered() {
    let renderer = RecordingRenderer::for_kind("only", NodeContentKind::DocumentTile);
    let log = renderer.shared_paint_log();
    let mut registry = RendererRegistry::with_default_selector();
    registry.register(Box::new(renderer)).unwrap();
    let mut host = SubstrateHost::new(registry);

    let mut scene = SubstrateScene::new();
    scene.insert(
        SubstrateNode::new(
            NodeContentKind::DocumentTile,
            Placement::IDENTITY,
            Size::new(50.0, 50.0),
        )
        .with_renderer_pin(RendererId::from_static("ghost")),
    );

    let mut target = vello::Scene::new();
    let report = host.paint_scene(&scene, &mut target, 1.0);
    // Pin doesn't match any registered renderer → falls through to
    // first-candidate ("only").
    assert_eq!(report.painted, 1);
    assert_eq!(log.borrow().len(), 1);
}

#[test]
fn edge_style_returns_default_when_no_override() {
    let host = SubstrateHost::with_default_registry();
    assert_eq!(host.edge_style(EdgeKind::Plain), EdgeStyle::PLAIN);
    assert_eq!(host.edge_style(EdgeKind::Reference), EdgeStyle::REFERENCE);
}

#[test]
fn set_edge_style_overrides_default() {
    use vello::peniko::Color;

    let mut host = SubstrateHost::with_default_registry();
    let custom = EdgeStyle {
        color: Color::from_rgba8(255, 0, 0, 255),
        width_px: 4.0,
    };
    host.set_edge_style(EdgeKind::Plain, custom);
    assert_eq!(host.edge_style(EdgeKind::Plain), custom);
    // Other kinds still default.
    assert_eq!(host.edge_style(EdgeKind::Reference), EdgeStyle::REFERENCE);
}

#[test]
fn clear_edge_style_returns_previous() {
    let mut host = SubstrateHost::with_default_registry();
    let prev = host.clear_edge_style(EdgeKind::Plain);
    assert_eq!(prev, None);
    host.set_edge_style(EdgeKind::Plain, EdgeStyle::REFERENCE);
    let prev = host.clear_edge_style(EdgeKind::Plain);
    assert_eq!(prev, Some(EdgeStyle::REFERENCE));
    // After clear, defaults restored.
    assert_eq!(host.edge_style(EdgeKind::Plain), EdgeStyle::PLAIN);
}

#[test]
fn camera_defaults_to_identity() {
    let host = SubstrateHost::with_default_registry();
    assert_eq!(host.camera(), Affine::IDENTITY);
}

#[test]
fn set_and_reset_camera() {
    let mut host = SubstrateHost::with_default_registry();
    host.set_camera(Affine::translate((100.0, 50.0)));
    assert_eq!(host.camera(), Affine::translate((100.0, 50.0)));
    host.reset_camera();
    assert_eq!(host.camera(), Affine::IDENTITY);
}

#[test]
fn pan_translates_camera() {
    let mut host = SubstrateHost::with_default_registry();
    host.pan(10.0, 20.0);
    host.pan(5.0, 5.0);
    // Pans compose; total is (15, 25).
    let mapped = host.camera() * kurbo::Point::new(0.0, 0.0);
    assert_eq!(mapped, kurbo::Point::new(15.0, 25.0));
}

#[test]
fn zoom_at_keeps_pivot_stationary() {
    let mut host = SubstrateHost::with_default_registry();
    let pivot = kurbo::Point::new(200.0, 150.0);
    host.zoom_at(pivot, 2.0);
    // The pivot point should map to itself.
    let mapped = host.camera() * pivot;
    assert!((mapped.x - pivot.x).abs() < 1e-9);
    assert!((mapped.y - pivot.y).abs() < 1e-9);
    // A scene point 50 units left of the pivot expands to 100 units left.
    let scene_pt = kurbo::Point::new(150.0, 150.0);
    let mapped_pt = host.camera() * scene_pt;
    assert!((mapped_pt.x - 100.0).abs() < 1e-9);
}

#[test]
fn scene_pos_from_host_inverts_camera() {
    let mut host = SubstrateHost::with_default_registry();
    host.set_camera(Affine::translate((100.0, 50.0)) * Affine::scale(2.0));
    // Host (200, 100) → through camera⁻¹ → scene (50, 25).
    let scene_pt = host.scene_pos_from_host(kurbo::Point::new(200.0, 100.0));
    assert!((scene_pt.x - 50.0).abs() < 1e-9);
    assert!((scene_pt.y - 25.0).abs() < 1e-9);
}

#[test]
fn deliver_input_at_respects_camera() {
    use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

    let renderer = RecordingRenderer::for_kind("rec", NodeContentKind::DocumentTile);
    let log = renderer.shared_input_log();
    let mut registry = RendererRegistry::with_default_selector();
    registry.register(Box::new(renderer)).unwrap();
    let mut host = SubstrateHost::new(registry);
    // Pan the camera so scene-space (0, 0) sits at host-space (200, 100).
    host.set_camera(Affine::translate((200.0, 100.0)));

    let mut scene = SubstrateScene::new();
    let id = scene.insert(SubstrateNode::new(
        NodeContentKind::DocumentTile,
        Placement::IDENTITY, // node at scene origin
        Size::new(80.0, 40.0),
    ));

    let event = InputEvent::Pointer {
        position: kurbo::Point::ZERO,
        kind: PointerEventKind::Down(PointerButton::Primary),
        modifiers: ModifiersState::default(),
    };
    // Click at host-space (220, 110), which maps to scene-space
    // (20, 10) — inside the panned node at tile-local (20, 10).
    let disposition = host
        .deliver_input_at(&scene, kurbo::Point::new(220.0, 110.0), &event)
        .expect("dispatch ok");
    assert_eq!(disposition, Some(InputDisposition::Consumed));
    let records = log.borrow();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].0, id);
    match &records[0].1 {
        InputEvent::Pointer { position, .. } => {
            assert!((position.x - 20.0).abs() < 1e-9, "x tile-local");
            assert!((position.y - 10.0).abs() < 1e-9, "y tile-local");
        }
        _ => panic!("expected pointer"),
    }
}

#[test]
fn deliver_input_at_picks_topmost_on_overlap() {
    use mere_renderer_registry::{ModifiersState, PointerButton, PointerEventKind};

    // Two renderers, two content kinds. The CustomCanvas background
    // and the Panel both cover (50, 50). hit-test should route to
    // the Panel (topmost).
    let bg_renderer = RecordingRenderer::for_kind("bg", NodeContentKind::CustomCanvas);
    let panel_renderer = RecordingRenderer::for_kind("panel", NodeContentKind::Panel);
    let bg_log = bg_renderer.shared_input_log();
    let panel_log = panel_renderer.shared_input_log();
    let mut registry = RendererRegistry::with_default_selector();
    registry.register(Box::new(bg_renderer)).unwrap();
    registry.register(Box::new(panel_renderer)).unwrap();
    let mut host = SubstrateHost::new(registry);

    let mut scene = SubstrateScene::new();
    scene.insert(SubstrateNode::new(
        NodeContentKind::CustomCanvas,
        Placement::IDENTITY,
        Size::new(200.0, 200.0),
    ));
    let panel_id = scene.insert(SubstrateNode::new(
        NodeContentKind::Panel,
        Placement::translate(40.0, 40.0),
        Size::new(100.0, 100.0),
    ));

    let event = InputEvent::Pointer {
        position: kurbo::Point::new(0.0, 0.0),
        kind: PointerEventKind::Down(PointerButton::Primary),
        modifiers: ModifiersState::default(),
    };
    host.deliver_input_at(&scene, kurbo::Point::new(60.0, 60.0), &event)
        .expect("dispatch ok");

    assert_eq!(bg_log.borrow().len(), 0, "background not hit");
    assert_eq!(panel_log.borrow().len(), 1, "panel hit");
    assert_eq!(panel_log.borrow()[0].0, panel_id);
}
