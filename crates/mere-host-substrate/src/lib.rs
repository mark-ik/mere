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
use mere_frame::{GraphId, PaneId, SessionId};
use mere_host_runtime::view_intent_store::{load_view_intent, save_view_intent};
use mere_host_runtime::{
    ActionBus, CameraSnapshot, GraphSessionManifest, LoadReport, ManifestStore, TileManager,
    ViewIntent,
};
use mere_kernel::graph::NodeKey;
use mere_renderer_registry::{DiagnosticEvent, DiagnosticSink, NodeContentKind, NodeIdentity, Placement};
use mere_spatial_prototype::{
    EdgeIdentity, ExternalTextureCompositor, SceneHit, SubstrateHost, SubstrateNode,
    SubstrateScene,
};

pub mod drop_zone;
pub mod frame_layout;

pub use drop_zone::infer_drop_side;
pub use frame_layout::{
    LeafBounds, SPLITTER_THICKNESS, SplitterBounds, SplitterDrag, compute_container_size,
    default_content_kind_for, walk_leaves, walk_splitters,
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
    /// Pointer hit landed on a pane-shaped substrate node, resolved
    /// back to its `PaneId`. Emitted when the scene was projected
    /// via `sync_scene_from_frame_layout` (frametree mode). Host
    /// typically translates to focus/activate actions on the pane.
    PaneClicked {
        pane_id: PaneId,
        host_pos: Point,
        scene_pos: Point,
    },
    /// Pointer hit landed on a splitter chrome node. Carries the
    /// `SplitPath` the host needs to snapshot a
    /// [`crate::SplitterDrag`] and start tracking cursor moves
    /// against `FrameLayout::set_split_ratio`. The host looks up
    /// the axis + current ratio via `FrameLayout::split_at(&path)`.
    SplitterClicked {
        path: mere_frame::SplitPath,
        host_pos: Point,
        scene_pos: Point,
    },
    /// Pointer hit landed on a tile-shaped substrate node, resolved
    /// back to its runtime `NodeKey`. Emitted when the scene was
    /// projected via `sync_scene_from_tiles` (tile-grid mode). Host
    /// typically translates to
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
    /// Pointer hit a substrate node but the host has no `PaneId` /
    /// `NodeKey` mapping for it (drift between scene and the
    /// identity maps — shouldn't happen if a sync_scene_from_* call
    /// is the only scene source). Reported as a degraded variant
    /// for diagnostic visibility rather than silently dropped.
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

    /// Sibling of [`tile_identity_map`] for the pane-level identity
    /// space. Populated by `sync_scene_from_frame_layout`; each leaf
    /// pane's `PaneId` maps to a stable `NodeIdentity` so producer
    /// handles + accessibility tree ids survive ratio adjustments
    /// and reparent ops that don't change leaf membership.
    pub(crate) pane_identity_map: HashMap<PaneId, NodeIdentity>,

    /// Identity map for splitter chrome — keyed by the
    /// [`SplitPath`] of the `PaneNode::Split` the splitter sits at.
    /// Populated alongside `pane_identity_map` by
    /// `sync_scene_from_frame_layout`. Drag handlers look up the
    /// split path from a `SplitterClicked` event's identity, then
    /// dispatch ratio updates against `FrameLayout::set_split_ratio`.
    pub(crate) splitter_identity_map: HashMap<mere_frame::SplitPath, NodeIdentity>,

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
            pane_identity_map: HashMap::new(),
            splitter_identity_map: HashMap::new(),
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

    /// Mint a fresh session: build a new `GraphSessionManifest`
    /// with a random `SessionId` + `GraphId`, insert it into the
    /// manifest store (marking it dirty so the next `flush_dirty`
    /// writes it), and set it as active. Returns the new session id.
    ///
    /// Common host bootstrap pattern: `bind_session_root` first; if
    /// the resulting `LoadReport` is empty, call `create_session()`
    /// to seed a default session. Otherwise pick one of the loaded
    /// ids and pass it to `activate_session`.
    pub fn create_session(&mut self) -> SessionId {
        let id = SessionId::from_uuid(uuid::Uuid::new_v4());
        let graph_id = GraphId::from_uuid(uuid::Uuid::new_v4());
        let manifest = GraphSessionManifest::new(id, graph_id);
        self.manifests.insert(manifest);
        self.active_session = Some(id);
        id
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

    /// Find the frame `PaneId` for a substrate `NodeIdentity`.
    /// Returns `None` if the identity isn't in the pane-identity map
    /// (the substrate scene wasn't projected from a `FrameLayout`,
    /// or the pane left the tree on the last sync).
    pub fn pane_id_for_identity(&self, identity: NodeIdentity) -> Option<PaneId> {
        self.pane_identity_map
            .iter()
            .find_map(|(k, v)| if *v == identity { Some(*k) } else { None })
    }

    /// Find the frame `SplitPath` for a substrate `NodeIdentity` —
    /// resolves splitter chrome clicks back to the Split node they
    /// sit at.
    pub fn split_path_for_identity(
        &self,
        identity: NodeIdentity,
    ) -> Option<mere_frame::SplitPath> {
        self.splitter_identity_map
            .iter()
            .find_map(|(k, v)| if *v == identity { Some(k.clone()) } else { None })
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
            Some(SceneHit::Node(identity)) => {
                // Splitter chrome wins over pane content — clicks on
                // the 4px boundary should grab the splitter even when
                // the cursor's also inside the abutting pane bounds.
                // Then pane (frametree mode), then tile (legacy /
                // probe mode), then unknown.
                if let Some(path) = self.split_path_for_identity(identity) {
                    SubstrateInputEvent::SplitterClicked {
                        path,
                        host_pos,
                        scene_pos,
                    }
                } else if let Some(pane_id) = self.pane_id_for_identity(identity) {
                    SubstrateInputEvent::PaneClicked {
                        pane_id,
                        host_pos,
                        scene_pos,
                    }
                } else if let Some(node_key) = self.node_key_for_identity(identity) {
                    SubstrateInputEvent::TileClicked {
                        node_key,
                        host_pos,
                        scene_pos,
                    }
                } else {
                    SubstrateInputEvent::UnknownTileHit {
                        identity,
                        host_pos,
                        scene_pos,
                    }
                }
            }
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
mod tests;
