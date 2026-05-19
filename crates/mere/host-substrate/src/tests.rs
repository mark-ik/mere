// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Test module for `host-substrate`. Split out of `lib.rs` to
//! keep the production half under the workspace's 600-LOC ceiling
//! (per `feedback_mere_file_size_ceiling`).

use inker::{DocumentProvenance, DocumentTrustState, EngineDocument};
use petgraph::graph::NodeIndex;

use super::*;

fn fake_document(url: &str) -> EngineDocument {
    EngineDocument {
        address: url.to_string(),
        title: None,
        content_type: "text/html".to_string(),
        lang: None,
        provenance: DocumentProvenance::default(),
        trust: DocumentTrustState::default(),
        diagnostics: Vec::new(),
        blocks: Vec::new(),
    }
}

/// Open one tile per URL in `tiles`, returning the keys in
/// open-order. NodeKeys are synthesised directly via
/// `NodeIndex::new`; the substrate doesn't see the kernel graph,
/// so any fresh sequence of indices works.
fn open_tiles_for(tiles: &mut TileManager, urls: &[&str]) -> Vec<NodeKey> {
    let mut keys = Vec::new();
    for (i, url) in urls.iter().enumerate() {
        let key = NodeIndex::new(i);
        keys.push(key);
        tiles.open_or_focus(key, url.to_string(), fake_document(url));
    }
    keys
}

#[test]
fn new_app_has_empty_scene_and_tile_map() {
    let app = HostApp::new();
    assert!(app.scene.is_empty());
    assert_eq!(app.tracked_tile_count(), 0);
    assert_eq!(app.tiles.open_tiles().len(), 0);
}

#[test]
fn sync_with_no_tiles_keeps_scene_empty() {
    let mut app = HostApp::new();
    app.sync_scene_from_tiles();
    assert!(app.scene.is_empty());
}

#[test]
fn sync_projects_each_tile_to_a_substrate_node() {
    let mut app = HostApp::new();
    let keys = open_tiles_for(
        &mut app.tiles,
        &[
            "https://a.example",
            "https://b.example",
            "https://c.example",
        ],
    );
    app.sync_scene_from_tiles();
    assert_eq!(app.scene.len(), 3);
    // Identity map populated for each open tile.
    assert_eq!(app.tracked_tile_count(), 3);
    for key in &keys {
        assert!(app.identity_for_tile(*key).is_some());
    }
}

#[test]
fn sync_preserves_identity_across_calls() {
    let mut app = HostApp::new();
    let keys = open_tiles_for(&mut app.tiles, &["https://a.example", "https://b.example"]);
    app.sync_scene_from_tiles();
    let id_a_first = app.identity_for_tile(keys[0]).unwrap();
    let id_b_first = app.identity_for_tile(keys[1]).unwrap();
    // Sync again — identities should match.
    app.sync_scene_from_tiles();
    assert_eq!(app.identity_for_tile(keys[0]), Some(id_a_first));
    assert_eq!(app.identity_for_tile(keys[1]), Some(id_b_first));
}

#[test]
fn closing_a_tile_drops_its_identity_on_next_sync() {
    let mut app = HostApp::new();
    let keys = open_tiles_for(&mut app.tiles, &["https://a.example", "https://b.example"]);
    app.sync_scene_from_tiles();
    assert_eq!(app.tracked_tile_count(), 2);

    // close_index closes by position in open-order.
    app.tiles.close_index(0);
    app.sync_scene_from_tiles();
    assert_eq!(app.tracked_tile_count(), 1);
    assert!(app.identity_for_tile(keys[0]).is_none());
    assert!(app.identity_for_tile(keys[1]).is_some());
}

#[test]
fn custom_layout_function_runs_per_tile() {
    let mut app = HostApp::new();
    let _ = open_tiles_for(
        &mut app.tiles,
        &[
            "https://a.example",
            "https://b.example",
            "https://c.example",
        ],
    );
    // Stack tiles vertically with a fixed 100×80 size.
    app.sync_scene_from_tiles_with(|index, _total| {
        let placement = Placement::translate(0.0, (index as f64) * 100.0);
        let size = Size::new(100.0, 80.0);
        (placement, size)
    });
    // Verify the third node ended up at y=200.
    let third = app.scene.iter().nth(2).unwrap();
    assert_eq!(third.placement.transform.translation().y, 200.0);
    assert_eq!(third.size, Size::new(100.0, 80.0));
}

#[test]
fn current_view_intent_identity_camera_is_empty() {
    let app = HostApp::new();
    let intent = app.current_view_intent();
    assert!(intent.is_empty());
    assert!(intent.camera.is_none());
}

#[test]
fn current_view_intent_pan_zoom_camera_is_not_empty() {
    let mut app = HostApp::new();
    app.substrate
        .set_camera(kurbo::Affine::translate((100.0, 50.0)));
    let intent = app.current_view_intent();
    assert!(!intent.is_empty());
    let snapshot = intent.camera.unwrap();
    // Translation is at coefficients[4], [5].
    assert_eq!(snapshot.coefficients[4], 100.0);
    assert_eq!(snapshot.coefficients[5], 50.0);
}

#[test]
fn apply_view_intent_restores_camera_from_snapshot() {
    let mut app = HostApp::new();
    // Build a saved intent that has a 2× zoom + (200, 100) pan.
    let camera = kurbo::Affine::translate((200.0, 100.0)) * kurbo::Affine::scale(2.0);
    let intent = ViewIntent {
        hidden_relations: Default::default(),
        camera: Some(session_runtime::CameraSnapshot {
            coefficients: camera.as_coeffs(),
        }),
    };
    app.apply_view_intent(&intent);
    assert_eq!(app.substrate.camera(), camera);
}

#[test]
fn save_then_load_round_trips_camera_through_disk() {
    // Per-test temp directory under cargo's target dir — avoids
    // depending on `tempfile` and survives parallel test runs by
    // namespacing on a fresh UUID per test invocation.
    let test_root = std::env::temp_dir()
        .join("host-substrate-tests")
        .join(uuid::Uuid::new_v4().to_string());
    let session_dir = test_root.join("session-1");
    let frame_id_str = "frame-a";
    let pane_id: u64 = 42;

    let mut app = HostApp::new();
    let camera = kurbo::Affine::translate((123.0, -45.5)) * kurbo::Affine::scale(0.75);
    app.substrate.set_camera(camera);

    // Save → file should be written.
    let saved = app
        .save_substrate_view_intent(&session_dir, frame_id_str, pane_id)
        .expect("save ok");
    assert!(saved, "non-identity camera should write a file");

    // Build a fresh app, load → camera restored.
    let mut app2 = HostApp::new();
    assert_eq!(app2.substrate.camera(), kurbo::Affine::IDENTITY);
    let loaded = app2
        .load_substrate_view_intent(&session_dir, frame_id_str, pane_id)
        .expect("load ok");
    assert!(loaded);
    assert_eq!(app2.substrate.camera(), camera);

    // Cleanup.
    let _ = std::fs::remove_dir_all(&test_root);
}

#[test]
fn save_identity_camera_skips_file() {
    let test_root = std::env::temp_dir()
        .join("host-substrate-tests")
        .join(uuid::Uuid::new_v4().to_string());
    let session_dir = test_root.join("session-2");
    let app = HostApp::new();
    let saved = app
        .save_substrate_view_intent(&session_dir, "frame-a", 1)
        .expect("save ok");
    assert!(!saved, "identity camera should not write");
    // Cleanup (no file was created, but the dir might exist if
    // host-runtime created intermediates — defensive cleanup).
    let _ = std::fs::remove_dir_all(&test_root);
}

#[test]
fn load_missing_view_intent_is_a_clean_miss() {
    let test_root = std::env::temp_dir()
        .join("host-substrate-tests")
        .join(uuid::Uuid::new_v4().to_string());
    let session_dir = test_root.join("nonexistent-session");
    let mut app = HostApp::new();
    let loaded = app
        .load_substrate_view_intent(&session_dir, "frame-z", 99)
        .expect("missing file is not an error");
    assert!(!loaded);
    assert_eq!(app.substrate.camera(), kurbo::Affine::IDENTITY);
}

#[test]
fn apply_then_snapshot_round_trips_through_view_intent() {
    let mut app = HostApp::new();
    let camera = kurbo::Affine::translate((40.0, 60.0)) * kurbo::Affine::scale(1.5);
    app.substrate.set_camera(camera);

    let saved = app.current_view_intent();
    // Reset the app's camera, then re-apply.
    app.substrate.reset_camera();
    assert_eq!(app.substrate.camera(), kurbo::Affine::IDENTITY);
    app.apply_view_intent(&saved);
    assert_eq!(app.substrate.camera(), camera);
}

/// Per-test scratch directory under the OS temp dir, namespaced
/// by a fresh UUID. Tests clean up on success; failures leave
/// the dir for inspection.
fn temp_session_root() -> PathBuf {
    std::env::temp_dir()
        .join("host-substrate-tests")
        .join(uuid::Uuid::new_v4().to_string())
}

fn fresh_session_id() -> SessionId {
    SessionId::from_uuid(uuid::Uuid::new_v4())
}

fn fresh_graph_id() -> frame::GraphId {
    frame::GraphId::from_uuid(uuid::Uuid::new_v4())
}

fn seed_manifest_dir(root: &Path, manifest: &session_runtime::GraphSessionManifest) {
    let session_dir = root.join(manifest.session_id.as_uuid().to_string());
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    let json = serde_json::to_string_pretty(manifest).expect("serialise");
    std::fs::write(session_dir.join("manifest.json"), json).expect("write manifest");
}

#[test]
fn bind_session_root_loads_existing_manifests() {
    let root = temp_session_root();
    let manifest =
        session_runtime::GraphSessionManifest::new(fresh_session_id(), fresh_graph_id());
    let session_id = manifest.session_id;
    seed_manifest_dir(&root, &manifest);

    let mut app = HostApp::new();
    let report = app.bind_session_root(&root).expect("bind ok");
    assert_eq!(report.loaded.len(), 1);
    assert_eq!(report.loaded[0], session_id);
    assert_eq!(report.failed.len(), 0);
    assert!(app.manifests.get(session_id).is_some());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn bind_session_root_missing_root_returns_empty_report() {
    let root = temp_session_root().join("nonexistent");
    let mut app = HostApp::new();
    let report = app.bind_session_root(&root).expect("missing root is OK");
    assert!(report.loaded.is_empty());
    assert!(report.failed.is_empty());
}

#[test]
fn create_session_inserts_manifest_and_activates_it() {
    let mut app = HostApp::new();
    let id = app.create_session();
    assert_eq!(app.manifests.len(), 1);
    assert!(app.manifests.get(id).is_some());
    assert!(app.manifests.is_dirty(id));
    assert_eq!(app.active_session_id(), Some(id));
}

#[test]
fn create_session_then_flush_writes_manifest_to_disk() {
    let root = temp_session_root();
    let mut app = HostApp::new();
    app.manifests.set_root(&root);
    let id = app.create_session();
    assert_eq!(app.manifests.flush_dirty().expect("flush"), 1);
    assert!(
        root.join(id.as_uuid().to_string())
            .join("manifest.json")
            .is_file()
    );

    let mut app2 = HostApp::new();
    let report = app2.bind_session_root(&root).expect("rebind");
    assert_eq!(report.loaded, vec![id]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn activate_session_only_succeeds_for_known_ids() {
    let root = temp_session_root();
    let manifest =
        session_runtime::GraphSessionManifest::new(fresh_session_id(), fresh_graph_id());
    let session_id = manifest.session_id;
    seed_manifest_dir(&root, &manifest);

    let mut app = HostApp::new();
    app.bind_session_root(&root).expect("bind ok");

    assert!(app.active_session_id().is_none());
    assert!(!app.activate_session(fresh_session_id()), "unknown id");
    assert!(app.active_session_id().is_none());

    assert!(app.activate_session(session_id));
    assert_eq!(app.active_session_id(), Some(session_id));

    app.deactivate_session();
    assert!(app.active_session_id().is_none());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn active_session_dir_matches_manifest_path_layout() {
    let root = temp_session_root();
    let manifest =
        session_runtime::GraphSessionManifest::new(fresh_session_id(), fresh_graph_id());
    let session_id = manifest.session_id;
    seed_manifest_dir(&root, &manifest);

    let mut app = HostApp::new();
    app.bind_session_root(&root).expect("bind ok");
    assert!(app.active_session_dir().is_none(), "no active session yet");
    app.activate_session(session_id);
    let dir = app.active_session_dir().expect("active dir");
    assert_eq!(dir, root.join(session_id.as_uuid().to_string()));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn save_active_view_intent_writes_under_session_dir() {
    let root = temp_session_root();
    let manifest =
        session_runtime::GraphSessionManifest::new(fresh_session_id(), fresh_graph_id());
    let session_id = manifest.session_id;
    seed_manifest_dir(&root, &manifest);

    let mut app = HostApp::new();
    app.bind_session_root(&root).expect("bind ok");
    app.activate_session(session_id);
    app.substrate
        .set_camera(kurbo::Affine::translate((10.0, 20.0)));

    let written = app.save_active_view_intent("frame-a", 1).expect("save ok");
    assert_eq!(written, Some(true));

    // Re-bind into a fresh app and confirm the file is reachable
    // via load_active_view_intent.
    let mut app2 = HostApp::new();
    app2.bind_session_root(&root).expect("rebind ok");
    app2.activate_session(session_id);
    let loaded = app2.load_active_view_intent("frame-a", 1).expect("load ok");
    assert_eq!(loaded, Some(true));
    assert_eq!(
        app2.substrate.camera(),
        kurbo::Affine::translate((10.0, 20.0))
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn save_active_view_intent_without_active_session_is_none() {
    let app = HostApp::new();
    let result = app.save_active_view_intent("frame-a", 1).expect("save ok");
    assert!(result.is_none(), "no active session → Ok(None)");
}

#[test]
fn pointer_press_routes_tile_click_to_node_key() {
    use std::sync::{Arc, Mutex};

    let captured: Arc<Mutex<Vec<SubstrateInputEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let buf = captured.clone();

    let mut app = HostApp::new();
    let keys = open_tiles_for(
        &mut app.tiles,
        &[
            "https://a.example",
            "https://b.example",
            "https://c.example",
        ],
    );
    app.sync_scene_from_tiles();
    app.set_input_callback(move |event| {
        buf.lock().expect("lock").push(event);
    });

    // First tile sits at grid (col=0, row=0), origin
    // (DEFAULT_TILE_GAP, DEFAULT_TILE_GAP). Click inside its rect.
    let click = Point::new(DEFAULT_TILE_GAP + 50.0, DEFAULT_TILE_GAP + 50.0);
    app.handle_pointer_press(click);

    let events = captured.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    match &events[0] {
        SubstrateInputEvent::TileClicked {
            node_key,
            host_pos,
            scene_pos,
        } => {
            assert_eq!(*node_key, keys[0]);
            assert_eq!(*host_pos, click);
            // Identity camera → scene_pos equals host_pos.
            assert_eq!(*scene_pos, click);
        }
        other => panic!("expected TileClicked, got {:?}", other),
    }
}

#[test]
fn pointer_press_on_background_emits_background_event() {
    use std::sync::{Arc, Mutex};

    let captured: Arc<Mutex<Vec<SubstrateInputEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let buf = captured.clone();

    let mut app = HostApp::new();
    let _ = open_tiles_for(&mut app.tiles, &["https://a.example"]);
    app.sync_scene_from_tiles();
    app.set_input_callback(move |event| {
        buf.lock().expect("lock").push(event);
    });

    // Click well outside any tile rect.
    app.handle_pointer_press(Point::new(2000.0, 2000.0));
    let events = captured.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        SubstrateInputEvent::BackgroundClicked { .. }
    ));
}

#[test]
fn pointer_press_under_panned_camera_uses_inverse() {
    use std::sync::{Arc, Mutex};

    let captured: Arc<Mutex<Vec<SubstrateInputEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let buf = captured.clone();

    let mut app = HostApp::new();
    let keys = open_tiles_for(&mut app.tiles, &["https://a.example"]);
    app.sync_scene_from_tiles();
    // Pan camera by (500, 300); first tile's host-space position
    // shifts from (32, 32) to (532, 332).
    app.substrate
        .set_camera(kurbo::Affine::translate((500.0, 300.0)));
    app.set_input_callback(move |event| {
        buf.lock().expect("lock").push(event);
    });

    // Click inside the panned tile's host-space rect.
    app.handle_pointer_press(Point::new(532.0 + 50.0, 332.0 + 50.0));
    let events = captured.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    match &events[0] {
        SubstrateInputEvent::TileClicked {
            node_key,
            scene_pos,
            ..
        } => {
            assert_eq!(*node_key, keys[0]);
            // scene_pos is the camera-inverse of host_pos:
            // host (582, 382) - pan (500, 300) = scene (82, 82).
            assert!((scene_pos.x - 82.0).abs() < 1e-9);
            assert!((scene_pos.y - 82.0).abs() < 1e-9);
        }
        other => panic!("expected TileClicked, got {:?}", other),
    }
}

#[test]
fn pointer_press_without_callback_is_a_silent_drop() {
    let mut app = HostApp::new();
    let _ = open_tiles_for(&mut app.tiles, &["https://a.example"]);
    app.sync_scene_from_tiles();
    // No callback set — should not panic.
    app.handle_pointer_press(Point::new(50.0, 50.0));
}

#[test]
fn node_key_for_identity_returns_none_for_unknown() {
    let app = HostApp::new();
    assert!(app.node_key_for_identity(NodeIdentity::next()).is_none());
}

#[test]
fn diagnostic_callback_receives_registry_events() {
    use std::sync::Arc;
    use std::sync::Mutex;

    use register_renderer::{
        CompositionMode, NodeContentKindSet, NodeRenderer, RendererCapabilities, RendererId,
    };
    use spatial_substrate::RecordingRenderer;

    let captured: Arc<Mutex<Vec<DiagnosticEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let mut app = HostApp::new();
    let sink_buf = captured.clone();
    app.set_diagnostic_callback(move |event| {
        sink_buf.lock().expect("sink lock").push(event);
    });

    // Register a renderer — should produce one RendererRegistered.
    let renderer = RecordingRenderer::for_kind("test.cb", NodeContentKind::DocumentTile);
    let id = renderer.id();
    app.substrate
        .registry_mut()
        .register(Box::new(renderer))
        .expect("register");

    let events = captured.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "exactly one event captured");
    match &events[0] {
        DiagnosticEvent::RendererRegistered { id: e_id, kinds } => {
            assert_eq!(*e_id, id);
            assert!(kinds.contains(&NodeContentKind::DocumentTile));
        }
        other => panic!("expected RendererRegistered, got {:?}", other),
    }

    // Unregister — second event.
    app.substrate.registry_mut().unregister(&id);
    let events = captured.lock().unwrap().clone();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1],
        DiagnosticEvent::RendererUnregistered { .. }
    ));

    // Silence the callback by installing a no-op. Subsequent
    // registration emits nothing visible to the prior buffer.
    let buf_len_before = captured.lock().unwrap().len();
    app.set_diagnostic_callback(|_| {});
    // Suppress unused warnings for the trait items we only used
    // above; this keeps the test file consistent if linters trim.
    let _: fn(CompositionMode) = |_| {};
    let _: NodeContentKindSet = NodeContentKindSet::new();
    let _: fn(&dyn NodeRenderer) = |_| {};
    let _: fn(RendererCapabilities) = |_| {};
    let _: fn(RendererId) = |_| {};
    app.substrate
        .registry_mut()
        .register(Box::new(RecordingRenderer::for_kind(
            "test.silenced",
            NodeContentKind::Panel,
        )))
        .expect("register again");
    assert_eq!(captured.lock().unwrap().len(), buf_len_before);
}

#[test]
fn default_grid_layout_is_row_major() {
    let (p0, s) = default_grid_layout(0, 6);
    assert_eq!(s, Size::new(DEFAULT_TILE_WIDTH, DEFAULT_TILE_HEIGHT));
    assert_eq!(p0.transform.translation().x, DEFAULT_TILE_GAP);
    assert_eq!(p0.transform.translation().y, DEFAULT_TILE_GAP);

    // Fourth tile is the first of the second row.
    let (p_next_row, _) = default_grid_layout(DEFAULT_TILES_PER_ROW, 6);
    assert_eq!(p_next_row.transform.translation().x, DEFAULT_TILE_GAP);
    assert_eq!(
        p_next_row.transform.translation().y,
        DEFAULT_TILE_GAP * 2.0 + DEFAULT_TILE_HEIGHT
    );
}
