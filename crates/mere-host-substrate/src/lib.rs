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
use std::path::{Path, PathBuf};

use kurbo::{Affine, Point, Size};
use mere_frame::SessionId;
use mere_host_runtime::view_intent_store::{load_view_intent, save_view_intent};
use mere_host_runtime::{
    ActionBus, CameraSnapshot, LoadReport, ManifestStore, TileManager, ViewIntent,
};
use mere_kernel::graph::NodeKey;
use mere_renderer_registry::{DiagnosticEvent, DiagnosticSink, NodeContentKind, NodeIdentity, Placement};
use mere_spatial_prototype::{
    EdgeIdentity, ExternalTextureCompositor, SceneHit, SubstrateHost, SubstrateNode,
    SubstrateScene,
};

/// Per-tile dimensions used by `sync_scene_from_tiles`. Placeholder
/// until cartography supplies real layout.
pub const DEFAULT_TILE_WIDTH: f64 = 320.0;
pub const DEFAULT_TILE_HEIGHT: f64 = 240.0;
pub const DEFAULT_TILE_GAP: f64 = 32.0;
pub const DEFAULT_TILES_PER_ROW: usize = 3;

/// Substrate-resolved input events the host translates into bus
/// actions. Substrate produces these from hit-tests + camera
/// inverses; the host wraps them in `BusAction`s with its own
/// pane/session/graph context.
#[derive(Clone, Debug)]
pub enum SubstrateInputEvent {
    /// Pointer hit landed on a tile-shaped substrate node, resolved
    /// back to its runtime `NodeKey`. Host typically translates to
    /// `BusAction { target: Pane(active), kind: FocusTile { index } }`
    /// or similar.
    TileClicked {
        node_key: NodeKey,
        host_pos: Point,
        scene_pos: Point,
    },
    /// Pointer hit a relation edge. Host typically opens an edge
    /// inspector / context menu rather than dispatching a bus action.
    EdgeClicked {
        edge: EdgeIdentity,
        host_pos: Point,
        scene_pos: Point,
    },
    /// Pointer hit a substrate node but the host has no `NodeKey`
    /// mapping for it (drift between scene and tile_identity_map —
    /// shouldn't happen if `sync_scene_from_tiles` is the only scene
    /// source). Reported as a degraded variant for diagnostic
    /// visibility rather than silently dropped.
    UnknownTileHit {
        identity: NodeIdentity,
        host_pos: Point,
        scene_pos: Point,
    },
    /// Pointer landed on substrate background (no node, no edge).
    /// Host typically clears tile focus / dismisses menus.
    BackgroundClicked { host_pos: Point, scene_pos: Point },
}

/// Boxed input-event callback. Owned by `MereHostApp`; replaced via
/// `set_input_callback`. `Fn` (not `FnMut`) keeps the callback
/// safely callable from `&self` paths; hosts needing mutable state
/// use interior mutability.
type InputCallback = Box<dyn Fn(SubstrateInputEvent) + Send + Sync + 'static>;

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

    /// Host-installed input callback. `handle_pointer_press` calls
    /// this when set; otherwise events are dropped silently (a
    /// reasonable default for tests and probes that don't care about
    /// click routing).
    input_callback: Option<InputCallback>,

    /// In-memory + on-disk store for `GraphSessionManifest`s.
    /// Bound to a sessions-root directory via `bind_session_root`;
    /// stays empty (no root, no manifests) until then.
    pub manifests: ManifestStore,

    /// Currently-active session id, if one has been chosen via
    /// `activate_session`. Drives `active_session_dir` and the
    /// `*_active_view_intent` save/load helpers.
    active_session: Option<SessionId>,

    /// Action bus — permission-checked dispatch with listener
    /// fan-out. Defaults to `PermitEverythingGate` with no
    /// listeners; hosts install policy via
    /// `action_bus.set_gate(...)` and handlers via
    /// `action_bus.add_listener(...)`.
    pub action_bus: ActionBus,
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
            input_callback: None,
            manifests: ManifestStore::new(),
            active_session: None,
            action_bus: ActionBus::with_permit_everything(),
        }
    }

    /// Bind the manifest store to a sessions-root directory and
    /// load every `manifest.json` found beneath it. Returns the
    /// runtime's `LoadReport` listing successful + malformed
    /// sessions for diagnostic emission.
    ///
    /// Missing root → `Ok(empty report)` (fresh install — host
    /// seeds a default session if it wants one). I/O errors on the
    /// root traversal surface as `Err`.
    pub fn bind_session_root(&mut self, root: impl Into<PathBuf>) -> io::Result<LoadReport> {
        self.manifests.load_from_disk(root)
    }

    /// Currently-active session id (set via `activate_session`).
    pub fn active_session_id(&self) -> Option<SessionId> {
        self.active_session
    }

    /// Mark a session as active. Returns `true` if the id exists in
    /// the manifest store; otherwise leaves the active session
    /// unchanged and returns `false`.
    pub fn activate_session(&mut self, id: SessionId) -> bool {
        if self.manifests.get(id).is_some() {
            self.active_session = Some(id);
            true
        } else {
            false
        }
    }

    /// Drop the active-session selection. Subsequent
    /// `active_session_dir` / `*_active_view_intent` calls return
    /// `None` / write nothing.
    pub fn deactivate_session(&mut self) {
        self.active_session = None;
    }

    /// On-disk directory for the active session, if both an active
    /// session and a bound manifest root are set. Path layout
    /// matches `ManifestStore::flush_dirty`: `<root>/<session_uuid>/`.
    pub fn active_session_dir(&self) -> Option<PathBuf> {
        let id = self.active_session?;
        let root = self.manifests.root()?;
        Some(root.join(id.as_uuid().to_string()))
    }

    /// Persist the substrate camera as a view-intent under the
    /// active session's directory. Returns `Ok(None)` when no
    /// active session / manifest root is set; otherwise returns
    /// `Ok(Some(written))` where `written` is the
    /// `save_substrate_view_intent` outcome (`false` = identity
    /// camera skipped).
    pub fn save_active_view_intent(
        &self,
        frame_id_str: &str,
        pane_id: u64,
    ) -> io::Result<Option<bool>> {
        let Some(dir) = self.active_session_dir() else {
            return Ok(None);
        };
        let written = self.save_substrate_view_intent(&dir, frame_id_str, pane_id)?;
        Ok(Some(written))
    }

    /// Load a view-intent sidecar from the active session's
    /// directory and apply it. Returns `Ok(None)` for no-active-
    /// session / no-bound-root; `Ok(Some(loaded))` otherwise.
    pub fn load_active_view_intent(
        &mut self,
        frame_id_str: &str,
        pane_id: u64,
    ) -> io::Result<Option<bool>> {
        let Some(dir) = self.active_session_dir() else {
            return Ok(None);
        };
        let loaded = self.load_substrate_view_intent(&dir, frame_id_str, pane_id)?;
        Ok(Some(loaded))
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

    /// Find the runtime `NodeKey` for a substrate `NodeIdentity`.
    /// Returns `None` if the identity isn't in the tile-identity map
    /// (the substrate scene has a node that didn't come from
    /// `sync_scene_from_tiles`, or a tile was closed mid-frame).
    pub fn node_key_for_identity(&self, identity: NodeIdentity) -> Option<NodeKey> {
        self.tile_identity_map
            .iter()
            .find_map(|(k, v)| if *v == identity { Some(*k) } else { None })
    }

    /// Install a closure that receives every `SubstrateInputEvent`
    /// the substrate resolves. Replaces any previously-installed
    /// callback. Pass `None`-ish by clearing via
    /// `clear_input_callback`.
    pub fn set_input_callback<F>(&mut self, callback: F)
    where
        F: Fn(SubstrateInputEvent) + Send + Sync + 'static,
    {
        self.input_callback = Some(Box::new(callback));
    }

    /// Drop the input callback. Subsequent `handle_pointer_press`
    /// calls drop events silently.
    pub fn clear_input_callback(&mut self) {
        self.input_callback = None;
    }

    /// Hit-test `host_pos` against the current scene + camera, emit
    /// the resolved `SubstrateInputEvent` through the installed
    /// callback (if any), and return it.
    ///
    /// This is the substrate-side seam between OS pointer events
    /// and the host's action-bus translation layer: the substrate
    /// resolves what was hit; the host translates the returned
    /// event into a `BusAction` with pane/session/graph context and
    /// dispatches it via [`Self::action_bus`].
    pub fn handle_pointer_press(&self, host_pos: Point) -> SubstrateInputEvent {
        let scene_pos = self.substrate.scene_pos_from_host(host_pos);
        let event = match self.scene.hit_test(scene_pos) {
            Some(SceneHit::Node(identity)) => match self.node_key_for_identity(identity) {
                Some(node_key) => SubstrateInputEvent::TileClicked {
                    node_key,
                    host_pos,
                    scene_pos,
                },
                None => SubstrateInputEvent::UnknownTileHit {
                    identity,
                    host_pos,
                    scene_pos,
                },
            },
            Some(SceneHit::Edge(edge)) => SubstrateInputEvent::EdgeClicked {
                edge,
                host_pos,
                scene_pos,
            },
            None => SubstrateInputEvent::BackgroundClicked { host_pos, scene_pos },
        };
        if let Some(cb) = &self.input_callback {
            cb(event.clone());
        }
        event
    }

    /// Open-order index of the tile with `node_key`, or `None` if
    /// the key isn't currently open. Helpful when constructing
    /// `ActionKind::FocusTile { index }` from a substrate hit.
    pub fn tile_index_for(&self, node_key: NodeKey) -> Option<usize> {
        self.tiles.open_tiles().iter().position(|k| *k == node_key)
    }

    /// Install a closure as the substrate registry's diagnostic
    /// sink. The closure receives every `DiagnosticEvent` the
    /// registry emits (renderer registration / unregistration /
    /// hot-swap, route-degraded misroutes) and routes them to the
    /// host's log / telemetry / action bus.
    ///
    /// Replaces any sink previously installed. Pass a no-op closure
    /// to silence the registry's diagnostic emissions:
    ///
    /// ```ignore
    /// app.set_diagnostic_callback(|_| {});
    /// ```
    ///
    /// The closure is wrapped in `CallbackSink`; bring your own
    /// `Box<dyn DiagnosticSink>` via
    /// `app.substrate.registry_mut().set_sink(...)` for richer sink
    /// state (event buffering, async dispatch, etc.).
    pub fn set_diagnostic_callback<F>(&mut self, callback: F)
    where
        F: Fn(DiagnosticEvent) + Send + Sync + 'static,
    {
        self.substrate
            .registry_mut()
            .set_sink(Box::new(CallbackSink::new(callback)));
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

/// `DiagnosticSink` impl that delegates to a host-supplied closure.
/// The closure runs synchronously inside the registry's emit path —
/// hosts wanting async / batched / deduped delivery should buffer
/// inside their closure (or implement `DiagnosticSink` directly with
/// richer state).
pub struct CallbackSink<F: Fn(DiagnosticEvent) + Send + Sync + 'static> {
    callback: F,
}

impl<F: Fn(DiagnosticEvent) + Send + Sync + 'static> CallbackSink<F> {
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<F: Fn(DiagnosticEvent) + Send + Sync + 'static> DiagnosticSink for CallbackSink<F> {
    fn record(&self, event: DiagnosticEvent) {
        (self.callback)(event);
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

    /// Per-test scratch directory under the OS temp dir, namespaced
    /// by a fresh UUID. Tests clean up on success; failures leave
    /// the dir for inspection.
    fn temp_session_root() -> PathBuf {
        std::env::temp_dir()
            .join("mere-host-substrate-tests")
            .join(uuid::Uuid::new_v4().to_string())
    }

    fn fresh_session_id() -> SessionId {
        SessionId::from_uuid(uuid::Uuid::new_v4())
    }

    fn fresh_graph_id() -> mere_frame::GraphId {
        mere_frame::GraphId::from_uuid(uuid::Uuid::new_v4())
    }

    fn seed_manifest_dir(root: &Path, manifest: &mere_host_runtime::GraphSessionManifest) {
        let session_dir = root.join(manifest.session_id.as_uuid().to_string());
        std::fs::create_dir_all(&session_dir).expect("create session dir");
        let json = serde_json::to_string_pretty(manifest).expect("serialise");
        std::fs::write(session_dir.join("manifest.json"), json).expect("write manifest");
    }

    #[test]
    fn bind_session_root_loads_existing_manifests() {
        let root = temp_session_root();
        let manifest =
            mere_host_runtime::GraphSessionManifest::new(fresh_session_id(), fresh_graph_id());
        let session_id = manifest.session_id;
        seed_manifest_dir(&root, &manifest);

        let mut app = MereHostApp::new();
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
        let mut app = MereHostApp::new();
        let report = app.bind_session_root(&root).expect("missing root is OK");
        assert!(report.loaded.is_empty());
        assert!(report.failed.is_empty());
    }

    #[test]
    fn activate_session_only_succeeds_for_known_ids() {
        let root = temp_session_root();
        let manifest =
            mere_host_runtime::GraphSessionManifest::new(fresh_session_id(), fresh_graph_id());
        let session_id = manifest.session_id;
        seed_manifest_dir(&root, &manifest);

        let mut app = MereHostApp::new();
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
            mere_host_runtime::GraphSessionManifest::new(fresh_session_id(), fresh_graph_id());
        let session_id = manifest.session_id;
        seed_manifest_dir(&root, &manifest);

        let mut app = MereHostApp::new();
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
            mere_host_runtime::GraphSessionManifest::new(fresh_session_id(), fresh_graph_id());
        let session_id = manifest.session_id;
        seed_manifest_dir(&root, &manifest);

        let mut app = MereHostApp::new();
        app.bind_session_root(&root).expect("bind ok");
        app.activate_session(session_id);
        app.substrate
            .set_camera(kurbo::Affine::translate((10.0, 20.0)));

        let written = app
            .save_active_view_intent("frame-a", 1)
            .expect("save ok");
        assert_eq!(written, Some(true));

        // Re-bind into a fresh app and confirm the file is reachable
        // via load_active_view_intent.
        let mut app2 = MereHostApp::new();
        app2.bind_session_root(&root).expect("rebind ok");
        app2.activate_session(session_id);
        let loaded = app2
            .load_active_view_intent("frame-a", 1)
            .expect("load ok");
        assert_eq!(loaded, Some(true));
        assert_eq!(
            app2.substrate.camera(),
            kurbo::Affine::translate((10.0, 20.0))
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_active_view_intent_without_active_session_is_none() {
        let app = MereHostApp::new();
        let result = app
            .save_active_view_intent("frame-a", 1)
            .expect("save ok");
        assert!(result.is_none(), "no active session → Ok(None)");
    }

    #[test]
    fn pointer_press_routes_tile_click_to_node_key() {
        use std::sync::{Arc, Mutex};

        let captured: Arc<Mutex<Vec<SubstrateInputEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let buf = captured.clone();

        let mut app = MereHostApp::new();
        let keys = open_tiles_for(
            &mut app.tiles,
            &["https://a.example", "https://b.example", "https://c.example"],
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

        let mut app = MereHostApp::new();
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

        let mut app = MereHostApp::new();
        let keys = open_tiles_for(&mut app.tiles, &["https://a.example"]);
        app.sync_scene_from_tiles();
        // Pan camera by (500, 300); first tile's host-space position
        // shifts from (32, 32) to (532, 332).
        app.substrate.set_camera(kurbo::Affine::translate((500.0, 300.0)));
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
        let mut app = MereHostApp::new();
        let _ = open_tiles_for(&mut app.tiles, &["https://a.example"]);
        app.sync_scene_from_tiles();
        // No callback set — should not panic.
        app.handle_pointer_press(Point::new(50.0, 50.0));
    }

    #[test]
    fn node_key_for_identity_returns_none_for_unknown() {
        let app = MereHostApp::new();
        assert!(app.node_key_for_identity(NodeIdentity::next()).is_none());
    }

    #[test]
    fn diagnostic_callback_receives_registry_events() {
        use std::sync::Arc;
        use std::sync::Mutex;

        use mere_renderer_registry::{
            CompositionMode, NodeContentKindSet, NodeRenderer, RendererCapabilities, RendererId,
        };
        use mere_spatial_prototype::RecordingRenderer;

        let captured: Arc<Mutex<Vec<DiagnosticEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let mut app = MereHostApp::new();
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
}
