/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! meerkat-shell: the on-screen serval host for Mere's chrome.
//!
//! A winit window that runs the reused chrome ([`meerkat::chrome_view`] over a
//! [`meerkat::Chrome`] wrapping the graphshell `ToolbarState`) through serval and
//! presents via netrender. Its `ScriptedDom → Scene` render glue (the
//! `serval_render` module: [`crate::serval_render::scene_from_session`] and
//! the point→node [`crate::serval_render::hit_test_node`]) calls serval-layout +
//! paint_list_render directly, so this file is the window + present +
//! input-dispatch harness, not a second engine.
//!
//! ## One shell document, one window
//!
//! The window draws one **shell document**: a single `ScriptedDom` under one
//! [`ServalAppRunner`], holding the chrome (toolbar, omnibar, palette, overlays),
//! the folded panes (roster, apparatus, steward, inspector, trail) as lensed
//! subtrees, and the orrery's gnodes as transform-positioned DOM. That
//! document runs through serval-layout into one chrome `Scene`. Around and beneath
//! it the host composites separate surfaces that are not serval documents: the
//! orrery graph scene ([`Orrery`]'s own `Scene` of gnodes / edges / physics from
//! `gyre`), the pelt workbench tile surface, and the focused node's content card.
//! Each rasterizes to its own texture and composites back to front: the orrery
//! scene underneath, the chrome on top. The capability-separation discipline holds
//! (neither the shell document nor a content surface sees the other's tree).
//!
//! Input routes top-down through the shell hit-test first: a press resolves
//! against the one document (chrome control, folded-pane row, or orrery card), and
//! an orrery-area miss falls through to `gyre`'s pan / zoom / drag / select.
//! Keyboard goes through the runner's `dispatch_key` (Tab traversal, Enter / Space
//! activation) for focusables, then to the graph handler. This is the
//! unified-document-host shape (Phase 1 complete); it replaced the earlier
//! two-root, route-by-Y-band composition.
//!
//! The orrery-as-element work lives in the unified-document-host plan: Phase 2a
//! landed (gnodes select + focus through the shell hit-test); retiring the
//! standalone orrery `Scene` into a scene underlay (cond 5) remains.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use crate::serval_render::fragments_from_scripted_dom;
use accesskit::NodeId as AccessNodeId;
use eidetic_fjall::FjallStore;
use forme::GraphMemberId;
use frame::{FrameId, FrameLayout, GraphId, PaneContent, PaneId, PaneNode, SessionId};
use inker::EngineRegistry;
use layout_dom_api::LayoutDom;
use meerkat::{Chrome, ChromeLogic, chrome_view};
use orrery::{CameraView, Orrery};
use platen::Workbench;
use register_diagnostics::{DiagnosticEvent, install_global_sender};
use register_theme::chrome::{ChromeTheme, Color32};
use register_theme::theme::ThemeRegistry;
use serval_layout::FragmentPlane;
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_winit_host::RenderCore;
use session_runtime::{
    ManifestStore, frame_layout_store, manifest::GraphSessionManifest, session_graph_store,
    settings_store, view_intent_store,
};
use tracing_subscriber::prelude::*;
use winit::window::{ResizeDirection, WindowId};

mod browse_capture;
mod card;
mod comms_host;
mod constellation;
mod content;
mod crawl;
mod doc_style;
mod note_surface;
mod resources;
mod sync;
mod wallet_pairing;

mod a11y_bridge;
#[cfg(any(test, feature = "agent-harness"))]
mod agent_harness;
mod app_handler;
mod apparatus;
mod command_drain;
mod engine_activation;
mod export;
mod find;
mod find_worker;
mod infer_host;
mod frame_a11y;
mod frame_a11y_panes;
mod frame_ops;
mod frame_view;
mod gloss_outline_data;
mod gloss_outline_view;
mod gloss_view;
mod graph_delta_log;
mod ime;
mod input;
mod inspector;
mod list_pane;
mod menus;
mod nav_sync;
mod node_ops;
mod note_sheet;
mod observability;
#[cfg(test)]
mod overlay_probe;
mod pane_data;
mod pane_geom;
mod pane_input_snapshot;
mod pane_session;
mod render;
mod roster;
#[cfg(test)]
mod roster_action_tests;
mod roster_data;
mod roster_facet_data;
mod roster_facet_view;
mod roster_view;
mod roster_view_graphlets;
mod roster_view_links;
mod roster_view_parts;
mod scene_settings;
mod settings_lane;
mod settings_node;
mod settings_pane_view;
mod sprite_import;
mod swatch;
mod viewport;
mod web_clip;
// `ViewPane` is the shared base for the `RosterPane` / `ListPane` test harnesses only;
// every product pane now folds into the shell document, so the module is test-gated.
// (Phase 1, step 2.)
mod mode_store;
mod scrying_host;
mod serval_a11y;
mod serval_render;
mod session_ops;
mod session_thumbs;
mod shell_command;
mod shellbar;
mod steward;
mod tags;
#[cfg(test)]
mod test_support;
mod text;
mod theme_edit;
mod theme_store;
mod tile_theme;
mod titlebar;
mod tracing_layer;
mod utility_panes;
#[cfg(test)]
mod view_pane;
mod window_view;

use constellation::Constellation;
pub use fetch;
pub use graphlets;
pub use graphlets::classifier as graphlet_classifier;
use observability::HostObservability;
pub(crate) use shell_command::ShellCommand;

mod theme_sheets;
pub(crate) use note_sheet::*;
pub(crate) use theme_sheets::*;
pub(crate) use tile_theme::*;

/// Single-pane view-intent identity for the default session (one frame, one
/// pane). Per-frame / per-pane ids arrive with the tiled workbench (S4) and
/// session manifests (S3.2b).
const DEFAULT_FRAME: &str = "00000000-0000-0000-0000-0000000f1a3e";
const DEFAULT_PANE: u64 = 0;

/// The frame tree's graph pane — the always-present leaf hosting the orrery. The
/// tiled workbench is a separate summonable pane that coexists beside it (no
/// longer a projection toggle inside one leaf). Summoned sibling panes (roster,
/// workbench, …) get fresh ids from `next_pane_id`. (Frame tree, F1 / W.)
const GRAPH_PANE: PaneId = PaneId(0);

mod app_state;
pub(crate) use app_state::*;
/// How many graph orreries stay warm in the pool before the least-recently-
/// focused non-focused one is evicted. Each live orrery costs memory + its own
/// physics actor thread, so the pool is bounded; "a handful" keeps several
/// sessions warm (instant switch-back) without unbounded growth. A configurable
/// setting later (per the configurability rule). (Window composition P1, OQ2.)
const MAX_POOLED_ORRERIES: usize = 8;

struct Shell {
    /// Session + app state shared across every window. (Multi-window MW2.)
    shared: SharedState,
    /// The pooled orrery authorities, keyed by graph. Each is a whole [`Orrery`]
    /// (graph + physics + camera) — the source of every pane's content for that graph.
    /// Panes resolve to one by `graph_id`; the ctx bundles the window's focused-graph
    /// orrery as `self.orrery`. A sibling `Shell` field (not in `SharedState`) so it
    /// borrows disjointly from `shared` / `view`, as the single `orrery` did before.
    /// (Window composition P1; was the single `orrery: Orrery`.)
    orreries: HashMap<GraphId, Orrery>,
    /// Pooled graphs in least-recently-focused order (front = stalest). A graph
    /// moves to the back when focused; over [`MAX_POOLED_ORRERIES`] the stalest
    /// non-focused one is evicted (dropped, ending its physics thread). The graph
    /// was already saved when it was last switched away from, so eviction needs no
    /// save; switching back to it reloads from disk. (Window composition P1, OQ2 —
    /// the unload half of pool eviction.)
    orrery_lru: Vec<GraphId>,
    /// Per-session graphlet indices, keyed by graph (a sibling pool to `orreries`).
    /// Each holds the named sub-structures over that graph's nodes — today a tear-out
    /// **branch** graphlet, later document-groups / relational-browse neighborhoods.
    /// Loaded lazily from the `graphlets.json` sidecar; mutated + persisted on branch.
    /// (Graphlet-wiring Phase 1; reuses forme's `GraphletRef`, not its `GraphTree`.)
    graphlets: HashMap<GraphId, graphlets::SessionGraphlets>,
    /// All live windows, keyed by OS `WindowId` — the registry. Every per-window
    /// handler is dispatched by resolving the event's id to its view here. At N=1
    /// it holds just the primary; tear-out (MW3+) inserts more. (Multi-window MW2 (d).)
    windows: HashMap<WindowId, window_view::WindowView>,
    /// Which window is primary (owns the orrery + save-on-close). `None` until the
    /// first window is created in `resumed`. (MW2 (d).)
    primary: Option<WindowId>,
    /// The primary view, built in `new()` and consumed by `resumed` once the OS
    /// window (and thus its `WindowId`) exists. winit splits construction from window
    /// creation, so the view outlives its registry key for exactly one step. (MW2 (d).)
    pending_view: Option<window_view::WindowView>,
    /// The shared present core: one wgpu device + netrender `Renderer`, booted once on
    /// the first `resumed`. Every window's `WindowSurface` is created from it, so N
    /// windows present through one device. `None` until the first window boots it.
    /// Shared infra, so it sits on `Shell` (like `clipboard`), not in `SharedState`.
    /// (MW3: one device, N surfaces.)
    render_core: Option<RenderCore>,
    /// System clipboard for the omnibar / palette Ctrl(Cmd)+C/X/V. `None` if the
    /// platform clipboard could not be opened (the shortcuts then no-op). System-
    /// global, so it stays on `Shell`, not in `SharedState`.
    clipboard: Option<arboard::Clipboard>,
    /// The **primary** window's platform AccessKit bridge, fed by the same host-local
    /// uxtree snapshot as Apparatus. Unsupported platforms keep this as an explicit
    /// degraded bridge. Secondary (leaf) windows get their own in
    /// [`secondary_a11y_bridges`](Self::secondary_a11y_bridges); `ctx()` (always the
    /// primary) uses this one, `window_ctx` forks by id. (Per-window a11y, MW3 step 6.)
    a11y_bridge: a11y_bridge::AccessKitBridge,
    /// Per-secondary-window AccessKit bridges (MW3 step 6). The primary's bridge is
    /// `a11y_bridge` above; each spawned leaf gets its own here, keyed by its
    /// `WindowId` and installed against its own window. `window_ctx` resolves the right
    /// one (primary field vs this map); `close_window` drops it.
    secondary_a11y_bridges: HashMap<WindowId, a11y_bridge::AccessKitBridge>,
    /// The event-loop proxy that wakes the kernel from any window's AccessKit adapter,
    /// kept so a spawned window can mint its own bridge with the same wake. (MW3 step 6.)
    a11y_proxy: winit::event_loop::EventLoopProxy<()>,
    /// Host-owned routes for actionable AccessKit nodes in the current snapshot.
    /// The bridge only queues raw AccessKit requests; the kernel thread resolves
    /// ids through this table and applies semantic host actions.
    a11y_action_routes: HashMap<AccessNodeId, A11yHostAction>,
    /// The cross-window command queue: per-window handlers push [`ShellCommand`]s
    /// here (spawn / close a window) and the event loop drains them through
    /// [`Shell::apply`] in `about_to_wait`, once the borrowing ctx has ended. (MW3.)
    commands: Vec<ShellCommand>,
    /// The wake every pooled orrery's physics actor pokes (the winit proxy, host-
    /// neutral). Held so a graph minted into the pool at session-switch time gets
    /// its own offloaded physics, like the seed orrery did at boot. (Window
    /// composition P1, multi-graph.)
    physics_wake: armillary::Wake,
    /// When the last real window input arrived — bumped by `note_activity` on every
    /// `on_window_event`. The idle-cadence forgetting pass (Alembic B1) reads this to
    /// stay off mid-interaction; an actor wake (`user_event`) does not bump it, so a
    /// settling physics actor cannot itself suppress the pass forever.
    last_activity: Instant,
    /// When the idle-cadence forgetting pass last ran, or `None` before the first one
    /// (steady-heat, throttled to at most every `PASS_INTERVAL`). `node_ops::run_forgetting_pass`'s
    /// own manual-trigger path (the Alembic pane's click) does not touch this — the
    /// idle cadence and the manual click are independent triggers of the same verb.
    /// (Alembic B1.)
    last_forgetting: Option<Instant>,
    /// When the idle-cadence snapshot-refresh pass last ran, or `None` before the
    /// first one. Independent cadence + trigger from `last_forgetting` — the two
    /// passes share the same idle signal (`last_activity`) but are otherwise
    /// unrelated verbs. (Node/card summoning design, §5 item 4.)
    last_snapshot_refresh: Option<Instant>,
    /// Marks this struct as the kernel-thread context: `!Send` by construction
    /// (armillary's typed boundary), so kernel authority cannot be moved onto an
    /// actor thread — the attempt is a compile error, not a review catch.
    _kernel: armillary::KernelThread,
}

/// The borrow bundle for handling **one window's** events. The bulk of the
/// event-handling logic hangs off `impl WindowCtx` rather than `impl Shell`, so a
/// handler operates on exactly one window's [`WindowView`] plus the shared state
/// and the shell singletons the active window legitimately drives (the orrery,
/// the clipboard, the a11y bridge). The registry picks *which* window by building
/// the ctx over `windows[&id]`; that construction is the seam — a ctx method
/// cannot reach another window or the window map, so cross-window work (spawn /
/// close / move-tile) goes through `ShellCommand` instead. Bodies are unchanged
/// from when these were `&mut self` on `Shell`: `self.view` / `self.shared`
/// resolve to these fields; `self.orrery()` / `self.orrery_mut()` resolve the
/// focused pane's orrery out of the pool. (Multi-window MW2 (c); Window
/// composition P2.)
struct WindowCtx<'a> {
    view: &'a mut window_view::WindowView,
    shared: &'a mut SharedState,
    /// The orrery pool (every live graph's authority), borrowed whole so render
    /// and input can resolve *any* pane's orrery by `graph_id`, not just the
    /// window-focused one. The focused-bucket sites reach it through
    /// [`WindowCtx::orrery`] / [`WindowCtx::orrery_mut`]; per-pane paths resolve a
    /// specific `graph_id`. Was the single bundled `orrery: &mut Orrery` (P1).
    /// (Window composition P2.)
    orreries: &'a mut HashMap<GraphId, Orrery>,
    /// The per-session graphlet pool (read-only here), so a **branch** window's render
    /// can scope its orrery to its graphlet's live roster. (Graphlet wiring Phase 2
    /// slice 3.)
    graphlets: &'a HashMap<GraphId, graphlets::SessionGraphlets>,
    clipboard: &'a mut Option<arboard::Clipboard>,
    a11y_bridge: &'a mut a11y_bridge::AccessKitBridge,
    a11y_action_routes: &'a mut HashMap<AccessNodeId, A11yHostAction>,
    /// The shared present core (device + renderer). `None` before the first window
    /// boots it, and in the headless harness; the render path early-returns then.
    render_core: Option<&'a RenderCore>,
    /// The shell command queue. A per-window handler reaches exactly one window, so
    /// work that touches the registry or a second window (spawn / close) can't run
    /// here — it's pushed as a [`ShellCommand`] and applied by `Shell` after the ctx
    /// borrow ends. (Multi-window MW3, the deferred MW2 (e).)
    commands: &'a mut Vec<ShellCommand>,
    /// How many graphs are live in the orrery pool right now (a plain count, not a
    /// borrow — resolved at ctx build). Surfaced in Steward as the tripwire for the
    /// pool's bound (live / cap). (Window composition P1, OQ2.)
    orrery_pool_count: usize,
    /// While a **branch** window's pass has the orrery scoped to its graphlet, this holds
    /// the orrery's prior scope, restored on ctx `Drop` so the override is transient and
    /// never leaks to the next window's pass. `None` when this pass set no branch scope.
    /// (Graphlet wiring Phase 2 slice 3.)
    branch_scope_restore: Option<Option<Vec<uuid::Uuid>>>,
}

/// A tile's cached rasterized texture: the scene version + size it was rasterized
/// at, plus the GPU texture and its view. Reused across frames while the version +
/// size hold, so an idle tile is not re-rasterized.
pub(crate) struct CachedTile {
    pub(crate) version: u64,
    pub(crate) size: (u32, u32),
    #[allow(dead_code)] // owns the texture the `view` references; kept alive here
    pub(crate) tex: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
}

/// An in-progress manual window resize (custom titlebar). winit's
/// `drag_resize_window` is inert on frameless Windows — the non-client area is
/// removed via `WM_NCCALCSIZE`, so the OS has no edge frame to grab — so the host
/// resizes the window itself: it anchors the opposite edge(s) to the press-time
/// rect and tracks the cursor in screen space (the press-time screen cursor is the
/// origin, so there is no first-move jump). On Wayland `set_outer_position` is a
/// no-op, so left/top edges there can't move the origin; right/bottom still size.
#[derive(Clone, Copy)]
pub(crate) struct ResizeDrag {
    dir: ResizeDirection,
    /// Window outer top-left (physical px) at press.
    start_outer: (i32, i32),
    /// Window inner size (physical px) at press.
    start_size: (u32, u32),
    /// Cursor position in screen space (physical px) at press.
    start_cursor_screen: (f32, f32),
}

/// The host kernel's inbox: the typed receivers each I/O actor delivers updates on,
/// all woken by the one bare `EventLoopProxy<()>`. Grouping them names the seam (the
/// kernel/actor boundary's inbound half) without collapsing the per-subsystem
/// channels into one mega enum, which would muddy ownership. The constellation's
/// per-tile content channels are drained separately (`Constellation::drain`); this
/// holds only the I/O streams.
struct KernelInbox {
    fetch: Receiver<fetch::FetchUpdate>,
    /// Find-in-page worker replies (match rects per query generation). (Find.)
    find: Receiver<find_worker::FindResult>,
    /// Inference actor updates for the `>ask` verb (Ready / streamed
    /// fragments / finished / failed). (burn brief Lane 3.)
    infer: Receiver<infer::InferUpdate>,
    sync: Receiver<sync::SyncUpdate>,
    comms: Receiver<comms_host::CommsUpdate>,
    /// Portable diagnostics emitted through `register_diagnostics::emit`.
    diagnostics: Receiver<DiagnosticEvent>,
}

mod shell_access;
mod shell_new;
/// The shared per-user data root (`<data_dir>/mere`). Settings, the content
/// cache, and comms live directly here; per-session graph/frame/views live under
/// `<mere_root>/sessions/<session_id>/`. (Multi-graph MG1.)
fn default_mere_root() -> PathBuf {
    // A `MERE_ROOT` override points the whole data root at a scratch profile, so a
    // headed-verification run (or any throwaway session) can isolate from the real
    // per-user data dir. The Windows `dirs` path uses the Known-Folder API, which
    // ignores `%APPDATA%`, so an explicit env hook is the reliable way to redirect.
    if let Some(root) = std::env::var_os("MERE_ROOT") {
        return PathBuf::from(root);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mere")
}

/// Best-effort local device label for the seeded wallet roster entry.
fn default_device_label() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "This device".to_string())
}

/// The registry default context menu as owned strings (the seed when no persona curation
/// exists, and the target of "Reset to default"). (Command registry P4.)
fn default_menu_actions() -> Vec<String> {
    meerkat::command::DEFAULT_MENU_ACTIONS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// The default content frame: a single orrery pane filling the band, bound to the
/// active `graph_id`. Used at first launch and when no window layout is saved.
/// (Frame tree F1 / MG2; graph-bound leaf per MG5.)
fn default_content_frame(graph_id: GraphId) -> FrameLayout {
    FrameLayout {
        id: FrameId::new("content"),
        label: "content".to_string(),
        root: PaneNode::Leaf {
            pane_id: GRAPH_PANE,
            content: PaneContent::Orrery,
            graph_id,
        },
    }
}

/// A torn **leaf**'s frame (G2): a single Workbench pane over `graph_id`, with no orrery
/// pane of its own (the leaf shows the torn node's tile, not the whole graph). The pane
/// still binds the donor's `graph_id`, so its tile resolves to the shared pooled orrery.
/// (Tear-out gestures G2.)
fn leaf_workbench_frame(graph_id: GraphId) -> FrameLayout {
    FrameLayout {
        id: FrameId::new("content"),
        label: "content".to_string(),
        root: PaneNode::Leaf {
            pane_id: GRAPH_PANE,
            content: PaneContent::Workbench,
            graph_id,
        },
    }
}

/// Bring up the session registry under `<mere_root>/sessions/`: scan existing
/// session manifests, migrate a pre-MG1 flat graph in if one is found, or seed
/// one default session on a fresh install. Returns the registry plus the session
/// to open as active (the most-recently-updated). (Multi-graph MG1.)
fn bootstrap_sessions(mere_root: &Path) -> (ManifestStore, SessionId) {
    let sessions_root = mere_root.join("sessions");
    let mut manifests = ManifestStore::new();
    if let Err(err) = manifests.load_from_disk(&sessions_root) {
        tracing::warn!(%err, dir = ?sessions_root, "scanning sessions/ failed; starting fresh");
    }
    manifests.set_root(&sessions_root);

    // One-time migration: a flat `<mere_root>/graph.json` with no sessions/ is a
    // pre-MG1 single-session install. Mint a session and move its graph + views into
    // `sessions/<id>/`. The frame layout stays at the root (it is window-scoped per
    // MG5), and the content cache, settings, comms stay at the root too.
    let flat_graph = mere_root.join(session_graph_store::GRAPH_FILE);
    if manifests.is_empty() && flat_graph.exists() {
        let session_id = SessionId::new();
        let session_dir = sessions_root.join(session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        let _ = std::fs::rename(
            &flat_graph,
            session_dir.join(session_graph_store::GRAPH_FILE),
        );
        let flat_views = mere_root.join(view_intent_store::VIEW_INTENT_DIR);
        if flat_views.is_dir() {
            let _ = std::fs::rename(
                &flat_views,
                session_dir.join(view_intent_store::VIEW_INTENT_DIR),
            );
        }
        let mut manifest = GraphSessionManifest::new(session_id, GraphId::new());
        manifest.storage_path = Some(session_dir);
        manifests.insert(manifest);
        let _ = manifests.flush_dirty();
        tracing::info!(?session_id, "migrated the flat session into sessions/");
        return (manifests, session_id);
    }

    // Fresh install (or an empty sessions/): seed one default session.
    if manifests.is_empty() {
        let session_id = SessionId::new();
        let session_dir = sessions_root.join(session_id.as_uuid().to_string());
        let _ = std::fs::create_dir_all(&session_dir);
        let mut manifest = GraphSessionManifest::new(session_id, GraphId::new());
        manifest.storage_path = Some(session_dir);
        manifests.insert(manifest);
        let _ = manifests.flush_dirty();
        return (manifests, session_id);
    }

    // Existing sessions: open the most-recently-updated one.
    let active = manifests
        .iter()
        .max_by_key(|(_, m)| m.updated_at)
        .map(|(id, _)| id)
        .expect("manifests is non-empty here");
    (manifests, active)
}

#[cfg(test)]
mod multi_graph_tests;

mod view_helpers;
pub(crate) use view_helpers::*;

fn main() {
    // The scrying compatibility tiles import a WebView2 D3D11 shared texture, which the
    // host wgpu device can only do on **D3D12** (the NT-handle interop is DX12-only). wgpu
    // on Windows otherwise picks Vulkan, where the import fails with "backend mismatch:
    // expected Dx12, found non-Dx12" and the tile stays blank — so pin the backend to DX12
    // before any wgpu instance is built. An explicit `WGPU_BACKEND` (e.g. for debugging)
    // still wins. (Scrying tile plan; scry-in-pelt.)
    #[cfg(target_os = "windows")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // SAFETY: the first statement in `main`, before any thread or wgpu instance
        // exists, so there is no concurrent environment access.
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }
    let (diagnostics_tx, diagnostics_rx) = mpsc::channel();
    install_global_sender(diagnostics_tx.clone());
    // Two consumers with opposite needs share this subscriber: the terminal (fmt) stays quiet and
    // RUST_LOG-controlled, while the Apparatus ring wants first-party traces regardless of RUST_LOG.
    // A single *global* `.with(env_filter)` gated both, so the `meerkat=info` default starved the
    // ring of every non-meerkat target before the bridge's allowlist ran. Per-layer filters decouple
    // them: RUST_LOG governs only the console; the ring runs its own filter below.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("meerkat=info"));
    // The ring's own filter, independent of RUST_LOG. The `info` floor captures the first-party
    // lifecycle events (meerkat/armillary/… at info) and every `warn`/`error` fault. The per-target
    // `=debug` opt-ins pull in the sibling libraries' per-operation *completion* traces (a fetch
    // finished, a page laid out): those libs log them at `debug` per conventional library hygiene,
    // so the ring opts in here rather than forcing the libs up to info. `netrender` stays at the info
    // floor so its per-frame `frame rendered` debug does not flood the ring (only its faults pass).
    // The layer's `interesting_target` scopes which targets it actually mirrors into the ring, and
    // its `on_enter` records span starts idempotently (a re-entrant span must not double-insert).
    let ring_filter = tracing_subscriber::EnvFilter::new(
        "info,netfetcher=debug,errand=debug,serval_layout=debug",
    );
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(env_filter))
        .with(tracing_layer::ApparatusTracingLayer::new(diagnostics_tx).with_filter(ring_filter))
        .init();
    tracing::info!("meerkat-shell starting");

    let event_loop = winit::event_loop::EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = Shell::new(proxy, diagnostics_rx);
    event_loop.run_app(&mut app).expect("event loop error");
}
