// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! `mere-host-substrate` — first concrete substrate-as-host integration.
//!
//! Bridges:
//!
//! - `mere_spatial_prototype` — substrate IR (scene + renderer
//!   registry + camera + hit-test + AccessKit projection + diagnostics).
//! - `mere_host_runtime` — durable state (session manifest + tile
//!   manager + action bus + view intents).
//!
//! The bridge is the data-model projection: a `MereHostApp` owns
//! both halves and provides `sync_scene_from_tiles` to translate the
//! runtime's per-tile state into substrate scene nodes that get
//! dispatched through the registry on the next frame.
//!
//! ## v0a scope
//!
//! - `MereHostApp` struct bundling `SubstrateHost`, `SubstrateScene`,
//!   `ExternalTextureCompositor`, `TileManager`. Action bus +
//!   manifest store integration is a follow-up; the data-model
//!   bridge is the first piece because every other integration
//!   builds on it.
//! - `sync_scene_from_tiles` lays out open tiles in a row-major
//!   grid, each as a `NodeContentKind::Panel` substrate node.
//!   Preserves substrate identity across syncs via the
//!   `tile_identity_map` so producer handles stay valid frame to
//!   frame.
//! - Default tile size + grid layout are placeholders for cartography
//!   — when the cartography crate exists, the projection consults it
//!   instead of computing fixed positions.

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

use std::collections::HashMap;
use std::io;
use std::path::Path;

use kurbo::{Affine, Size};
use mere_host_runtime::view_intent_store::{load_view_intent, save_view_intent};
use mere_host_runtime::{CameraSnapshot, TileManager, ViewIntent};
use mere_kernel::graph::NodeKey;
use mere_renderer_registry::{NodeContentKind, NodeIdentity, Placement};
use mere_spatial_prototype::{
    ExternalTextureCompositor, SubstrateHost, SubstrateNode, SubstrateScene,
};

/// Per-tile dimensions used by `sync_scene_from_tiles`. Placeholder
/// until cartography supplies real layout.
pub const DEFAULT_TILE_WIDTH: f64 = 320.0;
pub const DEFAULT_TILE_HEIGHT: f64 = 240.0;
pub const DEFAULT_TILE_GAP: f64 = 32.0;
pub const DEFAULT_TILES_PER_ROW: usize = 3;

/// Substrate-as-host integration: owns both the substrate machinery
/// and the runtime's durable state.
pub struct MereHostApp {
    /// Substrate scene + registry dispatcher + camera.
    pub substrate: SubstrateHost,
    /// Current scene state, rebuilt by `sync_scene_from_tiles` from
    /// the tile manager. Public so callers can construct manual
    /// scenes for tests / probes.
    pub scene: SubstrateScene,
    /// External-texture compositor — registered + composed by
    /// `substrate.render_scene` for EmbeddedFrame nodes.
    pub compositor: ExternalTextureCompositor,
    /// Runtime's per-tile state (history, documents, surface state).
    pub tiles: TileManager,

    /// Preserves substrate identity across syncs. Each open tile's
    /// `NodeKey` maps to the same `NodeIdentity` until the tile
    /// closes, so producer handles + accessibility tree ids remain
    /// stable.
    tile_identity_map: HashMap<NodeKey, NodeIdentity>,
}

impl Default for MereHostApp {
    fn default() -> Self {
        Self::new()
    }
}

impl MereHostApp {
    pub fn new() -> Self {
        Self {
            substrate: SubstrateHost::with_default_registry(),
            scene: SubstrateScene::new(),
            compositor: ExternalTextureCompositor::new(),
            tiles: TileManager::new(),
            tile_identity_map: HashMap::new(),
        }
    }

    /// Project the tile manager's open-tile list into a new
    /// `SubstrateScene`, replacing `self.scene`. Tiles get a row-
    /// major grid placement, each as a `Panel` content-kind node.
    ///
    /// Identity stability: tiles that were in the scene before sync
    /// keep the same `NodeIdentity`; new tiles get fresh identities;
    /// closed tiles drop out of the map. Producer handles + AccessKit
    /// tree ids in the substrate stay stable across syncs.
    ///
    /// Layout is fixed-grid placeholder pending cartography. Hosts
    /// wanting custom layout call `sync_scene_from_tiles_with`
    /// (below) and supply a `LayoutFn`.
    pub fn sync_scene_from_tiles(&mut self) {
        self.sync_scene_from_tiles_with(default_grid_layout);
    }

    /// Same as `sync_scene_from_tiles` but lets the caller supply a
    /// `(index, total_count) -> (placement, size)` layout function.
    /// Useful for cartography integration / per-host policies.
    pub fn sync_scene_from_tiles_with<F>(&mut self, layout: F)
    where
        F: Fn(usize, usize) -> (Placement, Size),
    {
        let open: Vec<NodeKey> = self.tiles.open_tiles().to_vec();
        let total = open.len();
        let mut new_scene = SubstrateScene::new();
        let mut new_map = HashMap::with_capacity(total);
        for (index, node_key) in open.iter().enumerate() {
            let (placement, size) = layout(index, total);
            let identity = self
                .tile_identity_map
                .get(node_key)
                .copied()
                .unwrap_or_else(NodeIdentity::next);
            new_scene.insert(SubstrateNode {
                identity,
                placement,
                size,
                lod: mere_renderer_registry::LodLevel::FullPane,
                content_kind: NodeContentKind::Panel,
                renderer_pin: None,
            });
            new_map.insert(*node_key, identity);
        }
        self.scene = new_scene;
        self.tile_identity_map = new_map;
    }

    /// Substrate identity assigned to `node_key`, if the tile is
    /// currently open in the scene. Useful for callers that need to
    /// translate runtime-side tile addressing to substrate-side node
    /// addressing (e.g. routing input to a specific tile, or
    /// looking up a producer handle).
    pub fn identity_for_tile(&self, node_key: NodeKey) -> Option<NodeIdentity> {
        self.tile_identity_map.get(&node_key).copied()
    }

    /// Number of `NodeKey → NodeIdentity` mappings currently held.
    /// Equals the count of open tiles after the last sync.
    pub fn tracked_tile_count(&self) -> usize {
        self.tile_identity_map.len()
    }

    /// Snapshot the substrate's current camera as a `ViewIntent`
    /// fragment. Hidden-relation state stays default (substrate
    /// doesn't manage that yet). Useful for save flows: the host
    /// merges this snapshot with its existing
    /// `hidden_relations` before persisting.
    ///
    /// Identity cameras serialise as `None` to keep
    /// `ViewIntent::is_empty` true and let the save path skip the
    /// file.
    pub fn current_view_intent(&self) -> ViewIntent {
        ViewIntent {
            hidden_relations: Default::default(),
            camera: camera_snapshot_for_save(self.substrate.camera()),
        }
    }

    /// Apply a loaded `ViewIntent` to the substrate's camera. `None`
    /// or identity camera leaves the host's camera unchanged
    /// (typical for a fresh pane). Hidden-relations are not yet
    /// substrate-side state; this method ignores that field.
    pub fn apply_view_intent(&mut self, intent: &ViewIntent) {
        if let Some(snapshot) = intent.camera {
            self.substrate
                .set_camera(Affine::new(snapshot.coefficients));
        }
    }
}

/// `Some(snapshot)` for non-identity cameras; `None` for identity.
/// Skipping identity persists less I/O and keeps view-intent files
/// out of the way for panes the user hasn't moved.
fn camera_snapshot_for_save(camera: Affine) -> Option<CameraSnapshot> {
    let snapshot = CameraSnapshot {
        coefficients: camera.as_coeffs(),
    };
    if snapshot.is_identity() {
        None
    } else {
        Some(snapshot)
    }
}

impl MereHostApp {
    /// Persist the substrate's current camera (and any future
    /// substrate-side view state) to disk at
    /// `<session_dir>/views/<frame_id_str>/<pane_id>.json`.
    ///
    /// Returns `Ok(false)` when the snapshot would be empty (identity
    /// camera + no hidden relations) — the save path skips writing
    /// to keep the on-disk tree clean. Returns `Ok(true)` when a
    /// file was written.
    pub fn save_substrate_view_intent(
        &self,
        session_dir: &Path,
        frame_id_str: &str,
        pane_id: u64,
    ) -> io::Result<bool> {
        let intent = self.current_view_intent();
        if intent.is_empty() {
            return Ok(false);
        }
        save_view_intent(session_dir, frame_id_str, pane_id, &intent)?;
        Ok(true)
    }

    /// Load a per-pane view-intent file from disk and apply it to
    /// the substrate. Missing file → `Ok(false)` (fresh pane, host
    /// keeps its defaults); applied → `Ok(true)`. Malformed JSON
    /// surfaces as `Err`.
    pub fn load_substrate_view_intent(
        &mut self,
        session_dir: &Path,
        frame_id_str: &str,
        pane_id: u64,
    ) -> io::Result<bool> {
        let Some(intent) = load_view_intent(session_dir, frame_id_str, pane_id)? else {
            return Ok(false);
        };
        self.apply_view_intent(&intent);
        Ok(true)
    }
}

/// v0a placeholder layout: row-major grid, `DEFAULT_TILES_PER_ROW`
/// tiles per row, each `DEFAULT_TILE_WIDTH × DEFAULT_TILE_HEIGHT`
/// with `DEFAULT_TILE_GAP` between.
pub fn default_grid_layout(index: usize, _total: usize) -> (Placement, Size) {
    let col = (index % DEFAULT_TILES_PER_ROW) as f64;
    let row = (index / DEFAULT_TILES_PER_ROW) as f64;
    let x = DEFAULT_TILE_GAP + col * (DEFAULT_TILE_WIDTH + DEFAULT_TILE_GAP);
    let y = DEFAULT_TILE_GAP + row * (DEFAULT_TILE_HEIGHT + DEFAULT_TILE_GAP);
    let placement = Placement::translate(x, y);
    let size = Size::new(DEFAULT_TILE_WIDTH, DEFAULT_TILE_HEIGHT);
    (placement, size)
}

#[cfg(test)]
mod tests {
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
        let app = MereHostApp::new();
        assert!(app.scene.is_empty());
        assert_eq!(app.tracked_tile_count(), 0);
        assert_eq!(app.tiles.open_tiles().len(), 0);
    }

    #[test]
    fn sync_with_no_tiles_keeps_scene_empty() {
        let mut app = MereHostApp::new();
        app.sync_scene_from_tiles();
        assert!(app.scene.is_empty());
    }

    #[test]
    fn sync_projects_each_tile_to_a_substrate_node() {
        let mut app = MereHostApp::new();
        let keys = open_tiles_for(
            &mut app.tiles,
            &["https://a.example", "https://b.example", "https://c.example"],
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
        let mut app = MereHostApp::new();
        let keys = open_tiles_for(
            &mut app.tiles,
            &["https://a.example", "https://b.example"],
        );
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
        let mut app = MereHostApp::new();
        let keys = open_tiles_for(
            &mut app.tiles,
            &["https://a.example", "https://b.example"],
        );
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
        let mut app = MereHostApp::new();
        let _ = open_tiles_for(
            &mut app.tiles,
            &["https://a.example", "https://b.example", "https://c.example"],
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
        let app = MereHostApp::new();
        let intent = app.current_view_intent();
        assert!(intent.is_empty());
        assert!(intent.camera.is_none());
    }

    #[test]
    fn current_view_intent_pan_zoom_camera_is_not_empty() {
        let mut app = MereHostApp::new();
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
        let mut app = MereHostApp::new();
        // Build a saved intent that has a 2× zoom + (200, 100) pan.
        let camera = kurbo::Affine::translate((200.0, 100.0)) * kurbo::Affine::scale(2.0);
        let intent = ViewIntent {
            hidden_relations: Default::default(),
            camera: Some(mere_host_runtime::CameraSnapshot {
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
            .join("mere-host-substrate-tests")
            .join(uuid::Uuid::new_v4().to_string());
        let session_dir = test_root.join("session-1");
        let frame_id_str = "frame-a";
        let pane_id: u64 = 42;

        let mut app = MereHostApp::new();
        let camera = kurbo::Affine::translate((123.0, -45.5)) * kurbo::Affine::scale(0.75);
        app.substrate.set_camera(camera);

        // Save → file should be written.
        let saved = app
            .save_substrate_view_intent(&session_dir, frame_id_str, pane_id)
            .expect("save ok");
        assert!(saved, "non-identity camera should write a file");

        // Build a fresh app, load → camera restored.
        let mut app2 = MereHostApp::new();
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
            .join("mere-host-substrate-tests")
            .join(uuid::Uuid::new_v4().to_string());
        let session_dir = test_root.join("session-2");
        let app = MereHostApp::new();
        let saved = app
            .save_substrate_view_intent(&session_dir, "frame-a", 1)
            .expect("save ok");
        assert!(!saved, "identity camera should not write");
        // Cleanup (no file was created, but the dir might exist if
        // mere-host-runtime created intermediates — defensive cleanup).
        let _ = std::fs::remove_dir_all(&test_root);
    }

    #[test]
    fn load_missing_view_intent_is_a_clean_miss() {
        let test_root = std::env::temp_dir()
            .join("mere-host-substrate-tests")
            .join(uuid::Uuid::new_v4().to_string());
        let session_dir = test_root.join("nonexistent-session");
        let mut app = MereHostApp::new();
        let loaded = app
            .load_substrate_view_intent(&session_dir, "frame-z", 99)
            .expect("missing file is not an error");
        assert!(!loaded);
        assert_eq!(app.substrate.camera(), kurbo::Affine::IDENTITY);
    }

    #[test]
    fn apply_then_snapshot_round_trips_through_view_intent() {
        let mut app = MereHostApp::new();
        let camera = kurbo::Affine::translate((40.0, 60.0)) * kurbo::Affine::scale(1.5);
        app.substrate.set_camera(camera);

        let saved = app.current_view_intent();
        // Reset the app's camera, then re-apply.
        app.substrate.reset_camera();
        assert_eq!(app.substrate.camera(), kurbo::Affine::IDENTITY);
        app.apply_view_intent(&saved);
        assert_eq!(app.substrate.camera(), camera);
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
}
